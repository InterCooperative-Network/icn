//! P5b slice 2, increment 3: the lifecycle-aware governance state store (EXPERIMENTAL,
//! INACTIVE: no production construction).
//!
//! # The problem this solves
//! The fenced writers live on [`LifecycleStore`], which **borrows** a [`SledStore`] and must be
//! the database's only lifecycle owner. The governance actor holds an owned
//! `Arc<dyn GovernanceStateStore>` for its whole life, so it cannot retain a borrowing
//! `LifecycleStore` (that would be self-referential), and scattering raw handles through the
//! actor would create parallel authoritative writers.
//!
//! # The shape
//! * [`LifecycleOwner::start`] spawns one dedicated owner thread holding a [`SledStore::share`]
//!   handle (same fault domain, so the exclusive claim still covers every handle). Inside that
//!   thread's frame it claims the database, runs the safe recovery
//!   ([`LifecycleRecovery::resolve_safe`]), and only then goes live. If anything stays
//!   quarantined, or the claim is refused, start fails and **no** authoritative access exists:
//!   there is no degraded mode.
//! * The Seal evidence is bound when the owner starts ([`BoundSealEvidence`]); it is the owner's
//!   for its whole life and nothing installed later replaces it.
//! * [`FencedGovernanceStateStore`] implements the actor's existing [`GovernanceStateStore`]
//!   trait. Reads pass through. The five authoritative writes the lifecycle governs
//!   (`create_proposal`, `save_proposal`, `save_domain`, `save_close_intent`,
//!   `delete_close_intent`) become requests to the owner thread, which loads the domain's
//!   lifecycle and applies the fenced, journaled write. Every actor path that writes through
//!   its state store (direct commands, scheduler callbacks, startup close recovery) is therefore
//!   routed by construction, with one funnel and no lifecycle rule restated outside the store.
//! * Writes the lifecycle does not govern (votes, delegations, proof bytes, institutional
//!   domains, policy bodies) pass through unchanged (writer inventory W5, W16, W7-proof, W22).
//!
//! # What remains outside this funnel (recorded, not claimed)
//! Raw writers that construct their own `SledGovernanceStateStore` or `Store` handle over the
//! same database (replication emission, institutional-authority and domain-policy adoption
//! helpers, the restart helper, administrative tooling) do not pass through it. They do not bump
//! the lifecycle revision, so the Seal cannot see them through step 1; the domain pin catches a
//! domain edit, nothing catches a raw proposal write. They stay unrouted until each is migrated
//! or structurally excluded.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, Result};
use icn_governance::{
    Delegation, DelegationId, DomainPolicyId, GovernanceDomain, GovernanceDomainId,
    InstitutionalDomain, Proposal, ProposalId, Timestamp, Vote,
};
use icn_identity::Did;
use icn_store::{SledStore, Store};

use crate::close_journal::CloseJournalEntry;
use crate::predecessor_lifecycle_store::{
    LifecycleRecovery, LifecycleStore, LifecycleStoreError, RecoveryReport,
};
use crate::seal_evidence::BoundSealEvidence;
use crate::state_store::{
    proposal_key, GovernanceStateStore, ProposalWriteRefused, SledGovernanceStateStore,
};

/// How many times a routed write reloads and retries after ordinary contention (`Stale`). A
/// conflict writes nothing, so a retry is safe; contention is never escalated to quarantine.
const STALE_RETRIES: usize = 3;

/// One authoritative write routed to the owner.
enum Write {
    CreateProposal(Box<Proposal>),
    SaveProposal(Box<Proposal>),
    SaveDomain(Box<GovernanceDomain>),
    SaveCloseIntent(Box<CloseJournalEntry>),
    DeleteCloseIntent(ProposalId),
    /// Test-only lifecycle transitions (the operational Begin/Abort/Seal are institutional
    /// actions, W18, and are not exposed by the owner).
    #[cfg(test)]
    TestBegin {
        domain: GovernanceDomainId,
        attempt: u64,
        commitment: Box<icn_governance::cutover::CutoverCommitmentV1>,
    },
    #[cfg(test)]
    TestSeal {
        domain: GovernanceDomainId,
        attempt: u64,
        commitment_hash: [u8; 32],
    },
}

struct Request {
    write: Write,
    reply: mpsc::SyncSender<Result<()>>,
}

/// Why the owner did not start. No authoritative access exists in any of these cases.
#[derive(Debug)]
pub enum OwnerStartError {
    /// The database is already claimed by another lifecycle owner, or the claim failed.
    Claim(anyhow::Error),
    /// Recovery left records that need adjudication; they stay quarantined (P8).
    Quarantined(RecoveryReport),
    /// Recovery or admission failed.
    Recovery(anyhow::Error),
    /// The owner thread could not be started or died during start.
    Thread(String),
}

impl std::fmt::Display for OwnerStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claim(e) => write!(f, "lifecycle owner claim refused: {e:#}"),
            Self::Quarantined(r) => write!(
                f,
                "lifecycle recovery left {} quarantined record(s); no authoritative access",
                r.quarantined.len()
            ),
            Self::Recovery(e) => write!(f, "lifecycle recovery failed: {e:#}"),
            Self::Thread(e) => write!(f, "lifecycle owner thread: {e}"),
        }
    }
}

impl std::error::Error for OwnerStartError {}

/// A handle to the running lifecycle owner. Cloning shares the owner; when the last handle is
/// dropped the owner thread ends and releases the database claim.
#[derive(Clone)]
pub struct LifecycleOwner {
    tx: mpsc::Sender<Request>,
}

impl LifecycleOwner {
    /// Start the owner over `store` (it keeps a [`SledStore::share`] handle), bound to `evidence`.
    /// Returns only once recovery has resolved everything safely and the owner is live.
    pub fn start(store: &SledStore, evidence: BoundSealEvidence) -> Result<Self, OwnerStartError> {
        let owned = store.share();
        let (tx, rx) = mpsc::channel::<Request>();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), OwnerStartError>>(1);
        thread::Builder::new()
            .name("governance-lifecycle-owner".into())
            .spawn(move || owner_main(owned, evidence, rx, ready_tx))
            .map_err(|e| OwnerStartError::Thread(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { tx }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(OwnerStartError::Thread("died during start".into())),
        }
    }

    /// Test-only: Begin attempt `attempt` of `domain` with an unverified test electorate.
    #[cfg(test)]
    pub(crate) fn begin_for_tests(
        &self,
        domain: &GovernanceDomainId,
        attempt: u64,
        commitment: icn_governance::cutover::CutoverCommitmentV1,
    ) -> Result<()> {
        self.call(Write::TestBegin {
            domain: domain.clone(),
            attempt,
            commitment: Box::new(commitment),
        })
    }

    /// Test-only: Seal through the owner's bound evidence.
    #[cfg(test)]
    pub(crate) fn seal_for_tests(
        &self,
        domain: &GovernanceDomainId,
        attempt: u64,
        commitment_hash: [u8; 32],
    ) -> Result<()> {
        self.call(Write::TestSeal {
            domain: domain.clone(),
            attempt,
            commitment_hash,
        })
    }

    fn call(&self, write: Write) -> Result<()> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.tx
            .send(Request { write, reply })
            .map_err(|_| anyhow!("the lifecycle owner has stopped; no authoritative access"))?;
        answer
            .recv()
            .map_err(|_| anyhow!("the lifecycle owner stopped before answering"))?
    }
}

fn owner_main(
    store: SledStore,
    evidence: BoundSealEvidence,
    rx: mpsc::Receiver<Request>,
    ready: mpsc::SyncSender<Result<(), OwnerStartError>>,
) {
    let live = match start_live(&store, evidence) {
        Ok(live) => live,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    // Requests are served one at a time: the owner is the single authoritative writer.
    for req in rx {
        let _ = req.reply.send(apply(&live, &store, &req.write));
    }
}

fn start_live(
    store: &SledStore,
    evidence: BoundSealEvidence,
) -> Result<LifecycleStore<'_>, OwnerStartError> {
    let recovery = LifecycleRecovery::claim(store, evidence).map_err(OwnerStartError::Claim)?;
    let report = recovery.resolve_safe().map_err(OwnerStartError::Recovery)?;
    if !report.quarantined.is_empty() {
        return Err(OwnerStartError::Quarantined(report));
    }
    recovery
        .into_live()
        .map_err(|(_, e)| OwnerStartError::Recovery(e))
}

fn txid() -> [u8; 16] {
    *uuid::Uuid::new_v4().as_bytes()
}

/// Apply one routed write: load the governing domain's lifecycle and commit through the fenced
/// writer, retrying only ordinary contention.
fn apply(live: &LifecycleStore<'_>, store: &SledStore, write: &Write) -> Result<()> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match apply_once(live, store, write) {
            Err(e)
                if attempt < STALE_RETRIES
                    && e.downcast_ref::<LifecycleStoreError>()
                        == Some(&LifecycleStoreError::Stale) =>
            {
                continue
            }
            other => return other,
        }
    }
}

fn apply_once(live: &LifecycleStore<'_>, store: &SledStore, write: &Write) -> Result<()> {
    match write {
        Write::CreateProposal(p) => {
            // Create-if-absent (#2953): any existing row under the id is refused, even an
            // identical one, and left unchanged. The fenced write expects the absence it saw.
            if store.get(&proposal_key(&p.id))?.is_some() {
                return Err(ProposalWriteRefused::AlreadyExists {
                    proposal: p.id.0.clone(),
                }
                .into());
            }
            live.write_proposal(&live.load(&p.domain_id)?, p, txid())?;
        }
        Write::SaveProposal(p) => {
            live.write_proposal(&live.load(&p.domain_id)?, p, txid())?;
        }
        Write::SaveDomain(d) => {
            live.write_domain(&live.load(&d.id)?, d, txid())?;
        }
        Write::SaveCloseIntent(entry) => {
            live.write_close_intent(&live.load(&entry.proposal.domain_id)?, entry, txid())?;
        }
        Write::DeleteCloseIntent(id) => {
            // The intent names its domain; deleting a missing intent is idempotent (trait
            // contract). The fenced writer decides whether the intent may be retired.
            let Some(entry) =
                SledGovernanceStateStore::new(Arc::new(store.share())).get_close_intent(id)?
            else {
                return Ok(());
            };
            live.delete_close_intent(&live.load(&entry.proposal.domain_id)?, id, txid())?;
        }
        #[cfg(test)]
        Write::TestBegin {
            domain,
            attempt,
            commitment,
        } => {
            use crate::predecessor_lifecycle_store::{ElectorateCommitment, LifecycleRequest};
            let request = LifecycleRequest::Begin {
                attempt: *attempt,
                commitment: (**commitment).clone(),
                electorate: ElectorateCommitment::unverified_for_tests([0xE1; 32]),
            };
            live.transition(&live.load(domain)?, request, txid())?;
        }
        #[cfg(test)]
        Write::TestSeal {
            domain,
            attempt,
            commitment_hash,
        } => {
            live.seal(&live.load(domain)?, *attempt, *commitment_hash, txid())?;
        }
    }
    Ok(())
}

/// The lifecycle-aware [`GovernanceStateStore`] (INACTIVE: no production construction). See the
/// module docs for what it routes and what it does not.
pub struct FencedGovernanceStateStore {
    inner: SledGovernanceStateStore,
    owner: LifecycleOwner,
}

impl FencedGovernanceStateStore {
    /// Wrap `store` (the daemon's owning governance database) with a started owner.
    pub fn new(store: &SledStore, owner: LifecycleOwner) -> Self {
        Self {
            inner: SledGovernanceStateStore::new(Arc::new(store.share())),
            owner,
        }
    }
}

impl GovernanceStateStore for FencedGovernanceStateStore {
    // --- Routed through the lifecycle owner ---
    fn create_proposal(&self, proposal: &Proposal) -> Result<()> {
        self.owner
            .call(Write::CreateProposal(Box::new(proposal.clone())))
    }
    fn save_proposal(&self, proposal: &Proposal) -> Result<()> {
        self.owner
            .call(Write::SaveProposal(Box::new(proposal.clone())))
    }
    fn save_domain(&self, domain: &GovernanceDomain) -> Result<()> {
        self.owner.call(Write::SaveDomain(Box::new(domain.clone())))
    }
    fn save_close_intent(&self, entry: &CloseJournalEntry) -> Result<()> {
        self.owner
            .call(Write::SaveCloseIntent(Box::new(entry.clone())))
    }
    fn delete_close_intent(&self, proposal_id: &ProposalId) -> Result<()> {
        self.owner
            .call(Write::DeleteCloseIntent(proposal_id.clone()))
    }

    // --- Reads and lifecycle-neutral writes pass through ---
    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        self.inner.get_domain(id)
    }
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        self.inner.list_domains()
    }
    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        self.inner.get_proposal(id)
    }
    fn list_proposals(&self) -> Result<Vec<Proposal>> {
        self.inner.list_proposals()
    }
    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>> {
        self.inner.get_vote(proposal_id, voter)
    }
    fn save_vote(&self, proposal_id: &ProposalId, vote: &Vote) -> Result<()> {
        self.inner.save_vote(proposal_id, vote)
    }
    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>> {
        self.inner.list_votes(proposal_id)
    }
    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        self.inner.get_delegation(id)
    }
    fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        self.inner.save_delegation(delegation)
    }
    fn list_all_delegations(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.list_all_delegations()
    }
    fn save_revoked_delegation(
        &self,
        delegation: &Delegation,
        revoked_at: Timestamp,
    ) -> Result<()> {
        self.inner.save_revoked_delegation(delegation, revoked_at)
    }
    fn get_proof_bytes(&self, proposal_id: &ProposalId) -> Result<Option<Vec<u8>>> {
        self.inner.get_proof_bytes(proposal_id)
    }
    fn save_proof_bytes(&self, proposal_id: &ProposalId, proof: &[u8]) -> Result<()> {
        self.inner.save_proof_bytes(proposal_id, proof)
    }
    fn list_close_intents(&self) -> Result<Vec<CloseJournalEntry>> {
        self.inner.list_close_intents()
    }
    fn get_close_intent(&self, proposal_id: &ProposalId) -> Result<Option<CloseJournalEntry>> {
        self.inner.get_close_intent(proposal_id)
    }
    fn flush(&self) -> Result<()> {
        self.inner.flush()
    }
    fn get_institutional_domain(
        &self,
        id: &GovernanceDomainId,
    ) -> Result<Option<InstitutionalDomain>> {
        self.inner.get_institutional_domain(id)
    }
    fn save_institutional_domain(&self, domain: &InstitutionalDomain) -> Result<()> {
        self.inner.save_institutional_domain(domain)
    }
    fn get_domain_policy_body(&self, id: &DomainPolicyId) -> Result<Option<Vec<u8>>> {
        self.inner.get_domain_policy_body(id)
    }
    fn save_domain_policy_body(&self, id: &DomainPolicyId, content: &[u8]) -> Result<()> {
        self.inner.save_domain_policy_body(id, content)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::predecessor_lifecycle_store::tests::{commitment, k};
    use crate::predecessor_lifecycle_store::FenceRefusal;
    use crate::seal_evidence::tests::Backend;
    use crate::state_store::domain_key;
    use icn_governance::ProposalState;

    const L: &str = "legacy.coop.example";

    fn l() -> GovernanceDomainId {
        GovernanceDomainId(L.into())
    }

    fn sled() -> SledStore {
        let s = SledStore::temporary().unwrap();
        let domain = icn_governance::GovernanceDomain::with_id(
            l(),
            "Legacy".into(),
            icn_governance::GovernanceConfig::cooperative_default(),
        );
        s.put(&domain_key(&l()), &serde_json::to_vec(&domain).unwrap())
            .unwrap();
        s
    }

    fn started(s: &SledStore) -> (LifecycleOwner, Arc<dyn GovernanceStateStore>) {
        let owner =
            LifecycleOwner::start(s, BoundSealEvidence::receipts(Arc::new(Backend::default())))
                .unwrap();
        let store: Arc<dyn GovernanceStateStore> =
            Arc::new(FencedGovernanceStateStore::new(s, owner.clone()));
        (owner, store)
    }

    fn prop(id: &str) -> Proposal {
        let mut p = Proposal::new(
            l(),
            icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone(),
            "t".into(),
            "d".into(),
            icn_governance::ProposalPayload::Text { body: "b".into() },
        );
        p.id = ProposalId(id.into());
        p
    }

    /// The stored row's exact bytes (Proposal has no PartialEq).
    fn bytes(store: &Arc<dyn GovernanceStateStore>, id: &ProposalId) -> Option<Vec<u8>> {
        store
            .get_proposal(id)
            .unwrap()
            .map(|p| serde_json::to_vec(&p).unwrap())
    }

    fn js(p: &Proposal) -> Option<Vec<u8>> {
        Some(serde_json::to_vec(p).unwrap())
    }

    fn fence(e: anyhow::Error) -> FenceRefusal {
        match e.downcast_ref::<FenceRefusal>() {
            Some(f) => *f,
            None => panic!("expected a FenceRefusal, got {e:#}"),
        }
    }

    #[test]
    fn legacy_governance_works_through_the_trait_and_create_stays_create_if_absent() {
        let s = sled();
        let (_owner, store) = started(&s);
        let mut p = prop("p");
        store.create_proposal(&p).unwrap();
        let e = store.create_proposal(&p).unwrap_err();
        assert!(
            matches!(
                e.downcast_ref::<ProposalWriteRefused>(),
                Some(ProposalWriteRefused::AlreadyExists { .. })
            ),
            "{e:#}"
        );
        p.open(60).unwrap();
        store.save_proposal(&p).unwrap();
        p.close(ProposalState::Rejected { closed_at: 3 }).unwrap();
        store.save_proposal(&p).unwrap();
        assert_eq!(bytes(&store, &p.id), js(&p));
        // The state store's own rules still apply through the owner: a closed row is final.
        let mut reopened = p.clone();
        reopened.state = ProposalState::Draft;
        assert!(store.save_proposal(&reopened).is_err());
        assert_eq!(bytes(&store, &p.id), js(&p));
    }

    #[test]
    fn pending_refuses_new_commitments_but_drains_existing_ones_through_the_trait() {
        let s = sled();
        let (owner, store) = started(&s);
        let mut open = prop("open");
        store.create_proposal(&open).unwrap();
        open.open(60).unwrap();
        store.save_proposal(&open).unwrap();
        let draft = prop("draft");
        store.create_proposal(&draft).unwrap();
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        // New proposal: refused, nothing written.
        let e = store.create_proposal(&prop("new")).unwrap_err();
        assert_eq!(fence(e), FenceRefusal::CreateWhilePending);
        assert!(store
            .get_proposal(&ProposalId("new".into()))
            .unwrap()
            .is_none());
        // Activation of a Draft: refused.
        let mut activated = draft.clone();
        activated.open(60).unwrap();
        assert!(store.save_proposal(&activated).is_err());
        assert_eq!(bytes(&store, &draft.id), js(&draft));
        // An existing obligation drains: the open proposal closes.
        open.close(ProposalState::Rejected { closed_at: 4 })
            .unwrap();
        store.save_proposal(&open).unwrap();
        // A domain change that would break the pinned basis: refused.
        let mut d = store.get_domain(&l()).unwrap().expect("L's domain record");
        let pinned = d.clone();
        d.name = format!("{}-renamed", d.name);
        assert_eq!(
            fence(store.save_domain(&d).unwrap_err()),
            FenceRefusal::DomainChangeWhilePending
        );
        assert_eq!(
            serde_json::to_vec(&store.get_domain(&l()).unwrap().unwrap()).unwrap(),
            serde_json::to_vec(&pinned).unwrap()
        );
    }

    #[test]
    fn sealed_refuses_every_routed_predecessor_write() {
        let s = sled();
        let (owner, store) = started(&s);
        let mut c = prop("c");
        store.create_proposal(&c).unwrap();
        c.cancel().unwrap();
        store.save_proposal(&c).unwrap();
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        // The matching Seal replays.
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        assert_eq!(
            fence(store.create_proposal(&prop("late")).unwrap_err()),
            FenceRefusal::Sealed
        );
        let mut rewrite = c.clone();
        rewrite.title = "changed".into();
        assert!(store.save_proposal(&rewrite).is_err());
        assert_eq!(bytes(&store, &c.id), js(&c), "history kept");
    }

    #[test]
    fn the_owner_is_exclusive_and_releases_its_claim_when_dropped() {
        let s = sled();
        let (owner, store) = started(&s);
        let second = LifecycleOwner::start(
            &s,
            BoundSealEvidence::receipts(Arc::new(Backend::default())),
        );
        assert!(matches!(second, Err(OwnerStartError::Claim(_))));
        drop(store);
        drop(owner);
        // The owner thread ends once every handle is gone; the claim goes with it.
        let mut restarted = None;
        for _ in 0..200 {
            if let Ok(o) = LifecycleOwner::start(
                &s,
                BoundSealEvidence::receipts(Arc::new(Backend::default())),
            ) {
                restarted = Some(o);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(restarted.is_some(), "the claim was released");
    }

    // ---- Routed through the actor's typed command path ----

    use crate::actor::{GovernanceActor, GovernanceCommand, GovernanceHandle};
    use icn_gossip::{AccessControl, GossipActor, Topic};
    use icn_governance::{
        GovernanceConfig, MembershipConfig, ProposalPayload, ProposalScope,
        StaticMembershipResolver,
    };

    fn member_domain(id: &str, member: &Did) -> GovernanceDomain {
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = MembershipConfig::static_list(vec![member.clone()]);
        GovernanceDomain::with_id(GovernanceDomainId::new(id), id.to_string(), config)
    }

    async fn actor_on(s: &SledStore, owner: LifecycleOwner, node: &Did) -> GovernanceHandle {
        let gossip = GossipActor::spawn(node.clone(), None);
        gossip.write().await.create_topic(Topic::new(
            "governance:proposal".into(),
            AccessControl::Public,
        ));
        GovernanceActor::spawn_with_owner(
            node.clone(),
            s,
            owner,
            Arc::new(Backend::default()),
            gossip,
            Arc::new(StaticMembershipResolver::new()),
            None,
            None,
        )
        .await
        .unwrap()
    }

    fn create(id: &str, dom: &str) -> GovernanceCommand {
        GovernanceCommand::CreateProposal {
            proposal_id: ProposalId(id.to_string()),
            domain_id: GovernanceDomainId::new(dom),
            title: "t".to_string(),
            description: "d".to_string(),
            payload: ProposalPayload::Text { body: "b".into() },
            scope: ProposalScope::Local,
            action_items_on_accept: vec![],
        }
    }

    #[tokio::test]
    async fn the_actor_command_path_is_fenced_per_domain_through_the_owner() {
        let s = SledStore::temporary().unwrap();
        let node = icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone();
        for id in [L, "other.coop.example"] {
            s.put(
                &domain_key(&GovernanceDomainId::new(id)),
                &serde_json::to_vec(&member_domain(id, &node)).unwrap(),
            )
            .unwrap();
        }
        let owner = LifecycleOwner::start(
            &s,
            BoundSealEvidence::receipts(Arc::new(Backend::default())),
        )
        .unwrap();
        let handle = actor_on(&s, owner.clone(), &node).await;
        let state = SledGovernanceStateStore::new(Arc::new(s.share()));
        let id = |x: &str| ProposalId(x.to_string());

        // Legacy: the ordinary command path works through the owner.
        handle.submit(create("p1", L)).await.unwrap();
        handle.submit(create("d1", L)).await.unwrap();
        assert!(state.get_proposal(&id("p1")).unwrap().is_some());
        handle
            .submit(GovernanceCommand::OpenProposal {
                proposal_id: id("p1"),
                voting_period_seconds: 600,
            })
            .await
            .unwrap();

        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        let d1_before = serde_json::to_vec(&state.get_proposal(&id("d1")).unwrap()).unwrap();

        // Pending: a new proposal in L is refused by the fence, through the actor, and nothing
        // is stored.
        let e = handle.submit(create("p2", L)).await.unwrap_err();
        assert_eq!(fence(e), FenceRefusal::CreateWhilePending);
        assert!(state.get_proposal(&id("p2")).unwrap().is_none());
        // Activating a Draft of L is refused too; the row is unchanged.
        assert!(handle
            .submit(GovernanceCommand::OpenProposal {
                proposal_id: id("d1"),
                voting_period_seconds: 600,
            })
            .await
            .is_err());
        assert_eq!(
            serde_json::to_vec(&state.get_proposal(&id("d1")).unwrap()).unwrap(),
            d1_before
        );
        // Another domain, never begun, is unaffected.
        handle
            .submit(create("q1", "other.coop.example"))
            .await
            .unwrap();
        assert!(state.get_proposal(&id("q1")).unwrap().is_some());
    }

    #[tokio::test]
    async fn a_second_lifecycle_aware_actor_on_the_same_database_is_refused() {
        let s = sled();
        let spawn = |did: Did| {
            let s = &s;
            async move {
                let gossip = GossipActor::spawn(did.clone(), None);
                gossip.write().await.create_topic(Topic::new(
                    "governance:proposal".into(),
                    AccessControl::Public,
                ));
                GovernanceActor::spawn_lifecycle_aware(
                    did,
                    s,
                    Arc::new(Backend::default()),
                    gossip,
                    Arc::new(StaticMembershipResolver::new()),
                    None,
                    None,
                )
                .await
            }
        };
        let node = || {
            icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone()
        };
        let first = spawn(node()).await.unwrap();
        let e = match spawn(node()).await {
            Ok(_) => panic!("a second lifecycle-aware actor on the same database started"),
            Err(e) => format!("{e:#}"),
        };
        assert!(
            e.contains("lifecycle-aware governance refused to start")
                && e.contains("claim refused"),
            "refused for the claim, not something else: {e}"
        );
        drop(first);
    }

    #[test]
    fn a_partial_journal_record_keeps_the_owner_from_starting_and_is_preserved() {
        use crate::predecessor_lifecycle_store::LIFECYCLE_JOURNAL_PREFIX;
        use icn_store::recovery::{RecoveryIntent, RecoveryJournal};
        let s = sled();
        let (a, b) = (b"gov:test:a".to_vec(), b"gov:test:b".to_vec());
        {
            // An interrupted two-key write: durable intent, then only the first key landed.
            let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
            journal
                .begin(&RecoveryIntent {
                    txid: [0xAB; 16],
                    expected: vec![(a.clone(), None), (b.clone(), None)],
                    intended: vec![
                        (a.clone(), Some(b"1".to_vec())),
                        (b.clone(), Some(b"2".to_vec())),
                    ],
                })
                .unwrap();
            s.put(&a, b"1").unwrap();
        }
        match LifecycleOwner::start(
            &s,
            BoundSealEvidence::receipts(Arc::new(Backend::default())),
        ) {
            Err(OwnerStartError::Quarantined(report)) => {
                assert_eq!(report.quarantined.len(), 1);
                assert_eq!((report.completed, report.abandoned), (0, 0));
            }
            Err(e) => panic!("expected quarantine, got {e}"),
            Ok(_) => panic!("the owner started over an unresolved partial write"),
        }
        // Evidence preserved: nothing was rolled back or completed, and the record remains.
        assert_eq!(s.get(&a).unwrap(), Some(b"1".to_vec()));
        assert_eq!(s.get(&b).unwrap(), None);
        let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
        assert_eq!(journal.classify().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_construction_bound_receipt_store_is_not_replaced_later() {
        let s = sled();
        let node = icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone();
        let gossip = GossipActor::spawn(node.clone(), None);
        gossip.write().await.create_topic(Topic::new(
            "governance:proposal".into(),
            AccessControl::Public,
        ));
        let bound: Arc<dyn crate::receipt_backend::GovernanceReceiptBackend> =
            Arc::new(Backend::default());
        let handle = GovernanceActor::spawn_lifecycle_aware(
            node,
            &s,
            bound.clone(),
            gossip,
            Arc::new(StaticMembershipResolver::new()),
            None,
            None,
        )
        .await
        .unwrap();
        handle
            .install_receipt_store(Arc::new(Backend::default()))
            .await;
        let held = handle.receipt_store_for_tests().await.unwrap();
        assert!(Arc::ptr_eq(&held, &bound), "the bound backend is kept");
    }
}
