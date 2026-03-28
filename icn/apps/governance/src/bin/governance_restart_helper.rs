//! Cross-process governance restart proof helper binary.
//!
//! Used exclusively by `apps/governance/tests/persistence_proof.rs` to prove
//! that governance state (domain, proposal, final outcome) survives a true
//! process-boundary restart — not just a same-runtime sled reopen.
//!
//! ## Usage
//!
//! ```text
//! governance_restart_helper write <sled_path> <domain_id>
//!     Runs the full governance lifecycle to Accepted.
//!     Prints the generated proposal UUID to stdout on success.
//!     Exits 0 on success, 1 on failure.
//!
//! governance_restart_helper read <sled_path> <domain_id> <proposal_id>
//!     Opens the same sled path in a fresh GovernanceActor.
//!     Reads domain and proposal through the manager API.
//!     Exits 0 if domain exists, proposal exists, and state is Accepted.
//!     Exits 1 with an error message on stderr otherwise.
//! ```
//!
//! Both modes use the canonical actor-backed path:
//!   `GovernanceActor::spawn` → `GovernanceManager::with_handle`
//!
//! They deliberately do NOT use direct sled or store inspection.

use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, ProposalState, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{actor::GovernanceActor, manager::GovernanceManager};
use icn_gossip::{AccessControl, GossipActor};
use icn_identity::IdentityBundle;
use icn_store::SledStore;
use std::path::PathBuf;
use std::sync::Arc;

const GOVERNANCE_TOPIC: &str = "governance:proposal";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: governance_restart_helper <write|read> <sled_path> ...");
        std::process::exit(1);
    }

    let mode = &args[1];
    let sled_path = PathBuf::from(&args[2]);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let exit_code = rt.block_on(async {
        match mode.as_str() {
            "write" => {
                if args.len() < 4 {
                    eprintln!("Usage: governance_restart_helper write <sled_path> <domain_id>");
                    return 1;
                }
                let domain_id = GovernanceDomainId(args[3].clone());
                write_phase(&sled_path, domain_id).await
            }
            "read" => {
                if args.len() < 5 {
                    eprintln!(
                        "Usage: governance_restart_helper read <sled_path> <domain_id> <proposal_id>"
                    );
                    return 1;
                }
                let domain_id = GovernanceDomainId(args[3].clone());
                let proposal_id = ProposalId(args[4].clone());
                read_phase(&sled_path, domain_id, proposal_id).await
            }
            other => {
                eprintln!("Unknown mode: {other}. Expected 'write' or 'read'.");
                1
            }
        }
    });

    // Explicitly drop the runtime before calling process::exit.
    // This cancels any surviving tokio tasks and runs their Drop impls,
    // releasing all Arc references (including to the sled store).
    // Without this, std::process::exit() skips Rust destructors, meaning
    // GovernanceActor / SledStore Drop might not run — only WAL recovery
    // on next open would save us. With this, sled flushes deterministically.
    drop(rt);

    std::process::exit(exit_code);
}

/// Subscribe to the governance topic before spawning the actor.
async fn setup_gossip(did: icn_identity::Did) -> Arc<tokio::sync::RwLock<GossipActor>> {
    let gossip = GossipActor::spawn(did, None);
    gossip.write().await.create_topic(icn_gossip::Topic::new(
        GOVERNANCE_TOPIC.to_string(),
        AccessControl::Public,
    ));
    gossip
}

/// Phase 1: open sled, run full governance lifecycle to Accepted, print proposal_id.
async fn write_phase(sled_path: &PathBuf, domain_id: GovernanceDomainId) -> i32 {
    let bundle = match IdentityBundle::generate() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("write: IdentityBundle::generate failed: {e}");
            return 1;
        }
    };
    let did = bundle.did().clone();

    let store = match SledStore::open(sled_path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("write: SledStore::open failed: {e}");
            return 1;
        }
    };

    let gossip = setup_gossip(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle = match GovernanceActor::spawn(
        did.clone(),
        store,
        gossip,
        resolver,
        None,
        None,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            eprintln!("write: GovernanceActor::spawn failed: {e}");
            return 1;
        }
    };

    let shutdown_handle = actor_handle.clone();
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle) as Arc<dyn GovernanceOps + Send + Sync>,
    );

    // Create domain with sole member = our DID (ensures voting thresholds are met).
    if let Err(e) = manager
        .create_domain(
            domain_id.clone(),
            "Cross-Process Test Domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
    {
        eprintln!("write: create_domain failed: {e}");
        return 1;
    }

    let proposal_id = match manager
        .create_proposal(
            ProposalId("_ignored_actor_mode".to_string()),
            domain_id,
            did.clone(),
            "Cross-Process Persistence Proposal".to_string(),
            "Proves state survives a real process exit and reopen.".to_string(),
            ProposalPayload::Text {
                body: "state persists across process boundary".to_string(),
            },
            ProposalScope::Local,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            eprintln!("write: create_proposal failed: {e}");
            return 1;
        }
    };

    if let Err(e) = manager.open_proposal(proposal_id.clone(), 3600).await {
        eprintln!("write: open_proposal failed: {e}");
        return 1;
    }

    if let Err(e) = manager
        .cast_vote(proposal_id.clone(), did.clone(), VoteChoice::For, None)
        .await
    {
        eprintln!("write: cast_vote failed: {e}");
        return 1;
    }

    if let Err(e) = manager.close_proposal(proposal_id.clone()).await {
        eprintln!("write: close_proposal failed: {e}");
        return 1;
    }

    // Verify Accepted before exit.
    match manager.get_proposal(&proposal_id).await {
        Ok(Some(p)) if matches!(p.state, ProposalState::Accepted { .. }) => {}
        Ok(Some(p)) => {
            eprintln!("write: proposal not Accepted before exit: {:?}", p.state);
            return 1;
        }
        Ok(None) => {
            eprintln!("write: proposal missing before exit");
            return 1;
        }
        Err(e) => {
            eprintln!("write: get_proposal failed: {e}");
            return 1;
        }
    }

    // Shutdown cleanly: signal + await task exit.
    shutdown_handle.shutdown().await;
    drop(manager);
    drop(shutdown_handle);

    // Print proposal_id for the calling test to pass to Phase 2.
    println!("{}", proposal_id.0);
    0
}

/// Phase 2: open the SAME sled path in a fresh process, read back domain + proposal.
async fn read_phase(
    sled_path: &PathBuf,
    domain_id: GovernanceDomainId,
    proposal_id: ProposalId,
) -> i32 {
    // Use a fresh DID for the actor — reading doesn't require membership.
    let bundle = match IdentityBundle::generate() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read: IdentityBundle::generate failed: {e}");
            return 1;
        }
    };
    let did = bundle.did().clone();

    let store = match SledStore::open(sled_path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("read: SledStore::open failed: {e}");
            return 1;
        }
    };

    let gossip = setup_gossip(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle = match GovernanceActor::spawn(
        did,
        store,
        gossip,
        resolver,
        None,
        None,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            eprintln!("read: GovernanceActor::spawn failed: {e}");
            return 1;
        }
    };

    let shutdown_handle = actor_handle.clone();
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle) as Arc<dyn GovernanceOps + Send + Sync>,
    );

    // Read domain through manager.
    let domain = match manager.get_domain(&domain_id).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            eprintln!(
                "read: domain '{}' not found after restart",
                domain_id.0
            );
            return 1;
        }
        Err(e) => {
            eprintln!("read: get_domain failed: {e}");
            return 1;
        }
    };

    if domain.id.0 != domain_id.0 {
        eprintln!(
            "read: domain id mismatch: got '{}', expected '{}'",
            domain.id.0, domain_id.0
        );
        return 1;
    }

    if domain.name != "Cross-Process Test Domain" {
        eprintln!(
            "read: domain name mismatch: got '{}', expected 'Cross-Process Test Domain'",
            domain.name
        );
        return 1;
    }

    // Read proposal through manager.
    let proposal = match manager.get_proposal(&proposal_id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!(
                "read: proposal '{}' not found after restart",
                proposal_id.0
            );
            return 1;
        }
        Err(e) => {
            eprintln!("read: get_proposal failed: {e}");
            return 1;
        }
    };

    if proposal.title != "Cross-Process Persistence Proposal" {
        eprintln!(
            "read: proposal title mismatch: got '{}', expected 'Cross-Process Persistence Proposal'",
            proposal.title
        );
        return 1;
    }

    if !matches!(proposal.state, ProposalState::Accepted { .. }) {
        eprintln!(
            "read: proposal state is not Accepted after restart: {:?}",
            proposal.state
        );
        return 1;
    }

    shutdown_handle.shutdown().await;
    drop(manager);
    drop(shutdown_handle);

    eprintln!("read: ok — domain and proposal verified as Accepted after process restart");
    0
}
