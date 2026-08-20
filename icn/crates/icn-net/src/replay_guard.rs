//! Replay protection through persistent sequence number tracking
//!
//! Prevents replay attacks by maintaining per-sender sequence windows
//! and rejecting duplicate or out-of-order messages.
//!
//! # Persistence (Security Critical)
//!
//! This module now supports persistent storage of replay protection state.
//! Without persistence, replay attacks would be possible after node restart:
//!
//! 1. Attacker records a signed message
//! 2. Target node restarts (crash, update, etc.)
//! 3. Attacker replays the message
//! 4. Node accepts it (no memory of prior sequences) ← VULNERABILITY
//!
//! With persistence:
//! - `max_seq` per peer is persisted to storage
//! - `finalized` sequences (processed transactions) are persisted
//! - On restart, the floor is the durable high-water, which equals the highest
//!   sequence ever accepted because it is flushed before acceptance returns
//!
//! # Architecture
//!
//! ```text
//! ReplayGuard (Persistent)
//!   ├── In-memory cache (HashMap<SenderPrincipal, SequenceWindow>)
//!   ├── Persistent store (Sled via icn-store)
//!   │   ├── replay_max_seq:<canonical did> → max sequence number
//!   │   └── replay_finalized:<canonical did>:<seq> → finalization timestamp
//!   └── Durable high-water as the restart floor (no sequence gap)
//! ```

use crate::envelope::SignedEnvelope;
use anyhow::{bail, Context, Result};
use ed25519_dalek::VerifyingKey;
use icn_gossip::BloomFilter;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The identity a replay window belongs to: the sender's Ed25519 key, never the text
/// that key was spelled with (#2640).
///
/// # Why the wire `Did` cannot be this identity
///
/// [`SignedEnvelope::canonical_encoding`] covers `sequence ‖ timestamp ‖ payload_type ‖
/// payload` and **not** `from`, while `Did::from_str` accepts any multibase base and stores
/// the string exactly as spelled. One Ed25519 key therefore has many unconditionally accepted
/// textual `did:icn:` names, all unequal under `Did`'s string `Eq`/`Hash`. A party holding
/// **no key material** can take a captured envelope, rewrite only the spelling of `from`,
/// leave the signature bytes untouched, and have it verify — so keying replay state by the
/// wire `Did` issues that party a fresh replay window for every spelling it invents.
///
/// # Why the decoded key is exactly the right equivalence class
///
/// Ed25519 hashes the *encoded* public key into its challenge (`h = H(R ‖ A ‖ M)`). A `from`
/// rewrite that changes the decoded 32 bytes therefore cannot verify, and one that leaves
/// them unchanged has changed nothing a replay guard should be able to see. Two DIDs map to
/// one `SenderPrincipal` **iff** a signature valid under one is valid under the other: the
/// class is neither wider than the attack nor narrower than it. That equality is also
/// precisely the one `verify_classical` already uses, since it derives the verifying key with
/// the same `Did::to_verifying_key` call — the guard and the signature check now agree on who
/// the sender is, which is the property that was missing.
///
/// # Scope — this is not I7
///
/// Deliberately **local to replay protection**. `Did` equality and hashing stay string
/// equality; making them key equality is I7 / N2-A (#2627), which is gated on the N2-A0
/// inventory (#2623) and owns the other keyspaces that inventory lists. Nothing here changes
/// the `Did` parser's acceptance policy, the unvalidated constructors (N2-B), or any
/// account/resource identifier semantics (N2-C′).
#[derive(Clone, Copy)]
pub struct SenderPrincipal(VerifyingKey);

impl SenderPrincipal {
    /// Derive the replay identity from a DID as spelled on the wire.
    ///
    /// Fails for any DID that does not decode to an Ed25519 public key — an anchor-derived
    /// DID, for instance, since `Did::from_anchor_id` bypasses validation and roughly half of
    /// those do not decode (`docs/architecture/n2-a0-stored-key-inventory.md` §10.1).
    ///
    /// Every caller **must** fail closed on the error. There is deliberately no textual
    /// fallback: falling back to the spelling is the defect this type exists to remove.
    pub fn from_did(did: &Did) -> Result<Self> {
        let key = did.to_verifying_key().context(
            "sender DID does not decode to an Ed25519 public key, so it names no replay identity",
        )?;
        Ok(SenderPrincipal(key))
    }

    /// The single spelling this principal is *written* as in durable state.
    ///
    /// `Did::from_public_key` is base58btc, which is also what every production sender emits
    /// (`KeyPair::did()` is derived the same way), so canonical-write is a no-op for honest
    /// state and a merge only for rows an alias put there.
    pub fn canonical_did(&self) -> Did {
        Did::from_public_key(&self.0)
    }

    /// The 32 key bytes the Ed25519 signature was checked against.
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

// Written out rather than derived, because the derive would silently inherit whatever
// `VerifyingKey` decides equality means. The equivalence class is the security property here,
// so it is spelled where a reviewer can see it: two principals are equal exactly when their
// canonical 32-byte key encodings are equal, and the `Hash` impl agrees with it by
// construction because it hashes the same bytes.
impl PartialEq for SenderPrincipal {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for SenderPrincipal {}

impl std::hash::Hash for SenderPrincipal {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl std::fmt::Display for SenderPrincipal {
    /// Renders the canonical spelling, never the one the wire happened to use — a log line
    /// that echoed the attacker's spelling would make one sender look like many.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.canonical_did())
    }
}

impl std::fmt::Debug for SenderPrincipal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SenderPrincipal({})", self.canonical_did())
    }
}

/// The sender's replay identity could not be derived, so nothing was accepted.
///
/// Fail-closed by construction, and distinct from a replay detection so callers do not score
/// it as one. There is no fallback to the textual spelling because that fallback *is* the
/// #2640 defect.
///
/// Structurally unreachable from the network path: `Did`'s `Deserialize` runs `Did::from_str`,
/// which requires the payload to be a valid Ed25519 public key, and `handle_signed` verifies
/// the signature — which derives the very same key — before the guard is consulted. It exists
/// for the crate-public API surface and for locally minted DIDs that bypass validation.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "replay identity could not be derived from sender {peer}; rejecting sequence {sequence} \
     rather than keying replay state by the DID's textual spelling"
)]
pub struct ReplayIdentityUndecodable {
    /// The DID exactly as it was spelled by whoever sent it.
    pub peer: String,
    /// The sequence number that was refused.
    pub sequence: u64,
}

/// Shorthand for the replay identity of a DID in tests.
///
/// Test-only. Production code derives this on the accept path and fails closed on the error;
/// every DID here is `KeyPair`-derived, so a failure to decode is a broken test rather than a
/// case to handle.
#[cfg(test)]
fn pk(did: &Did) -> SenderPrincipal {
    SenderPrincipal::from_did(did).expect("test DIDs are key-derived and must decode")
}

/// Hash a sequence number to a 32-byte hash for Bloom filter
fn hash_sequence(sequence: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(sequence.to_be_bytes());
    hasher.finalize().into()
}

// Restart recovery: why there is no sequence-space safety gap here.
//
// Let `A` be the highest sequence actually accepted from a peer before a crash
// and `floor` the acceptance floor installed on restart. Security needs
// `floor >= A` (otherwise a captured message in `(floor, A]` is accepted twice,
// since the Bloom filter is empty after restart). Liveness needs `floor <= A`,
// because a healthy sender's next emission is at most `A + 1` (see
// `signing_sequence.rs`: sequences may be skipped on sender restart, never
// reused). Both together force `floor == A` exactly — there is no slack to spend
// on conservatism, and any positive constant gap is a liveness bug.
//
// This previously carried `RESTART_SAFETY_GAP = 1_000`, which put the floor
// above a healthy sender's live position and rejected legitimate traffic until
// the sender burned 1,000 sequences or the peer window aged out (#2514).
//
// Let `A` be the highest sequence actually accepted and `D` the highest durably
// recorded. The floor restored on restart is `D`, so the gap only ever mattered
// because `D < A` was possible: `Store::put` is a buffered sled insert and
// `sled::open()` defaults to `flush_every_ms = Some(500)`, so a crash could lose
// the most recent acceptances.
//
// The fix is to eliminate that interval rather than paper over it: the
// high-water is made durable *before* acceptance is returned, so `D == A` always
// and the restored floor is exactly `A`. Measured cost is ~41us per advancing
// message (~24k msg/s), against a federation that runs at tens of messages per
// minute.
//
// A wall-clock "was this signed before my restart?" test was tried and rejected:
// `envelope.timestamp` is the *sender's* clock and any restart instant is the
// *receiver's*. Under the skew ICN already tolerates, that comparison both
// rejects legitimate traffic (sender behind) and admits crash-window replays
// (sender ahead) — see the `sender_skew` tests. Bounded clock difference does
// not confer the ability to order events across machines.
//
// See docs/architecture/replay-state-restart-invariants.md.

/// Maximum entries before Bloom filter rotation (80% of capacity)
const BLOOM_ROTATION_THRESHOLD: u64 = 8_000;

/// Bloom filter capacity
const BLOOM_CAPACITY: usize = 10_000;

/// Key prefix for max sequence storage
const MAX_SEQ_PREFIX: &[u8] = b"replay_max_seq:";

/// Key prefix for finalized sequence storage
const FINALIZED_PREFIX: &[u8] = b"replay_finalized:";

/// Key prefix for established sender-regime provenance (#2517).
///
/// **Deliberately a separate key from the high-water**, because the two facts have
/// different lifetimes.
///
/// The high-water is a numeric window that legitimately ages out: `cleanup()` drops
/// it after `max_peer_age_secs` of silence. The provenance record answers a different
/// question — "have we ever proven that this DID's legacy sequence namespace was
/// retired?" — and that answer does not stop being true because a peer went quiet.
///
/// Keeping them in one value would mean routine garbage collection erases the proof,
/// and a receiver that once knew better would be forced back into treating the peer
/// as never-established. It would then either re-impose the migration hold forever,
/// or — far worse — take the absence of a record as evidence that no legacy namespace
/// ever existed. See `docs/architecture/protocol-state-migration-invariants.md`.
const SENDER_REGIME_PREFIX: &[u8] = b"replay_sender_regime:";

/// Semantic regime this binary writes and can interpret (#2517).
///
/// Bump this only when the *meaning* of a persisted field changes in a way a
/// previous version would misread — not when a field is merely added. Every bump
/// needs a corresponding arm in `load_persisted_state`, or the entry falls into the
/// unknown-version branch and is fail-closed, which is the safe default but a poor
/// migration.
const REPLAY_STATE_SEMANTIC_VERSION: u32 = 1;

/// The regime of any entry written before the version field existed.
///
/// Not a real version number that anything ever wrote: it is the `serde` default
/// that an absent key deserializes to, which is exactly what makes legacy entries
/// detectable without a schema change.
const LEGACY_REPLAY_STATE_SEMANTIC_VERSION: u32 = 0;

// ---------------------------------------------------------------------------
// Sender sequence regime (#2517)
// ---------------------------------------------------------------------------
//
// `REPLAY_STATE_SEMANTIC_VERSION` above answers "which of *our* code versions
// wrote this entry?". These answer the independent question "whose sequence
// namespace produced the number in it?".
//
// Both are required. A receiver that knows only the first will happily record a
// number it learned from a pre-#2510 ephemeral sender and stamp it current,
// because the receiver *is* current — and when that sender later upgrades and its
// durable counter starts low, the receiver rejects it against a bound that never
// applied. That is #2517 recreated under a label the legacy migration cannot see.

/// The sender's sequence namespace is not proven to be durable.
///
/// Covers two situations that are *behaviourally identical* and must not be
/// separated: a sender known to be legacy, and a sender we have simply never
/// established anything about. Both mean the same thing operationally — we hold no
/// proof that this DID's legacy namespace was retired — and both must therefore gate
/// a durable claim behind the same hold.
///
/// Treating "no local record" as its own permissive state was the #2517 design gate:
/// it reads absence of *our* memory as evidence about the *sender's* history. A
/// receiver that just joined, whose store was repaired, or whose window was aged out
/// by `cleanup()` knows nothing about what the sender emitted seconds ago, and
/// envelopes from its previous namespace may still be inside their freshness window.
///
/// Deliberately zero, so an entry written before this field existed — by a
/// pre-#2517 receiver, or by the intermediate receiver-only-versioning build —
/// deserializes to it. Absence of proof is the conservative reading and it must be
/// the `serde` default; a default of `SENDER_REGIME_DURABLE_V1` would silently
/// launder every pre-existing entry.
const SENDER_REGIME_LEGACY_OR_UNPROVEN: u32 = 0;

/// The sender proved, on an authenticated connection, that its sequence is durable
/// per-DID state (#2510): crash-safe, monotonic, never reissued.
const SENDER_REGIME_DURABLE_V1: u32 = 1;

/// A Legacy→DurableV1 namespace change is underway for this sender.
///
/// `max_seq` in an entry carrying this tag is *legacy evidence only*: it is
/// retained so captured old-namespace envelopes stay rejected, and it is never a
/// bound on durable-v1 sequences. Deliberately persisted without any deadline —
/// see [`PeerHold::MigratingSenderRegime`].
const SENDER_REGIME_TRANSITION_TO_DURABLE_V1: u32 = 2;

/// What the **current authenticated connection** proves about a sender's sequence
/// namespace (#2517).
///
/// This is an input to every replay check rather than something [`ReplayGuard`]
/// looks up, and that is deliberate. Making it a parameter means no call site can
/// reach the replay check without stating which namespace it believes the sequence
/// came from; a guard that resolved it internally would let a future caller skip
/// the question and inherit whatever default was convenient.
///
/// # Attribution rests on #2520
///
/// A `DurableV1` observation is only sound because Hello claims are bound to the
/// certificate on the live QUIC connection: capabilities recorded against DID `B`
/// prove `B` authenticated *this* connection. Before that fix, any peer could
/// replay `B`'s published `BindingInfo` and assert `B`'s capabilities, which would
/// have let an unrelated attacker drive `B`'s replay state through a namespace
/// change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedSenderRegime {
    /// The authenticated peer does not advertise `DURABLE_SIGNING_SEQUENCE`, or we
    /// hold no authenticated capability record for it at all.
    ///
    /// Three different peers land here and only one is dangerous: a genuinely
    /// pre-#2510 ephemeral sender, a #2510-era durable sender built before the
    /// capability existed, and a peer whose Hello we have not seen on this
    /// connection. They are treated identically because nothing available to the
    /// receiver distinguishes them, and the cost of conflating them is a bounded
    /// one-time hold rather than an unbounded replay window.
    LegacyOrUnproven,

    /// The peer authenticated on the current connection advertises
    /// `DURABLE_SIGNING_SEQUENCE`.
    DurableV1,
}

/// Receiver-local monotonic time, injectable so migration holds can be tested at
/// the production horizon instead of a scaled-down one.
///
/// Returns elapsed time from an arbitrary fixed origin rather than a timestamp,
/// because that is all any hold here needs and because `std::time::Instant` cannot
/// be constructed at an arbitrary point — a trait returning `Instant` would be
/// untestable in exactly the place that matters.
///
/// **Monotonic, never wall-clock.** Every deadline in this module is a
/// receiver-local elapsed duration. Nothing here may be derived from a
/// sender-supplied timestamp or from `SystemTime`: the first is a different clock
/// domain (see the module header on why cross-machine event ordering is not
/// available under the skew ICN tolerates), and the second can jump or roll back,
/// which would let a clock change shorten a security hold.
pub trait MonotonicClock: Send + Sync {
    /// Elapsed time since this clock's origin. Must never decrease.
    fn elapsed(&self) -> Duration;
}

/// The production clock: elapsed time since the guard was constructed.
struct SystemMonotonicClock {
    origin: Instant,
}

impl MonotonicClock for SystemMonotonicClock {
    fn elapsed(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// What this receiver currently believes about a sender's sequence namespace.
///
/// Distinct from [`ObservedSenderRegime`], which is what the *live connection* says
/// right now. This is the durable belief; that is the current evidence. The whole
/// #2517 state machine is the rules for moving from one to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SenderRegimeState {
    /// We hold no proof that this sender's legacy namespace was retired.
    ///
    /// The default, and deliberately the *only* unproven state: "known legacy" and
    /// "never seen before" are not distinguished, because distinguishing them was
    /// unsound. Absence of a local high-water is a fact about this receiver's memory,
    /// not about the sender's history — see [`SENDER_REGIME_LEGACY_OR_UNPROVEN`].
    LegacyOrUnproven,

    /// A Legacy→DurableV1 namespace change is underway. `max_seq` is legacy
    /// evidence only and is never compared against a durable-v1 sequence.
    TransitionToDurableV1,

    /// `max_seq` belongs to the durable-v1 namespace; ordinary #2514 semantics
    /// apply to it.
    DurableV1,
}

/// The replay high-water could not be made durable, so the message was not
/// accepted.
///
/// Distinct from a replay detection: this is a local storage fault, not peer
/// misbehaviour, and callers must not score it against the sender's reputation.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "replay state for {peer} (sequence {sequence}) could not be made durable; \
     message rejected rather than accepted without a durable record"
)]
pub struct ReplayStateNotDurable {
    /// The peer whose message could not be durably recorded.
    pub peer: String,
    /// The sequence number that could not be durably recorded.
    pub sequence: u64,
}

/// This receiver's durable replay state for the peer could not be read, so the
/// peer is quarantined until nothing it sent before the restart could still be
/// fresh.
///
/// Like [`ReplayStateNotDurable`], this is a **local** storage fault, not peer
/// misbehaviour: the peer did nothing wrong, we simply cannot prove any sequence
/// is new. Callers must not score it against the sender's reputation.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "replay state for {peer} was unreadable at startup; rejecting sequence {sequence} \
     until {remaining_secs}s have passed and no pre-restart envelope can still be fresh"
)]
pub struct ReplayStateUnreadable {
    /// The peer whose durable replay state could not be read.
    pub peer: String,
    /// The sequence number rejected by the quarantine.
    pub sequence: u64,
    /// Seconds remaining before the quarantine lifts.
    pub remaining_secs: u64,
}

/// This receiver's durable replay state for the peer was written under an obsolete
/// sequence/replay semantic regime, so its high-water cannot be trusted as a
/// current-semantic bound. The peer is refused until nothing written under that
/// regime could still be fresh (#2517).
///
/// Like [`ReplayStateUnreadable`], this is a **local** state-migration event, not
/// peer misbehaviour: the peer is sending perfectly legitimate traffic and we are
/// the ones holding a number we can no longer interpret. Callers must not score it
/// against the sender's reputation — doing so is precisely the false-positive that
/// produced thousands of severity-1.0 events and bans against legitimate traffic on
/// the rehearsal federation.
///
/// Distinct from `ReplayStateUnreadable` so that a planned one-time migration is
/// legible in logs and metrics as a migration, rather than indistinguishable from
/// disk corruption.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "replay state for {peer} was written under semantic regime {found_version} \
     (current is {current_version}); rejecting sequence {sequence} for {remaining_secs}s \
     until no envelope from the old regime can still be fresh"
)]
pub struct ReplayStateLegacy {
    /// The peer whose durable replay state predates the current regime.
    pub peer: String,
    /// The sequence number rejected while the migration completes.
    pub sequence: u64,
    /// The semantic regime the stored entry was written under.
    pub found_version: u32,
    /// The semantic regime this binary interprets.
    pub current_version: u32,
    /// Seconds remaining before the migration completes.
    pub remaining_secs: u64,
}

/// This receiver's durable replay state for the peer was written under a semantic
/// regime this binary has no migration for, so its meaning cannot be established at
/// all. The peer is refused **indefinitely** (#2517).
///
/// Almost always: this node was rolled back onto a binary older than the one that
/// wrote its store. It is not a countdown and will not clear itself — an operator
/// must upgrade the binary or explicitly repair the state.
///
/// Distinct from [`ReplayStateLegacy`], which is the opposite situation: there the
/// old meaning is known exactly, which is what licenses a bounded migration. Here
/// nothing is known, and elapsed time cannot make an unknown regime interpretable.
///
/// Like the other state faults this is a **local** condition, not peer
/// misbehaviour, and callers must not score it against the sender's reputation.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "replay state for {peer} was written under semantic regime {found_version}, which \
     this binary (regime {current_version}) has no migration for; refusing {sequence} \
     and all further traffic from this peer until the binary is upgraded or the state \
     is repaired — this will not clear on its own"
)]
pub struct ReplayStateUnsupportedVersion {
    /// The peer whose durable replay state cannot be interpreted.
    pub peer: String,
    /// The sequence number refused.
    pub sequence: u64,
    /// The semantic regime the stored entry claims.
    pub found_version: u32,
    /// The semantic regime this binary implements.
    pub current_version: u32,
}

/// The sender changed sequence namespaces (legacy ephemeral → durable-v1), and the
/// receiver is retiring the old namespace before it will accept the new one (#2517).
///
/// **Not peer misbehaviour, and not a replay.** The peer is sending perfectly
/// legitimate traffic under its new numbering; the receiver is refusing it because
/// envelopes from the *old* numbering could still be fresh, and until they cannot
/// be, there is no way to tell a legitimate low durable sequence from a captured
/// legacy one. Callers must not score this against the sender's reputation.
///
/// Distinct from [`ReplayStateLegacy`], which is about *our* replay state predating
/// *our* versioning. This one is about the sender's numbering changing under a
/// receiver that is already current — the case receiver-only versioning could not
/// see.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "sender {peer} changed sequence namespace to durable-v1; holding sequence {sequence} \
     for {remaining_secs}s until no envelope from its previous namespace can still be fresh"
)]
pub struct SenderRegimeTransition {
    /// The peer whose sequence namespace is being migrated.
    pub peer: String,
    /// The sequence held while the old namespace is retired.
    pub sequence: u64,
    /// Seconds remaining before promotion can occur.
    pub remaining_secs: u64,
}

/// A sender previously proven to use the durable-v1 namespace is no longer
/// advertising it (#2517).
///
/// Fails closed and, critically, **preserves** the durable-v1 replay state. Erasing
/// it and starting over would make replay-state reset reachable by downgrade: an
/// attacker who could induce a peer to present as pre-capability would clear the
/// high-water that stops its captured traffic.
///
/// Legitimately reachable by rolling a peer back onto a pre-capability binary after
/// its migration completed. That is a real operational situation with a real answer
/// — roll it forward again — and it is deliberately not papered over, because the
/// receiver cannot distinguish an honest rollback from an induced one.
///
/// A **local** incompatibility, not peer misbehaviour: callers must not score it.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "sender {peer} previously proved the durable-v1 sequence regime but no longer \
     advertises it; refusing sequence {sequence} rather than discarding durable replay \
     state (high-water {retained_max_seq}), because discarding it on downgrade would make \
     replay-state reset reachable by downgrade"
)]
pub struct SenderRegimeDowngrade {
    /// The peer that stopped advertising the durable regime.
    pub peer: String,
    /// The sequence refused.
    pub sequence: u64,
    /// The durable-v1 high-water that is deliberately retained.
    pub retained_max_seq: u64,
}

/// The persisted sender regime tag is one this binary has no meaning for (#2517).
///
/// The sender-side twin of [`ReplayStateUnsupportedVersion`], and bounded by the
/// same principle: a known-obsolete namespace can have an explicit migration
/// because its meaning is known, but an unknown one cannot be reinterpreted by
/// waiting. **No deadline.** An operator must upgrade the binary or repair the
/// state.
///
/// A **local** condition, not peer misbehaviour: callers must not score it.
#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "replay state for {peer} is tagged with sender sequence regime {found_regime}, which \
     this binary has no migration for; refusing {sequence} and all further traffic from \
     this peer until the binary is upgraded or the state is repaired — this will not clear \
     on its own"
)]
pub struct UnsupportedSenderRegime {
    /// The peer whose recorded sender regime cannot be interpreted.
    pub peer: String,
    /// The sequence refused.
    pub sequence: u64,
    /// The regime tag found on disk.
    pub found_regime: u32,
}

/// Persisted max sequence entry
///
/// # Semantics are versioned, not just the schema (#2517)
///
/// The schema has never changed: `max_seq` has always been a `u64` and always
/// parsed. What changed is what the number *means*. Under the pre-#2510 regime the
/// sender's signing sequence was a process-local `AtomicU64` that restarted from
/// zero, so a peer's recorded high-water belonged to an incarnation that no longer
/// exists. Under the pre-#2514 regime a restart inflated the stored value by a
/// fixed gap. Both wrote entries that parse perfectly today and are still wrong.
///
/// `semantic_version` records which regime produced the entry. It is
/// `#[serde(default)]`, so an entry written before this field existed reads back as
/// [`LEGACY_REPLAY_STATE_SEMANTIC_VERSION`] — the absence of the key *is* the
/// signal. Old binaries ignore the extra key, so the change is safe in both
/// directions.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaxSeqEntry {
    /// Maximum sequence seen from this peer
    max_seq: u64,
    /// Timestamp of last update (for debugging/audit)
    updated_at_ms: u64,
    /// Which sequence/replay semantic regime produced `max_seq`.
    ///
    /// Absent in entries written before #2517; see the type docs for why that
    /// absence is load-bearing rather than incidental.
    #[serde(default)]
    semantic_version: u32,

    /// Which *sender* sequence namespace produced `max_seq` (#2517).
    ///
    /// The second, independent axis. `semantic_version` says which of our own code
    /// versions wrote the entry; this says whose numbering the value belongs to.
    /// One without the other is not enough to interpret `max_seq`: a current
    /// receiver can perfectly well have recorded a number from a peer's ephemeral
    /// incarnation.
    ///
    /// In the same value as `max_seq` on purpose. A separate key would leave a
    /// window in which the number and its namespace label disagree, and every write
    /// here is exactly the moment that pairing must stay true.
    ///
    /// `#[serde(default)]` to [`SENDER_REGIME_LEGACY_OR_UNPROVEN`], so any entry
    /// predating this field — including one written by the intermediate build that
    /// versioned only the receiver side — reads as unproven.
    #[serde(default)]
    sender_regime: u32,
}

/// Persisted finalized sequence entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalizedEntry {
    /// When the sequence was finalized
    finalized_at_ms: u64,
}

/// Per-peer sequence tracking for replay protection
///
/// Maintains sequence number windows for each sender to detect:
/// - **Replay attacks**: Same sequence number seen twice
/// - **Out-of-order delivery**: Sequences within acceptance window
/// - **Stale connections**: Cleanup old peer state
/// - **Finalized sequences**: Permanently prevent replay after processing
///
/// # Persistence
///
/// When created with `new_persistent()`, this guard persists:
/// - `max_seq` per peer (prevents replays after restart)
/// - `finalized` sequences (prevents replay of processed transactions)
pub struct ReplayGuard {
    /// Last seen sequence per sender, keyed by the sender's **key**, not by the spelling of
    /// `from` on the envelope that reached us (#2640). See [`SenderPrincipal`].
    sequences: HashMap<SenderPrincipal, SequenceWindow>,

    /// Maximum allowed clock skew (seconds)
    max_clock_skew: u64,

    /// Maximum age before peer state is evicted (seconds)
    max_peer_age_secs: u64,

    /// Persistent storage backend (None for in-memory only)
    store: Option<Arc<dyn Store>>,

    /// Whether we've loaded persisted state and applied safety gap
    initialized: AtomicBool,

    /// Receiver-local monotonic clock backing every migration hold (#2517).
    ///
    /// Injectable so the holds can be exercised at the production horizon rather
    /// than a shrunken one: a test that proves the 600-second retirement works by
    /// actually waiting 600 seconds is not a test anyone runs.
    clock: Arc<dyn MonotonicClock>,
}

/// Sequence window for a single peer
struct SequenceWindow {
    /// Highest sequence number seen from this peer
    max_seq: u64,

    /// Floor sequence number (reject all sequences <= this value)
    /// Set after restart with safety gap to reject all pre-restart sequences
    /// without relying on the bloom filter (which is lost on restart)
    floor_seq: u64,

    /// Bloom filter of recent sequences (for out-of-order detection)
    /// Size: ~10KB for 10,000 sequences with 0.1% false positive rate
    recent: BloomFilter,

    /// Count of entries inserted since last Bloom filter reset
    /// Used to detect when the filter is approaching saturation
    insertion_count: u64,

    /// Finalized sequences (permanently non-replayable)
    /// These are sequences that have been processed (e.g., ledger entry written)
    /// and should NEVER be accepted again, even within the time window
    finalized: HashMap<u64, Instant>,

    /// Last time we saw a message from this peer
    last_update: Instant,

    /// Why this peer is currently refused, if it is.
    ///
    /// All variants are **local** conditions — our own state is unusable — never
    /// peer misbehaviour. See [`PeerHold`] for why one of them has no deadline.
    hold: Option<PeerHold>,

    /// Which sender sequence namespace this receiver believes `max_seq` belongs to
    /// (#2517).
    ///
    /// Without it, `max_seq` is an uninterpretable number: the same `510` means
    /// "the sender's durable counter has reached 510" or "the sender's previous
    /// process happened to reach 510 before it died", and only the second makes a
    /// later `1` legitimate rather than a replay.
    sender_regime: SenderRegimeState,
}

/// Why a peer's traffic is being refused because of *our* replay state.
///
/// # Two of these expire and one does not
///
/// The bounded variants are bounded because we know precisely what the state we
/// are refusing to use meant, and can therefore reason about how long anything
/// produced under it stays dangerous. Once no envelope from that regime can pass
/// `verify_age`, discarding the state is safe and service resumes.
///
/// [`PeerHold::UnsupportedVersion`] carries no deadline, and that is the point.
/// State written by a regime this binary has no migration for might have changed
/// sequence interpretation, window semantics, freshness assumptions, or something
/// this binary has no name for. Waiting does not make an unknown meaning knowable,
/// so there is deliberately nothing here that can expire: the safety property is
/// structural rather than a matter of choosing a large enough constant.
///
/// `Copy` is derived deliberately: `check_replay_only` matches this out of a
/// `&mut SequenceWindow` and then clears it, which is only sound while every field
/// is `Copy`. Adding a non-`Copy` field is therefore a compile error at the derive
/// rather than a silent change in how that match borrows.
#[derive(Clone, Copy)]
enum PeerHold {
    /// Durable state exists but could not be read, so no sequence can be proven
    /// new (#2514). Bounded by the envelope validity horizon.
    ///
    /// This is a *receiver-local elapsed duration* on the monotonic clock. It
    /// deliberately does not compare a sender-supplied timestamp against a
    /// receiver-side instant: those are different clock domains and, under the
    /// skew ICN tolerates, cannot order events across machines.
    Unreadable { until: Duration },

    /// State was written under a known obsolete regime for which this binary has
    /// an explicit migration (#2517). Bounded by the same horizon, after which the
    /// legacy value is retired and current-semantic state is rebuilt from live
    /// traffic.
    MigratingFromLegacy { until: Duration, from_version: u32 },

    /// The sender changed sequence namespaces and the old one is being retired
    /// (#2517). Bounded by the same envelope validity horizon.
    ///
    /// `until` is a reading of the injected monotonic clock, **not** a persisted
    /// wall-clock deadline. On restart this hold is rebuilt from the durable
    /// transition tag with a *full* fresh horizon rather than a remembered
    /// deadline: a restart may therefore lengthen the migration, but no clock jump,
    /// rollback, or crash can shorten it. See the type docs on
    /// `SENDER_REGIME_TRANSITION_TO_DURABLE_V1`.
    MigratingSenderRegime { until: Duration },

    /// The persisted sender regime tag has no meaning in this binary (#2517).
    /// **No deadline**, for the same reason as `UnsupportedVersion`.
    UnsupportedSenderRegime { found_regime: u32 },

    /// State was written under a regime this binary has no migration for —
    /// typically a rollback under a store a newer binary wrote. **No deadline.**
    ///
    /// Resolving this needs an operator: upgrade the binary, or explicitly repair
    /// the state. It must never resolve itself by elapsed time.
    UnsupportedVersion { found_version: u32 },
}

impl ReplayGuard {
    /// Create a new in-memory replay guard (no persistence)
    ///
    /// **WARNING**: State is lost on restart. Use `new_persistent()` for production.
    ///
    /// # Arguments
    /// * `max_clock_skew` - Maximum allowed clock skew in seconds (default: 300)
    /// * `max_peer_age_secs` - Evict peer state after this many seconds of inactivity (default: 3600)
    pub fn new(max_clock_skew: u64, max_peer_age_secs: u64) -> Self {
        ReplayGuard {
            sequences: HashMap::new(),
            max_clock_skew,
            max_peer_age_secs,
            store: None,
            initialized: AtomicBool::new(true), // No initialization needed for in-memory
            clock: Arc::new(SystemMonotonicClock {
                origin: Instant::now(),
            }),
        }
    }

    /// Create a new persistent replay guard
    ///
    /// Persists replay protection state to storage for survival across restarts.
    /// Call `load_persisted_state()` after creation to initialize.
    ///
    /// # Arguments
    /// * `max_clock_skew` - Maximum allowed clock skew in seconds (default: 300)
    /// * `max_peer_age_secs` - Evict peer state after this many seconds of inactivity (default: 3600)
    /// * `store` - Persistent storage backend
    pub fn new_persistent(
        max_clock_skew: u64,
        max_peer_age_secs: u64,
        store: Arc<dyn Store>,
    ) -> Self {
        ReplayGuard {
            sequences: HashMap::new(),
            max_clock_skew,
            max_peer_age_secs,
            store: Some(store),
            initialized: AtomicBool::new(false),
            clock: Arc::new(SystemMonotonicClock {
                origin: Instant::now(),
            }),
        }
    }

    /// Replace the monotonic clock backing migration holds.
    ///
    /// Test-only. The production clock is not swappable at runtime: a security hold
    /// whose clock an operator could substitute is not a hold.
    #[cfg(test)]
    fn with_clock(mut self, clock: Arc<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }

    /// Load persisted replay state
    ///
    /// Must be called during node startup for persistent guards.
    /// Safe to call multiple times (idempotent via atomic flag).
    ///
    /// # Returns
    /// Number of peers loaded from storage
    pub fn load_persisted_state(&mut self) -> Result<usize> {
        // Only initialize once
        if self
            .initialized
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(0);
        }

        match self.load_persisted_state_inner() {
            Ok(loaded) => Ok(loaded),
            Err(e) => {
                // Nothing was loaded, so every in-memory window is absent and every floor is
                // 0. Leaving `initialized` set would make the *next* call skip the load
                // entirely and run against that empty state — which accepts every replay the
                // store was holding evidence of. Clearing it makes a load failure stay a
                // failure until it is fixed, instead of resolving itself into a fail-open on
                // the second message.
                //
                // Load-outcome hardening, not the #2640 canonicalization itself: this file
                // now performs durable writes during the load, so the failure path had to
                // stop being one that silently disarms the guard before the canonicalization
                // could be relied on. Failing to read replay state must never be cheaper for
                // an attacker than replaying against it.
                self.initialized.store(false, Ordering::SeqCst);
                tracing::error!(
                    error = %e,
                    "Replay state could not be loaded; the guard stays uninitialized and will \
                     retry rather than run against empty state"
                );
                Err(e)
            }
        }
    }

    /// The body of [`Self::load_persisted_state`], minus the once-only latch.
    ///
    /// Split out so every early return in here is covered by the latch reset above.
    fn load_persisted_state_inner(&mut self) -> Result<usize> {
        if self.store.is_none() {
            return Ok(0); // In-memory mode, nothing to load
        }

        // #2640 — collapse spelling-distinct rows onto one canonical key *before* anything
        // below interprets them, so the #2514 / #2517 state machine keeps seeing at most one
        // readable row per sender and is not modified by this fix at all.
        //
        // Taken before `store` is bound because it needs `&self` for the clock-independent
        // helpers and writes through the store itself.
        self.canonicalize_durable_identities()?;

        let store = match &self.store {
            Some(s) => s,
            None => return Ok(0),
        };

        let mut count = 0;

        // Peers whose durable state is unreadable are quarantined until every
        // envelope that could have been accepted before this restart is certain
        // to fail freshness. Receiver-local monotonic time; see
        // `envelope_validity_horizon` for the derivation.
        let quarantine_until = self.clock.elapsed() + self.envelope_validity_horizon();

        // Load max sequences
        let entries = store
            .scan(MAX_SEQ_PREFIX)
            .context("Failed to scan replay max_seq entries")?;

        for (key, value) in entries {
            if let Some(did) = Self::parse_max_seq_key(&key) {
                // Rows are keyed by the sender's key, never by its spelling (#2640).
                let Ok(principal) = Self::window_key(&did, "replay max_seq") else {
                    continue;
                };
                let parsed = serde_json::from_slice::<MaxSeqEntry>(&value);
                if let Err(ref e) = parsed {
                    // The key's existence proves we had state for this peer, but
                    // its high-water is unreadable, so no sequence can be shown
                    // to be new. Failing open would hand an attacker a replay
                    // window; instead reject this peer's traffic until anything
                    // captured before the restart is too old to be replayed.
                    let window = self
                        .sequences
                        .entry(principal)
                        .or_insert_with(SequenceWindow::new);
                    window.hold = Some(PeerHold::Unreadable {
                        until: quarantine_until,
                    });
                    tracing::error!(
                        peer = %principal,
                        error = %e,
                        quarantine_secs = self.envelope_validity_horizon().as_secs(),
                        "Corrupt replay state entry; quarantining this peer until captured \
                         traffic can no longer be fresh"
                    );
                }
                if let Ok(entry) = parsed {
                    let window = self
                        .sequences
                        .entry(principal)
                        .or_insert_with(SequenceWindow::new);

                    // The entry parsed, but parsing is a statement about schema and
                    // this decision is about semantics. Enumerated deliberately
                    // rather than tested with `!= current`: "we know exactly what
                    // this used to mean" and "we have no idea what this means" are
                    // different facts and must not share a branch. A future regime
                    // added here needs its own arm with an explicit migration; until
                    // it has one it falls to the catch-all and fails closed, which is
                    // the safe direction to forget something in.
                    match entry.semantic_version {
                        REPLAY_STATE_SEMANTIC_VERSION => {
                            // Our own regime is current. That settles only half the
                            // question — `max_seq` is still uninterpretable until we
                            // also know whose namespace produced it (#2517).
                            match entry.sender_regime {
                                SENDER_REGIME_DURABLE_V1 => {
                                    // Both axes current: the ordinary #2514 path.
                                    // The floor is the durable high-water, which —
                                    // because the high-water is flushed before
                                    // acceptance is returned — is exactly the highest
                                    // sequence ever accepted. Everything accepted
                                    // before the crash is at or below it and the
                                    // sender's next emission is above it, so this
                                    // rejects all of the former and none of the
                                    // latter. The Bloom filter is empty after
                                    // restart, which is why the floor carries
                                    // pre-restart replay rejection.
                                    window.max_seq = entry.max_seq;
                                    window.floor_seq = entry.max_seq;
                                    window.sender_regime = SenderRegimeState::DurableV1;

                                    tracing::debug!(
                                        peer = %principal,
                                        max_seq = entry.max_seq,
                                        floor_seq = entry.max_seq,
                                        "Loaded replay guard state"
                                    );
                                }

                                SENDER_REGIME_LEGACY_OR_UNPROVEN => {
                                    // A number from an unproven namespace, recorded
                                    // by a current receiver. It is a valid bound
                                    // *within that namespace* and is restored as one,
                                    // so captured legacy traffic stays rejected. What
                                    // it must never become is a bound on durable-v1
                                    // sequences — that conversion is gated behind the
                                    // explicit transition below.
                                    window.max_seq = entry.max_seq;
                                    window.floor_seq = entry.max_seq;
                                    window.sender_regime = SenderRegimeState::LegacyOrUnproven;

                                    tracing::debug!(
                                        peer = %principal,
                                        max_seq = entry.max_seq,
                                        "Loaded replay state from an unproven sender regime"
                                    );
                                }

                                SENDER_REGIME_TRANSITION_TO_DURABLE_V1 => {
                                    // A namespace change was underway when we stopped.
                                    // Restart the hold from the *full* horizon rather
                                    // than resuming a remembered deadline: nothing
                                    // durable records how much of it had elapsed, and
                                    // the only safe way to be wrong is long. The
                                    // legacy high-water is kept as legacy evidence so
                                    // captured old-namespace traffic stays rejected
                                    // for the duration.
                                    window.max_seq = entry.max_seq;
                                    window.floor_seq = entry.max_seq;
                                    window.sender_regime = SenderRegimeState::TransitionToDurableV1;
                                    window.hold = Some(PeerHold::MigratingSenderRegime {
                                        until: quarantine_until,
                                    });

                                    tracing::warn!(
                                        peer = %principal,
                                        legacy_max_seq = entry.max_seq,
                                        hold_secs = self.envelope_validity_horizon().as_secs(),
                                        "Resuming an incomplete sender sequence-regime migration; \
                                         restarting the full safety hold rather than trusting a \
                                         remembered deadline"
                                    );
                                }

                                found_regime => {
                                    // A namespace tag written by a binary that knows
                                    // something this one does not. Same principle as
                                    // an unknown receiver regime, applied to the other
                                    // axis: waiting cannot make an unknown numbering
                                    // interpretable, so there is no deadline here.
                                    window.hold =
                                        Some(PeerHold::UnsupportedSenderRegime { found_regime });

                                    tracing::error!(
                                        peer = %principal,
                                        found_regime,
                                        "Replay state is tagged with a sender sequence regime this \
                                         binary has no migration for; refusing this peer \
                                         indefinitely. Most likely an older binary against a store \
                                         a newer one wrote — upgrade it or repair the state"
                                    );
                                }
                            }
                        }

                        LEGACY_REPLAY_STATE_SEMANTIC_VERSION => {
                            // Known obsolete regime, and this binary has an explicit
                            // migration for it: the old meaning is understood well
                            // enough to bound how long anything produced under it
                            // stays dangerous. That bound is what licenses retiring
                            // the value rather than trusting it.
                            window.hold = Some(PeerHold::MigratingFromLegacy {
                                until: quarantine_until,
                                from_version: entry.semantic_version,
                            });

                            // max_seq and floor_seq stay at their SequenceWindow::new
                            // defaults of 0. Freshness, not the floor, carries replay
                            // rejection for the duration of the hold.
                            tracing::warn!(
                                peer = %principal,
                                found_version = entry.semantic_version,
                                current_version = REPLAY_STATE_SEMANTIC_VERSION,
                                discarded_max_seq = entry.max_seq,
                                hold_secs = self.envelope_validity_horizon().as_secs(),
                                "Replay state predates semantic versioning; holding this \
                                 peer until no envelope from that regime can still be \
                                 fresh, then rebuilding from live traffic"
                            );
                        }

                        found_version => {
                            // No migration exists from this regime, so its meaning
                            // cannot be established at all — it may have changed
                            // sequence interpretation, window semantics, freshness
                            // assumptions, or something this binary has no name for.
                            // Held with no deadline: elapsed time cannot make an
                            // unknown regime interpretable, and quietly adopting it
                            // as current after a wait would be a silent downgrade.
                            window.hold = Some(PeerHold::UnsupportedVersion { found_version });

                            tracing::error!(
                                peer = %principal,
                                found_version,
                                current_version = REPLAY_STATE_SEMANTIC_VERSION,
                                "Replay state was written under a semantic regime this binary \
                                 has no migration for; refusing this peer indefinitely. This \
                                 node is most likely running an older binary against a store a \
                                 newer one wrote — upgrade it or repair the state. This will \
                                 not clear on its own"
                            );
                        }
                    }

                    count += 1;
                }
            }
        }

        // Apply established sender-regime provenance (#2517).
        //
        // Authoritative, and applied *after* the max_seq entries so it wins: the
        // high-water tag describes the number, but provenance describes whether this
        // DID's legacy namespace was ever proven retired, and only the latter licenses
        // interpreting a durable claim. Provenance also outlives the high-water, so a
        // peer aged out by `cleanup()` is found here with no numeric state at all.
        let provenance = store
            .scan(SENDER_REGIME_PREFIX)
            .context("Failed to scan sender regime provenance")?;

        for (key, value) in provenance {
            let Some(did) = Self::parse_sender_regime_key(&key) else {
                continue;
            };
            let Ok(principal) = Self::window_key(&did, "replay sender regime") else {
                continue;
            };
            let Ok(raw) = <[u8; 4]>::try_from(value.as_slice()) else {
                // Unreadable provenance is not "no provenance": it is a record whose
                // meaning we cannot establish, and reading it as absent would silently
                // downgrade to unproven — which then permits establishing a fresh
                // durable namespace after a hold, on evidence we cannot actually read.
                let window = self
                    .sequences
                    .entry(principal)
                    .or_insert_with(SequenceWindow::new);
                window.hold = Some(PeerHold::Unreadable {
                    until: quarantine_until,
                });
                tracing::error!(peer = %principal, "Corrupt sender regime provenance; quarantining");
                continue;
            };
            let found = u32::from_be_bytes(raw);
            let window = self
                .sequences
                .entry(principal)
                .or_insert_with(SequenceWindow::new);

            match found {
                SENDER_REGIME_DURABLE_V1 => {
                    // Already proven. If the high-water aged out, this peer resumes
                    // with no numeric bound but keeps its established namespace, so it
                    // pays no second migration hold.
                    window.sender_regime = SenderRegimeState::DurableV1;
                }
                SENDER_REGIME_TRANSITION_TO_DURABLE_V1 => {
                    window.sender_regime = SenderRegimeState::TransitionToDurableV1;
                    window.hold = Some(PeerHold::MigratingSenderRegime {
                        until: quarantine_until,
                    });
                    tracing::warn!(
                        peer = %principal,
                        hold_secs = self.envelope_validity_horizon().as_secs(),
                        "Resuming an incomplete sender sequence-regime migration; restarting \
                         the full safety hold rather than trusting a remembered deadline"
                    );
                }
                other => {
                    window.hold = Some(PeerHold::UnsupportedSenderRegime {
                        found_regime: other,
                    });
                    tracing::error!(
                        peer = %principal,
                        found_regime = other,
                        "Sender regime provenance written by a binary this one has no \
                         migration for; refusing this peer indefinitely"
                    );
                }
            }
        }

        // Load finalized sequences
        let finalized_entries = store
            .scan(FINALIZED_PREFIX)
            .context("Failed to scan finalized entries")?;

        let now = Instant::now();
        let cutoff_ms = Self::current_time_ms().saturating_sub(24 * 60 * 60 * 1000); // 24h ago

        for (key, value) in finalized_entries {
            if let Some((did, seq)) = Self::parse_finalized_key(&key) {
                let Ok(principal) = Self::window_key(&did, "replay finalized") else {
                    continue;
                };
                if let Ok(entry) = serde_json::from_slice::<FinalizedEntry>(&value) {
                    // Only load finalized sequences less than 24h old
                    if entry.finalized_at_ms >= cutoff_ms {
                        // Only attach finalized entries to windows that were already
                        // initialized from max_seq (with safety gap and floor applied).
                        // This avoids creating new windows with floor_seq=0 based solely
                        // on finalized state, which could allow replay of older sequences.
                        if let Some(window) = self.sequences.get_mut(&principal) {
                            window.finalized.insert(seq, now);
                        }
                    }
                }
            }
        }

        tracing::info!(
            loaded_peers = count,
            "Loaded replay guard state; the floor for each peer is its durable \
             high-water, which is the highest sequence ever accepted"
        );

        Ok(count)
    }

    /// Check if message is fresh (not replayed)
    ///
    /// Validates:
    /// 1. Signature and timestamp (via envelope.verify())
    /// 2. Sequence number is not finalized (permanently blocked)
    /// 3. Sequence number is not a replay
    ///
    /// # Replay Detection Logic:
    /// - If sequence is finalized: Reject immediately (critical)
    /// - If sequence <= max_seq: Check Bloom filter
    ///   - If in filter: Reject as replay
    ///   - If not in filter: Accept as out-of-order (add to filter)
    /// - If sequence > max_seq: Accept and update max_seq
    ///
    /// This allows some out-of-order delivery while preventing replays.
    pub fn check(
        &mut self,
        envelope: &SignedEnvelope,
        observed_regime: ObservedSenderRegime,
    ) -> Result<()> {
        // 1. Verify signature and age
        envelope.verify(self.max_clock_skew)?;

        // 2. Perform replay detection (signature already verified, so use check_replay_only)
        self.check_replay_only(envelope, observed_regime)
    }

    /// Model a sender whose durable regime was **already established** before the
    /// test began — i.e. the ordinary steady state, long after any migration.
    ///
    /// Test-only, and not merely a convenience: the production signature takes the
    /// observed regime precisely so that no call site can omit it, and a
    /// non-`cfg(test)` helper with a baked-in regime would hand that omission back.
    ///
    /// The pre-establishment is what distinguishes "this test is about replay
    /// mechanics" from "this test is about migration". Every test that exercises
    /// establishment itself calls `check_replay_only` with an explicit regime instead
    /// and never touches this helper, so this cannot mask an establishment regression
    /// — see `sender_regime_tests`.
    #[cfg(test)]
    fn check_durable(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        self.pre_establish_durable(&envelope.from);
        self.check(envelope, ObservedSenderRegime::DurableV1)
    }

    /// Mark a peer as having completed durable-regime establishment.
    ///
    /// Idempotent, and deliberately a no-op once any regime state exists so it cannot
    /// paper over a hold that a migration test is asserting.
    #[cfg(test)]
    fn pre_establish_durable(&mut self, did: &Did) {
        let principal = SenderPrincipal::from_did(did).expect("test DID must decode to a key");
        let already_known = self
            .sequences
            .get(&principal)
            .map(|w| w.sender_regime != SenderRegimeState::LegacyOrUnproven || w.hold.is_some())
            .unwrap_or(false);
        if already_known {
            return;
        }
        let _ = self.persist_sender_regime(&principal, SENDER_REGIME_DURABLE_V1);
        self.sequences
            .entry(principal)
            .or_insert_with(SequenceWindow::new)
            .sender_regime = SenderRegimeState::DurableV1;
    }

    /// Model a sender that has not proven the durable regime.
    ///
    /// No pre-establishment: an unproven sender is the default state, which is exactly
    /// what these tests mean.
    #[cfg(test)]
    fn check_legacy(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        self.check(envelope, ObservedSenderRegime::LegacyOrUnproven)
    }

    /// [`Self::check_durable`], skipping signature verification.
    #[cfg(test)]
    fn check_replay_only_durable(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        self.pre_establish_durable(&envelope.from);
        self.check_replay_only(envelope, ObservedSenderRegime::DurableV1)
    }

    /// Check if message is fresh (not replayed) without verifying signature
    ///
    /// Use this when signature has already been verified by the caller
    /// (e.g., via `verify_with_cached_pq_key()` in the signed message handler).
    ///
    /// This method performs all replay detection checks but skips signature
    /// verification to avoid redundant cryptographic operations.
    ///
    /// # When to use
    ///
    /// - Use `check()` when you need both signature verification and replay detection
    /// - Use `check_replay_only()` when signature was already verified elsewhere
    ///
    /// # Safety
    ///
    /// Caller MUST ensure the signature has been verified before calling this method.
    /// Failure to verify signatures before replay checking could allow attackers
    /// to inject forged messages that bypass replay detection.
    pub fn check_replay_only(
        &mut self,
        envelope: &SignedEnvelope,
        observed_regime: ObservedSenderRegime,
    ) -> Result<()> {
        // The replay window belongs to the sender's *key*, not to the spelling of `from` on
        // this envelope (#2640). Derived once, before any state is read or written, and
        // failing closed: there is deliberately no fallback to the textual spelling, because
        // that fallback is the defect. Costs honest traffic nothing — a wire `from` always
        // decodes (`Did::deserialize` validates it) and the caller has already verified a
        // signature against this very key.
        let principal = SenderPrincipal::from_did(&envelope.from).map_err(|e| {
            anyhow::Error::new(ReplayIdentityUndecodable {
                peer: envelope.from.as_str().to_string(),
                sequence: envelope.sequence,
            })
            .context(e)
        })?;

        // Ensure initialized for persistent mode
        if !self.initialized.load(Ordering::Acquire) {
            self.load_persisted_state()?;
        }

        // Note: Signature verification is SKIPPED - caller must have already verified

        // Get or create sequence window for this sender
        let window = self
            .sequences
            .entry(principal)
            .or_insert_with(SequenceWindow::new);

        // Holds on our own state (CRITICAL: no sequence can be proven new). All are
        // typed so the signed-message handler does not score our local condition as
        // peer misbehaviour.
        //
        // Bounded holds are released strictly *after* the horizon, not at it: an
        // envelope stamped at the positive-skew limit has age exactly `max_age` at
        // the horizon, and `age > max_age` is false there — it is still valid for
        // that instant. See `envelope_validity_horizon`.
        //
        // The unsupported-version arm has no expiry to reach. It is matched first
        // and returns unconditionally, so no reordering of this block can
        // accidentally give it one.
        // One arm per variant: binding `until` and `from_version` together in a
        // single pattern is what lets the expiry path be written once per hold kind
        // without re-matching to recover the fields.
        let now = self.clock.elapsed();

        match window.hold {
            Some(PeerHold::UnsupportedVersion { found_version }) => {
                return Err(anyhow::Error::new(ReplayStateUnsupportedVersion {
                    peer: envelope.from.as_str().to_string(),
                    sequence: envelope.sequence,
                    found_version,
                    current_version: REPLAY_STATE_SEMANTIC_VERSION,
                }));
            }

            Some(PeerHold::UnsupportedSenderRegime { found_regime }) => {
                return Err(anyhow::Error::new(UnsupportedSenderRegime {
                    peer: envelope.from.as_str().to_string(),
                    sequence: envelope.sequence,
                    found_regime,
                }));
            }

            Some(PeerHold::Unreadable { until }) => {
                if now <= until {
                    return Err(anyhow::Error::new(ReplayStateUnreadable {
                        peer: envelope.from.as_str().to_string(),
                        sequence: envelope.sequence,
                        remaining_secs: (until - now).as_secs(),
                    }));
                }
                window.hold = None;
            }

            Some(PeerHold::MigratingFromLegacy {
                until,
                from_version,
            }) => {
                if now <= until {
                    return Err(anyhow::Error::new(ReplayStateLegacy {
                        peer: envelope.from.as_str().to_string(),
                        sequence: envelope.sequence,
                        found_version: from_version,
                        current_version: REPLAY_STATE_SEMANTIC_VERSION,
                        remaining_secs: (until - now).as_secs(),
                    }));
                }

                // The hold has expired. Nothing written under the old regime can
                // still be fresh, so the empty window this peer now has is a
                // *complete* current-semantic record rather than a gap in one.
                // Clearing the hold is what makes the migration one-way: the next
                // accept persists a current-version entry, and subsequent restarts
                // take the ordinary #2514 exact-restore path.
                //
                // The sender axis returns to unproven, and deliberately does NOT
                // shortcut to durable.
                //
                // It is tempting to argue that this hold already retired everything
                // still-valid, so a durable sender could be established directly. That
                // argument only holds if the sender upgraded *before* the hold began.
                // If it upgraded during the hold, its last legacy envelope was created
                // at some X > hold_start and stays valid until X + skew + max_age,
                // which is past hold_end. The receiver cannot tell those two cases
                // apart, so it must assume the worse one.
                //
                // Cost: the sender-first upgrade order pays two sequential holds. That
                // is the honest price of not being able to date the sender's upgrade.
                tracing::info!(
                    peer = %envelope.from,
                    from_version,
                    to_version = REPLAY_STATE_SEMANTIC_VERSION,
                    "Replay state migration complete; peer state is now current-semantic"
                );
                window.hold = None;
                window.sender_regime = SenderRegimeState::LegacyOrUnproven;
                window.max_seq = 0;
                window.floor_seq = 0;
            }

            Some(PeerHold::MigratingSenderRegime { until }) => {
                if now <= until {
                    return Err(anyhow::Error::new(SenderRegimeTransition {
                        peer: envelope.from.as_str().to_string(),
                        sequence: envelope.sequence,
                        remaining_secs: (until - now).as_secs(),
                    }));
                }

                // The horizon has passed, but elapsed time alone must not promote
                // (#2517 Phase 11). Promotion is a statement about the peer that is
                // talking to us *now*, so it requires evidence from now: if this
                // message did not arrive with a durable-v1 attribution, the peer has
                // disconnected and returned without the capability, or was rolled
                // back, and there is nothing to promote to.
                //
                // Note this cannot be satisfied by a stale record. `observed_regime`
                // is derived per-message from the capabilities of the connection the
                // peer authenticated on (#2520), so it is current by construction.
                if observed_regime != ObservedSenderRegime::DurableV1 {
                    return Err(anyhow::Error::new(SenderRegimeTransition {
                        peer: envelope.from.as_str().to_string(),
                        sequence: envelope.sequence,
                        remaining_secs: 0,
                    }));
                }

                // Promote. The old namespace is retired and a clean durable-v1 one
                // begins: the legacy high-water is dropped rather than carried over,
                // because carrying it would reimpose exactly the incomparable bound
                // this migration exists to remove.
                //
                // Persisted *before* the message is accepted, and before any durable-v1
                // high-water is written, so a crash here cannot leave a durable-v1
                // number under a transition tag. If the flush fails the promotion does
                // not happen and the hold stands.
                // Ordering matters: the numeric namespace is reset first, and the
                // provenance record — the authority — is written last. A crash between
                // them leaves provenance still saying "transition", so the restart
                // re-runs the hold rather than accepting under a namespace it never
                // finished proving. The safe direction to be interrupted in is the one
                // that repeats work.
                self.persist_max_seq_durable(&principal, 0, SENDER_REGIME_DURABLE_V1)
                    .and_then(|()| self.persist_sender_regime(&principal, SENDER_REGIME_DURABLE_V1))
                    .map_err(|e| {
                        anyhow::Error::new(ReplayStateNotDurable {
                            peer: envelope.from.as_str().to_string(),
                            sequence: envelope.sequence,
                        })
                        .context(e)
                    })?;

                let window = self
                    .sequences
                    .entry(principal)
                    .or_insert_with(SequenceWindow::new);
                window.hold = None;
                window.sender_regime = SenderRegimeState::DurableV1;
                window.max_seq = 0;
                window.floor_seq = 0;
                window.recent = BloomFilter::new(BLOOM_CAPACITY, 0.001);
                window.insertion_count = 0;

                tracing::info!(
                    peer = %envelope.from,
                    "Sender sequence-regime migration complete; durable-v1 replay namespace \
                     established and made durable"
                );
            }

            None => {}
        }

        // ------------------------------------------------------------------
        // Sender sequence regime transitions (#2517)
        // ------------------------------------------------------------------
        //
        // Reached only when no hold is active. `window.sender_regime` is what we
        // durably believe; `observed_regime` is what the current authenticated
        // connection proves. The rules below are the entire content of "a replay
        // high-water is valid only within the sender regime that produced it".
        let window = self
            .sequences
            .entry(principal)
            .or_insert_with(SequenceWindow::new);

        match (window.sender_regime, observed_regime) {
            // Steady state. Accepting unproven traffic is what compatibility during a
            // rolling upgrade requires; the number is tagged unproven when persisted.
            (SenderRegimeState::LegacyOrUnproven, ObservedSenderRegime::LegacyOrUnproven) => {}
            (SenderRegimeState::DurableV1, ObservedSenderRegime::DurableV1) => {}

            // The namespace change. `max_seq` was produced by the sender's previous,
            // unproven numbering and the incoming sequence belongs to its durable
            // one; the two are not comparable, so no accept/reject decision may be
            // made by comparing them. Enter an explicit transition instead.
            //
            // The hold is not a punishment and not a replay verdict: envelopes from
            // the old namespace may still be inside their validity window, and until
            // they are not, a low durable sequence and a captured legacy sequence are
            // indistinguishable.
            (SenderRegimeState::LegacyOrUnproven, ObservedSenderRegime::DurableV1) => {
                let legacy_max_seq = window.max_seq;

                // Durable *before* the hold takes effect, so a crash during the
                // transition resumes as a transition rather than as trusted legacy
                // state. The legacy high-water is retained in the same write, so it
                // keeps rejecting captured old-namespace traffic throughout.
                self.persist_max_seq_durable(
                    &principal,
                    legacy_max_seq,
                    SENDER_REGIME_TRANSITION_TO_DURABLE_V1,
                )
                .and_then(|()| {
                    self.persist_sender_regime(&principal, SENDER_REGIME_TRANSITION_TO_DURABLE_V1)
                })
                .map_err(|e| {
                    anyhow::Error::new(ReplayStateNotDurable {
                        peer: envelope.from.as_str().to_string(),
                        sequence: envelope.sequence,
                    })
                    .context(e)
                })?;

                let horizon = self.envelope_validity_horizon();
                let window = self
                    .sequences
                    .entry(principal)
                    .or_insert_with(SequenceWindow::new);
                window.sender_regime = SenderRegimeState::TransitionToDurableV1;
                window.hold = Some(PeerHold::MigratingSenderRegime {
                    until: now + horizon,
                });

                tracing::warn!(
                    peer = %envelope.from,
                    legacy_max_seq,
                    hold_secs = horizon.as_secs(),
                    "Sender proved the durable sequence regime after being unproven; its old \
                     sequence namespace is being retired. This is a local migration, not peer \
                     misbehaviour"
                );

                return Err(anyhow::Error::new(SenderRegimeTransition {
                    peer: envelope.from.as_str().to_string(),
                    sequence: envelope.sequence,
                    remaining_secs: horizon.as_secs(),
                }));
            }

            // Downgrade. Fail closed and keep the durable-v1 state: discarding it
            // here is what would make replay-state reset reachable by presenting as
            // an older binary.
            (SenderRegimeState::DurableV1, ObservedSenderRegime::LegacyOrUnproven) => {
                return Err(anyhow::Error::new(SenderRegimeDowngrade {
                    peer: envelope.from.as_str().to_string(),
                    sequence: envelope.sequence,
                    retained_max_seq: window.max_seq,
                }));
            }

            // Unreachable while a transition is held; a transition without a hold
            // would mean the promotion path above failed to clear one of the two.
            // Fail closed rather than guess which.
            (SenderRegimeState::TransitionToDurableV1, _) => {
                return Err(anyhow::Error::new(SenderRegimeTransition {
                    peer: envelope.from.as_str().to_string(),
                    sequence: envelope.sequence,
                    remaining_secs: 0,
                }));
            }
        }

        let window = self
            .sequences
            .entry(principal)
            .or_insert_with(SequenceWindow::new);

        // Captured now, while the window is borrowed, because the persist below
        // happens after the borrow ends. This is the namespace the accepted number
        // will be *recorded as belonging to*, and it is a property of the window's
        // established state — never of which binary is running.
        let established_regime = window.sender_regime;

        // Check if sequence is finalized (CRITICAL: prevents replay after processing)
        if window.finalized.contains_key(&envelope.sequence) {
            bail!(
                "Replay attempt detected from {}: sequence {} is finalized (processed)",
                envelope.from.as_str(),
                envelope.sequence
            );
        }

        // Check against floor_seq (CRITICAL: prevents replay after restart)
        if envelope.sequence <= window.floor_seq {
            bail!(
                "Replay detected from {}: sequence {} already seen (floor: {})",
                envelope.from.as_str(),
                envelope.sequence,
                window.floor_seq
            );
        }

        // Check sequence number against Bloom filter
        let seq_hash = hash_sequence(envelope.sequence);
        if envelope.sequence <= window.max_seq && window.recent.contains(&seq_hash) {
            bail!(
                "Replay detected from {}: sequence {} already seen (max: {})",
                envelope.from.as_str(),
                envelope.sequence,
                window.max_seq
            );
        }

        // Make the advance durable BEFORE accepting.
        //
        // This is what makes `D == A`: the restored floor is the highest
        // sequence ever accepted, not merely the highest that happened to reach
        // disk before a crash. Without it the interval `(D, A]` exists, and
        // nothing available to a restarted receiver can distinguish a replay in
        // that interval from the sender's next legitimate message (see the
        // module header and the `sender_skew` tests).
        //
        // Fail closed: if the high-water cannot be made durable, the message is
        // not accepted. Accepting it would recreate the interval this is here to
        // eliminate.
        let max_seq_changed = envelope.sequence > window.max_seq;
        if max_seq_changed {
            // The namespace this window is *established in*, never the one this
            // binary implements (#2517).
            //
            // This single expression is the fix. Stamping the current regime here —
            // which is what a receiver that versions only its own semantics does —
            // records a number learned from an unproven sender as durable-v1 state.
            // Nothing downstream can then tell it apart from a real durable
            // high-water, so when that sender upgrades and its durable counter
            // starts low, the receiver rejects it against a bound that never applied
            // and no migration can fire, because nothing looks legacy any more.
            let regime_tag = match established_regime {
                SenderRegimeState::DurableV1 => SENDER_REGIME_DURABLE_V1,
                // `TransitionToDurableV1` cannot reach here — it always returns. It
                // folds to the conservative tag rather than being spelled out, so a
                // future variant cannot silently acquire durable-v1 semantics merely
                // by being added to the enum.
                _ => SENDER_REGIME_LEGACY_OR_UNPROVEN,
            };
            if let Err(e) = self.persist_max_seq_durable(&principal, envelope.sequence, regime_tag)
            {
                return Err(anyhow::Error::new(ReplayStateNotDurable {
                    peer: envelope.from.as_str().to_string(),
                    sequence: envelope.sequence,
                })
                .context(e));
            }
        }

        // Durable (or unchanged) — now record acceptance in memory.
        let window = self
            .sequences
            .entry(principal)
            .or_insert_with(SequenceWindow::new);
        if max_seq_changed {
            window.max_seq = envelope.sequence;
        }
        window.insert_sequence(&seq_hash);
        window.last_update = Instant::now();

        Ok(())
    }

    /// Finalize a sequence number (permanently prevent replay)
    ///
    /// Call this after successfully processing a message (e.g., ledger entry written).
    /// Once finalized, the sequence cannot be replayed even within the time window.
    ///
    /// # Example
    /// ```ignore
    /// // Check message
    /// replay_guard.check(&envelope)?;
    ///
    /// // Process message (write to ledger, etc.)
    /// ledger.append(entry)?;
    ///
    /// // Finalize to prevent replay
    /// replay_guard.finalize(&envelope.from, envelope.sequence)?;
    /// ```
    pub fn finalize(&mut self, sender: &Did, sequence: u64) -> Result<()> {
        // Fail closed, exactly as on the accept path: a DID that names no key names no replay
        // state, and finalizing under its spelling would create a row nothing can ever match.
        let principal = SenderPrincipal::from_did(sender).map_err(|e| {
            anyhow::Error::new(ReplayIdentityUndecodable {
                peer: sender.as_str().to_string(),
                sequence,
            })
            .context(e)
        })?;

        let window = self
            .sequences
            .get_mut(&principal)
            .context("Cannot finalize sequence for unknown sender")?;

        window.finalized.insert(sequence, Instant::now());

        // Persist finalized sequence
        if let Err(e) = self.persist_finalized(&principal, sequence) {
            tracing::warn!(
                peer = %sender,
                seq = sequence,
                error = %e,
                "Failed to persist finalized sequence"
            );
        }

        Ok(())
    }

    /// Check if a sequence is finalized
    ///
    /// **Fails closed.** `true` is the blocking answer, so a DID whose replay identity cannot
    /// be derived reports `true` rather than reporting "not finalized" for a sender this node
    /// cannot even name (#2640). Returning `false` there would be a permissive fallback in a
    /// predicate whose whole job is to refuse.
    pub fn is_finalized(&self, sender: &Did, sequence: u64) -> bool {
        let Ok(principal) = SenderPrincipal::from_did(sender) else {
            tracing::error!(
                peer = %sender,
                sequence,
                "Replay identity could not be derived; reporting the sequence as finalized \
                 rather than answering for a sender that cannot be identified"
            );
            return true;
        };
        self.sequences
            .get(&principal)
            .map(|w| w.finalized.contains_key(&sequence))
            .unwrap_or(false)
    }

    /// Cleanup old peer state to prevent unbounded memory growth
    ///
    /// Should be called periodically (e.g., every 60 seconds)
    /// Prunes:
    /// - Inactive peer windows (no messages in max_peer_age_secs)
    /// - Old finalized sequences (>24 hours old)
    ///
    /// Also cleans up corresponding persistent storage.
    pub fn cleanup(&mut self) {
        let max_age = Duration::from_secs(self.max_peer_age_secs);
        let finalized_max_age = Duration::from_secs(24 * 60 * 60); // 24 hours
        let now = Instant::now();

        // Collect senders to remove from storage
        let mut principals_to_remove: Vec<SenderPrincipal> = Vec::new();

        // Remove inactive peer windows
        self.sequences.retain(|principal, window| {
            let keep = now.duration_since(window.last_update) < max_age;
            if !keep {
                principals_to_remove.push(*principal);
            }
            keep
        });

        // Delete the numeric high-water from storage.
        //
        // Deliberately NOT the sender-regime provenance record (#2517). The high-water
        // is a window that legitimately ages out; the provenance answers "did we ever
        // prove this DID's legacy namespace was retired?", and that does not stop
        // being true because the peer went quiet for an hour.
        //
        // Deleting it would make routine garbage collection manufacture the unsafe
        // precondition the migration exists to prevent: a receiver that once knew
        // better would be back to holding no proof, and would either re-impose the
        // migration hold forever or — far worse, if absence were ever read as
        // permission — accept a captured legacy sequence as a durable-v1 high-water.
        // Provenance is a few bytes per DID ever seen, bounded by federation size.
        if let Some(ref store) = self.store {
            for principal in &principals_to_remove {
                let key = Self::make_max_seq_key(principal);
                if let Err(e) = store.delete(&key) {
                    tracing::warn!(peer = %principal, error = %e, "Failed to delete max_seq from storage");
                }
            }
        }

        // Prune old finalized sequences from remaining windows
        let cutoff_ms = Self::current_time_ms().saturating_sub(24 * 60 * 60 * 1000);

        for (principal, window) in self.sequences.iter_mut() {
            let old_finalized: Vec<u64> = window
                .finalized
                .iter()
                .filter(|(_, &finalized_at)| now.duration_since(finalized_at) >= finalized_max_age)
                .map(|(&seq, _)| seq)
                .collect();

            for seq in &old_finalized {
                window.finalized.remove(seq);

                // Delete from storage
                if let Some(ref store) = self.store {
                    let key = Self::make_finalized_key(principal, *seq);
                    if let Err(e) = store.delete(&key) {
                        tracing::warn!(
                            peer = %principal,
                            seq = seq,
                            error = %e,
                            "Failed to delete finalized sequence from storage"
                        );
                    }
                }
            }
        }

        // Also clean up old finalized entries from storage that may not be in memory
        if let Some(ref store) = self.store {
            if let Ok(entries) = store.scan(FINALIZED_PREFIX) {
                for (key, value) in entries {
                    if let Ok(entry) = serde_json::from_slice::<FinalizedEntry>(&value) {
                        if entry.finalized_at_ms < cutoff_ms {
                            if let Err(e) = store.delete(&key) {
                                tracing::warn!(error = %e, "Failed to delete old finalized entry");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get the number of tracked peers
    pub fn peer_count(&self) -> usize {
        self.sequences.len()
    }

    /// Get the max sequence seen for a specific peer
    ///
    /// Keyed by the sender's key, so every spelling of one sender reports the one high-water
    /// (#2640). `None` for a DID that names no key, which is the same "no such sender" answer
    /// it already gives for a sender never seen.
    pub fn get_max_seq(&self, did: &Did) -> Option<u64> {
        let principal = SenderPrincipal::from_did(did).ok()?;
        self.sequences.get(&principal).map(|w| w.max_seq)
    }

    /// Check if the guard is using persistent storage
    pub fn is_persistent(&self) -> bool {
        self.store.is_some()
    }

    /// Check if the guard has been initialized (loaded from storage)
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    // -------------------------------------------------------------------------
    // Persistence helpers
    // -------------------------------------------------------------------------

    /// How long a pre-restart envelope can still pass freshness, measured from
    /// the restart on the receiver's own clock.
    ///
    /// Derived, not chosen. `verify_age` admits an envelope with timestamp `T`
    /// while the receiver's wall clock lies in `[T - max_age, T + max_age]`, so
    /// the bound is the sum of two *independent* tolerances:
    ///
    /// * **maximum permitted future skew** — an envelope accepted just before a
    ///   restart at `R` passed freshness then, so `T <= R + future_skew`;
    /// * **maximum permitted past age** — it then remains valid until
    ///   `now > T + max_age`.
    ///
    /// Worst case `T = R + future_skew`, giving validity until
    /// `R + future_skew + max_age`. In the current envelope implementation both
    /// tolerances are the same quantity (`verify_age` compares against
    /// `max_age_ms` in both directions), so this is `2 * max_age`.
    ///
    /// A quarantine of only one `max_age` would end while such an envelope has
    /// age ~0 — i.e. at its *freshest* — which is why the naive single-interval
    /// arithmetic is wrong.
    fn envelope_validity_horizon(&self) -> Duration {
        let future_skew = self.max_clock_skew;
        let max_past_age = self.max_clock_skew;
        Duration::from_secs(future_skew.saturating_add(max_past_age))
    }

    /// Persist the high-water **and force it durable** before returning.
    ///
    /// The flush is what makes the restored floor equal the highest sequence
    /// ever accepted rather than merely the highest that reached the page cache.
    /// `Store::put` on the sled backend is a buffered insert, and `sled::open()`
    /// defaults to `flush_every_ms = Some(500)`, so without this a crash can
    /// lose the most recent acceptances.
    ///
    /// This mirrors what the outbound side already does for its reservation
    /// watermark in `signing_sequence.rs`; the receiving side was the half that
    /// never got it.
    fn persist_max_seq_durable(
        &self,
        principal: &SenderPrincipal,
        max_seq: u64,
        sender_regime: u32,
    ) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode: no durability to promise
        };

        self.persist_max_seq_inner(principal, max_seq, sender_regime)?;
        store
            .flush()
            .context("Failed to flush replay high-water to durable storage")?;
        Ok(())
    }

    fn persist_max_seq_inner(
        &self,
        principal: &SenderPrincipal,
        max_seq: u64,
        sender_regime: u32,
    ) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode
        };

        let key = Self::make_max_seq_key(principal);
        let entry = MaxSeqEntry {
            max_seq,
            updated_at_ms: Self::current_time_ms(),
            // Stamped on every write, so a peer that completes migration is
            // promoted to current semantics by the first ordinary acceptance —
            // there is no separate promotion step to crash between.
            semantic_version: REPLAY_STATE_SEMANTIC_VERSION,
            // Supplied by the caller, never inferred here. This function knows
            // which of *our* versions is writing; only the caller knows which
            // sender namespace the number came from.
            sender_regime,
        };
        let value = serde_json::to_vec(&entry).context("Failed to serialize max_seq entry")?;

        store
            .put(&key, &value)
            .context("Failed to persist max_seq")?;

        Ok(())
    }

    fn persist_finalized(&self, principal: &SenderPrincipal, sequence: u64) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode
        };

        let key = Self::make_finalized_key(principal, sequence);
        let entry = FinalizedEntry {
            finalized_at_ms: Self::current_time_ms(),
        };
        let value = serde_json::to_vec(&entry).context("Failed to serialize finalized entry")?;

        store
            .put(&key, &value)
            .context("Failed to persist finalized sequence")?;

        Ok(())
    }

    /// Persist established sender-regime provenance, and flush before returning.
    ///
    /// Written only at state transitions, not per message: provenance changes rarely
    /// and an extra flush on the accept path would cost more than the whole #2514
    /// durability guarantee does.
    ///
    /// Only [`SENDER_REGIME_TRANSITION_TO_DURABLE_V1`] and
    /// [`SENDER_REGIME_DURABLE_V1`] are ever written. An absent record means
    /// "unproven", which is both the safe default and the common case, so the
    /// overwhelming majority of peers cost no provenance write at all.
    fn persist_sender_regime(&self, principal: &SenderPrincipal, regime: u32) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()),
        };
        store
            .put(
                &Self::make_sender_regime_key(principal),
                &regime.to_be_bytes(),
            )
            .context("Failed to persist sender regime provenance")?;
        store
            .flush()
            .context("Failed to flush sender regime provenance")?;
        Ok(())
    }

    /// Durable keys are built from the principal's **canonical** spelling (#2640).
    ///
    /// Reading stays textual and greppable, which is what the operator tooling and the tests
    /// below expect, but the text is now derived from the key rather than copied off the wire,
    /// so no spelling an attacker invents can open a second row.
    /// Which in-memory window a durable row belongs to (#2640).
    ///
    /// `Err(())` means the row's DID names no Ed25519 key, so it is the state of no sender
    /// whose envelopes this node could ever verify: `Did::deserialize` refuses such a DID on
    /// the wire, and `verify_classical` derives the very same key before the guard is
    /// consulted. Such a row therefore bounds nothing, and inventing a principal for it would
    /// be the guess this fix exists to remove. It is logged, skipped, and — importantly —
    /// left in the store rather than deleted, so an operator can still see it.
    fn window_key(did: &Did, keyspace: &str) -> std::result::Result<SenderPrincipal, ()> {
        SenderPrincipal::from_did(did).map_err(|e| {
            tracing::error!(
                keyspace,
                stored_did = %did,
                error = %e,
                "Durable replay row is stored under a DID that decodes to no Ed25519 key; \
                 skipping it. No verifiable sender can be identified by it, so it bounds \
                 nothing — but it is left in the store rather than deleted"
            );
        })
    }

    // -------------------------------------------------------------------------
    // #2640 — durable identity canonicalization
    // -------------------------------------------------------------------------

    /// Collapse spelling-distinct durable rows for one sender onto one canonical key.
    ///
    /// Runs *before* the ordinary load, so everything downstream sees at most one readable
    /// row per sender (and per `(sender, sequence)` for finalized rows) and the #2514 / #2517
    /// state machine is left exactly as it was.
    ///
    /// # The merge rule, axis by axis, and why each is the conservative direction
    ///
    /// **`max_seq` — maximum.** The floor rejects `sequence <= floor`, so a larger floor
    /// rejects a superset; it is the only direction that cannot admit a replay. An alias
    /// number may belong to a namespace the canonical number does not, which makes the
    /// maximum potentially *over*-restrictive — that is the accepted cost. What happened
    /// before is the opposite: two rows landed in one window by last-`sled`-key-wins, so a
    /// **lower** floor could win (N2-A0 inventory, row #2).
    ///
    /// **`sender_regime` — `DurableV1` > `TransitionToDurableV1` > `LegacyOrUnproven`.** A
    /// safety ordering, not a recency one. Both lower states reach a promotion that resets
    /// `max_seq` and `floor_seq` to 0; `DurableV1` never does. Choosing the strongest is the
    /// only choice that cannot destroy the merged floor. An unrecognised tag is preserved
    /// as-is and the existing unsupported-regime hold refuses the sender with no deadline.
    ///
    /// **`semantic_version` — the most restrictive.** Any unrecognised version wins (a
    /// no-deadline hold), else the legacy version wins (a bounded hold that discards the
    /// number), else the current one. Discarding a good number under a legacy hold is safe
    /// for the reason that path already gives: the hold outlasts the freshness of anything
    /// written under that regime, so the floor is not what is rejecting replays during it.
    ///
    /// **`finalized` — set union**, keeping the later `finalized_at_ms` on collision so the
    /// entry survives the 24h prune at least as long. A union can only block more sequences.
    ///
    /// # What is refused rather than merged
    ///
    /// A row whose value does not deserialize is **not** merged and **not** deleted. The load
    /// pass turns exactly that row into a quarantine hold on the merged sender, which is the
    /// fail-closed answer to "we had state here and cannot read it". Silently dropping it
    /// would be the one merge outcome that loses a bound.
    ///
    /// # Ordering and crash safety
    ///
    /// The canonical row is written **and flushed** before any alias row is deleted, so no
    /// interruption can leave a sender holding less state than it started with. Every axis of
    /// the merge is idempotent and order-independent (maximum, strongest, union), so a crash
    /// between the write and the deletes re-merges to the identical value on the next start.
    ///
    /// A sender whose only row is already the canonical one costs **zero writes**. That is
    /// every honest peer: production signs only with `keypair.did()`, which is
    /// `Did::from_public_key`, which is the canonical base58btc spelling — so this pass is a
    /// no-op except where an alias actually put a row.
    fn canonicalize_durable_identities(&self) -> Result<()> {
        let store = match &self.store {
            Some(s) => s.clone(),
            None => return Ok(()),
        };
        Self::canonicalize_max_seq_rows(store.as_ref())?;
        Self::canonicalize_sender_regime_rows(store.as_ref())?;
        Self::canonicalize_finalized_rows(store.as_ref())?;
        Ok(())
    }

    /// The merge itself, for one `replay_max_seq` row against another.
    fn merge_max_seq(a: &MaxSeqEntry, b: &MaxSeqEntry) -> MaxSeqEntry {
        MaxSeqEntry {
            max_seq: a.max_seq.max(b.max_seq),
            updated_at_ms: a.updated_at_ms.max(b.updated_at_ms),
            semantic_version: Self::most_restrictive_semantic_version(
                a.semantic_version,
                b.semantic_version,
            ),
            sender_regime: Self::strongest_sender_regime(a.sender_regime, b.sender_regime),
        }
    }

    /// Ranked by how much the load pass will refuse, not by how new the version is.
    ///
    /// An unrecognised version produces a hold with no deadline, the legacy version produces
    /// a bounded hold that discards the number, and the current version produces a usable
    /// floor — so "most restrictive" is exactly that order. Two different unrecognised
    /// versions resolve to the larger purely so the result is deterministic; both refuse the
    /// sender indefinitely either way.
    fn most_restrictive_semantic_version(a: u32, b: u32) -> u32 {
        let rank = |v: u32| match v {
            REPLAY_STATE_SEMANTIC_VERSION => 0u8,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION => 1,
            _ => 2,
        };
        match rank(a).cmp(&rank(b)) {
            std::cmp::Ordering::Less => b,
            std::cmp::Ordering::Greater => a,
            std::cmp::Ordering::Equal => a.max(b),
        }
    }

    /// `DurableV1` > `TransitionToDurableV1` > `LegacyOrUnproven`, and an unrecognised tag
    /// above all three.
    ///
    /// This is a safety ordering, not a lifecycle one. The two lower states each reach a
    /// promotion that resets `max_seq` and `floor_seq` to 0, so adopting either would let the
    /// merged floor be destroyed later; `DurableV1` has no such path. An unrecognised tag
    /// ranks highest because the load pass refuses it with no deadline.
    fn strongest_sender_regime(a: u32, b: u32) -> u32 {
        let rank = |v: u32| match v {
            SENDER_REGIME_LEGACY_OR_UNPROVEN => 0u8,
            SENDER_REGIME_TRANSITION_TO_DURABLE_V1 => 1,
            SENDER_REGIME_DURABLE_V1 => 2,
            _ => 3,
        };
        match rank(a).cmp(&rank(b)) {
            std::cmp::Ordering::Less => b,
            std::cmp::Ordering::Greater => a,
            std::cmp::Ordering::Equal => a.max(b),
        }
    }

    /// Write the merged row, flush it, and only then retire the spellings it absorbed.
    ///
    /// Returns without writing anything when the sender's only row is already the canonical
    /// one, which is every honest peer on every start.
    fn install_canonical_row(
        store: &dyn Store,
        canonical_key: &[u8],
        value: &[u8],
        source_keys: &[Vec<u8>],
        unreadable: &std::collections::HashSet<Vec<u8>>,
        keyspace: &str,
    ) -> Result<()> {
        if source_keys.len() == 1 && source_keys[0] == canonical_key {
            return Ok(());
        }

        // The canonical key is exactly where an unreadable row must be left alone. Writing the
        // merge over it would erase the one fact the load pass turns into a quarantine — "state
        // existed here and we cannot read it" — and replace it with a floor derived only from
        // the spellings we *could* read, which may be lower. The whole group is therefore left
        // as it is: nothing is written and nothing is retired, and the load pass holds the
        // sender exactly as it would have before this migration existed. That hold is bounded
        // by the envelope validity horizon, after which no pre-restart capture can pass
        // freshness anyway, so leaving the readable floors unmerged costs nothing.
        if unreadable.contains(canonical_key) {
            tracing::error!(
                keyspace,
                "The canonical replay row for a sender is unreadable; leaving its alias rows \
                 in place rather than overwriting the evidence that quarantines it (#2640)"
            );
            return Ok(());
        }

        store
            .put(canonical_key, value)
            .with_context(|| format!("Failed to write the merged {keyspace} row"))?;
        // Durable *before* any alias is retired. An interruption here may leave duplicate
        // rows, which re-merge to the identical value on the next start; the order that must
        // never happen is the one where a sender ends up with less state than it had.
        store.flush().with_context(|| {
            format!("Failed to flush the merged {keyspace} row before retiring alias rows")
        })?;

        for stale in source_keys.iter().filter(|k| k.as_slice() != canonical_key) {
            store
                .delete(stale)
                .with_context(|| format!("Failed to retire an alias {keyspace} row"))?;
        }
        Ok(())
    }

    fn canonicalize_max_seq_rows(store: &dyn Store) -> Result<()> {
        use std::collections::hash_map::Entry;

        let rows = store
            .scan(MAX_SEQ_PREFIX)
            .context("Failed to scan replay max_seq entries for canonicalization")?;

        let mut grouped: HashMap<SenderPrincipal, (MaxSeqEntry, Vec<Vec<u8>>)> = HashMap::new();
        let mut unreadable: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
        for (key, raw) in rows {
            let Some(did) = Self::parse_max_seq_key(&key) else {
                continue;
            };
            let Ok(principal) = Self::window_key(&did, "replay max_seq") else {
                continue;
            };
            let Ok(entry) = serde_json::from_slice::<MaxSeqEntry>(&raw) else {
                // Unreadable. Neither merged nor deleted — the load pass turns exactly this
                // row into a quarantine hold on the merged sender.
                unreadable.insert(key);
                continue;
            };
            match grouped.entry(principal) {
                Entry::Occupied(mut occupied) => {
                    let (acc, keys) = occupied.get_mut();
                    *acc = Self::merge_max_seq(acc, &entry);
                    keys.push(key);
                }
                Entry::Vacant(vacant) => {
                    vacant.insert((entry, vec![key]));
                }
            }
        }

        for (principal, (entry, source_keys)) in grouped {
            let canonical_key = Self::make_max_seq_key(&principal);
            let aliases = source_keys
                .iter()
                .filter(|k| k.as_slice() != canonical_key.as_slice())
                .count();
            if aliases > 0 {
                tracing::warn!(
                    peer = %principal,
                    alias_rows = aliases,
                    merged_max_seq = entry.max_seq,
                    merged_sender_regime = entry.sender_regime,
                    merged_semantic_version = entry.semantic_version,
                    "Collapsing spelling-distinct replay high-water rows onto one sender \
                     (#2640). The merged floor is the maximum across them, so it can only \
                     reject more than any of them did"
                );
            }
            let value =
                serde_json::to_vec(&entry).context("Failed to serialize merged max_seq entry")?;
            Self::install_canonical_row(
                store,
                &canonical_key,
                &value,
                &source_keys,
                &unreadable,
                "replay max_seq",
            )?;
        }
        Ok(())
    }

    fn canonicalize_sender_regime_rows(store: &dyn Store) -> Result<()> {
        use std::collections::hash_map::Entry;

        let rows = store
            .scan(SENDER_REGIME_PREFIX)
            .context("Failed to scan sender regime provenance for canonicalization")?;

        let mut grouped: HashMap<SenderPrincipal, (u32, Vec<Vec<u8>>)> = HashMap::new();
        let mut unreadable: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
        for (key, raw) in rows {
            let Some(did) = Self::parse_sender_regime_key(&key) else {
                continue;
            };
            let Ok(principal) = Self::window_key(&did, "replay sender regime") else {
                continue;
            };
            let Ok(bytes) = <[u8; 4]>::try_from(raw.as_slice()) else {
                unreadable.insert(key); // left for the load pass to quarantine on
                continue;
            };
            let regime = u32::from_be_bytes(bytes);
            match grouped.entry(principal) {
                Entry::Occupied(mut occupied) => {
                    let (acc, keys) = occupied.get_mut();
                    *acc = Self::strongest_sender_regime(*acc, regime);
                    keys.push(key);
                }
                Entry::Vacant(vacant) => {
                    vacant.insert((regime, vec![key]));
                }
            }
        }

        for (principal, (regime, source_keys)) in grouped {
            let canonical_key = Self::make_sender_regime_key(&principal);
            if source_keys
                .iter()
                .any(|k| k.as_slice() != canonical_key.as_slice())
            {
                tracing::warn!(
                    peer = %principal,
                    merged_regime = regime,
                    "Collapsing spelling-distinct sender regime provenance onto one sender \
                     (#2640); the strongest established regime wins, because the weaker ones \
                     each reach a promotion that would reset the replay floor"
                );
            }
            Self::install_canonical_row(
                store,
                &canonical_key,
                &regime.to_be_bytes(),
                &source_keys,
                &unreadable,
                "replay sender regime",
            )?;
        }
        Ok(())
    }

    fn canonicalize_finalized_rows(store: &dyn Store) -> Result<()> {
        use std::collections::hash_map::Entry;

        let rows = store
            .scan(FINALIZED_PREFIX)
            .context("Failed to scan finalized entries for canonicalization")?;

        /// One finalized sequence for one sender, with every stored key that supplied it.
        type FinalizedGroups = HashMap<(SenderPrincipal, u64), (FinalizedEntry, Vec<Vec<u8>>)>;
        let mut grouped: FinalizedGroups = HashMap::new();
        let mut unreadable: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
        for (key, raw) in rows {
            let Some((did, sequence)) = Self::parse_finalized_key(&key) else {
                continue;
            };
            let Ok(principal) = Self::window_key(&did, "replay finalized") else {
                continue;
            };
            let Ok(entry) = serde_json::from_slice::<FinalizedEntry>(&raw) else {
                unreadable.insert(key);
                continue;
            };
            match grouped.entry((principal, sequence)) {
                Entry::Occupied(mut occupied) => {
                    let (acc, keys) = occupied.get_mut();
                    // The later stamp, so the entry outlives the 24h prune at least as long
                    // as the longest-lived spelling did. A finalized set only ever blocks.
                    acc.finalized_at_ms = acc.finalized_at_ms.max(entry.finalized_at_ms);
                    keys.push(key);
                }
                Entry::Vacant(vacant) => {
                    vacant.insert((entry, vec![key]));
                }
            }
        }

        for ((principal, sequence), (entry, source_keys)) in grouped {
            let canonical_key = Self::make_finalized_key(&principal, sequence);
            let value =
                serde_json::to_vec(&entry).context("Failed to serialize merged finalized entry")?;
            Self::install_canonical_row(
                store,
                &canonical_key,
                &value,
                &source_keys,
                &unreadable,
                "replay finalized",
            )?;
        }
        Ok(())
    }

    fn make_sender_regime_key(principal: &SenderPrincipal) -> Vec<u8> {
        let mut key = SENDER_REGIME_PREFIX.to_vec();
        key.extend_from_slice(principal.canonical_did().as_str().as_bytes());
        key
    }

    fn parse_sender_regime_key(key: &[u8]) -> Option<Did> {
        let rest = key.strip_prefix(SENDER_REGIME_PREFIX)?;
        let did_str = std::str::from_utf8(rest).ok()?;
        Did::from_str(did_str).ok()
    }

    fn make_max_seq_key(principal: &SenderPrincipal) -> Vec<u8> {
        let mut key = Vec::with_capacity(MAX_SEQ_PREFIX.len() + 100);
        key.extend_from_slice(MAX_SEQ_PREFIX);
        key.extend_from_slice(principal.canonical_did().as_str().as_bytes());
        key
    }

    fn parse_max_seq_key(key: &[u8]) -> Option<Did> {
        if !key.starts_with(MAX_SEQ_PREFIX) {
            return None;
        }
        let rest = &key[MAX_SEQ_PREFIX.len()..];
        let did_str = std::str::from_utf8(rest).ok()?;
        Did::from_str(did_str).ok()
    }

    fn make_finalized_key(principal: &SenderPrincipal, sequence: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(FINALIZED_PREFIX.len() + 120);
        key.extend_from_slice(FINALIZED_PREFIX);
        key.extend_from_slice(principal.canonical_did().as_str().as_bytes());
        key.push(b':');
        key.extend_from_slice(sequence.to_string().as_bytes());
        key
    }

    fn parse_finalized_key(key: &[u8]) -> Option<(Did, u64)> {
        if !key.starts_with(FINALIZED_PREFIX) {
            return None;
        }
        let rest = &key[FINALIZED_PREFIX.len()..];
        let rest_str = std::str::from_utf8(rest).ok()?;

        // Find the last colon (sequence is after it)
        let colon_pos = rest_str.rfind(':')?;
        let did_str = &rest_str[..colon_pos];
        let seq_str = &rest_str[colon_pos + 1..];

        let did = Did::from_str(did_str).ok()?;
        let seq = seq_str.parse().ok()?;

        Some((did, seq))
    }

    fn current_time_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl SequenceWindow {
    /// Create a new sequence window
    ///
    /// Bloom filter sized for:
    /// - 10,000 recent sequences
    /// - 0.1% false positive rate
    /// - ~10KB memory per peer
    fn new() -> Self {
        SequenceWindow {
            max_seq: 0,
            floor_seq: 0,
            recent: BloomFilter::new(BLOOM_CAPACITY, 0.001),
            insertion_count: 0,
            finalized: HashMap::new(),
            last_update: Instant::now(),
            hold: None,
            // Nothing is proven about this peer yet, which is exactly the same
            // decision as "known to be legacy": we hold no evidence that its legacy
            // namespace was retired, so a durable claim must be held either way.
            sender_regime: SenderRegimeState::LegacyOrUnproven,
        }
    }

    /// Insert a sequence hash into the Bloom filter, rotating if necessary
    ///
    /// When the filter approaches saturation (80% capacity), it is reset
    /// to prevent false positives. The max_seq provides replay protection
    /// for sequences below the threshold even after reset.
    fn insert_sequence(&mut self, seq_hash: &[u8; 32]) {
        // Check if we need to rotate before inserting
        if self.insertion_count >= BLOOM_ROTATION_THRESHOLD {
            self.rotate_bloom_filter();
        }

        self.recent.insert(seq_hash);
        self.insertion_count += 1;
    }

    /// Rotate (reset) the Bloom filter to prevent saturation (#154)
    ///
    /// After rotation:
    /// - The filter is empty and can accept new sequences
    /// - max_seq still prevents replay of old sequences
    /// - Finalized sequences are still protected
    /// - There's a brief window where some out-of-order sequences
    ///   might be accepted twice, but this is acceptable as:
    ///   1. Finalized sequences are never replayed
    ///   2. Double-processing of non-finalized sequences is idempotent
    fn rotate_bloom_filter(&mut self) {
        tracing::debug!(
            max_seq = self.max_seq,
            insertion_count = self.insertion_count,
            "Rotating Bloom filter to prevent saturation"
        );
        self.recent = BloomFilter::new(BLOOM_CAPACITY, 0.001);
        self.insertion_count = 0;

        // Track rotation for monitoring (#154)
        icn_obs::metrics::network::replay_guard_bloom_rotations_inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::PayloadType;
    use icn_identity::KeyPair;

    #[test]
    fn test_fresh_message_accepted() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // First delivery: OK
        assert!(guard.check_durable(&envelope).is_ok());
        assert_eq!(guard.get_max_seq(keypair.did()), Some(1));
    }

    #[test]
    fn test_replay_rejected() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // First delivery: OK
        assert!(guard.check_durable(&envelope).is_ok());

        // Replay: Rejected
        let result = guard.check_durable(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Replay detected"));
    }

    #[test]
    fn test_monotonic_sequences_accepted() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Send messages 1, 2, 3 in order
        for seq in 1..=3 {
            let envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                seq,
                PayloadType::Gossip,
                format!("test {seq}").as_bytes().to_vec(),
            )
            .unwrap();

            assert!(guard.check_durable(&envelope).is_ok());
        }

        assert_eq!(guard.get_max_seq(keypair.did()), Some(3));
    }

    #[test]
    fn test_out_of_order_accepted_once() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Send sequence 3 first
        let env3 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            3,
            PayloadType::Gossip,
            b"msg3".to_vec(),
        )
        .unwrap();
        assert!(guard.check_durable(&env3).is_ok());
        assert_eq!(guard.get_max_seq(keypair.did()), Some(3));

        // Send sequence 2 (out of order but not a replay)
        let env2 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            2,
            PayloadType::Gossip,
            b"msg2".to_vec(),
        )
        .unwrap();
        assert!(guard.check_durable(&env2).is_ok());

        // Try to replay sequence 2 (should be rejected)
        let result = guard.check_durable(&env2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Replay detected"));
    }

    #[test]
    fn test_multiple_peers_independent() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair1 = KeyPair::generate().unwrap();
        let keypair2 = KeyPair::generate().unwrap();

        // Send seq 1 from peer 1
        let env1 = SignedEnvelope::new(
            keypair1.did(),
            &keypair1,
            1,
            PayloadType::Gossip,
            b"peer1-msg1".to_vec(),
        )
        .unwrap();
        assert!(guard.check_durable(&env1).is_ok());

        // Send seq 1 from peer 2 (different peer, should be OK)
        let env2 = SignedEnvelope::new(
            keypair2.did(),
            &keypair2,
            1,
            PayloadType::Gossip,
            b"peer2-msg1".to_vec(),
        )
        .unwrap();
        assert!(guard.check_durable(&env2).is_ok());

        assert_eq!(guard.peer_count(), 2);
        assert_eq!(guard.get_max_seq(keypair1.did()), Some(1));
        assert_eq!(guard.get_max_seq(keypair2.did()), Some(1));
    }

    #[test]
    fn test_cleanup_removes_old_peers() {
        let mut guard = ReplayGuard::new(300, 1); // 1 second max age
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        assert!(guard.check_durable(&envelope).is_ok());
        assert_eq!(guard.peer_count(), 1);

        // Wait for peer to age out
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Cleanup should remove the peer
        guard.cleanup();
        assert_eq!(guard.peer_count(), 0);
    }

    #[test]
    fn test_invalid_signature_rejected_before_sequence_check() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair1 = KeyPair::generate().unwrap();
        let keypair2 = KeyPair::generate().unwrap();

        // Create envelope signed by keypair1 but claiming to be from keypair2
        let envelope = SignedEnvelope::new(
            keypair2.did(), // Claim to be keypair2
            &keypair1,      // But sign with keypair1
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // Deliberately NOT `check_durable`: that helper pre-establishes the peer's
        // regime, which would create the very window this test asserts is absent.
        let result = guard.check(&envelope, ObservedSenderRegime::DurableV1);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("signature verification failed"));

        // No sequence state should be created for invalid messages
        assert_eq!(guard.peer_count(), 0);
    }

    #[test]
    fn test_finalize_prevents_replay() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"transaction".to_vec(),
        )
        .unwrap();

        // First check: OK
        assert!(guard.check_durable(&envelope).is_ok());

        // Finalize sequence (transaction processed)
        assert!(guard.finalize(keypair.did(), 1).is_ok());
        assert!(guard.is_finalized(keypair.did(), 1));

        // Attempt replay after finalization: REJECTED
        let result = guard.check_durable(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalized"));
    }

    #[test]
    fn test_finalize_different_sequence_independent() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope1 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"tx1".to_vec(),
        )
        .unwrap();

        let envelope2 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            2,
            PayloadType::Ledger,
            b"tx2".to_vec(),
        )
        .unwrap();

        // Check both
        assert!(guard.check_durable(&envelope1).is_ok());
        assert!(guard.check_durable(&envelope2).is_ok());

        // Finalize sequence 1 only
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // Sequence 1 blocked (finalized)
        assert!(guard.check_durable(&envelope1).is_err());

        // Sequence 2 blocked (already in Bloom filter from first check)
        // But NOT finalized, so if we create a NEW envelope with seq 3, it should work
        let envelope3 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            3,
            PayloadType::Ledger,
            b"tx3".to_vec(),
        )
        .unwrap();

        assert!(guard.check_durable(&envelope3).is_ok());

        // Finalize sequence 2
        assert!(guard.finalize(keypair.did(), 2).is_ok());

        // Now envelope3 can still be used (not finalized)
        // But envelope2 would be rejected as finalized if we check again
        assert!(guard.is_finalized(keypair.did(), 2));
        assert!(!guard.is_finalized(keypair.did(), 3));
    }

    #[test]
    fn test_replay_within_time_window_after_finalization() {
        // This is the KEY test - prevents the documented vulnerability
        let mut guard = ReplayGuard::new(300, 3600); // 5 minute window
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"critical_transaction".to_vec(),
        )
        .unwrap();

        // T=0: Transaction submitted
        assert!(guard.check_durable(&envelope).is_ok());

        // T=1: Transaction processed, finalize
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // T=2: Attacker replays within 5-minute window
        // WITHOUT finalization: would be accepted (vulnerability)
        // WITH finalization: REJECTED (fixed)
        let result = guard.check_durable(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalized"));
    }

    #[test]
    fn test_finalized_sequences_pruned_after_24h() {
        let mut guard = ReplayGuard::new(300, 1); // 1 second peer age for fast test
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Ledger,
            b"tx".to_vec(),
        )
        .unwrap();

        assert!(guard.check_durable(&envelope).is_ok());
        assert!(guard.finalize(keypair.did(), 1).is_ok());
        assert!(guard.is_finalized(keypair.did(), 1));

        // In real usage, finalized sequences are pruned after 24h
        // For testing, we just verify cleanup doesn't crash with finalized seqs
        guard.cleanup();

        // Peer still tracked (finalized sequences kept)
        assert_eq!(guard.peer_count(), 1);
    }

    #[test]
    fn test_bloom_filter_rotation() {
        // Test that Bloom filter rotates to prevent saturation
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Send many messages (more than BLOOM_ROTATION_THRESHOLD)
        for seq in 1..=9000 {
            let envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                seq,
                PayloadType::Gossip,
                format!("msg{seq}").as_bytes().to_vec(),
            )
            .unwrap();

            // All should be accepted
            assert!(
                guard.check_durable(&envelope).is_ok(),
                "Message {seq} should be accepted"
            );
        }

        // Verify max_seq was tracked correctly
        assert_eq!(guard.get_max_seq(keypair.did()), Some(9000));

        // After rotation, new messages should still be accepted
        let new_envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            9001,
            PayloadType::Gossip,
            b"new_msg".to_vec(),
        )
        .unwrap();
        assert!(guard.check_durable(&new_envelope).is_ok());

        // Finalized sequences should still be protected after rotation
        guard.finalize(keypair.did(), 100).unwrap();
        let old_envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            100,
            PayloadType::Gossip,
            b"replayed".to_vec(),
        )
        .unwrap();
        let result = guard.check_durable(&old_envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalized"));
    }

    // -------------------------------------------------------------------------
    // Persistence tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_persistent_guard_creation() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let guard = ReplayGuard::new_persistent(300, 3600, store);

        assert!(guard.is_persistent());
        assert!(!guard.is_initialized());
    }

    #[test]
    fn test_persistence_and_restart_safety_gap() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let keypair = KeyPair::generate().unwrap();

        // Session 1: Create guard, check some messages
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();

            for seq in 1..=5 {
                let envelope = SignedEnvelope::new(
                    keypair.did(),
                    &keypair,
                    seq,
                    PayloadType::Gossip,
                    format!("msg{seq}").as_bytes().to_vec(),
                )
                .unwrap();
                assert!(guard.check_durable(&envelope).is_ok());
            }

            assert_eq!(guard.get_max_seq(keypair.did()), Some(5));
        }

        // Session 2: Simulate restart - create new guard from same store
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            let loaded = guard.load_persisted_state().unwrap();

            assert_eq!(loaded, 1); // One peer loaded

            // The restored high-water is exactly what was accepted — restart
            // must not advance it into sequences the sender never emitted.
            assert_eq!(guard.get_max_seq(keypair.did()), Some(5));

            // Replays of already-accepted sequences are still rejected.
            let old_envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                5,
                PayloadType::Gossip,
                b"replay_attempt".to_vec(),
            )
            .unwrap();
            let result = guard.check_durable(&old_envelope);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already seen"));

            // The sender's next sequence is accepted immediately.
            let new_envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                6,
                PayloadType::Gossip,
                b"new_msg".to_vec(),
            )
            .unwrap();
            assert!(guard.check_durable(&new_envelope).is_ok());
        }
    }

    #[test]
    fn test_finalized_persistence_across_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let keypair = KeyPair::generate().unwrap();

        // Session 1: Finalize a sequence
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();

            let envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                100,
                PayloadType::Ledger,
                b"critical_tx".to_vec(),
            )
            .unwrap();

            assert!(guard.check_durable(&envelope).is_ok());
            assert!(guard.finalize(keypair.did(), 100).is_ok());
        }

        // Session 2: Verify finalized sequence is still protected
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();

            assert!(guard.is_finalized(keypair.did(), 100));

            // Attempting to replay finalized sequence should fail
            let replay_envelope = SignedEnvelope::new(
                keypair.did(),
                &keypair,
                100,
                PayloadType::Ledger,
                b"replay_critical_tx".to_vec(),
            )
            .unwrap();
            let result = guard.check_durable(&replay_envelope);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("finalized"));
        }
    }

    // `test_multiple_restart_compounds_safety_gap` used to live here, asserting
    // that each restart pushed the acceptance boundary another 1,000 sequences
    // into the sender's future. That behaviour was the #2514 defect, not a
    // security requirement — no incident, test, or comment ever justified it.
    // The property is now owned, inverted, by
    // `test_repeated_restart_without_traffic_does_not_compound`.

    #[test]
    fn test_key_parsing() {
        let did = KeyPair::generate().unwrap().did().clone();

        // Max seq key
        let key = ReplayGuard::make_max_seq_key(&pk(&did));
        let parsed = ReplayGuard::parse_max_seq_key(&key).unwrap();
        assert_eq!(parsed.as_str(), did.as_str());

        // Finalized key
        let seq = 12345u64;
        let fkey = ReplayGuard::make_finalized_key(&pk(&did), seq);
        let (parsed_did, parsed_seq) = ReplayGuard::parse_finalized_key(&fkey).unwrap();
        assert_eq!(parsed_did.as_str(), did.as_str());
        assert_eq!(parsed_seq, seq);
    }

    #[test]
    fn test_auto_initialization() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let keypair = KeyPair::generate().unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store);

        // Not initialized yet
        assert!(!guard.is_initialized());

        // First check auto-initializes
        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        assert!(guard.check_durable(&envelope).is_ok());
        assert!(guard.is_initialized());
    }

    #[test]
    fn test_check_replay_only_skips_signature_verification() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Create a valid envelope
        let mut envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // Tamper with the signature (would fail signature verification)
        envelope.signature[0] ^= 0xFF;

        // check() should fail because signature is invalid
        let result = guard.check_durable(&envelope);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("signature"),
            "Should fail signature verification"
        );

        // But check_replay_only() should succeed because it skips signature
        // (caller is responsible for verifying signature first)
        assert!(
            guard.check_replay_only_durable(&envelope).is_ok(),
            "check_replay_only should skip signature verification"
        );
    }

    #[test]
    fn test_check_replay_only_still_detects_replays() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // First delivery via check_replay_only: OK
        assert!(guard.check_replay_only_durable(&envelope).is_ok());

        // Replay via check_replay_only: Rejected
        let result = guard.check_replay_only_durable(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Replay detected"));
    }

    #[test]
    fn test_check_replay_only_respects_finalization() {
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // First delivery
        assert!(guard.check_replay_only_durable(&envelope).is_ok());

        // Finalize
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // Replay of finalized sequence: Rejected
        let result = guard.check_replay_only_durable(&envelope);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalized"));
    }

    #[test]
    fn test_check_and_check_replay_only_share_state() {
        // Verifies that check() and check_replay_only() share the same
        // replay detection state (since check() now calls check_replay_only())
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Create two envelopes with different sequences
        let envelope1 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"msg1".to_vec(),
        )
        .unwrap();

        let envelope2 = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            2,
            PayloadType::Gossip,
            b"msg2".to_vec(),
        )
        .unwrap();

        // Use check() for first envelope
        assert!(guard.check_durable(&envelope1).is_ok());

        // Use check_replay_only() for second envelope
        assert!(guard.check_replay_only_durable(&envelope2).is_ok());

        // Both sequences should now be tracked
        assert_eq!(guard.get_max_seq(keypair.did()), Some(2));

        // Replaying envelope1 via check_replay_only should fail
        let result1 = guard.check_replay_only_durable(&envelope1);
        assert!(result1.is_err());
        assert!(result1.unwrap_err().to_string().contains("Replay detected"));

        // Replaying envelope2 via check() should also fail
        let result2 = guard.check_durable(&envelope2);
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("Replay detected"));
    }

    #[test]
    fn test_check_calls_check_replay_only_internally() {
        // Verifies that check() properly calls check_replay_only() by checking
        // that state is consistent between both methods
        let mut guard = ReplayGuard::new(300, 3600);
        let keypair = KeyPair::generate().unwrap();

        // Create a sequence of envelopes
        let mut envelopes = Vec::new();
        for seq in 1..=5 {
            envelopes.push(
                SignedEnvelope::new(
                    keypair.did(),
                    &keypair,
                    seq,
                    PayloadType::Gossip,
                    format!("msg{seq}").as_bytes().to_vec(),
                )
                .unwrap(),
            );
        }

        // Alternate between check() and check_replay_only()
        assert!(guard.check_durable(&envelopes[0]).is_ok()); // seq 1 via check()
        assert!(guard.check_replay_only_durable(&envelopes[1]).is_ok()); // seq 2 via check_replay_only()
        assert!(guard.check_durable(&envelopes[2]).is_ok()); // seq 3 via check()
        assert!(guard.check_replay_only_durable(&envelopes[3]).is_ok()); // seq 4 via check_replay_only()
        assert!(guard.check_durable(&envelopes[4]).is_ok()); // seq 5 via check()

        // All 5 sequences should be tracked
        assert_eq!(guard.get_max_seq(keypair.did()), Some(5));

        // All should be rejected as replays regardless of which method is used
        for (i, envelope) in envelopes.iter().enumerate() {
            let result = if i % 2 == 0 {
                guard.check_durable(envelope)
            } else {
                guard.check_replay_only_durable(envelope)
            };
            assert!(
                result.is_err(),
                "Sequence {} should be rejected as replay",
                i + 1
            );
        }
    }

    // -------------------------------------------------------------------------
    // #2514 — receiver restart must not invent a future acceptance floor
    //
    // See docs/architecture/replay-state-restart-invariants.md. The governing
    // constraint is floor == highest-actually-accepted, exactly: a floor below
    // it admits replays, a floor above it rejects sequences the sender has not
    // emitted. These tests own both sides.
    // -------------------------------------------------------------------------

    /// Build an envelope carrying an explicit **sender-clock** timestamp.
    ///
    /// The sender's wall clock and the receiver's are different clock domains;
    /// ICN tolerates skew between them (`verify_age`). These helpers let tests
    /// model that skew deterministically, with no sleeping and no machine-clock
    /// manipulation.
    fn envelope_at(
        keypair: &KeyPair,
        sequence: u64,
        timestamp_ms: u64,
        body: &[u8],
    ) -> SignedEnvelope {
        let mut envelope = SignedEnvelope::new(
            keypair.did(),
            keypair,
            sequence,
            PayloadType::Gossip,
            body.to_vec(),
        )
        .unwrap();
        envelope.timestamp = timestamp_ms;
        let sig_input = envelope.canonical_encoding();
        envelope.signature = keypair.sign(&sig_input).to_vec();
        envelope
    }

    fn now_ms() -> u64 {
        ReplayGuard::current_time_ms()
    }

    /// Check an envelope expecting rejection, returning the error for typing.
    fn during_err(guard: &mut ReplayGuard, env: &SignedEnvelope) -> anyhow::Error {
        guard
            .check_durable(env)
            .expect_err("expected the guard to reject this envelope")
    }

    /// Overwrite the durable max_seq for a peer, simulating a crash that lost
    /// unflushed writes: the receiver accepted up to `accepted`, but only
    /// `durable` reached disk.
    fn force_durable_max_seq(store: &Arc<icn_store::SledStore>, did: &Did, durable: u64) {
        let key = ReplayGuard::make_max_seq_key(&pk(did));
        let entry = MaxSeqEntry {
            max_seq: durable,
            updated_at_ms: ReplayGuard::current_time_ms(),
            // Current-semantic: these tests are about crash consistency, not
            // migration, so the entry must take the ordinary #2514 restore path.
            semantic_version: REPLAY_STATE_SEMANTIC_VERSION,
            sender_regime: SENDER_REGIME_DURABLE_V1,
        };
        store
            .put(&key, &serde_json::to_vec(&entry).unwrap())
            .unwrap();
        // A real promotion writes both, so a test fixture standing in for one must too.
        store
            .put(
                &ReplayGuard::make_sender_regime_key(&pk(did)),
                &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
            )
            .unwrap();
    }

    /// The canonical #2514 case: a stable sender that never restarts, and a
    /// receiver that does. The receiver must accept the sender's very next
    /// legitimate sequence — not one 1000 higher.
    #[test]
    fn test_stable_sender_receiver_restart_accepts_next_sequence() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        // Receiver accepts 1..=10 from a sender that stays up throughout.
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            for seq in 1..=10 {
                let env = SignedEnvelope::new(
                    sender.did(),
                    &sender,
                    seq,
                    PayloadType::Gossip,
                    b"m".to_vec(),
                )
                .unwrap();
                assert!(guard.check_durable(&env).is_ok());
            }
        }

        // Receiver restarts. Sender did not: its next sequence is 11.
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        let next = SignedEnvelope::new(
            sender.did(),
            &sender,
            11,
            PayloadType::Gossip,
            b"next".to_vec(),
        )
        .unwrap();

        let result = guard.check_durable(&next);
        assert!(
            result.is_ok(),
            "restarted receiver rejected the stable sender's next legitimate \
             sequence (11); floor must not exceed what was actually accepted. \
             error: {:?}",
            result.unwrap_err().to_string()
        );
    }

    /// The floor installed on restart must equal the durable high-water exactly.
    #[test]
    fn test_receiver_restart_does_not_invent_future_floor() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            let env = SignedEnvelope::new(
                sender.did(),
                &sender,
                42,
                PayloadType::Gossip,
                b"m".to_vec(),
            )
            .unwrap();
            assert!(guard.check_durable(&env).is_ok());
        }

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            guard.get_max_seq(sender.did()),
            Some(42),
            "restart must not advance max_seq past what was accepted"
        );
        assert_eq!(
            guard.sequences.get(&pk(sender.did())).unwrap().floor_seq,
            42,
            "floor must equal the durable high-water, with no invented gap"
        );
    }

    /// Restarting repeatedly with no intervening traffic must leave the
    /// acceptance boundary where it was.
    #[test]
    fn test_repeated_restart_without_traffic_does_not_compound() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            let env = SignedEnvelope::new(
                sender.did(),
                &sender,
                10,
                PayloadType::Gossip,
                b"m".to_vec(),
            )
            .unwrap();
            assert!(guard.check_durable(&env).is_ok());
        }

        // Five restarts, no traffic in between.
        for restart in 1..=5 {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert_eq!(
                guard.get_max_seq(sender.did()),
                Some(10),
                "floor compounded after {restart} restart(s) with no traffic"
            );
        }

        // And the sender's next real sequence is still accepted.
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();
        let env = SignedEnvelope::new(
            sender.did(),
            &sender,
            11,
            PayloadType::Gossip,
            b"m".to_vec(),
        )
        .unwrap();
        assert!(guard.check_durable(&env).is_ok());
    }

    /// Security half: a genuinely captured envelope — the original bytes, with
    /// its original signed timestamp — must still be rejected after a restart.
    #[test]
    fn test_captured_envelope_replayed_after_restart_is_rejected() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        let captured = SignedEnvelope::new(
            sender.did(),
            &sender,
            7,
            PayloadType::Gossip,
            b"captured".to_vec(),
        )
        .unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert!(guard.check_durable(&captured).is_ok());
        }

        // Make the restart strictly later than the captured envelope's timestamp.
        std::thread::sleep(std::time::Duration::from_millis(5));

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            guard.check_durable(&captured).is_err(),
            "a captured pre-restart envelope must not be accepted after restart"
        );
    }

    /// A `Store` that records the exact order of `put`/`flush` calls, so tests
    /// can prove the high-water was made **durable** before acceptance
    /// returned — not merely written into sled's in-memory tree, which a plain
    /// `get()` would happily read back without any fsync.
    #[derive(Default)]
    struct OpLogStore {
        ops: std::sync::Mutex<Vec<String>>,
        data: std::sync::Mutex<std::collections::BTreeMap<Vec<u8>, Vec<u8>>>,
        fail_flush: bool,
    }

    impl icn_store::Store for OpLogStore {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(self.data.lock().unwrap().get(key).cloned())
        }
        fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
            self.ops.lock().unwrap().push("put".to_string());
            self.data
                .lock()
                .unwrap()
                .insert(key.to_vec(), value.to_vec());
            Ok(())
        }
        fn delete(&self, key: &[u8]) -> Result<()> {
            self.data.lock().unwrap().remove(key);
            Ok(())
        }
        fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
        fn flush(&self) -> Result<()> {
            self.ops.lock().unwrap().push("flush".to_string());
            if self.fail_flush {
                anyhow::bail!("simulated storage failure");
            }
            Ok(())
        }
        fn get_replica_metadata(
            &self,
            _h: &icn_store::ContentHash,
        ) -> Result<Option<icn_store::ReplicaMetadata>> {
            Ok(None)
        }
        fn put_replica_metadata(&self, _m: &icn_store::ReplicaMetadata) -> Result<()> {
            Ok(())
        }
        fn list_replica_hashes(&self) -> Result<Vec<icn_store::ContentHash>> {
            Ok(vec![])
        }
    }

    /// The #468 / PR #501 race, closed at the source: the high-water is flushed
    /// **before** acceptance returns, so `D == A` at every instant and no
    /// interval of accepted-but-not-durable sequences exists for a crash to
    /// expose.
    #[test]
    fn test_high_water_is_flushed_before_acceptance_returns() {
        let store = Arc::new(OpLogStore::default());
        let sender = KeyPair::generate().unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        let env = SignedEnvelope::new(sender.did(), &sender, 1, PayloadType::Gossip, b"m".to_vec())
            .unwrap();
        // Establish the peer's regime first and clear the log: this test is about the
        // ordering of the *acceptance* write, and the one-off provenance write from
        // establishment would otherwise be indistinguishable from it.
        guard.pre_establish_durable(sender.did());
        store.ops.lock().unwrap().clear();

        assert!(guard
            .check_replay_only(&env, ObservedSenderRegime::DurableV1)
            .is_ok());

        // By the time acceptance returned, the write must already have been
        // forced durable.
        let ops = store.ops.lock().unwrap().clone();
        assert_eq!(
            ops,
            vec!["put".to_string(), "flush".to_string()],
            "expected put-then-flush before acceptance returned, got {ops:?}; \
             without the flush a crash can lose this acceptance and reopen (D, A]"
        );
    }

    /// Fail closed: if the high-water cannot be made durable, the message is not
    /// accepted, and the failure is typed so callers do not score a local
    /// storage fault as peer misbehaviour.
    #[test]
    fn test_acceptance_fails_closed_when_state_cannot_be_made_durable() {
        let store = Arc::new(OpLogStore {
            fail_flush: true,
            ..Default::default()
        });
        let sender = KeyPair::generate().unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        let env = SignedEnvelope::new(sender.did(), &sender, 1, PayloadType::Gossip, b"m".to_vec())
            .unwrap();
        let err = guard
            .check_durable(&env)
            .expect_err("must not accept what it cannot durably record");
        assert!(
            err.downcast_ref::<ReplayStateNotDurable>().is_some(),
            "storage faults must be typed distinctly from replay detections so \
             they are not scored against the peer; got: {err}"
        );

        // And the in-memory high-water must not have advanced past durable
        // state: the window exists but still sits at 0, not at the rejected 1.
        assert_eq!(
            guard.get_max_seq(sender.did()),
            Some(0),
            "in-memory high-water advanced past what was durably recorded"
        );
    }

    /// A crash at any point leaves a floor that rejects everything already
    /// accepted and admits the sender's next sequence — no gap, no timestamps.
    #[test]
    fn test_crash_window_replay_rejected_after_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        let mut captured = Vec::new();
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            for seq in 1..=10 {
                let env = SignedEnvelope::new(
                    sender.did(),
                    &sender,
                    seq,
                    PayloadType::Gossip,
                    b"m".to_vec(),
                )
                .unwrap();
                assert!(guard.check_durable(&env).is_ok());
                if seq >= 6 {
                    captured.push(env);
                }
            }
        }

        // Crash here: whatever was accepted is already durable.
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        for env in &captured {
            assert!(
                guard.check_durable(env).is_err(),
                "captured envelope (seq {}) was replayed successfully after restart",
                env.sequence
            );
        }

        let next = SignedEnvelope::new(
            sender.did(),
            &sender,
            11,
            PayloadType::Gossip,
            b"m".to_vec(),
        )
        .unwrap();
        assert!(
            guard.check_durable(&next).is_ok(),
            "replay protection must not block the sender's next sequence"
        );
    }

    /// A storage *rollback* — durable state moved backwards by something other
    /// than a crash — is outside the crash-consistency guarantee. Pinned so the
    /// boundary is explicit rather than discovered later: with `D` rolled back
    /// below `A`, sequences in `(D, A]` become acceptable again and only
    /// envelope freshness bounds the exposure.
    #[test]
    fn test_storage_rollback_is_outside_the_crash_guarantee() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        let mut captured = None;
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            for seq in 1..=10 {
                let env = SignedEnvelope::new(
                    sender.did(),
                    &sender,
                    seq,
                    PayloadType::Gossip,
                    b"m".to_vec(),
                )
                .unwrap();
                assert!(guard.check_durable(&env).is_ok());
                if seq == 6 {
                    captured = Some(env);
                }
            }
        }

        force_durable_max_seq(&store, sender.did(), 5);

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // Documented consequence, not an endorsement: see section 8 of
        // docs/architecture/replay-state-restart-invariants.md.
        assert!(
            guard.check_durable(captured.as_ref().unwrap()).is_ok(),
            "if this now fails, the rollback boundary changed and the \
             architecture doc must be updated to match"
        );
    }

    /// The barrier is a startup transient keyed to the restart, not a permanent
    /// rule: a message signed after the restart is accepted even if its sequence
    /// sits inside the lost crash window.
    #[test]
    fn test_post_restart_traffic_in_crash_window_range_is_accepted() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            let env =
                SignedEnvelope::new(sender.did(), &sender, 5, PayloadType::Gossip, b"m".to_vec())
                    .unwrap();
            assert!(guard.check_durable(&env).is_ok());
        }

        std::thread::sleep(std::time::Duration::from_millis(5));

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // Signed now, i.e. after the restart.
        let fresh =
            SignedEnvelope::new(sender.did(), &sender, 6, PayloadType::Gossip, b"m".to_vec())
                .unwrap();
        assert!(
            guard.check_durable(&fresh).is_ok(),
            "post-restart traffic must not be blocked by the restart barrier"
        );
    }

    /// Replay state is per-peer: one peer's restart barrier and floor must not
    /// contaminate another's.
    #[test]
    fn test_multi_peer_replay_state_independent_across_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let a = KeyPair::generate().unwrap();
        let b = KeyPair::generate().unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            let ea =
                SignedEnvelope::new(a.did(), &a, 100, PayloadType::Gossip, b"a".to_vec()).unwrap();
            let eb =
                SignedEnvelope::new(b.did(), &b, 3, PayloadType::Gossip, b"b".to_vec()).unwrap();
            assert!(guard.check_durable(&ea).is_ok());
            assert!(guard.check_durable(&eb).is_ok());
        }

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(guard.get_max_seq(a.did()), Some(100));
        assert_eq!(guard.get_max_seq(b.did()), Some(3));

        // Each peer's next sequence is accepted independently.
        let na = SignedEnvelope::new(a.did(), &a, 101, PayloadType::Gossip, b"a".to_vec()).unwrap();
        let nb = SignedEnvelope::new(b.did(), &b, 4, PayloadType::Gossip, b"b".to_vec()).unwrap();
        assert!(guard.check_durable(&na).is_ok());
        assert!(guard.check_durable(&nb).is_ok());
    }

    /// Corrupt durable replay state must fail safe: the peer simply starts
    /// without a window, and the restart barrier still applies.
    #[test]
    fn test_corrupt_replay_state_fails_safely() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        let captured = SignedEnvelope::new(
            sender.did(),
            &sender,
            9,
            PayloadType::Gossip,
            b"captured".to_vec(),
        )
        .unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert!(guard.check_durable(&captured).is_ok());
        }

        // Corrupt the persisted entry.
        let key = ReplayGuard::make_max_seq_key(&pk(sender.did()));
        store.put(&key, b"{not valid json").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        // Load must not panic or error out.
        guard.load_persisted_state().unwrap();

        assert!(
            guard.check_durable(&captured).is_err(),
            "corrupt replay state must not open a replay window for a captured \
             pre-restart envelope"
        );
    }

    // -------------------------------------------------------------------------
    // Clock-domain tests.
    //
    // `envelope.timestamp` comes from the SENDER's wall clock; any restart
    // instant the receiver records comes from the RECEIVER's. ICN's freshness
    // check tolerates skew between them, so "timestamp < receiver_restart" does
    // NOT mean "sent before the restart". These tests hold the design to ICN's
    // real clock contract rather than to synchronized clocks.
    // -------------------------------------------------------------------------

    /// Liveness under negative sender skew: the sender's clock runs behind the
    /// receiver's by an amount ICN permits. A message emitted in real time
    /// *after* the receiver restarted still carries a timestamp that sorts
    /// before the receiver's restart instant. It is fresh, it is legitimate, and
    /// it must be accepted.
    #[test]
    fn test_legitimate_traffic_accepted_despite_negative_sender_skew() {
        for skew_secs in [1u64, 100, 250] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let restart_ms = now_ms();

            // Durable state: accepted through sequence 5, well before restart.
            {
                let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
                guard.load_persisted_state().unwrap();
                for seq in 1..=5 {
                    let env = envelope_at(&sender, seq, restart_ms - 400_000, b"old");
                    // Bypass freshness: we are staging durable state, not
                    // exercising verify_age here.
                    assert!(guard.check_replay_only_durable(&env).is_ok());
                }
            }

            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();

            // Emitted now, but stamped by a clock that lags the receiver.
            let ts = restart_ms - skew_secs * 1000;
            let legit = envelope_at(&sender, 6, ts, b"legit");

            // Freshness accepts it: this is inside ICN's tolerated skew.
            assert!(
                legit.verify(300).is_ok(),
                "skew {skew_secs}s must be inside the freshness window"
            );

            let result = guard.check_durable(&legit);
            assert!(
                result.is_ok(),
                "legitimate post-restart message rejected because the sender's \
                 clock lags the receiver by {skew_secs}s; wall-clock ordering \
                 across machines is not a valid replay discriminator. err: {:?}",
                result.unwrap_err().to_string()
            );
        }
    }

    /// Security under positive sender skew: a sender whose clock runs ahead
    /// produces envelopes whose timestamps sort *after* the receiver's restart
    /// instant even though they were emitted and accepted before it. Freshness
    /// will not reject them. The restart floor must — and does, because the
    /// floor is `A`, established without reference to any clock.
    #[test]
    fn test_captured_replay_rejected_despite_positive_sender_skew() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let restart_ms = now_ms();
        let skew_ms = 100_000; // sender clock 100s ahead — inside tolerance

        let mut captured = Vec::new();
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            for seq in 1..=10 {
                let env = envelope_at(&sender, seq, restart_ms + skew_ms, b"m");
                assert!(guard.check_replay_only_durable(&env).is_ok());
                if seq >= 6 {
                    captured.push(env);
                }
            }
        }

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        for env in &captured {
            assert!(
                env.verify(300).is_ok(),
                "captured envelope is still fresh, so freshness cannot be what \
                 rejects it"
            );
            assert!(
                guard.check_durable(env).is_err(),
                "captured envelope (seq {}) accepted after restart despite a \
                 durable floor; a sender clock running ahead must not create a \
                 replay window",
                env.sequence
            );
        }
    }

    // -------------------------------------------------------------------------
    // Corrupt-state quarantine boundary.
    //
    // These use a small max_age so they run fast; the property under test is the
    // *relation* between the quarantine and the freshness bounds, not the
    // production constant. With max_age = 1s the horizon is 2s.
    // -------------------------------------------------------------------------

    /// The quarantine must outlast every envelope that could have been accepted
    /// before the restart — including one stamped at the positive-skew limit,
    /// which is at its freshest exactly when a naive one-interval quarantine
    /// would end.
    #[test]
    fn test_corrupt_state_quarantine_outlasts_maximum_future_skew() {
        let max_age = 1u64; // horizon = 2s
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        // Accepted immediately before the restart, stamped at the positive-skew
        // limit by a sender whose clock runs fast.
        let restart_ms = now_ms();
        let captured = envelope_at(&sender, 7, restart_ms + max_age * 1000, b"captured");
        {
            let mut guard = ReplayGuard::new_persistent(max_age, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert!(guard.check_replay_only_durable(&captured).is_ok());
        }

        // Durable state is unreadable, so there is no floor to fall back on.
        store
            .put(
                &ReplayGuard::make_max_seq_key(&pk(sender.did())),
                b"{corrupt",
            )
            .unwrap();

        let mut guard = ReplayGuard::new_persistent(max_age, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // At one max_age the envelope is at age ~0 — maximally fresh. A
        // single-interval quarantine would release here and accept the replay.
        std::thread::sleep(Duration::from_millis(max_age * 1000 + 100));
        assert!(
            captured.verify(max_age).is_ok(),
            "at one max_age past restart the captured envelope is still fresh, \
             which is precisely why the quarantine cannot end here"
        );
        assert!(
            guard.check_durable(&captured).is_err(),
            "quarantine released after a single max_age and accepted a replay"
        );

        // Past the full horizon it can no longer pass freshness at all.
        std::thread::sleep(Duration::from_millis(max_age * 1000 + 200));
        assert!(
            captured.verify(max_age).is_err(),
            "past the horizon the captured envelope must fail freshness"
        );
        assert!(
            guard.check_durable(&captured).is_err(),
            "captured envelope must remain rejected"
        );
    }

    /// The quarantine is bounded: once the horizon passes, a peer whose state
    /// was corrupt is served normally again.
    #[test]
    fn test_legitimate_traffic_accepted_after_quarantine_expires() {
        let max_age = 1u64;
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(max_age, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            let env =
                SignedEnvelope::new(sender.did(), &sender, 3, PayloadType::Gossip, b"m".to_vec())
                    .unwrap();
            assert!(guard.check_replay_only_durable(&env).is_ok());
        }
        store
            .put(
                &ReplayGuard::make_max_seq_key(&pk(sender.did())),
                b"{corrupt",
            )
            .unwrap();

        let mut guard = ReplayGuard::new_persistent(max_age, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // Fresh traffic during the quarantine is rejected — and typed as a
        // local fault, so the peer is not scored for our unreadable state.
        let during =
            SignedEnvelope::new(sender.did(), &sender, 4, PayloadType::Gossip, b"m".to_vec())
                .unwrap();
        let err = during_err(&mut guard, &during);
        assert!(
            err.downcast_ref::<ReplayStateUnreadable>().is_some(),
            "quarantine rejections must be typed distinctly from replay \
             detections, or the handler bans an innocent peer for OUR corrupt \
             state — the exact false-positive class #2514 was about; got: {err}"
        );

        // ...and accepted once the horizon has fully elapsed.
        std::thread::sleep(Duration::from_millis(2 * max_age * 1000 + 300));
        let after =
            SignedEnvelope::new(sender.did(), &sender, 5, PayloadType::Gossip, b"m".to_vec())
                .unwrap();
        assert!(
            guard.check_durable(&after).is_ok(),
            "quarantine must be bounded; a corrupt key must not permanently \
             disable a peer"
        );
    }

    /// The horizon is derived from both tolerances, not from one of them.
    #[test]
    fn test_validity_horizon_is_future_skew_plus_max_age() {
        let guard = ReplayGuard::new(300, 3600);
        assert_eq!(
            guard.envelope_validity_horizon(),
            Duration::from_secs(600),
            "horizon must be future-skew + max-age, not a single interval"
        );
    }

    /// Cleanup may only forget a peer after no envelope it once accepted could
    /// still be replayed: max_peer_age_secs must exceed the validity horizon.
    #[test]
    fn test_peer_age_exceeds_envelope_validity_horizon_in_production_config() {
        // Same constants the network actor constructs.
        let guard = ReplayGuard::new(300, 3600);
        let horizon = guard.envelope_validity_horizon().as_secs();
        assert!(
            3600 > horizon,
            "max_peer_age_secs ({}) must exceed the envelope validity horizon \
             ({horizon}s), or cleanup can forget a peer while traffic it \
             accepted is still replayable",
            3600
        );
    }

    /// A window that rejects every message never refreshes its own liveness, so
    /// it ages out on a fixed timer. This is the mechanism that ended the #2514
    /// incident at exactly max_peer_age_secs; pinning it so the semantics cannot
    /// change silently.
    #[test]
    fn test_rejecting_window_does_not_refresh_liveness() {
        let mut guard = ReplayGuard::new(300, 1); // 1 second max age
        let sender = KeyPair::generate().unwrap();

        let env = SignedEnvelope::new(
            sender.did(),
            &sender,
            10,
            PayloadType::Gossip,
            b"m".to_vec(),
        )
        .unwrap();
        assert!(guard.check_durable(&env).is_ok());
        assert_eq!(guard.peer_count(), 1);

        // Drive rejections for longer than max_peer_age, re-signing each time so
        // only the replay floor (not freshness) is doing the rejecting.
        for _ in 0..11 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let replay = SignedEnvelope::new(
                sender.did(),
                &sender,
                10,
                PayloadType::Gossip,
                b"m".to_vec(),
            )
            .unwrap();
            assert!(
                guard.check_durable(&replay).is_err(),
                "replay must be rejected"
            );
        }

        guard.cleanup();
        assert_eq!(
            guard.peer_count(),
            0,
            "rejections must not extend a window's lifetime"
        );
    }

    /// Conversely, accepted traffic must refresh the window.
    #[test]
    fn test_accepted_traffic_refreshes_window_liveness() {
        let mut guard = ReplayGuard::new(300, 1);
        let sender = KeyPair::generate().unwrap();

        for seq in 1..=11 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let env = SignedEnvelope::new(
                sender.did(),
                &sender,
                seq,
                PayloadType::Gossip,
                b"m".to_vec(),
            )
            .unwrap();
            assert!(guard.check_durable(&env).is_ok());
        }

        guard.cleanup();
        assert_eq!(
            guard.peer_count(),
            1,
            "accepted traffic must keep the window alive"
        );
    }

    /// After cleanup forgets a peer entirely, replay protection is handed to
    /// envelope freshness. Cleanup requires max_peer_age_secs of silence, which
    /// is far longer than the freshness bound, so anything cleanup can forget is
    /// already too old to replay. This test pins that ordering.
    #[test]
    fn test_replay_after_cleanup_rejected_by_freshness() {
        // max_clock_skew of 1s stands in for the production 300s bound; the
        // property under test is freshness < peer age, not the constants.
        let mut guard = ReplayGuard::new(1, 1);
        let sender = KeyPair::generate().unwrap();

        let captured =
            SignedEnvelope::new(sender.did(), &sender, 4, PayloadType::Gossip, b"m".to_vec())
                .unwrap();
        assert!(guard.check_durable(&captured).is_ok());

        // Outlive both the freshness bound and the peer age.
        std::thread::sleep(std::time::Duration::from_millis(1200));
        guard.cleanup();
        assert_eq!(guard.peer_count(), 0, "peer should have been forgotten");

        let result = guard.check_durable(&captured);
        assert!(
            result.is_err(),
            "replay must still be rejected after replay state is forgotten"
        );
        assert!(
            result.unwrap_err().to_string().contains("too old"),
            "the rejecting boundary after cleanup must be envelope freshness"
        );
    }
}

/// Migration of replay state written under obsolete sequence semantics (#2517).
///
/// These tests write **real legacy bytes** — JSON with no `semantic_version` key,
/// exactly as a pre-#2517 binary emitted — rather than constructing a struct with
/// the field set to zero. A migration test that builds its input with the field it
/// is meant to detect the absence of proves nothing.
#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::envelope::PayloadType;
    use icn_identity::KeyPair;

    /// Byte-for-byte what a pre-#2517 receiver persisted: `max_seq` + `updated_at_ms`
    /// and nothing else.
    fn write_legacy_max_seq(store: &Arc<icn_store::SledStore>, did: &Did, max_seq: u64) {
        let legacy = serde_json::json!({
            "max_seq": max_seq,
            "updated_at_ms": ReplayGuard::current_time_ms(),
        });
        store
            .put(
                &ReplayGuard::make_max_seq_key(&pk(did)),
                &serde_json::to_vec(&legacy).unwrap(),
            )
            .unwrap();
    }

    fn envelope(sender: &KeyPair, sequence: u64) -> SignedEnvelope {
        SignedEnvelope::new(
            sender.did(),
            sender,
            sequence,
            PayloadType::Gossip,
            b"m".to_vec(),
        )
        .unwrap()
    }

    /// Legacy bytes must be recognised as legacy. Guards the detector itself: if
    /// `semantic_version` ever gains a non-zero `#[serde(default)]`, or the field is
    /// renamed, every other test in this module would pass vacuously.
    #[test]
    fn legacy_bytes_deserialize_as_version_zero() {
        let raw = serde_json::json!({ "max_seq": 15915u64, "updated_at_ms": 1u64 });
        let parsed: MaxSeqEntry = serde_json::from_slice(&serde_json::to_vec(&raw).unwrap())
            .expect("legacy entries must still parse; the schema is unchanged");

        assert_eq!(parsed.max_seq, 15915);
        assert_eq!(
            parsed.semantic_version, LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            "an entry with no semantic_version key must read as the legacy regime"
        );
        assert_ne!(
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION, REPLAY_STATE_SEMANTIC_VERSION,
            "legacy and current must be distinguishable or migration cannot trigger"
        );
    }

    /// The #2517 reproducer. Receiver holds a legacy high-water of 15915 for a sender
    /// whose durable counter now sits far below it. The legitimate current sequence
    /// must not be scored as a replay against that legacy number.
    #[test]
    fn legacy_high_water_does_not_reject_a_legitimate_lower_sequence_forever() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        // Gamma's real state: 15915 recorded from the sender's ephemeral incarnation.
        write_legacy_max_seq(&store, sender.did(), 15_915);

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // Beta's real durable sequence at the time of the incident.
        let legitimate = envelope(&sender, 12_901);
        let err = guard
            .check_legacy(&legitimate)
            .expect_err("during migration the peer is fail-closed, not accepted");

        assert!(
            err.downcast_ref::<ReplayStateLegacy>().is_some(),
            "rejection during migration must be typed as migration, not as a replay \
             attack — otherwise it is scored as severity-1.0 misbehaviour. got: {err}"
        );

        // The legacy number must never have become the floor.
        let window = guard.sequences.get(&pk(sender.did())).unwrap();
        assert_eq!(
            window.floor_seq, 0,
            "a legacy high-water was installed as a current-semantic floor"
        );
        assert_eq!(
            window.max_seq, 0,
            "a legacy high-water was trusted as max_seq"
        );
    }

    /// The migration must end deterministically, at the envelope validity horizon —
    /// not at `max_peer_age_secs`, and not when the sender burns past a legacy value.
    #[test]
    fn migration_completes_at_the_envelope_validity_horizon() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        write_legacy_max_seq(&store, sender.did(), 15_915);

        // 1s skew stands in for the production 300s; the property is that the bound
        // is the freshness horizon (2 x skew), not the 3600s peer age.
        let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            guard.check_legacy(&envelope(&sender, 12_901)).is_err(),
            "peer must be fail-closed while migrating"
        );

        // Outlive the horizon (2 x 1s), well short of max_peer_age_secs.
        std::thread::sleep(Duration::from_millis(2_100));

        assert!(
            guard.check_legacy(&envelope(&sender, 12_902)).is_ok(),
            "migration must complete at the validity horizon without waiting for \
             cleanup() to age the window out"
        );
    }

    /// The security cost of discarding a legacy high-water, pinned. A message captured
    /// under the legacy regime must not become acceptable just because we stopped
    /// trusting the number that used to block it.
    #[test]
    fn captured_legacy_envelope_stays_rejected_across_the_whole_migration() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        write_legacy_max_seq(&store, sender.did(), 15_915);

        // Captured while the legacy regime was live, i.e. now.
        let captured = envelope(&sender, 12_901);

        let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            guard.check_legacy(&captured).is_err(),
            "captured envelope accepted during migration"
        );

        std::thread::sleep(Duration::from_millis(2_100));

        let err = guard
            .check_legacy(&captured)
            .expect_err("captured legacy envelope became acceptable once migration ended");
        assert!(
            err.to_string().contains("too old"),
            "after migration the captured envelope must be refused on freshness, \
             which is what makes discarding the legacy high-water safe. got: {err}"
        );
    }

    /// Migration is one-way and idempotent: restarting repeatedly must not re-enter it
    /// once current-semantic state exists, or a crash-looping node never converges.
    #[test]
    fn migration_runs_once_and_does_not_re_trigger_on_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        write_legacy_max_seq(&store, sender.did(), 15_915);

        {
            let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert!(guard.check_legacy(&envelope(&sender, 12_901)).is_err());
            std::thread::sleep(Duration::from_millis(2_100));
            assert!(
                guard.check_legacy(&envelope(&sender, 12_902)).is_ok(),
                "migration should have completed"
            );
        }

        // Restart. The peer's state is now current-semantic, so no quarantine.
        let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            guard.check_legacy(&envelope(&sender, 12_903)).is_ok(),
            "a migrated peer re-entered migration on restart; migration must be one-way"
        );
        assert_eq!(
            guard.sequences.get(&pk(sender.did())).unwrap().floor_seq,
            12_902,
            "post-migration restart must restore the exact #2514 floor"
        );
    }

    /// Crash during migration: the marker never reached disk. The next boot must
    /// re-detect legacy and fail closed again, never fall through to trusting it.
    #[test]
    fn crash_before_migration_completes_re_quarantines_rather_than_trusting_legacy() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        write_legacy_max_seq(&store, sender.did(), 15_915);

        // Boot, detect legacy, then die before any traffic is accepted.
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert!(guard.check_legacy(&envelope(&sender, 12_901)).is_err());
        }

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        let err = guard
            .check_legacy(&envelope(&sender, 12_902))
            .expect_err("legacy state was trusted after a crash mid-migration");
        assert!(
            err.downcast_ref::<ReplayStateLegacy>().is_some(),
            "must re-enter migration, not fall through to the legacy floor. got: {err}"
        );
        assert_eq!(
            guard.sequences.get(&pk(sender.did())).unwrap().floor_seq,
            0,
            "legacy high-water leaked into the floor on the second boot"
        );
    }

    /// Write replay state claiming a regime this binary has no migration for.
    fn write_future_max_seq(store: &Arc<icn_store::SledStore>, did: &Did, max_seq: u64, bump: u32) {
        let future = serde_json::json!({
            "max_seq": max_seq,
            "updated_at_ms": ReplayGuard::current_time_ms(),
            "semantic_version": REPLAY_STATE_SEMANTIC_VERSION + bump,
        });
        store
            .put(
                &ReplayGuard::make_max_seq_key(&pk(did)),
                &serde_json::to_vec(&future).unwrap(),
            )
            .unwrap();
    }

    /// **Unknown-future is not a countdown.**
    ///
    /// The legacy hold is bounded because the old meaning is known well enough to
    /// bound how long anything produced under it stays dangerous. Nothing of the kind
    /// is known here, so waiting proves nothing — this test therefore outlives the
    /// *entire* legacy migration horizon and demands the peer still be refused.
    ///
    /// A 1s skew is used so the horizon is 2s and the test is deterministic in ~5s
    /// rather than 20 minutes; the property under test is "no elapsed time releases
    /// this", not the size of the constant.
    #[test]
    fn unknown_future_version_stays_fail_closed_past_the_legacy_migration_horizon() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        write_future_max_seq(&store, sender.did(), 500, 7);

        let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // (3) refused, and as a LOCAL fault rather than peer misbehaviour.
        let err = guard
            .check_legacy(&envelope(&sender, 501))
            .expect_err("future-regime state was interpreted under current rules");
        assert!(
            err.downcast_ref::<ReplayStateUnsupportedVersion>()
                .is_some(),
            "must be typed as an unsupported semantic regime, not scored as a replay \
             attack. got: {err}"
        );
        assert!(
            err.downcast_ref::<ReplayStateLegacy>().is_none(),
            "unknown-future must not be reported as the bounded legacy migration"
        );

        // (6) the future max_seq never became a current-semantic floor.
        assert_eq!(guard.sequences.get(&pk(sender.did())).unwrap().floor_seq, 0);
        assert_eq!(guard.sequences.get(&pk(sender.did())).unwrap().max_seq, 0);

        // (4) outlive the complete legacy migration horizon, several times over.
        std::thread::sleep(Duration::from_millis(2_100));
        assert!(
            guard.check_legacy(&envelope(&sender, 502)).is_err(),
            "unknown-future state graduated into current semantics by waiting"
        );
        std::thread::sleep(Duration::from_millis(2_100));

        // (5) still refused, and still typed as unsupported.
        let err = guard
            .check_legacy(&envelope(&sender, 503))
            .expect_err("unknown-future state was accepted after enough time passed");
        assert!(
            err.downcast_ref::<ReplayStateUnsupportedVersion>()
                .is_some(),
            "the hold must not decay into any other outcome. got: {err}"
        );

        // (7) ordinary traffic must not have stamped it as current — that would be a
        // silent downgrade of state a newer binary owns.
        let raw = store
            .get(&ReplayGuard::make_max_seq_key(&pk(sender.did())))
            .unwrap()
            .expect("the entry must still exist");
        let on_disk: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(
            on_disk["semantic_version"].as_u64().unwrap() as u32,
            REPLAY_STATE_SEMANTIC_VERSION + 7,
            "the future entry was overwritten or restamped by refused traffic"
        );
        assert_eq!(
            on_disk["max_seq"].as_u64().unwrap(),
            500,
            "the future entry's max_seq was modified"
        );

        // (8, 9) restart: still fail-closed, with no accumulated progress toward
        // acceptance.
        let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
        guard.load_persisted_state().unwrap();
        let err = guard
            .check_legacy(&envelope(&sender, 504))
            .expect_err("restarting cleared an unsupported-version hold");
        assert!(
            err.downcast_ref::<ReplayStateUnsupportedVersion>()
                .is_some(),
            "must remain unsupported across restart. got: {err}"
        );
        assert_eq!(guard.sequences.get(&pk(sender.did())).unwrap().floor_seq, 0);
    }

    /// A version *below* current that this binary has no explicit migration for must
    /// not fall through to the v0 migration. Only regimes with a written migration
    /// get one; everything else fails closed.
    ///
    /// Guards the shape of the state machine rather than a value: today
    /// `LEGACY + 1 == CURRENT`, so this constructs the case that will exist as soon
    /// as a v2 is introduced and a v1-era binary meets it.
    #[test]
    fn a_known_version_without_a_migration_does_not_borrow_the_legacy_path() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        // Any regime that is neither LEGACY nor CURRENT.
        write_future_max_seq(&store, sender.did(), 500, 1);

        let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        std::thread::sleep(Duration::from_millis(2_100));

        let err = guard
            .check_legacy(&envelope(&sender, 501))
            .expect_err("a regime with no migration was migrated anyway");
        assert!(
            err.downcast_ref::<ReplayStateUnsupportedVersion>()
                .is_some(),
            "an unmigrated regime must not silently reuse the v0 migration. got: {err}"
        );
    }

    /// The *other* legacy instance named in #2517, and the one #2514 cannot fix on its
    /// own: a value inflated by the pre-#2514 `+1000` restart gap. It is byte-identical
    /// to a genuine high-water, so nothing about the number itself gives it away —
    /// only the missing version does. Restoring it exactly, as #2514 correctly does,
    /// preserves the inflation.
    #[test]
    fn pre_2514_inflated_floor_does_not_survive_migration() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        // Receiver genuinely accepted up to 42; the old code then persisted 1042 on
        // restart. Both are plain integers; only the absent version distinguishes them.
        write_legacy_max_seq(&store, sender.did(), 42 + 1_000);

        let mut guard = ReplayGuard::new_persistent(1, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            guard.sequences.get(&pk(sender.did())).unwrap().floor_seq,
            0,
            "the inflated legacy floor was carried forward"
        );

        std::thread::sleep(Duration::from_millis(2_100));

        // The sender's real next sequence, far below the inflated 1042.
        assert!(
            guard.check_legacy(&envelope(&sender, 43)).is_ok(),
            "a legitimate sequence below the pre-#2514 inflated floor was still rejected"
        );
    }

    /// Downgrade: a current-semantic entry is overwritten by an old binary, which
    /// writes no version. The new binary must treat the result as legacy rather than
    /// assume its own earlier stamp still describes the bytes on disk.
    #[test]
    fn state_rewritten_by_an_older_binary_is_treated_as_legacy() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        // Current binary establishes current-semantic state.
        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert!(guard.check_legacy(&envelope(&sender, 100)).is_ok());
        }

        // Rollback: an old binary runs and rewrites the entry with no version key.
        write_legacy_max_seq(&store, sender.did(), 9_999);

        // Roll forward again.
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        let err = guard
            .check_legacy(&envelope(&sender, 101))
            .expect_err("downgraded state was trusted as current-semantic");
        assert!(
            err.downcast_ref::<ReplayStateLegacy>().is_some(),
            "an entry rewritten without a version must re-enter migration. got: {err}"
        );
        assert_eq!(
            guard.sequences.get(&pk(sender.did())).unwrap().floor_seq,
            0,
            "the downgraded value became a floor"
        );
    }

    /// Current-semantic state must be untouched by any of this: #2514's exact-restore
    /// invariant stays green.
    #[test]
    fn current_semantic_state_is_restored_exactly_and_not_migrated() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        {
            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();
            assert!(guard.check_legacy(&envelope(&sender, 42)).is_ok());
        }

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            guard.sequences.get(&pk(sender.did())).unwrap().floor_seq,
            42,
            "#2514: the floor must equal the durable high-water exactly"
        );
        assert!(
            guard.check_legacy(&envelope(&sender, 43)).is_ok(),
            "a current-semantic peer must not be quarantined"
        );
    }
}

/// #2517: a persisted high-water is meaningful only inside the *sender sequence
/// regime* that produced it, not merely the receiver regime that recorded it.
///
/// Every test here exists because receiver-side semantic versioning alone was
/// insufficient: it made `max_seq` interpretable with respect to our own code, and
/// said nothing about whose namespace the number came from.
#[cfg(test)]
mod sender_regime_tests {
    use super::*;
    use crate::envelope::PayloadType;
    use icn_identity::KeyPair;

    fn envelope(sender: &KeyPair, sequence: u64) -> SignedEnvelope {
        SignedEnvelope::new(
            sender.did(),
            sender,
            sequence,
            PayloadType::Gossip,
            b"m".to_vec(),
        )
        .unwrap()
    }

    /// Deterministic monotonic clock.
    ///
    /// Migration holds are the one thing here that must be exercised at the
    /// *production* horizon — 600s, derived from `max_clock_skew = 300` — and a test
    /// that proves that by waiting ten minutes is a test nobody runs. Advancing a
    /// counter is also the only way to assert "not yet, and then yes" without a
    /// sleep, which would make every negative assertion a race.
    struct TestClock {
        nanos: std::sync::atomic::AtomicU64,
    }

    impl TestClock {
        fn new() -> Arc<Self> {
            Arc::new(TestClock {
                nanos: std::sync::atomic::AtomicU64::new(0),
            })
        }

        fn advance(&self, by: Duration) {
            self.nanos.fetch_add(by.as_nanos() as u64, Ordering::SeqCst);
        }
    }

    impl MonotonicClock for TestClock {
        fn elapsed(&self) -> Duration {
            Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
        }
    }

    /// Production settings: 300s skew tolerance, so a 600s retirement horizon.
    const SKEW: u64 = 300;
    const HORIZON: Duration = Duration::from_secs(2 * SKEW);

    /// Boot a receiver against an existing store, as a restart would.
    fn boot(store: &Arc<icn_store::SledStore>, clock: Arc<TestClock>) -> ReplayGuard {
        let mut guard =
            ReplayGuard::new_persistent(SKEW, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();
        guard
    }

    /// An envelope stamped `age` in the past, so `verify_age` sees it as genuinely
    /// old rather than merely notionally so.
    ///
    /// Needed because the injected clock is virtual while `verify_age` reads the real
    /// wall clock. Backdating keeps the two consistent: when the test advances the
    /// hold by 600s, a "captured" envelope really is 600s old, and its rejection is
    /// the horizon doing its job rather than an artifact of the harness.
    fn captured_envelope(sender: &KeyPair, sequence: u64, age: Duration) -> SignedEnvelope {
        let mut envelope = SignedEnvelope::new(
            sender.did(),
            sender,
            sequence,
            PayloadType::Gossip,
            b"captured".to_vec(),
        )
        .unwrap();
        envelope.timestamp = ReplayGuard::current_time_ms() - age.as_millis() as u64;
        let sig_input = envelope.canonical_encoding();
        envelope.signature = sender.sign(&sig_input).to_vec();
        envelope
    }

    /// Drive a peer through first establishment of the durable regime.
    ///
    /// There is no shortcut, by design: every peer costs one retirement hold before
    /// its durable namespace can be interpreted, because nothing available to the
    /// receiver proves the peer's *previous* namespace is already retired.
    fn establish_durable(guard: &mut ReplayGuard, clock: &TestClock, sender: &KeyPair, seq: u64) {
        let held = guard.check_replay_only(&envelope(sender, seq), ObservedSenderRegime::DurableV1);
        if held.is_ok() {
            panic!("first establishment must cost a retirement hold, not be immediate");
        }
        clock.advance(HORIZON + Duration::from_secs(1));
        guard
            .check_replay_only(&envelope(sender, seq), ObservedSenderRegime::DurableV1)
            .expect("promotion after the horizon with current durable evidence");
    }

    fn regime_on_disk(store: &Arc<icn_store::SledStore>, did: &Did) -> u64 {
        on_disk(store, did)["sender_regime"].as_u64().unwrap()
    }

    /// Byte-for-byte what a pre-#2517 receiver persisted.
    fn write_legacy_max_seq(store: &Arc<icn_store::SledStore>, did: &Did, max_seq: u64) {
        let legacy = serde_json::json!({
            "max_seq": max_seq,
            "updated_at_ms": ReplayGuard::current_time_ms(),
        });
        store
            .put(
                &ReplayGuard::make_max_seq_key(&pk(did)),
                &serde_json::to_vec(&legacy).unwrap(),
            )
            .unwrap();
    }

    fn on_disk(store: &Arc<icn_store::SledStore>, did: &Did) -> serde_json::Value {
        let raw = store
            .get(&ReplayGuard::make_max_seq_key(&pk(did)))
            .unwrap()
            .expect("a persisted replay entry must exist");
        serde_json::from_slice(&raw).unwrap()
    }

    /// The bug receiver-only versioning missed (#2517, Phase 6).
    ///
    /// A *current* receiver talking to a peer that has not upgraded accepts perfectly
    /// ordinary legacy traffic — compatibility requires it. But the number it records
    /// belongs to the sender's ephemeral namespace, and if it is stamped as
    /// current-durable state then nothing later can tell that it isn't. When the
    /// sender eventually upgrades and its durable counter starts low, the receiver
    /// exact-restores a value it believes is comparable and rejects the sender
    /// forever — #2517, now invisible to the migration that exists to catch it.
    #[test]
    fn legacy_sender_traffic_is_never_recorded_as_durable_v1() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // The peer is authenticated, but its Hello does not advertise
        // DURABLE_SIGNING_SEQUENCE. Its sequence numbers are therefore from an
        // unproven namespace, however large and however monotonic they look.
        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect("legacy-regime traffic must still be accepted for compatibility");

        let entry = on_disk(&store, sender.did());
        assert_eq!(
            entry["max_seq"].as_u64().unwrap(),
            500,
            "the high-water itself is recorded normally"
        );
        assert_eq!(
            entry["sender_regime"].as_u64().unwrap(),
            u64::from(SENDER_REGIME_LEGACY_OR_UNPROVEN),
            "the receiver being current does not make the SENDER's number durable-v1; \
             stamping it so is the laundering that recreates #2517 under a label the \
             migration cannot see"
        );
    }

    /// The other half of the same property: when the sender *does* prove the durable
    /// regime, the number is durable-v1 state and #2514 exact-restore applies to it.
    #[test]
    fn durable_sender_traffic_is_recorded_as_durable_v1() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        establish_durable(&mut guard, &clock, &sender, 7);

        let entry = on_disk(&store, sender.did());
        assert_eq!(
            entry["sender_regime"].as_u64().unwrap(),
            u64::from(SENDER_REGIME_DURABLE_V1),
            "traffic proven to come from the durable namespace is recorded as such"
        );
    }

    /// **The canonical #2517 lifecycle regression.**
    ///
    /// The receiver-first upgrade order, end to end. This is the ordering that
    /// receiver-only semantic versioning could not survive, and the one that decides
    /// whether "any supported node upgrade order works" is a true claim.
    ///
    /// The failure it pins down: A upgrades, correctly migrates its legacy replay
    /// state, then keeps talking to a B that has *not* upgraded. A accepts B's
    /// ephemeral traffic — it must, for compatibility — and records a high-water. If
    /// that record is stamped current-durable merely because A is current, then when
    /// B finally upgrades and its durable counter starts at 1, A rejects it against a
    /// bound belonging to a process that no longer exists. And no migration can fire,
    /// because nothing looks legacy any more.
    ///
    /// Every step below is load-bearing; weakening any of them re-opens the bug. See
    /// `docs/architecture/protocol-state-migration-invariants.md`.
    #[test]
    fn receiver_first_upgrade_migrates_the_sender_regime_end_to_end() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();

        // (1)-(3) A is current; B is legacy; A holds pre-#2517 replay state for B,
        // recorded from B's long-lived ephemeral incarnation.
        write_legacy_max_seq(&store, did, 15_915);

        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        // (4) A recognises its own state as legacy and holds, rather than trusting a
        // number whose meaning it cannot establish.
        let held = guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("legacy receiver state must be held, not trusted");
        assert!(
            held.downcast_ref::<ReplayStateLegacy>().is_some(),
            "the hold must be typed as a local migration, not a replay verdict: {held}"
        );

        // The receiver-state migration completes at the horizon.
        clock.advance(HORIZON + Duration::from_secs(1));

        // (5)-(6) B is still ephemeral. Its process-local counter happens to sit
        // around 500 after long uptime. A accepts this perfectly ordinary traffic.
        for seq in 500..=510 {
            guard
                .check_replay_only(
                    &envelope(&sender, seq),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .unwrap_or_else(|e| panic!("legitimate legacy-sender traffic rejected: {e}"));
        }

        // (7) THE PROPERTY. What A persisted is a number from B's *legacy* namespace,
        // and it says so. A being current does not make B's number durable.
        assert_eq!(on_disk(&store, did)["max_seq"].as_u64().unwrap(), 510);
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_LEGACY_OR_UNPROVEN),
            "a current receiver must not launder an unproven sender's high-water into \
             durable-v1 replay state"
        );

        // (8) B restarts onto the durable implementation and now proves it on an
        // authenticated connection. Its fresh durable counter begins at 1 — far below
        // the 510 A recorded from B's previous incarnation.
        // (9)-(10) A detects the namespace change and enters an explicit transition.
        let transition = guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("a namespace change must be an explicit transition, not an accept");
        let typed = transition
            .downcast_ref::<SenderRegimeTransition>()
            .unwrap_or_else(|| panic!("expected a typed transition, got: {transition}"));
        assert_eq!(typed.remaining_secs, HORIZON.as_secs());

        // (12) Critically NOT a replay verdict. The numbers were never compared —
        // they are not comparable — so nothing here may look like an attack.
        assert!(
            transition.downcast_ref::<SenderRegimeDowngrade>().is_none()
                && !transition.to_string().contains("Replay detected"),
            "migration must never be reported as replay: {transition}"
        );

        // The transition is durable before it takes effect.
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_TRANSITION_TO_DURABLE_V1),
            "the transition must be persisted before it is relied on, or a crash here \
             resumes as trusted legacy state"
        );
        assert_eq!(
            on_disk(&store, did)["max_seq"].as_u64().unwrap(),
            510,
            "the legacy high-water is retained as legacy evidence during the transition"
        );

        // (11) A captured envelope from the old namespace stays rejected throughout.
        let captured = captured_envelope(&sender, 505, Duration::from_secs(1));
        assert!(
            guard
                .check_replay_only(&captured, ObservedSenderRegime::DurableV1)
                .is_err(),
            "captured legacy-namespace traffic must not be accepted mid-transition"
        );

        // (13) Advance through the complete validity horizon.
        clock.advance(HORIZON + Duration::from_secs(1));

        // (14)-(16) Promotion requires current authenticated durable-v1 evidence, and
        // then B's low durable sequence is accepted in a clean namespace.
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect("after the horizon, the sender's durable sequence 1 must be accepted");

        // (17) And is now recorded as durable-v1 state.
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_DURABLE_V1),
            "after promotion the high-water belongs to the durable-v1 namespace"
        );
        assert_eq!(on_disk(&store, did)["max_seq"].as_u64().unwrap(), 1);

        // A captured legacy envelope is now retired by age: it is older than the
        // horizon, so it cannot pass freshness. This is what licensed dropping the
        // legacy bound in the first place.
        let stale = captured_envelope(&sender, 509, HORIZON + Duration::from_secs(5));
        assert!(
            guard
                .check(&stale, ObservedSenderRegime::DurableV1)
                .is_err(),
            "an envelope from the retired namespace must fail freshness; if it can still \
             be fresh, the horizon is wrong and dropping the legacy bound was unsafe"
        );

        // (18)-(20) Restart A. Durable-v1 state restores exactly under #2514 — no
        // gap, no re-migration — and the sender's next sequence is accepted at once.
        let mut restarted = boot(&store, TestClock::new());
        restarted
            .check_replay_only(&envelope(&sender, 2), ObservedSenderRegime::DurableV1)
            .expect("#2514: the next durable sequence must be accepted immediately on restart");
        assert!(
            restarted
                .check_replay_only(&envelope(&sender, 2), ObservedSenderRegime::DurableV1)
                .is_err(),
            "replay protection must still work after the migration"
        );

        // (21) B cannot reset A's replay state by presenting as a pre-capability
        // binary.
        let downgrade = restarted
            .check_replay_only(
                &envelope(&sender, 3),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("a downgrade must not be silently accepted");
        let typed = downgrade
            .downcast_ref::<SenderRegimeDowngrade>()
            .unwrap_or_else(|| panic!("expected a typed downgrade, got: {downgrade}"));
        assert_eq!(typed.retained_max_seq, 2);
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_DURABLE_V1),
            "a downgrade attempt must not erase durable-v1 replay state — that would make \
             replay-state reset reachable by downgrade"
        );
    }

    /// Phase 16 — the other rolling-upgrade direction: the sender upgrades first.
    ///
    /// B is already durable when A finally upgrades and finds its own legacy replay
    /// state. A pays **two** sequential holds, and that is the correct answer rather
    /// than a missed optimisation.
    ///
    /// The tempting shortcut is to say the receiver-state migration already retired
    /// everything still-valid, so B could be established directly afterwards. That
    /// only holds if B upgraded *before* the first hold began. If B upgraded during
    /// it, B's last legacy envelope was created at some X after the hold started and
    /// stays valid until X + skew + max_age — past the hold's end. A cannot date B's
    /// upgrade, so it must assume the worse case.
    ///
    /// What B does *not* need is another restart: its durable counter is untouched
    /// throughout, and it is accepted at whatever sequence it has reached.
    #[test]
    fn sender_first_upgrade_costs_two_sequential_holds_and_no_sender_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();

        write_legacy_max_seq(&store, did, 15_915);

        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        // Hold 1: A's own recorded number is uninterpretable.
        let held = guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("legacy receiver state must be held");
        assert!(
            held.downcast_ref::<ReplayStateLegacy>().is_some(),
            "expected the receiver-state migration, got: {held}"
        );
        clock.advance(HORIZON + Duration::from_secs(1));

        // Hold 2: B's previous namespace must be retired on the sender axis too.
        let held2 = guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("the sender axis has its own retirement to do");
        assert!(
            held2.downcast_ref::<SenderRegimeTransition>().is_some(),
            "expected the sender-regime transition, got: {held2}"
        );
        clock.advance(HORIZON + Duration::from_secs(1));

        // B is accepted at its current durable sequence — no sender restart involved.
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect("the durable sender is accepted without ever restarting again");

        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_DURABLE_V1)
        );
    }

    /// Phase 17, revised by the design gate — first contact with any peer costs
    /// exactly one retirement hold, including on a clean install.
    ///
    /// The original rule here was "no local history means no old namespace to retire,
    /// so establish immediately". That was unsound: `NoHistory` is a fact about *this
    /// receiver's memory*, not about the sender's history. A receiver that just joined
    /// cannot know whether the peer switched namespaces ten seconds ago, and during
    /// the freshness window a captured pre-upgrade envelope and a legitimate
    /// post-upgrade one are indistinguishable — same signer, same freshness, no regime
    /// marker in the envelope.
    ///
    /// So the cost is real and is documented rather than optimised away: one 600s hold
    /// per (receiver, sender) pair, once ever. Eliminating it would require the
    /// envelope itself to name its namespace, which is a wire change.
    #[test]
    fn first_contact_with_a_durable_sender_costs_exactly_one_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        // Not immediate, however clean the install looks.
        let held = guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("first contact must retire the peer's possible previous namespace");
        assert_eq!(
            held.downcast_ref::<SenderRegimeTransition>()
                .expect("typed as a local migration, not a replay verdict")
                .remaining_secs,
            HORIZON.as_secs()
        );

        clock.advance(HORIZON + Duration::from_secs(1));
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect("after one horizon the durable namespace is established");

        // Exactly one: subsequent traffic is not held again.
        for seq in 2..=10 {
            guard
                .check_replay_only(&envelope(&sender, seq), ObservedSenderRegime::DurableV1)
                .expect("steady state after establishment must be unimpeded");
        }
        assert_eq!(
            regime_on_disk(&store, sender.did()),
            u64::from(SENDER_REGIME_DURABLE_V1)
        );
    }

    /// The provenance record is what keeps that "once ever" promise (#2517 design
    /// gate).
    ///
    /// `cleanup()` deletes an inactive peer's numeric high-water. If the established
    /// regime lived only in that entry, every quiet hour would cost the peer another
    /// 600s hold on its next message — and, far worse, absence of a record could be
    /// misread as evidence that no legacy namespace ever existed.
    #[test]
    fn established_regime_survives_replay_state_cleanup() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        establish_durable(&mut guard, &clock, &sender, 1);

        // The peer goes quiet past the inactivity horizon; routine cleanup evicts it.
        guard.sequences.get_mut(&pk(did)).unwrap().last_update =
            Instant::now() - Duration::from_secs(7_200);
        guard.cleanup();
        assert!(
            store
                .get(&ReplayGuard::make_max_seq_key(&pk(did)))
                .unwrap()
                .is_none(),
            "cleanup must still remove the numeric high-water"
        );
        assert!(
            store
                .get(&ReplayGuard::make_sender_regime_key(&pk(did)))
                .unwrap()
                .is_some(),
            "but it must NOT remove the proof that this peer's legacy namespace was \
             already retired; that fact does not expire because a peer went quiet"
        );

        // On restart the peer resumes with no numeric bound but an established
        // namespace, so it pays no second hold.
        let mut restarted = boot(&store, TestClock::new());
        restarted
            .check_replay_only(&envelope(&sender, 2), ObservedSenderRegime::DurableV1)
            .expect("an already-established peer must not be re-migrated after cleanup");
    }

    /// Matrix row 5 — a legacy sender that stays legacy keeps working, indefinitely,
    /// with its state tagged legacy the whole time.
    ///
    /// Compatibility is not grudging here: a federation mid-upgrade must keep
    /// running. What must *not* happen is the tag drifting to durable-v1 through
    /// sheer repetition.
    #[test]
    fn a_sender_that_stays_legacy_keeps_working_and_stays_tagged_legacy() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        for seq in 1..=50 {
            guard
                .check_replay_only(
                    &envelope(&sender, seq),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .expect("legacy senders must keep working");
        }
        clock.advance(HORIZON * 10);
        guard
            .check_replay_only(
                &envelope(&sender, 51),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect("elapsed time must not change anything for a steady legacy sender");

        assert_eq!(
            regime_on_disk(&store, sender.did()),
            u64::from(SENDER_REGIME_LEGACY_OR_UNPROVEN),
            "repetition and elapsed time must never promote an unproven sender"
        );

        // And it survives a restart as legacy, not as durable-v1.
        let restarted = boot(&store, TestClock::new());
        assert_eq!(
            restarted.sequences[&pk(sender.did())].sender_regime,
            SenderRegimeState::LegacyOrUnproven
        );
    }

    /// Phase 13 — an unknown future sender regime fails closed, with no deadline.
    ///
    /// The same principle already proven on the receiver axis, applied to the sender
    /// axis: a *known* obsolete namespace can have an explicit migration because its
    /// meaning is known. An unknown one cannot be reinterpreted by waiting. Time does
    /// not make unknown semantics knowable.
    #[test]
    fn unknown_future_sender_regime_fails_closed_and_never_expires() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();

        // A regime tag from a binary that knows something this one does not.
        let future = serde_json::json!({
            "max_seq": 42u64,
            "updated_at_ms": ReplayGuard::current_time_ms(),
            "semantic_version": REPLAY_STATE_SEMANTIC_VERSION,
            "sender_regime": SENDER_REGIME_TRANSITION_TO_DURABLE_V1 + 1,
        });
        store
            .put(
                &ReplayGuard::make_max_seq_key(&pk(did)),
                &serde_json::to_vec(&future).unwrap(),
            )
            .unwrap();

        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        for elapsed in [Duration::ZERO, HORIZON * 2, HORIZON * 100] {
            clock.advance(elapsed);
            let err = guard
                .check_replay_only(&envelope(&sender, 43), ObservedSenderRegime::DurableV1)
                .expect_err("an uninterpretable sender regime must fail closed");
            assert!(
                err.downcast_ref::<UnsupportedSenderRegime>().is_some(),
                "must be the typed unsupported-regime fault, not a replay verdict: {err}"
            );
        }
    }

    /// Phase 11 — the timer expiring is necessary but not sufficient.
    ///
    /// Promotion asserts something about the peer talking to us *now*. If the peer
    /// disconnected and came back without the capability — a rollback, or an attacker
    /// who cannot actually satisfy the durable invariant — there is nothing to
    /// promote to, however long the hold has run.
    #[test]
    fn transition_does_not_promote_when_the_peer_returns_without_the_capability() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        // Establish a legacy namespace, then observe the change.
        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .unwrap();
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("namespace change enters a transition");

        clock.advance(HORIZON * 3);

        // The peer is back, without the capability. No promotion.
        let err = guard
            .check_replay_only(
                &envelope(&sender, 2),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("elapsed time alone must not promote");
        assert!(
            err.downcast_ref::<SenderRegimeTransition>().is_some(),
            "still in transition, not promoted: {err}"
        );
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_TRANSITION_TO_DURABLE_V1),
            "the durable state must still say transition — promotion did not happen"
        );

        // Fresh authenticated durable-v1 evidence is what unblocks it.
        guard
            .check_replay_only(&envelope(&sender, 2), ObservedSenderRegime::DurableV1)
            .expect("current durable-v1 evidence after the horizon promotes");
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_DURABLE_V1)
        );
    }

    /// Phase 10 / Phase 23 — a receiver restart mid-transition restarts the *full*
    /// hold.
    ///
    /// Nothing durable records how much of the horizon had elapsed, and deliberately
    /// so: a persisted deadline would have to be a wall-clock time, and a clock jump
    /// or rollback could then shorten a security hold. Restarting the full monotonic
    /// hold can only ever lengthen the migration, which is the safe direction to be
    /// wrong in.
    #[test]
    fn receiver_restart_during_transition_restarts_the_full_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .unwrap();
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("transition begins");

        // Most of the way through, then crash.
        clock.advance(HORIZON - Duration::from_secs(5));
        drop(guard);

        // The restarted receiver resumes as a transition (not as trusted legacy
        // state) and starts the horizon over.
        let restart_clock = TestClock::new();
        let mut restarted = boot(&store, restart_clock.clone());

        let err = restarted
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("the transition must resume, not complete early");
        let typed = err.downcast_ref::<SenderRegimeTransition>().unwrap();
        assert_eq!(
            typed.remaining_secs,
            HORIZON.as_secs(),
            "the hold must restart at the FULL horizon; carrying over pre-restart \
             progress is what a persisted wall-clock deadline would do, and it is \
             shortenable by a clock jump"
        );

        // The remainder of the *old* hold is not enough.
        restart_clock.advance(Duration::from_secs(10));
        assert!(
            restarted
                .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
                .is_err(),
            "a restart must never shorten the hold"
        );

        // A full fresh horizon is.
        restart_clock.advance(HORIZON);
        restarted
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect("a full fresh horizon completes the migration");
    }

    /// Phase 23 — repeated restarts extend the hold, never shorten it, and never
    /// corrupt the namespace.
    #[test]
    fn repeated_restarts_during_transition_never_shorten_the_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .unwrap();
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("transition begins");
        drop(guard);

        for round in 0..5 {
            let c = TestClock::new();
            let mut g = boot(&store, c.clone());
            c.advance(HORIZON - Duration::from_secs(1));
            assert!(
                g.check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
                    .is_err(),
                "round {round}: just short of a full horizon must still hold"
            );
            assert_eq!(
                regime_on_disk(&store, did),
                u64::from(SENDER_REGIME_TRANSITION_TO_DURABLE_V1),
                "round {round}: repeated restarts must not corrupt the durable tag"
            );
            assert_eq!(
                on_disk(&store, did)["max_seq"].as_u64().unwrap(),
                500,
                "round {round}: the legacy evidence must survive intact"
            );
        }
    }

    /// Phase 23 — entering the transition twice is idempotent.
    ///
    /// No counter reset, no compounding floor, no state laundering. A peer that
    /// reconnects repeatedly mid-migration must not be able to restart the hold
    /// endlessly (Phase 19) nor corrupt what is recorded.
    #[test]
    fn re_entering_the_transition_is_idempotent_and_does_not_reset_the_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .unwrap();

        let first = guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("transition begins");
        let first_remaining = first
            .downcast_ref::<SenderRegimeTransition>()
            .unwrap()
            .remaining_secs;

        // Half the horizon passes with the peer reconnecting and re-announcing
        // durable-v1 on every message — connection churn, not a new migration.
        clock.advance(HORIZON / 2);
        for _ in 0..20 {
            let err = guard
                .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
                .expect_err("still holding");
            let remaining = err
                .downcast_ref::<SenderRegimeTransition>()
                .unwrap()
                .remaining_secs;
            assert!(
                remaining < first_remaining,
                "the hold must keep counting down across reconnects; restarting it on \
                 every reconnect would let a peer stall its own migration forever"
            );
        }

        assert_eq!(
            on_disk(&store, did)["max_seq"].as_u64().unwrap(),
            500,
            "re-entry must not move the legacy evidence"
        );

        // And it still completes on schedule from the original start.
        clock.advance(HORIZON / 2 + Duration::from_secs(1));
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect("the migration completes one horizon after it started");
    }

    /// Phase 23 — a crash *before* the transition marker is persisted resumes safely
    /// from legacy state rather than from a half-made decision.
    #[test]
    fn crash_before_the_transition_marker_resumes_from_legacy_state() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .unwrap();
        // Crash here: legacy state is durable, no transition was ever started.
        drop(guard);
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_LEGACY_OR_UNPROVEN)
        );

        let mut restarted = boot(&store, TestClock::new());
        assert_eq!(
            restarted.sequences[&pk(did)].sender_regime,
            SenderRegimeState::LegacyOrUnproven,
            "the restart must resume as legacy, and the legacy bound must still apply"
        );
        assert!(
            restarted
                .check_replay_only(
                    &envelope(&sender, 400),
                    ObservedSenderRegime::LegacyOrUnproven
                )
                .is_err(),
            "the legacy high-water must still reject replays within its own namespace"
        );
    }

    /// Phase 19 — a stale connection cannot downgrade a newer established regime.
    ///
    /// Capabilities are per-connection and last-write-wins, so a lingering
    /// pre-capability connection can deliver a `LegacyOrUnproven` attribution after
    /// durable-v1 was established. The durable regime must dominate: fail closed,
    /// keep the state.
    #[test]
    fn a_stale_legacy_connection_cannot_downgrade_established_durable_state() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        establish_durable(&mut guard, &clock, &sender, 1);
        for seq in 2..=5 {
            guard
                .check_replay_only(&envelope(&sender, seq), ObservedSenderRegime::DurableV1)
                .unwrap();
        }

        let err = guard
            .check_replay_only(
                &envelope(&sender, 6),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("a downgrade must fail closed");
        assert_eq!(
            err.downcast_ref::<SenderRegimeDowngrade>()
                .expect("typed downgrade")
                .retained_max_seq,
            5
        );

        // State preserved, and the peer works again as soon as it presents correctly.
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_DURABLE_V1)
        );
        assert_eq!(on_disk(&store, did)["max_seq"].as_u64().unwrap(), 5);
        guard
            .check_replay_only(&envelope(&sender, 6), ObservedSenderRegime::DurableV1)
            .expect("a correctly-presenting connection is unaffected by the stale one");
    }

    /// Phase 21 — the intermediate build's entries are read conservatively.
    ///
    /// The receiver-only-versioning build stamped `semantic_version: 1` and had no
    /// sender axis at all. Its entries must read as `LegacyOrUnproven`, not
    /// durable-v1: it recorded numbers from senders it never asked about.
    #[test]
    fn intermediate_receiver_only_versioned_entries_read_as_unproven() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();

        // Exactly what the intermediate build wrote: current receiver version, no
        // sender_regime key.
        let intermediate = serde_json::json!({
            "max_seq": 510u64,
            "updated_at_ms": ReplayGuard::current_time_ms(),
            "semantic_version": REPLAY_STATE_SEMANTIC_VERSION,
        });
        store
            .put(
                &ReplayGuard::make_max_seq_key(&pk(did)),
                &serde_json::to_vec(&intermediate).unwrap(),
            )
            .unwrap();

        let guard = boot(&store, TestClock::new());
        assert_eq!(
            guard.sequences[&pk(did)].sender_regime,
            SenderRegimeState::LegacyOrUnproven,
            "absence of the sender axis must mean unproven, never durable-v1; a default \
             of durable-v1 would launder every entry the intermediate build wrote"
        );
        assert_eq!(
            guard.sequences[&pk(did)].floor_seq,
            510,
            "the number is still a valid bound inside its own namespace"
        );
    }

    /// **RED (#2517 design gate): absence of local history is not absence of a legacy
    /// namespace.**
    ///
    /// `NoHistory` proves exactly one thing — *this receiver* holds no high-water for
    /// this DID. It does not prove the DID never emitted envelopes under the legacy
    /// ephemeral namespace, and it cannot: a receiver that just joined, or whose state
    /// was repaired, has no way to know what B was doing five seconds ago.
    ///
    /// The observables genuinely overlap. During the freshness window, a captured
    /// envelope signed by B just *before* its upgrade and a legitimate envelope signed
    /// just *after* both carry: a valid B signature, a fresh timestamp, some sequence
    /// number, and a current authenticated connection on which B advertises
    /// `DURABLE_SIGNING_SEQUENCE`. Nothing in the envelope says which namespace
    /// produced it.
    ///
    /// So this test deliberately attributes `DurableV1` to *both* — that is what the
    /// receiver actually sees. Attributing `LegacyOrUnproven` to the captured one
    /// would assume the very fact the receiver is missing.
    #[test]
    fn a_captured_legacy_envelope_must_not_poison_a_fresh_durable_namespace() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();

        // B is legacy-ephemeral with long uptime; this envelope is real traffic,
        // captured off the wire. It is still inside its freshness window.
        let captured_legacy = envelope(&sender, 500);

        // B upgrades. Its durable counter starts at 1. A is a brand-new receiver: no
        // replay history for B at all.
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());
        assert!(
            !guard.sequences.contains_key(&pk(did)),
            "precondition: A has no history for B"
        );

        // The attacker replays the captured legacy envelope. A's view of B — via B's
        // own authenticated connection — says DurableV1, because that is what B is
        // *now*. A cannot tell this envelope from a legitimate post-upgrade one.
        assert!(
            guard
                .check_replay_only(&captured_legacy, ObservedSenderRegime::DurableV1)
                .is_err(),
            "a first durable claim must be held, not accepted; accepting here is what \
             lets a captured legacy sequence become the durable-v1 high-water"
        );
        assert_ne!(
            on_disk(&store, did)["max_seq"].as_u64().unwrap(),
            500,
            "the legacy-namespace number must not be recorded as a durable-v1 high-water"
        );

        // Once the horizon has retired the old namespace, B's real durable sequence is
        // accepted, in a clean namespace.
        clock.advance(HORIZON + Duration::from_secs(1));
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect("B's legitimate durable sequence must be accepted after the horizon");

        // A restart restores a floor of 1, not 500, so B is not locked out.
        let mut restarted = boot(&store, TestClock::new());
        restarted
            .check_replay_only(&envelope(&sender, 2), ObservedSenderRegime::DurableV1)
            .expect(
                "the durable namespace must be uncontaminated by the captured legacy \
                 sequence, or B is locked out permanently with no migration path",
            );
    }

    /// **RED (#2517 design gate): replay-state cleanup must not launder "we forgot"
    /// into "there was never a legacy namespace".**
    ///
    /// `cleanup()` deletes the whole persisted entry for an inactive peer, sender
    /// regime included. If the fresh-namespace fast path keys off "no entry", then
    /// ordinary garbage collection manufactures exactly the unsafe precondition above
    /// — on a receiver that *did* once know better.
    #[test]
    fn cleanup_of_an_inactive_peer_does_not_prove_the_legacy_namespace_never_existed() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        // A knows B as a legacy sender and records a high-water in that namespace.
        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .unwrap();
        assert_eq!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_LEGACY_OR_UNPROVEN)
        );

        // B goes quiet past the inactivity horizon and normal cleanup removes it.
        guard.sequences.get_mut(&pk(did)).unwrap().last_update =
            Instant::now() - Duration::from_secs(7_200);
        guard.cleanup();
        assert!(
            !guard.sequences.contains_key(&pk(did)),
            "precondition: cleanup evicted B"
        );

        // Because this peer was never *established* as durable — only forgotten — the
        // durable claim is held rather than trusted.
        let captured_legacy = envelope(&sender, 400);
        assert!(
            guard
                .check_replay_only(&captured_legacy, ObservedSenderRegime::DurableV1)
                .is_err(),
            "forgetting a high-water through routine cleanup must not be treated as proof \
             that no legacy namespace ever existed"
        );
        assert_ne!(
            regime_on_disk(&store, did),
            u64::from(SENDER_REGIME_DURABLE_V1),
            "cleanup forgot a number; that must not be laundered into a durable-v1 tag"
        );

        clock.advance(HORIZON + Duration::from_secs(1));
        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect("after the horizon the peer establishes a clean durable namespace");
    }

    /// #2517 (mutation control M6): the transition is durably recorded in the
    /// provenance record, not only as a tag on the high-water entry.
    ///
    /// Asserted structurally rather than behaviourally, and that is the point. Both
    /// records carry the transition, so deleting either one alone still leaves a
    /// restart entering *a* 600-second hold — a fresh one is indistinguishable from a
    /// resumed one, because they are behaviourally identical and equally safe. A test
    /// that asserted "restarting holds" therefore passed with the provenance write
    /// removed entirely, proving nothing.
    ///
    /// The redundancy is deliberate: the high-water entry is the one `cleanup()` can
    /// delete, so the provenance record is what has to carry the transition when it
    /// does. Pinning it means asserting the record is actually there.
    #[test]
    fn the_transition_is_recorded_in_the_durable_provenance_record() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let did = sender.did();
        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        guard
            .check_replay_only(
                &envelope(&sender, 500),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .unwrap();
        assert!(
            store
                .get(&ReplayGuard::make_sender_regime_key(&pk(did)))
                .unwrap()
                .is_none(),
            "control: no provenance is written for an ordinary unproven peer, so the \
             common path costs no extra flush"
        );

        guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("transition begins");

        let raw = store
            .get(&ReplayGuard::make_sender_regime_key(&pk(did)))
            .unwrap()
            .expect(
                "the transition must be written to the durable provenance record before \
                 it is relied on; without it, cleanup() removing the high-water entry \
                 erases every trace that a migration was underway",
            );
        assert_eq!(
            u32::from_be_bytes(raw.as_slice().try_into().unwrap()),
            SENDER_REGIME_TRANSITION_TO_DURABLE_V1
        );

        // And it is written *before* the hold takes effect, so a crash cannot resume as
        // trusted legacy state.
        let mut restarted = boot(&store, TestClock::new());
        let err = restarted
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("a restart mid-transition must not accept");
        assert!(err.downcast_ref::<SenderRegimeTransition>().is_some());
    }

    /// #2517 (mutation control M7): an unknown regime in the *provenance* record fails
    /// closed, with no deadline.
    ///
    /// The equivalent unknown-value path on the high-water entry was already covered;
    /// this one was not, so the provenance load could have treated an unrecognised
    /// value as unproven — which is a silent downgrade, since unproven is permissive
    /// enough to establish a fresh durable namespace after a hold.
    #[test]
    fn unknown_provenance_value_fails_closed_and_never_expires() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        // Written by a binary that knows a regime this one does not.
        store
            .put(
                &ReplayGuard::make_sender_regime_key(&pk(sender.did())),
                &99u32.to_be_bytes(),
            )
            .unwrap();

        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());

        for _ in 0..3 {
            let err = guard
                .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
                .expect_err("an uninterpretable provenance value must fail closed");
            assert!(
                err.downcast_ref::<UnsupportedSenderRegime>().is_some(),
                "must be the typed unsupported-regime fault: {err}"
            );
            clock.advance(HORIZON * 5);
        }
    }

    /// Corrupt provenance is not the same as absent provenance.
    ///
    /// Reading an unreadable record as "no record" would downgrade it to unproven,
    /// which then permits establishing a durable namespace after a hold — on evidence
    /// that could not actually be read.
    #[test]
    fn corrupt_provenance_quarantines_rather_than_reading_as_absent() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        store
            .put(
                &ReplayGuard::make_sender_regime_key(&pk(sender.did())),
                b"not-four-bytes-at-all",
            )
            .unwrap();

        let clock = TestClock::new();
        let mut guard = boot(&store, clock.clone());
        let err = guard
            .check_replay_only(&envelope(&sender, 1), ObservedSenderRegime::DurableV1)
            .expect_err("corrupt provenance must not read as absent");
        assert!(
            err.downcast_ref::<ReplayStateUnreadable>().is_some(),
            "expected a typed local-state fault, got: {err}"
        );
        assert!(
            clock.elapsed() < HORIZON,
            "control: the quarantine is being asserted before it could have expired"
        );
    }
}

/// #2640 — durable replay state is keyed by the sender's key, not by its spelling.
///
/// These sit beside the state machine they protect because they seed and read the durable
/// keyspaces directly, which is the only way to prove a *merge* happened rather than a
/// coincidence. The third-party attack itself is exercised through the public API in
/// `tests/respelled_envelope_replay.rs`.
#[cfg(test)]
mod respelled_identity_tests {
    use super::*;
    use crate::envelope::PayloadType;
    use icn_identity::KeyPair;

    /// The base16-lower spelling of the same key. `f` is multibase's base16-lower code.
    fn alias_of(canonical: &Did) -> Did {
        let hex: String = canonical
            .to_verifying_key()
            .unwrap()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let alias = Did::from_str(&format!("did:icn:f{hex}")).expect("base16 spelling parses");
        assert_ne!(
            alias.as_str(),
            canonical.as_str(),
            "CONTROL: the alias must be a different string"
        );
        assert_eq!(
            alias.to_verifying_key().unwrap().as_bytes(),
            canonical.to_verifying_key().unwrap().as_bytes(),
            "CONTROL: the alias must decode to the same key"
        );
        alias
    }

    /// A durable key built from a DID **as spelled** — i.e. what `main` wrote, and what an
    /// alias row looks like in a store today. Deliberately not `make_max_seq_key`, which now
    /// canonicalizes and so could not seed the pre-fix shape at all.
    fn spelled_key(prefix: &[u8], did: &Did) -> Vec<u8> {
        let mut key = prefix.to_vec();
        key.extend_from_slice(did.as_str().as_bytes());
        key
    }

    fn spelled_finalized_key(did: &Did, sequence: u64) -> Vec<u8> {
        let mut key = spelled_key(FINALIZED_PREFIX, did);
        key.push(b':');
        key.extend_from_slice(sequence.to_string().as_bytes());
        key
    }

    fn seed_max_seq(
        store: &dyn Store,
        did: &Did,
        max_seq: u64,
        semantic_version: u32,
        sender_regime: u32,
    ) {
        let entry = MaxSeqEntry {
            max_seq,
            updated_at_ms: ReplayGuard::current_time_ms(),
            semantic_version,
            sender_regime,
        };
        store
            .put(
                &spelled_key(MAX_SEQ_PREFIX, did),
                &serde_json::to_vec(&entry).unwrap(),
            )
            .unwrap();
    }

    fn stored_max_seq(store: &dyn Store, did: &Did) -> Option<MaxSeqEntry> {
        store
            .get(&spelled_key(MAX_SEQ_PREFIX, did))
            .unwrap()
            .map(|raw| serde_json::from_slice(&raw).unwrap())
    }

    fn envelope(sender: &KeyPair, from: &Did, sequence: u64) -> SignedEnvelope {
        SignedEnvelope::new(from, sender, sequence, PayloadType::Gossip, b"m".to_vec()).unwrap()
    }

    /// Two spellings of one sender must reconstruct ONE window, with the **maximum** floor.
    ///
    /// Both directions are seeded on purpose. `sled` scans lexicographically and the base16
    /// alias (`…:f…`) sorts before the base58btc canonical (`…:z…`), so "keep the first row"
    /// and "keep the last row" each happen to produce the right answer for one of the two
    /// senders and the wrong one for the other. Only "keep the maximum" satisfies both.
    #[test]
    fn alias_rows_merge_to_the_maximum_floor_in_both_directions() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());

        // Sender A: the alias row holds the higher number.
        let a = KeyPair::generate().unwrap();
        let a_canonical = a.did().clone();
        let a_alias = alias_of(&a_canonical);
        seed_max_seq(
            store.as_ref(),
            &a_canonical,
            5,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_max_seq(
            store.as_ref(),
            &a_alias,
            42,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        // Sender B: the canonical row holds the higher number.
        let b = KeyPair::generate().unwrap();
        let b_canonical = b.did().clone();
        let b_alias = alias_of(&b_canonical);
        seed_max_seq(
            store.as_ref(),
            &b_canonical,
            42,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_max_seq(
            store.as_ref(),
            &b_alias,
            5,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            guard.peer_count(),
            2,
            "two keys and four spellings must reconstruct exactly two windows"
        );

        for (label, canonical, alias) in [
            ("alias-higher", &a_canonical, &a_alias),
            ("canonical-higher", &b_canonical, &b_alias),
        ] {
            assert_eq!(
                guard.get_max_seq(canonical),
                Some(42),
                "{label}: the merged high-water must be the maximum across spellings"
            );
            assert_eq!(
                guard.get_max_seq(alias),
                Some(42),
                "{label}: the alias spelling must read the same one window"
            );
            assert_eq!(
                stored_max_seq(store.as_ref(), canonical).map(|e| e.max_seq),
                Some(42),
                "{label}: the canonical durable row must hold the merged value"
            );
            assert!(
                stored_max_seq(store.as_ref(), alias).is_none(),
                "{label}: the alias row must be retired once the canonical row is durable"
            );
        }

        // And the merged floor is live: 42 is refused, 43 is the sender's next legitimate
        // sequence and is accepted.
        assert!(
            guard
                .check_replay_only(
                    &envelope(&a, &a_canonical, 42),
                    ObservedSenderRegime::LegacyOrUnproven
                )
                .is_err(),
            "the merged floor must reject a sequence at the maximum"
        );
        assert!(
            guard
                .check_replay_only(
                    &envelope(&a, &a_canonical, 43),
                    ObservedSenderRegime::LegacyOrUnproven
                )
                .is_ok(),
            "the merged floor must not reject the sender's next legitimate sequence"
        );
    }

    /// A sender whose *only* durable row is a non-canonical spelling is re-keyed, not dropped.
    ///
    /// The single-row case is separate from the merge case and is reachable on its own: a node
    /// whose own configured DID was spelled non-canonically writes exactly this shape, and so
    /// does a store in which every canonical row has already aged out of `cleanup()` while an
    /// alias row survived.
    #[test]
    fn a_lone_alias_row_is_re_keyed_onto_the_canonical_spelling() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &alias,
            11,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            stored_max_seq(store.as_ref(), &canonical).map(|e| e.max_seq),
            Some(11),
            "the lone alias row must be re-keyed onto the canonical spelling, not ignored"
        );
        assert!(
            stored_max_seq(store.as_ref(), &alias).is_none(),
            "and retired, so the durable keyspace stops being spelling-distinct"
        );
        assert_eq!(
            guard.get_max_seq(&canonical),
            Some(11),
            "and its floor must be carried into the window, not lost"
        );
        assert!(
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::LegacyOrUnproven
                )
                .is_err(),
            "the carried floor must still reject the sequence it recorded"
        );
    }

    /// The strongest established sender regime wins the merge, because the weaker ones each
    /// reach a promotion that would reset the merged floor to zero.
    #[test]
    fn alias_merge_keeps_the_strongest_sender_regime() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // The higher number is tagged unproven; the lower one is tagged durable. A merge that
        // took the regime from whichever row supplied the number would end up unproven.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            3,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            guard.get_max_seq(&canonical),
            Some(10),
            "the floor is still the maximum across spellings"
        );

        // Observable consequence: an established DurableV1 sender presenting as unproven is a
        // downgrade and is refused. Had the merge kept `LegacyOrUnproven`, this would be the
        // ordinary steady state and sequence 11 would be accepted.
        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("a durable-established sender must not be accepted as unproven");
        assert!(
            err.downcast_ref::<SenderRegimeDowngrade>().is_some(),
            "expected the merged regime to be DurableV1; got: {err}"
        );
    }

    /// An unreadable alias row is neither merged nor deleted, and quarantines the merged
    /// sender — the fail-closed reading of "state existed here and we cannot read it".
    #[test]
    fn an_unreadable_alias_row_quarantines_the_merged_sender() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        store
            .put(&spelled_key(MAX_SEQ_PREFIX, &alias), b"{not json")
            .unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            store
                .get(&spelled_key(MAX_SEQ_PREFIX, &alias))
                .unwrap()
                .is_some(),
            "an unreadable row must survive the merge so an operator can still see it"
        );

        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 99),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("a sender with unreadable state must be held, not admitted");
        assert!(
            err.downcast_ref::<ReplayStateUnreadable>().is_some(),
            "expected a quarantine on the merged sender; got: {err}"
        );
    }

    /// The canonical row is exactly where an unreadable row must be left alone.
    ///
    /// Merging over it would replace "state existed here and we cannot read it" — the fact the
    /// load pass turns into a quarantine — with a floor derived only from the spellings that
    /// happened to be readable, which may be lower. The whole group is left untouched instead.
    #[test]
    fn an_unreadable_canonical_row_is_never_overwritten_by_a_merge() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // The corrupt row is the canonical one; the readable row is the alias.
        store
            .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
            .unwrap();
        seed_max_seq(
            store.as_ref(),
            &alias,
            3,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            store
                .get(&spelled_key(MAX_SEQ_PREFIX, &canonical))
                .unwrap()
                .as_deref(),
            Some(&b"{not json"[..]),
            "the unreadable canonical row must survive the merge byte for byte"
        );
        assert!(
            store
                .get(&spelled_key(MAX_SEQ_PREFIX, &alias))
                .unwrap()
                .is_some(),
            "and its alias must not be retired against a merge that was never installed"
        );

        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 99),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("the sender must still be quarantined on its unreadable state");
        assert!(
            err.downcast_ref::<ReplayStateUnreadable>().is_some(),
            "expected the quarantine the corrupt canonical row licenses; got: {err}"
        );
    }

    /// Finalized rows are a set, and the merge is their union — under either spelling.
    ///
    /// Both halves are asserted, because they are carried by different code. The *behaviour*
    /// (either spelling reads one finalized set) follows from the in-memory keying alone: the
    /// load pass maps each stored key's DID to its principal. What the store-level merge adds
    /// is that the alias row stops existing on disk, which is the half the issue's acceptance
    /// criterion asks for and the half `cleanup()` needs in order to be able to delete it.
    #[test]
    fn finalized_alias_rows_merge_onto_one_sender() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // A low floor, so the finalized set — not the floor — is what rejects 7 and 8.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            1,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        let entry = serde_json::to_vec(&FinalizedEntry {
            finalized_at_ms: ReplayGuard::current_time_ms(),
        })
        .unwrap();
        store
            .put(&spelled_finalized_key(&canonical, 7), &entry)
            .unwrap();
        store
            .put(&spelled_finalized_key(&alias, 8), &entry)
            .unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // The physical half: one row per (sender, sequence), under the canonical spelling.
        assert!(
            store
                .get(&spelled_finalized_key(&canonical, 8))
                .unwrap()
                .is_some(),
            "the alias's finalized row must be re-keyed onto the canonical spelling"
        );
        assert!(
            store
                .get(&spelled_finalized_key(&alias, 8))
                .unwrap()
                .is_none(),
            "and the alias row itself must be retired — otherwise the durable keyspace is \
             still spelling-distinct and cleanup() cannot reach it"
        );

        // The behavioural half.
        for sequence in [7u64, 8] {
            assert!(
                guard.is_finalized(&canonical, sequence),
                "sequence {sequence} must be finalized for the merged sender"
            );
            assert!(
                guard.is_finalized(&alias, sequence),
                "sequence {sequence} must read the same finalized set under any spelling"
            );
            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, sequence),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .expect_err("a finalized sequence must never be accepted again");
            assert!(
                err.to_string().contains("finalized"),
                "expected a finalization rejection for {sequence}; got: {err}"
            );
        }
    }

    /// A store that counts writes, so "this costs honest peers nothing" can be asserted
    /// rather than asserted-about.
    #[derive(Default)]
    struct CountingStore {
        data: std::sync::Mutex<std::collections::BTreeMap<Vec<u8>, Vec<u8>>>,
        puts: std::sync::atomic::AtomicUsize,
        deletes: std::sync::atomic::AtomicUsize,
        /// Every mutating call in the order it happened, so the crash-safety *ordering* can
        /// be asserted and not merely the call counts. Counting proves how much was written;
        /// only the order proves a crash cannot land between them and lose a floor.
        ops: std::sync::Mutex<Vec<String>>,
        fail_scan: bool,
    }

    impl CountingStore {
        fn reset_counters(&self) {
            self.puts.store(0, Ordering::SeqCst);
            self.deletes.store(0, Ordering::SeqCst);
            self.ops.lock().unwrap().clear();
        }

        fn op_log(&self) -> Vec<String> {
            self.ops.lock().unwrap().clone()
        }
        fn counts(&self) -> (usize, usize) {
            (
                self.puts.load(Ordering::SeqCst),
                self.deletes.load(Ordering::SeqCst),
            )
        }
    }

    impl Store for CountingStore {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(self.data.lock().unwrap().get(key).cloned())
        }
        fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
            self.puts.fetch_add(1, Ordering::SeqCst);
            self.ops
                .lock()
                .unwrap()
                .push(format!("put:{}", String::from_utf8_lossy(key)));
            self.data
                .lock()
                .unwrap()
                .insert(key.to_vec(), value.to_vec());
            Ok(())
        }
        fn delete(&self, key: &[u8]) -> Result<()> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            self.ops
                .lock()
                .unwrap()
                .push(format!("delete:{}", String::from_utf8_lossy(key)));
            self.data.lock().unwrap().remove(key);
            Ok(())
        }
        fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            if self.fail_scan {
                anyhow::bail!("simulated unreadable replay keyspace");
            }
            Ok(self
                .data
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
        fn flush(&self) -> Result<()> {
            self.ops.lock().unwrap().push("flush".to_string());
            Ok(())
        }
        fn get_replica_metadata(
            &self,
            _h: &icn_store::ContentHash,
        ) -> Result<Option<icn_store::ReplicaMetadata>> {
            Ok(None)
        }
        fn put_replica_metadata(&self, _m: &icn_store::ReplicaMetadata) -> Result<()> {
            Ok(())
        }
        fn list_replica_hashes(&self) -> Result<Vec<icn_store::ContentHash>> {
            Ok(vec![])
        }
    }

    /// Canonicalization is idempotent, and free for a store that is already canonical.
    ///
    /// The zero-write case is not a micro-optimisation: every honest peer is in it on every
    /// start, because production only ever signs with `keypair.did()`, which is the canonical
    /// base58btc spelling. A pass that rewrote every row on every boot would make a security
    /// fix look like a storage regression.
    #[test]
    fn canonicalization_is_idempotent_and_free_when_no_alias_exists() {
        let store = Arc::new(CountingStore::default());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // Already canonical: nothing to merge, nothing to write.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            7,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        store.reset_counters();
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();
        assert_eq!(
            store.counts(),
            (0, 0),
            "a store with no alias rows must not be rewritten on load"
        );

        // Introduce an alias row: one merge, one retirement.
        seed_max_seq(
            store.as_ref(),
            &alias,
            9,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        store.reset_counters();
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();
        let (puts, deletes) = store.counts();
        assert_eq!(puts, 1, "the merged row is written exactly once");
        assert_eq!(deletes, 1, "the alias row is retired exactly once");
        assert_eq!(guard.get_max_seq(&canonical), Some(9));

        // Re-running finds a canonical store again and does nothing: idempotent.
        store.reset_counters();
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();
        assert_eq!(
            store.counts(),
            (0, 0),
            "re-running the merge on an already-merged store must be a no-op"
        );
        assert_eq!(guard.get_max_seq(&canonical), Some(9));
    }

    /// The merged canonical row is durable **before** any alias row is retired.
    ///
    /// Call *counts* cannot see this: a delete-first implementation writes exactly one row
    /// and retires exactly one, which is what the idempotence test above already asserts.
    /// Only the order distinguishes them, and the difference is a lost floor — a crash
    /// between a delete-first retire and the canonical write leaves the sender with **no**
    /// durable high-water at all, so its whole sequence space is replayable on the next boot.
    /// The safe interruption point is the one that leaves a duplicate row, because duplicates
    /// re-merge to the identical value (maximum, strongest, union are all idempotent).
    #[test]
    fn the_canonical_row_is_durable_before_any_alias_is_retired() {
        let store = Arc::new(CountingStore::default());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            5,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            9,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        store.reset_counters();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        let ops = store.op_log();
        let canonical_op = format!(
            "put:{}",
            String::from_utf8_lossy(&ReplayGuard::make_max_seq_key(&pk(&canonical)))
        );
        let alias_op = format!(
            "delete:{}",
            String::from_utf8_lossy(&spelled_key(MAX_SEQ_PREFIX, &alias))
        );

        // CONTROL: both operations must actually be in the log, or the ordering assertions
        // below would hold vacuously over an empty or half-empty sequence.
        let put_at = ops
            .iter()
            .position(|o| *o == canonical_op)
            .unwrap_or_else(|| panic!("CONTROL: the canonical row must be written; ops={ops:?}"));
        let delete_at = ops
            .iter()
            .position(|o| *o == alias_op)
            .unwrap_or_else(|| panic!("CONTROL: the alias row must be retired; ops={ops:?}"));
        let flush_at = ops
            .iter()
            .skip(put_at)
            .position(|o| o == "flush")
            .map(|i| i + put_at)
            .unwrap_or_else(|| panic!("CONTROL: the canonical write must be flushed; ops={ops:?}"));

        assert!(
            put_at < flush_at && flush_at < delete_at,
            "the merged canonical row must be written AND flushed before any alias row is \
             retired; a crash in the other order loses the sender's floor entirely. \
             put={put_at} flush={flush_at} delete={delete_at} ops={ops:?}"
        );

        // And no alias is retired ahead of the canonical write under any spelling.
        let first_delete = ops.iter().position(|o| o.starts_with("delete:")).unwrap();
        assert!(
            put_at < first_delete,
            "no row may be retired before the merged row exists; ops={ops:?}"
        );
    }

    /// The merge keeps the **most restrictive** semantic version, not the newest.
    ///
    /// Ranked by how much the load pass refuses: an unrecognised version holds with no
    /// deadline, the legacy version holds for the freshness horizon and discards the number,
    /// and the current version yields a usable floor. Taking the *less* restrictive of two
    /// spellings would let an alias row written under a regime this binary can still bound be
    /// laundered into a current-regime floor — adopting a number whose meaning was never
    /// established, which is the one merge outcome that can admit traffic rather than refuse
    /// it.
    #[test]
    fn alias_merge_keeps_the_most_restrictive_semantic_version() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // The canonical row is current-regime and carries the higher number; the alias row
        // predates semantic versioning. Taking the newer version would produce a clean floor
        // of 10 and accept sequence 11 as ordinary traffic.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            3,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        // CONTROL: the two regimes must genuinely differ, or "most restrictive" is untested.
        assert_ne!(
            REPLAY_STATE_SEMANTIC_VERSION, LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            "CONTROL: the seeded versions must differ"
        );

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            stored_max_seq(store.as_ref(), &canonical).map(|e| e.semantic_version),
            Some(LEGACY_REPLAY_STATE_SEMANTIC_VERSION),
            "the merged row must carry the most restrictive regime of the two spellings"
        );

        // Observable consequence: the legacy regime is a bounded hold that discards the
        // number, so the sender is refused outright rather than admitted against a floor
        // whose meaning was never established.
        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("a sender merged onto a legacy-regime row must be held, not admitted");
        assert!(
            err.downcast_ref::<ReplayStateLegacy>().is_some(),
            "expected the merged semantic version to be the legacy one; got: {err}"
        );
    }

    /// A load that fails must not disarm the guard.
    ///
    /// Load-outcome hardening that this fix depends on: canonicalization performs durable
    /// writes *during* the load, so the load's failure path had to stop being one that
    /// latches "initialized" and then runs the next message against an empty window map —
    /// which would accept every replay the store was holding evidence of.
    #[test]
    fn a_failed_load_leaves_the_guard_disarmed_for_nobody() {
        let store = Arc::new(CountingStore {
            fail_scan: true,
            ..Default::default()
        });
        let sender = KeyPair::generate().unwrap();
        let env = envelope(&sender, sender.did(), 1);

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());

        assert!(
            guard
                .check_replay_only(&env, ObservedSenderRegime::LegacyOrUnproven)
                .is_err(),
            "a message must not be accepted while replay state cannot be read"
        );
        assert!(
            !guard.is_initialized(),
            "a failed load must not latch; otherwise the next call skips it entirely"
        );
        assert!(
            guard
                .check_replay_only(&env, ObservedSenderRegime::LegacyOrUnproven)
                .is_err(),
            "the second message must be refused too — the failure must not resolve itself \
             into an empty-state acceptance"
        );
        assert!(!guard.is_initialized());
    }

    /// The replay identity fails closed. There is no fallback to the DID's spelling.
    #[test]
    fn an_undecodable_sender_did_fails_closed() {
        // `Did::from_anchor_id` bypasses validation, and this anchor id is not a curve point.
        let undecodable = Did::from_anchor_id(&[2u8; 32]);
        assert!(
            undecodable.to_verifying_key().is_err(),
            "CONTROL: this DID must genuinely name no Ed25519 key"
        );

        let sender = KeyPair::generate().unwrap();
        let mut forged = envelope(&sender, sender.did(), 1);
        forged.from = undecodable.clone();

        let mut guard = ReplayGuard::new(300, 3600);
        let err = guard
            .check_replay_only(&forged, ObservedSenderRegime::LegacyOrUnproven)
            .expect_err("a sender with no derivable key must be refused, not keyed by its text");
        assert!(
            err.downcast_ref::<ReplayIdentityUndecodable>().is_some(),
            "the refusal must be typed as an identity failure, not a replay; got: {err}"
        );
        assert_eq!(
            guard.peer_count(),
            0,
            "a refused sender must not have created a window keyed by its spelling"
        );

        assert!(
            guard.finalize(&undecodable, 1).is_err(),
            "finalize must refuse a DID that names no replay identity"
        );
        assert!(
            guard.is_finalized(&undecodable, 1),
            "is_finalized must answer with the blocking value when it cannot identify the sender"
        );
        assert_eq!(
            guard.get_max_seq(&undecodable),
            None,
            "no high-water can be reported for a sender that cannot be identified"
        );
    }
}
