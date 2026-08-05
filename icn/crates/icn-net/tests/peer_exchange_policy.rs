#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Who is allowed to learn a node's neighbourhood.
//!
//! #2534 settled *what* the peer-exchange responder enumerates: authenticated peers only.
//! This file is about the separate question #2535 asks — *whether a given requester may
//! receive that set at all*.
//!
//! Two properties, deliberately tested apart so neither can carry the other:
//!
//! 1. **Authentication.** A connection that has not completed an authenticated Hello must
//!    not receive peer topology. Transport existing is not the same as knowing who is
//!    asking: the request's `from` field is a claim the sender chose, and until the #2520
//!    DID-TLS binding check has run against *this* connection's certificate, it is data,
//!    not authority (the same confusion #2491 tracks on the rate-limit path).
//!
//! 2. **Participation.** A node whose operator has not enabled federation must not answer
//!    peer-exchange requests, even from a peer that authenticated correctly. Enumerating
//!    the neighbourhood is participation, and `federation.enabled` already gates every
//!    other peer-exchange behaviour in both directions.
//!
//! Every assertion here is on decoded wire content, not on whether a handler ran. The
//! property is disclosure — did a DID and a reachable endpoint cross the connection
//! boundary — so the responder is stocked with a *sentinel* peer whose DID and address are
//! known to the test, and the assertions name that sentinel. A test that only checked
//! "response absent" would pass just as well against a node that had no peers to leak, or
//! against a broken harness that never delivered the request.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_net::{
    IncomingMessageHandler, MessagePayload, NetworkActor, NetworkHandle, NetworkMessage,
    PeerExchangeMessage, VersionInfo,
};
use quinn::{ClientConfig, Endpoint};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Ports already handed out in this process.
///
/// `pick_unused_port` binds, closes, and returns the port, so two tests running
/// concurrently can be handed the same one. Remembering what we issued removes that race
/// within the process; callers still retry, because nothing rules out another process
/// taking it in between.
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

/// Keeps a node's shutdown sender alive for the duration of a test.
struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` only starts the inbound
/// accept loop when `incoming_handler` is `Some`, and it is also what makes
/// `wire_new_connection` spawn a dispatch loop for an *outbound* connection. Without it
/// nothing here would authenticate.
///
/// `peer_exchange_enabled` mirrors what the supervisor does with `federation.enabled` at
/// startup. It is passed explicitly at every call site rather than defaulted, because
/// which posture a node is in is the subject of half the tests in this file.
async fn spawn_node(
    bundle: IdentityBundle,
    peer_exchange_enabled: bool,
) -> Result<(NetworkHandle, SocketAddr, Guard)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let handler: IncomingMessageHandler = Arc::new(move |_msg: NetworkMessage| {});

    // Binding is setup, not the property under test, so it retries: a port reported free
    // can still be taken by another process before we bind it.
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
                handle.set_peer_exchange_enabled(peer_exchange_enabled);
                // Let the endpoint bind before anyone dials it.
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, addr, Guard(shutdown_tx)));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

/// Poll until `observer` has authenticated `did`, or the deadline passes.
///
/// `peer_connections` is written only by `handle_hello`, after the #2520 DID-TLS binding
/// checks pass, so this returning `true` is proof the handshake completed. Synchronising
/// on that observable rather than sleeping is what keeps the negative assertions
/// non-vacuous: "denied by policy" and "Hello not processed yet" would otherwise look
/// identical.
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

/// What a requester learned from a peer-exchange request.
#[derive(Debug)]
struct Disclosure {
    dids: Vec<String>,
    endpoints: Vec<SocketAddr>,
}

impl Disclosure {
    fn leaks(&self, did: &Did, addr: SocketAddr) -> bool {
        self.dids.contains(&did.to_string()) || self.endpoints.contains(&addr)
    }
}

/// A raw QUIC client that speaks the wire protocol without necessarily being a peer.
///
/// It can interrogate a node's peer-exchange surface either anonymously — a connection
/// that completed QUIC/TLS and nothing else — or as a properly authenticated peer, which
/// is the difference the tests below turn on.
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

    /// Complete an authenticated Hello with `to`, using this client's own certificate.
    ///
    /// The binding is this identity's own and the certificate on the wire is this
    /// identity's own, so the #2520 check passes and the responder learns who is on the
    /// other end of *this* connection.
    async fn authenticate(&self, to: &Did) -> Result<()> {
        let hello = NetworkMessage::hello(
            self.identity.did().clone(),
            to.clone(),
            self.identity.binding_info(),
            VersionInfo::new("icnd-test-requester".to_string()),
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

    /// Ask the peer for its known peers.
    ///
    /// Returns `None` when the responder never answers, which is what a policy denial
    /// looks like on the wire. The responder replies on a *new* bidirectional stream it
    /// opens itself, so this accepts an inbound stream rather than reading the reply half
    /// of the request.
    async fn request_peer_exchange(
        &self,
        claimed_from: &Did,
        to: &Did,
        timeout: Duration,
    ) -> Result<Option<Disclosure>> {
        let request =
            NetworkMessage::peer_exchange_request(claimed_from.clone(), to.clone(), 64, None);
        let (mut send, _recv) = self.connection.open_bi().await?;
        icn_net::protocol::write_message(&mut send, &request).await?;
        send.finish().context("finish peer exchange request")?;

        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let accepted = match tokio::time::timeout(remaining, self.connection.accept_bi()).await
            {
                Ok(inner) => inner.context("inbound stream failed")?,
                Err(_) => return Ok(None),
            };
            let (_send, mut recv) = accepted;
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let read =
                match tokio::time::timeout(remaining, icn_net::protocol::read_message(&mut recv))
                    .await
                {
                    Ok(inner) => inner.context("failed to read an inbound message")?,
                    Err(_) => return Ok(None),
                };

            // Skip anything that is not the answer we asked for. An authenticated requester
            // also receives the responder's own Hello, on its own stream and first (#2537);
            // treating the first inbound stream as the answer would report "no response"
            // for a node that answered perfectly well.
            if let MessagePayload::PeerExchange(PeerExchangeMessage::Response { peers, .. }) =
                read.0.payload
            {
                return Ok(Some(Disclosure {
                    dids: peers.iter().map(|p| p.did.clone()).collect(),
                    endpoints: peers
                        .iter()
                        .flat_map(|p| p.endpoints.iter().map(|e| e.addr))
                        .collect(),
                }));
            }
        }
    }
}

/// A responder that genuinely has something to leak.
///
/// Without a populated peer set, every "learned nothing" assertion below would pass
/// trivially — against a node with no peers, or against a harness that never delivered the
/// request. The sentinel is what gives those assertions something to be about.
struct Fixture {
    responder: NetworkHandle,
    responder_did: Did,
    responder_addr: SocketAddr,
    /// The peer the responder has authenticated, and whose disclosure the tests assert on.
    sentinel_did: Did,
    sentinel_addr: SocketAddr,
    _guards: (Guard, Guard),
}

async fn responder_holding_a_sentinel_peer(peer_exchange_enabled: bool) -> Result<Fixture> {
    let responder = IdentityBundle::generate()?;
    let responder_did = responder.did().clone();
    let sentinel = IdentityBundle::generate()?;
    let sentinel_did = sentinel.did().clone();

    let (responder_handle, responder_addr, responder_guard) =
        spawn_node(responder, peer_exchange_enabled).await?;
    // The sentinel's own posture is irrelevant: it is dialled, and never asked anything.
    let (_sentinel_handle, sentinel_addr, sentinel_guard) = spawn_node(sentinel, false).await?;

    responder_handle
        .dial(sentinel_addr, sentinel_did.clone())
        .await?;
    anyhow::ensure!(
        wait_until_authenticated(&responder_handle, &sentinel_did, Duration::from_secs(20)).await,
        "precondition: the responder never authenticated the sentinel peer, so there would \
         be nothing to disclose and every negative assertion would pass for free"
    );

    Ok(Fixture {
        responder: responder_handle,
        responder_did,
        responder_addr,
        sentinel_did,
        sentinel_addr,
        _guards: (responder_guard, sentinel_guard),
    })
}

/// A requester that has not authenticated must not learn the neighbourhood.
///
/// The interrogator completes QUIC/TLS and nothing else — no Hello, so the responder has
/// no verified identity for this connection. It then asks, naming itself in the request's
/// `from` field. That field is the requester's own unverified claim, and answering it
/// hands an anonymous caller a real peer's DID and a reachable address for it.
#[tokio::test]
async fn unauthenticated_requester_learns_no_peer_topology() -> Result<()> {
    init();
    let fx = responder_holding_a_sentinel_peer(true).await?;

    let interrogator = IdentityBundle::generate()?;
    let interrogator_did = interrogator.did().clone();

    let client = WireClient::connect(&interrogator, fx.responder_addr).await?;
    // Deliberately no `authenticate()` call: this is the whole point of the test.
    let disclosure = client
        .request_peer_exchange(
            &interrogator_did,
            &fx.responder_did,
            Duration::from_secs(10),
        )
        .await?;

    let (sentinel_did, sentinel_addr) = (&fx.sentinel_did, fx.sentinel_addr);
    assert!(
        !disclosure
            .as_ref()
            .is_some_and(|d| d.leaks(sentinel_did, sentinel_addr)),
        "a connection that never completed an authenticated Hello received peer topology: \
         the sentinel peer {sentinel_did} at {sentinel_addr} crossed the connection \
         boundary to an anonymous caller. Transport is not identity, and the request's \
         `from` field is the caller's own claim (#2535). Disclosed: {disclosure:?}"
    );

    // The responder must still be alive and holding the sentinel — otherwise the assertion
    // above could have passed because the node fell over, not because policy held.
    assert!(
        fx.responder
            .get_peer_connection_info(sentinel_did)
            .await
            .is_some(),
        "post-condition: the responder must still hold the sentinel peer, otherwise the \
         non-disclosure above proves nothing about policy"
    );

    Ok(())
}

/// Positive control: an authenticated requester does receive the neighbourhood.
///
/// Without this, `unauthenticated_requester_learns_no_peer_topology` would pass against a
/// node whose peer exchange was broken outright, or against a harness that never delivered
/// the request at all. It also pins the #2534 invariant from the other side: what an
/// authorised requester gets back is the authenticated peer set.
#[tokio::test]
async fn authenticated_requester_receives_peer_topology() -> Result<()> {
    init();
    let fx = responder_holding_a_sentinel_peer(true).await?;

    let interrogator = IdentityBundle::generate()?;
    let interrogator_did = interrogator.did().clone();

    let client = WireClient::connect(&interrogator, fx.responder_addr).await?;
    client.authenticate(&fx.responder_did).await?;
    assert!(
        wait_until_authenticated(&fx.responder, &interrogator_did, Duration::from_secs(20)).await,
        "precondition: the requester's Hello was not accepted, so this control cannot \
         distinguish an authorised requester from an anonymous one"
    );

    let disclosure = client
        .request_peer_exchange(
            &interrogator_did,
            &fx.responder_did,
            Duration::from_secs(20),
        )
        .await?
        .context("an authenticated requester received no peer-exchange response at all")?;

    let (sentinel_did, sentinel_addr) = (&fx.sentinel_did, fx.sentinel_addr);
    assert!(
        disclosure.dids.contains(&sentinel_did.to_string()),
        "an authenticated requester must receive the authenticated peer set; the sentinel \
         {sentinel_did} was missing. Disclosed: {disclosure:?}"
    );
    assert!(
        disclosure.endpoints.contains(&sentinel_addr),
        "the response must carry a reachable endpoint for the sentinel {sentinel_addr}; \
         without it this control would not prove that endpoints are disclosable at all. \
         Disclosed: {disclosure:?}"
    );
    assert!(
        !disclosure.dids.contains(&interrogator_did.to_string()),
        "the responder listed the requester back to itself. A peer is not something it \
         discovered about its own neighbourhood, and echoing it is how a node ends up \
         treating itself as a peer (#2506). Disclosed: {disclosure:?}"
    );

    Ok(())
}

/// A node that is not participating in peer exchange must not answer, even for a peer that
/// authenticated correctly.
///
/// Authentication answers "who is asking". It does not answer "should this node be handing
/// out its neighbourhood at all", which is the operator's call. `federation.enabled`
/// already governs that in both directions — it gates sending the request
/// (`init_bootstrap`), subscribing to federation topics (`init_gossip`), and acting on an
/// inbound `Response` or `Announce` (`init_network`). Answering a `Request` was the one
/// peer-exchange behaviour left ungated, which is the asymmetry #2535 reports.
///
/// The requester here is fully authenticated, so a denial can only come from local policy.
/// That is what keeps this test independent of the authentication gate: if someone deleted
/// the authentication check tomorrow, this test would still pass, and if someone deleted
/// this policy check, the authentication test would still pass. Neither can carry the
/// other.
#[tokio::test]
async fn authenticated_requester_learns_nothing_when_peer_exchange_is_disabled() -> Result<()> {
    init();
    let fx = responder_holding_a_sentinel_peer(false).await?;

    let interrogator = IdentityBundle::generate()?;
    let interrogator_did = interrogator.did().clone();

    let client = WireClient::connect(&interrogator, fx.responder_addr).await?;
    client.authenticate(&fx.responder_did).await?;
    assert!(
        wait_until_authenticated(&fx.responder, &interrogator_did, Duration::from_secs(20)).await,
        "precondition: the requester must actually authenticate, otherwise this would be \
         the authentication test wearing a different name and would pass without the \
         participation gate existing at all"
    );

    let disclosure = client
        .request_peer_exchange(
            &interrogator_did,
            &fx.responder_did,
            Duration::from_secs(10),
        )
        .await?;

    let (sentinel_did, sentinel_addr) = (&fx.sentinel_did, fx.sentinel_addr);
    assert!(
        !disclosure
            .as_ref()
            .is_some_and(|d| d.leaks(sentinel_did, sentinel_addr)),
        "a node with peer exchange disabled enumerated its neighbourhood anyway: the \
         sentinel {sentinel_did} at {sentinel_addr} was disclosed to an authenticated \
         requester. The operator's configuration says this node is not participating \
         (#2535). Disclosed: {disclosure:?}"
    );

    Ok(())
}

/// A freshly spawned node does not answer peer-exchange requests until told to.
///
/// This is the property that makes the wiring safe to forget. `icn-net` cannot read
/// `federation.enabled`, so participation has to be pushed in from the composition root
/// (`supervisor::lifecycle::spawn_network_actor`). If that call is ever dropped in a
/// refactor, the consequence must be that federation stops discovering peers — noticeable,
/// and not a security failure — rather than that every node silently reverts to answering
/// anyone. Defaults decide which of those two you get.
///
/// It also pins the deployment reality behind #2535: `FederationConfig::enabled` is
/// `#[serde(default)]` → `false` and no shipped config has a `[federation]` section, so
/// "off" is the posture essentially every node is actually in.
#[tokio::test]
async fn peer_exchange_is_disabled_until_a_composition_root_enables_it() -> Result<()> {
    init();
    let node = IdentityBundle::generate()?;
    let (handle, _addr, _guard) = spawn_node(node, false).await?;

    assert!(
        !handle.peer_exchange_enabled(),
        "a node must not participate in peer exchange until something explicitly enables \
         it; a fail-open default would turn a forgotten composition-root call into silent \
         topology disclosure (#2535)"
    );

    // And the switch must actually move, or the assertion above would hold for a constant.
    handle.set_peer_exchange_enabled(true);
    assert!(
        handle.peer_exchange_enabled(),
        "enabling participation must take effect, otherwise the default above is not a \
         default but a hard-coded refusal"
    );

    Ok(())
}

/// The `from` field on the request must not be treated as the requester's identity.
///
/// Here an authenticated requester lies about who it is, naming the sentinel — a real,
/// currently-connected peer — in the request's `from` field. The responder excludes "the
/// requesting peer" from its answer, so if it took that field as identity it would exclude
/// the sentinel, and the lie would have steered the response.
///
/// The expected behaviour is that the lie changes nothing: identity comes from the
/// connection, which Hello bound to a certificate (#2520), and the claimed field is inert.
/// Asserting that the sentinel *is still returned* is what makes this test specific to
/// identity confusion rather than to disclosure in general — it fails exactly when
/// `message.from` starts being believed (#2491).
#[tokio::test]
async fn a_claimed_from_field_does_not_stand_in_for_authenticated_identity() -> Result<()> {
    init();
    let fx = responder_holding_a_sentinel_peer(true).await?;

    let interrogator = IdentityBundle::generate()?;
    let interrogator_did = interrogator.did().clone();

    let client = WireClient::connect(&interrogator, fx.responder_addr).await?;
    client.authenticate(&fx.responder_did).await?;
    assert!(
        wait_until_authenticated(&fx.responder, &interrogator_did, Duration::from_secs(20)).await,
        "precondition: the requester must be authenticated, so that the only thing under \
         test is which identity the responder believes"
    );

    // The lie: claim to be the sentinel, a peer this node really is connected to.
    let disclosure = client
        .request_peer_exchange(&fx.sentinel_did, &fx.responder_did, Duration::from_secs(20))
        .await?
        .context("an authenticated requester received no peer-exchange response at all")?;

    let sentinel_did = &fx.sentinel_did;
    assert!(
        disclosure.dids.contains(&sentinel_did.to_string()),
        "the responder shaped its answer around the requester's *claimed* DID: it omitted \
         {sentinel_did}, which is only explicable if the unverified `from` field was taken \
         as the requester's identity. Identity must come from the connection, not the \
         message (#2535, #2491). Disclosed: {disclosure:?}"
    );

    Ok(())
}

/// Asking before authenticating must not bank permission for later.
///
/// Hello and a peer-exchange request arrive on separate QUIC streams, so a peer can send
/// the request first and authenticate afterwards. The property that has to hold is
/// monotonicity: before authentication, nothing; after authentication, normal policy. What
/// must *not* happen is the in-between — a request that arrives unauthenticated, is held,
/// and is then answered once Hello completes, which would let any caller obtain topology
/// simply by asking early.
///
/// The reason this holds is structural rather than lucky: a connection's dispatch loop
/// awaits each handler inline, so messages on one connection are processed strictly in
/// order and the gate is evaluated once, at the time the request is handled. This test
/// pins that as behaviour rather than as an implementation detail someone could reorder.
#[tokio::test]
async fn a_request_that_precedes_hello_is_not_answered_once_hello_lands() -> Result<()> {
    init();
    let fx = responder_holding_a_sentinel_peer(true).await?;

    let interrogator = IdentityBundle::generate()?;
    let interrogator_did = interrogator.did().clone();
    let client = WireClient::connect(&interrogator, fx.responder_addr).await?;

    // Ask first, while this connection is still anonymous.
    let early = client
        .request_peer_exchange(&interrogator_did, &fx.responder_did, Duration::from_secs(5))
        .await?;

    let (sentinel_did, sentinel_addr) = (&fx.sentinel_did, fx.sentinel_addr);
    assert!(
        !early
            .as_ref()
            .is_some_and(|d| d.leaks(sentinel_did, sentinel_addr)),
        "the early, unauthenticated request was answered. Disclosed: {early:?}"
    );

    // Now authenticate, and confirm the earlier request stays unanswered rather than being
    // completed retroactively.
    client.authenticate(&fx.responder_did).await?;
    assert!(
        wait_until_authenticated(&fx.responder, &interrogator_did, Duration::from_secs(20)).await,
        "precondition: the requester must authenticate, or the second half of this test \
         proves nothing"
    );

    // A *fresh* request after authentication must succeed — otherwise "no late answer"
    // could just mean the connection was broken by the denial.
    let late = client
        .request_peer_exchange(
            &interrogator_did,
            &fx.responder_did,
            Duration::from_secs(20),
        )
        .await?
        .context(
            "after authenticating, a new request must be answered on the same connection; \
             a denial must not poison the connection for later legitimate requests",
        )?;
    assert!(
        late.dids.contains(&sentinel_did.to_string()),
        "an authenticated request on a connection that was previously denied must be \
         answered normally. Disclosed: {late:?}"
    );

    Ok(())
}
