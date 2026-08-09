#![allow(clippy::unwrap_used, clippy::expect_used)]
//! What one source may spend on anonymous work, however many connections it opens (#2549).
//!
//! #2547 bounds how many unauthenticated connections one source holds *at once*, and #2552
//! bounds how long each may be held. Neither bounded what a source spends *over time*, because
//! the anonymous budget was minted per connection:
//!
//! ```text
//! connect  -> a brand-new PreAuthRateLimiter, full burst
//! spend it -> messages dispatched, each possibly a Hello
//! close    -> the budget is discarded with the connection
//! connect  -> a brand-new PreAuthRateLimiter, full burst again
//! ```
//!
//! Concurrency never rose and no deadline was ever reached, so both existing bounds held the
//! whole time while the aggregate went wherever the source wanted it to.
//!
//! # The property under test
//!
//! > For one normalized source, the anonymous **messages this node dispatches** — each of which
//! > may carry a Hello, and so a DID parse, a `BindingInfo` signature verification against this
//! > connection's certificate, and optionally an ML-DSA binding verification — are at most
//! > `PREAUTH_SOURCE_BURST + rate * T` over any window of length `T`, aggregated across every
//! > connection that source opens, and unaffected by how many it opens, closes, or successfully
//! > authenticates.
//!
//! Note the shape of that statement. It is deliberately **not** "at most `PREAUTH_SOURCE_BURST`
//! per `PREAUTH_SOURCE_RENEWAL_WINDOW`": a token bucket hands out a full burst at the start of a
//! window and another once it has refilled, so the sliding-window reading is wrong by up to a
//! factor of two. Every bound here is computed from *measured* elapsed time by [`allowance`].
//!
//! # What these tests do not claim
//!
//! The QUIC/TLS handshake is finished before the source key exists, so nothing here bounds its
//! rate — see the PR body. Neither is the `ConnectionContext` and task a connection allocates,
//! nor reading and deserializing a message that is then denied, which happens before this gate
//! and which one held connection can drive without reconnecting even once.
//!
//! # How these tests know
//!
//! The observable is the external handler's delivery log. A `Subscribe` reaches it untouched, so
//! an arrival records exactly one thing: this node *dispatched* an anonymous message, which is
//! the resource being bounded. Payloads the node answers itself never reach that log.
//!
//! [`authenticating_buys_no_exemption`] is the control that matters most. The draft this replaces
//! charged abandoned connections and forgave authenticated ones, which is no bound at all: Hello
//! authentication is not scarce, so an attacker self-mints an Ed25519 DID, a TLS certificate and
//! a binding of the one over the other, and buys the exemption for the price of a keypair. That
//! test performs exactly that sequence, with real self-issued identities, against the real node.
//!
//! [`many_peers_behind_one_source_all_authenticate`] is the positive control. A bound on a shared
//! address is only worth having if it leaves honest peers alone, and a NAT full of peers
//! restarting together is precisely the case that looks like an attack from outside.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_net::{
    IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage, VersionInfo,
    MAX_PREAUTH_CONNECTIONS_PER_SOURCE, PREAUTH_SOURCE_BURST, PREAUTH_SOURCE_RENEWAL_WINDOW,
};
use quinn::{ClientConfig, Endpoint};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
    // makes every TLS handshake fail, which presents as a connect timeout and would look exactly
    // like the budget refusing messages.
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

/// Every anonymous message the node dispatched to its external handler.
#[derive(Clone, Default)]
struct Delivered(Arc<Mutex<Vec<String>>>);

impl Delivered {
    fn count(&self) -> usize {
        self.0.lock().expect("delivery log poisoned").len()
    }
}

/// The per-connection anonymous burst this node granted before #2549.
///
/// Derived from the shipped constants rather than written down, so it cannot drift away from the
/// composition `PREAUTH_SOURCE_BURST` is built out of.
const LEGACY_PER_CONNECTION_BURST: usize =
    PREAUTH_SOURCE_BURST as usize / MAX_PREAUTH_CONNECTIONS_PER_SOURCE;

/// The most anonymous dispatches one source may cause in `elapsed`.
///
/// `burst + rate * elapsed`, the honest reading of a token bucket. Computed from measured time so
/// no assertion below depends on how long a QUIC handshake happened to take on this machine.
fn allowance(elapsed: Duration) -> f64 {
    let rate = PREAUTH_SOURCE_BURST as f64 / PREAUTH_SOURCE_RENEWAL_WINDOW.as_secs_f64();
    PREAUTH_SOURCE_BURST as f64 + rate * elapsed.as_secs_f64()
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` starts no inbound accept loop
/// without one, and every upper-bound assertion below would then pass for the wrong reason.
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
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, addr, Guard(shutdown_tx), delivered));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

/// How long to wait for the node's answer to a Hello before calling it a refusal.
///
/// Short on purpose, unlike the liveness backstops in the sibling suites: here a Hello going
/// unanswered is a *result* — it is what an exhausted budget looks like from the wire — so tests
/// that expect one must not each pay ten seconds for it.
const HELLO_RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

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

    /// Send one anonymous message that reaches the external handler.
    ///
    /// `Subscribe` specifically, because it arrives there untouched — no verification and no
    /// state change — so one arrival means exactly one anonymous dispatch was funded. Payloads
    /// the node answers itself never reach the log, which would make every count below zero.
    async fn tag(&self, to: &Did) -> Result<()> {
        let message = NetworkMessage::subscribe(
            self.identity.did().clone(),
            to.clone(),
            vec!["icn.test.2549".to_string()],
        );
        self.send(&message).await
    }

    /// Send a real, self-issued Hello and wait for the node to answer it.
    ///
    /// Nothing about this identity was granted by anybody: it is an Ed25519 DID, a TLS
    /// certificate, and a binding of the one over the other, all minted locally. That is the
    /// whole point — it is what makes "authenticated" useless as a discriminator, and it is why
    /// authentication is charged rather than forgiven.
    ///
    /// `Ok(false)` means the node never answered, which is what an exhausted anonymous budget
    /// looks like from the wire: the Hello was dropped before dispatch. Distinguished from an
    /// error so a test can assert on it.
    async fn authenticate(&self, to: &Did) -> Result<bool> {
        self.authenticate_within(to, HELLO_RESPONSE_TIMEOUT).await
    }

    /// As [`Self::authenticate`], but with an explicit patience for the answer.
    ///
    /// A retrying caller needs this. A refused Hello is answered with *silence* — it is dropped
    /// before dispatch, and nothing tells the peer why — so a caller that waits the full default
    /// for each attempt spends its whole retry window inside the first one, and would conclude
    /// the peer never got in when it had simply never asked twice.
    async fn authenticate_within(&self, to: &Did, patience: Duration) -> Result<bool> {
        let message = NetworkMessage::new(
            self.identity.did().clone(),
            Some(to.clone()),
            icn_net::MessagePayload::Hello {
                binding_info: self.identity.binding_info(),
                version_info: Some(VersionInfo::new("2549-test".to_string())),
                topology_info: None,
                x25519_public: *self.identity.x25519_public_bytes(),
                ml_dsa_public: None,
                ml_kem_public: None,
                pq_binding_proof: None,
            },
        );
        self.send(&message).await?;

        // The node answers on a stream *it* opens, not on the one the Hello arrived on.
        let accepted = tokio::time::timeout(patience, self.connection.accept_bi()).await;
        let Ok(Ok((_send, mut recv))) = accepted else {
            return Ok(false);
        };
        Ok(
            tokio::time::timeout(patience, icn_net::read_message(&mut recv))
                .await
                .is_ok_and(|read| read.is_ok()),
        )
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

/// Spend one source's whole anonymous allowance.
///
/// Takes several connections, and has to: the per-connection burst (#2491) still caps what any
/// single session may spend, so draining the source requires roughly
/// `PREAUTH_SOURCE_BURST / LEGACY_PER_CONNECTION_BURST` of them. Sequential, because #2547
/// independently allows only `MAX_PREAUTH_CONNECTIONS_PER_SOURCE` anonymous connections at once
/// and holding them all open would run into that bound instead of this one.
///
/// Overshoots deliberately — the source bucket refills while the connections are being made, so
/// draining it exactly would be a race.
async fn drain_source(
    node: &IdentityBundle,
    addr: SocketAddr,
    delivered: &Delivered,
) -> Result<()> {
    let connections = (PREAUTH_SOURCE_BURST as usize / LEGACY_PER_CONNECTION_BURST) + 3;
    for _ in 0..connections {
        let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
        for _ in 0..(LEGACY_PER_CONNECTION_BURST + 5) {
            let _ = client.tag(node.did()).await;
        }
        settle(delivered).await;
        drop(client);
    }
    Ok(())
}

/// Let the node finish dispatching before anything is counted.
///
/// Waits for the delivery total to stop moving rather than sleeping a fixed amount, so it can
/// only ever admit *more* messages than a fixed wait would — never fewer. That direction matters:
/// every assertion below is an upper bound, so a premature read could only make a test pass when
/// it should fail, and this removes that.
async fn settle(delivered: &Delivered) {
    let mut last = usize::MAX;
    for _ in 0..40 {
        let now = delivered.count();
        if now == last {
            return;
        }
        last = now;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---------------------------------------------------------------------------
// A1 / A3 — churn must not renew the allowance
// ---------------------------------------------------------------------------

/// A source that reconnects does not get a fresh anonymous budget each time.
///
/// Every client here connects from loopback, so they are all one source. Each cycle spends more
/// than one connection's old burst and then closes, which before this change discarded the spent
/// budget along with the connection and minted a full one for the next.
///
/// Both assertions are load-bearing. The upper bound is the property; the comparison against the
/// old per-connection behaviour is what makes the test fail on unfixed code rather than passing
/// because the traffic was too thin to reach any limit at all.
#[tokio::test]
async fn churn_does_not_renew_the_anonymous_allowance() -> Result<()> {
    init();
    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    const CYCLES: usize = 12;
    let per_cycle = LEGACY_PER_CONNECTION_BURST + 5;

    let started = Instant::now();
    for _ in 0..CYCLES {
        let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
        for _ in 0..per_cycle {
            // A refused message is dropped, not rejected, so writes keep succeeding; the
            // delivery log is the only place the refusal is visible.
            let _ = client.tag(node.did()).await;
        }
        settle(&delivered).await;
        drop(client);
    }
    settle(&delivered).await;
    let elapsed = started.elapsed();
    let count = delivered.count();

    assert!(
        (count as f64) <= allowance(elapsed),
        "one source dispatched {count} anonymous messages in {elapsed:?}, above the \
         burst-plus-refill allowance of {:.1}",
        allowance(elapsed)
    );
    assert!(
        count < CYCLES * LEGACY_PER_CONNECTION_BURST,
        "{count} dispatches is what {CYCLES} independent per-connection budgets would have \
         funded, so nothing aggregated them"
    );
    // Non-vacuity: the volley has to be able to breach the bound, or the assertions above say
    // nothing about whether one exists.
    assert!(
        ((CYCLES * LEGACY_PER_CONNECTION_BURST) as f64) > allowance(elapsed),
        "this run could not have detected a missing bound: {CYCLES} full per-connection bursts \
         is {} dispatches, inside the allowance of {:.1}",
        CYCLES * LEGACY_PER_CONNECTION_BURST,
        allowance(elapsed)
    );
    Ok(())
}

/// A reconnect continues the allowance it already spent instead of starting over.
///
/// The sharpest form of the defect: spend the source's whole budget, then open a fresh
/// connection and ask for a full per-connection burst again. What it is allowed is only what
/// refilled while it was being opened.
#[tokio::test]
async fn a_reconnect_does_not_mint_a_fresh_burst() -> Result<()> {
    init();
    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    drain_source(&node, addr, &delivered).await?;
    let after_drain = delivered.count();

    // Reconnect and ask for what a fresh per-connection budget used to give.
    let resumed = Instant::now();
    let second = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
    for _ in 0..LEGACY_PER_CONNECTION_BURST {
        let _ = second.tag(node.did()).await;
    }
    settle(&delivered).await;
    let gained = delivered.count() - after_drain;

    let refilled = allowance(resumed.elapsed()) - PREAUTH_SOURCE_BURST as f64;
    assert!(
        (gained as f64) <= refilled + 1.0,
        "the second connection was funded {gained} dispatches, more than the {refilled:.1} that \
         refilled while it was being opened — reconnecting minted a fresh budget"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// A2 — the control: authentication must not buy an exemption
// ---------------------------------------------------------------------------

/// Spending the anonymous budget and *then* authenticating does not restore it.
///
/// This is the sequence the previous design was defeated by. It charged connections that were
/// abandoned and forgave connections that authenticated, on the theory that authentication marks
/// an honest peer. It does not mark anything: every identity below is minted by the test, in a
/// loop, with no allowlist and no trust gate anywhere in its way. Under that design this loop
/// left the budget untouched at full.
///
/// Here every cycle spends first and authenticates second, and the Hello is itself charged —
/// so the sequence costs strictly *more* than abandoning would, never less.
///
/// Each cycle deliberately stops one message short of the per-connection burst. Spending all of
/// it would leave nothing to pay for the Hello, and the loop would then prove only that an
/// exhausted connection cannot authenticate — which is true, and is not the question.
#[tokio::test]
async fn authenticating_buys_no_exemption() -> Result<()> {
    init();
    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    const CYCLES: usize = 16;
    let per_cycle = LEGACY_PER_CONNECTION_BURST - 1;

    let started = Instant::now();
    let mut authenticated = 0usize;
    for _ in 0..CYCLES {
        // A fresh self-issued identity every cycle, because an attacker has no reason to reuse
        // one and a bound that assumed otherwise would be keyed on something it chooses.
        let identity = IdentityBundle::generate()?;
        let client = WireClient::connect(&identity, addr).await?;
        for _ in 0..per_cycle {
            let _ = client.tag(node.did()).await;
        }
        if client.authenticate(node.did()).await.unwrap_or(false) {
            authenticated += 1;
        }
        settle(&delivered).await;
        drop(client);
    }
    settle(&delivered).await;
    let elapsed = started.elapsed();
    let count = delivered.count();
    let attempted = CYCLES * per_cycle;

    assert!(
        (count as f64) <= allowance(elapsed),
        "authenticate-then-churn funded {count} anonymous dispatches in {elapsed:?}, above the \
         allowance of {:.1}: authentication refunded or avoided the charge",
        allowance(elapsed)
    );
    // Non-vacuity: the volley has to be able to breach the bound, or the assertion above says
    // nothing. Every one of these would have landed before this change, since each cycle arrived
    // with a full per-connection burst of its own.
    assert!(
        (attempted as f64) > allowance(elapsed),
        "only {attempted} dispatches were attempted against an allowance of {:.1}; this run could \
         not have detected a missing bound",
        allowance(elapsed)
    );
    // Not an assertion about how many succeeded — the budget legitimately starves later cycles.
    // Recorded so a run where *nothing* ever authenticated cannot masquerade as a passing bound.
    assert!(
        authenticated > 0,
        "no cycle ever authenticated, so this never exercised the authenticated path at all"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// A4 / A5 — the shared-source positive controls
// ---------------------------------------------------------------------------

/// Many honest peers behind one address all authenticate, in one burst.
///
/// The case a per-source bound must not break, and the reason the burst is sized at the node's
/// existing *simultaneous* allowance rather than at one connection's worth: a cooperative behind
/// one NAT restarting together looks exactly like churn from outside. Each peer spends a single
/// token — one Hello — so far more of them fit than either the concurrency limit or the old
/// per-connection burst would suggest.
///
/// Sequential because #2547 independently allows only
/// `MAX_PREAUTH_CONNECTIONS_PER_SOURCE` anonymous connections at once; this test is about the
/// rate bound, and holding them all open would measure the other one.
#[tokio::test]
async fn many_peers_behind_one_source_all_authenticate() -> Result<()> {
    init();
    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, _delivered) = spawn_node(node.clone()).await?;

    // Comfortably above both the concurrency limit and the old per-connection burst, and well
    // inside the source burst, which is what the property claims.
    let peers = LEGACY_PER_CONNECTION_BURST + MAX_PREAUTH_CONNECTIONS_PER_SOURCE;
    assert!(
        peers < PREAUTH_SOURCE_BURST as usize,
        "the honest-NAT control must sit inside the burst it is testing"
    );

    let mut authenticated = 0usize;
    for _ in 0..peers {
        let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
        if client.authenticate(node.did()).await? {
            authenticated += 1;
        }
    }

    assert_eq!(
        authenticated, peers,
        "only {authenticated} of {peers} honest peers behind one address authenticated; a \
         restart storm inside the burst must not be throttled"
    );
    Ok(())
}

/// An honest peer sharing a drained address is delayed, not locked out.
///
/// The collateral, quantified rather than asserted away. A peer needs one token to authenticate
/// and the bucket refills continuously, so the wait is about one token's worth of time — and the
/// connection survives the refusal, because a throttled anonymous message is dropped rather than
/// closing the session.
#[tokio::test]
async fn an_honest_peer_sharing_a_drained_source_recovers() -> Result<()> {
    init();
    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    drain_source(&node, addr, &delivered).await?;

    // Ten token-times, so the assertion is about the bound and not about scheduler jitter.
    let one_token = PREAUTH_SOURCE_RENEWAL_WINDOW.div_f64(PREAUTH_SOURCE_BURST as f64);
    let patience = one_token * 10;

    // Per attempt, not for the whole test: a refused Hello is answered with silence, so waiting
    // the full patience on the first attempt would spend the entire window without ever retrying
    // — which is what this test did before, and it failed for that reason rather than for the
    // property. When a token *is* available the answer is a couple of signature verifications
    // away, so one token-time is generous for a successful round trip.
    let per_attempt = one_token;

    let honest = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
    let waited_from = Instant::now();
    let mut attempts = 0usize;
    let mut got_in = false;
    while waited_from.elapsed() < patience {
        attempts += 1;
        if honest.authenticate_within(node.did(), per_attempt).await? {
            got_in = true;
            break;
        }
    }

    assert!(
        got_in,
        "an honest peer behind a drained source never authenticated in {attempts} attempts over \
         {patience:?}, which is ten times the {one_token:?} one token takes to refill"
    );
    Ok(())
}
