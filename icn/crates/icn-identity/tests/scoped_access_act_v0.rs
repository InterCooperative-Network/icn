//! ScopedAccessActV0 — fixture-only demonstration that an ICN-signed device act, carried in the
//! existing N4-B evidence bundle, can be interpreted by an independent, offline application-level
//! relying party to make a correctly scoped, auditable access decision.
//!
//! **Not production authorization.** Every identity here is derived from a seed byte; nothing is a
//! real person, resource or application. The payload codec and the relying party live in
//! `scoped_access_support` (test-only). No ICN production code is changed or extended: the only
//! ICN calls are the existing public `EvidenceBundle` / `verify_evidence_bundle` /
//! `sign_device_act` / authority-log constructors.
//!
//! Acceptance tests A1–A13 (contract in the introducing PR; spec anchor icn#2599), with A10 restated: N4-B bundles refuse
//! facts above the evaluation position, so a "later revocation carried in the bundle" cannot exist.
//! A10 instead tests revocation through the relying party's own authority view.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod scoped_access_support;

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    authorize_event, revoke_event, AuthorityStore, CapabilitySet, ContextNonce, ContinuityRoot,
    DeviceCapability, EstablishmentKind, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1, SignedDeviceAct};
use icn_identity::evidence_bundle::EvidenceBundle;
use scoped_access_support::*;

// ---------------------------------------------------------------------------------------------
// Synthetic fixture
// ---------------------------------------------------------------------------------------------

/// Fixed fixture clock reading (UTC seconds). Synthetic.
const NOW: u64 = 1_800_000_000;
const AUDIENCE: &str = "demo-app";

struct Person {
    subject: SubjectId,
    establishment: SigningKey,
    facts: Vec<SignedAuthorityEvent>,
}

fn key(seed: u8) -> (SigningKey, PrincipalKey) {
    let k = SigningKey::from_bytes(&[seed; 32]);
    let p = PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap();
    (k, p)
}

fn person(seed: u8) -> Person {
    let root = ContinuityRoot::with_plan(
        [seed; 32],
        ContextNonce::from_bytes([seed ^ 0xa5; 32]),
        vec![EstablishmentKind::Rotate; 4],
    );
    let inception = root.incept().unwrap();
    Person {
        subject: inception.body.subject(),
        establishment: root.authority_signing_key(0),
        facts: vec![inception],
    }
}

/// Alice (seed 0x01) at positions:
///   0 inception · 1 authorize phone {Sign, Present} · 2 authorize watch {Sign} only
/// `alice_with_revocation` adds 3 revoke phone.
struct World {
    alice: Person,
    phone_key: SigningKey,
    phone: PrincipalKey,
    watch_key: SigningKey,
    watch: PrincipalKey,
    /// Bob (seed 0x02): 0 inception · 1 authorize bob_phone {Sign, Present}.
    bob: Person,
    bob_phone_key: SigningKey,
    bob_phone: PrincipalKey,
}

fn world() -> World {
    let mut alice = person(0x01);
    let (phone_key, phone) = key(0x11);
    let (watch_key, watch) = key(0x12);
    let both = CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]);
    let auth_phone = authorize_event(
        &alice.establishment,
        alice.subject,
        1,
        alice.facts[0].body.event_id(),
        phone,
        both.clone(),
        None,
    );
    let auth_watch = authorize_event(
        &alice.establishment,
        alice.subject,
        2,
        auth_phone.body.event_id(),
        watch,
        CapabilitySet::new([DeviceCapability::Sign]),
        None,
    );
    alice.facts.push(auth_phone);
    alice.facts.push(auth_watch);

    let mut bob = person(0x02);
    let (bob_phone_key, bob_phone) = key(0x21);
    let auth_bob_phone = authorize_event(
        &bob.establishment,
        bob.subject,
        1,
        bob.facts[0].body.event_id(),
        bob_phone,
        both,
        None,
    );
    bob.facts.push(auth_bob_phone);

    World {
        alice,
        phone_key,
        phone,
        watch_key,
        watch,
        bob,
        bob_phone_key,
        bob_phone,
    }
}

/// Alice's facts plus `3 revoke phone`.
fn alice_with_revocation(w: &World) -> Vec<SignedAuthorityEvent> {
    let mut facts = w.alice.facts.clone();
    let revoke = revoke_event(
        &w.alice.establishment,
        w.alice.subject,
        3,
        facts[2].body.event_id(),
        w.phone,
    );
    facts.push(revoke);
    facts
}

fn note() -> ResourceKey {
    ResourceKey::new("note", "note-0001")
}

/// The default, valid payload: Alice's phone asks `demo-app` to `read` her note.
fn payload(w: &World) -> ScopedAccessActV0 {
    ScopedAccessActV0 {
        audience: AUDIENCE.to_string(),
        resource_kind: "note".to_string(),
        resource_id: "note-0001".to_string(),
        action: "read".to_string(),
        scope: Scope::Personal,
        context_id: *w.alice.subject.as_bytes(),
        not_before: NOW - 10,
        not_after: NOW + 240,
        nonce: [0x42; NONCE_LEN],
        purpose: "display".to_string(),
    }
}

fn sign_raw(
    subject: SubjectId,
    device: PrincipalKey,
    device_key: &SigningKey,
    capability: DeviceCapability,
    position: u64,
    payload_bytes: Vec<u8>,
) -> SignedDeviceAct {
    let act = DeviceActV1::new(subject, device, capability, position, payload_bytes).unwrap();
    sign_device_act(&act, device_key).unwrap()
}

/// Holder side: assemble an N4-B bundle through the N1 gate, for the act's (S, E).
fn bundle(facts: &[SignedAuthorityEvent], act: &SignedDeviceAct) -> Vec<u8> {
    let mut store = AuthorityStore::new();
    for fact in facts {
        store.ingest(fact).unwrap();
    }
    EvidenceBundle::assemble(&store, act)
        .unwrap()
        .canonical_bytes()
}

/// Alice's phone signs `p` at position 2 with `Present`, bundled with Alice's facts.
fn alice_bundle(w: &World, p: &ScopedAccessActV0) -> Vec<u8> {
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        2,
        p.encode().unwrap(),
    );
    bundle(&w.alice.facts, &act)
}

fn policy() -> RelyingPartyPolicy {
    let mut allowed_actions = BTreeMap::new();
    allowed_actions.insert(
        "note".to_string(),
        BTreeSet::from(["read".to_string(), "annotate".to_string()]),
    );
    RelyingPartyPolicy {
        audience: AUDIENCE.to_string(),
        allowed_actions,
        allowed_purposes: BTreeSet::from(["display".to_string()]),
        max_lifetime_secs: 300,
        clock_skew_secs: 60,
        max_authority_view_age_secs: 3600,
        freshness: Freshness::SignerNonce,
        presenter: PresenterBinding::Unbound,
        challenge_ttl_secs: 120,
    }
}

type Rp = RelyingParty<FixedClock, InMemoryReplayStore>;

/// `demo-app`: Alice owns note-0001 (personal); Bob owns note-0002; a household-scoped resource
/// exists to prove v0 refuses it. Authority views: Alice at 2, Bob at 1, both observed recently.
fn rp(w: &World) -> Rp {
    let mut ownership = BTreeMap::new();
    ownership.insert(
        note(),
        OwnershipEvidence {
            owner: w.alice.subject,
            scope: Scope::Personal,
        },
    );
    ownership.insert(
        ResourceKey::new("note", "note-0002"),
        OwnershipEvidence {
            owner: w.bob.subject,
            scope: Scope::Personal,
        },
    );
    ownership.insert(
        ResourceKey::new("note", "shared-pantry"),
        OwnershipEvidence {
            owner: w.alice.subject,
            scope: Scope::Household,
        },
    );
    let mut authority_views = BTreeMap::new();
    authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 2,
            head: w.alice.facts[2].body.event_id(),
            observed_at: NOW - 30,
        },
    );
    authority_views.insert(
        w.bob.subject,
        TrustedAuthorityView {
            position: 1,
            head: w.bob.facts[1].body.event_id(),
            observed_at: NOW - 30,
        },
    );
    RelyingParty {
        policy: policy(),
        ownership,
        authority_views,
        clock: FixedClock(Some(NOW)),
        replay: InMemoryReplayStore::default(),
        challenges: InMemoryChallengeTable::default(),
        challenge_source: Box::new(FixedChallengeSource::default()),
        audit_log: Vec::new(),
    }
}

fn request<'a>(bytes: &'a [u8], resource: ResourceKey, action: &str) -> AccessRequest<'a> {
    AccessRequest {
        bundle: bytes,
        resource,
        action: action.to_string(),
        observed_channel: None,
        presenter_binding: None,
    }
}

fn decide_default(rp: &mut Rp, bytes: &[u8]) -> Decision {
    rp.decide(&request(bytes, note(), "read"))
}

fn assert_refused(d: &Decision, expected: Refusal) {
    assert_eq!(d.refusal(), Some(expected), "audit: {}", d.audit.json());
    assert!(!d.allowed());
    let audit = d.audit.json();
    assert_eq!(audit["verdict"], expected.verdict());
    assert_eq!(audit["class"], expected.class());
    assert!(audit["caveats"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------------------------
// A1 — valid act: authorized, local record written
// ---------------------------------------------------------------------------------------------

#[test]
fn a1_valid_personal_act_is_allowed_and_recorded() {
    let w = world();
    let mut rp = rp(&w);
    let bytes = alice_bundle(&w, &payload(&w));
    let d = decide_default(&mut rp, &bytes);
    assert!(d.allowed(), "audit: {}", d.audit.json());
    assert_eq!(rp.audit_log.len(), 1);
    let a = d.audit.json();
    assert_eq!(a["verdict"], "allow");
    assert_eq!(a["class"], serde_json::Value::Null);
    assert_eq!(
        a["evaluated_subject"],
        hex::encode(w.alice.subject.as_bytes())
    );
    assert_eq!(a["verified_act"]["device"], hex::encode(w.phone.as_bytes()));
    assert_eq!(a["verified_act"]["grant_granted_at"], 1);
    assert_eq!(a["evaluation_position"], 2);
    assert_eq!(a["clock_reading"], NOW);
    // The decision is the RP's; the nonce is now consumed.
    assert_eq!(rp.replay.seen.len(), 1);
}

#[test]
fn a1_payload_round_trips_canonically() {
    let w = world();
    let p = payload(&w);
    let bytes = p.encode().unwrap();
    assert_eq!(ScopedAccessActV0::decode(&bytes).unwrap(), p);
    assert_eq!(
        ScopedAccessActV0::decode(&bytes).unwrap().encode().unwrap(),
        bytes
    );
}

// ---------------------------------------------------------------------------------------------
// A2 — wrong audience
// ---------------------------------------------------------------------------------------------

#[test]
fn a2_act_for_another_application_is_refused() {
    let w = world();
    let mut rp = rp(&w);
    let mut p = payload(&w);
    p.audience = "other-app".to_string();
    let d = decide_default(&mut rp, &alice_bundle(&w, &p));
    assert_refused(&d, Refusal::WrongAudience);
    assert!(
        rp.replay.seen.is_empty(),
        "a refusal never consumes a nonce"
    );
}

// ---------------------------------------------------------------------------------------------
// A3 — expired / not yet valid / window shape
// ---------------------------------------------------------------------------------------------

#[test]
fn a3_expired_act_is_refused_beyond_skew() {
    let w = world();
    let mut p = payload(&w);
    p.not_before = NOW - 300; // lifetime 239 s: isolates expiry from the lifetime cap
    p.not_after = NOW - 61; // skew is 60
    let d = decide_default(&mut rp(&w), &alice_bundle(&w, &p));
    assert_refused(&d, Refusal::Expired);
}

#[test]
fn a3_not_yet_valid_act_is_refused_beyond_skew() {
    let w = world();
    let mut p = payload(&w);
    p.not_before = NOW + 61;
    p.not_after = NOW + 200;
    let d = decide_default(&mut rp(&w), &alice_bundle(&w, &p));
    assert_refused(&d, Refusal::NotYetValid);
}

#[test]
fn a3_skew_boundaries_are_inclusive_and_exact() {
    let w = world();
    // Expired exactly at the skew edge: still allowed.
    let mut p = payload(&w);
    p.not_before = NOW - 300;
    p.not_after = NOW - 60;
    assert!(decide_default(&mut rp(&w), &alice_bundle(&w, &p)).allowed());
    // Not-yet-valid exactly at the skew edge: still allowed.
    let mut p = payload(&w);
    p.not_before = NOW + 60;
    p.not_after = NOW + 100;
    assert!(decide_default(&mut rp(&w), &alice_bundle(&w, &p)).allowed());
}

#[test]
fn a3_lifetime_over_five_minutes_or_inverted_window_is_refused() {
    let w = world();
    let mut p = payload(&w);
    p.not_before = NOW - 10;
    p.not_after = NOW - 10 + 301;
    assert_refused(
        &decide_default(&mut rp(&w), &alice_bundle(&w, &p)),
        Refusal::LifetimeExceeded,
    );
    let mut p = payload(&w);
    p.not_after = p.not_before; // zero-length
    assert_refused(
        &decide_default(&mut rp(&w), &alice_bundle(&w, &p)),
        Refusal::InvalidWindow,
    );
    let mut p = payload(&w);
    p.not_after = p.not_before - 1;
    assert_refused(
        &decide_default(&mut rp(&w), &alice_bundle(&w, &p)),
        Refusal::InvalidWindow,
    );
    // Exactly 300 s is allowed.
    let mut p = payload(&w);
    p.not_before = NOW - 10;
    p.not_after = NOW + 290;
    assert!(decide_default(&mut rp(&w), &alice_bundle(&w, &p)).allowed());
}

#[test]
fn a3_extreme_window_values_do_not_overflow_into_acceptance() {
    let w = world();
    let mut p = payload(&w);
    p.not_before = u64::MAX - 100;
    p.not_after = u64::MAX;
    assert_refused(
        &decide_default(&mut rp(&w), &alice_bundle(&w, &p)),
        Refusal::NotYetValid,
    );
    let mut p = payload(&w);
    p.not_before = 0;
    p.not_after = 200;
    assert_refused(
        &decide_default(&mut rp(&w), &alice_bundle(&w, &p)),
        Refusal::Expired,
    );
}

// ---------------------------------------------------------------------------------------------
// A4 — clock unavailable
// ---------------------------------------------------------------------------------------------

#[test]
fn a4_no_trusted_clock_refuses() {
    let w = world();
    let mut rp = rp(&w);
    rp.clock = FixedClock(None);
    let d = decide_default(&mut rp, &alice_bundle(&w, &payload(&w)));
    assert_refused(&d, Refusal::ClockUnavailable);
    assert_eq!(d.audit.json()["clock_reading"], serde_json::Value::Null);
    assert!(rp.replay.seen.is_empty());
}

// ---------------------------------------------------------------------------------------------
// A5 / A6 — replay
// ---------------------------------------------------------------------------------------------

#[test]
fn a5_replayed_act_is_refused_and_both_attempts_are_recorded() {
    let w = world();
    let mut rp = rp(&w);
    let bytes = alice_bundle(&w, &payload(&w));
    assert!(decide_default(&mut rp, &bytes).allowed());
    let second = decide_default(&mut rp, &bytes);
    assert_refused(&second, Refusal::Replayed);
    assert_eq!(rp.audit_log.len(), 2);
    assert_eq!(
        rp.audit_log[1].json()["prev_hash"],
        hex::encode(rp.audit_log[0].hash)
    );
}

#[test]
fn a5_a_new_act_with_a_reused_nonce_is_refused() {
    // Different act bytes (different action), same Subject and nonce: still a replay slot hit.
    let w = world();
    let mut rp = rp(&w);
    assert!(decide_default(&mut rp, &alice_bundle(&w, &payload(&w))).allowed());
    let mut p = payload(&w);
    p.action = "annotate".to_string();
    let d = rp.decide(&request(&alice_bundle(&w, &p), note(), "annotate"));
    assert_refused(&d, Refusal::Replayed);
}

#[test]
fn a5_replay_limit_is_disclosed_a_second_verifier_accepts_the_same_act() {
    // DISCLOSED LIMITATION, asserted so it cannot be forgotten: the seen-set is per process.
    // An independent verifier (or this one after a restart) accepts the act once more.
    let w = world();
    let bytes = alice_bundle(&w, &payload(&w));
    let mut first = rp(&w);
    assert!(decide_default(&mut first, &bytes).allowed());
    let mut restarted = rp(&w);
    assert!(decide_default(&mut restarted, &bytes).allowed());
    let caveats = restarted.audit_log[0].json()["caveats"].clone();
    assert!(caveats
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "replay_state:in_memory_single_verifier"));
}

#[test]
fn a6_unavailable_seen_set_refuses() {
    let w = world();
    let mut rp = rp(&w);
    rp.replay.unavailable = true;
    let d = decide_default(&mut rp, &alice_bundle(&w, &payload(&w)));
    assert_refused(&d, Refusal::ReplayStateUnavailable);
}

// ---------------------------------------------------------------------------------------------
// A7 — every non-personal scope is named and refused
// ---------------------------------------------------------------------------------------------

#[test]
fn a7_household_workgroup_cooperative_federation_are_scope_not_supported() {
    let w = world();
    for scope in [
        Scope::Household,
        Scope::Workgroup,
        Scope::Cooperative,
        Scope::Federation,
    ] {
        let mut p = payload(&w);
        p.scope = scope;
        let d = decide_default(&mut rp(&w), &alice_bundle(&w, &p));
        assert_refused(&d, Refusal::ScopeNotSupported(scope));
        assert_eq!(
            d.audit.json()["class"],
            format!("ScopeNotSupported:{}", scope.name())
        );
    }
}

#[test]
fn a7_household_owned_resource_is_refused_even_with_a_personal_act() {
    let w = world();
    let mut p = payload(&w);
    p.resource_id = "shared-pantry".to_string();
    let d = rp(&w).decide(&request(
        &alice_bundle(&w, &p),
        ResourceKey::new("note", "shared-pantry"),
        "read",
    ));
    assert_refused(&d, Refusal::UnsupportedOwnershipScope);
}

// ---------------------------------------------------------------------------------------------
// A8 — unknown version / action / purpose
// ---------------------------------------------------------------------------------------------

#[test]
fn a8_unknown_payload_version_is_refused() {
    let w = world();
    let p = payload(&w);
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        2,
        p.encode_with_version(1).unwrap(),
    );
    let d = decide_default(&mut rp(&w), &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::UnsupportedVersion);
}

#[test]
fn a8_action_outside_the_audience_vocabulary_is_refused() {
    let w = world();
    let mut p = payload(&w);
    p.action = "delete".to_string();
    let d = rp(&w).decide(&request(&alice_bundle(&w, &p), note(), "delete"));
    assert_refused(&d, Refusal::UnknownAction);
}

#[test]
fn a8_unaccepted_purpose_is_refused() {
    let w = world();
    let mut p = payload(&w);
    p.purpose = "marketing".to_string();
    let d = decide_default(&mut rp(&w), &alice_bundle(&w, &p));
    assert_refused(&d, Refusal::PurposeNotAllowed);
}

// ---------------------------------------------------------------------------------------------
// A9 — non-canonical bytes are malformed
// ---------------------------------------------------------------------------------------------

fn alice_bundle_raw(w: &World, payload_bytes: Vec<u8>) -> Vec<u8> {
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        2,
        payload_bytes,
    );
    bundle(&w.alice.facts, &act)
}

#[test]
fn a9_trailing_bytes_in_the_signed_payload_are_malformed() {
    let w = world();
    let mut bytes = payload(&w).encode().unwrap();
    bytes.push(0x00);
    let d = decide_default(&mut rp(&w), &alice_bundle_raw(&w, bytes));
    assert_refused(&d, Refusal::PayloadMalformed);
    assert_eq!(d.audit.json()["verdict"], "malformed");
}

#[test]
fn a9_non_canonical_payload_spellings_are_malformed() {
    let w = world();
    let good = payload(&w).encode().unwrap();
    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();

    // Truncated at every length.
    for cut in 0..good.len() {
        cases.push(("truncated", good[..cut].to_vec()));
    }
    // Unknown scope tag (position: after domain, version and four labels).
    let scope_at = scope_offset(&good);
    let mut bad_scope = good.clone();
    bad_scope[scope_at] = 5;
    cases.push(("unknown scope tag", bad_scope));
    // Upper-case label: a second spelling of "Read" is not "read".
    let mut p = payload(&w);
    p.action = "read".to_string();
    let mut upper = p.encode().unwrap();
    let pos = find(&upper, b"read").unwrap();
    upper[pos] = b'R';
    cases.push(("uppercase label", upper));
    // Empty label.
    cases.push((
        "empty audience",
        replace_label(&good, AUDIENCE.as_bytes(), b""),
    ));
    // Wrong domain.
    let mut wrong_domain = good.clone();
    let d = find(&wrong_domain, SCOPED_ACCESS_DOMAIN).unwrap();
    wrong_domain[d] ^= 0x01;
    cases.push(("wrong domain", wrong_domain));
    // Over-long label.
    let long = "a".repeat(MAX_LABEL + 1);
    cases.push((
        "label over MAX_LABEL",
        replace_label(&good, b"display", long.as_bytes()),
    ));

    for (name, bytes) in cases {
        assert!(
            ScopedAccessActV0::decode(&bytes).is_err(),
            "{name} must not decode"
        );
        let d = decide_default(&mut rp(&w), &alice_bundle_raw(&w, bytes));
        assert_eq!(d.refusal(), Some(Refusal::PayloadMalformed), "{name}");
    }
}

#[test]
fn a9_malformed_bundle_bytes_are_malformed_and_still_audited() {
    let w = world();
    let mut rp = rp(&w);
    let garbage = b"not a bundle".to_vec();
    let d = decide_default(&mut rp, &garbage);
    assert_refused(&d, Refusal::BundleMalformed);
    let a = d.audit.json();
    assert_eq!(a["presented_sha256"], hex::encode(sha256(&garbage)));
    assert_eq!(a["bundle_id"], serde_json::Value::Null);
    assert_eq!(a["verified_act"], serde_json::Value::Null);
}

// ---------------------------------------------------------------------------------------------
// A10 — revocation (restated: through the relying party's own authority view)
// ---------------------------------------------------------------------------------------------

#[test]
fn a10_revocation_inside_the_rp_view_refuses() {
    // The RP has observed position 3 (revoke phone). The phone signs at 3 with the full prefix.
    let w = world();
    let facts = alice_with_revocation(&w);
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        3,
        payload(&w).encode().unwrap(),
    );
    let mut rp = rp(&w);
    rp.authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 3,
            head: alice_with_revocation(&w)[3].body.event_id(),
            observed_at: NOW - 5,
        },
    );
    let d = decide_default(&mut rp, &bundle(&facts, &act));
    assert_refused(&d, Refusal::N4("DeviceNotAuthorized"));
}

#[test]
fn a10_stale_bundle_from_before_a_known_revocation_refuses() {
    // The phone presents an act bound to position 2 (before its revocation). The RP's view is 3.
    // N4-A refuses: the act does not choose the evaluation position.
    let w = world();
    let mut rp = rp(&w);
    rp.authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 3,
            head: alice_with_revocation(&w)[3].body.event_id(),
            observed_at: NOW - 5,
        },
    );
    let d = decide_default(&mut rp, &alice_bundle(&w, &payload(&w)));
    assert_refused(&d, Refusal::N4("EvaluationPositionMismatch"));
}

#[test]
fn a10_revocation_the_rp_has_not_seen_is_not_visible_and_the_allow_says_so() {
    // DISCLOSED LIMITATION: the revocation exists at position 3, but the RP's view is 2 and the
    // bundle (by N4-B construction) cannot carry facts above 2. The act is allowed — and the
    // audit record states the horizon instead of implying currentness.
    let w = world();
    // The holder retains the revocation, and assembles from that full store at E = 2.
    let full = alice_with_revocation(&w);
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        2,
        payload(&w).encode().unwrap(),
    );
    let bytes = bundle(&full, &act);
    let carried = EvidenceBundle::decode(&bytes).unwrap();
    assert_eq!(
        carried.facts().len(),
        3,
        "the revoke at 3 cannot ride in an E=2 bundle"
    );
    assert!(carried.summarize().iter().all(|f| f.position <= 2));
    let d = decide_default(&mut rp(&w), &bytes);
    assert!(d.allowed());
    let caveats = d.audit.json()["caveats"].clone();
    let caveats: Vec<&str> = caveats
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert!(caveats.contains(&"authority_as_of_position:2"));
    assert!(caveats.contains(&"revocation_above_position_not_visible"));
}

#[test]
fn a10_unknown_revocation_state_is_not_treated_as_not_revoked() {
    let w = world();
    let mut rp = rp(&w);
    rp.authority_views.clear();
    let d = decide_default(&mut rp, &alice_bundle(&w, &payload(&w)));
    assert_refused(&d, Refusal::AuthorityViewUnavailable);
}

#[test]
fn a10_an_authority_view_older_than_policy_refuses() {
    let w = world();
    let mut rp = rp(&w);
    rp.authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 2,
            head: w.alice.facts[2].body.event_id(),
            observed_at: NOW - 3601,
        },
    );
    let d = decide_default(&mut rp, &alice_bundle(&w, &payload(&w)));
    assert_refused(&d, Refusal::AuthorityViewStale);
    // A future-dated observation is not trusted either.
    rp.authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 2,
            head: w.alice.facts[2].body.event_id(),
            observed_at: NOW + 61,
        },
    );
    let d = decide_default(&mut rp, &alice_bundle(&w, &payload(&w)));
    assert_refused(&d, Refusal::AuthorityViewStale);
}

// ---------------------------------------------------------------------------------------------
// A11 — wrong signer / wrong Subject (identity substitution)
// ---------------------------------------------------------------------------------------------

#[test]
fn a11_a_device_never_granted_by_the_owner_is_refused() {
    let w = world();
    let (mallory_key, mallory) = key(0x66);
    let act = sign_raw(
        w.alice.subject,
        mallory,
        &mallory_key,
        DeviceCapability::Present,
        2,
        payload(&w).encode().unwrap(),
    );
    let d = decide_default(&mut rp(&w), &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::N4("DeviceNotAuthorized"));
}

#[test]
fn a11_another_subjects_fully_valid_act_cannot_reach_alices_resource() {
    // Bob's phone is genuinely authorized by Bob; the signature and the grant are valid. Bob even
    // names Alice's note and Alice's subject as context. The RP evaluates for the OWNER of the
    // resource (Alice, from its own ownership evidence), so N4-A refuses ActSubjectMismatch.
    let w = world();
    let mut p = payload(&w);
    p.context_id = *w.alice.subject.as_bytes();
    let act = sign_raw(
        w.bob.subject,
        w.bob_phone,
        &w.bob_phone_key,
        DeviceCapability::Present,
        1,
        p.encode().unwrap(),
    );
    let bytes = bundle(&w.bob.facts, &act);
    let d = decide_default(&mut rp(&w), &bytes);
    assert_refused(&d, Refusal::N4("ActSubjectMismatch"));
    assert_eq!(
        d.audit.json()["evaluated_subject"],
        hex::encode(w.alice.subject.as_bytes()),
        "the RP evaluated for the resource owner, not for whoever signed"
    );
}

#[test]
fn a11_the_same_act_is_allowed_only_for_the_signers_own_resource() {
    // Control for the test above: Bob's act for Bob's own note is allowed.
    let w = world();
    let mut p = payload(&w);
    p.resource_id = "note-0002".to_string();
    p.context_id = *w.bob.subject.as_bytes();
    let act = sign_raw(
        w.bob.subject,
        w.bob_phone,
        &w.bob_phone_key,
        DeviceCapability::Present,
        1,
        p.encode().unwrap(),
    );
    let d = rp(&w).decide(&request(
        &bundle(&w.bob.facts, &act),
        ResourceKey::new("note", "note-0002"),
        "read",
    ));
    assert!(d.allowed(), "audit: {}", d.audit.json());
}

#[test]
fn a11_the_establishment_key_is_not_a_device() {
    let w = world();
    let establishment =
        PrincipalKey::try_from_verifying_key(w.alice.establishment.verifying_key()).unwrap();
    let act = sign_raw(
        w.alice.subject,
        establishment,
        &w.alice.establishment,
        DeviceCapability::Present,
        2,
        payload(&w).encode().unwrap(),
    );
    let d = decide_default(&mut rp(&w), &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::N4("DeviceIsEstablishmentAuthority"));
}

#[test]
fn a11_a_device_without_present_is_refused_by_n4() {
    let w = world();
    let act = sign_raw(
        w.alice.subject,
        w.watch,
        &w.watch_key,
        DeviceCapability::Present,
        2,
        payload(&w).encode().unwrap(),
    );
    let d = decide_default(&mut rp(&w), &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::N4("CapabilityNotGranted"));
}

// ---------------------------------------------------------------------------------------------
// A12 — the payload cannot widen anything
// ---------------------------------------------------------------------------------------------

#[test]
fn a12_an_appended_extra_field_is_malformed_not_a_wider_grant() {
    let w = world();
    let mut bytes = payload(&w).encode().unwrap();
    // A would-be "scope override" field appended after purpose.
    bytes.extend_from_slice(&9u32.to_be_bytes());
    bytes.extend_from_slice(b"federation");
    let d = decide_default(&mut rp(&w), &alice_bundle_raw(&w, bytes));
    assert_refused(&d, Refusal::PayloadMalformed);
}

#[test]
fn a12_personal_context_must_be_the_evaluated_subject() {
    let w = world();
    let mut p = payload(&w);
    p.context_id = *w.bob.subject.as_bytes();
    let d = decide_default(&mut rp(&w), &alice_bundle(&w, &p));
    assert_refused(&d, Refusal::ContextMismatch);
}

#[test]
fn a12_a_sign_capability_act_is_not_an_access_act() {
    // The watch holds Sign; it signs a well-formed access payload under Sign. N4-A authorizes
    // the Sign act, but an access act must be exercised under Present.
    let w = world();
    let act = sign_raw(
        w.alice.subject,
        w.watch,
        &w.watch_key,
        DeviceCapability::Sign,
        2,
        payload(&w).encode().unwrap(),
    );
    let d = decide_default(&mut rp(&w), &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::WrongCapability);
}

#[test]
fn a12_requested_resource_and_action_must_match_the_signed_ones() {
    let w = world();
    let bytes = alice_bundle(&w, &payload(&w));
    // Act says read note-0001; request asks to annotate note-0001.
    let d = rp(&w).decide(&request(&bytes, note(), "annotate"));
    assert_refused(&d, Refusal::ActionMismatch);
    // Act says note-0001; request names Alice's household resource → ownership scope first.
    // Request names a resource Alice owns personally but the act does not name → mismatch.
    let mut rp2 = rp(&w);
    rp2.ownership.insert(
        ResourceKey::new("note", "note-0003"),
        OwnershipEvidence {
            owner: w.alice.subject,
            scope: Scope::Personal,
        },
    );
    let d = rp2.decide(&request(
        &bytes,
        ResourceKey::new("note", "note-0003"),
        "read",
    ));
    assert_refused(&d, Refusal::ResourceMismatch);
}

// ---------------------------------------------------------------------------------------------
// A13 — audit evidence
// ---------------------------------------------------------------------------------------------

#[test]
fn a13_audit_record_binds_the_decision_and_carries_no_secrets_or_payload() {
    let w = world();
    let mut rp = rp(&w);
    let p = payload(&w);
    let bytes = alice_bundle(&w, &p);
    let d = decide_default(&mut rp, &bytes);
    assert!(d.allowed());
    let a = d.audit.json();
    let decoded = EvidenceBundle::decode(&bytes).unwrap();
    assert_eq!(a["schema"], AUDIT_SCHEMA);
    assert_eq!(a["bundle_id"], hex::encode(decoded.bundle_id()));
    assert_eq!(a["presented_sha256"], hex::encode(sha256(&bytes)));
    assert_eq!(
        a["verified_act"]["act_id"],
        hex::encode(decoded.act().act.act_id())
    );
    assert_eq!(
        a["verified_act"]["payload_sha256"],
        hex::encode(sha256(&p.encode().unwrap()))
    );

    let text = String::from_utf8(d.audit.bytes.clone()).unwrap();
    let forbidden: Vec<(&str, String)> = vec![
        ("phone secret key", hex::encode(w.phone_key.to_bytes())),
        (
            "establishment secret key",
            hex::encode(w.alice.establishment.to_bytes()),
        ),
        ("raw payload", hex::encode(p.encode().unwrap())),
        ("nonce", hex::encode(p.nonce)),
        ("signature", hex::encode(decoded.act().signature.as_bytes())),
    ];
    for (name, needle) in forbidden {
        assert!(!text.contains(&needle), "audit record leaks {name}");
    }
    assert!(
        !text.contains("display"),
        "purpose label is payload content"
    );
}

#[test]
fn a13_audit_is_deterministic_across_independent_verifiers() {
    let w = world();
    let bytes = alice_bundle(&w, &payload(&w));
    let garbage = b"\x00\x01".to_vec();
    let mut one = rp(&w);
    let mut two = rp(&w);
    for rp in [&mut one, &mut two] {
        rp.decide(&request(&bytes, note(), "read"));
        rp.decide(&request(&bytes, note(), "read")); // replay
        rp.decide(&request(&garbage, note(), "read"));
    }
    assert_eq!(one.audit_log, two.audit_log);
    let verdicts: Vec<String> = one
        .audit_log
        .iter()
        .map(|r| r.json()["verdict"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(verdicts, ["allow", "refused", "malformed"]);
}

#[test]
fn a13_the_audit_chain_detects_a_rewritten_record() {
    let w = world();
    let mut rp = rp(&w);
    let bytes = alice_bundle(&w, &payload(&w));
    rp.decide(&request(&bytes, note(), "read"));
    rp.decide(&request(&bytes, note(), "read"));
    // Recompute the chain over the log; flipping a verdict in record 0 breaks record 1's link.
    let mut tampered = rp.audit_log.clone();
    let rewritten = String::from_utf8(tampered[0].bytes.clone())
        .unwrap()
        .replace("\"allow\"", "\"refused\"");
    tampered[0].bytes = rewritten.into_bytes();
    assert!(chain_ok(&rp.audit_log));
    assert!(!chain_ok(&tampered));
}

fn chain_ok(log: &[AuditRecord]) -> bool {
    let mut prev = [0u8; 32];
    for r in log {
        let mut c = prev.to_vec();
        c.extend_from_slice(&r.bytes);
        if sha256(&c) != r.hash || r.json()["prev_hash"] != hex::encode(prev) {
            return false;
        }
        prev = r.hash;
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Additional adversarial tests
// ---------------------------------------------------------------------------------------------

#[test]
fn x_tampered_payload_bytes_inside_a_bundle_fail_the_signature() {
    let w = world();
    let p = payload(&w);
    let mut bytes = alice_bundle(&w, &p);
    // Flip one byte of the nonce where it sits inside the bundle's act bytes.
    let at = find(&bytes, &p.nonce).unwrap();
    bytes[at] ^= 0x01;
    let d = decide_default(&mut rp(&w), &bytes);
    assert_refused(&d, Refusal::N4("BadSignature"));
}

#[test]
fn x_tampered_payload_with_a_resigned_wrapper_by_another_device_fails_authority() {
    // Mallory re-signs Alice's exact payload under her own unauthorized key.
    let w = world();
    let (mallory_key, mallory) = key(0x67);
    let act = sign_raw(
        w.alice.subject,
        mallory,
        &mallory_key,
        DeviceCapability::Present,
        2,
        payload(&w).encode().unwrap(),
    );
    let d = decide_default(&mut rp(&w), &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::N4("DeviceNotAuthorized"));
}

#[test]
fn x_payload_from_another_action_family_is_malformed_not_interpreted() {
    let w = world();
    let d = decide_default(
        &mut rp(&w),
        &alice_bundle_raw(&w, b"fixture:open-workspace".to_vec()),
    );
    assert_refused(&d, Refusal::PayloadMalformed);
}

#[test]
fn x_unknown_resource_has_no_owner_and_refuses() {
    let w = world();
    let mut p = payload(&w);
    p.resource_id = "nobody".to_string();
    let d = rp(&w).decide(&request(
        &alice_bundle(&w, &p),
        ResourceKey::new("note", "nobody"),
        "read",
    ));
    assert_refused(&d, Refusal::UnknownResource);
}

#[test]
fn x_a_refused_presentation_does_not_burn_the_nonce() {
    // An observer who replays Alice's bundle against the wrong action cannot consume her act.
    let w = world();
    let mut rp = rp(&w);
    let bytes = alice_bundle(&w, &payload(&w));
    assert_refused(
        &rp.decide(&request(&bytes, note(), "annotate")),
        Refusal::ActionMismatch,
    );
    assert!(decide_default(&mut rp, &bytes).allowed());
}

#[test]
fn x_no_truncation_or_single_bit_flip_of_a_valid_bundle_is_ever_allowed() {
    let w = world();
    let bytes = alice_bundle(&w, &payload(&w));
    // Positive control: without it, a checker that refuses everything passes this test.
    assert!(decide_default(&mut rp(&w), &bytes).allowed());
    for cut in 0..bytes.len() {
        let d = decide_default(&mut rp(&w), &bytes[..cut]);
        assert!(!d.allowed(), "truncation at {cut} allowed");
    }
    for i in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[i] ^= 0x80;
        let d = decide_default(&mut rp(&w), &flipped);
        assert!(!d.allowed(), "bit flip at byte {i} allowed");
    }
}

#[test]
fn x_signature_validity_alone_never_allows() {
    // Every refusal below is for an act whose device signature VERIFIES. If any of these were
    // allowed, signature verification would have been mistaken for authorization.
    let w = world();
    let cases: Vec<(&str, Vec<u8>, Refusal)> = vec![
        (
            "unauthorized device",
            {
                let (k, p) = key(0x68);
                bundle(
                    &w.alice.facts,
                    &sign_raw(
                        w.alice.subject,
                        p,
                        &k,
                        DeviceCapability::Present,
                        2,
                        payload(&w).encode().unwrap(),
                    ),
                )
            },
            Refusal::N4("DeviceNotAuthorized"),
        ),
        (
            "other subject's device",
            bundle(
                &w.bob.facts,
                &sign_raw(
                    w.bob.subject,
                    w.bob_phone,
                    &w.bob_phone_key,
                    DeviceCapability::Present,
                    1,
                    payload(&w).encode().unwrap(),
                ),
            ),
            Refusal::N4("ActSubjectMismatch"),
        ),
    ];
    for (name, bytes, expected) in cases {
        let decoded = EvidenceBundle::decode(&bytes).unwrap();
        let act = &decoded.act().act;
        let sig = ed25519_dalek::Signature::from_bytes(decoded.act().signature.as_bytes());
        assert!(
            act.device
                .verifying_key()
                .verify_strict(&act.canonical_bytes(), &sig)
                .is_ok(),
            "{name}: precondition — the signature verifies"
        );
        assert_refused(&decide_default(&mut rp(&w), &bytes), expected);
    }
}

// ---------------------------------------------------------------------------------------------
// Review-driven tests (independent review of 4b643bd0f; mutation survivor)
// ---------------------------------------------------------------------------------------------

#[test]
fn r_two_subjects_with_the_same_nonce_do_not_collide() {
    // Killed the surviving mutant `replay-key-includes-only-nonce`: if the replay key dropped the
    // Subject, Bob (who can read Alice's nonce from her bundle) could pre-burn her slot.
    let w = world();
    let mut rp = rp(&w);
    let mut bob_p = payload(&w);
    bob_p.resource_id = "note-0002".to_string();
    bob_p.context_id = *w.bob.subject.as_bytes();
    assert_eq!(bob_p.nonce, payload(&w).nonce, "same nonce on purpose");
    let bob_act = sign_raw(
        w.bob.subject,
        w.bob_phone,
        &w.bob_phone_key,
        DeviceCapability::Present,
        1,
        bob_p.encode().unwrap(),
    );
    let bob_req_bytes = bundle(&w.bob.facts, &bob_act);
    assert!(rp
        .decide(&request(
            &bob_req_bytes,
            ResourceKey::new("note", "note-0002"),
            "read"
        ))
        .allowed());
    assert!(decide_default(&mut rp, &alice_bundle(&w, &payload(&w))).allowed());
}

#[test]
fn r_disclosed_limit_the_act_is_a_bearer_credential() {
    // DISCLOSED LIMITATION (review S1), asserted so it cannot be forgotten: whoever presents
    // Alice's bytes first is allowed, and Alice's own presentation is then a replay. Nothing
    // here binds the act to the presenter; N4-D channel binding is the existing primitive for it.
    let w = world();
    let mut rp = rp(&w);
    let alices_bytes = alice_bundle(&w, &payload(&w));
    let observer_copy = alices_bytes.clone();
    let first = decide_default(&mut rp, &observer_copy);
    assert!(first.allowed());
    assert!(first.audit.json()["caveats"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "bearer_act_not_bound_to_presenter"));
    assert_refused(&decide_default(&mut rp, &alices_bytes), Refusal::Replayed);
}

/// A body at position 3 on a branch the RP never saw: the establishment key authorizes Mallory
/// at 3 instead of revoking the phone. Requires the establishment key (compromise scenario).
fn alice_fork_at_3(w: &World) -> Vec<SignedAuthorityEvent> {
    let (_, mallory) = key(0x69);
    let mut facts = w.alice.facts.clone();
    let alt = authorize_event(
        &w.alice.establishment,
        w.alice.subject,
        3,
        facts[2].body.event_id(),
        mallory,
        CapabilitySet::new([DeviceCapability::Sign]),
        None,
    );
    facts.push(alt);
    facts
}

#[test]
fn r_presenter_cannot_choose_the_branch_the_verifier_folds() {
    // Review S2. The RP's head at 3 is the revoke. The presenter carries only its preferred
    // branch (no revoke; an alternative body at 3), so the fold is Live and N4-A authorizes the
    // revoked phone. Only the RP's pinned head refuses it.
    let w = world();
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        3,
        payload(&w).encode().unwrap(),
    );
    let bytes = bundle(&alice_fork_at_3(&w), &act);
    let mut rp = rp(&w);
    rp.authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 3,
            head: alice_with_revocation(&w)[3].body.event_id(),
            observed_at: NOW - 5,
        },
    );
    let d = decide_default(&mut rp, &bytes);
    assert_refused(&d, Refusal::AuthorityHeadMismatch);
    assert_eq!(
        d.audit.json()["verified_act"],
        serde_json::Value::Null,
        "an act verified against the presenter's facts is not recorded as verified"
    );
}

#[test]
fn r_omitting_the_known_revoke_is_a_gap_not_an_allow() {
    // The presenter drops the revoke at 3 and signs at 3 over facts 0..=2: N4-A sees a gap.
    let w = world();
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        3,
        payload(&w).encode().unwrap(),
    );
    let mut rp = rp(&w);
    rp.authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 3,
            head: alice_with_revocation(&w)[3].body.event_id(),
            observed_at: NOW - 5,
        },
    );
    let d = decide_default(&mut rp, &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::N4("PrefixIncomplete"));
}

#[test]
fn r_a_real_channel_binding_act_is_not_an_access_act() {
    // Review N6: the realistic cross-family case. A genuine N4-D channel binding is a `Present`
    // act by an authorized device; its payload must never be read as an access act.
    let w = world();
    let act = icn_identity::device_channel_binding::bind_channel(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        2,
        [0x07; 32],
        [0x08; 32],
    )
    .unwrap();
    assert_eq!(act.act.capability, DeviceCapability::Present);
    let d = decide_default(&mut rp(&w), &bundle(&w.alice.facts, &act));
    assert_refused(&d, Refusal::PayloadMalformed);
}

#[test]
fn r_malformed_request_labels_are_refused_and_not_logged_raw() {
    let w = world();
    let bytes = alice_bundle(&w, &payload(&w));
    let huge = "X".repeat(10_000);
    let d = rp(&w).decide(&request(&bytes, ResourceKey::new("note", &huge), "read"));
    assert_refused(&d, Refusal::RequestMalformed);
    let text = String::from_utf8(d.audit.bytes.clone()).unwrap();
    assert!(!text.contains("XXXX"));
    assert!(d.audit.json()["requested"]["invalid_labels_sha256"].is_string());
}

#[test]
fn r_the_payload_separator_is_application_namespaced() {
    // ICN never interprets this payload, so it must not claim the `icn` namespace (that would
    // require a domain_separation_audit REGISTRY row). Pinned so a rename cannot drift back.
    assert!(!SCOPED_ACCESS_DOMAIN
        .to_ascii_lowercase()
        .starts_with(b"icn"));
}

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Offset of the scope tag in canonical payload bytes: walk domain, version and four labels.
fn scope_offset(bytes: &[u8]) -> usize {
    let mut at = 0;
    let lp = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    at += 4 + lp(at); // domain
    at += 2; // version
    for _ in 0..4 {
        at += 4 + lp(at);
    }
    at
}

/// Replace one LP-framed label value (first occurrence) and fix its length prefix.
fn replace_label(bytes: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let at = find(bytes, old).unwrap();
    let mut out = bytes[..at - 4].to_vec();
    out.extend_from_slice(&(new.len() as u32).to_be_bytes());
    out.extend_from_slice(new);
    out.extend_from_slice(&bytes[at + old.len()..]);
    out
}

// =============================================================================================
// M2 — RP-issued single-use challenge and presenter binding (test-only)
// =============================================================================================
//
// Three presenter rungs, each demonstrated against the attack it does and does not stop:
//   Unbound               challenge is a bearer value            → interception succeeds (disclosed)
//   SessionChannel        challenge tied to the RP's channel     → interception refused; relay succeeds (disclosed)
//   DeviceChannelBinding  + N4-D binding over the DEVICE's view  → interception and relay refused
// "Channels" are synthetic 32-byte digests standing in for a transport profile's value (e.g. a TLS
// exporter). No such profile exists for application connections; see the M0 decision record, D2.

const C1: Challenge = [0xc1; NONCE_LEN];
const C2: Challenge = [0xc2; NONCE_LEN];

fn ch(label: &str) -> [u8; 32] {
    sha256(format!("fixture-channel:{label}").as_bytes())
}

fn rp_m2(w: &World, presenter: PresenterBinding, source: Vec<Challenge>) -> Rp {
    let mut rp = rp(w);
    rp.policy.freshness = Freshness::IssuedChallenge;
    rp.policy.presenter = presenter;
    rp.challenge_source = Box::new(FixedChallengeSource(source));
    rp
}

/// What a device produces for one request: the access act (over the challenge) and an N4-D
/// channel binding over the channel the DEVICE sees, whose nonce is the access act's `act_id`.
struct M2Req {
    access: Vec<u8>,
    binding: Vec<u8>,
    act_id: [u8; 32],
}

fn device_request(
    w: &World,
    p: &ScopedAccessActV0,
    device: (PrincipalKey, &SigningKey),
    binder: (PrincipalKey, &SigningKey),
    facts: &[SignedAuthorityEvent],
    position: u64,
    device_sees: [u8; 32],
) -> M2Req {
    let act = sign_raw(
        w.alice.subject,
        device.0,
        device.1,
        DeviceCapability::Present,
        position,
        p.encode().unwrap(),
    );
    let act_id = act.act.act_id();
    let binding = icn_identity::device_channel_binding::bind_channel(
        w.alice.subject,
        binder.0,
        binder.1,
        position,
        device_sees,
        act_id,
    )
    .unwrap();
    M2Req {
        access: bundle(facts, &act),
        binding: bundle(facts, &binding),
        act_id,
    }
}

/// Alice's phone answers `challenge`, seeing `device_sees`.
fn alice_m2(w: &World, challenge: Challenge, device_sees: [u8; 32]) -> M2Req {
    let mut p = payload(w);
    p.nonce = challenge;
    device_request(
        w,
        &p,
        (w.phone, &w.phone_key),
        (w.phone, &w.phone_key),
        &w.alice.facts,
        2,
        device_sees,
    )
}

fn present<'a>(m: &'a M2Req, on: Option<[u8; 32]>, with_binding: bool) -> AccessRequest<'a> {
    AccessRequest {
        bundle: &m.access,
        resource: note(),
        action: "read".to_string(),
        observed_channel: on,
        presenter_binding: with_binding.then_some(&m.binding[..]),
    }
}

fn caveats(d: &Decision) -> Vec<String> {
    d.audit.json()["caveats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect()
}

// ---- happy path, single use, freshness ---------------------------------------------------

#[test]
fn m2_honest_device_bound_request_is_allowed_once() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    let c = rp.issue_challenge(Some(ch("alice"))).unwrap();
    assert_eq!(c, C1);
    let m = alice_m2(&w, c, ch("alice"));
    let d = rp.decide(&present(&m, Some(ch("alice")), true));
    assert!(d.allowed(), "audit: {}", d.audit.json());
    let cv = caveats(&d);
    assert!(cv.contains(&"channel_binding_depends_on_transport_profile".to_string()));
    assert!(!cv.iter().any(|c| c.contains("bearer")), "{cv:?}");
    assert!(rp.challenges.records[&C1].consumed);
    // Single use.
    assert_refused(
        &rp.decide(&present(&m, Some(ch("alice")), true)),
        Refusal::ChallengeConsumed,
    );
}

#[test]
fn m2_signer_chosen_nonce_is_not_a_challenge() {
    // Challenge substitution: the device picks its own nonce instead of the RP's.
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, [0x42; NONCE_LEN], ch("alice"));
    assert_refused(
        &rp.decide(&present(&m, Some(ch("alice")), true)),
        Refusal::ChallengeUnknown,
    );
}

#[test]
fn m2_a_challenge_issued_to_another_session_is_refused() {
    // Challenge substitution across sessions: C2 was issued on Mallory's channel.
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::SessionChannel, vec![C1, C2]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    rp.issue_challenge(Some(ch("mallory"))).unwrap();
    let m = alice_m2(&w, C2, ch("alice"));
    assert_refused(
        &rp.decide(&present(&m, Some(ch("alice")), false)),
        Refusal::ChallengeChannelMismatch,
    );
}

#[test]
fn m2_challenge_expiry_is_inclusive_and_enforced() {
    let w = world();
    for (at, allowed) in [(NOW + 120, true), (NOW + 121, false)] {
        let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
        rp.issue_challenge(Some(ch("alice"))).unwrap();
        rp.clock = FixedClock(Some(at));
        let d = rp.decide(&present(
            &alice_m2(&w, C1, ch("alice")),
            Some(ch("alice")),
            true,
        ));
        if allowed {
            assert!(d.allowed(), "at {at}: {}", d.audit.json());
        } else {
            assert_refused(&d, Refusal::ChallengeExpired);
            assert!(!rp.challenges.records[&C1].consumed);
        }
    }
}

#[test]
fn m2_clock_moved_back_before_issuance_refuses() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    rp.clock = FixedClock(Some(NOW - 1));
    assert_refused(
        &rp.decide(&present(
            &alice_m2(&w, C1, ch("alice")),
            Some(ch("alice")),
            true,
        )),
        Refusal::ChallengeNotYetIssued,
    );
}

#[test]
fn m2_a_consumed_challenge_survives_clock_rollback() {
    // Unlike M1's pruned seen-set, a consumed record is never removed: rollback cannot re-admit.
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, C1, ch("alice"));
    rp.clock = FixedClock(Some(NOW + 100));
    assert!(rp.decide(&present(&m, Some(ch("alice")), true)).allowed());
    rp.clock = FixedClock(Some(NOW + 1));
    assert_refused(
        &rp.decide(&present(&m, Some(ch("alice")), true)),
        Refusal::ChallengeConsumed,
    );
}

#[test]
fn m2_a_refused_presentation_does_not_consume_the_challenge() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, C1, ch("alice"));
    let mut wrong = present(&m, Some(ch("alice")), true);
    wrong.action = "annotate".to_string();
    assert_refused(&rp.decide(&wrong), Refusal::ActionMismatch);
    assert!(
        !rp.challenges.records[&C1].consumed,
        "a refusal consumed the challenge"
    );
    assert!(rp.decide(&present(&m, Some(ch("alice")), true)).allowed());
    assert!(rp.challenges.records[&C1].consumed);
}

// ---- audience and verifier isolation -------------------------------------------------------

#[test]
fn m2_cross_audience_and_cross_verifier_challenges_are_refused() {
    let w = world();
    let mut a = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    a.issue_challenge(Some(ch("alice"))).unwrap();
    // RP-B is another application. Alice signs for B over A's challenge.
    let mut b = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![]);
    b.policy.audience = "other-app".to_string();
    let mut p = payload(&w);
    p.audience = "other-app".to_string();
    p.nonce = C1;
    let m = device_request(
        &w,
        &p,
        (w.phone, &w.phone_key),
        (w.phone, &w.phone_key),
        &w.alice.facts,
        2,
        ch("alice"),
    );
    assert_refused(
        &b.decide(&present(&m, Some(ch("alice")), true)),
        Refusal::ChallengeUnknown,
    );
    // Even if B's table were (wrongly) shared with A's records, the record's audience refuses.
    b.challenges
        .records
        .insert(C1, a.challenges.records[&C1].clone());
    assert_refused(
        &b.decide(&present(&m, Some(ch("alice")), true)),
        Refusal::ChallengeAudienceMismatch,
    );
}

#[test]
fn m2_m1_cross_verifier_limit_is_closed_by_issued_challenges() {
    // M1's a5_replay_limit test shows a second verifier accepting a signer-nonce act once more.
    // With issued challenges, a fresh or restarted verifier does not know the challenge.
    let w = world();
    let mut first = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    first.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, C1, ch("alice"));
    assert!(first
        .decide(&present(&m, Some(ch("alice")), true))
        .allowed());
    let mut restarted = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![]);
    assert_refused(
        &restarted.decide(&present(&m, Some(ch("alice")), true)),
        Refusal::ChallengeUnknown,
    );
}

#[test]
fn m2_unavailable_challenge_state_refuses_issuance_and_decision() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1, C2]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    rp.challenges.unavailable = true;
    assert_eq!(
        rp.issue_challenge(Some(ch("alice"))),
        Err(Refusal::ChallengeStateUnavailable)
    );
    assert_refused(
        &rp.decide(&present(
            &alice_m2(&w, C1, ch("alice")),
            Some(ch("alice")),
            true,
        )),
        Refusal::ChallengeStateUnavailable,
    );
}

#[test]
fn m2_issuance_refuses_without_channel_exhausted_source_or_duplicate() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::SessionChannel, vec![C1, C1]);
    assert_eq!(rp.issue_challenge(None), Err(Refusal::ChannelUnavailable));
    assert_eq!(rp.issue_challenge(Some(ch("alice"))), Ok(C1));
    assert_eq!(
        rp.issue_challenge(Some(ch("alice"))),
        Err(Refusal::ChallengeIssuanceFailed),
        "a repeated value is refused, never re-issued"
    );
    assert_eq!(
        rp.issue_challenge(Some(ch("alice"))),
        Err(Refusal::ChallengeIssuanceFailed),
        "an exhausted source is refused"
    );
}

#[test]
fn m2_os_challenges_are_full_width_and_distinct() {
    // A smoke check of the OS CSPRNG wiring, not a proof of unpredictability.
    let mut source = OsChallengeSource;
    let mut seen = BTreeSet::new();
    for _ in 0..512 {
        let c = source.next_challenge().unwrap();
        assert_ne!(c, [0u8; NONCE_LEN]);
        assert!(seen.insert(c), "OS source repeated a 128-bit value");
    }
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![]);
    rp.challenge_source = Box::new(OsChallengeSource);
    let c = rp.issue_challenge(Some(ch("alice"))).unwrap();
    assert!(rp
        .decide(&present(
            &alice_m2(&w, c, ch("alice")),
            Some(ch("alice")),
            true
        ))
        .allowed());
}

// ---- the presenter-binding ladder: what each rung stops ----------------------------------

#[test]
fn m2_disclosed_challenge_only_is_vulnerable_to_first_presenter_interception() {
    // DISCLOSED LIMITATION. Unbound mode: whoever presents the signed act first is allowed; the
    // device's own presentation is then refused. Challenges alone do not bind a presenter.
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::Unbound, vec![C1]);
    rp.issue_challenge(None).unwrap();
    let m = alice_m2(&w, C1, ch("unused"));
    let interceptor = rp.decide(&present(&m, None, false));
    assert!(interceptor.allowed());
    assert!(caveats(&interceptor).contains(&"challenge_is_bearer:first_presenter_wins".to_string()));
    assert_refused(
        &rp.decide(&present(&m, None, false)),
        Refusal::ChallengeConsumed,
    );
}

#[test]
fn m2_session_channel_stops_capture_and_present_elsewhere() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::SessionChannel, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, C1, ch("alice"));
    assert_refused(
        &rp.decide(&present(&m, Some(ch("interceptor")), false)),
        Refusal::ChallengeChannelMismatch,
    );
    let d = rp.decide(&present(&m, Some(ch("alice")), false));
    assert!(d.allowed());
    assert!(caveats(&d).contains(&"session_channel_only:relay_not_prevented".to_string()));
}

#[test]
fn m2_disclosed_session_channel_does_not_stop_a_relay() {
    // DISCLOSED LIMITATION. The relay opens its own session (channel "relay<->rp"), obtains the
    // challenge, forwards it to Alice's device (which believes it is talking to the RP over
    // "alice<->relay"), and presents the signed act on its own channel. The RP sees a consistent
    // session and allows it.
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::SessionChannel, vec![C1]);
    let c = rp.issue_challenge(Some(ch("relay<->rp"))).unwrap();
    let m = alice_m2(&w, c, ch("alice<->relay"));
    assert!(rp
        .decide(&present(&m, Some(ch("relay<->rp")), false))
        .allowed());
    assert!(
        rp.challenges.records[&C1].consumed,
        "the relay really consumed Alice's challenge"
    );
}

#[test]
fn m2_device_channel_binding_exposes_the_relay() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    let c = rp.issue_challenge(Some(ch("relay<->rp"))).unwrap();
    let m = alice_m2(&w, c, ch("alice<->relay"));
    assert_refused(
        &rp.decide(&present(&m, Some(ch("relay<->rp")), true)),
        Refusal::PresenterChannelMismatch,
    );
    assert!(!rp.challenges.records[&C1].consumed);
}

#[test]
fn m2_device_channel_binding_also_refuses_interception() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, C1, ch("alice"));
    assert_refused(
        &rp.decide(&present(&m, Some(ch("interceptor")), true)),
        Refusal::ChallengeChannelMismatch,
    );
    assert!(rp.decide(&present(&m, Some(ch("alice")), true)).allowed());
}

// ---- wrong presenter / malformed binding ---------------------------------------------------

/// Alice's facts plus `3 authorize tablet {Present}`; returns (facts, tablet key, tablet).
fn alice_with_tablet(w: &World) -> (Vec<SignedAuthorityEvent>, SigningKey, PrincipalKey) {
    let (tablet_key, tablet) = key(0x13);
    let mut facts = w.alice.facts.clone();
    let auth = authorize_event(
        &w.alice.establishment,
        w.alice.subject,
        3,
        facts[2].body.event_id(),
        tablet,
        CapabilitySet::new([DeviceCapability::Present]),
        None,
    );
    facts.push(auth);
    (facts, tablet_key, tablet)
}

fn rp_m2_at3(w: &World, facts: &[SignedAuthorityEvent]) -> Rp {
    let mut rp = rp_m2(w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.authority_views.insert(
        w.alice.subject,
        TrustedAuthorityView {
            position: 3,
            head: facts[3].body.event_id(),
            observed_at: NOW - 5,
        },
    );
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    rp
}

fn challenged_payload(w: &World) -> ScopedAccessActV0 {
    let mut p = payload(w);
    p.nonce = C1;
    p
}

#[test]
fn m2_binding_by_another_authorized_device_of_the_same_subject_is_refused() {
    let w = world();
    let (facts, tablet_key, tablet) = alice_with_tablet(&w);
    let m = device_request(
        &w,
        &challenged_payload(&w),
        (w.phone, &w.phone_key),
        (tablet, &tablet_key),
        &facts,
        3,
        ch("alice"),
    );
    assert_refused(
        &rp_m2_at3(&w, &facts).decide(&present(&m, Some(ch("alice")), true)),
        Refusal::PresenterDeviceMismatch,
    );
}

#[test]
fn m2_binding_for_another_act_is_refused() {
    let w = world();
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, C1, ch("alice"));
    let other = alice_m2(&w, [0x07; NONCE_LEN], ch("alice"));
    assert_ne!(m.act_id, other.act_id);
    let req = AccessRequest {
        presenter_binding: Some(&other.binding),
        ..present(&m, Some(ch("alice")), true)
    };
    assert_refused(&rp.decide(&req), Refusal::PresenterBindingNotForThisAct);
}

#[test]
fn m2_binding_by_unauthorized_or_foreign_devices_is_refused_by_n4() {
    let w = world();
    let cases: Vec<(PrincipalKey, SigningKey, Refusal)> = vec![
        (
            w.watch,
            SigningKey::from_bytes(&[0x12; 32]),
            Refusal::PresenterN4("CapabilityNotGranted"),
        ),
        (
            key(0x6a).1,
            key(0x6a).0,
            Refusal::PresenterN4("DeviceNotAuthorized"),
        ),
    ];
    for (device, device_key, expected) in cases {
        let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
        rp.issue_challenge(Some(ch("alice"))).unwrap();
        let m = device_request(
            &w,
            &challenged_payload(&w),
            (w.phone, &w.phone_key),
            (device, &device_key),
            &w.alice.facts,
            2,
            ch("alice"),
        );
        assert_refused(&rp.decide(&present(&m, Some(ch("alice")), true)), expected);
    }
    // Bob's genuinely authorized phone binds the channel for Alice's act: wrong Subject.
    let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
    rp.issue_challenge(Some(ch("alice"))).unwrap();
    let m = alice_m2(&w, C1, ch("alice"));
    let bob_binding = icn_identity::device_channel_binding::bind_channel(
        w.bob.subject,
        w.bob_phone,
        &w.bob_phone_key,
        1,
        ch("alice"),
        m.act_id,
    )
    .unwrap();
    let bob_bytes = bundle(&w.bob.facts, &bob_binding);
    let req = AccessRequest {
        presenter_binding: Some(&bob_bytes),
        ..present(&m, Some(ch("alice")), true)
    };
    assert_refused(&rp.decide(&req), Refusal::PresenterN4("ActSubjectMismatch"));
}

#[test]
fn m2_missing_or_malformed_binding_and_missing_channel_refuse() {
    let w = world();
    let fresh = || {
        let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
        rp.issue_challenge(Some(ch("alice"))).unwrap();
        rp
    };
    let m = alice_m2(&w, C1, ch("alice"));
    assert_refused(
        &fresh().decide(&present(&m, Some(ch("alice")), false)),
        Refusal::PresenterBindingMissing,
    );
    assert_refused(
        &fresh().decide(&present(&m, None, true)),
        Refusal::ChannelUnavailable,
    );
    for bad in [b"garbage".to_vec(), m.access.clone()] {
        let req = AccessRequest {
            presenter_binding: Some(&bad),
            ..present(&m, Some(ch("alice")), true)
        };
        assert_refused(&fresh().decide(&req), Refusal::PresenterBindingMalformed);
    }
}

#[test]
fn m2_presenter_binding_bundle_is_head_pinned_too() {
    // The binding bundle carries a forked body at 3 (the tablet authorized with different caps):
    // the binding verifies under the fork, but not at the RP's pinned head.
    let w = world();
    let (facts, _, tablet) = alice_with_tablet(&w);
    let mut fork = w.alice.facts.clone();
    fork.push(authorize_event(
        &w.alice.establishment,
        w.alice.subject,
        3,
        fork[2].body.event_id(),
        tablet,
        CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]),
        None,
    ));
    let act = sign_raw(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        DeviceCapability::Present,
        3,
        challenged_payload(&w).encode().unwrap(),
    );
    let binding = icn_identity::device_channel_binding::bind_channel(
        w.alice.subject,
        w.phone,
        &w.phone_key,
        3,
        ch("alice"),
        act.act.act_id(),
    )
    .unwrap();
    let access = bundle(&facts, &act);
    let forked_binding = bundle(&fork, &binding);
    let req = AccessRequest {
        bundle: &access,
        resource: note(),
        action: "read".to_string(),
        observed_channel: Some(ch("alice")),
        presenter_binding: Some(&forked_binding),
    };
    assert_refused(
        &rp_m2_at3(&w, &facts).decide(&req),
        Refusal::PresenterHeadMismatch,
    );
    // Control: the honest binding bundle over the RP's facts is allowed.
    let honest = bundle(&facts, &binding);
    let req = AccessRequest {
        presenter_binding: Some(&honest),
        ..req
    };
    assert!(rp_m2_at3(&w, &facts).decide(&req).allowed());
}

// ---- audit ------------------------------------------------------------------------------

#[test]
fn m2_audit_is_deterministic_and_carries_digests_not_secrets() {
    let w = world();
    let run = || {
        let mut rp = rp_m2(&w, PresenterBinding::DeviceChannelBinding, vec![C1]);
        rp.issue_challenge(Some(ch("alice"))).unwrap();
        let m = alice_m2(&w, C1, ch("alice"));
        rp.decide(&present(&m, Some(ch("alice")), true));
        rp.decide(&present(&m, Some(ch("relay")), true));
        (rp.audit_log, m)
    };
    let (one, m) = run();
    let (two, _) = run();
    assert_eq!(one, two);
    let a = one[0].json();
    assert_eq!(a["verdict"], "allow");
    assert_eq!(a["freshness"], "issued_challenge");
    assert_eq!(a["presenter_binding"], "device_channel_binding");
    assert_eq!(a["challenge_sha256"], hex::encode(sha256(&C1)));
    assert_eq!(
        a["observed_channel_sha256"],
        hex::encode(sha256(&ch("alice")))
    );
    let binding_act_id = hex::encode(
        EvidenceBundle::decode(&m.binding)
            .unwrap()
            .act()
            .act
            .act_id(),
    );
    assert_eq!(a["presenter_binding_act_id"], binding_act_id);
    let text = String::from_utf8(one[0].bytes.clone()).unwrap();
    assert!(!text.contains(&hex::encode(C1)), "raw challenge logged");
    assert!(
        !text.contains(&hex::encode(ch("alice"))),
        "raw channel logged"
    );
    assert!(!text.contains(&hex::encode(w.phone_key.to_bytes())));
    // Already consumed by the allow above: consumption is reported before the channel.
    assert_eq!(one[1].json()["class"], "ChallengeConsumed");
}
