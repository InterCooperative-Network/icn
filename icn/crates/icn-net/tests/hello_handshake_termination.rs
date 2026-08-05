#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A Hello handshake must terminate because the protocol says it is complete.
//!
//! Both sides run the same `handle_hello`, and it answers every Hello with a Hello. A Hello
//! *response* is not distinguishable in kind from an initial Hello — same payload variant,
//! same fields — so each side answers the other's answer. Nothing in the protocol stops it.
//! What stops it today is the per-DID rate limiter, which runs *before* dispatch: after
//! roughly forty messages the limiter starts denying, a denied message produces no response,
//! and the chain breaks by accident.
//!
//! That has two costs. The obvious one is ~40 messages and ~40 stream opens per connection
//! against a transport configured for 10 concurrent bidirectional streams. The one that
//! actually bites is that the handshake spends the peer's application-message budget, so
//! legitimate traffic sent immediately after authentication is dropped.
//!
//! These tests pin the bounded exchange and, just as importantly, pin that bounding it does
//! not cost mutual authentication — a fix that simply ignored inbound responses would stop
//! the storm and silently break the initiator's ability to authenticate the responder.
//! See #2532.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_net::{IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage, VersionInfo};
use quinn::Endpoint;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Ports already handed out in this process.
///
/// `pick_unused_port` binds, closes, and returns the port, so two concurrently running tests
/// can be handed the same one. Remembering what we issued removes that race within the
/// process; callers still retry, since another process can take it in between.
static ISSUED_PORTS: std::sync::Mutex<Option<std::collections::HashSet<u16>>> =
    std::sync::Mutex::new(None);

fn pick_port() -> u16 {
    let mut guard = ISSUED_PORTS.lock().expect("issued-port lock poisoned");
    let issued = guard.get_or_insert_with(std::collections::HashSet::new);
    for _ in 0..200 {
        let port = portpicker::pick_unused_port().expect("no free port");
        if issued.insert(port) {
            return port;
        }
    }
    panic!("could not find an unissued free port");
}

fn init() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Spawn a real node on the production stack.
///
/// A handler is required: `NetworkActor::spawn` starts the inbound accept loop only when
/// `incoming_handler` is `Some`, and it is also what makes `wire_new_connection` spawn a
/// dispatch loop for an *outbound* connection. Without it nothing here would handshake.
async fn spawn_node(
    bundle: IdentityBundle,
    counter: Arc<AtomicUsize>,
    variant: &'static str,
) -> Result<(NetworkHandle, SocketAddr, Guard)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let handler: IncomingMessageHandler = Arc::new(move |msg: NetworkMessage| {
        if !variant.is_empty() && msg.payload.variant_name() == variant {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });

    let mut last_err = None;
    for _ in 0..8 {
        let addr: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
        match NetworkActor::spawn(
            bundle.clone(),
            addr,
            shutdown_tx.clone(),
            Some(handler.clone()),
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
        .await
        {
            Ok(handle) => {
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, addr, Guard(shutdown_tx)));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

async fn spawn_plain_node(bundle: IdentityBundle) -> Result<(NetworkHandle, SocketAddr, Guard)> {
    spawn_node(bundle, Arc::new(AtomicUsize::new(0)), "").await
}

struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// Poll until `observer` has authenticated `did`.
///
/// `peer_connections` is written only by `handle_hello`, after the #2520 binding checks pass,
/// so this is proof the handshake completed rather than a guess about elapsed time.
async fn wait_until_authenticated(observer: &NetworkHandle, did: &Did, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if observer.get_peer_connection_info(did).await.is_some() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A QUIC peer that behaves exactly like the current responder: it answers **every** Hello it
/// receives with a valid Hello of its own, and counts what arrives.
///
/// Mirroring today's `handle_hello` is the point. If the node under test is itself willing to
/// answer a response, the two of them sustain the exchange and the count climbs until the
/// rate limiter intervenes. If the node under test treats a response as the end of its
/// handshake, the count stops at one. So the count *is* the termination property, measured on
/// the wire and independent of how the fix is implemented internally.
struct MirroringPeer {
    addr: SocketAddr,
    hellos_received: Arc<AtomicUsize>,
    accept_task: tokio::task::JoinHandle<()>,
}

impl MirroringPeer {
    /// Answers with a Hello whose binding matches the certificate it presents, so the node
    /// under test accepts it.
    fn spawn(identity: &IdentityBundle) -> Result<Self> {
        Self::spawn_inner(identity, identity)
    }

    /// Answers with `impersonated`'s DID and `impersonated`'s own (internally valid) binding,
    /// while presenting `wire`'s certificate on the connection.
    ///
    /// That is the #2520 gap precisely: the binding proves `impersonated` authenticated *some*
    /// certificate, but not the one on this connection, so verification must reject it. The
    /// peer still answers every Hello, so if the node under test kept the exchange going on a
    /// rejected Hello the count would climb.
    fn spawn_with_invalid_binding(
        wire: &IdentityBundle,
        impersonated: &IdentityBundle,
    ) -> Result<Self> {
        Self::spawn_inner(wire, impersonated)
    }

    /// `wire` supplies the TLS certificate actually presented; `claimed` supplies the identity
    /// asserted in the Hello. Passing the same bundle for both makes an honest peer.
    fn spawn_inner(wire: &IdentityBundle, claimed: &IdentityBundle) -> Result<Self> {
        let addr_seed: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
        let server_config =
            icn_net::tls::create_server_config(vec![wire.tls_cert().clone()], wire.tls_key())?;
        let quic_crypto = Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(
            server_config,
        )?);

        let mut endpoint = None;
        let mut addr = addr_seed;
        for _ in 0..8 {
            let mut cfg = quinn::ServerConfig::with_crypto(quic_crypto.clone());
            cfg.transport_config(Arc::new({
                let mut t = quinn::TransportConfig::default();
                t.max_idle_timeout(Some(Duration::from_secs(60).try_into()?));
                t
            }));
            match Endpoint::server(cfg, addr) {
                Ok(ep) => {
                    endpoint = Some(ep);
                    break;
                }
                Err(_) => addr = format!("127.0.0.1:{}", pick_port()).parse()?,
            }
        }
        let endpoint = endpoint.context("could not bind the mirroring peer")?;

        let hellos_received = Arc::new(AtomicUsize::new(0));
        let counter = hellos_received.clone();
        let claimed_did = claimed.did().clone();
        let binding = claimed.binding_info();
        let x25519 = *claimed.x25519_public_bytes();

        let accept_task = tokio::spawn(async move {
            while let Some(incoming) = endpoint.accept().await {
                let Ok(conn) = incoming.await else { continue };
                let counter = counter.clone();
                let claimed_did = claimed_did.clone();
                let binding = binding.clone();
                tokio::spawn(async move {
                    while let Ok((mut send, mut recv)) = conn.accept_bi().await {
                        let Ok((msg, _)) = icn_net::protocol::read_message(&mut recv).await else {
                            continue;
                        };
                        let _ = send.finish();
                        if msg.payload.variant_name() != "Hello" {
                            continue;
                        }
                        counter.fetch_add(1, Ordering::SeqCst);
                        // Answer every Hello, exactly as the current responder does.
                        let reply = NetworkMessage::hello(
                            claimed_did.clone(),
                            msg.from.clone(),
                            binding.clone(),
                            VersionInfo::new("icnd-mirror".to_string()),
                            None,
                            x25519,
                            None,
                            None,
                        );
                        if let Ok((mut s, _r)) = conn.open_bi().await {
                            let _ = icn_net::protocol::write_message(&mut s, &reply).await;
                            let _ = s.finish();
                        }
                    }
                });
            }
        });

        std::thread::sleep(Duration::from_millis(300));
        Ok(Self {
            addr,
            hellos_received,
            accept_task,
        })
    }
}

impl Drop for MirroringPeer {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

/// A node must send exactly one Hello per connection it initiates.
///
/// The peer here answers every Hello, so the count measures only the node's own willingness
/// to keep the exchange going. One initial Hello is the whole of the initiator's obligation:
/// the response it gets back completes its handshake, it is not a new question to answer.
#[tokio::test]
async fn initiator_sends_exactly_one_hello_per_connection() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let responder = IdentityBundle::generate()?;
    let responder_did = responder.did().clone();

    let (dialer_handle, _addr, _guard) = spawn_plain_node(dialer).await?;
    let peer = MirroringPeer::spawn(&responder)?;

    dialer_handle.dial(peer.addr, responder_did.clone()).await?;

    // Synchronise on the handshake completing, then allow ample time for any further Hellos
    // to arrive and be counted. Asserting as soon as authentication lands would pass even
    // while a reflected Hello was still in flight.
    assert!(
        wait_until_authenticated(&dialer_handle, &responder_did, Duration::from_secs(20)).await,
        "precondition: the dialer never authenticated the peer, so no handshake was exercised"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let count = peer.hellos_received.load(Ordering::SeqCst);
    assert_eq!(
        count, 1,
        "a connection's initiator owes exactly one Hello; it sent {count}. Every extra one is \
         an answer to an answer, and the exchange stops only when the rate limiter starts \
         denying — that is not a protocol termination condition (#2532)"
    );

    Ok(())
}

/// Bounding the exchange must not cost mutual authentication.
///
/// This is the control that rules out the vacuous fix. Simply ignoring inbound Hello
/// responses would make the test above pass and would leave the initiator unable to
/// authenticate the responder — the response is the *only* evidence the initiator ever gets
/// about who it dialled.
#[tokio::test]
async fn both_sides_authenticate_each_other() -> Result<()> {
    init();
    let a = IdentityBundle::generate()?;
    let a_did = a.did().clone();
    let b = IdentityBundle::generate()?;
    let b_did = b.did().clone();

    let (a_handle, _a_addr, _a_guard) = spawn_plain_node(a).await?;
    let (b_handle, b_addr, _b_guard) = spawn_plain_node(b).await?;

    a_handle.dial(b_addr, b_did.clone()).await?;

    assert!(
        wait_until_authenticated(&a_handle, &b_did, Duration::from_secs(20)).await,
        "the initiator must authenticate the responder from its Hello response"
    );
    assert!(
        wait_until_authenticated(&b_handle, &a_did, Duration::from_secs(20)).await,
        "the responder must authenticate the initiator from its initial Hello"
    );

    assert_eq!(a_handle.connected_peers().await, vec![b_did]);
    assert_eq!(b_handle.connected_peers().await, vec![a_did]);

    Ok(())
}

/// Application traffic sent immediately after authentication must arrive.
///
/// This is the practical cost of #2532 and the reason a bigger rate-limit budget is not a
/// fix. The handshake currently spends the peer's per-DID allowance, so the first real
/// messages are denied before dispatch. Note there is no settling delay here: waiting out the
/// limiter's window is precisely the workaround this test exists to make unnecessary.
#[tokio::test]
async fn application_traffic_immediately_after_handshake_is_delivered() -> Result<()> {
    init();
    let a = IdentityBundle::generate()?;
    let a_did = a.did().clone();
    let b = IdentityBundle::generate()?;
    let b_did = b.did().clone();

    let received = Arc::new(AtomicUsize::new(0));
    let (a_handle, _a_addr, _a_guard) = spawn_plain_node(a).await?;
    let (_b_handle, b_addr, _b_guard) = spawn_node(b, received.clone(), "Subscribe").await?;

    a_handle.dial(b_addr, b_did.clone()).await?;
    assert!(
        wait_until_authenticated(&a_handle, &b_did, Duration::from_secs(20)).await,
        "precondition: handshake did not complete"
    );

    // No sleep between authentication and traffic. That is the property.
    for i in 0..3 {
        a_handle
            .send_message(
                b_did.clone(),
                NetworkMessage::subscribe(
                    a_did.clone(),
                    b_did.clone(),
                    vec![format!("test:topic:{i}")],
                ),
            )
            .await?;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while received.load(Ordering::SeqCst) < 3 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    assert_eq!(
        received.load(Ordering::SeqCst),
        3,
        "messages sent immediately after authentication must be delivered; the Hello exchange \
         consumed the per-DID rate-limit budget and they were denied before dispatch (#2532)"
    );

    Ok(())
}

/// A Hello that fails binding verification must not authenticate and must not be answered.
///
/// Termination has to hold on the failure path too: a rejected Hello must not produce a
/// response that invites another one.
#[tokio::test]
async fn hello_failing_verification_neither_authenticates_nor_answers() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    // The peer presents this certificate but will claim the impersonated identity's binding.
    let wire = IdentityBundle::generate()?;
    let impersonated = IdentityBundle::generate()?;
    let impersonated_did = impersonated.did().clone();

    let (dialer_handle, _addr, _guard) = spawn_plain_node(dialer).await?;
    let peer = MirroringPeer::spawn_with_invalid_binding(&wire, &impersonated)?;

    dialer_handle
        .dial(peer.addr, impersonated_did.clone())
        .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert!(
        dialer_handle
            .get_peer_connection_info(&impersonated_did)
            .await
            .is_none(),
        "a Hello that fails #2520 verification must not authenticate anyone"
    );
    assert!(
        dialer_handle.connected_peers().await.is_empty(),
        "no peer may be installed from an unverified Hello"
    );
    let count = peer.hellos_received.load(Ordering::SeqCst);
    assert_eq!(
        count, 1,
        "the initiator owes one Hello even when the exchange fails; it sent {count} (#2532)"
    );

    Ok(())
}

/// A repeated Hello on an already-answered connection must not be answered again.
///
/// Direction alone already bounds the exchange, since an initiator never replies. This pins
/// the responder's side of the contract: its obligation is one answer per connection, so a
/// peer that sends Hello twice — whether buggy, or probing for amplification — gets one
/// answer, not two. The repeat is still *verified*, so any state it carries is refreshed; it
/// simply does not restart a response chain.
#[tokio::test]
async fn responder_answers_a_connections_hello_exactly_once() -> Result<()> {
    init();
    let node = IdentityBundle::generate()?;
    let node_did = node.did().clone();
    let client = IdentityBundle::generate()?;
    let client_did = client.did().clone();

    let (_node_handle, node_addr, _guard) = spawn_plain_node(node).await?;

    let rustls_client =
        icn_net::tls::create_tofu_client_config(vec![client.tls_cert().clone()], client.tls_key())?;
    let mut endpoint = Endpoint::client("127.0.0.1:0".parse()?)?;
    endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client)?,
    )));
    let conn = tokio::time::timeout(
        Duration::from_secs(10),
        endpoint.connect(node_addr, "localhost")?,
    )
    .await
    .context("connect timed out")??;

    // Count the node's Hello responses on this connection.
    let responses = Arc::new(AtomicUsize::new(0));
    let counter = responses.clone();
    let reader = tokio::spawn({
        let conn = conn.clone();
        async move {
            while let Ok((_s, mut recv)) = conn.accept_bi().await {
                if let Ok((msg, _)) = icn_net::protocol::read_message(&mut recv).await {
                    if msg.payload.variant_name() == "Hello" {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }
    });

    let hello = || {
        NetworkMessage::hello(
            client_did.clone(),
            node_did.clone(),
            client.binding_info(),
            VersionInfo::new("icnd-repeat".to_string()),
            None,
            *client.x25519_public_bytes(),
            None,
            None,
        )
    };

    for _ in 0..3 {
        let (mut s, _r) = conn.open_bi().await?;
        icn_net::protocol::write_message(&mut s, &hello()).await?;
        s.finish()?;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let count = responses.load(Ordering::SeqCst);
    assert_eq!(
        count, 1,
        "a responder owes one Hello answer per connection; three Hellos drew {count} answers \
         (#2532)"
    );

    reader.abort();
    Ok(())
}
