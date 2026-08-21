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

/// Which of the sender's two sequence namespaces produced the number a window holds
/// (#2644).
///
/// Deliberately **not** [`SenderRegimeState`], and deliberately only two variants. The two
/// types answer different questions and are set by different evidence:
///
/// * `SenderRegimeState` is what this receiver believes about the *sender* — including
///   `TransitionToDurableV1`, which is a statement about a migration in flight and names no
///   namespace at all. A transitioning window still holds a **legacy** number; that is
///   exactly what `persist_max_seq_durable(.., legacy_max_seq, TRANSITION)` records.
/// * This is what produced `max_seq` / `floor_seq`. A number is comparable only inside it.
///
/// It exists because the two were previously read off one field, and the promotion at the
/// end of a sender-regime migration discards the retained number — correct when that number
/// belongs to the namespace being retired, and a replay fail-open when it is already a
/// durable-v1 bound that a current-version row established.
///
/// Not persisted. Like [`SequenceWindow::sender_regime_from_current_version`] it is
/// re-derived from the stored rows on every load, so it cannot drift from the rows it
/// describes, and its `LegacyOrUnproven` default is the direction that discards rather than
/// keeps — the one that costs a bound rather than inventing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumericNamespace {
    /// The number is the sender's previous, unproven numbering — or there is no number.
    ///
    /// Both spellings collapse here on purpose: a completed migration discards the retained
    /// number, and discarding `0` is what `SequenceWindow::new` already holds. Splitting
    /// "absent" out would create a distinction nothing downstream could act on, and would
    /// invite exactly the `max_seq == 0` proxy this type exists to avoid.
    LegacyOrUnproven,

    /// The number was produced by the sender's durable-v1 numbering.
    ///
    /// It survives a sender-regime promotion, because a promotion retires the namespace
    /// *before* durable-v1 and this number is not in it.
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

    /// Whether [`SequenceWindow::sender_regime`] was established by a **current**-semantic-
    /// version `replay_max_seq` row during the load pass (#2640).
    ///
    /// Read in exactly one place: the [`PeerHold::MigratingFromLegacy`] expiry in
    /// [`ReplayGuard::check_replay_only`], which demotes the regime to `LegacyOrUnproven`
    /// only when no such row established it.
    ///
    /// An explicit provenance bit rather than a test on `max_seq`, because "a current-version
    /// row established this regime" and "the number happens to be non-zero" are different
    /// facts: a current-version row legitimately carries `max_seq == 0` — that is exactly what
    /// a completed sender-regime promotion writes (`persist_max_seq_durable(.., 0,
    /// DURABLE_V1)`). Inferring provenance from the number would silently demote precisely the
    /// peers that finished a migration cleanly.
    ///
    /// Deliberately **not** persisted. It is re-derived from the store on every load, so it
    /// cannot drift from the rows it describes, and its `false` default is the fail-closed
    /// direction — demote, costing the peer a hold — rather than the fail-open one.
    sender_regime_from_current_version: bool,

    /// Which sender namespace produced [`SequenceWindow::max_seq`] and
    /// [`SequenceWindow::floor_seq`] (#2644).
    ///
    /// Read in exactly one place: the [`PeerHold::MigratingSenderRegime`] promotion in
    /// [`ReplayGuard::check_replay_only`], which discards the retained number only when the
    /// namespace being retired is the one that produced it.
    ///
    /// Kept beside [`SequenceWindow::sender_regime`] rather than derived from it because
    /// `sender_regime` is overwritten by evidence that says nothing about any number —
    /// provenance establishes a namespace, a hold expiry demotes one — and each of those
    /// writes used to silently re-tag the number too. See [`NumericNamespace`].
    numeric_namespace: NumericNamespace,
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
///
/// `Debug` carries no sender-supplied material — every field is a version tag, a regime tag,
/// or a receiver-local duration — and is what lets the hold-ordering properties name the
/// offending variant when they fail.
#[derive(Clone, Copy, Debug)]
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

impl PeerHold {
    /// How much this hold refuses, as a total order. Higher refuses more.
    ///
    /// Derived from what each hold *permits*, never from declaration order — see
    /// [`SequenceWindow::install_hold_conservatively`] for why the load pass needs an order at
    /// all. One arm per variant, and [`Self::ranks_are_distinct`] pins that the ranks stay
    /// distinct, so "equal rank" means "same variant" and adding a variant without deciding
    /// where it sits is a compile error rather than a silent tie.
    ///
    /// The order, weakest first, and the reason for each step:
    ///
    /// 1. [`PeerHold::MigratingFromLegacy`] — bounded by elapsed time, and its expiry
    ///    **destroys the replay floor** (`max_seq` and `floor_seq` are reset to 0). That is
    ///    correct when it is the only evidence, because the number it discards is its own
    ///    uninterpretable one. It is the weakest hold to hold *jointly*, because a floor
    ///    another row established is destroyed with it.
    /// 2. [`PeerHold::Unreadable`] — bounded by the same elapsed time, but its expiry clears
    ///    only the hold and leaves the window's floor standing. Ranked above
    ///    `MigratingFromLegacy` because a retained floor rejects a superset **permanently**,
    ///    whereas `MigratingFromLegacy`'s one advantage — demoting the regime to
    ///    `LegacyOrUnproven`, so a durable claim must re-earn a transition hold — only
    ///    *delays* an acceptance that then lands on a floor of 0 anyway. Permanent protection
    ///    outranks a delay.
    /// 3. [`PeerHold::MigratingSenderRegime`] — bounded by elapsed time **and** by live
    ///    durable-v1 evidence on the message that would release it (#2517). It therefore
    ///    refuses everything the two above refuse, for at least as long, plus every case where
    ///    the evidence never arrives. Ranking it *below* `Unreadable` would be the real
    ///    hazard: a window loaded as `TransitionToDurableV1` whose bounded hold clears without
    ///    promoting falls into the `(TransitionToDurableV1, _)` arm of the regime match, which
    ///    fails closed with no way out — one corrupt alias row would permanently brick a
    ///    sender that was only mid-migration.
    /// 4. [`PeerHold::UnsupportedSenderRegime`] — no deadline. Nothing elapsed time can do
    ///    makes an unknown namespace tag interpretable, so it outranks every bounded hold.
    /// 5. [`PeerHold::UnsupportedVersion`] — no deadline either, and refuses exactly as much
    ///    as (4). It is ranked above only so the combination is deterministic, and this way
    ///    round because it is the broader statement of ignorance: a row whose semantic version
    ///    has no meaning in this binary has no established meaning for its `sender_regime`
    ///    field either, so it is the more accurate thing to put in front of an operator. The
    ///    same tie-break convention as [`ReplayGuard::most_restrictive_semantic_version`].
    fn rank(&self) -> u8 {
        match self {
            PeerHold::MigratingFromLegacy { .. } => 1,
            PeerHold::Unreadable { .. } => 2,
            PeerHold::MigratingSenderRegime { .. } => 3,
            PeerHold::UnsupportedSenderRegime { .. } => 4,
            PeerHold::UnsupportedVersion { .. } => 5,
        }
    }

    /// The hold that refuses more, or — for two holds of the same kind — the one that refuses
    /// for longer.
    ///
    /// Commutative and idempotent, which is what lets the load pass apply it row by row in
    /// whatever order the store hands the rows over and still reach one answer.
    fn stronger_of(a: PeerHold, b: PeerHold) -> PeerHold {
        match a.rank().cmp(&b.rank()) {
            std::cmp::Ordering::Less => b,
            std::cmp::Ordering::Greater => a,
            std::cmp::Ordering::Equal => match (a, b) {
                // Same kind twice: keep the later deadline, so no combination can shorten a
                // hold, and the larger tag, so the operator-facing diagnostic is deterministic.
                (PeerHold::Unreadable { until: x }, PeerHold::Unreadable { until: y }) => {
                    PeerHold::Unreadable { until: x.max(y) }
                }
                (
                    PeerHold::MigratingSenderRegime { until: x },
                    PeerHold::MigratingSenderRegime { until: y },
                ) => PeerHold::MigratingSenderRegime { until: x.max(y) },
                (
                    PeerHold::MigratingFromLegacy {
                        until: x,
                        from_version: vx,
                    },
                    PeerHold::MigratingFromLegacy {
                        until: y,
                        from_version: vy,
                    },
                ) => PeerHold::MigratingFromLegacy {
                    until: x.max(y),
                    from_version: vx.max(vy),
                },
                (
                    PeerHold::UnsupportedSenderRegime { found_regime: x },
                    PeerHold::UnsupportedSenderRegime { found_regime: y },
                ) => PeerHold::UnsupportedSenderRegime {
                    found_regime: x.max(y),
                },
                (
                    PeerHold::UnsupportedVersion { found_version: x },
                    PeerHold::UnsupportedVersion { found_version: y },
                ) => PeerHold::UnsupportedVersion {
                    found_version: x.max(y),
                },
                // Unreachable while `rank` has one arm per variant, which
                // `ranks_are_distinct` pins. Kept rather than `unreachable!()` because the
                // fail-safe direction for a rule whose whole job is "never weaken" is to keep
                // what is already installed, not to panic a receiver at load time.
                (incumbent, _) => incumbent,
            },
        }
    }
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
                // No interpretation of the store was installed. That is a property of the
                // loader, not an assumption about where it failed: it builds every window in
                // a local map and assigns `self.sequences` only after the last fallible step,
                // so an error here means the guard still holds exactly what it held before.
                //
                // Leaving `initialized` set would make the *next* call skip the load entirely
                // and run against that state — which, on a first load, accepts every replay
                // the store was holding evidence of. Clearing it makes a load failure stay a
                // failure until it is fixed, instead of resolving itself into a fail-open on
                // the second message. Together the two give the retry a clean base: it
                // re-derives everything from the repaired store and inherits nothing from the
                // attempt that failed.
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
        // below interprets them, so the #2514 / #2517 state machine sees one readable row per
        // sender in every case where a merge is installed at all.
        //
        // It is not every case: a row this pass cannot parse is neither merged nor deleted,
        // and when the *canonical* row is that unreadable one the whole group is left as it
        // is. Several rows for one sender therefore still reach the loop below, which is why
        // the per-row holds it derives are combined with `install_hold_conservatively` rather
        // than assigned.
        //
        // Taken before `store` is bound because it needs `&self` for the clock-independent
        // helpers and writes through the store itself.
        self.canonicalize_durable_identities()?;

        let store = match &self.store {
            Some(s) => s,
            None => return Ok(0),
        };

        // Every window this load derives, built off to the side. Nothing below touches
        // `self.sequences`, so a failure at any `?` after this point leaves the guard exactly
        // as it was rather than half-rewritten — see the commit at the end of this function.
        let mut loaded: HashMap<SenderPrincipal, SequenceWindow> = HashMap::new();

        let mut count = 0;

        // Peers whose durable state is unreadable are quarantined until every
        // envelope that could have been accepted before this restart is certain
        // to fail freshness. Receiver-local monotonic time; see
        // `envelope_validity_horizon` for the derivation.
        let quarantine_until = self.clock.elapsed() + self.envelope_validity_horizon();

        // Load max sequences.
        //
        // Two passes, and the split is the security property rather than a tidy-up. Where
        // canonicalization declined to rewrite storage — an unreadable canonical row leaves
        // its readable alias rows in place — several physical rows for one principal reach
        // this loop, and each of them used to *assign* `max_seq` and `floor_seq`. That is
        // last-write-wins over an order which is a property of the spellings an attacker
        // chose, so a lower high-water could overwrite a higher one, and the floor left
        // standing once the bounded hold expired was the weaker of the two.
        //
        // Pass one combines the rows; pass two interprets the results and joins their
        // *effects*. The unit that is merged is one `semantic_version`, because that is the
        // unit within which a number is interpretable at all:
        //
        // * **Within** a version, `merge_max_seq` — literally the function canonicalization
        //   uses, not a second implementation of it — keeps `(max_seq, sender_regime)`
        //   coupled, so mixed sender namespaces still resolve to a transition rather than
        //   laundering one namespace's number under another's label (#2517, `5be3fdf0`).
        // * **Across** versions the arms below are not comparable: the current-version arm
        //   establishes a floor, the legacy arm establishes a bounded hold and deliberately
        //   establishes no floor. Merging those into one row lets whichever arm won discard
        //   the other's contribution — a `v0` sibling row would erase an interpretable
        //   current-version floor outright. Each arm is therefore run and its effects joined:
        //   holds by `install_hold_conservatively`, the floor by maximum. The join refuses at
        //   least as much as any single row did, which is the whole invariant.
        //
        // Only the current-version arm assigns a number or a regime, and a principal has at
        // most one current-version group, so the joins below are in practice single-sourced.
        // They are written as joins anyway: the safety must be structural rather than a
        // coincidence of which arms happen to assign today.
        let entries = store
            .scan(MAX_SEQ_PREFIX)
            .context("Failed to scan replay max_seq entries")?;

        // One combined high-water per (principal, semantic_version), established before
        // anything interprets it.
        let mut high_water: HashMap<(SenderPrincipal, u32), MaxSeqEntry> = HashMap::new();

        for (key, value) in entries {
            let Some(did) = Self::parse_max_seq_key(&key) else {
                continue;
            };
            // Rows are keyed by the sender's key, never by its spelling (#2640).
            let Ok(principal) = Self::window_key(&did, "replay max_seq") else {
                continue;
            };
            let entry = match serde_json::from_slice::<MaxSeqEntry>(&value) {
                Ok(entry) => entry,
                Err(e) => {
                    // The key's existence proves we had state for this peer, but
                    // its high-water is unreadable, so no sequence can be shown
                    // to be new. Failing open would hand an attacker a replay
                    // window; instead reject this peer's traffic until anything
                    // captured before the restart is too old to be replayed.
                    let window = loaded.entry(principal).or_insert_with(SequenceWindow::new);
                    window.install_hold_conservatively(PeerHold::Unreadable {
                        until: quarantine_until,
                    });
                    tracing::error!(
                        peer = %principal,
                        error = %e,
                        quarantine_secs = self.envelope_validity_horizon().as_secs(),
                        "Corrupt replay state entry; quarantining this peer until captured \
                         traffic can no longer be fresh"
                    );
                    continue;
                }
            };

            match high_water.entry((principal, entry.semantic_version)) {
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    let combined = Self::merge_max_seq(occupied.get(), &entry);
                    tracing::warn!(
                        peer = %principal,
                        combined_max_seq = combined.max_seq,
                        combined_sender_regime = combined.sender_regime,
                        combined_semantic_version = combined.semantic_version,
                        "Combining spelling-distinct replay high-water rows that \
                         canonicalization left in place (#2640). The combined row can only \
                         refuse at least as much as any single one of them did"
                    );
                    *occupied.get_mut() = combined;
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(entry);
                }
            }
        }

        // `count` is peers rather than physical rows, which is what the field it is logged
        // under has always claimed, and is identical for the one-row-per-sender store every
        // honest deployment has.
        count += high_water
            .keys()
            .map(|(principal, _)| *principal)
            .collect::<std::collections::HashSet<_>>()
            .len();

        // Interpret each version group and join its effect into the window. `merge_max_seq`
        // is commutative and associative and the joins below are commutative, so the result
        // does not depend on the scan order that produced any of it.
        for ((principal, _semantic_version), entry) in high_water {
            let window = loaded.entry(principal).or_insert_with(SequenceWindow::new);

            // The entry parsed, but parsing is a statement about schema and this
            // decision is about semantics. Enumerated deliberately rather than tested
            // with `!= current`: "we know exactly what this used to mean" and "we have
            // no idea what this means" are different facts and must not share a branch.
            // A future regime added here needs its own arm with an explicit migration;
            // until it has one it falls to the catch-all and fails closed, which is the
            // safe direction to forget something in.
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
                            window.max_seq = window.max_seq.max(entry.max_seq);
                            window.floor_seq = window.floor_seq.max(entry.max_seq);
                            window.sender_regime = SenderRegimeState::DurableV1;
                            window.sender_regime_from_current_version = true;
                            // The number came out of the sender's durable-v1 numbering,
                            // and stays comparable against it however this window's
                            // *regime* is later re-established (#2644).
                            window.numeric_namespace = NumericNamespace::DurableV1;

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
                            window.max_seq = window.max_seq.max(entry.max_seq);
                            window.floor_seq = window.floor_seq.max(entry.max_seq);
                            window.sender_regime = SenderRegimeState::LegacyOrUnproven;
                            window.sender_regime_from_current_version = true;
                            // Recorded explicitly, because it is the fact the provenance
                            // pass below must not overwrite: this number is a *legacy*
                            // number, said so by a row this binary can read in full
                            // (#2644).
                            window.numeric_namespace = NumericNamespace::LegacyOrUnproven;

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
                            window.max_seq = window.max_seq.max(entry.max_seq);
                            window.floor_seq = window.floor_seq.max(entry.max_seq);
                            window.sender_regime = SenderRegimeState::TransitionToDurableV1;
                            window.sender_regime_from_current_version = true;
                            // A transition retains the *legacy* high-water — that is
                            // literally what `persist_max_seq_durable(.., legacy_max_seq,
                            // TRANSITION)` wrote — so the promotion that ends this hold is
                            // right to discard it (#2644).
                            window.numeric_namespace = NumericNamespace::LegacyOrUnproven;
                            window.install_hold_conservatively(PeerHold::MigratingSenderRegime {
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
                            window.install_hold_conservatively(PeerHold::UnsupportedSenderRegime {
                                found_regime,
                            });

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
                    window.install_hold_conservatively(PeerHold::MigratingFromLegacy {
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
                    window.install_hold_conservatively(PeerHold::UnsupportedVersion {
                        found_version,
                    });

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
        }

        // Apply established sender-regime provenance (#2517).
        //
        // Authoritative about **one** thing, and applied *after* the max_seq entries so it
        // settles that thing: whether this DID's legacy namespace was ever proven retired.
        // Only that licenses interpreting a durable claim, and no high-water row records it
        // — provenance outlives the high-water by design, so a peer aged out by `cleanup()`
        // is found here with no numeric state at all.
        //
        // It is NOT authoritative about the number, and running last does not make it so
        // (#2644). The two keyspaces are separate evidence axes: a `replay_max_seq` row
        // carries a `sender_regime` field that says which namespace produced *that number*,
        // while provenance is a lone version-less `u32` that says which namespace the sender
        // was last known to have established. Where both are present and they disagree, this
        // pass may re-establish the regime but must never re-tag the number — a bound
        // detached from the namespace that produced it is not a bound, and the direction the
        // detachment runs decides whether the result over-blocks an honest peer or hands an
        // attacker a replay window. The arms below therefore route a disagreement into the
        // existing migration rather than resolving it by assignment.
        //
        // Two passes, for the same reason the high-water load above has two (#2644). Where
        // canonicalization declined to rewrite storage — an unreadable *canonical* row leaves
        // every readable alias row standing, by design — several readable provenance rows for
        // one principal reach this point, and each of them used to apply its own effect in
        // turn. A row still reading `TransitionToDurableV1` therefore installed a
        // `MigratingSenderRegime` hold even when a sibling row proved that migration had
        // already finished, and that hold's expiry with live durable-v1 evidence promotes,
        // resetting `max_seq` and `floor_seq` to 0. A stale alias could so destroy a floor
        // that a current-version high-water and a durable provenance sibling both licensed,
        // handing an authenticated sender back the very sequence numbers that floor rejected.
        //
        // Pass one folds the readable rows per principal with
        // `joined_sender_regime_provenance`; pass two applies the one logical value once.
        // Unreadable rows answer a different question — "a record exists here whose meaning
        // is unavailable" — so they contribute only a bounded quarantine and are composed
        // through `install_hold_conservatively`, which is already commutative. Nothing below
        // depends on the order `sled` hands the rows over, which is a property of the
        // spellings an attacker picked.
        let provenance = store
            .scan(SENDER_REGIME_PREFIX)
            .context("Failed to scan sender regime provenance")?;

        let mut joined_provenance: HashMap<SenderPrincipal, u32> = HashMap::new();
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
                //
                // Applied here rather than folded into the join above because it is not a
                // provenance *value*: it establishes no namespace, and it must not be able
                // to outvote a readable sibling in either direction. Its whole contribution
                // is the bounded hold, and that composes conservatively on its own.
                let window = loaded.entry(principal).or_insert_with(SequenceWindow::new);
                window.install_hold_conservatively(PeerHold::Unreadable {
                    until: quarantine_until,
                });
                tracing::error!(peer = %principal, "Corrupt sender regime provenance; quarantining");
                continue;
            };
            let found = u32::from_be_bytes(raw);
            match joined_provenance.entry(principal) {
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    let combined = Self::joined_sender_regime_provenance(*occupied.get(), found);
                    tracing::warn!(
                        peer = %principal,
                        combined_regime = combined,
                        "Joining spelling-distinct sender regime provenance rows that \
                         canonicalization left in place (#2644); the strongest established \
                         regime wins, because the weaker ones each reach a promotion that \
                         would reset the replay floor"
                    );
                    *occupied.get_mut() = combined;
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(found);
                }
            }
        }

        for (principal, found) in joined_provenance {
            let window = loaded.entry(principal).or_insert_with(SequenceWindow::new);

            match found {
                SENDER_REGIME_DURABLE_V1 => {
                    // Already proven. If the high-water aged out, this peer resumes
                    // with no numeric bound but keeps its established namespace, so it
                    // pays no second migration hold.
                    //
                    // `sender_regime_from_current_version` is deliberately NOT set here.
                    // Provenance is version-less — one `u32`, with no semantic version
                    // beside it — so it cannot state that a *current-version* row
                    // established this regime. That bit is set only by the current-version
                    // arms of the high-water load above, and it is what a
                    // `MigratingFromLegacy` expiry consults before demoting. Setting it
                    // from provenance would let a version-less record suppress that
                    // demotion and hand a legacy-only sender a durable namespace it never
                    // proved under this binary's regime (`ea599560`).
                    //
                    // What it establishes is a *namespace*, never a *number* (#2644). Where
                    // a current-version `replay_max_seq` row has explicitly said this
                    // window's number is a legacy one, both facts are true and they
                    // disagree, and assigning the regime here resolves that disagreement by
                    // silently re-tagging the number — the legacy bound `N` becomes a
                    // durable bound `N`, with no transition and no hold, and the sender's
                    // legitimate durable sequences at or below `N` come back as an ordinary
                    // `Replay detected` that `handlers::signed` scores as an attack.
                    //
                    // The disagreement has an existing, correct resolution: it is the same
                    // shape as a live `(LegacyOrUnproven, ObservedSenderRegime::DurableV1)`
                    // message, and it takes the same path. The legacy number is kept as
                    // legacy evidence for the full horizon — captured old-namespace traffic
                    // stays rejected — and the promotion at the end retires it.
                    if window.sender_regime == SenderRegimeState::LegacyOrUnproven
                        && window.sender_regime_from_current_version
                    {
                        window.sender_regime = SenderRegimeState::TransitionToDurableV1;
                        window.install_hold_conservatively(PeerHold::MigratingSenderRegime {
                            until: quarantine_until,
                        });
                        tracing::warn!(
                            peer = %principal,
                            legacy_max_seq = window.max_seq,
                            hold_secs = self.envelope_validity_horizon().as_secs(),
                            "Durable-v1 provenance meets a current-version high-water tagged \
                             legacy; entering the sender sequence-regime migration rather \
                             than reinterpreting that number as a durable-v1 bound. This is \
                             a local migration, not peer misbehaviour"
                        );
                    } else {
                        // Nothing has placed this window's number in a different namespace:
                        // either no row established a regime at all — the ordinary
                        // aged-out-high-water case provenance exists to serve, and the one
                        // that must NOT pay a migration hold — or the row that did agrees.
                        window.sender_regime = SenderRegimeState::DurableV1;
                    }
                }
                SENDER_REGIME_TRANSITION_TO_DURABLE_V1 => {
                    // Reached only when no readable sibling row proved the migration
                    // finished — the join above resolves `{DurableV1, TransitionToDurableV1}`
                    // to `DurableV1` precisely so this arm cannot fire on a fossil.
                    window.sender_regime = SenderRegimeState::TransitionToDurableV1;
                    window.install_hold_conservatively(PeerHold::MigratingSenderRegime {
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
                    // Includes `SENDER_REGIME_LEGACY_OR_UNPROVEN`, which no writer in this
                    // crate ever puts in this keyspace — see
                    // `joined_sender_regime_provenance`. The join ranks every such value
                    // above both legal ones, so one unreadable-in-meaning alias refuses the
                    // principal here even when a `DurableV1` sibling would have admitted it.
                    window.install_hold_conservatively(PeerHold::UnsupportedSenderRegime {
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
                        if let Some(window) = loaded.get_mut(&principal) {
                            window.finalized.insert(seq, now);
                        }
                    }
                }
            }
        }

        // Commit. Every fallible step is behind us, so this is the single point at which the
        // guard adopts an interpretation of the store — all of it or none of it.
        //
        // A whole-map replacement rather than a merge, because at this point `self.sequences`
        // can only be residue of a *previous failed attempt*, never live state:
        //
        // * `load_persisted_state` gates this function behind the `initialized` latch, so it
        //   runs only while the guard is uninitialized, and a successful load latches it for
        //   good — this function is never re-entered after it returns `Ok`.
        // * The only production path that inserts a window is `check_replay_only`, and it
        //   propagates a load failure (`self.load_persisted_state()?`) *before* it reaches
        //   `self.sequences`. So no live traffic can install a window while a load is failing.
        // * `finalize` uses `get_mut` and errors on an unknown sender, so it cannot create
        //   one; `cleanup` only removes and prunes.
        //
        // Merging into the old map instead would carry a failed attempt's holds and finalized
        // entries into the retry — `entry().or_insert_with()` returns the *stale* window and
        // no arm below clears `hold` — so a repaired store would stay quarantined by an
        // artifact of the failure that repairing it was supposed to erase.
        self.sequences = loaded;

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

                // The hold has expired: nothing written under the old regime can still
                // be fresh, so the *legacy* evidence is now retired. Retiring it is all
                // this arm may do. What is left standing must be exactly what the
                // surviving interpretable evidence establishes on its own — no more,
                // and no less (#2640).
                //
                // Clearing the hold is what makes the migration one-way: the next
                // accept persists a current-version entry, and subsequent restarts take
                // the ordinary #2514 exact-restore path.
                //
                // # `max_seq`/`floor_seq` are deliberately not reset
                //
                // The legacy row's own number never reached this window: the load pass
                // discards it and leaves the window at `SequenceWindow::new`'s 0. So in
                // a legacy-only window there is nothing here to clear, and the
                // unconditional reset this replaces was a no-op — which is why it never
                // showed up. It was destructive in exactly one shape: a principal whose
                // store *also* holds a current-version row, whose floor this window is
                // carrying. Zeroing that let a rolled-back or malicious authenticated
                // sender freshly sign and reuse a durable sequence the current-version
                // row had already rejected. The number discarded there was never this
                // hold's to discard.
                //
                // # The sender axis
                //
                // It returns to unproven, and deliberately does NOT shortcut to durable.
                // It is tempting to argue that this hold already retired everything
                // still-valid, so a durable sender could be established directly. That
                // argument only holds if the sender upgraded *before* the hold began. If
                // it upgraded during the hold, its last legacy envelope was created at
                // some X > hold_start and stays valid until X + skew + max_age, which is
                // past hold_end. The receiver cannot tell those two cases apart, so it
                // must assume the worse one.
                //
                // Cost: the sender-first upgrade order pays two sequential holds. That is
                // the honest price of not being able to date the sender's upgrade.
                //
                // The one exception is not a shortcut and establishes nothing. When a
                // current-version row established this regime, demoting would not be
                // conservative — it would be the same fail-open one hold later. A window
                // holding a durable-v1 floor under a `LegacyOrUnproven` label falls into
                // the `(LegacyOrUnproven, DurableV1)` transition arm below, whose
                // promotion resets `max_seq` and `floor_seq` to 0 by design (#2517: a
                // legacy number is incomparable to durable-v1 ones). So the demote would
                // destroy the very floor this arm must preserve. The bit is set only by
                // the current-version arms of the load pass, so it cannot be reached by a
                // window whose regime rests on legacy evidence.
                tracing::info!(
                    peer = %envelope.from,
                    from_version,
                    to_version = REPLAY_STATE_SEMANTIC_VERSION,
                    retained_floor_seq = window.floor_seq,
                    regime_from_current_version = window.sender_regime_from_current_version,
                    "Replay state migration complete; peer state is now current-semantic"
                );
                window.hold = None;
                if !window.sender_regime_from_current_version {
                    window.sender_regime = SenderRegimeState::LegacyOrUnproven;
                }
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

                // Promote. The namespace the sender used *before* durable-v1 is retired.
                //
                // The number this window holds is dropped when it belongs to that retired
                // namespace, because carrying it over would reimpose exactly the
                // incomparable bound this migration exists to remove — and kept when it does
                // not (#2644). A durable-v1 number is already on the far side of this
                // migration: it was produced by the numbering being promoted *to*, so
                // nothing about retiring the old one makes it stale. Discarding it anyway
                // would let a fossil transition record — provenance outlives the high-water
                // by design, and an unreadable canonical provenance row leaves stale alias
                // rows standing — reset a floor that a current-version durable row
                // established, handing an authenticated sender back every sequence that
                // floor rejects. Read while the borrow is still live; the persist below
                // needs `&self`.
                //
                // This is the same decision as `SequenceWindow::numeric_namespace` exists
                // for, and its only consumer. Deliberately not `max_seq == 0`: a legitimate
                // current-version row carries `max_seq == 0` — it is precisely what the
                // previous promotion wrote — so the number cannot testify about its own
                // namespace.
                //
                // Persisted *before* the message is accepted, and before any durable-v1
                // high-water is written, so a crash here cannot leave a durable-v1
                // number under a transition tag. If the flush fails the promotion does
                // not happen and the hold stands.
                // Ordering matters: the numeric namespace is settled first, and the
                // provenance record — the authority — is written last. A crash between
                // them leaves provenance still saying "transition", so the restart
                // re-runs the hold rather than accepting under a namespace it never
                // finished proving. The safe direction to be interrupted in is the one
                // that repeats work.
                //
                // Both bindings read `window`, which is why they are taken here: the
                // persist below needs `&self`, so the borrow must already have ended.
                let discard_retired_number =
                    window.numeric_namespace != NumericNamespace::DurableV1;
                let retained_seq = if discard_retired_number {
                    0
                } else {
                    window.max_seq
                };
                self.persist_max_seq_durable(&principal, retained_seq, SENDER_REGIME_DURABLE_V1)
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
                if discard_retired_number {
                    window.max_seq = 0;
                    window.floor_seq = 0;
                    window.recent = BloomFilter::new(BLOOM_CAPACITY, 0.001);
                    window.insertion_count = 0;
                }
                // Either way the window's number is now a durable-v1 number: the retained
                // one already was, and the discarded one has been replaced by 0, which is
                // the durable namespace's own starting bound.
                window.numeric_namespace = NumericNamespace::DurableV1;

                tracing::info!(
                    peer = %envelope.from,
                    retained_floor_seq = window.floor_seq,
                    discarded_retired_number = discard_retired_number,
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
                // The number just persisted under a `TRANSITION` tag *is* the legacy
                // high-water (`legacy_max_seq` above), so memory says what the row says
                // (#2644). Assigned rather than left alone: this is the write that makes
                // the retained number legacy evidence, and it must not depend on the
                // window having happened to carry that already.
                window.numeric_namespace = NumericNamespace::LegacyOrUnproven;
                // Assigned directly rather than through
                // `install_hold_conservatively`, and audited as such: this is a
                // runtime transition with a single evidence source, reached only
                // below the hold match, which either took the `None` arm or cleared
                // the hold it found. There is nothing here for a conservative
                // combination to preserve. The rule exists for the load pass, where
                // several persisted rows for one principal genuinely meet.
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

        // The namespace this window is *established in*, never the one this binary
        // implements (#2517).
        //
        // This single expression is the fix. Stamping the current regime here — which is
        // what a receiver that versions only its own semantics does — records a number
        // learned from an unproven sender as durable-v1 state. Nothing downstream can then
        // tell it apart from a real durable high-water, so when that sender upgrades and
        // its durable counter starts low, the receiver rejects it against a bound that
        // never applied and no migration can fire, because nothing looks legacy any more.
        //
        // One fold, two consumers (#2644): the tag persisted beside the number and the
        // namespace the in-memory window records for it are the same statement, and are
        // derived together so they cannot be spelled differently at the two sites.
        let (regime_tag, accepted_namespace) = match established_regime {
            SenderRegimeState::DurableV1 => (SENDER_REGIME_DURABLE_V1, NumericNamespace::DurableV1),
            // `TransitionToDurableV1` cannot reach here — it always returns. It
            // folds to the conservative tag rather than being spelled out, so a
            // future variant cannot silently acquire durable-v1 semantics merely
            // by being added to the enum.
            _ => (
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
                NumericNamespace::LegacyOrUnproven,
            ),
        };

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
            window.numeric_namespace = accepted_namespace;
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
    /// Runs *before* the ordinary load, so where a merge is installed everything downstream
    /// sees one readable row per sender (and per `(sender, sequence)` for finalized rows) and
    /// the #2514 / #2517 state machine is left exactly as it was.
    ///
    /// Two groups are deliberately left unmerged, and for those the load pass still meets
    /// several rows for one principal: a row that does not parse is neither merged nor
    /// deleted, and when the canonical row is the unreadable one its readable alias rows are
    /// left in place too. The conservative rules below therefore do not run for those rows,
    /// and combining them is [`SequenceWindow::install_hold_conservatively`]'s job instead.
    ///
    /// # The merge rule, axis by axis, and why each is the conservative direction
    ///
    /// **`max_seq` and `sender_regime` — merged as one unit, never axis by axis.** A
    /// high-water is only meaningful relative to the namespace that produced it, so the pair
    /// is the thing being merged. Within *one* namespace the maximum is the conservative
    /// choice: the floor rejects `sequence <= floor`, so a larger floor rejects a superset
    /// and cannot admit a replay. What happened before this fix is the opposite: two rows
    /// landed in one window by last-`sled`-key-wins, so a **lower** floor could win (N2-A0
    /// inventory, row #2).
    ///
    /// Across *different* namespaces the maximum is not conservative, it is meaningless.
    /// Taking `max(legacy 10, durable-v1 3)` and labelling the result `DurableV1` states a
    /// pair that never existed, and the load pass then installs a durable floor of 10 with no
    /// hold — so the sender's legitimate durable sequences 4..=10 are rejected as replays.
    /// That rejection is an ordinary `Replay detected`, which
    /// [`crate::handlers::signed`] scores as peer misbehaviour, so laundering the number
    /// bans an honest peer for our own merge. Mixed **recognised** namespaces therefore
    /// resolve to `TransitionToDurableV1`, the state that already exists for exactly this
    /// situation: the number is retained as legacy evidence only, the sender is held for the
    /// envelope validity horizon, and the promotion at the end of that hold — which requires
    /// live durable-v1 attribution, not merely elapsed time — resets `max_seq` and
    /// `floor_seq` to 0. Captured traffic from either namespace stays blocked throughout, and
    /// legitimate durable traffic becomes acceptable at the point the #2517 state machine
    /// already authorises. See [`Self::merge_high_water`].
    ///
    /// An unrecognised tag is preserved as-is and outranks all three recognised states, so the
    /// existing unsupported-regime hold refuses the sender with no deadline and never reads
    /// the number at all.
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
    /// A row whose value does not deserialize is **not** merged and **not** deleted, in all
    /// three keyspaces. Silently dropping it would be the one merge outcome that loses a
    /// bound.
    ///
    /// What the *load* pass then does with that retained row differs by keyspace, and only
    /// two of the three are fail-closed:
    ///
    /// * `replay_max_seq:` and `replay_sender_regime:` — the unreadable value becomes a
    ///   [`PeerHold::Unreadable`] quarantine on the merged sender, the fail-closed answer to
    ///   "we had state here and cannot read it".
    /// * `replay_finalized:` — the unreadable value is **skipped with no hold**, so a
    ///   finalized block whose `(sender, sequence)` has no other readable row is lost
    ///   silently. This arm is byte-identical to base: the behaviour predates #2640 and is
    ///   not a bypass introduced by it. It is survivable only because a finalized entry can
    ///   just add blocking — the floor comes from `replay_max_seq:` — and because
    ///   [`Self::finalize`] and [`Self::is_finalized`] have zero production callers
    ///   workspace-wide today. Making this arm quarantine-consistent with the other two is a
    ///   prerequisite before `finalize()` gains one; see
    ///   `docs/architecture/n2-a0-stored-key-inventory.md`.
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
        let (max_seq, sender_regime) = Self::merge_high_water(a, b);
        MaxSeqEntry {
            max_seq,
            updated_at_ms: a.updated_at_ms.max(b.updated_at_ms),
            semantic_version: Self::most_restrictive_semantic_version(
                a.semantic_version,
                b.semantic_version,
            ),
            sender_regime,
        }
    }

    /// Merge `(max_seq, sender_regime)` as one value, because that is what it is.
    ///
    /// The number and the namespace are stored in the same row on purpose (see
    /// [`MaxSeqEntry::sender_regime`]); merging them independently is the one operation that
    /// can break that pairing, by producing a number from one namespace under another's
    /// label. Three cases, and only the third is a judgement call:
    ///
    /// 1. **Same namespace** — the numbers are comparable, so the maximum is the strongest
    ///    bound that genuinely existed. This is every merge between two spellings of a sender
    ///    that never changed regime, which is the overwhelmingly common case.
    /// 2. **An unrecognised tag is present** — it outranks everything, the load pass refuses
    ///    the sender with no deadline, and no arm ever reads `max_seq`. The number is carried
    ///    through as the maximum purely so the result is deterministic.
    /// 3. **Mixed recognised namespaces** — the numbers are incomparable. Resolved to
    ///    `TransitionToDurableV1`: a bounded hold, then a promotion that discards the number.
    ///
    /// In case 3 the retained number is the maximum across both namespaces, which may be a
    /// durable-v1 number now labelled legacy evidence. That is deliberate and cannot mislead:
    /// under a transition the value is never compared against a durable-v1 sequence, the hold
    /// rejects everything while it stands, and the promotion resets it to 0. Its only effect
    /// is to reject at least as much as the true legacy bound would, for the duration.
    ///
    /// Order-independent and idempotent in every case, as the crash-safety argument on
    /// [`Self::canonicalize_durable_identities`] requires: `max` and `strongest` are both
    /// commutative, and the mixed-namespace answer does not depend on which row is `a`.
    fn merge_high_water(a: &MaxSeqEntry, b: &MaxSeqEntry) -> (u64, u32) {
        let strongest = Self::strongest_sender_regime(a.sender_regime, b.sender_regime);
        let highest = a.max_seq.max(b.max_seq);

        if a.sender_regime == b.sender_regime || !Self::is_recognised_sender_regime(strongest) {
            return (highest, strongest);
        }

        (highest, SENDER_REGIME_TRANSITION_TO_DURABLE_V1)
    }

    /// Whether this binary has an explicit meaning for a persisted sender-regime tag.
    ///
    /// Spelled as an exhaustive `matches!` over the three known constants rather than a range
    /// or a `< N` test, so a regime added later cannot be silently absorbed as recognised
    /// without someone deciding what merging it against the others means.
    fn is_recognised_sender_regime(regime: u32) -> bool {
        matches!(
            regime,
            SENDER_REGIME_LEGACY_OR_UNPROVEN
                | SENDER_REGIME_TRANSITION_TO_DURABLE_V1
                | SENDER_REGIME_DURABLE_V1
        )
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

    /// The join over rows in the **provenance** keyspace: `DurableV1` beats
    /// `TransitionToDurableV1`, and any value this keyspace never legally holds beats both.
    ///
    /// # Why this is not [`Self::strongest_sender_regime`]
    ///
    /// The two agree on every value they share, which is exactly what makes reusing the
    /// other one tempting and wrong. `strongest_sender_regime` joins the `sender_regime`
    /// **field of a `max_seq` row**, where `SENDER_REGIME_LEGACY_OR_UNPROVEN` is a legal and
    /// fully interpretable value: it is the `serde` default for every pre-#2517 row, and the
    /// load pass has an arm for it that establishes a floor. Ranking it lowest there is
    /// correct.
    ///
    /// This keyspace has a different alphabet. `persist_sender_regime` is reached from
    /// exactly two places and writes only `SENDER_REGIME_DURABLE_V1` and
    /// `SENDER_REGIME_TRANSITION_TO_DURABLE_V1`; nothing in this crate ever writes
    /// `SENDER_REGIME_LEGACY_OR_UNPROVEN` here. A `0` in a provenance row is therefore
    /// exactly as uninterpretable as a `7`, and the load pass already refuses both with no
    /// deadline. Ranking it lowest — as `strongest_sender_regime` does — would let a planted
    /// `0` alias be absorbed by a `DurableV1` sibling and admit a sender that the unfixed
    /// load pass refuses forever.
    ///
    /// Correcting `strongest_sender_regime` instead is not available: [`Self::merge_high_water`]
    /// needs LEGACY ranked lowest for the ordinary legacy-plus-durable high-water merge, and
    /// moving it up with the unrecognised tags there would resolve honest mid-migration peers
    /// to an unrecognised regime and refuse them with no deadline.
    ///
    /// # Why `DurableV1` outranks `TransitionToDurableV1`
    ///
    /// Not recency, and not "the stronger hold wins". `TransitionToDurableV1` installs
    /// [`PeerHold::MigratingSenderRegime`], and that hold's expiry with live durable-v1
    /// evidence *promotes*, resetting `max_seq` and `floor_seq` to 0. Adopting it therefore
    /// destroys a replay floor that a sibling row licensed; `DurableV1` has no such path.
    /// The same argument [`Self::strongest_sender_regime`] documents, re-derived here for a
    /// keyspace whose other end differs.
    ///
    /// It is also, for one principal, strictly the later state of one process. Promotion
    /// writes `SENDER_REGIME_DURABLE_V1` to `make_sender_regime_key` — the canonical spelling
    /// — and never touches an alias, so an alias still reading `TransitionToDurableV1`
    /// alongside a `DurableV1` sibling is the fossil of a migration that finished, not
    /// evidence of one still running (#2644).
    ///
    /// Commutative, associative and idempotent, which is what lets the load pass fold the
    /// rows in whatever order `sled` hands them over — an order that is a property of the
    /// spellings an attacker picked, not of anything this node controls.
    fn joined_sender_regime_provenance(a: u32, b: u32) -> u32 {
        let rank = |v: u32| match v {
            SENDER_REGIME_TRANSITION_TO_DURABLE_V1 => 0u8,
            SENDER_REGIME_DURABLE_V1 => 1,
            // Everything else, `SENDER_REGIME_LEGACY_OR_UNPROVEN` included: no meaning in
            // this keyspace, and no amount of elapsed time can give it one.
            _ => 2,
        };
        match rank(a).cmp(&rank(b)) {
            std::cmp::Ordering::Less => b,
            std::cmp::Ordering::Greater => a,
            // Two unrecognised tags resolve to the larger purely so the operator-facing
            // diagnostic is deterministic; both refuse the sender indefinitely either way.
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
        // merge over it would erase the fact that "state existed here and we cannot read it"
        // and replace it with a floor derived only from the spellings we *could* read, which
        // may be lower. The whole group is therefore left as it is: nothing is written and
        // nothing is retired, and the load pass treats the sender exactly as it would have
        // before this migration existed.
        //
        // In the two quarantining keyspaces that means a hold bounded by the envelope validity
        // horizon, after which no pre-restart capture can pass freshness anyway, so leaving the
        // readable floors unmerged costs nothing. In `replay_finalized:` there is no hold — see
        // the keyspace table on `canonicalize_durable_identities` — but nothing is lost by
        // *not* merging there either: the load pass derives each row's principal from the
        // decoded key, so an un-retired alias row still lands on the canonical window.
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

        // Grouped by `(principal, semantic_version)`, not by principal alone (#2640).
        //
        // `semantic_version` selects *how* a persisted high-water is to be interpreted; it is
        // not one more "most restrictive wins" scalar alongside the number. Rows written under
        // different versions therefore carry **independent** security effects, and a merge is
        // only meaningful between rows whose numbers mean the same thing. Within one version
        // group the already-reviewed `merge_max_seq` applies unchanged.
        type VersionGroup = HashMap<(SenderPrincipal, u32), (MaxSeqEntry, Vec<Vec<u8>>)>;
        let mut grouped: VersionGroup = HashMap::new();
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
                // row into a quarantine hold on the merged sender. It contributes no
                // semantic version, because it has none this binary can read.
                unreadable.insert(key);
                continue;
            };
            match grouped.entry((principal, entry.semantic_version)) {
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

        // How many distinct readable semantic versions each principal has rows under.
        //
        // Computed in full before anything is written, so the decision below is a property of
        // the whole scan rather than of the order `sled` happened to hand the rows over.
        let mut versions_per_principal: HashMap<SenderPrincipal, usize> = HashMap::new();
        for (principal, _) in grouped.keys() {
            *versions_per_principal.entry(*principal).or_insert(0) += 1;
        }

        for ((principal, semantic_version), (entry, source_keys)) in grouped {
            // A principal whose readable rows span more than one semantic version is not
            // canonicalized at all (#2640).
            //
            // The canonical key is derived from the `SenderPrincipal` alone, so there is
            // exactly one key available and two independent effects to record. Collapsing them
            // means choosing one version and *deleting* the rows carrying the other — and
            // "most restrictive version" is not a safe tie-break here, because restrictive is
            // not the same as interpretable: the legacy version wins that comparison, so a
            // current-version durable floor of 10 was being replaced by a legacy row whose
            // number the load pass discards, leaving a floor of 0 once the bounded migration
            // hold expired. That is a replay fail-open, not a conservative merge.
            //
            // Leaving every physical row in place loses nothing: the load pass groups by
            // `(principal, semantic_version)` too, merges within each group with the same
            // rule, and joins the groups' effects — floors by maximum, holds by
            // `PeerHold::stronger_of` — onto one window.
            //
            // Cost, and it is a real one: those alias rows are never retired, and `cleanup()`
            // deletes only the canonical key, so it cannot reach them either. The principal
            // pays a bounded `MigratingFromLegacy` hold on every restart for as long as a
            // legacy-version row survives under a spelling nothing overwrites. That is the
            // price of the canonical key being unable to encode both effects faithfully, and
            // it is the right side to err on: a bounded, self-clearing hold against a
            // permanent loss of replay protection.
            //
            // Writes nothing and deletes nothing, so it is trivially idempotent and
            // crash-safe, and a principal whose store later converges on a single version is
            // canonicalized normally on a subsequent start.
            if versions_per_principal.get(&principal).copied().unwrap_or(0) > 1 {
                tracing::warn!(
                    peer = %principal,
                    semantic_version,
                    rows = source_keys.len(),
                    "Declining to collapse replay high-water rows for a sender whose readable \
                     rows span several semantic versions (#2640); each version's effect is \
                     preserved for the load pass to join, because one canonical key cannot \
                     encode both"
                );
                continue;
            }

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
                    *acc = Self::joined_sender_regime_provenance(*acc, regime);
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
    /// Install a hold derived from one piece of persisted evidence, keeping the stronger of it
    /// and whatever is already installed.
    ///
    /// # Why the load pass needs this at all
    ///
    /// One principal can legitimately reach [`ReplayGuard::load_persisted_state`] with several
    /// physical rows, because [`ReplayGuard::canonicalize_durable_identities`] deliberately
    /// declines to merge two groups: a row it cannot parse is neither merged nor deleted, and
    /// when the *canonical* row is the unreadable one the whole group — every readable alias
    /// row included — is left exactly as it was. Both are the right call at the store level;
    /// what they mean here is that the merge's conservative rules
    /// ([`ReplayGuard::most_restrictive_semantic_version`],
    /// [`ReplayGuard::strongest_sender_regime`],
    /// [`ReplayGuard::joined_sender_regime_provenance`]) do not run for those rows, and the
    /// load pass inherits the job of combining them.
    ///
    /// Before this existed each row simply assigned `window.hold`, so the row `sled` happened
    /// to hand over last won. That is a fail-open in one direction: an unreadable alias row
    /// arriving after a readable row whose semantics this binary cannot interpret replaced a
    /// hold with **no deadline** by one bounded at the envelope validity horizon, after which
    /// [`ReplayGuard::check_replay_only`] clears it and the sender is admitted against a floor
    /// of 0 — every sequence it ever sent replayable.
    ///
    /// # The invariant
    ///
    /// Combining persisted evidence for one principal may only preserve or strengthen refusal.
    /// A later physical row can raise the hold, never lower it, and never shorten one.
    /// [`PeerHold::stronger_of`] is commutative, so the answer does not depend on key order —
    /// which is the point, since that order is a property of the spellings an attacker chose.
    fn install_hold_conservatively(&mut self, candidate: PeerHold) {
        self.hold = Some(match self.hold.take() {
            Some(incumbent) => PeerHold::stronger_of(incumbent, candidate),
            None => candidate,
        });
    }

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
            // No row has been read yet, let alone a current-version one.
            sender_regime_from_current_version: false,
            // `max_seq` is 0 and nothing established it, so there is no durable bound to
            // protect from a promotion. The variant that discards is the safe default.
            numeric_namespace: NumericNamespace::LegacyOrUnproven,
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

    /// The production envelope-validity horizon at `max_clock_skew = 300`.
    const HORIZON_SECS: Duration = Duration::from_secs(600);

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

    /// The merge must never take its regime from whichever row happened to supply the number.
    ///
    /// Narrower than
    /// `mixed_regime_alias_rows_hold_for_migration_rather_than_laundering_the_legacy_high_water`,
    /// which pins the whole migration. This pins only the half that a "carry the winning row's
    /// tag along with its number" implementation would get wrong: the higher number here is
    /// tagged unproven, so such an implementation yields `LegacyOrUnproven` with a floor of 10
    /// — the ordinary steady state, in which sequence 11 is simply accepted and the durable
    /// establishment this sender already paid for is silently forgotten.
    ///
    /// This test previously asserted the opposite resolution — `DurableV1` with a floor of 10 —
    /// on the reasoning that the weaker regimes each reach a promotion that would reset the
    /// merged floor to zero. That reasoning had the sign backwards. The two numbers belong to
    /// different namespaces, so there is no floor here worth preserving across them, and the
    /// reset is the *correct* outcome rather than the hazard: it is what lets the sender's
    /// legitimate durable sequences 4..=10 be used again. Adopting either row's tag is wrong;
    /// only the migration is right.
    #[test]
    fn alias_merge_never_adopts_the_regime_of_the_row_that_supplied_the_number() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // The higher number is tagged unproven; the lower one is tagged durable.
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

        assert_ne!(
            stored_max_seq(store.as_ref(), &canonical)
                .unwrap()
                .sender_regime,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
            "the merged row must not inherit the unproven tag from the row that supplied 10"
        );

        // Observable consequence, and the reason the tag matters rather than just the number:
        // had the merge landed on `LegacyOrUnproven`, this is the ordinary steady state and
        // sequence 11 is accepted outright.
        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("a sender with incomparable merged state must not be silently accepted");
        assert!(
            err.downcast_ref::<SenderRegimeTransition>().is_some(),
            "expected the merged state to be a migration hold; got: {err}"
        );
    }

    /// A virtual clock, so a 600s migration hold can be waited out without sleeping.
    struct MergeClock {
        nanos: std::sync::atomic::AtomicU64,
    }

    impl MergeClock {
        fn new() -> Arc<Self> {
            Arc::new(MergeClock {
                nanos: std::sync::atomic::AtomicU64::new(0),
            })
        }
        fn advance(&self, by: Duration) {
            self.nanos
                .fetch_add(by.as_nanos() as u64, std::sync::atomic::Ordering::SeqCst);
        }
    }

    impl MonotonicClock for MergeClock {
        fn elapsed(&self) -> Duration {
            Duration::from_nanos(self.nanos.load(std::sync::atomic::Ordering::SeqCst))
        }
    }

    /// An envelope spelled `from` and genuinely stamped `age` in the past, so the freshness
    /// layer sees it as old rather than merely notionally so.
    fn captured(sender: &KeyPair, from: &Did, sequence: u64, age: Duration) -> SignedEnvelope {
        let mut envelope =
            SignedEnvelope::new(from, sender, sequence, PayloadType::Gossip, b"cap".to_vec())
                .unwrap();
        envelope.timestamp = ReplayGuard::current_time_ms() - age.as_millis() as u64;
        let sig_input = envelope.canonical_encoding();
        envelope.signature = sender.sign(&sig_input).to_vec();
        envelope
    }

    /// Two spellings whose high-waters belong to **different** namespaces must not be merged
    /// into a number/namespace pair that never existed.
    ///
    /// Before this fix the two axes were merged independently — `max(10, 3)` and
    /// `strongest(Legacy, DurableV1)` — producing `(DurableV1, 10)`. The load pass installed
    /// that as a durable floor of 10 with no hold, so the sender's legitimate durable
    /// sequences 4..=10 came back as an ordinary `Replay detected`, which
    /// `handlers::signed` scores as peer misbehaviour. Merging axis-by-axis is safe on each
    /// axis alone and unsafe jointly, which is exactly why it needs its own test.
    #[test]
    fn mixed_regime_alias_rows_hold_for_migration_rather_than_laundering_the_legacy_high_water() {
        // Both role assignments. `sled` scans lexicographically and the base16 alias (`…:f…`)
        // sorts before the base58btc canonical (`…:z…`), so pinning which spelling carries the
        // legacy number would let a first-row-wins or last-row-wins bug pass in one direction.
        for legacy_on_alias in [true, false] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            let alias = alias_of(&canonical);

            let (legacy_spelling, durable_spelling) = if legacy_on_alias {
                (&alias, &canonical)
            } else {
                (&canonical, &alias)
            };

            // The pre-#2640 store this migration exists to consume. Each spelling ran its own
            // #2517 state machine, so one completed its Legacy→DurableV1 promotion — which
            // resets the number to 0, hence a low 3 — while the other never did and still
            // holds the pre-migration legacy high-water of 10.
            seed_max_seq(
                store.as_ref(),
                legacy_spelling,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
            );
            seed_max_seq(
                store.as_ref(),
                durable_spelling,
                3,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            let merged = stored_max_seq(store.as_ref(), &canonical)
                .expect("both spellings must merge onto the canonical key");
            assert_eq!(
                merged.sender_regime, SENDER_REGIME_TRANSITION_TO_DURABLE_V1,
                "incomparable namespaces must resolve to a migration, never to DurableV1 \
                 (direction: legacy_on_alias={legacy_on_alias})"
            );

            // THE REGRESSION: this was `Replay detected (floor: 10)` before the fix.
            let held = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 4),
                    ObservedSenderRegime::DurableV1,
                )
                .expect_err("a sender with incomparable merged state must be held, not admitted");
            assert!(
                held.downcast_ref::<SenderRegimeTransition>().is_some(),
                "the refusal must be the typed migration hold, which `handlers::signed` \
                 exempts from peer scoring — an ordinary replay rejection would ban an \
                 honest peer for our own merge; got: {held}"
            );

            // Captured traffic from the legacy namespace stays blocked throughout the hold.
            assert!(
                guard
                    .check_replay_only(
                        &captured(&sender, &canonical, 7, Duration::from_secs(30)),
                        ObservedSenderRegime::LegacyOrUnproven,
                    )
                    .is_err(),
                "captured legacy traffic must not be admitted during the migration hold"
            );

            clock.advance(HORIZON_SECS + Duration::from_secs(1));

            // Elapsed time alone must not promote: the release point is the policy-authorized
            // one — the horizon *and* live durable-v1 attribution on the connection.
            assert!(
                guard
                    .check_replay_only(
                        &envelope(&sender, &canonical, 4),
                        ObservedSenderRegime::LegacyOrUnproven,
                    )
                    .is_err(),
                "the horizon alone must not release the hold without durable-v1 evidence"
            );

            // With that evidence the namespace is retired, the incomparable number is
            // discarded, and the sender's legitimate sequence 4 is finally usable.
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 4),
                    ObservedSenderRegime::DurableV1,
                )
                .expect("legitimate durable traffic must be usable once the migration completes");

            // And the discarded floor did not reopen a replay window: a genuinely old capture
            // is now refused by freshness instead, which is what bounds the hold in the first
            // place. `check` — not `check_replay_only` — because that is the layer that runs
            // `verify`.
            assert!(
                guard
                    .check(
                        &captured(
                            &sender,
                            &canonical,
                            7,
                            HORIZON_SECS + Duration::from_secs(1)
                        ),
                        ObservedSenderRegime::DurableV1,
                    )
                    .is_err(),
                "a pre-migration capture must still be refused after the promotion"
            );
        }
    }

    /// The realistic shape of the mixed-regime store: the durable spelling also carries a
    /// **provenance** row, which the load pass applies *after* the high-waters and treats as
    /// authoritative for the namespace.
    ///
    /// Worth its own test because provenance is the one thing that could quietly undo the
    /// correction. `SENDER_REGIME_DURABLE_V1` provenance sets the window's regime to
    /// `DurableV1` — if that arm also cleared the hold, the merged sender would go straight
    /// back to a durable floor of 10 and the laundering would be back with an extra step. It
    /// does not: provenance answers "was this key's legacy namespace ever proven retired",
    /// which is a different question from "is the number in this row comparable to a
    /// durable-v1 sequence". Only the second one gates the floor, and only the migration
    /// answers it.
    #[test]
    fn durable_provenance_does_not_release_the_mixed_regime_migration_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // The alias never migrated: legacy high-water, and no provenance row at all — an
        // unpromoted spelling never writes one.
        seed_max_seq(
            store.as_ref(),
            &alias,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        // The canonical spelling completed its promotion, so it has both a reset high-water
        // and the durable provenance record that promotion writes last.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            3,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        store
            .put(
                &spelled_key(SENDER_REGIME_PREFIX, &canonical),
                &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
            )
            .unwrap();

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        // CONTROL: the provenance really did survive the merge and really is durable — so the
        // assertion below is about the hold outranking it, not about it being absent.
        assert_eq!(
            store
                .get(&ReplayGuard::make_sender_regime_key(&pk(&canonical)))
                .unwrap()
                .as_deref(),
            Some(&SENDER_REGIME_DURABLE_V1.to_be_bytes()[..]),
            "CONTROL: durable provenance must be present on the canonical key after the merge"
        );

        // THE PROPERTY: authoritative provenance does not license the incomparable number.
        let held = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("durable provenance must not release the migration hold");
        assert!(
            held.downcast_ref::<SenderRegimeTransition>().is_some(),
            "expected the migration hold to stand over durable provenance; got: {held}"
        );

        // And the migration still completes normally from there.
        clock.advance(HORIZON_SECS + Duration::from_secs(1));
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::DurableV1,
            )
            .expect("the hold must still resolve into a usable durable namespace");
    }

    /// The correction above must not fire when both spellings share a namespace: those numbers
    /// *are* comparable, and turning every alias merge into a hold would be its own regression.
    ///
    /// The control for the test above. `alias_rows_merge_to_the_maximum_floor_in_both_directions`
    /// pins the resulting floor; this pins that no migration hold was introduced to get it.
    #[test]
    fn same_regime_alias_rows_still_merge_to_the_maximum_without_a_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &alias,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            store.as_ref(),
            &canonical,
            3,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        let merged = stored_max_seq(store.as_ref(), &canonical).unwrap();
        assert_eq!(merged.sender_regime, SENDER_REGIME_DURABLE_V1);
        assert_eq!(
            merged.max_seq, 10,
            "one namespace: the maximum is the true bound"
        );

        // Immediately usable — no hold — and the merged floor is enforced.
        assert!(
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1
                )
                .is_err(),
            "sequence 5 is below the merged floor of 10 and must be rejected"
        );
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::DurableV1,
            )
            .expect("a same-namespace merge must not impose a migration hold");
    }

    /// An unrecognised regime tag still outranks every recognised one and still refuses with no
    /// deadline — the mixed-namespace correction must not downgrade it into a bounded hold.
    #[test]
    fn an_unrecognised_regime_mixed_with_a_recognised_one_still_holds_indefinitely() {
        const FUTURE_REGIME: u32 = 9;
        assert!(
            !ReplayGuard::is_recognised_sender_regime(FUTURE_REGIME),
            "CONTROL: the probe tag must be one this binary has no migration for"
        );

        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &alias,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            FUTURE_REGIME,
        );
        seed_max_seq(
            store.as_ref(),
            &canonical,
            3,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            stored_max_seq(store.as_ref(), &canonical)
                .unwrap()
                .sender_regime,
            FUTURE_REGIME,
            "an unrecognised tag must survive the merge rather than be resolved away"
        );

        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("an unrecognised regime must refuse the sender");
        assert!(
            err.downcast_ref::<UnsupportedSenderRegime>().is_some(),
            "expected the no-deadline unsupported-regime hold; got: {err}"
        );

        // Waiting does not make an unknown numbering interpretable.
        clock.advance(HORIZON_SECS * 10);
        assert!(
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 4),
                    ObservedSenderRegime::DurableV1
                )
                .expect_err("still refused")
                .downcast_ref::<UnsupportedSenderRegime>()
                .is_some(),
            "the unsupported-regime hold must have no deadline to reach"
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

    // ------------------------------------------------------------------
    // Cross-semantic-version canonicalization (#2640)
    // ------------------------------------------------------------------
    //
    // `semantic_version` selects *how* a persisted high-water is interpreted, so rows
    // written under different versions carry independent security effects. The rule these
    // tests pin is: merge **within** one interpretable version, join **effects** across
    // versions — and never let a version this binary cannot use destroy a bound one it can.

    /// A third spelling of the same key, so a group can hold three distinct rows.
    ///
    /// `F` is multibase's base16-**upper** code, as `f` is base16-lower. Same bytes, same
    /// principal, a string neither `alias_of` nor the canonical base58btc spelling produces.
    fn upper_alias_of(canonical: &Did) -> Did {
        let hex: String = canonical
            .to_verifying_key()
            .unwrap()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect();
        let alias =
            Did::from_str(&format!("did:icn:F{hex}")).expect("base16-upper spelling parses");
        assert_ne!(
            alias.as_str(),
            canonical.as_str(),
            "CONTROL: the upper alias must be a different string from the canonical spelling"
        );
        assert_ne!(
            alias.as_str(),
            alias_of(canonical).as_str(),
            "CONTROL: the upper alias must differ from the lower-case alias too"
        );
        assert_eq!(
            alias.to_verifying_key().unwrap().as_bytes(),
            canonical.to_verifying_key().unwrap().as_bytes(),
            "CONTROL: the upper alias must decode to the same key"
        );
        alias
    }

    /// Legacy evidence must not be laundered into a current-semantic floor — and the
    /// current-semantic floor must not be destroyed retiring it.
    ///
    /// This replaces `alias_merge_keeps_the_most_restrictive_semantic_version`, whose intent
    /// was right and whose mechanism was the defect. That test asserted the two versions
    /// collapse into **one** row carrying `LEGACY_REPLAY_STATE_SEMANTIC_VERSION`, which is how
    /// the anti-laundering property used to be delivered: pick the most restrictive version,
    /// and the load pass then discards the number. But "most restrictive" is not
    /// "interpretable". Collapsing deleted the current-version row, so the interpretable floor
    /// was gone from the *store*, and once the bounded legacy hold expired the sender resumed
    /// against a floor of 0 — every sequence it had ever sent replayable.
    ///
    /// The property is therefore restated over both halves at once, with the numbers chosen so
    /// a single answer cannot satisfy them by accident: the legacy row carries the **higher**
    /// number, so laundering it would raise the floor to 10, while the surviving current row
    /// says 3. A floor of 10 fails the second assertion; a floor of 0 fails the first.
    #[test]
    fn mixed_version_rows_neither_launder_the_legacy_number_nor_lose_the_current_floor() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // Current-semantic evidence: a real, interpretable bound of 3.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            3,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        // Pre-semantic-versioning evidence under another spelling, carrying a *higher*
        // number that this binary cannot interpret.
        seed_max_seq(
            store.as_ref(),
            &alias,
            10,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        // CONTROL: the two versions must genuinely differ, or nothing here is being tested.
        assert_ne!(
            REPLAY_STATE_SEMANTIC_VERSION, LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            "CONTROL: the seeded versions must differ"
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        // The rows are not destructively collapsed: one canonical key cannot encode both
        // effects, so both physical rows survive for the load pass to interpret separately.
        assert_eq!(
            stored_max_seq(store.as_ref(), &canonical).map(|e| e.semantic_version),
            Some(REPLAY_STATE_SEMANTIC_VERSION),
            "the current-version row must survive canonicalization intact"
        );
        assert_eq!(
            stored_max_seq(store.as_ref(), &alias).map(|e| e.semantic_version),
            Some(LEGACY_REPLAY_STATE_SEMANTIC_VERSION),
            "the legacy-version row must be left in place rather than merged away"
        );

        // The floor is the current row's, never the legacy row's: the legacy number is not
        // laundered into current semantics, which is the original test's whole point.
        assert_eq!(
            guard.get_max_seq(&canonical),
            Some(3),
            "the floor must come from the interpretable current-version row, not the legacy one"
        );

        // Legacy evidence still causes its migration hold, for its full horizon.
        let held = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("a legacy-version row must still hold the sender while it can be fresh");
        assert!(
            held.downcast_ref::<ReplayStateLegacy>().is_some(),
            "expected the legacy migration hold; got: {held}"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // After the hold, the current-version floor is still there.
        let replayed = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 3),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("the current-version floor must survive the legacy migration");
        assert!(
            replayed.downcast_ref::<ReplayStateLegacy>().is_none(),
            "the hold must have been released, not merely restated: {replayed}"
        );
        assert!(
            replayed.to_string().contains("Replay detected"),
            "expected an ordinary replay rejection against the retained floor; got: {replayed}"
        );

        // ...and it is only the *current* row's floor. Sequence 4 is above 3 and below the
        // legacy row's 10, so it is legitimate traffic that must not be blocked. This is the
        // over-correction control: taking the maximum across semantic versions would have
        // adopted 10 and rejected this.
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect(
                "a sequence above the current-version floor must not be blocked by a \
                     legacy number this binary never interpreted",
            );
    }

    /// A current-version durable floor survives a legacy sibling, whichever spelling holds it.
    ///
    /// Both role assignments are run because `sled` scans lexicographically and the base16
    /// alias (`…:f…`) sorts before the base58btc canonical (`…:z…`): a rule that happens to
    /// keep "the first row" or "the last row" would pass one assignment and fail the other.
    /// The measured pre-fix behaviour was identical in both, so the defect was never a lexical
    /// accident — and neither is the fix.
    #[test]
    fn a_current_durable_floor_survives_a_legacy_sibling_in_both_spelling_positions() {
        for current_on_canonical in [true, false] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            let alias = alias_of(&canonical);
            let (current_did, legacy_did) = if current_on_canonical {
                (canonical.clone(), alias.clone())
            } else {
                (alias.clone(), canonical.clone())
            };

            seed_max_seq(
                store.as_ref(),
                &current_did,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            seed_max_seq(
                store.as_ref(),
                &legacy_did,
                2,
                LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            assert!(
                stored_max_seq(store.as_ref(), &current_did).is_some()
                    && stored_max_seq(store.as_ref(), &legacy_did).is_some(),
                "current_on_canonical={current_on_canonical}: neither row may be retired"
            );
            assert_eq!(
                guard.get_max_seq(&canonical),
                Some(10),
                "current_on_canonical={current_on_canonical}: the durable floor must load"
            );

            let held = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )
                .expect_err("the legacy sibling must hold the sender initially");
            assert!(
                held.downcast_ref::<ReplayStateLegacy>().is_some(),
                "current_on_canonical={current_on_canonical}: expected the legacy hold; \
                 got: {held}"
            );

            clock.advance(HORIZON_SECS + Duration::from_secs(1));

            // THE PROPERTY. Sequence 5 is at or below the durable floor of 10 that the
            // current-version row established, so it must stay rejected — permanently, not
            // for a horizon. Before this fix it was accepted here.
            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )
                .expect_err(
                    "a durable sequence below a current-version floor must never become \
                     replayable by retiring an unrelated legacy row",
                );
            assert!(
                err.to_string().contains("Replay detected"),
                "current_on_canonical={current_on_canonical}: expected a replay rejection \
                 against the retained floor, not another hold; got: {err}"
            );

            // The regime the current-version row established survives too. A demotion to
            // `LegacyOrUnproven` here would not be conservative: it routes this window into
            // the `(LegacyOrUnproven, DurableV1)` transition, whose promotion resets
            // `max_seq`/`floor_seq` to 0 — the same fail-open, one hold later.
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "current_on_canonical={current_on_canonical}: a durable sender whose \
                         regime a current-version row established must not be made to re-earn \
                         a transition hold: {e}"
                    )
                });
        }
    }

    /// The fix must not over-correct into a maximum across semantic versions.
    ///
    /// The durable variant of the numeric half of
    /// `mixed_version_rows_neither_launder_the_legacy_number_nor_lose_the_current_floor`: an
    /// unconditional `max` across versions would adopt the legacy 10 as a durable-v1 floor and
    /// reject the sender's legitimate sequence 4 as a replay, which `handlers::signed` scores
    /// as peer misbehaviour. Conservative on one axis, wrong on the other.
    #[test]
    fn a_legacy_high_water_never_raises_a_current_version_floor() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        let current_floor: u64 = 3;
        let legacy_high_water: u64 = 10;

        seed_max_seq(
            store.as_ref(),
            &canonical,
            current_floor,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            legacy_high_water,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        // CONTROL: read back from the store, because "did not take the maximum" is
        // indistinguishable from "took the maximum" unless the legacy row genuinely carries
        // the larger number at the moment the load pass runs.
        assert!(
            stored_max_seq(store.as_ref(), &alias).unwrap().max_seq
                > stored_max_seq(store.as_ref(), &canonical).unwrap().max_seq,
            "CONTROL: the legacy row must carry the higher number"
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();
        assert_eq!(
            guard.get_max_seq(&canonical),
            Some(current_floor),
            "the floor must be the current-version row's, not the larger legacy number"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // At the floor: still rejected.
        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 3),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("the current-version floor of 3 must still reject its own high-water");
        assert!(
            err.to_string().contains("Replay detected"),
            "expected a replay rejection at the floor; got: {err}"
        );

        // Above it: accepted. This is the assertion an over-correction fails.
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::DurableV1,
            )
            .expect(
                "sequence 4 is above the only interpretable floor and must not be rejected \
                 merely because a legacy row said 10",
            );
    }

    /// Legacy-only state still converges exactly as #2517 specified.
    ///
    /// The case the old unconditional `max_seq = 0; floor_seq = 0;` reset was written for —
    /// and in which it was always a no-op, because the load pass never installs a legacy
    /// row's number in the first place. Removing it must therefore change nothing here.
    #[test]
    fn a_legacy_only_window_still_converges_through_its_migration_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // Seeded on the alias so the ordinary single-version canonicalization is exercised
        // too: one version means the collapse still happens, and the row is re-keyed.
        seed_max_seq(
            store.as_ref(),
            &alias,
            10,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            stored_max_seq(store.as_ref(), &alias).is_none()
                && stored_max_seq(store.as_ref(), &canonical).is_some(),
            "a single-version principal must still be canonicalized onto its canonical key"
        );
        assert_eq!(
            guard.get_max_seq(&canonical),
            Some(0),
            "a legacy row's number is discarded, not installed as a floor"
        );

        let held = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect_err("legacy-only state must hold for its horizon");
        assert!(
            held.downcast_ref::<ReplayStateLegacy>().is_some(),
            "expected the legacy migration hold; got: {held}"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // Converged: the migration completes and live traffic rebuilds current-semantic state.
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect("a legacy-only migration must converge and resume service");
    }

    /// Retiring legacy state must not manufacture a durable sender out of nothing.
    ///
    /// The provenance keyspace can carry `DurableV1` for a principal whose only `max_seq` row
    /// is legacy-version — a mixed-binary store. Preserving `DurableV1` through the expiry
    /// there would be exactly the shortcut the migration exists to prevent: the receiver
    /// cannot date the sender's upgrade, so a durable claim must re-earn its transition hold.
    /// The provenance bit is set only by the *current-version* arms of the load pass, so it
    /// is `false` here and the demotion still happens.
    #[test]
    fn legacy_only_state_with_durable_provenance_still_demotes_at_expiry() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            4,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        store
            .put(
                &spelled_key(SENDER_REGIME_PREFIX, &canonical),
                &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
            )
            .unwrap();

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        // CONTROL: the durable provenance really is present, so the assertion below is about
        // the expiry demoting it and not about it never having been read.
        assert_eq!(
            store
                .get(&ReplayGuard::make_sender_regime_key(&pk(&canonical)))
                .unwrap()
                .as_deref(),
            Some(&SENDER_REGIME_DURABLE_V1.to_be_bytes()[..]),
            "CONTROL: durable provenance must be present"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // THE PROPERTY: no current-version row established this regime, so the sender is
        // demoted and a durable claim pays its transition hold rather than being admitted.
        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 9),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err(
                "a durable regime not established by current-version evidence must not \
                 survive the legacy migration",
            );
        assert!(
            err.downcast_ref::<SenderRegimeTransition>().is_some(),
            "expected the sender-regime transition hold after demotion; got: {err}"
        );
    }

    /// Current-version-only state restores exactly, unchanged by any of this.
    #[test]
    fn current_version_only_state_restores_exactly() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &alias,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            stored_max_seq(store.as_ref(), &alias).is_none()
                && stored_max_seq(store.as_ref(), &canonical).is_some(),
            "a single-version principal is still re-keyed onto its canonical spelling"
        );
        assert_eq!(
            guard.get_max_seq(&canonical),
            Some(10),
            "the ordinary #2514 exact restore must be untouched"
        );

        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("a sequence below the restored floor is a replay");
        assert!(
            err.to_string().contains("Replay detected"),
            "expected an ordinary replay rejection with no hold involved; got: {err}"
        );
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::DurableV1,
            )
            .expect("the sender's next sequence must be accepted immediately, with no hold");
    }

    /// A current-version row cannot make an unsupported sibling usable.
    ///
    /// The version this binary has no migration for still fails closed with **no deadline**,
    /// and leaving the rows distinct must not have given it one. Elapsed time cannot make an
    /// unknown numbering interpretable, and the presence of a row this binary *can* read says
    /// nothing about the one it cannot.
    #[test]
    fn an_unsupported_version_sibling_holds_a_current_row_indefinitely() {
        const UNSUPPORTED: u32 = 99;
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // CONTROL: the tag must be one no arm of the load pass recognises.
        assert_ne!(UNSUPPORTED, REPLAY_STATE_SEMANTIC_VERSION);
        assert_ne!(UNSUPPORTED, LEGACY_REPLAY_STATE_SEMANTIC_VERSION);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            1,
            UNSUPPORTED,
            SENDER_REGIME_DURABLE_V1,
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            stored_max_seq(store.as_ref(), &alias).map(|e| e.semantic_version) == Some(UNSUPPORTED),
            "evidence whose semantics are unknown must never be rewritten or retired"
        );

        // Ten horizons is not a deadline; there is no deadline.
        for round in 0..10 {
            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )
                .expect_err("an unsupported semantic version must never expire into acceptance");
            assert!(
                err.downcast_ref::<ReplayStateUnsupportedVersion>()
                    .is_some(),
                "round {round}: expected the indefinite unsupported-version hold; got: {err}"
            );
            clock.advance(HORIZON_SECS + Duration::from_secs(1));
        }
    }

    /// Keeping versions apart must not reopen the `5be3fdf0` namespace coupling.
    ///
    /// Two spellings *inside* the current-version group carry different sender regimes, so
    /// `merge_max_seq` must still pair the number with the namespace that produced it and
    /// resolve to a transition rather than laundering a legacy 3 into a durable floor. A
    /// legacy-version third row sits alongside them. Two properties at once: the within-group
    /// merge is unchanged by the version grouping, and its `MigratingSenderRegime` hold —
    /// which is bounded by live durable evidence as well as by time — still outranks the
    /// legacy hold from the other version group.
    #[test]
    fn mixed_regimes_inside_a_version_group_still_couple_number_to_namespace() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);
        let upper = upper_alias_of(&canonical);

        // Same version, different namespaces: the `5be3fdf0` case.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            3,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        // A different version entirely, which must not be folded into either of them.
        seed_max_seq(
            store.as_ref(),
            &upper,
            7,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        // Every row survives: the principal spans two versions, so nothing is collapsed.
        for did in [&canonical, &alias, &upper] {
            assert!(
                stored_max_seq(store.as_ref(), did).is_some(),
                "no row may be retired for a principal spanning several versions"
            );
        }

        // The within-group mixed-regime merge still resolves to a transition, and its hold
        // outranks the legacy one, so that is the refusal the sender sees.
        let held = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("mixed regimes inside the current version must still hold");
        assert!(
            held.downcast_ref::<SenderRegimeTransition>().is_some(),
            "expected the sender-regime transition hold to outrank the legacy one; got: {held}"
        );

        // And the number was never laundered across namespaces: after the transition resolves
        // the legacy high-water is discarded rather than reimposed as a durable-v1 floor.
        clock.advance(HORIZON_SECS + Duration::from_secs(1));
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 4),
                ObservedSenderRegime::DurableV1,
            )
            .expect("the promotion must retire the incomparable number rather than reimpose it");
    }

    /// The strongest hold still wins when versions are left distinct (#2640 over `067bb7e6`).
    ///
    /// Three physical rows for one principal now genuinely coexist, so the conservative hold
    /// composition matters more than it did, not less. `Unreadable` outranks
    /// `MigratingFromLegacy` because its expiry leaves the window's floor standing, and that
    /// ordering must survive the versions being kept apart. After it clears, the
    /// current-version floor is still there — nothing about carrying three rows loosened it.
    #[test]
    fn an_unreadable_sibling_still_outranks_the_legacy_hold_across_versions() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);
        let upper = upper_alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            2,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        store
            .put(&spelled_key(MAX_SEQ_PREFIX, &upper), b"{ not json")
            .unwrap();

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        let held = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("an unreadable row must quarantine the sender");
        assert!(
            held.downcast_ref::<ReplayStateUnreadable>().is_some(),
            "the stronger hold must win over the legacy migration hold; got: {held}"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("the current-version floor must outlive every hold above it");
        assert!(
            err.to_string().contains("Replay detected"),
            "expected the retained floor to reject, not another hold; got: {err}"
        );
    }

    /// Declining to collapse writes nothing and deletes nothing.
    ///
    /// The crash-safety and idempotence argument, measured rather than asserted in prose: a
    /// mixed-version principal drives zero mutations, so there is no window in which an
    /// interruption can leave a sender with less state than it had, and a second load reaches
    /// the identical store. `install_canonical_row`'s write-then-flush-then-retire ordering is
    /// simply never entered.
    #[test]
    fn declining_a_mixed_version_collapse_mutates_nothing_and_is_idempotent() {
        let store = Arc::new(CountingStore::default());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            2,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        store.reset_counters();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();
        assert_eq!(
            store.counts(),
            (0, 0),
            "a declined collapse must perform no puts and no deletes; ops: {:?}",
            store.op_log()
        );

        // CONTROL: the same store with the legacy row's version corrected to current *does*
        // collapse, so the assertion above is about the decision and not about the harness
        // failing to count anything.
        let control = Arc::new(CountingStore::default());
        seed_max_seq(
            control.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            control.as_ref(),
            &alias,
            2,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        control.reset_counters();
        let mut control_guard = ReplayGuard::new_persistent(300, 3600, control.clone());
        control_guard.load_persisted_state().unwrap();
        let (puts, deletes) = control.counts();
        assert!(
            puts > 0 && deletes > 0,
            "CONTROL: a single-version principal must still be collapsed (puts={puts}, \
             deletes={deletes})"
        );
    }

    /// The preserved floor is durable, not an artefact of one process's memory.
    ///
    /// A second guard built over the same store after the migration has run must reach the
    /// same answer. This is what makes the fix a property of the *store* rather than of the
    /// window that happened to survive a hold.
    #[test]
    fn the_preserved_current_floor_survives_a_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_max_seq(
            store.as_ref(),
            &alias,
            2,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        let first_clock = MergeClock::new();
        let mut first =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(first_clock.clone());
        first.load_persisted_state().unwrap();
        first_clock.advance(HORIZON_SECS + Duration::from_secs(1));
        drop(first);

        // Restart over the same store.
        let second_clock = MergeClock::new();
        let mut second =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(second_clock.clone());
        second.load_persisted_state().unwrap();
        assert_eq!(
            second.get_max_seq(&canonical),
            Some(10),
            "the durable floor must be reconstructed identically after a restart"
        );

        second_clock.advance(HORIZON_SECS + Duration::from_secs(1));
        let err = second
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("the floor must reject the same sequence after a restart");
        assert!(
            err.to_string().contains("Replay detected"),
            "expected the restored floor to reject; got: {err}"
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

    /// A store whose sender-regime scan can be made to fail on a chosen call.
    ///
    /// Failing *every* scan (as `CountingStore { fail_scan: true }` does) aborts the load at
    /// its very first read, before any window exists — which is precisely the case that cannot
    /// exhibit a partial-state bug. The provenance scan is the interesting one: the load calls
    /// it once during canonicalization and again during the load proper, so failing only the
    /// second lands the error *after* the max_seq loop has already built windows.
    #[derive(Default)]
    struct PartialLoadStore {
        data: std::sync::Mutex<std::collections::BTreeMap<Vec<u8>, Vec<u8>>>,
        regime_scans: std::sync::atomic::AtomicU32,
        /// 1-based index of the `SENDER_REGIME_PREFIX` scan to fail; `None` never fails.
        fail_regime_scan_number: std::sync::Mutex<Option<u32>>,
    }

    impl PartialLoadStore {
        fn failing_on_regime_scan(n: u32) -> Arc<Self> {
            let store = Arc::new(PartialLoadStore::default());
            *store.fail_regime_scan_number.lock().unwrap() = Some(n);
            store
        }
        fn repair(&self) {
            *self.fail_regime_scan_number.lock().unwrap() = None;
        }
    }

    impl Store for PartialLoadStore {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(self.data.lock().unwrap().get(key).cloned())
        }
        fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
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
            if prefix == SENDER_REGIME_PREFIX {
                let n = self.regime_scans.fetch_add(1, Ordering::SeqCst) + 1;
                if *self.fail_regime_scan_number.lock().unwrap() == Some(n) {
                    anyhow::bail!("simulated unreadable sender-regime keyspace");
                }
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

    /// A load that fails partway through must install nothing, so the retry after a repair
    /// derives its replay state from the repaired store alone.
    ///
    /// `a_failed_load_leaves_the_guard_disarmed_for_nobody` covers the *latch*: a failure must
    /// not mark the guard initialized. This covers the other half, which the latch does not
    /// give on its own — the windows themselves. `load_persisted_state_inner` populates
    /// `sequences` from the max_seq keyspace and only afterwards scans provenance and
    /// finalized, either of which can fail. Merging a retry into whatever the failed attempt
    /// left behind carries its holds forward: `entry().or_insert_with()` returns the stale
    /// window and no arm below clears `hold`, so a peer quarantined by a corrupt row stays
    /// quarantined after the row is fixed.
    #[test]
    fn a_failed_load_installs_no_partial_state_for_the_retry_to_inherit() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        // CONTROL: with the provenance scan healthy, this corrupt row really does install a
        // quarantine hold. Without this, the rollback assertion below could pass simply
        // because phase one never produced any state to roll back.
        {
            let store = Arc::new(PartialLoadStore::default());
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                .unwrap();
            let mut control = ReplayGuard::new_persistent(300, 3600, store.clone());
            control.load_persisted_state().unwrap();
            assert!(
                control
                    .sequences
                    .get(&pk(&canonical))
                    .is_some_and(|w| w.hold.is_some()),
                "CONTROL: the corrupt max_seq row must quarantine the peer, or this test \
                 proves nothing about rolling that quarantine back"
            );
        }

        // Fail the *second* provenance scan: canonicalization takes the first, so the error
        // lands after the max_seq loop has already installed the hold proven above.
        let store = PartialLoadStore::failing_on_regime_scan(2);
        store
            .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
            .unwrap();

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        assert!(
            guard.load_persisted_state().is_err(),
            "CONTROL: the provenance scan must actually fail"
        );
        assert!(
            !guard.is_initialized(),
            "a failed load must not latch (see the sibling test)"
        );

        // THE REGRESSION. Before this fix the window built by the max_seq loop survived here.
        assert!(
            guard.sequences.is_empty(),
            "a failed load must install no windows at all; found {} left behind",
            guard.sequences.len()
        );

        // Repair the store completely: a readable durable row, and a working provenance scan.
        store.repair();
        seed_max_seq(
            store.as_ref(),
            &canonical,
            5,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );

        assert_eq!(
            guard.load_persisted_state().unwrap(),
            1,
            "the retry must load the repaired row"
        );

        let window = guard
            .sequences
            .get(&pk(&canonical))
            .expect("the repaired row must produce a window");
        assert!(
            window.hold.is_none(),
            "the quarantine from the failed attempt must not outlive it — the store that \
             justified it no longer exists"
        );
        assert_eq!(
            window.floor_seq, 5,
            "and the floor must come from the repaired row"
        );

        // The observable consequence: the peer is usable again.
        guard
            .check_replay_only(
                &envelope(&sender, &canonical, 6),
                ObservedSenderRegime::DurableV1,
            )
            .expect("a repaired store must admit the peer's next durable sequence");
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

    // ---------------------------------------------------------------------
    // #2644 review — conservative hold composition across alias rows
    // ---------------------------------------------------------------------

    /// The base256-emoji spelling of the same key, which sorts **after** the canonical
    /// base58btc one. [`alias_of`] sorts before it.
    ///
    /// Both are needed because the property under test is exactly that the answer does not
    /// depend on which physical row the scan hands over last — and that order is a property
    /// of the spelling an attacker picked, not of anything this node controls.
    fn late_sorting_alias_of(canonical: &Did) -> Did {
        let key_bytes = canonical.to_verifying_key().unwrap();
        let alias = Did::from_str(&format!(
            "did:icn:{}",
            multibase::encode(multibase::Base::Base256Emoji, key_bytes.as_bytes())
        ))
        .expect("the base256-emoji spelling of a key parses under current policy");
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

    /// One alias that `sled` scans *before* the canonical row and one it scans *after*, with
    /// the ordering asserted rather than assumed.
    ///
    /// Every scenario below runs under both, so no fix can be accidentally pinned to one key
    /// spelling: with last-write-wins, exactly one of the two positions happens to produce the
    /// right answer, and a test that ran only that one would pass on the defect.
    fn both_scan_positions(canonical: &Did) -> [(&'static str, Did); 2] {
        let early = alias_of(canonical);
        let late = late_sorting_alias_of(canonical);
        assert!(
            spelled_key(MAX_SEQ_PREFIX, &early) < spelled_key(MAX_SEQ_PREFIX, canonical),
            "CONTROL: the base16 alias must sort BEFORE the canonical row"
        );
        assert!(
            spelled_key(MAX_SEQ_PREFIX, canonical) < spelled_key(MAX_SEQ_PREFIX, &late),
            "CONTROL: the base256-emoji alias must sort AFTER the canonical row"
        );
        [
            ("base16 alias, scanned first", early),
            ("base256-emoji alias, scanned last", late),
        ]
    }

    /// One of each `PeerHold`, for the ordering properties below.
    fn every_hold() -> [PeerHold; 5] {
        [
            PeerHold::MigratingFromLegacy {
                until: Duration::from_secs(600),
                from_version: LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            },
            PeerHold::Unreadable {
                until: Duration::from_secs(600),
            },
            PeerHold::MigratingSenderRegime {
                until: Duration::from_secs(600),
            },
            PeerHold::UnsupportedSenderRegime { found_regime: 7 },
            PeerHold::UnsupportedVersion { found_version: 9 },
        ]
    }

    /// The control the `PeerHold::rank` doc claims: equal rank means one variant.
    ///
    /// `stronger_of`'s equal-rank arm falls back to keeping the incumbent for pairs it does
    /// not name. That fallback is safe but silent, so the thing that must not rot is the
    /// premise that it is unreachable.
    #[test]
    fn hold_ranks_are_distinct_so_equal_rank_means_one_variant() {
        let ranks: Vec<u8> = every_hold().iter().map(PeerHold::rank).collect();
        let mut sorted = ranks.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ranks.len(),
            "every PeerHold variant must have its own rank; got {ranks:?}"
        );
    }

    /// The invariant itself, over every pair: combining may only preserve or strengthen, and
    /// the answer may not depend on which row arrived first.
    #[test]
    fn combining_holds_never_weakens_and_never_depends_on_order() {
        for a in every_hold() {
            for b in every_hold() {
                let forward = PeerHold::stronger_of(a, b);
                let reverse = PeerHold::stronger_of(b, a);
                assert_eq!(
                    forward.rank(),
                    reverse.rank(),
                    "combining {a:?} and {b:?} must not depend on their order"
                );
                assert!(
                    forward.rank() >= a.rank() && forward.rank() >= b.rank(),
                    "combining {a:?} and {b:?} produced {forward:?}, which refuses less than \
                     one of its inputs"
                );
                // Idempotent, so re-applying the same evidence cannot drift.
                assert_eq!(
                    PeerHold::stronger_of(forward, forward).rank(),
                    forward.rank(),
                    "combining {forward:?} with itself must be a no-op"
                );
            }
        }
    }

    /// Two bounded holds of the same kind keep the **later** deadline, so no combination of
    /// rows can shorten a quarantine.
    #[test]
    fn two_bounded_holds_of_one_kind_keep_the_later_deadline() {
        let early = Duration::from_secs(100);
        let late = Duration::from_secs(900);

        for (a, b) in [
            (
                PeerHold::Unreadable { until: early },
                PeerHold::Unreadable { until: late },
            ),
            (
                PeerHold::MigratingSenderRegime { until: early },
                PeerHold::MigratingSenderRegime { until: late },
            ),
        ] {
            for (x, y) in [(a, b), (b, a)] {
                let combined = PeerHold::stronger_of(x, y);
                let until = match combined {
                    PeerHold::Unreadable { until } | PeerHold::MigratingSenderRegime { until } => {
                        until
                    }
                    other => panic!("expected the same kind back, got {other:?}"),
                };
                assert_eq!(until, late, "combining {x:?} with {y:?} shortened the hold");
            }
        }
    }

    /// A: an unreadable alias row must not downgrade an indefinite unsupported-version hold.
    ///
    /// The reported defect (#2644 review). Canonicalization leaves an unreadable row in place
    /// deliberately — it is the evidence that quarantines the sender — so a principal really
    /// can arrive at the load pass with a readable row this binary cannot interpret *and* an
    /// unreadable alias row. Assigning `window.hold` per row let the later one win, replacing
    /// a hold with no deadline by one bounded at the freshness horizon. After that horizon
    /// `check_replay_only` clears the bounded hold, and — because the unsupported-version arm
    /// never establishes a floor — the sender is admitted against `floor_seq = 0`, which makes
    /// every sequence it ever sent replayable.
    #[test]
    fn an_unreadable_alias_row_cannot_downgrade_an_unsupported_version_hold() {
        const UNSUPPORTED_VERSION: u32 = 9_999;
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        assert_ne!(
            UNSUPPORTED_VERSION, REPLAY_STATE_SEMANTIC_VERSION,
            "CONTROL: the seeded version must be one this binary cannot interpret"
        );
        assert_ne!(
            UNSUPPORTED_VERSION, LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            "CONTROL: and one it has no migration for"
        );

        for (position, alias) in both_scan_positions(&canonical) {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                UNSUPPORTED_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &alias), b"{not json")
                .unwrap();

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // CONTROL: both rows really did survive canonicalization, so two evidence
            // sources genuinely meet in one window. Without this the test could pass by
            // the alias row having been merged away, proving nothing about composition.
            assert!(
                store
                    .get(&spelled_key(MAX_SEQ_PREFIX, &alias))
                    .unwrap()
                    .is_some(),
                "{position}: CONTROL: the unreadable alias row must still be present"
            );
            assert!(
                store
                    .get(&spelled_key(MAX_SEQ_PREFIX, &canonical))
                    .unwrap()
                    .is_some(),
                "{position}: CONTROL: the readable canonical row must still be present"
            );

            // Past the freshness horizon, where only a hold with no deadline still refuses.
            clock.advance(HORIZON_SECS + Duration::from_secs(1));

            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .expect_err(
                    "persisted state whose semantics this binary cannot interpret must never \
                     expire into acceptance",
                );
            assert!(
                err.downcast_ref::<ReplayStateUnsupportedVersion>()
                    .is_some(),
                "{position}: expected the indefinite unsupported-version refusal to survive \
                 the unreadable alias row; got: {err}"
            );
        }
    }

    /// B: the same, on the sender-namespace axis — no bounded hold may replace indefinite
    /// refusal of an unrecognised sender regime.
    #[test]
    fn an_unreadable_alias_row_cannot_downgrade_an_unsupported_sender_regime_hold() {
        const UNSUPPORTED_REGIME: u32 = 7_777;
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        assert!(
            !ReplayGuard::is_recognised_sender_regime(UNSUPPORTED_REGIME),
            "CONTROL: the seeded regime must be one this binary has no migration for"
        );

        for (position, alias) in both_scan_positions(&canonical) {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                UNSUPPORTED_REGIME,
            );
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &alias), b"{not json")
                .unwrap();

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            clock.advance(HORIZON_SECS + Duration::from_secs(1));

            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .expect_err("an unrecognised sender regime must never expire into acceptance");
            assert!(
                err.downcast_ref::<UnsupportedSenderRegime>().is_some(),
                "{position}: expected the indefinite sender-regime refusal to survive the \
                 unreadable alias row; got: {err}"
            );
        }
    }

    /// C: two bounded holds meet — keep the one whose expiry preserves the replay floor.
    ///
    /// `Unreadable` and `MigratingFromLegacy` are incomparable in the abstract: the first
    /// clears leaving the window's floor standing, the second clears by resetting `max_seq`
    /// and `floor_seq` to 0 and demoting the regime to unproven. The tie is broken toward
    /// `Unreadable` because its advantage is permanent — a retained floor rejects a superset
    /// forever — while `MigratingFromLegacy`'s advantage is only that a durable claim must
    /// re-earn a transition hold, which delays an acceptance that then lands on a floor of 0
    /// anyway.
    ///
    /// Reached through the other place canonicalization declines to merge: when the
    /// *canonical* row is the unreadable one, its readable alias rows are left unmerged too,
    /// so their per-row holds meet here rather than in `merge_max_seq`.
    #[test]
    fn two_bounded_alias_holds_resolve_to_the_floor_preserving_one() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        for (position, alias) in both_scan_positions(&canonical) {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            // The canonical row is the unreadable one, so the whole group is left unmerged.
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                .unwrap();
            seed_max_seq(
                store.as_ref(),
                &alias,
                3,
                LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // CONTROL: the readable alias row survived, so a legacy hold really is one of the
            // two pieces of evidence being combined.
            assert!(
                stored_max_seq(store.as_ref(), &alias).is_some(),
                "{position}: CONTROL: the readable alias row must be left unmerged"
            );

            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .expect_err("a sender with unreadable state must be held");
            assert!(
                err.downcast_ref::<ReplayStateUnreadable>().is_some(),
                "{position}: the unreadable hold must outrank the legacy one, whichever row \
                 the scan hands over last; got: {err}"
            );

            // And it is still a *bounded* hold: the composition must not have invented an
            // indefinite refusal out of two temporary ones.
            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "{position}: two bounded holds must not compose into a permanent one: {e}"
                    )
                });
        }
    }

    /// D: a corrupt alias row must not permanently brick a sender that is only mid-migration.
    ///
    /// The over-correction control for the ranking. `MigratingSenderRegime` is ranked *above*
    /// `Unreadable` precisely so this case survives: a window loaded as `TransitionToDurableV1`
    /// whose hold cleared without promoting falls into the `(TransitionToDurableV1, _)` arm of
    /// the regime match, which fails closed with no way out. Ranking the bounded holds the
    /// other way round would turn one unparseable alias row into a permanent outage for a peer
    /// that did nothing wrong.
    #[test]
    fn a_corrupt_alias_row_does_not_brick_a_migrating_sender() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        for (position, alias) in both_scan_positions(&canonical) {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            seed_max_seq(
                store.as_ref(),
                &canonical,
                5,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_TRANSITION_TO_DURABLE_V1,
            );
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &alias), b"{not json")
                .unwrap();

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // Held while the horizon stands, as both pieces of evidence require.
            let held = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 1),
                    ObservedSenderRegime::DurableV1,
                )
                .expect_err("the migration hold must stand for the full horizon");
            assert!(
                held.downcast_ref::<SenderRegimeTransition>().is_some(),
                "{position}: expected the migration hold to outrank the unreadable one; \
                 got: {held}"
            );

            // THE PROPERTY: past the horizon, live durable evidence still completes the
            // migration. Nothing about the corrupt alias row made this refusal permanent.
            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 1),
                    ObservedSenderRegime::DurableV1,
                )
                .unwrap_or_else(|e| {
                    panic!("{position}: a legitimate durable-v1 promotion was over-blocked: {e}")
                });
        }
    }

    /// E: with nothing in conflict, loading is exactly what it was.
    ///
    /// The control that a rule which simply made every hold permanent — or installed one where
    /// there was none — cannot satisfy this suite.
    #[test]
    fn a_clean_pair_of_alias_rows_still_loads_with_no_hold_at_all() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        for (position, alias) in both_scan_positions(&canonical) {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
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
                4,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
            );

            let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
            guard.load_persisted_state().unwrap();

            // Ordinary traffic above the merged floor is accepted immediately — no hold, no
            // wait, no horizon.
            guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::LegacyOrUnproven,
                )
                .unwrap_or_else(|e| {
                    panic!("{position}: a clean two-spelling load must not install a hold: {e}")
                });

            // And the merged floor still rejects, so "no hold" did not become "no bound".
            assert!(
                guard
                    .check_replay_only(
                        &envelope(&sender, &canonical, 10),
                        ObservedSenderRegime::LegacyOrUnproven,
                    )
                    .is_err(),
                "{position}: the merged floor must still reject a replayed sequence"
            );
        }
    }

    // ---------------------------------------------------------------------
    // #2644 review — conservative numeric floor composition across alias rows
    // ---------------------------------------------------------------------
    //
    // The counterpart to the hold-composition block above, on the other axis of the same
    // window. Canonicalization declines to rewrite a group whose canonical row is unreadable,
    // so several readable rows reach the load pass; combining their holds was fixed first, and
    // these pin that combining their *numbers* is conservative too.

    /// One representative `MaxSeqEntry`, spelled where the permutation property can read it.
    fn row(max_seq: u64, semantic_version: u32, sender_regime: u32) -> MaxSeqEntry {
        MaxSeqEntry {
            max_seq,
            updated_at_ms: ReplayGuard::current_time_ms(),
            semantic_version,
            sender_regime,
        }
    }

    /// Reduce a probe to one comparable tag, so two load paths can be asserted *identical*
    /// rather than merely both-non-empty.
    fn outcome(result: Result<()>) -> &'static str {
        match result {
            Ok(()) => "accepted",
            Err(e) => {
                if e.downcast_ref::<ReplayStateUnreadable>().is_some() {
                    "held:unreadable"
                } else if e.downcast_ref::<SenderRegimeTransition>().is_some() {
                    "held:sender-regime-transition"
                } else if e.downcast_ref::<ReplayStateUnsupportedVersion>().is_some() {
                    "held:unsupported-version"
                } else if e.downcast_ref::<UnsupportedSenderRegime>().is_some() {
                    "held:unsupported-sender-regime"
                } else if e.downcast_ref::<ReplayStateLegacy>().is_some() {
                    "held:legacy"
                } else if e.to_string().contains("Replay detected") {
                    "rejected:replay"
                } else {
                    "rejected:other"
                }
            }
        }
    }

    /// THE BUG. Several readable rows for one sender must not resolve by scan order.
    ///
    /// An unreadable *canonical* row makes `install_canonical_row` decline to rewrite the
    /// group, so its readable alias rows stay physically present and all reach the load pass.
    /// Each of them used to assign `window.max_seq` and `window.floor_seq`, so the row `sled`
    /// handed over last won — and `sled` scans lexicographically over keys built from the
    /// spellings, which is to say over an order the attacker who wrote the alias rows chose.
    ///
    /// `PeerHold::Unreadable` covers the horizon and then clears, and the whole justification
    /// for clearing it is that the window's *floor* is left standing (see `PeerHold::rank`).
    /// If that floor is the lower of two durable high-waters, evidence the store still holds
    /// has stopped rejecting sequences it rejected before the restart.
    ///
    /// Both value arrangements are run, because with last-write-wins exactly one of them
    /// happens to produce the right answer: a test that seeded only "high last" would pass on
    /// the defect.
    #[test]
    fn surviving_alias_rows_never_lower_the_numeric_replay_floor() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let early = alias_of(&canonical);
        let late = late_sorting_alias_of(&canonical);

        // CONTROL: the physical scan order is known, and it is not the seeding order.
        assert!(
            spelled_key(MAX_SEQ_PREFIX, &early) < spelled_key(MAX_SEQ_PREFIX, &canonical)
                && spelled_key(MAX_SEQ_PREFIX, &canonical) < spelled_key(MAX_SEQ_PREFIX, &late),
            "CONTROL: base16 sorts before the canonical row and base256-emoji after it"
        );

        for (label, early_seq, late_seq) in [
            ("high scanned first, low scanned last", 10u64, 3u64),
            ("low scanned first, high scanned last", 3u64, 10u64),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                .unwrap();
            for (did, seq) in [(&early, early_seq), (&late, late_seq)] {
                seed_max_seq(
                    store.as_ref(),
                    did,
                    seq,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    SENDER_REGIME_DURABLE_V1,
                );
            }

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // CONTROL: all three rows really did survive canonicalization, so three evidence
            // sources genuinely meet in one window. Without this the test could pass by the
            // aliases having been merged away, proving nothing about the load pass.
            for (what, key) in [
                (
                    "unreadable canonical",
                    spelled_key(MAX_SEQ_PREFIX, &canonical),
                ),
                ("early alias", spelled_key(MAX_SEQ_PREFIX, &early)),
                ("late alias", spelled_key(MAX_SEQ_PREFIX, &late)),
            ] {
                assert!(
                    store.get(&key).unwrap().is_some(),
                    "{label}: CONTROL: the {what} row must still be present"
                );
            }

            assert_eq!(
                guard.get_max_seq(&canonical),
                Some(10),
                "{label}: the loaded high-water must be the maximum across surviving rows"
            );

            // CONTROL: the bounded hold really is standing, so the assertion below is about
            // the floor rather than about the quarantine.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )),
                "held:unreadable",
                "{label}: CONTROL: the unreadable canonical row must quarantine the sender"
            );

            // THE PROPERTY: past the horizon the hold is gone and only the floor is left. It
            // must still be the floor the store had evidence for.
            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )),
                "rejected:replay",
                "{label}: durable evidence of high-water 10 must keep rejecting sequence 5 \
                 once the bounded hold expires"
            );

            // And the floor did not become a blanket refusal: the sender's next legitimate
            // sequence is still accepted.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )),
                "accepted",
                "{label}: sequence 11 is above every surviving high-water and must be accepted"
            );
        }
    }

    /// The design, stated as a property: declining to rewrite storage changes the **hold**
    /// and nothing else.
    ///
    /// Two stores, same readable evidence. In one the canonical key is free, so
    /// canonicalization collapses the aliases and the load pass sees a single merged row. In
    /// the other the canonical key holds an unreadable row, so canonicalization declines and
    /// the load pass sees all three. The interpretation must be identical — same high-water,
    /// same post-horizon verdict — with the quarantine as the only difference.
    ///
    /// This is what "one auditable rule, shared" means operationally: the load pass calls
    /// `merge_max_seq`, the same function canonicalization calls, so the two paths cannot
    /// drift into two subtly different merge semantics.
    #[test]
    fn declining_to_rewrite_storage_changes_only_the_hold_not_the_interpretation() {
        for (label, early_regime, late_regime, observed) in [
            (
                "same namespace",
                SENDER_REGIME_DURABLE_V1,
                SENDER_REGIME_DURABLE_V1,
                ObservedSenderRegime::DurableV1,
            ),
            (
                "mixed namespaces",
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
                SENDER_REGIME_DURABLE_V1,
                ObservedSenderRegime::DurableV1,
            ),
        ] {
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            let early = alias_of(&canonical);
            let late = late_sorting_alias_of(&canonical);

            let mut results = Vec::new();
            for canonical_row in ["absent", "unreadable"] {
                let store = Arc::new(icn_store::SledStore::temporary().unwrap());
                if canonical_row == "unreadable" {
                    store
                        .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                        .unwrap();
                }
                seed_max_seq(
                    store.as_ref(),
                    &early,
                    10,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    early_regime,
                );
                seed_max_seq(
                    store.as_ref(),
                    &late,
                    3,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    late_regime,
                );

                let clock = MergeClock::new();
                let mut guard =
                    ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
                guard.load_persisted_state().unwrap();

                // CONTROL: the two stores really did take different canonicalization paths.
                let merged_away = stored_max_seq(store.as_ref(), &early).is_none();
                assert_eq!(
                    merged_away,
                    canonical_row == "absent",
                    "{label}/{canonical_row}: CONTROL: canonicalization must collapse the \
                     aliases only when the canonical key is free"
                );

                clock.advance(HORIZON_SECS + Duration::from_secs(1));
                results.push((
                    guard.get_max_seq(&canonical),
                    outcome(guard.check_replay_only(&envelope(&sender, &canonical, 4), observed)),
                ));
            }

            assert_eq!(
                results[0], results[1],
                "{label}: a store canonicalization declined to rewrite must be interpreted \
                 exactly as the rewritten one is; got {results:?}"
            );
        }
    }

    /// Over-correction control on the namespace axis (the `5be3fdf0` property, reached
    /// through the load pass instead of through `merge_max_seq`).
    ///
    /// "Always take the maximum" is the wrong fix. A legacy high-water of 10 and a durable-v1
    /// high-water of 3 are numbers from different namespaces; adopting 10 as a durable-v1
    /// floor rejects the sender's legitimate durable sequences 4..=10 as replays, which
    /// `handlers::signed` scores as peer misbehaviour — so the receiver would ban an honest
    /// peer for its own merge. The pair must enter the transition instead.
    #[test]
    fn mixed_regime_alias_rows_enter_the_migration_rather_than_inheriting_the_maximum() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let early = alias_of(&canonical);
        let late = late_sorting_alias_of(&canonical);

        for (label, legacy_alias, durable_alias) in [
            ("legacy scanned first", &early, &late),
            ("legacy scanned last", &late, &early),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                .unwrap();
            seed_max_seq(
                store.as_ref(),
                legacy_alias,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
            );
            seed_max_seq(
                store.as_ref(),
                durable_alias,
                3,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // Held for the horizon, as a namespace change requires — and by the transition
            // hold, which outranks the unreadable one.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 4),
                    ObservedSenderRegime::DurableV1,
                )),
                "held:sender-regime-transition",
                "{label}: incomparable high-waters must enter the migration, not resolve to a \
                 number"
            );

            // THE PROPERTY: past the horizon, live durable-v1 evidence completes the
            // promotion and the legacy 10 is discarded rather than reimposed. Under a naive
            // `max` this is "rejected:replay" — an honest peer banned for our merge.
            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 4),
                    ObservedSenderRegime::DurableV1,
                )),
                "accepted",
                "{label}: a legitimate low durable-v1 sequence must not be blocked by a \
                 legacy-namespace number laundered into durable-v1 state"
            );
        }
    }

    /// A readable, current-version, lower-numbered alias must not rescue a row whose
    /// semantic version this binary cannot interpret.
    ///
    /// The numeric merge runs over rows whose *meaning* differs, so it must not be able to
    /// hand the load pass a usable floor built from the half it happens to understand.
    /// `most_restrictive_semantic_version` keeps the unreadable meaning, and the resulting
    /// hold has no deadline to reach.
    #[test]
    fn an_unsupported_semantic_version_is_not_rescued_by_a_readable_current_alias() {
        const UNSUPPORTED_VERSION: u32 = 9_999;
        assert_ne!(UNSUPPORTED_VERSION, REPLAY_STATE_SEMANTIC_VERSION);
        assert_ne!(UNSUPPORTED_VERSION, LEGACY_REPLAY_STATE_SEMANTIC_VERSION);

        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let early = alias_of(&canonical);
        let late = late_sorting_alias_of(&canonical);

        for (label, unsupported_alias, current_alias) in [
            ("unsupported scanned first", &early, &late),
            ("unsupported scanned last", &late, &early),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                .unwrap();
            seed_max_seq(
                store.as_ref(),
                unsupported_alias,
                10,
                UNSUPPORTED_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            seed_max_seq(
                store.as_ref(),
                current_alias,
                3,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )),
                "held:unsupported-version",
                "{label}: no numeric fallback may make an uninterpretable regime usable"
            );
        }
    }

    /// The same, on the sender-namespace axis: an unrecognised regime tag outranks every
    /// recognised one, so the combined row is refused with no deadline and its number is
    /// never read.
    #[test]
    fn an_unsupported_sender_regime_is_not_rescued_by_a_readable_current_alias() {
        const UNSUPPORTED_REGIME: u32 = 7_777;
        assert!(
            !ReplayGuard::is_recognised_sender_regime(UNSUPPORTED_REGIME),
            "CONTROL: the seeded regime must be one this binary has no migration for"
        );

        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let early = alias_of(&canonical);
        let late = late_sorting_alias_of(&canonical);

        for (label, unsupported_alias, current_alias) in [
            ("unsupported scanned first", &early, &late),
            ("unsupported scanned last", &late, &early),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                .unwrap();
            seed_max_seq(
                store.as_ref(),
                unsupported_alias,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                UNSUPPORTED_REGIME,
            );
            seed_max_seq(
                store.as_ref(),
                current_alias,
                3,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )),
                "held:unsupported-sender-regime",
                "{label}: an unrecognised namespace tag must never expire into acceptance"
            );
        }
    }

    /// The stronger hold still wins while the numbers are being combined, in either row
    /// order — the `067bb7e6` property, re-run now that the numeric axis also composes.
    ///
    /// One row is a current-version durable high-water, the other is tagged with a sender
    /// regime this binary cannot interpret. The numeric merge must not produce a usable floor
    /// *and* the indefinite hold must survive: neither axis may rescue the other.
    #[test]
    fn hold_composition_survives_the_numeric_merge_in_either_row_order() {
        const UNSUPPORTED_REGIME: u32 = 4_242;
        assert!(!ReplayGuard::is_recognised_sender_regime(
            UNSUPPORTED_REGIME
        ));

        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        for (position, alias) in both_scan_positions(&canonical) {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            // The canonical row is readable but carries the unrecognised tag; the alias is
            // an ordinary current durable row that would otherwise supply a usable floor.
            seed_max_seq(
                store.as_ref(),
                &canonical,
                3,
                REPLAY_STATE_SEMANTIC_VERSION,
                UNSUPPORTED_REGIME,
            );
            seed_max_seq(
                store.as_ref(),
                &alias,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )),
                "held:unsupported-sender-regime",
                "{position}: the indefinite hold must survive the numeric merge"
            );
        }
    }

    /// The controls a rule that simply held everything, or installed a floor of 0, cannot
    /// satisfy: the ordinary one-row and clean-pair load paths are byte-for-byte unchanged.
    #[test]
    fn the_single_row_and_clean_pair_load_paths_are_unchanged() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        for (shape, alias) in [
            ("one clean row", None),
            ("clean canonicalized pair", Some(alias_of(&canonical))),
            (
                "clean canonicalized pair, alias scanned last",
                Some(late_sorting_alias_of(&canonical)),
            ),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            if let Some(alias) = &alias {
                seed_max_seq(
                    store.as_ref(),
                    alias,
                    4,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    SENDER_REGIME_DURABLE_V1,
                );
            }

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            assert_eq!(
                guard.peer_count(),
                1,
                "{shape}: one key must reconstruct exactly one window"
            );
            assert_eq!(
                guard.get_max_seq(&canonical),
                Some(10),
                "{shape}: the exact-restore floor is unchanged"
            );

            // No hold: ordinary traffic above the floor is accepted immediately, with no
            // horizon to wait out.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )),
                "accepted",
                "{shape}: a clean load must install no hold"
            );
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 10),
                    ObservedSenderRegime::DurableV1,
                )),
                "rejected:replay",
                "{shape}: and the restored floor must still reject"
            );
            if let Some(alias) = &alias {
                assert!(
                    stored_max_seq(store.as_ref(), alias).is_none(),
                    "{shape}: CONTROL: a clean alias row is still retired by canonicalization"
                );
            }
        }
    }

    /// `merge_max_seq` is commutative **and associative**, which is what licenses folding it
    /// over rows in whatever order the scan produced.
    ///
    /// Canonicalization already folded it over N rows; the load pass now does too, so the
    /// property is load-bearing twice over. Asserting only "all orders agree" would pass on a
    /// constant, so the maximum is asserted alongside it: the number may never be lowered.
    #[test]
    fn combining_high_water_rows_is_order_independent_and_never_lowers_the_number() {
        const UNSUPPORTED_VERSION: u32 = 9_999;
        const UNSUPPORTED_REGIME: u32 = 7_777;

        let sets: [[MaxSeqEntry; 3]; 4] = [
            // One namespace throughout: the maximum, and nothing else.
            [
                row(10, REPLAY_STATE_SEMANTIC_VERSION, SENDER_REGIME_DURABLE_V1),
                row(3, REPLAY_STATE_SEMANTIC_VERSION, SENDER_REGIME_DURABLE_V1),
                row(7, REPLAY_STATE_SEMANTIC_VERSION, SENDER_REGIME_DURABLE_V1),
            ],
            // Mixed recognised namespaces: resolves to the transition, from any direction.
            [
                row(10, REPLAY_STATE_SEMANTIC_VERSION, SENDER_REGIME_DURABLE_V1),
                row(
                    3,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    SENDER_REGIME_LEGACY_OR_UNPROVEN,
                ),
                row(
                    7,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    SENDER_REGIME_TRANSITION_TO_DURABLE_V1,
                ),
            ],
            // An unrecognised regime present: it outranks all three recognised states.
            [
                row(10, REPLAY_STATE_SEMANTIC_VERSION, UNSUPPORTED_REGIME),
                row(
                    3,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    SENDER_REGIME_LEGACY_OR_UNPROVEN,
                ),
                row(7, REPLAY_STATE_SEMANTIC_VERSION, SENDER_REGIME_DURABLE_V1),
            ],
            // Versions differ too, so both merge axes are exercised at once.
            [
                row(10, UNSUPPORTED_VERSION, SENDER_REGIME_DURABLE_V1),
                row(
                    3,
                    LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
                    SENDER_REGIME_LEGACY_OR_UNPROVEN,
                ),
                row(7, REPLAY_STATE_SEMANTIC_VERSION, SENDER_REGIME_DURABLE_V1),
            ],
        ];

        const PERMUTATIONS: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];

        for (index, set) in sets.iter().enumerate() {
            let highest = set.iter().map(|e| e.max_seq).max().unwrap();
            let mut answers = Vec::new();
            for order in PERMUTATIONS {
                let mut acc = set[order[0]].clone();
                for position in &order[1..] {
                    acc = ReplayGuard::merge_max_seq(&acc, &set[*position]);
                }
                assert_eq!(
                    acc.max_seq, highest,
                    "set {index} order {order:?}: combining rows must never lower the number"
                );
                answers.push((acc.max_seq, acc.semantic_version, acc.sender_regime));
            }
            answers.dedup();
            assert_eq!(
                answers.len(),
                1,
                "set {index}: every fold order must reach one answer; got {answers:?}"
            );
        }
    }

    /// A `v0` sibling row must not erase an interpretable current-version floor.
    ///
    /// The over-correction control for the *version* axis, and the reason the load pass
    /// merges within one `semantic_version` rather than across all of them. The two arms are
    /// not comparable: the current-version arm establishes a floor and no hold, the legacy
    /// arm establishes a bounded hold and deliberately no floor. Collapsing both rows into
    /// one row forces one arm to run, and if `most_restrictive_semantic_version` picks `v0`
    /// — which it must, so the hold stands — the current row's floor is discarded outright.
    /// Sequence 5 was then accepted after the horizon against durable evidence of 10.
    ///
    /// Both effects must survive: the bounded hold *and* the floor.
    #[test]
    fn a_legacy_sibling_row_cannot_erase_an_interpretable_current_version_floor() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let early = alias_of(&canonical);
        let late = late_sorting_alias_of(&canonical);

        for (label, current_alias, legacy_alias) in [
            ("current row scanned first", &early, &late),
            ("current row scanned last", &late, &early),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            // Unreadable canonical row, so canonicalization declines and both readable rows
            // reach the load pass.
            store
                .put(&spelled_key(MAX_SEQ_PREFIX, &canonical), b"{not json")
                .unwrap();
            seed_max_seq(
                store.as_ref(),
                current_alias,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            // Byte-for-byte what a pre-#2517 receiver wrote: neither field present, so both
            // read back as their `serde(default)`.
            store
                .put(
                    &spelled_key(MAX_SEQ_PREFIX, legacy_alias),
                    &serde_json::to_vec(&serde_json::json!({
                        "max_seq": 3,
                        "updated_at_ms": ReplayGuard::current_time_ms(),
                    }))
                    .unwrap(),
                )
                .unwrap();

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            assert_eq!(
                guard.get_max_seq(&canonical),
                Some(10),
                "{label}: an uninterpretable sibling row must not discard an interpretable \
                 current-version high-water"
            );

            // CONTROL: the legacy row's own effect survived too — this is a join, not a
            // preference for the current-version row.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::LegacyOrUnproven,
                )),
                "held:unreadable",
                "{label}: CONTROL: the bounded holds must still stand"
            );

            // THE PROPERTY: past the horizon the floor is what is left, and it is the
            // interpretable one.
            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )),
                "rejected:replay",
                "{label}: sequence 5 must stay rejected against the surviving floor of 10"
            );
        }
    }

    // ---------------------------------------------------------------------
    // #2644 review — readable provenance rows must be joined, not applied row by row
    // ---------------------------------------------------------------------

    /// One provenance row spelled exactly as a pre-#2640 receiver wrote it.
    fn seed_provenance(store: &dyn Store, did: &Did, value: &[u8]) {
        store
            .put(&spelled_key(SENDER_REGIME_PREFIX, did), value)
            .unwrap();
    }

    /// The escape hatch that puts several readable provenance rows in front of the load pass.
    ///
    /// `install_canonical_row` deliberately writes and deletes nothing when the *canonical*
    /// row is the unreadable one, so every readable alias for that principal survives
    /// canonicalization. Both spellings therefore reach the provenance loop, and one of them
    /// still carries a transition tag the other has already superseded.
    #[test]
    fn a_stale_transition_alias_cannot_destroy_a_durable_provenance_floor() {
        for durable_goes_late in [false, true] {
            let label = if durable_goes_late {
                "durable alias scanned last"
            } else {
                "durable alias scanned first"
            };
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            let [(_, early), (_, late)] = both_scan_positions(&canonical);
            let (durable_alias, transition_alias) = if durable_goes_late {
                (late, early)
            } else {
                (early, late)
            };

            // Current-version durable evidence: the floor that must survive.
            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            // Unreadable canonical provenance: what makes canonicalization decline.
            seed_provenance(store.as_ref(), &canonical, &[0xff, 0xff, 0xff]);
            seed_provenance(
                store.as_ref(),
                &durable_alias,
                &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
            );
            seed_provenance(
                store.as_ref(),
                &transition_alias,
                &SENDER_REGIME_TRANSITION_TO_DURABLE_V1.to_be_bytes(),
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // CONTROL: canonicalization really did decline, so all three physical rows are
            // still there and this is a test about interpreting them.
            for (what, did) in [
                ("canonical", &canonical),
                ("durable alias", &durable_alias),
                ("transition alias", &transition_alias),
            ] {
                assert!(
                    store
                        .get(&spelled_key(SENDER_REGIME_PREFIX, did))
                        .unwrap()
                        .is_some(),
                    "{label}: CONTROL: the {what} provenance row must survive canonicalization"
                );
            }

            // CONTROL: the floor really was established by the current-version row.
            assert_eq!(
                guard.get_max_seq(&canonical),
                Some(10),
                "{label}: CONTROL: current-version durable evidence must establish floor 10"
            );

            // CONTROL: the bounded hold stands for its horizon.
            assert_ne!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )),
                "accepted",
                "{label}: CONTROL: a bounded hold must stand before the horizon"
            );

            clock.advance(HORIZON_SECS + Duration::from_secs(1));

            // THE PROPERTY: sequence 5 sits below a floor of 10 that both a current-version
            // row and a durable provenance alias license. A staler alias carrying the
            // superseded transition tag must not route this sender into the migration
            // promotion, whose reset would zero that floor.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )),
                "rejected:replay",
                "{label}: a stale transition alias must not zero a durable replay floor"
            );
            assert_eq!(
                guard.get_max_seq(&canonical),
                Some(10),
                "{label}: the floor must still be standing afterwards"
            );
        }
    }

    /// The discriminating control: identical numeric state, identical unreadable canonical
    /// row, but no transition alias. The floor must be rejected permanently, which is what
    /// makes the test above a statement about the transition alias and not about the floor.
    #[test]
    fn a_durable_alias_alone_keeps_the_floor_permanently() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_provenance(store.as_ref(), &canonical, &[0xff, 0xff, 0xff]);
        seed_provenance(
            store.as_ref(),
            &alias,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:unreadable",
            "CONTROL: the unreadable canonical row must quarantine for its horizon"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "rejected:replay",
            "an unreadable quarantine clears the hold and leaves the floor standing"
        );
    }

    /// The join's algebra, over every value that can physically appear in a provenance row.
    ///
    /// The load pass folds rows in `sled`'s order, which is a property of the spellings an
    /// attacker picked. Commutativity is therefore the security property, not a nicety.
    #[test]
    fn the_provenance_join_is_commutative_associative_and_idempotent() {
        let values = [
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
            SENDER_REGIME_DURABLE_V1,
            SENDER_REGIME_TRANSITION_TO_DURABLE_V1,
            7,
            u32::MAX,
        ];
        for a in values {
            assert_eq!(
                ReplayGuard::joined_sender_regime_provenance(a, a),
                a,
                "joining {a} with itself must be a no-op"
            );
            for b in values {
                assert_eq!(
                    ReplayGuard::joined_sender_regime_provenance(a, b),
                    ReplayGuard::joined_sender_regime_provenance(b, a),
                    "joining {a} and {b} must not depend on their order"
                );
                for c in values {
                    let left = ReplayGuard::joined_sender_regime_provenance(
                        ReplayGuard::joined_sender_regime_provenance(a, b),
                        c,
                    );
                    let right = ReplayGuard::joined_sender_regime_provenance(
                        a,
                        ReplayGuard::joined_sender_regime_provenance(b, c),
                    );
                    assert_eq!(
                        left, right,
                        "joining {a}, {b}, {c} must not depend on grouping"
                    );
                }
            }
        }
    }

    /// The two orderings the join is derived from, stated directly.
    #[test]
    fn the_provenance_join_prefers_durable_and_refuses_anything_uninterpretable() {
        assert_eq!(
            ReplayGuard::joined_sender_regime_provenance(
                SENDER_REGIME_DURABLE_V1,
                SENDER_REGIME_TRANSITION_TO_DURABLE_V1
            ),
            SENDER_REGIME_DURABLE_V1,
            "a finished migration outranks the fossil of the migration that finished"
        );
        for uninterpretable in [SENDER_REGIME_LEGACY_OR_UNPROVEN, 7, u32::MAX] {
            assert_eq!(
                ReplayGuard::joined_sender_regime_provenance(
                    SENDER_REGIME_DURABLE_V1,
                    uninterpretable
                ),
                uninterpretable,
                "{uninterpretable} has no meaning in the provenance keyspace and must not be \
                 absorbed by a DurableV1 sibling"
            );
        }
        // CONTROL: the value that separates this join from `strongest_sender_regime`. That
        // helper ranks `SENDER_REGIME_LEGACY_OR_UNPROVEN` as the weakest *legal* value, which
        // is right for a `max_seq` row's field and wrong here — nothing writes it here.
        assert_eq!(
            ReplayGuard::strongest_sender_regime(
                SENDER_REGIME_DURABLE_V1,
                SENDER_REGIME_LEGACY_OR_UNPROVEN
            ),
            SENDER_REGIME_DURABLE_V1,
            "CONTROL: the high-water field join really does disagree, so this is two joins \
             and not one renamed"
        );
    }

    /// Matrix 3 — a transition row with no sibling still behaves exactly as #2517 requires.
    #[test]
    fn a_lone_transition_row_still_holds_and_still_promotes() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_TRANSITION_TO_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition",
            "an unfinished migration must restart its full hold"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // THE PROPERTY: the join did not disturb the one-row path — promotion still happens,
        // and still requires live durable evidence to do it.
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "the migration must still complete on live durable-v1 evidence"
        );
        assert_eq!(
            store
                .get(&ReplayGuard::make_sender_regime_key(&pk(&canonical)))
                .unwrap()
                .as_deref(),
            Some(&SENDER_REGIME_DURABLE_V1.to_be_bytes()[..]),
            "promotion must record the finished migration on the canonical key"
        );
    }

    /// Matrix 4 / 10 — the clean single-row paths, unchanged and holdless.
    #[test]
    fn a_clean_canonical_provenance_row_costs_no_hold_in_either_regime() {
        for (label, regime, seq, expected) in [
            ("durable", SENDER_REGIME_DURABLE_V1, 11u64, "accepted"),
            (
                "durable replay",
                SENDER_REGIME_DURABLE_V1,
                5,
                "rejected:replay",
            ),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            seed_provenance(store.as_ref(), &canonical, &regime.to_be_bytes());

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // No hold at all: an established sender must not pay for the join existing.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, seq),
                    ObservedSenderRegime::DurableV1,
                )),
                expected,
                "{label}: the ordinary established-sender path must be unchanged"
            );
            // CONTROL: canonicalization left the single canonical row exactly as it was.
            assert_eq!(
                store
                    .get(&ReplayGuard::make_sender_regime_key(&pk(&canonical)))
                    .unwrap()
                    .as_deref(),
                Some(&regime.to_be_bytes()[..]),
                "{label}: CONTROL: a lone canonical row must not be rewritten"
            );
        }
    }

    /// Matrix 5 — an uninterpretable alias refuses the principal even beside a durable one,
    /// and no elapsed time releases it.
    ///
    /// Run for `SENDER_REGIME_LEGACY_OR_UNPROVEN` as well as an unrecognised tag, because
    /// that value is exactly where the provenance join and `strongest_sender_regime` part
    /// company: nothing in this crate writes `0` to this keyspace, so a `0` here is a foreign
    /// or corrupt writer and the load pass has always refused it with no deadline. Merging it
    /// away under a `DurableV1` sibling would make canonicalization *more permissive than the
    /// load pass it feeds*, which is the escape hatch this whole review round is about.
    #[test]
    fn an_uninterpretable_provenance_alias_refuses_indefinitely_beside_a_durable_one() {
        for uninterpretable in [SENDER_REGIME_LEGACY_OR_UNPROVEN, 7] {
            for durable_goes_late in [false, true] {
                let label = format!("value {uninterpretable}, durable late = {durable_goes_late}");
                let store = Arc::new(icn_store::SledStore::temporary().unwrap());
                let sender = KeyPair::generate().unwrap();
                let canonical = sender.did().clone();
                let [(_, early), (_, late)] = both_scan_positions(&canonical);
                let (durable_alias, other_alias) = if durable_goes_late {
                    (late, early)
                } else {
                    (early, late)
                };

                seed_max_seq(
                    store.as_ref(),
                    &canonical,
                    10,
                    REPLAY_STATE_SEMANTIC_VERSION,
                    SENDER_REGIME_DURABLE_V1,
                );
                seed_provenance(
                    store.as_ref(),
                    &durable_alias,
                    &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
                );
                seed_provenance(store.as_ref(), &other_alias, &uninterpretable.to_be_bytes());

                let clock = MergeClock::new();
                let mut guard =
                    ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
                guard.load_persisted_state().unwrap();

                // THE PROPERTY: fail-closed wins, and it has no deadline.
                for stage in ["immediately", "after the envelope validity horizon"] {
                    assert_eq!(
                        outcome(guard.check_replay_only(
                            &envelope(&sender, &canonical, 11),
                            ObservedSenderRegime::DurableV1,
                        )),
                        "held:unsupported-sender-regime",
                        "{label}: an uninterpretable provenance value must refuse {stage}"
                    );
                    clock.advance(HORIZON_SECS + Duration::from_secs(1));
                }
            }
        }
    }

    /// Matrix 6 — an unreadable row contributes its bounded quarantine and nothing else: the
    /// durable state a readable sibling established is still there when the hold clears.
    #[test]
    fn an_unreadable_row_quarantines_without_disturbing_the_durable_state_it_sits_beside() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_provenance(store.as_ref(), &canonical, &[0xff, 0xff, 0xff]);
        seed_provenance(
            store.as_ref(),
            &alias,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::DurableV1,
            )),
            "held:unreadable",
            "the unreadable row must quarantine for its bounded horizon"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // THE PROPERTY: the hold cleared, and the durable regime plus its floor survived it.
        // A `DurableV1` observation is accepted without a transition hold, which is only true
        // if the window's regime is still `DurableV1`.
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "the durable regime the readable sibling established must survive the quarantine"
        );
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "rejected:replay",
            "and so must its floor"
        );
    }

    /// Matrix 7 — provenance outlives the high-water, so the join must work with no numeric
    /// row at all and must not invent one.
    #[test]
    fn joined_provenance_without_any_high_water_row_invents_no_floor() {
        for durable_goes_late in [false, true] {
            let label = if durable_goes_late {
                "durable alias scanned last"
            } else {
                "durable alias scanned first"
            };
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            let [(_, early), (_, late)] = both_scan_positions(&canonical);
            let (durable_alias, transition_alias) = if durable_goes_late {
                (late, early)
            } else {
                (early, late)
            };

            seed_provenance(store.as_ref(), &canonical, &[0xff, 0xff, 0xff]);
            seed_provenance(
                store.as_ref(),
                &durable_alias,
                &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
            );
            seed_provenance(
                store.as_ref(),
                &transition_alias,
                &SENDER_REGIME_TRANSITION_TO_DURABLE_V1.to_be_bytes(),
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            assert_eq!(
                guard.get_max_seq(&canonical),
                Some(0),
                "{label}: provenance rows carry no number and must not manufacture a floor"
            );

            clock.advance(HORIZON_SECS + Duration::from_secs(1));

            // THE PROPERTY: the joined regime is `DurableV1`, so an aged-out peer resumes on
            // its established namespace and pays no second migration hold — and sequence 1 is
            // accepted because there genuinely is no floor, not because one was destroyed.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 1),
                    ObservedSenderRegime::DurableV1,
                )),
                "accepted",
                "{label}: an established sender with no high-water resumes without a hold"
            );
        }
    }

    /// Matrix 8 — the `ea599560` distinction survives the join.
    ///
    /// Provenance is version-less. A `DurableV1` the join derived from provenance must NOT
    /// set `sender_regime_from_current_version`, so a `MigratingFromLegacy` expiry still
    /// demotes it and a durable claim still re-earns its transition hold. If the join were to
    /// set that bit, this sender would keep a durable namespace it never proved under the
    /// current semantic regime.
    ///
    /// Exercises the *canonicalization* side of the shared join on purpose: the load-side
    /// join is only reachable when the canonical row is unreadable, and an `Unreadable` hold
    /// outranks `MigratingFromLegacy`, which would mask the demotion this test is about.
    #[test]
    fn a_join_derived_durable_regime_still_demotes_at_a_legacy_migration_expiry() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let alias = alias_of(&canonical);

        // Only legacy-version numeric state, so the window's regime cannot come from a
        // current-version arm.
        seed_max_seq(
            store.as_ref(),
            &canonical,
            4,
            LEGACY_REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_TRANSITION_TO_DURABLE_V1.to_be_bytes(),
        );
        seed_provenance(
            store.as_ref(),
            &alias,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();

        // CONTROL: the join really did run and really did resolve to `DurableV1` — otherwise
        // the assertion below would be about a transition row, not a durable one.
        assert_eq!(
            store
                .get(&ReplayGuard::make_sender_regime_key(&pk(&canonical)))
                .unwrap()
                .as_deref(),
            Some(&SENDER_REGIME_DURABLE_V1.to_be_bytes()[..]),
            "CONTROL: the join must have resolved the alias pair to DurableV1"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // THE PROPERTY: version-less provenance did not become current-version provenance.
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 9),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition",
            "a provenance-derived durable regime must not survive the legacy migration expiry"
        );
    }

    /// The stronger reading of matrix 1: the fossil does not merely fail to win, it never
    /// installs a `MigratingSenderRegime` hold at all. Stated separately so the floor
    /// assertion in the test above stays the discriminator that reproduced the finding.
    #[test]
    fn a_superseded_transition_alias_installs_no_migration_hold() {
        for durable_goes_late in [false, true] {
            let label = if durable_goes_late {
                "durable late"
            } else {
                "durable first"
            };
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            let [(_, early), (_, late)] = both_scan_positions(&canonical);
            let (durable_alias, transition_alias) = if durable_goes_late {
                (late, early)
            } else {
                (early, late)
            };

            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_DURABLE_V1,
            );
            seed_provenance(store.as_ref(), &canonical, &[0xff, 0xff, 0xff]);
            seed_provenance(
                store.as_ref(),
                &durable_alias,
                &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
            );
            seed_provenance(
                store.as_ref(),
                &transition_alias,
                &SENDER_REGIME_TRANSITION_TO_DURABLE_V1.to_be_bytes(),
            );

            let clock = MergeClock::new();
            let mut guard =
                ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
            guard.load_persisted_state().unwrap();

            // The only hold is the unreadable canonical row's bounded quarantine. A
            // `MigratingSenderRegime` hold would outrank it and show up here instead.
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 11),
                    ObservedSenderRegime::DurableV1,
                )),
                "held:unreadable",
                "{label}: the superseded transition alias must install no migration hold"
            );
        }
    }
    // ------------------------------------------------------------------
    // Cross-keyspace joins: provenance against the high-water (#2644)
    // ------------------------------------------------------------------
    //
    // `replay_max_seq` and `replay_sender_regime` are two keyspaces holding two different
    // kinds of evidence about one sender, and the load pass joins them. The rule these tests
    // pin is the same one `5be3fdf0` established *between* spelling-distinct `max_seq` rows,
    // now applied *across* the two keyspaces:
    //
    //   Provenance may establish which sender namespace has been proven. It may not
    //   reinterpret a number that a readable current-version row has already placed in a
    //   different one.
    //
    // Provenance is one version-less `u32`, written at state transitions and deliberately
    // outliving the high-water beside it (`cleanup()` retires the number and keeps the
    // proof). So it is the only evidence in the common aged-out case and must settle it — but
    // where a current-version `replay_max_seq` row *has* spoken, the two facts are both true
    // and can disagree, and the number keeps the namespace that produced it.
    //
    // The two directions the join can fail are opposites and both are covered: relabelling a
    // legacy number as durable over-blocks an honest peer and scores it, while discarding a
    // durable number under a fossil transition record hands an authenticated sender a replay
    // window.

    fn boot_merge(store: &Arc<icn_store::SledStore>, clock: Arc<MergeClock>) -> ReplayGuard {
        let mut guard =
            ReplayGuard::new_persistent(300, 3600, store.clone()).with_clock(clock.clone());
        guard.load_persisted_state().unwrap();
        guard
    }

    /// **A — THE REPORTED BUG.** Durable provenance must not relabel a legacy-tagged floor.
    ///
    /// The store holds two true facts about one sender: a current-version high-water saying
    /// "10, and it is a number from the sender's *unproven* numbering", and provenance saying
    /// "this sender did once establish durable-v1". Before this fix the provenance pass
    /// assigned `window.sender_regime = DurableV1` over the top of the first, leaving the
    /// number 10 in place and installing no hold. The window then read as an ordinary
    /// established durable-v1 sender with a durable floor of 10, so the sender's legitimate
    /// durable sequences 1..=10 came back as a bare `Replay detected` — which
    /// `handlers::signed` records as `Violation::ReplayAttack` against the peer.
    ///
    /// Both spelling positions are exercised. The reachable history the reviewer described
    /// runs through an alias — a pre-#2640 store keyed rows by the DID *as spelled*, so a
    /// legacy row and a durable provenance row can sit under different spellings of one key —
    /// and #2644's canonicalization is what brings them onto one principal. The defect does
    /// not actually need the alias: once canonicalization has run, the shape is an ordinary
    /// canonical high-water beside an ordinary canonical provenance row, which is why the
    /// third case seeds exactly that.
    #[test]
    fn durable_provenance_does_not_relabel_a_current_version_legacy_floor() {
        for (label, max_seq_spelling, provenance_spelling) in [
            ("both canonical", false, false),
            ("legacy number on the alias", true, false),
            ("provenance on the alias", false, true),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            let alias = alias_of(&canonical);
            let spell = |on_alias: bool| if on_alias { &alias } else { &canonical };

            seed_max_seq(
                store.as_ref(),
                spell(max_seq_spelling),
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
            );
            seed_provenance(
                store.as_ref(),
                spell(provenance_spelling),
                &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
            );

            let clock = MergeClock::new();
            let mut guard = boot_merge(&store, clock.clone());

            // CONTROL: both rows were actually read. A load that skipped either would make
            // every assertion below pass for the wrong reason — a window with no numeric
            // state also refuses a durable claim, and so does one with no provenance.
            let window = guard
                .sequences
                .get(&pk(&canonical))
                .unwrap_or_else(|| panic!("{label}: one window must exist for the principal"));
            assert_eq!(
                window.floor_seq, 10,
                "{label}: CONTROL: the legacy high-water must have been restored"
            );
            assert_eq!(
                window.numeric_namespace,
                NumericNamespace::LegacyOrUnproven,
                "{label}: the number must keep the namespace the row gave it"
            );

            // THE PROPERTY: the sender's legitimate durable sequence 5 is not an ordinary
            // replay. The two facts disagree, and the disagreement is resolved by the
            // existing migration, not by relabelling the number.
            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )
                .expect_err("a durable-v1 sequence under an unresolved namespace must be held");
            assert!(
                err.downcast_ref::<SenderRegimeTransition>().is_some(),
                "{label}: expected the typed sender-regime transition; got: {err}"
            );
            assert!(
                !err.to_string().contains("Replay detected"),
                "{label}: the legacy floor must not be reused as a durable floor; got: {err}"
            );
            assert!(
                matches!(
                    guard.sequences[&pk(&canonical)].hold,
                    Some(PeerHold::MigratingSenderRegime { .. })
                ),
                "{label}: the migration hold must be installed"
            );
            assert_eq!(
                guard.sequences[&pk(&canonical)].floor_seq,
                10,
                "{label}: the legacy number is retained as legacy evidence for the hold, so \
                 captured old-namespace traffic stays rejected"
            );
        }
    }

    /// **A′ — the discriminating control for A.** Provenance must only ever *add* refusal.
    ///
    /// Same store minus the provenance row. If A passed because the legacy floor alone always
    /// produces a transition, this control passes identically and A proves nothing about the
    /// join. It is here to show the two inputs reach the same safe answer — which is the
    /// point: the presence of durable provenance used to *weaken* the outcome from a typed,
    /// unscored migration refusal to a scored replay verdict.
    #[test]
    fn a_legacy_floor_without_provenance_reaches_the_same_safe_answer() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );

        // CONTROL: no provenance row exists, so this is the other half of the pair.
        assert!(
            store
                .get(&ReplayGuard::make_sender_regime_key(&pk(&canonical)))
                .unwrap()
                .is_none(),
            "CONTROL: this case must have no provenance at all"
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());
        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("a durable claim against a legacy floor must be held");
        assert!(
            err.downcast_ref::<SenderRegimeTransition>().is_some(),
            "expected the transition path with no provenance either; got: {err}"
        );
    }

    /// **B — the migration the reported shape enters actually completes, and only correctly.**
    ///
    /// Three gates in one test, because each is only meaningful against the others: elapsed
    /// time alone must not promote, live durable-v1 evidence alone must not shorten the
    /// horizon, and the promotion must retire the legacy number rather than carry it into the
    /// new namespace.
    #[test]
    fn the_relabel_refusal_completes_through_the_ordinary_transition() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());

        // Before the horizon: typed refusal, and the legacy number still rejects captured
        // old-namespace traffic on its own terms.
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition",
            "before the horizon the namespace is unresolved"
        );

        // Horizon reached, but the peer is no longer advertising the capability: elapsed time
        // alone must not promote (#2517 Phase 11).
        clock.advance(HORIZON_SECS + Duration::from_secs(1));
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::LegacyOrUnproven,
            )),
            "held:sender-regime-transition",
            "the horizon alone must not promote — promotion needs evidence from now"
        );
        assert_eq!(
            guard.sequences[&pk(&canonical)].floor_seq,
            10,
            "CONTROL: the legacy floor must still be standing, so the next step is a real \
             promotion rather than a floor that was already 0"
        );

        // Horizon plus live durable-v1 evidence: the migration completes, the incomparable
        // legacy number is retired, and the sender's durable sequence 5 becomes usable.
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "with the horizon elapsed and live durable-v1 evidence the migration completes"
        );
        let window = &guard.sequences[&pk(&canonical)];
        assert_eq!(
            window.floor_seq, 0,
            "the legacy number is incomparable with the new namespace and must be retired"
        );
        assert_eq!(window.sender_regime, SenderRegimeState::DurableV1);
        assert_eq!(window.numeric_namespace, NumericNamespace::DurableV1);
    }

    /// **C — provenance-only state must NOT pay a migration hold.**
    ///
    /// The case provenance exists for: `cleanup()` retires the numeric high-water of a peer
    /// that went quiet and deliberately keeps the proof that its legacy namespace was
    /// retired. Such a peer resumes as an established durable-v1 sender with no numeric bound.
    /// A fix that routed every durable provenance row through the migration would make routine
    /// garbage collection cost every quiet peer a ten-minute outage.
    #[test]
    fn provenance_without_a_high_water_resumes_durable_with_no_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        // CONTROL: there is genuinely no numeric row — the distinction under test is
        // "no evidence" versus "evidence that says legacy", not a difference in numbers.
        assert!(
            stored_max_seq(store.as_ref(), &canonical).is_none(),
            "CONTROL: no high-water row may exist"
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());

        let window = &guard.sequences[&pk(&canonical)];
        assert_eq!(window.sender_regime, SenderRegimeState::DurableV1);
        assert_eq!(window.floor_seq, 0, "no numeric bound survives cleanup");
        assert!(
            window.hold.is_none(),
            "no hold may be imposed: {:?}",
            window.hold
        );
        assert!(
            !window.sender_regime_from_current_version,
            "provenance is version-less and must not claim a current-version row said this"
        );

        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "an established durable-v1 sender with no numeric state resumes immediately"
        );
    }

    /// **D — a durable floor beside agreeing provenance is preserved, permanently.**
    #[test]
    fn a_durable_floor_with_durable_provenance_is_preserved_permanently() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());

        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "rejected:replay",
            "a durable sequence at or below a durable floor is an ordinary replay"
        );
        clock.advance(HORIZON_SECS * 10);
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "rejected:replay",
            "no amount of elapsed time may retire a durable floor — nothing is being migrated"
        );
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "CONTROL: the window is a working durable namespace, not a blanket refusal"
        );
    }

    /// **E — stale transition provenance must not reset a valid durable floor.**
    ///
    /// The mirror-image failure, and the dangerous direction. Provenance outlives the
    /// high-water by design and an unreadable *canonical* provenance row leaves readable alias
    /// rows standing (`fd7665c8`), so a fossil `TransitionToDurableV1` record can meet a
    /// current-version durable high-water. Entering the migration is the conservative answer —
    /// it refuses strictly more, and it is what the promotion's own write ordering asks for
    /// after a crash between the two persists. What must not follow is the promotion
    /// *discarding* the number: that number was produced by the namespace being promoted to,
    /// so retiring the previous namespace says nothing about it, and zeroing it hands an
    /// authenticated sender back every sequence the floor was rejecting.
    #[test]
    fn stale_transition_provenance_does_not_reset_a_durable_floor() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_TRANSITION_TO_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());

        // CONTROL: the transition provenance really did install its hold. Without this the
        // test could pass on a build that ignored the provenance row outright, which is a
        // different (and less safe) fix.
        assert!(
            matches!(
                guard.sequences[&pk(&canonical)].hold,
                Some(PeerHold::MigratingSenderRegime { .. })
            ),
            "CONTROL: transition provenance must still impose its hold"
        );
        assert_eq!(
            guard.sequences[&pk(&canonical)].numeric_namespace,
            NumericNamespace::DurableV1,
            "the number stays a durable-v1 number through the regime change"
        );
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition",
            "CONTROL: the hold is real and refuses during the horizon"
        );

        // The hold runs its full course and promotes on live durable evidence.
        clock.advance(HORIZON_SECS + Duration::from_secs(1));

        // THE PROPERTY: the promotion completed, and the durable floor survived it.
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "rejected:replay",
            "a fossil transition record must not hand back sequences a durable floor rejects"
        );
        assert_eq!(
            guard.sequences[&pk(&canonical)].floor_seq,
            10,
            "the durable floor must survive the promotion intact"
        );
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 11),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "CONTROL: the sender is not bricked — its next real sequence is accepted"
        );
        assert_eq!(
            stored_max_seq(store.as_ref(), &canonical)
                .expect("the promotion rewrote the row")
                .max_seq,
            11,
            "and the retained floor was persisted rather than zeroed, so a restart keeps it"
        );
    }

    /// **F — durable provenance must not release a migration the high-water requires.**
    ///
    /// The canonical-keyspace form of `durable_provenance_does_not_release_the_mixed_regime_
    /// migration_hold`, which pins the same rule for spelling-distinct `max_seq` rows. A
    /// current-version row tagged `TransitionToDurableV1` is a receiver-local statement that a
    /// namespace change was in flight when this node stopped; a provenance row saying the
    /// sender once reached durable-v1 does not establish that this node finished retiring the
    /// old numbering, and must not cut the hold short.
    #[test]
    fn durable_provenance_does_not_release_a_transition_high_waters_hold() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_TRANSITION_TO_DURABLE_V1,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());

        assert!(
            matches!(
                guard.sequences[&pk(&canonical)].hold,
                Some(PeerHold::MigratingSenderRegime { .. })
            ),
            "the hold the high-water requires must survive the provenance pass"
        );
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition",
            "provenance must not release the hold early"
        );
        // CONTROL: the hold is bounded, not permanent — it is a migration, not a refusal.
        clock.advance(HORIZON_SECS + Duration::from_secs(1));
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "CONTROL: the transition still completes at the horizon on live evidence"
        );
    }

    /// **G — a legacy floor beside transition provenance keeps the existing behaviour.**
    ///
    /// Unchanged by this fix, and asserted so a future edit to the `DurableV1` arm cannot
    /// silently drag its neighbour with it: the number is legacy, the migration is in flight,
    /// and the promotion at the end retires the number.
    #[test]
    fn a_legacy_floor_with_transition_provenance_still_migrates_coherently() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_TRANSITION_TO_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());

        // The legacy number is retained as legacy evidence: a captured old-namespace envelope
        // stays rejected for the whole hold.
        assert_eq!(
            guard.sequences[&pk(&canonical)].floor_seq,
            10,
            "CONTROL: the legacy bound must still be standing during the migration"
        );
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition"
        );

        clock.advance(HORIZON_SECS + Duration::from_secs(1));
        assert_eq!(
            outcome(guard.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "the legacy number is incomparable with the new namespace and is retired"
        );
        assert_eq!(
            guard.sequences[&pk(&canonical)].floor_seq,
            0,
            "a legacy number must NOT survive its own migration — only a durable one does"
        );
    }

    /// **H — unsupported provenance stays fail-closed beside any interpretable high-water.**
    ///
    /// Both high-water regimes, because the fix added a branch to the arm next door and an
    /// unsupported value must reach neither side of it. There is no deadline here: elapsed
    /// time cannot make an unknown namespace tag interpretable.
    #[test]
    fn unsupported_provenance_holds_any_high_water_indefinitely() {
        const UNSUPPORTED: u32 = 77;
        for (label, regime) in [
            ("legacy high-water", SENDER_REGIME_LEGACY_OR_UNPROVEN),
            ("durable high-water", SENDER_REGIME_DURABLE_V1),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();

            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                regime,
            );
            seed_provenance(store.as_ref(), &canonical, &UNSUPPORTED.to_be_bytes());

            let clock = MergeClock::new();
            let mut guard = boot_merge(&store, clock.clone());

            for elapsed in [Duration::ZERO, HORIZON_SECS * 100] {
                clock.advance(elapsed);
                assert_eq!(
                    outcome(guard.check_replay_only(
                        &envelope(&sender, &canonical, 5),
                        ObservedSenderRegime::DurableV1,
                    )),
                    "held:unsupported-sender-regime",
                    "{label}: an unsupported provenance value has no deadline to reach"
                );
            }
        }
    }

    /// **I — unreadable provenance contributes a bounded hold and relabels nothing.**
    ///
    /// Unreadable provenance answers a different question from any provenance *value*: "a
    /// record exists here whose meaning is unavailable". Its whole contribution is the
    /// quarantine, and when that expires the window must be exactly what the readable
    /// evidence establishes on its own — a legacy floor still legacy, a durable floor still
    /// durable. An expiry that resolved the corrupt row into a `DurableV1` reading would
    /// reintroduce the reported defect through the back door.
    #[test]
    fn unreadable_provenance_expires_without_relabelling_a_floor() {
        for (label, regime, after_expiry) in [
            (
                "legacy floor",
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
                "held:sender-regime-transition",
            ),
            ("durable floor", SENDER_REGIME_DURABLE_V1, "rejected:replay"),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();

            seed_max_seq(
                store.as_ref(),
                &canonical,
                10,
                REPLAY_STATE_SEMANTIC_VERSION,
                regime,
            );
            // Not four bytes: a record that exists and cannot be read as a regime.
            store
                .put(&spelled_key(SENDER_REGIME_PREFIX, &canonical), b"xx")
                .unwrap();

            let clock = MergeClock::new();
            let mut guard = boot_merge(&store, clock.clone());

            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )),
                "held:unreadable",
                "{label}: CONTROL: the corrupt row must quarantine the sender"
            );

            clock.advance(HORIZON_SECS + Duration::from_secs(1));
            assert_eq!(
                outcome(guard.check_replay_only(
                    &envelope(&sender, &canonical, 5),
                    ObservedSenderRegime::DurableV1,
                )),
                after_expiry,
                "{label}: the expiry clears the hold and must leave the floor's namespace \
                 exactly as the readable row set it"
            );
            assert_eq!(
                guard.sequences[&pk(&canonical)].floor_seq,
                10,
                "{label}: and the number itself must still be standing"
            );
        }
    }

    /// **J — `max_seq == 0` is not a proxy for "no numeric evidence".**
    ///
    /// The distinction the fix turns on is "a readable current-version row placed this number
    /// in the legacy namespace" versus "nothing established a namespace at all", and it must
    /// be carried by an explicit bit rather than by the number. A current-version row
    /// legitimately carries `max_seq == 0` — it is exactly what a completed promotion writes —
    /// so a zero cannot testify about its own provenance. Seeded here with a legacy tag, which
    /// is the adversarial half: read as "no evidence" it would resolve straight to an
    /// established durable-v1 sender on provenance alone.
    #[test]
    fn a_zero_legacy_high_water_is_still_explicit_legacy_evidence() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            0,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        // CONTROL: the row really is there and really says zero, so the assertion below is
        // about how a zero is read and not about a row that failed to load.
        assert_eq!(
            stored_max_seq(store.as_ref(), &canonical)
                .expect("CONTROL: the row must exist")
                .max_seq,
            0
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());
        let err = guard
            .check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("an explicit legacy row must be honoured whatever number it carries");
        assert!(
            err.downcast_ref::<SenderRegimeTransition>().is_some(),
            "expected the transition path; got: {err}"
        );

        // And the contrast that makes it a distinction rather than a blanket refusal: the same
        // provenance with genuinely *no* row resumes immediately — covered in full by
        // `provenance_without_a_high_water_resumes_durable_with_no_hold`, asserted here
        // side-by-side so the two inputs cannot drift apart.
        let bare = Arc::new(icn_store::SledStore::temporary().unwrap());
        let other = KeyPair::generate().unwrap();
        seed_provenance(
            bare.as_ref(),
            other.did(),
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );
        let mut bare_guard = boot_merge(&bare, MergeClock::new());
        assert_eq!(
            outcome(bare_guard.check_replay_only(
                &envelope(&other, other.did(), 5),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "CONTROL: absence of a row is not the same fact as a row saying legacy"
        );
    }

    /// **K — the property survives a restart, because it is re-derived and never latched.**
    ///
    /// The window is rebuilt from the rows on every load, so a receiver that restarts mid-hold
    /// restarts the *full* horizon rather than resuming a remembered deadline, and one that
    /// restarts after the promotion comes back as an ordinary durable-v1 sender with the
    /// legacy number gone from the store as well as from memory.
    #[test]
    fn the_cross_axis_resolution_is_rebuilt_from_the_store_on_every_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        // First boot, most of the way through the hold, then crash.
        let first_clock = MergeClock::new();
        let mut first = boot_merge(&store, first_clock.clone());
        assert_eq!(
            outcome(first.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition"
        );
        first_clock.advance(HORIZON_SECS - Duration::from_secs(1));
        drop(first);

        // Second boot on a fresh clock: the load re-derives the same disagreement and the
        // hold starts over. Nothing about the near-expired first hold survived.
        let second_clock = MergeClock::new();
        let mut second = boot_merge(&store, second_clock.clone());
        assert_eq!(
            second.sequences[&pk(&canonical)].floor_seq,
            10,
            "the legacy number is still on disk and still legacy"
        );
        assert_eq!(
            outcome(second.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "held:sender-regime-transition",
            "a restart restarts the full hold; it must not inherit the elapsed one"
        );

        // Complete the migration, then restart again.
        second_clock.advance(HORIZON_SECS + Duration::from_secs(1));
        assert_eq!(
            outcome(second.check_replay_only(
                &envelope(&sender, &canonical, 5),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted"
        );
        drop(second);

        let stored = stored_max_seq(store.as_ref(), &canonical).expect("the row survives");
        assert_eq!(
            stored.sender_regime, SENDER_REGIME_DURABLE_V1,
            "the promotion re-tagged the row, so the legacy number is gone from the store too"
        );

        let mut third = boot_merge(&store, MergeClock::new());
        let window = &third.sequences[&pk(&canonical)];
        assert_eq!(window.sender_regime, SenderRegimeState::DurableV1);
        assert!(window.hold.is_none(), "no second migration may be imposed");
        assert_eq!(
            outcome(third.check_replay_only(
                &envelope(&sender, &canonical, 6),
                ObservedSenderRegime::DurableV1,
            )),
            "accepted",
            "the sender resumes in its durable namespace with no further hold"
        );
    }

    /// **L — the ambiguous case must not be scored against the peer.**
    ///
    /// This is the whole cost of the defect, stated as a property. `handlers::signed` splits
    /// replay-guard errors into two classes: a set of typed local-state faults that are
    /// logged and dropped, and everything else, which records
    /// `icn_security::Violation::ReplayAttack` against `envelope.from`. `SenderRegimeTransition`
    /// is in the first set and a bare `Replay detected` is in the second, so relabelling the
    /// legacy floor did not merely refuse an honest peer's traffic — it accumulated
    /// misbehaviour severity against it for a state-reconstruction ambiguity that is entirely
    /// ours.
    #[test]
    fn an_unresolved_namespace_is_a_local_fault_and_never_a_scored_replay() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        seed_max_seq(
            store.as_ref(),
            &canonical,
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_LEGACY_OR_UNPROVEN,
        );
        seed_provenance(
            store.as_ref(),
            &canonical,
            &SENDER_REGIME_DURABLE_V1.to_be_bytes(),
        );

        let clock = MergeClock::new();
        let mut guard = boot_merge(&store, clock.clone());

        // Every sequence at or below the retained legacy floor — the whole range the defect
        // converted into scored replay verdicts.
        for sequence in 1..=10 {
            let err = guard
                .check_replay_only(
                    &envelope(&sender, &canonical, sequence),
                    ObservedSenderRegime::DurableV1,
                )
                .expect_err("every sequence at or below the retained legacy floor is held");
            assert!(
                err.downcast_ref::<SenderRegimeTransition>().is_some(),
                "sequence {sequence} must be a typed migration refusal; got: {err}"
            );
            assert!(
                !err.to_string().contains("Replay detected"),
                "sequence {sequence} must not reach the scoring branch of handlers::signed; \
                 got: {err}"
            );
        }

        // CONTROL: the classification is a property of this state, not of the error type
        // always being returned. An established durable sender really does produce the scored
        // form for the very same sequence.
        let clean = Arc::new(icn_store::SledStore::temporary().unwrap());
        let other = KeyPair::generate().unwrap();
        seed_max_seq(
            clean.as_ref(),
            other.did(),
            10,
            REPLAY_STATE_SEMANTIC_VERSION,
            SENDER_REGIME_DURABLE_V1,
        );
        let mut clean_guard = boot_merge(&clean, MergeClock::new());
        let scored = clean_guard
            .check_replay_only(
                &envelope(&other, other.did(), 5),
                ObservedSenderRegime::DurableV1,
            )
            .expect_err("a real durable replay must still be reported as one");
        assert!(
            scored.downcast_ref::<SenderRegimeTransition>().is_none()
                && scored.to_string().contains("Replay detected"),
            "CONTROL: a genuine replay against a proven durable floor is still scored; got: \
             {scored}"
        );
    }

    /// **M — a live acceptance records in memory exactly the namespace it persists.**
    ///
    /// The accept path writes the number's namespace twice: as the `sender_regime` field of
    /// the persisted `MaxSeqEntry`, and as the window's `numeric_namespace`. They are folded
    /// out of one `match` for that reason, and this pins that they cannot drift — a build that
    /// persisted `DURABLE_V1` while leaving the window's number tagged legacy would hold a
    /// window whose memory and disk disagree, and the disagreement would surface only at the
    /// next promotion, as a silently discarded durable floor.
    ///
    /// Asserted as state rather than through a rejection, deliberately. Today nothing
    /// downstream can tell the difference *within one process*: the only consumer is the
    /// promotion, and no in-memory path installs a `MigratingSenderRegime` hold on a window
    /// that is already established `DurableV1`. That makes the property one another layer
    /// currently supplies — the load pass re-derives it from the persisted tag on the next
    /// restart — which is precisely the kind of invariant that survives a mutation unless the
    /// state itself is asserted.
    #[test]
    fn an_accepted_number_records_the_same_namespace_it_persists() {
        for (label, observed, provenance, expected_tag, expected_namespace) in [
            (
                "durable sender resumed from provenance alone",
                ObservedSenderRegime::DurableV1,
                Some(SENDER_REGIME_DURABLE_V1),
                SENDER_REGIME_DURABLE_V1,
                NumericNamespace::DurableV1,
            ),
            (
                "unproven sender",
                ObservedSenderRegime::LegacyOrUnproven,
                None,
                SENDER_REGIME_LEGACY_OR_UNPROVEN,
                NumericNamespace::LegacyOrUnproven,
            ),
        ] {
            let store = Arc::new(icn_store::SledStore::temporary().unwrap());
            let sender = KeyPair::generate().unwrap();
            let canonical = sender.did().clone();
            if let Some(regime) = provenance {
                seed_provenance(store.as_ref(), &canonical, &regime.to_be_bytes());
            }

            let mut guard = boot_merge(&store, MergeClock::new());
            guard
                .check_replay_only(&envelope(&sender, &canonical, 5), observed)
                .unwrap_or_else(|e| panic!("{label}: this traffic must be accepted: {e}"));

            let window = &guard.sequences[&pk(&canonical)];
            assert_eq!(
                window.max_seq, 5,
                "{label}: CONTROL: the acceptance must have raised the high-water, otherwise \
                 neither half of the property was exercised"
            );
            assert_eq!(
                stored_max_seq(store.as_ref(), &canonical)
                    .unwrap_or_else(|| panic!("{label}: the acceptance must be durable"))
                    .sender_regime,
                expected_tag,
                "{label}: the persisted half"
            );
            assert_eq!(
                window.numeric_namespace, expected_namespace,
                "{label}: the in-memory half must be the same statement as the persisted tag"
            );
        }
    }
}
