#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A bootstrap entry that names a DID states an *expectation*, and an expectation that goes
//! unmet must not go unrecorded.
//!
//! `icn://did:icn:X@HOST:PORT` says "I expect X at this address". Nothing about reaching that
//! address proves X is there: identity is proven only by the DID-TLS binding checks in
//! `handle_hello` (#2520), and whoever passes them becomes the canonical peer (#2530). So
//! when the operator configured X and B answers, the node has learned something the operator
//! has not — and until #2533 it kept it to itself.
//!
//! ## What these tests pin, and what they deliberately do not
//!
//! The policy pinned here is **warn-and-accept**: a divergence is recorded, and the
//! authenticated peer stays canonical. That is a decision, not a law of nature — the
//! alternative is a strict pin that refuses the connection, which would turn a stale
//! bootstrap entry into a hard connectivity failure after any key rotation or redeploy.
//! `an_unmet_expectation_is_recorded_and_the_authenticated_peer_stays_canonical` asserts
//! *both* halves, so a future change to strict rejection has to come and edit it. The
//! policy is therefore executable, not folklore.
//!
//! ## Why the counter is the oracle
//!
//! The property is "this stopped being silent", and silence is only measurable against
//! something that speaks. A log line would need a global subscriber; the counter can be read
//! from a recorder that is local to this thread, so each test observes exactly the events it
//! caused and nothing another test caused. `metrics::with_local_recorder` installs a
//! *thread-local* recorder, which is why each test drives a current-thread runtime by hand:
//! on a multi-thread runtime the connection handler task would run on a worker thread that
//! cannot see the recorder, and every assertion would read zero for the wrong reason.
//!
//! The label set is asserted to be empty. That is not tidiness: the two values a mismatch is
//! *about* — the expected DID and the authenticated one — are exactly the values that must
//! never become label dimensions, because they are unbounded. They belong in the log line,
//! which is a diagnostic record, not in a time series.

use anyhow::{Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_net::{IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage};
use metrics::with_local_recorder;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use quinn::Endpoint;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

const MISMATCH_METRIC: &str = "icn_network_bootstrap_did_expectation_mismatch_total";

/// Ports already handed out in this process.
///
/// `pick_unused_port` binds, closes, and returns the port, so two tests running concurrently
/// can be handed the same one — the port is genuinely free at each moment it is checked.
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

/// Run `body` on a current-thread runtime with a recorder only this thread can see.
///
/// Both halves matter. The recorder must be local, or concurrent tests in this binary would
/// read each other's increments. The runtime must be current-thread, or the connection
/// handler — which is where the mismatch is decided — would be polled on a worker thread
/// outside the recorder's scope.
fn run_with_local_metrics<F>(body: F) -> Result<Snapshotter>
where
    F: std::future::Future<Output = Result<()>>,
{
    let recorder = DebuggingRecorder::new();
    // Taken before the recorder is consumed: the snapshotter shares the registry, so it
    // still reads what the body recorded once the run is over.
    let snapshotter = recorder.snapshotter();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    with_local_recorder(&recorder, || runtime.block_on(body))?;
    Ok(snapshotter)
}

/// Every emitted mismatch series, as `(sorted labels, value)`.
fn mismatch_series(snapshotter: &Snapshotter) -> Vec<(Vec<(String, String)>, u64)> {
    let mut out: Vec<(Vec<(String, String)>, u64)> = snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| {
            let k = key.key();
            if k.name() != MISMATCH_METRIC {
                return None;
            }
            let mut labels: Vec<(String, String)> = k
                .labels()
                .map(|l| (l.key().to_string(), l.value().to_string()))
                .collect();
            labels.sort();
            match value {
                DebugValue::Counter(v) => Some((labels, v)),
                _ => None,
            }
        })
        .collect();
    out.sort();
    out
}

/// Total mismatches recorded, across every label set.
fn mismatch_total(snapshotter: &Snapshotter) -> u64 {
    mismatch_series(snapshotter).iter().map(|(_, v)| v).sum()
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: it is what makes `wire_new_connection` spawn a
/// dispatch loop for an *outbound* connection. Without it the dialer would never process the
/// peer's Hello, so nothing here would authenticate and every assertion would read zero for
/// a reason that has nothing to do with expectations.
async fn spawn_node(bundle: IdentityBundle) -> Result<(NetworkHandle, SocketAddr, Guard)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let handler: IncomingMessageHandler = Arc::new(|_msg: NetworkMessage| {});

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
/// `peer_connections` is written only by `handle_hello`, after the #2520 binding checks pass,
/// so this returning `true` is proof that an identity was proven on the connection — the
/// exact moment the expectation becomes answerable. Synchronising on that observable, rather
/// than sleeping, is what keeps "no mismatch recorded" distinguishable from "nothing has
/// happened yet".
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

/// A bare QUIC listener that answers each accepted connection with a scripted Hello sequence.
///
/// Two things a real node cannot do are needed here: send a Hello that fails verification,
/// and send the same Hello twice on one connection.
struct ScriptedPeer {
    addr: SocketAddr,
    accept_task: tokio::task::JoinHandle<Vec<quinn::Connection>>,
}

impl ScriptedPeer {
    /// Async because the settle below must not block: on the current-thread runtime these
    /// tests use, a `std::thread::sleep` here would also stop the accept task from being
    /// polled, so the listener would not actually be warm when it returned.
    async fn spawn(identity: &IdentityBundle, hellos: Vec<NetworkMessage>) -> Result<Self> {
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
                    for hello in &hellos {
                        if let Ok((mut send, _recv)) = conn.open_bi().await {
                            let _ = icn_net::protocol::write_message(&mut send, hello).await;
                            let _ = send.finish();
                        }
                        // Each Hello on its own stream, delivered in order: the dispatch loop
                        // accepts one stream at a time, so without a gap the second can be
                        // opened before the first is read and the ordering is not the one the
                        // test names.
                        tokio::time::sleep(Duration::from_millis(150)).await;
                    }
                    held.push(conn);
                }
            }
            held
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(Self { addr, accept_task })
    }
}

impl Drop for ScriptedPeer {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

/// Build a Hello from `speaker`, addressed to `to`, carrying `binding`'s binding info.
///
/// Passing a `binding` other than `speaker`'s own is how a Hello is made to fail the #2520
/// checks while still being a well-formed message.
fn hello_from(speaker: &IdentityBundle, to: &Did, binding: &IdentityBundle) -> NetworkMessage {
    NetworkMessage::hello(
        speaker.did().clone(),
        to.clone(),
        binding.binding_info(),
        icn_net::VersionInfo::new("icnd-scripted".to_string()),
        None,
        *speaker.x25519_public_bytes(),
        None,
        None,
    )
}

/// Positive control: when the peer that answers is the peer the operator named, nothing is
/// recorded.
///
/// This is the case that must stay quiet, and it is the reason the mismatch signal is worth
/// having at all. A counter that also fires on correct configuration would be noise, and an
/// operator would learn to ignore it.
#[test]
fn a_met_expectation_records_no_mismatch() -> Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let listener = IdentityBundle::generate()?;
        let listener_did = listener.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

        dialer_handle
            .dial_expecting(listener_addr, listener_did.clone())
            .await?;
        assert!(
            wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
            "precondition: the peer we expected never authenticated, so nothing was \
                 compared and this test would pass vacuously"
        );

        assert_eq!(
            dialer_handle.connected_peers().await,
            vec![listener_did.clone()],
            "the expected peer authenticated, so it must be the canonical peer"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        0,
        "the configured DID is who answered, so there is no divergence to report; recorded \
         series: {:?}",
        mismatch_series(&snapshotter)
    );
    Ok(())
}

/// The policy test: a divergence is recorded, **and** the peer that authenticated stays
/// canonical.
///
/// Both halves are the decision. Recording without accepting would be a strict pin — a
/// materially different availability posture, in which a bootstrap entry left stale by a key
/// rotation or a redeploy stops being degraded connectivity and becomes none at all, with no
/// in-band way to recover. Accepting without recording is the status quo #2533 exists to end.
///
/// A future change to strict rejection is legitimate, but it must come here and rewrite this
/// test, which is the point: it makes the policy something the repository states rather than
/// something it happens to do.
#[test]
fn an_unmet_expectation_is_recorded_and_the_authenticated_peer_stays_canonical() -> Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let listener = IdentityBundle::generate()?;
        let listener_did = listener.did().clone();
        // Nobody on this connection holds this DID's key — the stale-config case.
        let expected_did = IdentityBundle::generate()?.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

        dialer_handle
            .dial_expecting(listener_addr, expected_did.clone())
            .await?;
        assert!(
            wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
            "precondition: the peer that actually answered never authenticated"
        );

        assert_eq!(
            dialer_handle.connected_peers().await,
            vec![listener_did.clone()],
            "the authenticated DID stays canonical: an expectation is metadata, and \
                 {expected_did} proved nothing"
        );
        let stats = dialer_handle.get_stats().await?;
        assert_eq!(
            stats.connections_active, 1,
            "the connection is accepted, not refused — this is warn-and-accept, not a \
                 strict pin"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        1,
        "the operator configured one DID and a different one authenticated; that divergence \
         must be recorded exactly once (#2533). Recorded series: {:?}",
        mismatch_series(&snapshotter)
    );
    assert_eq!(
        mismatch_series(&snapshotter),
        vec![(Vec::new(), 1)],
        "the mismatch counter must carry no labels: the DIDs it is about are unbounded, and \
         labelling by them would let bootstrap configuration drive metric cardinality"
    );
    Ok(())
}

/// An address-only bootstrap entry states no expectation, so it can never fail one.
///
/// This is the control that keeps the feature honest. `dial_addr` synthesises a placeholder
/// DID from the socket address and hands it to the same dial path a configured DID uses, so
/// by the time the actor sees it the two are the same type carrying the same kind of value.
/// An implementation that compared "the DID we dialled with" against the authenticated one
/// would therefore report a mismatch on *every* `icn://HOST:PORT` bootstrap — manufacturing
/// an expectation the operator never wrote.
#[test]
fn an_addr_only_dial_states_no_expectation_and_never_mismatches() -> Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let listener = IdentityBundle::generate()?;
        let listener_did = listener.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

        let placeholder = dialer_handle.dial_addr(listener_addr).await?;
        assert_ne!(
            placeholder, listener_did,
            "precondition: the placeholder must differ from the authenticated DID, or \
                 this test could not distinguish the two implementations"
        );
        assert!(
            wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
            "precondition: the peer never authenticated, so no comparison was reachable"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        0,
        "an addr-only bootstrap entry names nobody, so the peer that answers cannot be the \
         wrong one; recorded series: {:?}",
        mismatch_series(&snapshotter)
    );
    Ok(())
}

/// A peer that fails the #2520 checks must not produce an expectation mismatch, however
/// different the DID it claimed.
///
/// The comparison has exactly one legitimate right-hand side: the DID *proven* against this
/// connection's certificate. `message.from` is chosen by the sender and verified by nobody,
/// so comparing against it would let whoever is at that address decide what this node reports
/// about its own configuration — and, worse, would report a divergence at a moment when the
/// node cannot say who it is talking to at all. "Not authenticated" is a statement about our
/// knowledge, not about the peer, and it is not evidence that the operator's expectation was
/// wrong.
#[test]
fn a_peer_that_never_authenticates_records_no_expectation_mismatch() -> Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let dialer_did = dialer.did().clone();
        // Presents this identity's certificate on the wire...
        let wire_identity = IdentityBundle::generate()?;
        // ...but claims to be this one, with that one's own valid — but wrong-certificate
        // — binding. Fails check (3): the binding is not of this connection's cert.
        let impersonated = IdentityBundle::generate()?;
        let impersonated_did = impersonated.did().clone();
        // What the operator configured. Different from every DID above.
        let expected_did = IdentityBundle::generate()?.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let liar = ScriptedPeer::spawn(
            &wire_identity,
            vec![hello_from(&impersonated, &dialer_did, &impersonated)],
        )
        .await?;

        dialer_handle
            .dial_expecting(liar.addr, expected_did.clone())
            .await?;
        tokio::time::sleep(Duration::from_secs(3)).await;

        let stats = dialer_handle.get_stats().await?;
        assert_eq!(
            stats.connections_active, 0,
            "precondition: the scripted Hello must be rejected, or this test is not \
                 about an unauthenticated peer at all (#2520)"
        );
        assert!(
            dialer_handle
                .get_peer_connection_info(&impersonated_did)
                .await
                .is_none(),
            "precondition: a rejected Hello must not populate authenticated peer info"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        0,
        "nobody authenticated on that connection, so there is no identity to weigh the \
         operator's expectation against; a claimed DID is not one. Recorded series: {:?}",
        mismatch_series(&snapshotter)
    );
    Ok(())
}

/// A later message *claiming* to be the expected DID does not retire the mismatch.
///
/// `message.from` is chosen by the sender and verified by nobody. A single Hello cannot both
/// claim X and authenticate as B — that is precisely what the #2520 binding checks forbid, and
/// such a Hello is refused before it reaches any of this. But nothing stops a peer that has
/// legitimately authenticated as B from putting X in the `from` field of its *next* message,
/// and an implementation that consulted that field would read it as the expectation having
/// been satisfied after all.
///
/// So: B authenticates, the divergence from X is recorded, and then the peer says "actually,
/// I'm X". The recorded divergence must stand, B must remain the only canonical peer, and X
/// must never become one. Expectation is answered by proof, once, and a claim is not proof —
/// not before authentication, and not after it either.
#[test]
fn a_later_message_claiming_the_expected_did_does_not_retire_the_mismatch() -> Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let dialer_did = dialer.did().clone();
        let peer_identity = IdentityBundle::generate()?;
        let authenticated_did = peer_identity.did().clone();
        // Configured by the operator; nobody on this connection holds its key.
        let expected_did = IdentityBundle::generate()?.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let peer = ScriptedPeer::spawn(
            &peer_identity,
            vec![
                // Legitimate: authenticates as B.
                hello_from(&peer_identity, &dialer_did, &peer_identity),
                // Then claims to be the DID the operator configured. Unverified, and on a
                // payload that carries no binding at all.
                NetworkMessage::ping(expected_did.clone(), dialer_did.clone()),
            ],
        )
        .await?;

        dialer_handle
            .dial_expecting(peer.addr, expected_did.clone())
            .await?;
        assert!(
            wait_until_authenticated(&dialer_handle, &authenticated_did, Duration::from_secs(20))
                .await,
            "precondition: the peer never authenticated, so no expectation was ever weighed"
        );
        // Let the forged claim land after the Hello has been processed.
        tokio::time::sleep(Duration::from_secs(2)).await;

        assert_eq!(
            dialer_handle.connected_peers().await,
            vec![authenticated_did.clone()],
            "claiming to be {expected_did} must not make the claimant that peer, nor add it \
             alongside the DID that actually authenticated"
        );
        assert!(
            dialer_handle
                .get_peer_connection_info(&expected_did)
                .await
                .is_none(),
            "the configured DID never authenticated on this connection, so it must hold no \
             authenticated peer state however often it is named"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        1,
        "the divergence was established by proof and cannot be talked away by a later \
         sender-chosen `from` (#2533, #2491)"
    );
    Ok(())
}

/// One connection, one mismatch — however many Hellos arrive on it.
///
/// A Hello is verified every time it arrives, so a peer that repeats one drives the
/// comparison repeatedly. The thing being counted is a *connection whose expectation was not
/// met*, which happens once per connection no matter how talkative the peer is; counting
/// Hellos instead would let the remote end set the rate of a metric about local
/// configuration.
///
/// Reconnection is the other repeat vector and is deliberately *not* deduplicated: a fresh
/// connection to a still-misconfigured entry is a fresh observation, and a bootstrap entry
/// that keeps reconnecting to the wrong node is exactly the condition worth seeing a rate on.
#[test]
fn repeated_hellos_on_one_connection_record_the_mismatch_once() -> Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let dialer_did = dialer.did().clone();
        let talkative = IdentityBundle::generate()?;
        let talkative_did = talkative.did().clone();
        let expected_did = IdentityBundle::generate()?.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let peer = ScriptedPeer::spawn(
            &talkative,
            vec![
                hello_from(&talkative, &dialer_did, &talkative),
                hello_from(&talkative, &dialer_did, &talkative),
                hello_from(&talkative, &dialer_did, &talkative),
            ],
        )
        .await?;

        dialer_handle
            .dial_expecting(peer.addr, expected_did.clone())
            .await?;
        assert!(
            wait_until_authenticated(&dialer_handle, &talkative_did, Duration::from_secs(20)).await,
            "precondition: the scripted peer never authenticated"
        );
        // Let the remaining Hellos land after the first has been processed.
        tokio::time::sleep(Duration::from_secs(2)).await;

        assert_eq!(
            dialer_handle.connected_peers().await,
            vec![talkative_did.clone()],
            "precondition: repeated Hellos must not change who is canonical"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        1,
        "three Hellos arrived on one connection whose expectation was unmet; the connection \
         diverged once. Recorded series: {:?}",
        mismatch_series(&snapshotter)
    );
    Ok(())
}
