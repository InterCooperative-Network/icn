//! Standalone-mode governance domain persistence proof (issue #1600).
//!
//! Proves that domains created through `GovernanceManager::new_with_sled`
//! survive a sled close+reopen boundary — the standalone-mode equivalent
//! of `persistence_proof::test_governance_restart_persistence` (which
//! covers actor-backed mode).
//!
//! Before this fix, standalone-mode domains lived only in an in-memory
//! `HashMap` and disappeared on every gateway restart. After the fix,
//! they are written through to the same `gov:domain:*` key space the
//! actor uses, and seeded back into the cache on construction.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_governance::{
    GovernanceDomainId, GovernanceParams, MembershipAction, MembershipConfig, MembershipSource,
};
use icn_governance_actor::manager::GovernanceManager;
use icn_identity::{Did, IdentityBundle};
use std::sync::Arc;

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

#[tokio::test]
async fn standalone_domain_survives_sled_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let domain_id = GovernanceDomainId("nycn-bootstrap".to_string());
    let alice = fresh_did();
    let bob = fresh_did();

    // ── Phase 1: write a domain and drop ────────────────────────────────────
    {
        let db = Arc::new(sled::open(&db_path).expect("Phase1: sled::open"));
        let manager = GovernanceManager::new_with_sled(db.clone());

        manager
            .create_domain(
                domain_id.clone(),
                "NYCN Bootstrap".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![alice.clone()]),
            )
            .await
            .expect("Phase1: create_domain");

        let listed = manager.list_domains().await.expect("Phase1: list_domains");
        assert_eq!(listed.len(), 1, "Phase1: exactly one domain after create");

        // Drop manager and Arc so sled flushes and releases the lock.
        drop(manager);
        drop(db);
    }

    // ── Phase 2: reopen and prove the domain is still there ─────────────────
    {
        let db = Arc::new(sled::open(&db_path).expect("Phase2: sled::open after drop"));
        let manager = GovernanceManager::new_with_sled(db.clone());

        let listed = manager.list_domains().await.expect("Phase2: list_domains");
        assert_eq!(
            listed.len(),
            1,
            "Phase2: domain must persist across restart, found {}",
            listed.len()
        );

        let loaded = manager
            .get_domain(&domain_id)
            .await
            .expect("Phase2: get_domain")
            .expect("Phase2: domain must exist after restart");
        assert_eq!(loaded.id, domain_id);
        assert_eq!(loaded.name, "NYCN Bootstrap");
        match &loaded.config.membership.source {
            MembershipSource::StaticList(members) => {
                assert_eq!(members, &vec![alice.clone()], "members must round-trip");
            }
            other => panic!("unexpected membership source after restart: {other:?}"),
        }

        // Re-creating the same domain id must error — proving the cache was
        // seeded from disk (not blank).
        let dup = manager
            .create_domain(
                domain_id.clone(),
                "Duplicate".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![alice.clone()]),
            )
            .await;
        assert!(
            dup.is_err(),
            "creating an already-persisted domain id must fail"
        );

        // Membership update must also persist.
        manager
            .update_domain_membership(domain_id.clone(), bob.clone(), MembershipAction::Add)
            .await
            .expect("Phase2: update_domain_membership add bob");

        drop(manager);
        drop(db);
    }

    // ── Phase 3: reopen once more and prove the membership update persisted ─
    {
        let db = Arc::new(sled::open(&db_path).expect("Phase3: sled::open"));
        let manager = GovernanceManager::new_with_sled(db);

        let loaded = manager
            .get_domain(&domain_id)
            .await
            .expect("Phase3: get_domain")
            .expect("Phase3: domain still present");

        match &loaded.config.membership.source {
            MembershipSource::StaticList(members) => {
                assert_eq!(
                    members.len(),
                    2,
                    "Phase3: membership update must persist, got {members:?}"
                );
                assert!(members.contains(&alice), "alice must persist");
                assert!(members.contains(&bob), "bob must persist");
            }
            other => panic!("Phase3: unexpected membership source: {other:?}"),
        }
    }
}

#[tokio::test]
async fn standalone_empty_db_starts_clean() {
    // Sanity: a fresh sled with no prior domains should yield an empty manager.
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(sled::open(tmp.path()).expect("sled::open"));
    let manager = GovernanceManager::new_with_sled(db);

    let listed = manager.list_domains().await.expect("list_domains");
    assert!(
        listed.is_empty(),
        "fresh standalone manager must have no domains"
    );
}
