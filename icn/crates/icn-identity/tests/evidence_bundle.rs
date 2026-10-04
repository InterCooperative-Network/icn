//! N4-A portable evidence bundle — the transport boundary over canonical N1 facts plus one signed
//! device act (`docs/architecture/N4A_PORTABLE_EVIDENCE_BUNDLE.md`).
//!
//! The one question this suite answers, from the reviewer's side:
//!
//! > Can these bytes transport exactly the evidence needed by the existing N1 + N4-A semantics,
//! > without becoming a second semantic engine?
//!
//! So every test here is one of: the bytes are canonical and deterministic; a structural defect is
//! refused at decode; a bad witness is refused at N1 admission; or the **existing** verifier,
//! handed the store rebuilt from the bytes, reaches the verdict it reached on the original store.
//! No test asserts a verdict the verifier did not already produce before transport.
//!
//! Fixture identities only. Every key is derived from a seed byte; nothing here is a real person
//! or a real device.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use std::collections::BTreeSet;

use authority_log_support::{
    authorize_at, device, permutations, revoke_at, store_of, stranger, subject, Subject,
};
use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    derive_prefix, AdmissionError, AuthorityBody, AuthorityStore, AuthorityView, CodecError,
    DeviceCapability, EventId, PrincipalKey, SignedAuthorityEvent, SubjectId, WitnessSignature,
    COMMITMENT_DOMAIN, DOMAIN, KDF_DOMAIN, MAX_POSITION, SIGNATURE_DOMAIN,
};
use icn_identity::device_authority::{
    sign_device_act, verify_device_act, DeviceActError, DeviceActV1, DeviceActVerifyError,
    DeviceAuthorityRefusal, SignedDeviceAct, DEVICE_ACT_DOMAIN, MAX_DEVICE_ACT_PAYLOAD,
};
use icn_identity::evidence_bundle::{BundleError, EvidenceBundle, BUNDLE_DOMAIN, BUNDLE_VERSION};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------------------------
// Fixture: S with devices A (authorized at 1) and B (authorized at 2), A revoked at 3.
// The same forcing scenario as `device_authority.rs`, so verdicts can be compared across transport.
// ---------------------------------------------------------------------------------------------

const DEVICE_A_SEED: u8 = 0x11;
const DEVICE_B_SEED: u8 = 0x22;
const PAYLOAD: &[u8] = b"fixture:open-workspace";

struct Fixture {
    s: Subject,
    a_key: SigningKey,
    a: PrincipalKey,
    b_key: SigningKey,
    b: PrincipalKey,
    /// `[inception, authorize A @1, authorize B @2, revoke A @3]`.
    events: Vec<SignedAuthorityEvent>,
    store: AuthorityStore,
}

fn fixture() -> Fixture {
    let s = subject(0x01, 4);
    let (a_key, a) = stranger(DEVICE_A_SEED);
    let (b_key, b) = stranger(DEVICE_B_SEED);
    let auth_a = authorize_at(&s, 0, 1, s.genesis(), a);
    let auth_b = authorize_at(&s, 0, 2, auth_a.body.event_id(), b);
    let revoke_a = revoke_at(&s, 0, 3, auth_b.body.event_id(), a);
    let events = vec![s.inception.clone(), auth_a, auth_b, revoke_a];
    let store = store_of(&events);
    Fixture {
        s,
        a_key,
        a,
        b_key,
        b,
        events,
        store,
    }
}

fn signed_act(
    subject: SubjectId,
    device: PrincipalKey,
    key: &SigningKey,
    position: u64,
) -> SignedDeviceAct {
    let act = DeviceActV1::new(
        subject,
        device,
        DeviceCapability::Sign,
        position,
        PAYLOAD.to_vec(),
    )
    .unwrap();
    sign_device_act(&act, key).unwrap()
}

/// Round-trip helper: bytes → decode → admit → the existing verifier at the bundle's own scope.
fn verify_bytes(bytes: &[u8]) -> Result<DeviceAuthorityRefusalOrEvidence, BundleError> {
    let bundle = EvidenceBundle::decode(bytes)?;
    let store = bundle.admit()?;
    Ok(verify_device_act(
        bundle.act(),
        bundle.subject(),
        &store,
        bundle.evaluation_position(),
    ))
}

type DeviceAuthorityRefusalOrEvidence =
    Result<icn_identity::device_authority::DeviceAuthorityEvidence, DeviceActVerifyError>;

// Minimal raw framing helpers for building hostile bytes by hand. These deliberately do not call
// any encoder in `icn-identity`; the point is to construct bytes the encoder would never emit.
fn lp(x: &[u8]) -> Vec<u8> {
    let mut out = (x.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(x);
    out
}
fn u32be(n: u32) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}
fn u64be(n: u64) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}

/// One hand-framed fact.
fn raw_fact(event_id: &[u8; 32], body: &[u8], witnesses: &[[u8; 64]]) -> Vec<u8> {
    let mut out = event_id.to_vec();
    out.extend(lp(body));
    out.extend(u32be(witnesses.len() as u32));
    for w in witnesses {
        out.extend_from_slice(w);
    }
    out
}

/// A hand-framed bundle, with every field chosen by the test.
fn raw_bundle(
    subject: &[u8; 32],
    position: u64,
    facts: &[Vec<u8>],
    act_bytes: &[u8],
    act_sig: &[u8; 64],
) -> Vec<u8> {
    let mut out = lp(BUNDLE_DOMAIN);
    out.extend(BUNDLE_VERSION.to_be_bytes());
    out.extend_from_slice(subject);
    out.extend(u64be(position));
    out.extend(u32be(facts.len() as u32));
    for f in facts {
        out.extend_from_slice(f);
    }
    out.extend(lp(act_bytes));
    out.extend_from_slice(act_sig);
    out
}

/// The facts of a store for `subject` through `through`, hand-framed in canonical order, so a
/// test can splice one and keep the rest exact.
fn raw_facts_of(store: &AuthorityStore, subject: SubjectId, through: u64) -> Vec<Vec<u8>> {
    let mut by_id: Vec<(EventId, AuthorityBody)> = store
        .bodies_for(subject)
        .into_iter()
        .filter(|b| b.position() <= through)
        .map(|b| (b.event_id(), b))
        .collect();
    by_id.sort_by_key(|(id, _)| *id);
    by_id
        .into_iter()
        .map(|(id, body)| {
            let sigs: Vec<[u8; 64]> = store
                .witnesses_for(id)
                .into_iter()
                .map(|w| *w.signature.as_bytes())
                .collect();
            raw_fact(id.as_bytes(), &body.canonical_bytes(), &sigs)
        })
        .collect()
}

/// A second, distinct, valid Ed25519 signature over one body (same scalar, different nonce
/// prefix). The authority-log model must carry it as a second witness of one body, not a fork.
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
// 1. Canonical round trip.
// ---------------------------------------------------------------------------------------------

#[test]
fn canonical_round_trip_is_byte_exact() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let bundle = EvidenceBundle::assemble(&f.store, &act).unwrap();
    let bytes = bundle.canonical_bytes();

    let decoded = EvidenceBundle::decode(&bytes).unwrap();
    assert_eq!(decoded, bundle);
    assert_eq!(decoded.canonical_bytes(), bytes);
    assert_eq!(decoded.bundle_id(), bundle.bundle_id());
    assert_eq!(bundle.bundle_id(), sha256(&bytes));

    assert_eq!(decoded.subject(), f.s.subject);
    assert_eq!(decoded.evaluation_position(), 3);
    assert_eq!(decoded.act(), &act);
    // All four facts are at or below 3, each with exactly the one witness the store holds.
    assert_eq!(decoded.facts().len(), 4);
    assert!(decoded
        .facts()
        .values()
        .all(|fact| fact.witnesses.len() == 1));

    // The store rebuilt through canonical admission is the store the bundle came from.
    assert_eq!(decoded.admit().unwrap(), f.store);
}

// ---------------------------------------------------------------------------------------------
// 2. Deterministic bytes under every ingest order, and under join.
// ---------------------------------------------------------------------------------------------

#[test]
fn bytes_are_identical_under_every_ingest_permutation_and_join() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let reference = EvidenceBundle::assemble(&f.store, &act).unwrap();
    let reference_bytes = reference.canonical_bytes();

    let orders = permutations(&f.events);
    assert_eq!(orders.len(), 24);
    for order in &orders {
        let store = store_of(order);
        let bundle = EvidenceBundle::assemble(&store, &act).unwrap();
        assert_eq!(bundle.canonical_bytes(), reference_bytes);
        assert_eq!(bundle.bundle_id(), reference.bundle_id());
    }

    // Every split-and-join of the same facts yields the same bytes.
    for order in &orders {
        for split in 0..=order.len() {
            let left = store_of(&order[..split]);
            let right = store_of(&order[split..]);
            let joined = AuthorityStore::join(&left, &right);
            assert_eq!(
                EvidenceBundle::assemble(&joined, &act)
                    .unwrap()
                    .canonical_bytes(),
                reference_bytes
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Multiple witnesses over one body survive transport as witnesses, not as a fork.
// ---------------------------------------------------------------------------------------------

#[test]
fn several_witnesses_over_one_body_survive_transport() {
    let f = fixture();
    let auth_b = &f.events[2];
    let resigned = SignedAuthorityEvent::new(
        auth_b.body.clone(),
        second_witness(&auth_b.body, &f.s.root.authority_signing_key(0), 0x5a),
    );
    assert_ne!(resigned.signature, auth_b.signature);

    let mut store = f.store.clone();
    assert!(store.ingest(&resigned).is_ok());
    assert_eq!(store.body_count(), 4);
    assert_eq!(store.witness_count(), 5);

    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let bundle = EvidenceBundle::assemble(&store, &act).unwrap();
    let carried = &bundle.facts()[&auth_b.body.event_id()];
    assert_eq!(carried.witnesses.len(), 2);
    assert!(carried.witnesses.contains(&auth_b.signature));
    assert!(carried.witnesses.contains(&resigned.signature));

    let bytes = bundle.canonical_bytes();
    let decoded = EvidenceBundle::decode(&bytes).unwrap();
    let rebuilt = decoded.admit().unwrap();
    assert_eq!(rebuilt, store);
    assert_eq!(rebuilt.witness_count(), 5);
    assert_eq!(rebuilt.body_count(), 4);

    // Which witness arrived first does not change the bytes.
    let mut other_order = store_of(&[f.events[0].clone(), f.events[1].clone(), resigned.clone()]);
    other_order.ingest(auth_b).unwrap();
    other_order.ingest(&f.events[3]).unwrap();
    assert_eq!(
        EvidenceBundle::assemble(&other_order, &act)
            .unwrap()
            .canonical_bytes(),
        bytes
    );

    // And the verifier is unmoved: one body, two witnesses, no dispute.
    let ev = verify_device_act(&act, f.s.subject, &rebuilt, 3).unwrap();
    assert_eq!(ev.grant.granted_at, 2);
}

// ---------------------------------------------------------------------------------------------
// 4. Corrupt event_id / body association refuses.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_stated_event_id_that_is_not_the_body_digest_is_refused() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let facts = raw_facts_of(&f.store, f.s.subject, 3);
    let act_bytes = act.act.canonical_bytes();
    let act_sig = *act.signature.as_bytes();

    // (a) Corrupt the stated digest of the first fact. Re-sorting is not needed: the test checks
    // the digest error fires before the order check could matter for a *decreasing* flip, so flip
    // a low-order byte and keep the fact in place.
    let mut facts_a = facts.clone();
    facts_a[0][31] ^= 0x01;
    let computed = EventId::from_bytes(facts[0][..32].try_into().unwrap());
    let mut stated_bytes = *computed.as_bytes();
    stated_bytes[31] ^= 0x01;
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts_a, &act_bytes, &act_sig);
    match EvidenceBundle::decode(&bytes) {
        Err(BundleError::EventIdMismatch {
            stated,
            computed: c,
        }) => {
            assert_eq!(stated, EventId::from_bytes(stated_bytes));
            assert_eq!(c, computed);
        }
        // Flipping the digest may also break ascending order against its neighbour, which is an
        // equally fail-closed refusal.
        Err(BundleError::FactOrder) => {}
        other => panic!("expected EventIdMismatch, got {other:?}"),
    }

    // (b) Swap the bodies of two facts while keeping their stated digests. Both facts decode;
    // neither is the body its digest names.
    let mut facts_b = facts.clone();
    let body_0 = f.store.bodies_for(f.s.subject);
    let mut by_id: Vec<AuthorityBody> = body_0.into_iter().collect();
    by_id.sort_by_key(|b| b.event_id());
    let sig_of = |b: &AuthorityBody| -> Vec<[u8; 64]> {
        f.store
            .witnesses_for(b.event_id())
            .into_iter()
            .map(|w| *w.signature.as_bytes())
            .collect()
    };
    facts_b[0] = raw_fact(
        by_id[0].event_id().as_bytes(),
        &by_id[1].canonical_bytes(),
        &sig_of(&by_id[0]),
    );
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts_b, &act_bytes, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::EventIdMismatch { .. })
    ));

    // (c) A body that still decodes but is not the one witnessed: flip a byte inside the
    // inception's context nonce. The digest no longer matches.
    let inception_id = f.s.inception.body.event_id();
    let idx = facts
        .iter()
        .position(|raw| raw[..32] == *inception_id.as_bytes())
        .unwrap();
    let mut facts_c = facts.clone();
    // Layout inside the fact: 32 (event_id) + 4 (len) + LP(domain)=21 + 2 + 1 + 33 (signer) …
    // the context nonce begins at offset 32 + 4 + 21 + 2 + 1 + 33 = 93.
    facts_c[idx][93] ^= 0xff;
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts_c, &act_bytes, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::EventIdMismatch { .. })
    ));
}

// ---------------------------------------------------------------------------------------------
// 5. Malformed N1 body refuses, with N1's own error.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_malformed_n1_body_is_refused_by_the_n1_decoder() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let act_bytes = act.act.canonical_bytes();
    let act_sig = *act.signature.as_bytes();
    let facts = raw_facts_of(&f.store, f.s.subject, 3);

    // Wrong body version (offset 32 + 4 + 21 = 57 is the u16 version's high byte).
    let mut bad_version = facts.clone();
    bad_version[0][58] ^= 0x01;
    let bytes = raw_bundle(
        f.s.subject.as_bytes(),
        3,
        &bad_version,
        &act_bytes,
        &act_sig,
    );
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Body {
            source: CodecError::UnsupportedVersion(_),
            ..
        })
    ));

    // Unknown kind tag (offset 59).
    let mut bad_kind = facts.clone();
    bad_kind[0][59] = 0x7f;
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &bad_kind, &act_bytes, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Body {
            source: CodecError::UnknownKind(0x7f),
            ..
        })
    ));

    // A body with one trailing byte inside its own length prefix: the inner decoder's strictness
    // is what fires, not a bundle-level guess.
    let body = f.s.inception.body.canonical_bytes();
    let mut padded = body.clone();
    padded.push(0x00);
    let sigs = vec![*f.s.inception.signature.as_bytes()];
    let fact = raw_fact(f.s.inception.body.event_id().as_bytes(), &padded, &sigs);
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &[fact], &act_bytes, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Body {
            source: CodecError::TrailingBytes(1),
            ..
        })
    ));

    // Act bytes placed where a body belongs are refused as a body: the domains are disjoint.
    let fact = raw_fact(&sha256(&act_bytes), &act_bytes, &sigs);
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &[fact], &act_bytes, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Body {
            source: CodecError::BadDomain,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------------------------
// 6. Malformed device act refuses, with N4-A's own error.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_malformed_device_act_is_refused_by_the_n4a_decoder() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let facts = raw_facts_of(&f.store, f.s.subject, 3);
    let act_sig = *act.signature.as_bytes();

    // An N1 body in the act slot.
    let bytes = raw_bundle(
        f.s.subject.as_bytes(),
        3,
        &facts,
        &f.s.inception.body.canonical_bytes(),
        &act_sig,
    );
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Act(DeviceActError::Codec(
            CodecError::BadDomain
        )))
    ));

    // An act whose bound position is 0: well-framed, refused by N4-A's own bound.
    let mut zero_pos = act.act.canonical_bytes();
    let pos_off = 4 + DEVICE_ACT_DOMAIN.len() + 2 + 32 + 33;
    zero_pos[pos_off..pos_off + 8].copy_from_slice(&0u64.to_be_bytes());
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts, &zero_pos, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Act(DeviceActError::PositionOutOfRange(0)))
    ));

    // A SubjectId in the act's device slot (tag 0x00 instead of the principal tag).
    let mut untagged = act.act.canonical_bytes();
    untagged[4 + DEVICE_ACT_DOMAIN.len() + 2 + 32] = 0x00;
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts, &untagged, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Act(DeviceActError::Codec(
            CodecError::BadPrincipalTag(0x00)
        )))
    ));

    // Truncated act inside an otherwise exact bundle.
    let mut short = act.act.canonical_bytes();
    short.pop();
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts, &short, &act_sig);
    assert!(matches!(
        EvidenceBundle::decode(&bytes),
        Err(BundleError::Act(DeviceActError::Codec(_)))
    ));
}

// ---------------------------------------------------------------------------------------------
// 7 & 8. Truncation and trailing bytes refuse.
// ---------------------------------------------------------------------------------------------

#[test]
fn every_truncation_and_any_trailing_byte_is_refused() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let bytes = EvidenceBundle::assemble(&f.store, &act)
        .unwrap()
        .canonical_bytes();

    for len in 0..bytes.len() {
        assert!(
            EvidenceBundle::decode(&bytes[..len]).is_err(),
            "prefix of {len} bytes must not decode"
        );
    }

    let mut trailing = bytes.clone();
    trailing.push(0x00);
    assert_eq!(
        EvidenceBundle::decode(&trailing).unwrap_err(),
        BundleError::Framing(CodecError::TrailingBytes(1))
    );
    let mut trailing_more = bytes.clone();
    trailing_more.extend_from_slice(&bytes);
    assert_eq!(
        EvidenceBundle::decode(&trailing_more).unwrap_err(),
        BundleError::Framing(CodecError::TrailingBytes(bytes.len()))
    );
}

// ---------------------------------------------------------------------------------------------
// 9. Bounded fields refuse according to the contract: input-bounded counts and lengths, N1's
//    position bound, N4-A's payload bound. No new limit is introduced.
// ---------------------------------------------------------------------------------------------

#[test]
fn impossible_counts_lengths_and_out_of_bound_positions_are_refused() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let act_bytes = act.act.canonical_bytes();
    let act_sig = *act.signature.as_bytes();
    let facts = raw_facts_of(&f.store, f.s.subject, 3);

    // Wrong domain, wrong version.
    let good = raw_bundle(f.s.subject.as_bytes(), 3, &facts, &act_bytes, &act_sig);
    let mut wrong_domain = good.clone();
    wrong_domain[4] ^= 0x01;
    assert_eq!(
        EvidenceBundle::decode(&wrong_domain).unwrap_err(),
        BundleError::Framing(CodecError::BadDomain)
    );
    let mut wrong_version = good.clone();
    let ver_off = 4 + BUNDLE_DOMAIN.len();
    wrong_version[ver_off + 1] = 0x02;
    assert_eq!(
        EvidenceBundle::decode(&wrong_version).unwrap_err(),
        BundleError::Framing(CodecError::UnsupportedVersion(2))
    );

    // A declared fact count the remaining bytes cannot hold.
    let mut huge_count = good.clone();
    let count_off = ver_off + 2 + 32 + 8;
    huge_count[count_off..count_off + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        EvidenceBundle::decode(&huge_count).unwrap_err(),
        BundleError::Framing(CodecError::CountOverflow(u32::MAX))
    );

    // A declared body length that overruns the input.
    let mut long_body = good.clone();
    let body_len_off = count_off + 4 + 32;
    long_body[body_len_off..body_len_off + 4].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
    assert_eq!(
        EvidenceBundle::decode(&long_body).unwrap_err(),
        BundleError::Framing(CodecError::UnexpectedEnd {
            field: "canonical_body"
        })
    );

    // A declared witness count the remaining bytes cannot hold.
    let body_len = u32::from_be_bytes(good[body_len_off..body_len_off + 4].try_into().unwrap());
    let wit_count_off = body_len_off + 4 + body_len as usize;
    let mut huge_witnesses = good.clone();
    huge_witnesses[wit_count_off..wit_count_off + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        EvidenceBundle::decode(&huge_witnesses).unwrap_err(),
        BundleError::Framing(CodecError::CountOverflow(u32::MAX))
    );

    // Position 0 and position above the N1 bound, with no facts so the header check is isolated.
    for (pos, act_pos_ok) in [(0u64, false), (MAX_POSITION + 1, false)] {
        let bytes = raw_bundle(f.s.subject.as_bytes(), pos, &[], &act_bytes, &act_sig);
        assert_eq!(
            EvidenceBundle::decode(&bytes).unwrap_err(),
            BundleError::PositionOutOfRange(pos)
        );
        let _ = act_pos_ok;
    }

    // An act payload above N4-A's bound, spliced in by hand (the constructor refuses to build it).
    let mut big_act = lp(DEVICE_ACT_DOMAIN);
    big_act.extend(1u16.to_be_bytes());
    big_act.extend_from_slice(f.s.subject.as_bytes());
    big_act.push(0x01);
    big_act.extend_from_slice(&f.b.as_bytes());
    big_act.extend(u64be(3));
    big_act.push(0x01);
    big_act.extend(lp(&vec![0u8; MAX_DEVICE_ACT_PAYLOAD + 1]));
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts, &big_act, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::Act(DeviceActError::PayloadTooLarge(MAX_DEVICE_ACT_PAYLOAD + 1))
    );

    // Assembly refuses an act whose position is out of N1's bound — but `DeviceActV1::new`
    // already refuses to build one, so there is no act to assemble around.
    assert!(matches!(
        DeviceActV1::new(
            f.s.subject,
            f.b,
            DeviceCapability::Sign,
            MAX_POSITION + 1,
            vec![]
        ),
        Err(DeviceActError::PositionOutOfRange(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// Canonical-identity strictness: order, duplicates, empty witness lists, scope, act agreement.
// ---------------------------------------------------------------------------------------------

#[test]
fn reordered_or_duplicated_facts_and_witnesses_are_refused() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let act_bytes = act.act.canonical_bytes();
    let act_sig = *act.signature.as_bytes();
    let facts = raw_facts_of(&f.store, f.s.subject, 3);

    let mut swapped = facts.clone();
    swapped.swap(0, 1);
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &swapped, &act_bytes, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::FactOrder
    );

    let mut duplicated = facts.clone();
    duplicated.insert(1, facts[0].clone());
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &duplicated, &act_bytes, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::FactOrder
    );

    // Two witnesses in the wrong order, and a duplicated witness.
    let auth_b = &f.events[2];
    let w1 = *auth_b.signature.as_bytes();
    let w2 = *second_witness(&auth_b.body, &f.s.root.authority_signing_key(0), 0x5a).as_bytes();
    let (lo, hi) = if w1 < w2 { (w1, w2) } else { (w2, w1) };
    let id = auth_b.body.event_id();
    let body = auth_b.body.canonical_bytes();
    for (witnesses, label) in [([hi, lo], "descending"), ([lo, lo], "duplicate")] {
        let fact = raw_fact(id.as_bytes(), &body, &witnesses);
        let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &[fact], &act_bytes, &act_sig);
        assert_eq!(
            EvidenceBundle::decode(&bytes).unwrap_err(),
            BundleError::WitnessOrder { event_id: id },
            "{label} witnesses must be refused"
        );
    }

    // A fact with zero witnesses is not a fact.
    let fact = raw_fact(id.as_bytes(), &body, &[]);
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &[fact], &act_bytes, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::NoWitness { event_id: id }
    );
}

#[test]
fn facts_outside_the_bundle_scope_and_disagreeing_acts_are_refused() {
    let f = fixture();
    let act3 = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let act_bytes = act3.act.canonical_bytes();
    let act_sig = *act3.signature.as_bytes();

    // A fact for another Subject.
    let other = subject(0x02, 4);
    let foreign = raw_fact(
        other.inception.body.event_id().as_bytes(),
        &other.inception.body.canonical_bytes(),
        &[*other.inception.signature.as_bytes()],
    );
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &[foreign], &act_bytes, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::FactSubjectMismatch {
            event_id: other.inception.body.event_id()
        }
    );

    // Header Subject differs from the act's, with no facts to trip first.
    let bytes = raw_bundle(other.subject.as_bytes(), 3, &[], &act_bytes, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::ActSubjectMismatch
    );

    // Header position differs from the act's.
    let bytes = raw_bundle(f.s.subject.as_bytes(), 2, &[], &act_bytes, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::ActPositionMismatch { act: 3, bundle: 2 }
    );
}

// ---------------------------------------------------------------------------------------------
// Admission is N1's, and it is where a bad witness is refused — after decode, before any verdict.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_witness_that_does_not_verify_is_refused_at_n1_admission_not_normalized() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let act_bytes = act.act.canonical_bytes();
    let act_sig = *act.signature.as_bytes();
    let mut facts = raw_facts_of(&f.store, f.s.subject, 3);

    // Flip one bit of the single witness of the first fact. The bundle is structurally canonical.
    let last = facts[0].len() - 1;
    facts[0][last] ^= 0x01;
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts, &act_bytes, &act_sig);
    let bundle = EvidenceBundle::decode(&bytes).expect("structure is canonical");
    let refused = bundle.admit().unwrap_err();
    assert!(matches!(
        refused,
        BundleError::Inadmissible {
            source: AdmissionError::SignatureInvalid,
            ..
        }
    ));

    // A witness by a stranger over the right body: admissible-looking shape, wrong key.
    let (stranger_key, _) = stranger(0x99);
    let forged = icn_identity::authority_log::sign_body(f.events[1].body.clone(), &stranger_key);
    assert_eq!(forged.body, f.events[1].body);
    let mut facts = raw_facts_of(&f.store, f.s.subject, 3);
    let idx = facts
        .iter()
        .position(|raw| raw[..32] == *forged.body.event_id().as_bytes())
        .unwrap();
    facts[idx] = raw_fact(
        forged.body.event_id().as_bytes(),
        &forged.body.canonical_bytes(),
        &[*forged.signature.as_bytes()],
    );
    let bytes = raw_bundle(f.s.subject.as_bytes(), 3, &facts, &act_bytes, &act_sig);
    let bundle = EvidenceBundle::decode(&bytes).unwrap();
    assert!(matches!(
        bundle.admit().unwrap_err(),
        BundleError::Inadmissible {
            source: AdmissionError::SignatureInvalid,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------------------------
// 10. A valid bundle reconstructs evidence on which the existing verifier reaches the same verdict.
// ---------------------------------------------------------------------------------------------

#[test]
fn the_verifier_reaches_the_same_verdict_after_transport() {
    let f = fixture();

    // Accepted: B at 3.
    let b_act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let direct = verify_device_act(&b_act, f.s.subject, &f.store, 3);
    let bytes = EvidenceBundle::assemble(&f.store, &b_act)
        .unwrap()
        .canonical_bytes();
    let transported = verify_bytes(&bytes).unwrap();
    assert_eq!(transported, direct);
    let ev = transported.unwrap();
    assert_eq!(ev.device, f.b);
    assert_eq!(ev.grant.granted_at, 2);
    assert_eq!(ev.generation, 0);

    // Refused: A at 3, after revocation. The refusal class is identical.
    let a_act = signed_act(f.s.subject, f.a, &f.a_key, 3);
    let direct = verify_device_act(&a_act, f.s.subject, &f.store, 3);
    assert_eq!(
        direct,
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        ))
    );
    let bytes = EvidenceBundle::assemble(&f.store, &a_act)
        .unwrap()
        .canonical_bytes();
    assert_eq!(verify_bytes(&bytes).unwrap(), direct);

    // Unknown Subject: the bundler holds nothing for S. The bundle is empty, decodes, admits to an
    // empty store, and the verifier — not the decoder — says so.
    let ghost = subject(0x07, 4);
    let ghost_act = signed_act(ghost.subject, f.b, &f.b_key, 3);
    let bundle = EvidenceBundle::assemble(&f.store, &ghost_act).unwrap();
    assert!(bundle.facts().is_empty());
    let bytes = bundle.canonical_bytes();
    assert_eq!(
        verify_bytes(&bytes).unwrap(),
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::SubjectUnknown
        ))
    );
}

// ---------------------------------------------------------------------------------------------
// 11 & 12. A fork through E remains a fork; a gap through E remains a gap.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_fork_through_the_evaluated_position_survives_transport_as_a_fork() {
    let s = subject(0x05, 4);
    let (b_key, b) = stranger(DEVICE_B_SEED);
    let auth_b = authorize_at(&s, 0, 1, s.genesis(), b);
    let fork_1 = authorize_at(&s, 0, 2, auth_b.body.event_id(), device(0x61));
    let fork_2 = authorize_at(&s, 0, 2, auth_b.body.event_id(), device(0x62));
    let store = store_of(&[s.inception.clone(), auth_b, fork_1.clone(), fork_2.clone()]);

    let act = signed_act(s.subject, b, &b_key, 2);
    let direct = verify_device_act(&act, s.subject, &store, 2);
    assert_eq!(
        direct,
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::AuthorityHalted { disputed_at: 2 }
        ))
    );

    let bundle = EvidenceBundle::assemble(&store, &act).unwrap();
    // Both candidates are carried; the bundler chose neither.
    assert!(bundle.facts().contains_key(&fork_1.body.event_id()));
    assert!(bundle.facts().contains_key(&fork_2.body.event_id()));
    assert_eq!(bundle.facts().len(), 4);

    let bytes = bundle.canonical_bytes();
    assert_eq!(verify_bytes(&bytes).unwrap(), direct);
    let rebuilt = EvidenceBundle::decode(&bytes).unwrap().admit().unwrap();
    assert!(matches!(
        derive_prefix(s.subject, &rebuilt, 2),
        AuthorityView::Halted { disputed_at: 2, .. }
    ));

    // Before the fork the same facts evaluate cleanly — and that bundle at E = 1 does not carry
    // position 2 at all, so the fork is neither hidden nor smuggled; it is simply out of scope.
    let act1 = signed_act(s.subject, b, &b_key, 1);
    let bundle1 = EvidenceBundle::assemble(&store, &act1).unwrap();
    assert_eq!(bundle1.facts().len(), 2);
    assert!(verify_bytes(&bundle1.canonical_bytes()).unwrap().is_ok());
}

#[test]
fn a_gap_through_the_evaluated_position_survives_transport_as_a_gap() {
    let s = subject(0x06, 4);
    let (b_key, b) = stranger(DEVICE_B_SEED);
    let auth_b = authorize_at(&s, 0, 1, s.genesis(), b);
    // Position 2 is missing; position 3 names a parent nobody retained.
    let orphan = authorize_at(&s, 0, 3, auth_b.body.event_id(), device(0x63));
    let store = store_of(&[s.inception.clone(), auth_b, orphan.clone()]);

    let act = signed_act(s.subject, b, &b_key, 3);
    let direct = verify_device_act(&act, s.subject, &store, 3);
    assert_eq!(
        direct,
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::PrefixIncomplete {
                frontier: 2,
                required: 3,
            }
        ))
    );

    let bundle = EvidenceBundle::assemble(&store, &act).unwrap();
    // The orphan is carried too: it is at or below E, and whether it is selectable is the fold's
    // business, not the bundler's.
    assert!(bundle.facts().contains_key(&orphan.body.event_id()));
    assert_eq!(bundle.facts().len(), 3);
    assert_eq!(verify_bytes(&bundle.canonical_bytes()).unwrap(), direct);
}

// ---------------------------------------------------------------------------------------------
// 13. A later revocation outside E is not required for a historical proof — and cannot be carried.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_revocation_after_the_evaluated_position_is_neither_required_nor_carriable() {
    let f = fixture();
    let revoke_a = &f.events[3];
    assert_eq!(revoke_a.body.position(), 3);

    // A at E = 2: authorized then, revoked later. The full store says so …
    let a_at_2 = signed_act(f.s.subject, f.a, &f.a_key, 2);
    let direct = verify_device_act(&a_at_2, f.s.subject, &f.store, 2);
    assert!(direct.is_ok());

    // … and the bundle for E = 2 carries exactly the three facts at or below 2.
    let bundle = EvidenceBundle::assemble(&f.store, &a_at_2).unwrap();
    assert_eq!(bundle.facts().len(), 3);
    assert!(!bundle.facts().contains_key(&revoke_a.body.event_id()));
    assert_eq!(verify_bytes(&bundle.canonical_bytes()).unwrap(), direct);

    // Splicing the revoke into the E = 2 bundle is refused at decode: a fact above E is out of
    // scope. The later fact cannot be smuggled into the historical proof, in either direction.
    let act_bytes = a_at_2.act.canonical_bytes();
    let act_sig = *a_at_2.signature.as_bytes();
    let facts = raw_facts_of(&f.store, f.s.subject, 3); // includes the revoke at 3
    let bytes = raw_bundle(f.s.subject.as_bytes(), 2, &facts, &act_bytes, &act_sig);
    assert_eq!(
        EvidenceBundle::decode(&bytes).unwrap_err(),
        BundleError::FactOutsideScope {
            event_id: revoke_a.body.event_id(),
            position: 3,
            through: 2,
        }
    );

    // The same device at E = 3 is refused — because the bundle for E = 3 carries the revoke.
    let a_at_3 = signed_act(f.s.subject, f.a, &f.a_key, 3);
    let bundle3 = EvidenceBundle::assemble(&f.store, &a_at_3).unwrap();
    assert!(bundle3.facts().contains_key(&revoke_a.body.event_id()));
    assert_eq!(
        verify_bytes(&bundle3.canonical_bytes()).unwrap(),
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        ))
    );
}

// ---------------------------------------------------------------------------------------------
// 14. Every single-byte change changes the container identity, and none yields a verified act.
// ---------------------------------------------------------------------------------------------

#[test]
fn no_single_byte_mutation_preserves_identity_or_yields_a_verified_act() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let bundle = EvidenceBundle::assemble(&f.store, &act).unwrap();
    let bytes = bundle.canonical_bytes();
    let id = bundle.bundle_id();
    assert!(
        verify_bytes(&bytes).unwrap().is_ok(),
        "the unmutated bundle verifies"
    );

    for index in 0..bytes.len() {
        for mask in [0x01u8, 0x80u8] {
            let mut mutated = bytes.clone();
            mutated[index] ^= mask;
            assert_ne!(sha256(&mutated), id, "byte {index} mask {mask:#x}");
            match verify_bytes(&mutated) {
                Err(_) => {}
                Ok(Err(_)) => {}
                Ok(Ok(evidence)) => panic!(
                    "byte {index} mask {mask:#x}: mutated bundle produced evidence {evidence:?}"
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The bundle is not an N1 body and not an act, and vice versa; and the domains are disjoint.
// ---------------------------------------------------------------------------------------------

#[test]
fn bundle_bytes_are_not_a_body_or_an_act_and_the_domains_are_pairwise_distinct() {
    let f = fixture();
    let act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let bytes = EvidenceBundle::assemble(&f.store, &act)
        .unwrap()
        .canonical_bytes();
    assert!(AuthorityBody::decode(&bytes).is_err());
    assert!(DeviceActV1::decode(&bytes).is_err());
    assert!(EvidenceBundle::decode(&f.s.inception.body.canonical_bytes()).is_err());
    assert!(EvidenceBundle::decode(&act.act.canonical_bytes()).is_err());

    let domains: BTreeSet<&[u8]> = [
        BUNDLE_DOMAIN,
        DEVICE_ACT_DOMAIN,
        DOMAIN,
        SIGNATURE_DOMAIN,
        COMMITMENT_DOMAIN,
        KDF_DOMAIN,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        domains.len(),
        6,
        "domain separators must be pairwise distinct"
    );
    assert_eq!(BUNDLE_DOMAIN, b"icn.n4.evidence-bundle");
    assert_eq!(BUNDLE_VERSION, 1);
}

// ---------------------------------------------------------------------------------------------
// 15. The module reads no clock and no randomness. Scoped to this one file, mirroring the N4-A
//     guard in `device_authority.rs`.
// ---------------------------------------------------------------------------------------------

#[test]
fn evidence_bundle_module_reads_no_wall_clock_or_randomness() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/evidence_bundle.rs");
    let text = std::fs::read_to_string(&path).expect("evidence_bundle.rs must exist");
    for forbidden in [
        "SystemTime",
        "UNIX_EPOCH",
        "Instant",
        "current_timestamp",
        "try_current_timestamp",
        "icn_time",
        "Utc::now",
        "chrono",
        "Local::now",
        "OffsetDateTime",
        "elapsed(",
        "std::time",
        "OsRng",
        "rand::",
        "thread_rng",
        "HashMap",
        "HashSet",
    ] {
        assert!(
            !text.contains(forbidden),
            "evidence_bundle.rs must not reference `{forbidden}`"
        );
    }
    // And it defines no selector of its own: the N1 fold and selection entry points are never
    // called. Documentation may *name* `derive_prefix`; code may not invoke it. `#[derive(` is the
    // Rust attribute, which is why `derive(` is checked for call sites rather than as a substring.
    let code_only: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["supersede(", "resolve(", "derive_prefix(", "::derive("] {
        assert!(
            !code_only.contains(forbidden),
            "evidence_bundle.rs must not call `{forbidden}` — the container does not interpret authority"
        );
    }
    let bare_derive_calls = code_only.matches("derive(").count();
    let derive_attributes = code_only.matches("#[derive(").count();
    assert_eq!(
        bare_derive_calls, derive_attributes,
        "every `derive(` in evidence_bundle.rs must be the Rust attribute, not a fold call"
    );
}

// ---------------------------------------------------------------------------------------------
// Cross-implementation vector. The expected bytes below are reproduced by an independent reference
// written from the specification prose alone (`tests/reference/evidence_bundle_reference.py`),
// which reads these literals back purely as a comparison target. Inputs are stated in full so a
// third implementation can reproduce them.
// ---------------------------------------------------------------------------------------------

/// Subject root secret `[0x31; 32]`, context nonce `[0x31 ^ 0xa5 = 0x94; 32]`, horizon 4 — the
/// `authority_log_support::subject(0x31, 4)` fixture, stated here as raw inputs.
const VECTOR_SUBJECT_SEED: u8 = 0x31;
const VECTOR_DEVICE_A_SEED: u8 = 0x41;
const VECTOR_DEVICE_B_SEED: u8 = 0x42;
const VECTOR_EVALUATION_POSITION: u64 = 3;

const EXPECT_SUBJECT_ID_HEX: &str =
    "a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d";
const EXPECT_FACT_EVENT_IDS_HEX: [&str; 4] = [
    "1bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc",
    "9791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb",
    "a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d",
    "b22bf1bcf74d7cf901f8218b4d6185c1a9c4cc09792c536e777f3eddb35e3dd3",
];
const EXPECT_ACT_BYTES_HEX: &str = "0000001169636e2e6e342e6465766963652d6163740001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d012152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db1200000000000000030100000016666978747572653a6f70656e2d776f726b7370616365";
const EXPECT_BUNDLE_BYTES_HEX: &str = "0000001669636e2e6e342e65766964656e63652d62756e646c650001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000000000000003000000041bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000000000000001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a01db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d00000001010000000001abbd59db38f897f9a93a43f5ac88a8c975dd02ac658930d4e404373afae4d5e629ee42b9cc317efd9ea2b9608db8f6f5565ed06f811c7c2cac03fe1dcb724b0d9791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb000000a80000001169636e2e617574686f726974792d6c6f67000103a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000021bd0cea4f6f4f62cc9d8e4f091b8d3b0a2cae7a10038720f4a833a4636c1d2bc01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a012152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12000000010100000000014bd1a312691f057ae0bd38200e13c9ab518d7e46f4020163080543cf92f1368bef5c490df84a20d11e8d4237efb23bc0b18ee9b42139b26d832e90fd706d8500a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d0000009e0000001169636e2e617574686f726974792d6c6f6700010101216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a94949494949494949494949494949494949494949494949494949494949494940000000101216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a03e35d304806178a1a133aec9a7f61be02612aa8b936efff6f8150040cef9f0e00000001cd8c3ab7a2d1302f2566d49188b47700113347117dd846d883cddf10cd9c3613c3d9e4ba9d416152d21503b6faf3add9afa860784e00fbd6865ead25187e6701b22bf1bcf74d7cf901f8218b4d6185c1a9c4cc09792c536e777f3eddb35e3dd3000000a20000001169636e2e617574686f726974792d6c6f67000104a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d00000000000000039791575df688d534f00463ffd73607c860831557fd2fd66f16894b65dec82afb01216bcf74f3b911f6fe9065b158c006b7524b4597ddf343364a0bc94859fb948a01db995fe25169d141cab9bbba92baa01f9f2e1ece7df4cb2ac05190f37fcc1f9d000000011f40c4466d86e0394c211944911e60eace9633175e7314a4aa7c194c0846036cd74c73a41a80e4593bfedc444ee0b62d960110a2f45064fb7444da68dad5990c0000007b0000001169636e2e6e342e6465766963652d6163740001a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d012152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db1200000000000000030100000016666978747572653a6f70656e2d776f726b7370616365d62723fc7edfe5011bcea6b09e3da66076cfdcbc2a7b6be85359ed4e213707024fb7cd3c4d46745589589c6bd661bd6c4ded49ac4d15d7c9985018dfca967003";
const EXPECT_BUNDLE_ID_HEX: &str =
    "009fb4ce160121684041689c82bc4d01563dc6e852d5c9390c8aaa31a658deaf";

#[test]
fn canonical_bundle_vector_matches_the_independent_reference() {
    let s = subject(VECTOR_SUBJECT_SEED, 4);
    let (_a_key, a) = stranger(VECTOR_DEVICE_A_SEED);
    let (b_key, b) = stranger(VECTOR_DEVICE_B_SEED);
    let auth_a = authorize_at(&s, 0, 1, s.genesis(), a);
    let auth_b = authorize_at(&s, 0, 2, auth_a.body.event_id(), b);
    let revoke_a = revoke_at(&s, 0, 3, auth_b.body.event_id(), a);
    let store = store_of(&[s.inception.clone(), auth_a, auth_b, revoke_a]);
    let act = signed_act(s.subject, b, &b_key, VECTOR_EVALUATION_POSITION);

    let bundle = EvidenceBundle::assemble(&store, &act).unwrap();
    let bytes = bundle.canonical_bytes();

    assert_eq!(hex::encode(s.subject.as_bytes()), EXPECT_SUBJECT_ID_HEX);
    let ids: Vec<String> = bundle
        .facts()
        .keys()
        .map(|id| hex::encode(id.as_bytes()))
        .collect();
    assert_eq!(ids, EXPECT_FACT_EVENT_IDS_HEX);
    assert_eq!(hex::encode(act.act.canonical_bytes()), EXPECT_ACT_BYTES_HEX);
    assert_eq!(hex::encode(&bytes), EXPECT_BUNDLE_BYTES_HEX);
    assert_eq!(hex::encode(bundle.bundle_id()), EXPECT_BUNDLE_ID_HEX);

    // The pinned bytes decode to the same bundle, admit to the same store, and verify.
    let decoded = EvidenceBundle::decode(&hex::decode(EXPECT_BUNDLE_BYTES_HEX).unwrap()).unwrap();
    assert_eq!(decoded, bundle);
    assert_eq!(decoded.admit().unwrap(), store);
    let ev =
        verify_device_act(decoded.act(), s.subject, &store, VECTOR_EVALUATION_POSITION).unwrap();
    assert_eq!(ev.device, b);
    assert_eq!(ev.grant.granted_at, 2);
}

// ---------------------------------------------------------------------------------------------
// Ported from the superseded N4-B `DeviceAuthorityBundleV1` suite (convergence, 2026-10-04): the
// behaviours that suite pinned and this one did not, restated over the canonical container.
// ---------------------------------------------------------------------------------------------

use icn_identity::device_channel_binding::CHANNEL_BINDING_DOMAIN;
use icn_identity::device_enrollment::ENROLLMENT_REQUEST_DOMAIN;
use icn_identity::evidence_bundle::{
    verify_evidence_bundle, verify_evidence_bundle_bytes, EvidenceVerifyError,
};
use icn_identity::subject_context::GEN_CONTEXT_DOMAIN;

#[test]
fn withholding_the_fact_at_e_cannot_extend_a_revoked_devices_authority() {
    // The bundler drops the position-3 revoke and still claims E = 3. The prefix through 3 is
    // then incomplete, so the verifier fails closed instead of treating A as still authorized.
    let f = fixture();
    let without_revoke = store_of(&f.events[..3]);
    let a_act = signed_act(f.s.subject, f.a, &f.a_key, 3);
    let bundle = EvidenceBundle::assemble(&without_revoke, &a_act).unwrap();
    assert_eq!(bundle.facts().len(), 3);
    assert_eq!(
        verify_bytes(&bundle.canonical_bytes()).unwrap(),
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::PrefixIncomplete {
                frontier: 3,
                required: 3
            }
        ))
    );
}

#[test]
fn the_convenience_wrapper_is_admit_then_the_existing_verifier_and_nothing_else() {
    let f = fixture();
    let b_act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let bundle = EvidenceBundle::assemble(&f.store, &b_act).unwrap();
    let direct = verify_device_act(&b_act, f.s.subject, &f.store, 3).unwrap();
    assert_eq!(
        verify_evidence_bundle(&bundle, f.s.subject, 3).unwrap(),
        direct
    );
    assert_eq!(
        verify_evidence_bundle_bytes(&bundle.canonical_bytes(), f.s.subject, 3).unwrap(),
        direct
    );

    // A refusal is the verifier's, verbatim.
    let a_act = signed_act(f.s.subject, f.a, &f.a_key, 3);
    let refused = EvidenceBundle::assemble(&f.store, &a_act).unwrap();
    assert_eq!(
        verify_evidence_bundle(&refused, f.s.subject, 3),
        Err(EvidenceVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        )))
    );

    // An inadmissible witness is N1 admission's refusal, surfaced before any verdict: flip one
    // bit of a carried signature. The bytes still decode (decode verifies no signature).
    let bytes = bundle.canonical_bytes();
    let sig = f.events[1].signature.as_bytes();
    let at = bytes.windows(64).position(|w| w == sig).unwrap();
    let mut mutated = bytes.clone();
    mutated[at] ^= 0x01;
    assert!(EvidenceBundle::decode(&mutated).is_ok());
    assert!(matches!(
        verify_evidence_bundle_bytes(&mutated, f.s.subject, 3),
        Err(EvidenceVerifyError::Bundle(
            BundleError::Inadmissible { .. }
        ))
    ));
}

#[test]
fn the_relying_party_supplies_the_subject_and_the_position_never_the_bundle() {
    // The header's (S, E) must agree with the act (decode), but the relying party still states
    // its own; disagreement is N4-A's refusal, not a bundle error and not a fallback.
    let f = fixture();
    let b_act = signed_act(f.s.subject, f.b, &f.b_key, 3);
    let bundle = EvidenceBundle::assemble(&f.store, &b_act).unwrap();
    let other = subject(0x07, 4).subject;
    assert!(matches!(
        verify_evidence_bundle(&bundle, other, 3),
        Err(EvidenceVerifyError::Act(
            DeviceActVerifyError::ActSubjectMismatch
        ))
    ));
    assert!(matches!(
        verify_evidence_bundle(&bundle, f.s.subject, 2),
        Err(EvidenceVerifyError::Act(
            DeviceActVerifyError::EvaluationPositionMismatch { .. }
        ))
    ));
}

#[test]
fn bundle_domain_is_distinct_from_every_later_n4_and_gen_domain() {
    for other in [
        ENROLLMENT_REQUEST_DOMAIN,
        CHANNEL_BINDING_DOMAIN,
        GEN_CONTEXT_DOMAIN,
    ] {
        assert_ne!(BUNDLE_DOMAIN, other);
    }
}

#[test]
fn summarize_describes_the_facts_in_canonical_order_and_decides_nothing() {
    let f = fixture();
    // An act the verifier would refuse: inspection is indifferent to that.
    let a_act = signed_act(f.s.subject, f.a, &f.a_key, 3);
    let bundle = EvidenceBundle::assemble(&f.store, &a_act).unwrap();
    let summary = bundle.summarize();
    assert_eq!(summary.len(), 4);
    let ids: Vec<EventId> = summary.iter().map(|s| s.event_id).collect();
    assert_eq!(ids, bundle.facts().keys().copied().collect::<Vec<_>>());
    for (i, s) in summary.iter().enumerate() {
        assert_eq!(
            (s.index as usize, s.subject, s.witness_count),
            (i, f.s.subject, 1)
        );
    }
    let mut kinds: Vec<u8> = summary.iter().map(|s| s.kind_tag).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec![0x01, 0x03, 0x03, 0x04]);
    let mut positions: Vec<u64> = summary.iter().map(|s| s.position).collect();
    positions.sort_unstable();
    assert_eq!(positions, vec![0, 1, 2, 3]);
}
