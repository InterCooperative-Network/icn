//! Invariant Gate — frozen-core predicate enforcement.
//!
//! Runs in two contexts with the same interface:
//! 1. Governance interceptor (Draft → DraftWithArtifacts) — early rejection
//! 2. Kernel write-path (Ratified → Activated) — final rejection
//!
//! Same code. Same hashes. Same deterministic outcome.

use crate::effect_manifest::*;
use crate::power_diff::PowerDiff;
use icn_kernel_api::invariants::*;

/// Read-only state view for invariant checking.
/// Implemented by kernel state store (authoritative) and governance simulator (preview).
pub trait InvariantStateView: Send + Sync {
    /// Minimum timelock delay in blocks for an entity.
    fn current_min_timelock_blocks(&self, entity_id: &str) -> u64;
}

/// Context passed to each invariant. Deterministic: no wall-clock, no network calls.
pub struct InvariantContext<'a> {
    pub current_block: BlockHeight,
    pub manifest: &'a EffectManifest,
    pub power_diff: Option<&'a PowerDiff>,
    pub state: &'a dyn InvariantStateView,
}

/// Individual invariant predicate.
pub trait Invariant: Send + Sync {
    fn id(&self) -> InvariantId;
    fn validate(&self, ctx: &InvariantContext) -> Result<(), InvariantViolation>;
}

/// The gate — runs all registered invariants, produces a report.
pub struct InvariantGate {
    invariants: Vec<Box<dyn Invariant>>,
}

impl InvariantGate {
    pub fn new(invariants: Vec<Box<dyn Invariant>>) -> Self {
        Self { invariants }
    }

    /// Build a gate with the default frozen-core invariants (#4, #5, #6).
    /// More invariants will be added as they are implemented.
    pub fn default_frozen_core() -> Self {
        Self::new(vec![
            Box::new(NoSilentPowerChanges),
            Box::new(BoundedAuthority),
            Box::new(MandatoryExecutionDelay),
        ])
    }

    /// Run all invariants and produce a deterministic report.
    pub fn evaluate(&self, ctx: &InvariantContext) -> InvariantReport {
        let violations: Vec<InvariantViolation> = self
            .invariants
            .iter()
            .filter_map(|inv| inv.validate(ctx).err())
            .collect();
        InvariantReport::new(
            ctx.manifest.manifest_hash,
            ctx.power_diff.map(|pd| pd.diff_hash),
            violations,
        )
    }
}

// =============================================================================
// Invariant #4: No Silent Power Changes
// =============================================================================

/// Any authz perimeter change must include a PowerDiff with a non-empty
/// capability delta. If the manifest touches authz but the diff is missing
/// or empty, reject.
pub struct NoSilentPowerChanges;

impl Invariant for NoSilentPowerChanges {
    fn id(&self) -> InvariantId {
        InvariantId::NoSilentPowerChanges
    }

    fn validate(&self, ctx: &InvariantContext) -> Result<(), InvariantViolation> {
        if !ctx.manifest.touches_authz() {
            return Ok(());
        }

        let pd = ctx.power_diff.ok_or_else(|| InvariantViolation {
            id: self.id(),
            domain: self.id().domain(),
            message: "Authz perimeter change requires PowerDiff".into(),
            details: vec![],
        })?;

        // Require at least one subject delta when authz is touched
        if pd.subject_deltas.is_empty() {
            return Err(InvariantViolation {
                id: self.id(),
                domain: self.id().domain(),
                message: "PowerDiff present but has no subject deltas for authz change".into(),
                details: vec![],
            });
        }

        Ok(())
    }
}

// =============================================================================
// Invariant #5: Bounded Authority (No God Mode)
// =============================================================================

/// Rejects capability assignments with wildcard scope/resource.
pub struct BoundedAuthority;

impl Invariant for BoundedAuthority {
    fn id(&self) -> InvariantId {
        InvariantId::BoundedAuthority
    }

    fn validate(&self, ctx: &InvariantContext) -> Result<(), InvariantViolation> {
        for effect in &ctx.manifest.capability_effects {
            let (scope, resource) = match effect {
                CapabilityEffect::Grant {
                    scope, resource, ..
                } => (scope, resource),
                CapabilityEffect::Revoke {
                    scope, resource, ..
                } => (scope, resource),
            };
            if scope == "*" || resource == "*" {
                return Err(InvariantViolation {
                    id: self.id(),
                    domain: self.id().domain(),
                    message: "Wildcard '*' is not a valid scope/resource".into(),
                    details: vec![
                        ("scope".into(), scope.clone()),
                        ("resource".into(), resource.clone()),
                    ],
                });
            }
        }
        Ok(())
    }
}

// =============================================================================
// Invariant #6: Mandatory Execution Delay (Timelock)
// =============================================================================

/// Economic/capability changes require a minimum delay between ratification
/// and activation. Prevents flash-governance attacks.
pub struct MandatoryExecutionDelay;

/// Minimum global timelock: 100 blocks. Entities can set higher.
pub const MIN_GLOBAL_TIMELOCK_BLOCKS: u64 = 100;

impl Invariant for MandatoryExecutionDelay {
    fn id(&self) -> InvariantId {
        InvariantId::MandatoryExecutionDelay
    }

    fn validate(&self, ctx: &InvariantContext) -> Result<(), InvariantViolation> {
        let needs_timelock = ctx.manifest.touches_economics() || ctx.manifest.touches_authz();
        if !needs_timelock {
            return Ok(());
        }

        let rat_block = match ctx.manifest.ratification_block {
            Some(b) => b,
            None => {
                // If no ratification block set yet, this is still in draft.
                // We'll check again at activation time.
                return Ok(());
            }
        };

        let act_block = ctx
            .manifest
            .activation_block
            .ok_or_else(|| InvariantViolation {
                id: self.id(),
                domain: self.id().domain(),
                message: "activation_block must be set for amendments touching econ/caps".into(),
                details: vec![],
            })?;

        // Use entity-specific delay or global minimum, whichever is higher
        let entity_delay = ctx
            .manifest
            .economic_effects
            .first()
            .map(|e| match e {
                EconomicEffect::MutualCreditPolicyChange { entity_id, .. }
                | EconomicEffect::BudgetScopeChange { entity_id, .. }
                | EconomicEffect::DemurrageChange { entity_id, .. }
                | EconomicEffect::DisputeHookChange { entity_id, .. }
                | EconomicEffect::SettlementRuleChange { entity_id, .. } => {
                    ctx.state.current_min_timelock_blocks(entity_id)
                }
            })
            .unwrap_or(0);

        let min_delay = entity_delay.max(MIN_GLOBAL_TIMELOCK_BLOCKS);

        if act_block < rat_block.saturating_add(min_delay) {
            return Err(InvariantViolation {
                id: self.id(),
                domain: self.id().domain(),
                message: "Activation block violates mandatory timelock".into(),
                details: vec![
                    ("ratification_block".into(), rat_block.to_string()),
                    ("activation_block".into(), act_block.to_string()),
                    ("min_delay_blocks".into(), min_delay.to_string()),
                ],
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_diff::{DiffSummary, SubjectDelta};

    /// Test state that returns a fixed timelock.
    struct TestState {
        timelock_blocks: u64,
    }

    impl InvariantStateView for TestState {
        fn current_min_timelock_blocks(&self, _entity_id: &str) -> u64 {
            self.timelock_blocks
        }
    }

    fn make_ctx<'a>(
        manifest: &'a EffectManifest,
        power_diff: Option<&'a PowerDiff>,
        state: &'a dyn InvariantStateView,
    ) -> InvariantContext<'a> {
        InvariantContext {
            current_block: 1000,
            manifest,
            power_diff,
            state,
        }
    }

    // --- Invariant #4 tests ---

    #[test]
    fn test_no_silent_power_changes_passes_for_non_authz() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        assert!(NoSilentPowerChanges.validate(&ctx).is_ok());
    }

    #[test]
    fn test_no_silent_power_changes_rejects_authz_without_diff() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![CapabilityEffect::Grant {
                subject: "did:icn:b".into(),
                capability: "vote".into(),
                scope: "coop:x".into(),
                resource: "governance".into(),
            }],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        let result = NoSilentPowerChanges.validate(&ctx);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().id, InvariantId::NoSilentPowerChanges);
    }

    #[test]
    fn test_no_silent_power_changes_passes_with_diff() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![CapabilityEffect::Grant {
                subject: "did:icn:b".into(),
                capability: "vote".into(),
                scope: "coop:x".into(),
                resource: "governance".into(),
            }],
            vec![],
            vec![],
            vec![],
        );
        let pd = PowerDiff::new(
            m.manifest_hash,
            [0u8; 32],
            vec![SubjectDelta {
                subject: "did:icn:b".into(),
                capabilities_gained: vec![],
                capabilities_lost: vec![],
                scope_widened: vec![],
                scope_narrowed: vec![],
            }],
            DiffSummary::default(),
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, Some(&pd), &state);
        assert!(NoSilentPowerChanges.validate(&ctx).is_ok());
    }

    // --- Invariant #5 tests ---

    #[test]
    fn test_bounded_authority_passes_for_scoped() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![CapabilityEffect::Grant {
                subject: "did:icn:b".into(),
                capability: "vote".into(),
                scope: "coop:sunrise".into(),
                resource: "governance".into(),
            }],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        assert!(BoundedAuthority.validate(&ctx).is_ok());
    }

    #[test]
    fn test_bounded_authority_rejects_wildcard_scope() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![CapabilityEffect::Grant {
                subject: "did:icn:b".into(),
                capability: "admin".into(),
                scope: "*".into(),
                resource: "everything".into(),
            }],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        let result = BoundedAuthority.validate(&ctx);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().id, InvariantId::BoundedAuthority);
    }

    #[test]
    fn test_bounded_authority_rejects_wildcard_resource() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![CapabilityEffect::Grant {
                subject: "did:icn:b".into(),
                capability: "admin".into(),
                scope: "coop:x".into(),
                resource: "*".into(),
            }],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        assert!(BoundedAuthority.validate(&ctx).is_err());
    }

    // --- Invariant #6 tests ---

    #[test]
    fn test_timelock_passes_for_non_econ() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        assert!(MandatoryExecutionDelay.validate(&ctx).is_ok());
    }

    #[test]
    fn test_timelock_passes_for_draft_without_ratification() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![],
            vec![EconomicEffect::DemurrageChange {
                entity_id: "coop:x".into(),
                old_rate_bps: 100,
                new_rate_bps: 200,
            }],
            vec![],
            vec![],
        );
        // ratification_block is None → skip check (still in draft)
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        assert!(MandatoryExecutionDelay.validate(&ctx).is_ok());
    }

    #[test]
    fn test_timelock_rejects_too_short() {
        let mut m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![],
            vec![EconomicEffect::DemurrageChange {
                entity_id: "coop:x".into(),
                old_rate_bps: 100,
                new_rate_bps: 200,
            }],
            vec![],
            vec![],
        );
        m.ratification_block = Some(1000);
        m.activation_block = Some(1050); // Only 50 blocks — below MIN_GLOBAL (100)
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        let result = MandatoryExecutionDelay.validate(&ctx);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().id, InvariantId::MandatoryExecutionDelay);
    }

    #[test]
    fn test_timelock_passes_with_sufficient_delay() {
        let mut m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![],
            vec![EconomicEffect::DemurrageChange {
                entity_id: "coop:x".into(),
                old_rate_bps: 100,
                new_rate_bps: 200,
            }],
            vec![],
            vec![],
        );
        m.ratification_block = Some(1000);
        m.activation_block = Some(1200); // 200 blocks — above MIN_GLOBAL (100)
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        assert!(MandatoryExecutionDelay.validate(&ctx).is_ok());
    }

    #[test]
    fn test_timelock_uses_entity_override() {
        let mut m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![],
            vec![EconomicEffect::DemurrageChange {
                entity_id: "coop:x".into(),
                old_rate_bps: 100,
                new_rate_bps: 200,
            }],
            vec![],
            vec![],
        );
        m.ratification_block = Some(1000);
        m.activation_block = Some(1200); // 200 blocks
                                         // Entity requires 500 blocks
        let state = TestState {
            timelock_blocks: 500,
        };
        let ctx = make_ctx(&m, None, &state);
        let result = MandatoryExecutionDelay.validate(&ctx);
        assert!(result.is_err()); // 200 < 500
    }

    // --- Gate integration tests ---

    #[test]
    fn test_gate_all_pass() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        let gate = InvariantGate::default_frozen_core();
        let report = gate.evaluate(&ctx);
        assert!(report.passed);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn test_gate_collects_multiple_violations() {
        // Wildcard + missing diff = two violations
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:a".into(),
            vec![CapabilityEffect::Grant {
                subject: "did:icn:b".into(),
                capability: "admin".into(),
                scope: "*".into(),
                resource: "*".into(),
            }],
            vec![],
            vec![],
            vec![],
        );
        let state = TestState { timelock_blocks: 0 };
        let ctx = make_ctx(&m, None, &state);
        let gate = InvariantGate::default_frozen_core();
        let report = gate.evaluate(&ctx);
        assert!(!report.passed);
        // Should have violations from both #4 (no diff) and #5 (wildcard)
        assert!(report.violations.len() >= 2);
    }
}
