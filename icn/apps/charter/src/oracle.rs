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
    fn constraints_for_charter(doc: &CclDocument, ctx: &CharterContext) -> ConstraintSet {
        match charter_to_constraints(doc, ctx) {
            Ok(cs) => cs,
            Err(e) => {
                tracing::warn!(error = %e, "charter_to_constraints() failed; returning empty constraints");
                ConstraintSet::new()
            }
        }
    }
}

impl Default for CharterPolicyOracle {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyOracle for CharterPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Resolve the charter to use for this request.
        // Callers pass `charter_id` in request metadata.
        let charter_id = request.context.metadata.get("charter_id").cloned();

        let constraints = match charter_id {
            None => {
                // No charter specified — allow with empty constraints.
                tracing::debug!(
                    actor = %request.core.actor,
                    "No charter_id in request; returning empty constraints"
                );
                ConstraintSet::new()
            }
            Some(id) => {
                let guard = self.charters.read();
                match guard.get(&id) {
                    Some((doc, ctx)) => {
                        tracing::debug!(
                            actor = %request.core.actor,
                            charter_id = %id,
                            "Charter oracle evaluating constraints"
                        );
                        // Drop guard before the (potentially allocating) call.
                        let doc = doc.clone();
                        let ctx_clone = ctx.clone();
                        drop(guard);
                        Self::constraints_for_charter(&doc, &ctx_clone)
                    }
                    None => {
                        tracing::warn!(
                            actor = %request.core.actor,
                            charter_id = %id,
                            "Unknown charter_id; returning empty constraints"
                        );
                        ConstraintSet::new()
                    }
                }
            }
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        PolicyDecision::allow_with_provenance(constraints, "charter", now)
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
    fn test_unknown_charter_id_returns_allow_empty() {
        let oracle = CharterPolicyOracle::new();
        let decision = oracle.evaluate(&make_request(Some("nonexistent")));

        assert!(decision.is_allowed());
        let cs = decision.constraints().unwrap();
        assert!(
            cs.custom.is_empty(),
            "Unknown charter → empty ConstraintSet"
        );
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

        // After removal: empty constraints
        let cs = oracle
            .evaluate(&make_request(Some("coop-gamma")))
            .constraints()
            .unwrap()
            .clone();
        assert!(cs.custom.is_empty());
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
}
