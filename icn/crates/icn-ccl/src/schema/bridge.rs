//! Charter-to-constraint bridge.
//!
//! Converts a [`CclDocument`] into a [`ConstraintSet`] by evaluating charter
//! fields against a [`CharterContext`].
//!
//! This is the **Meaning Firewall** boundary for charters: cooperative
//! semantics (vote thresholds, credit limits, surplus rules) are evaluated
//! here and converted to generic constraints the kernel enforces blindly.
//!
//! # Expression Evaluation
//!
//! Many charter fields are expression strings (`"0.67 * members"`,
//! `"min(1000, patronage * 0.5)"`). The [`CharterContext`] binds runtime data
//! (member count, patronage, trust score) so the expression evaluator can
//! resolve them.
//!
//! # Flat Field Names
//!
//! [`EvalContext`] does not support dotted field references (`board.seats`).
//! All runtime fields must be pre-flattened. Charter expression strings must
//! reference them by their flat name (`board_seats`, not `board.seats`).

use super::{
    parse_expr, AgreementSchema, AllocationTarget, CclDocument, DisputeStageName, EconomicsSchema,
    EvalContext, GovernanceSchema, PaymentTerms, SchemaError, SchemaValue, SettlementCycle,
    SettlementMethod, ThresholdName, VoteThreshold,
};
use icn_kernel_api::authz::{ConstraintSet, ConstraintValue};

/// Runtime data available when evaluating charter expressions.
///
/// All fields are pre-flattened because [`EvalContext`] does not support
/// dotted field references. Charter YAML expressions must use flat names
/// (e.g., `board_seats`) matching the fields bound here.
///
/// # Builder Pattern
///
/// ```
/// # use icn_ccl::schema::bridge::CharterContext;
/// let ctx = CharterContext::new()
///     .with_members(100)
///     .with_board_seats(7)
///     .with_patronage(800.0);
/// ```
#[derive(Debug, Clone, Default)]
pub struct CharterContext {
    eval_ctx: EvalContext,
}

impl CharterContext {
    /// Create an empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set member count.
    pub fn with_members(mut self, count: u64) -> Self {
        self.eval_ctx
            .set_field("members", SchemaValue::Float(count as f64));
        self
    }

    /// Set board seat count (flat; not `board.seats`).
    pub fn with_board_seats(mut self, seats: u32) -> Self {
        self.eval_ctx
            .set_field("board_seats", SchemaValue::Float(seats as f64));
        self
    }

    /// Set trust score (0.0–1.0).
    pub fn with_trust_score(mut self, score: f64) -> Self {
        self.eval_ctx
            .set_field("trust_score", SchemaValue::Float(score));
        self
    }

    /// Set current-period patronage amount.
    pub fn with_patronage(mut self, amount: f64) -> Self {
        self.eval_ctx
            .set_field("patronage", SchemaValue::Float(amount));
        self
    }

    /// Set historical patronage amount (lookback variant).
    pub fn with_patronage_history(mut self, amount: f64) -> Self {
        self.eval_ctx
            .set_field("patronage_history", SchemaValue::Float(amount));
        self
    }

    /// Set membership duration in months.
    pub fn with_membership_months(mut self, months: u32) -> Self {
        self.eval_ctx
            .set_field("membership_months", SchemaValue::Float(months as f64));
        self
    }

    /// Set current reserve balance.
    pub fn with_reserves(mut self, amount: f64) -> Self {
        self.eval_ctx
            .set_field("reserves", SchemaValue::Float(amount));
        self
    }

    /// Set monthly operating cost.
    pub fn with_monthly_operating(mut self, amount: f64) -> Self {
        self.eval_ctx
            .set_field("monthly_operating", SchemaValue::Float(amount));
        self
    }

    /// Set whether worker-owners exist (for conditional surplus rules).
    pub fn with_worker_owners_exist(mut self, exists: bool) -> Self {
        self.eval_ctx
            .set_field("worker_owners_exist", SchemaValue::Bool(exists));
        self
    }
}

/// Translate a CCL charter document into a kernel [`ConstraintSet`].
///
/// Processes governance, economics, and agreement sections if present.
/// Sections absent from the document produce no constraints — this is not an
/// error.
///
/// # Errors
///
/// Returns [`SchemaError`] if any expression fails to parse or evaluate, or
/// if an expression produces a non-finite result (NaN / Infinity).
pub fn charter_to_constraints(
    doc: &CclDocument,
    ctx: &CharterContext,
) -> Result<ConstraintSet, SchemaError> {
    let mut constraints = ConstraintSet::new();

    if let Some(gov) = &doc.governance {
        constraints = apply_governance(constraints, gov, ctx)?;
    }

    if let Some(econ) = &doc.economics {
        constraints = apply_economics(constraints, econ, ctx)?;
    }

    if let Some(agreement) = &doc.agreement {
        constraints = apply_agreement(constraints, agreement)?;
    }

    Ok(constraints)
}

// ─── Governance ──────────────────────────────────────────────────────────────

fn apply_governance(
    mut constraints: ConstraintSet,
    gov: &GovernanceSchema,
    ctx: &CharterContext,
) -> Result<ConstraintSet, SchemaError> {
    // Decision types: threshold and optional quorum per type
    for decision in &gov.decisions {
        let name = &decision.name;

        let threshold = eval_threshold(&decision.threshold, &ctx.eval_ctx)?;
        constraints = constraints.with_custom(
            format!("min_votes_{}", name),
            ConstraintValue::from(threshold),
        );

        if let Some(quorum_expr) = &decision.quorum {
            let expr = parse_expr(quorum_expr)?;
            let val = ctx.eval_ctx.evaluate(&expr)?;
            let q = val.as_f64().ok_or_else(|| {
                SchemaError::Expression(format!(
                    "Quorum expression '{}' evaluated to non-finite value",
                    quorum_expr
                ))
            })?;
            constraints =
                constraints.with_custom(format!("min_quorum_{}", name), ConstraintValue::from(q));
        }
    }

    // Bodies: seats and term years (when present)
    for body in &gov.bodies {
        let name = &body.name;

        if let Some(seats) = body.seats {
            constraints = constraints.with_custom(
                format!("body_{}_seats", name),
                ConstraintValue::from(seats as i64),
            );
        }

        if let Some(term) = &body.term {
            if let Some(years) = term.years {
                constraints = constraints.with_custom(
                    format!("body_{}_term_years", name),
                    ConstraintValue::from(years as i64),
                );
            }
        }
    }

    // Delegation
    if let Some(delegation) = &gov.delegation {
        constraints = constraints
            .with_custom(
                "delegation_allowed",
                ConstraintValue::from(delegation.allowed),
            )
            .with_custom(
                "delegation_transitive",
                ConstraintValue::from(delegation.transitive),
            );
    }

    Ok(constraints)
}

fn eval_threshold(threshold: &VoteThreshold, ctx: &EvalContext) -> Result<f64, SchemaError> {
    match threshold {
        VoteThreshold::Named(name) => Ok(named_threshold_ratio(*name)),
        VoteThreshold::Fraction { fraction } => {
            let expr = parse_expr(fraction)?;
            let val = ctx.evaluate(&expr)?;
            val.as_f64().ok_or_else(|| {
                SchemaError::Expression(format!(
                    "Threshold fraction '{}' evaluated to non-finite value",
                    fraction
                ))
            })
        }
        VoteThreshold::Expression(s) => {
            let expr = parse_expr(s)?;
            let val = ctx.evaluate(&expr)?;
            val.as_f64().ok_or_else(|| {
                SchemaError::Expression(format!(
                    "Threshold expression '{}' evaluated to non-finite value",
                    s
                ))
            })
        }
    }
}

fn named_threshold_ratio(name: ThresholdName) -> f64 {
    match name {
        ThresholdName::SimpleMajority => 0.5,
        ThresholdName::Supermajority => 2.0 / 3.0,
        ThresholdName::Unanimous => 1.0,
        ThresholdName::Consensus => 1.0,
    }
}

// ─── Economics ───────────────────────────────────────────────────────────────

fn apply_economics(
    mut constraints: ConstraintSet,
    econ: &EconomicsSchema,
    ctx: &CharterContext,
) -> Result<ConstraintSet, SchemaError> {
    // Member equity (all literal — no expression evaluation needed)
    if let Some(capital) = &econ.capital {
        if let Some(equity) = &capital.member_equity {
            if let Some(min) = equity.minimum {
                constraints = constraints.with_custom("equity_min", ConstraintValue::from(min));
            }
            if let Some(max) = equity.maximum {
                constraints = constraints.with_custom("equity_max", ConstraintValue::from(max));
            }
            constraints = constraints.with_custom(
                "equity_interest_rate",
                ConstraintValue::from(equity.interest_rate),
            );
        }
    }

    // Credit
    if let Some(credit) = &econ.credit {
        if let Some(limit_expr) = &credit.limit {
            let expr = parse_expr(limit_expr)?;
            let val = ctx.eval_ctx.evaluate(&expr)?;
            let limit = val.as_f64().ok_or_else(|| {
                SchemaError::Expression(format!(
                    "Credit limit expression '{}' evaluated to non-finite value",
                    limit_expr
                ))
            })?;
            constraints = constraints.with_custom("credit_limit", ConstraintValue::from(limit));
        }

        if let Some(eligibility) = &credit.eligibility {
            if eligibility.field == "membership_months" {
                if let Some(months) = eligibility.value.as_i64() {
                    constraints =
                        constraints.with_custom("credit_min_months", ConstraintValue::from(months));
                }
            }
        }

        if let Some(terms) = credit.terms {
            constraints = constraints.with_custom(
                "credit_payment_terms",
                ConstraintValue::from(payment_terms_str(terms)),
            );
        }
    }

    // Surplus allocations
    if let Some(surplus) = &econ.surplus {
        for rule in &surplus.allocation {
            let key = allocation_target_key(&rule.target);

            let expr = parse_expr(&rule.fraction)?;
            let val = ctx.eval_ctx.evaluate(&expr)?;
            let fraction = val.as_f64().ok_or_else(|| {
                SchemaError::Expression(format!(
                    "Surplus fraction '{}' evaluated to non-finite value",
                    rule.fraction
                ))
            })?;
            constraints = constraints.with_custom(
                format!("surplus_{}_pct", key),
                ConstraintValue::from(fraction),
            );

            // Opaque strings: stored verbatim, interpreted by CharterPolicyOracle
            if let Some(condition) = &rule.condition {
                constraints = constraints.with_custom(
                    format!("surplus_{}_condition", key),
                    ConstraintValue::from(condition.as_str()),
                );
            }
            if let Some(until) = &rule.until {
                constraints = constraints.with_custom(
                    format!("surplus_{}_until", key),
                    ConstraintValue::from(until.as_str()),
                );
            }
        }
    }

    Ok(constraints)
}

fn payment_terms_str(terms: PaymentTerms) -> &'static str {
    match terms {
        PaymentTerms::Net7 => "net7",
        PaymentTerms::Net15 => "net15",
        PaymentTerms::Net30 => "net30",
        PaymentTerms::Net60 => "net60",
        PaymentTerms::Net90 => "net90",
    }
}

fn allocation_target_key(target: &AllocationTarget) -> String {
    match target {
        AllocationTarget::Reserves => "reserves".to_string(),
        AllocationTarget::PatronageRefund => "patronage_refund".to_string(),
        AllocationTarget::CommunityFund => "community_fund".to_string(),
        AllocationTarget::WorkerBonus => "worker_bonus".to_string(),
        AllocationTarget::EducationFund => "education_fund".to_string(),
        AllocationTarget::IndivisibleReserves => "indivisible_reserves".to_string(),
        AllocationTarget::Custom(s) => s.clone(),
    }
}

// ─── Agreement ───────────────────────────────────────────────────────────────

fn apply_agreement(
    mut constraints: ConstraintSet,
    agreement: &AgreementSchema,
) -> Result<ConstraintSet, SchemaError> {
    // Settlement config
    if let Some(boundary) = &agreement.boundary {
        if let Some(transfers) = &boundary.transfers {
            if let Some(settlement) = &transfers.settlement {
                let cycle = match settlement.cycle {
                    SettlementCycle::Daily => "daily",
                    SettlementCycle::Weekly => "weekly",
                    SettlementCycle::Biweekly => "biweekly",
                    SettlementCycle::Monthly => "monthly",
                };
                let method = match settlement.method {
                    SettlementMethod::NetSettlement => "net_settlement",
                    SettlementMethod::GrossSettlement => "gross_settlement",
                    SettlementMethod::RealTime => "real_time",
                };
                constraints = constraints
                    .with_custom("settlement_cycle", ConstraintValue::from(cycle))
                    .with_custom("settlement_method", ConstraintValue::from(method));
            }
        }
    }

    // Dispute stages as an ordered list
    if let Some(disputes) = &agreement.disputes {
        let stages: Vec<ConstraintValue> = disputes
            .ladder
            .iter()
            .map(|stage| {
                let name = match stage.stage {
                    DisputeStageName::Negotiation => "negotiation",
                    DisputeStageName::Mediation => "mediation",
                    DisputeStageName::Arbitration => "arbitration",
                    DisputeStageName::Adjudication => "adjudication",
                };
                ConstraintValue::from(name)
            })
            .collect();
        constraints = constraints.with_custom("dispute_stages", ConstraintValue::List(stages));
    }

    // Exit notice period
    if let Some(exit) = &agreement.exit {
        if let Some(notice) = &exit.notice_period {
            if let Some(days) = notice.days {
                constraints =
                    constraints.with_custom("exit_notice_days", ConstraintValue::from(days as i64));
            }
        }
    }

    Ok(constraints)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::CclDocument;

    // ── Governance helpers ──

    fn simple_governance_yaml() -> &'static str {
        r#"schema_version: v0
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
  delegation:
    allowed: true
    transitive: false
"#
    }

    // ── Economics helpers ──

    fn simple_economics_yaml() -> &'static str {
        r#"schema_version: v0
economics:
  capital:
    member_equity:
      minimum: 100
      maximum: 5000
      interest_rate: 0.0
  credit:
    limit: "min(1000, patronage * 0.5)"
    terms: net30
  surplus:
    allocation:
      - target: reserves
        fraction: "0.20"
        until: "reserves >= 6 * monthly_operating"
      - target: patronage_refund
        fraction: "0.70"
"#
    }

    // ── Governance tests ──

    #[test]
    fn test_named_threshold_simple_majority() {
        let doc = CclDocument::from_yaml(simple_governance_yaml()).unwrap();
        let ctx = CharterContext::new().with_members(100);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        assert_eq!(
            cs.custom.get("min_votes_ordinary"),
            Some(&ConstraintValue::from(0.5f64))
        );
    }

    #[test]
    fn test_fraction_threshold() {
        let doc = CclDocument::from_yaml(simple_governance_yaml()).unwrap();
        let ctx = CharterContext::new().with_members(100);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        match cs.custom.get("min_votes_constitutional") {
            Some(ConstraintValue::Float(f)) => {
                assert!(
                    (**f - 2.0 / 3.0).abs() < 1e-9,
                    "Expected 0.667, got {}",
                    **f
                );
            }
            other => panic!("Expected Float, got {:?}", other),
        }
    }

    #[test]
    fn test_quorum_expression() {
        let doc = CclDocument::from_yaml(simple_governance_yaml()).unwrap();
        // "0.25 * members" with members=100 → 25.0
        let ctx = CharterContext::new().with_members(100);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        match cs.custom.get("min_quorum_ordinary") {
            Some(ConstraintValue::Float(f)) => {
                assert!((**f - 25.0).abs() < 1e-9, "Expected 25.0, got {}", **f);
            }
            other => panic!("Expected Float, got {:?}", other),
        }
    }

    #[test]
    fn test_body_seats_and_term() {
        let doc = CclDocument::from_yaml(simple_governance_yaml()).unwrap();
        // Board seats and term are literals — no field bindings needed for them.
        // Quorum expressions reference members, so bind it.
        let ctx = CharterContext::new().with_members(1);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        assert_eq!(
            cs.custom.get("body_board_seats"),
            Some(&ConstraintValue::Int(7))
        );
        assert_eq!(
            cs.custom.get("body_board_term_years"),
            Some(&ConstraintValue::Int(2))
        );
    }

    #[test]
    fn test_delegation_booleans() {
        let doc = CclDocument::from_yaml(simple_governance_yaml()).unwrap();
        // Quorum expressions reference members, so bind it.
        let ctx = CharterContext::new().with_members(1);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        assert_eq!(
            cs.custom.get("delegation_allowed"),
            Some(&ConstraintValue::Bool(true))
        );
        assert_eq!(
            cs.custom.get("delegation_transitive"),
            Some(&ConstraintValue::Bool(false))
        );
    }

    #[test]
    fn test_missing_optional_fields_produce_no_keys() {
        // Body without seats → no body_assembly_seats key.
        // Decision without quorum → no min_quorum_standard key.
        let yaml = r#"schema_version: v0
governance:
  bodies:
    - name: assembly
  decisions:
    - name: standard
      authority: assembly
      threshold: simple_majority
"#;
        let doc = CclDocument::from_yaml(yaml).unwrap();
        let ctx = CharterContext::new();
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        assert!(!cs.custom.contains_key("body_assembly_seats"));
        assert!(!cs.custom.contains_key("min_quorum_standard"));
    }

    #[test]
    fn test_expression_threshold_with_members() {
        let yaml = r#"schema_version: v0
governance:
  bodies:
    - name: assembly
  decisions:
    - name: supermajority_plus
      authority: assembly
      threshold: "0.75 * members"
"#;
        let doc = CclDocument::from_yaml(yaml).unwrap();
        // 0.75 * 200 = 150.0
        let ctx = CharterContext::new().with_members(200);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        match cs.custom.get("min_votes_supermajority_plus") {
            Some(ConstraintValue::Float(f)) => {
                assert!((**f - 150.0).abs() < 1e-9, "Expected 150.0, got {}", **f);
            }
            other => panic!("Expected Float, got {:?}", other),
        }
    }

    // ── Economics tests ──

    #[test]
    fn test_credit_limit_expression() {
        let doc = CclDocument::from_yaml(simple_economics_yaml()).unwrap();
        // min(1000, 800 * 0.5) = min(1000, 400) = 400
        let ctx = CharterContext::new().with_patronage(800.0);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        match cs.custom.get("credit_limit") {
            Some(ConstraintValue::Float(f)) => {
                assert!((**f - 400.0).abs() < 1e-9, "Expected 400.0, got {}", **f);
            }
            other => panic!("Expected Float, got {:?}", other),
        }
    }

    #[test]
    fn test_surplus_allocation_fractions() {
        let doc = CclDocument::from_yaml(simple_economics_yaml()).unwrap();
        // Surplus fractions are literals ("0.20", "0.70") — no field bindings needed.
        // "until" expression is stored opaquely, never evaluated here.
        let ctx = CharterContext::new().with_patronage(0.0);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        match cs.custom.get("surplus_reserves_pct") {
            Some(ConstraintValue::Float(f)) => {
                assert!((**f - 0.20).abs() < 1e-9, "Expected 0.20, got {}", **f);
            }
            other => panic!("Expected Float, got {:?}", other),
        }
        match cs.custom.get("surplus_patronage_refund_pct") {
            Some(ConstraintValue::Float(f)) => {
                assert!((**f - 0.70).abs() < 1e-9, "Expected 0.70, got {}", **f);
            }
            other => panic!("Expected Float, got {:?}", other),
        }
        // "until" stored verbatim for oracle interpretation
        assert!(cs.custom.contains_key("surplus_reserves_until"));
    }

    #[test]
    fn test_equity_literals() {
        let doc = CclDocument::from_yaml(simple_economics_yaml()).unwrap();
        let ctx = CharterContext::new().with_patronage(0.0);
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        assert_eq!(
            cs.custom.get("equity_min"),
            Some(&ConstraintValue::Int(100))
        );
        assert_eq!(
            cs.custom.get("equity_max"),
            Some(&ConstraintValue::Int(5000))
        );
    }

    // ── Agreement tests ──

    #[test]
    fn test_agreement_settlement_and_disputes() {
        let yaml = r#"schema_version: v0
agreement:
  name: "Test Compact"
  parties:
    - type: cooperative
      id: "did:icn:alpha"
    - type: cooperative
      id: "did:icn:beta"
  boundary:
    transfers:
      allowed: true
      settlement:
        cycle: weekly
        method: net_settlement
  disputes:
    ladder:
      - stage: negotiation
        duration:
          days: 30
      - stage: mediation
        binding: false
      - stage: arbitration
        binding: true
  exit:
    notice_period:
      days: 90
"#;
        let doc = CclDocument::from_yaml(yaml).unwrap();
        let ctx = CharterContext::new();
        let cs = charter_to_constraints(&doc, &ctx).unwrap();

        assert_eq!(
            cs.custom.get("settlement_cycle"),
            Some(&ConstraintValue::from("weekly"))
        );
        assert_eq!(
            cs.custom.get("settlement_method"),
            Some(&ConstraintValue::from("net_settlement"))
        );
        assert_eq!(
            cs.custom.get("exit_notice_days"),
            Some(&ConstraintValue::Int(90))
        );

        match cs.custom.get("dispute_stages") {
            Some(ConstraintValue::List(stages)) => {
                assert_eq!(stages.len(), 3);
                assert_eq!(stages[0], ConstraintValue::from("negotiation"));
                assert_eq!(stages[2], ConstraintValue::from("arbitration"));
            }
            other => panic!("Expected List, got {:?}", other),
        }
    }

    // ── Cross-section test ──

    #[test]
    fn test_no_sections_produces_empty_constraints() {
        let doc = CclDocument::from_yaml("schema_version: v0\n").unwrap();
        let ctx = CharterContext::new();
        let cs = charter_to_constraints(&doc, &ctx).unwrap();
        assert!(cs.custom.is_empty());
    }
}
