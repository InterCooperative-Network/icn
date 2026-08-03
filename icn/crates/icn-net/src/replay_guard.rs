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
//! - On restart, a safety gap is applied to prevent any edge cases
//!
//! # Architecture
//!
//! ```text
//! ReplayGuard (Persistent)
//!   ├── In-memory cache (HashMap<Did, SequenceWindow>)
//!   ├── Persistent store (Sled via icn-store)
//!   │   ├── replay_max_seq:<did> → max sequence number
//!   │   └── replay_finalized:<did>:<seq> → finalization timestamp
//!   └── Safety gap on restart (+1000)
//! ```

use crate::envelope::SignedEnvelope;
use anyhow::{bail, Context, Result};
use icn_gossip::BloomFilter;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// Persisted max sequence entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaxSeqEntry {
    /// Maximum sequence seen from this peer
    max_seq: u64,
    /// Timestamp of last update (for debugging/audit)
    updated_at_ms: u64,
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
    /// Last seen sequence per peer (in-memory cache)
    sequences: HashMap<Did, SequenceWindow>,

    /// Maximum allowed clock skew (seconds)
    max_clock_skew: u64,

    /// Maximum age before peer state is evicted (seconds)
    max_peer_age_secs: u64,

    /// Persistent storage backend (None for in-memory only)
    store: Option<Arc<dyn Store>>,

    /// Whether we've loaded persisted state and applied safety gap
    initialized: AtomicBool,
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

    /// Reject everything from this peer until this receiver-local instant.
    ///
    /// Used only when durable replay state for a peer exists but cannot be read,
    /// so the high-water is unknown and no sequence can be proven new. The
    /// quarantine runs for the signed-message validity bound, after which any
    /// envelope captured before the restart is too old to replay and normal
    /// service resumes.
    ///
    /// This is a *receiver-local elapsed duration* on the monotonic clock. It
    /// deliberately does not compare a sender-supplied timestamp against a
    /// receiver-side instant: those are different clock domains and, under the
    /// skew ICN tolerates, cannot order events across machines.
    quarantined_until: Option<Instant>,
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
        }
    }

    /// Load persisted state and apply restart safety gap
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

        let store = match &self.store {
            Some(s) => s,
            None => return Ok(0), // In-memory mode, nothing to load
        };

        let mut count = 0;

        // Peers whose durable state is unreadable are quarantined until every
        // envelope that could have been accepted before this restart is certain
        // to fail freshness. Receiver-local monotonic time; see
        // `envelope_validity_horizon` for the derivation.
        let quarantine_until = Instant::now() + self.envelope_validity_horizon();

        // Load max sequences
        let entries = store
            .scan(MAX_SEQ_PREFIX)
            .context("Failed to scan replay max_seq entries")?;

        for (key, value) in entries {
            if let Some(did) = Self::parse_max_seq_key(&key) {
                let parsed = serde_json::from_slice::<MaxSeqEntry>(&value);
                if let Err(ref e) = parsed {
                    // The key's existence proves we had state for this peer, but
                    // its high-water is unreadable, so no sequence can be shown
                    // to be new. Failing open would hand an attacker a replay
                    // window; instead reject this peer's traffic until anything
                    // captured before the restart is too old to be replayed.
                    let window = self
                        .sequences
                        .entry(did.clone())
                        .or_insert_with(SequenceWindow::new);
                    window.quarantined_until = Some(quarantine_until);
                    tracing::error!(
                        peer = %did,
                        error = %e,
                        quarantine_secs = self.envelope_validity_horizon().as_secs(),
                        "Corrupt replay state entry; quarantining this peer until captured \
                         traffic can no longer be fresh"
                    );
                }
                if let Ok(entry) = parsed {
                    let window = self
                        .sequences
                        .entry(did.clone())
                        .or_insert_with(SequenceWindow::new);

                    // The floor is the durable high-water, which — because the
                    // high-water is flushed before acceptance is returned — is
                    // exactly the highest sequence ever accepted. Everything
                    // accepted before the crash is at or below it and the
                    // sender's next emission is above it, so this rejects all of
                    // the former and none of the latter. The Bloom filter is
                    // empty after restart, which is why the floor carries
                    // pre-restart replay rejection.
                    window.max_seq = entry.max_seq;
                    window.floor_seq = entry.max_seq;

                    tracing::debug!(
                        peer = %did,
                        max_seq = entry.max_seq,
                        floor_seq = entry.max_seq,
                        "Loaded replay guard state"
                    );

                    count += 1;
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
                if let Ok(entry) = serde_json::from_slice::<FinalizedEntry>(&value) {
                    // Only load finalized sequences less than 24h old
                    if entry.finalized_at_ms >= cutoff_ms {
                        // Only attach finalized entries to windows that were already
                        // initialized from max_seq (with safety gap and floor applied).
                        // This avoids creating new windows with floor_seq=0 based solely
                        // on finalized state, which could allow replay of older sequences.
                        if let Some(window) = self.sequences.get_mut(&did) {
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
    pub fn check(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        // 1. Verify signature and age
        envelope.verify(self.max_clock_skew)?;

        // 2. Perform replay detection (signature already verified, so use check_replay_only)
        self.check_replay_only(envelope)
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
    pub fn check_replay_only(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        // Ensure initialized for persistent mode
        if !self.initialized.load(Ordering::Acquire) {
            self.load_persisted_state()?;
        }

        // Note: Signature verification is SKIPPED - caller must have already verified

        // Get or create sequence window for this sender
        let window = self
            .sequences
            .entry(envelope.from.clone())
            .or_insert_with(SequenceWindow::new);

        // Quarantine (CRITICAL: durable state for this peer was unreadable, so
        // no sequence can be proven new). Receiver-local elapsed time only.
        //
        // Released strictly *after* the horizon, not at it: an envelope stamped
        // at the positive-skew limit has age exactly `max_age` at the horizon,
        // and `age > max_age` is false there — it is still valid for that
        // instant. See `envelope_validity_horizon`.
        if let Some(until) = window.quarantined_until {
            if Instant::now() <= until {
                bail!(
                    "Rejecting {} sequence {}: durable replay state for this peer was \
                     unreadable at startup, so the accepted high-water is unknown",
                    envelope.from.as_str(),
                    envelope.sequence
                );
            }
            window.quarantined_until = None;
        }

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
            if let Err(e) = self.persist_max_seq_durable(&envelope.from, envelope.sequence) {
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
            .entry(envelope.from.clone())
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
        let window = self
            .sequences
            .get_mut(sender)
            .context("Cannot finalize sequence for unknown sender")?;

        window.finalized.insert(sequence, Instant::now());

        // Persist finalized sequence
        if let Err(e) = self.persist_finalized(sender, sequence) {
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
    pub fn is_finalized(&self, sender: &Did, sequence: u64) -> bool {
        self.sequences
            .get(sender)
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

        // Collect DIDs to remove from storage
        let mut dids_to_remove: Vec<Did> = Vec::new();

        // Remove inactive peer windows
        self.sequences.retain(|did, window| {
            let keep = now.duration_since(window.last_update) < max_age;
            if !keep {
                dids_to_remove.push(did.clone());
            }
            keep
        });

        // Delete from storage
        if let Some(ref store) = self.store {
            for did in &dids_to_remove {
                let key = Self::make_max_seq_key(did);
                if let Err(e) = store.delete(&key) {
                    tracing::warn!(peer = %did, error = %e, "Failed to delete max_seq from storage");
                }
            }
        }

        // Prune old finalized sequences from remaining windows
        let cutoff_ms = Self::current_time_ms().saturating_sub(24 * 60 * 60 * 1000);

        for (did, window) in self.sequences.iter_mut() {
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
                    let key = Self::make_finalized_key(did, *seq);
                    if let Err(e) = store.delete(&key) {
                        tracing::warn!(
                            peer = %did,
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
    pub fn get_max_seq(&self, did: &Did) -> Option<u64> {
        self.sequences.get(did).map(|w| w.max_seq)
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
    fn persist_max_seq_durable(&self, did: &Did, max_seq: u64) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode: no durability to promise
        };

        self.persist_max_seq_inner(did, max_seq)?;
        store
            .flush()
            .context("Failed to flush replay high-water to durable storage")?;
        Ok(())
    }

    fn persist_max_seq_inner(&self, did: &Did, max_seq: u64) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode
        };

        let key = Self::make_max_seq_key(did);
        let entry = MaxSeqEntry {
            max_seq,
            updated_at_ms: Self::current_time_ms(),
        };
        let value = serde_json::to_vec(&entry).context("Failed to serialize max_seq entry")?;

        store
            .put(&key, &value)
            .context("Failed to persist max_seq")?;

        Ok(())
    }

    fn persist_finalized(&self, did: &Did, sequence: u64) -> Result<()> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()), // In-memory mode
        };

        let key = Self::make_finalized_key(did, sequence);
        let entry = FinalizedEntry {
            finalized_at_ms: Self::current_time_ms(),
        };
        let value = serde_json::to_vec(&entry).context("Failed to serialize finalized entry")?;

        store
            .put(&key, &value)
            .context("Failed to persist finalized sequence")?;

        Ok(())
    }

    fn make_max_seq_key(did: &Did) -> Vec<u8> {
        let mut key = Vec::with_capacity(MAX_SEQ_PREFIX.len() + 100);
        key.extend_from_slice(MAX_SEQ_PREFIX);
        key.extend_from_slice(did.as_str().as_bytes());
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

    fn make_finalized_key(did: &Did, sequence: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(FINALIZED_PREFIX.len() + 120);
        key.extend_from_slice(FINALIZED_PREFIX);
        key.extend_from_slice(did.as_str().as_bytes());
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
            quarantined_until: None,
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
        assert!(guard.check(&envelope).is_ok());
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
        assert!(guard.check(&envelope).is_ok());

        // Replay: Rejected
        let result = guard.check(&envelope);
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

            assert!(guard.check(&envelope).is_ok());
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
        assert!(guard.check(&env3).is_ok());
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
        assert!(guard.check(&env2).is_ok());

        // Try to replay sequence 2 (should be rejected)
        let result = guard.check(&env2);
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
        assert!(guard.check(&env1).is_ok());

        // Send seq 1 from peer 2 (different peer, should be OK)
        let env2 = SignedEnvelope::new(
            keypair2.did(),
            &keypair2,
            1,
            PayloadType::Gossip,
            b"peer2-msg1".to_vec(),
        )
        .unwrap();
        assert!(guard.check(&env2).is_ok());

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

        assert!(guard.check(&envelope).is_ok());
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

        // Should be rejected due to signature mismatch (before sequence check)
        let result = guard.check(&envelope);
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
        assert!(guard.check(&envelope).is_ok());

        // Finalize sequence (transaction processed)
        assert!(guard.finalize(keypair.did(), 1).is_ok());
        assert!(guard.is_finalized(keypair.did(), 1));

        // Attempt replay after finalization: REJECTED
        let result = guard.check(&envelope);
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
        assert!(guard.check(&envelope1).is_ok());
        assert!(guard.check(&envelope2).is_ok());

        // Finalize sequence 1 only
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // Sequence 1 blocked (finalized)
        assert!(guard.check(&envelope1).is_err());

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

        assert!(guard.check(&envelope3).is_ok());

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
        assert!(guard.check(&envelope).is_ok());

        // T=1: Transaction processed, finalize
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // T=2: Attacker replays within 5-minute window
        // WITHOUT finalization: would be accepted (vulnerability)
        // WITH finalization: REJECTED (fixed)
        let result = guard.check(&envelope);
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

        assert!(guard.check(&envelope).is_ok());
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
                guard.check(&envelope).is_ok(),
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
        assert!(guard.check(&new_envelope).is_ok());

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
        let result = guard.check(&old_envelope);
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
                assert!(guard.check(&envelope).is_ok());
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
            let result = guard.check(&old_envelope);
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
            assert!(guard.check(&new_envelope).is_ok());
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

            assert!(guard.check(&envelope).is_ok());
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
            let result = guard.check(&replay_envelope);
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
        let key = ReplayGuard::make_max_seq_key(&did);
        let parsed = ReplayGuard::parse_max_seq_key(&key).unwrap();
        assert_eq!(parsed.as_str(), did.as_str());

        // Finalized key
        let seq = 12345u64;
        let fkey = ReplayGuard::make_finalized_key(&did, seq);
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

        assert!(guard.check(&envelope).is_ok());
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
        let result = guard.check(&envelope);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("signature"),
            "Should fail signature verification"
        );

        // But check_replay_only() should succeed because it skips signature
        // (caller is responsible for verifying signature first)
        assert!(
            guard.check_replay_only(&envelope).is_ok(),
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
        assert!(guard.check_replay_only(&envelope).is_ok());

        // Replay via check_replay_only: Rejected
        let result = guard.check_replay_only(&envelope);
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
        assert!(guard.check_replay_only(&envelope).is_ok());

        // Finalize
        assert!(guard.finalize(keypair.did(), 1).is_ok());

        // Replay of finalized sequence: Rejected
        let result = guard.check_replay_only(&envelope);
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
        assert!(guard.check(&envelope1).is_ok());

        // Use check_replay_only() for second envelope
        assert!(guard.check_replay_only(&envelope2).is_ok());

        // Both sequences should now be tracked
        assert_eq!(guard.get_max_seq(keypair.did()), Some(2));

        // Replaying envelope1 via check_replay_only should fail
        let result1 = guard.check_replay_only(&envelope1);
        assert!(result1.is_err());
        assert!(result1.unwrap_err().to_string().contains("Replay detected"));

        // Replaying envelope2 via check() should also fail
        let result2 = guard.check(&envelope2);
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
        assert!(guard.check(&envelopes[0]).is_ok()); // seq 1 via check()
        assert!(guard.check_replay_only(&envelopes[1]).is_ok()); // seq 2 via check_replay_only()
        assert!(guard.check(&envelopes[2]).is_ok()); // seq 3 via check()
        assert!(guard.check_replay_only(&envelopes[3]).is_ok()); // seq 4 via check_replay_only()
        assert!(guard.check(&envelopes[4]).is_ok()); // seq 5 via check()

        // All 5 sequences should be tracked
        assert_eq!(guard.get_max_seq(keypair.did()), Some(5));

        // All should be rejected as replays regardless of which method is used
        for (i, envelope) in envelopes.iter().enumerate() {
            let result = if i % 2 == 0 {
                guard.check(envelope)
            } else {
                guard.check_replay_only(envelope)
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

    /// Overwrite the durable max_seq for a peer, simulating a crash that lost
    /// unflushed writes: the receiver accepted up to `accepted`, but only
    /// `durable` reached disk.
    fn force_durable_max_seq(store: &Arc<icn_store::SledStore>, did: &Did, durable: u64) {
        let key = ReplayGuard::make_max_seq_key(did);
        let entry = MaxSeqEntry {
            max_seq: durable,
            updated_at_ms: ReplayGuard::current_time_ms(),
        };
        store
            .put(&key, &serde_json::to_vec(&entry).unwrap())
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
                assert!(guard.check(&env).is_ok());
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

        let result = guard.check(&next);
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
            assert!(guard.check(&env).is_ok());
        }

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(
            guard.get_max_seq(sender.did()),
            Some(42),
            "restart must not advance max_seq past what was accepted"
        );
        assert_eq!(
            guard.sequences.get(sender.did()).unwrap().floor_seq,
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
            assert!(guard.check(&env).is_ok());
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
        assert!(guard.check(&env).is_ok());
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
            assert!(guard.check(&captured).is_ok());
        }

        // Make the restart strictly later than the captured envelope's timestamp.
        std::thread::sleep(std::time::Duration::from_millis(5));

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert!(
            guard.check(&captured).is_err(),
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
        assert!(guard.check(&env).is_ok());

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
            .check(&env)
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
                assert!(guard.check(&env).is_ok());
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
                guard.check(env).is_err(),
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
            guard.check(&next).is_ok(),
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
                assert!(guard.check(&env).is_ok());
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
            guard.check(captured.as_ref().unwrap()).is_ok(),
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
            assert!(guard.check(&env).is_ok());
        }

        std::thread::sleep(std::time::Duration::from_millis(5));

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // Signed now, i.e. after the restart.
        let fresh =
            SignedEnvelope::new(sender.did(), &sender, 6, PayloadType::Gossip, b"m".to_vec())
                .unwrap();
        assert!(
            guard.check(&fresh).is_ok(),
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
            assert!(guard.check(&ea).is_ok());
            assert!(guard.check(&eb).is_ok());
        }

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        assert_eq!(guard.get_max_seq(a.did()), Some(100));
        assert_eq!(guard.get_max_seq(b.did()), Some(3));

        // Each peer's next sequence is accepted independently.
        let na = SignedEnvelope::new(a.did(), &a, 101, PayloadType::Gossip, b"a".to_vec()).unwrap();
        let nb = SignedEnvelope::new(b.did(), &b, 4, PayloadType::Gossip, b"b".to_vec()).unwrap();
        assert!(guard.check(&na).is_ok());
        assert!(guard.check(&nb).is_ok());
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
            assert!(guard.check(&captured).is_ok());
        }

        // Corrupt the persisted entry.
        let key = ReplayGuard::make_max_seq_key(sender.did());
        store.put(&key, b"{not valid json").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));

        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        // Load must not panic or error out.
        guard.load_persisted_state().unwrap();

        assert!(
            guard.check(&captured).is_err(),
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
                    assert!(guard.check_replay_only(&env).is_ok());
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

            let result = guard.check(&legit);
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
                assert!(guard.check_replay_only(&env).is_ok());
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
                guard.check(env).is_err(),
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
            assert!(guard.check_replay_only(&captured).is_ok());
        }

        // Durable state is unreadable, so there is no floor to fall back on.
        store
            .put(&ReplayGuard::make_max_seq_key(sender.did()), b"{corrupt")
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
            guard.check(&captured).is_err(),
            "quarantine released after a single max_age and accepted a replay"
        );

        // Past the full horizon it can no longer pass freshness at all.
        std::thread::sleep(Duration::from_millis(max_age * 1000 + 200));
        assert!(
            captured.verify(max_age).is_err(),
            "past the horizon the captured envelope must fail freshness"
        );
        assert!(
            guard.check(&captured).is_err(),
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
            assert!(guard.check_replay_only(&env).is_ok());
        }
        store
            .put(&ReplayGuard::make_max_seq_key(sender.did()), b"{corrupt")
            .unwrap();

        let mut guard = ReplayGuard::new_persistent(max_age, 3600, store.clone());
        guard.load_persisted_state().unwrap();

        // Fresh traffic during the quarantine is rejected...
        let during =
            SignedEnvelope::new(sender.did(), &sender, 4, PayloadType::Gossip, b"m".to_vec())
                .unwrap();
        assert!(guard.check(&during).is_err());

        // ...and accepted once the horizon has fully elapsed.
        std::thread::sleep(Duration::from_millis(2 * max_age * 1000 + 300));
        let after =
            SignedEnvelope::new(sender.did(), &sender, 5, PayloadType::Gossip, b"m".to_vec())
                .unwrap();
        assert!(
            guard.check(&after).is_ok(),
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
        assert!(guard.check(&env).is_ok());
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
            assert!(guard.check(&replay).is_err(), "replay must be rejected");
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
            assert!(guard.check(&env).is_ok());
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
        assert!(guard.check(&captured).is_ok());

        // Outlive both the freshness bound and the peer age.
        std::thread::sleep(std::time::Duration::from_millis(1200));
        guard.cleanup();
        assert_eq!(guard.peer_count(), 0, "peer should have been forgotten");

        let result = guard.check(&captured);
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
