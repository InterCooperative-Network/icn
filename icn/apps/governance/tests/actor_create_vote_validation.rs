//! The governance actor validates proposal creation and vote casting the way the manager does.
//!
//! Reproduced on a release daemon over local RPC: a node accepted a proposal in a domain it did
//! not hold, and recorded a vote on a proposal that was never opened. The actor's close tally
//! counts every stored vote row when no eligibility filter is passed, so a stored vote is not
//! harmless. These tests drive the production `GovernanceActor` (real sled store) through
//! `GovernanceOps` and pin each refusal, that nothing is persisted on refusal, and that the
//! valid path is unchanged.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, ProposalState, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{actor::GovernanceActor, manager::GovernanceManager};
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;

struct Node {
    _dir: tempfile::TempDir,
    handle: icn_governance_actor::actor::GovernanceHandle,
    manager: GovernanceManager,
    did: Did,
}

async fn node() -> Node {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SledStore::open(dir.path().join("governance")).unwrap());
    let did = IdentityBundle::generate().unwrap().did().clone();
    let gossip = GossipActor::spawn(did.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        did.clone(),
        store,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();
    let manager = GovernanceManager::with_handle(
        Arc::new(handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    Node {
        _dir: dir,
        handle,
        manager,
        did,
    }
}

impl Node {
    async fn domain(&self, id: &str, members: Vec<Did>) -> GovernanceDomainId {
        let domain = GovernanceDomainId(id.to_string());
        self.manager
            .create_domain(
                domain.clone(),
                id.to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(members),
            )
            .await
            .unwrap();
        domain
    }

    async fn propose(&self, domain: &GovernanceDomainId) -> anyhow::Result<ProposalId> {
        self.manager
            .create_proposal(
                ProposalId("_ignored".to_string()),
                domain.clone(),
                self.did.clone(),
                "A fictional proposal".to_string(),
                "validation test".to_string(),
                ProposalPayload::Text {
                    body: "We adopt this fictional statement.".to_string(),
                },
                ProposalScope::Local,
            )
            .await
    }

    async fn votes(&self, id: &ProposalId) -> usize {
        let t = self.handle.get_vote_tally(id).await.unwrap();
        t.for_votes + t.against_votes + t.abstain_votes
    }
}

#[tokio::test]
async fn a_proposal_in_a_domain_this_node_does_not_hold_is_refused_and_not_persisted() {
    let n = node().await;
    let err = n
        .propose(&GovernanceDomainId("no-such-domain".to_string()))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
    assert!(
        n.handle.list_proposals().await.unwrap().is_empty(),
        "nothing persisted"
    );
}

#[tokio::test]
async fn a_vote_on_a_proposal_that_does_not_exist_is_refused() {
    let n = node().await;
    n.domain("coop", vec![n.did.clone()]).await;
    let missing = ProposalId("00000000-0000-0000-0000-000000000000".to_string());
    let err = n
        .manager
        .cast_vote(missing.clone(), n.did.clone(), VoteChoice::For, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}

#[tokio::test]
async fn a_vote_on_an_unopened_draft_is_refused_and_no_vote_row_exists() {
    let n = node().await;
    let d = n.domain("coop", vec![n.did.clone()]).await;
    let id = n.propose(&d).await.unwrap();
    let state = n.handle.get_proposal(&id).await.unwrap().unwrap().state;
    assert!(
        matches!(state, ProposalState::Draft),
        "precondition: {state:?}"
    );
    let err = n
        .manager
        .cast_vote(id.clone(), n.did.clone(), VoteChoice::For, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not open for voting"), "{err}");
    assert_eq!(n.votes(&id).await, 0, "no vote row was written");
}

#[tokio::test]
async fn a_vote_by_a_non_member_is_refused_and_no_vote_row_exists() {
    let n = node().await;
    let d = n.domain("coop", vec![n.did.clone()]).await;
    let id = n.propose(&d).await.unwrap();
    n.manager.open_proposal(id.clone(), 3600).await.unwrap();
    let stranger = IdentityBundle::generate().unwrap().did().clone();
    let err = n
        .manager
        .cast_vote(id.clone(), stranger, VoteChoice::For, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not a member"), "{err}");
    assert_eq!(n.votes(&id).await, 0, "no vote row was written");
}

#[tokio::test]
async fn a_vote_after_close_is_refused() {
    let n = node().await;
    let d = n.domain("coop", vec![n.did.clone()]).await;
    let id = n.propose(&d).await.unwrap();
    n.manager.open_proposal(id.clone(), 3600).await.unwrap();
    n.manager
        .cast_vote(id.clone(), n.did.clone(), VoteChoice::For, None)
        .await
        .unwrap();
    n.manager
        .close_proposal_with_suspension(id.clone(), None, None, None)
        .await
        .unwrap();
    let err = n
        .manager
        .cast_vote(id.clone(), n.did.clone(), VoteChoice::Against, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not open for voting"), "{err}");
    let t = n.handle.get_vote_tally(&id).await.unwrap();
    assert_eq!(
        (t.for_votes, t.against_votes),
        (1, 0),
        "the counted vote is unchanged, not overwritten"
    );
}

#[tokio::test]
async fn a_vote_during_deliberation_is_refused() {
    let n = node().await;
    let d = n.domain("coop", vec![n.did.clone()]).await;
    let id = n.propose(&d).await.unwrap();
    n.manager
        .start_deliberation(&id, 3600)
        .await
        .expect("start deliberation");
    let state = n.handle.get_proposal(&id).await.unwrap().unwrap().state;
    assert!(
        matches!(state, ProposalState::Deliberation { .. }),
        "precondition: {state:?}"
    );
    let err = n
        .manager
        .cast_vote(id.clone(), n.did.clone(), VoteChoice::For, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not open for voting"), "{err}");
    assert_eq!(n.votes(&id).await, 0, "no vote row was written");
}

/// TrustThreshold eligibility is decided by the membership resolver, like every other source:
/// the actor admits a vote only from a principal the resolver names (the same electorate the
/// close-time quorum is drawn from). This node is wired with `StaticMembershipResolver`, which
/// cannot name a TrustThreshold electorate, so the vote is refused fail-closed and no row is
/// written; a proposal in such a domain could not be closed under this resolver either. The
/// admitted TrustThreshold path (a TrustService-backed resolver, as the daemon wires it) is
/// covered by `signed_governance_emission::a_trust_threshold_domain_emits_the_legacy_payload`.
#[tokio::test]
async fn trust_threshold_eligibility_is_resolver_decided() {
    let n = node().await;
    let domain = GovernanceDomainId("tt".to_string());
    n.manager
        .create_domain(
            domain.clone(),
            "tt".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::trust_threshold(0.3),
        )
        .await
        .unwrap();
    let id = n.propose(&domain).await.unwrap();
    n.manager.open_proposal(id.clone(), 3600).await.unwrap();
    let anyone = IdentityBundle::generate().unwrap().did().clone();
    let err = n
        .manager
        .cast_vote(id.clone(), anyone, VoteChoice::For, None)
        .await
        .expect_err("an electorate the resolver cannot name admits no vote");
    assert!(err.to_string().contains("TrustThreshold"), "{err}");
    // Count stored rows directly: the read tally of an Open proposal resolves the same
    // electorate, so under this resolver it refuses too.
    assert!(
        n.handle.get_voter_dids(&id).await.unwrap().is_empty(),
        "no vote row was written"
    );
    let tally_err = n
        .handle
        .get_vote_tally(&id)
        .await
        .expect_err("the read tally cannot name the electorate either");
    assert!(
        tally_err.to_string().contains("TrustThreshold"),
        "{tally_err}"
    );
}

#[tokio::test]
async fn the_valid_path_is_unchanged_create_open_vote_close() {
    let n = node().await;
    let member = IdentityBundle::generate().unwrap().did().clone();
    let d = n.domain("coop", vec![n.did.clone(), member.clone()]).await;
    let id = n.propose(&d).await.unwrap();
    n.manager.open_proposal(id.clone(), 3600).await.unwrap();
    n.manager
        .cast_vote(id.clone(), n.did.clone(), VoteChoice::For, None)
        .await
        .unwrap();
    n.manager
        .cast_vote(id.clone(), member, VoteChoice::For, None)
        .await
        .expect("a listed member votes");
    assert_eq!(n.votes(&id).await, 2);
    n.manager
        .close_proposal_with_suspension(id.clone(), None, None, None)
        .await
        .unwrap();
    let state = n.handle.get_proposal(&id).await.unwrap().unwrap().state;
    assert!(matches!(state, ProposalState::Accepted { .. }), "{state:?}");
}

#[tokio::test]
async fn a_member_spelled_differently_is_still_the_member() {
    let n = node().await;
    let d = n.domain("coop", vec![n.did.clone()]).await;
    let id = n.propose(&d).await.unwrap();
    n.manager.open_proposal(id.clone(), 3600).await.unwrap();
    let body = n.did.as_str().strip_prefix("did:icn:").unwrap();
    let (_, bytes) = multibase::decode(body).unwrap();
    let alias: Did = format!(
        "did:icn:{}",
        multibase::encode(multibase::Base::Base16Lower, &bytes)
    )
    .parse()
    .unwrap();
    assert_ne!(alias.as_str(), n.did.as_str(), "a different spelling");
    n.manager
        .cast_vote(id.clone(), alias, VoteChoice::For, None)
        .await
        .expect("membership is by key, not spelling");
    assert_eq!(n.votes(&id).await, 1);
}
