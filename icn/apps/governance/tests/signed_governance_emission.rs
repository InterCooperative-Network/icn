//! Signed governance emission through the production composition (#2469 slice 3).
//!
//! Every test here drives `init_governance_actor` — the same entry point
//! `icn-core/src/supervisor/init_governance.rs` calls — and then reads the bytes that
//! actually reached the gossip topic. Nothing asserts on a helper in isolation: the claim
//! under test is "what this node puts on the wire", and only the wire can answer it.
//!
//! # What this suite does NOT claim
//!
//! Slice 3 restores **no** remote state application. A signed envelope proves content and
//! authorship; it proves nothing about authority over a domain, and authority is what
//! applying state would require. The containment assertions at the bottom of this file are
//! part of the slice, not a formality — and the full #2470 suite in
//! `fp02_governance_replication_containment.rs` must stay green alongside it.
//!
//! # A deliberately unkillable mutant
//!
//! Deleting the signed-frame recognition branch from the ingress callback entirely — so a
//! signed frame falls through to the legacy decode and merely logs a different line — fails
//! **no** test here or in the #2470 suite. That was checked, and it is the correct outcome
//! rather than a coverage hole.
//!
//! Recognition is a *diagnostic*, and the containment it sits inside is *structural*: the
//! callback captures no `GovernanceStateStore`, so neither branch can reach governance state
//! and neither branch is an authorization decision. The mutant is equivalent with respect to
//! every property this slice guarantees, which is exactly the claim
//! `observe_replicated_governance_message` already makes about itself — "there is
//! deliberately no telemetry to evade, because there is deliberately nothing to gate."
//!
//! Emission is where the observable behaviour lives, and that is where the mutation proof
//! bites: collapsing the sequence key, unbinding the domain, dropping the author/key check,
//! or letting `publish_to_topic` bypass the shared encoder each kill tests below.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{GossipActor, GossipEntry};
use icn_governance::replication::{GovernanceOpKind, SignedGovernanceOp, GOV_OP_MAGIC};
use icn_governance::{
    GovernanceDomainId, GovernanceMessage, GovernanceOps, GovernanceParams, MembershipConfig,
    ProposalId, ProposalPayload, ProposalScope, Vote, VoteChoice,
};
use icn_governance_actor::init::{init_governance_actor, GovernanceActorDeps};
use icn_governance_actor::manager::GovernanceManager;
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::events::{EventEmitter, SystemEvent};

const GOVERNANCE_TOPIC: &str = "governance:proposal";

struct NoopKernelEvents;

#[async_trait::async_trait]
impl EventEmitter for NoopKernelEvents {
    async fn emit(&self, _event: SystemEvent) {}
}

/// A node wired exactly as production wires one, optionally holding its own signing key.
///
/// `with_key` mirrors `lifecycle.rs:782`: the key is derived from the node's *own* identity
/// bundle, so `Did::from_public_key(key)` equals the actor DID. A node built without one
/// stands in for a locked or hardware-backed keystore, which is a supported production state.
struct Node {
    _tmp: tempfile::TempDir,
    did: Did,
    gossip: Arc<tokio::sync::RwLock<GossipActor>>,
    ops: GovernanceManager,
    state: SledGovernanceStateStore,
    /// Positive control on *delivery*, counting every entry dispatched to this node's own
    /// subscription on a governance topic — signed or legacy.
    ///
    /// Deliberately shape-agnostic, unlike the production callback, which returns early on
    /// the signed shape. A control that could only see legacy payloads would report zero for
    /// exactly the injections the containment tests care about most.
    delivered: Arc<std::sync::atomic::AtomicUsize>,
}

impl Node {
    fn delivered(&self) -> usize {
        self.delivered.load(std::sync::atomic::Ordering::SeqCst)
    }

    async fn spawn(with_key: bool) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
        let did = bundle.did().clone();

        let signing_key = with_key.then(|| {
            let kp = bundle.keypair().expect("keypair");
            Arc::new(ed25519_dalek::SigningKey::from_bytes(
                &kp.to_signing_key_bytes(),
            ))
        });

        let gossip = Arc::new(tokio::sync::RwLock::new(GossipActor::new(
            did.clone(),
            None,
        )));

        let services = init_governance_actor(
            tmp.path(),
            did.clone(),
            GovernanceActorDeps {
                gossip_handle: gossip.clone(),
                event_bus: Arc::new(NoopKernelEvents),
                signing_key,
                trust_service: None,
            },
        )
        .await
        .expect("init_governance_actor");

        let delivered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        {
            let counter = delivered.clone();
            let own = did.clone();
            gossip.write().await.add_notification_callback(Arc::new(
                move |topic: String, _entry: GossipEntry, subscriber_did: Did| {
                    let federation_root = icn_federation::TOPIC_FEDERATION_GOVERNANCE;
                    let is_gov = topic == GOVERNANCE_TOPIC || topic.starts_with(federation_root);
                    if is_gov && subscriber_did == own {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                },
            ));
        }

        let state = SledGovernanceStateStore::new(services.governance_store.clone());
        let ops = GovernanceManager::with_handle(
            Arc::new(services.governance_handle) as Arc<dyn GovernanceOps + Send + Sync>
        );

        Self {
            _tmp: tmp,
            did,
            gossip,
            ops,
            state,
            delivered,
        }
    }

    async fn entries(&self, topic: &str) -> Vec<GossipEntry> {
        self.gossip.read().await.get_entries(topic)
    }

    /// Create and subscribe a topic that production leaves undeclared.
    ///
    /// Needed only for per-federation governance topics — see
    /// `a_per_federation_topic_is_never_created_in_the_default_configuration`.
    async fn declare_topic(&self, topic: &str) {
        let mut g = self.gossip.write().await;
        g.create_topic(icn_gossip::Topic::new(
            topic.to_string(),
            icn_gossip::AccessControl::Public,
        ));
        g.subscribe(topic, self.did.clone())
            .await
            .expect("subscribe");
    }

    /// The logical payloads on `topic`, decompressed.
    ///
    /// `publish` compresses entries above a size threshold, so `entry.data` is not the
    /// payload for large entries. Every reader here goes through `get_data`, matching the
    /// production ingress and `computed_content_hash`.
    async fn payloads(&self, topic: &str) -> Vec<Vec<u8>> {
        self.entries(topic)
            .await
            .iter()
            .map(|e| e.get_data().expect("entry payload must be readable"))
            .collect()
    }

    /// Every signed frame on `topic`, decoded.
    ///
    /// Note that a seeded domain emits more than one: the node is also the *proposer*, so its
    /// own `ProposalCreated` is eligible and signed too. Assertions below therefore filter by
    /// op kind rather than counting frames — a bare count would silently conflate the two and
    /// would keep passing if the wrong operation were the one being signed.
    async fn signed_ops(&self, topic: &str) -> Vec<SignedGovernanceOp> {
        self.payloads(topic)
            .await
            .iter()
            .filter(|p| p.starts_with(GOV_OP_MAGIC))
            .map(|p| SignedGovernanceOp::decode(p).expect("frame must decode"))
            .collect()
    }

    async fn signed_ops_of_kind(
        &self,
        topic: &str,
        kind: GovernanceOpKind,
    ) -> Vec<SignedGovernanceOp> {
        self.signed_ops(topic)
            .await
            .into_iter()
            .filter(|op| op.op_kind() == kind)
            .collect()
    }

    /// The single signed operation of `kind` on `topic`.
    async fn only_signed_op(&self, topic: &str, kind: GovernanceOpKind) -> SignedGovernanceOp {
        let mut ops = self.signed_ops_of_kind(topic, kind).await;
        assert_eq!(
            ops.len(),
            1,
            "expected exactly one signed {kind:?} on {topic}, found {}",
            ops.len()
        );
        ops.remove(0)
    }

    async fn signed_vote_count(&self, topic: &str) -> usize {
        self.signed_ops_of_kind(topic, GovernanceOpKind::VoteCast)
            .await
            .len()
    }

    /// Establish a `StaticList` domain containing this node, plus an open proposal.
    async fn seed_static_domain(&self, domain: &str, scope: ProposalScope) -> ProposalId {
        let domain_id = GovernanceDomainId(domain.to_string());
        self.ops
            .create_domain(
                domain_id.clone(),
                format!("Domain {domain}"),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![self.did.clone()]),
            )
            .await
            .expect("create_domain");

        let proposal_id = self
            .ops
            .create_proposal(
                ProposalId("_ignored".to_string()),
                domain_id,
                self.did.clone(),
                "Seed".to_string(),
                "Seed".to_string(),
                ProposalPayload::Text {
                    body: "seed".to_string(),
                },
                scope,
            )
            .await
            .expect("create_proposal");

        self.ops
            .open_proposal(proposal_id.clone(), 3600)
            .await
            .expect("open_proposal");

        proposal_id
    }

    /// A domain whose membership is `TrustThreshold` — the stock cooperative default, and
    /// deliberately not eligible for a v1 signed envelope (design §5.5).
    async fn seed_trust_threshold_domain(&self, domain: &str) -> ProposalId {
        let domain_id = GovernanceDomainId(domain.to_string());
        self.ops
            .create_domain(
                domain_id.clone(),
                format!("Domain {domain}"),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::trust_threshold(0.3),
            )
            .await
            .expect("create_domain");

        let proposal_id = self
            .ops
            .create_proposal(
                ProposalId("_ignored".to_string()),
                domain_id,
                self.did.clone(),
                "Seed".to_string(),
                "Seed".to_string(),
                ProposalPayload::Text {
                    body: "seed".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .expect("create_proposal");

        self.ops
            .open_proposal(proposal_id.clone(), 3600)
            .await
            .expect("open_proposal");

        proposal_id
    }
}

// ---------------------------------------------------------------------------
// Emission — the signed shape
// ---------------------------------------------------------------------------

/// Proofs 1, 2, 4, 5: an eligible `StaticList` vote the node itself authors is emitted as a
/// valid envelope, whose author is the signing-key-derived DID, whose domain came from the
/// proposal, and whose signed bytes are the exact operation.
#[tokio::test(flavor = "current_thread")]
async fn a_vote_the_node_authors_is_emitted_as_a_verifiable_signed_frame() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain("signed-domain", ProposalScope::Local)
        .await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    let op = node
        .only_signed_op(GOVERNANCE_TOPIC, GovernanceOpKind::VoteCast)
        .await;

    op.verify().expect("emitted envelope must verify");
    assert_eq!(
        *op.author(),
        node.did,
        "author must be the signing identity"
    );

    let proposal = node
        .state
        .get_proposal(&proposal_id)
        .unwrap()
        .expect("proposal");
    assert_eq!(
        *op.domain_id(),
        proposal.domain_id,
        "domain must be bound from the proposal, not from the topic"
    );

    // Proof 5: the signed bytes are the exact operation, verbatim — not a re-serialization.
    match op.decode_op().expect("op_bytes must decode") {
        GovernanceMessage::VoteCast { vote, .. } => {
            assert_eq!(vote.voter, node.did);
            assert_eq!(vote.proposal_id, proposal_id);
            assert_eq!(vote.choice, VoteChoice::For);
        }
        other => panic!("expected VoteCast, got {}", other.message_type()),
    }

    // The seeded proposal took seq 1 in this domain (the node is its proposer, so it is
    // signed too); the vote is the second operation this author made here.
    assert_eq!(op.seq(), 2);
}

/// Signing is not restricted to the restorable set: the node's own `ProposalCreated` is
/// signed too, because it has an unambiguous acting principal (`proposal.proposer`).
///
/// Pinned deliberately. `V1_RESTORABLE_OP_KINDS` is `[VoteCast]` alone, so this operation is
/// **eligible but not restorable** — a receiver may never apply it. Emitting it costs
/// nothing and keeps one emission rule instead of two, but the distinction is easy to lose,
/// and losing it in the other direction (reading "signed" as "may be applied") is exactly
/// the mistake slice 7 must not make.
#[tokio::test(flavor = "current_thread")]
async fn a_signed_kind_is_not_thereby_a_restorable_kind() {
    let node = Node::spawn(true).await;
    node.seed_static_domain("eligible-domain", ProposalScope::Local)
        .await;

    let created = node
        .only_signed_op(GOVERNANCE_TOPIC, GovernanceOpKind::ProposalCreated)
        .await;
    created.verify().expect("it is a valid envelope");
    assert_eq!(*created.author(), node.did);

    assert!(
        !created.op_kind().is_restorable_in_v1(),
        "ProposalCreated must remain outside the v1 restorable set despite being signed"
    );
    assert!(
        GovernanceOpKind::VoteCast.is_restorable_in_v1(),
        "sanity: VoteCast is the one restorable kind"
    );

    // ProposalOpened has no acting principal, so it stays legacy even here.
    assert!(
        node.signed_ops_of_kind(GOVERNANCE_TOPIC, GovernanceOpKind::ProposalOpened)
            .await
            .is_empty(),
        "a lifecycle transition has no member-level author to sign as"
    );
}

/// Proof 6: the gossip entry's content hash binds the signed frame, through #2583's
/// mechanism rather than a second one added here.
#[tokio::test(flavor = "current_thread")]
async fn the_gossip_content_hash_binds_the_signed_frame() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain("hash-domain", ProposalScope::Local)
        .await;

    node.ops
        .cast_vote(proposal_id, node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    let entry = node
        .entries(GOVERNANCE_TOPIC)
        .await
        .into_iter()
        .find(|e| {
            e.get_data()
                .map(|p| p.starts_with(GOV_OP_MAGIC))
                .unwrap_or(false)
        })
        .expect("a signed entry");

    entry
        .validate_content_integrity()
        .expect("#2583: entry.hash must re-derive from the signed frame");

    // And the binding is to the frame specifically: mutating it breaks the check.
    let mut tampered = entry.clone();
    tampered.data[GOV_OP_MAGIC.len() + 1] ^= 0x01;
    assert!(
        tampered.validate_content_integrity().is_err(),
        "a mutated frame must no longer match the claimed content hash"
    );
}

/// Proof 12: the federation route signs on the same policy as `governance:proposal`.
///
/// `publish_to_topic` has exactly one caller (`publish_federation_if_scoped`) and shares the
/// encoder with `publish`, so this is the seam that would catch the two drifting apart.
///
/// The per-federation topic must be created explicitly here — see
/// `a_per_federation_topic_is_never_created_in_the_default_configuration` for why, and for
/// the pre-existing defect that makes it necessary.
#[tokio::test(flavor = "current_thread")]
async fn the_federation_topic_carries_the_same_signed_shape() {
    let node = Node::spawn(true).await;

    let federation_topic = format!(
        "{}:{}",
        icn_federation::TOPIC_FEDERATION_GOVERNANCE,
        "fed-alpha"
    );
    node.declare_topic(&federation_topic).await;

    let proposal_id = node
        .seed_static_domain(
            "fed-domain",
            ProposalScope::Federation("fed-alpha".to_string()),
        )
        .await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    let on_governance = node
        .only_signed_op(GOVERNANCE_TOPIC, GovernanceOpKind::VoteCast)
        .await;
    let on_federation = node
        .only_signed_op(&federation_topic, GovernanceOpKind::VoteCast)
        .await;

    on_federation
        .verify()
        .expect("the federation copy must verify too");
    assert_eq!(*on_federation.author(), node.did);
    assert_eq!(on_federation.domain_id(), on_governance.domain_id());
    assert_eq!(
        on_federation.op_bytes(),
        on_governance.op_bytes(),
        "both topics must carry the identical signed operation"
    );
}

/// A pre-existing defect on `main`, pinned so slice 3 does not get blamed for it and so a
/// later fix is noticed.
///
/// `publish_federation_if_scoped` publishes to `federation:governance:<fed_id>`, but the only
/// federation governance topic anything creates or subscribes is the **root**
/// `federation:governance` (`icn-core/src/supervisor/init_gossip.rs:325`). With the default
/// `TopicAutoCreationPolicy::Reject`, the per-federation publish is refused, and
/// `publish_federation_if_scoped` swallows the error with a `warn!`.
///
/// So federation-scoped governance never reaches the wire in the default configuration —
/// signed or legacy, before this change or after it. The encoder covers the route by
/// construction (proved by the test above, which declares the topic first); the route itself
/// is simply not wired. Fixing that is a topic-lifecycle change well outside #2469.
#[tokio::test(flavor = "current_thread")]
async fn a_per_federation_topic_is_never_created_in_the_default_configuration() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain(
            "fed-unwired",
            ProposalScope::Federation("fed-unwired".to_string()),
        )
        .await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote must succeed even though the federation publish is dropped");

    let topic = format!(
        "{}:{}",
        icn_federation::TOPIC_FEDERATION_GOVERNANCE,
        "fed-unwired"
    );
    assert!(
        node.entries(&topic).await.is_empty(),
        "if this topic now receives entries, the per-federation route was wired up and the \
         federation emission path needs re-examining"
    );

    // The local vote and the `governance:proposal` copy are unaffected.
    assert_eq!(node.signed_vote_count(GOVERNANCE_TOPIC).await, 1);
    assert!(node
        .state
        .get_vote(&proposal_id, &node.did)
        .unwrap()
        .is_some());
}

// ---------------------------------------------------------------------------
// Emission — the fallbacks, each for its own reason
// ---------------------------------------------------------------------------

/// Proof 9: a `TrustThreshold` domain is ineligible and must degrade, not fail.
///
/// `GovernanceConfig::cooperative_default()` is `trust_threshold(0.3)`, so this is the common
/// configuration. Turning it into an error would break ordinary governance for most domains.
#[tokio::test(flavor = "current_thread")]
async fn a_trust_threshold_domain_emits_the_legacy_payload() {
    let node = Node::spawn(true).await;
    let proposal_id = node.seed_trust_threshold_domain("trust-domain").await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote must still succeed on a TrustThreshold domain");

    assert_eq!(
        node.signed_vote_count(GOVERNANCE_TOPIC).await,
        0,
        "a TrustThreshold domain has no deterministic membership snapshot to bind"
    );

    // The legacy payload is still there and still decodes — nothing was lost.
    let decoded_votes = node
        .entries(GOVERNANCE_TOPIC)
        .await
        .iter()
        .filter_map(|e| GovernanceMessage::from_bytes(&e.data).ok())
        .filter(|m| matches!(m, GovernanceMessage::VoteCast { .. }))
        .count();
    assert_eq!(decoded_votes, 1, "the vote must still be published legacy");

    // And the vote was recorded locally regardless.
    assert!(node
        .state
        .get_vote(&proposal_id, &node.did)
        .unwrap()
        .is_some());
}

/// Proof 10: a locked or hardware-backed keystore is a supported production state. Governance
/// must keep working, unsigned, without a panic.
#[tokio::test(flavor = "current_thread")]
async fn a_node_without_a_signing_key_emits_the_legacy_payload() {
    let node = Node::spawn(false).await;
    let proposal_id = node
        .seed_static_domain("nokey-domain", ProposalScope::Local)
        .await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote must succeed without a signing key");

    assert_eq!(
        node.signed_ops(GOVERNANCE_TOPIC).await.len(),
        0,
        "no key means nothing at all is signed, not merely no votes"
    );
    assert!(node
        .state
        .get_vote(&proposal_id, &node.did)
        .unwrap()
        .is_some());
}

/// Proof 3: the gateway composition. A node relaying another member's vote holds none of
/// their key material, so it must not sign — naming itself author would assert an authorship
/// that does not exist, and naming the voter is refused by `SignedGovernanceOp::sign`.
#[tokio::test(flavor = "current_thread")]
async fn a_vote_cast_for_another_member_is_never_signed() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain("relay-domain", ProposalScope::Local)
        .await;

    let bob = IdentityBundle::generate().unwrap().did().clone();

    node.ops
        .cast_vote(proposal_id.clone(), bob.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    assert_eq!(
        node.signed_vote_count(GOVERNANCE_TOPIC).await,
        0,
        "a node must not sign a vote for a principal whose key it does not hold"
    );
    assert!(node.state.get_vote(&proposal_id, &bob).unwrap().is_some());
}

/// Proofs 7-8 at the composition seam: sequences are per-`(author, domain)` and independent.
///
/// The unit tests cover restart and key separation directly; this pins that the actor is
/// actually keying by domain rather than sharing one counter across the node.
#[tokio::test(flavor = "current_thread")]
async fn sequences_are_independent_per_domain_through_the_real_path() {
    let node = Node::spawn(true).await;

    let first = node.seed_static_domain("seq-a", ProposalScope::Local).await;
    let second = node.seed_static_domain("seq-b", ProposalScope::Local).await;

    node.ops
        .cast_vote(first.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .unwrap();
    node.ops
        .cast_vote(second.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .unwrap();

    let votes = node
        .signed_ops_of_kind(GOVERNANCE_TOPIC, GovernanceOpKind::VoteCast)
        .await;

    assert_eq!(votes.len(), 2, "both votes should be signed");
    assert_ne!(
        votes[0].domain_id(),
        votes[1].domain_id(),
        "the two votes are in distinct domains"
    );

    // Both votes share one author, one node and one topic, and differ only in domain. A
    // node-wide counter or a per-topic counter would therefore have given the second vote a
    // higher number than the first. Only per-(author, domain) keying makes them equal.
    for op in &votes {
        assert_eq!(
            op.seq(),
            2,
            "domain {:?} must run its own sequence (1 = its own ProposalCreated, 2 = its \
             vote) rather than continuing another domain's",
            op.domain_id()
        );
    }
}

/// A second vote in the *same* domain advances that domain's sequence.
#[tokio::test(flavor = "current_thread")]
async fn a_second_operation_in_one_domain_advances_its_sequence() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain("seq-same", ProposalScope::Local)
        .await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .unwrap();
    node.ops
        .cast_vote(
            proposal_id.clone(),
            node.did.clone(),
            VoteChoice::Against,
            None,
        )
        .await
        .unwrap();

    let mut vote_seqs: Vec<u64> = node
        .signed_ops_of_kind(GOVERNANCE_TOPIC, GovernanceOpKind::VoteCast)
        .await
        .iter()
        .map(|op| op.seq())
        .collect();
    vote_seqs.sort_unstable();

    assert_eq!(vote_seqs.len(), 2, "both votes should be signed");
    assert!(
        vote_seqs[1] > vote_seqs[0],
        "the author's intent order must advance within one domain: got {vote_seqs:?}"
    );

    // Nothing in this domain reused a sequence — the property that makes `seq` usable as a
    // same-key comparator at all.
    let all: Vec<u64> = node
        .signed_ops(GOVERNANCE_TOPIC)
        .await
        .iter()
        .map(|op| op.seq())
        .collect();
    let mut unique = all.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        all.len(),
        "no sequence may be issued twice within one (author, domain): got {all:?}"
    );
}

// ---------------------------------------------------------------------------
// Recognition and containment
// ---------------------------------------------------------------------------

/// Proofs 13-14: both shapes are recognised during rollout, and recognition is by magic.
#[tokio::test(flavor = "current_thread")]
async fn both_wire_shapes_are_distinguishable_by_magic() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain("mixed-domain", ProposalScope::Local)
        .await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .unwrap();

    let payloads = node.payloads(GOVERNANCE_TOPIC).await;

    let signed: Vec<_> = payloads
        .iter()
        .filter(|p| p.starts_with(GOV_OP_MAGIC))
        .collect();
    let legacy: Vec<_> = payloads
        .iter()
        .filter(|p| !p.starts_with(GOV_OP_MAGIC))
        .collect();

    // Both shapes coexist on one topic during rollout: the vote and the node's own proposal
    // are signed; the domain and lifecycle messages have no acting principal and stay legacy.
    assert_eq!(
        node.signed_vote_count(GOVERNANCE_TOPIC).await,
        1,
        "the vote is signed"
    );
    assert!(
        !legacy.is_empty(),
        "domain and lifecycle messages remain legacy during rollout"
    );

    // The discriminator is sound in both directions: a legacy payload never carries the
    // magic, and a signed frame never decodes as a bare GovernanceMessage. Neither shape can
    // be mistaken for the other, which is what lets recognition be a prefix check rather
    // than a trial decode.
    for payload in &legacy {
        assert!(
            GovernanceMessage::from_bytes(payload).is_ok(),
            "a non-magic payload must still be a decodable legacy message"
        );
    }
    for payload in &signed {
        assert!(
            GovernanceMessage::from_bytes(payload).is_err(),
            "a signed frame must not be mistaken for a legacy payload"
        );
    }
}

/// Proof 15, the load-bearing one: a **validly signed** envelope from a real member,
/// delivered over the remote ingress, still applies nothing.
///
/// This is the test that separates slice 3 from slice 7. Everything about this envelope is
/// legitimate — real key, real member, real domain, real proposal, signature verifies — and
/// it must *still* be inert, because authenticity is not authority.
#[tokio::test(flavor = "current_thread")]
async fn a_validly_signed_remote_vote_still_applies_nothing() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain("contained", ProposalScope::Local)
        .await;

    // A second member, with their own real key, authoring a genuinely valid envelope.
    let bundle = IdentityBundle::generate().unwrap();
    let member = bundle.did().clone();
    let key =
        ed25519_dalek::SigningKey::from_bytes(&bundle.keypair().unwrap().to_signing_key_bytes());

    let proposal = node.state.get_proposal(&proposal_id).unwrap().unwrap();
    let domain = node.state.get_domain(&proposal.domain_id).unwrap().unwrap();
    let members = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(m) => m.clone(),
        other => panic!("expected StaticList, got {other:?}"),
    };

    let msg = GovernanceMessage::vote_cast(
        Vote::new(proposal_id.clone(), member.clone(), VoteChoice::For),
        None,
    );
    let op = SignedGovernanceOp::sign(
        &key,
        member.clone(),
        proposal.domain_id.clone(),
        icn_governance::replication::ReplicationAuthority::StaticMembership {
            membership_hash: icn_governance::replication::static_membership_hash(
                &proposal.domain_id,
                &members,
            ),
        },
        1,
        &msg,
    )
    .expect("sign");
    op.verify().expect("this envelope is genuinely valid");

    inject_remote(&node, GOVERNANCE_TOPIC, &member, op.encode()).await;

    assert!(
        node.state
            .get_vote(&proposal_id, &member)
            .unwrap()
            .is_none(),
        "#2470 containment: a signed remote vote must not be applied in slice 3"
    );
}

/// The same, on a federation topic — the route added alongside the signed emission.
#[tokio::test(flavor = "current_thread")]
async fn a_signed_frame_on_a_federation_topic_applies_nothing() {
    let node = Node::spawn(true).await;
    let topic = format!(
        "{}:{}",
        icn_federation::TOPIC_FEDERATION_GOVERNANCE,
        "fed-beta"
    );
    node.declare_topic(&topic).await;

    let proposal_id = node
        .seed_static_domain(
            "contained-fed",
            ProposalScope::Federation("fed-beta".to_string()),
        )
        .await;

    let bundle = IdentityBundle::generate().unwrap();
    let member = bundle.did().clone();
    let key =
        ed25519_dalek::SigningKey::from_bytes(&bundle.keypair().unwrap().to_signing_key_bytes());

    let proposal = node.state.get_proposal(&proposal_id).unwrap().unwrap();
    let msg = GovernanceMessage::vote_cast(
        Vote::new(proposal_id.clone(), member.clone(), VoteChoice::For),
        None,
    );
    let op = SignedGovernanceOp::sign(
        &key,
        member.clone(),
        proposal.domain_id.clone(),
        icn_governance::replication::ReplicationAuthority::StaticMembership {
            membership_hash: [0u8; 32],
        },
        1,
        &msg,
    )
    .expect("sign");

    inject_remote(&node, &topic, &member, op.encode()).await;

    assert!(node
        .state
        .get_vote(&proposal_id, &member)
        .unwrap()
        .is_none());
}

/// Deliver bytes over the remote gossip ingress, exactly as a peer would.
///
/// Hashes the payload honestly so #2583 admits the entry: the claim under test is that a
/// *well-formed* signed entry is still inert, not that a malformed one is rejected — which
/// `icn-gossip/tests/entry_hash_integrity.rs` already covers.
///
/// Returns after asserting the entry was actually delivered on this node's subscription. A
/// containment assertion is only meaningful if the message got there to be refused; without
/// this control, a dropped subscription or a topic-filter regression would leave every
/// "applies nothing" test below green while testing nothing at all.
async fn inject_remote(node: &Node, topic: &str, author: &Did, data: Vec<u8>) {
    use icn_gossip::{GossipMessage, VectorClock};

    let before = node.delivered();

    let entry = GossipEntry {
        hash: icn_gossip::content_hash(&data),
        author: author.clone(),
        clock: VectorClock::new(),
        topic: topic.to_string(),
        data,
        compressed: false,
        timestamp: 1_700_000_000,
        replica_offered: None,
    };

    node.gossip
        .write()
        .await
        .handle_message(author, GossipMessage::Response { entry })
        .await
        .expect("gossip transport must keep accepting the entry");

    assert_eq!(
        node.delivered(),
        before + 1,
        "injected entry was not delivered exactly once on the local subscription"
    );
}

/// A signed frame large enough to be compressed must still be recognised.
///
/// `GossipActor::publish` compresses entries above a size threshold *after* hashing them, so
/// `entry.data` stops being the payload. Recognition that read the raw field would silently
/// misclassify every large entry — legacy ones as undecodable, signed ones as legacy — while
/// every small-payload test above stayed green.
#[tokio::test(flavor = "current_thread")]
async fn a_compressed_signed_frame_is_still_recognised_by_magic() {
    let node = Node::spawn(true).await;
    let proposal_id = node
        .seed_static_domain("compress-domain", ProposalScope::Local)
        .await;

    // A comment long enough to push the entry over the compression threshold. Highly
    // repetitive so zstd definitely shrinks it.
    let bulky = "ratify the amended bylaws. ".repeat(400);

    node.ops
        .cast_vote(
            proposal_id.clone(),
            node.did.clone(),
            VoteChoice::For,
            Some(bulky.clone()),
        )
        .await
        .expect("cast_vote");

    let compressed_entries = node
        .entries(GOVERNANCE_TOPIC)
        .await
        .into_iter()
        .filter(|e| e.compressed)
        .count();
    assert!(
        compressed_entries > 0,
        "this test is only meaningful if something actually got compressed; \
         raise the payload size if the threshold moved"
    );

    // Recognition still works, because it reads the logical payload.
    let op = node
        .only_signed_op(GOVERNANCE_TOPIC, GovernanceOpKind::VoteCast)
        .await;
    op.verify().expect("a compressed frame must still verify");

    match op.decode_op().expect("op_bytes must decode") {
        GovernanceMessage::VoteCast { vote, .. } => {
            assert_eq!(vote.comment.as_deref(), Some(bulky.as_str()));
        }
        other => panic!("expected VoteCast, got {}", other.message_type()),
    }
}
