#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A physical connection gets exactly one connection-handler lifecycle.
//!
//! `SessionManager::dial` has two semantically different successful outcomes: it either
//! established a new QUIC connection, or it found a live authenticated session for the peer,
//! closed the connection it had just made, and returned the *existing* one. Both used to come
//! back as a bare `quinn::Connection`, so `NetworkActor::handle_dial` could not tell them
//! apart and called `wire_new_connection` either way.
//!
//! On an already-wired connection that call is pure damage: it spawns a second
//! `handle_connection` loop over a connection that already has one — so each inbound stream is
//! delivered to whichever loop wins the race, and any per-connection state a handler holds
//! becomes ambiguous — and it writes a redundant Hello onto a session that has already
//! completed its handshake. Redundant dials are ordinary: `background_tasks` re-dials
//! bootstrap peers whenever its liveness view disagrees with `close_reason()`.
//!
//! Two independent oracles are used here, deliberately:
//!
//! * `NetworkStats::connections_total` counts `wire_new_connection` invocations, and that
//!   function is the sole thing that spawns an outbound `handle_connection` task. It is
//!   therefore a direct count of handler lifecycles, not a proxy for one.
//! * the Hello count observed on the wire by [`CountingPeer`], which is independent of any
//!   internal counter and of how the fix is implemented.
//!
//! See #2536. The handshake semantics these tests lean on are #2532's (an initiator sends one
//! Hello and does not answer the response) and #2530's (a dial does not register a peer;
//! only an authenticated Hello does).

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

struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// Spawn a real node on the production stack.
///
/// A handler is required: `NetworkActor::spawn` starts the inbound accept loop only when
/// `incoming_handler` is `Some`, and it is also what makes `wire_new_connection` spawn a
/// dispatch loop for an *outbound* connection — which is the very thing under test.
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

/// Poll until the node reports at least `want` connection-handler installs.
///
/// Returns the count observed when the predicate fired, or the final count at the deadline.
/// `connections_total` is incremented from a spawned task, so it is eventually consistent;
/// waiting for the *violating* value rather than sleeping a fixed span means a build that
/// double-wires fails as soon as the second install lands, and a correct build pays the
/// timeout only once per assertion.
async fn wait_for_installs(node: &NetworkHandle, want: u64, timeout: Duration) -> u64 {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let seen = node
            .get_stats()
            .await
            .map(|s| s.connections_total)
            .unwrap_or(0);
        if seen >= want {
            return seen;
        }
        if tokio::time::Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Poll until `counter` reaches `want`, returning the value observed.
///
/// `connections_total` is incremented at the *start* of `wire_new_connection`, before the Hello
/// it sends reaches the wire, so observing the install is not evidence the Hello has arrived.
async fn wait_for_count(counter: &Arc<AtomicUsize>, want: usize, timeout: Duration) -> usize {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let seen = counter.load(Ordering::SeqCst);
        if seen >= want {
            return seen;
        }
        if tokio::time::Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A QUIC peer that answers every Hello with a valid Hello of its own and counts, on the wire,
/// both the Hellos it received and the physical connections it accepted.
///
/// Counting accepted connections matters for honesty about the mechanism: a redundant dial
/// always opens a fresh QUIC connection *before* the session lookup happens, so the peer sees
/// two connections either way. What must not happen is a second Hello, because that Hello is
/// written by the same `wire_new_connection` call that spawns the duplicate handler.
struct CountingPeer {
    addr: SocketAddr,
    hellos_received: Arc<AtomicUsize>,
    connections_accepted: Arc<AtomicUsize>,
    accepted: Arc<std::sync::Mutex<Vec<quinn::Connection>>>,
    accept_task: tokio::task::JoinHandle<()>,
}

impl CountingPeer {
    /// Close every accepted connection with an explicit CONNECTION_CLOSE.
    ///
    /// Merely dropping the peer is not enough to make the *dialer* consider its cached
    /// connection dead: without a close frame the dialer only finds out at idle timeout, so a
    /// test that just dropped the peer would still be exercising the live-session branch.
    fn close_all(&self) {
        for conn in self.accepted.lock().expect("peer conn lock").iter() {
            conn.close(0u32.into(), b"peer-gone");
        }
    }
}

impl CountingPeer {
    fn spawn(identity: &IdentityBundle) -> Result<Self> {
        let addr_seed: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
        let server_config = icn_net::tls::create_server_config(
            vec![identity.tls_cert().clone()],
            identity.tls_key(),
        )?;
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
        let endpoint = endpoint.context("could not bind the counting peer")?;

        let hellos_received = Arc::new(AtomicUsize::new(0));
        let connections_accepted = Arc::new(AtomicUsize::new(0));
        let accepted: Arc<std::sync::Mutex<Vec<quinn::Connection>>> = Arc::default();
        let hello_counter = hellos_received.clone();
        let conn_counter = connections_accepted.clone();
        let accepted_sink = accepted.clone();
        let claimed_did = identity.did().clone();
        let binding = identity.binding_info();
        let x25519 = *identity.x25519_public_bytes();

        let accept_task = tokio::spawn(async move {
            while let Some(incoming) = endpoint.accept().await {
                let Ok(conn) = incoming.await else { continue };
                conn_counter.fetch_add(1, Ordering::SeqCst);
                accepted_sink
                    .lock()
                    .expect("peer conn lock")
                    .push(conn.clone());
                let hello_counter = hello_counter.clone();
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
                        hello_counter.fetch_add(1, Ordering::SeqCst);
                        let reply = NetworkMessage::hello(
                            claimed_did.clone(),
                            msg.from.clone(),
                            binding.clone(),
                            VersionInfo::new("icnd-counting".to_string()),
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
            connections_accepted,
            accepted,
            accept_task,
        })
    }
}

impl Drop for CountingPeer {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

/// THE regression: dialling a peer we already hold a live session with must not re-wire it.
///
/// The dial still reports success — it is not an error to ask for a connection that already
/// exists — but success here means "you are connected", not "a connection was created". If the
/// caller cannot tell those apart it installs a second handler lifecycle on one connection.
#[tokio::test]
async fn redundant_dial_does_not_rewire_live_connection() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let responder = IdentityBundle::generate()?;
    let responder_did = responder.did().clone();

    let (dialer_handle, _addr, _guard) = spawn_plain_node(dialer).await?;
    let peer = CountingPeer::spawn(&responder)?;

    dialer_handle.dial(peer.addr, responder_did.clone()).await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &responder_did, Duration::from_secs(20)).await,
        "precondition: the first dial never authenticated, so no live session existed to \
         redundantly dial"
    );
    assert_eq!(
        wait_for_installs(&dialer_handle, 1, Duration::from_secs(5)).await,
        1,
        "precondition: the first dial must install exactly one connection handler"
    );

    // The peer is now live and authenticated in the session map. Dial it again.
    dialer_handle
        .dial(peer.addr, responder_did.clone())
        .await
        .context("a redundant dial must still report success")?;

    let installs = wait_for_installs(&dialer_handle, 2, Duration::from_secs(5)).await;
    assert_eq!(
        installs, 1,
        "a redundant dial installed a second connection-handler lifecycle ({installs} total). \
         `SessionManager::dial` returned the *existing* connection and `handle_dial` wired it as \
         though it were new, so one physical connection now has two `handle_connection` loops \
         racing for its inbound streams (#2536)"
    );

    let hellos = peer.hellos_received.load(Ordering::SeqCst);
    assert_eq!(
        hellos, 1,
        "the redundant dial wrote a second Hello onto an already-handshaken connection \
         ({hellos} received). An initiator owes exactly one Hello per connection (#2532), and \
         re-wiring an existing connection breaks that on the wire"
    );

    // Characterisation, not regression: this holds before and after the fix. A redundant dial
    // completes a QUIC handshake *before* the session lookup can happen, so the peer accepts a
    // second physical connection which is then immediately closed as a duplicate. #2536 is not
    // about suppressing that connection — it is about not treating it as a reason to re-wire.
    assert_eq!(
        peer.connections_accepted.load(Ordering::SeqCst),
        2,
        "the redundant dial should still have opened (and closed) a second physical connection"
    );

    Ok(())
}

/// Positive control: a genuinely new connection must still be wired, handshake, and authenticate.
///
/// Without this, a "fix" that never called `wire_new_connection` at all would satisfy the
/// regression above while disabling outbound networking entirely.
#[tokio::test]
async fn fresh_dial_wires_and_authenticates() -> Result<()> {
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
        "a fresh dial must authenticate the responder from its Hello response"
    );
    assert!(
        wait_until_authenticated(&b_handle, &a_did, Duration::from_secs(20)).await,
        "a fresh dial must let the responder authenticate the initiator"
    );
    assert_eq!(
        wait_for_installs(&a_handle, 1, Duration::from_secs(5)).await,
        1,
        "a fresh dial must install exactly one connection handler"
    );
    assert_eq!(a_handle.connected_peers().await, vec![b_did]);

    Ok(())
}

/// Redundant dials must be idempotent however many times they happen.
///
/// `background_tasks` re-dials on a timer, so the interesting failure is not one extra handler
/// but unbounded accumulation: every redundant dial adds one more loop to the same connection.
#[tokio::test]
async fn repeated_redundant_dials_do_not_accumulate_handlers() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let responder = IdentityBundle::generate()?;
    let responder_did = responder.did().clone();

    let (dialer_handle, _addr, _guard) = spawn_plain_node(dialer).await?;
    let peer = CountingPeer::spawn(&responder)?;

    dialer_handle.dial(peer.addr, responder_did.clone()).await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &responder_did, Duration::from_secs(20)).await,
        "precondition: the first dial never authenticated"
    );

    for _ in 0..3 {
        dialer_handle.dial(peer.addr, responder_did.clone()).await?;
    }

    let installs = wait_for_installs(&dialer_handle, 2, Duration::from_secs(5)).await;
    assert_eq!(
        installs, 1,
        "three redundant dials produced {installs} connection-handler installs; handler \
         lifecycles accumulate without bound on a single connection (#2536)"
    );
    assert_eq!(
        peer.hellos_received.load(Ordering::SeqCst),
        1,
        "redundant dials must not write further Hellos onto a live connection"
    );

    // One physical connection remains represented exactly once in authenticated session state.
    assert_eq!(dialer_handle.connected_peers().await, vec![responder_did]);

    Ok(())
}

/// Application traffic must still flow after a redundant dial.
///
/// This is the property that duplicate handlers put at risk: two loops racing `accept_bi` on
/// one connection split inbound streams nondeterministically between two handler stacks.
#[tokio::test]
async fn application_traffic_survives_redundant_dial() -> Result<()> {
    init();
    let a = IdentityBundle::generate()?;
    let a_did = a.did().clone();
    let b = IdentityBundle::generate()?;
    let b_did = b.did().clone();

    let received = Arc::new(AtomicUsize::new(0));
    let (a_handle, _a_addr, _a_guard) = spawn_plain_node(a).await?;
    let (b_handle, b_addr, _b_guard) = spawn_node(b, received.clone(), "Gossip").await?;

    a_handle.dial(b_addr, b_did.clone()).await?;
    assert!(
        wait_until_authenticated(&a_handle, &b_did, Duration::from_secs(20)).await,
        "precondition: initial dial never authenticated"
    );
    assert!(
        wait_until_authenticated(&b_handle, &a_did, Duration::from_secs(20)).await,
        "precondition: responder never authenticated the initiator"
    );

    a_handle.dial(b_addr, b_did.clone()).await?;

    let payload = icn_net::MessagePayload::Gossip(icn_gossip::GossipMessage::Announce {
        hash: Default::default(),
        author: a_did.clone(),
        clock: Default::default(),
        topic: "test:redundant-dial".to_string(),
    });
    a_handle
        .send_message(
            b_did.clone(),
            NetworkMessage::new(a_did, Some(b_did), payload),
        )
        .await
        .context("sending after a redundant dial must succeed")?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while received.load(Ordering::SeqCst) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        received.load(Ordering::SeqCst),
        1,
        "an application message sent after a redundant dial must be delivered exactly once"
    );

    Ok(())
}

/// Preservation (#2505): a *closed* cached connection must still be replaced by a fresh dial.
///
/// The existing-session branch must key off liveness, not mere presence. If a closed entry won,
/// every send after a peer restart would fail until this process restarted.
#[tokio::test]
async fn closed_connection_is_replaced_by_fresh_dial() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let responder = IdentityBundle::generate()?;
    let responder_did = responder.did().clone();

    let (dialer_handle, _addr, _guard) = spawn_plain_node(dialer).await?;

    let first = CountingPeer::spawn(&responder)?;
    let first_addr = first.addr;
    dialer_handle
        .dial(first_addr, responder_did.clone())
        .await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &responder_did, Duration::from_secs(20)).await,
        "precondition: the first peer was never authenticated"
    );
    assert_eq!(
        wait_for_installs(&dialer_handle, 1, Duration::from_secs(5)).await,
        1,
        "precondition: exactly one handler for the first connection"
    );

    // Kill the peer: the cached connection goes dead, mirroring a peer restart. The close must
    // be explicit — see `close_all` — or the dialer still believes the session is live and this
    // would silently become another live-session test instead of the #2505 branch.
    first.close_all();
    drop(first);
    tokio::time::sleep(Duration::from_secs(1)).await;

    // A new peer with the same identity comes up; the stale entry must not win.
    let second = CountingPeer::spawn(&responder)?;
    dialer_handle
        .dial(second.addr, responder_did.clone())
        .await
        .context("dialling a peer whose cached connection is closed must succeed")?;

    let installs = wait_for_installs(&dialer_handle, 2, Duration::from_secs(10)).await;
    assert_eq!(
        installs, 2,
        "a closed cached connection must be passed over so the freshly dialled one is wired; \
         only {installs} handler install(s) happened, so the dead entry won (#2505)"
    );
    assert_eq!(
        wait_for_count(&second.hellos_received, 1, Duration::from_secs(10)).await,
        1,
        "the replacement connection must run its own Hello handshake"
    );

    Ok(())
}
