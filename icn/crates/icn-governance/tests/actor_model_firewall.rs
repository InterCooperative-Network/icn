//! Rung 4 — human actor-model firewall, library-level obligations.
//!
//! Covers adversarial tests 1–3 of the rung-4 acceptance contract plus the
//! electorate-resolution and config-mutation boundaries that live in this
//! crate. Actor- and manager-level refusal (tests 4–8) lives in
//! `apps/governance/tests/actor_model_firewall.rs`.
//!
//! Every negative test drives a real guarded path and asserts observable state
//! afterwards; none merely compares enum variants.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_governance::resolver::CompositeMembershipResolver;
use icn_governance::{
    ActorModelError, GovernanceConfig, GovernanceDomain, GovernanceDomainId, GovernanceStore,
    HumanActorModel, InMemoryGovernanceStore, LegacyRailOp, MembershipConfig, MembershipResolver,
    MembershipSource, StaticMembershipResolver,
};
use icn_identity::KeyPair;

fn did() -> icn_identity::Did {
    KeyPair::generate().unwrap().did().clone()
}

fn legacy_domain(id: &str, members: Vec<icn_identity::Did>) -> GovernanceDomain {
    let mut config = GovernanceConfig::cooperative_default();
    config.membership = MembershipConfig::static_list(members);
    GovernanceDomain::with_id(GovernanceDomainId::new(id), id.to_string(), config)
}

/// A Subject-mode domain: the one inert membership shape (`StaticList([])`).
fn subject_domain(id: &str) -> GovernanceDomain {
    legacy_domain(id, vec![])
        .with_human_actor_model(HumanActorModel::ContextSubjectV1)
        .unwrap()
}

// ---------------------------------------------------------------------------
// Test 1 — legacy default compatibility
// ---------------------------------------------------------------------------

/// A record that predates the field (no `human_actor_model` key) loads as
/// legacy, keeps its membership, and still resolves its electorate.
#[test]
fn t1_pre_field_record_loads_as_legacy_and_keeps_behaviour() {
    let member = did();
    let domain = legacy_domain("coop-a", vec![member.clone()]);

    // Build the exact pre-field key set by removing nothing: a legacy domain
    // must not emit the key at all.
    let mut json = serde_json::to_value(&domain).unwrap();
    let obj = json.as_object_mut().unwrap();
    assert!(
        !obj.contains_key("human_actor_model"),
        "legacy domains must serialize without the new field (byte-stable)"
    );
    let mut keys: Vec<_> = obj.keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "config",
            "created_at",
            "description",
            "id",
            "name",
            "updated_at"
        ],
        "legacy key set must equal the pre-field key set"
    );

    let loaded: GovernanceDomain = serde_json::from_value(json).unwrap();
    assert_eq!(
        loaded.human_actor_model(),
        HumanActorModel::LegacyPrincipalV0
    );
    assert!(loaded.require_legacy_rail(LegacyRailOp::VoteCast).is_ok());
    assert_eq!(
        StaticMembershipResolver::new()
            .resolve_members(&loaded)
            .unwrap(),
        vec![member],
        "legacy electorate must be unchanged"
    );
    // Loading is read-only: re-serializing yields the same value we loaded from.
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&domain).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Test 2 — explicit Subject-mode selection survives persistence
// ---------------------------------------------------------------------------

#[test]
fn t2_subject_mode_round_trips_json_and_store() {
    let domain = subject_domain("coop-subject");

    let json = serde_json::to_value(&domain).unwrap();
    assert_eq!(json["human_actor_model"], "context_subject_v1");
    let back: GovernanceDomain = serde_json::from_value(json).unwrap();
    assert_eq!(back.human_actor_model(), HumanActorModel::ContextSubjectV1);

    let store = InMemoryGovernanceStore::new();
    store.store_domain(&domain).unwrap();
    let got = store.get_domain(&domain.id).unwrap().unwrap();
    assert_eq!(got.human_actor_model(), HumanActorModel::ContextSubjectV1);
}

/// Subject mode is never inferred: not from the name, not from an absent or
/// `null` or unknown value.
#[test]
fn t2b_subject_mode_is_never_inferred() {
    // A name that screams "subject" does not change the model.
    let by_name = legacy_domain("context-subject-v1-domain", vec![did()]);
    assert_eq!(
        by_name.human_actor_model(),
        HumanActorModel::LegacyPrincipalV0
    );

    let base = serde_json::to_value(legacy_domain("x", vec![])).unwrap();
    for bad in [
        serde_json::Value::Null,
        serde_json::json!(""),
        serde_json::json!("ContextSubjectV2"),
        serde_json::json!("subject"),
        serde_json::json!(1),
    ] {
        let mut v = base.clone();
        v["human_actor_model"] = bad.clone();
        assert!(
            serde_json::from_value::<GovernanceDomain>(v).is_err(),
            "unspecified/unknown value {bad} must be an error, not a silent fallback"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3 — mixed legacy membership cannot silently become Subject mode
// ---------------------------------------------------------------------------

#[test]
fn t3_domain_with_legacy_members_cannot_become_subject() {
    let populated = legacy_domain("coop-m", vec![did()]);
    let err = populated
        .clone()
        .with_human_actor_model(HumanActorModel::ContextSubjectV1)
        .unwrap_err();
    assert!(matches!(
        err,
        ActorModelError::LegacyMembershipPresent { .. }
    ));
    // The source domain is untouched (it was cloned; the original is intact).
    assert_eq!(
        populated.human_actor_model(),
        HumanActorModel::LegacyPrincipalV0
    );

    let mut config = GovernanceConfig::cooperative_default();
    config.membership = MembershipConfig::trust_threshold(0.5);
    let trust = GovernanceDomain::with_id(GovernanceDomainId::new("t"), "t".into(), config);
    assert!(matches!(
        trust
            .with_human_actor_model(HumanActorModel::ContextSubjectV1)
            .unwrap_err(),
        ActorModelError::LegacyMembershipPresent { .. }
    ));
}

/// The same invariant holds at the load boundary: a hand-written record that
/// combines Subject mode with legacy members must not deserialize.
#[test]
fn t3b_mixed_record_is_rejected_on_load() {
    let mut v = serde_json::to_value(legacy_domain("mixed", vec![did()])).unwrap();
    v["human_actor_model"] = serde_json::json!("context_subject_v1");
    assert!(serde_json::from_value::<GovernanceDomain>(v).is_err());

    let store = InMemoryGovernanceStore::new();
    let mut v = serde_json::to_value(legacy_domain("mixed2", vec![])).unwrap();
    v["config"]["membership"]["source"] = serde_json::json!({"TrustThreshold": 0.5});
    v["human_actor_model"] = serde_json::json!("context_subject_v1");
    assert!(serde_json::from_value::<GovernanceDomain>(v).is_err());
    assert!(store.list_domains().unwrap().is_empty());
}

/// Config replacement cannot smuggle legacy membership into a Subject domain,
/// and a refused replacement changes nothing.
#[test]
fn t3c_update_config_cannot_introduce_legacy_membership() {
    let mut domain = subject_domain("coop-u");
    let before = serde_json::to_value(&domain).unwrap();

    let mut smuggled = GovernanceConfig::cooperative_default();
    smuggled.membership = MembershipConfig::static_list(vec![did()]);
    assert!(domain.update_config(smuggled).is_err());
    assert_eq!(
        serde_json::to_value(&domain).unwrap(),
        before,
        "no mutation on refusal"
    );

    let mut trust = GovernanceConfig::cooperative_default();
    trust.membership = MembershipConfig::trust_threshold(0.1);
    assert!(domain.update_config(trust).is_err());
    assert_eq!(serde_json::to_value(&domain).unwrap(), before);

    // A config that keeps the inert shape is accepted, and the mode survives.
    let mut ok = GovernanceConfig::cooperative_default();
    ok.membership = MembershipConfig::static_list(vec![]);
    domain.update_config(ok).unwrap();
    assert_eq!(
        domain.human_actor_model(),
        HumanActorModel::ContextSubjectV1
    );
    assert!(matches!(
        domain.config.membership.source,
        MembershipSource::StaticList(ref m) if m.is_empty()
    ));
}

// ---------------------------------------------------------------------------
// Electorate resolution is a legacy-rail operation
// ---------------------------------------------------------------------------

#[test]
fn electorate_resolution_refuses_subject_domain() {
    let domain = subject_domain("coop-e");
    let r = StaticMembershipResolver::new();
    assert!(r.resolve_members(&domain).is_err());
    assert!(r.is_member(&domain, &did()).is_err());
    assert!(r.member_count(&domain).is_err());

    // A composite must not launder the refusal through an empty/odd chain.
    let composite =
        CompositeMembershipResolver::new().add_resolver(Box::new(StaticMembershipResolver::new()));
    assert!(composite.resolve_members(&domain).is_err());
    // ...and an empty composite must still not produce an electorate.
    assert!(CompositeMembershipResolver::new()
        .resolve_members(&domain)
        .is_err());
}

#[test]
fn legacy_rail_guard_names_the_operation() {
    let err = subject_domain("g")
        .require_legacy_rail(LegacyRailOp::VoteCast)
        .unwrap_err();
    assert!(err.to_string().contains("vote_cast"), "{err}");
    assert!(legacy_domain("g2", vec![did()])
        .require_legacy_rail(LegacyRailOp::VoteCast)
        .is_ok());
}

/// `GovernanceDomain.config` is a public field, so in-process code can build a
/// mixed domain without going through a guarded constructor. It must be
/// refused at the write boundary instead of becoming an unloadable row.
#[test]
fn store_refuses_to_persist_a_mixed_domain() {
    let mut mixed = subject_domain("mixed-in-process");
    mixed.config.membership = MembershipConfig::static_list(vec![did()]);

    let store = InMemoryGovernanceStore::new();
    assert!(store.store_domain(&mixed).is_err());
    assert!(store.get_domain(&mixed.id).unwrap().is_none());
    assert!(store.list_domains().unwrap().is_empty());
}

/// Same boundary on the sled-backed store, which is the one production uses.
#[test]
fn sled_store_refuses_to_persist_a_mixed_domain() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let store = icn_governance::store::SledGovernanceStore::new(db);

    let ok = subject_domain("subject-ok");
    store.store_domain(&ok).unwrap();

    let mut mixed = subject_domain("mixed-in-process");
    mixed.config.membership = MembershipConfig::static_list(vec![did()]);
    assert!(store.store_domain(&mixed).is_err());
    assert!(store.get_domain(&mixed.id).unwrap().is_none());
    // The valid row is intact and the listing still loads.
    assert_eq!(store.list_domains().unwrap().len(), 1);
}
