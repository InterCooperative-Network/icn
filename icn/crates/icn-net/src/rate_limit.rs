//! Rate limiting for network messages
//!
//! Implements a token bucket algorithm for per-peer rate limiting to prevent DoS attacks.
//! Supports policy-based rate limiting where limits are determined by PolicyOracle.
//!
//! ## Sybil Resistance (Issue #675)
//!
//! In addition to per-DID rate limiting, this module supports per-anchor (per-person)
//! rate limiting to prevent Sybil attacks where an attacker creates multiple DIDs
//! to bypass aggregate rate limits. When enabled, all DIDs belonging to the same
//! PersonhoodAnchor share a single rate limit bucket.

use crate::preauth_admission::SourceKey;
use icn_identity::{Did, PersonhoodStoreTrait};
use icn_kernel_api::authz::{
    ActionKind, ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest,
};

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Wrapper for lazy hex encoding - only encodes when Display is called.
///
/// This avoids computing hex strings when log levels would filter out the message.
struct HexDisplay<'a>(&'a [u8; 32]);

impl fmt::Display for HexDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

const NETWORK_MESSAGE_ACTION: &str = "network_message";

/// Policy domain under which the network layer requests per-peer constraints.
///
/// The composition root MUST register a `PolicyOracle` for this domain. Once the
/// `OracleRegistry` reaches `BootstrapPhase::Running`, an unregistered domain is
/// denied by default, and [`RateLimiter::check_rate_limit`] reports that denial
/// the same way it reports a token-bucket rejection — so a missing registration
/// silently drops every inbound message. Exported so the registration site and
/// this query site share one symbol instead of two string literals that can drift.
pub const NETWORK_DOMAIN: &str = "net";

/// Configuration for rate limiting
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum messages per second per peer
    pub max_messages_per_second: u32,

    /// Bucket capacity (allows bursts up to this many messages)
    pub burst_capacity: u32,

    /// How often to refill tokens (default: 100ms)
    pub refill_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            max_messages_per_second: 100, // 100 msgs/sec = reasonable for gossip
            burst_capacity: 20,           // Allow bursts of 20 messages
            refill_interval: Duration::from_millis(100), // Refill every 100ms
        }
    }
}

/// Enforcement mode for per-person (per-anchor) rate limiting
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum EnforcementMode {
    /// Log violations but allow messages through (for gradual rollout)
    LogOnly,
    /// Reject messages that exceed the per-person limit
    #[default]
    Enforce,
    /// Reject ALL messages from DIDs without personhood anchors
    RequirePersonhood,
}

impl EnforcementMode {
    /// Parse from string (for config file)
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "log_only" | "logonly" => EnforcementMode::LogOnly,
            "require_personhood" | "requirepersonhood" => EnforcementMode::RequirePersonhood,
            _ => EnforcementMode::Enforce,
        }
    }
}

/// Configuration for per-anchor (per-person) rate limiting
///
/// This provides Sybil resistance by aggregating rate limits across all DIDs
/// belonging to the same PersonhoodAnchor.
#[derive(Clone, Debug)]
pub struct AnchorRateLimitConfig {
    /// Maximum messages per second per person (across all their DIDs)
    pub max_messages_per_person_per_second: u32,

    /// Bucket capacity for per-person rate limiting (allows bursts)
    pub person_burst_capacity: u32,

    /// How to handle rate limit violations
    pub enforcement_mode: EnforcementMode,

    /// Multiplier for verified personhood (e.g., 2.0 = verified persons get 2x limit)
    /// Applied when the anchor has POPLevel::Verified or higher
    pub verified_multiplier: f64,

    /// Refill interval (shared with per-DID)
    pub refill_interval: Duration,
}

impl Default for AnchorRateLimitConfig {
    fn default() -> Self {
        AnchorRateLimitConfig {
            max_messages_per_person_per_second: 500,
            person_burst_capacity: 100,
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 2.0,
            refill_interval: Duration::from_millis(100),
        }
    }
}

/// Tokens added per refill interval for a configured rate.
///
/// # Why this is not floored at one token (#2503)
///
/// This used to end in `.max(1.0)`, forcing at least one token per interval.
/// With the default 100 ms interval that is 10 messages/s, so **every** configured
/// rate below 10/s was silently raised to 10/s — including a configured `0`, which
/// reads as "deny sustained traffic" and instead produced the floor. An operator
/// tightening a pre-authentication DoS cap got 10x what they asked for, with no
/// warning. That made #2496's guarantee — operator-configured limits control
/// behaviour — untrue at the last hop, however correct the policy mapping was.
///
/// Fractional rates need no special handling: [`TokenBucket::tokens`] is `f64` and
/// [`TokenBucket::refill`] adds `intervals * refill_rate` where `intervals` is
/// itself fractional, so 0.1 tokens per 100 ms accumulates to one token per second.
/// Removing the floor is therefore sufficient — the accumulation machinery was
/// already correct, and the identity
///
/// ```text
/// intervals * refill_rate
///   == (elapsed / interval) * (rate * interval)
///   == elapsed * rate
/// ```
///
/// means the existing interval mechanism already computes exactly
/// "elapsed time x configured rate", with no additional quantisation.
///
/// A configured rate of `0` now yields `0`, so the bucket never replenishes and
/// burst is a one-time allowance. That is the honest reading of the number.
fn refill_rate_per_interval(config: &RateLimitConfig) -> f64 {
    config.max_messages_per_second as f64 * config.refill_interval.as_secs_f64()
}

/// Token bucket for a single peer
#[derive(Debug)]
struct TokenBucket {
    /// Current number of tokens
    tokens: f64,

    /// Maximum tokens (burst capacity)
    capacity: f64,

    /// Tokens added per refill
    refill_rate: f64,

    /// Last refill timestamp
    last_refill: Instant,

    /// Refill interval
    refill_interval: Duration,
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64, refill_interval: Duration) -> Self {
        TokenBucket {
            tokens: capacity, // Start with full bucket
            capacity,
            refill_rate,
            last_refill: Instant::now(),
            refill_interval,
        }
    }

    /// Tolerance for floating point comparison (1e-6 is appropriate for user-configured values)
    const CONFIG_EPSILON: f64 = 1e-6;

    /// Update bucket configuration with proportional refill
    fn update_config(&mut self, new_capacity: f64, new_refill_rate: f64) -> bool {
        // Check if config changed using appropriate tolerance
        let changed = (self.capacity - new_capacity).abs() > Self::CONFIG_EPSILON
            || (self.refill_rate - new_refill_rate).abs() > Self::CONFIG_EPSILON;

        if changed {
            // Proportional refill: scale tokens by the capacity ratio to prevent
            // exploitation via rapid trust cycling (repeatedly getting full buckets)
            // Token bucket invariant: capacity should never be < 1.0 token
            const MIN_CAPACITY_FOR_RATIO: f64 = 1.0;
            let ratio = new_capacity / self.capacity.max(MIN_CAPACITY_FOR_RATIO);
            self.tokens = (self.tokens * ratio).min(new_capacity);
            self.capacity = new_capacity;
            self.refill_rate = new_refill_rate;
            self.last_refill = Instant::now();
        }
        changed
    }

    /// Try to consume a token. Returns true if allowed, false if rate limited.
    fn try_consume(&mut self) -> bool {
        // Refill tokens based on elapsed time
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false // Rate limited
        }
    }

    /// Whether this bucket has come all the way back to capacity.
    ///
    /// Takes `&mut self` because the answer is only current after a refill. A bucket that is full
    /// grants exactly what a newly built one grants, which is what lets a table of these drop
    /// entries instead of evicting them (see [`SourcePreAuthBudget::spend`]).
    fn is_replenished(&mut self) -> bool {
        self.refill();
        self.tokens >= self.capacity
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        // Calculate how many refill intervals have passed
        let intervals = elapsed.as_secs_f64() / self.refill_interval.as_secs_f64();

        if intervals >= 1.0 {
            // Add tokens proportional to intervals passed
            let tokens_to_add = intervals * self.refill_rate;
            self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
            self.last_refill = now;
        }
    }
}

/// What a connection may spend before it has authenticated anybody (#2491).
///
/// # Why this is a constant and not a trust tier
///
/// Every other limit in this module is selected by trust class, from operator
/// configuration. This one cannot be, and the reason is the whole of #2491: selecting a
/// tier requires knowing *who* the peer is, and before the Hello binding runs
/// (`handlers::hello`) nobody knows. The only DID available is `NetworkMessage.from`,
/// which the sender chose. Consulting trust for it is exactly the defect.
///
/// Nor is it any of the four configured tiers. `isolated` is the tempting one — it is the
/// smallest — but it means "a peer we have classified and found unconnected", which is a
/// statement about a known identity. "We do not know who this is" is a different claim, and
/// giving it a trust class would assert something the node cannot support. The tiers are
/// also not required to be ordered (see [`NetworkRateLimitTiers::ceiling`] in
/// `icn-core`), so `min` over them is not a policy either.
///
/// # Where the numbers come from
///
/// This budget exists to fund *one thing*: reaching authentication. That is a property of
/// the Hello protocol, not of an operator's trust posture, which is why it lives here
/// rather than in configuration.
///
/// What the protocol fixes, though, is a *range* rather than a pair of values. It says the
/// budget must be nonzero, because a node that cannot afford a Hello cannot join; that it
/// can be small, because an unauthenticated connection has nothing legitimate to do besides
/// authenticate; and that it needs deterministic headroom above the one-Hello floor, because
/// the cost of being wrong in that direction is a silent bootstrap failure. The two
/// constants below are a conservative default chosen inside that range, deliberately
/// generous against the floor. They are **not** uniquely derived, and no invariant here
/// depends on their exact values — only on their being small, positive, and unrelated to
/// anything the sender said.
///
/// - **burst 20** — a handshake costs one Hello. Twenty leaves room for a peer that
///   interleaves other traffic before its Hello lands, for version renegotiation, and for
///   retries. Sizing this near the true cost would fail closed on the one path a node needs
///   in order to join at all, and would do it silently.
/// - **10 messages/second** — a connection that has not authenticated has no legitimate
///   sustained traffic, so this is deliberately far below every default tier except
///   `isolated`'s rate. It is not zero: a handshake that is retrying must still make
///   progress.
///
/// The burst is worth one more note, because at 20 it is *larger* than the default
/// `isolated` (2) and `known` (10) bursts and equal to `partner`'s. That is not a better
/// deal for staying anonymous, because nothing ever obliged an attacker to authenticate:
/// the comparison that matters is against what an unauthenticated connection could spend
/// *before* this change, which was the tier of whatever DID it cared to name — up to
/// `federated`'s 200/s and burst 50, and before #2490's ceiling, `unlimited`. What the
/// extra burst buys is at most twenty deserializations on a connection that still cannot
/// reach peer exchange (#2535), a personhood budget, or anything else DID-gated. Sizing it
/// down to `isolated`'s 2 would buy nothing against that and would risk a silent bootstrap
/// failure, which is the one outcome this must not have.
///
/// # What this does not do
///
/// It bounds one connection. An attacker who opens *N* connections gets *N* of these
/// budgets, because each is scoped to a `ConnectionContext`. That is connection admission /
/// source aggregation, a separate problem this deliberately does not claim to solve — see
/// the module note on [`PreAuthRateLimiter`].
pub const PRE_AUTH_RATE_LIMIT: RateLimitConfig = RateLimitConfig {
    max_messages_per_second: 10,
    burst_capacity: 20,
    refill_interval: Duration::from_millis(100),
};

/// The budget an unauthenticated connection spends, scoped to that connection.
///
/// **Inbound connections no longer rely on this alone** — it is one of *two* bounds they spend,
/// paired with [`SourcePreAuthBudget`] inside [`PreAuthBudget::Source`]. It remains the *only*
/// budget for outbound dials and handler unit tests, where there is no source to aggregate
/// against because *this node* chose the peer.
///
/// # Why the connection was the key, and why that was not enough
///
/// The key has to be something the remote end cannot choose, and on an inbound stream almost
/// nothing qualifies. `NetworkMessage.from` is chosen by the sender outright. The connection
/// itself has neither problem: it is created by *this* node when a handshake completes, nothing
/// in any message influences which one a byte lands on, and it is the resource actually being
/// consumed. Rotating the claimed DID buys nothing — every message on one connection meets the
/// same bucket, whatever name it carries. All of that still holds.
///
/// What it missed is that the peer decides how many connections exist. One connection, one
/// budget makes *closing* the connection a way to discard the spent budget and be issued a
/// fresh one, so a source's aggregate was bounded by nothing at all (#2549).
///
/// Keying inbound traffic on the source instead costs exactly what this doc originally said it
/// would: the remote IP is shared, so one bucket per source charges every peer behind a NAT for
/// its noisiest neighbour, and it needs a map with an expiry policy. Both prices are paid
/// deliberately and are documented on [`SourcePreAuthBudget`] — the port is dropped and IPv6 is
/// aggregated by /64 precisely so that a new source port is *not* a new key, and the burst is
/// sized at the node's whole simultaneous allowance rather than one connection's so that a NAT
/// full of honest peers still fits.
///
/// # Scope
///
/// One connection, one budget. *N* connections is *N* budgets — which is why inbound traffic
/// cannot be bounded by this *alone*: closing and reopening mints a fresh burst, so a source's
/// aggregate would be bounded by nothing. Inbound therefore composes it with the source-scoped
/// budget, which the reconnect does not reset.
#[derive(Debug)]
pub struct PreAuthRateLimiter {
    bucket: RwLock<TokenBucket>,
}

impl PreAuthRateLimiter {
    /// A fresh budget for a newly established connection.
    pub fn new() -> Self {
        Self {
            bucket: RwLock::new(TokenBucket::new(
                PRE_AUTH_RATE_LIMIT.burst_capacity as f64,
                refill_rate_per_interval(&PRE_AUTH_RATE_LIMIT),
                PRE_AUTH_RATE_LIMIT.refill_interval,
            )),
        }
    }

    /// Spend one message's worth of the connection's anonymous budget.
    ///
    /// Takes no DID, and that is the point: there is nothing to pass. A signature here
    /// would be an invitation to key this on a claim.
    pub async fn check(&self) -> bool {
        self.bucket.write().await.try_consume()
    }
}

impl Default for PreAuthRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// What one source may spend on anonymous messages, however many connections it opens (#2549).
///
/// # The resource
///
/// Every message dispatched while a connection has not authenticated may be a Hello, and a Hello
/// costs a DID parse, a `BindingInfo` signature verification, a comparison against the
/// certificate this connection is actually using (#2520), and — when the peer offers one — an
/// ML-DSA binding verification. That is the most expensive thing an unauthenticated peer can make
/// this node do, and [`PreAuthRateLimiter`] bounds it *per connection*, which is not a bound at
/// all against a peer willing to open another one: closing and reconnecting minted a fresh burst,
/// so the aggregate was limited only by how fast a source could complete handshakes.
///
/// So the bucket is keyed by source and outlives the connections that spend from it. A reconnect
/// meets the allowance it already spent.
///
/// # Why authentication does not buy an exemption
///
/// It cannot, because there is nothing to exempt: a token is spent when the work happens and is
/// never returned. The Hello that authenticates is itself charged — the gate reads the
/// connection's authenticated peer *before* dispatching, and for that Hello it is still `None`.
///
/// This matters more than it looks. Hello authentication is not scarce: a peer can self-mint an
/// Ed25519 DID, a TLS certificate, and a binding of the one over the other, with no allowlist and
/// no external trust gate. Any design that charged abandoned connections and forgave authenticated
/// ones would therefore be selling exemptions for the price of a keypair. Charging the work itself
/// is the only discriminator an attacker cannot buy.
///
/// A side effect worth naming, with its limit: acquiring a fresh DID-keyed post-authentication
/// bucket costs one anonymous dispatch **on a connection that is not yet authenticated**, so the
/// rate at which one source can mint fresh identities *that way* is bounded by this too.
///
/// It does **not** bound identity rotation on a connection that has already authenticated. This
/// gate runs only while `authenticated_peer` is `None`; a later Hello on the same connection is
/// charged to the currently authenticated DID's bucket, and `record_authenticated_peer` then
/// overwrites the identity unconditionally. So a peer can walk A → B → C on one established
/// connection, minting a per-DID bucket each time, without spending another source token. That is
/// the writable-identity property #2556 exists to remove, not something this budget can reach —
/// the source key is not consulted after authentication by design, because post-authentication
/// traffic is bounded per DID and per personhood anchor instead.
///
/// # What this does not bound
///
/// The QUIC/TLS handshake (#2559), the `ConnectionContext` and task a connection allocates, and
/// the *reading* of a frame that is then denied here — all of that happens before or outside this
/// gate. **Deserializing** that frame does not: #2558 moved this gate to sit between frame
/// acquisition and decode, so one token buys the decode as well as the dispatch it feeds. See the
/// module note on [`SourcePreAuthBudget::spend`].
///
/// # The NAT price, stated rather than buried
///
/// One bucket per source is one bucket per NAT, so a noisy peer behind a shared address can spend
/// its neighbours' allowance. That cost is paid deliberately and sized for: the burst is the full
/// simultaneous allowance the node already grants a source, not one connection's worth, and an
/// exhausted budget throttles a *message* rather than refusing a connection — a peer needs one
/// token to authenticate and waits under a fifth of a second for it.
pub const PREAUTH_SOURCE_BURST: u32 = crate::preauth_admission::MAX_PREAUTH_CONNECTIONS_PER_SOURCE
    as u32
    * PRE_AUTH_RATE_LIMIT.burst_capacity;

/// How long a source takes to renew its whole anonymous allowance.
///
/// The burst above is what the node already permits one source to hold *at once*:
/// [`crate::preauth_admission::MAX_PREAUTH_CONNECTIONS_PER_SOURCE`] concurrent anonymous
/// connections (#2547), each with [`PRE_AUTH_RATE_LIMIT`]'s burst (#2491). Restating it as a
/// shared burst is deliberately non-regressive — no instantaneous capability any source has today
/// is taken away. What changes is only how fast that allowance comes back, which is exactly the
/// defect #2549 filed.
///
/// The renewal period is the authentication deadline (#2552): a source that occupies all of its
/// concurrent slots for the full deadline and spends every message it is allowed sits exactly at
/// this rate, so it is the fastest legitimate steady state the existing constants describe.
///
/// These numbers are composed from three constants this repository already commits to. The
/// composition is a policy choice and is not uniquely derived — nothing here depends on the exact
/// values, only on the burst being the existing simultaneous maximum and the period being the
/// existing connection lifetime.
pub const PREAUTH_SOURCE_RENEWAL_WINDOW: Duration =
    crate::preauth_admission::PREAUTH_AUTHENTICATION_DEADLINE;

/// How many sources may be tracked individually before the table degrades.
///
/// A **chosen memory budget**, not a derived bound: at roughly 64 bytes an entry this is a
/// quarter of a megabyte. Its value decides *when* [`SourcePreAuthBudget`] degrades, never
/// whether the aggregate stays bounded — see [`SourcePreAuthBudget::spend`].
pub const MAX_PREAUTH_BUDGET_SOURCES: usize = 4096;

/// Least time between sweeps of replenished entries.
///
/// A sweep is O(tracked sources) and only ever runs on the miss path of a full table, so this
/// keeps a stream of misses against an unreclaimable table from walking it every time. Blocking a
/// sweep is safe: the caller falls through to the shared budget, which is bounded.
const BUDGET_SWEEP_MIN_INTERVAL: Duration = Duration::from_secs(1);

/// Tokens added per refill interval to renew `PREAUTH_SOURCE_BURST` over the renewal window.
fn source_refill_rate_per_interval() -> f64 {
    PREAUTH_SOURCE_BURST as f64 * PRE_AUTH_RATE_LIMIT.refill_interval.as_secs_f64()
        / PREAUTH_SOURCE_RENEWAL_WINDOW.as_secs_f64()
}

/// The anonymous-message allowance of every source this endpoint has heard from recently.
///
/// One of these per endpoint, created with the accept loop and shared by every inbound
/// connection.
#[derive(Debug)]
pub struct SourcePreAuthBudget {
    state: std::sync::Mutex<BudgetState>,
}

#[derive(Debug)]
struct BudgetState {
    /// Sources this table is currently holding an allowance for.
    ///
    /// A replenished bucket is indistinguishable from an absent one, which is what keeps this
    /// bounded without an eviction policy — see [`SourcePreAuthBudget::spend`]. Reclamation is
    /// *lazy*: a refilled entry becomes eligible to be dropped but is only actually dropped by
    /// [`Self::sweep_replenished`], which runs on the miss path of a full table. Below capacity
    /// nothing sweeps, so this holds every source seen since startup, refilled or not — bounded
    /// by `max_sources` either way, which is all it has to be.
    per_source: HashMap<SourceKey, TokenBucket>,
    /// What every source the table had no room to track shares.
    shared: TokenBucket,
    max_sources: usize,
    /// When the last sweep ran, or `None` if none ever has.
    ///
    /// `None` rather than "construction time": the throttle exists so a *stream* of misses
    /// against an unreclaimable table does not walk it every time, and the first miss has no
    /// previous sweep to be too close to. Starting the clock at construction made the one miss
    /// with the most to reclaim — the first time the table fills — the only miss that could not
    /// sweep, which is also the opposite of what the docs above promise.
    last_sweep: Option<Instant>,
    capacity: f64,
    refill_rate: f64,
    refill_interval: Duration,
}

impl BudgetState {
    fn fresh_bucket(&self) -> TokenBucket {
        TokenBucket::new(self.capacity, self.refill_rate, self.refill_interval)
    }

    /// Drop every entry whose allowance has come all the way back.
    ///
    /// Returns the number removed. Rate-limited by [`BUDGET_SWEEP_MIN_INTERVAL`]; a refused sweep
    /// reports zero rather than pretending to have run.
    fn sweep_replenished(&mut self) -> usize {
        let now = Instant::now();
        if let Some(last) = self.last_sweep {
            if now.duration_since(last) < BUDGET_SWEEP_MIN_INTERVAL {
                return 0;
            }
        }
        self.last_sweep = Some(now);
        let before = self.per_source.len();
        self.per_source.retain(|_, bucket| !bucket.is_replenished());
        icn_obs::metrics::network::preauth_source_budget_tracked_set(self.per_source.len());
        before - self.per_source.len()
    }
}

impl SourcePreAuthBudget {
    /// The allowance table this endpoint's accept loop hands to its connections.
    pub fn new() -> Self {
        Self::with_policy(
            PREAUTH_SOURCE_BURST as f64,
            source_refill_rate_per_interval(),
            PRE_AUTH_RATE_LIMIT.refill_interval,
            MAX_PREAUTH_BUDGET_SOURCES,
        )
    }

    /// A table with explicit policy, so tests can state a bound instead of waiting for one.
    pub fn with_policy(
        capacity: f64,
        refill_rate: f64,
        refill_interval: Duration,
        max_sources: usize,
    ) -> Self {
        Self {
            state: std::sync::Mutex::new(BudgetState {
                per_source: HashMap::new(),
                shared: TokenBucket::new(capacity, refill_rate, refill_interval),
                max_sources,
                last_sweep: None,
                capacity,
                refill_rate,
                refill_interval,
            }),
        }
    }

    /// Spend one anonymous message against `key`'s allowance.
    ///
    /// # What this bounds
    ///
    /// Over any window of length *T*, one source's anonymous **dispatches** are at most
    /// `PREAUTH_SOURCE_BURST + rate * T`. Not `PREAUTH_SOURCE_BURST` per window: a token bucket
    /// permits a full burst at the start of a window and another once it has refilled, so the
    /// sliding-window reading of it is wrong by up to a factor of two.
    ///
    /// One exception, in the saturated table's promotion path only — see *The promotion transition
    /// is the one place the per-source burst is not exact* below, and #2562.
    ///
    /// It does **not** bound the QUIC/TLS handshake, which is complete before the source key
    /// exists at all, nor the `ConnectionContext` and task a connection allocates, nor reading and
    /// deserializing a message that is then denied here — that read happens before this gate, and
    /// one held connection can force it without reconnecting even once.
    ///
    /// # Bounded state, and what happens when it runs out
    ///
    /// An entry is reclaimable once the source's allowance is back to full, because a replenished
    /// bucket grants exactly what a fresh one does. Entries therefore **expire by refilling**, and
    /// are never evicted while live. That is the security property, not merely a memory one: an
    /// attacker cannot evict a victim's entry to reset it, and cannot evict its own — its entry is
    /// below full precisely because it is spending, and a full one would buy nothing.
    ///
    /// Reclamation is lazy, and deliberately so. Refilling makes an entry *eligible* to be
    /// dropped; [`BudgetState::sweep_replenished`] is what actually drops it, and it runs only on
    /// the miss path of a full table. Below capacity nothing sweeps, so the table simply retains
    /// every source it has seen. That costs a bounded amount of memory and buys the absence of a
    /// timer, and it cannot loosen the bound in either direction: a retained-but-refilled entry
    /// grants exactly the burst a fresh one would.
    ///
    /// When the table is full and a sweep frees nothing, an untracked source spends from a single
    /// **shared** bucket with one source's burst and rate. So the protection degrades from "each
    /// source is bounded" to "every untracked source is bounded *together*, by one source's
    /// worth" — never to "unbounded".
    ///
    /// # The promotion transition is the one place the per-source burst is not exact
    ///
    /// Stated because it is a real exception to the headline bound (#2562 tracks it). A source
    /// that spends from the shared bucket while untracked and is *then* promoted — a later sweep
    /// frees a slot — is inserted with [`BudgetState::fresh_bucket`], a full one. Its shared
    /// spending is not carried across, so across that transition its burst term can reach `2B`
    /// rather than `B`, once, before settling back to `B + rate × T`.
    ///
    /// It is not fixed here because carrying the debt means per-source accounting for sources the
    /// table has *no room to track* — the exact state the fallback exists to avoid. Bounding it by
    /// seeding the promoted bucket from the shared level would instead charge a newly-arrived
    /// honest source for its neighbours' spending.
    ///
    /// What survives unchanged is the anti-gaming argument, which is an aggregate one: the extra
    /// is drawn from the shared bucket, and there is only one of those, so *all* promoted sources
    /// together can gain at most its throughput. Filling the table costs `max_sources` distinct
    /// keys and leaves every one of the attacker's sources with less than being tracked would have
    /// given it. So there is still nothing to gain by filling the table — but "being untracked is
    /// never better than being tracked", which this doc used to claim outright, is false at the
    /// moment of promotion.
    pub fn spend(&self, key: SourceKey) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(bucket) = state.per_source.get_mut(&key) {
            return bucket.try_consume();
        }

        if state.per_source.len() >= state.max_sources {
            state.sweep_replenished();
        }
        if state.per_source.len() >= state.max_sources {
            icn_obs::metrics::network::preauth_source_budget_degraded_inc();
            return state.shared.try_consume();
        }

        let mut bucket = state.fresh_bucket();
        let allowed = bucket.try_consume();
        state.per_source.insert(key, bucket);
        // Published from the two places the table can change size — here and the sweep above —
        // rather than from the caller, so no exit path can leave it stale.
        icn_obs::metrics::network::preauth_source_budget_tracked_set(state.per_source.len());
        allowed
    }

    /// Rewind every bucket so the table believes `d` has elapsed.
    ///
    /// Private and test-only on purpose. Production accounting reads `Instant::now()` inside
    /// [`TokenBucket::refill`] and takes no timestamp from any caller, which is what makes a
    /// clock regression unrepresentable rather than merely unlikely; exposing a
    /// caller-supplied instant to make tests deterministic would put back exactly the seam
    /// that property removes.
    #[cfg(test)]
    fn advance(&self, d: Duration) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let rewind = |at: &mut Instant| {
            *at = at
                .checked_sub(d)
                .expect("test durations stay within Instant range");
        };
        for bucket in state.per_source.values_mut() {
            rewind(&mut bucket.last_refill);
        }
        rewind(&mut state.shared.last_refill);
        if let Some(last_sweep) = state.last_sweep.as_mut() {
            rewind(last_sweep);
        }
    }

    /// How many sources this table currently holds an allowance for.
    ///
    /// An upper bound on "sources below full", not a count of them: reclamation is lazy, so a
    /// source that has refilled stays counted until a sweep needs its slot. See
    /// [`Self::spend`].
    pub fn tracked_sources(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .per_source
            .len()
    }
}

impl Default for SourcePreAuthBudget {
    fn default() -> Self {
        Self::new()
    }
}

/// Where a connection's anonymous messages are charged.
///
/// Two arms because the question "which source is this" has an answer for exactly one of them.
#[derive(Debug)]
pub enum PreAuthBudget {
    /// Inbound: two bounds, because they bound two different things.
    ///
    /// The per-connection bucket caps what *one session* may burst (#2491); the source bucket
    /// caps what one source may spend in aggregate over time, across every connection it opens
    /// (#2549). Dropping the first in favour of the second would not have been a redesign but a
    /// trade: it would have let a single fresh connection spend the whole source allowance at
    /// once, eight times what #2491 permits it today.
    Source {
        /// What this one connection may burst, unchanged from #2491.
        connection: PreAuthRateLimiter,
        /// The endpoint's allowance table.
        budget: Arc<SourcePreAuthBudget>,
        /// Which source this connection is charged to.
        key: SourceKey,
    },
    /// Outbound dials and handler unit tests.
    ///
    /// *This node* chose the peer, so there is no *reconnect* churn to aggregate: closing an
    /// inbound connection cannot make this node dial. The per-connection budget is the right one
    /// here, and it is the same one `admission_guard` is `None` for.
    ///
    /// "This node chose the peer" is narrower than it sounds, and the gap is deliberate rather
    /// than overlooked. With peer exchange enabled an authenticated peer can influence *which*
    /// endpoints this node dials — `supervisor::init_network` auto-dials discovered peers, and
    /// already caps them for exactly that reason — so an induced dial does get a fresh
    /// per-connection burst without touching any source budget.
    ///
    /// Charging those to the target's source budget is not obviously an improvement: it would let
    /// a peer that can induce dials at an address drain *that address's* inbound allowance, which
    /// converts an outbound-churn concern into a remote denial-of-service against the victim.
    /// Bounding induced dials belongs with the dial cap and the trust gate on peer exchange, not
    /// with a bucket keyed on the address being dialled.
    Connection(PreAuthRateLimiter),
}

impl PreAuthBudget {
    /// Spend one message's worth of every allowance this connection draws on.
    ///
    /// The connection's own bucket is asked first, so a connection that has exhausted its burst
    /// never reaches the shared lock. If it permits and the source does not, the connection has
    /// still spent a token on a message that was never dispatched. That is deliberate: it can
    /// only ever make the bound *tighter* than stated, never looser, and the alternative — a
    /// peek across two locks followed by a commit — buys nothing for a message that is being
    /// dropped either way.
    pub async fn check(&self) -> bool {
        match self {
            Self::Source {
                connection,
                budget,
                key,
            } => connection.check().await && budget.spend(*key),
            Self::Connection(limiter) => limiter.check().await,
        }
    }
}

/// Per-peer rate limiter using token bucket algorithm
///
/// Supports two layers of rate limiting:
/// 1. Per-DID: Policy-based rate limiting via PolicyOracle
/// 2. Per-anchor (Sybil resistance): Aggregate limits across DIDs sharing same PersonhoodAnchor
///
/// Both layers key on a DID, so both are usable **only after** that DID has been
/// authenticated against the connection carrying the message (#2491). Before that, see
/// [`PreAuthRateLimiter`].
pub struct RateLimiter {
    /// Policy oracle for determining rate limits (optional)
    oracle: Option<Arc<dyn PolicyOracle>>,

    /// Fallback configuration (used when oracle is not provided)
    fallback_config: RateLimitConfig,

    /// Token buckets per peer DID
    buckets: Arc<RwLock<HashMap<Did, TokenBucket>>>,

    // --- Sybil resistance (Issue #675) ---
    /// Per-anchor (per-person) token buckets for Sybil resistance
    anchor_buckets: Arc<RwLock<HashMap<[u8; 32], TokenBucket>>>,

    /// Personhood store for DID-to-anchor lookups
    personhood_store: Option<Arc<dyn PersonhoodStoreTrait>>,

    /// Per-anchor rate limit configuration
    anchor_rate_config: Option<AnchorRateLimitConfig>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given fallback configuration
    pub fn new(config: RateLimitConfig) -> Self {
        RateLimiter {
            oracle: None,
            fallback_config: config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: None,
            anchor_rate_config: None,
        }
    }

    /// Create a new rate limiter with PolicyOracle
    ///
    /// The oracle is consulted on each message to determine the appropriate rate limit.
    /// If the oracle returns `PolicyDecision::Allow`, rate limits from the constraints
    /// are applied. If it returns `PolicyDecision::Deny`, messages are immediately rejected.
    ///
    /// The `fallback_config` is used when constraint extraction fails or when constraints
    /// don't specify a rate limit. This provides sensible defaults while still allowing
    /// the oracle to make authorization decisions.
    pub fn new_with_oracle(
        oracle: Arc<dyn PolicyOracle>,
        fallback_config: RateLimitConfig,
    ) -> Self {
        RateLimiter {
            oracle: Some(oracle),
            fallback_config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: None,
            anchor_rate_config: None,
        }
    }

    /// Create a new rate limiter with PolicyOracle and Sybil resistance
    ///
    /// This constructor enables per-anchor (per-person) rate limiting in addition
    /// to per-DID rate limiting, preventing Sybil attacks where an attacker
    /// creates multiple DIDs to bypass aggregate rate limits.
    pub fn new_with_oracle_and_sybil_resistance(
        oracle: Arc<dyn PolicyOracle>,
        fallback_config: RateLimitConfig,
        personhood_store: Arc<dyn PersonhoodStoreTrait>,
        anchor_config: AnchorRateLimitConfig,
    ) -> Self {
        RateLimiter {
            oracle: Some(oracle),
            fallback_config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: Some(personhood_store),
            anchor_rate_config: Some(anchor_config),
        }
    }

    /// Convert an oracle decision into a rate limit config (or None if denied).
    fn rate_limit_from_decision(
        fallback_config: &RateLimitConfig,
        decision: PolicyDecision,
    ) -> Option<RateLimitConfig> {
        match decision {
            PolicyDecision::Allow { constraints, .. } => Some(Self::rate_limit_from_constraints(
                fallback_config,
                &constraints,
            )),
            PolicyDecision::Deny { .. } => None,
        }
    }

    fn rate_limit_from_constraints(
        fallback_config: &RateLimitConfig,
        constraints: &ConstraintSet,
    ) -> RateLimitConfig {
        match &constraints.rate_limit {
            Some(rate_limit) => RateLimitConfig {
                max_messages_per_second: rate_limit.messages_per_second,
                burst_capacity: rate_limit.burst_size,
                refill_interval: fallback_config.refill_interval,
            },
            None => fallback_config.clone(),
        }
    }

    /// Enable Sybil resistance on an existing rate limiter
    pub fn with_sybil_resistance(
        mut self,
        personhood_store: Arc<dyn PersonhoodStoreTrait>,
        anchor_config: AnchorRateLimitConfig,
    ) -> Self {
        self.personhood_store = Some(personhood_store);
        self.anchor_rate_config = Some(anchor_config);
        self
    }

    /// Check if a message from the given peer should be allowed.
    /// Returns true if allowed, false if rate limited.
    ///
    /// Uses PolicyOracle to determine rate limits based on trust/policy.
    ///
    /// ## Oracle Evaluation
    ///
    /// The oracle is consulted on each message to get the current policy decision:
    /// - `PolicyDecision::Allow { constraints }`: Extract rate limit from constraints
    /// - `PolicyDecision::Deny { .. }`: Return false immediately (message rejected)
    ///
    /// ## Deny Decision Semantics
    ///
    /// When the oracle returns `Deny`, no token bucket is created for the peer.
    /// This means subsequent messages from denied peers will re-query the oracle
    /// each time. This is intentional:
    /// - Allows oracle to change its decision if circumstances change
    /// - Avoids stale cached decisions for peers whose trust status improves
    /// - Negligible overhead since denied peers should be rare in healthy networks
    ///
    /// If oracle call overhead becomes a concern, consider adding a TTL cache
    /// for Deny decisions in the future.
    pub async fn check_rate_limit(&self, peer: &Did) -> bool {
        // Get rate limit config from oracle or fallback
        let config = if let Some(oracle) = &self.oracle {
            let request = PolicyRequest::new(
                peer.to_string(),
                ActionKind::Custom(NETWORK_MESSAGE_ACTION.to_string()),
                Domain::new(NETWORK_DOMAIN),
            );

            match Self::rate_limit_from_decision(&self.fallback_config, oracle.evaluate(&request)) {
                Some(config) => config,
                None => return false,
            }
        } else {
            self.fallback_config.clone()
        };

        // LOCK ORDER INVARIANT: Always acquire `buckets` before `anchor_buckets`
        // to prevent deadlocks. Any code path that needs both locks must follow
        // this order. See also line 485 where anchor_buckets is acquired after
        // check_rate_limit completes (never holding buckets simultaneously).
        let mut buckets = self.buckets.write().await;

        // Perform rate limit check
        self.do_rate_limit_check(&mut buckets, peer, &config)
    }

    /// Internal helper to perform the actual rate limit check
    fn do_rate_limit_check(
        &self,
        buckets: &mut HashMap<Did, TokenBucket>,
        peer: &Did,
        config: &RateLimitConfig,
    ) -> bool {
        // Calculate refill rate: tokens per interval
        let refill_rate = refill_rate_per_interval(config);

        let capacity = config.burst_capacity as f64;
        let refill_interval = config.refill_interval;

        // Get or create bucket for this peer
        let bucket = buckets
            .entry(peer.clone())
            .or_insert_with(|| TokenBucket::new(capacity, refill_rate, refill_interval));

        // Update bucket config if it has changed
        let changed = bucket.update_config(capacity, refill_rate);
        if changed {
            icn_obs::metrics::network::rate_limit_config_changes_inc();
        }

        let allowed = bucket.try_consume();

        // Record rate limiting metric by rate value
        if !allowed {
            icn_obs::metrics::network::messages_rate_limited_by_rate_inc(
                config.max_messages_per_second,
            );
        }

        allowed
    }

    /// Check rate limit with Sybil resistance (per-person limiting)
    ///
    /// This method performs dual-path rate limiting:
    /// 1. Per-DID rate limit check (existing behavior)
    /// 2. Per-anchor (per-person) rate limit check (Sybil resistance)
    ///
    /// Both checks must pass for the message to be allowed.
    ///
    /// Returns `(did_allowed, anchor_allowed)`:
    /// - `(true, true)` = message allowed
    /// - `(false, _)` = rate limited by per-DID limit
    /// - `(_, false)` = rate limited by per-person limit (Sybil mitigation)
    pub async fn check_rate_limit_with_personhood(&self, peer: &Did) -> (bool, bool) {
        // Phase 1: Per-DID rate limit check (existing logic)
        let did_allowed = self.check_rate_limit(peer).await;

        // Phase 2: Per-anchor (per-person) rate limit check
        let anchor_allowed = self.check_anchor_rate_limit(peer).await;

        (did_allowed, anchor_allowed)
    }

    /// Internal helper to check per-anchor rate limit
    async fn check_anchor_rate_limit(&self, peer: &Did) -> bool {
        // If Sybil resistance is not enabled, allow everything
        let (personhood_store, anchor_config) =
            match (&self.personhood_store, &self.anchor_rate_config) {
                (Some(store), Some(config)) => (store, config),
                _ => return true, // Sybil resistance not enabled
            };

        // Look up the anchor ID for this DID
        let anchor_id = match personhood_store.get_anchor_id_for_did(peer) {
            Ok(Some(id)) => id,
            Ok(None) => {
                // No personhood anchor for this DID
                return match anchor_config.enforcement_mode {
                    EnforcementMode::RequirePersonhood => {
                        warn!(
                            did = %peer,
                            "Rejecting message: DID has no personhood anchor (RequirePersonhood mode)"
                        );
                        icn_obs::metrics::network::personhood_required_rejections_inc();
                        icn_obs::metrics::network::sybil_detection_inc(
                            icn_obs::metrics::network::SybilDetectionType::NoPersonhoodRequired,
                        );
                        false
                    }
                    EnforcementMode::LogOnly | EnforcementMode::Enforce => {
                        // DID has no anchor - fall back to per-DID limiting only
                        debug!(
                            did = %peer,
                            "DID has no personhood anchor, skipping per-person rate limit"
                        );
                        true
                    }
                };
            }
            Err(e) => {
                // Store lookup failed - graceful degradation
                // But increment metric so operators know Sybil protection is degraded
                warn!(
                    did = %peer,
                    error = %e,
                    "Failed to look up personhood anchor, falling back to per-DID limiting"
                );
                icn_obs::metrics::network::personhood_store_failures_inc();
                return true;
            }
        };

        // Check if we need to apply verified multiplier
        let (capacity, refill_rate) =
            self.get_anchor_bucket_params(peer, &anchor_id, anchor_config);

        // Acquire the anchor buckets lock
        let mut anchor_buckets = self.anchor_buckets.write().await;

        // Get or create bucket for this anchor
        let bucket = anchor_buckets.entry(anchor_id).or_insert_with(|| {
            TokenBucket::new(capacity, refill_rate, anchor_config.refill_interval)
        });

        let allowed = bucket.try_consume();

        // Update metrics
        icn_obs::metrics::network::personhood_anchors_active_set(anchor_buckets.len());

        if !allowed {
            // Per-person rate limit exceeded - this is Sybil mitigation
            icn_obs::metrics::network::messages_rate_limited_by_anchor_inc();
            icn_obs::metrics::network::sybil_detection_inc(
                icn_obs::metrics::network::SybilDetectionType::PerPersonLimit,
            );

            match anchor_config.enforcement_mode {
                EnforcementMode::LogOnly => {
                    warn!(
                        did = %peer,
                        anchor_id = %HexDisplay(&anchor_id),
                        "Per-person rate limit exceeded (LogOnly mode - allowing)"
                    );
                    return true; // Allow in LogOnly mode
                }
                EnforcementMode::Enforce | EnforcementMode::RequirePersonhood => {
                    debug!(
                        did = %peer,
                        anchor_id = %HexDisplay(&anchor_id),
                        "Per-person rate limit exceeded"
                    );
                    return false;
                }
            }
        }

        true
    }

    /// Calculate bucket parameters, applying verified multiplier if applicable
    fn get_anchor_bucket_params(
        &self,
        peer: &Did,
        anchor_id: &[u8; 32],
        config: &AnchorRateLimitConfig,
    ) -> (f64, f64) {
        let mut multiplier = 1.0;

        // Check if the anchor has verified personhood (for multiplier)
        if config.verified_multiplier > 1.0 {
            if let Some(store) = &self.personhood_store {
                if let Ok(Some(anchor)) = store.get_anchor(anchor_id) {
                    // Check if anchor meets verified POP level
                    if anchor.meets_pop_level(icn_identity::POPLevel::Verified) {
                        multiplier = config.verified_multiplier;
                        debug!(
                            did = %peer,
                            anchor_id = %HexDisplay(anchor_id),
                            multiplier = multiplier,
                            "Applying verified personhood multiplier"
                        );
                    }
                }
            }
        }

        let capacity = config.person_burst_capacity as f64 * multiplier;
        let refill_rate = (config.max_messages_per_person_per_second as f64
            * config.refill_interval.as_secs_f64()
            * multiplier)
            .max(1.0);

        (capacity, refill_rate)
    }

    /// Clean up old buckets for peers that haven't sent messages recently
    /// (Call this periodically to prevent unbounded memory growth)
    pub async fn cleanup_old_buckets(&self, max_age: Duration) {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();

        buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);

        // Also clean up anchor buckets
        let mut anchor_buckets = self.anchor_buckets.write().await;
        anchor_buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);

        // Update metrics
        icn_obs::metrics::network::personhood_anchors_active_set(anchor_buckets.len());
    }

    /// Get the number of tracked peers
    pub async fn tracked_peers(&self) -> usize {
        self.buckets.read().await.len()
    }

    /// Get the number of tracked personhood anchors (for Sybil resistance)
    pub async fn tracked_anchors(&self) -> usize {
        self.anchor_buckets.read().await.len()
    }

    /// Check if Sybil resistance is enabled
    pub fn is_sybil_resistance_enabled(&self) -> bool {
        self.personhood_store.is_some() && self.anchor_rate_config.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_kernel_api::authz::{AllowAllOracle, ConstraintSet, DenyAllOracle, RateLimit};

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let config = RateLimitConfig {
            max_messages_per_second: 10,
            burst_capacity: 5,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new(config);
        let peer = KeyPair::generate().unwrap().did().clone();

        // Should allow burst_capacity messages immediately
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(&peer).await);
        }

        // Next message should be rate limited
        assert!(!limiter.check_rate_limit(&peer).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_refills() {
        let config = RateLimitConfig {
            max_messages_per_second: 10,
            burst_capacity: 2,
            refill_interval: Duration::from_millis(50),
        };

        let limiter = RateLimiter::new(config);
        let peer = KeyPair::generate().unwrap().did().clone();

        // Consume all tokens
        assert!(limiter.check_rate_limit(&peer).await);
        assert!(limiter.check_rate_limit(&peer).await);
        assert!(!limiter.check_rate_limit(&peer).await);

        // Wait for refill
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be allowed again
        assert!(limiter.check_rate_limit(&peer).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_per_peer() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        let peer1 = KeyPair::generate().unwrap().did().clone();
        let peer2 = KeyPair::generate().unwrap().did().clone();

        // Consume all tokens for peer1
        for _ in 0..20 {
            limiter.check_rate_limit(&peer1).await;
        }
        assert!(!limiter.check_rate_limit(&peer1).await);

        // peer2 should still be allowed
        assert!(limiter.check_rate_limit(&peer2).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_oracle_deny_blocks() {
        let oracle = Arc::new(DenyAllOracle::new(Domain::new(NETWORK_DOMAIN), "denied"));
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();
        assert!(!limiter.check_rate_limit(&peer).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_oracle_rate_limit() {
        struct TestOracle;
        impl PolicyOracle for TestOracle {
            fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
                PolicyDecision::allow_with(
                    ConstraintSet::new().with_rate_limit(RateLimit::new(5, 2)),
                )
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        let oracle = Arc::new(TestOracle);
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();

        for _ in 0..2 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await);
    }

    #[tokio::test]
    async fn test_cleanup_old_buckets() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        let peer1 = KeyPair::generate().unwrap().did().clone();
        let peer2 = KeyPair::generate().unwrap().did().clone();

        limiter.check_rate_limit(&peer1).await;
        limiter.check_rate_limit(&peer2).await;

        assert_eq!(limiter.tracked_peers().await, 2);

        // Clean up buckets older than 1ms (should clean peer1 and peer2)
        tokio::time::sleep(Duration::from_millis(10)).await;
        limiter.cleanup_old_buckets(Duration::from_millis(5)).await;

        assert_eq!(limiter.tracked_peers().await, 0);
    }

    #[tokio::test]
    async fn test_trust_gated_rate_limiting_different_classes() {
        struct PerPeerOracle {
            isolated_peer: Did,
            known_peer: Did,
            partner_peer: Did,
            federated_peer: Did,
        }

        impl PolicyOracle for PerPeerOracle {
            fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
                let limits = match request.actor().as_str() {
                    actor if actor == self.isolated_peer.as_str() => RateLimit::new(10, 2),
                    actor if actor == self.known_peer.as_str() => RateLimit::new(50, 10),
                    actor if actor == self.partner_peer.as_str() => RateLimit::new(100, 20),
                    actor if actor == self.federated_peer.as_str() => RateLimit::new(200, 50),
                    _ => RateLimit::new(10, 2),
                };
                PolicyDecision::allow_with(ConstraintSet::new().with_rate_limit(limits))
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        let isolated_peer = KeyPair::generate().unwrap().did().clone();
        let known_peer = KeyPair::generate().unwrap().did().clone();
        let partner_peer = KeyPair::generate().unwrap().did().clone();
        let federated_peer = KeyPair::generate().unwrap().did().clone();

        let oracle = Arc::new(PerPeerOracle {
            isolated_peer: isolated_peer.clone(),
            known_peer: known_peer.clone(),
            partner_peer: partner_peer.clone(),
            federated_peer: federated_peer.clone(),
        });
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());

        // Isolated peer (burst 2) - should be rate limited after 2 messages
        assert!(limiter.check_rate_limit(&isolated_peer).await);
        assert!(limiter.check_rate_limit(&isolated_peer).await);
        assert!(!limiter.check_rate_limit(&isolated_peer).await); // Rate limited

        // Known peer (burst 10) - should be rate limited after 10 messages
        for _ in 0..10 {
            assert!(limiter.check_rate_limit(&known_peer).await);
        }
        assert!(!limiter.check_rate_limit(&known_peer).await); // Rate limited

        // Partner peer (burst 20) - should be rate limited after 20 messages
        for _ in 0..20 {
            assert!(limiter.check_rate_limit(&partner_peer).await);
        }
        assert!(!limiter.check_rate_limit(&partner_peer).await); // Rate limited

        // Federated peer (burst 50) - should be rate limited after 50 messages
        for _ in 0..50 {
            assert!(limiter.check_rate_limit(&federated_peer).await);
        }
        assert!(!limiter.check_rate_limit(&federated_peer).await); // Rate limited
    }

    #[tokio::test]
    async fn test_trust_gated_rate_limiting_trust_class_change() {
        struct MutableOracle {
            first_limit: RateLimit,
            upgraded_limit: RateLimit,
            upgraded: std::sync::atomic::AtomicBool,
        }

        impl PolicyOracle for MutableOracle {
            fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
                let limit = if self.upgraded.load(std::sync::atomic::Ordering::SeqCst) {
                    self.upgraded_limit.clone()
                } else {
                    self.first_limit.clone()
                };
                PolicyDecision::allow_with(ConstraintSet::new().with_rate_limit(limit))
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        // RateLimit::new(messages_per_second, burst_size)
        // burst_size is used as the bucket capacity
        let oracle = Arc::new(MutableOracle {
            first_limit: RateLimit::new(10, 10),    // capacity=10
            upgraded_limit: RateLimit::new(50, 50), // capacity=50
            upgraded: std::sync::atomic::AtomicBool::new(false),
        });
        let limiter = RateLimiter::new_with_oracle(oracle.clone(), RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();

        // Consume 5 out of 10 tokens (leaving 5)
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(&peer).await);
        }

        // Upgrade trust - proportional refill scales tokens: 5 * (50/10) = 25
        oracle
            .upgraded
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // After trust upgrade with proportional refill, we should have 25 tokens
        // (proportional: 5 remaining * (new_capacity / old_capacity) = 5 * 5 = 25)
        for _ in 0..25 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await); // Rate limited at new threshold
    }

    #[tokio::test]
    async fn test_trust_gated_config_for_class() {
        struct SimpleOracle;

        impl PolicyOracle for SimpleOracle {
            fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
                PolicyDecision::allow_with(
                    ConstraintSet::new().with_rate_limit(RateLimit::new(25, 5)),
                )
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        let oracle = Arc::new(SimpleOracle);
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();

        for _ in 0..5 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await);
    }

    // ========================================================================
    // Sybil Resistance Tests (Issue #675)
    // ========================================================================

    use icn_identity::{
        InMemoryPersonhoodStore, PersonhoodAnchor, PersonhoodAnchorStore, PersonhoodStoreTrait,
    };

    /// Create a test personhood store with anchors
    fn create_test_personhood_store() -> Arc<PersonhoodAnchorStore<InMemoryPersonhoodStore>> {
        let store = Arc::new(InMemoryPersonhoodStore::new());
        Arc::new(PersonhoodAnchorStore::new(store))
    }

    /// Test that per-person rate limiting works correctly for different people.
    ///
    /// This test creates 10 different people (10 anchors), each with their own DID.
    /// Each person should get their own per-anchor bucket, so 10 people × 5 burst
    /// = 50 total messages allowed.
    ///
    /// Note: This is NOT a Sybil attack test. See `test_sybil_attack_multiple_dids_one_anchor`
    /// for the actual Sybil resistance test.
    #[tokio::test]
    async fn test_per_person_rate_limiting() {
        let personhood_store = create_test_personhood_store();

        // Create 10 different people, each with their own anchor and DID
        let mut person_dids = Vec::new();

        for i in 0..10 {
            let anchor_key = [i as u8; 32];
            let anchor = PersonhoodAnchor::genesis(&format!("person_{i}"), anchor_key);
            personhood_store.store(&anchor).unwrap();
            // Each person uses their anchor's DID
            person_dids.push(anchor.to_did());
        }

        // Create rate limiter with Sybil resistance
        // Use high per-DID limits so per-anchor limits are the bottleneck
        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let fallback_config = RateLimitConfig {
            max_messages_per_second: 100,
            burst_capacity: 20, // High per-DID limit
            refill_interval: Duration::from_millis(100),
        };

        let anchor_config = AnchorRateLimitConfig {
            max_messages_per_person_per_second: 100,
            person_burst_capacity: 5, // Low limit per person - this is the bottleneck
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 1.0,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            fallback_config,
            personhood_store.clone() as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Each person (anchor) should be able to send up to their burst capacity
        // 10 people × 5 burst each = 50 total messages
        let mut total_allowed = 0;

        for did in &person_dids {
            // Each person tries to send 10 messages
            for _ in 0..10 {
                let (did_allowed, anchor_allowed) =
                    limiter.check_rate_limit_with_personhood(did).await;
                if did_allowed && anchor_allowed {
                    total_allowed += 1;
                }
            }
        }

        // With per-person limits (burst=5) and 10 people: ~50 messages allowed
        // Each person gets 5 messages, rest are blocked by per-anchor limit
        assert!(
            (45..=55).contains(&total_allowed),
            "Expected ~50 messages allowed (10 people × 5 burst), got {total_allowed}"
        );

        // Verify we're tracking all 10 anchors
        assert_eq!(
            limiter.tracked_anchors().await,
            10,
            "Should track 10 anchors (one per person)"
        );
    }

    /// Test actual Sybil attack mitigation: multiple DIDs belonging to one person.
    ///
    /// A Sybil attacker creates multiple DIDs but they all resolve to the same
    /// PersonhoodAnchor. Without per-anchor limiting, they'd get N × burst capacity.
    /// With per-anchor limiting, all their DIDs share ONE bucket.
    #[tokio::test]
    async fn test_sybil_attack_multiple_dids_one_anchor() {
        let personhood_store = create_test_personhood_store();

        // Create ONE person (one anchor)
        let anchor_key = [42u8; 32];
        let anchor = PersonhoodAnchor::genesis("sybil_attacker", anchor_key);
        let anchor_id = anchor.anchor.id;
        personhood_store.store(&anchor).unwrap();

        // The attacker creates 10 different DIDs (simulating key rotation or multi-device)
        // but they all point to the same anchor (same person)
        let mut attacker_dids = vec![anchor.to_did()]; // Primary DID

        for _ in 0..9 {
            // Generate additional keypairs (simulating Sybil attack)
            let extra_keypair = KeyPair::generate().unwrap();
            let extra_did = extra_keypair.did().clone();
            // Link this DID to the same anchor
            personhood_store.link_did(&anchor_id, &extra_did).unwrap();
            attacker_dids.push(extra_did);
        }

        assert_eq!(
            attacker_dids.len(),
            10,
            "Should have 10 DIDs for the attacker"
        );

        // Create rate limiter with Sybil resistance
        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let fallback_config = RateLimitConfig {
            max_messages_per_second: 100,
            burst_capacity: 20, // High per-DID limit (would allow 200 total without anchor limiting)
            refill_interval: Duration::from_millis(100),
        };

        let anchor_config = AnchorRateLimitConfig {
            max_messages_per_person_per_second: 100,
            person_burst_capacity: 10, // Per-person limit: only 10 messages total
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 1.0,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            fallback_config,
            personhood_store.clone() as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Attacker tries to send messages from all their DIDs
        // Without Sybil protection: 10 DIDs × 20 burst = 200 messages
        // With Sybil protection: all DIDs share ONE anchor bucket = 10 messages total
        let mut total_allowed = 0;
        let mut anchor_rejections = 0;

        for did in &attacker_dids {
            for _ in 0..5 {
                let (did_allowed, anchor_allowed) =
                    limiter.check_rate_limit_with_personhood(did).await;
                if did_allowed && anchor_allowed {
                    total_allowed += 1;
                } else if did_allowed && !anchor_allowed {
                    // DID limit passed but anchor limit blocked - this is Sybil mitigation
                    anchor_rejections += 1;
                }
            }
        }

        // Key assertion: despite 10 DIDs, only ~10 messages should be allowed
        // (the per-person burst capacity), not 200 (10 × 20)
        assert!(
            (8..=12).contains(&total_allowed),
            "Expected ~10 messages allowed (per-person limit), got {total_allowed}. \
             Sybil resistance should limit aggregate throughput."
        );

        // Verify significant rejections due to anchor limit (Sybil mitigation)
        assert!(
            anchor_rejections > 30,
            "Expected many anchor-based rejections (Sybil mitigation), got {anchor_rejections}"
        );

        // Verify only ONE anchor is tracked (the attacker is one person)
        assert_eq!(
            limiter.tracked_anchors().await,
            1,
            "Should track only 1 anchor (all DIDs belong to same person)"
        );
    }

    #[tokio::test]
    async fn test_no_personhood_anchor_graceful_fallback() {
        let personhood_store = create_test_personhood_store();

        // Create a peer without any personhood anchor
        let peer_without_anchor = KeyPair::generate().unwrap().did().clone();

        // Create rate limiter with Sybil resistance in Enforce mode (not RequirePersonhood)
        let anchor_config = AnchorRateLimitConfig {
            enforcement_mode: EnforcementMode::Enforce,
            ..Default::default()
        };

        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            RateLimitConfig::default(),
            personhood_store as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Peer without anchor should still be allowed (falls back to per-DID limiting)
        let (did_allowed, anchor_allowed) = limiter
            .check_rate_limit_with_personhood(&peer_without_anchor)
            .await;

        assert!(did_allowed, "DID should be allowed by per-DID limit");
        assert!(
            anchor_allowed,
            "Anchor check should pass for DIDs without anchor (graceful fallback)"
        );
    }

    #[tokio::test]
    async fn test_require_personhood_mode() {
        let personhood_store = create_test_personhood_store();

        // Create a peer without any personhood anchor
        let peer_without_anchor = KeyPair::generate().unwrap().did().clone();

        // Create rate limiter with RequirePersonhood mode
        let anchor_config = AnchorRateLimitConfig {
            enforcement_mode: EnforcementMode::RequirePersonhood,
            ..Default::default()
        };

        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            RateLimitConfig::default(),
            personhood_store as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Peer without anchor should be rejected in RequirePersonhood mode
        let (did_allowed, anchor_allowed) = limiter
            .check_rate_limit_with_personhood(&peer_without_anchor)
            .await;

        assert!(did_allowed, "DID should be allowed by per-DID limit");
        assert!(
            !anchor_allowed,
            "Anchor check should fail for DIDs without anchor in RequirePersonhood mode"
        );
    }

    #[tokio::test]
    async fn test_log_only_mode_allows_excess() {
        let personhood_store = create_test_personhood_store();

        // Create anchor and use its DID
        let anchor_key = [1u8; 32];
        let anchor = PersonhoodAnchor::genesis("test", anchor_key);
        personhood_store.store(&anchor).unwrap();

        // Use the anchor's own DID (which is automatically indexed)
        let peer = anchor.to_did();

        // Create rate limiter with LogOnly mode and very low limit
        let anchor_config = AnchorRateLimitConfig {
            person_burst_capacity: 5,
            enforcement_mode: EnforcementMode::LogOnly,
            ..Default::default()
        };

        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            RateLimitConfig::default(),
            personhood_store as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Exhaust the per-person limit
        for _ in 0..5 {
            let (_, anchor_allowed) = limiter.check_rate_limit_with_personhood(&peer).await;
            assert!(anchor_allowed);
        }

        // Subsequent messages should still be allowed in LogOnly mode
        for _ in 0..5 {
            let (_, anchor_allowed) = limiter.check_rate_limit_with_personhood(&peer).await;
            assert!(
                anchor_allowed,
                "LogOnly mode should allow messages even after limit exceeded"
            );
        }
    }

    #[tokio::test]
    async fn test_sybil_resistance_not_enabled_passthrough() {
        let config = RateLimitConfig {
            max_messages_per_second: 10,
            burst_capacity: 5,
            refill_interval: Duration::from_millis(100),
        };

        // Create limiter WITHOUT Sybil resistance
        let limiter = RateLimiter::new(config);
        let peer = KeyPair::generate().unwrap().did().clone();

        assert!(
            !limiter.is_sybil_resistance_enabled(),
            "Sybil resistance should not be enabled"
        );

        // check_rate_limit_with_personhood should work (anchor check passes)
        let (did_allowed, anchor_allowed) = limiter.check_rate_limit_with_personhood(&peer).await;
        assert!(did_allowed);
        assert!(
            anchor_allowed,
            "Anchor check should pass when Sybil resistance is disabled"
        );
    }

    #[test]
    fn test_enforcement_mode_parsing() {
        assert_eq!(EnforcementMode::parse("log_only"), EnforcementMode::LogOnly);
        assert_eq!(EnforcementMode::parse("LogOnly"), EnforcementMode::LogOnly);
        assert_eq!(EnforcementMode::parse("enforce"), EnforcementMode::Enforce);
        assert_eq!(EnforcementMode::parse("ENFORCE"), EnforcementMode::Enforce);
        assert_eq!(
            EnforcementMode::parse("require_personhood"),
            EnforcementMode::RequirePersonhood
        );
        assert_eq!(
            EnforcementMode::parse("requirepersonhood"),
            EnforcementMode::RequirePersonhood
        );
        assert_eq!(EnforcementMode::parse("unknown"), EnforcementMode::Enforce);
        // Default
    }
}

/// Refill semantics of the production token bucket (#2503).
///
/// # The contract
///
/// 1. `burst_capacity` is the immediately-available token count.
/// 2. `max_messages_per_second` is the long-run replenishment rate, independent
///    of burst.
/// 3. A configured rate below `1 / refill_interval` is still representable — it
///    refills fractionally rather than being rounded up.
/// 4. Fractional refill accumulates; it is never discarded.
/// 5. No configured rate is ever silently *increased*.
/// 6. Capacity remains the upper bound, however long the gap.
/// 7. A configured rate of zero means no replenishment: burst is a one-time
///    allowance, not a floor.
///
/// # What was wrong
///
/// `refill_rate_per_interval` used to end in `.max(1.0)`, forcing at least one
/// token per interval. At the default 100 ms interval that is 10 messages/s, so
/// every configured rate below 10/s — including 0 — was silently raised to 10/s.
///
/// These tests drive `TokenBucket` itself and control time by rewinding
/// `last_refill`, so they are deterministic: no `sleep`, no wall-clock races.
#[cfg(test)]
mod refill_semantics_tests {
    use super::*;

    /// Build a bucket the way production does, via the real refill-rate formula.
    fn bucket(rate: u32, burst: u32, interval_ms: u64) -> TokenBucket {
        let interval = Duration::from_millis(interval_ms);
        let config = RateLimitConfig {
            max_messages_per_second: rate,
            burst_capacity: burst,
            refill_interval: interval,
        };
        TokenBucket::new(burst as f64, refill_rate_per_interval(&config), interval)
    }

    /// Tolerance for token-count assertions.
    ///
    /// `refill()` reads `Instant::now()` itself, so rewinding `last_refill` gives
    /// "at least `d` elapsed", not exactly `d` — a few microseconds of real time
    /// leak in. At the highest rate under test (20 msg/s) a microsecond is 2e-5
    /// tokens, so this bound is ~50x looser than the jitter and ~500x tighter
    /// than the 0.5-token differences being distinguished.
    const TOKEN_EPSILON: f64 = 1e-3;

    /// Rewind `last_refill` so the bucket believes `d` has elapsed.
    fn advance(b: &mut TokenBucket, d: Duration) {
        b.last_refill = b
            .last_refill
            .checked_sub(d)
            .expect("test durations stay within Instant range");
    }

    /// Drain every token so subsequent behaviour is pure refill.
    fn drain(b: &mut TokenBucket) {
        while b.try_consume() {}
        assert!(!b.try_consume(), "bucket must be empty after draining");
    }

    /// A configured 1 msg/s must take ~1 s to yield a token, not 100 ms.
    ///
    /// This is the operator-visible defect: someone tightening a
    /// pre-authentication DoS cap to 1/s was silently given 10/s.
    #[test]
    fn one_message_per_second_is_not_inflated_to_ten() {
        let mut b = bucket(1, 1, 100);
        drain(&mut b);

        advance(&mut b, Duration::from_millis(100));
        assert!(
            !b.try_consume(),
            "at 1 msg/s, 100 ms must NOT yield a token - that would be 10 msg/s"
        );

        // 900 ms more brings the total to a full second.
        advance(&mut b, Duration::from_millis(900));
        assert!(
            b.try_consume(),
            "at 1 msg/s, one full second must yield exactly one token"
        );
    }

    /// A configured 5 msg/s refills in ~200 ms, not ~100 ms.
    #[test]
    fn five_messages_per_second_needs_two_hundred_millis_per_token() {
        let mut b = bucket(5, 1, 100);
        drain(&mut b);

        advance(&mut b, Duration::from_millis(100));
        assert!(
            !b.try_consume(),
            "at 5 msg/s, one 100 ms interval yields only 0.5 tokens"
        );

        advance(&mut b, Duration::from_millis(100));
        assert!(
            b.try_consume(),
            "two 100 ms intervals accumulate 0.5 + 0.5 = 1 token"
        );
    }

    /// Fractional refills accumulate rather than being discarded per interval.
    ///
    /// Stated separately from the timing tests because "0.5 tokens twice" is the
    /// property a naive rounding implementation would break while still passing a
    /// single-interval check.
    #[test]
    fn fractional_refills_accumulate_across_intervals() {
        let mut b = bucket(5, 4, 100);
        drain(&mut b);

        for _ in 0..2 {
            advance(&mut b, Duration::from_millis(100));
            b.refill();
        }

        assert!(
            (b.tokens - 1.0).abs() < TOKEN_EPSILON,
            "two 0.5-token refills must accumulate to exactly 1.0, got {}",
            b.tokens
        );
    }

    /// At the granularity boundary the configured rate is exact.
    #[test]
    fn ten_messages_per_second_is_one_token_per_interval() {
        let mut b = bucket(10, 5, 100);
        drain(&mut b);

        advance(&mut b, Duration::from_millis(100));
        b.refill();

        assert!(
            (b.tokens - 1.0).abs() < TOKEN_EPSILON,
            "10 msg/s at a 100 ms interval is exactly 1 token per interval, got {}",
            b.tokens
        );
    }

    /// Above the granularity the rate scales linearly.
    #[test]
    fn twenty_messages_per_second_is_two_tokens_per_interval() {
        let mut b = bucket(20, 5, 100);
        drain(&mut b);

        advance(&mut b, Duration::from_millis(100));
        b.refill();

        assert!(
            (b.tokens - 2.0).abs() < TOKEN_EPSILON,
            "20 msg/s at a 100 ms interval is exactly 2 tokens per interval, got {}",
            b.tokens
        );
    }

    /// Burst capacity does not change the long-run rate.
    ///
    /// Burst and sustained rate are independent knobs; a bigger bucket must not
    /// refill faster.
    #[test]
    fn burst_capacity_does_not_alter_the_configured_rate() {
        for burst in [1u32, 2, 5, 50] {
            let mut b = bucket(5, burst, 100);
            drain(&mut b);

            advance(&mut b, Duration::from_millis(100));
            b.refill();

            assert!(
                (b.tokens - 0.5).abs() < TOKEN_EPSILON,
                "burst {burst} must not change the 0.5 tokens/interval implied by \
                 5 msg/s, got {}",
                b.tokens
            );
        }
    }

    /// A long gap refills only up to capacity.
    #[test]
    fn a_long_gap_refills_only_to_capacity() {
        let mut b = bucket(200, 5, 100);
        drain(&mut b);

        advance(&mut b, Duration::from_secs(3600));
        b.refill();

        assert!(
            (b.tokens - 5.0).abs() < TOKEN_EPSILON,
            "an hour of accrual must clamp at the burst capacity of 5, got {}",
            b.tokens
        );
    }

    /// A configured rate of zero does not replenish.
    ///
    /// Previously `.max(1.0)` turned a configured 0 into 10 msg/s — the most
    /// surprising case of the floor, since 0 reads as "deny sustained traffic".
    #[test]
    fn zero_rate_does_not_replenish() {
        let mut b = bucket(0, 2, 100);
        drain(&mut b);

        advance(&mut b, Duration::from_secs(60));
        assert!(
            !b.try_consume(),
            "a configured 0 msg/s must not replenish - burst is a one-time \
             allowance, not a floor"
        );
    }
}

/// What one source may spend on anonymous work, and what happens when the table runs out (#2549).
///
/// The connection-level behaviour is proven end to end against a real node in
/// `tests/preauth_source_work_budget.rs`. These cover what that cannot reach deterministically:
/// exact points on the refill curve, source-key normalisation, and the behaviour of a saturated
/// table — which needs more source cardinality than a loopback test has.
#[cfg(test)]
mod source_budget_tests {
    use super::*;
    use std::net::SocketAddr;

    /// A table with a small, exactly-known policy, so an assertion can name a number.
    ///
    /// Deliberately not the production constants: these tests are about the *mechanism*, and a
    /// mechanism test that has to spend 160 tokens to reach an edge hides which edge it reached.
    /// The shipped numbers are exercised by the integration suite.
    fn table(burst: f64, max_sources: usize) -> SourcePreAuthBudget {
        // One whole burst per second, so "advance(1s)" is "one full renewal".
        SourcePreAuthBudget::with_policy(
            burst,
            burst / 10.0,
            Duration::from_millis(100),
            max_sources,
        )
    }

    fn key(addr: &str) -> SourceKey {
        SourceKey::from_addr(addr.parse::<SocketAddr>().expect("test address"))
    }

    /// Spend until refused, and report how many went through.
    fn drain(budget: &SourcePreAuthBudget, k: SourceKey) -> usize {
        let mut spent = 0;
        while budget.spend(k) {
            spent += 1;
            assert!(spent < 10_000, "bucket never refused; it is not bounded");
        }
        spent
    }

    // -- A9: burst, sustained rate, refill ---------------------------------------------------

    /// The burst is a one-time allowance and the rate is what renews it.
    #[test]
    fn burst_then_sustained_rate() {
        let budget = table(8.0, 16);
        let k = key("198.51.100.7:1000");

        assert_eq!(
            drain(&budget, k),
            8,
            "the first allowance is the whole burst"
        );
        assert!(!budget.spend(k), "and it does not renew instantly");

        // Half a renewal window returns half the burst, not all of it.
        budget.advance(Duration::from_millis(500));
        assert_eq!(
            drain(&budget, k),
            4,
            "refill is proportional to elapsed time"
        );

        // A long silence refills to capacity and no further.
        budget.advance(Duration::from_secs(60));
        assert_eq!(drain(&budget, k), 8, "a long gap refills only to the burst");
    }

    /// Over a window of length T a source gets `burst + rate * T`, not `burst`.
    ///
    /// Pinned because the claim is easy to state wrongly, and the wrong statement — "at most
    /// `burst` per window" — is what a reviewer will check the PR body against. A bucket
    /// deliberately permits a full burst at the start of a window and another once it has
    /// refilled, so the sliding-window reading is short by up to a factor of two.
    #[test]
    fn a_window_permits_burst_plus_refill_not_burst() {
        let budget = table(8.0, 16);
        let k = key("198.51.100.8:1000");

        let first = drain(&budget, k);
        budget.advance(Duration::from_secs(1)); // exactly one renewal window
        let second = drain(&budget, k);

        assert_eq!(
            first + second,
            16,
            "one full window yielded {first} + {second}; the honest bound is burst + rate*T"
        );
    }

    // -- A7: source-key rotation --------------------------------------------------------------

    /// A new source port is not a new source.
    #[test]
    fn rotating_the_source_port_does_not_renew_the_allowance() {
        let budget = table(4.0, 16);
        assert_eq!(drain(&budget, key("203.0.113.5:1000")), 4);
        assert!(
            !budget.spend(key("203.0.113.5:65000")),
            "an ephemeral port change bought a fresh allowance"
        );
    }

    /// Host bits inside one IPv6 /64 are not new sources.
    #[test]
    fn rotating_ipv6_host_bits_does_not_renew_the_allowance() {
        let budget = table(4.0, 16);
        assert_eq!(drain(&budget, key("[2001:db8:1:2::1]:1000")), 4);
        assert!(
            !budget.spend(key("[2001:db8:1:2:ffff:ffff:ffff:ffff]:1000")),
            "a host-bit rotation inside one /64 bought a fresh allowance"
        );
        assert!(
            budget.spend(key("[2001:db8:1:3::1]:1000")),
            "a genuinely different /64 must still be its own source"
        );
    }

    /// One source's spending never charges another's.
    #[test]
    fn sources_do_not_charge_each_other() {
        let budget = table(4.0, 16);
        assert_eq!(drain(&budget, key("203.0.113.1:1000")), 4);
        assert_eq!(
            drain(&budget, key("203.0.113.2:1000")),
            4,
            "an exhausted neighbour must not spend this source's allowance"
        );
    }

    // -- A8: order of use, and the absence of retroactive accounting --------------------------

    /// Interleaving old and new users of a source cannot manufacture refill.
    ///
    /// The draft this replaces charged at connection *release* using the admission's own
    /// timestamp, so a late release from an older connection moved the bucket's clock backwards
    /// and paid out refill that had not happened. There is no timestamp to supply here: a token
    /// is taken at the moment the work is about to be done, and nothing is charged when a
    /// connection ends. So teardown order is not an input, and this pins that it stays that way.
    #[test]
    fn use_order_and_teardown_order_are_not_inputs() {
        let budget = table(8.0, 16);
        let k = key("203.0.113.9:1000");

        // Two "connections" from one source, used in strict reverse order of creation, with the
        // older one going last — the shape that used to rewind the clock.
        let mut spent = 0;
        for _ in 0..4 {
            if budget.spend(k) {
                spent += 1;
            }
        }
        for _ in 0..8 {
            if budget.spend(k) {
                spent += 1;
            }
        }
        assert_eq!(spent, 8, "interleaved use paid out more than the burst");
        assert!(!budget.spend(k), "and left the bucket spent, not renewed");
    }

    // -- A6: a saturated table degrades, and never fails open ---------------------------------

    /// An entry disappears once its allowance has come all the way back.
    ///
    /// This is what bounds the table without an eviction policy, and it is why a full table is
    /// hard to arrange: only sources that are *currently* below full occupy a slot.
    #[test]
    fn a_replenished_source_stops_being_tracked() {
        let budget = table(4.0, 2);
        assert!(budget.spend(key("203.0.113.20:1000")));
        assert_eq!(budget.tracked_sources(), 1);

        // Refill past capacity, then force the sweep by making the table need a slot.
        budget.advance(Duration::from_secs(30));
        assert!(budget.spend(key("203.0.113.21:1000")));
        assert!(budget.spend(key("203.0.113.22:1000")));
        assert!(
            budget.tracked_sources() <= 2,
            "replenished entries were never reclaimed; the table is not bounded by its cap"
        );
    }

    /// A full table degrades to a shared allowance — it does not stop bounding anything.
    ///
    /// The failure the previous draft had: at capacity it admitted every untracked source without
    /// accounting, so the headline per-source bound simply ceased to exist once enough other keys
    /// were present. Here an untracked source draws from one shared bucket, so every untracked
    /// source *together* gets at most one source's worth.
    #[test]
    fn a_full_table_degrades_rather_than_failing_open() {
        const CAP: usize = 8;
        let budget = table(4.0, CAP);

        // Fill it with sources that are all below full, so nothing is reclaimable.
        for i in 0..CAP {
            assert!(budget.spend(key(&format!("203.0.113.{}:1000", 100 + i))));
        }
        assert_eq!(budget.tracked_sources(), CAP);

        // Now attack with a key the table has no room for.
        let untracked = key("192.0.2.77:1000");
        let spent = drain(&budget, untracked);
        assert_eq!(
            spent, 4,
            "an untracked source got {spent} dispatches, not the shared burst of 4 — a full \
             table must not fail open"
        );

        // And a second untracked source shares that same exhausted allowance rather than
        // receiving its own.
        assert!(
            !budget.spend(key("192.0.2.78:1000")),
            "each untracked source got its own allowance, so the degraded mode is unbounded in \
             source cardinality"
        );
    }

    /// The very first full-table miss may reclaim, rather than waiting out an interval that
    /// never started.
    ///
    /// `last_sweep` used to be stamped at construction, which made the throttle measure from a
    /// sweep that had not happened. A table filling inside the first
    /// [`BUDGET_SWEEP_MIN_INTERVAL`] therefore degraded to the shared bucket with a table full of
    /// entries it was entitled to reclaim. Nothing about the bound changed — the fallback is
    /// bounded either way — but the docs promise reclamation "on the miss path of a full table",
    /// and this is the one miss where that was untrue.
    #[test]
    fn the_first_full_table_miss_can_still_sweep() {
        const CAP: usize = 8;
        let budget = table(4.0, CAP);

        // Fill the table with entries that are *already* reclaimable: a fresh bucket is full,
        // which is exactly the condition a sweep looks for. Populated directly rather than by
        // spending and calling `advance`, because `advance` rewinds `last_sweep` along with the
        // buckets — it would hand the old code the elapsed interval it is missing and the test
        // would pass either way.
        {
            let mut state = budget.state.lock().expect("test lock");
            for i in 0..CAP {
                state.per_source.insert(
                    key(&format!("203.0.113.{}:1000", 100 + i)),
                    TokenBucket::new(4.0, 0.4, Duration::from_millis(100)),
                );
            }
        }
        assert_eq!(budget.tracked_sources(), CAP);

        // The first miss against a full table. No sweep has ever run, so nothing has grounds to
        // throttle this one. The spend itself succeeds either way — the degraded path would
        // serve it from the shared bucket — so what separates the two is whether the table was
        // reclaimed.
        assert!(budget.spend(key("192.0.2.77:1000")));
        assert_eq!(
            budget.tracked_sources(),
            1,
            "the first miss against a full table did not reclaim its replenished entries, so a \
             table that fills before the first interval elapses degrades to the shared bucket \
             while holding nothing worth holding"
        );
    }

    /// Making the first sweep eligible must not remove the throttle from the ones after it.
    ///
    /// The interval exists so a stream of misses against an unreclaimable table does not walk it
    /// every time. Exercised on `BudgetState` directly because the point is the refusal itself,
    /// and `advance` deliberately rewinds `last_sweep` along with the buckets.
    #[test]
    fn the_sweep_throttle_still_applies_after_the_first_sweep() {
        let budget = table(4.0, 8);
        let mut state = budget.state.lock().expect("test lock");

        assert_eq!(
            state.sweep_replenished(),
            0,
            "an empty table has nothing to reclaim"
        );
        assert!(
            state.last_sweep.is_some(),
            "the first sweep must record that it ran, or the throttle never starts"
        );

        // A brand-new bucket is full, so it is exactly what a sweep would reclaim.
        state.per_source.insert(
            key("203.0.113.9:1000"),
            TokenBucket::new(4.0, 0.4, Duration::from_millis(100)),
        );

        assert_eq!(
            state.sweep_replenished(),
            0,
            "a second sweep inside BUDGET_SWEEP_MIN_INTERVAL reclaimed an entry, so the throttle \
             is gone and every miss now walks the whole table"
        );
        assert_eq!(
            state.per_source.len(),
            1,
            "a refused sweep must leave the table exactly as it found it"
        );
    }

    // -- A10: concurrency ---------------------------------------------------------------------

    /// Simultaneous spenders from one source share one allowance exactly.
    ///
    /// The realistic shape: several connections from one address, each on its own task, all
    /// charging the same bucket. An accounting race here would show up as a burst larger than the
    /// one configured.
    #[test]
    fn concurrent_spenders_from_one_source_share_one_allowance() {
        const BURST: usize = 64;
        const THREADS: usize = 8;
        // Zero refill, so the assertion is exact rather than a race against wall-clock: any
        // token beyond the burst is an accounting fault, never replenishment that leaked in
        // while the threads ran.
        let budget = Arc::new(SourcePreAuthBudget::with_policy(
            BURST as f64,
            0.0,
            Duration::from_millis(100),
            16,
        ));
        let k = key("203.0.113.30:1000");

        let granted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..THREADS {
            let budget = budget.clone();
            let granted = granted.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..BURST {
                    if budget.spend(k) {
                        granted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }));
        }
        for h in handles {
            h.join().expect("spender thread panicked");
        }

        let total = granted.load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            total <= BURST,
            "{THREADS} concurrent spenders were granted {total} tokens from a burst of {BURST}"
        );
        assert_eq!(
            total, BURST,
            "concurrent spenders were granted {total} of a {BURST} burst; an allowance was lost \
             or duplicated under contention"
        );
    }

    // -- A11: the two buckets, and which one is asked first ------------------------------------

    /// A connection that has spent its own burst cannot spend its source's.
    ///
    /// [`PreAuthBudget::check`] asks the connection's bucket before the source's, and `&&` is
    /// short-circuiting, so a spent connection never reaches the shared lock. That operand order
    /// *is* the property, which is why it is pinned here: it survives no compiler check and reads
    /// as an arbitrary style choice to anyone refactoring the expression.
    ///
    /// Reversed, denial becomes free. A source token would be taken before the connection is
    /// consulted, so one connection that had already exhausted its 20 could keep debiting the
    /// shared bucket at whatever rate it can put packets on the wire, dispatching nothing —
    /// pinning its whole NAT at zero tokens for the price of messages the node throws away. That
    /// is strictly worse than the defect #2549 filed, and cheaper.
    ///
    /// The identity asserted is exact rather than approximate: with the source's refill set to
    /// zero, every grant costs exactly one source token, so `granted + left` must equal the burst
    /// no matter how many tokens the *connection* bucket refills mid-run. Under the reversed
    /// order the hammer drains the source outright and the sum collapses to `granted`.
    #[tokio::test]
    async fn a_spent_connection_does_not_charge_its_source() {
        const SOURCE_BURST: usize = 100;
        // Hammered far past the source burst, so a reversed order cannot merely dent it.
        const HAMMER: usize = 1_000;

        let budget = Arc::new(SourcePreAuthBudget::with_policy(
            SOURCE_BURST as f64,
            0.0,
            PRE_AUTH_RATE_LIMIT.refill_interval,
            16,
        ));
        let k = key("203.0.113.40:1000");
        let inbound = PreAuthBudget::Source {
            connection: PreAuthRateLimiter::new(),
            budget: budget.clone(),
            key: k,
        };

        let mut granted = 0usize;
        for _ in 0..(PRE_AUTH_RATE_LIMIT.burst_capacity as usize + HAMMER) {
            if inbound.check().await {
                granted += 1;
            }
        }

        // Non-vacuity: the connection has to have been able to spend something, or the sum below
        // would balance for the trivial reason that nothing ever happened.
        assert!(
            granted >= PRE_AUTH_RATE_LIMIT.burst_capacity as usize,
            "the connection was granted only {granted} of its own burst of {}; this run never \
             exercised a *spent* connection at all",
            PRE_AUTH_RATE_LIMIT.burst_capacity
        );
        assert!(
            granted < SOURCE_BURST,
            "the connection bucket refilled enough to spend the whole source burst ({granted} \
             grants); the assertion below could no longer tell the two orders apart"
        );

        let left = drain(&budget, k);
        assert_eq!(
            granted + left,
            SOURCE_BURST,
            "{granted} dispatches were funded and {left} source tokens remain of {SOURCE_BURST}: \
             {} went to messages that were never dispatched, so a spent connection is draining \
             its source's allowance",
            SOURCE_BURST as i64 - (granted + left) as i64
        );
    }
}
