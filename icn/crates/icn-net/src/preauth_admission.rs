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
//! **Rate** (#2549):
//!
//! > A source may take at most [`PREAUTH_CHURN_BUDGET_PER_SOURCE`] admissions that end without
//! > ever authenticating per [`PREAUTH_CHURN_REFILL_WINDOW`], however quickly it releases them.
//!
//! The first two are instantaneous and say nothing about how fast a source may open and close
//! connections underneath them. A source that connected, spent its anonymous burst, closed and
//! reconnected stayed inside both forever, because concurrency never rose and no deadline was
//! ever reached — and each cycle was issued a *fresh* anonymous budget, because the table's
//! entry for that source had been removed at zero. The third bound is what makes the first two
//! aggregate over time rather than only describe an instant.
//!
//! The three are one policy, and the third is derived from the other two rather than chosen:
//! see [`PREAUTH_CHURN_BUDGET_PER_SOURCE`].
//!
//! **The charge lands at release, not at admission**, because only then is it known whether the
//! admission was abandoned. One consequence is worth stating rather than leaving to be
//! rediscovered: a source may hold admissions whose charges have not landed yet, so a burst can
//! momentarily exceed the budget before any of it is debited. That burst is bounded by
//! [`MAX_PREAUTH_CONNECTIONS_PER_SOURCE`] — the connections have to be *held* to be uncharged,
//! and holding is what #2547 already bounds — so it grants nothing that bound did not already
//! grant. The *sustained* rate is unaffected: every one of those admissions is charged as it
//! ends. `preauth_source_churn_rate.rs` pins both halves.
//!
//! Debiting at admission instead would remove the burst and break the property the bound exists
//! to protect: a NAT whose peers all reconnect at once would be refused on the ninth peer,
//! before any of them had a chance to authenticate and prove they were not churn.
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
//! - Churn by a source that *does* authenticate each time. The rate bound counts abandoned
//!   admissions, so a peer that proves an identity and disconnects is never charged. That is
//!   deliberate — it is what keeps the bound off honest reconnects — and it means an adversary
//!   willing to mint a DID and complete a Hello each cycle is outside this bound. It is then
//!   inside #2490's and #2491's per-DID limits, and how many such connections it may hold at
//!   once is #2550.
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

/// How many admissions a source may take *and never authenticate* per
/// [`PREAUTH_CHURN_REFILL_WINDOW`] (#2549).
///
/// # Why this is the same number, and not a new policy
///
/// The two bounds above already permit one source to hold
/// [`MAX_PREAUTH_CONNECTIONS_PER_SOURCE`] anonymous connections for
/// [`PREAUTH_AUTHENTICATION_DEADLINE`] each, indefinitely, by taking its full allowance and
/// sitting on it until every deadline expires. That is a *sustained* eight
/// never-authenticated admissions per thirty seconds, available to any source today, and
/// #2547 accepted it deliberately.
///
/// Deriving the churn budget from that ceiling rather than choosing one gives the bound its
/// statement:
///
/// > Cycling connections cannot obtain more anonymous admissions than squatting on them
/// > already could.
///
/// It is therefore the tightest rate bound that provably refuses nothing the existing count
/// and duration bounds already allow — a source at the old ceiling is exactly at the new one
/// and is never refused for churning. Picking any other number would introduce an
/// externally-visible quota that neither #2547 nor #2552 implies, which is the thing #2549
/// was filed to avoid smuggling in.
pub const PREAUTH_CHURN_BUDGET_PER_SOURCE: usize = MAX_PREAUTH_CONNECTIONS_PER_SOURCE;

/// How long a source's exhausted churn budget takes to refill completely (#2549).
///
/// Equal to [`PREAUTH_AUTHENTICATION_DEADLINE`] for the reason given on
/// [`PREAUTH_CHURN_BUDGET_PER_SOURCE`]: the pair is one derivation, not two choices. Budget
/// over window is precisely the rate a maximally-squatting source already sustains.
///
/// Refill is continuous rather than a window that resets, so there is no edge an adversary
/// can align with to take two full budgets back to back.
pub const PREAUTH_CHURN_REFILL_WINDOW: Duration = PREAUTH_AUTHENTICATION_DEADLINE;

/// Maximum sources whose churn history is retained at once (#2549).
///
/// # What this bound is, and what it is not
///
/// Unlike the three above it, this is not a security policy — no property stated anywhere in
/// this module depends on its value. It is a memory ceiling, and it exists because a defence
/// that answers unbounded connection work with an unbounded map has not defended anything.
///
/// The churn table is the one piece of state here that outlives its connections, which is
/// what #2549 needs and what #2547's table deliberately refused to have. What keeps it small
/// in the case this module is *about* is that an entry is created only by an admission that
/// failed to authenticate, and is charged to a source that completed a QUIC handshake from a
/// return-routable address. One attacker cycling a single address as fast as it can produces
/// exactly one entry. Reaching this ceiling requires four thousand distinct return-routable
/// sources sustaining failed admissions at once — the distributed case this module has always
/// said it cannot solve.
///
/// At capacity the table stops *tracking* new sources rather than refusing them; see
/// [`PreAuthAdmission`] on why the other direction would be worse than the defect.
pub const MAX_PREAUTH_CHURN_SOURCES: usize = 4096;

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
    pub fn from_addr(addr: SocketAddr) -> Self {
        match addr.ip() {
            IpAddr::V4(ip) => SourceKey::V4(ip),
            IpAddr::V6(ip) => {
                let octets = ip.octets();
                let mut prefix = [0u8; 8];
                prefix.copy_from_slice(&octets[..8]);
                SourceKey::V6Prefix(prefix)
            }
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
    /// This source has spent its budget of admissions that never authenticated (#2549).
    ///
    /// Distinct from [`Self::SourceLimit`] because the two describe opposite shapes and an
    /// operator needs to tell them apart: `SourceLimit` means "you are holding too many at
    /// once", `SourceChurn` means "you are holding none, and that is the problem — the last
    /// several you took were abandoned without ever saying who you were".
    SourceChurn,
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
            AdmissionRefusal::SourceChurn => "source_churn",
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

/// One source's remaining budget of admissions that may end without authenticating (#2549).
///
/// A continuously-refilling allowance rather than a counter with a reset, so there is no
/// window boundary to align with. `tokens` is what is left; it is debited when an admission
/// is released without ever having authenticated, and refills at
/// `budget / PREAUTH_CHURN_REFILL_WINDOW`.
///
/// The type carries no capacity or rate of its own: both live on [`PreAuthAdmission`], so a
/// bucket cannot disagree with the policy that created it.
#[derive(Debug, Clone, Copy)]
struct ChurnBucket {
    tokens: f64,
    /// When `tokens` was last brought up to date. Refill is computed from here on touch
    /// rather than by a timer, so an untouched entry costs nothing until it is read.
    updated_at: Instant,
}

impl ChurnBucket {
    /// A source seen for the first time: nothing spent yet.
    fn full(budget: f64, now: Instant) -> Self {
        Self {
            tokens: budget,
            updated_at: now,
        }
    }

    /// Bring `tokens` up to `now`, saturating at `budget`.
    ///
    /// `saturating_duration_since` rather than subtraction: `now` is supplied by the caller,
    /// and a clock that appears to go backwards must refill nothing rather than panic.
    fn refill(&mut self, budget: f64, rate_per_sec: f64, now: Instant) {
        let elapsed = now.saturating_duration_since(self.updated_at).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rate_per_sec).min(budget);
        self.updated_at = now;
    }

    /// Whether this entry still says anything.
    ///
    /// A full bucket is indistinguishable from a source that has never been seen: both admit
    /// and both debit from the same starting point. Dropping one is therefore not an eviction
    /// policy with a security consequence — it is removing a record that carries no
    /// information, which is what lets this map be swept without choosing victims.
    fn is_spent_out(&self, budget: f64) -> bool {
        self.tokens >= budget
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
    /// Per-source churn budgets, which deliberately **outlive** their connections (#2549).
    ///
    /// This is the one piece of state here that `per_source` could not be: a rate bound whose
    /// records die with the last connection is re-minted on every reconnect, which is the
    /// defect. Bounded by [`MAX_PREAUTH_CHURN_SOURCES`] and swept of spent-out entries rather
    /// than pinned by construction.
    churn: HashMap<SourceKey, ChurnBucket>,
    /// When the churn map was last swept of full buckets.
    ///
    /// Sweeping is O(map), so it must not be reachable once per admission or the defence
    /// becomes its own CPU amplifier. `None` means never swept.
    churn_swept_at: Option<Instant>,
}

/// How often the churn map may be swept of spent-out entries.
///
/// The sweep is O(map) and is reached only when the map is at capacity, which is exactly when
/// an adversary would like it to run on every admission. Rate-limiting it converts that from a
/// per-connection cost into a per-second one; between sweeps a full map simply stops tracking
/// new sources.
const CHURN_SWEEP_MIN_INTERVAL: Duration = Duration::from_secs(1);

impl AdmissionState {
    /// Whether `key` may take another admission, debiting nothing.
    ///
    /// Refills the source's bucket to `now` and reports whether a whole token is available.
    /// A source with no entry has spent nothing and is always admitted — and, importantly,
    /// **no entry is created here**. Checking must not be able to grow the map, or an
    /// adversary would populate it by being refused, which is the amplification #2547 was
    /// careful to avoid in the sibling table.
    fn churn_admits(
        &mut self,
        key: SourceKey,
        budget: f64,
        refill_per_sec: f64,
        now: Instant,
    ) -> bool {
        let Some(bucket) = self.churn.get_mut(&key) else {
            return true;
        };
        bucket.refill(budget, refill_per_sec, now);
        if bucket.is_spent_out(budget) {
            // Refilled to full: the record now says exactly what no record says. Dropping it
            // here is the ordinary way entries leave, so the map shrinks under normal traffic
            // without any sweep having to run.
            self.churn.remove(&key);
            return true;
        }
        bucket.tokens >= 1.0
    }

    /// Charge `key` for one admission that ended without ever authenticating.
    ///
    /// This is the only thing that writes the churn map, which is what makes the bound's
    /// subject precise: not connections, not admissions, but *abandoned* admissions.
    fn charge_churn(
        &mut self,
        key: SourceKey,
        budget: f64,
        refill_per_sec: f64,
        max_sources: usize,
        now: Instant,
    ) {
        match self.churn.get_mut(&key) {
            Some(bucket) => {
                bucket.refill(budget, refill_per_sec, now);
                bucket.tokens = (bucket.tokens - 1.0).max(0.0);
            }
            None => {
                if self.churn.len() >= max_sources {
                    self.sweep_churn(budget, refill_per_sec, now);
                }
                if self.churn.len() >= max_sources {
                    // Full even after a sweep — or too soon after the last one. Admit the
                    // source untracked rather than refuse it: see `PreAuthAdmission` on why
                    // the other direction turns this defence into an outage.
                    icn_obs::metrics::network::preauth_churn_untracked_inc();
                    return;
                }
                let mut bucket = ChurnBucket::full(budget, now);
                bucket.tokens = (bucket.tokens - 1.0).max(0.0);
                self.churn.insert(key, bucket);
            }
        }
    }

    /// Drop every churn entry that has refilled to full.
    ///
    /// Removes only records that carry no information, so it is not an eviction policy and
    /// has no victim: a swept source is admitted and charged exactly as an unswept one would
    /// have been. Rate-limited by [`CHURN_SWEEP_MIN_INTERVAL`].
    fn sweep_churn(&mut self, budget: f64, refill_per_sec: f64, now: Instant) {
        if let Some(last) = self.churn_swept_at {
            if now.saturating_duration_since(last) < CHURN_SWEEP_MIN_INTERVAL {
                return;
            }
        }
        self.churn_swept_at = Some(now);
        self.churn.retain(|_, bucket| {
            bucket.refill(budget, refill_per_sec, now);
            !bucket.is_spent_out(budget)
        });
    }
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
///
/// # The churn table, which is bounded differently and had to be
///
/// The rate bound (#2549) could not reuse that argument. A record that dies with the last
/// connection from a source is re-minted on the next one, so the budget resets on every
/// reconnect — which is the defect, not the fix. The churn table therefore *outlives* its
/// connections, and pays for it with an explicit ceiling
/// ([`MAX_PREAUTH_CHURN_SOURCES`]) and a sweep instead of a proof.
///
/// Three things keep that from being a worse leak than the one it closes:
///
/// - **only failures are recorded.** An admission that authenticates leaves nothing behind, so
///   ordinary peers — including many behind one NAT — never create an entry at all.
/// - **spent-out entries carry no information.** A refilled bucket admits exactly like an
///   absent one, so sweeping is removal of nothing rather than a choice of victim.
/// - **at capacity it stops tracking, never stops admitting.** Refusing on a full table would
///   hand an adversary a way to turn this defence into the outage it exists to prevent: fill
///   the table from throwaway sources, and every honest peer is refused. Declining to *charge*
///   a source degrades the rate bound toward its pre-#2549 behaviour for as long as the
///   pressure lasts, which is the failure this is willing to have.
#[derive(Debug)]
pub struct PreAuthAdmission {
    state: Mutex<AdmissionState>,
    max_total: usize,
    max_per_source: usize,
    authentication_deadline: Duration,
    /// Admissions a source may abandon unauthenticated before it is refused (#2549).
    churn_budget: f64,
    /// How long an exhausted [`Self::churn_budget`] takes to refill completely.
    churn_window: Duration,
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

    /// An admission table with explicit counts and deadline, and the default churn budget.
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
        Self::with_churn_policy(
            max_total,
            max_per_source,
            authentication_deadline,
            PREAUTH_CHURN_BUDGET_PER_SOURCE,
            PREAUTH_CHURN_REFILL_WINDOW,
        )
    }

    /// An admission table with every bound stated explicitly, rate included.
    ///
    /// The third half of the same policy (#2549). The counts say how much anonymous
    /// concurrency may exist, the deadline says for how long, and this says how fast a source
    /// may keep taking fresh allowances after abandoning the last one. All three live on one
    /// object because an adversary reads them as one number.
    ///
    /// Exists in this shape so tests can drive the rate boundary without waiting out a
    /// thirty-second window.
    pub fn with_churn_policy(
        max_total: usize,
        max_per_source: usize,
        authentication_deadline: Duration,
        churn_budget: usize,
        churn_window: Duration,
    ) -> Self {
        Self {
            state: Mutex::new(AdmissionState::default()),
            max_total,
            max_per_source,
            authentication_deadline,
            churn_budget: churn_budget as f64,
            churn_window,
        }
    }

    /// Tokens returned to a source's churn budget per second.
    ///
    /// Zero if the window is zero — a degenerate configuration that must not divide by zero
    /// and must not silently become an infinite allowance.
    fn churn_refill_per_sec(&self) -> f64 {
        let window = self.churn_window.as_secs_f64();
        if window <= 0.0 {
            0.0
        } else {
            self.churn_budget / window
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
        self.try_admit_at(addr, Instant::now())
    }

    /// [`Self::try_admit`] with the current instant supplied by the caller.
    ///
    /// Exists so the churn bound's refill can be asserted at an exact point on the curve
    /// rather than slept towards. A rate limiter tested by sleeping is tested on the machine
    /// that ran it; this lets the same assertion mean the same thing everywhere.
    ///
    /// Production calls [`Self::try_admit`], which reads the clock itself. Passing a `now`
    /// that runs backwards cannot manufacture allowance — refill saturates at zero elapsed.
    pub fn try_admit_at(
        self: &Arc<Self>,
        addr: SocketAddr,
        now: Instant,
    ) -> Result<AdmissionGuard, AdmissionRefusal> {
        let key = SourceKey::from_addr(addr);
        // Read before the slot is taken, so the deadline can only ever be *shorter* than the
        // slot's real life, never longer. The two are created by one operation, which is what
        // lets the guard state its own expiry rather than the caller guessing when it started.
        let authenticate_by = now + self.authentication_deadline;
        let cardinality = {
            let mut state = self.lock_state();

            if state.total >= self.max_total {
                return Err(AdmissionRefusal::GlobalLimit);
            }

            // The rate bound is checked before the count bound is *taken* but after the count
            // bound is checked, so a refused-for-churn source is never charged concurrency it
            // did not receive. Reading it here — before any of the work above this hook is
            // started — is what makes it bound that work rather than describe it.
            if !state.churn_admits(key, self.churn_budget, self.churn_refill_per_sec(), now) {
                return Err(AdmissionRefusal::SourceChurn);
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
            // Unauthenticated until something proves otherwise. The default is the charged
            // one on purpose: a release path that forgets to mark itself over-charges a
            // source, which costs an honest peer a retry, where the opposite default would
            // silently un-bound the thing this exists to bound.
            authenticated: false,
            admitted_at: now,
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

    /// Number of sources whose churn history is currently retained (#2549).
    ///
    /// The churn map's cardinality. Exposed for the same reason as [`Self::tracked_sources`]:
    /// a bound on a map that outlives its connections is worth nothing if it can only be
    /// argued about, and this is the one table here whose size is capped by policy rather than
    /// pinned by construction.
    pub fn tracked_churn_sources(&self) -> usize {
        self.lock_state().churn.len()
    }

    /// Whole admissions `addr`'s source may still abandon before it is refused (#2549).
    ///
    /// Read-only: unlike [`Self::try_admit`] this neither creates, charges, nor sweeps an
    /// entry, so observing a source cannot change what happens to it. A source with no entry
    /// reports the full budget, which is what it would in fact receive.
    pub fn churn_budget_remaining(&self, addr: SocketAddr) -> usize {
        let key = SourceKey::from_addr(addr);
        let now = Instant::now();
        let state = self.lock_state();
        match state.churn.get(&key) {
            Some(bucket) => {
                let mut bucket = *bucket;
                bucket.refill(self.churn_budget, self.churn_refill_per_sec(), now);
                bucket.tokens.max(0.0) as usize
            }
            None => self.churn_budget as usize,
        }
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
    ///
    /// `authenticated` decides whether the source is also charged for churn (#2549). It is the
    /// distinction that keeps the rate bound off ordinary peers: a connection that said who it
    /// was is not churn however briefly it lived, so a NAT full of peers reconnecting — the
    /// case #2549 must not break — costs nothing at all.
    ///
    /// The charge is stamped `admitted_at` rather than now, because the quantity being
    /// rate-limited is *taking an allowance*, and that happened when the slot was taken. Two
    /// consequences, both wanted. Holding a connection open cannot postpone the charge's ageing,
    /// so squatting to the deadline earns no rate credit over releasing at once — and equally
    /// cannot *accumulate* it, so a source at the concurrency ceiling is never also refused for
    /// churn. And the whole accounting then runs on the clock supplied to
    /// [`Self::try_admit_at`], so nothing here reads a second clock that a test could not
    /// control or a slow drop could skew.
    fn release(&self, key: SourceKey, authenticated: bool, admitted_at: Instant) {
        let cardinality = {
            let mut state = self.lock_state();
            state.total = state.total.saturating_sub(1);
            if !authenticated {
                state.charge_churn(
                    key,
                    self.churn_budget,
                    self.churn_refill_per_sec(),
                    MAX_PREAUTH_CHURN_SOURCES,
                    admitted_at,
                );
            }
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
    /// Whether the connection holding this slot ever proved an identity (#2549).
    ///
    /// Set only by [`Self::release_authenticated`], and read only by the destructor, to decide
    /// whether the source is charged for churn. It is a property of the *release*, not of the
    /// connection: the question the churn bound asks is "did this admission end in a peer, or
    /// in nothing?".
    authenticated: bool,
    /// The instant this slot was taken, and the date the churn charge carries (#2549).
    ///
    /// See [`PreAuthAdmission::release`] on why the charge is stamped here rather than read
    /// from the clock when the guard drops.
    admitted_at: Instant,
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

    /// Give the slot back for a connection that authenticated, without charging churn (#2549).
    ///
    /// **Call this only where an identity has actually been proven against the connection** —
    /// in practice `ConnectionContext::record_authenticated_peer`, which runs only after the
    /// DID-TLS binding checks. Calling it anywhere else would let an unauthenticated
    /// connection buy its way out of the rate bound, which is the whole of what the bound
    /// counts.
    ///
    /// Consuming `self` is deliberate: the release *is* the drop, so there is no window in
    /// which a guard sits marked-but-unreleased, and no second call to get wrong.
    pub fn release_authenticated(mut self) {
        self.authenticated = true;
        drop(self);
    }
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        self.admission
            .release(self.key, self.authenticated, self.admitted_at);
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
