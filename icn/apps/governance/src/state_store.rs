//! GovernanceStateStore — typed storage abstraction for the GovernanceActor.
//!
//! This module decouples the actor from raw `icn_store::Store` byte operations.
//! The actor works entirely in domain terms; this module owns all key encoding
//! and serialization details.

use anyhow::Result;
use std::sync::Arc;

use icn_governance::{
    ActorModelError, Delegation, DelegationId, DomainPolicyId, GovernanceDomain,
    GovernanceDomainId, HumanActorModel, InstitutionalDomain, Proposal, ProposalId, Timestamp,
    Vote,
};
use icn_identity::Did;
use icn_store::{CompareAndPut, Store};

use crate::close_journal::CloseJournalEntry;

// ---- Trait ----

/// State storage abstraction for the GovernanceActor.
///
/// The actor works in domain terms; the implementor handles serialization
/// and key encoding. This keeps raw byte-level concerns out of actor.rs.
pub trait GovernanceStateStore: Send + Sync {
    // --- Domains ---

    /// Load a domain by ID.
    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>>;

    /// Persist a domain (insert or overwrite), monotonic in the actor model:
    /// a stored `ContextSubjectV1` domain is never replaced by one that is not
    /// (`ActorModelError::ModeChangeRefused`). Implementations must make that
    /// check atomic with the write; an unreadable stored record is refused, and
    /// a backend that cannot do an atomic conditional write must fail rather
    /// than fall back to an unconditional overwrite. This binds callers of this
    /// trait only: it does not stop a raw `Store` write.
    fn save_domain(&self, domain: &GovernanceDomain) -> Result<()>;

    /// Return all domains in the store.
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>>;

    // --- Proposals ---

    /// Load a proposal by ID.
    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>>;

    /// Persist a proposal (insert or overwrite).
    fn save_proposal(&self, proposal: &Proposal) -> Result<()>;

    /// Return all proposals in the store.
    fn list_proposals(&self) -> Result<Vec<Proposal>>;

    // --- Votes ---

    /// Load a single vote cast by `voter` on `proposal_id`.
    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>>;

    /// Persist a vote (insert or overwrite).
    fn save_vote(&self, proposal_id: &ProposalId, vote: &Vote) -> Result<()>;

    /// Return all votes for a given proposal.
    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>>;

    // --- Delegations ---

    /// Load a delegation by ID.
    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>>;

    /// Persist a delegation (insert or overwrite).
    fn save_delegation(&self, delegation: &Delegation) -> Result<()>;

    /// Return all delegations in the store (unfiltered).
    fn list_all_delegations(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Persist a delegation with an updated `revoked_at` field.
    fn save_revoked_delegation(&self, delegation: &Delegation, revoked_at: Timestamp)
        -> Result<()>;

    // --- Governance proofs ---

    /// Load the raw proof bytes for a closed proposal.
    fn get_proof_bytes(&self, proposal_id: &ProposalId) -> Result<Option<Vec<u8>>>;

    /// Persist raw proof bytes for a proposal.
    fn save_proof_bytes(&self, proposal_id: &ProposalId, proof: &[u8]) -> Result<()>;

    // --- Close journal (write-ahead log for cross-store-atomic close) ---

    /// Persist a write-ahead [`CloseJournalEntry`] recording an in-flight
    /// proposal close, keyed by the proposal id. Written before any
    /// terminal-region provenance write so a crash or terminal-save failure
    /// after a receipt is durable can be replayed to completion rather than
    /// left as a permanent phantom-accepted half-close. See
    /// [`crate::close_journal`].
    ///
    /// Default impl is a no-op so non-durable test stores opt out of
    /// write-ahead recovery without breaking; the sled-backed store overrides.
    fn save_close_intent(&self, _entry: &CloseJournalEntry) -> Result<()> {
        Ok(())
    }

    /// Delete the close-journal entry for a proposal once its close is fully
    /// durable. Idempotent: deleting a missing entry is `Ok(())`.
    fn delete_close_intent(&self, _proposal_id: &ProposalId) -> Result<()> {
        Ok(())
    }

    /// List all lingering close-journal entries, for replay on actor startup.
    /// Default impl returns an empty list (the store opts out of recovery).
    fn list_close_intents(&self) -> Result<Vec<CloseJournalEntry>> {
        Ok(vec![])
    }

    /// Fetch the close-journal entry for a specific proposal, if one is in
    /// flight. Lets the close handler detect a prior incomplete attempt and
    /// complete THAT entry rather than overwriting it with a fresh close (which
    /// would orphan the first attempt's already-durable append-only receipt).
    /// Default impl returns `None`.
    fn get_close_intent(&self, _proposal_id: &ProposalId) -> Result<Option<CloseJournalEntry>> {
        Ok(None)
    }

    /// Force buffered state-store writes durable (fsync).
    ///
    /// Default no-op; the sled-backed store overrides it. `save_close_intent`
    /// uses this so the write-ahead record is durable before the caller writes
    /// cross-store provenance artifacts.
    fn flush(&self) -> Result<()> {
        Ok(())
    }

    // --- Institutional domains (ADR-0083 persistence addendum, #2142) ---

    /// Load the [`InstitutionalDomain`] authority record for `id`, if one has
    /// been declared. Keyed by the **existing** `GovernanceDomainId` and stored
    /// as a **separate sibling record** from `GovernanceDomain` (a distinct key
    /// space); it carries only the adopted `current_policy: DomainPolicyRef`
    /// pointer, never the `DomainPolicy` body (#1817).
    ///
    /// **Default impl FAILS CLOSED** (returns `Err`): a store that does not
    /// implement institutional-domain persistence must not masquerade as
    /// "no domain declared". Callers (declare / persisted-adopt) treat the error
    /// as a wiring fault, not as absence. The sled-backed store overrides this.
    fn get_institutional_domain(
        &self,
        _id: &GovernanceDomainId,
    ) -> Result<Option<InstitutionalDomain>> {
        anyhow::bail!(
            "institutional_domain persistence not implemented by this GovernanceStateStore"
        )
    }

    /// Persist an [`InstitutionalDomain`] authority record (insert or overwrite),
    /// keyed by its `domain_id`. **Default impl FAILS CLOSED** for the same
    /// reason as [`Self::get_institutional_domain`]; the sled-backed store
    /// overrides this.
    fn save_institutional_domain(&self, _domain: &InstitutionalDomain) -> Result<()> {
        anyhow::bail!(
            "institutional_domain persistence not implemented by this GovernanceStateStore"
        )
    }

    // --- DomainPolicy bodies (content-addressed; #2178, after #2142) ---

    /// Load the raw `DomainPolicy` body bytes whose blake3 hash is `id`, if a
    /// body has been persisted. `DomainPolicyId` is content-addressed
    /// (`DomainPolicyId::from_content`), so the key *is* the content hash; a
    /// stored body must re-hash to its key or the implementor MUST fail closed.
    ///
    /// `#2142` persists only the adopted `current_policy: DomainPolicyRef`
    /// pointer on the `InstitutionalDomain` record and drops the body after
    /// hashing; this seam lets an adopted ref be resolved back to the bytes that
    /// produced it. The adoption route is not yet wired to persist bodies, and
    /// the full CCL policy registry (#1817) remains future work.
    ///
    /// **Default impl FAILS CLOSED** (returns `Err`): a store that does not
    /// implement body persistence must not masquerade as "no body stored".
    /// The sled-backed store overrides this.
    fn get_domain_policy_body(&self, _id: &DomainPolicyId) -> Result<Option<Vec<u8>>> {
        anyhow::bail!("domain policy body storage is not implemented by this GovernanceStateStore")
    }

    /// Persist the raw `DomainPolicy` body `content` under its content-addressed
    /// `id`. The implementor MUST verify `DomainPolicyId::from_content(content)`
    /// equals `id` and fail closed on mismatch, so a body can never be filed
    /// under a key it does not hash to. Idempotent by construction (identical
    /// content → identical key). **Default impl FAILS CLOSED** for the same
    /// reason as [`Self::get_domain_policy_body`]; the sled-backed store
    /// overrides this.
    fn save_domain_policy_body(&self, _id: &DomainPolicyId, _content: &[u8]) -> Result<()> {
        anyhow::bail!("domain policy body storage is not implemented by this GovernanceStateStore")
    }
}

// ---- Key helpers ----

fn domain_key(id: &GovernanceDomainId) -> Vec<u8> {
    format!("gov:domain:{}", id.0).into_bytes()
}

fn domain_key_prefix() -> &'static [u8] {
    b"gov:domain:"
}

fn institutional_domain_key(id: &GovernanceDomainId) -> Vec<u8> {
    // Distinct key space from `gov:domain:` (GovernanceDomain) — the
    // InstitutionalDomain authority record is a sibling, not a replacement.
    format!("gov:institutional-domain:{}", id.0).into_bytes()
}

fn domain_policy_body_key(id: &DomainPolicyId) -> Vec<u8> {
    // Content-addressed key space, sibling to `gov:institutional-domain:`.
    // Keyed by the policy's content hash (`DomainPolicyId`, hex via Display),
    // not by a domain id — identical policy text dedupes across domains by
    // construction, and the key doubles as an integrity tag on read.
    format!("gov:domain-policy-body:{id}").into_bytes()
}

fn proposal_key(id: &ProposalId) -> Vec<u8> {
    format!("gov:proposal:{}", id.0).into_bytes()
}

fn proposal_key_prefix() -> &'static [u8] {
    b"gov:proposal:"
}

fn vote_key(proposal_id: &ProposalId, voter: &Did) -> Vec<u8> {
    format!("gov:vote:{}:{}", proposal_id.0, voter).into_bytes()
}

fn vote_key_prefix(proposal_id: &ProposalId) -> Vec<u8> {
    format!("gov:vote:{}:", proposal_id.0).into_bytes()
}

fn delegation_key(id: &DelegationId) -> Vec<u8> {
    format!("gov:delegation:{}", id.0).into_bytes()
}

fn delegation_key_prefix() -> &'static [u8] {
    b"gov:delegation:"
}

fn proof_key(proposal_id: &ProposalId) -> Vec<u8> {
    format!("governance:proof:{}", proposal_id.0).into_bytes()
}

fn close_intent_key(id: &ProposalId) -> Vec<u8> {
    format!("gov:close-intent:{}", id.0).into_bytes()
}

fn close_intent_key_prefix() -> &'static [u8] {
    b"gov:close-intent:"
}

// ---- Utility ----

pub(crate) fn load_json<T: for<'a> serde::Deserialize<'a>>(
    store: &dyn Store,
    key: &[u8],
) -> Result<Option<T>> {
    match store.get(key)? {
        Some(v) => Ok(Some(serde_json::from_slice::<T>(&v)?)),
        None => Ok(None),
    }
}

// ---- SledGovernanceStateStore ----

/// `icn_store::Store`-backed implementation of [`GovernanceStateStore`].
///
/// All key encoding is centralised here; the actor never constructs raw keys.
///
/// # No raw-handle accessor (#2864 A2)
///
/// A dependent crate holding this type cannot obtain the underlying
/// `dyn Store` from it: a raw handle would let it `put`/`delete` any key,
/// including a `gov:domain:*` record, without the monotonic check in
/// [`GovernanceStateStore::save_domain`]. The pair below guards that. The
/// control compiles; the `compile_fail` block differs from it only by the call
/// to the removed accessor, so it cannot pass for an unrelated reason.
///
/// ```
/// use icn_governance::GovernanceDomainId;
/// use icn_governance_actor::{GovernanceStateStore, SledGovernanceStateStore};
/// use icn_store::SledStore;
/// use std::sync::Arc;
///
/// let typed = SledGovernanceStateStore::new(Arc::new(SledStore::temporary().unwrap()));
/// let _ = typed.get_domain(&GovernanceDomainId::new("d")).unwrap();
/// ```
///
/// ```compile_fail
/// use icn_governance::GovernanceDomainId;
/// use icn_governance_actor::{GovernanceStateStore, SledGovernanceStateStore};
/// use icn_store::SledStore;
/// use std::sync::Arc;
///
/// let typed = SledGovernanceStateStore::new(Arc::new(SledStore::temporary().unwrap()));
/// let _ = typed.get_domain(&GovernanceDomainId::new("d")).unwrap();
/// let _raw = typed.as_store();
/// ```
pub struct SledGovernanceStateStore {
    store: Arc<dyn Store>,
}

impl SledGovernanceStateStore {
    /// Wrap an existing `Arc<dyn Store>`.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Raw KV access to the inner store, for this module's own tests that
    /// corrupt a stored row on purpose.
    ///
    /// Test-only by design (#2864 A2). It used to be `pub` and name the gossip
    /// ingress as its motivating caller; that ingress no longer applies
    /// replicated state and holds no store handle at all (see
    /// `actor::observe_replicated_governance_message`), so the only remaining
    /// caller is a test. A public raw accessor would let any holder of this type
    /// bypass the monotonic check in `save_domain`.
    #[cfg(test)]
    pub(crate) fn as_store(&self) -> &dyn Store {
        self.store.as_ref()
    }
}

/// Upper bound on read→validate→compare-and-put rounds in `save_domain`.
///
/// Every lost round means another writer's write landed, so the system as a
/// whole makes progress; the bound only stops one caller from spinning forever
/// against a pathological writer. Exhaustion fails closed.
pub(crate) const SAVE_DOMAIN_MAX_ATTEMPTS: usize = 16;

impl GovernanceStateStore for SledGovernanceStateStore {
    // --- Domains ---

    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        load_json(self.store.as_ref(), &domain_key(id))
    }

    fn save_domain(&self, domain: &GovernanceDomain) -> Result<()> {
        // Refuse a mixed-actor-model domain at write time; it could not be
        // loaded back, so persisting it would poison `list_domains`.
        domain.check_actor_model_invariant()?;
        let id = GovernanceDomainId(domain.id.0.clone());
        let key = domain_key(&id);
        let new_bytes = serde_json::to_vec(domain)?;

        // Monotonic actor model: a stored `ContextSubjectV1` domain is never
        // replaced by a record that is not. The check is made against the exact
        // bytes that the write then replaces, by compare-and-put, so a second
        // writer landing between the read and the write turns into a conflict
        // that is re-read and re-validated, not into a lost update.
        for _ in 0..SAVE_DOMAIN_MAX_ATTEMPTS {
            let current = self.store.get(&key)?;
            if let Some(bytes) = current.as_deref() {
                // An unreadable record cannot be proven not to be a Subject
                // domain, so it is refused, never overwritten.
                let existing: GovernanceDomain = serde_json::from_slice(bytes).map_err(|e| {
                    anyhow::anyhow!(
                        "Domain '{}': stored record cannot be read ({e}); refusing to overwrite it",
                        id.0
                    )
                })?;
                if existing.human_actor_model() == HumanActorModel::ContextSubjectV1
                    && domain.human_actor_model() != HumanActorModel::ContextSubjectV1
                {
                    return Err(ActorModelError::ModeChangeRefused {
                        domain: id.0.clone(),
                    }
                    .into());
                }
            }
            match self
                .store
                .compare_and_put(&key, current.as_deref(), &new_bytes)?
            {
                CompareAndPut::Applied => return Ok(()),
                // Someone else wrote first. Re-read and re-validate.
                CompareAndPut::Conflict { .. } => continue,
            }
        }
        anyhow::bail!(
            "Domain '{}': gave up saving after {SAVE_DOMAIN_MAX_ATTEMPTS} attempts \
             because concurrent writers kept changing the record",
            id.0
        )
    }

    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let rows = self.store.scan(domain_key_prefix())?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<GovernanceDomain>(&v)?))
            .collect()
    }

    // --- Institutional domains ---

    fn get_institutional_domain(
        &self,
        id: &GovernanceDomainId,
    ) -> Result<Option<InstitutionalDomain>> {
        load_json(self.store.as_ref(), &institutional_domain_key(id))
    }

    fn save_institutional_domain(&self, domain: &InstitutionalDomain) -> Result<()> {
        self.store.put(
            &institutional_domain_key(&domain.domain_id),
            &serde_json::to_vec(domain)?,
        )
    }

    // --- DomainPolicy bodies ---

    fn get_domain_policy_body(&self, id: &DomainPolicyId) -> Result<Option<Vec<u8>>> {
        match self.store.get(&domain_policy_body_key(id))? {
            Some(bytes) => {
                // The key IS the content hash, so a stored body that no longer
                // re-hashes to its key is corruption. Fail closed rather than
                // hand back mismatched bytes.
                let actual = DomainPolicyId::from_content(&bytes);
                if &actual != id {
                    anyhow::bail!(
                        "domain policy body integrity check failed: body under {id} hashes to {actual}"
                    );
                }
                Ok(Some(bytes))
            }
            None => Ok(None),
        }
    }

    fn save_domain_policy_body(&self, id: &DomainPolicyId, content: &[u8]) -> Result<()> {
        // Refuse to file a body under a key it does not hash to, so the keyspace
        // stays an honest content address.
        let actual = DomainPolicyId::from_content(content);
        if &actual != id {
            anyhow::bail!(
                "domain policy body integrity check failed: content hashes to {actual}, not the claimed {id}"
            );
        }
        self.store.put(&domain_policy_body_key(id), content)
    }

    // --- Proposals ---

    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        load_json(self.store.as_ref(), &proposal_key(id))
    }

    fn save_proposal(&self, proposal: &Proposal) -> Result<()> {
        self.store
            .put(&proposal_key(&proposal.id), &serde_json::to_vec(proposal)?)
    }

    fn list_proposals(&self) -> Result<Vec<Proposal>> {
        let rows = self.store.scan(proposal_key_prefix())?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<Proposal>(&v)?))
            .collect()
    }

    // --- Votes ---

    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>> {
        load_json(self.store.as_ref(), &vote_key(proposal_id, voter))
    }

    fn save_vote(&self, proposal_id: &ProposalId, vote: &Vote) -> Result<()> {
        self.store.put(
            &vote_key(proposal_id, &vote.voter),
            &serde_json::to_vec(vote)?,
        )
    }

    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>> {
        let prefix = vote_key_prefix(proposal_id);
        let rows = self.store.scan(&prefix)?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<Vote>(&v)?))
            .collect()
    }

    // --- Delegations ---

    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        load_json(self.store.as_ref(), &delegation_key(id))
    }

    fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        self.store.put(
            &delegation_key(&delegation.id),
            &serde_json::to_vec(delegation)?,
        )
    }

    fn list_all_delegations(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.store.scan(delegation_key_prefix())
    }

    fn save_revoked_delegation(
        &self,
        delegation: &Delegation,
        revoked_at: Timestamp,
    ) -> Result<()> {
        let mut d = delegation.clone();
        d.revoked_at = Some(revoked_at);
        self.store
            .put(&delegation_key(&d.id), &serde_json::to_vec(&d)?)
    }

    // --- Proofs ---

    fn get_proof_bytes(&self, proposal_id: &ProposalId) -> Result<Option<Vec<u8>>> {
        self.store.get(&proof_key(proposal_id))
    }

    fn save_proof_bytes(&self, proposal_id: &ProposalId, proof: &[u8]) -> Result<()> {
        self.store.put(&proof_key(proposal_id), proof)
    }

    // --- Close journal ---

    fn save_close_intent(&self, entry: &CloseJournalEntry) -> Result<()> {
        self.store.put(
            &close_intent_key(&entry.proposal.id),
            &serde_json::to_vec(entry)?,
        )?;
        // Force the write-ahead record durable BEFORE the caller writes any
        // cross-store provenance artifact. `Store::put` only buffers the sled
        // insert; without this fsync a crash could persist a Db-A governance
        // receipt while losing the unflushed intent in Db-B, reviving the
        // phantom-accepted half-close the journal exists to prevent.
        self.store.flush()
    }

    fn delete_close_intent(&self, proposal_id: &ProposalId) -> Result<()> {
        self.store.delete(&close_intent_key(proposal_id))
    }

    fn flush(&self) -> Result<()> {
        self.store.flush()
    }

    fn list_close_intents(&self) -> Result<Vec<CloseJournalEntry>> {
        let rows = self.store.scan(close_intent_key_prefix())?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<CloseJournalEntry>(&v)?))
            .collect()
    }

    fn get_close_intent(&self, proposal_id: &ProposalId) -> Result<Option<CloseJournalEntry>> {
        match self.store.get(&close_intent_key(proposal_id))? {
            Some(bytes) => Ok(Some(serde_json::from_slice::<CloseJournalEntry>(&bytes)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use icn_governance::BootstrapEntityType;
    use icn_store::SledStore;

    fn temp_store() -> SledGovernanceStateStore {
        SledGovernanceStateStore::new(Arc::new(SledStore::temporary().expect("temp sled")))
    }

    #[test]
    fn institutional_domain_round_trips() {
        let store = temp_store();
        let id = GovernanceDomainId::new("coop-alpha");
        assert!(store.get_institutional_domain(&id).unwrap().is_none());

        let domain = InstitutionalDomain::declare(id.clone(), BootstrapEntityType::Cooperative);
        store.save_institutional_domain(&domain).unwrap();

        let loaded = store
            .get_institutional_domain(&id)
            .unwrap()
            .expect("present after save");
        assert_eq!(loaded, domain);

        // Distinct key space: the GovernanceDomain get for the same id is
        // unaffected by the institutional-domain write.
        assert!(store.get_domain(&id).unwrap().is_none());
    }

    /// A `GovernanceStateStore` that does NOT override the institutional-domain
    /// methods must hit the fail-closed defaults (`Err`), never silent
    /// `Ok`/`None`. All other (required) methods are unreachable in this test.
    struct NoInstitutionalDomainStore;

    impl GovernanceStateStore for NoInstitutionalDomainStore {
        fn get_domain(&self, _: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
            unimplemented!()
        }
        fn save_domain(&self, _: &GovernanceDomain) -> Result<()> {
            unimplemented!()
        }
        fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
            unimplemented!()
        }
        fn get_proposal(&self, _: &ProposalId) -> Result<Option<Proposal>> {
            unimplemented!()
        }
        fn save_proposal(&self, _: &Proposal) -> Result<()> {
            unimplemented!()
        }
        fn list_proposals(&self) -> Result<Vec<Proposal>> {
            unimplemented!()
        }
        fn get_vote(&self, _: &ProposalId, _: &Did) -> Result<Option<Vote>> {
            unimplemented!()
        }
        fn save_vote(&self, _: &ProposalId, _: &Vote) -> Result<()> {
            unimplemented!()
        }
        fn list_votes(&self, _: &ProposalId) -> Result<Vec<Vote>> {
            unimplemented!()
        }
        fn get_delegation(&self, _: &DelegationId) -> Result<Option<Delegation>> {
            unimplemented!()
        }
        fn save_delegation(&self, _: &Delegation) -> Result<()> {
            unimplemented!()
        }
        fn list_all_delegations(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            unimplemented!()
        }
        fn save_revoked_delegation(&self, _: &Delegation, _: Timestamp) -> Result<()> {
            unimplemented!()
        }
        fn get_proof_bytes(&self, _: &ProposalId) -> Result<Option<Vec<u8>>> {
            unimplemented!()
        }
        fn save_proof_bytes(&self, _: &ProposalId, _: &[u8]) -> Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn default_institutional_domain_methods_fail_closed() {
        let store = NoInstitutionalDomainStore;
        let id = GovernanceDomainId::new("coop-alpha");
        assert!(
            store.get_institutional_domain(&id).is_err(),
            "default get must fail closed, not return Ok(None)"
        );
        let domain = InstitutionalDomain::declare(id, BootstrapEntityType::Cooperative);
        assert!(
            store.save_institutional_domain(&domain).is_err(),
            "default save must fail closed, not silently succeed"
        );
    }

    #[test]
    fn domain_policy_body_round_trips() {
        let store = temp_store();
        let content: &[u8] = b"charter: { quorum: 0.5 }";
        let id = DomainPolicyId::from_content(content);
        assert!(store.get_domain_policy_body(&id).unwrap().is_none());

        store.save_domain_policy_body(&id, content).unwrap();

        let loaded = store
            .get_domain_policy_body(&id)
            .unwrap()
            .expect("present after save");
        assert_eq!(loaded.as_slice(), content);
    }

    #[test]
    fn domain_policy_body_missing_returns_none() {
        let store = temp_store();
        let id = DomainPolicyId::from_content(b"never stored");
        assert!(store.get_domain_policy_body(&id).unwrap().is_none());
    }

    #[test]
    fn domain_policy_body_key_is_content_addressed() {
        // The key is derived from the content, so identical bytes land on the
        // same key (natural dedup) and re-saving is idempotent.
        let store = temp_store();
        let content: &[u8] = b"identical policy text";
        let id_a = DomainPolicyId::from_content(content);
        let id_b = DomainPolicyId::from_content(content);
        assert_eq!(id_a, id_b, "content addressing is deterministic");

        store.save_domain_policy_body(&id_a, content).unwrap();
        store.save_domain_policy_body(&id_b, content).unwrap();

        let loaded = store
            .get_domain_policy_body(&id_a)
            .unwrap()
            .expect("present after save");
        assert_eq!(loaded.as_slice(), content);
    }

    #[test]
    fn domain_policy_body_corrupted_fails_closed() {
        // A stored body whose bytes do not hash to its key is corruption; the
        // read must fail closed, never hand back mismatched bytes.
        let store = temp_store();
        let id = DomainPolicyId::from_content(b"the real policy");
        store
            .as_store()
            .put(&domain_policy_body_key(&id), b"tampered bytes")
            .unwrap();
        assert!(
            store.get_domain_policy_body(&id).is_err(),
            "stored body that does not re-hash to its key must fail closed"
        );
    }

    #[test]
    fn domain_policy_body_save_rejects_id_content_mismatch() {
        // A caller-supplied id that does not hash the content is rejected, so a
        // body can never be filed under a key it does not hash to.
        let store = temp_store();
        let wrong_id = DomainPolicyId::from_content(b"some other policy");
        assert!(
            store
                .save_domain_policy_body(&wrong_id, b"the actual policy")
                .is_err(),
            "saving content under an id it does not hash to must fail closed"
        );
        assert!(
            store.get_domain_policy_body(&wrong_id).unwrap().is_none(),
            "the rejected save must not have written anything"
        );
    }

    #[test]
    fn domain_policy_body_default_impl_fails_closed() {
        let store = NoInstitutionalDomainStore;
        let id = DomainPolicyId::from_content(b"anything");
        assert!(
            store.get_domain_policy_body(&id).is_err(),
            "default get must fail closed, not return Ok(None)"
        );
        assert!(
            store.save_domain_policy_body(&id, b"anything").is_err(),
            "default save must fail closed, not silently succeed"
        );
    }
}

/// Test-only decorator for deterministic interleavings against a real sled.
#[cfg(test)]
pub(crate) mod test_support {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use icn_store::{
        CompareAndPut, CompareAndPutUnsupported, ContentHash, ReplicaMetadata, SledStore, Store,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Called after every `get` of the armed key, with the underlying store and
    /// the 1-based count of such reads. The hook plays "another writer".
    pub type GetHook = Box<dyn Fn(&SledStore, usize) + Send + Sync>;

    pub struct HookStore {
        pub inner: SledStore,
        hook: Mutex<Option<(Vec<u8>, GetHook)>>,
        gets: AtomicUsize,
        /// `false` models a backend that does not override `compare_and_put`.
        forward_cas: bool,
    }

    impl HookStore {
        pub fn new(forward_cas: bool) -> Self {
            Self {
                inner: SledStore::temporary().expect("temp sled"),
                hook: Mutex::new(None),
                gets: AtomicUsize::new(0),
                forward_cas,
            }
        }
        pub fn arm(&self, key: Vec<u8>, hook: GetHook) {
            *self.hook.lock().unwrap() = Some((key, hook));
        }
        pub fn reads_of_armed_key(&self) -> usize {
            self.gets.load(Ordering::SeqCst)
        }
    }

    impl Store for HookStore {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            // The value is read BEFORE the hook runs, so the caller holds a
            // view that the hook then makes stale: a deterministic race.
            let seen = self.inner.get(key)?;
            if let Some((armed, hook)) = self.hook.lock().unwrap().as_ref() {
                if armed.as_slice() == key {
                    let n = self.gets.fetch_add(1, Ordering::SeqCst) + 1;
                    hook(&self.inner, n);
                }
            }
            Ok(seen)
        }
        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
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
        fn get_replica_metadata(&self, h: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
            self.inner.get_replica_metadata(h)
        }
        fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
            self.inner.put_replica_metadata(m)
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
            self.inner.list_replica_hashes()
        }
        fn compare_and_put(
            &self,
            key: &[u8],
            expected: Option<&[u8]>,
            new: &[u8],
        ) -> anyhow::Result<CompareAndPut> {
            if self.forward_cas {
                self.inner.compare_and_put(key, expected, new)
            } else {
                Err(CompareAndPutUnsupported.into())
            }
        }
    }
}

/// `save_domain` must never turn a persisted `ContextSubjectV1` domain back
/// into a legacy one, for any cooperating writer going through this API.
#[cfg(test)]
mod monotonic_domain_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::test_support::HookStore;
    use super::*;
    use icn_governance::{ActorModelError, GovernanceConfig, HumanActorModel, MembershipConfig};
    use icn_identity::IdentityBundle;
    use icn_store::{CompareAndPutUnsupported, Store};

    fn legacy(id: &str, name: &str) -> GovernanceDomain {
        let mut config = GovernanceConfig::cooperative_default();
        config.membership =
            MembershipConfig::static_list(vec![IdentityBundle::generate().unwrap().did().clone()]);
        GovernanceDomain::with_id(GovernanceDomainId::new(id), name.to_string(), config)
    }

    fn subject(id: &str) -> GovernanceDomain {
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = MembershipConfig::static_list(vec![]);
        GovernanceDomain::with_id(GovernanceDomainId::new(id), id.to_string(), config)
            .with_human_actor_model(HumanActorModel::ContextSubjectV1)
            .unwrap()
    }

    fn key(id: &str) -> Vec<u8> {
        domain_key(&GovernanceDomainId::new(id))
    }

    fn typed(store: &Arc<HookStore>) -> SledGovernanceStateStore {
        SledGovernanceStateStore::new(store.clone())
    }

    fn stored(store: &HookStore, id: &str) -> Option<Vec<u8>> {
        store.inner.get(&key(id)).unwrap()
    }

    fn stored_model(store: &HookStore, id: &str) -> HumanActorModel {
        let bytes = stored(store, id).expect("record present");
        serde_json::from_slice::<GovernanceDomain>(&bytes)
            .unwrap()
            .human_actor_model()
    }

    fn assert_mode_change_refused(err: anyhow::Error) {
        assert!(
            matches!(
                err.downcast_ref::<ActorModelError>(),
                Some(ActorModelError::ModeChangeRefused { .. })
            ),
            "expected ModeChangeRefused, got: {err:#}"
        );
    }

    #[test]
    fn creating_an_absent_domain_succeeds_in_both_models() {
        let store = Arc::new(HookStore::new(true));
        let s = typed(&store);
        s.save_domain(&legacy("l", "l")).unwrap();
        s.save_domain(&subject("s")).unwrap();
        assert_eq!(
            stored_model(&store, "l"),
            HumanActorModel::LegacyPrincipalV0
        );
        assert_eq!(stored_model(&store, "s"), HumanActorModel::ContextSubjectV1);
    }

    #[test]
    fn legacy_serialization_and_replacement_are_unchanged() {
        let store = Arc::new(HookStore::new(true));
        let s = typed(&store);
        let first = legacy("d", "first");
        s.save_domain(&first).unwrap();
        let second = legacy("d", "second");
        s.save_domain(&second).unwrap();
        let bytes = stored(&store, "d").unwrap();
        assert_eq!(bytes, serde_json::to_vec(&second).unwrap());
        // The legacy wire form carries no actor-model field.
        assert!(!String::from_utf8(bytes)
            .unwrap()
            .contains("human_actor_model"));
    }

    #[test]
    fn subject_to_subject_update_and_legacy_to_subject_are_allowed_at_the_store() {
        let store = Arc::new(HookStore::new(true));
        let s = typed(&store);
        s.save_domain(&subject("s")).unwrap();
        s.save_domain(&subject("s")).unwrap();
        // Whether a legacy domain may become Subject-mode is the domain
        // builder's rule (no members may be dropped); the store only forbids
        // leaving Subject mode.
        s.save_domain(&legacy("l", "l")).unwrap();
        s.save_domain(&subject("l")).unwrap();
        assert_eq!(stored_model(&store, "l"), HumanActorModel::ContextSubjectV1);
    }

    #[test]
    fn subject_to_legacy_is_refused_and_leaves_the_record_byte_identical() {
        let store = Arc::new(HookStore::new(true));
        let s = typed(&store);
        s.save_domain(&subject("d")).unwrap();
        let before = stored(&store, "d").unwrap();
        assert_mode_change_refused(s.save_domain(&legacy("d", "evil")).unwrap_err());
        assert_eq!(stored(&store, "d").unwrap(), before);
    }

    /// The deterministic race. The legacy writer's first view of the key is
    /// "absent"; a second writer then persists a Subject domain; the legacy
    /// writer proceeds on its stale view. It must not win.
    #[test]
    fn stale_legacy_writer_cannot_overwrite_an_interleaved_subject_write() {
        let store = Arc::new(HookStore::new(true));
        let subject_bytes = serde_json::to_vec(&subject("d")).unwrap();
        let injected = subject_bytes.clone();
        store.arm(
            key("d"),
            Box::new(move |inner, n| {
                if n == 1 {
                    inner.put(&key("d"), &injected).unwrap();
                }
            }),
        );
        let err = typed(&store)
            .save_domain(&legacy("d", "stale"))
            .expect_err("a stale legacy write must not replace a Subject domain");
        assert_mode_change_refused(err);
        assert_eq!(stored(&store, "d").unwrap(), subject_bytes);
    }

    /// A conflict with a harmless concurrent write is retried against the
    /// freshly read record rather than failing or clobbering it blindly.
    #[test]
    fn a_conflicting_legacy_write_is_revalidated_and_then_applied() {
        let store = Arc::new(HookStore::new(true));
        let intruder = serde_json::to_vec(&legacy("d", "intruder")).unwrap();
        store.arm(
            key("d"),
            Box::new(move |inner, n| {
                if n == 1 {
                    inner.put(&key("d"), &intruder).unwrap();
                }
            }),
        );
        let mine = legacy("d", "mine");
        typed(&store).save_domain(&mine).unwrap();
        assert_eq!(
            stored(&store, "d").unwrap(),
            serde_json::to_vec(&mine).unwrap()
        );
        assert_eq!(store.reads_of_armed_key(), 2, "one stale read, one retry");
    }

    /// A writer that keeps winning exhausts the bound, which fails closed.
    #[test]
    fn retry_exhaustion_fails_closed_without_overwriting() {
        let store = Arc::new(HookStore::new(true));
        store.arm(
            key("d"),
            Box::new(|inner, n| {
                let other = serde_json::to_vec(&legacy("d", &format!("w{n}"))).unwrap();
                inner.put(&key("d"), &other).unwrap();
            }),
        );
        let err = typed(&store)
            .save_domain(&legacy("d", "mine"))
            .expect_err("must give up");
        assert!(format!("{err:#}").contains("concurrent"), "{err:#}");
        assert_eq!(store.reads_of_armed_key(), SAVE_DOMAIN_MAX_ATTEMPTS);
        let last = format!("w{SAVE_DOMAIN_MAX_ATTEMPTS}");
        let kept: GovernanceDomain = serde_json::from_slice(&stored(&store, "d").unwrap()).unwrap();
        assert_eq!(kept.name, last, "the writer's own record must not land");
    }

    #[test]
    fn an_unreadable_existing_record_is_refused_not_overwritten() {
        let store = Arc::new(HookStore::new(true));
        store.inner.put(&key("d"), b"\xff not a domain").unwrap();
        let err = typed(&store).save_domain(&legacy("d", "x")).unwrap_err();
        assert!(format!("{err:#}").contains("cannot be read"), "{err:#}");
        assert_eq!(stored(&store, "d").unwrap(), b"\xff not a domain".to_vec());
    }

    #[test]
    fn a_backend_without_atomic_compare_and_put_fails_closed_and_writes_nothing() {
        let store = Arc::new(HookStore::new(false));
        let err = typed(&store).save_domain(&legacy("d", "x")).unwrap_err();
        assert!(
            err.downcast_ref::<CompareAndPutUnsupported>().is_some(),
            "{err:#}"
        );
        assert!(stored(&store, "d").is_none());
    }

    /// Real threads, real sled, no hooks. Whatever the interleaving, a Subject
    /// write that returned Ok must still be what is stored at the end.
    #[test]
    fn concurrent_subject_and_legacy_writers_never_regress_the_mode() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Barrier;
        for round in 0..40 {
            let store = Arc::new(HookStore::new(true));
            let gate = Arc::new(Barrier::new(8));
            let subject_ok = Arc::new(AtomicBool::new(false));
            let handles: Vec<_> = (0..8)
                .map(|i| {
                    let (store, gate, subject_ok) =
                        (store.clone(), gate.clone(), subject_ok.clone());
                    std::thread::spawn(move || {
                        let s = typed(&store);
                        gate.wait();
                        if i % 2 == 0 {
                            if s.save_domain(&subject("d")).is_ok() {
                                subject_ok.store(true, Ordering::SeqCst);
                            }
                        } else {
                            let _ = s.save_domain(&legacy("d", &format!("l{i}")));
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
            if subject_ok.load(Ordering::SeqCst) {
                assert_eq!(
                    stored_model(&store, "d"),
                    HumanActorModel::ContextSubjectV1,
                    "round {round}: a successful Subject write was lost to a legacy writer"
                );
            }
        }
    }

    /// KNOWN LIMITATION, pinned so it is not mistaken for protection: the
    /// invariant lives in `save_domain`. A raw `Store::delete` of the key (as
    /// any code holding the raw store could do) followed by a save is a
    /// "create", which is allowed. `GovernanceStateStore` has no delete.
    #[test]
    fn raw_store_delete_then_recreate_bypasses_the_typed_invariant() {
        let store = Arc::new(HookStore::new(true));
        let s = typed(&store);
        s.save_domain(&subject("d")).unwrap();
        store.inner.delete(&key("d")).unwrap();
        s.save_domain(&legacy("d", "recreated")).unwrap();
        assert_eq!(
            stored_model(&store, "d"),
            HumanActorModel::LegacyPrincipalV0
        );
    }
}
