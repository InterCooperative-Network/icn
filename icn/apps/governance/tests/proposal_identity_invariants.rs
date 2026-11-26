//! Proposal identity through the public command path (D-S20-12).
//!
//! `GovernanceHandle::submit` is public and `CreateProposal` carries a
//! caller-chosen id. These tests drive the real actor against a real sled
//! store and prove that a reused id never replaces a stored proposal: not with
//! a fresh Draft, not from another domain, not after the original closed, and
//! not across a restart. Each refusal is checked against a byte-equal snapshot
//! of the stored row, and each test has a control that succeeds.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceDomainId, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, StaticMembershipResolver,
};
use icn_governance_actor::actor::{GovernanceActor, GovernanceHandle};
use icn_governance_actor::state_store::{
    GovernanceStateStore, ProposalWriteRefused, SledGovernanceStateStore,
};
use icn_governance_actor::GovernanceCommand;
use icn_identity::{Did, IdentityBundle};
use icn_store::{SledStore, Store};

struct Fx {
    handle: GovernanceHandle,
    state: SledGovernanceStateStore,
    store: Arc<SledStore>,
}

fn domain(id: &str, members: Vec<Did>) -> GovernanceDomain {
    let mut config = GovernanceConfig::cooperative_default();
    config.membership = MembershipConfig::static_list(members);
    GovernanceDomain::with_id(GovernanceDomainId::new(id), id.to_string(), config)
}

async fn spawn_on(store: Arc<SledStore>) -> Fx {
    let node = IdentityBundle::generate().unwrap().did().clone();
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        node,
        store.clone(),
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();
    let state = SledGovernanceStateStore::new(store.clone());
    Fx {
        handle,
        state,
        store,
    }
}

async fn fx_at(path: &std::path::Path) -> Fx {
    let fx = spawn_on(Arc::new(SledStore::open(path).unwrap())).await;
    for d in ["dom-a", "dom-b"] {
        if fx
            .state
            .get_domain(&GovernanceDomainId::new(d))
            .unwrap()
            .is_none()
        {
            fx.state.save_domain(&domain(d, vec![])).unwrap();
        }
    }
    fx
}

fn create(id: &str, dom: &str, title: &str) -> GovernanceCommand {
    GovernanceCommand::CreateProposal {
        proposal_id: ProposalId(id.to_string()),
        domain_id: GovernanceDomainId::new(dom),
        title: title.to_string(),
        description: "d".to_string(),
        payload: ProposalPayload::Text { body: "b".into() },
        scope: ProposalScope::Local,
        action_items_on_accept: vec![],
    }
}

/// The exact stored bytes of a proposal row (`gov:proposal:<id>`, as written
/// by `SledGovernanceStateStore`).
fn row(fx: &Fx, id: &str) -> Option<Vec<u8>> {
    fx.store
        .get(format!("gov:proposal:{id}").as_bytes())
        .unwrap()
}

fn refused(e: anyhow::Error) -> ProposalWriteRefused {
    e.downcast::<ProposalWriteRefused>()
        .expect("refusal must be a typed ProposalWriteRefused")
}

#[tokio::test]
async fn submitted_create_with_a_reused_id_is_refused_and_the_row_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = fx_at(tmp.path()).await;

    // Control: a fresh id is created.
    fx.handle
        .submit(create("p1", "dom-a", "original"))
        .await
        .unwrap();
    let before = row(&fx, "p1").unwrap();

    let err = fx
        .handle
        .submit(create("p1", "dom-a", "replacement"))
        .await
        .unwrap_err();
    assert!(matches!(
        refused(err),
        ProposalWriteRefused::AlreadyExists { .. }
    ));
    assert_eq!(row(&fx, "p1").unwrap(), before);
    let stored = fx
        .state
        .get_proposal(&ProposalId("p1".into()))
        .unwrap()
        .unwrap();
    assert_eq!(stored.title, "original");
}

#[tokio::test]
async fn submitted_create_cannot_move_a_proposal_to_another_domain() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = fx_at(tmp.path()).await;
    fx.handle
        .submit(create("p1", "dom-a", "original"))
        .await
        .unwrap();
    let before = row(&fx, "p1").unwrap();

    let err = fx
        .handle
        .submit(create("p1", "dom-b", "hijack"))
        .await
        .unwrap_err();
    assert!(matches!(
        refused(err),
        ProposalWriteRefused::AlreadyExists { .. }
    ));
    assert_eq!(row(&fx, "p1").unwrap(), before);
    assert_eq!(
        fx.state
            .get_proposal(&ProposalId("p1".into()))
            .unwrap()
            .unwrap()
            .domain_id,
        GovernanceDomainId::new("dom-a")
    );
    // Control: dom-b still accepts a proposal under a fresh id.
    fx.handle
        .submit(create("p2", "dom-b", "fresh"))
        .await
        .unwrap();
}

#[tokio::test]
async fn submitted_create_cannot_replace_a_closed_proposal_with_a_fresh_draft() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = fx_at(tmp.path()).await;
    fx.handle
        .submit(create("p1", "dom-a", "original"))
        .await
        .unwrap();
    fx.handle
        .submit(GovernanceCommand::VetoProposal {
            proposal_id: ProposalId("p1".into()),
            reason: "stop".into(),
        })
        .await
        .unwrap();
    let before = row(&fx, "p1").unwrap();
    assert!(fx
        .state
        .get_proposal(&ProposalId("p1".into()))
        .unwrap()
        .unwrap()
        .state
        .is_closed());

    let err = fx
        .handle
        .submit(create("p1", "dom-a", "reopened"))
        .await
        .unwrap_err();
    assert!(matches!(
        refused(err),
        ProposalWriteRefused::AlreadyExists { .. }
    ));
    assert_eq!(row(&fx, "p1").unwrap(), before);
}

#[tokio::test]
async fn a_reused_id_is_still_refused_after_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let before = {
        let fx = fx_at(tmp.path()).await;
        fx.handle
            .submit(create("p1", "dom-a", "original"))
            .await
            .unwrap();
        fx.store.flush().unwrap();
        let bytes = row(&fx, "p1").unwrap();
        // Stop the actor's task so every handle to the sled store is released; only then
        // can the same directory be reopened in this process (a real restart).
        fx.handle.shutdown().await;
        bytes
    };
    // The previous actor and store are dropped; reopen the same directory.
    let fx = fx_at(tmp.path()).await;
    assert_eq!(row(&fx, "p1").unwrap(), before);
    let err = fx
        .handle
        .submit(create("p1", "dom-b", "after restart"))
        .await
        .unwrap_err();
    assert!(matches!(
        refused(err),
        ProposalWriteRefused::AlreadyExists { .. }
    ));
    assert_eq!(row(&fx, "p1").unwrap(), before);
}

#[tokio::test]
async fn two_actors_on_one_store_create_one_id_exactly_once() {
    // Two independent actor instances share one store: their command locks do
    // not serialize each other, so only the store's atomic create can.
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(SledStore::open(tmp.path()).unwrap());
    let a = spawn_on(store.clone()).await;
    let b = spawn_on(store.clone()).await;
    a.state.save_domain(&domain("dom-a", vec![])).unwrap();
    a.state.save_domain(&domain("dom-b", vec![])).unwrap();

    let (ra, rb) = tokio::join!(
        a.handle.submit(create("p1", "dom-a", "from a")),
        b.handle.submit(create("p1", "dom-b", "from b")),
    );
    assert_eq!(
        usize::from(ra.is_ok()) + usize::from(rb.is_ok()),
        1,
        "exactly one create may win: a={ra:?} b={rb:?}"
    );
    let stored = a
        .state
        .get_proposal(&ProposalId("p1".into()))
        .unwrap()
        .unwrap();
    let (winner, loser) = match (ra, rb) {
        (Ok(()), Err(e)) => ("dom-a", e),
        (Err(e), Ok(())) => ("dom-b", e),
        other => panic!("exactly one create may win: {other:?}"),
    };
    assert_eq!(stored.domain_id, GovernanceDomainId::new(winner));
    assert!(matches!(
        refused(loser),
        ProposalWriteRefused::AlreadyExists { .. }
    ));
}
