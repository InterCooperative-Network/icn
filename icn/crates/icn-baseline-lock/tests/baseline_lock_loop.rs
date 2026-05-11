//! Executable baseline-lock loop + negative cases.
#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::field_reassign_with_default)]

use icn_baseline_lock::{
    action_card_allocation_authorized, build_receipt_chain, evaluate_guest, project,
    validate_hostile_output, BaselineProcessGateResultReceipt, EvidencePacket,
    ExecutionInputEnvelopeV1, FixtureKeys, FixtureOptions, HostError, ProjectedState,
    ProjectorError, ABI_VERSION, INPUT_SCHEMA_ID, OUTPUT_SCHEMA_ID, PROCESS_ID, RULE_REF,
    TARGET_REF, WORKLOAD_ID,
};
use icn_boundary::{DeterminismClass, FinalityClass, Hash, PrivacyClass, ResultKind};
use icn_encoding::encode;
use icn_kernel_api::receipts::CanonicalReceipt;
use icn_kernel_api::{AllocationReceipt, ScopeLevel};

const WASM_BYTES: &[u8] = include_bytes!("fixtures/icn_baseline_lock_guest.wasm");
const MAX_OUT: usize = 64 * 1024;

fn module_hash() -> Hash {
    Hash(*blake3::hash(WASM_BYTES).as_bytes())
}

#[test]
fn test_baseline_lock_loop() {
    let keys = FixtureKeys::default();
    let receipts = build_receipt_chain(&keys, &FixtureOptions::default());
    let projected = project(&receipts).expect("project");
    assert_eq!(projected.facts.approvals, 2);
    assert_eq!(projected.facts.required_approvals, 2);
    assert!(projected.facts.notice_delivered);

    let mh = module_hash();
    let mut input_artifact_refs = projected.capsule.receipt_hashes_sorted.clone();
    input_artifact_refs.sort();

    let input = ExecutionInputEnvelopeV1 {
        abi_version: ABI_VERSION,
        schema_id: INPUT_SCHEMA_ID.into(),
        workload_id: WORKLOAD_ID.into(),
        module_hash: mh,
        process_id: PROCESS_ID.into(),
        target_ref: TARGET_REF.into(),
        state_resolution_capsule_hash: projected.capsule_hash,
        canonical_fact_snapshot_hash: projected.canonical_fact_snapshot_hash,
        authority_context_hash: projected.authority_context_hash,
        standing_context_hash: projected.capsule.standing_context_hash,
        agreement_context_hash: projected.agreement_context_hash,
        mandate_ref: None,
        rule_ref: RULE_REF.into(),
        determinism_class: DeterminismClass::Strict,
        privacy_class: PrivacyClass::Public,
        finality_class: FinalityClass::InstitutionLocal,
        fuel_limit: 200_000,
        input_artifact_refs,
        canonical_facts: projected.facts.clone(),
        expected_output_schema: OUTPUT_SCHEMA_ID.into(),
    };

    let input_bytes = encode(&input).expect("encode input");
    let input_envelope_hash = Hash(*blake3::hash(&input_bytes).as_bytes());

    let (out, fuel) = evaluate_guest(WASM_BYTES, &mh, &input, MAX_OUT).expect("wasm evaluate");
    assert!(out.passed, "guest passed");
    assert!(fuel <= input.fuel_limit);

    let output_bytes = encode(&out).expect("encode output");
    let output_envelope_hash = Hash(*blake3::hash(&output_bytes).as_bytes());

    let gate = BaselineProcessGateResultReceipt {
        session_id: PROCESS_ID.into(),
        passed: true,
        input_envelope_hash,
        output_envelope_hash,
        module_hash: mh,
        recorded_at: 1_700_000_000,
    };
    let gate_record_hash = gate.record_hash();
    let _gate_sig = gate.sign(&keys.host);

    let alloc = AllocationReceipt::new(gate_record_hash.0, ScopeLevel::Org);
    let allocation_canonical = alloc.canonical_hash();

    let evidence = EvidencePacket {
        input_envelope_hash,
        output_envelope_hash,
        module_hash: mh,
        canonical_fact_snapshot_hash: projected.canonical_fact_snapshot_hash,
        state_resolution_capsule_hash: projected.capsule_hash,
        receipt_ref_hashes: projected.capsule.receipt_hashes_sorted.clone(),
        gate_result_receipt_hash: gate_record_hash,
        allocation_receipt_canonical_hash: Hash(allocation_canonical),
    };

    assert_eq!(evidence.module_hash, mh);
    assert_ne!(evidence.input_envelope_hash, Hash([0u8; 32]));

    let card = action_card_allocation_authorized();
    assert_eq!(card.title, "Allocation authorized");
    assert_eq!(card.status, "Completed");
}

#[test]
fn negative_invalid_signature() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.invalid_signature_receipt_index = Some(0);
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::ReceiptVerify(_))
    ));
}

#[test]
fn negative_non_member_vote() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.non_member_vote = true;
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::NonMemberVote)
    ));
}

#[test]
fn negative_duplicate_vote() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.duplicate_vote_member = Some(0);
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::DuplicateVote)
    ));
}

#[test]
fn negative_missing_notice() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.skip_notice_index = Some(0);
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::MissingNotice)
    ));
}

#[test]
fn negative_threshold() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.threshold_fail = true;
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::ThresholdNotMet)
    ));
}

#[test]
fn negative_wasm_wrong_process_id() {
    let (input, mh) = happy_input();
    let (mut out, fuel) = evaluate_guest(WASM_BYTES, &mh, &input, MAX_OUT).unwrap();
    out.process_id = "evil".into();
    let out_len = encode(&out).expect("encode").len();
    let err = validate_hostile_output(&input, &out, fuel, MAX_OUT, out_len).unwrap_err();
    assert!(matches!(err, HostError::Hostile("process_id")));
}

#[test]
fn negative_wasm_wrong_result_kind() {
    let (input, mh) = happy_input();
    let (mut out, fuel) = evaluate_guest(WASM_BYTES, &mh, &input, MAX_OUT).unwrap();
    out.result_kind = ResultKind::MutationProposal;
    let out_len = encode(&out).expect("encode").len();
    let err = validate_hostile_output(&input, &out, fuel, MAX_OUT, out_len).unwrap_err();
    assert!(matches!(err, HostError::Hostile("result_kind")));
}

#[test]
fn negative_allocation_over_limit_guest_fails() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.allocation_over_limit = true;
    let receipts = build_receipt_chain(&keys, &opt);
    let projected = project(&receipts).expect("facts still project");
    assert!(projected.facts.allocation_requested > projected.facts.allocation_limit);
    let mh = module_hash();
    let input = mk_input(&projected, mh);
    let (out, _) = evaluate_guest(WASM_BYTES, &mh, &input, MAX_OUT).expect("run");
    assert!(!out.passed);
}

#[test]
fn negative_double_reservation() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.double_reservation_consumed = true;
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::MissingReservation)
    ));
}

#[test]
fn negative_module_hash_mismatch() {
    let (input, _) = happy_input();
    let bad = Hash([0x11; 32]);
    let err = evaluate_guest(WASM_BYTES, &bad, &input, MAX_OUT).unwrap_err();
    assert!(matches!(err, HostError::ModuleHashMismatch));
}

#[test]
fn negative_oversized_output() {
    let (input, mh) = happy_input();
    let err = evaluate_guest(WASM_BYTES, &mh, &input, 8).unwrap_err();
    assert!(matches!(err, HostError::OutputTooLarge(_, _)));
}

#[test]
fn negative_wrong_process_fixture() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.wrong_process_id_globally = true;
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::ReceiptVerify(_))
    ));
}

#[test]
fn negative_wrong_target_fixture() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.wrong_target_ref_globally = true;
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::ReceiptVerify(_))
    ));
}

#[test]
fn negative_broken_prior_link() {
    let keys = FixtureKeys::default();
    let mut opt = FixtureOptions::default();
    opt.broken_prior_link_index = Some(3);
    let receipts = build_receipt_chain(&keys, &opt);
    assert!(matches!(
        project(&receipts),
        Err(ProjectorError::PriorLink(_))
    ));
}

fn happy_input() -> (ExecutionInputEnvelopeV1, Hash) {
    let keys = FixtureKeys::default();
    let receipts = build_receipt_chain(&keys, &FixtureOptions::default());
    let projected = project(&receipts).unwrap();
    let mh = module_hash();
    (mk_input(&projected, mh), mh)
}

fn mk_input(projected: &ProjectedState, mh: Hash) -> ExecutionInputEnvelopeV1 {
    let mut input_artifact_refs = projected.capsule.receipt_hashes_sorted.clone();
    input_artifact_refs.sort();
    ExecutionInputEnvelopeV1 {
        abi_version: ABI_VERSION,
        schema_id: INPUT_SCHEMA_ID.into(),
        workload_id: WORKLOAD_ID.into(),
        module_hash: mh,
        process_id: PROCESS_ID.into(),
        target_ref: TARGET_REF.into(),
        state_resolution_capsule_hash: projected.capsule_hash,
        canonical_fact_snapshot_hash: projected.canonical_fact_snapshot_hash,
        authority_context_hash: projected.authority_context_hash,
        standing_context_hash: projected.capsule.standing_context_hash,
        agreement_context_hash: projected.agreement_context_hash,
        mandate_ref: None,
        rule_ref: RULE_REF.into(),
        determinism_class: DeterminismClass::Strict,
        privacy_class: PrivacyClass::Public,
        finality_class: FinalityClass::InstitutionLocal,
        fuel_limit: 200_000,
        input_artifact_refs,
        canonical_facts: projected.facts.clone(),
        expected_output_schema: OUTPUT_SCHEMA_ID.into(),
    }
}
