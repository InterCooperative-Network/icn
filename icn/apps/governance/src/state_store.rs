//! GovernanceStateStore — typed storage abstraction for the GovernanceActor.
//!
//! This module decouples the actor from raw `icn_store::Store` byte operations.
//! The actor works entirely in domain terms; this module owns all key encoding
//! and serialization details.

use anyhow::Result;
use std::sync::Arc;

use icn_governance::{
    Delegation, DelegationId, DomainPolicyId, GovernanceDomain, GovernanceDomainId,
    InstitutionalDomain, Proposal, ProposalId, Timestamp, Vote,
};
use icn_identity::Did;
use icn_store::Store;

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

    /// Persist a domain (insert or overwrite).
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
pub struct SledGovernanceStateStore {
    store: Arc<dyn Store>,
}

impl SledGovernanceStateStore {
    /// Wrap an existing `Arc<dyn Store>`.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Expose the inner store for callers that still need raw KV access.
    ///
    /// This used to name the gossip ingress (`handle_incoming`) as the motivating
    /// caller. That ingress no longer applies replicated state and holds no store
    /// handle at all — see `actor::refuse_replicated_governance_message`.
    pub fn as_store(&self) -> &dyn Store {
        self.store.as_ref()
    }
}

impl GovernanceStateStore for SledGovernanceStateStore {
    // --- Domains ---

    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        load_json(self.store.as_ref(), &domain_key(id))
    }

    fn save_domain(&self, domain: &GovernanceDomain) -> Result<()> {
        let id = GovernanceDomainId(domain.id.0.clone());
        self.store
            .put(&domain_key(&id), &serde_json::to_vec(domain)?)
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
