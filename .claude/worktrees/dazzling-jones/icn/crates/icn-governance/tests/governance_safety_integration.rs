//! Integration test: governance safety pipeline
//!
//! Tests the full flow: Amendment -> EffectManifest -> InvariantGate -> PowerDiff
//! -> DraftWithArtifacts

use icn_governance::amendment::*;
use icn_governance::effect_manifest::*;
use icn_governance::invariant_gate::*;
use icn_kernel_api::invariants::*;

struct TestState;
impl InvariantStateView for TestState {
    fn current_min_timelock_blocks(&self, _: &str) -> u64 {
        0
    }
}

fn test_proposer() -> icn_identity::Did {
    icn_identity::Did::from_anchor_id(&[42u8; 32])
}

#[test]
fn test_full_pipeline_clean_amendment() {
    // 1. Create amendment
    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "sunrise-bakery".into(),
        },
        "Increase board seats".into(),
        "Add 2 seats to the board".into(),
        test_proposer(),
    );

    // 2. Generate manifest (no authz/econ effects for this simple case)
    let manifest = EffectManifest::new(
        amendment.document_hash.unwrap_or([0u8; 32]),
        [0u8; 32], // baseline snapshot
        "did:icn:proposer".into(),
        vec![],
        vec![],
        vec![],
        vec![],
    );

    // 3. Run invariant gate
    let state = TestState;
    let ctx = InvariantContext {
        current_block: 1000,
        manifest: &manifest,
        power_diff: None,
        state: &state,
    };
    let gate = InvariantGate::default_frozen_core();
    let report = gate.evaluate(&ctx);
    assert!(report.passed);

    // 4. Attach artifacts
    let result = amendment.attach_artifacts(manifest, None, report);
    assert!(result.is_ok());
    assert!(matches!(
        amendment.status,
        AmendmentStatus::DraftWithArtifacts { .. }
    ));

    // 5. Submit for review
    let result = amendment.submit_for_review();
    assert!(result.is_ok());
    assert!(matches!(
        amendment.status,
        AmendmentStatus::Submitted { .. }
    ));
}

#[test]
fn test_pipeline_rejects_wildcard_capability() {
    let mut amendment = Amendment::new(
        AmendmentType::Governance,
        AmendmentScope::Jurisdiction {
            domain_id: "test".into(),
        },
        "Grant emergency powers".into(),
        "Give founder all permissions".into(),
        test_proposer(),
    );

    // Manifest with wildcard -- should be rejected by invariant #5
    let manifest = EffectManifest::new(
        [0u8; 32],
        [0u8; 32],
        "did:icn:proposer".into(),
        vec![CapabilityEffect::Grant {
            subject: "did:icn:founder".into(),
            capability: "admin".into(),
            scope: "*".into(),
            resource: "*".into(),
        }],
        vec![],
        vec![],
        vec![],
    );

    let state = TestState;
    let ctx = InvariantContext {
        current_block: 1000,
        manifest: &manifest,
        power_diff: None,
        state: &state,
    };
    let gate = InvariantGate::default_frozen_core();
    let report = gate.evaluate(&ctx);
    assert!(!report.passed);

    // Attach should fail because report has violations
    let result = amendment.attach_artifacts(manifest, None, report);
    assert!(result.is_err());
    assert!(matches!(amendment.status, AmendmentStatus::Draft));
}

#[test]
fn test_pipeline_rejects_flash_governance() {
    let mut amendment = Amendment::new(
        AmendmentType::Economic,
        AmendmentScope::Jurisdiction {
            domain_id: "test".into(),
        },
        "Change credit limits".into(),
        "Increase all limits 10x".into(),
        test_proposer(),
    );

    // Manifest with economic effects but too-short timelock
    let mut manifest = EffectManifest::new(
        [0u8; 32],
        [0u8; 32],
        "did:icn:proposer".into(),
        vec![],
        vec![EconomicEffect::MutualCreditPolicyChange {
            entity_id: "coop:x".into(),
            policy_key: "credit_limit".into(),
            before_hash: [0u8; 32],
            after_hash: [1u8; 32],
        }],
        vec![],
        vec![],
    );
    manifest.ratification_block = Some(1000);
    manifest.activation_block = Some(1010); // Only 10 blocks -- way too short

    let state = TestState;
    let ctx = InvariantContext {
        current_block: 1000,
        manifest: &manifest,
        power_diff: None,
        state: &state,
    };
    let gate = InvariantGate::default_frozen_core();
    let report = gate.evaluate(&ctx);
    assert!(!report.passed);

    // Verify the specific invariant that failed
    let timelock_violation = report
        .violations
        .iter()
        .find(|v| v.id == InvariantId::MandatoryExecutionDelay);
    assert!(timelock_violation.is_some());

    // Attach should fail
    let result = amendment.attach_artifacts(manifest, None, report);
    assert!(result.is_err());
}

#[test]
fn test_cannot_submit_without_artifacts() {
    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "test".into(),
        },
        "Test".into(),
        "Test".into(),
        test_proposer(),
    );

    // Try to submit directly -- should fail
    let result = amendment.submit_for_review();
    assert!(result.is_err());
}
