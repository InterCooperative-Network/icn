//! QUIC connection handling for the network actor
//!
//! This module manages the lifecycle of incoming QUIC connections:
//!
//! - **Connection acceptance**: Listens for new connections via the session manager
//! - **Stream processing**: Handles bidirectional streams for message exchange
//! - **Rate limiting**: Two-phase. While the peer is anonymous an inbound message spends *both*
//!   a per-connection burst (#2491) and a shared per-source budget that survives reconnects
//!   (#2549); once it authenticates, per-DID and per-personhood-anchor limits apply for Sybil
//!   resistance
//! - **Byzantine detection**: Reports misbehavior to the misbehavior detector
//! - **Blob announcements**: Extracts and registers blob availability from incoming messages
//!
//! Each connection spawns a dedicated handler task that processes messages until
//! the peer disconnects or a shutdown signal is received.
//!
//! # Acceptance is two phases, not one
//!
//! Waiting for a new inbound connection and completing that connection's QUIC/TLS
//! handshake have opposite cancellation properties, and the accept loop keeps them
//! strictly apart (#2521):
//!
//! - **Waiting** ([`quinn::Endpoint::accept`]) is cancel-safe — a pending `Incoming` stays
//!   queued on the endpoint — so it is raced against shutdown and may be abandoned freely.
//! - **Handshaking** (`incoming.await`) is *not*. Once an `Incoming` is owned it must be
//!   driven to completion; dropping it mid-handshake makes quinn implicitly close the
//!   connection, destroying a legitimate peer's connection attempt with no error and no
//!   application-level trace.
//!
//! So a shutdown deadline may cancel the wait for new work, but must never become a
//! maximum lifetime for a handshake that has already arrived.
//!
//! # Rate limiting is two phases, not one
//!
//! Both limit checks run before any dispatch — which means before anything has verified who sent
//! the message. So neither check can ask the message who to charge (#2491):
//!
//! ```text
//! before authentication:
//!     charge the transport SOURCE's anonymous budget, shared with every other
//!     connection from that source.
//!     No DID input: no trust tier, no personhood anchor.
//!
//! after an authenticated Hello:
//!     charge the DID cryptographically bound to THIS connection's certificate (#2520),
//!     which selects the operator-configured tier (#2490).
//! ```
//!
//! `NetworkMessage.from` selects rate-limit authority in neither phase. It remains a claim
//! about origin, useful for diagnostics and nothing else here. Content authorship and relay
//! semantics are a separate question, tracked by #2480; a transport limiter only needs to
//! know whose traffic this connection is carrying.
//!
//! The anonymous budget is keyed by source rather than by connection because the connection
//! was a key the peer could discard (#2549). One connection, one budget answered "which claim
//! selects this limit" correctly, but it also meant reconnecting was issued a new one, so a
//! source's aggregate was bounded by nothing. The source key outlives the connections that
//! spend from it, and authentication does not refund what was already spent — self-minting a
//! DID and a binding is cheap, so any exemption it could buy would be no bound at all.
//!
//! Scope: this bounds the anonymous *work* one source can fund — the decode as well as the
//! dispatch it feeds. It does not bound that source's QUIC/TLS handshake rate, which is complete
//! before the source key exists (#2559). Bounding how many connections one source holds *at once*
//! is connection admission, below.
//!
//! # The gate sits between framing and decoding, not after both
//!
//! It used to sit after both, and that was the defect #2558 filed. `read_message` is two steps
//! welded together — acquire the frame, then interpret it — and only the first is work the peer
//! has already paid for in bytes it actually sent. Interpreting a frame means a wire-format parse,
//! zstd decompression bounded only by `MAX_MESSAGE_SIZE`, and a full postcard deserialization of
//! the envelope. All of that ran before either budget was consulted, so a refusal cost the sender
//! nothing and the budget bounded what this node would *act on* rather than what it would *do*:
//!
//! ```text
//! was:  accept_bi -> read frame -> DECODE -> budget -> dispatch
//! now:  accept_bi -> read frame -> budget -> DECODE -> dispatch
//! ```
//!
//! The new position is the earliest point where both halves of the charge are true: enough of the
//! peer's input has arrived to justify charging a unit, and the work the budget meters has not
//! happened yet. Charging on stream accept instead would bill an idle or stalled connection for
//! work nobody has asked for, which would turn a rate limit into an availability primitive.
//!
//! Only the anonymous phase moved. The post-authentication per-DID and per-anchor limits stay
//! below the decode, because that traffic is attributable to an identity this connection has
//! proven and because those log lines quote `message.from`. A malformed body is now charged where
//! it used to be free — deliberately, since unparseable input is exactly what an attacker gets to
//! manufacture cheaply. It is the same single token the message would have spent one step later,
//! so nothing is charged twice.
//!
//! What this does **not** change is how many frames the node *reads*. A refused peer still gets
//! its frame read; that is bounded by the authentication deadline and by admission, not by this
//! budget. Reading bytes is not the resource #2558 is about.
//!
//! # The anonymous phase is bounded in size and in time
//!
//! Admission (#2547) reserves a slot per established-but-unauthenticated inbound connection and
//! releases it at the authenticated Hello. That bounds how many anonymous connections exist at
//! an instant. It does not, on its own, make any of them ever *stop* being anonymous — and a
//! silent connection is durable here, because the handshake permit is already released, this
//! node's own keepalives hold the transport open, and nothing maps or evicts an unauthenticated
//! peer. So the reservation had a ceiling but no expiry (#2552).
//!
//! The deadline that fixes it is enforced in [`NetworkActor::handle_connection`]. It bounds
//! everything the peer can make this task wait on, and stops at the point where the node starts
//! deciding things:
//!
//! ```text
//! anonymous:      check expiry -> select { accept_bi , deadline } -> read within deadline -> dispatch
//! authenticated:  accept_bi -> read -> dispatch
//! ```
//!
//! The boundary between the last two columns is the whole design. Cancelling a wait is free and
//! cancelling an unread message costs nothing anybody has looked at, so both are bounded;
//! cancelling *dispatch* could abandon a peer mid-verification, or after
//! `record_authenticated_peer` had already released its slot, so dispatch is not. `accept_bi` is
//! cancel-safe, so a losing arm cannot swallow a Hello.
//!
//! Bounding the read is not belt-and-braces. `read_frame` has no timeout of its own, and
//! `read_exact` on a length prefix whose remaining bytes never arrive never returns — so a peer
//! that sends three bytes of a four-byte header parks this task inside the loop body, past every
//! deadline, holding the slot exactly as if it had sent nothing at all. A deadline that guards
//! only the wait is defeated by anything that keeps the task out of the wait. The deadline wraps
//! exactly that peer-controlled await; the decode below it is CPU-bound and has no await to
//! cancel at, so the split changed nothing about what the deadline covers.
//!
//! The expiry is also *checked* at the loop boundary rather than left to the `select!`, because
//! `biased` returns on the first ready arm without polling the rest and a tokio `Sleep` completes
//! only when polled. Empirically a maximal stream flood does not starve it — stream credit costs
//! a round trip to return, so the accept queue drains to empty constantly — but that is
//! `max_concurrent_bidi_streams` underwriting the property, not this loop.
//!
//! Because the loop is a single sequential task, "has this connection authenticated" is answered
//! by program order rather than by synchronisation: the Hello is verified inside the loop body,
//! the deadline is consulted at the top of the next iteration, and the two never run
//! concurrently. There is no timer task to outlive the phase it belongs to.

use anyhow::Result;
use icn_identity::{Did, IdentityBundle};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, info, instrument, warn};

use crate::{
    handlers::ConnectionContext,
    protocol::{read_frame, MessagePayload, NetworkMessage},
    replay_guard::ReplayGuard,
    topology::{NeighborSets, TopologyConfig},
    IncomingMessageHandler, SessionManager,
};

use super::PeerConnectionInfo;

use super::NetworkActor;

/// Maximum number of inbound QUIC/TLS handshakes driven concurrently.
///
/// Each handshake runs in its own task so that one slow or unresponsive peer cannot stall
/// acceptance for everyone else. The slot is reserved *before* a new `Incoming` is taken,
/// so a burst backs up inside quinn's own accept queue — where it is subject to the
/// endpoint's limits and can be refused cleanly — rather than accumulating unbounded tasks
/// here.
const MAX_CONCURRENT_INBOUND_HANDSHAKES: usize = 64;

/// QUIC application close code for a connection refused a pre-authentication slot (#2547).
///
/// Distinct from a silent drop so the peer learns it was refused rather than timing out, and
/// distinct from a protocol error because it is not one: a refused connection may be a
/// perfectly well-behaved peer that arrived while its source's allowance was spent, and
/// retrying later is the correct response.
const PREAUTH_ADMISSION_REFUSED_CODE: u32 = 0x1c4b;

use crate::preauth_admission::PREAUTH_AUTHENTICATION_TIMEOUT_CODE;

/// End a connection that held a pre-authentication slot without ever authenticating (#2552).
///
/// Closes the transport and leaves the slot to the guard's destructor, which the caller reaches
/// by breaking out of the handler loop. Releasing the reservation while letting the anonymous
/// connection live on would satisfy the admission table and let the thing it bounds escape the
/// bound, so the transport is always what ends first.
///
/// Shared by the three places the deadline can be observed — expired before a wait, expired
/// during one, expired mid-read — because they are one event and an operator watching
/// `icn_network_preauth_authentication_timeout_total` should not have to know which of them
/// noticed.
fn close_for_missed_authentication(connection: &quinn::Connection) {
    warn!(
        remote_addr = %connection.remote_address(),
        "Closing inbound connection: it held a pre-authentication slot without ever authenticating"
    );
    icn_obs::metrics::network::preauth_authentication_timeout_inc();
    connection.close(
        PREAUTH_AUTHENTICATION_TIMEOUT_CODE.into(),
        b"pre-authentication deadline",
    );
}

/// Backoff before re-checking for an endpoint that has not been started yet.
const ENDPOINT_RETRY_INTERVAL: Duration = Duration::from_millis(100);

impl NetworkActor {
    /// Handle incoming QUIC connections
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_incoming_connections(
        session_manager: Arc<RwLock<SessionManager>>,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<crate::rate_limit::RateLimiter>,
        replay_guard: Arc<RwLock<ReplayGuard>>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        peer_connections: Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>,
        capability_registry: Arc<crate::capability_evidence::LiveCapabilityRegistry>,
        blob_registry: Option<Arc<RwLock<crate::BlobLocationRegistry>>>,
        misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
        identity_bundle: IdentityBundle,
        own_did: Did,
        peer_exchange_enabled: Arc<std::sync::atomic::AtomicBool>,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        info!("Starting incoming connection handler");

        let handshake_slots = Arc::new(Semaphore::new(MAX_CONCURRENT_INBOUND_HANDSHAKES));

        // One admission table for this endpoint's whole lifetime. It is the only state here
        // that aggregates across connections, which is exactly the point: every other bound
        // on this path is per-connection and so cannot see a source opening many (#2547).
        let admission_ctl = Arc::new(crate::preauth_admission::PreAuthAdmission::new());

        // The other state that aggregates across connections, and the reason it has to (#2549).
        // The admission table above bounds how many anonymous connections one source holds *at
        // once*; it cannot bound what a source spends over time, because its entries exist only
        // while a connection is live and so are re-minted on every reconnect. This one deliberately
        // outlives the connections that draw on it, which is the whole difference.
        let pre_auth_budget = Arc::new(crate::rate_limit::SourcePreAuthBudget::new());

        loop {
            // Reserve a handshake slot before taking on new work. Acquiring is cancel-safe,
            // so losing this race to shutdown costs nothing.
            let permit = tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    info!("Incoming connection handler received shutdown signal");
                    break;
                }
                permit = handshake_slots.clone().acquire_owned() => match permit {
                    Ok(permit) => permit,
                    Err(_closed) => break,
                },
            };

            // Take a cheap clone of the endpoint and release the session manager lock
            // right away. `SessionManager::stop` needs the write side of this lock, so a
            // read guard held across the wait below would deadlock shutdown — the reason
            // this loop previously polled on a short timeout at all.
            let endpoint = {
                let manager = session_manager.read().await;
                manager.endpoint_handle().await
            };
            let endpoint = match endpoint {
                Some(endpoint) => endpoint,
                None => {
                    // Not started yet, or already stopped: back off instead of spinning.
                    tokio::select! {
                        biased;
                        _ = shutdown_rx.recv() => break,
                        _ = tokio::time::sleep(ENDPOINT_RETRY_INTERVAL) => {}
                    }
                    continue;
                }
            };

            // Phase 1: wait for a *new* inbound connection. This is the only part of
            // acceptance that may be cancelled. `Endpoint::accept()` is cancel-safe, so if
            // shutdown wins this race any already-queued `Incoming` stays on the endpoint
            // and is refused cleanly when the endpoint closes.
            let incoming = tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    info!("Incoming connection handler received shutdown signal");
                    break;
                }
                incoming = endpoint.accept() => match incoming {
                    Some(incoming) => incoming,
                    None => {
                        info!("QUIC endpoint closed; stopping incoming connection handler");
                        break;
                    }
                },
            };

            // Phase 2: the `Incoming` is now owned, so its handshake runs to completion in
            // its own task, outside every cancellation scope in this loop. #2521: this step
            // used to sit inside the loop's shutdown-polling timeout, which silently
            // destroyed legitimate connections whose handshake outlived the poll interval.
            let handler_clone = handler.clone();
            let rate_limiter_clone = rate_limiter.clone();
            let replay_guard_clone = replay_guard.clone();
            let neighbor_sets_clone = neighbor_sets.clone();
            let topology_config_clone = topology_config.clone();
            let session_mgr_clone = session_manager.clone();
            let peer_connections_clone = peer_connections.clone();
            let capability_registry_clone = capability_registry.clone();
            let blob_registry_clone = blob_registry.clone();
            let misbehavior_detector_clone = misbehavior_detector.clone();
            let identity_bundle_clone = identity_bundle.clone();
            let own_did_clone = own_did.clone();
            let peer_exchange_enabled = peer_exchange_enabled.clone();
            let admission_ctl = admission_ctl.clone();
            let pre_auth_budget = pre_auth_budget.clone();
            tokio::spawn(async move {
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    Err(e) => {
                        warn!("Inbound QUIC handshake failed: {}", e);
                        return;
                    }
                };
                // The slot bounds concurrent handshakes, not connection lifetimes: release
                // it as soon as the handshake is done, before the long-lived stream loop.
                drop(permit);

                // Take this connection's share of its source's pre-authentication allowance
                // (#2547). This is the first moment it can be taken and the last moment it is
                // useful: only now is the remote address meaningful, because completing a QUIC
                // handshake proves the peer receives traffic there (RFC 9000 §8), and a
                // spoofed address never gets this far. Any earlier and the key would be a
                // claim; any later and the connection would already own a handler task, a
                // context and a budget — the things being bounded.
                let admission = match admission_ctl.try_admit(connection.remote_address()) {
                    Ok(guard) => guard,
                    Err(refusal) => {
                        // Deliberately logged, not counted by address: the label set stays
                        // closed while the diagnostic detail stays available.
                        warn!(
                            remote_addr = %connection.remote_address(),
                            bound = refusal.as_str(),
                            "Refusing inbound connection: pre-authentication admission limit \
                             reached"
                        );
                        icn_obs::metrics::network::preauth_admission_refused_inc(refusal.as_str());
                        connection.close(
                            PREAUTH_ADMISSION_REFUSED_CODE.into(),
                            b"pre-authentication admission limit",
                        );
                        return;
                    }
                };

                info!("Accepted connection from {}", connection.remote_address());

                if let Err(e) = Self::handle_connection(
                    connection,
                    handler_clone,
                    rate_limiter_clone,
                    replay_guard_clone,
                    neighbor_sets_clone,
                    topology_config_clone,
                    session_mgr_clone,
                    peer_connections_clone,
                    capability_registry_clone,
                    blob_registry_clone,
                    misbehavior_detector_clone,
                    identity_bundle_clone,
                    own_did_clone,
                    crate::handlers::ConnectionDirection::Inbound,
                    peer_exchange_enabled,
                    // We did not choose this peer, so there is nobody we expected (#2533).
                    None,
                    Some(admission),
                    Some(pre_auth_budget),
                )
                .await
                {
                    warn!("Connection handler error: {}", e);
                }

                // The guard travelled into the context, so whatever happened above —
                // authenticated (released early), errored, or ran to completion — the slot
                // is given back by its destructor, which republishes the gauges itself. No
                // refresh here: doing it at the call site is what left them stale for every
                // connection that authenticated and stayed open.
            });
        }

        // In-flight handshake tasks are intentionally left detached: `SessionManager::stop`
        // closes the endpoint, which fails them promptly, and the accept loop must not block
        // shutdown waiting on a peer that may never finish its handshake.
        info!("Incoming connection handler stopped");
        Ok(())
    }

    /// Handle a single QUIC connection (process all incoming streams)
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(remote_addr = %connection.remote_address()))]
    pub(super) async fn handle_connection(
        connection: quinn::Connection,
        handler: IncomingMessageHandler,
        rate_limiter: Arc<crate::rate_limit::RateLimiter>,
        replay_guard: Arc<RwLock<ReplayGuard>>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        session_manager: Arc<RwLock<SessionManager>>,
        peer_connections: Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>,
        capability_registry: Arc<crate::capability_evidence::LiveCapabilityRegistry>,
        blob_registry: Option<Arc<RwLock<crate::BlobLocationRegistry>>>,
        misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
        identity_bundle: IdentityBundle,
        own_did: Did,
        direction: crate::handlers::ConnectionDirection,
        peer_exchange_enabled: Arc<std::sync::atomic::AtomicBool>,
        expected_peer: Option<Did>,
        // This connection's pre-authentication slot, for inbound connections (#2547).
        // `None` for outbound: we chose to dial that peer, so it is not consuming an
        // allowance meant to bound connections chosen by somebody else.
        admission: Option<crate::preauth_admission::AdmissionGuard>,
        // The endpoint's per-source anonymous allowance, for inbound connections (#2549).
        // `None` for outbound, for the same reason `admission` is: nobody can make this node
        // dial by reconnecting, so there is no churn to aggregate against.
        pre_auth_budget: Option<Arc<crate::rate_limit::SourcePreAuthBudget>>,
    ) -> Result<()> {
        info!("Handling connection from {}", connection.remote_address());

        // When this connection must have authenticated by, while it still holds a
        // pre-authentication slot (#2552).
        //
        // `None` for a connection that took no slot — an outbound dial, which this node chose.
        // The deadline exists to bound a reservation somebody else's connection is spending, so
        // a connection that spends none is not its subject.
        //
        // Read off the guard rather than computed here, so the clock starts when the *slot* was
        // taken. Set below to `None` the moment this connection authenticates: the slot is gone
        // by then, and an expiry that outlived the resource it bounds is exactly the state that
        // would let this close a peer that had already identified itself.
        let mut authenticate_by = admission.as_ref().map(|guard| guard.authenticate_by());

        // Create connection context for handlers (clone shared state)
        let ctx = ConnectionContext::new(
            handler.clone(),
            rate_limiter.clone(),
            replay_guard.clone(),
            neighbor_sets.clone(),
            topology_config.clone(),
            session_manager.clone(),
            peer_connections.clone(),
            capability_registry.clone(),
            blob_registry.clone(),
            misbehavior_detector.clone(),
            identity_bundle.clone(),
            own_did.clone(),
            direction,
            peer_exchange_enabled,
            expected_peer,
            admission,
            // Keyed the same way the admission slot is, so a source cannot be two sources to
            // the two bounds — the port it dialled from is not part of either (#2547).
            pre_auth_budget.map(|budget| {
                (
                    budget,
                    crate::preauth_admission::SourceKey::from_addr(connection.remote_address()),
                )
            }),
        );

        loop {
            // A connection stops being anonymous exactly once and never goes back, so this only
            // ever clears the deadline — and it is read here, in the same sequential task that
            // writes it, so "has it authenticated yet" needs no synchronisation beyond program
            // order. `handle_hello` runs inside this loop's body; this runs at the top of the
            // next iteration, strictly after it.
            if authenticate_by.is_some() && ctx.authenticated_peer().await.is_some() {
                authenticate_by = None;
            }

            // The expiry is *checked*, not raced.
            //
            // Deciding it inside the `select!` below alone would make enforcement depend on the
            // scheduler choosing to poll a timer, and `biased` deliberately does not: it polls
            // its arms in order and returns on the first that is ready, without touching the
            // rest. A tokio `Sleep` is not an interrupt — it completes only when polled — so a
            // peer that keeps `accept_bi` ready on every iteration keeps the timer arm from ever
            // being polled, and an absolute instant that nothing observes has no effect however
            // far into the past it goes. What currently prevents that is `max_concurrent_bidi_
            // streams`, since returning stream credit costs a round trip and so drains the
            // accept queue to empty constantly; that is a transport tuning constant doing a
            // security property's job. Asking `Instant::now()` here costs one comparison and
            // owes nothing to either the scheduler or the stream limit.
            if let Some(deadline) = authenticate_by {
                if Instant::now() >= deadline {
                    close_for_missed_authentication(&connection);
                    break;
                }
            }

            // Accept incoming bidirectional stream.
            //
            // While the connection is still anonymous, the deadline races the *wait* — never
            // the verification. That is the same rule the accept loop above follows for
            // shutdown, and it is what makes the expiry safe:
            //
            // - `accept_bi` is cancel-safe. quinn removes a stream from the connection's queue
            //   only on the poll that returns it (`poll_accept`), so a losing arm here cannot
            //   swallow a peer's Hello — it stays queued for the next call.
            // - the deadline can only fire while nothing is being verified. If a Hello has
            //   arrived, `accept_bi` has already returned and this future is dropped; if the
            //   deadline fires, no Hello has been read, so none can have authenticated. There
            //   is no interleaving in which a verified Hello is discarded by a timeout.
            // - `biased` gives a simultaneous readiness to the peer. A stream that arrives in
            //   the same instant the timer expires is served, and the check above ends the
            //   connection on the next iteration. So the tie buys exactly one more message,
            //   which the anonymous budget already bounds, and never a renewed lease.
            //
            // Absolute, not per-iteration: a `timeout` around each wait would restart on every
            // message, and "send something every T-1 seconds" would hold the slot forever
            // again. The property is *authenticate* within T, not *speak* within T.
            let accepted = match authenticate_by {
                None => connection.accept_bi().await,
                Some(deadline) => tokio::select! {
                    biased;
                    accepted = connection.accept_bi() => accepted,
                    _ = tokio::time::sleep_until(deadline.into()) => {
                        close_for_missed_authentication(&connection);
                        break;
                    }
                },
            };

            match accepted {
                Ok((mut send, mut recv)) => {
                    // Acquire the frame — bytes only, nothing interpreted yet.
                    //
                    // This is the first half of what `read_message` used to do in one step. The
                    // split is the subject of #2558: `read_frame` is bounded work the peer has
                    // already paid for in bytes it actually sent (#2573), while turning those
                    // bytes into a `NetworkMessage` is unbounded-by-comparison work this node
                    // performs on the peer's behalf — a wire-format parse, zstd decompression
                    // bounded only by `MAX_MESSAGE_SIZE`, and a full postcard deserialization.
                    // The budget that decides whether an anonymous peer may spend that work now
                    // sits between the two, below.
                    //
                    // Bounded by the same absolute instant while the connection is anonymous.
                    // Frame acquisition is an unbounded await the *peer* controls: `read_exact`
                    // on a length prefix whose remaining bytes never arrive never returns, so
                    // three bytes of a four-byte header parks this task inside the loop body
                    // forever — past the wait, where no deadline could reach it. That is the
                    // same squat #2552 is about, reached for the price of one stream. The
                    // deadline wraps exactly the peer-controlled await and nothing else; decode
                    // is CPU-bound and has no await to cancel at.
                    //
                    // Bounding the read and not the dispatch is the whole distinction. Reading
                    // bytes is not verifying them: a cancelled read discards a message nobody
                    // has looked at yet, on a connection that is being closed regardless.
                    // Cancelling `handle_hello` could instead abandon a peer *mid-verification*
                    // — or, worse, after `record_authenticated_peer` had already released the
                    // slot — so dispatch stays outside the deadline exactly as before.
                    let frame = match authenticate_by {
                        None => read_frame(&mut recv).await,
                        Some(deadline) => {
                            match tokio::time::timeout_at(deadline.into(), read_frame(&mut recv))
                                .await
                            {
                                Ok(frame) => frame,
                                Err(_elapsed) => {
                                    close_for_missed_authentication(&connection);
                                    break;
                                }
                            }
                        }
                    };

                    // Track bandwidth contribution (aggregate, no per-DID tracking).
                    //
                    // Counted against the frame, before the gate, rather than against a successful
                    // decode. Refused traffic was already counted here before this change — a
                    // refused message had necessarily been decoded by the time it reached the gate
                    // — and moving the gate earlier without moving this would blind the counter in
                    // exactly the case #2558 is about. Total bytes: 4 (length prefix) + frame
                    // body, as `read_message` reported. A frame that never completed is not
                    // counted, as before.
                    if let Ok(frame) = &frame {
                        icn_obs::metrics::contribution::total_bandwidth_bytes_add(
                            (4 + frame.len()) as u64,
                        );
                    }

                    // Which identity this connection has proven, read once. Nothing between here
                    // and the dispatch below can change it — decode is pure — so the pre- and
                    // post-authentication gates are answering the same question about the same
                    // instant, as they did when both sat after the decode.
                    let authenticated = ctx.authenticated_peer().await;

                    // PRE-AUTHENTICATION WORK AUTHORIZATION (#2558).
                    //
                    // The earliest point at which both halves of the charge are true: enough of
                    // the peer's input has arrived to justify charging one unit, and the work the
                    // budget meters has not happened yet. Charging on stream accept instead would
                    // bill an idle or stalled connection for work nobody has asked for, turning a
                    // rate limit into an availability primitive; charging after the decode is
                    // what this fixes.
                    //
                    // The invariant is one-directional, and only that direction is claimed: a
                    // frame is decoded **only if** this gate permitted it. The converse does not
                    // hold and does not need to. `PreAuthBudget::check` is
                    // `connection.check() && budget.spend()`, so an exhausted connection bucket
                    // short-circuits and costs nothing at all, while a connection bucket that
                    // permits over a dry source budget spends its token on a frame that is never
                    // decoded. That asymmetry predates this change and is documented as
                    // deliberate on `PreAuthBudget::check`; it can only make the bound tighter
                    // than stated, never looser.
                    //
                    // What did change is that a malformed body is now charged for the decode
                    // *attempt* where it used to be free — the decode error short-circuited past
                    // this gate entirely, so unparseable input, exactly what an attacker
                    // manufactures cheaply, was never metered. It is the same single token the
                    // message would have spent one step later, so nothing is charged twice.
                    //
                    // Only the anonymous phase moves. An authenticated peer's DID- and
                    // anchor-keyed limits stay where they were, below the decode: that traffic is
                    // attributable to a proven identity, and #2558 is about work performed for a
                    // peer that has not proven one.
                    //
                    // The guard order is load-bearing, not stylistic. `check_pre_auth_rate_limit`
                    // *consumes* a token, so it must stay innermost: the two conditions ahead of
                    // it are what keep an incomplete frame and an authenticated peer from being
                    // charged. Reordering this spends tokens on both.
                    if frame.is_ok() && authenticated.is_none() {
                        if let Err(refusal) = ctx.check_pre_auth_rate_limit().await {
                            // No `claimed_from` here, unlike the authenticated arm below: the
                            // frame has deliberately not been decoded, so there is no claim to
                            // log — and the claim was never authority for anything in this phase
                            // anyway.
                            //
                            // `refusal` names which of the two budgets refused (#2558): this
                            // connection's own burst (#2491), or the budget shared by everything
                            // from its source (#2549). Those want different responses — one
                            // connection misbehaving against one peer's worth of allowance, or
                            // an address whose whole allowance is gone — and the counter could
                            // not tell them apart, which is the operability gap #2558 names.
                            // It is a closed two-value set, so it is safe to carry as a label.
                            warn!(
                                bound = refusal.as_str(),
                                "Refused pre-authentication work (the named pre-authentication \
                                 budget is exhausted)"
                            );

                            icn_obs::metrics::network::messages_rate_limited_inc();
                            icn_obs::metrics::network::messages_rate_limited_pre_auth_inc(
                                refusal.as_str(),
                            );

                            // Close stream before continuing to avoid resource leak
                            if let Err(e) = send.finish() {
                                tracing::debug!("Stream finish error during rate limit: {}", e);
                            }

                            // Drop the frame — undecoded (don't call handler)
                            continue;
                        }
                    }

                    // Second half: interpret the authorized frame.
                    let read =
                        frame.and_then(|frame| NetworkMessage::from_bytes_negotiated(&frame));

                    match read {
                        Ok(message) => {
                            // Check the authenticated peer's rate limit BEFORE processing.
                            //
                            // Which identity may select this message's limit is decided by
                            // what *this connection* has proven, never by what the message
                            // claims (#2491):
                            //
                            //   PRE-AUTH   the transport source is known, the peer's DID is
                            //              not. Trust cannot select a tier, and there is no
                            //              DID to derive a personhood anchor from either. That
                            //              phase is gated above, before the decode (#2558) —
                            //              nothing about it needed the message, which is why it
                            //              could move.
                            //   POST-AUTH  this connection has a DID bound to the
                            //              certificate it is actually using (#2520). That
                            //              DID selects the tier. It stays here: the work is
                            //              attributable to a proven identity, and `message.from`
                            //              is available for the diagnostics below.
                            //
                            // `message.from` describes a claimed origin and is kept for
                            // diagnostics below. It never selects transport rate-limit
                            // authority in either phase — that is the whole defect: DIDs are
                            // public, so keying on `from` let any sender name a well-trusted
                            // peer and be charged as one.
                            //
                            // `authenticated` is read once per message above rather than cached
                            // across messages, because the connection's identity is genuinely
                            // mutable. `handlers::hello` records an authenticated peer on *every*
                            // valid Hello — the `hello_responded` guard bounds the reply (#2537),
                            // not the binding — and #2520's checks tie a DID to this connection's
                            // certificate without tying that certificate to the DID's key. A
                            // second Hello can therefore move this connection from A to B, and the
                            // limit must follow the identity in force *now*. Caching the first one
                            // would keep charging a peer that is no longer the one on this
                            // session.
                            //
                            // With no DID, nothing DID-keyed runs: not the trust tier and not the
                            // anchor lookup. Inventing an anchor from the claim would let a sender
                            // spend somebody else's per-person budget. That also means
                            // `EnforcementMode::RequirePersonhood` does not cover
                            // pre-authentication traffic. It never usefully did: the anchor was
                            // looked up for `message.from`, so naming any anchored DID satisfied
                            // it, while the peers it actually turned away were honest un-anchored
                            // ones — whose Hello it dropped before they could ever authenticate.
                            if let Some(peer) = authenticated.as_ref() {
                                let (did_allowed, anchor_allowed) =
                                    rate_limiter.check_rate_limit_with_personhood(peer).await;

                                if !did_allowed {
                                    warn!(
                                        claimed_from = %message.from,
                                        authenticated = %peer,
                                        "Rate limited message (per-DID limit exceeded)"
                                    );

                                    // Track rate limiting metric
                                    icn_obs::metrics::network::messages_rate_limited_inc();

                                    // Close stream before continuing to avoid resource leak
                                    if let Err(e) = send.finish() {
                                        tracing::debug!(
                                            "Stream finish error during rate limit: {}",
                                            e
                                        );
                                    }

                                    // Drop the message (don't call handler)
                                    continue;
                                }

                                if !anchor_allowed {
                                    warn!(
                                        claimed_from = %message.from,
                                        authenticated = %peer,
                                        "Rate limited message (per-person limit exceeded - Sybil mitigation)"
                                    );

                                    // Track Sybil-specific rate limiting metric
                                    icn_obs::metrics::network::messages_rate_limited_by_anchor_inc(
                                    );

                                    // Close stream before continuing to avoid resource leak
                                    if let Err(e) = send.finish() {
                                        tracing::debug!(
                                            "Stream finish error during rate limit: {}",
                                            e
                                        );
                                    }

                                    // Drop the message (don't call handler)
                                    continue;
                                }
                            }

                            info!(
                                peer_did = %message.from,
                                protocol = ?message.payload.variant_name(),
                                "Received network message"
                            );

                            // Track metrics
                            icn_obs::metrics::network::messages_received_inc();

                            // Dispatch to handlers
                            match &message.payload {
                                MessagePayload::Hello {
                                    binding_info,
                                    version_info,
                                    topology_info,
                                    x25519_public,
                                    ml_dsa_public,
                                    ml_kem_public,
                                    pq_binding_proof,
                                } => {
                                    ctx.handle_hello(
                                        &connection,
                                        &message.from,
                                        binding_info,
                                        version_info,
                                        topology_info,
                                        x25519_public,
                                        ml_dsa_public.clone(),
                                        ml_kem_public.clone(),
                                        pq_binding_proof.clone(),
                                    )
                                    .await?;
                                }
                                MessagePayload::Handshake {
                                    region,
                                    cluster_id,
                                    role,
                                } => {
                                    ctx.handle_handshake(
                                        &connection,
                                        &message.from,
                                        region,
                                        cluster_id,
                                        role,
                                    )
                                    .await;
                                }
                                MessagePayload::HandshakeAck => {
                                    ctx.handle_handshake_ack(&message.from);
                                }
                                MessagePayload::Ping { sent_at } => {
                                    ctx.handle_ping(
                                        &connection,
                                        message.clone(),
                                        &message.from,
                                        *sent_at,
                                    )
                                    .await;
                                }
                                MessagePayload::Pong {
                                    ping_sent_at,
                                    pong_sent_at,
                                } => {
                                    ctx.handle_pong(&message.from, *ping_sent_at, *pong_sent_at)
                                        .await;
                                }
                                MessagePayload::Gossip(ref gossip_msg) => {
                                    // Extract BlobAnnounce from gossip messages for data locality tracking
                                    if let icn_gossip::types::GossipMessage::BlobAnnounce {
                                        blob_hash,
                                        peer_did,
                                        size_bytes,
                                    } = gossip_msg
                                    {
                                        debug!(
                                            peer_did = %peer_did,
                                            blob_hash_len = blob_hash.len(),
                                            size_bytes = size_bytes,
                                            "Received blob announcement via gossip"
                                        );

                                        // Update blob location registry
                                        if let Some(ref registry) = blob_registry {
                                            if let Err(e) = registry.write().await.announce_blob(
                                                *blob_hash,
                                                peer_did.clone(),
                                                *size_bytes,
                                            ) {
                                                debug!(
                                                    error = %e,
                                                    peer_did = %peer_did,
                                                    blob_size = size_bytes,
                                                    "Rejected blob announcement from peer"
                                                );
                                            }
                                        }
                                    }

                                    // Forward gossip message to external handler
                                    handler(message);
                                }
                                MessagePayload::Signed(ref envelope) => {
                                    ctx.handle_signed(message.clone(), envelope).await;
                                }
                                MessagePayload::PeerExchange(ref peer_msg) => {
                                    ctx.handle_peer_exchange(
                                        &connection,
                                        message.clone(),
                                        &message.from,
                                        peer_msg,
                                    )
                                    .await;
                                }
                                MessagePayload::Onion(ref onion_msg) => {
                                    ctx.handle_onion(onion_msg).await;
                                }
                                _ => {
                                    // Forward other messages to the external handler
                                    handler(message);
                                }
                            }
                        }
                        Err(e) => {
                            let err_msg = e.to_string();

                            // Track protocol version mismatches
                            if err_msg.contains("too old") {
                                warn!("Protocol version too old: {}", err_msg);
                                icn_obs::metrics::network::protocol_version_too_old_inc();
                                icn_obs::metrics::network::protocol_version_mismatch_inc();
                            } else if err_msg.contains("too new") {
                                warn!("Protocol version too new: {}", err_msg);
                                icn_obs::metrics::network::protocol_version_too_new_inc();
                                icn_obs::metrics::network::protocol_version_mismatch_inc();
                            } else {
                                warn!("Failed to read message: {}", e);
                            }
                        }
                    }

                    // Close the stream
                    if let Err(e) = send.finish() {
                        tracing::debug!("Stream finish error (normal during disconnect): {}", e);
                    }
                }
                Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                    info!("Connection closed by peer");
                    break;
                }
                Err(e) => {
                    warn!("Error accepting stream: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}
