#![allow(clippy::unwrap_used, clippy::expect_used)]
//! G1-A `SemanticProposalCommitmentV1` (treasury-spend, v1): pinned vectors, mutation matrix,
//! refusals and the `verify` substitution check.
//!
//! The `EXPECT_*` literals were computed by `tests/reference/proposal_commitment_reference.py`,
//! an independent Python implementation of the documented layout. Re-check with
//! `python3 -I icn/crates/icn-governance/tests/reference/proposal_commitment_reference.py`.
//!
//! None of this wires the commitment into vote or close handling: it is a pure function of
//! (domain id, proposal id, payload).

use icn_governance::proposal::{ProposalPayload, TreasuryProposalOperation};
use icn_governance::proposal_commitment::{
    CommitmentRefusal, SemanticProposalCommitmentV1, TREASURY_SPEND_COMMITMENT_TAG,
};
use icn_identity::Did;

const DID_A: &str = "did:icn:z29d2S7vB453rNYFdR5Ycwt7y9haRT5fwVwL9zTmBhfV2";
const DID_B: &str = "did:icn:z3JF3sEqM796hk5WFqA6EtmEwJQ9quALszsfJyvXNQKy3";
const DID_C: &str = "did:icn:z4Ss5JMkXAD9Z7cktFEdrqeMuT6jGMF1pVozTyPHZ6zT4";

const EXPECT_BASE_BYTES_HEX: &str = "69636e3a676f763a70726f706f73616c2d636f6d6d69746d656e743a74726561737572792d7370656e643a76310900000000000000746573742d636f6f700e000000000000007370656e642d70726f702d30303135000000000000006469643a69636e3a7a3239643253377642343533724e5946645235596377743779396861525435667756774c397a546d426866563235000000000000006469643a69636e3a7a334a46337345714d373936686b35574671413645746d45774a51397175414c737a73664a7976584e514b79330500000000000000484f5552531a00000000000000737072696e742d32362070726f76656e616e63652070726f6f6664000000000000000000000000000000";
const EXPECT_BASE_HASH: &str = "e3bfc7e9b03adb4f6714cd6a2b4efe1806636f4954f5c9ba0d400f76da85d27b";
const EXPECT_OTHER_RECIPIENT_HASH: &str =
    "29f8983fd10f8c5bc7e8fbfb897adf13687b11fd46317f6c54973731e88d8082";
const EXPECT_AMOUNT_101_HASH: &str =
    "cd52f3e5e99368204e20f50e92daa82a89a42d904d861a2a7aa81846444b0096";
const EXPECT_NONCE_1_HASH: &str =
    "31d42b165ac6593d705b9a32fb9b186619efad699a06927ec2903c7f09c08eda";
const EXPECT_SHIFT_A_HASH: &str =
    "a4dfedcc740fe3827fa3e647526bbc570329de9cb73131563116f6e770b6c70f";
const EXPECT_SHIFT_B_HASH: &str =
    "a57d98b53297d9265644c907d3702de6768579b4dd5f19dd9f5cfdf3dcf859b4";
const EXPECT_UTF8_MEMO_HASH: &str =
    "01c12ed43a0218ce7c23a0b475355898ba12317ae3db8c759caaa029e8228fa0";

#[derive(Clone)]
struct Case {
    domain: &'static str,
    proposal: &'static str,
    treasury: &'static str,
    recipient: &'static str,
    currency: &'static str,
    memo: &'static str,
    amount: i64,
    nonce: u64,
}

const BASE: Case = Case {
    domain: "test-coop",
    proposal: "spend-prop-001",
    treasury: DID_A,
    recipient: DID_B,
    currency: "HOURS",
    memo: "sprint-26 provenance proof",
    amount: 100,
    nonce: 0,
};

fn payload(c: &Case) -> ProposalPayload {
    ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Spend {
            treasury_did: Did::from_str(c.treasury).unwrap(),
            amount: c.amount,
            currency: c.currency.to_string(),
            recipient: Did::from_str(c.recipient).unwrap(),
            memo: c.memo.to_string(),
            nonce: c.nonce,
        },
    }
}

fn commit(c: &Case) -> Result<SemanticProposalCommitmentV1, CommitmentRefusal> {
    SemanticProposalCommitmentV1::commit(c.domain, c.proposal, &payload(c))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hash_hex(c: &Case) -> String {
    hex(commit(c).unwrap().commitment_hash())
}

// Fails if the preimage layout, field order, tag or framing drifts from the documented v1.
#[test]
fn base_vector_bytes_and_hash_match_the_independent_reference() {
    let c = commit(&BASE).unwrap();
    assert_eq!(hex(c.canonical_bytes()), EXPECT_BASE_BYTES_HEX);
    assert_eq!(hex(c.commitment_hash()), EXPECT_BASE_HASH);
}

// Fails if the hash is not exactly BLAKE3 of the exposed canonical bytes.
#[test]
fn hash_is_blake3_of_the_exposed_canonical_bytes() {
    let c = commit(&BASE).unwrap();
    assert_eq!(
        c.commitment_hash(),
        blake3::hash(c.canonical_bytes()).as_bytes()
    );
}

// Fails if the preimage does not begin with the registered domain tag.
#[test]
fn canonical_bytes_start_with_the_versioned_tag() {
    let c = commit(&BASE).unwrap();
    assert!(c
        .canonical_bytes()
        .starts_with(TREASURY_SPEND_COMMITMENT_TAG.as_bytes()));
    assert_eq!(
        TREASURY_SPEND_COMMITMENT_TAG.as_bytes(),
        b"icn:gov:proposal-commitment:treasury-spend:v1"
    );
}

// Fails if anything nondeterministic (time, iteration order, randomness) enters the preimage.
#[test]
fn identical_input_gives_identical_bytes() {
    let a = commit(&BASE).unwrap();
    let b = commit(&BASE).unwrap();
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    assert_eq!(a, b);
}

// Fails if any single effect-relevant or binding field is omitted from the preimage.
#[test]
fn changing_any_one_field_changes_the_commitment() {
    let base = hash_hex(&BASE);
    let mutants: Vec<(&str, Case)> = vec![
        (
            "domain",
            Case {
                domain: "other-coop",
                ..BASE
            },
        ),
        (
            "proposal",
            Case {
                proposal: "spend-prop-002",
                ..BASE
            },
        ),
        (
            "treasury",
            Case {
                treasury: DID_C,
                ..BASE
            },
        ),
        (
            "recipient",
            Case {
                recipient: DID_C,
                ..BASE
            },
        ),
        (
            "currency",
            Case {
                currency: "hours",
                ..BASE
            },
        ),
        (
            "memo",
            Case {
                memo: "sprint-26 provenance proof.",
                ..BASE
            },
        ),
        (
            "amount",
            Case {
                amount: 101,
                ..BASE
            },
        ),
        ("nonce", Case { nonce: 1, ..BASE }),
    ];
    for (name, m) in mutants {
        assert_ne!(hash_hex(&m), base, "field `{name}` is not committed");
    }
}

// Fails if the destination or amount mutation is not the exact pinned digest.
#[test]
fn destination_amount_and_nonce_mutants_match_the_reference() {
    assert_eq!(
        hash_hex(&Case {
            recipient: DID_C,
            ..BASE
        }),
        EXPECT_OTHER_RECIPIENT_HASH
    );
    assert_eq!(
        hash_hex(&Case {
            amount: 101,
            ..BASE
        }),
        EXPECT_AMOUNT_101_HASH
    );
    assert_eq!(hash_hex(&Case { nonce: 1, ..BASE }), EXPECT_NONCE_1_HASH);
}

// Fails if string fields lose their length prefix (ambiguous framing: "ab"+"c" vs "a"+"bc").
#[test]
fn moving_a_byte_between_adjacent_strings_changes_the_commitment() {
    let a = hash_hex(&Case {
        domain: "ab",
        proposal: "c",
        ..BASE
    });
    let b = hash_hex(&Case {
        domain: "a",
        proposal: "bc",
        ..BASE
    });
    assert_ne!(a, b);
    assert_eq!(a, EXPECT_SHIFT_A_HASH);
    assert_eq!(b, EXPECT_SHIFT_B_HASH);
}

// Fails if the memo is normalised, truncated or re-encoded instead of committed as exact UTF-8.
#[test]
fn non_ascii_memo_is_committed_as_exact_utf8() {
    assert_eq!(
        hash_hex(&Case {
            memo: "caf\u{e9} \u{2713}",
            ..BASE
        }),
        EXPECT_UTF8_MEMO_HASH
    );
}

// Fails if the commitment accepts an amount that cannot be an authorized spend.
#[test]
fn non_positive_amounts_are_refused() {
    for amount in [0, -1, i64::MIN] {
        assert_eq!(
            commit(&Case { amount, ..BASE }).unwrap_err(),
            CommitmentRefusal::NonPositiveAmount(amount)
        );
    }
    assert!(commit(&Case {
        amount: i64::MAX,
        ..BASE
    })
    .is_ok());
}

// Fails if an empty required field is silently committed as an empty string.
#[test]
fn empty_required_fields_are_refused() {
    assert_eq!(
        commit(&Case { domain: "", ..BASE }).unwrap_err(),
        CommitmentRefusal::EmptyDomainId
    );
    assert_eq!(
        commit(&Case {
            proposal: "",
            ..BASE
        })
        .unwrap_err(),
        CommitmentRefusal::EmptyProposalId
    );
    assert_eq!(
        commit(&Case {
            currency: "",
            ..BASE
        })
        .unwrap_err(),
        CommitmentRefusal::EmptyCurrency
    );
    assert_eq!(
        commit(&Case { memo: "", ..BASE }).unwrap_err(),
        CommitmentRefusal::EmptyMemo
    );
}

// Fails if any other payload kind is given a commitment (fail closed, no invented encoding).
#[test]
fn every_other_payload_variant_is_refused_as_unsupported() {
    let text = ProposalPayload::Text {
        body: "hello".into(),
    };
    assert_eq!(
        SemanticProposalCommitmentV1::commit("d", "p", &text).unwrap_err(),
        CommitmentRefusal::UnsupportedPayload { kind: "text" }
    );
    let budget = ProposalPayload::Budget {
        amount: 5,
        currency: "HOURS".into(),
        recipient: Did::from_str(DID_B).unwrap(),
        purpose: "x".into(),
    };
    assert_eq!(
        SemanticProposalCommitmentV1::commit("d", "p", &budget).unwrap_err(),
        CommitmentRefusal::UnsupportedPayload { kind: "budget" }
    );
}

// Fails if a sibling treasury operation is committed under the Spend layout.
#[test]
fn sibling_treasury_operations_are_refused_as_unsupported() {
    let withdraw = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Withdraw {
            treasury_did: Did::from_str(DID_A).unwrap(),
            recipient: Did::from_str(DID_B).unwrap(),
            amount: 100,
            currency: "HOURS".into(),
            purpose: "p".into(),
            budget_id: None,
            nonce: 0,
        },
    };
    assert_eq!(
        SemanticProposalCommitmentV1::commit("d", "p", &withdraw).unwrap_err(),
        CommitmentRefusal::UnsupportedTreasuryOperation {
            operation: "withdraw"
        }
    );
    let create = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::CreateBudget {
            treasury_did: Did::from_str(DID_A).unwrap(),
            purpose: "p".into(),
            amount: 1,
            currency: "HOURS".into(),
            period_end: None,
        },
    };
    assert_eq!(
        SemanticProposalCommitmentV1::commit("d", "p", &create).unwrap_err(),
        CommitmentRefusal::UnsupportedTreasuryOperation {
            operation: "create_budget"
        }
    );
}

// Fails if `verify` accepts a payload other than the one committed (payload substitution),
// or a different domain / proposal id (domain replay, proposal-id reuse).
#[test]
fn verify_accepts_only_the_committed_triple() {
    let c = commit(&BASE).unwrap();
    assert!(c
        .verify(BASE.domain, BASE.proposal, &payload(&BASE))
        .is_ok());
    let swapped = payload(&Case {
        recipient: DID_C,
        ..BASE
    });
    assert_eq!(
        c.verify(BASE.domain, BASE.proposal, &swapped),
        Err(CommitmentRefusal::Mismatch)
    );
    assert_eq!(
        c.verify("other-coop", BASE.proposal, &payload(&BASE)),
        Err(CommitmentRefusal::Mismatch)
    );
    assert_eq!(
        c.verify(BASE.domain, "spend-prop-002", &payload(&BASE)),
        Err(CommitmentRefusal::Mismatch)
    );
}

// Fails if the same fields under a different tag/version hash alike (domain separation).
#[test]
fn same_fields_under_another_tag_hash_differently() {
    use icn_kernel_api::canonical::{CanonicalHasher, DomainTag};
    let c = commit(&BASE).unwrap();
    let mut h = CanonicalHasher::new(DomainTag::new(b"icn:gov:decision:v1"));
    h.str(BASE.domain)
        .str(BASE.proposal)
        .str(BASE.treasury)
        .str(BASE.recipient)
        .str(BASE.currency)
        .str(BASE.memo)
        .u64(BASE.amount as u64)
        .u64(BASE.nonce);
    assert_ne!(&h.finish(), c.commitment_hash());
    // and under the real tag the shared mechanism reproduces the commitment byte for byte.
    let mut h = CanonicalHasher::new(TREASURY_SPEND_COMMITMENT_TAG);
    h.str(BASE.domain)
        .str(BASE.proposal)
        .str(BASE.treasury)
        .str(BASE.recipient)
        .str(BASE.currency)
        .str(BASE.memo)
        .u64(BASE.amount as u64)
        .u64(BASE.nonce);
    assert_eq!(&h.finish(), c.commitment_hash());
}

// Fails if the commitment is confused with the legacy serde_json payload hash.
#[test]
fn commitment_is_not_the_legacy_json_payload_hash() {
    let v = serde_json::to_value(payload(&BASE)).unwrap();
    let legacy = blake3::hash(serde_json::to_string(&v).unwrap().as_bytes());
    assert_ne!(legacy.as_bytes(), commit(&BASE).unwrap().commitment_hash());
}

// Pins the legacy serde_json shape of the payload (the input to the legacy payload hash) so a
// change to it is a visible, deliberate break. It does not call `commit`.
#[test]
fn legacy_payload_json_shape_is_unchanged() {
    let json = serde_json::to_string(&payload(&BASE)).unwrap();
    assert_eq!(
        json,
        format!(
            "{{\"Treasury\":{{\"operation\":{{\"Spend\":{{\"treasury_did\":\"{DID_A}\",\"amount\":100,\"currency\":\"HOURS\",\"recipient\":\"{DID_B}\",\"memo\":\"sprint-26 provenance proof\",\"nonce\":0}}}}}}}}"
        )
    );
}

// Fails if any sibling treasury operation stops being refused, or its pinned name changes.
#[test]
fn remaining_treasury_operations_are_refused_by_name() {
    use icn_governance::proposal::TreasuryApprovalType;
    let t = || Did::from_str(DID_A).unwrap();
    let ops = vec![
        (
            "modify_spending_rule",
            TreasuryProposalOperation::ModifySpendingRule {
                treasury_did: t(),
                rule_id: None,
                name: "n".into(),
                threshold_amount: 1,
                currency: "HOURS".into(),
                approval_type: TreasuryApprovalType::SimpleMajority,
                is_active: true,
            },
        ),
        (
            "transfer_between_budgets",
            TreasuryProposalOperation::TransferBetweenBudgets {
                treasury_did: t(),
                from_budget: "a".into(),
                to_budget: "b".into(),
                amount: 1,
                currency: "HOURS".into(),
                reason: "r".into(),
            },
        ),
        (
            "cancel_budget",
            TreasuryProposalOperation::CancelBudget {
                budget_id: "a".into(),
                reason: "r".into(),
                return_to_treasury: true,
            },
        ),
        (
            "reclaim_budget",
            TreasuryProposalOperation::ReclaimBudget {
                budget_id: "a".into(),
                amount: 1,
                currency: "HOURS".into(),
                reason: "r".into(),
            },
        ),
    ];
    for (name, operation) in ops {
        let p = ProposalPayload::Treasury { operation };
        assert_eq!(
            SemanticProposalCommitmentV1::commit("d", "p", &p).unwrap_err(),
            CommitmentRefusal::UnsupportedTreasuryOperation { operation: name }
        );
    }
}

// Fails if an equal-principal DID spelling is committed as if it were the same bytes: the
// commitment is over the exact spelling the executor passes on, the fail-safe direction.
#[test]
fn another_spelling_of_the_same_principal_commits_differently() {
    let hex_spelling = format!("did:icn:f{}", "11".repeat(32));
    assert_eq!(
        Did::from_str(&hex_spelling).unwrap(),
        Did::from_str(DID_A).unwrap()
    );
    let alt_payload = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Spend {
            treasury_did: Did::from_str(&hex_spelling).unwrap(),
            amount: BASE.amount,
            currency: BASE.currency.to_string(),
            recipient: Did::from_str(BASE.recipient).unwrap(),
            memo: BASE.memo.to_string(),
            nonce: BASE.nonce,
        },
    };
    let alt =
        SemanticProposalCommitmentV1::commit(BASE.domain, BASE.proposal, &alt_payload).unwrap();
    let committed = commit(&BASE).unwrap();
    assert_ne!(alt.commitment_hash(), committed.commitment_hash());
    assert_eq!(
        committed.verify(BASE.domain, BASE.proposal, &alt_payload),
        Err(CommitmentRefusal::Mismatch)
    );
}
