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
) -> Result<()> {
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

    let connection = tokio::time::timeout(
        Duration::from_secs(10),
        endpoint.connect(receiver_addr, "localhost")?,
    )
    .await??;

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

    // Give the receiver's dispatch loop time to process the Hello before we tear down.
    tokio::time::sleep(Duration::from_millis(600)).await;
    drop(connection);
    Ok(())
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
    send_hello_as(
        Some(&peer),
        &peer_did,
        peer.binding_info(),
        *peer.x25519_public_bytes(),
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

    let info = handle.get_peer_connection_info(&peer_did).await;
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
    send_hello_as(
        Some(&attacker_x),
        &victim_b_did,
        victim_b.binding_info(),
        [0x42u8; 32], // attacker-chosen X25519 key, attributed to B if accepted
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

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

    send_hello_as(
        None, // no client certificate at all
        &victim_b_did,
        victim_b.binding_info(),
        [0x43u8; 32],
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

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
    send_hello_as(
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

    send_hello_as(
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
    init();
    let receiver = IdentityBundle::generate()?;
    let receiver_did = receiver.did().clone();
    let victim_b = IdentityBundle::generate()?;
    let victim_b_did = victim_b.did().clone();
    let attacker_x = IdentityBundle::generate()?;

    let (handle, addr, shutdown) = spawn_receiver(receiver).await?;

    // 1. B establishes itself legitimately.
    send_hello_as(
        Some(&victim_b),
        &victim_b_did,
        victim_b.binding_info(),
        *victim_b.x25519_public_bytes(),
        CapabilityFlags::E2E_ENCRYPTION,
        addr,
        &receiver_did,
    )
    .await?;

    let before = handle
        .get_peer_connection_info(&victim_b_did)
        .await
        .expect("B must be established by its legitimate Hello");
    assert_eq!(
        before.x25519_key,
        *victim_b.x25519_public_bytes(),
        "control: B's own key must be stored"
    );

    // 2. Attacker replays B's binding on their own cert, with a substituted key.
    send_hello_as(
        Some(&attacker_x),
        &victim_b_did,
        victim_b.binding_info(),
        [0x99u8; 32],
        CapabilityFlags::empty(),
        addr,
        &receiver_did,
    )
    .await?;

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

    let _ = shutdown.send(());
    Ok(())
}
