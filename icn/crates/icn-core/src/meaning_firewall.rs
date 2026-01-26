//! Meaning Firewall Validation Tests
//!
//! These tests verify that the kernel/app separation architecture is maintained.
//! The "meaning firewall" ensures kernel crates only deal with mechanical constraints
//! (e.g., `min_votes = 67`) and never domain-specific concepts (e.g., "supermajority").
//!
//! # Architecture Rule
//!
//! Kernel crates should only use types from icn-kernel-api:
//! - PolicyRequest, PolicyDecision, ConstraintSet
//! - Domain, ActionKind
//!
//! They should NOT use domain-specific types:
//! - TrustGraph, TrustClass, TrustScore (from icn-trust)
//! - GovernanceRules, DecisionType (from icn-governance)  
//! - MembershipCriteria (from icn-entity/coop/community)
//!
//! # Current Status
//!
//! These tests document the target state after Phase 2 refactoring.
//! Tests prefixed with `current_` show existing violations.
//! Tests prefixed with `target_` will pass after Phase 2.

use std::path::PathBuf;

/// Core kernel crates that must not depend on icn-trust.
const KERNEL_CRATES: &[&str] = &["icn-net", "icn-gateway", "icn-gossip", "icn-ledger"];

/// Domain-specific crates that kernel must not depend on directly.
const DOMAIN_CRATES: &[&str] = &["icn-trust", "icn-governance"];

/// Forbidden import patterns in kernel crates.
#[allow(dead_code)] // Documented for reference; will be used in Phase 2 migration
const FORBIDDEN_IMPORTS: &[&str] = &[
    "use icn_trust::",
    "icn_trust::",
    "TrustGraph",
    "TrustClass",
    "TrustScore",
    "GovernanceRules",
    "MembershipCriteria",
];

/// Allowed kernel-api types that kernel crates CAN use.
#[allow(dead_code)] // Documented for reference; will be used in Phase 2 migration
const ALLOWED_ORACLE_TYPES: &[&str] = &[
    "PolicyRequest",
    "PolicyDecision",
    "ConstraintSet",
    "PolicyOracle",
    "Domain",
    "ActionKind",
];

fn get_workspace_root() -> PathBuf {
    // Navigate from icn/crates/icn-core to icn/crates
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("Could not find crates directory")
        .to_path_buf()
}

fn read_cargo_toml(crate_name: &str) -> Option<String> {
    let path = get_workspace_root().join(crate_name).join("Cargo.toml");
    std::fs::read_to_string(path).ok()
}

fn list_rust_files(crate_name: &str) -> Vec<PathBuf> {
    let src_path = get_workspace_root().join(crate_name).join("src");
    let mut files = Vec::new();

    fn collect_rs_files(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_rs_files(&path, files);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
    }

    collect_rs_files(&src_path, &mut files);
    files
}

/// Check if a Cargo.toml contains a dependency on a specific crate.
fn has_dependency(cargo_toml: &str, dep_name: &str) -> bool {
    // Handle various dependency declaration patterns:
    // 1. Standard key forms:
    //    icn-trust = "0.1.0"
    //    icn-trust = { path = "...", version = "0.1.0" }
    // 2. Workspace key form:
    //    icn-trust.workspace = true
    // 3. Table headers:
    //    [dependencies.icn-trust]
    //    [dev-dependencies.icn-trust]
    //    [build-dependencies.icn-trust]
    //
    // Note: We intentionally do NOT match generic quoted occurrences like
    // `"icn-trust"` to avoid false positives from comments or descriptions.
    cargo_toml.contains(&format!("{dep_name} ="))
        || cargo_toml.contains(&format!("{dep_name}="))
        || cargo_toml.contains(&format!("{dep_name}.workspace"))
        || cargo_toml.contains(&format!("[dependencies.{dep_name}]"))
        || cargo_toml.contains(&format!("[dev-dependencies.{dep_name}]"))
        || cargo_toml.contains(&format!("[build-dependencies.{dep_name}]"))
}

/// Count occurrences of a pattern in source files.
fn count_imports_in_crate(crate_name: &str, pattern: &str) -> usize {
    let mut count = 0;
    for file in list_rust_files(crate_name) {
        if let Ok(content) = std::fs::read_to_string(&file) {
            count += content.matches(pattern).count();
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Documents CURRENT state: kernel crates DO have icn-trust dependencies.
    /// This test should PASS now but will need to be removed after Phase 2.
    #[test]
    fn current_kernel_crates_have_trust_dependencies() {
        // Explicitly document expected violations for Phase 1
        const EXPECTED_VIOLATIONS: &[&str] =
            &["icn-net", "icn-gateway", "icn-gossip", "icn-ledger"];

        let mut violations = Vec::new();

        for crate_name in KERNEL_CRATES {
            if let Some(cargo_toml) = read_cargo_toml(crate_name) {
                if has_dependency(&cargo_toml, "icn-trust") {
                    violations.push(*crate_name);
                }
            }
        }

        // This documents the current state - all 4 kernel crates depend on icn-trust
        assert_eq!(
            violations.len(),
            EXPECTED_VIOLATIONS.len(),
            "Expected violations in {:?}, found {:?}. \
             If this changed, Phase 2 is making progress!",
            EXPECTED_VIOLATIONS,
            violations
        );
    }

    /// Documents CURRENT import violations.
    /// After Phase 2, this count should be 0.
    #[test]
    fn current_trust_import_count() {
        let mut total_imports = 0;

        for crate_name in KERNEL_CRATES {
            let count = count_imports_in_crate(crate_name, "use icn_trust::");
            total_imports += count;
        }

        // Document current state - expected to be >0 before Phase 2
        // As Phase 2 progresses, this number should decrease
        println!("Current icn_trust imports in kernel crates: {total_imports}");
        assert!(
            total_imports > 0,
            "If this is 0, Phase 2 may be complete! \
             Update this test to verify no regressions."
        );
    }

    /// TARGET state: kernel crates should NOT depend on icn-trust.
    /// This test is currently IGNORED and will be enabled after Phase 2.
    #[test]
    #[ignore = "Enable after Phase 2 completes - tracks #865, #866, #867"]
    fn target_kernel_crates_no_trust_dependency() {
        for crate_name in KERNEL_CRATES {
            if let Some(cargo_toml) = read_cargo_toml(crate_name) {
                assert!(
                    !has_dependency(&cargo_toml, "icn-trust"),
                    "{crate_name} depends on icn-trust - firewall breached! \
                     Use PolicyOracle instead of direct TrustGraph access."
                );
            }
        }
    }

    /// TARGET state: no direct icn_trust imports in kernel code.
    #[test]
    #[ignore = "Enable after Phase 2 completes"]
    fn target_no_trust_imports_in_kernel() {
        for crate_name in KERNEL_CRATES {
            for file in list_rust_files(crate_name) {
                if let Ok(content) = std::fs::read_to_string(&file) {
                    for pattern in &["use icn_trust::", "TrustGraph", "TrustClass"] {
                        assert!(
                            !content.contains(pattern),
                            "File {} contains forbidden import: {pattern}",
                            file.display()
                        );
                    }
                }
            }
        }
    }

    /// Verify kernel crates CAN use PolicyOracle types from kernel-api.
    /// This is the CORRECT pattern after Phase 2.
    #[test]
    fn kernel_api_provides_oracle_types() {
        // Verify icn-kernel-api has the types kernel crates should use
        if let Some(cargo_toml) = read_cargo_toml("icn-kernel-api") {
            // icn-kernel-api should exist and be the bridge
            assert!(
                cargo_toml.contains("icn-kernel-api"),
                "icn-kernel-api crate exists"
            );
        }

        // Verify the PolicyOracle types are defined in kernel-api
        let kernel_api_files = list_rust_files("icn-kernel-api");
        let mut has_policy_oracle = false;

        for file in kernel_api_files {
            if let Ok(content) = std::fs::read_to_string(&file) {
                if content.contains("PolicyOracle") || content.contains("PolicyRequest") {
                    has_policy_oracle = true;
                    break;
                }
            }
        }

        assert!(
            has_policy_oracle,
            "icn-kernel-api must provide PolicyOracle/PolicyRequest types \
             for kernel crates to use instead of direct TrustGraph access"
        );
    }

    /// Verify kernel-api does NOT expose domain-specific types.
    #[test]
    fn kernel_api_no_domain_types() {
        for file in list_rust_files("icn-kernel-api") {
            if let Ok(content) = std::fs::read_to_string(&file) {
                // kernel-api should not define or re-export domain types
                for pattern in &[
                    "struct TrustGraph",
                    "pub struct TrustClass",
                    "GovernanceRules",
                ] {
                    assert!(
                        !content.contains(pattern),
                        "icn-kernel-api should not define domain type: {pattern}"
                    );
                }
            }
        }
    }

    /// Summary test that prints firewall status.
    #[test]
    fn firewall_status_summary() {
        println!("\n=== Meaning Firewall Status ===\n");

        // Check Cargo.toml dependencies
        let mut cargo_violations = Vec::new();
        for crate_name in KERNEL_CRATES {
            if let Some(cargo_toml) = read_cargo_toml(crate_name) {
                for domain_crate in DOMAIN_CRATES {
                    if has_dependency(&cargo_toml, domain_crate) {
                        cargo_violations.push(format!("{crate_name} -> {domain_crate}"));
                    }
                }
            }
        }

        // Check import statements
        let mut import_violations = Vec::new();
        for crate_name in KERNEL_CRATES {
            let count = count_imports_in_crate(crate_name, "use icn_trust::");
            if count > 0 {
                import_violations.push(format!("{crate_name}: {count} imports"));
            }
        }

        println!("Cargo.toml violations: {}", cargo_violations.len());
        for v in &cargo_violations {
            println!("  - {v}");
        }

        println!("\nImport violations: {}", import_violations.len());
        for v in &import_violations {
            println!("  - {v}");
        }

        let is_clean = cargo_violations.is_empty() && import_violations.is_empty();
        println!(
            "\nFirewall status: {}",
            if is_clean {
                "✅ CLEAN"
            } else {
                "⚠️ VIOLATIONS DETECTED"
            }
        );
        println!("(Violations expected until Phase 2 completes - see #865, #866, #867)");
    }
}
