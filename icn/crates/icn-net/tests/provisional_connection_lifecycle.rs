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

fn pick_port() -> u16 {
    portpicker::pick_unused_port().expect("no free port")
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
    let addr: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
    let (shutdown_tx, _) = broadcast::channel(1);
    let handler: IncomingMessageHandler = Arc::new(move |msg: NetworkMessage| {
        if !variant.is_empty() && msg.payload.variant_name() == variant {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
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
    Ok((handle, addr, Guard(shutdown_tx)))
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

        let (_send, mut recv) = tokio::time::timeout(timeout, self.connection.accept_bi())
            .await
            .context("timed out waiting for the peer exchange response stream")?
            .context("peer exchange response stream failed")?;
        let (message, _len) =
            tokio::time::timeout(timeout, icn_net::protocol::read_message(&mut recv))
                .await
                .context("timed out reading the peer exchange response")?
                .context("failed to read the peer exchange response")?;

        match message.payload {
            MessagePayload::PeerExchange(PeerExchangeMessage::Response { peers, .. }) => {
                Ok(peers.into_iter().map(|peer| peer.did).collect())
            }
            other => anyhow::bail!("expected a PeerExchange::Response, got {other:?}"),
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

/// A dial keyed on a DID that turns out not to be who answered must not leave that DID
/// behind as a peer.
///
/// `SessionManager::dial` registers the caller's *claimed* key before any handshake has
/// happened, so a bootstrap entry carrying a stale or wrong DID for an address registers a
/// peer that does not exist. Hello then authenticates whoever actually answered and
/// registers them too, leaving one connection under two identities — the same aliasing the
/// address-only path produces, reached by a different route. This is what the eviction in
/// `install_incoming_connection` covers, independently of where the stale key came from.
#[tokio::test]
async fn dial_keyed_on_the_wrong_did_does_not_leave_a_phantom_peer() -> Result<()> {
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

    let client = WireClient::connect(&interrogator, dialer_addr).await?;
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

    // Let the handshake settle before broadcasting.
    //
    // Two nodes currently reflect Hellos at each other until the receiver's per-DID rate
    // limiter starts dropping them — `handle_hello` answers every Hello with a Hello, and a
    // Hello response is indistinguishable on the wire from an initial one. That burst
    // exhausts the sender's rate-limit budget, so a broadcast issued immediately after
    // authentication is dropped before dispatch and this test would read 0 for a reason
    // that has nothing to do with connection aliasing. The reflection is pre-existing on
    // `main` and out of scope here; waiting for the limiter's window to refill sidesteps it
    // without hiding it.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    received.store(0, Ordering::SeqCst);

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

    // A bare QUIC listener: completes the handshake, accepts the connection, and then
    // says nothing. No Hello, so the dialer never learns an authenticated identity.
    let silent_addr: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
    let server_config =
        icn_net::tls::create_server_config(vec![silent.tls_cert().clone()], silent.tls_key())?;
    let mut quic_server = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_config)?,
    ));
    quic_server.transport_config(Arc::new({
        let mut t = quinn::TransportConfig::default();
        t.max_idle_timeout(Some(Duration::from_secs(60).try_into()?));
        t
    }));
    let silent_endpoint = Endpoint::server(quic_server, silent_addr)?;
    let accepted = tokio::spawn({
        let endpoint = silent_endpoint.clone();
        async move {
            // Hold the accepted connection open; dropping it would close the connection
            // and let the dialer clean up for the wrong reason.
            let mut held = Vec::new();
            while let Some(incoming) = endpoint.accept().await {
                if let Ok(conn) = incoming.await {
                    held.push(conn);
                }
            }
            held
        }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let placeholder = dialer_handle.dial_addr(silent_addr).await?;

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

    accepted.abort();
    Ok(())
}
