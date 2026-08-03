#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Hello must bind the claimed DID to the certificate of the CURRENT connection.
//!
//! A `BindingInfo` proves "this DID authenticated *some* certificate". On its own that
//! is replayable: it says nothing about the certificate the peer is presenting right
//! now. These tests pin the stronger property the session layer actually needs —
//! the claimed DID must have authenticated the certificate presented by *this* QUIC
//! connection — and pin it through the production composition path (real QUIC, real
//! TLS config, real dispatch loop, real `handle_hello`), not by calling a helper.

use anyhow::Result;
use icn_identity::{Did, IdentityBundle};
use icn_net::{CapabilityFlags, NetworkActor, NetworkHandle, NetworkMessage, VersionInfo};
use quinn::{ClientConfig, Endpoint};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

fn pick_port() -> u16 {
    portpicker::pick_unused_port().expect("no free port")
}

fn init() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Spawn a real receiving node on the production stack.
async fn spawn_receiver(
    bundle: IdentityBundle,
) -> Result<(NetworkHandle, SocketAddr, broadcast::Sender<()>)> {
    let addr: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
    let (shutdown_tx, _) = broadcast::channel(1);
    // A handler is REQUIRED: `NetworkActor::spawn` only starts the inbound accept loop
    // when `incoming_handler` is `Some`. Passing `None` yields a node that binds a
    // socket but never accepts, which presents as a TLS handshake timeout.
    let handler: icn_net::IncomingMessageHandler = Arc::new(|_msg| {});
    let handle = NetworkActor::spawn(
        bundle,
        addr,
        shutdown_tx.clone(),
        Some(handler),
        None, // oracle
        None, // fallback_config
        None, // topology_config
        None, // stun_servers
        None, // turn_config
        None, // misbehavior_detector
        None, // store
        None, // personhood_store
        None, // anchor_rate_config
        None, // advertised_addr
    )
    .await?;
    // Let the endpoint bind before anyone dials it.
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok((handle, addr, shutdown_tx))
}

/// Dial the receiver as a raw QUIC client presenting `wire_identity`'s certificate,
/// then send a Hello whose claimed DID and BindingInfo are supplied independently.
///
/// Separating "certificate actually presented" from "identity claimed in the Hello"
/// is the whole point: that gap is the vulnerability under test.
async fn send_hello_as(
    wire_identity: Option<&IdentityBundle>,
    claimed_from: &Did,
    claimed_binding: icn_identity::BindingInfo,
    x25519_public: [u8; 32],
    capabilities: CapabilityFlags,
    receiver_addr: SocketAddr,
    receiver_did: &Did,
) -> Result<HelloAttempt> {
    let rustls_client = match wire_identity {
        Some(bundle) => icn_net::tls::create_tofu_client_config(
            vec![bundle.tls_cert().clone()],
            bundle.tls_key(),
        )?,
        // No client certificate at all: the server's TOFU verifier has
        // `client_auth_mandatory() == false`, so this still completes the handshake.
        None => {
            let mut cfg = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCert))
                .with_no_client_auth();
            cfg.alpn_protocols = vec![b"icn/1".to_vec()];
            cfg
        }
    };

    let mut endpoint = Endpoint::client("127.0.0.1:0".parse()?)?;
    endpoint.set_default_client_config(ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client)?,
    )));

    // Establishing the connection is a precondition, not the property under test, so it
    // is retried: several of these tests run concurrently, each with its own receiver and
    // client endpoint, and a contended runner can lose the first dial before the
    // receiver's accept loop is ready. Only the setup retries — no assertion below does.
    let mut last_err = None;
    let mut connection = None;
    for attempt in 0..5 {
        match tokio::time::timeout(
            Duration::from_secs(10),
            endpoint.connect(receiver_addr, "localhost")?,
        )
        .await
        {
            Ok(Ok(conn)) => {
                connection = Some(conn);
                break;
            }
            Ok(Err(e)) => last_err = Some(e.to_string()),
            Err(_) => last_err = Some("connect timed out".to_string()),
        }
        tokio::time::sleep(Duration::from_millis(150 * (attempt + 1))).await;
    }
    let connection = connection.ok_or_else(|| {
        anyhow::anyhow!(
            "could not establish the test connection after 5 attempts: {}",
            last_err.unwrap_or_default()
        )
    })?;

    let mut version_info = VersionInfo::new("icnd-attacker".to_string());
    version_info.capabilities = capabilities;

    let hello = NetworkMessage::hello(
        claimed_from.clone(),
        receiver_did.clone(),
        claimed_binding,
        version_info,
        None, // topology_info
        x25519_public,
        None, // ml_dsa_public
        None, // ml_kem_public
    );

    let (mut send, _recv) = connection.open_bi().await?;
    icn_net::protocol::write_message(&mut send, &hello).await?;
    let _ = send.finish();

    // Return the live connection. Callers synchronise on an observable outcome rather
    // than a fixed delay: `wait_for_peer` for an accepted Hello, `wait_until_refused`
    // for a rejected one. A sleep here would make every negative assertion vacuous on a
    // slow runner, since "not yet processed" and "correctly rejected" both look like an
    // absent peer entry.
    Ok(HelloAttempt {
        _endpoint: endpoint,
        connection,
    })
}

/// Keeps the sending endpoint and connection alive for the duration of an assertion.
struct HelloAttempt {
    _endpoint: Endpoint,
    connection: quinn::Connection,
}

/// Poll until the receiver has installed `did`, or the deadline passes.
async fn wait_for_peer(
    handle: &NetworkHandle,
    did: &Did,
    timeout: Duration,
) -> Option<icn_net::actor::PeerConnectionInfo> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(info) = handle.get_peer_connection_info(did).await {
            return Some(info);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Wait for the receiver to tear the connection down.
///
/// A rejected Hello makes `handle_hello` return `Err`, which ends `handle_connection` and
/// drops the receiver's `quinn::Connection`, so the sender observes a close. That close is
/// the barrier proving the Hello was *processed and refused* — as opposed to merely not
/// having been looked at yet, which is what a bare sleep would leave ambiguous.
async fn wait_until_refused(attempt: &HelloAttempt, timeout: Duration) -> bool {
    tokio::time::timeout(timeout, attempt.connection.closed())
        .await
        .is_ok()
}

#[derive(Debug)]
struct AcceptAnyServerCert;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyServerCert {
    fn verify_server_cert(
        &self,
        _e: &rustls::pki_types::CertificateDer<'_>,
        _i: &[rustls::pki_types::CertificateDer<'_>],
        _n: &rustls::pki_types::ServerName<'_>,
        _o: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _m: &[u8],
        _c: &rustls::pki_types::CertificateDer<'_>,
        _d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _m: &[u8],
        _c: &rustls::pki_types::CertificateDer<'_>,
        _d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
}

/// POSITIVE CONTROL: the honest case must keep working.
///
/// Guards against "fixing" the bug by rejecting everything.
#[tokio::test]
async fn hello_with_matching_current_cert_is_accepted() -> Result<()> {
    init();
    let receiver = IdentityBundle::generate()?;
    let receiver_did = receiver.did().clone();
    let peer = IdentityBundle::generate()?;
    let peer_did = peer.did().clone();

    let (handle, addr, shutdown) = spawn_receiver(receiver).await?;

    // Presents peer's own cert AND peer's own binding: consistent.
    let _attempt = send_hello_as(
        Some(&peer),
        &peer_did,
        peer.binding_info(),
        *peer.x25519_public_bytes(),
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

    let info = wait_for_peer(&handle, &peer_did, Duration::from_secs(20)).await;
    assert!(
        info.is_some(),
        "a Hello whose BindingInfo matches the current connection's certificate must be accepted"
    );

    let _ = shutdown.send(());
    Ok(())
}

/// RED: the defect itself.
///
/// The attacker holds a genuine, correctly-signed `BindingInfo` for victim DID B
/// (every node broadcasts its own BindingInfo in each Hello, so this is not privileged
/// material). The attacker presents their *own* certificate on the wire. Nothing about
/// this connection proves possession of B's key on *this* TLS session.
#[tokio::test]
async fn hello_replayed_onto_a_different_current_cert_is_rejected() -> Result<()> {
    init();
    let receiver = IdentityBundle::generate()?;
    let receiver_did = receiver.did().clone();

    let victim_b = IdentityBundle::generate()?;
    let victim_b_did = victim_b.did().clone();
    let attacker_x = IdentityBundle::generate()?;

    let (handle, addr, shutdown) = spawn_receiver(receiver).await?;

    // Wire cert = attacker's. Claimed DID + BindingInfo = B's authentic material.
    let attempt = send_hello_as(
        Some(&attacker_x),
        &victim_b_did,
        victim_b.binding_info(),
        [0x42u8; 32], // attacker-chosen X25519 key, attributed to B if accepted
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

    assert!(
        wait_until_refused(&attempt, Duration::from_secs(20)).await,
        "the receiver must refuse the connection; without observing that, an absent peer \
         entry below would only mean the Hello had not been processed yet"
    );
    let info = handle.get_peer_connection_info(&victim_b_did).await;
    assert!(
        info.is_none(),
        "a replayed BindingInfo bound to a DIFFERENT certificate than the current \
         connection presents must not attribute that connection to B (got {info:?})"
    );

    let _ = shutdown.send(());
    Ok(())
}

/// The certificate-absent path must fail closed.
///
/// `client_auth_mandatory() == false`, so an attacker can simply decline to present a
/// certificate. If absence were treated as "cannot check, so allow", the fix above
/// would be trivially bypassed.
#[tokio::test]
async fn hello_without_any_peer_certificate_is_rejected() -> Result<()> {
    init();
    let receiver = IdentityBundle::generate()?;
    let receiver_did = receiver.did().clone();
    let victim_b = IdentityBundle::generate()?;
    let victim_b_did = victim_b.did().clone();

    let (handle, addr, shutdown) = spawn_receiver(receiver).await?;

    let attempt = send_hello_as(
        None, // no client certificate at all
        &victim_b_did,
        victim_b.binding_info(),
        [0x43u8; 32],
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

    assert!(
        wait_until_refused(&attempt, Duration::from_secs(20)).await,
        "the receiver must refuse a connection with no peer certificate"
    );
    let info = handle.get_peer_connection_info(&victim_b_did).await;
    assert!(
        info.is_none(),
        "a Hello on a connection with no peer certificate cannot prove current-connection \
         possession and must be rejected, not accepted by default (got {info:?})"
    );

    let _ = shutdown.send(());
    Ok(())
}

/// A DID mismatch between `from` and `binding_info.did` must be rejected even when the
/// BindingInfo is internally valid and matches the presented certificate.
///
/// This is the hole in `NetworkMessage::verify_hello`: it checks the certificate hash
/// but never compares the binding's DID against the message sender.
#[tokio::test]
async fn hello_claiming_another_did_with_own_valid_binding_is_rejected() -> Result<()> {
    init();
    let receiver = IdentityBundle::generate()?;
    let receiver_did = receiver.did().clone();
    let victim_b_did = IdentityBundle::generate()?.did().clone();
    let attacker_x = IdentityBundle::generate()?;

    let (handle, addr, shutdown) = spawn_receiver(receiver).await?;

    // Attacker presents their own cert and their OWN internally-valid binding,
    // but claims to be B.
    let attempt = send_hello_as(
        Some(&attacker_x),
        &victim_b_did,
        attacker_x.binding_info(),
        [0x44u8; 32],
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

    assert!(
        wait_until_refused(&attempt, Duration::from_secs(20)).await,
        "the receiver must refuse a Hello whose binding belongs to a different DID"
    );
    assert!(
        handle
            .get_peer_connection_info(&victim_b_did)
            .await
            .is_none(),
        "binding DID must be checked against the Hello sender"
    );

    let _ = shutdown.send(());
    Ok(())
}

/// A tampered binding signature must be rejected.
#[tokio::test]
async fn hello_with_tampered_binding_signature_is_rejected() -> Result<()> {
    init();
    let receiver = IdentityBundle::generate()?;
    let receiver_did = receiver.did().clone();
    let peer = IdentityBundle::generate()?;
    let peer_did = peer.did().clone();

    let (handle, addr, shutdown) = spawn_receiver(receiver).await?;

    let mut binding = peer.binding_info();
    binding.tls_binding_sig[0] ^= 0xff;

    let attempt = send_hello_as(
        Some(&peer),
        &peer_did,
        binding,
        *peer.x25519_public_bytes(),
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

    assert!(
        wait_until_refused(&attempt, Duration::from_secs(20)).await,
        "the receiver must refuse a Hello with a tampered binding signature"
    );
    assert!(
        handle.get_peer_connection_info(&peer_did).await.is_none(),
        "tampered binding signature must be rejected"
    );

    let _ = shutdown.send(());
    Ok(())
}

/// A forged Hello must not disturb an already-established, legitimately-authenticated
/// connection for B — no displacement, and no overwrite of B's negotiated state.
///
/// Guards the #2505/#2504 connection-replacement interaction: the security fix must not
/// hand an unauthenticated party a way to corrupt a healthy peer entry.
#[tokio::test]
async fn forged_hello_does_not_corrupt_established_peer_state() -> Result<()> {
    // Extended for #2517: peer *capabilities* are now security state, not just
    // metadata. `DURABLE_SIGNING_SEQUENCE` tells the replay guard that B's sequence
    // numbers belong to the durable namespace, and the guard acts on it — it will
    // retire B's old replay bound and start a fresh namespace. So the same binding
    // that protects B's X25519 key has to protect B's advertised regime, and both are
    // asserted here rather than in a separate node: standing up another real QUIC
    // endpoint in this file destabilises the whole suite (see the note on
    // `spawn_receiver`).
    init();
    let receiver = IdentityBundle::generate()?;
    let receiver_did = receiver.did().clone();
    let victim_b = IdentityBundle::generate()?;
    let victim_b_did = victim_b.did().clone();
    let attacker_x = IdentityBundle::generate()?;

    let (handle, addr, shutdown) = spawn_receiver(receiver).await?;

    // 1. B establishes itself legitimately, advertising the durable sequence regime.
    let _legit = send_hello_as(
        Some(&victim_b),
        &victim_b_did,
        victim_b.binding_info(),
        *victim_b.x25519_public_bytes(),
        CapabilityFlags::E2E_ENCRYPTION | CapabilityFlags::DURABLE_SIGNING_SEQUENCE,
        addr,
        &receiver_did,
    )
    .await?;

    let before = wait_for_peer(&handle, &victim_b_did, Duration::from_secs(20))
        .await
        .expect("B must be established by its legitimate Hello");
    assert_eq!(
        before.x25519_key,
        *victim_b.x25519_public_bytes(),
        "control: B's own key must be stored"
    );
    assert!(
        before
            .peer_capabilities
            .contains(CapabilityFlags::DURABLE_SIGNING_SEQUENCE),
        "control (#2517): an authenticated peer advertising the durable sequence regime \
         must have it recorded. Without this the negative assertion below would pass \
         vacuously on a build that never records the capability at all — which would \
         silently read every honest upgraded sender as unproven"
    );

    // 2. Attacker replays B's binding on their own cert, with a substituted key and a
    //    substituted sequence-regime claim. B's BindingInfo is not a secret — every
    //    node publishes it in every Hello — so this is available to any peer that has
    //    ever spoken to B.
    let forged = send_hello_as(
        Some(&attacker_x),
        &victim_b_did,
        victim_b.binding_info(),
        [0x99u8; 32],
        CapabilityFlags::GOSSIP_PULL,
        addr,
        &receiver_did,
    )
    .await?;

    assert!(
        wait_until_refused(&forged, Duration::from_secs(20)).await,
        "the forged connection must be refused"
    );
    let after = handle
        .get_peer_connection_info(&victim_b_did)
        .await
        .expect("B's legitimate entry must survive a forged Hello");

    assert_eq!(
        after.x25519_key,
        *victim_b.x25519_public_bytes(),
        "forged Hello must not replace B's X25519 key (this is the confidentiality break: \
         messages encrypted to B would be readable by the attacker)"
    );
    assert_eq!(
        after.peer_capabilities, before.peer_capabilities,
        "forged Hello must not renegotiate B's capabilities"
    );
    assert!(
        after
            .peer_capabilities
            .contains(CapabilityFlags::DURABLE_SIGNING_SEQUENCE)
            && !after
                .peer_capabilities
                .contains(CapabilityFlags::GOSSIP_PULL),
        "#2517: a peer that is not authenticated as B must not be able to change what \
         the receiver believes about B's sequence regime — in either direction. If it \
         could, it could force B's replay namespace to be retired and then own the \
         empty namespace that replaces it"
    );

    let _ = shutdown.send(());
    Ok(())
}

/// COMPOSITION GUARD: the weak verifier must not be reachable from a new production path.
///
/// `verify_did_matches_binding` proves the binding belongs to the sender, but says nothing
/// about the current connection. It is only safe when paired with `verify_binding_info`
/// against the live peer certificate. This test fails if a future path picks up the weak
/// helper on its own — the failure mode that produced this vulnerability in the first place.
#[test]
fn weak_binding_verifier_is_confined_to_authorised_sites() {
    const AUTHORISED: &[&str] = &["src/handlers/hello.rs", "src/protocol.rs"];

    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    let mut stack = vec![crate_root.join("src")];

    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("readable src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let body = std::fs::read_to_string(&path).expect("readable rust file");
            if !body.contains("verify_did_matches_binding") {
                continue;
            }
            let rel = path
                .strip_prefix(crate_root)
                .expect("path under crate root")
                .to_string_lossy()
                .replace('\\', "/");
            if !AUTHORISED.contains(&rel.as_str()) {
                offenders.push(rel);
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "`verify_did_matches_binding` appears outside the authorised sites {AUTHORISED:?}: \
         {offenders:?}. On its own it does not bind the claimed DID to the current \
         connection; pair it with `verify_binding_info(binding, current_peer_certificate(conn))` \
         or route through the existing Hello handler."
    );

    // And the authorised Hello path must still perform the current-connection check.
    let hello = std::fs::read_to_string(crate_root.join("src/handlers/hello.rs"))
        .expect("hello handler readable");
    assert!(
        hello.contains("current_peer_certificate") && hello.contains("verify_binding_info"),
        "the Hello handler must verify the binding against the current peer certificate"
    );
}
