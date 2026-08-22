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
//! # Coverage: the whole equivalence class, not one example
//!
//! `multibase::Base` declares 24 bases. `Identity` cannot be applied to a typical Ed25519 key
//! at all — `multibase::encode` panics when the bytes are not valid UTF-8 — so 23 remain, of
//! which base58btc is what `Did::from_public_key` emits. That leaves [`ALTERNATE_SPELLINGS`]:
//! **22 distinct textual spellings of every key**, each one a usable `from` for this attack.
//! The suite drives all 22, so no single arbitrary example is what stands between the repo and
//! a regression. This is the same count the N2-A0 inventory measured against `Did` itself
//! (`docs/architecture/n2-a0-stored-key-inventory.md`).
//!
//! # Non-vacuity
//!
//! Every test proves the spellings are **distinct strings** that decode to the **same key**,
//! and that the *same-spelling* replay is rejected — so a green result cannot come from the
//! alias being unrepresentable, from the DIDs being equal, or from the guard being inert.
//!
//! Crucially, a spelling that stops parsing is a **failure**, not a skip. An earlier version of
//! this file returned `None` when the alias no longer parsed and let its callers return green,
//! so tightening `Did::from_str` would have retired the replay coverage silently. Under current
//! policy every base in [`ALTERNATE_SPELLINGS`] must parse. If a future architecture (N2-A /
//! #2627) deliberately pins encodings at parse instead, that is a real behaviour change and it
//! must be made here explicitly — by moving the affected bases out of this list and asserting
//! their rejection in [`unparseable_spellings_would_close_the_attack_at_the_parse_boundary`],
//! which shows what that alternative closure looks like using an input that is *already*
//! rejected today.

use icn_identity::{Did, KeyPair};
use icn_net::envelope::{PayloadType, SignedEnvelope};
use icn_net::replay_guard::{ObservedSenderRegime, ReplayGuard};
use std::sync::Arc;

/// Every base that can spell an Ed25519 key *other than* the canonical base58btc.
///
/// `Identity` is absent because `multibase::encode` panics on it for non-UTF-8 bytes, which is
/// a property of the key rather than of `Did`; `Base58Btc` is absent because it is the
/// canonical spelling that `Did::from_public_key` emits, and re-spelling to it is a no-op.
/// Everything else the crate declares is here, and all of it is accepted by `Did::from_str`
/// today — [`the_alternate_spellings_are_exactly_the_class_the_parser_accepts`] is the proof,
/// and it fails if any entry stops parsing rather than quietly covering less.
const ALTERNATE_SPELLINGS: [(&str, multibase::Base); 22] = [
    ("base2", multibase::Base::Base2),
    ("base8", multibase::Base::Base8),
    ("base10", multibase::Base::Base10),
    ("base16-lower", multibase::Base::Base16Lower),
    ("base16-upper", multibase::Base::Base16Upper),
    ("base32-lower", multibase::Base::Base32Lower),
    ("base32-upper", multibase::Base::Base32Upper),
    ("base32-pad-lower", multibase::Base::Base32PadLower),
    ("base32-pad-upper", multibase::Base::Base32PadUpper),
    ("base32-hex-lower", multibase::Base::Base32HexLower),
    ("base32-hex-upper", multibase::Base::Base32HexUpper),
    ("base32-hex-pad-lower", multibase::Base::Base32HexPadLower),
    ("base32-hex-pad-upper", multibase::Base::Base32HexPadUpper),
    ("base32-z", multibase::Base::Base32Z),
    ("base36-lower", multibase::Base::Base36Lower),
    ("base36-upper", multibase::Base::Base36Upper),
    ("base58-flickr", multibase::Base::Base58Flickr),
    ("base64", multibase::Base::Base64),
    ("base64-pad", multibase::Base::Base64Pad),
    ("base64-url", multibase::Base::Base64Url),
    ("base64-url-pad", multibase::Base::Base64UrlPad),
    ("base256-emoji", multibase::Base::Base256Emoji),
];

/// Re-spell one key under one base, and assert the controls that make the result meaningful.
///
/// Panics if the spelling no longer parses. That is deliberate: under current policy every
/// base in [`ALTERNATE_SPELLINGS`] is accepted, so a parse failure is a change in behaviour
/// that must be seen, not a reason to skip a replay assertion. See the module header.
fn alias_in(base: multibase::Base, label: &str, canonical: &Did) -> Did {
    let key_bytes = canonical.to_verifying_key().expect("canonical DID decodes");
    let alias_str = format!("did:icn:{}", multibase::encode(base, key_bytes.as_bytes()));

    let alias = Did::from_str(&alias_str).unwrap_or_else(|e| {
        panic!(
            "the {label} spelling of a key is accepted by `Did::from_str` under current policy, \
             and the #2640 replay coverage for it depends on that. It was rejected: {e}. If this \
             is an intentional tightening, move {label} out of ALTERNATE_SPELLINGS and assert \
             its rejection explicitly — do not let the replay assertions stop running."
        )
    });

    assert_ne!(
        alias.as_str(),
        canonical.as_str(),
        "CONTROL: the {label} alias must be a *different string*, or the test proves nothing"
    );
    assert_eq!(
        alias.to_verifying_key().unwrap().as_bytes(),
        canonical.to_verifying_key().unwrap().as_bytes(),
        "CONTROL: the {label} alias must decode to the *same key*, or it is a different sender"
    );
    alias
}

/// All 22 alternate spellings of one key.
fn alias_spellings(canonical: &Did) -> Vec<(&'static str, Did)> {
    ALTERNATE_SPELLINGS
        .iter()
        .map(|(label, base)| (*label, alias_in(*base, label, canonical)))
        .collect()
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

/// The exact third-party attack from #2640, in memory — under every accepted spelling.
#[test]
fn respelled_captured_envelope_does_not_get_a_second_replay_window() {
    let sender = KeyPair::generate().unwrap();
    let canonical = sender.did().clone();
    let aliases = alias_spellings(&canonical);

    let captured = SignedEnvelope::new(
        &canonical,
        &sender,
        9,
        PayloadType::Gossip,
        b"real-payload".to_vec(),
    )
    .unwrap();

    let mut guard = ReplayGuard::new(300, 3600);
    guard
        .check(&captured, ObservedSenderRegime::LegacyOrUnproven)
        .expect("the genuine envelope must be accepted once");

    // CONTROL: the guard really is live for this sender and sequence.
    guard
        .check(&captured, ObservedSenderRegime::LegacyOrUnproven)
        .expect_err("same-spelling replay must be rejected — otherwise the guard is inert");

    // THE PROPERTY, for the whole equivalence class rather than one arbitrary base.
    for (label, alias) in &aliases {
        let forged = respell(&captured, alias);
        assert!(
            guard
                .check(&forged, ObservedSenderRegime::LegacyOrUnproven)
                .is_err(),
            "#2640: a captured envelope re-spelled as {label} must not obtain a fresh replay \
             window"
        );
    }

    assert_eq!(
        guard.peer_count(),
        1,
        "one key must own exactly one replay window, whatever it is spelled — {} spellings \
         were replayed",
        aliases.len()
    );
}

/// The same attack across a restart, through durable state — under every accepted spelling.
#[test]
fn respelled_replay_is_still_rejected_after_a_restart() {
    let sender = KeyPair::generate().unwrap();
    let canonical = sender.did().clone();
    let aliases = alias_spellings(&canonical);
    let store = Arc::new(icn_store::SledStore::temporary().unwrap());

    let captured =
        SignedEnvelope::new(&canonical, &sender, 9, PayloadType::Gossip, b"m".to_vec()).unwrap();

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

    // THE PROPERTY. One restarted guard, every spelling — a durable floor that recognised only
    // some of them would fail here on the ones it missed.
    for (label, alias) in &aliases {
        let forged = respell(&captured, alias);
        assert!(
            guard
                .check(&forged, ObservedSenderRegime::LegacyOrUnproven)
                .is_err(),
            "#2640: a captured envelope re-spelled as {label} must not obtain a fresh replay \
             window"
        );
    }

    assert_eq!(
        guard.peer_count(),
        1,
        "the restored state must still be one window per key across all {} spellings",
        aliases.len()
    );
}

/// The equivalence class itself: what it contains, that it is real, and that it is bounded.
///
/// Guards the coverage the two replay tests above depend on. If a `multibase` upgrade adds a
/// base, or `Did::from_str` starts refusing one, this is what fails — rather than the replay
/// tests quietly exercising a smaller class.
#[test]
fn the_alternate_spellings_are_exactly_the_class_the_parser_accepts() {
    let sender = KeyPair::generate().unwrap();
    let canonical = sender.did().clone();
    let key = canonical.to_verifying_key().unwrap();

    // The canonical emitter really is base58btc, so excluding it from the alternates is right.
    assert_eq!(
        canonical.as_str(),
        format!(
            "did:icn:{}",
            multibase::encode(multibase::Base::Base58Btc, key.as_bytes())
        ),
        "CONTROL: `Did::from_public_key` must still emit base58btc"
    );

    // Every alternate parses (enforced inside), decodes to the same key, and is textually
    // distinct from the canonical spelling and from every other alternate.
    let aliases = alias_spellings(&canonical);
    assert_eq!(
        aliases.len(),
        22,
        "the class is 22 alternates; see the module header"
    );

    let mut seen: Vec<&str> = vec![canonical.as_str()];
    for (label, alias) in &aliases {
        assert!(
            !seen.contains(&alias.as_str()),
            "{label} must be a spelling no other base already produced"
        );
        seen.push(alias.as_str());
    }

    // And the class does not over-reach: a genuinely different key shares none of it.
    let other = KeyPair::generate().unwrap();
    assert_ne!(
        other.did().to_verifying_key().unwrap().as_bytes(),
        key.as_bytes(),
        "CONTROL: the second keypair must actually differ"
    );
    for (label, alias) in alias_spellings(other.did()) {
        assert!(
            !seen.contains(&alias.as_str()),
            "{label} of a different key must never collide with this key's class"
        );
    }
}

/// What closing the attack *at the parse boundary* would look like — the N2-A / #2627 shape.
///
/// Not the current policy: today all 22 alternates parse, which is why the replay tests above
/// must run. This pins the alternative mechanism against an input that is already rejected, so
/// the file states both closures explicitly instead of letting a future tightening silently
/// convert the replay tests into no-ops.
#[test]
fn unparseable_spellings_would_close_the_attack_at_the_parse_boundary() {
    let sender = KeyPair::generate().unwrap();
    let key = sender.did().to_verifying_key().unwrap();

    // A multibase code the crate does not define. Rejected at parse, so no envelope can even
    // carry it — the layer at which N2-A would place the whole alternate class.
    let hex: String = key.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    let undefined_base = format!("did:icn:\u{00a7}{hex}");
    assert!(
        Did::from_str(&undefined_base).is_err(),
        "an undefined multibase code must be refused by `Did::from_str`"
    );

    // CONTROL: the rejection is the *encoding*, not the key — the same bytes spell a valid DID
    // under a defined base. Without this the assertion above would pass for the wrong reason.
    assert!(
        Did::from_str(&format!(
            "did:icn:{}",
            multibase::encode(multibase::Base::Base16Lower, key.as_bytes())
        ))
        .is_ok(),
        "CONTROL: the same key under a defined base must still parse today"
    );
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
