//! The N2-A startup gate after a real governance close.
//!
//! Closing a proposal on a node with a signing key emits a signed replication envelope, which
//! durably writes a `gov:repl:seq:v1:` row naming the node's own DID. Before that keyspace was
//! registered, the gate saw the row as uncovered and refused the data directory: the daemon that
//! had just closed a proposal could not start again. This drives the production close path and
//! then runs the gate over the daemon's real layout.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{actor::GovernanceActor, manager::GovernanceManager};
use icn_identity::IdentityBundle;
use icn_store::SledStore;

struct ClosedDecision {
    _dir: tempfile::TempDir,
    store_path: std::path::PathBuf,
    #[allow(dead_code)]
    proposal_id: ProposalId,
    #[allow(dead_code)]
    node_did: String,
}

/// Run the close on its own runtime and shut it down, so every task (and store handle) is gone.
fn close_a_real_decision(choice: VoteChoice) -> ClosedDecision {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("producer runtime");
    let closed = runtime.block_on(produce(choice));
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    closed
}

async fn produce(choice: VoteChoice) -> ClosedDecision {
    let dir = tempfile::tempdir().expect("tempdir");
    // The daemon's layout: `<data-dir>/store/governance`.
    let store_path = dir.path().join("store").join("governance");
    std::fs::create_dir_all(&store_path).expect("store dir");
    let bundle = IdentityBundle::generate().expect("identity");
    let did = bundle.did().clone();
    let signing_key = Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &bundle.keypair().expect("keypair").to_signing_key_bytes(),
    ));

    let proposal_id = {
        let store = Arc::new(SledStore::open(&store_path).expect("sled"));
        let gossip = GossipActor::spawn(did.clone(), None);
        gossip.write().await.create_topic(Topic::new(
            "governance:proposal".to_string(),
            AccessControl::Public,
        ));
        let handle = GovernanceActor::spawn(
            did.clone(),
            store.clone(),
            gossip,
            Arc::new(StaticMembershipResolver::new()),
            None,
            Some(signing_key),
        )
        .await
        .expect("spawn actor");
        let manager = GovernanceManager::with_handle(
            Arc::new(handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
        );
        let domain = GovernanceDomainId("n2a-gate".to_string());
        manager
            .create_domain(
                domain.clone(),
                "Offline proof".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![did.clone()]),
            )
            .await
            .expect("create_domain");
        let proposal_id = manager
            .create_proposal(
                ProposalId("_ignored".to_string()),
                domain,
                did.clone(),
                "Adopt a fictional statement".to_string(),
                "A declarative text proposal.".to_string(),
                ProposalPayload::Text {
                    body: "We adopt this fictional statement.".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .expect("create_proposal");
        manager
            .open_proposal(proposal_id.clone(), 3600)
            .await
            .expect("open_proposal");
        manager
            .cast_vote(proposal_id.clone(), did.clone(), choice, None)
            .await
            .expect("cast_vote");
        manager
            .close_proposal_with_suspension(proposal_id.clone(), None, None, None)
            .await
            .expect("close via the production actor path");
        assert!(
            handle
                .get_proof(&proposal_id)
                .await
                .expect("get_proof")
                .is_some(),
            "precondition: the close persisted a signed GovernanceProofV2"
        );
        icn_store::Store::flush(store.as_ref()).expect("flush");
        proposal_id
        // manager, handle, gossip and store drop here: the producer is gone.
    };

    ClosedDecision {
        _dir: dir,
        store_path,
        proposal_id,
        node_did: did.to_string(),
    }
}

#[test]
fn the_startup_gate_is_clear_after_a_real_close() {
    let decision = close_a_real_decision(VoteChoice::For);
    let data_dir = decision.store_path.parent().unwrap().parent().unwrap();

    // sled releases its lock from a background thread; the gate opens the store itself.
    let mut verdict = None;
    for _ in 0..50 {
        match icn_store::n2a_startup_gate::enforce(data_dir, std::time::SystemTime::now()) {
            Ok(r) => {
                verdict = Some(Ok(r));
                break;
            }
            Err(e)
                if format!("{e}").contains("WouldBlock")
                    || format!("{e:?}").contains("WouldBlock") =>
            {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                verdict = Some(Err(e));
                break;
            }
        }
    }
    let verdict = verdict.expect("gate ran");
    assert!(
        verdict.is_ok(),
        "a node that closed one proposal must still pass its startup gate: {verdict:?}"
    );
}
