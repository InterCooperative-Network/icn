//! Durable signing-sequence counter for outbound [`SignedEnvelope`]s.
//!
//! # Why this exists (issue #2510)
//!
//! ICN has **two distinct outbound sequence spaces**, and conflating them caused a
//! security-lifecycle defect:
//!
//! | | encryption nonce sequence | signing sequence (this module) |
//! |---|---|---|
//! | producer | [`OutgoingSequenceTracker`] | [`SigningSequenceCounter`] |
//! | scope | per `(sender, recipient)` pair | per sender, shared across all recipients |
//! | consumed by | ChaCha20-Poly1305 nonce derivation | `ReplayGuard` on the receiving node |
//!
//! Only the *signing* sequence is checked by the peer's replay guard. That guard
//! persists its per-peer high-water mark to disk and reloads it at startup, so it
//! assumes the sender's counter is durable and monotonic. Before this module, the
//! signing counter was a process-local `AtomicU64::new(0)`: every restart reset it to
//! zero, and surviving peers scored the restarted node's legitimate traffic as a replay
//! attack at severity 1.0.
//!
//! The receiver was built against a durable sender that did not exist. This module
//! supplies the missing half; it deliberately changes no wire format.
//!
//! # Durability strategy: block reservation
//!
//! [`OutgoingSequenceTracker`] persists on every increment, which is affordable because
//! it only runs on the encrypted *unicast* path. The signing sequence is consumed by
//! **every** outbound gossip message, including broadcast, so a disk write per message
//! would be a hot-path regression.
//!
//! Instead we reserve sequences in blocks. The store always holds `reserved_end`, an
//! exclusive upper bound on the reservation:
//!
//! ```text
//! issued sequences  [.....next.....)  reserved_end
//!                   ^ handed out      ^ persisted watermark
//! ```
//!
//! The durable invariant is:
//!
//! > **every sequence ever issued is strictly less than the persisted `reserved_end`.**
//!
//! The watermark is written *before* any sequence in the new block is handed out, so a
//! crash at any point resumes at or above the highest sequence that could have reached
//! the network. Crashing mid-block simply skips the unused remainder; gaps are harmless
//! because the replay guard tracks a high-water mark rather than strict succession.
//!
//! [`OutgoingSequenceTracker`]: crate::sequence_tracker::OutgoingSequenceTracker
//! [`SignedEnvelope`]: crate::envelope::SignedEnvelope

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};

use icn_store::Store;

/// Number of sequences claimed per durable reservation.
///
/// Trades restart-time sequence skip against write amplification: one store write per
/// 1,000 outbound messages, and at most 1,000 sequences skipped per restart. At u64
/// width the sequence space tolerates ~1.8e16 restarts at this block size.
const RESERVATION_BLOCK: u64 = 1_000;

/// Storage key holding the exclusive upper bound of the current reservation.
const SIGNING_SEQUENCE_KEY: &[u8] = b"outgoing_signing_seq_reserved_end";

/// First sequence a fresh counter issues.
///
/// Sequence 0 is unusable: `ReplayGuard` rejects `sequence <= floor_seq` and a fresh
/// per-peer window starts at `floor_seq = 0`, so a message numbered 0 is always scored
/// as a replay. The previous `AtomicU64::new(0)` + `fetch_add(1)` returned the *old*
/// value, so every node's first gossip message was silently dropped by every peer.
const FIRST_SEQUENCE: u64 = 1;

/// Mutable counter state.
///
/// Sequences in `[next, reserved_end)` are safe to hand out without touching disk.
struct State {
    /// Next sequence to issue.
    next: u64,
    /// Exclusive upper bound of the durably reserved range.
    reserved_end: u64,
}

/// Per-sender monotonic sequence source for outbound signed envelopes.
///
/// Monotonic across process restarts when constructed with [`Self::new`]. Use
/// [`Self::in_memory`] only for tests and genuinely ephemeral nodes: an in-memory
/// counter restarts from the bottom of the sequence space, which is exactly the
/// condition that peers score as a replay attack.
pub struct SigningSequenceCounter {
    state: Mutex<State>,
    store: Option<Arc<dyn Store>>,
}

impl SigningSequenceCounter {
    /// Open a durable counter, resuming past every previously issued sequence.
    ///
    /// Reads the persisted reservation watermark and resumes there. Because the
    /// watermark is always written before the block it covers is used, the first
    /// sequence issued after a restart is guaranteed to exceed every sequence issued by
    /// any prior incarnation.
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let reserved_end = match store
            .get(SIGNING_SEQUENCE_KEY)
            .context("Failed to read persisted signing sequence watermark")?
        {
            Some(bytes) => {
                let raw: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
                    anyhow!(
                        "Corrupt signing sequence watermark: expected 8 bytes, found {}",
                        bytes.len()
                    )
                })?;
                u64::from_be_bytes(raw)
            }
            None => 0,
        };

        tracing::info!(
            resumed_at = reserved_end,
            block = RESERVATION_BLOCK,
            "Loaded durable signing sequence counter"
        );

        Ok(SigningSequenceCounter {
            // Resume at the watermark: everything below it may already be on the wire.
            // A fresh counter starts at FIRST_SEQUENCE, not zero - see its docs.
            state: Mutex::new(State {
                next: reserved_end.max(FIRST_SEQUENCE),
                reserved_end,
            }),
            store: Some(store),
        })
    }

    /// Create a non-durable counter for tests and ephemeral nodes.
    ///
    /// **Warning**: restarts reset this to [`FIRST_SEQUENCE`], far below any high-water
    /// mark a peer is holding, which surviving peers classify as a replay attack
    /// (#2510). Never use on a node with persistent storage.
    pub fn in_memory() -> Self {
        SigningSequenceCounter {
            state: Mutex::new(State {
                next: FIRST_SEQUENCE,
                reserved_end: 0,
            }),
            store: None,
        }
    }

    /// Issue the next sequence number, extending the durable reservation if needed.
    ///
    /// Returns an error if the reservation cannot be persisted. Callers must treat that
    /// as fatal for the message: issuing a sequence we failed to record would let a
    /// later incarnation reuse it.
    ///
    /// # Why this is synchronous
    ///
    /// The store write happens under the lock, on the caller's thread, rather than via
    /// `spawn_blocking`. That is deliberate: correctness here depends on "persist, then
    /// issue from the new block" being atomic. Releasing the lock to await a blocking
    /// task would let two senders each observe an exhausted reservation and both extend
    /// it, which is precisely the reuse this type exists to prevent.
    ///
    /// The cost is bounded by amortisation - one write per [`RESERVATION_BLOCK`]
    /// messages, not one per message. The sibling `OutgoingSequenceTracker` already
    /// persists synchronously from async on *every* encrypted message.
    pub fn next_sequence(&self) -> Result<u64> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Signing sequence counter mutex poisoned"))?;

        if state.next >= state.reserved_end {
            let new_end = state.next.checked_add(RESERVATION_BLOCK).context(
                "Signing sequence space exhausted (requires 2^64 messages); \
                 node identity should be rotated",
            )?;

            // Persist BEFORE issuing anything from the new block, so a crash between
            // here and the send resumes above every sequence that could have escaped.
            // The flush is what makes that claim true rather than aspirational: an
            // unflushed watermark lost to power failure would let the next incarnation
            // reissue sequences this one already put on the wire.
            if let Some(ref store) = self.store {
                store
                    .put(SIGNING_SEQUENCE_KEY, &new_end.to_be_bytes())
                    .context("Failed to persist signing sequence reservation")?;
                store
                    .flush()
                    .context("Failed to flush signing sequence reservation")?;
            }

            state.reserved_end = new_end;
        }

        let sequence = state.next;
        state.next += 1;
        Ok(sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> Arc<dyn Store> {
        Arc::new(icn_store::SledStore::temporary().unwrap())
    }

    #[test]
    fn issues_monotonically_within_one_incarnation() {
        let counter = SigningSequenceCounter::new(temp_store()).unwrap();

        let first = counter.next_sequence().unwrap();
        for expected in 1..2_500u64 {
            assert_eq!(counter.next_sequence().unwrap(), first + expected);
        }
    }

    /// The #2510 invariant: a restart must not reissue a consumed sequence.
    #[test]
    fn never_reissues_a_sequence_across_restart() {
        let store = temp_store();
        let mut issued = Vec::new();

        // Three incarnations against the same store, each crossing block boundaries.
        for _ in 0..3 {
            let counter = SigningSequenceCounter::new(store.clone()).unwrap();
            for _ in 0..1_500 {
                issued.push(counter.next_sequence().unwrap());
            }
        }

        let mut sorted = issued.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), issued.len(), "a sequence was reissued");

        // Strictly increasing overall, so a peer's high-water mark is never revisited.
        assert!(
            issued.windows(2).all(|w| w[1] > w[0]),
            "sequence regressed across a restart"
        );
    }

    /// A crash mid-block must not replay the unused remainder of that block.
    #[test]
    fn crash_mid_block_does_not_reuse_reserved_sequences() {
        let store = temp_store();

        let before = {
            let counter = SigningSequenceCounter::new(store.clone()).unwrap();
            // Consume one sequence, abandoning the rest of the reserved block.
            counter.next_sequence().unwrap()
        };

        let after = SigningSequenceCounter::new(store.clone())
            .unwrap()
            .next_sequence()
            .unwrap();

        assert!(
            after >= before + RESERVATION_BLOCK,
            "resumed at {after}, inside the block reserved by the previous incarnation \
             starting at {before}"
        );
    }

    /// Guards the durability contract itself: the watermark must be written before the
    /// block it covers is handed out, never lazily afterwards.
    #[test]
    fn watermark_is_persisted_before_the_block_is_used() {
        let store = temp_store();
        let counter = SigningSequenceCounter::new(store.clone()).unwrap();

        let issued = counter.next_sequence().unwrap();

        let raw = store
            .get(SIGNING_SEQUENCE_KEY)
            .unwrap()
            .expect("watermark must exist immediately after the first issue");
        let persisted = u64::from_be_bytes(raw.as_slice().try_into().unwrap());

        assert!(
            persisted > issued,
            "persisted watermark {persisted} does not cover issued sequence {issued}"
        );
    }

    #[test]
    fn in_memory_counter_starts_at_the_first_usable_sequence() {
        let counter = SigningSequenceCounter::in_memory();
        assert_eq!(counter.next_sequence().unwrap(), FIRST_SEQUENCE);
        assert_eq!(counter.next_sequence().unwrap(), FIRST_SEQUENCE + 1);
    }

    /// Sequence 0 is always rejected by `ReplayGuard` (`sequence <= floor_seq`, floor
    /// starts at 0), so it must never be issued by either constructor.
    #[test]
    fn never_issues_the_unusable_zero_sequence() {
        assert_ne!(
            SigningSequenceCounter::in_memory().next_sequence().unwrap(),
            0
        );
        assert_ne!(
            SigningSequenceCounter::new(temp_store())
                .unwrap()
                .next_sequence()
                .unwrap(),
            0
        );
    }

    #[test]
    fn corrupt_watermark_is_rejected_rather_than_silently_reset() {
        let store = temp_store();
        store.put(SIGNING_SEQUENCE_KEY, b"garbage").unwrap();

        // Silently restarting at zero here would recreate #2510 on a corrupt store.
        assert!(SigningSequenceCounter::new(store).is_err());
    }
}
