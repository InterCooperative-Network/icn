//! Authorized idempotent proposal republication (S27).
//!
//! `CreateProposal` persists the row before it broadcasts. If the broadcast then fails the
//! proposal exists locally but was never announced, and re-submitting the create is refused
//! because the id is taken (#2953). `RepublishProposal` is the recovery path: it re-emits the
//! **stored** row through the single emission seam and never writes it. These tests drive the
//! real actor against a real sled store and a real gossip actor, with a signing key, so the
//! emitted frames are the production signed shape.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, GossipHandle, Topic};
use icn_governance::replication::{GovernanceOpKind, SignedGovernanceOp, GOV_OP_MAGIC};
use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceDomainId, GovernanceMessage, MembershipConfig,
    Proposal, ProposalId, ProposalPayload, ProposalScope, StaticMembershipResolver,
};
use icn_governance_actor::actor::{GovernanceActor, GovernanceHandle, RepublishRefused};
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_governance_actor::GovernanceCommand;
use icn_identity::{Did, IdentityBundle};
use icn_store::{SledStore, Store};

const TOPIC: &str = "governance:proposal";
const DOMAIN: &str = "dom-r";

struct Fx {
    did: Did,
    handle: GovernanceHandle,
    gossip: GossipHandle,
    state: SledGovernanceStateStore,
    store: Arc<SledStore>,
}

/// A signing node whose governance topic does not admit it as a publisher yet, so every
/// publish fails until `declare_topic` re-creates the topic as public.
async fn fx() -> Fx {
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();
    let key = Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &bundle.keypair().unwrap().to_signing_key_bytes(),
    ));
    let gossip = GossipActor::spawn(did.clone(), None);
    gossip
        .write()
        .await
        .create_topic(Topic::new(TOPIC.into(), AccessControl::Public));
    let store = Arc::new(SledStore::temporary().unwrap());
    let handle = GovernanceActor::spawn(
        did.clone(),
        store.clone(),
        gossip.clone(),
        Arc::new(StaticMembershipResolver::new()),
        None,
        Some(key),
    )
    .await
    .unwrap();
    // The actor subscribed while the topic was public; now nobody may publish to it.
    gossip.write().await.create_topic(Topic::new(
        TOPIC.into(),
        AccessControl::Participants(vec![]),
    ));
    let state = SledGovernanceStateStore::new(store.clone());
    let mut config = GovernanceConfig::cooperative_default();
    config.membership = MembershipConfig::static_list(vec![did.clone()]);
    state
        .save_domain(&GovernanceDomain::with_id(
            GovernanceDomainId::new(DOMAIN),
            DOMAIN.to_string(),
            config,
        ))
        .unwrap();
    Fx {
        did,
        handle,
        gossip,
        state,
        store,
    }
}

impl Fx {
    async fn declare_topic(&self) {
        self.gossip
            .write()
            .await
            .create_topic(Topic::new(TOPIC.into(), AccessControl::Public));
    }

    async fn created_frames(&self) -> Vec<SignedGovernanceOp> {
        self.gossip
            .read()
            .await
            .get_entries(TOPIC)
            .iter()
            .map(|e| e.get_data().unwrap())
            .filter(|p| p.starts_with(GOV_OP_MAGIC))
            .map(|p| SignedGovernanceOp::decode(&p).unwrap())
            .filter(|op| op.op_kind() == GovernanceOpKind::ProposalCreated)
            .collect()
    }

    /// Every stored key and value except the emission sequence counter, which a signed
    /// emission is required to advance. This is the institutional state.
    fn institutional_state(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.store
            .scan(b"")
            .unwrap()
            .into_iter()
            .filter(|(k, _)| !k.starts_with(b"gov:repl:seq:"))
            .collect()
    }

    async fn republish(&self, id: &str) -> anyhow::Result<()> {
        self.handle
            .submit(GovernanceCommand::RepublishProposal {
                proposal_id: ProposalId(id.to_string()),
            })
            .await
    }
}

fn create(id: &str) -> GovernanceCommand {
    GovernanceCommand::CreateProposal {
        proposal_id: ProposalId(id.to_string()),
        domain_id: GovernanceDomainId::new(DOMAIN),
        title: "t".to_string(),
        description: "d".to_string(),
        payload: ProposalPayload::Text { body: "b".into() },
        scope: ProposalScope::Local,
        action_items_on_accept: vec![],
    }
}

fn refusal(e: anyhow::Error) -> RepublishRefused {
    e.downcast::<RepublishRefused>()
        .expect("refusal must be a typed RepublishRefused")
}

#[tokio::test]
async fn a_proposal_persisted_but_never_published_is_recovered_by_republication() {
    let fx = fx().await;
    // Persisted, then publication failed.
    assert!(fx.handle.submit(create("p1")).await.is_err());
    let stored = fx.state.get_proposal(&ProposalId("p1".into())).unwrap();
    assert!(
        stored.is_some(),
        "the row was persisted before the broadcast"
    );
    assert!(
        fx.handle.submit(create("p1")).await.is_err(),
        "re-creating is refused: the id is taken"
    );
    let before = fx.institutional_state();

    fx.declare_topic().await;
    fx.republish("p1").await.unwrap();

    let frames = fx.created_frames().await;
    assert_eq!(frames.len(), 1);
    frames[0].verify().expect("the republished frame verifies");
    assert_eq!(frames[0].author(), &fx.did);
    match frames[0].decode_op().unwrap() {
        GovernanceMessage::ProposalCreated { proposal } => {
            assert_eq!(
                serde_json::to_value(&*proposal).unwrap(),
                serde_json::to_value(stored.as_ref().unwrap()).unwrap(),
                "the stored row is what was emitted"
            )
        }
        other => panic!("unexpected op {other:?}"),
    }
    assert_eq!(fx.institutional_state(), before, "no write");
}

#[tokio::test]
async fn repeated_republication_uses_fresh_sequences_and_changes_no_state() {
    let fx = fx().await;
    fx.declare_topic().await;
    fx.handle.submit(create("p2")).await.unwrap();
    let before = fx.institutional_state();
    for _ in 0..3 {
        fx.republish("p2").await.unwrap();
    }
    let frames = fx.created_frames().await;
    assert_eq!(frames.len(), 4, "the create plus three republications");
    let seqs: Vec<u64> = frames.iter().map(|f| f.seq()).collect();
    let mut sorted = seqs.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        4,
        "every emission has a fresh sequence: {seqs:?}"
    );
    for f in &frames {
        f.verify().unwrap();
    }
    assert_eq!(fx.institutional_state(), before, "no institutional write");
}

#[tokio::test]
async fn republishing_an_unknown_proposal_is_refused_and_emits_nothing() {
    let fx = fx().await;
    fx.declare_topic().await;
    let before = fx.institutional_state();
    assert_eq!(
        refusal(fx.republish("missing").await.unwrap_err()),
        RepublishRefused::NotFound
    );
    assert!(fx.created_frames().await.is_empty());
    assert_eq!(fx.institutional_state(), before);
}

#[tokio::test]
async fn another_principals_proposal_is_never_republished_as_this_nodes() {
    let fx = fx().await;
    fx.declare_topic().await;
    let other = IdentityBundle::generate().unwrap().did().clone();
    let mut p = Proposal::new(
        GovernanceDomainId::new(DOMAIN),
        other,
        "t".into(),
        "d".into(),
        ProposalPayload::Text { body: "b".into() },
    );
    p.id = ProposalId("foreign".into());
    fx.state.create_proposal(&p).unwrap();
    let before = fx.institutional_state();
    assert_eq!(
        refusal(fx.republish("foreign").await.unwrap_err()),
        RepublishRefused::NotAuthor
    );
    assert!(fx.gossip.read().await.get_entries(TOPIC).is_empty());
    assert_eq!(fx.institutional_state(), before);
}
