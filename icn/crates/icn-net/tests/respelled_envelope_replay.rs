//! #2640 — a captured `SignedEnvelope` must not be replayable by re-spelling `from`.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! # The attack this file exists to keep closed
//!
//! `SignedEnvelope::canonical_encoding` covers `sequence ‖ timestamp ‖ payload_type ‖
//! payload` and **not** `from`, while `Did::from_str` accepts any of multibase's bases.
//! One Ed25519 key therefore has many textual `did:icn:` spellings, all accepted, all
//! decoding to the same key — so a party holding **no key material** can take a captured
//! envelope, rewrite only the spelling of `from`, leave the signature bytes untouched, and
//! have it verify. Before this fix it then received its own replay window and was accepted
//! a second time.
//!
//! # Why the equivalence class is exactly the decoded key bytes
//!
//! Ed25519 hashes the *encoded* public key into its challenge (`h = H(R ‖ A ‖ M)`), so a
//! `from` rewrite that changes the decoded 32 bytes cannot verify, and one that leaves them
//! unchanged is pure re-spelling. "Same decoded key" is therefore neither wider nor narrower
//! than "a signature valid under one spelling is valid under the other".
//!
//! # Non-vacuity
//!
//! Every test here first proves the two spellings are **distinct strings** that decode to
//! the **same key**, and that the *same-spelling* replay is rejected — so a green result
//! cannot come from the alias being unrepresentable, from the two DIDs being equal, or from
//! the guard being inert. If a future architecture (N2-A / #2627) rejects alternate spellings
//! at parse instead, [`alias_spelling`] records that as the rejection mechanism and asserts
//! it, rather than letting these tests pass by never building an alias.

use icn_identity::{Did, KeyPair};
use icn_net::envelope::{PayloadType, SignedEnvelope};
use icn_net::replay_guard::{ObservedSenderRegime, ReplayGuard};
use std::sync::Arc;

/// The base16-lower multibase spelling of the same key (`f` is its multibase code).
///
/// Returns `Err(the alias string)` if this spelling is no longer an accepted `Did`, which is
/// the N2-A "encoding pinned at parse" outcome. Callers assert that rejection explicitly so
/// the suite records *which* boundary refused the alias instead of silently passing.
fn alias_spelling(canonical: &Did) -> Result<Did, String> {
    let key_bytes = canonical.to_verifying_key().expect("canonical DID decodes");
    let hex: String = key_bytes
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let alias_str = format!("did:icn:f{hex}");
    Did::from_str(&alias_str).map_err(|_| alias_str)
}

/// Assert the controls that make every test below non-vacuous, and return the alias.
///
/// `None` means the alias is no longer parseable at all; the caller has already been given
/// proof (inside this function) that the rejection is at the parse boundary.
fn alias_or_recorded_parse_rejection(canonical: &Did) -> Option<Did> {
    let alias = match alias_spelling(canonical) {
        Ok(alias) => alias,
        Err(alias_str) => {
            // MECHANISM: rejected at `Did` parse. Prove it, and prove it is not merely
            // this helper refusing to build the string.
            assert!(
                Did::from_str(&alias_str).is_err(),
                "alias must be rejected at parse if it is rejected at all"
            );
            return None;
        }
    };

    assert_ne!(
        alias.as_str(),
        canonical.as_str(),
        "CONTROL: the alias must be a *different string*, or the test proves nothing"
    );
    assert_eq!(
        alias.to_verifying_key().unwrap().as_bytes(),
        canonical.to_verifying_key().unwrap().as_bytes(),
        "CONTROL: the alias must decode to the *same key*, or it is a different sender"
    );
    Some(alias)
}

/// Capture a validly-signed envelope and rewrite only the spelling of `from`.
///
/// This is the whole attacker capability: no key material, no re-signing.
fn respell(captured: &SignedEnvelope, alias: &Did) -> SignedEnvelope {
    let mut forged = captured.clone();
    forged.from = alias.clone();
    assert_eq!(
        forged.signature, captured.signature,
        "CONTROL: the attacker must not have touched the signature bytes"
    );
    assert!(
        forged.verify(3600).is_ok(),
        "CONTROL: the re-spelled envelope must still verify — if it did not, this test \
         would be proving something the signature layer already handles"
    );
    forged
}

/// The exact third-party attack from #2640, in memory.
#[test]
fn respelled_captured_envelope_does_not_get_a_second_replay_window() {
    let sender = KeyPair::generate().unwrap();
    let canonical = sender.did().clone();
    let Some(alias) = alias_or_recorded_parse_rejection(&canonical) else {
        return; // MECHANISM: parse. Asserted above.
    };

    let captured = SignedEnvelope::new(
        &canonical,
        &sender,
        9,
        PayloadType::Gossip,
        b"real-payload".to_vec(),
    )
    .unwrap();
    let forged = respell(&captured, &alias);

    let mut guard = ReplayGuard::new(300, 3600);
    guard
        .check(&captured, ObservedSenderRegime::LegacyOrUnproven)
        .expect("the genuine envelope must be accepted once");

    // CONTROL: the guard really is live for this sender and sequence.
    guard
        .check(&captured, ObservedSenderRegime::LegacyOrUnproven)
        .expect_err("same-spelling replay must be rejected — otherwise the guard is inert");

    // THE PROPERTY.
    guard
        .check(&forged, ObservedSenderRegime::LegacyOrUnproven)
        .expect_err(
            "#2640: a captured envelope whose `from` was only re-spelled must not obtain a \
             fresh replay window",
        );

    assert_eq!(
        guard.peer_count(),
        1,
        "one key must own exactly one replay window, whatever it is spelled"
    );
}

/// The same attack across a restart, through durable state.
#[test]
fn respelled_replay_is_still_rejected_after_a_restart() {
    let sender = KeyPair::generate().unwrap();
    let canonical = sender.did().clone();
    let Some(alias) = alias_or_recorded_parse_rejection(&canonical) else {
        return; // MECHANISM: parse.
    };
    let store = Arc::new(icn_store::SledStore::temporary().unwrap());

    let captured =
        SignedEnvelope::new(&canonical, &sender, 9, PayloadType::Gossip, b"m".to_vec()).unwrap();
    let forged = respell(&captured, &alias);

    {
        let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
        guard.load_persisted_state().unwrap();
        guard
            .check(&captured, ObservedSenderRegime::LegacyOrUnproven)
            .expect("genuine envelope accepted before the restart");
    }

    // Restart: a fresh guard over the same store.
    let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
    guard.load_persisted_state().unwrap();

    // CONTROL: the durable floor survived the restart for the canonical spelling.
    guard
        .check(&captured, ObservedSenderRegime::LegacyOrUnproven)
        .expect_err("same-spelling replay must still be rejected after a restart");

    // THE PROPERTY.
    guard
        .check(&forged, ObservedSenderRegime::LegacyOrUnproven)
        .expect_err("#2640: the re-spelled replay must not survive a restart either");
}

/// Over-canonicalization control: two genuinely different keys keep independent state.
#[test]
fn distinct_keys_keep_independent_replay_state() {
    let a = KeyPair::generate().unwrap();
    let b = KeyPair::generate().unwrap();
    assert_ne!(
        a.did().to_verifying_key().unwrap().as_bytes(),
        b.did().to_verifying_key().unwrap().as_bytes(),
        "CONTROL: the two keypairs must actually differ"
    );

    let mut guard = ReplayGuard::new(300, 3600);
    let ea = SignedEnvelope::new(a.did(), &a, 5, PayloadType::Gossip, b"a".to_vec()).unwrap();
    let eb = SignedEnvelope::new(b.did(), &b, 5, PayloadType::Gossip, b"b".to_vec()).unwrap();

    guard
        .check(&ea, ObservedSenderRegime::LegacyOrUnproven)
        .unwrap();
    guard
        .check(&eb, ObservedSenderRegime::LegacyOrUnproven)
        .expect("a second key's sequence 5 is not the first key's replay");

    assert_eq!(guard.peer_count(), 2, "distinct keys must not be merged");
    guard
        .check(&ea, ObservedSenderRegime::LegacyOrUnproven)
        .expect_err("A's own replay is still a replay");
    guard
        .check(&eb, ObservedSenderRegime::LegacyOrUnproven)
        .expect_err("B's own replay is still a replay");
}
