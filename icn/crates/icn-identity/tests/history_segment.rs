//! N1 history segment — a canonical, facts-only excerpt of one Subject's authority log
//! (`docs/adr/ADR-0089-n1-offline-authority-fact-import.md` §3).
//!
//! The one question this suite answers, from the reviewer's side:
//!
//! > Do these bytes carry exactly one Subject's admissible facts over a contiguous, linked range —
//! > keeping every candidate, refusing every structural defect — without deciding authority?
//!
//! So every test here is one of: the bytes are canonical and deterministic; a structural defect is
//! refused at decode with its own variant; a bad witness is refused at N1 admission; or evidence
//! that the N1 fold would not select (a fork, an unauthorized signer) is carried and admitted
//! unchanged. Framing, ordering, witness and bound refusals build their bytes with the local
//! encoder below, which is written from the ADR layout and shares nothing with the crate's
//! `Writer`, so those refusals are tested against bytes the crate did not produce. Range,
//! continuity, linkage and inception refusals go through `AuthorityHistorySegment::new`, which
//! encodes with the crate's `Writer` before decoding; the independent Python reference
//! (`tests/reference/history_segment_reference.py`) refuses the same defects from its own bytes.
//!
//! Fixture identities only. Every key is derived from a seed byte; nothing here is a real person
//! or a real device.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use std::collections::{BTreeMap, BTreeSet};

use authority_log_support::{authorize_at, revoke_at, store_of, stranger, subject, Subject};
use ed25519_dalek::{Signer, SigningKey};
use icn_identity::authority_log::{
    authorize_event, derive, AdmissionError, AuthorityBody, AuthorityBody as Body, AuthorityStore,
    AuthorityView, CapabilitySet, CodecError, DeviceCapability, EventId, SignedAuthorityEvent,
    SubjectId, ValiditySpan, Witness, WitnessSignature, COMMITMENT_DOMAIN, DOMAIN, KDF_DOMAIN,
    MAX_POSITION, SIGNATURE_DOMAIN,
};
use icn_identity::device_authority::DEVICE_ACT_DOMAIN;
use icn_identity::evidence_bundle::{BundledFact, BUNDLE_DOMAIN};
use icn_identity::history_segment::{
    AdmittedAuthorityHistorySegment, AuthorityHistorySegment, SegmentError, MAX_SEGMENT_BYTES,
    MAX_SEGMENT_FACTS, MAX_WITNESSES_PER_FACT, SEGMENT_DOMAIN, SEGMENT_VERSION,
};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------------------------
// Fixture: S with A authorized at 1, B authorized at 2, A revoked at 3, and a competing candidate
// C authorized at 2 (both 2s name the 1 as parent). All N1 facts are signed by S's generation-0
// authority key.
// ---------------------------------------------------------------------------------------------

struct Fixture {
    s: Subject,
    auth_a: SignedAuthorityEvent,
    auth_b: SignedAuthorityEvent,
    revoke_a: SignedAuthorityEvent,
    auth_c: SignedAuthorityEvent,
}

fn fixture_with(subject_seed: u8, a_seed: u8, b_seed: u8, c_seed: u8) -> Fixture {
    let s = subject(subject_seed, 4);
    let (_, a) = stranger(a_seed);
    let (_, b) = stranger(b_seed);
    let (_, c) = stranger(c_seed);
    let auth_a = authorize_at(&s, 0, 1, s.genesis(), a);
    let auth_b = authorize_at(&s, 0, 2, auth_a.body.event_id(), b);
    let revoke_a = revoke_at(&s, 0, 3, auth_b.body.event_id(), a);
    let auth_c = authorize_at(&s, 0, 2, auth_a.body.event_id(), c);
    Fixture {
        s,
        auth_a,
        auth_b,
        revoke_a,
        auth_c,
    }
}

fn fixture() -> Fixture {
    fixture_with(0x01, 0x11, 0x22, 0x33)
}

/// Group events by body, collecting every signature over each.
fn facts_of(events: &[&SignedAuthorityEvent]) -> BTreeMap<EventId, BundledFact> {
    let mut out: BTreeMap<EventId, BundledFact> = BTreeMap::new();
    for event in events {
        out.entry(event.body.event_id())
            .or_insert_with(|| BundledFact {
                body: event.body.clone(),
                witnesses: BTreeSet::new(),
            })
            .witnesses
            .insert(event.signature);
    }
    out
}

fn segment(
    s: SubjectId,
    from: u64,
    to: u64,
    events: &[&SignedAuthorityEvent],
) -> Result<AuthorityHistorySegment, SegmentError> {
    AuthorityHistorySegment::new(s, from, to, facts_of(events))
}

/// A second, distinct, valid Ed25519 signature over one body (same scalar, different nonce
/// prefix): a re-signing, which must be carried as a second witness, not as a fork.
fn second_witness(body: &AuthorityBody, key: &SigningKey, prefix_byte: u8) -> WitnessSignature {
    use ed25519_dalek::hazmat::{raw_sign, ExpandedSecretKey};
    use sha2::Sha512;
    let mut expanded = ExpandedSecretKey::from(&key.to_bytes());
    expanded.hash_prefix = [prefix_byte; 32];
    let signature = raw_sign::<Sha512>(&expanded, &body.signature_preimage(), &key.verifying_key());
    WitnessSignature::from_bytes(signature.to_bytes())
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

// ---------------------------------------------------------------------------------------------
// A local encoder, written from the ADR-0089 §3 layout. It emits facts and witnesses in the order
// given — not sorted — so tests can produce the non-canonical bytes the decoder must refuse.
// ---------------------------------------------------------------------------------------------

struct RawFact {
    stated: [u8; 32],
    body: Vec<u8>,
    witnesses: Vec<[u8; 64]>,
}

impl RawFact {
    fn of(event: &SignedAuthorityEvent) -> Self {
        RawFact {
            stated: *event.body.event_id().as_bytes(),
            body: event.body.canonical_bytes(),
            witnesses: vec![*event.signature.as_bytes()],
        }
    }
}

fn raw_header(
    domain: &[u8],
    version: u16,
    s: SubjectId,
    from: u64,
    to: u64,
    count: u32,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(domain.len() as u32).to_be_bytes());
    out.extend_from_slice(domain);
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(s.as_bytes());
    out.extend_from_slice(&from.to_be_bytes());
    out.extend_from_slice(&to.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out
}

fn raw_segment(s: SubjectId, from: u64, to: u64, facts: &[RawFact]) -> Vec<u8> {
    let mut out = raw_header(
        SEGMENT_DOMAIN,
        SEGMENT_VERSION,
        s,
        from,
        to,
        facts.len() as u32,
    );
    for fact in facts {
        out.extend_from_slice(&fact.stated);
        out.extend_from_slice(&(fact.body.len() as u32).to_be_bytes());
        out.extend_from_slice(&fact.body);
        out.extend_from_slice(&(fact.witnesses.len() as u32).to_be_bytes());
        for witness in &fact.witnesses {
            out.extend_from_slice(witness);
        }
    }
    out
}

/// Raw facts for `events`, in canonical (ascending event_id) order.
fn raw_sorted(events: &[&SignedAuthorityEvent]) -> Vec<RawFact> {
    let mut facts: Vec<RawFact> = events.iter().map(|e| RawFact::of(e)).collect();
    facts.sort_by_key(|fact| fact.stated);
    facts
}

fn decode_err(bytes: &[u8]) -> SegmentError {
    AuthorityHistorySegment::decode(bytes).expect_err("decode must refuse")
}

// ---------------------------------------------------------------------------------------------
// 1. Canonical round trip and identity.
// ---------------------------------------------------------------------------------------------

#[test]
fn round_trip_is_byte_identical_and_the_local_encoder_agrees() {
    let f = fixture();
    let seg = segment(
        f.s.subject,
        0,
        3,
        &[&f.s.inception, &f.auth_a, &f.auth_b, &f.revoke_a],
    )
    .unwrap();
    let bytes = seg.canonical_bytes();
    let decoded = AuthorityHistorySegment::decode(&bytes).unwrap();
    assert_eq!(decoded, seg);
    assert_eq!(decoded.canonical_bytes(), bytes);
    // The crate's encoder and the layout-derived local encoder produce the same bytes.
    let raw = raw_segment(
        f.s.subject,
        0,
        3,
        &raw_sorted(&[&f.s.inception, &f.auth_a, &f.auth_b, &f.revoke_a]),
    );
    assert_eq!(bytes, raw);
    assert_eq!(seg.segment_id(), sha256(&bytes));
    assert_eq!(decoded.segment_id(), seg.segment_id());
    assert_eq!(seg.subject(), f.s.subject);
    assert_eq!(seg.range(), (0, 3));
}

#[test]
fn segment_identity_does_not_depend_on_how_the_facts_were_supplied() {
    let f = fixture();
    let forward = segment(f.s.subject, 1, 3, &[&f.auth_a, &f.auth_b, &f.revoke_a]).unwrap();
    let backward = segment(f.s.subject, 1, 3, &[&f.revoke_a, &f.auth_b, &f.auth_a]).unwrap();
    assert_eq!(forward.canonical_bytes(), backward.canonical_bytes());
    assert_eq!(forward.segment_id(), backward.segment_id());
}

#[test]
fn every_field_changes_the_segment_id() {
    let f = fixture();
    let base = segment(f.s.subject, 1, 2, &[&f.auth_a, &f.auth_b]).unwrap();
    let other_range = segment(f.s.subject, 1, 3, &[&f.auth_a, &f.auth_b, &f.revoke_a]).unwrap();
    let other_fact = segment(f.s.subject, 1, 2, &[&f.auth_a, &f.auth_c]).unwrap();
    let resigned = SignedAuthorityEvent::new(
        f.auth_b.body.clone(),
        second_witness(&f.auth_b.body, &f.s.root.authority_signing_key(0), 0x5a),
    );
    let more_witnesses = segment(f.s.subject, 1, 2, &[&f.auth_a, &f.auth_b, &resigned]).unwrap();
    let ids: BTreeSet<[u8; 32]> = [&base, &other_range, &other_fact, &more_witnesses]
        .iter()
        .map(|s| s.segment_id())
        .collect();
    assert_eq!(ids.len(), 4);
}

// ---------------------------------------------------------------------------------------------
// 2. Accepted shapes: from inception, from a later position, forks, re-signings, and admissible
//    evidence that confers no authority.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_segment_from_inception_and_one_from_a_later_position_are_both_valid() {
    let f = fixture();
    let only_inception = segment(f.s.subject, 0, 0, &[&f.s.inception]).unwrap();
    assert_eq!(only_inception.facts().len(), 1);
    let later = segment(f.s.subject, 2, 3, &[&f.auth_b, &f.revoke_a]).unwrap();
    assert_eq!(later.range(), (2, 3));
    // A fact at `from > 0` is not required to link inside the segment: anchoring is an import
    // rule against retained facts, not a segment rule.
    assert!(!later.facts().contains_key(&f.auth_a.body.event_id()));
}

#[test]
fn a_fork_is_carried_as_a_fork_and_admits_as_one() {
    let f = fixture();
    let seg = segment(
        f.s.subject,
        0,
        2,
        &[&f.s.inception, &f.auth_a, &f.auth_b, &f.auth_c],
    )
    .unwrap();
    let at_two: BTreeSet<EventId> = seg
        .facts()
        .iter()
        .filter(|(_, fact)| fact.body.position() == 2)
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(
        at_two,
        BTreeSet::from([f.auth_b.body.event_id(), f.auth_c.body.event_id()])
    );
    let decoded = AuthorityHistorySegment::decode(&seg.canonical_bytes()).unwrap();
    assert_eq!(decoded, seg);
    // Admission keeps both candidates, and the fold over the admitted store sees the dispute:
    // neither the segment nor admission picked a side.
    let admitted = decoded.admit().unwrap();
    assert_eq!(admitted.segment(), &seg);
    assert!(admitted.store().contains_body(&f.auth_b.body));
    assert!(admitted.store().contains_body(&f.auth_c.body));
    match derive(f.s.subject, admitted.store()) {
        AuthorityView::Halted {
            disputed_at,
            candidates,
            ..
        } => {
            assert_eq!(disputed_at, 2);
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("expected the fork to halt the subject, got {other:?}"),
    }
}

#[test]
fn a_fork_with_one_continued_branch_keeps_the_dead_branch() {
    let f = fixture();
    // revoke_a at 3 continues the auth_b branch; auth_c at 2 has no child. Both 2s are carried.
    let seg = segment(
        f.s.subject,
        0,
        3,
        &[&f.s.inception, &f.auth_a, &f.auth_b, &f.auth_c, &f.revoke_a],
    )
    .unwrap();
    assert!(seg.facts().contains_key(&f.auth_c.body.event_id()));
    assert_eq!(seg.facts().len(), 5);
}

#[test]
fn an_admissible_fact_from_an_unauthorized_signer_is_carried_and_admitted() {
    let f = fixture();
    // A stranger signs an `authorize` for S at position 2. N1 admission accepts it (the signature
    // verifies under its inline signer); the N1 fold gives it no authority. The segment must carry
    // it regardless: filtering by derived authority would be interpretation.
    let (rogue_key, _) = stranger(0x77);
    let (_, rogue_device) = stranger(0x78);
    let rogue = authorize_event(
        &rogue_key,
        f.s.subject,
        2,
        f.auth_a.body.event_id(),
        rogue_device,
        CapabilitySet::new([DeviceCapability::Sign]),
        None,
    );
    let seg = segment(f.s.subject, 0, 2, &[&f.s.inception, &f.auth_a, &rogue]).unwrap();
    assert!(seg.facts().contains_key(&rogue.body.event_id()));
    // Admitted, because admission is N1's signature check and nothing more.
    let admitted = seg.admit().unwrap();
    assert!(admitted
        .segment()
        .facts()
        .contains_key(&rogue.body.event_id()));
    assert!(admitted.store().contains_body(&rogue.body));
    // And the fold, not the segment or admission, is what declines it: position 2 has no
    // authorized candidate, so the admitted rogue authorize confers nothing.
    match derive(f.s.subject, admitted.store()) {
        AuthorityView::Live { frontier, .. } => assert_eq!(frontier, 2),
        other => panic!("expected a live view stopped at 2, got {other:?}"),
    }
}

#[test]
fn admit_equals_ingesting_every_pair_directly_including_every_witness() {
    let f = fixture();
    let resigned = SignedAuthorityEvent::new(
        f.auth_b.body.clone(),
        second_witness(&f.auth_b.body, &f.s.root.authority_signing_key(0), 0x5a),
    );
    assert_ne!(resigned.signature, f.auth_b.signature);
    let events = [&f.s.inception, &f.auth_a, &f.auth_b, &resigned, &f.revoke_a];
    let seg = segment(f.s.subject, 0, 3, &events).unwrap();
    assert_eq!(
        seg.facts()[&f.auth_b.body.event_id()].witnesses.len(),
        2,
        "a re-signing is a second witness, not a second fact"
    );
    let owned: Vec<SignedAuthorityEvent> = events.iter().map(|e| (*e).clone()).collect();
    let direct = store_of(&owned);
    let admitted = seg.admit().unwrap();
    assert_eq!(admitted.store(), &direct);
    assert_eq!(admitted.store().witness_count(), 5);
}

#[test]
fn new_carries_exactly_the_supplied_facts() {
    let f = fixture();
    let supplied = facts_of(&[&f.s.inception, &f.auth_a, &f.auth_b, &f.auth_c]);
    let seg = AuthorityHistorySegment::new(f.s.subject, 0, 2, supplied.clone()).unwrap();
    assert_eq!(seg.facts(), &supplied);
}

// ---------------------------------------------------------------------------------------------
// 3. Framing refusals.
// ---------------------------------------------------------------------------------------------

fn linear_bytes(f: &Fixture) -> Vec<u8> {
    segment(f.s.subject, 1, 3, &[&f.auth_a, &f.auth_b, &f.revoke_a])
        .unwrap()
        .canonical_bytes()
}

#[test]
fn every_truncation_is_refused_as_framing() {
    let f = fixture();
    let bytes = linear_bytes(&f);
    for len in 0..bytes.len() {
        match decode_err(&bytes[..len]) {
            SegmentError::Framing(_) => {}
            other => panic!("truncation to {len} bytes refused as {other:?}, not framing"),
        }
    }
}

#[test]
fn trailing_bytes_are_refused() {
    let f = fixture();
    let mut bytes = linear_bytes(&f);
    bytes.push(0);
    assert_eq!(
        decode_err(&bytes),
        SegmentError::Framing(CodecError::TrailingBytes(1))
    );
}

#[test]
fn a_wrong_domain_is_refused() {
    let f = fixture();
    let facts = raw_sorted(&[&f.auth_a]);
    let mut bytes = raw_header(BUNDLE_DOMAIN, SEGMENT_VERSION, f.s.subject, 1, 1, 1);
    bytes.extend_from_slice(
        &raw_segment(f.s.subject, 1, 1, &facts)
            [raw_header(SEGMENT_DOMAIN, 1, f.s.subject, 1, 1, 1).len()..],
    );
    assert_eq!(
        decode_err(&bytes),
        SegmentError::Framing(CodecError::BadDomain)
    );
}

#[test]
fn a_wrong_version_is_refused() {
    let f = fixture();
    let facts = raw_sorted(&[&f.auth_a]);
    let header_len = raw_header(SEGMENT_DOMAIN, 1, f.s.subject, 1, 1, 1).len();
    let mut bytes = raw_header(SEGMENT_DOMAIN, 2, f.s.subject, 1, 1, 1);
    bytes.extend_from_slice(&raw_segment(f.s.subject, 1, 1, &facts)[header_len..]);
    assert_eq!(decode_err(&bytes), SegmentError::Version(2));
}

#[test]
fn a_range_out_of_order_or_out_of_bounds_is_refused() {
    let f = fixture();
    let facts = raw_sorted(&[&f.auth_a]);
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 3, 1, &facts)),
        SegmentError::Range { from: 3, to: 1 }
    );
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 1, MAX_POSITION + 1, &facts)),
        SegmentError::Range {
            from: 1,
            to: MAX_POSITION + 1
        }
    );
    assert_eq!(
        segment(f.s.subject, 3, 1, &[&f.auth_a]).unwrap_err(),
        SegmentError::Range { from: 3, to: 1 }
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Per-fact refusals (rules 2, 3, 6).
// ---------------------------------------------------------------------------------------------

#[test]
fn a_fact_for_another_subject_is_refused() {
    let f = fixture();
    let other = fixture_with(0x02, 0x11, 0x22, 0x33);
    let facts = raw_sorted(&[&other.auth_a]);
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 1, 1, &facts)),
        SegmentError::FactSubjectMismatch {
            event_id: other.auth_a.body.event_id()
        }
    );
}

#[test]
fn a_stated_event_id_that_is_not_the_body_digest_is_refused() {
    let f = fixture();
    let mut fact = RawFact::of(&f.auth_a);
    fact.stated[31] ^= 0x01;
    let stated = EventId::from_bytes(fact.stated);
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 1, 1, &[fact])),
        SegmentError::EventIdMismatch {
            stated,
            computed: f.auth_a.body.event_id()
        }
    );
    // `new` refuses a mis-keyed map rather than re-keying it.
    let mut facts = facts_of(&[&f.auth_a]);
    let fact = facts.remove(&f.auth_a.body.event_id()).unwrap();
    facts.insert(stated, fact);
    assert!(matches!(
        AuthorityHistorySegment::new(f.s.subject, 1, 1, facts),
        Err(SegmentError::EventIdMismatch { .. })
    ));
}

#[test]
fn a_malformed_body_is_refused_with_n1s_own_error() {
    let f = fixture();
    let garbage = b"not an N1 body".to_vec();
    let fact = RawFact {
        stated: sha256(&garbage),
        body: garbage,
        witnesses: vec![[1u8; 64]],
    };
    assert!(matches!(
        decode_err(&raw_segment(f.s.subject, 1, 1, &[fact])),
        SegmentError::Body { .. }
    ));
}

#[test]
fn new_refuses_a_body_that_n1_would_not_decode() {
    let f = fixture();
    // An inverted validity span can only be built by struct literal; its bytes do not decode.
    let mut body = f.auth_a.body.clone();
    if let Body::Authorize(ref mut inner) = body {
        inner.validity = Some(ValiditySpan {
            from_position: 9,
            until_position: 3,
        });
    } else {
        panic!("fixture changed: auth_a is not an authorize body");
    }
    let mut facts = BTreeMap::new();
    facts.insert(
        body.event_id(),
        BundledFact {
            body,
            witnesses: BTreeSet::from([f.auth_a.signature]),
        },
    );
    assert!(matches!(
        AuthorityHistorySegment::new(f.s.subject, 1, 1, facts),
        Err(SegmentError::Body {
            source: CodecError::InvertedSpan { .. },
            ..
        })
    ));
}

#[test]
fn a_fact_outside_the_range_is_refused_above_and_below() {
    let f = fixture();
    // Above `to`.
    assert_eq!(
        segment(f.s.subject, 1, 2, &[&f.auth_a, &f.auth_b, &f.revoke_a]).unwrap_err(),
        SegmentError::FactOutsideRange {
            event_id: f.revoke_a.body.event_id(),
            position: 3,
            from: 1,
            to: 2
        }
    );
    // Below `from`.
    assert_eq!(
        segment(f.s.subject, 1, 1, &[&f.s.inception, &f.auth_a]).unwrap_err(),
        SegmentError::FactOutsideRange {
            event_id: f.s.genesis(),
            position: 0,
            from: 1,
            to: 1
        }
    );
}

#[test]
fn facts_out_of_order_or_duplicated_are_refused() {
    let f = fixture();
    let mut facts = raw_sorted(&[&f.auth_a, &f.auth_b]);
    facts.reverse();
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 1, 2, &facts)),
        SegmentError::FactOrder
    );
    let dup = vec![RawFact::of(&f.auth_a), RawFact::of(&f.auth_a)];
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 1, 1, &dup)),
        SegmentError::FactOrder
    );
}

#[test]
fn witnesses_out_of_order_or_duplicated_are_refused() {
    let f = fixture();
    let w1 = *f.auth_a.signature.as_bytes();
    let w2 = *second_witness(&f.auth_a.body, &f.s.root.authority_signing_key(0), 0x5a).as_bytes();
    let (low, high) = if w1 < w2 { (w1, w2) } else { (w2, w1) };
    for witnesses in [vec![high, low], vec![low, low]] {
        let fact = RawFact {
            witnesses,
            ..RawFact::of(&f.auth_a)
        };
        assert_eq!(
            decode_err(&raw_segment(f.s.subject, 1, 1, &[fact])),
            SegmentError::WitnessOrder {
                event_id: f.auth_a.body.event_id()
            }
        );
    }
}

#[test]
fn a_fact_with_no_witness_is_refused() {
    let f = fixture();
    let fact = RawFact {
        witnesses: vec![],
        ..RawFact::of(&f.auth_a)
    };
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 1, 1, &[fact])),
        SegmentError::NoWitness {
            event_id: f.auth_a.body.event_id()
        }
    );
    let mut facts = facts_of(&[&f.auth_a]);
    facts.values_mut().for_each(|fact| fact.witnesses.clear());
    assert_eq!(
        AuthorityHistorySegment::new(f.s.subject, 1, 1, facts).unwrap_err(),
        SegmentError::NoWitness {
            event_id: f.auth_a.body.event_id()
        }
    );
}

// ---------------------------------------------------------------------------------------------
// 5. Whole-segment refusals (rules 4 and 5).
// ---------------------------------------------------------------------------------------------

#[test]
fn a_gap_in_continuity_is_refused_at_its_lowest_position() {
    let f = fixture();
    // Interior gap.
    assert_eq!(
        segment(f.s.subject, 1, 3, &[&f.auth_a, &f.revoke_a]).unwrap_err(),
        SegmentError::ContinuityGap { position: 2 }
    );
    // Gap at the top of the range.
    assert_eq!(
        segment(f.s.subject, 1, 3, &[&f.auth_a, &f.auth_b]).unwrap_err(),
        SegmentError::ContinuityGap { position: 3 }
    );
    // Gap at the bottom of the range.
    assert_eq!(
        segment(f.s.subject, 1, 3, &[&f.auth_b, &f.revoke_a]).unwrap_err(),
        SegmentError::ContinuityGap { position: 1 }
    );
    // No facts at all.
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 4, 4, &[])),
        SegmentError::ContinuityGap { position: 4 }
    );
}

#[test]
fn a_fact_whose_parent_is_not_carried_is_refused() {
    let f = fixture();
    let (_, b) = stranger(0x22);
    let unlinked = authorize_at(&f.s, 0, 2, EventId::from_bytes([0xab; 32]), b);
    assert_eq!(
        segment(f.s.subject, 0, 2, &[&f.s.inception, &f.auth_a, &unlinked]).unwrap_err(),
        SegmentError::BrokenLinkage {
            event_id: unlinked.body.event_id(),
            position: 2
        }
    );
}

#[test]
fn a_fact_whose_parent_is_carried_at_the_wrong_position_is_refused() {
    let f = fixture();
    // Position 3 names auth_a (position 1) as its parent; position 2 is covered by auth_b, so
    // continuity holds and only the linkage position check can refuse it.
    let (_, a) = stranger(0x11);
    let skipping = revoke_at(&f.s, 0, 3, f.auth_a.body.event_id(), a);
    assert_eq!(
        segment(f.s.subject, 1, 3, &[&f.auth_a, &f.auth_b, &skipping]).unwrap_err(),
        SegmentError::BrokenLinkage {
            event_id: skipping.body.event_id(),
            position: 3
        }
    );
}

#[test]
fn a_segment_from_zero_without_the_inception_is_refused() {
    let f = fixture();
    assert_eq!(
        segment(f.s.subject, 0, 2, &[&f.auth_a, &f.auth_b]).unwrap_err(),
        SegmentError::MissingInception
    );
}

// ---------------------------------------------------------------------------------------------
// 6. Bounds, checked before parsing or allocation.
// ---------------------------------------------------------------------------------------------

#[test]
fn an_oversized_input_is_refused_before_it_is_parsed() {
    // Not a segment at all: if the size check ran after parsing, this would be a framing error.
    let oversized = vec![0u8; MAX_SEGMENT_BYTES + 1];
    assert_eq!(
        decode_err(&oversized),
        SegmentError::TooLarge {
            len: MAX_SEGMENT_BYTES + 1,
            max: MAX_SEGMENT_BYTES
        }
    );
    // At exactly the bound, the input is parsed (and refused for what it is).
    let at_bound = vec![0u8; MAX_SEGMENT_BYTES];
    assert!(matches!(decode_err(&at_bound), SegmentError::Framing(_)));
}

#[test]
fn new_refuses_a_well_formed_segment_above_the_byte_bound() {
    let s = subject(0x05, 4);
    let (_, dev) = stranger(0x55);
    // A linear chain with MAX_WITNESSES_PER_FACT witnesses per fact. Witness bytes are distinct
    // placeholders: the byte bound is structural and is checked before admission.
    // Enough facts that the witness bytes alone exceed the bound, whatever a body's length.
    let n = (MAX_SEGMENT_BYTES / (64 * MAX_WITNESSES_PER_FACT) + 1) as u64;
    assert!(
        (n as usize) < MAX_SEGMENT_FACTS,
        "must hit the byte bound, not the fact bound"
    );
    let mut facts = BTreeMap::new();
    let mut prev = s.genesis();
    for position in 1..=n {
        let event = authorize_at(&s, 0, position, prev, dev);
        prev = event.body.event_id();
        let witnesses: BTreeSet<WitnessSignature> = (0..MAX_WITNESSES_PER_FACT)
            .map(|i| {
                let mut w = [0u8; 64];
                w[..8].copy_from_slice(&position.to_be_bytes());
                w[8] = i as u8;
                WitnessSignature::from_bytes(w)
            })
            .collect();
        facts.insert(
            prev,
            BundledFact {
                body: event.body,
                witnesses,
            },
        );
    }
    match AuthorityHistorySegment::new(s.subject, 1, n, facts) {
        Err(SegmentError::TooLarge { len, max }) => {
            assert!(len > MAX_SEGMENT_BYTES);
            assert_eq!(max, MAX_SEGMENT_BYTES);
        }
        Ok(_) => panic!("expected TooLarge, got a segment"),
        Err(other) => panic!("expected TooLarge, got {other}"),
    }
}

#[test]
fn more_facts_than_the_bound_are_refused_even_when_the_input_could_hold_them() {
    let f = fixture();
    let declared = MAX_SEGMENT_FACTS as u32 + 1;
    let mut bytes = raw_header(SEGMENT_DOMAIN, SEGMENT_VERSION, f.s.subject, 1, 1, declared);
    // Enough bytes that the remaining-input check alone would not refuse the count.
    bytes.resize(bytes.len() + (declared as usize) * 40, 0);
    assert!(bytes.len() <= MAX_SEGMENT_BYTES);
    assert_eq!(
        decode_err(&bytes),
        SegmentError::TooManyFacts {
            count: MAX_SEGMENT_FACTS + 1,
            max: MAX_SEGMENT_FACTS
        }
    );
    // At the bound, the count is accepted and the first (all-zero) fact is refused for its body.
    let mut at_bound = raw_header(
        SEGMENT_DOMAIN,
        SEGMENT_VERSION,
        f.s.subject,
        1,
        1,
        MAX_SEGMENT_FACTS as u32,
    );
    at_bound.resize(at_bound.len() + MAX_SEGMENT_FACTS * 40, 0);
    assert!(matches!(decode_err(&at_bound), SegmentError::Body { .. }));

    // `new` refuses the count before encoding anything.
    let one = facts_of(&[&f.auth_a]).into_values().next().unwrap();
    let many: BTreeMap<EventId, BundledFact> = (0..=MAX_SEGMENT_FACTS)
        .map(|i| {
            let mut id = [0u8; 32];
            id[..8].copy_from_slice(&(i as u64).to_be_bytes());
            (EventId::from_bytes(id), one.clone())
        })
        .collect();
    assert_eq!(
        AuthorityHistorySegment::new(f.s.subject, 1, 1, many).unwrap_err(),
        SegmentError::TooManyFacts {
            count: MAX_SEGMENT_FACTS + 1,
            max: MAX_SEGMENT_FACTS
        }
    );
}

fn distinct_witnesses(n: usize) -> Vec<[u8; 64]> {
    (0..n)
        .map(|i| {
            let mut w = [0u8; 64];
            w[0] = i as u8;
            w
        })
        .collect()
}

#[test]
fn more_witnesses_than_the_bound_are_refused_and_the_bound_itself_is_accepted() {
    let f = fixture();
    let over = RawFact {
        witnesses: distinct_witnesses(MAX_WITNESSES_PER_FACT + 1),
        ..RawFact::of(&f.auth_a)
    };
    assert_eq!(
        decode_err(&raw_segment(f.s.subject, 1, 1, &[over])),
        SegmentError::TooManyWitnesses {
            event_id: f.auth_a.body.event_id(),
            count: MAX_WITNESSES_PER_FACT + 1,
            max: MAX_WITNESSES_PER_FACT
        }
    );
    // Exactly at the bound the structure decodes; the placeholder witnesses then fail admission.
    let at = RawFact {
        witnesses: distinct_witnesses(MAX_WITNESSES_PER_FACT),
        ..RawFact::of(&f.auth_a)
    };
    let seg = AuthorityHistorySegment::decode(&raw_segment(f.s.subject, 1, 1, &[at])).unwrap();
    assert!(matches!(
        seg.admit(),
        Err(SegmentError::Inadmissible {
            source: AdmissionError::SignatureInvalid,
            ..
        })
    ));
    // `new` applies the same bound.
    let mut facts = facts_of(&[&f.auth_a]);
    facts.values_mut().for_each(|fact| {
        fact.witnesses = distinct_witnesses(MAX_WITNESSES_PER_FACT + 1)
            .into_iter()
            .map(WitnessSignature::from_bytes)
            .collect()
    });
    assert!(matches!(
        AuthorityHistorySegment::new(f.s.subject, 1, 1, facts),
        Err(SegmentError::TooManyWitnesses { .. })
    ));
}

// ---------------------------------------------------------------------------------------------
// 7. Admission (rule 7): every pair, all or nothing.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_bad_witness_decodes_but_is_refused_by_admission() {
    let f = fixture();
    let mut fact = RawFact::of(&f.auth_b);
    fact.witnesses[0][0] ^= 0x01;
    let mut facts = raw_sorted(&[&f.auth_a, &f.revoke_a]);
    facts.push(fact);
    facts.sort_by_key(|fact| fact.stated);
    let seg = AuthorityHistorySegment::decode(&raw_segment(f.s.subject, 1, 3, &facts)).unwrap();
    assert_eq!(
        seg.admit().unwrap_err(),
        SegmentError::Inadmissible {
            event_id: f.auth_b.body.event_id(),
            source: AdmissionError::SignatureInvalid
        }
    );
}

#[test]
fn a_bad_witness_after_a_good_one_on_the_same_fact_is_still_refused() {
    let f = fixture();
    // The valid witness sorts first, so admitting only the first witness per fact would pass.
    let good = *f.auth_a.signature.as_bytes();
    let bad = [0xffu8; 64];
    assert!(good < bad);
    let fact = RawFact {
        witnesses: vec![good, bad],
        ..RawFact::of(&f.auth_a)
    };
    let seg = AuthorityHistorySegment::decode(&raw_segment(f.s.subject, 1, 1, &[fact])).unwrap();
    assert_eq!(
        seg.admit().unwrap_err(),
        SegmentError::Inadmissible {
            event_id: f.auth_a.body.event_id(),
            source: AdmissionError::SignatureInvalid
        }
    );
}

/// The linear 1..=3 segment with `target`'s witnesses replaced. `new` checks structure only, so
/// the witnesses need not verify: that is admission's job.
fn linear_with_witnesses(
    f: &Fixture,
    target: &SignedAuthorityEvent,
    witnesses: BTreeSet<WitnessSignature>,
) -> AuthorityHistorySegment {
    let mut facts = facts_of(&[&f.auth_a, &f.auth_b, &f.revoke_a]);
    facts
        .get_mut(&target.body.event_id())
        .expect("target is one of the linear facts")
        .witnesses = witnesses;
    AuthorityHistorySegment::new(f.s.subject, 1, 3, facts).unwrap()
}

/// `witness` with one bit of `S` flipped. `S` starts at byte 32, so the bytewise order against
/// any other distinct witness is unchanged.
fn corrupt(witness: WitnessSignature) -> WitnessSignature {
    let mut bytes = *witness.as_bytes();
    bytes[32] ^= 0x01;
    WitnessSignature::from_bytes(bytes)
}

#[test]
fn an_admitted_segment_is_the_decoded_segment_and_exactly_its_evidence() {
    let f = fixture();
    let mut facts = facts_of(&[&f.s.inception, &f.auth_a, &f.auth_b, &f.auth_c, &f.revoke_a]);
    facts
        .get_mut(&f.auth_b.body.event_id())
        .unwrap()
        .witnesses
        .insert(second_witness(
            &f.auth_b.body,
            &f.s.root.authority_signing_key(0),
            0x5a,
        ));
    let seg = AuthorityHistorySegment::new(f.s.subject, 0, 3, facts.clone()).unwrap();
    let bytes = seg.canonical_bytes();

    let admitted: AdmittedAuthorityHistorySegment = seg.clone().admit().unwrap();

    // The segment passes through admission unchanged: same facts, fork included, same bytes.
    assert_eq!(admitted.segment(), &seg);
    assert_eq!(admitted.segment().facts(), &facts);
    assert_eq!(admitted.segment().canonical_bytes(), bytes);
    // The store holds exactly the carried bodies and exactly the carried (body, witness) pairs:
    // nothing dropped for being a fork candidate, nothing added.
    let bodies: BTreeSet<AuthorityBody> = facts.values().map(|fact| fact.body.clone()).collect();
    let witnesses: BTreeSet<Witness> = facts
        .iter()
        .flat_map(|(event_id, fact)| {
            fact.witnesses.iter().map(|signature| Witness {
                event_id: *event_id,
                signature: *signature,
            })
        })
        .collect();
    assert_eq!(witnesses.len(), 6);
    assert_eq!(admitted.store().bodies(), &bodies);
    assert_eq!(admitted.store().witnesses(), &witnesses);
}

#[test]
fn a_real_signature_by_a_key_the_body_does_not_name_is_refused() {
    let f = fixture();
    // A genuine Ed25519 signature over auth_b's exact preimage, by a key other than the signer
    // auth_b names inline. Structurally indistinguishable from a good witness; N1 refuses it.
    let (other, _) = stranger(0x66);
    let misattributed =
        WitnessSignature::from_bytes(other.sign(&f.auth_b.body.signature_preimage()).to_bytes());
    let seg = linear_with_witnesses(&f, &f.auth_b, BTreeSet::from([misattributed]));
    assert_eq!(
        seg.admit().unwrap_err(),
        SegmentError::Inadmissible {
            event_id: f.auth_b.body.event_id(),
            source: AdmissionError::SignatureInvalid
        }
    );
}

#[test]
fn a_multi_witness_fact_is_refused_wherever_its_one_bad_witness_sorts() {
    let f = fixture();
    let key = f.s.root.authority_signing_key(0);
    let valid: BTreeSet<WitnessSignature> = BTreeSet::from([
        f.auth_b.signature,
        second_witness(&f.auth_b.body, &key, 0x5a),
        second_witness(&f.auth_b.body, &key, 0xa5),
    ]);
    assert_eq!(valid.len(), 3);
    let admitted = linear_with_witnesses(&f, &f.auth_b, valid.clone())
        .admit()
        .unwrap();
    assert_eq!(admitted.store().witness_count(), 5);

    // Corrupt the first, then the middle, then the last witness in canonical order.
    let sorted: Vec<WitnessSignature> = valid.iter().copied().collect();
    for rank in 0..sorted.len() {
        let mut witnesses = sorted.clone();
        witnesses[rank] = corrupt(witnesses[rank]);
        let witnesses: BTreeSet<WitnessSignature> = witnesses.into_iter().collect();
        assert_eq!(
            witnesses.iter().position(|w| *w == corrupt(sorted[rank])),
            Some(rank),
            "the bad witness must sit at rank {rank}"
        );
        assert_eq!(
            linear_with_witnesses(&f, &f.auth_b, witnesses)
                .admit()
                .unwrap_err(),
            SegmentError::Inadmissible {
                event_id: f.auth_b.body.event_id(),
                source: AdmissionError::SignatureInvalid
            },
            "rank {rank}"
        );
    }
}

#[test]
fn a_refusal_after_earlier_pairs_were_ingested_returns_no_segment_and_no_store() {
    let f = fixture();
    // Corrupt the fact that sorts last, so every other pair has already been ingested into the
    // scratch store when admission refuses.
    let linear = [&f.auth_a, &f.auth_b, &f.revoke_a];
    let last = *linear
        .iter()
        .max_by_key(|event| event.body.event_id())
        .unwrap();
    for event in linear {
        if event.body.event_id() != last.body.event_id() {
            let mut alone = AuthorityStore::new();
            alone
                .ingest(event)
                .expect("every earlier pair is admissible");
        }
    }
    let seg = linear_with_witnesses(&f, last, BTreeSet::from([corrupt(last.signature)]));

    // The only thing that comes back is the refusal naming the fact. There is no partially
    // admitted segment or store to hand on: the Result has no such value.
    let result: Result<AdmittedAuthorityHistorySegment, SegmentError> = seg.admit();
    assert_eq!(
        result,
        Err(SegmentError::Inadmissible {
            event_id: last.body.event_id(),
            source: AdmissionError::SignatureInvalid
        })
    );
}

// ---------------------------------------------------------------------------------------------
// 8. Domain separation and purity.
// ---------------------------------------------------------------------------------------------

#[test]
fn the_segment_domain_is_distinct_from_every_neighbouring_domain() {
    assert_eq!(SEGMENT_DOMAIN, b"icn.n1.history-segment");
    for other in [
        DOMAIN,
        SIGNATURE_DOMAIN,
        COMMITMENT_DOMAIN,
        KDF_DOMAIN,
        DEVICE_ACT_DOMAIN,
        BUNDLE_DOMAIN,
    ] {
        assert_ne!(SEGMENT_DOMAIN, other);
    }
    // Segment bytes are not an evidence bundle, and bundle-domain bytes are not a segment.
    let f = fixture();
    let bytes = linear_bytes(&f);
    assert!(icn_identity::evidence_bundle::EvidenceBundle::decode(&bytes).is_err());
}

#[test]
fn the_module_has_no_clock_randomness_io_or_authority_fold() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/history_segment.rs");
    let text = std::fs::read_to_string(&path).expect("history_segment.rs must exist");
    let code_only: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "SystemTime",
        "Instant",
        "std::time",
        "chrono",
        "rand::",
        "OsRng",
        "HashMap",
        "HashSet",
        "std::fs",
        "std::io",
        "std::net",
        "tokio",
        "sled",
        "File",
        "supersede(",
        "resolve(",
        "derive_prefix(",
        "::derive(",
        "verify_strict",
        "verify(",
    ] {
        assert!(
            !code_only.contains(forbidden),
            "history_segment.rs must not use `{forbidden}`"
        );
    }
    assert_eq!(
        code_only.matches("derive(").count(),
        code_only.matches("#[derive(").count(),
        "every `derive(` in history_segment.rs must be the Rust attribute, not a fold call"
    );
    // The only path to an admitted store is AuthorityStore::ingest.
    assert_eq!(code_only.matches(".ingest(").count(), 1);
}

// ---------------------------------------------------------------------------------------------
// 9. Cross-implementation vectors. The expected bytes below are reproduced by an independent
// reference written from the ADR-0089 §3 prose alone (`tests/reference/history_segment_reference.py`),
// which reads these literals back purely as a comparison target.
//
// Inputs, stated in full: Subject root secret `[0x31; 32]`, context nonce `[0x31 ^ 0xa5 = 0x94; 32]`,
// horizon 4 (`authority_log_support::subject(0x31, 4)`); every fact signed by the generation-0
// authority key; devices A, B, C are the Ed25519 keys with seeds `[0x41; 32]`, `[0x42; 32]`,
// `[0x43; 32]`; authorize bodies carry capabilities {Sign} and no validity span.
//   INCEPTION: from 0 to 0, {inception}
//   LINEAR:    from 1 to 3, {authorize A @1, authorize B @2 (parent A@1), revoke A @3 (parent B@2)}
//   FORK:      from 0 to 2, {inception, authorize A @1, authorize B @2, authorize C @2} (both 2s
//              name A@1 as parent)
//   MULTIWITNESS: LINEAR, with authorize B also carrying a second valid witness by the same key:
//              the RFC 8032 signature with nonce prefix `[0x5a; 32]` (`second_witness`)
// ---------------------------------------------------------------------------------------------

const EXPECT_SUBJECT_ID_HEX: &str =
    "a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d";
const EXPECT_INCEPTION_SEGMENT_HEX: &str = "0000001669636e2e6e312e686973746f72792d7365676d656e740001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000000000000000000000000000000000000001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000009e0000001169636e2e617574686f726974792d6c6f6700010101216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a94949494949494949494949494949494949494949494949494949494949494940000000101216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a03e35d304806178a1a133aec9a7f61be02612aa8b936efff6f8150040cef9f0e00000001cd8c3ab7a2d1302f2566d49188b47700113347117dd846d883cddf10cd9c3613c3d9e4ba9d416152d21503b6faf3add9afa860784e00fbd6865ead25187e6701";
const EXPECT_INCEPTION_SEGMENT_ID_HEX: &str =
    "712acbb3050d4ac2c3649625ecc32196be1785f504325a4340195110f598f06f";
const EXPECT_LINEAR_SEGMENT_HEX: &str = "0000001669636e2e6e312e686973746f72792d7365676d656e740001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000010000000000000003000000031bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000000000000001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a01db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d00000001010000000001abbd59db38f897f9a93a43f5ac88a8c975dd02ac658930d4e404373afae4d5e629ee42b9cc317efd9ea2b9608db8f6f5565ed06f811c7c2cac03fe1dcb724b0d9791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000021bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a012152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12000000010100000000014bd1a312691f057ae0bd38200e13c9ab518d7e46f4020163080543cf92f1368bef5c490df84a20d11e8d4237efb23bc0b18ee9b42139b26d832e90fd706d8500b22bf1bcf74d7cf901f8218b4d6185c1a9c4cc09792c536e777f3eddb35e3dd3000000a20000001169636e2e617574686f726974792d6c6f67000104a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000039791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a01db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d000000011f40c4466d86e0394c211944911e60eace9633175e7314a4aa7c194c0846036cd74c73a41a80e4593bfedc444ee0b62d960110a2f45064fb7444da68dad5990c";
const EXPECT_LINEAR_SEGMENT_ID_HEX: &str =
    "f320220a862f9e626da7edb0837e18198539ea27f04919d94441cc61897468ba";
const EXPECT_FORK_SEGMENT_HEX: &str = "0000001669636e2e6e312e686973746f72792d7365676d656e740001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000000000000000000002000000041bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000000000000001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a01db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d00000001010000000001abbd59db38f897f9a93a43f5ac88a8c975dd02ac658930d4e404373afae4d5e629ee42b9cc317efd9ea2b9608db8f6f5565ed06f811c7c2cac03fe1dcb724b0d64b8a7e2c69bba64e972c5b2d1a29df390614b76bc3ba89e5ed83844f37d96d5000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000021bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a0122fc297792f0b6ffc0bfcfdb7edb0c0aa14e025a365ec0e342e86e3829cb74b6000000010100000000016eac35140530f46220f7d52e8e58424cbb1fdff9b6b82adda848ff8f45ac99ee7db46a1b30f7fcea42510df06cc6d1c2f777db0e922df17e7f6045ed14d792059791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000021bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a012152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12000000010100000000014bd1a312691f057ae0bd38200e13c9ab518d7e46f4020163080543cf92f1368bef5c490df84a20d11e8d4237efb23bc0b18ee9b42139b26d832e90fd706d8500a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000009e0000001169636e2e617574686f726974792d6c6f6700010101216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a94949494949494949494949494949494949494949494949494949494949494940000000101216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a03e35d304806178a1a133aec9a7f61be02612aa8b936efff6f8150040cef9f0e00000001cd8c3ab7a2d1302f2566d49188b47700113347117dd846d883cddf10cd9c3613c3d9e4ba9d416152d21503b6faf3add9afa860784e00fbd6865ead25187e6701";
const EXPECT_FORK_SEGMENT_ID_HEX: &str =
    "d142dba4e58c428fd2b632270622423d4578749503c091aa5e978262f8ec8dd3";
const EXPECT_MULTIWITNESS_SEGMENT_HEX: &str = "0000001669636e2e6e312e686973746f72792d7365676d656e740001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000010000000000000003000000031bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000000000000001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a01db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d00000001010000000001abbd59db38f897f9a93a43f5ac88a8c975dd02ac658930d4e404373afae4d5e629ee42b9cc317efd9ea2b9608db8f6f5565ed06f811c7c2cac03fe1dcb724b0d9791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000021bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a012152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12000000010100000000024bd1a312691f057ae0bd38200e13c9ab518d7e46f4020163080543cf92f1368bef5c490df84a20d11e8d4237efb23bc0b18ee9b42139b26d832e90fd706d85006004acdf6ff26e1defa492c40dfc3af19928307fd830545daa696b6c9ccc91a30a592df0f44635066fff51adf159b304ed4933ddebdc5929806bb33403a72d0eb22bf1bcf74d7cf901f8218b4d6185c1a9c4cc09792c536e777f3eddb35e3dd3000000a20000001169636e2e617574686f726974792d6c6f67000104a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000039791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a01db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d000000011f40c4466d86e0394c211944911e60eace9633175e7314a4aa7c194c0846036cd74c73a41a80e4593bfedc444ee0b62d960110a2f45064fb7444da68dad5990c";
const EXPECT_MULTIWITNESS_SEGMENT_ID_HEX: &str =
    "afc210fa95088d65f26bc943a384ab6a56d03ba6c8b21c80c01b41d509a614e4";

fn vector_segments() -> (SubjectId, [AuthorityHistorySegment; 4]) {
    let f = fixture_with(0x31, 0x41, 0x42, 0x43);
    let s = f.s.subject;
    let inception = segment(s, 0, 0, &[&f.s.inception]).unwrap();
    let linear = segment(s, 1, 3, &[&f.auth_a, &f.auth_b, &f.revoke_a]).unwrap();
    let fork = segment(s, 0, 2, &[&f.s.inception, &f.auth_a, &f.auth_b, &f.auth_c]).unwrap();
    let resigned = second_witness(&f.auth_b.body, &f.s.root.authority_signing_key(0), 0x5a);
    let multi = linear_with_witnesses(
        &f,
        &f.auth_b,
        BTreeSet::from([f.auth_b.signature, resigned]),
    );
    (s, [inception, linear, fork, multi])
}

#[test]
fn canonical_vectors_match_the_independent_reference() {
    let (s, segments) = vector_segments();
    assert_eq!(hex::encode(s.as_bytes()), EXPECT_SUBJECT_ID_HEX);
    let pinned = [
        (
            EXPECT_INCEPTION_SEGMENT_HEX,
            EXPECT_INCEPTION_SEGMENT_ID_HEX,
        ),
        (EXPECT_LINEAR_SEGMENT_HEX, EXPECT_LINEAR_SEGMENT_ID_HEX),
        (EXPECT_FORK_SEGMENT_HEX, EXPECT_FORK_SEGMENT_ID_HEX),
        (
            EXPECT_MULTIWITNESS_SEGMENT_HEX,
            EXPECT_MULTIWITNESS_SEGMENT_ID_HEX,
        ),
    ];
    for (seg, (bytes_hex, id_hex)) in segments.iter().zip(pinned) {
        assert_eq!(hex::encode(seg.canonical_bytes()), bytes_hex);
        assert_eq!(hex::encode(seg.segment_id()), id_hex);
        // The pinned bytes decode to the same segment and admit, every witness included.
        let decoded = AuthorityHistorySegment::decode(&hex::decode(bytes_hex).unwrap()).unwrap();
        assert_eq!(&decoded, seg);
        let pairs: usize = seg.facts().values().map(|fact| fact.witnesses.len()).sum();
        assert_eq!(decoded.admit().unwrap().store().witness_count(), pairs);
    }
}
