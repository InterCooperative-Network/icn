//! Effect Path Migration Tripwires
//!
//! These tests document and verify the state of the legacy → effect path migration.
//! They serve as:
//! 1. Documentation of which paths are migrated
//! 2. Tripwires that fail if migration state changes unexpectedly
//! 3. Guard rails preventing regression
//!
//! The goal is to delete `governance_handlers/` entirely once all paths are migrated.
//! Current state: 5,335 lines of legacy code to remove.

use std::path::PathBuf;

/// Counts lines in a directory recursively
fn count_lines_in_dir(dir: &PathBuf) -> usize {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "rs") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    total += content.lines().count();
                }
            }
        }
    }
    total
}

/// Find the workspace root by looking for Cargo.toml with [workspace]
fn find_workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().parent().unwrap().to_path_buf()
}

// =============================================================================
// LEGACY PATH TRIPWIRES
// =============================================================================

/// Tripwire: governance_handlers directory must exist (or migration is complete)
///
/// This test documents that the legacy path still exists.
/// When we finish migration, this test should be inverted to assert the directory is GONE.
#[test]
fn tripwire_legacy_governance_handlers_exists() {
    let workspace = find_workspace_root();
    let handlers_dir = workspace.join("crates/icn-core/src/supervisor/governance_handlers");

    if handlers_dir.exists() {
        let line_count = count_lines_in_dir(&handlers_dir);
        println!("╔══════════════════════════════════════════════════════════════════╗");
        println!("║            LEGACY GOVERNANCE HANDLERS STATUS                     ║");
        println!("╠══════════════════════════════════════════════════════════════════╣");
        println!("║ Status: LEGACY PATH STILL ACTIVE                                 ║");
        println!("║ Directory: governance_handlers/                                  ║");
        println!("║ Line count: {:5} lines                                        ║", line_count);
        println!("║                                                                  ║");
        println!("║ To complete migration:                                           ║");
        println!("║ 1. Implement remaining services (Membership, Control, Protocol)  ║");
        println!("║ 2. Wire effect dispatcher into lifecycle.rs                      ║");
        println!("║ 3. Delete governance_handlers/                                   ║");
        println!("║ 4. Flip this test to assert directory is GONE                    ║");
        println!("╚══════════════════════════════════════════════════════════════════╝");

        // Document expected line counts per file
        assert!(
            line_count > 4000,
            "Expected ~5335 lines in governance_handlers, got {}. \
             If significantly lower, migration may be in progress - update test.",
            line_count
        );
    } else {
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
    }
}

/// Tripwire: Track which proposal types are handled by effect path
///
/// Update this list as we migrate each proposal type.
#[test]
fn tripwire_effect_path_coverage() {
    // Proposal types with full effect path (Decision → Effect → Service → Durable State)
    let effect_path_complete = [
        "Treasury::Spend",        // LedgerService
        "Federation::Join",       // FederationService
        "Federation::Vouch",      // FederationService
        "Control::Veto",          // ControlService
        "Control::ForceClose",    // ControlService
        "Control::TextResolution", // ControlService (no-op)
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
        "Membership::Add",
        "Membership::Remove",
        "Membership::Update",
        "Membership::Freeze",
        "Membership::Unfreeze",
        "Protocol::SetParameter",
        "Protocol::Upgrade",
        "Protocol::SetSchedulingPolicy",
        "Protocol::SetGovernanceConfig",
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

    // Current target: 6 complete, 23 defined, 0 legacy-only
    assert_eq!(
        effect_path_complete.len(),
        6,
        "Update this test when more effects are wired to services"
    );
}

// =============================================================================
// EFFECT DISPATCHER WIRING TRIPWIRES
// =============================================================================

/// Tripwire: Effect dispatcher should not be in lifecycle.rs yet
///
/// This test will fail when we wire the effect dispatcher into production.
/// That's the signal to update the test and document the change.
#[test]
fn tripwire_effect_dispatcher_not_in_lifecycle() {
    let workspace = find_workspace_root();
    let lifecycle_path = workspace.join("crates/icn-core/src/supervisor/lifecycle.rs");

    let content = std::fs::read_to_string(&lifecycle_path).expect("Failed to read lifecycle.rs");

    let has_effect_dispatcher = content.contains("EffectDispatcher")
        || content.contains("effect_dispatcher");

    if has_effect_dispatcher {
        println!("╔══════════════════════════════════════════════════════════════════╗");
        println!("║         🎉 EFFECT DISPATCHER WIRED INTO LIFECYCLE 🎉             ║");
        println!("╠══════════════════════════════════════════════════════════════════╣");
        println!("║ EffectDispatcher is now in production path!                      ║");
        println!("║ Update this test to verify correct wiring.                       ║");
        println!("╚══════════════════════════════════════════════════════════════════╝");
        // When this happens, update the test to verify the wiring is correct
    } else {
        println!("╔══════════════════════════════════════════════════════════════════╗");
        println!("║         EFFECT DISPATCHER STATUS                                 ║");
        println!("╠══════════════════════════════════════════════════════════════════╣");
        println!("║ Status: NOT YET IN PRODUCTION                                    ║");
        println!("║ Effect dispatcher exists but is test-only                        ║");
        println!("║                                                                  ║");
        println!("║ Production path: lifecycle.rs → governance_handlers              ║");
        println!("║ Test path: effect_dispatcher.rs → governance_executor            ║");
        println!("╚══════════════════════════════════════════════════════════════════╝");
    }

    // Document current state: effect dispatcher is NOT in lifecycle.rs
    assert!(
        !has_effect_dispatcher,
        "Effect dispatcher found in lifecycle.rs! \
         Update this test to verify correct wiring."
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
