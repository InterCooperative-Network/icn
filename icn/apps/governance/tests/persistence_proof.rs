//! Governance persistence proofs — two complementary tests.
//!
//! ## test_governance_restart_persistence (same-runtime proof)
//!
//! Proves that governance state survives a sled close-and-reopen boundary
//! within a single Tokio runtime.
//!
//! After calling `GovernanceHandle::shutdown()` (which now awaits the scheduler
//! task's JoinHandle for deterministic completion), the sled file lock is
//! definitively released and the same path can be reopened. A fresh
//! `GovernanceActor` reads back domain and proposal through the manager API.
//!
//! ## test_governance_cross_process_restart (cross-process proof)
//!
//! Proves that governance state survives a true OS-level process boundary.
//!
//! - Phase 1: `governance_restart_helper write` process writes domain + proposal,
//!   drives lifecycle to Accepted, exits cleanly.
//! - Phase 2: `governance_restart_helper read` process opens the same sled path,
//!   creates a fresh GovernanceActor, reads back via manager, asserts Accepted.
//!
//! Both phases use the canonical actor-backed path:
//!   `GovernanceActor::spawn` → `GovernanceManager::with_handle`
//!
//! ## What is proven
//!
//! - State written through the actor-backed sled path survives a same-runtime
//!   sled close+reopen (test 1).
//! - State written in one OS process is readable in a fresh OS process with a
//!   fresh actor (test 2).
//! - The scheduler task exits deterministically on `shutdown().await` — not
//!   via a fixed sleep.
//!
//! ## What is NOT proven
//!
//! - Gossip/network sync — GossipActor is spawned with no oracle and no peers.
//! - GovernanceProof signing (signing_key is None).
//! - State durability against process kill (SIGKILL) — only clean exit is proven.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, ProposalState, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{actor::GovernanceActor, manager::GovernanceManager};
use icn_identity::IdentityBundle;
use icn_store::SledStore;
use std::sync::Arc;

async fn gossip_with_governance_topic(
    did: icn_identity::Did,
) -> Arc<tokio::sync::RwLock<GossipActor>> {
    let gossip = GossipActor::spawn(did, None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    gossip
}

/// Same-runtime sled close+reopen proof with deterministic shutdown.
///
/// `shutdown().await` guarantees the scheduler task has fully exited (JoinHandle
/// awaited) before the sled path is reopened — no sleep required.
#[tokio::test]
async fn test_governance_restart_persistence() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();

    let domain_id = GovernanceDomainId("restart-domain".to_string());
    let proposal_id: ProposalId;

    // ── Phase 1: write lifecycle ─────────────────────────────────────────────
    {
        let store = Arc::new(SledStore::open(&db_path).expect("Phase1: SledStore::open"));
        let gossip = gossip_with_governance_topic(did.clone()).await;
        let resolver = Arc::new(StaticMembershipResolver::new());

        let actor_handle =
            GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
                .await
                .expect("Phase1: GovernanceActor::spawn");

        let shutdown_handle = actor_handle.clone();
        let manager = GovernanceManager::with_handle(
            Arc::new(actor_handle) as Arc<dyn GovernanceOps + Send + Sync>
        );

        manager
            .create_domain(
                domain_id.clone(),
                "Restart Test Domain".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![did.clone()]),
            )
            .await
            .expect("Phase1: create_domain");

        let returned_id = manager
            .create_proposal(
                ProposalId("_ignored".to_string()),
                domain_id.clone(),
                did.clone(),
                "Restart Proof Proposal".to_string(),
                "Proves state survives sled close+reopen.".to_string(),
                ProposalPayload::Text {
                    body: "state persists".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .expect("Phase1: create_proposal");

        manager
            .open_proposal(returned_id.clone(), 3600)
            .await
            .expect("Phase1: open_proposal");
        manager
            .cast_vote(returned_id.clone(), did.clone(), VoteChoice::For, None)
            .await
            .expect("Phase1: cast_vote");
        manager
            .close_proposal(returned_id.clone())
            .await
            .expect("Phase1: close_proposal");

        let p = manager
            .get_proposal(&returned_id)
            .await
            .expect("Phase1: get_proposal")
            .expect("proposal exists before restart");
        assert!(
            matches!(p.state, ProposalState::Accepted { .. }),
            "must be Accepted before restart boundary, got: {:?}",
            p.state
        );

        proposal_id = returned_id;

        // Deterministic shutdown: awaits JoinHandle — no sleep needed.
        shutdown_handle.shutdown().await;
        drop(manager);
        drop(shutdown_handle);
        // `store` Arc drops here (scope exit) → sled lock fully released
    }

    // ── Phase 2: reopen and read via fresh actor ─────────────────────────────
    //
    // SledStore::open succeeds iff the file lock was actually released above.
    let store2 = Arc::new(
        SledStore::open(&db_path).expect("Phase2: SledStore::open — lock must have been released"),
    );
    let gossip2 = gossip_with_governance_topic(did.clone()).await;
    let resolver2 = Arc::new(StaticMembershipResolver::new());

    let actor_handle2 = GovernanceActor::spawn(did.clone(), store2, gossip2, resolver2, None, None)
        .await
        .expect("Phase2: GovernanceActor::spawn");

    let shutdown2 = actor_handle2.clone();
    let manager2 = GovernanceManager::with_handle(
        Arc::new(actor_handle2) as Arc<dyn GovernanceOps + Send + Sync>
    );

    let domain = manager2
        .get_domain(&domain_id)
        .await
        .expect("Phase2: get_domain")
        .expect("domain must exist after restart");
    assert_eq!(domain.id.0, "restart-domain");
    assert_eq!(domain.name, "Restart Test Domain");

    let proposal = manager2
        .get_proposal(&proposal_id)
        .await
        .expect("Phase2: get_proposal")
        .expect("proposal must exist after restart");
    assert_eq!(proposal.id, proposal_id);
    assert_eq!(proposal.title, "Restart Proof Proposal");
    assert!(
        matches!(proposal.state, ProposalState::Accepted { .. }),
        "must be Accepted after restart, got: {:?}",
        proposal.state
    );

    shutdown2.shutdown().await;
    drop(manager2);
    drop(shutdown2);
}

/// Cross-process restart proof.
///
/// Phase 1 and Phase 2 are separate OS processes — governance_restart_helper
/// binary — that each open sled independently. The test is synchronous (no
/// Tokio runtime in the test thread) and uses `std::process::Command` to
/// spawn the helper.
///
/// The CARGO_BIN_EXE_governance_restart_helper environment variable is set
/// by Cargo when building test binaries in the same package, giving the
/// absolute path to the compiled binary.
#[test]
fn test_governance_cross_process_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_str().expect("db_path to str");
    let domain_id = "cross-process-domain";

    // ── Phase 1: write in a subprocess ──────────────────────────────────────
    let write_out = std::process::Command::new(env!("CARGO_BIN_EXE_governance_restart_helper"))
        .args(["write", db_path, domain_id])
        .output()
        .expect("failed to spawn write phase subprocess");

    assert!(
        write_out.status.success(),
        "write phase failed (exit {:?}):\nstdout: {}\nstderr: {}",
        write_out.status.code(),
        String::from_utf8_lossy(&write_out.stdout),
        String::from_utf8_lossy(&write_out.stderr),
    );

    let proposal_id = String::from_utf8(write_out.stdout)
        .expect("stdout is valid UTF-8")
        .trim()
        .to_string();

    assert!(
        !proposal_id.is_empty(),
        "write phase must print proposal_id to stdout"
    );

    // Validate the proposal_id looks like a UUID (36 chars, 4 hyphens).
    // If the write phase accidentally printed debug output or log lines instead
    // of just the UUID, this catches it before Phase 2 gets a garbage ID.
    assert_eq!(
        proposal_id.len(),
        36,
        "proposal_id must be a 36-char UUID, got {:?}",
        proposal_id
    );
    assert_eq!(
        proposal_id.chars().filter(|c| *c == '-').count(),
        4,
        "proposal_id must contain 4 hyphens (UUID format), got {:?}",
        proposal_id
    );

    // ── Phase 2: read in a fresh subprocess ──────────────────────────────────
    let read_out = std::process::Command::new(env!("CARGO_BIN_EXE_governance_restart_helper"))
        .args(["read", db_path, domain_id, &proposal_id])
        .output()
        .expect("failed to spawn read phase subprocess");

    assert!(
        read_out.status.success(),
        "read phase failed (exit {:?}):\nstdout: {}\nstderr: {}",
        read_out.status.code(),
        String::from_utf8_lossy(&read_out.stdout),
        String::from_utf8_lossy(&read_out.stderr),
    );
}
