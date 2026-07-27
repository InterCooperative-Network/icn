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

// Crate classes below MIRROR scripts/firewall-taxonomy.toml (the single source
// of truth). .github/scripts/test_firewall_taxonomy.py fails CI if these
// constants drift from the taxonomy — edit the taxonomy first.

/// Kernel-class crates that must not depend on domain crates.
const KERNEL_CRATES: &[&str] = &[
    "icn-core",
    "icn-net",
    "icn-gossip",
    "icn-store",
    "icn-kernel-api",
];

/// Domain-class crates that kernel crates must not depend on directly.
const DOMAIN_CRATES: &[&str] = &[
    "icn-trust",
    "icn-governance",
    "icn-ledger",
    "icn-ccl",
    "icn-compute",
    "icn-entity",
    "icn-community",
    "icn-federation",
    "icn-steward",
    "icn-coop",
    "icn-commons",
    "icn-zkp",
];

/// API-shell crates: HTTP/RPC hosting layers. Their domain coupling is
/// EXPECTED during the kernel/app migration and is pinned exactly (Cargo pins
/// in the taxonomy `[shells]`; import pins in the shell ratchets below).
#[allow(dead_code)]
const API_SHELL_CRATES: &[&str] = &["icn-gateway", "icn-rpc", "icn-api"];

/// App crates (apps/): the kernel must not depend on applications. Current
/// violations are pinned in `strict_core_app_crate_deps` (migration B10).
const APP_CRATES: &[&str] = &[
    "icn-charter-app",
    "icn-governance-actor",
    "icn-governance-app",
    "icn-ledger-actor",
    "icn-ledger-app",
    "icn-membership-app",
    "icn-trust-app",
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
/// Uses proper TOML parsing to inspect only `[dependencies]`, excluding
/// `[dev-dependencies]` and `[build-dependencies]`. Also handles
/// `[dependencies.crate_name]` sub-table syntax and `[target.*.dependencies]`.
fn has_dependency(cargo_toml: &str, dep_name: &str) -> bool {
    let parsed: toml::Table = match toml::from_str(cargo_toml) {
        Ok(v) => v,
        Err(_) => return false,
    };

    // Check [dependencies] table
    if let Some(toml::Value::Table(deps)) = parsed.get("dependencies") {
        if deps.contains_key(dep_name) {
            return true;
        }
    }

    // Check [target.*.dependencies] tables
    if let Some(toml::Value::Table(targets)) = parsed.get("target") {
        for target_cfg in targets.values() {
            if let Some(toml::Value::Table(deps)) = target_cfg.get("dependencies") {
                if deps.contains_key(dep_name) {
                    return true;
                }
            }
        }
    }

    false
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
    /// Current state (2026-07-22, taxonomy reconciliation):
    /// - icn-net / icn-gossip / icn-store / icn-kernel-api: CLEAN against all
    ///   12 domain crates.
    /// - icn-core: carries pinned domain deps — covered by
    ///   `strict_core_cargo_domain_deps` (finer-grained), excluded here.
    /// - icn-gateway/icn-rpc/icn-api are API-SHELL class, not kernel: their
    ///   Cargo domain deps are pinned in scripts/firewall-taxonomy.toml
    ///   `[shells]` and enforced by firewall_denylist.py.
    /// - icn-ledger is DOMAIN class (2026-07-22 reclassification) — no longer
    ///   scanned as kernel. Public evidence: both CI dependency gates already
    ///   denylisted it; its own trust/governance deps are dev-only; its public
    ///   surface is settlement/credit/treasury semantics (see the taxonomy
    ///   header's rules-of-change note).
    #[test]
    fn strict_cargo_dependency_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-gossip", 0),     // CLEAN ✅
            ("icn-net", 0),        // CLEAN ✅
            ("icn-store", 0),      // CLEAN ✅
            ("icn-kernel-api", 0), // CLEAN ✅
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
    /// Current state (2026-07-22, taxonomy reconciliation): ALL kernel-class
    /// crates are at ZERO trust imports (icn-core's remaining trust coupling is
    /// dev-dependency/test-only; `count_imports_in_crate` scans src/).
    /// icn-gateway's 3 imports moved to `strict_shell_import_violations`
    /// (api-shell class); icn-ledger is domain class and no longer scanned.
    #[test]
    fn strict_trust_import_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-core", 0), // CLEAN ✅ in src/ (trust is a dev-dep; its uses live in tests/, outside the scan)
            ("icn-gossip", 0), // CLEAN ✅
            ("icn-net", 0),  // CLEAN ✅
            ("icn-store", 0), // CLEAN ✅
            ("icn-kernel-api", 0), // CLEAN ✅
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
    /// Current state (2026-02-22, Sprint 14 easy sweep):
    /// - icn-net: CLEAN ✅
    /// - icn-gossip: CLEAN ✅
    /// - icn-ledger: CLEAN ✅
    /// - icn-gateway: 10 (governance_dashboard, flow_c, steward extracted; commons/treasury/entity residue)
    ///
    /// Current state (2026-03-30, governance legitimacy stack):
    /// - server.rs: +1 scoped `use icn_governance::` inside DeployCharter tokio::spawn handler
    ///   (Charter creation wiring — required until commons accepts primitive args; see #1456)
    /// - icn-gateway: 11
    ///
    /// Current state (2026-04-06, Tranche 9 TrustThreshold close-time enforcement):
    /// - server.rs: +1 scoped `use icn_governance::{MembershipResolver, TrustServiceMembershipResolver}`
    ///   for TrustThreshold membership resolver wiring at proposal close time (#1500)
    /// - icn-gateway: 12
    ///
    /// Current state (2026-04-20, ADR-0014 durable Mandate+AuthorityGrant storage):
    /// - receipt_store.rs: +5 `use icn_governance::{Mandate, MandateId, AuthorityGrant,
    ///   AuthorityGrantId, ...}` for sled-backed ADR-0014 authorization-side storage (#1575)
    /// - icn-gateway: 17
    ///
    /// Current state (2026-07-22, taxonomy reconciliation): kernel class is at
    /// ZERO governance imports across the board (icn-core reached 0 earlier —
    /// see `strict_core_governance_reference_ratchet`). icn-gateway's 17 moved
    /// to `strict_shell_import_violations` (api-shell class); icn-ledger is
    /// domain class and no longer scanned here.
    #[test]
    fn strict_governance_import_violations() {
        let expected: &[(&str, usize)] = &[
            ("icn-core", 0),       // CLEAN ✅
            ("icn-net", 0),        // CLEAN ✅
            ("icn-gossip", 0),     // CLEAN ✅
            ("icn-store", 0),      // CLEAN ✅
            ("icn-kernel-api", 0), // CLEAN ✅
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
    /// Current state (2026-02-22, Sprint 14 easy sweep):
    /// - api/governance.rs: DELETED ✅ (was 54 refs)
    /// - governance_mgr.rs: REPLACED with re-export ✅ (was 5 refs)
    /// - governance_dashboard.rs: CLEANED ✅ (was 1 ref)
    /// - flow_c.rs: CLEANED ✅ (was 1 ref)
    /// - steward/mod.rs: CLEANED ✅ (was 1 ref)
    /// - models.rs comment: CLEANED ✅ (was 1 ref)
    /// - receipt_store.rs test: CLEANED ✅ (was 1 ref)
    /// - commons_store.rs tests: consolidated ✅ (-2 refs)
    /// - Hard residue: 16 (commons_mgr, commons_store, treasury, entity, receipts, constitutional)
    ///
    /// Current state (2026-03-28, Tranche 4 boundary-semantics):
    /// - constitutional/mod.rs: charter/domain validation refactored ✅ (-2 refs)
    /// - Hard residue: 14 (commons_mgr, commons_store, treasury, entity, receipts, constitutional)
    ///
    /// Current state (2026-03-30, governance legitimacy stack):
    /// - server.rs: +1 `use icn_governance::` inside DeployCharter tokio::spawn handler (#1456)
    ///   Charter creation wiring — pending extraction to commons primitive API
    /// - Hard residue: 15
    ///
    /// Current state (2026-04-06, Tranche 9 TrustThreshold close-time enforcement):
    /// - server.rs: +1 `use icn_governance::{MembershipResolver, TrustServiceMembershipResolver}`
    ///   for TrustThreshold membership resolver wiring at proposal close time (#1500)
    ///   TrustManagerMembershipResolver removed from trust_mgr.rs (moved to tests/ only)
    /// - Hard residue: 16
    ///
    /// Current state (2026-04-20, ADR-0014 durable Mandate+AuthorityGrant storage):
    /// - receipt_store.rs: +6 references for sled-backed ADR-0014 authorization-side storage —
    ///   Mandate, MandateId, AuthorityGrant, AuthorityGrantId primary + secondary index writes
    ///   plus atomic put_mandate_with_grants_atomic transaction (#1575). These are durable
    ///   governance artifacts; extraction to gateway DTOs would duplicate the canonical types.
    /// - Hard residue: 22
    #[test]
    fn strict_gateway_governance_total_refs() {
        let expected: usize = 22; // +6 ADR-0014 Mandate/AuthorityGrant storage in receipt_store.rs (#1575)
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
    /// (Full DOMAIN_CRATES class since 2026-07-22 — previously only 3 of the
    /// 8 live edges were tracked; compute/federation/coop/steward/entity/
    /// community/commons had NO ratchet coverage.)
    const CORE_DOMAIN_DEPS: &[&str] = DOMAIN_CRATES;

    /// Pinned count of ALL `icn_ledger::` references in icn-core source.
    ///
    /// Tracks both `use icn_ledger::` import statements AND inline qualified
    /// paths like `icn_ledger::Ledger`. This gives a complete picture of
    /// coupling between icn-core and icn-ledger.
    ///
    /// Target state: 0 after ledger extraction completes (#914).
    ///
    /// Current state (2026-04-04):
    /// - actors.rs: 3 refs — consolidated type aliases (LedgerHandle, DisputeManagerHandle, TreasuryManagerHandle)
    /// - services/ledger_service.rs: 1 ref — composition root for LedgerService
    /// - src/bin/ledger_restart_helper.rs: 1 ref — Layer 4 cross-process
    ///   persistence proof helper; test infrastructure only.
    /// - init_compute.rs: 16 refs — settlement engine construction + commons settlement
    ///   callback + journal entry building. These are the legitimate compute→ledger
    ///   wiring points that remain until #914 (ledger extraction).
    ///
    /// Boundary improvement (2026-04-04): passthrough structs (GatewayActorHandles,
    /// GatewayHandles, ComputeServices) no longer hold concrete `icn_ledger::SettlementEngine`.
    /// They now use `Arc<dyn icn_kernel_api::services::SettlementQueryService>` — reducing
    /// the count from 24 → 21. Settlement query result types moved to icn-kernel-api.
    ///
    /// Remaining hotspots out of scope here:
    /// - init_compute.rs construction of SettlementEngine (must be concrete at creation)
    /// - init_compute.rs commons_credits references (ledger domain, no kernel-api equivalent)
    /// - actors.rs type aliases (BootstrapHandles, pre-existing, requires full extraction)
    ///
    /// Tracked for extraction in #914 (ledger extraction).
    #[test]
    fn strict_core_ledger_reference_ratchet() {
        // actors.rs: 3 refs (LedgerHandle, DisputeManagerHandle, TreasuryManagerHandle type aliases).
        // ledger_service.rs: 1 ref — composition root.
        // bin/ledger_restart_helper.rs: 1 ref — test infrastructure.
        //
        // init_compute.rs: 0 refs — factory functions (balance/payment/commons settlement) and
        //   settlement engine construction moved to bins/icnd/src/compute_wiring.rs (daemon
        //   composition root). Callbacks are injected via BootstrapHandles as type-erased
        //   icn_compute callback types, keeping the kernel/app boundary intact.
        // Passthrough structs (GatewayActorHandles, GatewayHandles, ComputeServices)
        //   use Arc<dyn SettlementQueryService> — concrete SettlementEngine fully removed.
        let expected: usize = 5;
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
    /// Now measured against the FULL domain class (12 crates). icn-core's
    /// production deps as of 2026-07-23 (post-B0): icn-ledger, icn-ccl,
    /// icn-compute, icn-entity, icn-federation, icn-steward, icn-coop,
    /// icn-commons (8). icn-community was removed by migration B0 (construction
    /// moved to the daemon composition root). icn-trust/icn-governance are
    /// dev-deps only; icn-zkp is transitive-only. Each remaining edge has an
    /// [[exception]] in scripts/firewall-taxonomy.toml with an edge-absent expiry.
    ///
    /// Target state: 0 after all domain crate extraction completes (Phases B0–B9).
    #[test]
    fn strict_core_cargo_domain_deps() {
        let cargo_toml = read_cargo_toml("icn-core").expect("Could not read icn-core/Cargo.toml");

        let mut actual = 0;
        for domain_crate in CORE_DOMAIN_DEPS {
            if has_dependency(&cargo_toml, domain_crate) {
                actual += 1;
            }
        }

        let expected: usize = 8; // ledger,ccl,compute,entity,federation,steward,coop,commons (community removed, B0)

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

    /// Pinned count of ALL references for the icn-core domain edges that had
    /// NO ratchet coverage before 2026-07-22 (214 references could grow
    /// silently). Same semantics as the ledger/ccl/governance ratchets:
    /// counts `use` statements AND inline qualified paths, src/ only,
    /// excluding this file.
    ///
    /// Baselines measured 2026-07-22 at 767ece63. Target: 0 per edge as the
    /// corresponding migration tranche (B0–B9) lands.
    ///
    /// icn_community:: reached 0 on 2026-07-23 (migration B0 complete —
    /// construction moved to the daemon composition root; kept pinned at 0
    /// as a permanent regression guard, matching the ledger/ccl/governance
    /// precedent, rather than removed from this array).
    #[test]
    fn strict_core_remaining_domain_reference_ratchets() {
        let expected: &[(&str, usize)] = &[
            ("icn_compute::", 86),    // migration B8
            ("icn_federation::", 48), // migration B6
            ("icn_coop::", 34),       // migration B5
            ("icn_steward::", 32),    // migration B4
            ("icn_entity::", 14),     // migration B3
            ("icn_commons::", 10),    // migration B9
            ("icn_community::", 0),   // migration B0 COMPLETE (2026-07-23)
        ];

        for &(pattern, expected_count) in expected {
            let actual = count_imports_in_crate("icn-core", pattern);

            assert!(
                actual <= expected_count,
                "REGRESSION: icn-core has {actual} `{pattern}` references \
                 (expected at most {expected_count}). \
                 Do not add new domain references to icn-core — \
                 use kernel-api traits or BootstrapHandles instead. \
                 See docs/architecture/KERNEL_APP_SEPARATION.md for extraction guidance."
            );

            if actual < expected_count {
                panic!(
                    "✅ PROGRESS: icn-core now has only {actual} `{pattern}` references \
                     (was pinned at {expected_count}). Update the pinned count in \
                     meaning_firewall.rs::strict_core_remaining_domain_reference_ratchets()."
                );
            }
        }
    }

    /// Pinned Cargo.toml APP-crate dependency count for icn-core.
    ///
    /// The kernel must not depend on applications; these edges were on NO
    /// denylist before 2026-07-22 (undetectable bypass, now closed). Each has
    /// an [[exception]] in scripts/firewall-taxonomy.toml.
    ///
    /// Current: icn-trust-app, icn-governance-actor, icn-ledger-actor (3).
    /// Target: 0 after migration B10 (composition moves to bins/icnd).
    #[test]
    fn strict_core_app_crate_deps() {
        let cargo_toml = read_cargo_toml("icn-core").expect("Could not read icn-core/Cargo.toml");

        let mut actual = 0;
        for app_crate in APP_CRATES {
            if has_dependency(&cargo_toml, app_crate) {
                actual += 1;
            }
        }

        let expected: usize = 3; // icn-trust-app + icn-governance-actor + icn-ledger-actor

        assert!(
            actual <= expected,
            "REGRESSION: icn-core has {actual} app-crate Cargo.toml deps \
             (expected at most {expected}). \
             The kernel must not gain new dependencies on apps/ crates — \
             register apps from the composition root (bins/icnd) instead."
        );

        if actual < expected {
            panic!(
                "✅ PROGRESS: icn-core now has only {actual} app-crate deps \
                 (was pinned at {expected}). Update the pinned count in \
                 meaning_firewall.rs::strict_core_app_crate_deps()."
            );
        }
    }

    /// Pinned domain-import counts for API-SHELL crates (icn-gateway, icn-rpc,
    /// icn-api). Shells host domain routes during the transition, so their
    /// coupling is EXPECTED — but pinned exactly so it can only shrink.
    /// Compensates for the edit hook no longer treating icn-gateway as kernel
    /// (2026-07-22 taxonomy reconciliation).
    ///
    /// Baselines measured 2026-07-22 at 767ece63.
    #[test]
    fn strict_shell_import_violations() {
        let expected: &[(&str, &str, usize)] = &[
            ("icn-gateway", "use icn_trust::", 3), // trust_mgr.rs:2, api/trust.rs:1
            ("icn-gateway", "use icn_governance::", 17), // see strict_governance history above
            ("icn-rpc", "use icn_trust::", 0),
            ("icn-rpc", "use icn_governance::", 2),
            ("icn-api", "use icn_trust::", 0),
            ("icn-api", "use icn_governance::", 1),
        ];

        for &(crate_name, pattern, expected_count) in expected {
            let actual = count_imports_in_crate(crate_name, pattern);

            assert!(
                actual <= expected_count,
                "REGRESSION: {crate_name} has {actual} `{pattern}` imports \
                 (expected at most {expected_count}). \
                 API-shell domain coupling may only shrink — new handlers belong \
                 in apps/ crates mounted as route plugins."
            );

            if actual < expected_count {
                panic!(
                    "✅ PROGRESS: {crate_name} now has only {actual} `{pattern}` \
                     imports (was pinned at {expected_count}). Update the pinned count in \
                     meaning_firewall.rs::strict_shell_import_violations()."
                );
            }
        }
    }

    /// Pinned DOMAIN-TOKEN counts inside icn-kernel-api (the boundary contract
    /// crate itself). icn-kernel-api was scanned by NOTHING before 2026-07-22,
    /// while natively defining a domain service layer (services.rs) — domain
    /// meaning that no Cargo-dependency check can see.
    ///
    /// These pins are the honest baseline for the kernel-api purification
    /// (migration Phase A2/F; layer-contract ADR pending — HD-9). Counts are
    /// raw token occurrences in icn-kernel-api/src, measured 2026-07-22.
    /// They may only DECREASE; new domain vocabulary in kernel-api fails here.
    #[test]
    fn kernel_api_domain_surface_ratchet() {
        let expected: &[(&str, usize)] = &[
            ("TrustClass", 18),              // native enum + thresholds (services.rs)
            ("MembershipService", 2),        // domain service trait
            ("FederationService", 6),        // domain service trait
            ("SdisService", 3),              // domain service trait
            ("GovernanceService", 5),        // domain service trait
            ("LedgerService", 8),            // domain service trait
            ("ControlService", 2),           // domain service trait
            ("TreasuryOperationType", 28),   // operation enum driving match logic
            ("FederationOperationType", 10), // operation enum
            ("AssetType", 65),               // operation enum
        ];

        for &(token, expected_count) in expected {
            let actual = count_imports_in_crate("icn-kernel-api", token);

            assert!(
                actual <= expected_count,
                "REGRESSION: icn-kernel-api has {actual} `{token}` occurrences \
                 (expected at most {expected_count}). \
                 The kernel contract crate must not GROW domain vocabulary — \
                 domain service contracts belong in per-app api crates."
            );

            if actual < expected_count {
                panic!(
                    "✅ PROGRESS: icn-kernel-api now has only {actual} `{token}` \
                     occurrences (was pinned at {expected_count}). Update the pinned \
                     count in meaning_firewall.rs::kernel_api_domain_surface_ratchet()."
                );
            }
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
        // NOTE (2026-07-22): this test previously scanned for the literal
        // "pub struct TrustClass" — a FALSE NEGATIVE, because the real
        // definition is `pub enum TrustClass` (services.rs). TrustClass and
        // the rest of kernel-api's existing domain surface are now honestly
        // pinned (shrink-only) in `kernel_api_domain_surface_ratchet`; this
        // test keeps only the absolute never-present assertions.
        for file in list_rust_files("icn-kernel-api") {
            if let Ok(content) = std::fs::read_to_string(&file) {
                for pattern in &["struct TrustGraph", "enum TrustGraph", "GovernanceRules"] {
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
