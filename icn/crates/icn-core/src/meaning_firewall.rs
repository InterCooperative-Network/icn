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
const KERNEL_CRATES: &[&str] = &["icn-net", "icn-gateway", "icn-gossip", "icn-ledger"];

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

/// Check if a Cargo.toml contains a **production** dependency on a specific crate.
///
/// Only searches the `[dependencies]` section, stopping at `[dev-dependencies]`
/// or `[build-dependencies]`. Dev-deps are intentionally excluded because they
/// don't affect the runtime dependency graph.
fn has_dependency(cargo_toml: &str, dep_name: &str) -> bool {
    // Extract only the [dependencies] section (before [dev-dependencies] etc.)
    let deps_section = if let Some(start) = cargo_toml.find("[dependencies]") {
        let after_header = &cargo_toml[start..];
        // Find the next section header after [dependencies]
        if let Some(next) = after_header[1..]
            .find("\n[")
            .map(|pos| pos + 1)
        {
            &after_header[..next]
        } else {
            after_header
        }
    } else {
        return false;
    };

    deps_section.contains(&format!("{dep_name} ="))
        || deps_section.contains(&format!("{dep_name}="))
        || deps_section.contains(&format!("{dep_name}.workspace"))
}

/// Count occurrences of a pattern in source files.
///
/// Excludes `meaning_firewall.rs` itself to avoid counting string literals
/// in ratchet tests and doc comments as false positives.
fn count_imports_in_crate(crate_name: &str, pattern: &str) -> usize {
    let mut count = 0;
    for file in list_rust_files(crate_name) {
        // Skip this file — it contains the pattern strings as test literals
        if file.file_name().is_some_and(|n| n == "meaning_firewall.rs") {
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
    /// Current state (2026-02-15):
    /// - icn-gossip: CLEAN
    /// - icn-net: CLEAN
    /// - icn-gateway: 2 (icn-trust + icn-governance in prod deps)
    /// - icn-ledger: CLEAN (icn-trust + icn-governance are dev-deps only)
    #[test]
    fn strict_cargo_dependency_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-gossip", 0),  // CLEAN ✅
            ("icn-net", 0),     // CLEAN ✅
            ("icn-gateway", 2), // icn-trust + icn-governance
            ("icn-ledger", 0),  // CLEAN ✅ (domain crates are dev-deps only)
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
    /// Current state (2026-01-30):
    /// - icn-gossip: CLEAN
    /// - icn-net: CLEAN (no source imports, only had Cargo.toml dev-dep)
    /// - icn-gateway: 3 (trust_mgr.rs:2, api/trust.rs:1)
    /// - icn-ledger: 3 (credit_policy.rs:1, ledger.rs:1, fork_resolution.rs:1)
    #[test]
    fn strict_trust_import_violations() {
        let expected: &[(&str, usize)] = &[
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
    /// Current state (2026-01-30):
    /// - icn-net: CLEAN ✅
    /// - icn-gossip: CLEAN ✅
    /// - icn-ledger: CLEAN ✅
    /// - icn-gateway: 17 (governance admin endpoints — Phase 4 work)
    #[test]
    fn strict_governance_import_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-net", 0),      // CLEAN ✅
            ("icn-gossip", 0),   // CLEAN ✅
            ("icn-ledger", 0),   // CLEAN ✅
            ("icn-gateway", 20), // Phase 4 governance extraction pending (P0: +3 for receipt types)
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

    /// Pinned count of ALL `icn_governance::` references (imports + inline) in icn-gateway.
    ///
    /// This ratchet captures both `use icn_governance::` import statements AND
    /// inline qualified paths like `icn_governance::ProposalPayload`. Together with
    /// `strict_governance_import_violations`, this prevents governance surface from
    /// growing even if developers avoid adding `use` statements.
    ///
    /// Target state: 0 after gateway governance extraction (Phase 4).
    ///
    /// Current state (2026-02-10):
    /// - api/governance.rs: 54 (proposal handlers, membership, voting)
    /// - governance_mgr.rs: 5 (manager implementation)
    /// - commons_store.rs: 5 (commons governance)
    /// - commons_mgr.rs: 3 (commons manager)
    /// - Other scattered: 10 (entity, treasury, steward, constitutional)
    #[test]
    fn strict_gateway_governance_total_refs() {
        let expected: usize = 81; // P0: +4 for GovernanceDecisionReceipt, ProofOutcome, VoteTally, GovernanceProofV2
        let actual = count_imports_in_crate("icn-gateway", "icn_governance::");

        assert!(
            actual <= expected,
            "REGRESSION: icn-gateway has {actual} `icn_governance::` references \
             (expected at most {expected}). \
             Do not add new governance references to icn-gateway — \
             extract to gateway DTOs or use kernel-api types instead."
        );

        if actual < expected {
            panic!(
                "✅ PROGRESS: icn-gateway now has only {actual} `icn_governance::` \
                 references (was pinned at {expected}). Update the pinned count in \
                 meaning_firewall.rs::strict_gateway_governance_total_refs()."
            );
        }
    }

    // ===================================================================
    // ICN-CORE DOMAIN DEPENDENCY RATCHETS
    //
    // icn-core is the supervisor/wiring crate. It currently imports domain
    // crates directly (icn-ledger, icn-ccl, icn-governance) for actor
    // initialization. These ratchets pin the current import counts to
    // prevent regressions while incremental extraction proceeds.
    //
    // Goal: icn-core should wire actors through traits/handles only,
    // never importing domain types directly.
    // ===================================================================

    /// Domain crates that icn-core should eventually stop depending on.
    /// These are crates whose types leak into the supervisor/wiring layer.
    const CORE_DOMAIN_DEPS: &[&str] = &["icn-ledger", "icn-ccl", "icn-governance"];

    /// Pinned count of ALL `icn_ledger::` references in icn-core source.
    ///
    /// Tracks both `use icn_ledger::` import statements AND inline qualified
    /// paths like `icn_ledger::Ledger`. This gives a complete picture of
    /// coupling between icn-core and icn-ledger.
    ///
    /// Target state: 0 after ledger extraction completes (#914).
    ///
    /// Current state (2026-02-11):
    /// - services/ledger_service.rs: Legitimate composition root for LedgerService
    /// - lifecycle.rs: ledger_handle fn signature
    /// - init_compute.rs: LedgerHandle alias, JournalEntryBuilder
    /// - init_notifications.rs: LedgerHandle alias
    /// - init_rpc.rs: Ledger, DisputeManager imports
    /// - actors.rs: CoreActorHandles ledger types
    ///
    /// governance_handlers/ has been DELETED (Sprint 4 migration complete).
    ///
    /// Tracked for extraction in #914 (ledger extraction).
    #[test]
    fn strict_core_ledger_reference_ratchet() {
        // Note: services/ledger_service.rs legitimately imports icn_ledger types
        // to implement LedgerService trait. This is the composition root.
        let expected: usize = 12;
        let actual = count_imports_in_crate("icn-core", "icn_ledger::");

        assert!(
            actual <= expected,
            "REGRESSION: icn-core has {actual} `icn_ledger::` references \
             (expected at most {expected}). \
             Do not add new icn-ledger references to icn-core — \
             use kernel-api traits or BootstrapHandles instead. \
             See docs/architecture/KERNEL_APP_SEPARATION.md for extraction guidance."
        );

        if actual < expected {
            panic!(
                "✅ PROGRESS: icn-core now has only {actual} `icn_ledger::` references \
                 (was pinned at {expected}). Update the pinned count in \
                 meaning_firewall.rs::strict_core_ledger_reference_ratchet()."
            );
        }
    }

    /// Pinned count of ALL `icn_ccl::` references in icn-core source.
    ///
    /// Target state: 0 after CCL extraction completes.
    ///
    /// Current state (2026-01-31):
    /// - init_compute.rs: 6 (DisputeActorHandle, ContractRegistryHandle, DisputeConfig, etc.)
    /// - init_notifications.rs: 16 (ContractActor, topics, message types, dispute outcomes)
    /// - init_contract_registry.rs: 1 (use statement)
    /// - init_rpc.rs: 1 (ContractRuntime import)
    /// - lifecycle.rs: 8 (raw_handle extractions, fn signatures, holder types)
    /// - actors.rs: 2 (BootstrapHandles placeholder — from B3 when merged)
    #[test]
    fn strict_core_ccl_reference_ratchet() {
        let expected: usize = 32;
        let actual = count_imports_in_crate("icn-core", "icn_ccl::");

        assert!(
            actual <= expected,
            "REGRESSION: icn-core has {actual} `icn_ccl::` references \
             (expected at most {expected}). \
             Do not add new icn-ccl references to icn-core — \
             use kernel-api traits or BootstrapHandles instead. \
             See docs/architecture/KERNEL_APP_SEPARATION.md for extraction guidance."
        );

        if actual < expected {
            panic!(
                "✅ PROGRESS: icn-core now has only {actual} `icn_ccl::` references \
                 (was pinned at {expected}). Update the pinned count in \
                 meaning_firewall.rs::strict_core_ccl_reference_ratchet()."
            );
        }
    }

    /// Pinned count of ALL governance-crate references in icn-core source.
    ///
    /// Tracks both import statements AND inline qualified paths.
    /// Consistent with ledger/CCL ratchets.
    ///
    /// Target state: 0 after governance extraction to apps/governance (#913).
    ///
    /// Current state (2026-02-14):
    /// CLEAN - all governance-crate references eliminated from icn-core.
    /// - actors.rs: uses gateway governance handle type alias
    /// - init_gateway.rs: uses gateway governance handle type alias
    /// - control_service.rs: uses crate::governance re-exports from actor crate
    ///
    /// All proposal execution routes through the effect path.
    #[test]
    fn strict_core_governance_reference_ratchet() {
        let expected: usize = 0; // CLEAN: all references routed through actor crate or gateway
        let actual = count_imports_in_crate("icn-core", "icn_governance::");

        assert!(
            actual <= expected,
            "REGRESSION: icn-core has {actual} `icn_governance::` references \
             (expected at most {expected}). \
             Do not add new icn-governance references to icn-core — \
             use kernel-api traits or BootstrapHandles instead. \
             See docs/architecture/KERNEL_APP_SEPARATION.md for extraction guidance."
        );

        if actual < expected {
            panic!(
                "✅ PROGRESS: icn-core now has only {actual} `icn_governance::` \
                 references (was pinned at {expected}). Update the pinned count in \
                 meaning_firewall.rs::strict_core_governance_reference_ratchet()."
            );
        }
    }

    /// Pinned Cargo.toml domain dependency count for icn-core.
    ///
    /// icn-core currently depends on 3 domain crates (icn-ledger, icn-ccl,
    /// icn-governance). As extraction proceeds, these should be removed.
    ///
    /// Target state: 0 after all domain crate extraction completes.
    #[test]
    fn strict_core_cargo_domain_deps() {
        let cargo_toml = read_cargo_toml("icn-core").expect("Could not read icn-core/Cargo.toml");

        let mut actual = 0;
        for domain_crate in CORE_DOMAIN_DEPS {
            if has_dependency(&cargo_toml, domain_crate) {
                actual += 1;
            }
        }

        let expected: usize = 2; // icn-ledger + icn-ccl (icn-governance moved to dev-deps only)

        assert!(
            actual <= expected,
            "REGRESSION: icn-core has {actual} domain-crate Cargo.toml deps \
             (expected at most {expected}). \
             Do not add new domain deps to icn-core. \
             See docs/architecture/KERNEL_APP_SEPARATION.md for extraction guidance."
        );

        if actual < expected {
            panic!(
                "✅ PROGRESS: icn-core now has only {actual} domain-crate deps \
                 (was pinned at {expected}). Update the pinned count in \
                 meaning_firewall.rs::strict_core_cargo_domain_deps()."
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

        // Kernel crate violations (domain deps in kernel crates)
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
                import_violations.push(format!("{crate_name}: {count} icn_trust imports"));
            }
        }

        println!(
            "Kernel crate Cargo.toml violations: {}",
            cargo_violations.len()
        );
        for v in &cargo_violations {
            println!("  - {v}");
        }

        println!(
            "\nKernel crate import violations: {}",
            import_violations.len()
        );
        for v in &import_violations {
            println!("  - {v}");
        }

        // icn-core domain coupling
        println!("\n--- icn-core domain coupling ---");
        let ledger_refs = count_imports_in_crate("icn-core", "icn_ledger::");
        let ccl_refs = count_imports_in_crate("icn-core", "icn_ccl::");
        let gov_refs = count_imports_in_crate("icn-core", "icn_governance::");
        println!("  icn_ledger:: references: {ledger_refs}");
        println!("  icn_ccl:: references: {ccl_refs}");
        println!("  icn_governance:: references: {gov_refs}");

        let is_clean = cargo_violations.is_empty()
            && import_violations.is_empty()
            && ledger_refs == 0
            && ccl_refs == 0
            && gov_refs == 0;
        println!(
            "\nFirewall status: {}",
            if is_clean {
                "CLEAN"
            } else {
                "VIOLATIONS DETECTED (expected until kernel cleanup completes)"
            }
        );
    }
}
