//! Charter Policy Oracle
//!
//! Implements the PolicyOracle trait, converting charter documents into
//! kernel-enforceable constraints.
//!
//! # The Meaning Firewall
//!
//! Everything above `charter_to_constraints()` is charter semantics:
//! governance bodies, decision thresholds, surplus allocation rules.
//!
//! Everything below is generic kernel constraints:
//! opaque key-value pairs the kernel enforces without interpretation.
//!
//! The kernel never knows:
//! - What a "quorum" is
//! - What "surplus_patronage_refund_pct" means
//! - How governance thresholds were derived
//!
//! It only knows:
//! - Custom constraint values to check
//! - Whether to allow or deny the request

use icn_ccl::schema::{bridge::charter_to_constraints, CclDocument, CharterContext};
use icn_kernel_api::authz::{ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Charter app's PolicyOracle implementation.
///
/// Holds a registry of deployed charter documents, keyed by charter ID
/// (typically the cooperative DID or a human-readable name).  When `evaluate()`
/// is called, it looks up the charter referenced by `charter_id` in the request
/// metadata, runs `charter_to_constraints()`, and returns the resulting
/// `ConstraintSet` to the kernel.
///
/// If no `charter_id` is provided, or the referenced charter is not deployed,
/// the oracle returns an empty `ConstraintSet` (permissive allow).
///
/// # Thread Safety
///
/// Uses `parking_lot::RwLock` because `PolicyOracle::evaluate()` is
/// synchronous.  Charter deployments are rare relative to evaluations, so
/// read-heavy RwLock is appropriate.
pub struct CharterPolicyOracle {
    /// Active charters: charter_id → (document, runtime context)
    charters: Arc<RwLock<HashMap<String, (CclDocument, CharterContext)>>>,
}

impl CharterPolicyOracle {
    /// Create a new, empty CharterPolicyOracle.
    pub fn new() -> Self {
        Self {
            charters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Deploy (or replace) a charter.
    ///
    /// After this call, `evaluate()` requests that reference `charter_id`
    /// will use `doc` and `ctx` to produce constraints.
    ///
    /// # Arguments
    /// * `charter_id` — Stable identifier (e.g., cooperative DID or name).
    /// * `doc`        — Parsed CCL document.
    /// * `ctx`        — Runtime bindings (member count, balances, etc.).
    pub fn deploy_charter(&self, charter_id: String, doc: CclDocument, ctx: CharterContext) {
        self.charters.write().insert(charter_id, (doc, ctx));
    }

    /// Update the runtime context for an already-deployed charter.
    ///
    /// Use this when context values change (e.g., member count after an
    /// election) without needing to re-parse the charter document.
    ///
    /// Returns `true` if the charter was found and updated, `false` otherwise.
    pub fn update_context(&self, charter_id: &str, ctx: CharterContext) -> bool {
        let mut guard = self.charters.write();
        if let Some(entry) = guard.get_mut(charter_id) {
            entry.1 = ctx;
            true
        } else {
            false
        }
    }

    /// Remove a deployed charter.
    ///
    /// After removal, requests referencing this charter receive empty constraints.
    pub fn remove_charter(&self, charter_id: &str) -> bool {
        self.charters.write().remove(charter_id).is_some()
    }

    /// Return the number of currently deployed charters.
    pub fn deployed_count(&self) -> usize {
        self.charters.read().len()
    }

    /// Query charter-derived thresholds for a governance decision type.
    ///
    /// This is the **intended consumption interface** for governance vote
    /// evaluation.  The governance actor calls this to determine the
    /// approval ratio and quorum count mandated by the cooperative's charter
    /// for a named decision type (e.g., `"ordinary"`, `"constitutional"`).
    ///
    /// # Parameters
    /// - `charter_id`: the cooperative's charter identifier (usually its DID)
    /// - `decision_type`: the decision name as it appears in the charter YAML
    ///   (`governance.decisions[].name`)
    /// - `member_count`: current cooperative member count, used to evaluate
    ///   quorum expressions like `"0.25 * members"` with live data
    ///
    /// # Returns
    ///
    /// `Some((approval_ratio, quorum_count))` when the charter defines
    /// thresholds for `decision_type`:
    /// - `approval_ratio`: fractional threshold (0.0–1.0); e.g., 0.67
    /// - `quorum_count`: absolute member count required for quorum; e.g., 25.0
    ///   (already multiplied by member count if the charter used an expression)
    ///
    /// `None` when the charter is not deployed, does not define `decision_type`,
    /// or translation fails.
    ///
    /// # Intended consumer
    ///
    /// `GovernanceActor::get_thresholds_from_charter()` — called during
    /// `CloseProposal` after `get_thresholds_from_params()` returns `None`,
    /// before falling back to `domain.config.thresholds_for_proposal()`.
    pub fn thresholds_for(
        &self,
        charter_id: &str,
        decision_type: &str,
        member_count: u64,
    ) -> Option<(f64, f64)> {
        use icn_kernel_api::authz::ConstraintValue;

        let guard = self.charters.read();
        let (doc, ctx) = guard.get(charter_id)?;
        let doc = doc.clone();
        let ctx_live = ctx.clone().with_members(member_count);
        drop(guard);

        let cs = charter_to_constraints(&doc, &ctx_live).ok()?;

        // Extract approval ratio (min_votes_{decision_type})
        let approval_ratio = match cs.custom.get(&format!("min_votes_{decision_type}"))? {
            ConstraintValue::Float(f) => **f,
            ConstraintValue::Int(i) => *i as f64,
            _ => return None,
        };

        // Extract quorum count (min_quorum_{decision_type}), defaulting to 0
        let quorum_count = match cs.custom.get(&format!("min_quorum_{decision_type}")) {
            Some(ConstraintValue::Float(f)) => **f,
            Some(ConstraintValue::Int(i)) => *i as f64,
            _ => 0.0,
        };

        Some((approval_ratio, quorum_count))
    }

    /// Query the charter-derived credit limit for a member.
    ///
    /// This is the **intended consumption interface** for the ledger's credit
    /// enforcement path. The ledger calls this from `process_entry()` to
    /// determine the institutionally ratified credit cap for a specific
    /// member, given that member's runtime patronage and trust score.
    ///
    /// # Parameters
    /// - `charter_id`: the cooperative's charter identifier (usually its DID)
    /// - `member_patronage`: the per-member patronage value bound into the
    ///   `patronage` variable when the charter expression is evaluated.
    ///   Callers commonly pass `cleared_volume` as a proxy.
    /// - `member_trust_score`: the per-member trust score (clamped 0.0–1.0)
    ///   bound into the `trust_score` variable.
    ///
    /// # Returns
    /// `Some(limit)` when the charter is deployed, defines a `credit_limit`
    /// expression, and the expression evaluates to a finite value. The value
    /// is truncated to `i64` for ledger-side comparison against balance.
    ///
    /// `None` when the charter is not deployed, the charter has no
    /// `economics.credit.limit` expression, or evaluation fails. The caller
    /// must fall back to its default credit policy chain.
    ///
    /// # Per-member context binding
    ///
    /// This method **clones the charter's frozen context** and overlays the
    /// per-member `patronage` and `trust_score` bindings before evaluating
    /// `charter_to_constraints()`. The frozen context (member count, etc.)
    /// is preserved; only per-member fields are overridden.
    pub fn credit_limit_for(
        &self,
        charter_id: &str,
        member_patronage: f64,
        member_trust_score: f64,
    ) -> Option<i64> {
        use icn_kernel_api::authz::ConstraintValue;

        let guard = self.charters.read();
        let (doc, ctx) = guard.get(charter_id)?;
        let doc = doc.clone();
        // Overlay per-member bindings on the frozen deploy-time context.
        let ctx_member = ctx
            .clone()
            .with_patronage(member_patronage)
            .with_trust_score(member_trust_score.clamp(0.0, 1.0));
        drop(guard);

        let cs = charter_to_constraints(&doc, &ctx_member).ok()?;

        match cs.custom.get("credit_limit")? {
            ConstraintValue::Float(f) => Some(**f as i64),
            ConstraintValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Convert a charter document + context into kernel-enforceable constraints.
    ///
    /// # THIS IS THE MEANING FIREWALL BOUNDARY
    ///
    /// Above this call: charter semantics (governance bodies, thresholds,
    /// surplus allocation, credit eligibility).
    ///
    /// Below this call: generic kernel constraints (opaque key-value pairs).
    ///
    /// The kernel cannot determine WHY these constraint values were chosen.
    ///
    /// Returns `None` if translation fails — callers must treat this as Deny,
    /// not Allow. A malformed charter must not silently grant unconstrained access.
    fn constraints_for_charter(doc: &CclDocument, ctx: &CharterContext) -> Option<ConstraintSet> {
        match charter_to_constraints(doc, ctx) {
            Ok(cs) => Some(cs),
            Err(e) => {
                tracing::warn!(error = %e, "charter_to_constraints() failed; denying request");
                None
            }
        }
    }
}

impl Default for CharterPolicyOracle {
    fn default() -> Self {
        Self::new()
    }
}

/// Implementation of the ledger's `SurplusPolicyView` typed adapter.
///
/// This is the **CCL → surplus distribution enforcement bridge**. The ledger
/// does not import any charter or CCL types — it only sees this trait. The
/// implementation reads the charter's `surplus_reserves_pct` constraint and
/// returns it as an `f64` for the ledger to enforce at distribution time.
///
/// Direction of dependency: `apps/charter` depends on `icn-ledger` for the
/// trait. The ledger never depends on `apps/charter`. Meaning firewall holds.
impl icn_ledger::credit_policy::SurplusPolicyView for CharterPolicyOracle {
    fn reserves_pct_for(&self, charter_id: &str) -> Option<f64> {
        use icn_kernel_api::authz::ConstraintValue;

        let guard = self.charters.read();
        let (doc, ctx) = guard.get(charter_id)?;
        let doc = doc.clone();
        // Surplus fractions are literal values — no per-member context needed.
        // Use the frozen deploy-time context; it is sufficient for evaluating
        // the constant `surplus.allocation[target=reserves].fraction` field.
        let ctx_clone = ctx.clone();
        drop(guard);

        let cs = icn_ccl::schema::bridge::charter_to_constraints(&doc, &ctx_clone).ok()?;

        match cs.custom.get("surplus_reserves_pct")? {
            ConstraintValue::Float(f) => {
                let pct = (**f).clamp(0.0, 1.0);
                Some(pct)
            }
            ConstraintValue::Int(i) => Some((*i as f64).clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

/// Implementation of the ledger's `EconomicPolicyView` typed adapter.
///
/// This is the **CCL → economics consumption bridge**. The ledger does not
/// import any charter or CCL types — it only sees this trait. The
/// implementation translates the ledger's per-member economic inputs into a
/// charter context, evaluates `charter_to_constraints()`, and reads the
/// resulting `credit_limit` value.
///
/// Direction of dependency: `apps/charter` (this crate) depends on
/// `icn-ledger` for the trait definition. The ledger never depends on
/// `apps/charter`. The meaning firewall is preserved.
impl icn_ledger::credit_policy::EconomicPolicyView for CharterPolicyOracle {
    fn credit_limit_for(
        &self,
        charter_id: &str,
        member_patronage: f64,
        member_trust_score: f64,
    ) -> Option<i64> {
        // Delegate to the inherent method — the trait impl is just a thin
        // pass-through so the inherent method remains usable from inside the
        // crate without trait-import gymnastics.
        CharterPolicyOracle::credit_limit_for(
            self,
            charter_id,
            member_patronage,
            member_trust_score,
        )
    }
}

impl PolicyOracle for CharterPolicyOracle {
    /// Evaluate charter constraints for a request.
    ///
    /// # Request metadata contract
    ///
    /// Callers may pass the following metadata keys in `PolicyRequest.context`:
    ///
    /// - `"charter_id"` — required: which charter to evaluate against.  If
    ///   absent, the oracle abstains (returns empty constraints).  If present
    ///   but not deployed, the oracle fails closed (denies).
    ///
    /// - `"member_count"` — optional: current cooperative member count as a
    ///   decimal string (`"42"`).  When provided, the runtime context is
    ///   updated before evaluation so threshold expressions like
    ///   `"0.25 * members"` use the live count rather than the frozen value
    ///   that was set at charter deployment time.
    ///
    /// # Hardcoded member count gap
    ///
    /// If `"member_count"` is NOT in the request metadata, the oracle falls
    /// back to the `CharterContext` that was passed to `deploy_charter()`.
    /// The daemon's ratification hook currently sets this to a fixed value
    /// (`with_members(100)`), which means quorum expressions may be stale.
    /// Callers that know the real member count SHOULD pass it here.
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Resolve the charter to use for this request.
        let charter_id = request.context.metadata.get("charter_id").cloned();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Optional live member count from request metadata.
        // When present, override the frozen context so threshold expressions
        // that reference `members` evaluate correctly.
        let live_member_count: Option<u64> = request
            .context
            .metadata
            .get("member_count")
            .and_then(|s| s.parse().ok());

        match charter_id {
            None => {
                // No charter_id in request — operation doesn't reference a charter.
                // Allow with empty constraints; charter policy is not applicable here.
                tracing::debug!(
                    actor = %request.core.actor,
                    "No charter_id in request; charter oracle abstaining"
                );
                PolicyDecision::allow_with_provenance(ConstraintSet::new(), "charter", now)
            }
            Some(id) => {
                let guard = self.charters.read();
                match guard.get(&id) {
                    Some((doc, ctx)) => {
                        tracing::debug!(
                            actor = %request.core.actor,
                            charter_id = %id,
                            live_member_count = ?live_member_count,
                            "Charter oracle evaluating constraints"
                        );
                        // Clone before dropping guard; apply live member count if provided.
                        let doc = doc.clone();
                        let ctx_live = match live_member_count {
                            Some(count) => ctx.clone().with_members(count),
                            None => ctx.clone(),
                        };
                        drop(guard);
                        match Self::constraints_for_charter(&doc, &ctx_live) {
                            Some(cs) => PolicyDecision::allow_with_provenance(cs, "charter", now),
                            None => {
                                // Translation failed — fail closed.
                                PolicyDecision::deny("charter_translation_failed")
                            }
                        }
                    }
                    None => {
                        // Charter explicitly requested but not deployed — fail closed.
                        tracing::warn!(
                            actor = %request.core.actor,
                            charter_id = %id,
                            "Unknown charter_id; denying request"
                        );
                        PolicyDecision::deny("charter_not_deployed")
                    }
                }
            }
        }
    }

    fn domain(&self) -> Domain {
        Domain::new("charter")
    }

    fn cache_ttl(&self) -> Duration {
        // Charter constraints are stable between deployments.
        // 60s TTL is safe — deployments invalidate the cache externally.
        Duration::from_secs(60)
    }

    fn handles_cross_org(&self) -> bool {
        // Each cooperative runs its own charter oracle instance.
        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use icn_ccl::schema::CclDocument;
    use icn_kernel_api::authz::{ActionKind, PolicyRequest};

    const MINIMAL_CHARTER: &str = r#"schema_version: v0
governance:
  decisions:
    - name: ordinary
      authority: general_assembly
      threshold: simple_majority
      quorum: "0.25 * members"
"#;

    fn make_request(charter_id: Option<&str>) -> PolicyRequest {
        let core = icn_kernel_api::authz::PolicyRequestCore::new(
            "did:icn:test-actor".to_string(),
            ActionKind::Write,
            Domain::new("charter"),
        );
        match charter_id {
            Some(id) => {
                let ctx =
                    icn_kernel_api::authz::PolicyContext::new().with_metadata("charter_id", id);
                PolicyRequest::with_context(core, ctx)
            }
            None => PolicyRequest::with_context(core, icn_kernel_api::authz::PolicyContext::new()),
        }
    }

    #[test]
    fn test_no_charter_id_returns_allow_empty() {
        let oracle = CharterPolicyOracle::new();
        let decision = oracle.evaluate(&make_request(None));

        assert!(decision.is_allowed());
        let cs = decision.constraints().unwrap();
        assert!(cs.custom.is_empty(), "No charter → empty ConstraintSet");
    }

    #[test]
    fn test_unknown_charter_id_returns_deny() {
        let oracle = CharterPolicyOracle::new();
        let decision = oracle.evaluate(&make_request(Some("nonexistent")));

        // Fail closed: an explicitly-referenced charter that isn't deployed must deny,
        // not silently allow with empty constraints.
        assert!(!decision.is_allowed(), "Unknown charter_id must deny");
    }

    #[test]
    fn test_deploy_and_evaluate_produces_constraints() {
        let oracle = CharterPolicyOracle::new();

        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        let ctx = CharterContext::new().with_members(100);
        oracle.deploy_charter("coop-alpha".to_string(), doc, ctx);

        let decision = oracle.evaluate(&make_request(Some("coop-alpha")));
        assert!(decision.is_allowed());

        let cs = decision.constraints().unwrap();
        assert!(
            cs.custom.contains_key("min_votes_ordinary"),
            "Deployed charter must produce constraints"
        );
    }

    #[test]
    fn test_update_context_changes_constraints() {
        let oracle = CharterPolicyOracle::new();

        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        let ctx = CharterContext::new().with_members(100);
        oracle.deploy_charter("coop-beta".to_string(), doc, ctx);

        // Evaluate with 100 members → quorum = 0.25 * 100 = 25.0
        let cs1 = oracle
            .evaluate(&make_request(Some("coop-beta")))
            .constraints()
            .unwrap()
            .clone();

        // Update to 200 members → quorum = 0.25 * 200 = 50.0
        let updated_ctx = CharterContext::new().with_members(200);
        let updated = oracle.update_context("coop-beta", updated_ctx);
        assert!(updated, "update_context must return true for known charter");

        let cs2 = oracle
            .evaluate(&make_request(Some("coop-beta")))
            .constraints()
            .unwrap()
            .clone();

        assert_ne!(
            cs1.custom.get("min_quorum_ordinary"),
            cs2.custom.get("min_quorum_ordinary"),
            "Quorum must change after context update"
        );
    }

    #[test]
    fn test_update_context_unknown_charter_returns_false() {
        let oracle = CharterPolicyOracle::new();
        let ctx = CharterContext::new().with_members(50);
        assert!(!oracle.update_context("does-not-exist", ctx));
    }

    #[test]
    fn test_remove_charter_stops_producing_constraints() {
        let oracle = CharterPolicyOracle::new();

        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        let ctx = CharterContext::new().with_members(100);
        oracle.deploy_charter("coop-gamma".to_string(), doc, ctx);

        assert_eq!(oracle.deployed_count(), 1);

        let removed = oracle.remove_charter("coop-gamma");
        assert!(removed);
        assert_eq!(oracle.deployed_count(), 0);

        // After removal: deny (charter no longer deployed — fail closed)
        let decision = oracle.evaluate(&make_request(Some("coop-gamma")));
        assert!(
            !decision.is_allowed(),
            "Removed charter must deny, not allow"
        );
    }

    #[test]
    fn test_oracle_domain_is_charter() {
        let oracle = CharterPolicyOracle::new();
        assert_eq!(oracle.domain().as_str(), "charter");
    }

    #[test]
    fn test_cache_ttl_is_60_seconds() {
        let oracle = CharterPolicyOracle::new();
        assert_eq!(oracle.cache_ttl(), Duration::from_secs(60));
    }

    #[test]
    fn test_multiple_charters_are_independent() {
        let oracle = CharterPolicyOracle::new();

        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        oracle.deploy_charter(
            "coop-a".to_string(),
            doc.clone(),
            CharterContext::new().with_members(100),
        );
        oracle.deploy_charter(
            "coop-b".to_string(),
            doc,
            CharterContext::new().with_members(200),
        );

        let cs_a = oracle
            .evaluate(&make_request(Some("coop-a")))
            .constraints()
            .unwrap()
            .clone();
        let cs_b = oracle
            .evaluate(&make_request(Some("coop-b")))
            .constraints()
            .unwrap()
            .clone();

        // Quorums differ because member counts differ
        assert_ne!(
            cs_a.custom.get("min_quorum_ordinary"),
            cs_b.custom.get("min_quorum_ordinary"),
            "Each charter must be evaluated independently"
        );
    }

    /// Passing `member_count` in request metadata overrides the frozen context.
    /// This closes the hardcoded-member-count gap for callers that know the real count.
    #[test]
    fn test_live_member_count_from_metadata_overrides_frozen_context() {
        let oracle = CharterPolicyOracle::new();

        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        // Deploy with frozen count of 100 (simulating the daemon's charter_accepted_hook).
        let ctx = CharterContext::new().with_members(100);
        oracle.deploy_charter("coop-live".to_string(), doc, ctx);

        // Request without member_count: uses frozen 100 → quorum = 0.25 * 100 = 25.0
        let cs_frozen = oracle
            .evaluate(&make_request(Some("coop-live")))
            .constraints()
            .unwrap()
            .clone();

        // Request with member_count=400: uses live 400 → quorum = 0.25 * 400 = 100.0
        let core = icn_kernel_api::authz::PolicyRequestCore::new(
            "did:icn:test-actor".to_string(),
            icn_kernel_api::authz::ActionKind::Write,
            Domain::new("charter"),
        );
        let ctx_with_count = icn_kernel_api::authz::PolicyContext::new()
            .with_metadata("charter_id", "coop-live")
            .with_metadata("member_count", "400");
        let request_live = PolicyRequest::with_context(core, ctx_with_count);

        let cs_live = oracle
            .evaluate(&request_live)
            .constraints()
            .unwrap()
            .clone();

        // Frozen context: quorum = 25.0
        match cs_frozen.custom.get("min_quorum_ordinary") {
            Some(icn_kernel_api::authz::ConstraintValue::Float(f)) => {
                assert!(
                    (**f - 25.0).abs() < 1e-9,
                    "Frozen quorum should be 25.0, got {f}"
                );
            }
            other => panic!("Expected Float, got {other:?}"),
        }

        // Live context: quorum = 100.0 (different from frozen)
        match cs_live.custom.get("min_quorum_ordinary") {
            Some(icn_kernel_api::authz::ConstraintValue::Float(f)) => {
                assert!(
                    (**f - 100.0).abs() < 1e-9,
                    "Live quorum should be 100.0, got {f}"
                );
            }
            other => panic!("Expected Float, got {other:?}"),
        }

        // The quorums must differ — live count must not silently use frozen context.
        assert_ne!(
            cs_frozen.custom.get("min_quorum_ordinary"),
            cs_live.custom.get("min_quorum_ordinary"),
            "member_count in metadata must override the frozen context"
        );
    }

    /// thresholds_for() returns charter-derived approval ratio and quorum count.
    /// This is the intended interface for the governance actor.
    #[test]
    fn test_thresholds_for_returns_charter_derived_values() {
        let oracle = CharterPolicyOracle::new();

        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        oracle.deploy_charter(
            "coop-thresh".to_string(),
            doc,
            CharterContext::new().with_members(50),
        );

        // decision_type="ordinary": threshold=simple_majority(0.5), quorum="0.25*members"
        let result = oracle.thresholds_for("coop-thresh", "ordinary", 80);
        assert!(
            result.is_some(),
            "Should return thresholds for 'ordinary' decision type"
        );

        let (approval, quorum) = result.unwrap();
        // approval_ratio = 0.5 (simple majority)
        assert!(
            (approval - 0.5).abs() < 1e-9,
            "Approval ratio should be 0.5, got {approval}"
        );
        // quorum_count = 0.25 * 80 = 20.0 (live member count overrides frozen 50)
        assert!(
            (quorum - 20.0).abs() < 1e-9,
            "Quorum count should be 20.0 (0.25*80), got {quorum}"
        );
    }

    /// credit_limit_for() returns charter-derived credit limit, evaluated
    /// in a per-member context (patronage + trust score).
    #[test]
    fn test_credit_limit_for_returns_charter_value() {
        const ECON_CHARTER: &str = r#"schema_version: v0
economics:
  credit:
    limit: "min(1000, patronage * 0.5 * trust_score)"
"#;

        let oracle = CharterPolicyOracle::new();
        let doc = CclDocument::from_yaml(ECON_CHARTER).unwrap();
        // Frozen context — irrelevant once credit_limit_for binds per-member values
        oracle.deploy_charter("coop-econ".to_string(), doc, CharterContext::new());

        // patronage=400, trust_score=1.0 → min(1000, 400*0.5*1.0) = min(1000, 200) = 200
        let limit = oracle.credit_limit_for("coop-econ", 400.0, 1.0);
        assert_eq!(limit, Some(200), "Expected 200, got {limit:?}");

        // patronage=10000, trust_score=0.5 → min(1000, 10000*0.5*0.5) = min(1000, 2500) = 1000
        let limit = oracle.credit_limit_for("coop-econ", 10000.0, 0.5);
        assert_eq!(limit, Some(1000), "Expected 1000 (capped), got {limit:?}");

        // patronage=0 → min(1000, 0) = 0
        let limit = oracle.credit_limit_for("coop-econ", 0.0, 1.0);
        assert_eq!(limit, Some(0), "Expected 0 (no patronage), got {limit:?}");
    }

    #[test]
    fn test_credit_limit_for_unknown_charter_returns_none() {
        let oracle = CharterPolicyOracle::new();
        assert!(oracle
            .credit_limit_for("does-not-exist", 100.0, 0.5)
            .is_none());
    }

    #[test]
    fn test_credit_limit_for_charter_without_credit_policy_returns_none() {
        // MINIMAL_CHARTER has only governance, no economics.credit
        let oracle = CharterPolicyOracle::new();
        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        oracle.deploy_charter(
            "coop-no-credit".to_string(),
            doc,
            CharterContext::new().with_members(10),
        );
        assert!(oracle
            .credit_limit_for("coop-no-credit", 100.0, 0.5)
            .is_none());
    }

    #[test]
    fn test_thresholds_for_unknown_charter_returns_none() {
        let oracle = CharterPolicyOracle::new();
        assert!(oracle
            .thresholds_for("does-not-exist", "ordinary", 100)
            .is_none());
    }

    #[test]
    fn test_thresholds_for_unknown_decision_type_returns_none() {
        let oracle = CharterPolicyOracle::new();
        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        oracle.deploy_charter(
            "coop-x".to_string(),
            doc,
            CharterContext::new().with_members(10),
        );
        // "constitutional" is not in MINIMAL_CHARTER
        assert!(oracle
            .thresholds_for("coop-x", "constitutional", 10)
            .is_none());
    }

    // ── SurplusPolicyView tests ───────────────────────────────────────────────

    const SURPLUS_CHARTER: &str = r#"schema_version: v0
economics:
  surplus:
    allocation:
      - target: reserves
        fraction: "0.20"
      - target: patronage_refund
        fraction: "0.70"
"#;

    /// reserves_pct_for() returns the correct fraction from the charter.
    #[test]
    fn test_reserves_pct_for_returns_charter_value() {
        use icn_ledger::credit_policy::SurplusPolicyView;

        let oracle = CharterPolicyOracle::new();
        let doc = CclDocument::from_yaml(SURPLUS_CHARTER).unwrap();
        oracle.deploy_charter("coop-surplus".to_string(), doc, CharterContext::new());

        let pct = oracle.reserves_pct_for("coop-surplus");
        assert!(pct.is_some(), "Should return Some(0.20)");
        let pct = pct.unwrap();
        assert!(
            (pct - 0.20).abs() < 1e-9,
            "reserves_pct should be 0.20, got {pct}"
        );
    }

    /// reserves_pct_for() returns None when charter has no surplus allocation.
    #[test]
    fn test_reserves_pct_for_charter_without_surplus_returns_none() {
        use icn_ledger::credit_policy::SurplusPolicyView;

        // MINIMAL_CHARTER has only governance, no economics.surplus
        let oracle = CharterPolicyOracle::new();
        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        oracle.deploy_charter(
            "coop-no-surplus".to_string(),
            doc,
            CharterContext::new().with_members(10),
        );
        assert!(
            oracle.reserves_pct_for("coop-no-surplus").is_none(),
            "Charter with no surplus section should return None"
        );
    }

    /// reserves_pct_for() returns None for unknown charter.
    #[test]
    fn test_reserves_pct_for_unknown_charter_returns_none() {
        use icn_ledger::credit_policy::SurplusPolicyView;

        let oracle = CharterPolicyOracle::new();
        assert!(
            oracle.reserves_pct_for("does-not-exist").is_none(),
            "Unknown charter should return None"
        );
    }
}
