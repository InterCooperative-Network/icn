//! Integration test: YAML charter → ConstraintSet → kernel enforcement
//!
//! Exercises the full `charter_to_constraints()` bridge with a realistic
//! worker cooperative charter that uses all three schema sections.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_ccl::schema::bridge::{charter_to_constraints, CharterContext};
use icn_ccl::schema::CclDocument;
use icn_kernel_api::authz::ConstraintValue;

/// Full worker cooperative charter.
///
/// Notes on field names in expressions:
/// - Uses `board_seats` (flat), not `board.seats` (dotted) — EvalContext
///   does not support nested references.
/// - Surplus fractions are literal floats so `validate()` passes.
const WORKER_COOP_CHARTER: &str = r#"schema_version: v0
governance:
  bodies:
    - name: general_assembly
      composition: all_members

    - name: board
      seats: 7
      elected_by: general_assembly
      term:
        years: 2
        staggered: true
      recall:
        petition: "0.25 * members"

  decisions:
    - name: ordinary
      authority: general_assembly
      threshold: simple_majority
      quorum: "0.25 * members"

    - name: constitutional
      authority: general_assembly
      threshold:
        fraction: "2/3"
      quorum: "0.50 * members"
      ratification_period:
        days: 30

  delegation:
    allowed: true
    transitive: false
    revocable: instant
    scope: [vote, propose]

economics:
  capital:
    member_equity:
      minimum: 100
      maximum: 5000
      interest_rate: 0.0
      refund_on_exit:
        full: true
        within:
          days: 90

  credit:
    eligibility:
      field: membership_months
      op: ">="
      value: 6
    limit: "min(1000, patronage * 0.5)"
    terms: net30

  surplus:
    allocation:
      - target: reserves
        fraction: "0.20"
        until: "reserves >= 6 * monthly_operating"
      - target: patronage_refund
        fraction: "0.70"
        proportional_to: purchases
      - target: worker_bonus
        fraction: "0.10"
        condition: "worker_owners_exist"
"#;

fn parse_charter() -> CclDocument {
    CclDocument::from_yaml(WORKER_COOP_CHARTER).expect("Failed to parse charter YAML")
}

fn charter_context() -> CharterContext {
    CharterContext::new()
        .with_members(100)
        .with_board_seats(7)
        .with_patronage(800.0)
        .with_patronage_history(800.0)
        .with_membership_months(12)
        .with_trust_score(0.85)
        .with_reserves(3000.0)
        .with_monthly_operating(1000.0)
        .with_worker_owners_exist(true)
}

// ── Governance assertions ─────────────────────────────────────────────────────

#[test]
fn test_ordinary_threshold_is_simple_majority() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    assert_eq!(
        cs.custom.get("min_votes_ordinary"),
        Some(&ConstraintValue::from(0.5f64)),
        "ordinary decision must require simple majority (0.5)"
    );
}

#[test]
fn test_constitutional_threshold_is_two_thirds() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    match cs.custom.get("min_votes_constitutional") {
        Some(ConstraintValue::Float(f)) => {
            assert!(
                (**f - 2.0 / 3.0).abs() < 1e-9,
                "constitutional threshold must be 2/3, got {}",
                **f
            );
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_ordinary_quorum_with_100_members() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    // "0.25 * members" = 0.25 * 100 = 25.0
    match cs.custom.get("min_quorum_ordinary") {
        Some(ConstraintValue::Float(f)) => {
            assert!((**f - 25.0).abs() < 1e-9, "Expected 25.0, got {}", **f);
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_constitutional_quorum_with_100_members() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    // "0.50 * members" = 0.50 * 100 = 50.0
    match cs.custom.get("min_quorum_constitutional") {
        Some(ConstraintValue::Float(f)) => {
            assert!((**f - 50.0).abs() < 1e-9, "Expected 50.0, got {}", **f);
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_board_body_seats_and_term() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    assert_eq!(
        cs.custom.get("body_board_seats"),
        Some(&ConstraintValue::Int(7)),
        "board must have 7 seats"
    );
    assert_eq!(
        cs.custom.get("body_board_term_years"),
        Some(&ConstraintValue::Int(2)),
        "board term must be 2 years"
    );
}

#[test]
fn test_general_assembly_has_no_seats_key() {
    // general_assembly has no seats field — must not produce a seats constraint
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    assert!(
        !cs.custom.contains_key("body_general_assembly_seats"),
        "general_assembly has no seats — key must not be emitted"
    );
}

#[test]
fn test_delegation_config() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    assert_eq!(
        cs.custom.get("delegation_allowed"),
        Some(&ConstraintValue::Bool(true))
    );
    assert_eq!(
        cs.custom.get("delegation_transitive"),
        Some(&ConstraintValue::Bool(false))
    );
}

// ── Economics assertions ──────────────────────────────────────────────────────

#[test]
fn test_equity_minimum_and_maximum() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    assert_eq!(
        cs.custom.get("equity_min"),
        Some(&ConstraintValue::Int(100))
    );
    assert_eq!(
        cs.custom.get("equity_max"),
        Some(&ConstraintValue::Int(5000))
    );
}

#[test]
fn test_credit_limit_expression_with_patronage_800() {
    let doc = parse_charter();
    // min(1000, 800 * 0.5) = min(1000, 400) = 400
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    match cs.custom.get("credit_limit") {
        Some(ConstraintValue::Float(f)) => {
            assert!((**f - 400.0).abs() < 1e-9, "Expected 400.0, got {}", **f);
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_credit_limit_caps_at_1000_for_high_patronage() {
    let doc = parse_charter();
    // min(1000, 5000 * 0.5) = min(1000, 2500) = 1000
    let ctx = CharterContext::new()
        .with_members(100)
        .with_patronage(5000.0);
    let cs = charter_to_constraints(&doc, &ctx).unwrap();

    match cs.custom.get("credit_limit") {
        Some(ConstraintValue::Float(f)) => {
            assert!((**f - 1000.0).abs() < 1e-9, "Expected 1000.0, got {}", **f);
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_credit_eligibility_min_months() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    assert_eq!(
        cs.custom.get("credit_min_months"),
        Some(&ConstraintValue::Int(6)),
        "credit eligibility requires 6 months membership"
    );
}

#[test]
fn test_credit_payment_terms() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    assert_eq!(
        cs.custom.get("credit_payment_terms"),
        Some(&ConstraintValue::from("net30"))
    );
}

#[test]
fn test_surplus_reserves_fraction() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    match cs.custom.get("surplus_reserves_pct") {
        Some(ConstraintValue::Float(f)) => {
            assert!((**f - 0.20).abs() < 1e-9, "Expected 0.20, got {}", **f);
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_surplus_patronage_refund_fraction() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    match cs.custom.get("surplus_patronage_refund_pct") {
        Some(ConstraintValue::Float(f)) => {
            assert!((**f - 0.70).abs() < 1e-9, "Expected 0.70, got {}", **f);
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_surplus_worker_bonus_fraction() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    match cs.custom.get("surplus_worker_bonus_pct") {
        Some(ConstraintValue::Float(f)) => {
            assert!((**f - 0.10).abs() < 1e-9, "Expected 0.10, got {}", **f);
        }
        other => panic!("Expected Float, got {:?}", other),
    }
}

#[test]
fn test_surplus_reserves_until_stored_as_opaque_string() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    match cs.custom.get("surplus_reserves_until") {
        Some(ConstraintValue::String(s)) => {
            assert!(!s.is_empty(), "until condition must not be empty string");
        }
        other => panic!("Expected String, got {:?}", other),
    }
}

#[test]
fn test_surplus_worker_bonus_condition_stored_as_opaque_string() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    match cs.custom.get("surplus_worker_bonus_condition") {
        Some(ConstraintValue::String(s)) => {
            assert_eq!(s, "worker_owners_exist");
        }
        other => panic!("Expected String, got {:?}", other),
    }
}

// ── Determinism assertion ─────────────────────────────────────────────────────

#[test]
fn test_same_context_produces_identical_constraint_set() {
    let doc = parse_charter();
    let ctx = charter_context();

    let cs1 = charter_to_constraints(&doc, &ctx).unwrap();
    let cs2 = charter_to_constraints(&doc, &ctx).unwrap();

    // ConstraintSet.custom: HashMap<String, ConstraintValue> — compare key by key
    for (key, val) in &cs1.custom {
        assert_eq!(
            cs2.custom.get(key),
            Some(val),
            "Constraint '{}' must be deterministic",
            key
        );
    }
    assert_eq!(cs1.custom.len(), cs2.custom.len(), "Key count must match");
}

// ── Total key count sanity check ──────────────────────────────────────────────

#[test]
fn test_expected_constraint_keys_produced() {
    let doc = parse_charter();
    let cs = charter_to_constraints(&doc, &charter_context()).unwrap();

    let expected_keys = [
        // Governance — decisions
        "min_votes_ordinary",
        "min_votes_constitutional",
        "min_quorum_ordinary",
        "min_quorum_constitutional",
        // Governance — bodies
        "body_board_seats",
        "body_board_term_years",
        // Governance — delegation
        "delegation_allowed",
        "delegation_transitive",
        // Economics — equity
        "equity_min",
        "equity_max",
        "equity_interest_rate",
        // Economics — credit
        "credit_limit",
        "credit_min_months",
        "credit_payment_terms",
        // Economics — surplus
        "surplus_reserves_pct",
        "surplus_reserves_until",
        "surplus_patronage_refund_pct",
        "surplus_worker_bonus_pct",
        "surplus_worker_bonus_condition",
    ];

    for key in &expected_keys {
        assert!(
            cs.custom.contains_key(*key),
            "Expected constraint key '{}' to be present",
            key
        );
    }
}

/// Verify that every template in `contracts/templates/` parses and produces
/// at least one constraint.  This catches YAML syntax errors and schema
/// validation failures before they reach the oracle at runtime.
#[test]
fn test_all_templates_parse_and_produce_constraints() {
    // CARGO_MANIFEST_DIR is `icn/crates/icn-ccl`; templates live three levels up.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let templates_dir = std::path::PathBuf::from(manifest_dir).join("../../../contracts/templates");

    let templates = [
        "worker-coop.yaml",
        "consumer-coop.yaml",
        "housing-coop.yaml",
        "community-org.yaml",
        "federation.yaml",
    ];

    let ctx = CharterContext::new()
        .with_members(100)
        .with_patronage(1000.0)
        .with_membership_months(12)
        .with_reserves(5000.0)
        .with_monthly_operating(500.0)
        .with_worker_owners_exist(true);

    for name in &templates {
        let path = templates_dir.join(name);
        let yaml = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read template {name}: {e}"));

        let doc = CclDocument::from_yaml(&yaml)
            .unwrap_or_else(|e| panic!("Template {name} failed to parse: {e}"));

        let cs = charter_to_constraints(&doc, &ctx)
            .unwrap_or_else(|e| panic!("Template {name} failed charter_to_constraints: {e}"));

        assert!(
            !cs.custom.is_empty(),
            "Template {name} produced an empty ConstraintSet — check governance section"
        );
    }
}
