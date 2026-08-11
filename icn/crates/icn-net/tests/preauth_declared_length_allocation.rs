#![allow(clippy::unwrap_used, clippy::expect_used)]
//! What an unauthenticated peer may make this node *allocate* by declaring a length (#2558).
//!
//! Every `read_message*` helper reads a four-byte big-endian length prefix and rejects it if it
//! exceeds `MAX_MESSAGE_SIZE`. Each then used to do this:
//!
//! ```text
//! let mut buf = vec![0u8; len];   // sized from the DECLARED length
//! recv.read_exact(&mut buf).await // …before a single body byte has arrived
//! ```
//!
//! So the buffer was committed on the strength of a number the peer chose, not on bytes the peer
//! actually sent. A peer that wrote the prefix and then stopped held that allocation until the
//! body arrived — which it never did — or until `PREAUTH_AUTHENTICATION_DEADLINE` (30 s) closed
//! the connection.
//!
//! All three helpers now share one `read_frame`, which keeps the same prefix validation and the
//! same `MAX_MESSAGE_SIZE` authority but grows the buffer a step at a time as bytes arrive. The
//! wire format is untouched.
//!
//! # The property under test
//!
//! > Before this node has authenticated a peer, the memory it commits on that peer's behalf is a
//! > function of the bytes the peer has actually sent, not of a length the peer merely claimed.
//!
//! # Why the existing bounds do not establish it
//!
//! - `MAX_MESSAGE_SIZE` (10 MiB) caps the *claim*. It does not make the claim cost anything.
//! - QUIC's `receive_window` (10 MiB/connection) and `stream_receive_window` (1 MiB) cap the bytes
//!   a peer may **send**. They cannot bound an allocation sized from a length the peer never
//!   fulfils — this is exactly the case where a transport byte limit does not imply a decoder
//!   allocation limit.
//! - `PREAUTH_AUTHENTICATION_DEADLINE` bounds how *long* the commitment is held, not how large it
//!   is.
//! - The per-connection (#2491) and per-source (#2549/#2557) budgets sit *after* the frame has
//!   been acquired — they gate the decode, not the read (#2558). A frame is read before either is
//!   consulted, so an allocation made during the read is made on nobody's authority.
//!
//! # Scale
//!
//! The connection loop reads **sequentially** — `accept_bi().await` then `read_frame().await` in
//! one task per connection, with no per-stream spawn — so one connection has at most one read in
//! flight. The multiplier is therefore connections, not streams:
//! `MAX_PREAUTH_CONNECTIONS_PER_SOURCE` (8) × `MAX_MESSAGE_SIZE` (10 MiB) = **80 MiB per source**,
//! bought with 8 × 4 = **32 bytes**, held for up to 30 s, entirely before authentication. That
//! figure was measured here before the fix; afterwards the same sequence commits 0.5 MiB, because
//! the reservation now tracks the read step rather than the declaration.
//!
//! # How this test knows
//!
//! A counting global allocator, measuring **bytes requested from the allocator**, not resident set
//! size. That distinction is deliberate: `vec![0u8; n]` may be served by a lazily-zeroed mapping
//! whose pages are not resident until touched, so RSS would under-report a commitment the program
//! genuinely made. The number this test asserts on is the size the program asked for.
//!
//! The node runs in this process, so the counter sees its allocations. Connection setup is
//! excluded by snapshotting *after* every connection and stream exists and before any prefix is
//! written, which leaves the declared-length allocation as the only thing of this magnitude that
//! can happen in the measured window.

use anyhow::{Context, Result};
use icn_identity::IdentityBundle;
use icn_net::{
    IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage, MAX_MESSAGE_SIZE,
    MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
};
use quinn::{ClientConfig, Endpoint};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Counting allocator
// ---------------------------------------------------------------------------

static IN_USE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// Tracks bytes requested from the allocator, and the high-water mark of that figure.
///
/// `alloc_zeroed` is overridden rather than left to the trait default because `vec![0u8; n]` takes
/// that path; counting only `alloc` would miss the exact allocation this test exists to observe.
struct Counting;

fn record_growth(bytes: usize) {
    let now = IN_USE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            record_growth(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc_zeroed(layout);
        if !ptr.is_null() {
            record_growth(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = System.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() {
            IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
            record_growth(new_size);
        }
        new_ptr
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Re-arms the high-water mark at the current in-use figure and returns that baseline.
fn arm_peak() -> usize {
    let now = IN_USE.load(Ordering::Relaxed);
    PEAK.store(now, Ordering::Relaxed);
    now
}

fn peak_growth_since(baseline: usize) -> usize {
    PEAK.load(Ordering::Relaxed).saturating_sub(baseline)
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

// ---------------------------------------------------------------------------
// Node harness (same shape as preauth_source_work_budget.rs)
// ---------------------------------------------------------------------------

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

struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// Topic counts of every `Subscribe` the node dispatched to its external handler.
///
/// A `Subscribe` reaches that handler untouched, so an arrival records that the node read,
/// reassembled and decoded the whole frame — which is the property the chunked reader must not
/// break.
#[derive(Clone, Default)]
struct Delivered(Arc<Mutex<Vec<usize>>>);

impl Delivered {
    fn topic_counts(&self) -> Vec<usize> {
        self.0.lock().expect("delivery log poisoned").clone()
    }
}

/// Spawn a real node on the production stack.
///
/// The handler is required, not optional: `NetworkActor::spawn` starts no inbound accept loop
/// without one, and a node that never accepts a stream never reaches `read_message` — which would
/// make the upper-bound assertion below pass for the wrong reason.
async fn spawn_node(
    bundle: IdentityBundle,
) -> Result<(NetworkHandle, SocketAddr, Guard, Delivered)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let delivered = Delivered::default();
    let sink = delivered.0.clone();
    let handler: IncomingMessageHandler = Arc::new(move |msg: NetworkMessage| {
        if let icn_net::MessagePayload::Subscribe { topics } = &msg.payload {
            sink.lock()
                .expect("delivery log poisoned")
                .push(topics.len());
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

// ---------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------

/// A declared length must not, by itself, commit memory.
///
/// Every connection writes only the four-byte prefix and then goes quiet. No body byte is ever
/// sent, so a node that sizes its buffer from bytes received allocates essentially nothing here,
/// while a node that sizes it from the declared length commits `MAX_MESSAGE_SIZE` per connection.
///
/// The ceiling asserted is one `MAX_MESSAGE_SIZE` for the *whole* fleet of connections. That is
/// far above anything an incremental reader would use and far below the
/// `MAX_PREAUTH_CONNECTIONS_PER_SOURCE × MAX_MESSAGE_SIZE` this path currently commits, so the
/// assertion does not depend on where between those two a fix happens to land.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn declared_length_alone_does_not_commit_memory() -> Result<()> {
    init();

    let server = IdentityBundle::generate().expect("server identity");
    let (_handle, addr, _guard, _delivered) = spawn_node(server).await?;

    // One source, the most connections admission will grant it (#2551).
    let fleet = MAX_PREAUTH_CONNECTIONS_PER_SOURCE;
    let mut held = Vec::with_capacity(fleet);
    for _ in 0..fleet {
        let client = IdentityBundle::generate().expect("client identity");
        let (endpoint, connection) = connect(&client, addr).await?;
        let (send, _recv) = connection
            .open_bi()
            .await
            .context("open_bi on the test connection")?;
        held.push((endpoint, connection, send));
    }

    // Let connection setup and stream acceptance settle, so the measured window contains the
    // declared-length allocation and nothing else of consequence.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let baseline = arm_peak();

    // The entire attack: a four-byte length prefix per connection, and then silence.
    let declared = MAX_MESSAGE_SIZE as u32;
    let mut attacker_bytes = 0usize;
    for (_endpoint, _connection, send) in held.iter_mut() {
        send.write_all(&declared.to_be_bytes())
            .await
            .context("write length prefix")?;
        attacker_bytes += 4;
        // Deliberately no body, and deliberately no finish(): the stream stays open so the node
        // remains parked in `read_exact` on a body that will never arrive.
    }

    // Give the node time to accept each stream, read the prefix, and reach the allocation.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let growth = peak_growth_since(baseline);
    let committed_if_declared_is_trusted = fleet * MAX_MESSAGE_SIZE;
    let ceiling = MAX_MESSAGE_SIZE;

    assert!(
        growth < ceiling,
        "a declared length committed memory before any body byte arrived.\n\
         attacker sent      : {attacker_bytes} bytes ({fleet} connections x 4-byte prefix)\n\
         node peak growth   : {:.1} MiB\n\
         ceiling asserted   : {:.1} MiB (one MAX_MESSAGE_SIZE for the whole fleet)\n\
         would-be commitment: {:.1} MiB (MAX_PREAUTH_CONNECTIONS_PER_SOURCE x MAX_MESSAGE_SIZE)\n\
         amplification      : {:.0}x",
        mib(growth),
        mib(ceiling),
        mib(committed_if_declared_is_trusted),
        growth as f64 / attacker_bytes as f64,
    );

    Ok(())
}

/// A frame larger than one read step still arrives intact.
///
/// This is the risk the fix introduces: the body is no longer read by a single `read_exact` over
/// the whole buffer, but in steps that are appended as they arrive. A reassembly that dropped,
/// duplicated or reordered a step would corrupt exactly the messages too large to fit in one —
/// and every other test in this crate sends messages far below that size, so nothing else would
/// notice. The payload here is chosen to span several steps.
///
/// It also pins the other half of the contract: bounding the *reservation* must not bound the
/// *message*. `MAX_MESSAGE_SIZE` remains the only authority on how large a message may be.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_frame_spanning_several_read_steps_arrives_intact() -> Result<()> {
    init();

    let server = IdentityBundle::generate().expect("server identity");
    let server_did = server.did().clone();
    let (_handle, addr, _guard, delivered) = spawn_node(server).await?;

    let client = IdentityBundle::generate().expect("client identity");
    let (_endpoint, connection) = connect(&client, addr).await?;

    // Sized in absolute terms rather than against the current step, so tuning the step upward
    // cannot quietly stop this exercising the multi-step path while still passing. It also lands
    // just past a power of two, which is where an unbounded growth strategy overshoots worst.
    const TOPICS: usize = 20_000;
    const SPANNING_FLOOR: usize = 1024 * 1024;
    let topics: Vec<String> = (0..TOPICS)
        .map(|i| format!("icn.test.2558.reassembly.{i:0>48}"))
        .collect();
    let message = NetworkMessage::subscribe(client.did().clone(), server_did, topics);
    let encoded = message.to_bytes_negotiated(false, false)?.len();
    assert!(
        encoded > SPANNING_FLOOR,
        "payload must be large enough to span several read steps for any plausible step size, \
         got {encoded} bytes"
    );
    assert!(
        encoded < MAX_MESSAGE_SIZE,
        "payload must remain a legal message, got {encoded} bytes"
    );

    let (mut send, _recv) = connection.open_bi().await.context("open_bi")?;
    icn_net::write_message(&mut send, &message).await?;
    send.finish()?;

    // Give the node time to read every step and dispatch.
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(
        delivered.topic_counts(),
        vec![TOPICS],
        "a {encoded}-byte frame spanning several read steps must arrive once, intact"
    );

    // NOTE: this test deliberately does *not* assert a capacity ceiling. Measured end to end,
    // that figure is dominated by whether the sender's copy is still live when the receiver
    // allocates — the same code measured 0.1 MiB locally and 4.7 MiB on CI, which overlaps the
    // 6.2 MiB the uncapped defect produces. An assertion that cannot separate those is not
    // evidence. The capacity bound is proven exactly instead, by
    // `protocol::tests::frame_capacity_never_exceeds_the_declared_length`.

    Ok(())
}
