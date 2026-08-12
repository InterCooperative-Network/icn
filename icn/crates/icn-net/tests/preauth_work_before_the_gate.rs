#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Decoding an unauthenticated peer's frame is work, and work must be authorized before it is
//! performed (#2558, work/decode axis).
//!
//! The allocation axis of #2558 is closed: `read_frame` no longer sizes anything from the declared
//! length (#2573). This is the other half, and that change did not touch it. The read loop was:
//!
//! ```text
//! accept_bi
//!   -> read_message           <- bounded frame read AND full decode happen together here
//!   -> per-connection budget (#2491)
//!   -> per-source budget     (#2549 / #2557)
//!   -> dispatch
//! ```
//!
//! `read_message` is `read_frame` followed by `NetworkMessage::from_bytes_negotiated`, and the
//! second half is the expensive one: a wire-format byte, then **zstd decompression bounded only by
//! `MAX_MESSAGE_SIZE`**, then a postcard deserialization of the whole envelope, then the version
//! check. All of it ran before either budget was consulted.
//!
//! # The property under test
//!
//! > A frame from an unauthenticated peer is decoded only if that peer's pre-authentication
//! > allowance authorized the decode. Frames the allowance refuses are never handed to the
//! > decoder.
//!
//! # Why this is not #2549, and not a rate measurement
//!
//! #2549 is about what a source re-acquires by *reconnecting*. Nothing here reconnects: one
//! connection is opened once and every frame rides a fresh bidirectional stream over it.
//! `preauth_source_work_budget.rs` scopes itself out of this in its own words: one held connection
//! "can drive it without reconnecting even once".
//!
//! Nor is the assertion a throughput threshold. Wall-clock rate is reported below as
//! characterization only. The assertion is an **ordering** fact, read off two counters this node
//! already emits in production, and it does not move with the speed of the machine.
//!
//! # How this knows whether the decoder ran
//!
//! Every frame sent here is *poison*: outer framing is valid, and the body is a well-formed
//! postcard `NetworkMessage` whose `version` is one past `MAX_SUPPORTED_VERSION`. Version
//! validation is the **last** step of `from_bytes_negotiated`, after decompression and after the
//! envelope has been fully deserialized — so a recorded version mismatch is proof that the whole
//! decode ran, not just that a header byte was glanced at.
//!
//! That gives two disjoint outcomes per frame, each with its own production counter:
//!
//! ```text
//! icn_network_protocol_version_mismatch_total     <- the decoder ran on this frame
//! icn_network_messages_rate_limited_pre_auth_total <- the budget refused this frame
//! ```
//!
//! Their sum must account for every frame the node consumed, and the first must never exceed what
//! the peer was allowed to spend. Before the fix the first counter reads the *whole burst* and the
//! second reads **zero** — the budget never saw a single frame, because the decoder had already
//! consumed them all.
//!
//! Consumption itself needs no counter. QUIC bounds concurrency: `session.rs` sets
//! `max_concurrent_bidi_streams(10)`, so a client may hold only ten unfinished streams at a time
//! and `open_bi` blocks once that credit is spent. Credit returns only when the node accepts a
//! stream and finishes with it. Opening hundreds of streams sequentially therefore *cannot*
//! complete unless the node read essentially all of them — the loop would stall on the eleventh
//! otherwise. That argument is what makes this test honest on a multi-threaded runtime.

use anyhow::{Context, Result};
use icn_identity::IdentityBundle;
use icn_net::{
    IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage, PREAUTH_SOURCE_BURST,
    PREAUTH_SOURCE_RENEWAL_WINDOW,
};
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use quinn::{ClientConfig, Endpoint};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Recorded once per decode that reached version validation — i.e. per frame fully deserialized.
const DECODED_METRIC: &str = "icn_network_protocol_version_mismatch_total";
/// Recorded once per frame an anonymous connection's budget refused.
const REFUSED_METRIC: &str = "icn_network_messages_rate_limited_pre_auth_total";
/// Recorded once per message that cleared every gate and reached dispatch.
const DISPATCHED_METRIC: &str = "icn_network_messages_received_total";

/// Streams one client may hold unfinished at once — `session.rs`'s `max_concurrent_bidi_streams`.
///
/// Duplicated deliberately and asserted against behaviour rather than imported: the point is that
/// opening far more than this many streams *sequentially* proves consumption, and that argument
/// holds for whatever the real limit is, so long as it is far below the frame count below.
const CONCURRENT_STREAM_CREDIT: usize = 10;

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
    // Must be the provider the workspace builds against: a different one makes every TLS
    // handshake fail, which presents as a connect timeout rather than as a TLS error.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Install a process-global recorder and hand back its snapshotter.
///
/// Global rather than thread-local, and that is the point. A `with_local_recorder` recorder is
/// visible only to the installing thread, which forces the whole test onto a current-thread
/// runtime — and this file's consumption argument, and its characterization figure, are only
/// honest on a multi-threaded one. This binary runs exactly one test, so a global recorder has
/// nobody to collide with and no other test's increments to read.
fn install_metrics() -> Snapshotter {
    let recorder = DebuggingRecorder::new();
    // Taken before the recorder is consumed: the snapshotter shares the registry, so it still
    // reads what the run recorded afterwards.
    let snapshotter = recorder.snapshotter();
    recorder
        .install()
        .expect("this binary installs the global recorder exactly once");
    snapshotter
}

/// Total value of `name` across every label set, or 0 if it was never recorded.
///
/// Summing across label sets is what keeps the aggregate readings below meaning the same thing
/// they meant when the refusal counter carried no labels at all (#2558 added `bound`).
fn counter(snapshotter: &Snapshotter, name: &str) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(
            |(key, _, _, value)| match (key.key().name() == name, value) {
                (true, DebugValue::Counter(v)) => Some(v),
                _ => None,
            },
        )
        .sum()
}

/// Total value of `name` for label sets carrying `label=value`, or 0 if there are none.
fn counter_labelled(snapshotter: &Snapshotter, name: &str, label: &str, value: &str) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, recorded)| {
            let key = key.key();
            let matches =
                key.name() == name && key.labels().any(|l| l.key() == label && l.value() == value);
            match (matches, recorded) {
                (true, DebugValue::Counter(v)) => Some(v),
                _ => None,
            }
        })
        .sum()
}

struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// Messages the node dispatched to its external handler.
#[derive(Clone, Default)]
struct Delivered(Arc<Mutex<usize>>);

impl Delivered {
    fn count(&self) -> usize {
        *self.0.lock().expect("delivery log poisoned")
    }
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` starts no inbound accept loop
/// without one, and a node that accepts nothing would stall the stream loop below rather than
/// measuring anything.
async fn spawn_node(
    bundle: IdentityBundle,
) -> Result<(NetworkHandle, SocketAddr, Guard, Delivered)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let delivered = Delivered::default();
    let sink = delivered.0.clone();
    let handler: IncomingMessageHandler = Arc::new(move |_msg: NetworkMessage| {
        *sink.lock().expect("delivery log poisoned") += 1;
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
                // The handle is returned, not dropped: dropping it tears the actor down, and the
                // connection below would then time out as if TLS had failed.
                return Ok((handle, addr, Guard(shutdown_tx), delivered));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

async fn connect(
    identity: &IdentityBundle,
    target: SocketAddr,
) -> Result<(Endpoint, quinn::Connection)> {
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
            Ok(Ok(connection)) => return Ok((endpoint, connection)),
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

/// The anonymous units one source may fund over `elapsed`.
///
/// `burst + rate * elapsed`, the honest reading of a token bucket — computed from measured time so
/// nothing here depends on how long a handshake happened to take on this machine. Deliberately
/// *not* "burst per window": a bucket hands out a full burst at the start and another once it has
/// refilled, so the sliding-window reading is wrong by up to a factor of two.
///
/// This is the *source* allowance. The per-connection burst (#2491) is asked first and is far
/// tighter, so the real ceiling on one connection is lower still; the looser of the two is used
/// because it is the one built from constants this crate exports, and it is already an order of
/// magnitude below the burst sent below.
fn source_allowance(elapsed: Duration) -> f64 {
    let rate = PREAUTH_SOURCE_BURST as f64 / PREAUTH_SOURCE_RENEWAL_WINDOW.as_secs_f64();
    PREAUTH_SOURCE_BURST as f64 + rate * elapsed.as_secs_f64()
}

/// A frame that is valid to the framer and fatal to the decoder.
///
/// Outer framing is exactly what any peer sends, so `read_frame` accepts it without complaint.
/// The body is a fully-formed postcard `NetworkMessage` carrying a version one past
/// `MAX_SUPPORTED_VERSION` — which `from_bytes_negotiated` rejects only at its *final* step, after
/// decompression and after the envelope has been deserialized in full. Nothing short of a complete
/// decode can produce that rejection.
fn poison(from: &IdentityBundle, to: &IdentityBundle) -> NetworkMessage {
    let mut message = NetworkMessage::subscribe(
        from.did().clone(),
        to.did().clone(),
        vec!["icn.test.2558.work".to_string()],
    );
    message.version = icn_net::protocol::MAX_SUPPORTED_VERSION + 1;
    message
}

/// The decoder must run only on frames the pre-authentication allowance paid for.
///
/// One connection, opened once, never reconnected.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn frames_are_authorized_before_they_are_decoded() -> Result<()> {
    init();
    let metrics = install_metrics();

    // Far above the concurrent-stream credit, so completing the loop is itself the proof that the
    // node consumed them, and far above any plausible allowance for the window.
    const FRAMES: usize = 400;
    const {
        assert!(
            FRAMES > CONCURRENT_STREAM_CREDIT * 10,
            "the frame count must dwarf the stream credit for the consumption argument to hold"
        )
    };

    let server = IdentityBundle::generate().expect("server identity");
    let (_handle, addr, _guard, delivered) = spawn_node(server.clone()).await?;

    let client = IdentityBundle::generate().expect("client identity");
    let (_endpoint, connection) = connect(&client, addr).await?;

    // Baseline after the handshake, so nothing the node emitted while starting up is counted.
    let decoded_before = counter(&metrics, DECODED_METRIC);
    let refused_before = counter(&metrics, REFUSED_METRIC);
    let dispatched_before = counter(&metrics, DISPATCHED_METRIC);

    let started = Instant::now();
    for i in 0..FRAMES {
        let message = poison(&client, &server);
        // `open_bi` is the consumption proof: it blocks once `CONCURRENT_STREAM_CREDIT` streams
        // are outstanding, and credit returns only as the node accepts and finishes them.
        let (mut send, _recv) = connection
            .open_bi()
            .await
            .with_context(|| format!("open_bi for frame {i} — the node stopped consuming"))?;
        icn_net::write_message(&mut send, &message).await?;
        send.finish()?;
    }
    let send_window = started.elapsed();

    // Let the node finish anything still in flight before reading the counters.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let elapsed = started.elapsed();

    assert!(
        connection.close_reason().is_none(),
        "the connection was closed during the run, so this measured a closure rather than the \
         budget refusing frames on a live connection"
    );

    let decoded = counter(&metrics, DECODED_METRIC) - decoded_before;
    let refused = counter(&metrics, REFUSED_METRIC) - refused_before;
    let dispatched = counter(&metrics, DISPATCHED_METRIC) - dispatched_before;
    let allowance = source_allowance(elapsed);

    // Characterization, reported and never asserted on: what this machine did, not a contract.
    println!(
        "#2558 characterization: {FRAMES} frames in {send_window:?} ({:.0} frame/s), \
         {decoded} decoded, {refused} refused before decode, {dispatched} dispatched, \
         source allowance over {elapsed:?} = {allowance:.1}",
        FRAMES as f64 / send_window.as_secs_f64(),
    );

    // Non-vacuity, in both directions.
    //
    // The poison frame must really be poisonous: if the decoder never rejected one, either it was
    // never reached at all (so the node consumed nothing and the run proves nothing) or the frame
    // is not the probe this test believes it is. Either way the assertion below would pass for a
    // reason that has nothing to do with ordering.
    assert!(
        decoded >= 1,
        "no frame ever reached the decoder, so this run cannot distinguish anything: the probe is \
         not poisonous, the metric is not wired, or the node consumed nothing"
    );
    // And every consumed frame must be accounted for by *exactly* one of the two outcomes. Both
    // directions are real failures: short of {FRAMES} means frames were consumed without recording
    // either, so neither figure below can be trusted; past {FRAMES} means the node did both to the
    // same frame — decoded it *and* refused it — which is #2558 in its purest form.
    assert_eq!(
        decoded + refused,
        FRAMES as u64,
        "the two outcomes do not add up to the {FRAMES} frames sent: {decoded} decoded plus \
         {refused} refused. Under-count means frames were consumed unaccounted; over-count means \
         the node decoded frames it had already refused"
    );

    // Every refusal names exactly one budget (#2558).
    //
    // Conservation rather than an expected split, because the split is a property of the machine
    // this ran on and the identity is a property of the code. It fails in both directions that
    // matter: short means a refusal was counted without a `bound` label at all, long means one
    // refused frame incremented more than one of them.
    //
    // This says nothing about *which* label is right — one connection sending far past its own
    // burst over a source budget it cannot drain produces connection refusals, but an
    // implementation reporting one label for everything would satisfy this too. That the two are
    // distinguishable at all is proven deterministically in `rate_limit.rs`, by
    // `a_dry_source_is_named_until_the_connection_itself_runs_out`; this only proves the gate's
    // counter carries the attribution end to end.
    let by_connection = counter_labelled(&metrics, REFUSED_METRIC, "bound", "connection");
    let by_source = counter_labelled(&metrics, REFUSED_METRIC, "bound", "source");
    assert_eq!(
        by_connection + by_source,
        refused,
        "{refused} frames were refused but {by_connection} + {by_source} carry a bound label: \
         a refusal reached the counter without naming a budget, or named two"
    );
    assert!(
        by_connection >= 1,
        "no refusal was attributed to this connection's own burst, though one connection sent \
         {FRAMES} frames against a burst of far fewer: the label is not being set from the gate"
    );

    assert!(
        (decoded as f64) <= allowance,
        "one unauthenticated connection, never reconnected, made this node fully deserialize \
         {decoded} frames in {elapsed:?} while its pre-authentication allowance over that window \
         was {allowance:.1}.\n\
         frames consumed by the node : {FRAMES} (proved by {FRAMES} sequential open_bi against a \
         credit of {CONCURRENT_STREAM_CREDIT})\n\
         fully decoded               : {decoded}\n\
         refused before decoding     : {refused}\n\
         dispatched                  : {dispatched} (handler saw {})\n\
         send window                 : {send_window:?} ({:.0} frame/s, characterization only)\n\
         decode per authorised unit  : {:.1}x\n\
         Each of those decodes ran a wire-format parse, zstd decompression bounded only by \
         MAX_MESSAGE_SIZE, and a full postcard deserialization — the version check that recorded \
         them is the last step of the decode, so none of them stopped early. The budget refused \
         {refused} of the {FRAMES}: it bounds what this node will *act on*, not what it will *do*.",
        delivered.count(),
        FRAMES as f64 / send_window.as_secs_f64(),
        decoded as f64 / allowance,
    );

    Ok(())
}
