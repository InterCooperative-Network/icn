//! Charter App
//!
//! Converts YAML charter documents into kernel-enforced constraints.
//!
//! # Architecture
//!
//! ```text
//! Kernel                    Charter App                   CCL Schema
//! ------                    -----------                   ----------
//! PolicyRequest  ────────>  CharterPolicyOracle  ──────>  charter_to_constraints()
//!                                    │
//!                                    │ constraints_for_charter()
//!                                    │ (MEANING FIREWALL BOUNDARY)
//!                                    v
//! ConstraintSet  <────────  PolicyDecision
//! ```
//!
//! The kernel calls `PolicyOracle::evaluate()` and receives a `PolicyDecision`
//! with a `ConstraintSet`.  It enforces these constraints without knowing they
//! came from a cooperative charter.

pub mod oracle;

pub use oracle::CharterPolicyOracle;

use icn_ccl::schema::{CclDocument, CharterContext};
use icn_kernel_api::authz::PolicyOracle;
use std::sync::Arc;

/// Create a new CharterPolicyOracle instance.
///
/// The oracle starts empty.  Use `CharterPolicyOracle::deploy_charter()` to
/// register active charters before routing requests through it.
pub fn create_oracle() -> Arc<dyn PolicyOracle> {
    Arc::new(CharterPolicyOracle::new())
}

/// Create a CharterPolicyOracle pre-loaded with a single charter.
///
/// Convenience wrapper for the common single-cooperative use case.
pub fn create_oracle_with_charter(
    charter_id: String,
    doc: CclDocument,
    ctx: CharterContext,
) -> Arc<CharterPolicyOracle> {
    let oracle = CharterPolicyOracle::new();
    oracle.deploy_charter(charter_id, doc, ctx);
    Arc::new(oracle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_ccl::schema::CclDocument;

    const MINIMAL_CHARTER: &str = "schema_version: v0\n";

    #[test]
    fn test_create_oracle_is_empty() {
        let oracle = create_oracle();
        assert_eq!(oracle.domain().as_str(), "charter");
    }

    #[test]
    fn test_create_oracle_with_charter() {
        let doc = CclDocument::from_yaml(MINIMAL_CHARTER).unwrap();
        let ctx = CharterContext::new();
        let oracle = create_oracle_with_charter("test-coop".to_string(), doc, ctx);
        assert_eq!(oracle.deployed_count(), 1);
    }
}
