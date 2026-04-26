#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Store-isolation tests for ADR-0013 federation clearing adoption.
//!
//! These tests prove that ADR-0012 Model C (Explicit Parallel) and ADR-0013
//! Model B (Ratified Re-instantiation) are not silently violated by the current
//! implementation:
//!
//! - The gateway-local "direct-management" clearing path and the supervisor-owned
//!   "governance" clearing path keep their state in **separate stores**. Each
//!   `ClearingManager` is backed by its own `Arc<dyn Store>`. There is no shared
//!   tree, no implicit cross-store sync.
//!
//! - When a direct-management agreement is "adopted" through governance, the
//!   adoption produces a NEW canonical record in the supervisor store. The new
//!   record carries `source_agreement_id` pointing back to the gateway-local
//!   source. The source record is unchanged. No positions are silently copied.
//!
//! - Position writes against one store do not appear in the other. Lists are
//!   disjoint. `source_agreement_id` survives the persistence round-trip.
//!
//! ## Why ClearingManager-level isolation is the right substrate
//!
//! In production, `FederationManager` (gateway, direct-management) and
//! `FederationServiceImpl` (supervisor, governance) each construct their own
//! `ClearingManager` against their own Sled tree (`data_dir/federation_store`
//! vs `store_path/clearing`). The store-isolation invariant is therefore a
//! property of the `ClearingManager` + `Store` composition: two managers, two
//! stores, no shared state.
//!
//! These tests construct that composition directly. They do not require a
//! running daemon, supervisor, or gateway HTTP layer. If they fail, the
//! invariant is broken at the substrate level and every higher-layer test that
//! depends on dual-path semantics is suspect.
//!
//! ## Closes
//!
//! - icn#1645
//! - ADR-0013 Phase 5 store-isolation verification gap

use icn_federation::{
    BilateralClearingAgreement, ClearingManager, CrossCoopTransfer, SettlementInterval,
};
use icn_identity::{Did, KeyPair};
use icn_store::{SledStore, Store};
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn fresh_store() -> Arc<dyn Store> {
    Arc::new(SledStore::temporary().unwrap())
}

fn fresh_did() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

/// Build a `ClearingManager` that stands in for one side of the dual-path model.
///
/// The `coop_id` matches the role of the manager: `"gateway-coop"` for the
/// direct-management path, `"supervisor-coop"` for the governance path. The
/// store is fresh per call, so two callers receive two isolated managers.
fn fresh_manager(coop_id: &str) -> ClearingManager {
    ClearingManager::new(fresh_store(), coop_id.to_string()).unwrap()
}

/// Build a bilateral agreement with both coop pair info.
///
/// `source_agreement_id` is `None` by default — this matches the direct-management
/// path. The supervisor-owned adoption records set it explicitly via the
/// public field after construction.
fn build_agreement(
    agreement_id: &str,
    coop_a: &str,
    coop_a_did: Did,
    coop_b: &str,
    coop_b_did: Did,
) -> BilateralClearingAgreement {
    BilateralClearingAgreement::new(
        agreement_id.to_string(),
        coop_a.to_string(),
        coop_a_did,
        coop_b.to_string(),
        coop_b_did,
    )
    .with_rate("hours", "hours", 1.0)
    .with_interval(SettlementInterval::Weekly)
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Cross-store invisibility
// ─────────────────────────────────────────────────────────────────────────────

/// A direct-management agreement written to the gateway-side manager is invisible
/// to the supervisor-side manager. Adoption does not silently pull the same
/// record across the boundary; the supervisor store has no knowledge of the
/// direct-management agreement_id.
#[test]
fn direct_management_agreement_invisible_in_supervisor_store() {
    let gateway_mgr = fresh_manager("gateway-coop");
    let supervisor_mgr = fresh_manager("supervisor-coop");

    let dm_id = "agr-dm-001";
    let coop_a_did = fresh_did();
    let coop_b_did = fresh_did();

    gateway_mgr
        .create_agreement(build_agreement(
            dm_id,
            "food-coop",
            coop_a_did.clone(),
            "tech-coop",
            coop_b_did.clone(),
        ))
        .unwrap();

    // Gateway side sees the record.
    let from_gateway = gateway_mgr.get_agreement(dm_id).unwrap();
    assert!(
        from_gateway.is_some(),
        "direct-management agreement must be present in gateway-side store"
    );

    // Supervisor side does NOT see it. The two stores are isolated; there is no
    // implicit promotion path that would make a direct-management write visible
    // in the supervisor canonical store. ADR-0011/0012 forbid this.
    let from_supervisor = supervisor_mgr.get_agreement(dm_id).unwrap();
    assert!(
        from_supervisor.is_none(),
        "direct-management agreement must NOT appear in supervisor store; \
         a Some(_) here means the dual-path model has collapsed"
    );

    // List on the supervisor side is empty — no leakage of any kind.
    assert!(
        supervisor_mgr.list_agreements().is_empty(),
        "supervisor store must be empty; got {} record(s)",
        supervisor_mgr.list_agreements().len()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Adoption produces a distinct canonical record
// ─────────────────────────────────────────────────────────────────────────────

/// Governance adoption (ADR-0013 Model B — Ratified Re-instantiation) produces
/// a NEW supervisor-owned record with a new agreement_id. The source remains in
/// the gateway store, untouched. The supervisor record carries
/// `source_agreement_id` pointing back. The two records have different IDs, so
/// they are independently addressable; the supervisor record is not the same
/// object as the source.
#[test]
fn governance_adoption_creates_distinct_canonical_record() {
    let gateway_mgr = fresh_manager("gateway-coop");
    let supervisor_mgr = fresh_manager("supervisor-coop");

    let dm_id = "agr-dm-001";
    let gov_id = "agr-gov-001";
    let coop_a_did = fresh_did();
    let coop_b_did = fresh_did();

    // Direct-management path: source agreement in the gateway store.
    gateway_mgr
        .create_agreement(build_agreement(
            dm_id,
            "food-coop",
            coop_a_did.clone(),
            "tech-coop",
            coop_b_did.clone(),
        ))
        .unwrap();

    // Adoption: governance proposal passes, supervisor creates a new canonical
    // record with a fresh agreement_id and source_agreement_id pointing back.
    // Per ADR-0013 §Phase 6 Q1: the canonical record uses a NEW id, not the
    // source id, to prevent split-brain.
    let mut adopted = build_agreement(
        gov_id,
        "food-coop",
        coop_a_did.clone(),
        "tech-coop",
        coop_b_did.clone(),
    );
    adopted.source_agreement_id = Some(dm_id.to_string());
    supervisor_mgr.create_agreement(adopted).unwrap();

    // Both records exist, on their respective sides.
    let dm = gateway_mgr.get_agreement(dm_id).unwrap().expect(
        "source direct-management record must remain in gateway store after adoption — \
         ADR-0011 forbids governance from mutating gateway state",
    );
    let canonical = supervisor_mgr
        .get_agreement(gov_id)
        .unwrap()
        .expect("adopted canonical record must be present in supervisor store");

    // They are distinct records.
    assert_ne!(
        dm.agreement_id, canonical.agreement_id,
        "ADR-0013 §Phase 6 Q1: the canonical record must have a NEW id distinct \
         from the source. Same-id risks split-brain across stores."
    );

    // The source has no source_agreement_id (it is itself the source).
    assert!(
        dm.source_agreement_id.is_none(),
        "the direct-management source record must not carry source_agreement_id; \
         it IS the source. Got: {:?}",
        dm.source_agreement_id
    );

    // The canonical record carries the source link.
    assert_eq!(
        canonical.source_agreement_id.as_deref(),
        Some(dm_id),
        "the supervisor-owned canonical record must carry source_agreement_id \
         pointing back to the direct-management source. Got: {:?}",
        canonical.source_agreement_id
    );

    // Cross-store lookups remain clean: the supervisor does not know about the
    // direct-management id, and the gateway does not know about the canonical
    // id. The records coexist; the stores do not.
    assert!(
        supervisor_mgr.get_agreement(dm_id).unwrap().is_none(),
        "supervisor must not see the direct-management agreement_id; the source \
         lives in the gateway store only"
    );
    assert!(
        gateway_mgr.get_agreement(gov_id).unwrap().is_none(),
        "gateway must not see the supervisor-owned canonical agreement_id; the \
         canonical record lives in the supervisor store only"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. source_agreement_id survives a store reload
// ─────────────────────────────────────────────────────────────────────────────

/// `source_agreement_id` is part of the persisted record. A supervisor restart
/// (modeled here as constructing a fresh `ClearingManager` over the same
/// underlying `Store`) must reload the field intact. Dropping it would silently
/// erase the adoption lineage and leave the canonical record indistinguishable
/// from a fresh governance-originated agreement.
#[test]
fn source_agreement_id_survives_store_reload() {
    let supervisor_store = fresh_store();
    let dm_id = "agr-dm-002";
    let gov_id = "agr-gov-002";

    {
        let supervisor_mgr =
            ClearingManager::new(supervisor_store.clone(), "supervisor-coop".to_string()).unwrap();

        let mut adopted =
            build_agreement(gov_id, "food-coop", fresh_did(), "tech-coop", fresh_did());
        adopted.source_agreement_id = Some(dm_id.to_string());
        supervisor_mgr.create_agreement(adopted).unwrap();
    }

    // Reload — same underlying store, fresh in-memory cache. This models a
    // supervisor restart: the canonical record must come back with its
    // adoption lineage intact.
    let reloaded = ClearingManager::new(supervisor_store, "supervisor-coop".to_string()).unwrap();
    let canonical = reloaded
        .get_agreement(gov_id)
        .unwrap()
        .expect("canonical record must reload from store");

    assert_eq!(
        canonical.source_agreement_id.as_deref(),
        Some(dm_id),
        "source_agreement_id must round-trip through serialization. Losing it \
         here would silently erase ADR-0013 adoption lineage on every restart."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Positions are isolated across stores
// ─────────────────────────────────────────────────────────────────────────────

/// Position writes against one manager do not appear in the other manager. The
/// underlying `Store` instances are different `Arc<dyn Store>`s with different
/// Sled trees; there is no shared backing for positions, transfers, or
/// agreements.
///
/// This test exercises the real transfer flow: `propose_transfer` writes to the
/// gateway-side store; `confirm_transfer` updates the gateway-side position.
/// The supervisor side, observing its own canonical (adopted) record, sees
/// only its own (zero) position.
#[test]
fn position_writes_isolated_across_stores() {
    let gateway_mgr = fresh_manager("food-coop");
    let supervisor_mgr = fresh_manager("supervisor-coop");

    let dm_id = "agr-dm-003";
    let gov_id = "agr-gov-003";

    let food_did = fresh_did();
    let tech_did = fresh_did();

    // Direct-management agreement on the gateway side.
    gateway_mgr
        .create_agreement(build_agreement(
            dm_id,
            "food-coop",
            food_did.clone(),
            "tech-coop",
            tech_did.clone(),
        ))
        .unwrap();

    // Governance-adopted canonical agreement on the supervisor side, linked
    // back via source_agreement_id.
    let mut adopted = build_agreement(
        gov_id,
        "food-coop",
        food_did.clone(),
        "tech-coop",
        tech_did.clone(),
    );
    adopted.source_agreement_id = Some(dm_id.to_string());
    supervisor_mgr.create_agreement(adopted).unwrap();

    // Push a real transfer through the gateway-side flow.
    let transfer = CrossCoopTransfer::new(
        "transfer-001".to_string(),
        "food-coop".to_string(),
        food_did.clone(),
        "tech-coop".to_string(),
        tech_did.clone(),
        500,
        "hours".to_string(),
        500,
        "hours".to_string(),
    );
    let transfer_id = gateway_mgr.propose_transfer(transfer).unwrap();
    gateway_mgr.confirm_transfer(&transfer_id).unwrap();

    // Gateway-side position reflects the transfer.
    let gateway_position = gateway_mgr.calculate_position(dm_id).unwrap();
    let net = gateway_position.net_position().abs();
    assert!(
        net > 0,
        "gateway-side position must reflect the confirmed transfer (got net = {net})",
    );

    // Supervisor-side canonical position is still zero. The transfer happened
    // in the gateway store; there is no implicit channel that would surface it
    // on the supervisor side.
    let supervisor_position = supervisor_mgr.calculate_position(gov_id).unwrap();
    assert_eq!(
        supervisor_position.coop_a_owes_b, 0,
        "supervisor canonical position must remain zero; gateway-side transfers \
         must not leak into the supervisor store"
    );
    assert_eq!(
        supervisor_position.coop_b_owes_a, 0,
        "supervisor canonical position must remain zero; gateway-side transfers \
         must not leak into the supervisor store"
    );
    assert!(
        supervisor_position.pending_transfers.is_empty(),
        "supervisor canonical record must have no pending transfers; got {} — \
         this would mean a transfer crossed the store boundary",
        supervisor_position.pending_transfers.len()
    );

    // Cross-id position lookups fail cleanly: each manager only knows its own
    // record's position.
    assert!(
        supervisor_mgr.calculate_position(dm_id).is_err(),
        "supervisor must reject position lookup for the direct-management id; \
         it has no record of that agreement"
    );
    assert!(
        gateway_mgr.calculate_position(gov_id).is_err(),
        "gateway must reject position lookup for the supervisor-owned canonical \
         id; it has no record of that agreement"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Adopted canonical record starts at a fresh position
// ─────────────────────────────────────────────────────────────────────────────

/// Adoption does not carry positions across stores. Per ADR-0013 §Phase 6 Q2,
/// positions start fresh for the canonical record; carrying them over would
/// blend two stores' state and violate the dual-path invariant.
#[test]
fn adopted_canonical_record_starts_with_fresh_position() {
    let gateway_mgr = fresh_manager("food-coop");
    let supervisor_mgr = fresh_manager("supervisor-coop");

    let dm_id = "agr-dm-004";
    let gov_id = "agr-gov-004";

    let food_did = fresh_did();
    let tech_did = fresh_did();

    // Direct-management agreement with several confirmed transfers, simulating
    // pre-existing operational history before adoption.
    gateway_mgr
        .create_agreement(build_agreement(
            dm_id,
            "food-coop",
            food_did.clone(),
            "tech-coop",
            tech_did.clone(),
        ))
        .unwrap();

    for i in 0..3 {
        let t = CrossCoopTransfer::new(
            format!("pre-adopt-transfer-{i:03}"),
            "food-coop".to_string(),
            food_did.clone(),
            "tech-coop".to_string(),
            tech_did.clone(),
            100,
            "hours".to_string(),
            100,
            "hours".to_string(),
        );
        let id = gateway_mgr.propose_transfer(t).unwrap();
        gateway_mgr.confirm_transfer(&id).unwrap();
    }

    // Sanity: gateway position is non-zero before adoption.
    let pre_adoption_position = gateway_mgr.calculate_position(dm_id).unwrap();
    assert!(
        pre_adoption_position.net_position().abs() > 0,
        "gateway position must accumulate transfers (preflight)"
    );

    // Adoption: supervisor mints a new canonical record. Per ADR-0013, no
    // positions cross the boundary — the canonical record's position starts at
    // zero with no pending transfers.
    let mut adopted = build_agreement(
        gov_id,
        "food-coop",
        food_did.clone(),
        "tech-coop",
        tech_did.clone(),
    );
    adopted.source_agreement_id = Some(dm_id.to_string());
    supervisor_mgr.create_agreement(adopted).unwrap();

    let canonical_position = supervisor_mgr.calculate_position(gov_id).unwrap();

    assert_eq!(
        canonical_position.coop_a_owes_b, 0,
        "canonical position must start fresh — silent inheritance from the \
         direct-management source would violate ADR-0013 Phase 6 Q2"
    );
    assert_eq!(
        canonical_position.coop_b_owes_a, 0,
        "canonical position must start fresh — silent inheritance from the \
         direct-management source would violate ADR-0013 Phase 6 Q2"
    );
    assert!(
        canonical_position.pending_transfers.is_empty(),
        "canonical record must have no pending transfers at adoption; got {} — \
         that would mean transfers leaked across stores",
        canonical_position.pending_transfers.len()
    );

    // Source position is unchanged by the adoption — gateway state is not
    // mutated by governance execution (ADR-0011).
    let post_adoption_source_position = gateway_mgr.calculate_position(dm_id).unwrap();
    assert_eq!(
        post_adoption_source_position.net_position(),
        pre_adoption_position.net_position(),
        "direct-management source position must be unchanged by adoption \
         (ADR-0011: governance must not mutate gateway state)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. List surfaces are disjoint
// ─────────────────────────────────────────────────────────────────────────────

/// `list_agreements` on each manager returns only that manager's own records.
/// This is the dual-path invariant at the read surface: a caller asking the
/// gateway "which clearing agreements do you have?" must get back gateway-side
/// records only; the same question to the supervisor must get back
/// supervisor-side records only.
///
/// Origin labeling at the API read surface (gateway/src/api/federation.rs marks
/// each path explicitly: `origin = "direct-management"` vs `origin =
/// "governance"`) is built on top of this substrate-level disjointness.
#[test]
fn list_surfaces_are_disjoint_across_stores() {
    let gateway_mgr = fresh_manager("gateway-coop");
    let supervisor_mgr = fresh_manager("supervisor-coop");

    // Two unrelated direct-management agreements.
    for id in ["agr-dm-101", "agr-dm-102"] {
        gateway_mgr
            .create_agreement(build_agreement(
                id,
                "food-coop",
                fresh_did(),
                "tech-coop",
                fresh_did(),
            ))
            .unwrap();
    }

    // One adoption-derived canonical record on the supervisor side.
    let mut adopted = build_agreement(
        "agr-gov-101",
        "food-coop",
        fresh_did(),
        "tech-coop",
        fresh_did(),
    );
    adopted.source_agreement_id = Some("agr-dm-101".to_string());
    supervisor_mgr.create_agreement(adopted).unwrap();

    let gateway_ids: Vec<String> = gateway_mgr
        .list_agreements()
        .into_iter()
        .map(|a| a.agreement_id)
        .collect();
    let supervisor_ids: Vec<String> = supervisor_mgr
        .list_agreements()
        .into_iter()
        .map(|a| a.agreement_id)
        .collect();

    assert_eq!(gateway_ids.len(), 2);
    assert!(gateway_ids.contains(&"agr-dm-101".to_string()));
    assert!(gateway_ids.contains(&"agr-dm-102".to_string()));
    assert!(
        !gateway_ids.contains(&"agr-gov-101".to_string()),
        "gateway list must not surface supervisor-owned canonical records"
    );

    assert_eq!(supervisor_ids.len(), 1);
    assert!(supervisor_ids.contains(&"agr-gov-101".to_string()));
    assert!(
        !supervisor_ids.contains(&"agr-dm-101".to_string()),
        "supervisor list must not surface direct-management source records"
    );
    assert!(
        !supervisor_ids.contains(&"agr-dm-102".to_string()),
        "supervisor list must not surface direct-management source records"
    );

    // The intersection is empty — store boundaries are tight.
    let gateway_set: std::collections::HashSet<&String> = gateway_ids.iter().collect();
    let supervisor_set: std::collections::HashSet<&String> = supervisor_ids.iter().collect();
    let intersection: Vec<&&String> = gateway_set.intersection(&supervisor_set).collect();
    assert!(
        intersection.is_empty(),
        "gateway and supervisor agreement lists must not share any agreement_id; \
         intersection: {intersection:?}"
    );
}
