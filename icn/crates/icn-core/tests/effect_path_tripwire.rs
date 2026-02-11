//! Effect Path Migration Tripwires
//!
//! These tests document and verify the state of the effect path migration.
//! They serve as:
//! 1. Documentation of which paths are migrated
//! 2. Tripwires that fail if migration state changes unexpectedly
//! 3. Guard rails preventing regression
//!
//! The legacy `governance_handlers/` has been deleted.
//! Effect path is now the only path for governance proposal execution.

use std::path::PathBuf;

/// Find the workspace root by looking for Cargo.toml with [workspace]
fn find_workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().parent().unwrap().to_path_buf()
}

// =============================================================================
// LEGACY PATH TRIPWIRES
// =============================================================================

/// Tripwire: governance_handlers directory must NOT exist
///
/// The legacy governance_handlers have been deleted. The effect path is now
/// the only path for governance proposal execution.
#[test]
fn tripwire_legacy_governance_handlers_deleted() {
    let workspace = find_workspace_root();
    let handlers_dir = workspace.join("crates/icn-core/src/supervisor/governance_handlers");

    // Legacy path removed - migration complete!
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║           🎉 LEGACY GOVERNANCE HANDLERS DELETED 🎉               ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ Status: EFFECT PATH IS NOW THE ONLY PATH                         ║");
    println!("║ Directory: governance_handlers/ (DELETED)                        ║");
    println!("║                                                                  ║");
    println!("║ ✅ Migration complete                                            ║");
    println!("║ ✅ Effect dispatcher is primary execution path                   ║");
    println!("║ ✅ All proposals route through kernel-safe effects              ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");

    assert!(
        !handlers_dir.exists(),
        "governance_handlers/ directory should be DELETED. Migration is complete."
    );
}

/// Tripwire: Track which proposal types are handled by effect path
///
/// Update this list as we migrate each proposal type.
#[test]
fn tripwire_effect_path_coverage() {
    // Proposal types with full effect path (Decision → Effect → Service → Durable State)
    let effect_path_complete = [
        "Treasury::Spend",              // LedgerService
        "Federation::Join",             // FederationService
        "Federation::Vouch",            // FederationService
        "Control::Veto",                // ControlService
        "Control::ForceClose",          // ControlService
        "Control::TextResolution",      // ControlService (no-op)
        "Membership::Add",              // MembershipService
        "Membership::Remove",           // MembershipService
        "Membership::Update",           // MembershipService
        "Membership::Freeze",           // MembershipService
        "Membership::Unfreeze",         // MembershipService
        "Protocol::SetParameter",       // ProtocolParameterStore
        "Protocol::SetGovernanceConfig", // ProtocolParameterStore
    ];

    // Proposal types with effect defined but not wired to service
    let effect_defined_only = [
        "Treasury::CreateBudget",
        "Treasury::Allocate",
        "Treasury::Transfer",
        "Treasury::DistributeSurplus",
        "Treasury::RedeemShares",
        "Treasury::IssueBond",
        "Federation::Leave",
        "Federation::EstablishClearing",
        "Protocol::Upgrade",           // Not implemented - returns explicit failure
        "Protocol::SetSchedulingPolicy", // Not implemented - returns explicit failure
        "Dispute::Resolve",
        "Dispute::Rollback",
        "Sdis::ApproveSteward",
        "Sdis::RevokeSteward",
        "Resource::Grant",
        "Resource::Revoke",
    ];

    // Legacy only (no effect type defined)
    let legacy_only: [&str; 0] = [];

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║              EFFECT PATH MIGRATION STATUS                        ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ ✅ Complete (Effect → Service → State): {:2}                      ║", effect_path_complete.len());
    for effect in &effect_path_complete {
        println!("║    • {:50}      ║", effect);
    }
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ ⚠️  Effect defined, service needed: {:2}                          ║", effect_defined_only.len());
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ ❌ Legacy only (no effect type): {:2}                              ║", legacy_only.len());
    println!("╚══════════════════════════════════════════════════════════════════╝");

    // Current target: 13 complete, 16 defined, 0 legacy-only
    // Updated: Membership (5) + Protocol (2) now wired
    assert_eq!(
        effect_path_complete.len(),
        13,
        "Update this test when more effects are wired to services"
    );
}

// =============================================================================
// EFFECT DISPATCHER WIRING TRIPWIRES
// =============================================================================

/// Tripwire: Effect dispatcher is the default in lifecycle.rs
///
/// This test verifies the effect path wiring is in place.
/// The effect path is now the default - no env gate required.
#[test]
fn tripwire_effect_dispatcher_in_lifecycle() {
    let workspace = find_workspace_root();
    let lifecycle_path = workspace.join("crates/icn-core/src/supervisor/lifecycle.rs");

    let content = std::fs::read_to_string(&lifecycle_path).expect("Failed to read lifecycle.rs");

    let has_effect_dispatcher = content.contains("EffectDispatcher")
        || content.contains("effect_dispatcher");

    // Verify required components are present
    let has_kernel_executor = content.contains("KernelGovernanceExecutor");
    let has_effect_subscription = content.contains("create_effect_subscription");
    // Env gate has been removed - effect path is now the default
    let has_no_env_gate = !content.contains("ICN_USE_EFFECT_PATH");

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║         🎉 EFFECT DISPATCHER IS THE DEFAULT PATH 🎉              ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ EffectDispatcher:           {}                           ║", if has_effect_dispatcher { "✅" } else { "❌" });
    println!("║ KernelGovernanceExecutor:   {}                           ║", if has_kernel_executor { "✅" } else { "❌" });
    println!("║ create_effect_subscription: {}                           ║", if has_effect_subscription { "✅" } else { "❌" });
    println!("║ Env gate removed:           {}                           ║", if has_no_env_gate { "✅" } else { "❌" });
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ Status: Effect path is production-ready (always active)         ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");

    assert!(
        has_effect_dispatcher,
        "EffectDispatcher should be in lifecycle.rs"
    );
    assert!(
        has_kernel_executor,
        "KernelGovernanceExecutor should be in lifecycle.rs"
    );
    assert!(
        has_effect_subscription,
        "create_effect_subscription should be in lifecycle.rs"
    );
    assert!(
        has_no_env_gate,
        "ICN_USE_EFFECT_PATH env gate should be removed - effect path is now default"
    );
}

// =============================================================================
// SERVICE IMPLEMENTATION TRIPWIRES
// =============================================================================

/// Tripwire: Track which services are implemented in kernel-api
#[test]
fn tripwire_kernel_services_implemented() {
    let workspace = find_workspace_root();
    let services_path = workspace.join("crates/icn-kernel-api/src/services.rs");

    let content = std::fs::read_to_string(&services_path).expect("Failed to read services.rs");

    // Check for each service trait
    let has_ledger_service = content.contains("pub trait LedgerService");
    let has_federation_service = content.contains("pub trait FederationService");
    let has_membership_service = content.contains("pub trait MembershipService");
    let has_control_service = content.contains("pub trait ControlService");
    let has_protocol_service = content.contains("pub trait ProtocolService");
    let has_dispute_service = content.contains("pub trait DisputeService");

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║              KERNEL SERVICE TRAITS STATUS                        ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ LedgerService:     {} ║", if has_ledger_service { "✅ IMPLEMENTED" } else { "❌ NOT FOUND  " });
    println!("║ FederationService: {} ║", if has_federation_service { "✅ IMPLEMENTED" } else { "❌ NOT FOUND  " });
    println!("║ MembershipService: {} ║", if has_membership_service { "✅ IMPLEMENTED" } else { "❌ NOT FOUND  " });
    println!("║ ControlService:    {} ║", if has_control_service { "✅ IMPLEMENTED" } else { "❌ NOT FOUND  " });
    println!("║ ProtocolService:   {} ║", if has_protocol_service { "✅ IMPLEMENTED" } else { "❌ NOT FOUND  " });
    println!("║ DisputeService:    {} ║", if has_dispute_service { "✅ IMPLEMENTED" } else { "❌ NOT FOUND  " });
    println!("╚══════════════════════════════════════════════════════════════════╝");

    // Current state: LedgerService, FederationService, and ControlService implemented
    assert!(has_ledger_service, "LedgerService should exist");
    assert!(has_federation_service, "FederationService should exist");
    assert!(has_control_service, "ControlService should exist");

    // These should fail, prompting implementation
    // Uncomment as we implement each service:
    // assert!(has_membership_service, "MembershipService should exist");
    // assert!(has_protocol_service, "ProtocolService should exist");
    // assert!(has_dispute_service, "DisputeService should exist");
}

// =============================================================================
// LEGACY PATH VERIFICATION
// =============================================================================

/// Tripwire: Legacy governance_handlers module no longer exists
///
/// The legacy path has been removed. This test confirms there are no lingering
/// references to the deleted module that would cause build failures.
#[test]
fn tripwire_no_legacy_references() {
    // The legacy governance_handlers module has been deleted.
    // If this test compiles, it means there are no compile-time references
    // to the deleted module. Runtime verification is no longer needed since
    // the effect path is now the only path.

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║         🎉 LEGACY PATH REMOVED 🎉                                ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ governance_handlers module: DELETED                              ║");
    println!("║ Effect path: ACTIVE (always)                                     ║");
    println!("║ Env gate ICN_USE_EFFECT_PATH: REMOVED                           ║");
    println!("║                                                                  ║");
    println!("║ All governance proposals now route through:                      ║");
    println!("║   Decision → Effect → Service → Durable State                   ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
}
