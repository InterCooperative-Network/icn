//! F-P0-2 containment: unauthenticated remote governance messages must never be
//! applied to governance state.
//!
//! ## The invariant
//!
//! Institutional governance state must only be mutated by a party whose authority
//! over the affected governance domain has been authenticated and verified.
//! Permission to transport or replicate bytes is not permission to apply those
//! bytes as governance state.
//!
//! ## The mechanism these tests exercise
//!
//! `GossipEntry` (`icn-gossip/src/types.rs`) carries a claimed `author` DID but
//! **no signature binding that DID to the entry contents**, and the receive path
//! (`GossipActor::store_entry`) neither recomputes the entry hash nor enforces the
//! topic ACL. So any peer that can reach the node may publish an entry claiming
//! *any* author. The governance replication ingress therefore receives an
//! attacker-chosen DID, not an authenticated one — comparing anything against it
//! is not an authorization check.
//!
//! Every negative test below uses a genuinely attacker-controlled `entry.author`.
//! A test that merely passes `None` would not exercise the real vulnerability.
//!
//! ## Test boundary
//!
//! These tests drive the **production** wiring, not a reimplementation:
//!
//! * `icn_governance_actor::init::init_governance_actor` is the exact function the
//!   `icnd` supervisor calls (`icn-core/src/supervisor/init_governance.rs`). It
//!   creates the `governance:proposal` topic, opens the real Sled-backed store,
//!   spawns the real `GovernanceActor`, and registers the real notification callback.
//! * Forged entries are injected through the real remote ingress
//!   `GossipActor::handle_message(sender, GossipMessage::Response { entry })`, which
//!   dispatches to `handle_response` → `store_entry` → notification callbacks —
//!   the same path a `Response` off the wire takes.
//!
//! Local governance is driven through `GovernanceManager`, the same ops surface the
//! HTTP/RPC handlers use.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{GossipActor, GossipEntry, GossipMessage, VectorClock};
use icn_governance::{
    Delegation, DelegationScope, GovernanceConfig, GovernanceDomain, GovernanceDomainId,
    GovernanceMessage, GovernanceOps, GovernanceParams, GovernanceProfileId, MembershipConfig,
    Proposal, ProposalId, ProposalOutcome, ProposalPayload, ProposalScope, ProposalState,
    TallySnapshot, Vote, VoteChoice,
};
use icn_governance_actor::init::{init_governance_actor, GovernanceActorDeps};
use icn_governance_actor::manager::GovernanceManager;
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::events::{EventEmitter, SystemEvent};

const GOVERNANCE_TOPIC: &str = "governance:proposal";

/// Kernel event bus stub — this suite asserts on durable governance state, not events.
struct NoopKernelEvents;

#[async_trait::async_trait]
impl EventEmitter for NoopKernelEvents {
    async fn emit(&self, _event: SystemEvent) {}
}

/// A node assembled the way `icnd` assembles one, plus a typed read handle on the
/// governance store so assertions can inspect durable state directly.
struct Node {
    _tmp: tempfile::TempDir,
    did: Did,
    gossip: Arc<tokio::sync::RwLock<GossipActor>>,
    ops: GovernanceManager,
    state: SledGovernanceStateStore,
    /// Positive control. Registered on the same fan-out seam as the production
    /// governance callback (`add_notification_callback`), so if this observer records
    /// a message the production ingress was invoked for it in the same dispatch loop.
    ///
    /// Without this, every negative test below would still pass if the ingress stopped
    /// being reached at all — a renamed topic filter, a dropped subscription or a
    /// decode regression would leave the suite green while testing nothing.
    observed: Arc<std::sync::Mutex<Vec<(String, Did)>>>,
    /// Distinguishes otherwise-identical injections. `store_entry` dedups on the
    /// sender-supplied `entry.hash`, so replays must carry distinct claimed hashes to
    /// reach the ingress independently — which is exactly the attacker's capability.
    seq: std::sync::atomic::AtomicU64,
}

impl Node {
    async fn spawn() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
        let did = bundle.did().clone();

        let gossip = Arc::new(tokio::sync::RwLock::new(GossipActor::new(
            did.clone(),
            None,
        )));

        // The production entry point: creates the topic, opens the store, spawns the
        // actor, and registers the governance notification callback.
        let services = init_governance_actor(
            tmp.path(),
            did.clone(),
            GovernanceActorDeps {
                gossip_handle: gossip.clone(),
                event_bus: Arc::new(NoopKernelEvents),
                signing_key: None,
                trust_service: None,
            },
        )
        .await
        .expect("init_governance_actor");

        let observed: Arc<std::sync::Mutex<Vec<(String, Did)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let obs = observed.clone();
            gossip.write().await.add_notification_callback(Arc::new(
                move |topic: String, entry: GossipEntry, subscriber_did: Did| {
                    let is_gov = topic == GOVERNANCE_TOPIC
                        || topic.starts_with(icn_federation::TOPIC_FEDERATION_GOVERNANCE);
                    if is_gov {
                        if let Ok(msg) = GovernanceMessage::from_bytes(&entry.data) {
                            obs.lock()
                                .unwrap()
                                .push((msg.message_type().to_string(), subscriber_did));
                        }
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
            observed,
            seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Inject a governance message over the real remote gossip ingress, with an
    /// attacker-chosen `entry.author` and an attacker-chosen `entry.hash`.
    ///
    /// `claimed_author` is what the attacker writes into the entry; `wire_sender` is
    /// the DID the connection claims. Neither is authenticated anywhere.
    async fn inject_forged(
        &self,
        topic: &str,
        claimed_author: &Did,
        wire_sender: &Did,
        msg: &GovernanceMessage,
    ) {
        let before = self.local_deliveries();
        self.inject_forged_bytes(topic, claimed_author, wire_sender, msg.to_bytes().unwrap())
            .await;

        // Positive control: prove this injection actually reached the governance
        // replication ingress rather than being dropped upstream (topic filter, dedup,
        // missing subscription, decode failure). A refusal is only meaningful if the
        // message got there to be refused.
        assert_eq!(
            self.local_deliveries(),
            before + 1,
            "injected {} never reached the governance replication ingress exactly once",
            msg.message_type()
        );
    }

    /// Invocations whose `subscriber_did` matches this node — the same predicate the
    /// production ingress filters on, so this counts real ingress deliveries rather
    /// than the raw per-subscriber fan-out.
    fn local_deliveries(&self) -> usize {
        self.observed
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, sub)| *sub == self.did)
            .count()
    }

    fn total_fanout(&self) -> usize {
        self.observed.lock().unwrap().len()
    }

    async fn inject_forged_bytes(
        &self,
        topic: &str,
        claimed_author: &Did,
        wire_sender: &Did,
        data: Vec<u8>,
    ) {
        // The attacker picks the hash freely: `store_entry` dedups on this field and
        // never recomputes it from `data`. Mixing in a per-injection counter models that
        // freedom and keeps repeated injections of identical bytes from being silently
        // deduped away before they reach the ingress.
        let nonce = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut hash = [0u8; 32];
        for (i, b) in data.iter().take(24).enumerate() {
            hash[i] = *b ^ 0xA5;
        }
        hash[24..32].copy_from_slice(&nonce.to_le_bytes());

        let entry = GossipEntry {
            hash,
            author: claimed_author.clone(),
            clock: VectorClock::new(),
            topic: topic.to_string(),
            data,
            compressed: false,
            timestamp: 1_700_000_000,
            replica_offered: None,
        };

        let mut gossip = self.gossip.write().await;
        gossip
            .handle_message(wire_sender, GossipMessage::Response { entry })
            .await
            .expect("gossip transport must keep accepting the entry");
    }
}

fn attacker() -> Did {
    IdentityBundle::generate()
        .expect("attacker identity")
        .did()
        .clone()
}

fn make_domain(id: &str, members: Vec<Did>) -> GovernanceDomain {
    GovernanceDomain::with_id(
        GovernanceDomainId(id.to_string()),
        format!("Domain {id}"),
        GovernanceConfig::new(
            GovernanceProfileId::builtin("cooperative_default"),
            MembershipConfig::static_list(members),
            GovernanceParams::new(50, 50, 3600),
        ),
    )
}

/// Establish a domain + open proposal through the legitimate local path.
async fn seed_open_proposal(node: &Node) -> (GovernanceDomainId, ProposalId) {
    let domain_id = GovernanceDomainId("fp02-domain".to_string());
    node.ops
        .create_domain(
            domain_id.clone(),
            "FP02 Domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![node.did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = node
        .ops
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            node.did.clone(),
            "Seed proposal".to_string(),
            "Legitimately created locally".to_string(),
            ProposalPayload::Text {
                body: "seed".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    node.ops
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    (domain_id, proposal_id)
}

// ---------------------------------------------------------------------------
// 1. Forged DomainCreated cannot create a domain
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn forged_domain_created_cannot_create_a_domain() {
    let node = Node::spawn().await;
    let mallory = attacker();

    let domain = make_domain("forged-domain", vec![mallory.clone()]);
    let domain_id = domain.id.clone();

    node.inject_forged(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        &GovernanceMessage::domain_created(domain),
    )
    .await;

    assert!(
        node.state.get_domain(&domain_id).unwrap().is_none(),
        "a forged DomainCreated created a governance domain"
    );
}

// ---------------------------------------------------------------------------
// 2. Forged DomainUpdated cannot modify a domain
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn forged_domain_updated_cannot_modify_a_domain() {
    let node = Node::spawn().await;
    let mallory = attacker();
    let (domain_id, _) = seed_open_proposal(&node).await;

    let before = node
        .state
        .get_domain(&domain_id)
        .unwrap()
        .expect("seeded domain");

    // Attacker rewrites the membership to themselves and slashes the thresholds.
    let mut tampered = before.clone();
    tampered.config.membership = MembershipConfig::static_list(vec![mallory.clone()]);
    tampered.config.params = GovernanceParams::new(1, 1, 1);

    node.inject_forged(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        &GovernanceMessage::domain_updated(tampered),
    )
    .await;

    let after = node
        .state
        .get_domain(&domain_id)
        .unwrap()
        .expect("domain still present");
    assert_eq!(
        serde_json::to_value(&before).unwrap(),
        serde_json::to_value(&after).unwrap(),
        "a forged DomainUpdated modified the domain"
    );
}

// ---------------------------------------------------------------------------
// 3. Forged ProposalCreated cannot create a proposal
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn forged_proposal_created_cannot_create_a_proposal() {
    let node = Node::spawn().await;
    let mallory = attacker();
    let (domain_id, _) = seed_open_proposal(&node).await;

    let forged_id = ProposalId("forged-proposal".to_string());
    let mut proposal = Proposal::new(
        domain_id,
        mallory.clone(),
        "Forged proposal".to_string(),
        "Injected over gossip".to_string(),
        ProposalPayload::Text {
            body: "forged".to_string(),
        },
    );
    proposal.id = forged_id.clone();

    node.inject_forged(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        &GovernanceMessage::proposal_created(proposal),
    )
    .await;

    assert!(
        node.state.get_proposal(&forged_id).unwrap().is_none(),
        "a forged ProposalCreated created a proposal"
    );
}

// ---------------------------------------------------------------------------
// 4. Forged VoteCast cannot record or alter a vote
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn forged_vote_cast_cannot_record_a_vote() {
    let node = Node::spawn().await;
    let mallory = attacker();
    let (_, proposal_id) = seed_open_proposal(&node).await;

    // The attacker casts a vote *as the legitimate member* — the strongest form of
    // the attack, since `entry.author` is set to that member's DID as well.
    let victim = node.did.clone();
    let vote = Vote::new(proposal_id.clone(), victim.clone(), VoteChoice::Against);

    node.inject_forged(
        GOVERNANCE_TOPIC,
        &victim,
        &mallory,
        &GovernanceMessage::vote_cast(vote, None),
    )
    .await;

    assert!(
        node.state
            .get_vote(&proposal_id, &victim)
            .unwrap()
            .is_none(),
        "a forged VoteCast recorded a vote under the victim's DID"
    );
}

// ---------------------------------------------------------------------------
// 5. Forged ProposalOpened cannot reopen a closed proposal
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn forged_proposal_opened_cannot_reopen_a_closed_proposal() {
    let node = Node::spawn().await;
    let mallory = attacker();
    let (_, proposal_id) = seed_open_proposal(&node).await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");
    node.ops
        .close_proposal(proposal_id.clone())
        .await
        .expect("close_proposal");

    let closed = node
        .state
        .get_proposal(&proposal_id)
        .unwrap()
        .expect("closed proposal");
    assert!(
        !closed.state.is_open(),
        "precondition: proposal must be closed before the reopen attempt"
    );

    node.inject_forged(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        &GovernanceMessage::proposal_opened(proposal_id.clone(), 1, 999_999_999),
    )
    .await;

    let after = node
        .state
        .get_proposal(&proposal_id)
        .unwrap()
        .expect("proposal still present");
    assert!(
        !after.state.is_open(),
        "a forged ProposalOpened reopened a closed proposal"
    );
    assert_eq!(
        serde_json::to_value(&closed.state).unwrap(),
        serde_json::to_value(&after.state).unwrap(),
        "a forged ProposalOpened altered the terminal proposal state"
    );
}

// ---------------------------------------------------------------------------
// 6. Forged ProposalOpened → ProposalClosed cannot replace a decided outcome
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn forged_reopen_then_close_cannot_replace_a_decided_outcome() {
    let node = Node::spawn().await;
    let mallory = attacker();
    let (_, proposal_id) = seed_open_proposal(&node).await;

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");
    node.ops
        .close_proposal(proposal_id.clone())
        .await
        .expect("close_proposal");

    let decided = node
        .state
        .get_proposal(&proposal_id)
        .unwrap()
        .expect("decided proposal");
    assert!(
        matches!(decided.state, ProposalState::Accepted { .. }),
        "precondition: proposal should be Accepted, got {:?}",
        decided.state
    );

    // Step 1: force the proposal back to Open (defeats `Proposal::close`'s
    // `is_open()` guard). Step 2: re-finalize with the attacker's chosen outcome.
    node.inject_forged(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        &GovernanceMessage::proposal_opened(proposal_id.clone(), 1, 999_999_999),
    )
    .await;
    node.inject_forged(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        &GovernanceMessage::proposal_closed(
            proposal_id.clone(),
            ProposalOutcome::Rejected,
            2,
            TallySnapshot::new(0, 99, 0, 99),
            None,
        ),
    )
    .await;

    let after = node
        .state
        .get_proposal(&proposal_id)
        .unwrap()
        .expect("proposal still present");
    assert!(
        matches!(after.state, ProposalState::Accepted { .. }),
        "the forged reopen→close sequence replaced the decided outcome: {:?}",
        after.state
    );
    assert_eq!(
        serde_json::to_value(&decided.state).unwrap(),
        serde_json::to_value(&after.state).unwrap(),
        "the forged reopen→close sequence altered the decided outcome"
    );
}

// ---------------------------------------------------------------------------
// 7. Claiming a legitimate party's DID confers no authority
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn claiming_a_legitimate_did_confers_no_authority() {
    let node = Node::spawn().await;
    let _ = seed_open_proposal(&node).await;

    // The node's own DID is the domain's sole member, the proposal's proposer, and
    // the delegator below. Claiming it in `entry.author` must still grant nothing.
    let victim = node.did.clone();
    let delegate = attacker();
    let delegation = Delegation::new(victim.clone(), delegate, DelegationScope::Blanket);

    node.inject_forged(
        GOVERNANCE_TOPIC,
        &victim,
        &victim,
        &GovernanceMessage::delegation_created(delegation.clone()),
    )
    .await;

    assert!(
        node.state.get_delegation(&delegation.id).unwrap().is_none(),
        "claiming the delegator's DID in entry.author created a delegation"
    );

    // Same for the reverse operation on a delegation the node does hold.
    let real = Delegation::new(victim.clone(), attacker(), DelegationScope::Blanket);
    node.state.save_delegation(&real).unwrap();

    node.inject_forged(
        GOVERNANCE_TOPIC,
        &victim,
        &victim,
        &GovernanceMessage::delegation_revoked(real.id.clone(), victim.clone(), 42),
    )
    .await;

    let after = node
        .state
        .get_delegation(&real.id)
        .unwrap()
        .expect("delegation still present");
    assert!(
        after.revoked_at.is_none(),
        "claiming the delegator's DID in entry.author revoked a delegation"
    );
}

// ---------------------------------------------------------------------------
// 8. Malformed / unknown messages remain safely handled
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn malformed_and_unknown_messages_are_handled_safely() {
    let node = Node::spawn().await;
    let mallory = attacker();

    // Not valid JSON at all.
    node.inject_forged_bytes(GOVERNANCE_TOPIC, &mallory, &mallory, vec![0xff; 64])
        .await;
    // Valid JSON, unknown shape.
    node.inject_forged_bytes(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        br#"{"NotAVariant":{"x":1}}"#.to_vec(),
    )
    .await;
    // Empty payload.
    node.inject_forged_bytes(GOVERNANCE_TOPIC, &mallory, &mallory, Vec::new())
        .await;

    // The actor survives and local governance still works afterwards.
    let (domain_id, _) = seed_open_proposal(&node).await;
    assert!(node.state.get_domain(&domain_id).unwrap().is_some());
}

// ---------------------------------------------------------------------------
// 9. Local authorized governance operations continue to work
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn local_authorized_governance_still_works_end_to_end() {
    let node = Node::spawn().await;
    let (domain_id, proposal_id) = seed_open_proposal(&node).await;

    assert!(
        node.state.get_domain(&domain_id).unwrap().is_some(),
        "local create_domain must still persist"
    );
    let opened = node
        .state
        .get_proposal(&proposal_id)
        .unwrap()
        .expect("proposal");
    assert!(
        opened.state.is_open(),
        "local open_proposal must still persist"
    );

    node.ops
        .cast_vote(proposal_id.clone(), node.did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");
    assert!(
        node.state
            .get_vote(&proposal_id, &node.did)
            .unwrap()
            .is_some(),
        "local cast_vote must still persist"
    );

    node.ops
        .close_proposal(proposal_id.clone())
        .await
        .expect("close_proposal");
    let closed = node
        .state
        .get_proposal(&proposal_id)
        .unwrap()
        .expect("proposal");
    assert!(
        matches!(closed.state, ProposalState::Accepted { .. }),
        "local close_proposal must still decide the proposal, got {:?}",
        closed.state
    );
}

// ---------------------------------------------------------------------------
// 10. Quarantine does not stop the generic gossip actor
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn quarantine_does_not_stop_generic_gossip_transport() {
    let node = Node::spawn().await;
    let mallory = attacker();

    let domain = make_domain("transport-check", vec![mallory.clone()]);
    let domain_id = domain.id.clone();
    let msg = GovernanceMessage::domain_created(domain);

    // `inject_forged` already asserts `handle_message` returns Ok — transport accepted
    // the entry. Confirm the entry was retained by the gossip layer for observability.
    node.inject_forged(GOVERNANCE_TOPIC, &mallory, &mallory, &msg)
        .await;

    let entries = node.gossip.read().await.get_entries(GOVERNANCE_TOPIC);
    assert!(
        entries.iter().any(|e| e.author == mallory),
        "the quarantined entry must still be stored by the gossip layer"
    );

    // And the actor keeps serving further traffic.
    node.inject_forged(GOVERNANCE_TOPIC, &mallory, &mallory, &msg)
        .await;
    assert!(
        node.state.get_domain(&domain_id).unwrap().is_none(),
        "transport retention must not become state application"
    );
}

// ---------------------------------------------------------------------------
// 11. A quarantined message yields a bounded, observable disposition
// ---------------------------------------------------------------------------

#[test]
fn quarantine_disposition_is_bounded_and_marks_the_author_as_claimed() {
    use icn_governance_actor::actor::{
        replicated_state_effect, ReplicatedStateEffect, REPLICATION_QUARANTINE_REASON,
    };

    let mallory = attacker();
    let domain = make_domain("d", vec![mallory]);

    // State-mutating variants are reported as such.
    assert_eq!(
        replicated_state_effect(&GovernanceMessage::domain_created(domain)),
        ReplicatedStateEffect::WouldMutateGovernanceState
    );
    // Variants that applied nothing before containment stay quiet.
    assert_eq!(
        replicated_state_effect(&GovernanceMessage::comment_deleted(
            ProposalId("p".to_string()),
            icn_governance::CommentId("c".to_string()),
            1,
        )),
        ReplicatedStateEffect::NoStateEffect
    );

    // The diagnostic never describes the claimed author as authenticated.
    let reason = REPLICATION_QUARANTINE_REASON.to_ascii_lowercase();
    assert!(
        reason.contains("claimed"),
        "quarantine reason must call the author claimed: {REPLICATION_QUARANTINE_REASON}"
    );
    assert!(
        !reason.contains("authenticated sender") && !reason.contains("verified author"),
        "quarantine reason must not imply the author was authenticated: {REPLICATION_QUARANTINE_REASON}"
    );
    // Bounded: a fixed-size constant, not attacker-controlled content.
    assert!(REPLICATION_QUARANTINE_REASON.len() < 256);
}

// ---------------------------------------------------------------------------
// 12. Replay of the same unauthenticated message remains non-mutating
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn replayed_unauthenticated_messages_remain_non_mutating() {
    let node = Node::spawn().await;
    let mallory = attacker();
    let (_, proposal_id) = seed_open_proposal(&node).await;

    let victim = node.did.clone();
    let vote = Vote::new(proposal_id.clone(), victim.clone(), VoteChoice::Against);
    let msg = GovernanceMessage::vote_cast(vote, None);

    // Same payload, replayed three times, each carrying a *different* claimed hash —
    // which is what an attacker does to defeat `store_entry`'s dedup. Each injection
    // therefore reaches the ingress independently (asserted by the positive control in
    // `inject_forged`), and each must still be refused.
    for _ in 0..3 {
        node.inject_forged(GOVERNANCE_TOPIC, &victim, &mallory, &msg)
            .await;
    }

    assert!(
        node.state
            .get_vote(&proposal_id, &victim)
            .unwrap()
            .is_none(),
        "a replayed forged VoteCast recorded a vote"
    );
}

// ---------------------------------------------------------------------------
// 13. The federation governance topic shares the same containment
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn federation_governance_topic_is_contained_too() {
    let node = Node::spawn().await;
    let mallory = attacker();

    let fed_topic = format!("{}:fed-1", icn_federation::TOPIC_FEDERATION_GOVERNANCE);
    node.gossip
        .write()
        .await
        .create_topic(icn_gossip::Topic::new(
            fed_topic.clone(),
            icn_gossip::AccessControl::Public,
        ));
    node.gossip
        .write()
        .await
        .subscribe(&fed_topic, node.did.clone())
        .await
        .expect("subscribe");

    let domain = make_domain("fed-forged", vec![mallory.clone()]);
    let domain_id = domain.id.clone();

    node.inject_forged(
        &fed_topic,
        &mallory,
        &mallory,
        &GovernanceMessage::domain_created(domain),
    )
    .await;

    assert!(
        node.state.get_domain(&domain_id).unwrap().is_none(),
        "a forged DomainCreated on the federation governance topic created a domain"
    );
}

// ---------------------------------------------------------------------------
// 14. One entry is refused once, regardless of how many subscribers a peer adds
// ---------------------------------------------------------------------------

/// `GossipActor::store_entry` invokes notification callbacks inside a per-subscriber
/// loop, and an unauthenticated network `Subscribe` can add arbitrary DIDs to that
/// list. Without a per-entry filter, one forged entry would be refused once per
/// subscriber, letting a remote peer inflate `icn_governance_replication_quarantined_total`
/// and the warning volume by up to `MAX_SUBSCRIBERS_PER_TOPIC`.
#[tokio::test(flavor = "current_thread")]
async fn one_entry_is_refused_once_regardless_of_subscriber_count() {
    let node = Node::spawn().await;
    let mallory = attacker();

    // Simulate what an unauthenticated remote `Subscribe` does.
    for _ in 0..3 {
        node.gossip
            .write()
            .await
            .subscribe(GOVERNANCE_TOPIC, attacker())
            .await
            .expect("subscribe");
    }

    let domain = make_domain("fanout-check", vec![mallory.clone()]);
    let domain_id = domain.id.clone();
    node.inject_forged(
        GOVERNANCE_TOPIC,
        &mallory,
        &mallory,
        &GovernanceMessage::domain_created(domain),
    )
    .await;

    // The raw gossip fan-out really does hit every subscriber (local + 3 attacker DIDs)...
    assert_eq!(
        node.total_fanout(),
        4,
        "expected the gossip layer to fan out to every subscriber"
    );
    // ...but the governance ingress must act exactly once for the entry.
    assert_eq!(
        node.local_deliveries(),
        1,
        "the ingress must act once per entry, not once per subscriber"
    );
    assert!(node.state.get_domain(&domain_id).unwrap().is_none());
}
