//! End-to-end replay behaviour across a sender restart (#2510).
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! These tests wire the real [`SigningSequenceCounter`] to the real [`ReplayGuard`] so
//! they exercise the actual production relationship: the sender allocates a signing
//! sequence, the receiving peer enforces a persisted high-water mark against it.
//!
//! The security controls here exist to keep a future refactor from "fixing" a restart
//! by clearing replay state on reconnect, which would reopen captured traffic.

use std::sync::Arc;

use icn_identity::KeyPair;
use icn_net::envelope::PayloadType;
use icn_net::replay_guard::{ObservedSenderRegime, ReplayGuard};
use icn_net::signing_sequence::SigningSequenceCounter;
use icn_net::SignedEnvelope;
use icn_store::Store;

/// One block of the counter's reservation, so tests cross a reservation boundary.
const OVER_ONE_BLOCK: usize = 1_500;

fn temp_store() -> Arc<dyn Store> {
    Arc::new(icn_store::SledStore::temporary().unwrap())
}

/// Every sender in this file is the real [`SigningSequenceCounter`], so every sender
/// here genuinely *is* durable. The receiver, however, has not yet completed the
/// #2517 first-establishment hold for it — and that is deliberately the state these
/// tests run in.
///
/// "Durable sender, not yet proven to this receiver" is a real and long-lived
/// condition: it is what every peer looks like for the first 600 seconds of contact,
/// and what a rolling upgrade looks like throughout. Replay protection must be fully
/// intact there — floor, Bloom filter, durable high-water, crash safety — and pinning
/// these tests to that state is what proves it. The post-establishment path is covered
/// by `replay_guard::sender_regime_tests`, which can drive the clock.
///
/// Spelling the attribution at each call site rather than defaulting it is deliberate:
/// the receiver may only treat a high-water as durable-v1 state when the sender has
/// proven that namespace, and a helper that hid the argument would let a future test
/// model the wrong thing silently.
const UNPROVEN: ObservedSenderRegime = ObservedSenderRegime::LegacyOrUnproven;

/// A receiver that persists replay state, as production does.
fn persistent_guard(store: Arc<dyn Store>) -> ReplayGuard {
    let mut guard = ReplayGuard::new_persistent(300, 3600, store);
    guard.load_persisted_state().unwrap();
    guard
}

fn envelope(keypair: &KeyPair, sequence: u64, body: &[u8]) -> SignedEnvelope {
    SignedEnvelope::new(
        keypair.did(),
        keypair,
        sequence,
        PayloadType::Gossip,
        body.to_vec(),
    )
    .unwrap()
}

/// The #2510 regression: a peer that restarts must not be scored as a replay attacker.
///
/// Fails on the pre-fix implementation, where the signing counter restarted at zero.
#[test]
fn restarted_sender_is_accepted_by_a_peer_that_remembers_it() {
    let sender_store = temp_store();
    let mut guard = persistent_guard(temp_store());
    let keypair = KeyPair::generate().unwrap();

    // Incarnation A: enough traffic to push the peer's high-water mark well past zero.
    {
        let counter = SigningSequenceCounter::new(sender_store.clone()).unwrap();
        for _ in 0..OVER_ONE_BLOCK {
            let seq = counter.next_sequence().unwrap();
            guard
                .check(&envelope(&keypair, seq, b"incarnation-a"), UNPROVEN)
                .expect("steady-state traffic must be accepted");
        }
    }

    let high_water = guard.get_max_seq(keypair.did()).unwrap();
    assert!(high_water >= OVER_ONE_BLOCK as u64 - 1);

    // Incarnation B: same DID, same disk, brand new process.
    let counter = SigningSequenceCounter::new(sender_store).unwrap();
    let seq = counter.next_sequence().unwrap();

    assert!(
        seq > high_water,
        "restarted sender issued {seq}, at or below the peer's high-water mark {high_water}"
    );
    guard
        .check(&envelope(&keypair, seq, b"incarnation-b"), UNPROVEN)
        .expect("a restarted peer's first message must not be scored as a replay attack");
}

/// Negative control for the fix itself: an ephemeral counter still reproduces #2510.
///
/// This is the pre-fix behaviour, kept executable so the suite proves it can tell the
/// broken implementation from the fixed one. If someone swaps `new()` for `in_memory()`
/// in the composition root, the test above fails and this one keeps passing.
#[test]
fn ephemeral_counter_reproduces_the_2510_defect() {
    let mut guard = persistent_guard(temp_store());
    let keypair = KeyPair::generate().unwrap();

    {
        let counter = SigningSequenceCounter::in_memory();
        for _ in 0..OVER_ONE_BLOCK {
            let seq = counter.next_sequence().unwrap();
            guard
                .check(&envelope(&keypair, seq, b"incarnation-a"), UNPROVEN)
                .ok();
        }
    }

    // "Restart" with no durable state: the counter returns to the start of its range,
    // far below the high-water mark the peer is holding.
    let counter = SigningSequenceCounter::in_memory();
    let seq = counter.next_sequence().unwrap();
    assert!(
        seq < guard.get_max_seq(keypair.did()).unwrap(),
        "precondition: the restarted counter must regress below the peer's high-water mark"
    );

    let result = guard.check(&envelope(&keypair, seq, b"incarnation-b"), UNPROVEN);
    assert!(
        result.is_err(),
        "an ephemeral counter must still be caught by replay protection - \
         this test documents the defect the durable counter fixes"
    );
}

/// Security control: the fix must not make captured traffic replayable.
///
/// A message from the previous incarnation, captured verbatim, stays rejected after the
/// sender restarts.
#[test]
fn captured_traffic_from_a_previous_incarnation_is_still_rejected() {
    let sender_store = temp_store();
    let mut guard = persistent_guard(temp_store());
    let keypair = KeyPair::generate().unwrap();

    let captured = {
        let counter = SigningSequenceCounter::new(sender_store.clone()).unwrap();
        let mut captured = Vec::new();
        for i in 0..OVER_ONE_BLOCK {
            let seq = counter.next_sequence().unwrap();
            let env = envelope(&keypair, seq, b"incarnation-a");
            guard.check(&env, UNPROVEN).unwrap();
            // Attacker records a sample of the traffic.
            if i % 500 == 0 {
                captured.push(env);
            }
        }
        captured
    };

    // Sender restarts and is accepted.
    let counter = SigningSequenceCounter::new(sender_store).unwrap();
    let seq = counter.next_sequence().unwrap();
    guard
        .check(&envelope(&keypair, seq, b"incarnation-b"), UNPROVEN)
        .unwrap();

    // Every captured message must still be refused.
    for env in &captured {
        assert!(
            guard.check(env, UNPROVEN).is_err(),
            "captured incarnation-A message at sequence {} was accepted after restart",
            env.sequence
        );
    }
}

/// Demonstrates why "clear the peer's replay state when it reconnects" is NOT a safe
/// fix for #2510, and must not be reintroduced as a simplification.
///
/// The tempting shortcut is to drop a peer's window on a fresh authenticated
/// connection. This test shows the cost directly: the *same* message that is correctly
/// refused while the window is held becomes acceptable once the window is dropped.
///
/// Note the residual exposure is bounded by the signed timestamp - `verify_age(300)`
/// rejects anything older than the freshness window regardless. That bound is what
/// makes the durable-counter fix sufficient without an incarnation field; it is *not*
/// a licence to clear state on reconnect, because everything inside those 300 seconds
/// becomes replayable.
#[test]
fn clearing_replay_state_on_reconnect_would_reopen_captured_traffic() {
    let keypair = KeyPair::generate().unwrap();
    let counter = SigningSequenceCounter::new(temp_store()).unwrap();

    let captured = {
        let mut guard = persistent_guard(temp_store());
        let env = envelope(&keypair, counter.next_sequence().unwrap(), b"captured");
        guard.check(&env, UNPROVEN).unwrap();
        assert!(
            guard.check(&env, UNPROVEN).is_err(),
            "precondition: while the window is held, the duplicate must be refused"
        );
        env
    };

    // Model the naive fix: a reconnect drops this peer's replay state, leaving a guard
    // with no memory of the DID. Everything else is unchanged.
    let mut cleared = persistent_guard(temp_store());

    assert!(
        cleared.check(&captured, UNPROVEN).is_ok(),
        "expected the cleared-state guard to accept previously-seen traffic; if this \
         ever fails the demonstration is stale, not fixed"
    );
}

/// Ordinary transport churn is not an incarnation boundary.
///
/// The replay guard is keyed by DID alone and holds no connection state, so reconnects
/// cannot reset it. Asserted behaviourally: after repeated reconnect-shaped activity, an
/// old sequence is still refused.
#[test]
fn reconnect_churn_does_not_reset_replay_state() {
    let sender_store = temp_store();
    let mut guard = persistent_guard(temp_store());
    let keypair = KeyPair::generate().unwrap();
    let counter = SigningSequenceCounter::new(sender_store).unwrap();

    let first = envelope(&keypair, counter.next_sequence().unwrap(), b"first");
    guard.check(&first, UNPROVEN).unwrap();

    // Ten "reconnects" within one incarnation: the sequence namespace is unchanged.
    for _ in 0..10 {
        let seq = counter.next_sequence().unwrap();
        guard
            .check(&envelope(&keypair, seq, b"same-incarnation"), UNPROVEN)
            .unwrap();

        assert!(
            guard.check(&first, UNPROVEN).is_err(),
            "replay state was reset by connection churn"
        );
    }
}

/// A receiver restart must keep rejecting pre-restart sequences.
///
/// This is the guard's own persistence path, unchanged by #2510; asserted here so the
/// durable-sender work cannot silently weaken it.
#[test]
fn receiver_restart_preserves_replay_protection() {
    let guard_store = temp_store();
    let sender_store = temp_store();
    let keypair = KeyPair::generate().unwrap();
    let counter = SigningSequenceCounter::new(sender_store).unwrap();

    let old = {
        let mut guard = persistent_guard(guard_store.clone());
        // Send several: the guard only persists max_seq when it advances, so a lone
        // message at sequence 0 would leave nothing on disk to reload.
        let first = envelope(&keypair, counter.next_sequence().unwrap(), b"before");
        guard.check(&first, UNPROVEN).unwrap();
        for _ in 0..4 {
            let env = envelope(&keypair, counter.next_sequence().unwrap(), b"before");
            guard.check(&env, UNPROVEN).unwrap();
        }
        first
    };

    // Receiver restarts, reloading persisted state.
    let mut guard = persistent_guard(guard_store);
    assert!(
        guard.check(&old, UNPROVEN).is_err(),
        "a pre-restart sequence was accepted after the receiver restarted"
    );
}

/// Replay state is per-DID: one identity cannot disturb another's window.
#[test]
fn traffic_from_one_did_does_not_reset_another_dids_state() {
    let mut guard = persistent_guard(temp_store());
    let alice = KeyPair::generate().unwrap();
    let mallory = KeyPair::generate().unwrap();
    let counter = SigningSequenceCounter::new(temp_store()).unwrap();

    let alice_msg = envelope(&alice, counter.next_sequence().unwrap(), b"alice");
    guard.check(&alice_msg, UNPROVEN).unwrap();

    // Mallory floods low sequences under her own DID.
    // (From 1: sequence 0 is always rejected, floor_seq starts at 0.)
    for seq in 1..50 {
        guard
            .check(&envelope(&mallory, seq, b"mallory"), UNPROVEN)
            .unwrap();
    }

    assert!(
        guard.check(&alice_msg, UNPROVEN).is_err(),
        "another DID's traffic reset Alice's replay window"
    );
}
