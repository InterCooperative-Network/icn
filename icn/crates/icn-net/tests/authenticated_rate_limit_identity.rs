#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Which identity may select an inbound message's rate limit.
//!
//! #2491. The rate-limit check runs before any dispatch — for an anonymous connection, before
//! the decode as well (#2558) — and it used to be keyed on `NetworkMessage.from`, a field the
//! sender chooses and nobody has verified at that point. DIDs are public, so naming a well-trusted one bought that
//! peer's tier without holding its key.
//!
//! The fix is an ordering, not a new identity mechanism. A connection has two phases, and
//! the phase decides what may key the limiter:
//!
//! ```text
//! before an authenticated Hello:  the connection's own budget. No DID input at all.
//! after  an authenticated Hello:  the DID bound to THIS connection's certificate (#2520).
//! ```
//!
//! `NetworkMessage.from` selects nothing in either phase.
//!
//! # How these tests know
//!
//! Two observables, both structural — neither depends on wall-clock refill timing.
//!
//! 1. **Who the oracle was asked about.** [`TierOracle`] records every actor DID passed to
//!    `PolicyOracle::evaluate`. That is precisely "whose trust class was consulted", which
//!    is the question #2491 asks. An assertion that a DID never appears cannot pass
//!    vacuously the way a timing assertion can.
//!
//! 2. **How many messages reached the external handler.** A rate-limited message never
//!    reaches it, so counting arrivals measures the budget that was actually applied. The
//!    tiers below are deliberately far apart — a spoofable tier admits *everything*, a
//!    starved tier admits *two* — so no arrival count is reachable from two different
//!    tiers by accident.
//!
//! Every negative assertion is paired with a positive control in the same file:
//! [`authentication_moves_the_connection_onto_its_configured_tier`] proves traffic still
//! flows, so "nothing arrived" cannot be mistaken for a dead harness, and
//! [`the_pre_authentication_budget_admits_a_handshake`] proves the anonymous budget is not
//! so tight that bootstrap cannot happen.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle, PersonhoodAnchor, PersonhoodStoreTrait};
use icn_kernel_api::authz::{
    ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest, RateLimit,
};
use icn_net::{
    envelope::{PayloadType, SignedEnvelope},
    IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage, VersionInfo,
};
use quinn::{ClientConfig, Endpoint};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

/// Ports already handed out in this process.
///
/// `pick_unused_port` binds, closes, and returns the port, so two tests running
/// concurrently can be handed the same one. Remembering what we issued removes that race
/// within the process; callers still retry, because nothing rules out another process
/// taking it in between.
static ISSUED_PORTS: Mutex<Option<HashSet<u16>>> = Mutex::new(None);

fn pick_port() -> u16 {
    let mut guard = ISSUED_PORTS.lock().expect("port registry poisoned");
    let issued = guard.get_or_insert_with(HashSet::new);
    for _ in 0..64 {
        if let Some(port) = portpicker::pick_unused_port() {
            if issued.insert(port) {
                return port;
            }
        }
    }
    panic!("could not find an unused port");
}

fn init() {
    // Without a process-level provider every TLS handshake panics, which would fail every
    // assertion in this file for a reason that has nothing to do with rate limiting.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter("icn_net=debug")
        .try_init();
}

/// Keeps a node's shutdown sender alive for the duration of a test.
struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

// ---------------------------------------------------------------------------
// Observables
// ---------------------------------------------------------------------------

/// A `PolicyOracle` that records *who it was asked about* and answers with a fixed tier.
///
/// The recording is the point. #2491 is not a question about numbers, it is a question
/// about which identity reached the trust lookup, and this answers it directly: a DID that
/// never appears in [`TierOracle::consulted`] never selected a rate limit.
///
/// It serves [`icn_net::NETWORK_DOMAIN`], the domain the limiter queries, so registering it
/// exercises the same path #2490 wired at the composition root.
#[derive(Debug)]
struct TierOracle {
    consulted: Arc<Mutex<Vec<String>>>,
    tiers: HashMap<String, RateLimit>,
    default_tier: RateLimit,
}

impl TierOracle {
    fn new(default_tier: RateLimit) -> Self {
        Self {
            consulted: Arc::new(Mutex::new(Vec::new())),
            tiers: HashMap::new(),
            default_tier,
        }
    }

    fn with_tier(mut self, did: &Did, tier: RateLimit) -> Self {
        self.tiers.insert(did.to_string(), tier);
        self
    }

    fn consulted_handle(&self) -> Arc<Mutex<Vec<String>>> {
        self.consulted.clone()
    }
}

impl PolicyOracle for TierOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let actor = request.core.actor.to_string();
        self.consulted
            .lock()
            .expect("consulted log poisoned")
            .push(actor.clone());
        let tier = self
            .tiers
            .get(&actor)
            .cloned()
            .unwrap_or_else(|| self.default_tier.clone());
        PolicyDecision::allow_with(ConstraintSet::new().with_rate_limit(tier))
    }

    fn domain(&self) -> Domain {
        Domain::new(icn_net::NETWORK_DOMAIN)
    }
}

/// A personhood store that records every DID whose anchor was looked up.
///
/// It holds no anchors: the question here is not what the answer was, but whether the
/// question was asked about a DID nobody had authenticated (#2491 phase 10). Returning
/// `None` is also the graceful-degradation path, so its presence cannot change any
/// admission decision and cannot mask the tier assertions.
#[derive(Debug, Default)]
struct RecordingPersonhoodStore {
    looked_up: Arc<Mutex<Vec<String>>>,
}

impl RecordingPersonhoodStore {
    fn looked_up_handle(&self) -> Arc<Mutex<Vec<String>>> {
        self.looked_up.clone()
    }
}

impl PersonhoodStoreTrait for RecordingPersonhoodStore {
    fn get_anchor(&self, _anchor_id: &[u8; 32]) -> anyhow::Result<Option<PersonhoodAnchor>> {
        Ok(None)
    }

    fn get_anchor_by_did(&self, did: &Did) -> anyhow::Result<Option<PersonhoodAnchor>> {
        self.looked_up
            .lock()
            .expect("lookup log poisoned")
            .push(did.to_string());
        Ok(None)
    }

    fn get_anchor_id_for_did(&self, did: &Did) -> anyhow::Result<Option<[u8; 32]>> {
        self.looked_up
            .lock()
            .expect("lookup log poisoned")
            .push(did.to_string());
        Ok(None)
    }
}

/// Every message that survived the rate limiter and reached the external handler.
///
/// A denied message is dropped before dispatch, so this count *is* the budget that was
/// applied. Recording `from` as well makes it possible to assert that the messages which
/// arrived are the ones the test sent, not incidental protocol traffic.
#[derive(Clone, Default)]
struct Delivered(Arc<Mutex<Vec<String>>>);

impl Delivered {
    fn count_from(&self, claimed: &Did) -> usize {
        let claimed = claimed.to_string();
        self.0
            .lock()
            .expect("delivery log poisoned")
            .iter()
            .filter(|from| **from == claimed)
            .count()
    }

    fn total(&self) -> usize {
        self.0.lock().expect("delivery log poisoned").len()
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` only starts the inbound
/// accept loop when `incoming_handler` is `Some`. Without it nothing here would receive a
/// message at all, and every "denied" assertion would pass for the wrong reason.
async fn spawn_node(
    bundle: IdentityBundle,
    oracle: Option<Arc<dyn PolicyOracle>>,
    personhood: Option<Arc<dyn PersonhoodStoreTrait>>,
) -> Result<(NetworkHandle, SocketAddr, Guard, Delivered)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let delivered = Delivered::default();
    let sink = delivered.0.clone();
    let handler: IncomingMessageHandler = Arc::new(move |msg: NetworkMessage| {
        sink.lock()
            .expect("delivery log poisoned")
            .push(msg.from.to_string());
    });

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
            oracle.clone(),
            None, // fallback_config
            None, // topology_config
            None, // stun_servers
            None, // turn_config
            None, // misbehavior_detector
            None, // store
            personhood.clone(),
            None, // anchor_rate_config
            None, // advertised_addr
        )
        .await
        {
            Ok(handle) => {
                // Let the endpoint bind before anyone dials it.
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, addr, Guard(shutdown_tx), delivered));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

/// A raw QUIC client that speaks the wire protocol, authenticating only when asked to.
///
/// The gap between "has a connection" and "has authenticated" is the whole subject of this
/// file, so the two are separate methods and no test does the second by accident.
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
            VersionInfo::new("icnd-test-client".to_string()),
            None,
            *self.identity.x25519_public_bytes(),
            None,
            None,
        );
        self.send(&hello).await
    }

    /// Authenticate a *different* identity on this already-established connection.
    ///
    /// This is a real rebinding, not a trick. #2520 asks two questions of a Hello: does the
    /// binding's certificate hash match the certificate on this connection, and did the
    /// named DID's key sign that hash. Neither requires the certificate to belong to the
    /// DID — signing this connection's certificate hash is how a key holder says "I am the
    /// party on this session". So `other` genuinely authenticates here, and the connection's
    /// identity genuinely changes.
    async fn authenticate_as(&self, other: &IdentityBundle, to: &Did) -> Result<()> {
        use sha2::{Digest, Sha256};

        let cert_hash: [u8; 32] = {
            let mut hasher = Sha256::new();
            hasher.update(self.identity.tls_cert().as_ref());
            hasher.finalize().into()
        };
        let binding = icn_identity::BindingInfo {
            did: other.did().clone(),
            tls_cert_hash: cert_hash,
            tls_binding_sig: other.sign(&cert_hash)?.to_bytes().to_vec(),
            created_at: 0,
        };

        let hello = NetworkMessage::hello(
            other.did().clone(),
            to.clone(),
            binding,
            VersionInfo::new("icnd-test-client".to_string()),
            None,
            *other.x25519_public_bytes(),
            None,
            None,
        );
        self.send(&hello).await
    }

    /// Send one message claiming to be `claimed_from`.
    ///
    /// `Subscribe` is used as the carrier because the inbound dispatch forwards it to the
    /// external handler untouched — no verification, no state change — so an arrival
    /// records exactly one thing: the rate limiter admitted it.
    async fn send_claiming(&self, claimed_from: &Did, to: &Did) -> Result<()> {
        let message = NetworkMessage::subscribe(
            claimed_from.clone(),
            to.clone(),
            vec!["icn.test.2491".to_string()],
        );
        self.send(&message).await
    }

    async fn send(&self, message: &NetworkMessage) -> Result<()> {
        let (mut send, _recv) = self.connection.open_bi().await?;
        icn_net::protocol::write_message(&mut send, message).await?;
        send.finish().context("finish stream")?;
        Ok(())
    }
}

/// Poll until `observer` has authenticated `did`, or the deadline passes.
///
/// `peer_connections` is written only by `handle_hello`, after the #2520 DID-TLS binding
/// checks pass, so this returning `true` is proof the handshake completed. Synchronising on
/// that observable rather than sleeping is what keeps the post-authentication assertions
/// honest: "charged to the pre-auth bucket" and "the Hello has not landed yet" would
/// otherwise be indistinguishable.
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

/// Give the node time to drain what was written before reading the observables.
///
/// Writes are streams the node accepts asynchronously, so a test that asserts immediately
/// after its last `send` can observe a partial count. This waits for quiescence — the
/// delivered total holding still — rather than a fixed sleep, and it can only ever let
/// *more* messages through, so it cannot manufacture a passing denial assertion.
async fn settle(delivered: &Delivered) {
    let mut last = usize::MAX;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let now = delivered.total();
        if now == last {
            return;
        }
        last = now;
    }
}

/// A tier so large that no test sends enough to exhaust it.
///
/// Standing in for "a well-trusted peer": if a spoofed claim selects this, every message
/// the test sends arrives.
fn spoofable_tier() -> RateLimit {
    RateLimit {
        messages_per_second: 5_000,
        burst_size: 500,
    }
}

/// A tier that admits two messages and never refills.
///
/// `messages_per_second: 0` genuinely means no replenishment (#2503), so an arrival count
/// above two proves this tier was *not* the one applied — with no timing tolerance needed.
fn starved_tier() -> RateLimit {
    RateLimit {
        messages_per_second: 0,
        burst_size: 2,
    }
}

/// How many messages each test fires.
///
/// Comfortably above the pre-authentication burst and comfortably below the spoofable
/// tier's burst, so the three possible outcomes — spoofable tier, pre-auth budget, starved
/// tier — land in three non-overlapping ranges.
const VOLLEY: usize = 60;

/// The largest arrival count still explainable by the pre-authentication budget.
///
/// The budget is a burst plus whatever refills while the volley is in flight. Sending
/// [`VOLLEY`] small messages over loopback takes well under a second, so this leaves
/// generous headroom above burst-plus-refill while staying far below [`VOLLEY`] itself.
/// The primary assertion in every test is the oracle log, which has no timing component at
/// all; this bound is the corroborating one.
const PRE_AUTH_CEILING: usize = 45;

// ---------------------------------------------------------------------------
// Pre-authentication: no DID claim may select a tier
// ---------------------------------------------------------------------------

/// RED A — naming a well-trusted DID must not buy its tier.
///
/// The attacker holds its own key and its own certificate. It never proves anything about
/// `trusted`; it merely writes that DID into `from`.
#[tokio::test]
async fn an_unauthenticated_claim_cannot_buy_a_trusted_peers_tier() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let trusted = IdentityBundle::generate()?;
    let attacker = IdentityBundle::generate()?;

    let oracle = TierOracle::new(starved_tier()).with_tier(trusted.did(), spoofable_tier());
    let consulted = oracle.consulted_handle();

    let (_handle, addr, _guard, delivered) =
        spawn_node(node.clone(), Some(Arc::new(oracle)), None).await?;

    let client = WireClient::connect(&attacker, addr).await?;
    for _ in 0..VOLLEY {
        client.send_claiming(trusted.did(), node.did()).await?;
    }
    settle(&delivered).await;

    let consulted = consulted.lock().expect("consulted log poisoned").clone();
    assert!(
        !consulted.contains(&trusted.did().to_string()),
        "the trusted DID's tier was looked up for a connection that never authenticated it; \
         consulted = {consulted:?}"
    );

    let arrived = delivered.count_from(trusted.did());
    assert!(
        arrived <= PRE_AUTH_CEILING,
        "{arrived} of {VOLLEY} spoofed messages were admitted, which is more than the \
         pre-authentication budget can explain — the claimed DID's tier was applied"
    );

    Ok(())
}

/// RED B — rotating the claimed DID must not multiply the budget.
///
/// Keying the limiter on `from` gives every new name its own bucket, so an attacker with a
/// list of DIDs gets one starved tier's worth of budget *per name*. Thirty names times two
/// tokens is the entire volley. A connection-scoped budget is indifferent to the name.
#[tokio::test]
async fn rotating_the_claimed_did_does_not_multiply_the_budget() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let attacker = IdentityBundle::generate()?;

    // Thirty distinct names, two messages each. Under `from`-keying every one of them
    // opens a fresh starved bucket holding exactly the two tokens it is about to spend.
    let names: Vec<Did> = (0..30)
        .map(|_| IdentityBundle::generate().map(|b| b.did().clone()))
        .collect::<Result<_, _>>()?;

    let oracle = TierOracle::new(starved_tier());
    let consulted = oracle.consulted_handle();

    let (_handle, addr, _guard, delivered) =
        spawn_node(node.clone(), Some(Arc::new(oracle)), None).await?;

    let client = WireClient::connect(&attacker, addr).await?;
    for _ in 0..2 {
        for name in &names {
            client.send_claiming(name, node.did()).await?;
        }
    }
    settle(&delivered).await;

    let consulted = consulted.lock().expect("consulted log poisoned").clone();
    for name in &names {
        assert!(
            !consulted.contains(&name.to_string()),
            "a claimed DID reached the trust lookup on an unauthenticated connection: {name}"
        );
    }

    let arrived = delivered.total();
    assert!(
        arrived <= PRE_AUTH_CEILING,
        "{arrived} of {} messages were admitted across {} rotating claims — changing the \
         claimed DID bought additional budget",
        names.len() * 2,
        names.len()
    );

    Ok(())
}

/// RED F — a connection that never authenticates never leaves the anonymous phase.
///
/// Traffic alone is not evidence of identity, however much of it there is and however long
/// it goes on. This sends in two separated waves so that "the first wave exhausted the
/// bucket" cannot be confused with "the connection was promoted".
#[tokio::test]
async fn an_unauthenticated_connection_never_earns_a_trust_tier() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let trusted = IdentityBundle::generate()?;
    let attacker = IdentityBundle::generate()?;

    let oracle = TierOracle::new(starved_tier()).with_tier(trusted.did(), spoofable_tier());
    let consulted = oracle.consulted_handle();

    let (_handle, addr, _guard, delivered) =
        spawn_node(node.clone(), Some(Arc::new(oracle)), None).await?;

    let client = WireClient::connect(&attacker, addr).await?;
    for _ in 0..VOLLEY {
        client.send_claiming(trusted.did(), node.did()).await?;
    }
    settle(&delivered).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    for _ in 0..VOLLEY {
        client.send_claiming(trusted.did(), node.did()).await?;
    }
    settle(&delivered).await;

    let consulted = consulted.lock().expect("consulted log poisoned").clone();
    assert!(
        consulted.is_empty(),
        "an unauthenticated connection reached the trust lookup: {consulted:?}"
    );

    Ok(())
}

/// Phase 7 — a valid signature on a payload does not authenticate the *connection*.
///
/// A `SignedEnvelope` proves who signed that envelope. It says nothing about who holds the
/// QUIC session it arrived on: the envelope is a self-contained artefact, and anyone who
/// has seen one can put it on their own connection. Only the #2520 Hello binds a DID to
/// *this* connection's certificate, so only the Hello may move the connection into the
/// trust-derived phase.
#[tokio::test]
async fn a_signed_envelope_does_not_authenticate_the_connection() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let signer = IdentityBundle::generate()?;
    let attacker = IdentityBundle::generate()?;

    let oracle = TierOracle::new(starved_tier()).with_tier(signer.did(), spoofable_tier());
    let consulted = oracle.consulted_handle();

    let (_handle, addr, _guard, delivered) =
        spawn_node(node.clone(), Some(Arc::new(oracle)), None).await?;

    // A genuinely valid envelope, signed by `signer`'s real key — but relayed by the
    // attacker on the attacker's own connection, which is exactly the confusion under test.
    let keypair = signer.keypair()?;
    let envelope = SignedEnvelope::new(
        signer.did(),
        &keypair,
        1,
        PayloadType::Trust,
        b"2491-signed-before-hello".to_vec(),
    )?;

    let client = WireClient::connect(&attacker, addr).await?;
    client
        .send(&NetworkMessage::signed(Some(node.did().clone()), envelope))
        .await?;
    for _ in 0..VOLLEY {
        client.send_claiming(signer.did(), node.did()).await?;
    }
    settle(&delivered).await;

    let consulted = consulted.lock().expect("consulted log poisoned").clone();
    assert!(
        !consulted.contains(&signer.did().to_string()),
        "a signed payload promoted the connection to the signer's tier; consulted = \
         {consulted:?}"
    );

    let arrived = delivered.count_from(signer.did());
    assert!(
        arrived <= PRE_AUTH_CEILING,
        "{arrived} of {VOLLEY} messages were admitted after a signed envelope — the \
         connection left the anonymous phase without a Hello"
    );

    Ok(())
}

/// RED G — an unauthenticated claim must not reach the personhood store either.
///
/// Fixing the trust tier while leaving the anchor lookup keyed on `from` would still let a
/// sender spend somebody else's per-person budget. Before authentication there is no DID,
/// so there is no anchor to look up — not a default one, and certainly not one derived from
/// the claim.
#[tokio::test]
async fn an_unauthenticated_claim_never_reaches_the_personhood_store() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let victim = IdentityBundle::generate()?;
    let attacker = IdentityBundle::generate()?;

    let personhood = RecordingPersonhoodStore::default();
    let looked_up = personhood.looked_up_handle();

    let (_handle, addr, _guard, delivered) = spawn_node(
        node.clone(),
        Some(Arc::new(TierOracle::new(starved_tier()))),
        Some(Arc::new(personhood)),
    )
    .await?;

    let client = WireClient::connect(&attacker, addr).await?;
    for _ in 0..VOLLEY {
        client.send_claiming(victim.did(), node.did()).await?;
    }
    settle(&delivered).await;

    let looked_up = looked_up.lock().expect("lookup log poisoned").clone();
    assert!(
        !looked_up.contains(&victim.did().to_string()),
        "a personhood anchor was looked up for an unauthenticated claim: {looked_up:?}"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Post-authentication: the connection's proven identity, and only it
// ---------------------------------------------------------------------------

/// RED C and RED E — authentication really does switch the phase, onto the real tier.
///
/// This is the positive control the negative tests lean on. A fix that simply refused
/// trust-derived tiering forever would satisfy every assertion above and fail here.
#[tokio::test]
async fn authentication_moves_the_connection_onto_its_configured_tier() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let peer = IdentityBundle::generate()?;

    let oracle = TierOracle::new(starved_tier()).with_tier(peer.did(), spoofable_tier());
    let consulted = oracle.consulted_handle();

    let (handle, addr, _guard, delivered) =
        spawn_node(node.clone(), Some(Arc::new(oracle)), None).await?;

    let client = WireClient::connect(&peer, addr).await?;
    client.authenticate(node.did()).await?;
    assert!(
        wait_until_authenticated(&handle, peer.did(), Duration::from_secs(10)).await,
        "the Hello never authenticated; nothing below would be measuring the right phase"
    );

    for _ in 0..VOLLEY {
        client.send_claiming(peer.did(), node.did()).await?;
    }
    settle(&delivered).await;

    let consulted = consulted.lock().expect("consulted log poisoned").clone();
    assert!(
        consulted.contains(&peer.did().to_string()),
        "the authenticated peer's tier was never consulted, so the connection never left \
         the anonymous phase; consulted = {consulted:?}"
    );

    let arrived = delivered.count_from(peer.did());
    assert!(
        arrived > PRE_AUTH_CEILING,
        "only {arrived} of {VOLLEY} messages from an authenticated, well-trusted peer were \
         admitted — its configured tier is not being applied"
    );

    Ok(())
}

/// RED D — after authentication, a forged `from` cannot upgrade the connection.
///
/// The connection belongs to `peer`, whose tier admits two messages and never refills. The
/// messages name a well-trusted DID. They are `peer`'s traffic on `peer`'s connection, and
/// they must be charged as such.
#[tokio::test]
async fn an_authenticated_connection_is_charged_to_its_own_tier() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let peer = IdentityBundle::generate()?;
    let trusted = IdentityBundle::generate()?;

    let oracle = TierOracle::new(starved_tier()).with_tier(trusted.did(), spoofable_tier());
    let consulted = oracle.consulted_handle();

    let (handle, addr, _guard, delivered) =
        spawn_node(node.clone(), Some(Arc::new(oracle)), None).await?;

    let client = WireClient::connect(&peer, addr).await?;
    client.authenticate(node.did()).await?;
    assert!(
        wait_until_authenticated(&handle, peer.did(), Duration::from_secs(10)).await,
        "the Hello never authenticated; the assertion below would pass for the wrong reason"
    );

    for _ in 0..VOLLEY {
        client.send_claiming(trusted.did(), node.did()).await?;
    }
    settle(&delivered).await;

    let consulted = consulted.lock().expect("consulted log poisoned").clone();
    assert!(
        !consulted.contains(&trusted.did().to_string()),
        "a claimed DID reached the trust lookup on a connection authenticated as somebody \
         else; consulted = {consulted:?}"
    );
    assert!(
        consulted.contains(&peer.did().to_string()),
        "the authenticated peer's own tier was never consulted; consulted = {consulted:?}"
    );

    let arrived = delivered.count_from(trusted.did());
    assert!(
        arrived <= 2,
        "{arrived} messages were admitted, but the authenticated peer's tier allows two — \
         the claimed DID's tier was applied instead"
    );

    Ok(())
}

/// Phase 6 — the anonymous budget is not so tight that a peer cannot authenticate.
///
/// The positive control for bootstrap. A pre-authentication limiter sized below the cost of
/// a handshake would fail closed in the most damaging way possible: silently, and only on
/// the path that lets a node join at all.
#[tokio::test]
async fn the_pre_authentication_budget_admits_a_handshake() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let peer = IdentityBundle::generate()?;

    // The starved tier applies to everyone here, including `peer` once it authenticates.
    // If the Hello itself were charged to a trust tier this would be the tightest possible
    // configuration; charged to the anonymous budget it is irrelevant, which is the point.
    let (handle, addr, _guard, _delivered) = spawn_node(
        node.clone(),
        Some(Arc::new(TierOracle::new(starved_tier()))),
        None,
    )
    .await?;

    let client = WireClient::connect(&peer, addr).await?;
    client.authenticate(node.did()).await?;

    assert!(
        wait_until_authenticated(&handle, peer.did(), Duration::from_secs(10)).await,
        "a first-contact Hello was not admitted by the pre-authentication budget — \
         bootstrap is broken"
    );

    Ok(())
}

/// Phase 9 — the rate-limit identity follows a rebinding, and never anticipates one.
///
/// A repeated Hello is verified like any other (#2537), and a valid one can move the
/// connection's authenticated identity. Two things must hold across that move:
///
/// - the rebinding Hello is charged to the identity in force *before* it, because the new
///   DID is not authenticated until the Hello succeeds;
/// - traffic after it is charged to the new identity, which requires reading the
///   connection's current identity per message rather than caching the first one.
///
/// The two identities are given opposite tiers, so "which one is charged" is answered by an
/// arrival count that cannot be produced by the other. `first` is starved; `second` is not.
/// A node that cached the identity it saw first would keep charging `first` and admit two
/// messages, which is the mutation this pins.
#[tokio::test]
async fn the_rate_limit_identity_is_the_connections_current_one() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let first = IdentityBundle::generate()?;
    let second = IdentityBundle::generate()?;

    let oracle = TierOracle::new(starved_tier()).with_tier(second.did(), spoofable_tier());
    let consulted = oracle.consulted_handle();

    let (handle, addr, _guard, delivered) =
        spawn_node(node.clone(), Some(Arc::new(oracle)), None).await?;

    // Phase one: the connection belongs to `first`, whose tier is starved.
    let client = WireClient::connect(&first, addr).await?;
    client.authenticate(node.did()).await?;
    assert!(
        wait_until_authenticated(&handle, first.did(), Duration::from_secs(10)).await,
        "the first Hello never authenticated"
    );

    // Phase two: `second` authenticates on the same connection, over the same certificate.
    client.authenticate_as(&second, node.did()).await?;
    assert!(
        wait_until_authenticated(&handle, second.did(), Duration::from_secs(10)).await,
        "the rebinding Hello never authenticated, so the assertion below would be measuring \
         the wrong phase"
    );

    for _ in 0..VOLLEY {
        client.send_claiming(second.did(), node.did()).await?;
    }
    settle(&delivered).await;

    let consulted = consulted.lock().expect("consulted log poisoned").clone();
    let first_hits = consulted
        .iter()
        .filter(|d| **d == first.did().to_string())
        .count();
    let second_hits = consulted
        .iter()
        .filter(|d| **d == second.did().to_string())
        .count();
    assert!(
        consulted.contains(&second.did().to_string()),
        "the rebound identity's tier was never consulted; first={} hits={first_hits} \
         second={} hits={second_hits} total={} delivered_second={} delivered_total={}",
        first.did(),
        second.did(),
        consulted.len(),
        delivered.count_from(second.did()),
        delivered.total()
    );

    let arrived = delivered.count_from(second.did());
    assert!(
        arrived > PRE_AUTH_CEILING,
        "only {arrived} of {VOLLEY} messages were admitted after the rebinding — the \
         connection is still being charged to the identity it authenticated first"
    );

    Ok(())
}
