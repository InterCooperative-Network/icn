//! JSON-RPC `governance.vote.cast` against the real governance actor.
//!
//! The icn-rpc crate tests its handlers against in-memory doubles only. This
//! test puts a real `GovernanceHandle` (sled store, static-list domain) behind
//! `RpcServer::set_governance_handle`, exactly as `init_rpc` wires it, and
//! drives `handle_governance_vote_cast` with an authenticated caller context.
//! It proves that the RPC path reaches the actor's vote admission and that its
//! refusals arrive as specific error codes, with no row written.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, StaticMembershipResolver,
};
use icn_governance_actor::GovernanceActor;
use icn_identity::KeyPair;
use icn_rpc::context::RpcContext;
use icn_rpc::error_codes::{CONFLICT, NOT_FOUND, PERMISSION_DENIED};
use icn_rpc::handler::governance::handle_governance_vote_cast;
use icn_rpc::server::RpcServer;
use icn_store::SledStore;

#[tokio::test]
async fn rpc_vote_cast_reaches_actor_admission() {
    let node = KeyPair::generate().unwrap().did().clone();
    let alice = KeyPair::generate().unwrap().did().clone();
    let mallory = KeyPair::generate().unwrap().did().clone();

    let store = Arc::new(SledStore::temporary().unwrap());
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        node,
        store,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();

    let domain = GovernanceDomainId("rpc-vote".to_string());
    GovernanceOps::create_domain(
        &handle,
        domain.clone(),
        "d".to_string(),
        "cooperative_default".to_string(),
        GovernanceParams::new(50, 50, 3600),
        MembershipConfig::static_list(vec![alice.clone()]),
    )
    .await
    .unwrap();
    let mk = |title: &'static str| {
        let handle = handle.clone();
        let domain = domain.clone();
        async move {
            GovernanceOps::create_proposal(
                &handle,
                domain,
                title.to_string(),
                "d".to_string(),
                ProposalPayload::Text { body: "b".into() },
                ProposalScope::Local,
            )
            .await
            .unwrap()
        }
    };
    let open = mk("open").await;
    GovernanceOps::open_proposal(&handle, open.clone(), 3600)
        .await
        .unwrap();
    let draft = mk("draft").await;

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let mut server = RpcServer::new(addr, Arc::new(KeyPair::generate().unwrap()));
    server.set_governance_handle(handle.clone());
    let state = Arc::new(server);

    let cast = |caller: &icn_identity::Did, id: &ProposalId| {
        let ctx = RpcContext::new(caller.clone(), None, vec![]);
        let params = serde_json::json!({ "proposal_id": id.0, "choice": "for" });
        let state = state.clone();
        async move { handle_governance_vote_cast(7, &params, &state, Some(&ctx)).await }
    };

    let code = |r: icn_rpc::types::RpcResponse| r.error.map(|e| e.code);

    assert_eq!(code(cast(&mallory, &open).await), Some(PERMISSION_DENIED));
    assert_eq!(code(cast(&alice, &draft).await), Some(CONFLICT));
    assert_eq!(
        code(cast(&alice, &ProposalId("missing".to_string())).await),
        Some(NOT_FOUND)
    );
    assert!(
        handle.get_voter_dids(&open).await.unwrap().is_empty(),
        "refused RPC votes must not write rows"
    );

    // Control: the member's vote through the same RPC path is admitted.
    assert_eq!(code(cast(&alice, &open).await), None);
    assert_eq!(handle.get_voter_dids(&open).await.unwrap(), vec![alice]);

    handle.shutdown().await;
}
