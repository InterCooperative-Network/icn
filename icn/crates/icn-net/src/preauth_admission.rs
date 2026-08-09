//! How many unauthenticated connections one source may hold at once (#2547).
//!
//! # What #2491 left open
//!
//! #2491 gave every connection its own anonymous budget, keyed on the `ConnectionContext`
//! so that nothing the sender says can be issued a different one. That closed the
//! identity-ordering defect. It deliberately did not bound *how many* of those budgets one
//! source may hold:
//!
//! ```text
//! attacker opens N connections -> attacker holds N anonymous budgets
//! ```
//!
//! Three facts on the inbound path make that worse than it looks. The handshake semaphore
//! releases its permit as soon as the handshake completes, so it caps concurrent
//! *handshakes* and never the number of established connections. This node installs
//! `keep_alive_interval` on its **server** config, so it PINGs its own inbound connections
//! and an idle one never reaches the idle timeout. And an unauthenticated connection is in
//! no map at all — the session map is populated only by an authenticated Hello (#2530) — so
//! nothing tracks or evicts it.
//!
//! Established-but-unauthenticated connections were therefore unbounded in count and
//! effectively immortal. This module bounds them on both axes.
//!
//! # The invariants
//!
//! **Count** (#2547):
//!
//! > At any instant, the number of established-but-unauthenticated inbound connections is
//! > bounded, both server-wide and per source.
//!
//! **Duration** (#2552):
//!
//! > A connection holding a pre-authentication slot must authenticate within
//! > [`PREAUTH_AUTHENTICATION_DEADLINE`] of taking it, or its transport is closed and the slot
//! > released.
//!
//! Neither is sufficient alone, and the second is why the first is worth having. A count bound
//! on its own describes an instant: an adversary that reaches the ceiling and never moves holds
//! it indefinitely, and since every new peer starts anonymous, a full table refuses *all* new
//! inbound connections rather than only anonymous ones. Silence is the cheapest way to do it —
//! cheaper than traffic, because this node's own keepalives maintain the connection and the
//! peer's QUIC stack answers at the transport layer. The duration bound converts "hold forever"
//! into "hold for `T`", which is what makes the count bound mean something over time.
//!
//! A duration bound alone would be equally useless: it bounds one connection and aggregates
//! nothing.
//!
//! **Neither is a rate bound.** Together they say how much anonymous concurrency may exist and
//! for how long, and nothing about how fast a source may open and close connections underneath
//! them; see "what this does not bound" below.
//!
//! **Pre-authentication only.** The reservation is released the moment a connection
//! authenticates, so this never constrains established authenticated peers — a peer that
//! says who it is stops consuming source admission immediately, and is thereafter governed
//! by the per-DID and per-anchor limits (#2490, #2491). The deadline ends at the same instant
//! and for the same reason: it is the slot's expiry, not the connection's maximum lifetime.
//!
//! # Why the post-handshake remote IP can be trusted as a key
//!
//! Admission is taken **after** the QUIC handshake completes, and that is what makes the
//! address meaningful. Completing a QUIC handshake proves return-routability (RFC 9000 §8):
//! the server sends its handshake packets to the claimed address and the peer must answer
//! from it. A spoofed source address cannot reach this hook at all.
//!
//! So the key is attacker-*chosen* but not attacker-*forgeable*: to present a different one,
//! an attacker must actually receive traffic there. That is precisely the property
//! `NetworkMessage.from` lacked in #2491, and it is why keying on it here is sound while
//! keying on a claimed DID was not.
//!
//! **QUIC migration does not multiply reservations.** The key is read once, at admission, and
//! the resulting [`AdmissionGuard`] carries it for the connection's whole life. A connection
//! that later migrates to a new address therefore stays charged to the source that was
//! return-routable when it was admitted; it cannot acquire a second slot, because a second
//! slot is only ever issued by a second admission, and admission happens once per connection.
//! Charging a migrated connection to its original source is the conservative direction: the
//! alternative — re-keying on migration — would let a peer release a contended allowance by
//! moving, which is the amplification this exists to prevent.
//!
//! The alternatives aggregate nothing or cost too much:
//!
//! - **remote `SocketAddr`** — a new source port is a new key, so reconnecting mints a fresh
//!   allowance and the bound aggregates nothing. QUIC connection migration can also change
//!   it mid-connection.
//! - **a global cap alone** — bounds total work, but one attacker can consume the entire
//!   allowance and lock every honest peer out. Kept here as a second bound, not the only one.
//!
//! # What this does not bound
//!
//! - Attacks distributed across many source addresses or many /64s. A single node cannot
//!   solve that locally, and nothing here pretends to.
//! - QUIC/TLS handshake work *below* this hook — that is the existing concurrent-handshake
//!   semaphore's job, and it bounds concurrency rather than rate.
//! - Connection *rate*. A source that opens, spends its anonymous burst, and closes stays
//!   within every bound here, because concurrency never rises. Churn is throttled only
//!   indirectly, by handshake concurrency divided by handshake latency. Tracked by #2549,
//!   which is separate because a rate bound needs entries that *outlive* their connections —
//!   the opposite of what makes this table's cardinality provable.
//!
//!   The deadline does not change this, and it is worth being exact about why, because
//!   "connections now expire" sounds like it should. Expiry bounds how long one connection may
//!   squat; it places no floor on how quickly the next may arrive. A source that connects,
//!   waits, is closed, and immediately reconnects stays inside every bound here forever — it
//!   simply pays a handshake each time instead of nothing. So the deadline converts a *free*
//!   permanent hold into a *recurring* cost, which is a real change in the attacker's economics
//!   and not a rate limit. #2549 remains exactly as necessary as it was.
//! - How many connections one source may hold once they are *authenticated*: the slot is
//!   released at the Hello, and DIDs are free to mint. Tracked by #2550. The deadline stops at
//!   the same boundary deliberately — extending it past authentication would make it a maximum
//!   connection lifetime, which is a different bound with different victims, and belongs to
//!   #2550 if it is wanted at all.
//! - Authenticated application traffic, which is #2490's and #2491's subject.
//!
//! None of this is general DoS prevention.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long an admitted connection may stay unauthenticated before it is closed (#2552).
///
/// # Why a duration bound exists at all
///
/// The two count bounds below are instantaneous: they say how many anonymous connections may
/// exist *now*. On their own that is a ceiling an adversary can reach and simply stay at, because
/// nothing here made a connection ever finish becoming a peer. Silence was free — cheaper than
/// traffic, since the node's own `keep_alive_interval` maintains the connection and the peer's
/// QUIC stack answers at the transport layer. Converting "forever" into a finite `T` is what
/// makes a count bound mean anything over time rather than only at an instant.
///
/// # Why thirty seconds
///
/// The value is policy, and deliberately generous, because no security property here depends on
/// it — every value converts an unbounded hold into a bounded one, and the choice only trades
/// how long an adversary may squat against how much slack an honest peer gets.
///
/// A healthy exchange needs about one round trip. An ICN dialer sends its Hello immediately and
/// unconditionally on connecting (`actor::messages::wire_new_connection`); there is no lazy or
/// deferred path in which an honest peer legitimately holds an inbound connection anonymous and
/// waits. So the accepting side expects a Hello roughly one RTT after the handshake, plus this
/// node's own Ed25519 binding checks, which are sub-millisecond.
///
/// Thirty seconds is therefore some thirty times the *pathological* healthy case — a WAN RTT of
/// a second, retransmits, a badly overloaded node's scheduling delay, a peer whose path changed
/// mid-connection. Two further properties pin it rather than leaving it a round number:
///
/// - it equals `keep_alive_interval`, so the node spends at most one keepalive cycle on a peer
///   that will not identify itself — a direct answer to the mechanism that caused the defect;
/// - it is strictly below `max_idle_timeout` (60 s), so for a silent connection *this* is the
///   binding constraint rather than a race with the idle timer, which keeps the behaviour
///   attributable to one cause.
///
/// Like the two counts, this is a protocol constant rather than operator configuration. That is
/// consistency with the bound it completes, and a conservative starting point — not a claim that
/// no deployment will want to tune it.
pub const PREAUTH_AUTHENTICATION_DEADLINE: Duration = Duration::from_secs(30);

/// QUIC application close code for a connection that never authenticated in time (#2552).
///
/// Distinct from a silent drop so the peer learns *why*, and distinct from the admission refusal
/// code next to it (`0x1c4b`) because the two are different events an operator needs to tell
/// apart: refused means "the table was full when you arrived", timed out means "you were given a
/// slot and never used it". Neither is a protocol error — a peer that is slow, or that connected
/// speculatively, is not misbehaving, and retrying later is the correct response.
pub const PREAUTH_AUTHENTICATION_TIMEOUT_CODE: u32 = 0x1c4c;

/// Maximum established-but-unauthenticated inbound connections, server-wide.
///
/// Sized above `MAX_CONCURRENT_INBOUND_HANDSHAKES` (64, in `actor::connection`)
/// so that this bound never shadows it: every handshake the node is willing to run
/// concurrently must have somewhere to land, or the two limits would fight and the symptom
/// would be refused connections under ordinary load rather than under attack.
pub const MAX_PREAUTH_CONNECTIONS_TOTAL: usize = 256;

/// Maximum established-but-unauthenticated inbound connections from one source.
///
/// A peer needs exactly one: connect, Hello, authenticated. This is not a limit on how many
/// connections a source may hold — an authenticated connection has already released its
/// reservation — it is a limit on how many it may hold *while still refusing to say who it
/// is*. Eight leaves room for genuine concurrency (several peers behind one NAT starting at
/// once, a retry racing an original) while keeping the per-source share of
/// [`MAX_PREAUTH_CONNECTIONS_TOTAL`] small enough that one source cannot crowd the table.
///
/// Like the concurrent-handshake cap this sits next to, it is a protocol constant rather
/// than operator configuration. That is a deliberately conservative starting point, not a
/// claim that no deployment will want to tune it.
pub const MAX_PREAUTH_CONNECTIONS_PER_SOURCE: usize = 8;

/// The unit a pre-authentication allowance is charged to.
///
/// IPv4 is keyed on the exact address. IPv6 is keyed on the **/64**.
///
/// The v6 argument is narrower than "every host owns a /64", which is not true in general.
/// It is only this: an IPv6 client can often change its source address *cheaply* — SLAAC and
/// privacy extensions rotate within a prefix by design, and delegations of a /64 or shorter
/// are common — so keying on the exact address would frequently aggregate nothing, the same
/// way keying on a source port does. /64 is the smallest unit that is not routinely cheap to
/// rotate within. It is a heuristic about cost, not a claim about allocation.
///
/// That heuristic has a real price, stated rather than buried: hosts sharing a /64 share an
/// allowance, so a v6 network that puts many distinct peers in one /64 is charged as one
/// source. The same is true of IPv4 behind a NAT. What keeps that cost small is *when* the
/// allowance is released — at authentication, not at close — so peers only contend while
/// they are still anonymous. See [`PreAuthAdmission`].
///
/// Treating v6 structurally rather than as an opaque address matches what this crate already
/// does when it classifies addresses (`handlers::peer_exchange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKey {
    /// An exact IPv4 address.
    V4(std::net::Ipv4Addr),
    /// The leading 64 bits of an IPv6 address.
    V6Prefix([u8; 8]),
}

impl SourceKey {
    /// The key a connection from `addr` is charged to.
    ///
    /// An IPv4-mapped address is unmapped first. This is not a tidiness step — it is load
    /// bearing on every dual-stack deployment, which is the default one. `listen_addr` defaults
    /// to `[::]:7777`, `IPV6_V6ONLY` is left at the OS default (0 on Linux), and nothing in this
    /// crate sets it, so an IPv4 peer is reported by the socket as `::ffff:a.b.c.d`. Rust does
    /// not unmap those implicitly, so taking the v6 branch would read a /64 of all zeros and
    /// collapse **every IPv4 client on the internet into one key** — one shared allowance for the
    /// entire v4 population, which any single v4 peer could hold empty.
    ///
    /// Only `::ffff:0:0/96` is unmapped, via [`std::net::Ipv6Addr::to_ipv4_mapped`]. Deprecated
    /// IPv4-*compatible* addresses (`::a.b.c.d`) are deliberately left on the v6 path: they are
    /// not what a dual-stack socket produces, and `to_ipv4` would also rewrite `::1` to
    /// `0.0.0.1`, quietly merging v6 loopback into a v4 key.
    pub fn from_addr(addr: SocketAddr) -> Self {
        match addr.ip() {
            IpAddr::V4(ip) => SourceKey::V4(ip),
            IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
                Some(v4) => SourceKey::V4(v4),
                None => {
                    let octets = ip.octets();
                    let mut prefix = [0u8; 8];
                    prefix.copy_from_slice(&octets[..8]);
                    SourceKey::V6Prefix(prefix)
                }
            },
        }
    }
}

/// Why a connection was refused admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionRefusal {
    /// The server-wide pre-authentication allowance is exhausted.
    GlobalLimit,
    /// This source already holds its maximum pre-authentication allowance.
    SourceLimit,
}

impl AdmissionRefusal {
    /// A short, bounded label for logs and metrics.
    ///
    /// Deliberately a fixed set of two strings: it is safe as a metric label precisely
    /// because nothing an attacker sends can add a new value.
    pub fn as_str(self) -> &'static str {
        match self {
            AdmissionRefusal::GlobalLimit => "global_limit",
            AdmissionRefusal::SourceLimit => "source_limit",
        }
    }
}

/// The table's live cardinality — exactly what the two gauges report.
///
/// Captured while the state lock is held and published *after* it is released. Both halves
/// matter: reading under the lock is what makes the pair consistent rather than two
/// independently-sampled numbers, and publishing outside it keeps foreign metrics code —
/// which may allocate — out of a critical section that serialises every inbound admission.
#[derive(Debug, Clone, Copy)]
struct Cardinality {
    total: usize,
    sources: usize,
}

impl Cardinality {
    fn of(state: &AdmissionState) -> Self {
        Self {
            total: state.total,
            sources: state.per_source.len(),
        }
    }

    fn publish(self) {
        icn_obs::metrics::network::preauth_connections_live_set(self.total);
        icn_obs::metrics::network::preauth_sources_tracked_set(self.sources);
    }
}

#[derive(Debug, Default)]
struct AdmissionState {
    /// Live pre-authentication connections per source.
    ///
    /// An entry exists only while its count is non-zero — see [`PreAuthAdmission`] on why
    /// that is what keeps this map from becoming the leak it is meant to prevent.
    per_source: HashMap<SourceKey, usize>,
    /// Live pre-authentication connections, all sources.
    total: usize,
}

/// The pre-authentication connection allowance, shared by the whole node.
///
/// # Why this map cannot be turned into a leak
///
/// A DoS defence that introduces an attacker-controlled unbounded map is not a defence. This
/// one is bounded by construction rather than by policy:
///
/// - an entry is created only by a successful admission, and admission first checks `total`
///   against [`MAX_PREAUTH_CONNECTIONS_TOTAL`];
/// - an entry is removed the instant its count reaches zero;
/// - therefore every entry has at least one live connection behind it, and
///   `per_source.len() <= total <= MAX_PREAUTH_CONNECTIONS_TOTAL` holds at all times.
///
/// There is no expiry timer, no eviction policy and no cleanup task, because there is
/// nothing to expire: the map's size is pinned to the number of connections currently being
/// counted, and those are bounded. Releasing is not a background job that could fall behind —
/// it is [`AdmissionGuard`]'s destructor, so it runs on every exit path including panics.
#[derive(Debug)]
pub struct PreAuthAdmission {
    state: Mutex<AdmissionState>,
    max_total: usize,
    max_per_source: usize,
    authentication_deadline: Duration,
}

impl PreAuthAdmission {
    /// The node-wide admission table with the default bounds.
    pub fn new() -> Self {
        Self::with_limits(
            MAX_PREAUTH_CONNECTIONS_TOTAL,
            MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
        )
    }

    /// An admission table with explicit count bounds and the default deadline.
    ///
    /// Exists so tests can drive the boundary without opening hundreds of real connections.
    pub fn with_limits(max_total: usize, max_per_source: usize) -> Self {
        Self::with_policy(max_total, max_per_source, PREAUTH_AUTHENTICATION_DEADLINE)
    }

    /// An admission table with every bound stated explicitly.
    ///
    /// The whole pre-authentication policy — how many anonymous connections, and for how long —
    /// lives on one object because the two halves are the same bound. A count without a duration
    /// is a ceiling an adversary can park at; a duration without a count bounds one connection
    /// and no aggregate.
    pub fn with_policy(
        max_total: usize,
        max_per_source: usize,
        authentication_deadline: Duration,
    ) -> Self {
        Self {
            state: Mutex::new(AdmissionState::default()),
            max_total,
            max_per_source,
            authentication_deadline,
        }
    }

    /// Reserve a pre-authentication slot for a connection from `addr`.
    ///
    /// Returns the reservation on success. Dropping it releases the slot, so the caller
    /// cannot forget: there is deliberately no `release` free function to leave uncalled.
    pub fn try_admit(
        self: &Arc<Self>,
        addr: SocketAddr,
    ) -> Result<AdmissionGuard, AdmissionRefusal> {
        let key = SourceKey::from_addr(addr);
        // Read before the slot is taken, so the deadline can only ever be *shorter* than the
        // slot's real life, never longer. The two are created by one operation, which is what
        // lets the guard state its own expiry rather than the caller guessing when it started.
        let authenticate_by = Instant::now() + self.authentication_deadline;
        let cardinality = {
            let mut state = self.lock_state();

            if state.total >= self.max_total {
                return Err(AdmissionRefusal::GlobalLimit);
            }
            let source_count = state.per_source.entry(key).or_insert(0);
            if *source_count >= self.max_per_source {
                // Leave no zero-valued entry behind: a refused admission must not be able to
                // grow the map, or refusing would itself become the amplification.
                if *source_count == 0 {
                    state.per_source.remove(&key);
                }
                return Err(AdmissionRefusal::SourceLimit);
            }
            *source_count += 1;
            state.total += 1;

            Cardinality::of(&state)
        };
        cardinality.publish();

        Ok(AdmissionGuard {
            admission: Arc::clone(self),
            key,
            authenticate_by,
        })
    }

    /// Live pre-authentication connections, all sources.
    pub fn live_total(&self) -> usize {
        self.lock_state().total
    }

    /// Live pre-authentication connections charged to `addr`'s source.
    pub fn live_for(&self, addr: SocketAddr) -> usize {
        let key = SourceKey::from_addr(addr);
        self.lock_state().per_source.get(&key).copied().unwrap_or(0)
    }

    /// Number of sources currently holding at least one pre-authentication slot.
    ///
    /// This is the map's cardinality, exposed so the bound above can be asserted rather than
    /// asserted-about.
    pub fn tracked_sources(&self) -> usize {
        self.lock_state().per_source.len()
    }

    /// Take the state lock, recovering from a poisoned mutex rather than panicking.
    ///
    /// A panic elsewhere must not turn connection admission into a permanent outage: the
    /// counters are plain integers, so the worst a poisoned lock can mean is that one
    /// release was interrupted, and refusing every future connection would be a far worse
    /// failure than proceeding with a slightly stale count.
    fn lock_state(&self) -> std::sync::MutexGuard<'_, AdmissionState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Give back one slot charged to `key`, and republish the gauges.
    ///
    /// The gauges are refreshed *here*, at the mutation, rather than by whoever happened to
    /// cause it. There are two release paths — the connection authenticates, or its handler
    /// task ends — and only the second passes through the accept loop. Refreshing at the
    /// call sites therefore left the gauges stale for the whole life of every connection
    /// that authenticated and stayed open, which is the ordinary case and precisely the one
    /// an operator reads these gauges to understand.
    fn release(&self, key: SourceKey) {
        let cardinality = {
            let mut state = self.lock_state();
            state.total = state.total.saturating_sub(1);
            if let Some(count) = state.per_source.get_mut(&key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    // The entry's whole purpose was to carry a non-zero count. Removing it
                    // here is what bounds the map's cardinality by the global limit.
                    state.per_source.remove(&key);
                }
            }
            Cardinality::of(&state)
        };
        cardinality.publish();
    }
}

impl Default for PreAuthAdmission {
    fn default() -> Self {
        Self::new()
    }
}

/// A reserved pre-authentication slot, released when dropped.
///
/// Held by the connection's [`ConnectionContext`](crate::handlers::ConnectionContext) until
/// either the connection authenticates — at which point the context drops it, because an
/// identified peer no longer spends anonymous admission — or the handler task ends and the
/// context is dropped with it. Both paths are the same destructor, so there is no ordering
/// to get wrong and no failure path that leaks a slot.
#[derive(Debug)]
pub struct AdmissionGuard {
    admission: Arc<PreAuthAdmission>,
    key: SourceKey,
    /// When this reservation stops being justified — see [`Self::authenticate_by`].
    authenticate_by: Instant,
}

impl AdmissionGuard {
    /// The instant by which the connection holding this slot must have authenticated.
    ///
    /// Stamped by [`PreAuthAdmission::try_admit`], so the answer to "when did the clock start"
    /// is "when the resource was taken" — not "when some handler got around to noticing". The
    /// guard carries it for its whole life, so nothing downstream has to reconstruct a start
    /// time, and a connection cannot be given a fresh deadline by being passed around.
    ///
    /// Enforcing it is the connection handler's job (`actor::connection`), because expiry has to
    /// close a transport and this type deliberately knows nothing about transports. What lives
    /// here is the *when*; what lives there is the *what happens*.
    pub fn authenticate_by(&self) -> Instant {
        self.authenticate_by
    }
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        self.admission.release(self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(addr: &str) -> SocketAddr {
        format!("{addr}:9000").parse().expect("test address")
    }

    fn v6(addr: &str) -> SocketAddr {
        format!("[{addr}]:9000").parse().expect("test address")
    }

    /// Two IPv4 clients arriving over a dual-stack listener are two sources, not one.
    ///
    /// The default `listen_addr` is `[::]:7777` and nothing sets `IPV6_V6ONLY`, so on Linux
    /// (`bindv6only=0`) an IPv4 peer is reported as `::ffff:a.b.c.d`. Rust does not unmap that
    /// implicitly, so the v6 branch would read a /64 of all zeros and give every IPv4 client on
    /// the internet one shared key — a single peer could then hold the whole v4 population's
    /// allowance at empty. This is the regression test for that.
    #[test]
    fn ipv4_mapped_sources_do_not_collapse_into_one_key() {
        let a = SourceKey::from_addr(v6("::ffff:198.51.100.7"));
        let b = SourceKey::from_addr(v6("::ffff:203.0.113.9"));

        assert_ne!(
            a, b,
            "two different IPv4 clients over a dual-stack listener collapsed to one source key"
        );
        assert_eq!(
            a,
            SourceKey::from_addr(v4("198.51.100.7")),
            "the same client must key identically whether it arrives mapped or native"
        );
        assert!(
            matches!(a, SourceKey::V4(_)),
            "a mapped IPv4 address must be charged as IPv4, not as a /64"
        );
    }

    /// Unmapping must not swallow addresses that are genuinely IPv6.
    ///
    /// `to_ipv4_mapped` is used rather than `to_ipv4` precisely so that `::1` stays v6 — `to_ipv4`
    /// would rewrite it to `0.0.0.1` and merge v6 loopback into a v4 key.
    #[test]
    fn genuine_ipv6_sources_still_key_on_the_prefix() {
        let a = SourceKey::from_addr(v6("2001:db8::1"));
        let b = SourceKey::from_addr(v6("2001:db8::dead:beef"));
        let other = SourceKey::from_addr(v6("2001:db8:1::1"));

        assert_eq!(a, b, "host bits inside one /64 are not new sources");
        assert_ne!(a, other, "a different /64 must still be its own source");
        assert!(matches!(a, SourceKey::V6Prefix(_)));
        assert!(
            matches!(SourceKey::from_addr(v6("::1")), SourceKey::V6Prefix(_)),
            "v6 loopback must not be rewritten into an IPv4 key"
        );
    }

    #[test]
    fn a_source_cannot_exceed_its_share() {
        let admission = Arc::new(PreAuthAdmission::with_limits(100, 3));
        let addr = v4("203.0.113.7");

        let guards: Vec<_> = (0..3)
            .map(|_| admission.try_admit(addr).expect("within the source limit"))
            .collect();
        assert_eq!(admission.live_for(addr), 3);

        assert_eq!(
            admission.try_admit(addr).unwrap_err(),
            AdmissionRefusal::SourceLimit,
            "a fourth slot was issued to a source limited to three"
        );

        drop(guards);
        assert_eq!(admission.live_for(addr), 0);
        assert!(
            admission.try_admit(addr).is_ok(),
            "releasing every slot must make the source admissible again"
        );
    }

    #[test]
    fn one_source_cannot_lock_out_another() {
        let admission = Arc::new(PreAuthAdmission::with_limits(100, 2));
        let noisy = v4("203.0.113.7");
        let honest = v4("198.51.100.9");

        let _held: Vec<_> = (0..2)
            .map(|_| admission.try_admit(noisy).expect("within the source limit"))
            .collect();
        assert_eq!(
            admission.try_admit(noisy).unwrap_err(),
            AdmissionRefusal::SourceLimit
        );

        assert!(
            admission.try_admit(honest).is_ok(),
            "an exhausted source must not deny an unrelated one — that would make the \
             defence itself the outage"
        );
    }

    #[test]
    fn the_global_limit_binds_across_sources() {
        let admission = Arc::new(PreAuthAdmission::with_limits(3, 100));

        let _a = admission.try_admit(v4("203.0.113.1")).expect("first");
        let _b = admission.try_admit(v4("203.0.113.2")).expect("second");
        let _c = admission.try_admit(v4("203.0.113.3")).expect("third");

        assert_eq!(
            admission.try_admit(v4("203.0.113.4")).unwrap_err(),
            AdmissionRefusal::GlobalLimit,
            "the server-wide bound must hold even though no single source exceeded its share"
        );
    }

    #[test]
    fn ipv6_is_aggregated_by_prefix_not_by_address() {
        let admission = Arc::new(PreAuthAdmission::with_limits(100, 2));

        // Three different addresses, one /64. Rotating inside a prefix is free, so it must
        // not buy a fresh allowance.
        let _a = admission.try_admit(v6("2001:db8:0:1::1")).expect("first");
        let _b = admission.try_admit(v6("2001:db8:0:1::2")).expect("second");
        assert_eq!(
            admission.try_admit(v6("2001:db8:0:1::3")).unwrap_err(),
            AdmissionRefusal::SourceLimit,
            "rotating the low 64 bits bought a fresh allowance"
        );

        // A genuinely different /64 is a different source.
        assert!(
            admission.try_admit(v6("2001:db8:0:2::1")).is_ok(),
            "a different /64 must be charged separately"
        );
    }

    /// The map's cardinality is the thing that could turn this defence into the leak it
    /// prevents, so it is asserted directly rather than reasoned about.
    #[test]
    fn the_table_never_outgrows_the_global_limit() {
        let admission = Arc::new(PreAuthAdmission::with_limits(4, 4));

        // Far more distinct sources than the global limit allows.
        let mut guards = Vec::new();
        for i in 0..200u32 {
            let addr = v4(&format!("203.0.113.{}", i % 256));
            if let Ok(guard) = admission.try_admit(addr) {
                guards.push(guard);
            }
        }
        assert_eq!(admission.live_total(), 4);
        assert!(
            admission.tracked_sources() <= 4,
            "the admission table grew past the global limit: {} entries",
            admission.tracked_sources()
        );

        // And a refused admission must not leave an entry behind either.
        drop(guards);
        assert_eq!(admission.live_total(), 0);
        assert_eq!(
            admission.tracked_sources(),
            0,
            "releasing every slot must leave no residue in the table"
        );
    }

    /// The expiry is born with the slot, not with whoever later looks at it.
    ///
    /// This is the "when does the clock start" answer made checkable: a guard handed around,
    /// stored, or read late still reports the instant its *reservation* was taken.
    #[test]
    fn a_slot_carries_the_deadline_it_was_admitted_with() {
        let deadline = Duration::from_millis(250);
        let admission = Arc::new(PreAuthAdmission::with_policy(10, 10, deadline));

        let before = Instant::now();
        let guard = admission.try_admit(v4("203.0.113.7")).expect("admitted");
        let after = Instant::now();

        assert!(
            guard.authenticate_by() >= before + deadline
                && guard.authenticate_by() <= after + deadline,
            "the deadline was not stamped from the moment the slot was taken"
        );
    }

    /// Slots admitted at different moments expire at different moments.
    ///
    /// Guards against a deadline shared by the table rather than owned by the reservation — a
    /// shape in which one long-lived squatter's expiry would silently become everybody's.
    #[test]
    fn each_slot_expires_on_its_own_clock() {
        let admission = Arc::new(PreAuthAdmission::with_policy(
            10,
            10,
            Duration::from_millis(250),
        ));

        let first = admission.try_admit(v4("203.0.113.1")).expect("admitted");
        std::thread::sleep(Duration::from_millis(20));
        let second = admission.try_admit(v4("203.0.113.2")).expect("admitted");

        assert!(
            second.authenticate_by() > first.authenticate_by(),
            "two slots taken at different times were given the same expiry"
        );
    }

    /// The production constructors carry the production deadline.
    ///
    /// Without this, `with_policy` could drift into being the only path that sets a deadline at
    /// all and every real connection would quietly get whatever `Default` produced — the failure
    /// mode where the tests exercise a policy the node never uses.
    #[test]
    fn the_default_table_uses_the_protocol_deadline() {
        for admission in [
            Arc::new(PreAuthAdmission::new()),
            Arc::new(PreAuthAdmission::with_limits(10, 10)),
        ] {
            let before = Instant::now();
            let guard = admission.try_admit(v4("203.0.113.7")).expect("admitted");

            assert!(
                guard.authenticate_by() >= before + PREAUTH_AUTHENTICATION_DEADLINE,
                "a table built for production did not use PREAUTH_AUTHENTICATION_DEADLINE"
            );
        }
    }

    /// A refusal must not itself grow the map — otherwise attacking the limiter would be
    /// cheaper than attacking what it protects.
    #[test]
    fn refusals_leave_no_residue() {
        let admission = Arc::new(PreAuthAdmission::with_limits(1, 1));
        let _held = admission.try_admit(v4("203.0.113.1")).expect("first");

        for i in 0..100u32 {
            let addr = v4(&format!("198.51.100.{}", i % 256));
            assert_eq!(
                admission.try_admit(addr).unwrap_err(),
                AdmissionRefusal::GlobalLimit
            );
        }
        assert_eq!(
            admission.tracked_sources(),
            1,
            "refused admissions accumulated entries in the table"
        );
    }
}
