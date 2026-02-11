//! Golden list test for translator-emitted effects surface contract.
//!
//! This test ensures that the governance app translator (`execution.rs`) only
//! emits KernelEffects from a known, approved list. If a new effect is added
//! to the translator, this test will fail until the corresponding executor,
//! service, and tests are implemented.
//!
//! # Why this test exists
//!
//! The kernel executes effects without understanding domain semantics.
//! Every new effect type requires:
//! - Kernel executor implementation
//! - Service layer integration
//! - Integration tests
//!
//! This test prevents accidentally landing new effects without the above.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Canonical list of KernelEffects that the governance translator is approved to emit.
///
/// **IMPORTANT**: Do not add to this list without implementing:
/// 1. Executor in kernel crates
/// 2. Service layer integration
/// 3. Integration tests proving the effect works end-to-end
///
/// Effects are listed as "Category::Variant" strings for easy grepping.
fn approved_emitted_effects() -> BTreeSet<&'static str> {
    [
        // Treasury effects
        "Treasury::Spend",
        "Treasury::CreateBudget",
        // Membership effects
        "Membership::AddMember",
        "Membership::RemoveMember",
        "Membership::FreezeMember",
        "Membership::UnfreezeMember",
        // Protocol effects
        "Protocol::SetGovernanceConfig",
        "Protocol::SetSchedulingPolicy",
        "Protocol::Upgrade",
        // Control effects
        "Control::VetoProposal",
        "Control::ForceCloseProposal",
        "Control::TextResolution",
        // Federation effects
        "Federation::JoinFederation",
        "Federation::LeaveFederation",
        "Federation::EstablishClearing",
        // Fallback (always allowed)
        "NoOp",
    ]
    .into_iter()
    .collect()
}

/// Extract all KernelEffect patterns from the translator source file.
///
/// Looks for patterns like:
/// - `KernelEffect::Treasury(TreasuryEffect::Spend { ... })`
/// - `KernelEffect::NoOp { ... }`
fn extract_emitted_effects(source: &str) -> BTreeSet<String> {
    let mut effects = BTreeSet::new();

    // Pattern 1: KernelEffect::Category(CategoryEffect::Variant { ... })
    // e.g., KernelEffect::Treasury(TreasuryEffect::Spend {
    let nested_re = regex::Regex::new(
        r"KernelEffect::(\w+)\(\s*(\w+)Effect::(\w+)\s*\{",
    )
    .expect("valid regex");

    for cap in nested_re.captures_iter(source) {
        let category = &cap[1];
        let variant = &cap[3];
        effects.insert(format!("{category}::{variant}"));
    }

    // Pattern 2: KernelEffect::NoOp { ... }
    let noop_re = regex::Regex::new(r"KernelEffect::NoOp\s*\{").expect("valid regex");
    if noop_re.is_match(source) {
        effects.insert("NoOp".to_string());
    }

    effects
}

/// Find the translator source file, trying multiple possible locations.
fn find_translator_source() -> Option<String> {
    // Try relative paths from the test execution directory
    let possible_paths = [
        // From icn/ directory (cargo test)
        "apps/governance/src/handlers/execution.rs",
        // From workspace root
        "../apps/governance/src/handlers/execution.rs",
        // Absolute path fallback
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/governance/src/handlers/execution.rs"
        ),
    ];

    for path in &possible_paths {
        if let Ok(content) = fs::read_to_string(path) {
            return Some(content);
        }
    }

    // Try using CARGO_MANIFEST_DIR to construct path
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let from_manifest = Path::new(manifest_dir)
        .join("../../apps/governance/src/handlers/execution.rs");
    
    fs::read_to_string(&from_manifest).ok()
}

#[test]
fn test_emitted_effects_surface_contract() {
    let source = find_translator_source().expect(
        "Could not find translator source file: apps/governance/src/handlers/execution.rs\n\
         This test must be run from the icn/ directory.",
    );

    let approved = approved_emitted_effects();
    let actual = extract_emitted_effects(&source);

    // Find any effects that are emitted but not in the approved list
    let approved_set: BTreeSet<String> = approved.iter().map(|s| s.to_string()).collect();
    let unapproved: Vec<_> = actual.difference(&approved_set).collect();

    if !unapproved.is_empty() {
        panic!(
            "\n\
            ╔══════════════════════════════════════════════════════════════════════════════╗\n\
            ║  EMITTED EFFECTS SURFACE CONTRACT VIOLATION                                  ║\n\
            ╠══════════════════════════════════════════════════════════════════════════════╣\n\
            ║  New effect(s) detected in translator that are NOT in the approved list:    ║\n\
            ║                                                                              ║\n\
            {effects_display}\
            ║                                                                              ║\n\
            ║  Before landing this change, you MUST:                                       ║\n\
            ║  1. Implement executor in kernel crates                                      ║\n\
            ║  2. Add service layer integration                                            ║\n\
            ║  3. Add integration tests proving the effect works end-to-end                ║\n\
            ║  4. Add the effect to approved_emitted_effects() in this test                ║\n\
            ╚══════════════════════════════════════════════════════════════════════════════╝\n",
            effects_display = unapproved
                .iter()
                .map(|e| format!("║    • {e:<70} ║\n"))
                .collect::<String>()
        );
    }

    // Also check for effects in approved list that are no longer emitted (stale entries)
    let stale: Vec<_> = approved
        .iter()
        .filter(|a| !actual.contains(&a.to_string()))
        .collect();

    if !stale.is_empty() {
        eprintln!(
            "\n\
            ⚠️  WARNING: The following approved effects are no longer emitted by the translator.\n\
            Consider removing them from approved_emitted_effects() if they are obsolete:\n"
        );
        for effect in &stale {
            eprintln!("    • {effect}");
        }
        eprintln!();
    }

    // Print summary
    println!("\n✅ Emitted effects surface contract check passed!");
    println!("   Approved effects: {}", approved.len());
    println!("   Emitted effects:  {}", actual.len());
    println!("\n   Emitted effects found:");
    for effect in &actual {
        println!("     • {effect}");
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_extract_nested_effect() {
        let source = r#"
            vec![KernelEffect::Treasury(TreasuryEffect::Spend {
                treasury_did: treasury_did.to_string(),
            })]
        "#;
        let effects = extract_emitted_effects(source);
        assert!(effects.contains("Treasury::Spend"), "Should find Treasury::Spend");
    }

    #[test]
    fn test_extract_noop() {
        let source = r#"
            vec![KernelEffect::NoOp {
                reason: format!("Unhandled"),
            }]
        "#;
        let effects = extract_emitted_effects(source);
        assert!(effects.contains("NoOp"), "Should find NoOp");
    }

    #[test]
    fn test_extract_multiple_effects() {
        let source = r#"
            KernelEffect::Membership(MembershipEffect::AddMember {
            KernelEffect::Membership(MembershipEffect::RemoveMember {
            KernelEffect::Protocol(ProtocolEffect::Upgrade {
        "#;
        let effects = extract_emitted_effects(source);
        assert_eq!(effects.len(), 3);
        assert!(effects.contains("Membership::AddMember"));
        assert!(effects.contains("Membership::RemoveMember"));
        assert!(effects.contains("Protocol::Upgrade"));
    }

    #[test]
    fn test_approved_list_is_complete() {
        let approved = approved_emitted_effects();
        // Sanity check that our approved list has the expected items
        assert!(approved.contains("Treasury::Spend"));
        assert!(approved.contains("Membership::AddMember"));
        assert!(approved.contains("NoOp"));
    }
}
