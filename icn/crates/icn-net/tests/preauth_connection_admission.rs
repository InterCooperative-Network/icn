#![allow(clippy::unwrap_used, clippy::expect_used)]
//! How many unauthenticated connections one source may hold at once (#2547).
//!
//! #2491 gave every connection its own anonymous budget and deliberately stopped there. The
//! budget is scoped to a `ConnectionContext`, one per accepted connection, so:
//!
//! ```text
//! attacker opens N connections -> attacker holds N anonymous budgets
//! ```
//!
//! Three things on the inbound path made that accumulate rather than drain. The handshake
//! semaphore releases its permit the moment the handshake completes, so it caps concurrent
//! *handshakes* and never established connections. This node installs `keep_alive_interval`
//! on its **server** transport config, so it PINGs its own inbound connections and an idle
//! one never reaches the 60 s idle timeout. And an unauthenticated connection is in no map —
//! the session map is populated only by an authenticated Hello (#2530) — so nothing evicts
//! it.
//!
//! # The property under test
//!
//! > At any instant, one source may hold at most `MAX_PREAUTH_CONNECTIONS_PER_SOURCE`
//! > established-but-unauthenticated connections.
//!
//! Concurrency, not rate — these tests say nothing about churn, and neither does the fix.
//!
//! # How these tests know
//!
//! The observable is **how many separate connections can get a message to the handler while
//! refusing to authenticate**. A connection that was refused admission is closed before it
//! ever owns a handler loop, so it cannot deliver; one that was admitted can. Each client
//! stamps a distinct DID into `from`, so the delivery log counts *connections*, not messages,
//! and the count is structural rather than timing-derived.
//!
//! `from` is untrusted, which is the whole of #2491 — it is used here only as a test-local
//! tag for counting, never as an identity, and the node still charges every one of these
//! messages to the connection's anonymous budget.
//!
//! Every negative is paired with a positive control:
//! [`a_source_below_the_limit_is_not_throttled`] proves the bound does not break ordinary
//! peers, and [`authenticating_releases_the_sources_slot`] proves it is a bound on *anonymous*
//! connections rather than a connection cap.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_net::{
    IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage, VersionInfo,
    MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
};
use quinn::{ClientConfig, Endpoint};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

static ISSUED_PORTS: Mutex<Option<HashSet<u16>>> = Mutex::new(None);

fn pick_port() -> u16 {
    let mut issued = ISSUED_PORTS.lock().expect("port registry poisoned");
    let issued = issued.get_or_insert_with(HashSet::new);
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
    // Must be the provider the workspace actually builds against: installing a different one
    // makes every TLS handshake fail, which presents as a connect timeout and would look
    // exactly like the admission bound refusing connections.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter("icn_net=debug")
        .try_init();
}

/// Shuts the node down when the test ends.
struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// Every message that reached the external handler, by claimed `from`.
///
/// A connection refused admission is closed before it owns a handler loop, so it contributes
/// nothing here. Counting *distinct* tags therefore counts admitted connections.
#[derive(Clone, Default)]
struct Delivered(Arc<Mutex<Vec<String>>>);

impl Delivered {
    fn distinct_senders(&self) -> usize {
        self.0
            .lock()
            .expect("delivery log poisoned")
            .iter()
            .collect::<HashSet<_>>()
            .len()
    }
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` starts no inbound accept loop
/// without one, and every "was not admitted" assertion below would then pass for the wrong
/// reason.
async fn spawn_node(
    bundle: IdentityBundle,
) -> Result<(NetworkHandle, SocketAddr, Guard, Delivered)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let delivered = Delivered::default();
    let sink = delivered.0.clone();
    let handler: IncomingMessageHandler = Arc::new(move |msg: NetworkMessage| {
        sink.lock()
            .expect("delivery log poisoned")
            .push(msg.from.to_string());
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
            None, // personhood
            None, // anchor_rate_config
            None, // advertised_addr
        )
        .await
        {
            Ok(handle) => {
                // The handle is returned, not dropped: it owns the actor's mailbox sender,
                // and letting it fall out of scope here would tear the node down before any
                // test could connect to it.
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, addr, Guard(shutdown_tx), delivered));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

/// A raw QUIC client that authenticates only when asked to.
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

        // Establishing the connection is a precondition, not the property under test.
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

    /// Send one message tagged with this client's own DID.
    ///
    /// `Subscribe` specifically, because it reaches the external handler untouched — no
    /// verification and no state change — so an arrival records exactly one thing: this
    /// connection was admitted and got a handler loop. Payloads the node answers itself
    /// (`Ping`, `Hello`) never reach the handler, which would make every count below zero and
    /// every upper-bound assertion pass for the wrong reason.
    ///
    /// Errors are swallowed by the caller on purpose: a connection the node refused is
    /// closed, so writing to it *should* fail, and that failure is the signal.
    async fn tag(&self, to: &Did) -> Result<()> {
        let message = NetworkMessage::subscribe(
            self.identity.did().clone(),
            to.clone(),
            vec!["icn.test.2547".to_string()],
        );
        self.send(&message).await
    }

    async fn authenticate(&self, to: &Did) -> Result<()> {
        let message = NetworkMessage::new(
            self.identity.did().clone(),
            Some(to.clone()),
            icn_net::MessagePayload::Hello {
                binding_info: self.identity.binding_info(),
                version_info: Some(VersionInfo::new("2547-test".to_string())),
                topology_info: None,
                x25519_public: *self.identity.x25519_public_bytes(),
                ml_dsa_public: None,
                ml_kem_public: None,
                pq_binding_proof: None,
            },
        );
        self.send(&message).await
    }

    async fn send(&self, message: &NetworkMessage) -> Result<()> {
        let (mut send, _recv) = self
            .connection
            .open_bi()
            .await
            .context("open_bi on the test connection")?;
        icn_net::write_message(&mut send, message).await?;
        send.finish()?;
        Ok(())
    }
}

/// Let the node finish admitting and dispatching before anything is counted.
///
/// Waits for the delivery total to stop moving rather than sleeping a fixed amount, so it can
/// only ever admit *more* messages than a fixed wait would — never fewer. That direction
/// matters: every assertion below is an upper bound, so a premature read could only make the
/// test pass when it should fail, and this removes that.
async fn settle(delivered: &Delivered) {
    let mut last = usize::MAX;
    for _ in 0..40 {
        let now = delivered.0.lock().expect("delivery log poisoned").len();
        if now == last {
            return;
        }
        last = now;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// How far past the limit to push. Small: each one is a real QUIC handshake.
const OVERSHOOT: usize = 4;

// ---------------------------------------------------------------------------
// RED — one source must not hold unbounded anonymous connections
// ---------------------------------------------------------------------------

/// RED A — a source that never authenticates cannot hold more than its share.
///
/// Every client here connects from loopback, so they are all one source, and none of them
/// sends a Hello. Before this change all of them were admitted, each acquiring a handler
/// task, a `ConnectionContext` and its own anonymous budget, and the node kept every one of
/// them alive with its own keepalives.
#[tokio::test]
async fn one_source_cannot_hold_unbounded_unauthenticated_connections() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    let attempts = MAX_PREAUTH_CONNECTIONS_PER_SOURCE + OVERSHOOT;
    let mut clients = Vec::new();
    for _ in 0..attempts {
        let identity = IdentityBundle::generate()?;
        // A refused connection may fail to connect or fail to write; both mean "not
        // admitted", and neither is an error for this test.
        if let Ok(client) = WireClient::connect(&identity, addr).await {
            let _ = client.tag(node.did()).await;
            clients.push(client);
        }
    }
    settle(&delivered).await;

    let admitted = delivered.distinct_senders();
    assert!(
        admitted <= MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
        "{admitted} of {attempts} unauthenticated connections from one source were admitted, \
         but the per-source pre-authentication limit is {MAX_PREAUTH_CONNECTIONS_PER_SOURCE}"
    );

    // Hold the clients until after the assertion: the property is about *simultaneously*
    // live connections, and dropping them early would release slots and measure nothing.
    drop(clients);
    Ok(())
}

// ---------------------------------------------------------------------------
// Positive controls
// ---------------------------------------------------------------------------

/// A source below the limit must be completely unaffected.
///
/// Without this, "nothing was admitted" and "the bound works" look identical, and a node that
/// refused every inbound connection would pass the test above.
#[tokio::test]
async fn a_source_below_the_limit_is_not_throttled() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    let below = MAX_PREAUTH_CONNECTIONS_PER_SOURCE - 1;
    let mut clients = Vec::new();
    for _ in 0..below {
        let identity = IdentityBundle::generate()?;
        let client = WireClient::connect(&identity, addr).await?;
        client.tag(node.did()).await?;
        clients.push(client);
    }
    settle(&delivered).await;

    assert_eq!(
        delivered.distinct_senders(),
        below,
        "a source under the limit was refused: the bound is throttling ordinary peers"
    );

    drop(clients);
    Ok(())
}

/// The bound is on *anonymous* connections, not on connections.
///
/// A peer that authenticates releases its slot immediately, so one source can hold far more
/// than the limit once those connections have said who they are. This is what keeps the fix
/// off legitimate traffic — and it is the assertion that fails if the slot were released at
/// close instead of at authentication.
#[tokio::test]
async fn authenticating_releases_the_sources_slot() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    // Comfortably more than the anonymous limit, each authenticating before the next opens.
    let total = MAX_PREAUTH_CONNECTIONS_PER_SOURCE + OVERSHOOT;
    let mut clients = Vec::new();
    for _ in 0..total {
        let identity = IdentityBundle::generate()?;
        let client = WireClient::connect(&identity, addr)
            .await
            .context("an authenticated peer was refused admission")?;
        client.authenticate(node.did()).await?;
        // Let the Hello land, so this connection has released its slot before the next one
        // asks for one.
        settle(&delivered).await;
        client.tag(node.did()).await?;
        clients.push(client);
    }
    settle(&delivered).await;

    assert_eq!(
        delivered.distinct_senders(),
        total,
        "only some of {total} authenticated connections from one source got through — the \
         pre-authentication bound is being applied to peers that already identified themselves"
    );

    drop(clients);
    Ok(())
}
