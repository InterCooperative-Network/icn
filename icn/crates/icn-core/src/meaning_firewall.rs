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
//! # Strict Mode
//!
//! These tests run in CI and **fail on regressions**. The known violation counts
//! are pinned to exact values — adding new violations will break the build.
//! Reducing violations requires updating the pinned counts downward.

use std::path::PathBuf;

/// Core kernel crates that must not depend on domain crates.
///
/// icn-core is included with pinned violation counts. The governance/ledger
/// dependencies are being migrated to apps/ (see #913, #914). The ratchet
/// tests prevent regressions while migration proceeds incrementally.
const KERNEL_CRATES: &[&str] = &["icn-core", "icn-net", "icn-gateway", "icn-gossip", "icn-ledger"];

/// Domain-specific crates that kernel must not depend on directly.
const DOMAIN_CRATES: &[&str] = &["icn-trust", "icn-governance"];

/// Forbidden import patterns in kernel crates.
/// Used as reference for what constitutes a Meaning Firewall violation.
/// The strict_*_import_violations tests use `use icn_*::` patterns directly;
/// bare type names here are too broad for automated scanning (appear in docs).
#[allow(dead_code)]
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
#[allow(dead_code)]
const ALLOWED_ORACLE_TYPES: &[&str] = &[
    "PolicyRequest",
    "PolicyDecision",
    "ConstraintSet",
    "PolicyOracle",
    "Domain",
    "ActionKind",
];

fn get_workspace_root() -> PathBuf {
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
    cargo_toml.contains(&format!("{dep_name} ="))
        || cargo_toml.contains(&format!("{dep_name}="))
        || cargo_toml.contains(&format!("{dep_name}.workspace"))
        || cargo_toml.contains(&format!("[dependencies.{dep_name}]"))
        || cargo_toml.contains(&format!("[dev-dependencies.{dep_name}]"))
        || cargo_toml.contains(&format!("[build-dependencies.{dep_name}]"))
}

/// Count occurrences of a pattern in source files.
///
/// Skips `meaning_firewall.rs` to avoid counting the test infrastructure's
/// own string literals as violations.
fn count_imports_in_crate(crate_name: &str, pattern: &str) -> usize {
    let mut count = 0;
    for file in list_rust_files(crate_name) {
        // Skip counting ourselves — our string literals contain the patterns we're searching for
        if file
            .file_name()
            .is_some_and(|f| f == "meaning_firewall.rs")
        {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&file) {
            count += content.matches(pattern).count();
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===================================================================
    // STRICT REGRESSION TESTS
    //
    // These tests pin the EXACT violation counts. They prevent regressions
    // (adding new domain imports to kernel crates) while tracking progress.
    //
    // When you remove a violation, LOWER the expected count.
    // If a test fails because the count is LOWER than expected, that's
    // progress — update the constant.
    // If a test fails because the count is HIGHER, that's a regression.
    // ===================================================================

    /// Pinned Cargo.toml dependency violations per kernel crate.
    ///
    /// Update these when violations are removed. Adding new violations
    /// will cause this test to fail.
    ///
    /// Current state (2026-01-31):
    /// - icn-core: 2 (icn-trust dev-dep + icn-governance main+dev-dep)
    /// - icn-gossip: CLEAN
    /// - icn-net: CLEAN (dev-dep removed by #915/PR #973)
    /// - icn-gateway: 2 (icn-trust + icn-governance)
    /// - icn-ledger: 2 (icn-trust + icn-governance)
    #[test]
    fn strict_cargo_dependency_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-core", 2),    // icn-trust (dev) + icn-governance (main+dev) — #913 migration
            ("icn-gossip", 0),  // CLEAN ✅
            ("icn-net", 0),     // CLEAN ✅ (dev-dep removed by #915/PR #973)
            ("icn-gateway", 2), // icn-trust + icn-governance
            ("icn-ledger", 2),  // icn-trust + icn-governance
        ];

        for &(crate_name, expected_count) in expected {
            let cargo_toml = read_cargo_toml(crate_name)
                .unwrap_or_else(|| panic!("Could not read {crate_name}/Cargo.toml"));

            let mut actual = 0;
            for domain_crate in DOMAIN_CRATES {
                if has_dependency(&cargo_toml, domain_crate) {
                    actual += 1;
                }
            }

            assert!(
                actual <= expected_count,
                "REGRESSION: {crate_name} has {actual} domain-crate dependencies \
                 (expected at most {expected_count}). \
                 Do not add new domain deps to kernel crates — use PolicyOracle instead."
            );

            if actual < expected_count {
                panic!(
                    "✅ PROGRESS: {crate_name} now has only {actual} domain-crate deps \
                     (was pinned at {expected_count}). Update the pinned count in \
                     meaning_firewall.rs::strict_cargo_dependency_violations()."
                );
            }
        }
    }

    /// Pinned `use icn_trust::` import count per kernel crate.
    ///
    /// Update these when imports are removed. Adding new imports
    /// will cause this test to fail.
    ///
    /// Current state (2026-01-31):
    /// - icn-core: 0 (icn-trust is dev-only; no source imports)
    /// - icn-gossip: CLEAN
    /// - icn-net: CLEAN (no source imports, only had Cargo.toml dev-dep)
    /// - icn-gateway: 3 (trust_mgr.rs:2, api/trust.rs:1)
    /// - icn-ledger: 3 (credit_policy.rs:1, ledger.rs:1, fork_resolution.rs:1)
    #[test]
    fn strict_trust_import_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-core", 0),   // icn-trust is dev-dep only, no source imports
            ("icn-gossip", 0), // CLEAN ✅
            ("icn-net", 0),    // CLEAN ✅
            ("icn-gateway", 3),
            ("icn-ledger", 0), // CLEAN ✅ (imports removed by #970)
        ];

        for &(crate_name, expected_count) in expected {
            let actual = count_imports_in_crate(crate_name, "use icn_trust::");

            assert!(
                actual <= expected_count,
                "REGRESSION: {crate_name} has {actual} `use icn_trust::` imports \
                 (expected at most {expected_count}). \
                 Do not add new icn_trust imports to kernel crates."
            );

            if actual < expected_count {
                panic!(
                    "✅ PROGRESS: {crate_name} now has only {actual} `use icn_trust::` \
                     imports (was pinned at {expected_count}). Update the pinned count in \
                     meaning_firewall.rs::strict_trust_import_violations()."
                );
            }
        }
    }

    /// Pinned `use icn_governance::` import count per kernel crate.
    ///
    /// Mirrors `strict_trust_import_violations` for governance imports.
    /// Governance extraction is Phase 4 — these counts will ratchet to 0.
    ///
    /// Current state (2026-01-31):
    /// - icn-core: 11 (governance actor + handlers + init — #913 migration)
    /// - icn-net: CLEAN ✅
    /// - icn-gossip: CLEAN ✅
    /// - icn-ledger: CLEAN ✅
    /// - icn-gateway: 17 (governance admin endpoints — Phase 4 work)
    #[test]
    fn strict_governance_import_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-core", 11),    // governance actor + handlers + init — #913 migration target
            ("icn-net", 0),      // CLEAN ✅
            ("icn-gossip", 0),   // CLEAN ✅
            ("icn-ledger", 0),   // CLEAN ✅
            ("icn-gateway", 17), // Phase 4 governance extraction pending
        ];

        for &(crate_name, expected_count) in expected {
            let actual = count_imports_in_crate(crate_name, "use icn_governance::");

            assert!(
                actual <= expected_count,
                "REGRESSION: {crate_name} has {actual} `use icn_governance::` imports \
                 (expected at most {expected_count}). \
                 Do not add new icn_governance imports to kernel crates."
            );

            if actual < expected_count {
                panic!(
                    "✅ PROGRESS: {crate_name} now has only {actual} `use icn_governance::` \
                     imports (was pinned at {expected_count}). Update the pinned count in \
                     meaning_firewall.rs::strict_governance_import_violations()."
                );
            }
        }
    }

    /// Pinned `use icn_ledger::` import count for icn-core.
    ///
    /// icn-ledger is not in DOMAIN_CRATES (it's shared infrastructure),
    /// but icn-core should not directly import ledger types (#914).
    /// This ratchet tracks the migration progress.
    ///
    /// Current state (2026-01-31):
    /// - icn-core: 11 (governance_handlers + lifecycle + init_rpc + init_compute)
    #[test]
    fn strict_ledger_import_violations_in_core() {
        let actual = count_imports_in_crate("icn-core", "use icn_ledger::");
        let expected = 11; // #914 migration target
        assert!(
            actual <= expected,
            "REGRESSION: icn-core has {actual} `use icn_ledger::` imports \
             (expected at most {expected}). \
             Do not add new icn_ledger imports to icn-core."
        );
        if actual < expected {
            panic!(
                "✅ PROGRESS: icn-core now has only {actual} `use icn_ledger::` \
                 imports (was pinned at {expected}). Update the pinned count in \
                 meaning_firewall.rs::strict_ledger_import_violations_in_core()."
            );
        }
    }

    /// Ensure icn-gossip remains clean (no domain-crate dependencies or imports).
    /// This crate was cleaned in Wave 1A and must stay clean.
    #[test]
    fn gossip_crate_stays_clean() {
        if let Some(cargo_toml) = read_cargo_toml("icn-gossip") {
            assert!(
                !has_dependency(&cargo_toml, "icn-trust"),
                "icn-gossip must not depend on icn-trust — it was cleaned in Wave 1A"
            );
            assert!(
                !has_dependency(&cargo_toml, "icn-governance"),
                "icn-gossip must not depend on icn-governance"
            );
        }

        // Verify no domain-crate imports leak into gossip source.
        // Uses `use icn_*::` patterns (not bare type names from
        // FORBIDDEN_IMPORTS, which can appear in doc comments/strings).
        for domain in DOMAIN_CRATES {
            let import_pattern = format!("use {}::", domain.replace('-', "_"));
            let hits = count_imports_in_crate("icn-gossip", &import_pattern);
            assert_eq!(
                hits, 0,
                "icn-gossip has {hits} `{import_pattern}` imports — must be 0"
            );
        }
    }

    /// Verify kernel-api provides the required abstraction types.
    #[test]
    fn kernel_api_provides_oracle_types() {
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
                "CLEAN"
            } else {
                "VIOLATIONS DETECTED (expected until Phase 2 kernel cleanup completes)"
            }
        );
    }
}
