#![allow(clippy::unwrap_used, clippy::expect_used)]
//! An address-only bootstrap dial must not leave a second, unauthenticated identity
//! behind once the peer authenticates.
//!
//! `dial_addr` has no authenticated remote DID to key on, so it derives a placeholder
//! from the socket address alone (`derive_placeholder_did`). That placeholder is local
//! transport bookkeeping: it has no private key, nobody can ever authenticate as it, and
//! it is computable offline by anyone who knows the address. The peer's real DID arrives
//! later, on the Hello handshake, verified against the certificate of *this* connection
//! (#2520).
//!
//! The session manager's connection map is documented as mapping *authenticated remote
//! peer DIDs* to connections — `install_incoming_connection` states the precondition
//! outright — and its readers act on that: peer exchange publishes its keys to other
//! nodes as topology, broadcast opens one stream per entry, and `connections_active`
//! counts entries as peers. So a placeholder that survives past Hello is not a cosmetic
//! duplicate; it is an unauthenticated key sitting in an authenticated-only structure.
//!
//! These tests pin the lifecycle through the production composition path — two real
//! `NetworkActor`s, real QUIC/TLS, real dispatch loop, real `handle_hello` — and assert
//! on observable surfaces (`get_stats`, the peer-exchange wire response) rather than on
//! private state. See #2530.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_net::{
    IncomingMessageHandler, MessagePayload, NetworkActor, NetworkHandle, NetworkMessage,
    PeerExchangeMessage,
};
use quinn::{ClientConfig, Endpoint};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Ports already handed out in this process.
///
/// `pick_unused_port` binds, closes, and returns the port, so two tests running
/// concurrently can be handed the same one — the port is genuinely free at each moment it
/// is checked. Remembering what we issued removes that race within the process; callers
/// still retry, because nothing can rule out another process taking it in between.
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
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` only starts the inbound
/// accept loop when `incoming_handler` is `Some`, and it is also what makes
/// `wire_new_connection` spawn a dispatch loop for an *outbound* connection. Without it
/// the dialer would never process the peer's Hello response, so nothing in this file
/// would authenticate and every assertion would pass or fail for the wrong reason.
async fn spawn_node(bundle: IdentityBundle) -> Result<(NetworkHandle, SocketAddr, Guard)> {
    spawn_node_counting(bundle, Arc::new(AtomicUsize::new(0)), "").await
}

/// Spawn a real node whose handler counts inbound messages of one payload variant.
///
/// Counting at the *handler* — the far end of the receiver's dispatch loop — is what makes
/// the broadcast assertion meaningful: it counts messages that actually arrived and were
/// decoded, not streams the sender believes it opened.
async fn spawn_node_counting(
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

    // Binding is setup, not the property under test, so it retries: a port reported free can
    // still be taken by another process before we bind it.
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
                // These tests are about *what* a node advertises, so it has to be a node
                // that advertises at all. Peer exchange is off by default (#2535), which is
                // a separate policy question from the one under test here.
                handle.set_peer_exchange_enabled(true);
                // Let the endpoint bind before anyone dials it.
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, addr, Guard(shutdown_tx)));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

/// Keeps a node's shutdown sender alive for the duration of a test.
struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// Poll until `observer` has authenticated `did`, or the deadline passes.
///
/// `peer_connections` is written only by `handle_hello`, after the #2520 DID-TLS binding
/// checks pass, so this returning `true` is proof the handshake completed and the peer's
/// real identity is known — the exact moment after which a provisional key has no reason
/// to exist. Synchronising on that observable, rather than sleeping, is what keeps the
/// negative assertions below non-vacuous: "placeholder already evicted" and "Hello not
/// processed yet" would otherwise look identical.
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

/// A raw QUIC client that speaks the wire protocol without being an ICN node.
///
/// Used to interrogate a node's peer-exchange surface from outside: what a *remote* node
/// is told is the property under test, and reading it off the wire keeps the assertion
/// independent of how the responder happens to store its connections.
struct WireClient {
    _endpoint: Endpoint,
    connection: quinn::Connection,
    identity: IdentityBundle,
}

impl WireClient {
    async fn connect(identity: &IdentityBundle, target: SocketAddr) -> Result<Self> {
        let rustls_client = icn_net::tls::create_tofu_client_config(
            vec![identity.tls_cert().clone()],
            identity.tls_key(),
        )?;
        let mut endpoint = Endpoint::client("127.0.0.1:0".parse()?)?;
        endpoint.set_default_client_config(ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client)?,
        )));

        // Establishing the connection is a precondition, not the property under test, so
        // it retries. No assertion below retries.
        let mut last_err = None;
        for attempt in 0..5 {
            match tokio::time::timeout(
                Duration::from_secs(10),
                endpoint.connect(target, "localhost")?,
            )
            .await
            {
                Ok(Ok(connection)) => {
                    return Ok(Self {
                        _endpoint: endpoint,
                        connection,
                        identity: identity.clone(),
                    })
                }
                Ok(Err(e)) => last_err = Some(e.to_string()),
                Err(_) => last_err = Some("connect timed out".to_string()),
            }
            tokio::time::sleep(Duration::from_millis(150 * (attempt + 1))).await;
        }
        anyhow::bail!(
            "could not establish the test connection after 5 attempts: {}",
            last_err.unwrap_or_default()
        )
    }

    /// Complete an authenticated Hello, so the responder knows who is asking.
    ///
    /// Peer exchange does not answer an unauthenticated connection (#2535), so an
    /// interrogator that wants to read the advertised peer set has to be a peer first.
    /// This uses the client's own binding and its own certificate, which is exactly what
    /// the #2520 check requires.
    async fn authenticate(&self, to: &Did) -> Result<()> {
        let hello = NetworkMessage::hello(
            self.identity.did().clone(),
            to.clone(),
            self.identity.binding_info(),
            icn_net::VersionInfo::new("icnd-interrogator".to_string()),
            None,
            *self.identity.x25519_public_bytes(),
            None,
            None,
        );
        let (mut send, _recv) = self.connection.open_bi().await?;
        icn_net::protocol::write_message(&mut send, &hello).await?;
        send.finish().context("finish hello")?;
        Ok(())
    }

    /// Ask the peer for its known peers and return the DIDs it advertises.
    ///
    /// The responder answers on a *new* bidirectional stream it opens itself, so this
    /// accepts an inbound stream rather than reading the reply half of the request.
    async fn request_peer_exchange(
        &self,
        from: &Did,
        to: &Did,
        timeout: Duration,
    ) -> Result<Vec<String>> {
        let request = NetworkMessage::peer_exchange_request(from.clone(), to.clone(), 64, None);
        let (mut send, _recv) = self.connection.open_bi().await?;
        icn_net::protocol::write_message(&mut send, &request).await?;
        send.finish().context("finish peer exchange request")?;

        // An authenticated interrogator is answered with the responder's own Hello too, on
        // its own stream and first (#2537), so skip anything that is not the answer.
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            anyhow::ensure!(
                !remaining.is_zero(),
                "timed out waiting for the peer exchange response"
            );
            let (_send, mut recv) = tokio::time::timeout(remaining, self.connection.accept_bi())
                .await
                .context("timed out waiting for the peer exchange response stream")?
                .context("peer exchange response stream failed")?;
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let (message, _len) =
                tokio::time::timeout(remaining, icn_net::protocol::read_message(&mut recv))
                    .await
                    .context("timed out reading the peer exchange response")?
                    .context("failed to read the peer exchange response")?;

            if let MessagePayload::PeerExchange(PeerExchangeMessage::Response { peers, .. }) =
                message.payload
            {
                return Ok(peers.into_iter().map(|peer| peer.did).collect());
            }
        }
    }
}

/// A successful address-only dial that authenticates must leave exactly one connection
/// identity behind — the authenticated DID.
///
/// `connections_active` is the count of session-map entries and feeds the
/// `icn_network_connections_active` gauge, so a surviving placeholder is not merely
/// untidy: it reports one physical QUIC connection as two peers.
#[tokio::test]
async fn addr_only_dial_that_authenticates_leaves_one_connection_identity() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let listener = IdentityBundle::generate()?;
    let listener_did = listener.did().clone();

    let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

    // Address-only bootstrap: no DID is known in advance.
    let placeholder = dialer_handle.dial_addr(listener_addr).await?;
    assert_ne!(
        placeholder, listener_did,
        "precondition: the placeholder must not coincide with the peer's real DID"
    );

    assert!(
        wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
        "precondition: the dialer never authenticated the peer, so nothing about the \
         provisional key's fate has been exercised"
    );

    let stats = dialer_handle.get_stats().await?;
    assert_eq!(
        stats.connections_active, 1,
        "one physical QUIC connection must have exactly one identity in the session map \
         once the peer has authenticated; found {} entries, which means the provisional \
         placeholder {placeholder} survived alongside the authenticated DID {listener_did} \
         (#2530)",
        stats.connections_active
    );

    Ok(())
}

/// Peer exchange must advertise only authenticated peers.
///
/// The placeholder is derivable offline from the address by anyone, and no peer can ever
/// authenticate as it. Advertising it hands other nodes a phantom identity that they will
/// treat as real topology.
#[tokio::test]
async fn peer_exchange_must_not_advertise_address_derived_placeholders() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let dialer_did = dialer.did().clone();
    let listener = IdentityBundle::generate()?;
    let listener_did = listener.did().clone();
    let interrogator = IdentityBundle::generate()?;
    let interrogator_did = interrogator.did().clone();

    let (dialer_handle, dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

    let placeholder = dialer_handle.dial_addr(listener_addr).await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
        "precondition: the dialer never authenticated the peer"
    );

    let client = WireClient::connect(&interrogator, dialer_addr).await?;
    // Reading the advertised set requires being a peer, not merely a socket (#2535).
    client.authenticate(&dialer_did).await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &interrogator_did, Duration::from_secs(20))
            .await,
        "precondition: the interrogator must authenticate before it can read the          advertised peer set"
    );
    let advertised = client
        .request_peer_exchange(&interrogator_did, &dialer_did, Duration::from_secs(20))
        .await?;

    // Positive control: without this, the negative assertion below would also pass on an
    // empty response — i.e. if peer exchange were broken outright.
    assert!(
        advertised.contains(&listener_did.to_string()),
        "precondition: the authenticated peer {listener_did} should be advertised, got {advertised:?}"
    );
    assert!(
        !advertised.contains(&placeholder.to_string()),
        "peer exchange advertised the address-derived placeholder {placeholder} as a peer \
         identity; no node can ever authenticate as it, so every recipient now carries a \
         phantom topology entry (#2530). Advertised: {advertised:?}"
    );

    Ok(())
}

/// When the supplied DID is not who answered, only the DID that authenticated becomes
/// canonical — and the supplied one is never exposed as a peer.
///
/// This pins the **hint** semantics, which is what ICN actually implements: per
/// `parse_bootstrap_peer`, a supplied DID "yields configuration", and identity "is proven
/// only by DID-TLS binding verification in the Hello handler". Nothing anywhere compares
/// the supplied DID against the authenticated one, so a supplied DID is *not* a
/// cryptographic pin, and this test deliberately does not assert that a mismatch is
/// rejected.
///
/// What it does assert is the part that is a trust boundary either way: the unproven DID
/// must never appear as authenticated topology, and one physical connection must end up
/// under exactly one identity. Making a supplied DID an enforced pin is a separate design
/// decision with real operational consequences — a stale bootstrap entry would become a
/// hard connectivity failure rather than a degraded one — tracked in #2533.
#[tokio::test]
async fn only_the_authenticated_did_becomes_canonical_when_the_supplied_did_differs() -> Result<()>
{
    init();
    let dialer = IdentityBundle::generate()?;
    let dialer_did = dialer.did().clone();
    let listener = IdentityBundle::generate()?;
    let listener_did = listener.did().clone();
    let interrogator = IdentityBundle::generate()?;
    let interrogator_did = interrogator.did().clone();

    // A DID that belongs to nobody on this connection.
    let wrong_did = IdentityBundle::generate()?.did().clone();

    let (dialer_handle, dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

    dialer_handle.dial(listener_addr, wrong_did.clone()).await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
        "precondition: the dialer never authenticated the peer that actually answered"
    );

    let stats = dialer_handle.get_stats().await?;
    assert_eq!(
        stats.connections_active, 1,
        "the connection must be held under the DID that authenticated, not also under the \
         DID we guessed; found {} entries, so {wrong_did} survived as a phantom peer (#2530)",
        stats.connections_active
    );

    let authenticated = dialer_handle.connected_peers().await;
    assert_eq!(
        authenticated,
        vec![listener_did.clone()],
        "only the DID proven by Hello may be reported as an authenticated peer (#2530)"
    );

    let client = WireClient::connect(&interrogator, dialer_addr).await?;
    // Reading the advertised set requires being a peer, not merely a socket (#2535).
    client.authenticate(&dialer_did).await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &interrogator_did, Duration::from_secs(20))
            .await,
        "precondition: the interrogator must authenticate before it can read the          advertised peer set"
    );
    let advertised = client
        .request_peer_exchange(&interrogator_did, &dialer_did, Duration::from_secs(20))
        .await?;
    assert!(
        advertised.contains(&listener_did.to_string()),
        "precondition: the authenticated peer should be advertised, got {advertised:?}"
    );
    assert!(
        !advertised.contains(&wrong_did.to_string()),
        "peer exchange advertised {wrong_did}, a DID that never authenticated on this \
         connection, as reachable topology (#2530). Advertised: {advertised:?}"
    );

    Ok(())
}

/// A broadcast must write once per physical connection, not once per map key.
///
/// `broadcast_message` iterates the session map and opens a stream per entry, so two keys
/// aliasing one connection meant the peer received every broadcast twice — duplicate
/// delivery and double the stream budget on a connection limited to 10 concurrent
/// bidirectional streams.
#[tokio::test]
async fn broadcast_writes_once_per_physical_connection() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let dialer_did = dialer.did().clone();
    let listener = IdentityBundle::generate()?;
    let listener_did = listener.did().clone();

    let received = Arc::new(AtomicUsize::new(0));
    let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let (_listener_handle, listener_addr, _listener_guard) =
        spawn_node_counting(listener, received.clone(), "Subscribe").await?;

    let placeholder = dialer_handle.dial_addr(listener_addr).await?;
    assert!(
        wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
        "precondition: the dialer never authenticated the peer"
    );

    // Broadcast immediately — no settling delay.
    //
    // This used to wait out the per-DID rate-limit window, because the Hello exchange
    // reflected until the limiter denied one and left no budget for real traffic (#2532).
    // The handshake now terminates on protocol state, so the budget is intact and a
    // broadcast issued the moment authentication lands is delivered. If #2532 ever
    // regresses, this reads 0 rather than 1.

    dialer_handle
        .broadcast(NetworkMessage::subscribe(
            dialer_did.clone(),
            listener_did.clone(),
            vec!["test:topic".to_string()],
        ))
        .await?;

    // Poll up to the deadline for the first delivery, then keep waiting out the remainder
    // so a second, duplicate delivery has time to land and be counted. Asserting as soon
    // as the count reaches 1 would pass even while a duplicate was still in flight.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while received.load(Ordering::SeqCst) == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    tokio::time::sleep(Duration::from_millis(750)).await;

    assert_eq!(
        received.load(Ordering::SeqCst),
        1,
        "one broadcast over one physical QUIC connection must be delivered once; the \
         provisional placeholder {placeholder} aliasing the authenticated DID {listener_did} \
         makes broadcast open a second stream and send the message twice (#2530)"
    );

    Ok(())
}

/// A connection that completes QUIC but never authenticates must leave nothing behind.
///
/// This is the failure mode a re-key-at-Hello fix does not cover: if the fix only evicts
/// the placeholder when Hello arrives, a peer that accepts the connection and stays silent
/// strands the placeholder permanently. There is no reaper — `disconnect_peer` and `stop`
/// are the only removals — so "permanently" means until the process exits.
#[tokio::test]
async fn addr_only_dial_that_never_authenticates_leaves_no_peer_identity() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let silent = IdentityBundle::generate()?;

    let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let silent = SilentPeer::spawn(&silent)?;

    let placeholder = dialer_handle.dial_addr(silent.addr).await?;

    // Give the dialer ample opportunity to have registered something. There is no
    // observable "authentication finished" event here — that is the point — so this
    // waits out the window in which a Hello would plausibly have arrived.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let stats = dialer_handle.get_stats().await?;
    assert_eq!(
        stats.connections_active, 0,
        "a connection whose peer never authenticated must not appear as an active peer; \
         found {} entries, so the provisional placeholder {placeholder} was promoted into \
         authenticated peer state on QUIC success alone (#2530)",
        stats.connections_active
    );

    Ok(())
}

/// A **DID-supplied** dial whose peer never authenticates must leave nothing behind either.
///
/// This is the same rule as the address-only case, reached through the ordinary
/// `dial(addr, did)` path. A supplied DID is configuration or, worse, a claim relayed by
/// another node through peer exchange — `init_network`'s auto-dial feeds
/// `KnownPeer.did` straight into this path. `parse_bootstrap_peer` is explicit that
/// neither bootstrap form authenticates anything: "parsing yields configuration, and the
/// peer's identity is proven only by DID-TLS binding verification in the Hello handler."
///
/// So transport success must not promote the supplied DID into authenticated peer state.
/// Otherwise any node can name an arbitrary DID at a reachable address and have every
/// receiver adopt it as a peer, then re-advertise it — the placeholder was one instance of
/// this, not the whole of it (#2530).
#[tokio::test]
async fn did_supplied_dial_that_never_authenticates_leaves_no_peer_identity() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let silent_identity = IdentityBundle::generate()?;

    // The DID a caller claims lives at that address. Nothing has proven it.
    let claimed_did = IdentityBundle::generate()?.did().clone();

    let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let silent = SilentPeer::spawn(&silent_identity)?;

    dialer_handle.dial(silent.addr, claimed_did.clone()).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    let stats = dialer_handle.get_stats().await?;
    assert_eq!(
        stats.connections_active, 0,
        "QUIC success is not authentication: the claimed DID {claimed_did} must not become \
         an active peer without an authenticated Hello; found {} entries (#2530)",
        stats.connections_active
    );

    let authenticated = dialer_handle.connected_peers().await;
    assert!(
        authenticated.is_empty(),
        "connected_peers() reports authenticated peers, so an unauthenticated claimed DID \
         must never appear in it; got {authenticated:?} (#2530)"
    );

    Ok(())
}

/// A peer whose Hello fails the #2520 binding checks must not become authenticated state.
///
/// The dial supplies a DID and the peer answers with a Hello, so both of the inputs that
/// could tempt an implementation into registering something are present — but the Hello is
/// bound to a different certificate than the one on this connection, so it proves nothing.
/// Neither the supplied DID nor the DID the Hello claims may end up as a peer, and #2530
/// must not have opened a path that installs before verification.
#[tokio::test]
async fn peer_whose_hello_fails_binding_verification_becomes_no_ones_peer() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    // Presents this identity's certificate on the wire...
    let wire_identity = IdentityBundle::generate()?;
    // ...but claims to be this one, with that one's own (valid, but wrong-cert) binding.
    let impersonated = IdentityBundle::generate()?;
    let impersonated_did = impersonated.did().clone();
    let claimed_did = IdentityBundle::generate()?.did().clone();

    let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let liar = SilentPeer::spawn_replying_with_hello(
        &wire_identity,
        NetworkMessage::hello(
            impersonated_did.clone(),
            claimed_did.clone(),
            impersonated.binding_info(),
            icn_net::VersionInfo::new("icnd-liar".to_string()),
            None,
            *impersonated.x25519_public_bytes(),
            None,
            None,
        ),
    )?;

    dialer_handle.dial(liar.addr, claimed_did.clone()).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    let stats = dialer_handle.get_stats().await?;
    assert_eq!(
        stats.connections_active, 0,
        "a Hello bound to a different certificate proves nothing, so neither the supplied \
         DID {claimed_did} nor the claimed DID {impersonated_did} may become a peer; found \
         {} entries (#2520, #2530)",
        stats.connections_active
    );
    assert!(
        dialer_handle
            .get_peer_connection_info(&impersonated_did)
            .await
            .is_none(),
        "a rejected Hello must not populate authenticated peer info (#2520)"
    );

    Ok(())
}

/// Positive control for the test above: the same scripted-peer harness, with a Hello whose
/// binding *does* match the certificate it is presenting, must authenticate.
///
/// Without this, `peer_whose_hello_fails_binding_verification_becomes_no_ones_peer` would
/// pass for free if the harness silently failed to deliver a Hello at all — the assertion
/// there is an absence, and an absence proves nothing unless presence is reachable.
#[tokio::test]
async fn scripted_peer_with_a_matching_binding_does_authenticate() -> Result<()> {
    init();
    let dialer = IdentityBundle::generate()?;
    let honest = IdentityBundle::generate()?;
    let honest_did = honest.did().clone();
    let claimed_did = IdentityBundle::generate()?.did().clone();

    let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
    let peer = SilentPeer::spawn_replying_with_hello(
        &honest,
        NetworkMessage::hello(
            honest_did.clone(),
            claimed_did.clone(),
            honest.binding_info(),
            icn_net::VersionInfo::new("icnd-honest".to_string()),
            None,
            *honest.x25519_public_bytes(),
            None,
            None,
        ),
    )?;

    dialer_handle.dial(peer.addr, claimed_did.clone()).await?;

    assert!(
        wait_until_authenticated(&dialer_handle, &honest_did, Duration::from_secs(20)).await,
        "the harness must be able to deliver an acceptable Hello, otherwise the rejection \
         test above proves nothing"
    );
    assert_eq!(
        dialer_handle.connected_peers().await,
        vec![honest_did],
        "only the authenticated DID becomes a peer — not the DID we dialed with"
    );

    Ok(())
}

/// A bare QUIC listener that completes the handshake and then says nothing.
///
/// No Hello, so a dialer never learns an authenticated identity from it. It holds accepted
/// connections open: dropping them would close the connection and let the dialer clean up
/// for the wrong reason, which would make the assertions pass vacuously.
struct SilentPeer {
    addr: SocketAddr,
    accept_task: tokio::task::JoinHandle<Vec<quinn::Connection>>,
}

impl SilentPeer {
    fn spawn(identity: &IdentityBundle) -> Result<Self> {
        Self::spawn_inner(identity, None)
    }

    /// Same, but answers each accepted connection with `hello` on a fresh stream.
    fn spawn_replying_with_hello(identity: &IdentityBundle, hello: NetworkMessage) -> Result<Self> {
        Self::spawn_inner(identity, Some(hello))
    }

    fn spawn_inner(identity: &IdentityBundle, hello: Option<NetworkMessage>) -> Result<Self> {
        let addr: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
        let server_config = icn_net::tls::create_server_config(
            vec![identity.tls_cert().clone()],
            identity.tls_key(),
        )?;
        let mut quic_server = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_config)?,
        ));
        quic_server.transport_config(Arc::new({
            let mut t = quinn::TransportConfig::default();
            t.max_idle_timeout(Some(Duration::from_secs(60).try_into()?));
            t
        }));
        // Same retry rationale as `spawn_node_counting`.
        let mut endpoint = None;
        let mut addr = addr;
        for _ in 0..8 {
            match Endpoint::server(quic_server.clone(), addr) {
                Ok(ep) => {
                    endpoint = Some(ep);
                    break;
                }
                Err(_) => addr = format!("127.0.0.1:{}", pick_port()).parse()?,
            }
        }
        let endpoint = endpoint.context("could not bind the scripted peer")?;
        let accept_task = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Some(incoming) = endpoint.accept().await {
                if let Ok(conn) = incoming.await {
                    if let Some(ref hello) = hello {
                        if let Ok((mut send, _recv)) = conn.open_bi().await {
                            let _ = icn_net::protocol::write_message(&mut send, hello).await;
                            let _ = send.finish();
                        }
                    }
                    held.push(conn);
                }
            }
            held
        });
        std::thread::sleep(Duration::from_millis(300));
        Ok(Self { addr, accept_task })
    }
}

impl Drop for SilentPeer {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}
