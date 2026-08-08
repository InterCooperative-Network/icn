#![allow(clippy::unwrap_used, clippy::expect_used)]
//! How long an admitted connection may stay unauthenticated (#2552).
//!
//! #2551 bounded the *number* of established-but-unauthenticated inbound connections, per
//! source and server-wide. It deliberately released the reservation at authentication rather
//! than at close, which is what keeps the bound off legitimate peers. Nothing, however,
//! required a connection to ever reach that point:
//!
//! ```text
//! handshake completes -> slot acquired -> peer says nothing -> slot held until the peer
//!                                                              closes, or the node restarts
//! ```
//!
//! Four mechanisms could have terminated such a connection and none does. The
//! concurrent-handshake permit is dropped before admission is taken. `max_idle_timeout` (60 s)
//! never fires, because this node installs `keep_alive_interval` (30 s) on its **server**
//! transport config and so PINGs its own inbound connections — the peer's QUIC stack answers
//! at the transport layer, for free, and the idle timer never advances. The session map holds
//! only authenticated peers (#2530), so nothing evicts it. And the admission table has, by
//! design, no expiry.
//!
//! So a spatial bound without a temporal one is a bound an attacker can pin at its ceiling
//! and keep there. `MAX_PREAUTH_CONNECTIONS_PER_SOURCE` silent connections from enough sources
//! refuse *all* new inbound connections, because every new peer starts anonymous.
//!
//! # The property under test
//!
//! > A connection that has taken a pre-authentication slot must authenticate within
//! > [`PREAUTH_AUTHENTICATION_DEADLINE`], or its transport is closed and its slot released.
//!
//! Duration, not rate and not count. These tests say nothing about how fast a source may churn
//! connections (#2549) or how many it may hold once authenticated (#2550).
//!
//! # How these tests know
//!
//! Two structural observables, never a bare sleep:
//!
//! - **the QUIC application close code.** A timed-out connection is closed with
//!   [`PREAUTH_AUTHENTICATION_TIMEOUT_CODE`], which no other path uses, so the client can
//!   distinguish "the node timed me out" from "the node went away".
//! - **slot reuse.** A connection that could not be admitted while the table was full, and can
//!   be admitted afterwards, proves the slot actually returned to the table — which closing the
//!   transport alone would not.
//!
//! Every negative is paired with a positive control:
//! [`authenticating_before_the_deadline_survives_it`] proves the deadline does not touch peers
//! that identify themselves, and is the assertion that fails if a timer were left running past
//! authentication.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_net::{
    IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage, VersionInfo,
    MAX_PREAUTH_CONNECTIONS_PER_SOURCE, PREAUTH_AUTHENTICATION_DEADLINE,
    PREAUTH_AUTHENTICATION_TIMEOUT_CODE,
};
use quinn::{ClientConfig, Endpoint};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

/// How long past the deadline a test waits before calling the node's behaviour final.
///
/// Slack for scheduling, not a timing assumption: every assertion below is about *what*
/// happened (a close code, an admission), never about when. Widening this can only make a
/// failing test take longer to fail — it cannot turn a failure into a pass.
const SLACK: Duration = Duration::from_secs(10);

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
    // exactly like the deadline closing connections.
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
/// A connection with no handler loop — refused, or closed by the deadline — contributes
/// nothing here, so counting distinct tags counts connections that were live at the time.
#[derive(Clone, Default)]
struct Delivered(Arc<Mutex<Vec<String>>>);

impl Delivered {
    fn saw(&self, did: &Did) -> bool {
        self.0
            .lock()
            .expect("delivery log poisoned")
            .iter()
            .any(|seen| seen == did.as_str())
    }
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` starts no inbound accept loop
/// without one, and every assertion below would then hold for the wrong reason.
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
                // Returned rather than dropped: the handle owns the actor's mailbox sender.
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, addr, Guard(shutdown_tx), delivered));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

/// How long to wait for the node's answer to a Hello before calling it a failure.
///
/// A liveness backstop so a broken node fails the test instead of hanging it, not a timing
/// assumption. The property is carried by the response arriving at all.
const HELLO_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Transport settings for a client that intends to *keep* what it was given.
///
/// This is the difference between measuring the node and measuring the test harness, and it is
/// load-bearing enough to be worth stating plainly.
///
/// A quinn client left on its defaults gives up on an idle connection after about thirty
/// seconds. Every test below then ends with the connection closed and the node's slot returned —
/// on unfixed code, with no deadline anywhere — because the *client* got bored. A slot-reuse
/// assertion passes, a close is observed, and the whole file certifies a fix that does nothing.
/// (The close-code assertions are what exposed this: the observed close was a transport idle
/// timeout, not an application close.)
///
/// A real adversary is not polite. Holding a connection open costs it nothing, so it sets a long
/// idle timeout and keeps its own side alive — which is exactly what makes the defect durable
/// rather than self-healing. Modelling that is the whole point: with these settings the *only*
/// thing that can end a connection here is the node deciding to end it.
fn squatter_transport() -> quinn::TransportConfig {
    let mut transport = quinn::TransportConfig::default();
    // Comfortably longer than any deadline these tests wait out, so the client's own timer can
    // never be the cause of a close.
    transport.max_idle_timeout(Some(
        Duration::from_secs(600)
            .try_into()
            .expect("600s is a valid QUIC idle timeout"),
    ));
    transport.keep_alive_interval(Some(Duration::from_secs(2)));
    transport
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
        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client)?,
        ));
        client_config.transport_config(Arc::new(squatter_transport()));
        endpoint.set_default_client_config(client_config);

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

    fn did(&self) -> &Did {
        self.identity.did()
    }

    /// Send one `Subscribe` tagged with this client's own DID.
    ///
    /// `Subscribe` specifically, because it reaches the external handler untouched — no
    /// verification and no state change — so an arrival records exactly one thing: this
    /// connection had a live handler loop. Payloads the node answers itself (`Ping`, `Hello`)
    /// never reach the handler.
    async fn tag(&self, to: &Did) -> Result<()> {
        let message = NetworkMessage::subscribe(
            self.identity.did().clone(),
            to.clone(),
            vec!["icn.test.2552".to_string()],
        );
        self.send(&message).await
    }

    /// Authenticate, and do not return until the node has processed the Hello.
    ///
    /// The node answers a valid Hello on a stream *it* opens, and it opens that stream strictly
    /// after binding the identity to the connection. So the answer arriving is the
    /// synchronisation: authentication had already been recorded when it was written.
    async fn authenticate(&self, to: &Did) -> Result<()> {
        let message = NetworkMessage::new(
            self.identity.did().clone(),
            Some(to.clone()),
            icn_net::MessagePayload::Hello {
                binding_info: self.identity.binding_info(),
                version_info: Some(VersionInfo::new("2552-test".to_string())),
                topology_info: None,
                x25519_public: *self.identity.x25519_public_bytes(),
                ml_dsa_public: None,
                ml_kem_public: None,
                pq_binding_proof: None,
            },
        );

        self.send(&message).await?;

        let (_send, mut recv) =
            tokio::time::timeout(HELLO_RESPONSE_TIMEOUT, self.connection.accept_bi())
                .await
                .context("timed out waiting for the node to answer the Hello")?
                .context("accepting the node's Hello response stream")?;
        tokio::time::timeout(HELLO_RESPONSE_TIMEOUT, icn_net::read_message(&mut recv))
            .await
            .context("timed out reading the node's Hello response")?
            .context("reading the node's Hello response")?;
        Ok(())
    }

    /// Send a Hello carrying somebody else's binding.
    ///
    /// The binding names `impostor` while `from` names this client, so
    /// `verify_did_matches_binding` rejects it — the first of the three DID-TLS facts
    /// `handle_hello` requires (#2520).
    async fn send_invalid_hello(&self, to: &Did, impostor: &IdentityBundle) -> Result<()> {
        let message = NetworkMessage::new(
            self.identity.did().clone(),
            Some(to.clone()),
            icn_net::MessagePayload::Hello {
                binding_info: impostor.binding_info(),
                version_info: Some(VersionInfo::new("2552-test".to_string())),
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

    /// Wait for the node to close this connection, and report why.
    ///
    /// Returns the QUIC application close code, which is the structural observable: a
    /// deadline close is distinguishable from every other reason the connection could end.
    async fn closed_within(&self, patience: Duration) -> Result<u64> {
        let reason = tokio::time::timeout(patience, self.connection.closed())
            .await
            .context("the connection was still open")?;
        match reason {
            quinn::ConnectionError::ApplicationClosed(closed) => Ok(closed.error_code.into_inner()),
            other => anyhow::bail!("connection ended for a non-application reason: {other}"),
        }
    }
}

/// Let the node finish dispatching before anything is counted.
///
/// Waits for the delivery total to stop moving rather than sleeping a fixed amount.
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

// ---------------------------------------------------------------------------
// RED — an admitted connection must not hold its slot forever
// ---------------------------------------------------------------------------

/// RED A — the transport of a silent connection is closed, with a reason that names why.
///
/// The close code matters as much as the close. Releasing the slot while leaving the anonymous
/// connection alive would satisfy the admission table and leave the resource the table exists
/// to bound running free, so this asserts the transport itself ended — and ended *because of
/// the deadline*, not because the node went away.
#[tokio::test]
async fn a_silent_connection_is_closed_when_its_deadline_expires() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, _delivered) = spawn_node(node.clone()).await?;

    let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;

    let code = client
        .closed_within(PREAUTH_AUTHENTICATION_DEADLINE + SLACK)
        .await
        .context(
            "an admitted connection that never authenticated was still open past its deadline",
        )?;

    assert_eq!(
        code,
        u64::from(PREAUTH_AUTHENTICATION_TIMEOUT_CODE),
        "the connection was closed, but not by the pre-authentication deadline"
    );
    Ok(())
}

/// RED B — the slot a silent connection held becomes reusable.
///
/// Closing the transport is not enough on its own: the bound is on the *reservation*, and a
/// close that failed to release it would leave the table permanently full. This drives the
/// per-source allowance to its ceiling, confirms it is genuinely full, and then confirms that
/// waiting out the deadline makes the source admissible again.
#[tokio::test]
async fn expiring_deadlines_return_slots_to_the_source() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    // Fill this source's whole anonymous allowance, and keep every one of them silent.
    let mut squatters = Vec::new();
    for _ in 0..MAX_PREAUTH_CONNECTIONS_PER_SOURCE {
        squatters.push(WireClient::connect(&IdentityBundle::generate()?, addr).await?);
    }

    // Positive control on the precondition: with the table full, a further connection is
    // refused, so it never gets a handler loop and cannot deliver. Without this the final
    // assertion could pass on a node that was never full in the first place.
    let refused = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
    let _ = refused.tag(node.did()).await;
    settle(&delivered).await;
    assert!(
        !delivered.saw(refused.did()),
        "the source's allowance was not full, so this test cannot show a slot being returned"
    );

    // Wait the squatters out structurally, not on a clock.
    //
    // A fixed `DEADLINE + SLACK` sleep costs its full length every run and proves less than it
    // looks: it establishes that time passed, not that the node expired anything, so it would
    // read identically if the connections had ended for some other reason entirely. Observing
    // each close — and its code — is the stronger statement and returns as soon as it is true.
    for squatter in &squatters {
        let code = squatter
            .closed_within(PREAUTH_AUTHENTICATION_DEADLINE + SLACK)
            .await
            .context("a squatting connection was still open past its deadline")?;
        assert_eq!(
            code,
            u64::from(PREAUTH_AUTHENTICATION_TIMEOUT_CODE),
            "a squatting connection ended, but not because of the pre-authentication deadline"
        );
    }

    // Closed is not yet released, and the difference is the point of this test. The frame the
    // client just saw is sent by `connection.close()`, which runs *before* the handler returns
    // and drops the guard, so at this instant every release is imminent rather than done.
    // Retrying converges as fast as the node actually releases, and still fails outright if the
    // release never happens — which a sleep long enough to hide the window would not.
    let mut admitted = false;
    for _ in 0..20 {
        let after = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
        after.tag(node.did()).await?;
        settle(&delivered).await;
        if delivered.saw(after.did()) {
            admitted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        admitted,
        "the source was still at its ceiling after every squatting connection timed out: the \
         deadline closed connections without returning their reservations"
    );

    drop(squatters);
    Ok(())
}

// ---------------------------------------------------------------------------
// Positive controls
// ---------------------------------------------------------------------------

/// A peer that authenticates in time is untouched by the deadline.
///
/// This is the assertion that fails if authentication did not cancel the deadline, or if a
/// timer were left running that could close the connection afterwards. It waits well past the
/// deadline and then requires the connection to still carry traffic — liveness proven by use,
/// not by the absence of a close event.
#[tokio::test]
async fn authenticating_before_the_deadline_survives_it() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, delivered) = spawn_node(node.clone()).await?;

    let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
    client.authenticate(node.did()).await?;

    // Well past the point at which an unauthenticated connection would have been closed.
    tokio::time::sleep(PREAUTH_AUTHENTICATION_DEADLINE + SLACK).await;

    client
        .tag(node.did())
        .await
        .context("an authenticated connection was closed by the pre-authentication deadline")?;
    settle(&delivered).await;

    assert!(
        delivered.saw(client.did()),
        "an authenticated connection stopped being served after the pre-authentication \
         deadline elapsed — the deadline outlived the phase it belongs to"
    );
    Ok(())
}

/// Traffic is not authentication: sending messages does not buy more time.
///
/// A peer that keeps a connection busy without ever saying who it is holds exactly the
/// resource this bounds. The deadline is therefore absolute from admission, not a
/// last-activity idle timer — the distinction between "authenticate within T" and "send
/// packets", which is the property #2552 is about.
#[tokio::test]
async fn unauthenticated_traffic_does_not_extend_the_deadline() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, _delivered) = spawn_node(node.clone()).await?;

    let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;

    // The chatter must run on *the connection under assertion*, not a sibling. A mutation that
    // let any inbound message clear the deadline would leave a silent observer timing out
    // exactly on schedule, so asserting on a second connection would pass straight through it.
    let chatter = tokio::spawn({
        let connection = client.connection.clone();
        let from = client.did().clone();
        let to = node.did().clone();
        async move {
            // Every ~3 s across the whole window: far inside the anonymous budget (burst 20,
            // refilling every 100 ms), so these are genuinely read and dispatched rather than
            // dropped by the rate limiter — which would make the traffic decorative.
            loop {
                for message in [
                    NetworkMessage::ping(from.clone(), to.clone()),
                    NetworkMessage::subscribe(
                        from.clone(),
                        to.clone(),
                        vec!["icn.test.2552".to_string()],
                    ),
                ] {
                    if let Ok((mut send, _recv)) = connection.open_bi().await {
                        let _ = icn_net::write_message(&mut send, &message).await;
                        let _ = send.finish();
                    }
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    });

    let code = client
        .closed_within(PREAUTH_AUTHENTICATION_DEADLINE + SLACK)
        .await
        .context("a connection sending pre-authentication traffic outlived its deadline");
    chatter.abort();

    assert_eq!(
        code?,
        u64::from(PREAUTH_AUTHENTICATION_TIMEOUT_CODE),
        "the connection was closed, but not by the pre-authentication deadline"
    );
    Ok(())
}

/// An invalid Hello authenticates nothing, and costs the connection immediately.
///
/// Pins why "a failed Hello cancels the deadline" is not a state this design can reach: a
/// Hello whose binding does not verify makes `handle_hello` return an error, which ends the
/// handler loop there and then. The connection is gone strictly *before* its deadline, so
/// there is no window in which a rejected Hello could have cancelled anything.
#[tokio::test]
async fn an_invalid_hello_does_not_authenticate_the_connection() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, _delivered) = spawn_node(node.clone()).await?;

    let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;
    let impostor = IdentityBundle::generate()?;
    client.send_invalid_hello(node.did(), &impostor).await?;

    // Strictly sooner than the deadline: this connection is not being timed out, it is being
    // rejected. Asserting the tighter bound is what keeps the two causes distinguishable.
    let ended = tokio::time::timeout(HELLO_RESPONSE_TIMEOUT, client.connection.closed()).await;
    assert!(
        ended.is_ok(),
        "a connection whose Hello failed DID-TLS binding verification was still open"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// RED — the deadline must not be starvable by keeping the handler busy
// ---------------------------------------------------------------------------

/// A peer that always has another stream waiting must not outlive its deadline.
///
/// [`unauthenticated_traffic_does_not_extend_the_deadline`] sends a burst every few seconds, so
/// the node spends nearly all of the window *waiting* — and a wait is where the deadline was
/// being enforced. That leaves the adversarial case untested: a peer that never lets the wait
/// happen at all.
///
/// The enforcement point was a `select!` marked `biased`, which polls its arms in order and
/// returns on the first that is ready, without polling the rest. A tokio timer is not an
/// interrupt — a `Sleep` only completes when something polls it. So if `accept_bi` is ready on
/// every iteration, the timer arm is never polled, never registered, and the absolute deadline
/// passes unobserved however far into the past it goes.
///
/// `max_concurrent_bidi_streams` is 10, which makes the attack self-clocking rather than
/// bandwidth-bound: each opener parks until the node retires a stream and opens the replacement
/// immediately, so several openers keep a waiter queued for every credit the node frees. The
/// anonymous budget (#2491) does not help — an exhausted budget `continue`s the loop, which is
/// the *fastest* path back to `accept_bi` and so makes the queue easier to keep full, not harder.
#[tokio::test]
async fn a_continuous_stream_backlog_does_not_starve_the_deadline() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, _delivered) = spawn_node(node.clone()).await?;

    let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;

    let mut flood = Vec::new();
    for _ in 0..4 {
        flood.push(tokio::spawn({
            let connection = client.connection.clone();
            let from = client.did().clone();
            let to = node.did().clone();
            async move {
                while let Ok((mut send, _recv)) = connection.open_bi().await {
                    let message = NetworkMessage::subscribe(
                        from.clone(),
                        to.clone(),
                        vec!["icn.test.2552".to_string()],
                    );
                    let _ = icn_net::write_message(&mut send, &message).await;
                    let _ = send.finish();
                }
            }
        }));
    }

    let code = client
        .closed_within(PREAUTH_AUTHENTICATION_DEADLINE + SLACK)
        .await;
    for opener in flood {
        opener.abort();
    }

    assert_eq!(
        code.context(
            "a connection that kept a stream always ready outlived its pre-authentication \
             deadline"
        )?,
        u64::from(PREAUTH_AUTHENTICATION_TIMEOUT_CODE),
        "the connection was closed, but not by the pre-authentication deadline"
    );
    Ok(())
}

/// One stream the peer opens and never completes must not postpone the deadline either.
///
/// The same starvation reached without any flood, and without racing the scheduler for it.
/// `read_message` is called *inside* the loop body, after the deadline has been left behind,
/// and it has no timeout of its own: `read_exact` on a stream whose remaining bytes never
/// arrive is an await that never returns. The node is then parked in the body of an iteration
/// forever, so the deadline is not merely lost to a biased poll — it is never reached again.
///
/// Three bytes of a four-byte length prefix is the whole attack. It costs one stream and one
/// packet, and unlike the flood it is deterministic: there is no instant at which the node
/// could have noticed.
#[tokio::test]
async fn a_stalled_stream_does_not_postpone_the_deadline() -> Result<()> {
    init();

    let node = IdentityBundle::generate()?;
    let (_handle, addr, _guard, _delivered) = spawn_node(node.clone()).await?;

    let client = WireClient::connect(&IdentityBundle::generate()?, addr).await?;

    // Held for the rest of the test: dropping the stream would reset it and hand the node the
    // error that ends its wait, which is the node escaping rather than the node coping.
    let (mut send, _recv) = client.connection.open_bi().await?;
    send.write_all(&[0u8, 0, 0]).await?;

    let code = client
        .closed_within(PREAUTH_AUTHENTICATION_DEADLINE + SLACK)
        .await
        .context(
            "a connection holding one incomplete stream outlived its pre-authentication deadline",
        )?;

    assert_eq!(
        code,
        u64::from(PREAUTH_AUTHENTICATION_TIMEOUT_CODE),
        "the connection was closed, but not by the pre-authentication deadline"
    );
    Ok(())
}
