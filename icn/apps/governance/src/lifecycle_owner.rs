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
//!   trait. Reads pass through. The authoritative writes the lifecycle governs
//!   (`create_proposal`, `save_proposal`, `save_domain`, `save_close_intent`,
//!   `delete_close_intent`; since S29 also `save_vote`, `save_delegation` and
//!   `save_revoked_delegation`) become requests to the owner thread, which loads the domain's
//!   lifecycle and applies the fenced, journaled write. Every actor path that writes through
//!   its state store (direct commands, scheduler callbacks, startup close recovery) is therefore
//!   routed by construction, with one funnel and no lifecycle rule restated outside the store.
//! * S29: votes (`save_vote`), delegation creation (`save_delegation`) and revocation
//!   (`save_revoked_delegation`) are routed too: a vote is fenced by its stored proposal's
//!   domain; a delegation by the domain its scope resolves into (Blanket resolves into none and
//!   is never refused by a lifecycle); revocation is allowed in every state and write-once.
//! * Writes the lifecycle does not govern (proof bytes, institutional domains, policy bodies)
//!   pass through unchanged (writer inventory W7-proof, W16, W22).
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
use icn_governance::predecessor_lifecycle::lifecycle_key;
use icn_governance::{
    Delegation, DelegationId, DelegationScope, DomainPolicyId, GovernanceDomain,
    GovernanceDomainId, InstitutionalDomain, Proposal, ProposalId, Timestamp, Vote,
};
use icn_identity::Did;
use icn_store::SledStore;

use crate::close_journal::CloseJournalEntry;
use crate::predecessor_lifecycle_store::{
    FenceRefusal, LifecycleRecovery, LifecycleStore, LifecycleStoreError, LoadedLifecycle,
    RecoveryReport,
};
use crate::seal_evidence::BoundSealEvidence;
use crate::state_store::{GovernanceStateStore, SledGovernanceStateStore};

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
    SaveVote {
        proposal_id: ProposalId,
        vote: Box<Vote>,
    },
    CreateDelegation(Box<Delegation>),
    RevokeDelegation {
        delegation: Box<Delegation>,
        revoked_at: Timestamp,
    },
    /// Test-only lifecycle transitions (the operational Begin/Abort/Seal are institutional
    /// actions, W18, and are not exposed by the owner).
    #[cfg(test)]
    TestBegin {
        domain: GovernanceDomainId,
        attempt: u64,
        commitment: Box<icn_governance::cutover::CutoverCommitmentV1>,
    },
    #[cfg(test)]
    TestArmHook(crate::predecessor_lifecycle_store::BeforeApplyHook<'static>),
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

    /// Test-only: run `hook` inside the owner thread between journaling its next write's
    /// intent and applying it (a panic there models a crash of the owner mid-commit).
    #[cfg(test)]
    pub(crate) fn arm_before_apply_for_tests(
        &self,
        hook: crate::predecessor_lifecycle_store::BeforeApplyHook<'static>,
    ) -> Result<()> {
        self.call(Write::TestArmHook(hook))
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
        #[cfg(test)]
        if let Write::TestArmHook(hook) = req.write {
            live.set_before_apply(hook);
            let _ = req.reply.send(Ok(()));
            continue;
        }
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
            // Create-if-absent (#2953) under the fence: the commit expects the id's absence, so a
            // row racing the check conflicts and the retry refuses it as AlreadyExists.
            live.create_proposal(&live.load(&p.domain_id)?, p, txid())?;
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
        Write::SaveVote { proposal_id, vote } => {
            // The vote's institutional context is its stored proposal's domain. No stored
            // proposal, no context: refused (the store re-checks under the commit guard).
            let Some(proposal) = reader(store).get_proposal(proposal_id)? else {
                return Err(FenceRefusal::VoteWithoutProposal.into());
            };
            live.write_vote(&live.load(&proposal.domain_id)?, proposal_id, vote, txid())?;
        }
        Write::CreateDelegation(d) => {
            let loaded = scope_lifecycle(live, store, &d.scope, ScopeUse::Create)?;
            live.write_delegation(loaded.as_ref(), d, txid())?;
        }
        Write::RevokeDelegation {
            delegation,
            revoked_at,
        } => {
            let loaded = scope_lifecycle(live, store, &delegation.scope, ScopeUse::Revoke)?;
            live.write_revocation(loaded.as_ref(), delegation, *revoked_at, txid())?;
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
        Write::TestArmHook(_) => {
            unreachable!("armed in the owner loop, never applied")
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

fn reader(store: &SledStore) -> SledGovernanceStateStore {
    SledGovernanceStateStore::new(Arc::new(store.share()))
}

/// Why a delegation scope is being resolved: creation must resolve to a context, and
/// revocation must never be trapped by one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeUse {
    Create,
    Revoke,
}

/// The lifecycle of the domain a delegation scope resolves into, or `None` for a scope that
/// resolves into no fenced domain.
///
/// * Blanket names no domain.
/// * `Domain(D)` resolves to D. A domain id that cannot key a lifecycle record can never have
///   been begun, so revoking such a delegation is unfenced rather than trapped; creating one
///   fails like every other routed write for that id (writer inventory, DP-7).
/// * `Proposal(P)` resolves to P's stored domain. Creating one requires a stored P. Revoking
///   one whose P is not stored or not readable, or whose domain id can never have a lifecycle
///   (rows the pre-fence path accepted), is unfenced, never trapped.
fn scope_lifecycle(
    live: &LifecycleStore<'_>,
    store: &SledStore,
    scope: &DelegationScope,
    usage: ScopeUse,
) -> Result<Option<LoadedLifecycle>> {
    match scope {
        DelegationScope::Blanket => Ok(None),
        DelegationScope::Domain(d) => {
            if usage == ScopeUse::Revoke && lifecycle_key(d).is_err() {
                return Ok(None);
            }
            Ok(Some(live.load(d)?))
        }
        DelegationScope::Proposal(p) => {
            // Revocation is never trapped: a proposal row that cannot be read, or whose domain
            // can never have a lifecycle, revokes unfenced (as for `Domain(D)` above).
            let found = match (usage, reader(store).get_proposal(p)) {
                (ScopeUse::Revoke, Err(_)) => return Ok(None),
                (_, found) => found?,
            };
            match found {
                Some(proposal)
                    if usage == ScopeUse::Revoke && lifecycle_key(&proposal.domain_id).is_err() =>
                {
                    Ok(None)
                }
                Some(proposal) => Ok(Some(live.load(&proposal.domain_id)?)),
                None if usage == ScopeUse::Revoke => Ok(None),
                None => Err(FenceRefusal::DelegationWithoutProposal.into()),
            }
        }
    }
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
        self.owner.call(Write::SaveVote {
            proposal_id: proposal_id.clone(),
            vote: Box::new(vote.clone()),
        })
    }
    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>> {
        self.inner.list_votes(proposal_id)
    }
    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        self.inner.get_delegation(id)
    }
    fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        self.owner
            .call(Write::CreateDelegation(Box::new(delegation.clone())))
    }
    fn list_all_delegations(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.list_all_delegations()
    }
    fn save_revoked_delegation(
        &self,
        delegation: &Delegation,
        revoked_at: Timestamp,
    ) -> Result<()> {
        self.owner.call(Write::RevokeDelegation {
            delegation: Box::new(delegation.clone()),
            revoked_at,
        })
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
    use crate::state_store::{domain_key, ProposalWriteRefused};
    use icn_governance::ProposalState;
    use icn_store::Store;

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
        assert_eq!(
            fence(store.save_proposal(&activated).unwrap_err()),
            FenceRefusal::ActivationWhilePending
        );
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
        // A never-activated Draft does not block the Seal and is preserved by it.
        let draft = prop("draft");
        store.create_proposal(&draft).unwrap();
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        // The matching Seal replays.
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        assert_eq!(
            fence(store.create_proposal(&prop("late")).unwrap_err()),
            FenceRefusal::Sealed
        );
        // Activating the Draft: only the Sealed fence can refuse this (it is not closed).
        let mut activated = draft.clone();
        activated.open(60).unwrap();
        assert_eq!(
            fence(store.save_proposal(&activated).unwrap_err()),
            FenceRefusal::Sealed
        );
        // The domain record is final once sealed.
        let mut d = store.get_domain(&l()).unwrap().expect("L's domain record");
        d.name = format!("{}-renamed", d.name);
        assert_eq!(
            fence(store.save_domain(&d).unwrap_err()),
            FenceRefusal::Sealed
        );
        assert_eq!(bytes(&store, &c.id), js(&c), "history kept");
        assert_eq!(bytes(&store, &draft.id), js(&draft), "history kept");
    }

    // ---- S29: votes and delegations through the owner ----

    const M: &str = "other.coop.example";

    fn did() -> Did {
        icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone()
    }

    fn prop_in(id: &str, dom: &str) -> Proposal {
        let mut p = prop(id);
        p.domain_id = GovernanceDomainId::new(dom);
        p
    }

    fn opened(store: &Arc<dyn GovernanceStateStore>, id: &str, dom: &str) -> Proposal {
        let mut p = prop_in(id, dom);
        store.create_proposal(&p).unwrap();
        p.open(600).unwrap();
        store.save_proposal(&p).unwrap();
        p
    }

    fn ballot(p: &str, voter: &Did) -> Vote {
        Vote::new(
            ProposalId(p.into()),
            voter.clone(),
            icn_governance::VoteChoice::For,
        )
    }

    fn vote_row(store: &Arc<dyn GovernanceStateStore>, p: &str, voter: &Did) -> Option<Vec<u8>> {
        store
            .get_vote(&ProposalId(p.into()), voter)
            .unwrap()
            .map(|v| serde_json::to_vec(&v).unwrap())
    }

    fn deleg(scope: DelegationScope) -> Delegation {
        Delegation::new(did(), did(), scope)
    }

    fn deleg_row(store: &Arc<dyn GovernanceStateStore>, d: &Delegation) -> Option<Vec<u8>> {
        store
            .get_delegation(&d.id)
            .unwrap()
            .map(|d| serde_json::to_vec(&d).unwrap())
    }

    /// Close every open proposal of L and seal it (attempt 1).
    fn drain_and_seal(
        owner: &LifecycleOwner,
        store: &Arc<dyn GovernanceStateStore>,
        open: &[Proposal],
    ) {
        for p in open {
            let mut p = p.clone();
            p.close(ProposalState::Rejected { closed_at: 9 }).unwrap();
            store.save_proposal(&p).unwrap();
        }
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
    }

    #[test]
    fn votes_follow_the_lifecycle_of_their_proposals_domain() {
        let s = sled();
        let (owner, store) = started(&s);
        let lp = opened(&store, "lp", L);
        let draft = prop("draft");
        store.create_proposal(&draft).unwrap();
        let mp = opened(&store, "mp", M);
        let (a, b, c) = (did(), did(), did());

        // Legacy: an ordinary vote is written; context and identity are checked.
        let va = ballot("lp", &a);
        store.save_vote(&lp.id, &va).unwrap();
        assert_eq!(
            vote_row(&store, "lp", &a),
            Some(serde_json::to_vec(&va).unwrap())
        );
        assert_eq!(
            fence(
                store
                    .save_vote(&ProposalId("nope".into()), &ballot("nope", &a))
                    .unwrap_err()
            ),
            FenceRefusal::VoteWithoutProposal
        );
        assert_eq!(
            fence(store.save_vote(&lp.id, &ballot("mp", &a)).unwrap_err()),
            FenceRefusal::VoteProposalMismatch
        );
        assert_eq!(vote_row(&store, "mp", &a), None);

        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        // Pending: an open proposal of L drains (votes accepted); a draft of L does not.
        let vb = ballot("lp", &b);
        store.save_vote(&lp.id, &vb).unwrap();
        assert_eq!(
            vote_row(&store, "lp", &b),
            Some(serde_json::to_vec(&vb).unwrap())
        );
        assert_eq!(
            fence(
                store
                    .save_vote(&draft.id, &ballot("draft", &b))
                    .unwrap_err()
            ),
            FenceRefusal::VoteNotDraining
        );
        assert_eq!(vote_row(&store, "draft", &b), None);
        // An unrelated domain is unaffected.
        store.save_vote(&mp.id, &ballot("mp", &b)).unwrap();

        drain_and_seal(&owner, &store, std::slice::from_ref(&lp));
        // Sealed: no new vote on L; the stored votes are kept; a byte-identical replay of an
        // already stored vote writes nothing and is accepted.
        assert_eq!(
            fence(store.save_vote(&lp.id, &ballot("lp", &c)).unwrap_err()),
            FenceRefusal::Sealed
        );
        assert_eq!(vote_row(&store, "lp", &c), None);
        store.save_vote(&lp.id, &va).unwrap();
        assert_eq!(
            vote_row(&store, "lp", &a),
            Some(serde_json::to_vec(&va).unwrap())
        );
        assert_eq!(
            vote_row(&store, "lp", &b),
            Some(serde_json::to_vec(&vb).unwrap())
        );
        // A changed vote by an existing voter is a new act, refused like any other.
        let mut changed = va.clone();
        changed.choice = icn_governance::VoteChoice::Against;
        assert_eq!(
            fence(store.save_vote(&lp.id, &changed).unwrap_err()),
            FenceRefusal::Sealed
        );
        // M still votes after L is sealed.
        store.save_vote(&mp.id, &ballot("mp", &c)).unwrap();
        assert!(vote_row(&store, "mp", &c).is_some());
    }

    #[test]
    fn delegations_are_contained_to_the_domain_their_scope_resolves_into() {
        let s = sled();
        let (owner, store) = started(&s);
        let lp = opened(&store, "lp", L);
        opened(&store, "mp", M);

        // Legacy: scoped and blanket delegations are created; a proposal scope must name a
        // stored proposal; an id is never re-pointed; an identical replay is accepted.
        let dl = deleg(DelegationScope::Domain(l()));
        store.save_delegation(&dl).unwrap();
        store.save_delegation(&dl).unwrap();
        let mut repointed = dl.clone();
        repointed.delegate = did();
        assert_eq!(
            fence(store.save_delegation(&repointed).unwrap_err()),
            FenceRefusal::DelegationExists
        );
        assert_eq!(
            deleg_row(&store, &dl),
            Some(serde_json::to_vec(&dl).unwrap())
        );
        let ghost = deleg(DelegationScope::Proposal(ProposalId("nope".into())));
        assert_eq!(
            fence(store.save_delegation(&ghost).unwrap_err()),
            FenceRefusal::DelegationWithoutProposal
        );
        assert_eq!(deleg_row(&store, &ghost), None);
        let mut blanket_repointed = deleg(DelegationScope::Blanket);
        store.save_delegation(&blanket_repointed).unwrap();
        blanket_repointed.delegate = did();
        assert_eq!(
            fence(store.save_delegation(&blanket_repointed).unwrap_err()),
            FenceRefusal::DelegationExists
        );

        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        // Pending: nothing new resolves into L (scoped interim) ...
        for scope in [
            DelegationScope::Domain(l()),
            DelegationScope::Proposal(lp.id.clone()),
        ] {
            let d = deleg(scope);
            assert_eq!(
                fence(store.save_delegation(&d).unwrap_err()),
                FenceRefusal::DelegationWhilePending
            );
            assert_eq!(deleg_row(&store, &d), None);
        }
        // ... but L's transition disables nothing elsewhere: M and Blanket still work.
        for scope in [
            DelegationScope::Domain(GovernanceDomainId::new(M)),
            DelegationScope::Proposal(ProposalId("mp".into())),
            DelegationScope::Blanket,
        ] {
            let d = deleg(scope);
            store.save_delegation(&d).unwrap();
            assert_eq!(deleg_row(&store, &d), Some(serde_json::to_vec(&d).unwrap()));
        }

        drain_and_seal(&owner, &store, std::slice::from_ref(&lp));
        // Sealed: no new authority enters L; others are unaffected.
        for scope in [
            DelegationScope::Domain(l()),
            DelegationScope::Proposal(lp.id.clone()),
        ] {
            let d = deleg(scope);
            assert_eq!(
                fence(store.save_delegation(&d).unwrap_err()),
                FenceRefusal::Sealed
            );
            assert_eq!(deleg_row(&store, &d), None);
        }
        let d = deleg(DelegationScope::Blanket);
        store.save_delegation(&d).unwrap();
        let d = deleg(DelegationScope::Domain(GovernanceDomainId::new(M)));
        store.save_delegation(&d).unwrap();
    }

    #[test]
    fn revocation_is_never_trapped_prospective_and_final() {
        let s = sled();
        let (owner, store) = started(&s);
        let lp = opened(&store, "lp", L);
        let in_l = deleg(DelegationScope::Domain(l()));
        let on_lp = deleg(DelegationScope::Proposal(lp.id.clone()));
        let blanket = deleg(DelegationScope::Blanket);
        for d in [&in_l, &on_lp, &blanket] {
            store.save_delegation(d).unwrap();
        }
        // A proposal-scoped row the pre-fence path accepted for a proposal that was never
        // stored: written raw, as legacy data.
        let stray = deleg(DelegationScope::Proposal(ProposalId("never".into())));
        SledGovernanceStateStore::new(Arc::new(s.share()))
            .save_delegation(&stray)
            .unwrap();
        let revoked = |d: &Delegation, t| {
            let mut d = d.clone();
            d.revoked_at = Some(t);
            Some(serde_json::to_vec(&d).unwrap())
        };

        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        // Pending: the delegator may withdraw a delegation into L.
        let t1 = on_lp.created_at + 1;
        store.save_revoked_delegation(&on_lp, t1).unwrap();
        assert_eq!(deleg_row(&store, &on_lp), revoked(&on_lp, t1));

        drain_and_seal(&owner, &store, std::slice::from_ref(&lp));
        // Sealed: still revocable (no one is trapped), and for every scope.
        let t2 = in_l.created_at + 2;
        store.save_revoked_delegation(&in_l, t2).unwrap();
        assert_eq!(deleg_row(&store, &in_l), revoked(&in_l, t2));
        store
            .save_revoked_delegation(&blanket, blanket.created_at)
            .unwrap();
        assert_eq!(
            deleg_row(&store, &blanket),
            revoked(&blanket, blanket.created_at)
        );
        store
            .save_revoked_delegation(&stray, stray.created_at)
            .unwrap();
        assert_eq!(deleg_row(&store, &stray), revoked(&stray, stray.created_at));

        // Final: an identical revocation replays; a different time is refused, not moved.
        store.save_revoked_delegation(&in_l, t2).unwrap();
        assert_eq!(
            fence(store.save_revoked_delegation(&in_l, t2 + 5).unwrap_err()),
            FenceRefusal::RevocationFinal
        );
        assert_eq!(deleg_row(&store, &in_l), revoked(&in_l, t2));
        // Not backdated before the delegation existed; names the stored delegation exactly;
        // only stored delegations are revoked.
        let fresh = deleg(DelegationScope::Blanket);
        store.save_delegation(&fresh).unwrap();
        assert_eq!(
            fence(
                store
                    .save_revoked_delegation(&fresh, fresh.created_at - 1)
                    .unwrap_err()
            ),
            FenceRefusal::RevocationBeforeCreation
        );
        let mut other = fresh.clone();
        other.delegate = did();
        assert_eq!(
            fence(
                store
                    .save_revoked_delegation(&other, fresh.created_at)
                    .unwrap_err()
            ),
            FenceRefusal::RevocationMismatch
        );
        assert_eq!(
            deleg_row(&store, &fresh),
            Some(serde_json::to_vec(&fresh).unwrap())
        );
        let unknown = deleg(DelegationScope::Blanket);
        assert_eq!(
            fence(
                store
                    .save_revoked_delegation(&unknown, unknown.created_at)
                    .unwrap_err()
            ),
            FenceRefusal::RevocationWithoutDelegation
        );
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
        let e = handle
            .submit(GovernanceCommand::OpenProposal {
                proposal_id: id("d1"),
                voting_period_seconds: 600,
            })
            .await
            .unwrap_err();
        assert_eq!(fence(e), FenceRefusal::ActivationWhilePending);
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
    async fn the_actor_vote_and_delegation_commands_are_fenced_through_the_owner() {
        let s = SledStore::temporary().unwrap();
        let node = icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone();
        let other = "other.coop.example";
        for id in [L, other] {
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
        let gen = || {
            icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone()
        };
        // A fresh delegator each time, so the actor's one-active-delegation-per-scope rule never
        // answers before the fence does.
        let delegate = |scope| GovernanceCommand::CreateDelegation {
            delegation: Delegation::new(gen(), gen(), scope),
        };

        for (p, dom) in [("p1", L), ("q1", other)] {
            handle.submit(create(p, dom)).await.unwrap();
            handle.submit(open(p)).await.unwrap();
        }
        // Legacy: a delegation into L, made by the node, through the command path.
        let in_l = Delegation::new(node.clone(), gen(), DelegationScope::Domain(l()));
        handle
            .submit(GovernanceCommand::CreateDelegation {
                delegation: in_l.clone(),
            })
            .await
            .unwrap();

        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        // Pending: L's open proposal drains through a real CastVote; no new delegation enters L.
        handle.submit(vote("p1", &node)).await.unwrap();
        assert!(state.get_vote(&id("p1"), &node).unwrap().is_some());
        let e = handle
            .submit(delegate(DelegationScope::Domain(l())))
            .await
            .unwrap_err();
        assert_eq!(fence(e), FenceRefusal::DelegationWhilePending);
        handle
            .submit(delegate(DelegationScope::Blanket))
            .await
            .unwrap();

        handle.submit(close("p1")).await.unwrap();
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        // Sealed: a new vote and a new delegation into L are refused through the actor ...
        let voter_rows = state.list_votes(&id("p1")).unwrap().len();
        let e = handle
            .submit(GovernanceCommand::CastVote {
                proposal_id: id("p1"),
                voter: gen(),
                choice: icn_governance::VoteChoice::Against,
                comment: None,
            })
            .await
            .unwrap_err();
        assert_eq!(fence(e), FenceRefusal::Sealed);
        assert_eq!(state.list_votes(&id("p1")).unwrap().len(), voter_rows);
        let e = handle
            .submit(delegate(DelegationScope::Domain(l())))
            .await
            .unwrap_err();
        assert_eq!(fence(e), FenceRefusal::Sealed);
        // ... while revocation of the existing delegation into L stays possible ...
        handle
            .submit(GovernanceCommand::RevokeDelegation {
                id: in_l.id.clone(),
                revoked_at: in_l.created_at + 1,
            })
            .await
            .unwrap();
        assert_eq!(
            state.get_delegation(&in_l.id).unwrap().unwrap().revoked_at,
            Some(in_l.created_at + 1)
        );
        // ... and the unrelated domain keeps voting.
        handle.submit(vote("q1", &node)).await.unwrap();
        assert!(state.get_vote(&id("q1"), &node).unwrap().is_some());
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

    fn intent_for(p: &Proposal, closed: ProposalState) -> CloseJournalEntry {
        let mut proposal = p.clone();
        proposal.state = closed;
        CloseJournalEntry {
            proposal,
            proof_bytes: None,
            receipts: Default::default(),
            decided_at: 7,
            governance_decision_hash: None,
        }
    }

    #[test]
    fn pending_drains_a_close_through_the_routed_intent_writes_and_then_seals() {
        let s = sled();
        let (owner, store) = started(&s);
        let mut p = prop("p");
        store.create_proposal(&p).unwrap();
        p.open(60).unwrap();
        store.save_proposal(&p).unwrap();
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        // The journaled close of an existing obligation: intent, terminal row, retirement.
        let entry = intent_for(&p, ProposalState::Rejected { closed_at: 7 });
        store.save_close_intent(&entry).unwrap();
        assert!(store.get_close_intent(&p.id).unwrap().is_some());
        // While the intent is outstanding the Seal refuses (a decided close must complete).
        assert!(owner.seal_for_tests(&l(), 1, k(0x4D)).is_err());
        store.save_proposal(&entry.proposal).unwrap();
        store.delete_close_intent(&p.id).unwrap();
        assert!(store.get_close_intent(&p.id).unwrap().is_none());
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
    }

    #[test]
    fn sealed_refuses_routed_intent_writes_and_keeps_a_stray_intent_as_evidence() {
        use crate::state_store::close_intent_key;
        let s = sled();
        let (owner, store) = started(&s);
        let mut c = prop("c");
        store.create_proposal(&c).unwrap();
        c.cancel().unwrap();
        store.save_proposal(&c).unwrap();
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        // A new intent for a predecessor proposal: refused, nothing written.
        let entry = intent_for(&c, ProposalState::Accepted { closed_at: 7 });
        assert_eq!(
            fence(store.save_close_intent(&entry).unwrap_err()),
            FenceRefusal::Sealed
        );
        assert!(s.get(&close_intent_key(&c.id)).unwrap().is_none());
        // An intent that appears after the seal (raw writer) is evidence: retirement through the
        // owner is refused and the record stays.
        let raw = serde_json::to_vec(&entry).unwrap();
        s.put(&close_intent_key(&c.id), &raw).unwrap();
        assert_eq!(
            fence(store.delete_close_intent(&c.id).unwrap_err()),
            FenceRefusal::Sealed
        );
        assert_eq!(s.get(&close_intent_key(&c.id)).unwrap(), Some(raw));
    }

    #[test]
    fn a_create_racing_a_same_id_row_retries_and_refuses_already_exists() {
        // S28 routing delta-review FOLLOW_UP: the owner's Stale retry surfaces AlreadyExists.
        use crate::state_store::state_store_test_put;
        let s = sled();
        let (owner, store) = started(&s);
        let mut planted = prop("x");
        planted.title = "planted".into();
        let racer = planted.clone();
        owner
            .arm_before_apply_for_tests(Box::new(move |st: &SledStore| {
                state_store_test_put(st, &racer);
            }))
            .unwrap();
        let e = store.create_proposal(&prop("x")).unwrap_err();
        assert!(
            matches!(
                e.downcast_ref::<ProposalWriteRefused>(),
                Some(ProposalWriteRefused::AlreadyExists { .. })
            ),
            "{e:#}"
        );
        assert_eq!(
            bytes(&store, &planted.id),
            js(&planted),
            "never overwritten"
        );
    }

    #[test]
    fn a_close_racing_a_draining_vote_wins_and_the_retry_refuses_the_vote() {
        // S29 mutant V14 (vote commit without its proposal guard) survived the first suite:
        // a close landing between the owner's check and the commit must not be outrun.
        use crate::state_store::state_store_test_put;
        let s = sled();
        let (owner, store) = started(&s);
        let p = opened(&store, "p", L);
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        let mut closed = p.clone();
        closed
            .close(ProposalState::Rejected { closed_at: 7 })
            .unwrap();
        let racer = closed.clone();
        owner
            .arm_before_apply_for_tests(Box::new(move |st: &SledStore| {
                state_store_test_put(st, &racer);
            }))
            .unwrap();
        let voter = did();
        let e = store.save_vote(&p.id, &ballot("p", &voter)).unwrap_err();
        assert_eq!(fence(e), FenceRefusal::VoteNotDraining);
        assert_eq!(
            vote_row(&store, "p", &voter),
            None,
            "no vote on a closed proposal"
        );
        assert_eq!(bytes(&store, &p.id), js(&closed), "the close stands");
    }

    #[test]
    fn a_vote_after_the_close_intent_is_journaled_is_refused() {
        // S29 review FOLLOW_UP 1: once the close intent fixes the decision, no new vote may
        // change the stored rows the decision evidence was computed from.
        let s = sled();
        let (owner, store) = started(&s);
        let p = opened(&store, "p", L);
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        store
            .save_close_intent(&intent_for(&p, ProposalState::Rejected { closed_at: 5 }))
            .unwrap();
        let voter = did();
        assert_eq!(
            fence(store.save_vote(&p.id, &ballot("p", &voter)).unwrap_err()),
            FenceRefusal::VoteWhileClosing
        );
        assert_eq!(vote_row(&store, "p", &voter), None);
    }

    #[test]
    fn revoking_a_proposal_scoped_delegation_in_a_lifecycle_less_domain_is_not_trapped() {
        // S29 review FOLLOW_UP 2: a stored proposal whose domain id can never key a lifecycle
        // (pre-fence data) must not make its delegations unrevocable.
        use crate::state_store::state_store_test_put;
        let s = sled();
        let (_owner, store) = started(&s);
        let mut odd = prop("odd");
        odd.domain_id = GovernanceDomainId::new("");
        assert!(
            lifecycle_key(&odd.domain_id).is_err(),
            "precondition: no lifecycle key"
        );
        state_store_test_put(&s, &odd);
        let d = deleg(DelegationScope::Proposal(odd.id.clone()));
        SledGovernanceStateStore::new(Arc::new(s.share()))
            .save_delegation(&d)
            .unwrap();
        store.save_revoked_delegation(&d, d.created_at).unwrap();
        assert_eq!(
            store.get_delegation(&d.id).unwrap().unwrap().revoked_at,
            Some(d.created_at)
        );
    }

    // ---- S28 §8: isolated institutional lifecycle, end to end ----
    //
    // EXPERIMENTAL lifecycle integration, NOT production succession: Begin and Seal use the
    // owner's cfg(test) routes with an unverified test electorate, and the receipt backend is
    // the crate's in-memory test backend (it records what the actor's close path writes:
    // decision receipts, mandates, effect records, grants). Everything else is real: a sled
    // database on disk, the lifecycle owner, recovery, and the actor's typed commands.

    fn vote(id: &str, voter: &Did) -> GovernanceCommand {
        GovernanceCommand::CastVote {
            proposal_id: ProposalId(id.into()),
            voter: voter.clone(),
            choice: icn_governance::VoteChoice::For,
            comment: None,
        }
    }

    fn close(id: &str) -> GovernanceCommand {
        GovernanceCommand::CloseProposal {
            proposal_id: ProposalId(id.into()),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        }
    }

    fn open(id: &str) -> GovernanceCommand {
        GovernanceCommand::OpenProposal {
            proposal_id: ProposalId(id.into()),
            voting_period_seconds: 600,
        }
    }

    fn create_with(id: &str, dom: &str, payload: ProposalPayload) -> GovernanceCommand {
        GovernanceCommand::CreateProposal {
            proposal_id: ProposalId(id.to_string()),
            domain_id: GovernanceDomainId::new(dom),
            title: "t".to_string(),
            description: "d".to_string(),
            payload,
            scope: ProposalScope::Local,
            action_items_on_accept: vec![],
        }
    }

    /// Open the database at `path`, start the owner (claim + safe recovery) and the actor.
    async fn boot(
        path: &std::path::Path,
        receipts: &Arc<Backend>,
        node: &Did,
    ) -> (SledStore, LifecycleOwner, GovernanceHandle) {
        // A real reopen: the previous process image released every handle (actor shut down,
        // owner thread gone); the owner thread exits asynchronously, so allow it a moment.
        let mut opened = None;
        for _ in 0..400 {
            match SledStore::open(path) {
                Ok(s) => {
                    opened = Some(s);
                    break;
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
        let s = opened.expect("the database reopens once every handle is released");
        let owner =
            LifecycleOwner::start(&s, BoundSealEvidence::receipts(receipts.clone())).unwrap();
        let gossip = GossipActor::spawn(node.clone(), None);
        gossip.write().await.create_topic(Topic::new(
            "governance:proposal".into(),
            AccessControl::Public,
        ));
        let handle = GovernanceActor::spawn_with_owner(
            node.clone(),
            &s,
            owner.clone(),
            receipts.clone(),
            gossip,
            Arc::new(StaticMembershipResolver::new()),
            None,
            None,
        )
        .await
        .unwrap();
        (s, owner, handle)
    }

    #[tokio::test]
    async fn an_interrupted_vote_is_abandoned_and_a_final_revocation_survives_restart() {
        use crate::predecessor_lifecycle_store::LIFECYCLE_JOURNAL_PREFIX;
        use icn_store::recovery::{Outcome, RecoveryJournal};
        let dir = tempfile::tempdir().unwrap();
        let receipts = Arc::new(Backend::default());
        let node = icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone();
        {
            let s = SledStore::open(dir.path()).unwrap();
            s.put(
                &domain_key(&l()),
                &serde_json::to_vec(&member_domain(L, &node)).unwrap(),
            )
            .unwrap();
            s.flush().unwrap();
        }
        let id = |x: &str| ProposalId(x.to_string());
        let (s, owner, h) = boot(dir.path(), &receipts, &node).await;
        h.submit(create("p1", L)).await.unwrap();
        h.submit(open("p1")).await.unwrap();
        let d = Delegation::new(
            node.clone(),
            icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone(),
            DelegationScope::Domain(l()),
        );
        h.submit(GovernanceCommand::CreateDelegation {
            delegation: d.clone(),
        })
        .await
        .unwrap();
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();

        // Interrupted: the owner dies after journaling a draining vote, before applying it.
        owner
            .arm_before_apply_for_tests(Box::new(|_: &SledStore| panic!("crash mid-commit")))
            .unwrap();
        assert!(h.submit(vote("p1", &node)).await.is_err(), "interrupted");
        let state = SledGovernanceStateStore::new(Arc::new(s.share()));
        assert!(
            state.get_vote(&id("p1"), &node).unwrap().is_none(),
            "nothing applied"
        );
        let outcomes = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX)
            .unwrap()
            .classify()
            .unwrap();
        assert_eq!(outcomes.len(), 1);
        assert!(matches!(outcomes[0], Outcome::NotApplied(_)));
        s.flush().unwrap();
        h.shutdown().await;
        drop((h, owner, state));
        drop(s);

        // Restart: recovery abandons the unapplied vote; the voter can still vote (drainage).
        let (s, owner, h) = boot(dir.path(), &receipts, &node).await;
        assert!(RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX)
            .unwrap()
            .classify()
            .unwrap()
            .is_empty());
        let state = SledGovernanceStateStore::new(Arc::new(s.share()));
        assert!(state.get_vote(&id("p1"), &node).unwrap().is_none());
        h.submit(vote("p1", &node)).await.unwrap();
        assert!(state.get_vote(&id("p1"), &node).unwrap().is_some());
        // The delegator withdraws, then L drains and seals.
        let t = d.created_at + 1;
        h.submit(GovernanceCommand::RevokeDelegation {
            id: d.id.clone(),
            revoked_at: t,
        })
        .await
        .unwrap();
        h.submit(close("p1")).await.unwrap();
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        s.flush().unwrap();
        h.shutdown().await;
        drop((h, owner, state));
        drop(s);

        // Restart again: the revocation is durable and final; L stays sealed for votes.
        let (s, _owner, h) = boot(dir.path(), &receipts, &node).await;
        let state = SledGovernanceStateStore::new(Arc::new(s.share()));
        assert_eq!(
            state.get_delegation(&d.id).unwrap().unwrap().revoked_at,
            Some(t)
        );
        let e = h
            .submit(GovernanceCommand::RevokeDelegation {
                id: d.id.clone(),
                revoked_at: t + 100,
            })
            .await
            .unwrap_err();
        assert_eq!(fence(e), FenceRefusal::RevocationFinal);
        assert_eq!(
            state.get_delegation(&d.id).unwrap().unwrap().revoked_at,
            Some(t)
        );
        let late = icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone();
        assert_eq!(
            fence(h.submit(vote("p1", &late)).await.unwrap_err()),
            FenceRefusal::Sealed
        );
        h.shutdown().await;
    }

    #[tokio::test]
    async fn an_institution_drains_survives_a_crash_seals_and_stays_sealed_across_restart() {
        use icn_governance::predecessor_lifecycle::{
            check_witness, domain_record_pin, lifecycle_key, witness_key, LifecyclePresence,
            StoredLifecycle,
        };
        let dir = tempfile::tempdir().unwrap();
        let node = icn_identity::IdentityBundle::generate()
            .unwrap()
            .did()
            .clone();
        let m_dom = "other.coop.example";
        {
            let s = SledStore::open(dir.path()).unwrap();
            for id in [L, m_dom] {
                s.put(
                    &domain_key(&GovernanceDomainId::new(id)),
                    &serde_json::to_vec(&member_domain(id, &node)).unwrap(),
                )
                .unwrap();
            }
            s.flush().unwrap();
        }
        let receipts = Arc::new(Backend::default());
        let lifecycle = |s: &SledStore| -> LifecyclePresence {
            check_witness(
                &l(),
                s.get(&lifecycle_key(&l()).unwrap()).unwrap().as_deref(),
                s.get(&witness_key(&l()).unwrap()).unwrap().as_deref(),
            )
            .unwrap()
        };
        let stored = |s: &SledStore, id: &str| -> Option<Proposal> {
            SledGovernanceStateStore::new(Arc::new(s.share()))
                .get_proposal(&ProposalId(id.into()))
                .unwrap()
        };
        let budget = || ProposalPayload::Budget {
            amount: 5,
            currency: "hours".into(),
            recipient: icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone(),
            purpose: "p".into(),
        };
        let text = || ProposalPayload::Text { body: "b".into() };

        // ---- Legacy: ordinary governance through the typed command path ----
        let (s, owner, h) = boot(dir.path(), &receipts, &node).await;
        for (id, payload) in [("a", text()), ("b", budget())] {
            h.submit(create_with(id, L, payload)).await.unwrap();
            h.submit(open(id)).await.unwrap();
            h.submit(vote(id, &node)).await.unwrap();
            h.submit(close(id)).await.unwrap();
            assert!(stored(&s, id).unwrap().state.is_closed(), "{id} closed");
        }
        // The execution-required acceptance left its durable intent in the receipt backend.
        let b_mandate = receipts
            .mandates
            .lock()
            .unwrap()
            .iter()
            .any(|m| m.decision.proposal_id == "b");
        assert!(b_mandate, "mandate for b");
        assert!(receipts
            .receipts
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.proposal_id == "b"));
        h.submit(create_with("draft", L, text())).await.unwrap();
        h.submit(create_with("o", L, text())).await.unwrap();
        h.submit(open("o")).await.unwrap();
        h.submit(vote("o", &node)).await.unwrap();
        assert!(matches!(lifecycle(&s), LifecyclePresence::NeverBegun));

        // ---- Begin (test-authorized) ----
        owner.begin_for_tests(&l(), 1, commitment(0x4D)).unwrap();
        let pinned_domain = s.get(&domain_key(&l())).unwrap().unwrap();
        let begin_rev = match lifecycle(&s) {
            LifecyclePresence::Established(r) => match r.state {
                StoredLifecycle::Pending {
                    begin_revision,
                    domain_pin,
                    ..
                } => {
                    assert_eq!(domain_pin, domain_record_pin(&pinned_domain));
                    assert_eq!(begin_revision, 0, "fenced on the never-begun state");
                    r.revision
                }
                other => panic!("expected Pending, got {other:?}"),
            },
            other => panic!("expected a record, got {other:?}"),
        };

        // ---- Pending: new commitments refused, the institution elsewhere unaffected ----
        let refused = |e: anyhow::Error| fence(e);
        assert_eq!(
            refused(h.submit(create_with("new", L, text())).await.unwrap_err()),
            FenceRefusal::CreateWhilePending
        );
        assert_eq!(
            refused(h.submit(open("draft")).await.unwrap_err()),
            FenceRefusal::ActivationWhilePending
        );
        let mut cfg = member_domain(L, &node).config;
        cfg.params.voting_period_seconds += 60;
        assert_eq!(
            refused(
                h.submit(GovernanceCommand::UpdateDomainConfig {
                    domain_id: l(),
                    new_config: cfg,
                })
                .await
                .unwrap_err()
            ),
            FenceRefusal::DomainChangeWhilePending
        );
        assert_eq!(s.get(&domain_key(&l())).unwrap().unwrap(), pinned_domain);
        h.submit(create_with("m1", m_dom, text())).await.unwrap();

        // ---- Interrupted: the owner dies after journaling the drain's first write ----
        owner
            .arm_before_apply_for_tests(Box::new(|_: &SledStore| panic!("crash mid-commit")))
            .unwrap();
        assert!(
            h.submit(close("o")).await.is_err(),
            "the close is interrupted"
        );
        assert!(
            !stored(&s, "o").unwrap().state.is_closed(),
            "nothing applied"
        );
        {
            // The crash left exactly one durable intent, classified NotApplied.
            use crate::predecessor_lifecycle_store::LIFECYCLE_JOURNAL_PREFIX;
            use icn_store::recovery::{Outcome, RecoveryJournal};
            let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
            let outcomes = journal.classify().unwrap();
            assert_eq!(outcomes.len(), 1);
            assert!(matches!(outcomes[0], Outcome::NotApplied(_)));
        }
        s.flush().unwrap();
        h.shutdown().await;
        drop((h, owner));
        drop(s);

        // ---- Restart: claim, safe recovery (the unapplied write is abandoned), live ----
        let (s, owner, h) = boot(dir.path(), &receipts, &node).await;
        h.recover_incomplete_closes().await;
        {
            use crate::predecessor_lifecycle_store::LIFECYCLE_JOURNAL_PREFIX;
            use icn_store::recovery::RecoveryJournal;
            let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
            assert!(
                journal.classify().unwrap().is_empty(),
                "recovery abandoned it"
            );
        }
        // The legitimate obligation drains after the restart.
        h.submit(close("o")).await.unwrap();
        assert!(stored(&s, "o").unwrap().state.is_closed());
        let drained_rev = match lifecycle(&s) {
            LifecyclePresence::Established(r) => r.revision,
            other => panic!("{other:?}"),
        };
        assert!(
            drained_rev > begin_rev,
            "fenced writes advanced the revision"
        );

        // ---- Seal from what is stored, with the bound receipt evidence; replay ----
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        owner.seal_for_tests(&l(), 1, k(0x4D)).unwrap();
        let sealed_bytes = s.get(&lifecycle_key(&l()).unwrap()).unwrap();
        match lifecycle(&s) {
            LifecyclePresence::Established(r) => {
                assert!(matches!(r.state, StoredLifecycle::Sealed { .. }));
                assert_eq!(r.first_commitment_hash, k(0x4D), "K_first witness");
            }
            other => panic!("{other:?}"),
        }

        // ---- Sealed: predecessor authority refused through the command path ----
        assert_eq!(
            refused(h.submit(create_with("late", L, text())).await.unwrap_err()),
            FenceRefusal::Sealed
        );
        assert_eq!(
            refused(h.submit(open("draft")).await.unwrap_err()),
            FenceRefusal::Sealed
        );
        let mut cfg = member_domain(L, &node).config;
        cfg.params.voting_period_seconds += 120;
        assert_eq!(
            refused(
                h.submit(GovernanceCommand::UpdateDomainConfig {
                    domain_id: l(),
                    new_config: cfg,
                })
                .await
                .unwrap_err()
            ),
            FenceRefusal::Sealed
        );
        assert_eq!(s.get(&domain_key(&l())).unwrap().unwrap(), pinned_domain);
        h.submit(create_with("m2", m_dom, text())).await.unwrap();
        let history: Vec<_> = ["a", "b", "o", "draft"]
            .iter()
            .map(|id| serde_json::to_vec(&stored(&s, id).unwrap()).unwrap())
            .collect();
        s.flush().unwrap();
        h.shutdown().await;
        drop((h, owner));
        drop(s);

        // ---- Restart again: nothing resurrects the predecessor ----
        let (s, _owner, h) = boot(dir.path(), &receipts, &node).await;
        h.recover_incomplete_closes().await;
        assert_eq!(s.get(&lifecycle_key(&l()).unwrap()).unwrap(), sealed_bytes);
        assert_eq!(
            refused(h.submit(create_with("again", L, text())).await.unwrap_err()),
            FenceRefusal::Sealed
        );
        assert_eq!(
            refused(h.submit(open("draft")).await.unwrap_err()),
            FenceRefusal::Sealed
        );
        let after: Vec<_> = ["a", "b", "o", "draft"]
            .iter()
            .map(|id| serde_json::to_vec(&stored(&s, id).unwrap()).unwrap())
            .collect();
        assert_eq!(
            after, history,
            "predecessor history preserved byte for byte"
        );
        h.submit(create_with("m3", m_dom, text())).await.unwrap();
    }
}
