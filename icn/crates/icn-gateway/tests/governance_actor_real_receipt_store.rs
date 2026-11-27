//! The production governance actor over the gateway's real sled `ReceiptStore`.
//!
//! Every test here drives `GovernanceActor` over a real sled governance state
//! store with the real `icn_gateway::receipt_store::ReceiptStore` installed as
//! its receipt backend (no in-memory recorder). Decisions, mandates, grants,
//! revocations and institutional-effect records are read back from that store
//! and from the actor's own decision-commitment source, and checked through
//! the production `DefaultMandateGate`.
//!
//! Limits that apply to the whole suite:
//!
//! * A "restart" flushes both stores, drops every handle on both sled `Db`s
//!   and reopens them from disk in the same process. It is flush-then-reopen,
//!   not crash or torn-write durability.
//! * Records the test writes directly (planted proposals, an orphan grant and
//!   mandate, a conflicting decision receipt) prove how the runtime reacts to
//!   that stored state. They do not prove that any production path issues
//!   such records; the Execution-class grant exists only because a test wrote
//!   it.
//! * Uniqueness claims are about the final stored state (one receipt, one
//!   mandate, one grant). Nothing here proves a record is write-once.
//! * The only proposal type that derives a grant is SDIS steward appointment,
//!   whose grant is Attestation-class. The gate never accepts Attestation for
//!   a gated act, so no test claims the gate authorizes an appointment.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use icn_gateway::receipt_store::{MandateIdentityClass, ReceiptStore};
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    proof::ProofOutcome, sdis::SdisProposal, AuthorityClass, AuthorityGrant, AuthorityGrantId,
    DecisionProvenance, ForcedOutcome, GovernanceDecisionReceipt, GovernanceDomainId,
    GovernanceError, GovernanceOps, GovernanceParams, Grantee, GrantorEntityId, Mandate,
    MembershipConfig, Proposal, ProposalId, ProposalOutcome, ProposalPayload, ProposalScope,
    ProposalState, StaticMembershipResolver, TypedScope, Vote, VoteChoice, VoteTally,
};
use icn_governance_actor::{
    actor::GovernanceActor,
    decision_commitment::DecisionCommitment,
    grant_minting::MANDATE_IDENTITY_TAKEN,
    mandate_gate::{
        DefaultMandateGate, MandateAct, MandateGate, MandateGateError, MandateRejection,
        MandateRequest, MandateTarget,
    },
    receipt_backend::GovernanceReceiptBackend,
    state_store::{GovernanceStateStore, SledGovernanceStateStore},
    EvidenceReadiness, GovernanceCommand, GovernanceHandle,
};
use icn_identity::{Did, IdentityBundle};
use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};

const DOMAIN: &str = "crosslane-domain";
const OTHER_DOMAIN: &str = "crosslane-other-domain";
/// Proposal id named by the planted orphan mandate; no such proposal exists.
const ORPHAN_PROPOSAL: &str = "prop-never-decided";
/// The scheduler ticks every 10 s; allow two ticks plus slack.
const CLOSE_WAIT: Duration = Duration::from_secs(25);

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Sled-backed `Store` that can be armed to fail writes to keys under one
/// prefix (`gov:proposal:` = the terminal proposal save).
struct PrefixFailStore {
    inner: SledStore,
    fail_prefix: Mutex<Option<Vec<u8>>>,
}

impl PrefixFailStore {
    fn new(inner: SledStore) -> Self {
        Self {
            inner,
            fail_prefix: Mutex::new(None),
        }
    }
    fn arm(&self, prefix: &[u8]) {
        *self.fail_prefix.lock().unwrap() = Some(prefix.to_vec());
    }
    fn fails(&self, key: &[u8]) -> bool {
        matches!(&*self.fail_prefix.lock().unwrap(), Some(p) if key.starts_with(p))
    }
}

impl Store for PrefixFailStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        if self.fails(key) {
            anyhow::bail!("injected write failure for prefixed key");
        }
        self.inner.put(key, value)
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        self.inner.delete_atomic(keys)
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
    }
    fn compare_and_put(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new: &[u8],
    ) -> anyhow::Result<icn_store::CompareAndPut> {
        if self.fails(key) {
            anyhow::bail!("injected write failure for prefixed key");
        }
        self.inner.compare_and_put(key, expected, new)
    }
    fn get_replica_metadata(&self, h: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
        self.inner.get_replica_metadata(h)
    }
    fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
        self.inner.put_replica_metadata(m)
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
        self.inner.list_replica_hashes()
    }
    fn flush(&self) -> anyhow::Result<()> {
        self.inner.flush().map(|_| ())
    }
}

fn new_did() -> Did {
    IdentityBundle::generate()
        .expect("generate identity")
        .did()
        .clone()
}

fn now() -> u64 {
    icn_time::current_timestamp_secs()
}

async fn gossip(did: Did) -> Arc<tokio::sync::RwLock<GossipActor>> {
    let gossip = GossipActor::spawn(did, None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    gossip
}

/// The two on-disk databases: the governance state store and the gateway
/// receipt store.
struct Dirs {
    _tmp: tempfile::TempDir,
    state: PathBuf,
    receipts: PathBuf,
}

fn dirs() -> Dirs {
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = tmp.path().join("state");
    let receipts = tmp.path().join("receipts");
    Dirs {
        _tmp: tmp,
        state,
        receipts,
    }
}

/// The real gateway receipt store over a sled `Db` opened from `path`.
fn open_receipts(path: &Path) -> Arc<ReceiptStore> {
    let db = sled::open(path).expect("open receipt sled Db (previous handles released?)");
    Arc::new(ReceiptStore::new(db))
}

/// A running production actor with the real receipt store installed.
struct Node {
    handle: GovernanceHandle,
    fail_store: Arc<PrefixFailStore>,
    receipts: Arc<ReceiptStore>,
}

/// Spawn the actor over the state `Db` at `state_path`, install `receipts`
/// and, if `recover`, run startup close recovery (which also releases the
/// scheduler).
async fn spawn_node(state_path: &Path, receipts: Arc<ReceiptStore>, recover: bool) -> Node {
    let fail_store = Arc::new(PrefixFailStore::new(
        SledStore::open(state_path).expect("open state sled Db (previous handles released?)"),
    ));
    let store: Arc<dyn Store> = fail_store.clone();
    let did = new_did();
    let handle = GovernanceActor::spawn(
        did.clone(),
        store,
        gossip(did).await,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .expect("spawn governance actor");
    handle.install_receipt_store(receipts.clone()).await;
    if recover {
        handle.recover_incomplete_closes().await;
    }
    Node {
        handle,
        fail_store,
        receipts,
    }
}

impl Node {
    fn state(&self) -> SledGovernanceStateStore {
        SledGovernanceStateStore::new(self.fail_store.clone())
    }

    fn stored_state(&self, id: &ProposalId) -> ProposalState {
        self.state()
            .get_proposal(id)
            .expect("read proposal")
            .expect("proposal exists")
            .state
    }

    fn commitment(&self, id: &ProposalId) -> DecisionCommitment {
        self.handle
            .decision_commitments()
            .decision_commitment(&id.0)
            .expect("commitment source reads")
    }

    fn gate_require(&self, req: &MandateRequest) -> Result<(), MandateGateError> {
        let source = self.handle.decision_commitments();
        let backend: Arc<dyn GovernanceReceiptBackend> = self.receipts.clone();
        DefaultMandateGate::new(backend, source.as_ref())
            .require(req)
            .map(|_| ())
    }

    /// Flush both stores, shut the actor down and drop every handle on both
    /// sled `Db`s, so they can be reopened from disk.
    async fn release(self) {
        let Node {
            handle,
            fail_store,
            receipts,
        } = self;
        fail_store.flush().expect("flush state store");
        GovernanceReceiptBackend::flush(receipts.as_ref()).expect("flush receipt store");
        handle.shutdown().await;
        drop(handle);
        drop(fail_store);
        assert_eq!(
            Arc::strong_count(&receipts),
            1,
            "the actor must have released the receipt store before reopen"
        );
        drop(receipts);
    }

    /// Release both databases and reopen them from disk with a fresh actor;
    /// install the reopened receipt store and run recovery if `recover`.
    async fn restart(self, d: &Dirs, recover: bool) -> Node {
        self.release().await;
        spawn_node(&d.state, open_receipts(&d.receipts), recover).await
    }

    async fn ensure_domain(&self, domain: &str, member: &Did) {
        let ops: &dyn GovernanceOps = &self.handle;
        // A domain already created earlier in the same test is fine.
        let _ = ops
            .create_domain(
                GovernanceDomainId(domain.into()),
                "Cross-lane".into(),
                "cooperative_default".into(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![member.clone()]),
            )
            .await;
    }

    /// Create and open a proposal carrying `payload` in `domain`.
    async fn propose(&self, domain: &str, payload: ProposalPayload) -> ProposalId {
        let ops: &dyn GovernanceOps = &self.handle;
        let id = ops
            .create_proposal(
                GovernanceDomainId(domain.into()),
                "Cross-lane proposal".into(),
                "d".into(),
                payload,
                ProposalScope::Local,
            )
            .await
            .expect("create proposal");
        ops.open_proposal(id.clone(), 3600)
            .await
            .expect("open proposal");
        id
    }

    async fn vote(&self, id: &ProposalId, member: &Did, choice: VoteChoice) {
        let ops: &dyn GovernanceOps = &self.handle;
        ops.cast_vote(id.clone(), member.clone(), choice, None)
            .await
            .expect("cast vote");
    }

    async fn close(&self, id: &ProposalId) -> anyhow::Result<()> {
        self.handle
            .submit(GovernanceCommand::CloseProposal {
                proposal_id: id.clone(),
                eligible_voters: None,
                excluded_delegators: None,
                capability_scope: None,
            })
            .await
    }

    /// Propose `payload`, vote For as `member`, close; expect Accepted.
    async fn decide(&self, domain: &str, member: &Did, payload: ProposalPayload) -> ProposalId {
        let id = self.propose(domain, payload).await;
        self.vote(&id, member, VoteChoice::For).await;
        self.close(&id).await.expect("close proposal");
        let s = self.stored_state(&id);
        assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
        id
    }

    fn mandate(&self, id: &ProposalId) -> Mandate {
        self.receipts
            .get_mandate_by_proposal(&id.0)
            .expect("read mandate")
            .expect("mandate recorded")
    }

    fn grants_of(&self, did: &Did) -> Vec<AuthorityGrant> {
        self.receipts
            .list_authority_grants_by_grantee(&Grantee::Person(did.clone()))
            .expect("list grants")
    }

    fn receipt_count(&self, id: &ProposalId) -> usize {
        self.receipts
            .list_governance_by_proposal(&id.0)
            .expect("list receipts")
            .len()
    }

    /// The proposal's mandate identity row: exactly one stored mandate, keyed.
    /// Every other row in the store must also be keyed to a single mandate.
    fn assert_identity_clean(&self, id: &ProposalId) -> String {
        let report = self
            .receipts
            .mandate_identity_report()
            .expect("identity report");
        for row in &report {
            assert_eq!(row.class, MandateIdentityClass::Keyed, "{row:?}");
            assert_eq!(row.mandate_ids.len(), 1, "{row:?}");
        }
        let row = report
            .iter()
            .find(|r| r.proposal_id == id.0)
            .expect("the proposal has a mandate identity row");
        row.mandate_ids[0].clone()
    }
}

fn appoint(candidate: &Did, sponsor: &Did) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::AppointSteward {
            candidate: candidate.clone(),
            sponsors: vec![sponsor.clone()],
            region: "region-crosslane".into(),
            bond_amount: 1_000,
            term_length: 3600 * 24 * 30,
        },
    }
}

fn remove(steward: &Did) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::RemoveSteward {
            steward: steward.clone(),
            reason: "removed".into(),
            return_bond: false,
        },
    }
}

fn reconfirm(steward: &Did) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::ReconfirmSteward {
            steward: steward.clone(),
            new_term_end: now() + 3600 * 24 * 90,
            performance_notes: None,
        },
    }
}

fn reinstate(steward: &Did) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::ReinstateSteward {
            steward: steward.clone(),
            reason: "appeal upheld".into(),
        },
    }
}

fn request(actor: &Did, act: MandateAct, target: MandateTarget) -> MandateRequest {
    MandateRequest {
        actor: actor.clone(),
        domain: GovernanceDomainId(DOMAIN.into()),
        act,
        target,
        at: now(),
    }
}

fn rejected(r: MandateRejection) -> Result<(), MandateGateError> {
    Err(MandateGateError::Rejected(r))
}

/// An Execution-class grant for `steward`, issued in `DOMAIN`'s name and
/// naming a decision this runtime never made, written with its mandate through
/// the real `put_mandate_with_grants`.
fn plant_orphan(receipts: &ReceiptStore, steward: &Did) -> (AuthorityGrant, Mandate) {
    let t = now();
    let decision = DecisionProvenance {
        proposal_id: ORPHAN_PROPOSAL.into(),
        decision_hash: [0xD7; 32],
    };
    let grant = AuthorityGrant {
        id: AuthorityGrantId::new(),
        class: AuthorityClass::Execution,
        grantor: GrantorEntityId(DOMAIN.into()),
        grantee: Grantee::Person(steward.clone()),
        scope: TypedScope {
            domain: Some(GovernanceDomainId(DOMAIN.into())),
            proposal_class: vec!["Sdis".into()],
            ..TypedScope::default()
        },
        granted_by: Some(decision.clone()),
        valid_from: t.saturating_sub(100),
        valid_until: Some(t + 1_000_000),
        revoked_at: None,
    };
    let mandate = Mandate::new(decision, [0xA5; 32], vec![grant.id.clone()], None, None, t)
        .expect("non-empty mandate");
    GovernanceReceiptBackend::put_mandate_with_grants(
        receipts,
        &mandate,
        std::slice::from_ref(&grant),
    )
    .expect("plant orphan mandate and grant");
    (grant, mandate)
}

/// A decision receipt for `id` that verifies against its own fields but
/// records a different outcome and tally than the actor's close.
fn conflicting_receipt(id: &ProposalId) -> GovernanceDecisionReceipt {
    let r = GovernanceDecisionReceipt::new(
        id.0.clone(),
        DOMAIN.into(),
        ProofOutcome::Rejected,
        VoteTally::new(0, 3, 0),
        &[],
    );
    assert!(r.verify(), "the conflicting receipt must verify");
    r
}

async fn wait_closed(node: &Node, id: &ProposalId) -> ProposalState {
    let deadline = tokio::time::Instant::now() + CLOSE_WAIT;
    loop {
        let s = node.stored_state(id);
        if !s.is_open() || tokio::time::Instant::now() >= deadline {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Write an Open steward-appointment proposal (already past its deadline)
/// and an accepting vote straight into the state `Db` at `path`. The `Db`
/// must not be open elsewhere.
fn plant_overdue_appointment(path: &Path, member: &Did, candidate: &Did) -> ProposalId {
    let state = SledGovernanceStateStore::new(Arc::new(
        SledStore::open(path).expect("open state Db for planting"),
    ));
    let t = now();
    let mut p = Proposal::new(
        GovernanceDomainId(DOMAIN.into()),
        member.clone(),
        "overdue appointment".into(),
        "d".into(),
        appoint(candidate, member),
    );
    p.state = ProposalState::Open {
        opened_at: t - 7200,
        closes_at: t - 60,
    };
    state.create_proposal(&p).expect("plant proposal");
    state
        .save_vote(
            &p.id,
            &Vote::new(p.id.clone(), member.clone(), VoteChoice::For),
        )
        .expect("plant vote");
    state.flush().expect("flush planted state");
    p.id
}

// ---------------------------------------------------------------------------
// 1. Decision -> exactly one bound mandate and grant
// ---------------------------------------------------------------------------

/// A real appointment close writes exactly one decision receipt, one mandate
/// bound to that decision and one grant whose provenance is the mandate's
/// decision, all in the real receipt store; the actor's commitment source
/// reports the decision as finalized and bound to the mandate.
///
/// Does NOT show the gate authorizing anything: the derived grant is
/// Attestation-class, so each gate answer is asserted as the specific
/// rejection it gives. On the Proposal target the commitment check runs
/// before the actor and class checks, so WrongTarget (grantee) and WrongActor
/// (anyone else) show the commitment stage was passed. On the Domain target
/// the class/binding filter runs before any commitment lookup, so its
/// WrongTarget shows only that an Attestation grant does not bind an
/// Execution act; it says nothing about the commitment.
#[tokio::test]
async fn test_actor_binds_one_mandate_and_grant_when_appointment_closes() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, candidate) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    let id = node
        .decide(DOMAIN, &member, appoint(&candidate, &member))
        .await;

    let receipts = node
        .receipts
        .list_governance_by_proposal(&id.0)
        .expect("receipts");
    assert_eq!(receipts.len(), 1, "{receipts:?}");
    let mandate = node.mandate(&id);
    assert_eq!(node.assert_identity_clean(&id), mandate.id.0.to_string());
    assert_eq!(mandate.decision.proposal_id, id.0);
    assert_eq!(mandate.decision.decision_hash, receipts[0].decision_hash);
    assert_eq!(mandate.grants.len(), 1, "{mandate:?}");

    let grants = node.grants_of(&candidate);
    assert_eq!(grants.len(), 1, "{grants:?}");
    assert_eq!(grants[0].id, mandate.grants[0]);
    assert_eq!(grants[0].granted_by, Some(mandate.decision.clone()));
    assert_eq!(grants[0].class, AuthorityClass::Attestation);

    let c = node.commitment(&id);
    assert!(
        matches!(c, DecisionCommitment::FinalizedAccepted(_)),
        "{c:?}"
    );
    assert!(c.authorizes(&mandate));

    // The gate's exact answers. None is an authorization.
    let target = MandateTarget::Proposal(id.clone());
    assert_eq!(
        node.gate_require(&request(
            &candidate,
            MandateAct::AppointSteward,
            target.clone()
        )),
        rejected(MandateRejection::WrongTarget)
    );
    assert_eq!(
        node.gate_require(&request(&member, MandateAct::AppointSteward, target)),
        rejected(MandateRejection::WrongActor)
    );
    assert_eq!(
        node.gate_require(&request(
            &candidate,
            MandateAct::AppointSteward,
            MandateTarget::Domain(GovernanceDomainId(DOMAIN.into()))
        )),
        rejected(MandateRejection::WrongTarget)
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// 2. Ordinary close interrupted, then reopened and recovered
// ---------------------------------------------------------------------------

/// An ordinary appointment close whose terminal proposal save fails returns
/// an error and leaves a durable close intent. After both databases are
/// flushed, released and reopened from disk, startup recovery over the real
/// receipt store completes the same decision: Accepted, no intent left,
/// exactly one receipt, one mandate (identity keyed) and one grant bound to
/// that mandate's decision.
///
/// Does NOT prove crash durability (the reopen follows a flush) or that any
/// record was written only once; only the final stored state is observed.
#[tokio::test]
async fn test_recovery_completes_close_once_when_terminal_save_failed_before_reopen() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, candidate) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    let id = node.propose(DOMAIN, appoint(&candidate, &member)).await;
    node.vote(&id, &member, VoteChoice::For).await;

    node.fail_store.arm(b"gov:proposal:");
    let err = node
        .close(&id)
        .await
        .expect_err("the terminal save fails")
        .to_string();
    assert!(err.contains("injected write failure"), "{err}");
    assert!(node.stored_state(&id).is_open());
    assert_eq!(node.state().list_close_intents().expect("intents").len(), 1);
    // Written ahead of the failed terminal save: recovery below must complete
    // over these, not write them for the first time.
    assert_eq!(node.receipt_count(&id), 1);
    let before = node.mandate(&id);
    let grants_before = node.grants_of(&candidate);
    assert_eq!(grants_before.len(), 1, "{grants_before:?}");
    assert_eq!(before.grants, vec![grants_before[0].id.clone()]);

    let node = node.restart(&d, true).await;

    let s = node.stored_state(&id);
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert!(node
        .state()
        .list_close_intents()
        .expect("intents")
        .is_empty());
    assert_eq!(node.receipt_count(&id), 1);
    let mandate = node.mandate(&id);
    assert_eq!(mandate, before, "recovery kept the pre-written mandate");
    assert_eq!(node.assert_identity_clean(&id), mandate.id.0.to_string());
    let grants = node.grants_of(&candidate);
    assert_eq!(grants, grants_before, "recovery kept the pre-written grant");
    assert_eq!(mandate.grants, vec![grants[0].id.clone()]);
    assert_eq!(grants[0].granted_by, Some(mandate.decision.clone()));
    assert!(node.commitment(&id).authorizes(&mandate));
    node.release().await;
}

// ---------------------------------------------------------------------------
// 3. Force-accept interrupted, then reopened and recovered
// ---------------------------------------------------------------------------

/// The force-accept twin of the ordinary-close test: the terminal save of a
/// forced Accept fails after the effect record, mandate and grant are
/// written; both databases are reopened from disk, and recovery completes it
/// as ForceClosed/Accepted keeping exactly that one institutional-effect
/// record, mandate and grant in the real receipt store.
///
/// Characterizes a documented gap: a force-accept writes no v1 decision
/// receipt, so once its intent is cleared the decision commitment is
/// `Unknown` and this mandate cannot pass the gate (asserted). It is not
/// committed authority. Force-accept has no production sender today; this
/// drives the actor command directly. Same limits: flush-then-reopen,
/// final-state uniqueness only.
#[tokio::test]
async fn test_recovery_completes_force_accept_once_when_terminal_save_failed_before_reopen() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, candidate) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    let id = node.propose(DOMAIN, appoint(&candidate, &member)).await;

    node.fail_store.arm(b"gov:proposal:");
    let err = node
        .handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "cross-lane force accept".into(),
        })
        .await
        .expect_err("the terminal save fails")
        .to_string();
    assert!(err.contains("injected write failure"), "{err}");
    assert!(node.stored_state(&id).is_open());
    assert_eq!(node.state().list_close_intents().expect("intents").len(), 1);
    // Written ahead of the failed terminal save.
    let effects_before = node
        .receipts
        .list_institutional_effects_by_proposal(&id.0)
        .expect("effects");
    assert_eq!(effects_before.len(), 1, "{effects_before:?}");
    let before = node.mandate(&id);
    let grants_before = node.grants_of(&candidate);
    assert_eq!(grants_before.len(), 1, "{grants_before:?}");
    assert_eq!(before.grants, vec![grants_before[0].id.clone()]);

    let node = node.restart(&d, true).await;

    let s = node.stored_state(&id);
    assert!(
        matches!(
            s,
            ProposalState::ForceClosed {
                outcome: ProposalOutcome::Accepted,
                ..
            }
        ),
        "{s:?}"
    );
    assert!(node
        .state()
        .list_close_intents()
        .expect("intents")
        .is_empty());
    let effects = node
        .receipts
        .list_institutional_effects_by_proposal(&id.0)
        .expect("effects");
    assert_eq!(effects.len(), 1, "{effects:?}");
    let mandate = node.mandate(&id);
    assert_eq!(mandate, before, "recovery kept the pre-written mandate");
    assert_eq!(node.assert_identity_clean(&id), mandate.id.0.to_string());
    let grants = node.grants_of(&candidate);
    assert_eq!(grants, grants_before, "recovery kept the pre-written grant");
    assert_eq!(mandate.grants, vec![grants[0].id.clone()]);
    assert_eq!(grants[0].granted_by, Some(mandate.decision.clone()));

    // The documented force-accept gap: no v1 receipt, commitment Unknown,
    // so the mandate is not authority at the gate.
    assert_eq!(node.receipt_count(&id), 0);
    let c = node.commitment(&id);
    assert!(matches!(c, DecisionCommitment::Unknown(_)), "{c:?}");
    assert!(!c.authorizes(&mandate));
    assert_eq!(
        node.gate_require(&request(
            &candidate,
            MandateAct::AppointSteward,
            MandateTarget::Proposal(id.clone())
        )),
        rejected(MandateRejection::DecisionNotCommitted)
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// 4. Orphan containment
// ---------------------------------------------------------------------------

/// A planted orphan (an Execution grant and its mandate naming a decision no
/// proposal record backs, written through the real `put_mandate_with_grants`)
/// is refused by the gate on both the Proposal and the Domain target with
/// DecisionNotCommitted. Real Reconfirm and Reinstate closes mint nothing
/// from it; a real RemoveSteward close revokes it (removal is scoped by the
/// grantor domain, not by provenance).
///
/// Proves the reaction to stored state only: no production path is shown to
/// issue such a grant, and the Execution class exists only because the test
/// wrote it.
#[tokio::test]
async fn test_gate_and_lifecycle_contain_orphan_when_grant_has_no_committed_decision() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, steward) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    let (orphan, _) = plant_orphan(&node.receipts, &steward);

    let domain_target = MandateTarget::Domain(GovernanceDomainId(DOMAIN.into()));
    let proposal_target = MandateTarget::Proposal(ProposalId(ORPHAN_PROPOSAL.into()));
    for (act, target) in [
        (MandateAct::AppointSteward, proposal_target.clone()),
        (MandateAct::AppointSteward, domain_target.clone()),
        (MandateAct::RemoveSteward, domain_target.clone()),
    ] {
        assert_eq!(
            node.gate_require(&request(&steward, act.clone(), target.clone())),
            rejected(MandateRejection::DecisionNotCommitted),
            "{act:?} on {target:?}"
        );
    }

    // Reconfirm: the orphan is the only active in-domain grant; nothing minted.
    let id = node.decide(DOMAIN, &member, reconfirm(&steward)).await;
    assert!(node.mandate(&id).grants.is_empty());
    assert_eq!(node.grants_of(&steward), vec![orphan.clone()]);

    // Remove: the orphan is revoked; nothing else changes.
    let id = node.decide(DOMAIN, &member, remove(&steward)).await;
    assert!(node.mandate(&id).grants.is_empty());
    let after_remove = node.grants_of(&steward);
    assert_eq!(after_remove.len(), 1, "{after_remove:?}");
    assert_eq!(after_remove[0].id, orphan.id);
    assert!(after_remove[0].revoked_at.is_some());

    // Reinstate: the revoked orphan is the only template; nothing minted.
    let id = node.decide(DOMAIN, &member, reinstate(&steward)).await;
    assert!(node.mandate(&id).grants.is_empty());
    assert_eq!(node.grants_of(&steward), after_remove);
    node.release().await;
}

/// Positive counterpart of the Reconfirm half of the orphan test: when the
/// steward's active in-domain grant comes from a committed appointment, a
/// real Reconfirm close mints exactly one fresh grant bound to the
/// reconfirmation mandate's decision, with class and scope copied. So the
/// orphan case minting nothing is a provenance refusal, not a reconfirmation
/// that never mints.
///
/// Does NOT show the gate authorizing the steward (Attestation-class).
#[tokio::test]
async fn test_reconfirm_mints_fresh_grant_when_template_comes_from_committed_appointment() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, steward) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    node.decide(DOMAIN, &member, appoint(&steward, &member))
        .await;
    let appointed = node.grants_of(&steward);
    assert_eq!(appointed.len(), 1, "{appointed:?}");

    let id = node.decide(DOMAIN, &member, reconfirm(&steward)).await;
    let mandate = node.mandate(&id);
    assert_eq!(mandate.grants.len(), 1, "{mandate:?}");
    let all = node.grants_of(&steward);
    assert_eq!(all.len(), 2, "{all:?}");
    let fresh = all
        .iter()
        .find(|g| g.id == mandate.grants[0])
        .expect("fresh grant stored");
    assert_ne!(fresh.id, appointed[0].id);
    assert_eq!(fresh.granted_by, Some(mandate.decision.clone()));
    assert_eq!(fresh.class, appointed[0].class);
    assert_eq!(fresh.scope, appointed[0].scope);
    assert!(node.commitment(&id).authorizes(&mandate));
    node.release().await;
}

// ---------------------------------------------------------------------------
// 5. Appoint -> Remove -> Reinstate
// ---------------------------------------------------------------------------

/// Through the actor and the real store: an appointment grant is revoked by a
/// removal, and a reinstatement mints a fresh grant (new id, provenance = the
/// reinstatement mandate's decision, class and scope copied) while the
/// original grant stays revoked and otherwise byte-for-byte unchanged.
///
/// Does NOT show the gate authorizing the reinstated steward (the grant is
/// Attestation-class).
#[tokio::test]
async fn test_reinstate_mints_fresh_grant_when_steward_was_appointed_and_removed() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, steward) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;

    node.decide(DOMAIN, &member, appoint(&steward, &member))
        .await;
    let appointed = node.grants_of(&steward);
    assert_eq!(appointed.len(), 1, "{appointed:?}");
    assert!(appointed[0].revoked_at.is_none());

    node.decide(DOMAIN, &member, remove(&steward)).await;
    let removed = node.grants_of(&steward);
    assert_eq!(removed.len(), 1, "{removed:?}");
    assert!(removed[0].revoked_at.is_some());
    let original = removed[0].clone();

    let id = node.decide(DOMAIN, &member, reinstate(&steward)).await;
    let mandate = node.mandate(&id);
    assert_eq!(mandate.grants.len(), 1, "{mandate:?}");
    let all = node.grants_of(&steward);
    assert_eq!(all.len(), 2, "{all:?}");
    let kept = all.iter().find(|g| g.id == original.id).expect("original");
    assert_eq!(kept, &original, "the original grant is not modified");
    let fresh = all.iter().find(|g| g.id != original.id).expect("fresh");
    assert_eq!(fresh.id, mandate.grants[0]);
    assert_eq!(fresh.granted_by, Some(mandate.decision.clone()));
    assert_eq!(fresh.class, original.class);
    assert_eq!(fresh.scope, original.scope);
    assert!(fresh.revoked_at.is_none());
    assert!(fresh.is_active_at(now()));
    assert!(node.commitment(&id).authorizes(&mandate));
    node.release().await;
}

// ---------------------------------------------------------------------------
// 6. No duplicate authority
// ---------------------------------------------------------------------------

/// Replaying the close, running recovery twice, and recovering again after a
/// reopen leave exactly one receipt, the same single mandate and the same
/// single grant. A second mandate (different id) for the decided proposal is
/// refused by the real store with an error starting MANDATE_IDENTITY_TAKEN,
/// and its grant is not written.
///
/// Final-state uniqueness only; not a write-once claim.
#[tokio::test]
async fn test_store_refuses_new_authority_when_close_and_recovery_are_replayed() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, candidate) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    let id = node
        .decide(DOMAIN, &member, appoint(&candidate, &member))
        .await;
    let mandate = node.mandate(&id);
    let grants = node.grants_of(&candidate);
    assert_eq!(grants.len(), 1);

    assert!(
        node.close(&id).await.is_err(),
        "a decided proposal re-closes"
    );
    node.handle.recover_incomplete_closes().await;
    node.handle.recover_incomplete_closes().await;
    let node = node.restart(&d, true).await;
    node.handle.recover_incomplete_closes().await;

    assert!(matches!(
        node.stored_state(&id),
        ProposalState::Accepted { .. }
    ));
    assert_eq!(node.receipt_count(&id), 1);
    assert_eq!(node.assert_identity_clean(&id), mandate.id.0.to_string());
    assert_eq!(node.mandate(&id), mandate);
    assert_eq!(node.grants_of(&candidate), grants);

    let mut extra = grants[0].clone();
    extra.id = AuthorityGrantId::new();
    let second = Mandate::new(
        mandate.decision.clone(),
        mandate.payload_hash,
        vec![extra.id.clone()],
        None,
        None,
        now(),
    )
    .expect("non-empty mandate");
    assert_ne!(second.id, mandate.id);
    let err = GovernanceReceiptBackend::put_mandate_with_grants(
        node.receipts.as_ref(),
        &second,
        std::slice::from_ref(&extra),
    )
    .expect_err("a second mandate for the proposal is refused");
    assert!(err.starts_with(MANDATE_IDENTITY_TAKEN), "{err}");
    assert!(node
        .receipts
        .get_authority_grant(&extra.id)
        .expect("read")
        .is_none());
    assert_eq!(node.mandate(&id), mandate);
    assert_eq!(node.grants_of(&candidate), grants);
    node.release().await;
}

// ---------------------------------------------------------------------------
// 7. Contradictory commitment from a verifying receipt
// ---------------------------------------------------------------------------

/// A second decision receipt for a decided proposal, built with
/// `GovernanceDecisionReceipt::new` so it verifies (and so the real store
/// accepts it), but recording another outcome and tally, makes the
/// commitment Contradictory: it no longer authorizes the real mandate, and
/// the gate's answer changes from the class check (WrongTarget) to
/// DecisionNotCommitted.
///
/// The conflicting receipt is test-written; no production path is shown to
/// produce one.
#[tokio::test]
async fn test_commitment_is_contradictory_when_a_verifying_conflicting_receipt_is_stored() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, candidate) = (new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    let id = node
        .decide(DOMAIN, &member, appoint(&candidate, &member))
        .await;
    let mandate = node.mandate(&id);
    let req = request(
        &candidate,
        MandateAct::AppointSteward,
        MandateTarget::Proposal(id.clone()),
    );
    assert_eq!(
        node.gate_require(&req),
        rejected(MandateRejection::WrongTarget)
    );

    node.receipts
        .put_governance(&conflicting_receipt(&id))
        .expect("the real store accepts a verifying receipt");
    assert_eq!(node.receipt_count(&id), 2);

    let c = node.commitment(&id);
    assert!(matches!(c, DecisionCommitment::Contradictory(_)), "{c:?}");
    assert!(!c.authorizes(&mandate));
    assert_eq!(
        node.gate_require(&req),
        rejected(MandateRejection::DecisionNotCommitted)
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// 8. Unrelated activity proceeds under containment
// ---------------------------------------------------------------------------

/// With an orphan grant (test 4) and a contradicted decision (test 7) held in
/// the same actor and store, an unrelated appointment in another domain and
/// an unrelated text proposal in the contained domain still close; the new
/// appointment gets its one bound mandate and grant and a finalized
/// commitment, while the containment answers are unchanged.
///
/// Same seeded-state limits as tests 4 and 7.
#[tokio::test]
async fn test_unrelated_governance_proceeds_when_containment_applies() {
    let d = dirs();
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    let (member, steward, candidate, other_member, other_candidate) =
        (new_did(), new_did(), new_did(), new_did(), new_did());
    node.ensure_domain(DOMAIN, &member).await;
    node.ensure_domain(OTHER_DOMAIN, &other_member).await;
    plant_orphan(&node.receipts, &steward);
    let contradicted = node
        .decide(DOMAIN, &member, appoint(&candidate, &member))
        .await;
    node.receipts
        .put_governance(&conflicting_receipt(&contradicted))
        .expect("plant conflicting receipt");

    let other = node
        .decide(
            OTHER_DOMAIN,
            &other_member,
            appoint(&other_candidate, &other_member),
        )
        .await;
    let mandate = node.mandate(&other);
    let grants = node.grants_of(&other_candidate);
    assert_eq!(grants.len(), 1, "{grants:?}");
    assert_eq!(mandate.grants, vec![grants[0].id.clone()]);
    assert_eq!(grants[0].granted_by, Some(mandate.decision.clone()));
    let c = node.commitment(&other);
    assert!(
        matches!(c, DecisionCommitment::FinalizedAccepted(_)),
        "{c:?}"
    );
    assert!(c.authorizes(&mandate));

    node.decide(
        DOMAIN,
        &member,
        ProposalPayload::Text {
            body: "unrelated".into(),
        },
    )
    .await;

    assert!(matches!(
        node.commitment(&contradicted),
        DecisionCommitment::Contradictory(_)
    ));
    assert_eq!(
        node.gate_require(&request(
            &steward,
            MandateAct::AppointSteward,
            MandateTarget::Domain(GovernanceDomainId(DOMAIN.into()))
        )),
        rejected(MandateRejection::DecisionNotCommitted)
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// 9. Overdue grant-bearing appointment: held, then recovered after restart
// ---------------------------------------------------------------------------

/// A planted overdue appointment with an accepting vote, on an actor whose
/// real receipt store is installed but whose evidence path is then marked
/// unavailable before recovery (`mark_evidence_unavailable`, the call the
/// supervisor makes for a configured gateway that failed): past at least one
/// scheduler tick it is still Open, a manual close is refused with
/// EvidenceUnavailable, and nothing is written. After both databases are
/// reopened and recovery runs, the scheduler closes it with exactly one
/// mandate and one grant.
///
/// The overdue proposal is planted state, not a production issuance path.
/// The supervisor's own startup step lives in icn-core, which this crate
/// cannot depend on; the readiness transition is driven on the handle.
#[tokio::test]
async fn test_overdue_appointment_is_held_then_closes_once_when_recovered_after_restart() {
    let d = dirs();
    let (member, candidate) = (new_did(), new_did());
    let node = spawn_node(&d.state, open_receipts(&d.receipts), true).await;
    node.ensure_domain(DOMAIN, &member).await;
    node.release().await;
    let id = plant_overdue_appointment(&d.state, &member, &candidate);

    let node = spawn_node(&d.state, open_receipts(&d.receipts), false).await;
    assert_eq!(
        node.handle.mark_evidence_unavailable(),
        EvidenceReadiness::Failed
    );
    tokio::time::sleep(Duration::from_secs(12)).await;
    assert!(
        node.stored_state(&id).is_open(),
        "held: the overdue proposal is preserved"
    );
    let err = node.close(&id).await.expect_err("close refused while held");
    assert!(
        matches!(
            err.downcast::<GovernanceError>(),
            Ok(GovernanceError::EvidenceUnavailable(_))
        ),
        "refusal is the typed evidence error"
    );
    assert!(node.stored_state(&id).is_open());
    assert!(node
        .receipts
        .get_mandate_by_proposal(&id.0)
        .expect("read")
        .is_none());
    assert!(node.grants_of(&candidate).is_empty());

    let node = node.restart(&d, true).await;
    assert_eq!(node.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let s = wait_closed(&node, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert_eq!(node.receipt_count(&id), 1);
    let mandate = node.mandate(&id);
    assert_eq!(node.assert_identity_clean(&id), mandate.id.0.to_string());
    let grants = node.grants_of(&candidate);
    assert_eq!(grants.len(), 1, "{grants:?}");
    assert_eq!(mandate.grants, vec![grants[0].id.clone()]);
    assert_eq!(grants[0].granted_by, Some(mandate.decision.clone()));
    node.release().await;
}
