//! P5b Seal evidence bound to the receipt backend at construction (EXPERIMENTAL, INACTIVE).
//!
//! The Seal needs two facts the governance store cannot establish itself (contract §4.2 step 5):
//! whether an accepted execution-required proposal's **durable intent** exists in the receipt
//! database (Db-A), and whether its payload changes a condition the cutover relies on.
//!
//! Before S28 those facts came from a `&dyn SealEvidence` passed **per call**, so any holder of
//! the lifecycle store could hand the Seal an oracle that reports every closure durable and no
//! basis change. This module closes that (S27 review FOLLOW_UP (b)):
//!
//! * [`SealEvidence`] is **crate-private**: no code outside this crate can implement it.
//! * The only operational source is [`BoundSealEvidence::receipts`], built from the actor's
//!   [`GovernanceReceiptBackend`] and bound to the lifecycle owner when it is claimed; the Seal
//!   takes no evidence argument.
//! * Test fixtures exist only under `#[cfg(test)]`.
//!
//! # What "durable" means here (contract §4.4)
//! Durable intent = the ADR-0014 **mandate** minted for the accepted proposal (every
//! execution-required payload gets one) **plus**, for the payloads that translate to an
//! institutional effect, that **institutional-effect record** (IER). Completion evidence
//! (dispatch evidence) is not required (D-S20-10). Each fact is checked for identity, not mere
//! presence: a mandate must name the proposal and the exact payload hash; an IER must name the
//! proposal, its domain and the expected kind, and there must be exactly one of that kind.
//! Anything contradictory is an error (the Seal refuses), never "not durable" and never
//! "durable".
//!
//! # What this module does NOT establish
//! * **No institutional authority.** Reading receipts confers none; this is a predicate over
//!   append-only artifacts that the governance close already wrote.
//! * **No cross-database atomicity.** Db-A is a separate sled database. The Seal relies on the
//!   receipt facts being monotone (append-only, no delete: contract §4.3.4), so a fact read here
//!   cannot later become false; a concurrent receipt write can only turn a refusal into a later
//!   success. [`SealEvidence::flush`] makes every fact relied on durable before the Seal commits.
//!   IER uniqueness per (proposal, kind) is enforced by the gateway receipt store from #2955
//!   (on main; this stack does not yet include it); this module refuses duplicates
//!   independently rather than assuming it.
//! * **An unwired backend refuses.** The trait's default lookups return empty, which reads as
//!   "not durable": the Seal is refused, never granted.
//! * The P5c predicate is a **conservative compiled-in list** (D-P5c-1 open): S19's closed list
//!   extended by payload semantics (see [`changes_cutover_basis`]). It refuses more, never less,
//!   and does not make P5c activation-ready.

use std::sync::Arc;

use icn_governance::{ProofOutcome, Proposal, ProposalPayload};

use crate::grant_minting::hash_proposal_payload;
use crate::institutional_effect::record_from_accepted_payload;
use crate::receipt_backend::GovernanceReceiptBackend;

/// The facts the Seal needs that the governance store cannot establish itself. Crate-private:
/// implementations are this module's receipt-backed source and `#[cfg(test)]` fixtures only.
pub(crate) trait SealEvidence: Send + Sync {
    /// Is `p`'s durable intent present? The Seal consults it only for rows whose
    /// classification depends on it (accepted, execution-required). `Err` on any contradiction
    /// or backend failure.
    fn closure_durable(&self, p: &Proposal) -> anyhow::Result<bool>;
    /// Does accepting `p` change a condition the cutover relies on (P5c predicate)?
    fn changes_cutover_basis(&self, p: &Proposal) -> bool;
    /// Make every fact reported so far durable before the Seal commits.
    fn flush(&self) -> anyhow::Result<()>;
}

/// The Seal evidence a lifecycle owner is bound to at construction. Opaque: the only public
/// constructor reads the receipt backend.
///
/// Code outside this crate cannot supply its own oracle: the evidence trait is crate-private.
///
/// ```compile_fail,E0603
/// // Complete apart from the trait's visibility, so privacy is the only reason this fails.
/// use icn_governance::Proposal;
/// struct AlwaysDurable;
/// impl icn_governance_actor::seal_evidence::SealEvidence for AlwaysDurable {
///     fn closure_durable(&self, _: &Proposal) -> anyhow::Result<bool> { Ok(true) }
///     fn changes_cutover_basis(&self, _: &Proposal) -> bool { false }
///     fn flush(&self) -> anyhow::Result<()> { Ok(()) }
/// }
/// ```
pub struct BoundSealEvidence(Arc<dyn SealEvidence>);

impl BoundSealEvidence {
    /// Evidence read from the actor's receipt backend (Db-A) with the compiled-in P5c predicate.
    pub fn receipts(backend: Arc<dyn GovernanceReceiptBackend>) -> Self {
        Self(Arc::new(ReceiptSealEvidence { backend }))
    }

    /// A test fixture. Never compiled outside tests.
    #[cfg(test)]
    pub(crate) fn fixture(evidence: Arc<dyn SealEvidence>) -> Self {
        Self(evidence)
    }

    pub(crate) fn get(&self) -> &dyn SealEvidence {
        self.0.as_ref()
    }
}

/// Why the receipt evidence was refused: a contradiction between artifacts, or a backend that
/// cannot answer. Missing evidence is not an error (it reads as "not durable").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealEvidenceError {
    Backend(String),
    PayloadHash,
    MandateForeignProposal(String),
    MandatePayloadMismatch(String),
    MandatesNotEnumerable(String),
    MandateDecisionShared(String),
    DecisionMissing(String),
    DecisionMismatch(String),
    EffectForeign(String),
    EffectDuplicate(String),
}

impl std::fmt::Display for SealEvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(x) => write!(f, "receipt backend: {x}"),
            Self::PayloadHash => f.write_str("the proposal payload cannot be hashed"),
            Self::MandateForeignProposal(x) => write!(f, "the mandate stored for proposal {x} names another proposal"),
            Self::MandatePayloadMismatch(x) => write!(f, "a mandate for proposal {x} binds a different payload"),
            Self::MandatesNotEnumerable(x) => write!(f, "the backend cannot enumerate the mandates of proposal {x}'s decision"),
            Self::MandateDecisionShared(x) => write!(f, "a mandate under proposal {x}'s decision names another proposal"),
            Self::DecisionMissing(x) => write!(f, "proposal {x} has a mandate but no decision receipt under its decision hash"),
            Self::DecisionMismatch(x) => write!(f, "the decision receipt under proposal {x}'s mandate does not match the proposal"),
            Self::EffectForeign(x) => write!(f, "an institutional-effect record of proposal {x} names another proposal, domain or decision"),
            Self::EffectDuplicate(x) => write!(f, "proposal {x} has more than one institutional-effect record of its kind"),
        }
    }
}

impl std::error::Error for SealEvidenceError {}

struct ReceiptSealEvidence {
    backend: Arc<dyn GovernanceReceiptBackend>,
}

impl SealEvidence for ReceiptSealEvidence {
    fn closure_durable(&self, p: &Proposal) -> anyhow::Result<bool> {
        closure_durable_from(self.backend.as_ref(), p)
    }

    fn changes_cutover_basis(&self, p: &Proposal) -> bool {
        changes_cutover_basis(&p.payload)
    }

    fn flush(&self) -> anyhow::Result<()> {
        self.backend
            .flush()
            .map_err(|e| SealEvidenceError::Backend(e).into())
    }
}

/// Contract §4.4 durable intent of the accepted execution-required proposal `p`, read from
/// `backend`. `Ok(false)`: an artifact is missing (the Seal refuses with unresolved effects).
/// `Err`: the artifacts contradict each other or the backend cannot answer (the Seal refuses).
///
/// Checked, in order:
/// 1. the earliest mandate for `p` names `p` and binds `p`'s exact payload hash;
/// 2. the backend can enumerate that mandate's decision (an unwired default returns nothing,
///    which is refused rather than read as "no other mandates"), and every mandate under the
///    decision names `p` and binds the same payload (several are permitted; contradictory ones
///    are not);
/// 3. the v1 decision receipt under that decision hash exists and names `p`, `p`'s domain and an
///    accepted outcome;
/// 4. if `p`'s payload translates to an institutional effect, exactly one record of that kind
///    exists, naming `p`, its domain and (when recorded) the same decision.
pub(crate) fn closure_durable_from(
    backend: &dyn GovernanceReceiptBackend,
    p: &Proposal,
) -> anyhow::Result<bool> {
    let id = p.id.0.as_str();
    let domain = p.domain_id.0.as_str();
    let be = |e: String| SealEvidenceError::Backend(e);
    let Some(mandate) = backend.get_mandate_by_proposal(id).map_err(be)? else {
        return Ok(false);
    };
    if mandate.decision.proposal_id != id {
        return Err(SealEvidenceError::MandateForeignProposal(id.to_string()).into());
    }
    let payload_hash =
        hash_proposal_payload(&p.payload).map_err(|_| SealEvidenceError::PayloadHash)?;
    if mandate.payload_hash != payload_hash {
        return Err(SealEvidenceError::MandatePayloadMismatch(id.to_string()).into());
    }
    let decision_hash = mandate.decision.decision_hash;
    let siblings = backend
        .list_mandates_by_decision(&decision_hash)
        .map_err(be)?;
    if !siblings.iter().any(|m| m.id == mandate.id) {
        return Err(SealEvidenceError::MandatesNotEnumerable(id.to_string()).into());
    }
    for m in &siblings {
        if m.decision.proposal_id != id {
            return Err(SealEvidenceError::MandateDecisionShared(id.to_string()).into());
        }
        if m.payload_hash != payload_hash {
            return Err(SealEvidenceError::MandatePayloadMismatch(id.to_string()).into());
        }
    }
    let Some(receipt) = backend
        .get_governance_by_decision(&decision_hash)
        .map_err(be)?
    else {
        return Err(SealEvidenceError::DecisionMissing(id.to_string()).into());
    };
    if receipt.proposal_id != id
        || receipt.domain_id != domain
        || receipt.decision_hash != decision_hash
        || receipt.outcome != ProofOutcome::Accepted
    {
        return Err(SealEvidenceError::DecisionMismatch(id.to_string()).into());
    }
    // Only the payloads that translate to an institutional effect have a record to find.
    let Some(expected) = record_from_accepted_payload(id, domain, None, &p.payload, 0) else {
        return Ok(true);
    };
    let mut found: Option<String> = None;
    for r in backend
        .list_institutional_effects_by_proposal(id)
        .map_err(be)?
    {
        if r.proposal_id != id
            || r.domain_id != domain
            || r.decision_hash.is_some_and(|h| h != decision_hash)
        {
            return Err(SealEvidenceError::EffectForeign(id.to_string()).into());
        }
        if r.effect_kind != expected.effect_kind {
            continue;
        }
        match &found {
            None => found = Some(r.record_id),
            Some(prior) if *prior == r.record_id => {}
            Some(_) => return Err(SealEvidenceError::EffectDuplicate(id.to_string()).into()),
        }
    }
    Ok(found.is_some())
}

/// The compiled-in P5c predicate (EXPERIMENTAL; D-P5c-1, the exact list, is open). A payload is
/// basis-changing when its acceptance can change who decides or under which rules: authority,
/// the electorate, membership standing, governance policy or configuration, recognition, or
/// authorization rules. S19's closed list (Membership, ConfigChange, Freeze/Unfreeze, protocol
/// changes) is extended by those semantics, and ambiguous cases are included (fail closed: a
/// spurious `true` only refuses the Seal until abort or reconciliation; a spurious `false`
/// would seal over a changed basis). The match is exhaustive with no wildcard so a new payload
/// variant cannot be added without classifying it here.
pub(crate) fn changes_cutover_basis(payload: &ProposalPayload) -> bool {
    match payload {
        // S19 closed list (every protocol change counts: whether it alters the electorate or
        // voting rules is not decidable from the payload yet).
        ProposalPayload::Membership { .. }
        | ProposalPayload::ConfigChange { .. }
        | ProposalPayload::FreezeMember { .. }
        | ProposalPayload::UnfreezeMember { .. }
        | ProposalPayload::ProtocolUpgrade { .. }
        | ProposalPayload::ProtocolChange { .. } => true,
        // By semantics: a kernel-enforced charter (authorization rules), steward authority and
        // thresholds, inter-cooperative recognition, and resource-access grants (ambiguous:
        // authorization over shared resources; included fail-closed).
        ProposalPayload::Charter { .. }
        | ProposalPayload::Sdis { .. }
        | ProposalPayload::Federation(..)
        | ProposalPayload::ResourceAccess { .. } => true,
        // Declarative, economic, operational, or acting on other proposals (whose own terminal
        // state already carries the effect): not inputs of the cutover basis.
        ProposalPayload::Text { .. }
        | ProposalPayload::Budget { .. }
        | ProposalPayload::Allocation { .. }
        | ProposalPayload::SchedulingPolicy { .. }
        | ProposalPayload::VetoProposal { .. }
        | ProposalPayload::ForceCloseProposal { .. }
        | ProposalPayload::RollbackLedger { .. }
        | ProposalPayload::DisputeResolution { .. }
        | ProposalPayload::Treasury { .. }
        | ProposalPayload::SurplusAllocation { .. }
        | ProposalPayload::ShareRedemption { .. }
        | ProposalPayload::BondIssuance { .. } => false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::institutional_effect::InstitutionalEffectRecord;
    use icn_governance::authority::DecisionProvenance;
    use icn_governance::mandate::Mandate;
    use icn_governance::{GovernanceDecisionReceipt, GovernanceDomainId, ProposalId, VoteTally};
    use std::sync::Mutex;

    /// An in-memory receipt backend holding real artifacts. `enumerate` off models a backend
    /// that keeps the trait's default (empty) `list_mandates_by_decision`.
    #[derive(Default)]
    pub(crate) struct Backend {
        pub(crate) mandates: Mutex<Vec<Mandate>>,
        pub(crate) receipts: Mutex<Vec<GovernanceDecisionReceipt>>,
        pub(crate) effects: Mutex<Vec<InstitutionalEffectRecord>>,
        pub(crate) no_enumerate: bool,
        /// When set, the by-proposal lookup returns this copy (an index/primary disagreement).
        pub(crate) by_proposal_copy: Mutex<Option<Mandate>>,
        pub(crate) flush_fails: bool,
    }

    impl GovernanceReceiptBackend for Backend {
        fn put_governance(&self, r: &GovernanceDecisionReceipt) -> Result<(), String> {
            self.receipts.lock().unwrap().push(r.clone());
            Ok(())
        }
        fn get_governance_by_proposal(
            &self,
            id: &str,
        ) -> Result<Option<GovernanceDecisionReceipt>, String> {
            let r = self.receipts.lock().unwrap();
            Ok(r.iter().find(|x| x.proposal_id == id).cloned())
        }
        fn put_allocation(
            &self,
            _: &icn_kernel_api::AllocationReceipt,
        ) -> Result<icn_kernel_api::Hash, String> {
            Ok([0; 32])
        }
        fn get_governance_by_decision(
            &self,
            h: &icn_kernel_api::Hash,
        ) -> Result<Option<GovernanceDecisionReceipt>, String> {
            let r = self.receipts.lock().unwrap();
            Ok(r.iter().find(|x| x.decision_hash == *h).cloned())
        }
        fn list_allocations_by_decision(
            &self,
            _: &icn_kernel_api::Hash,
        ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
            Ok(vec![])
        }
        fn list_institutional_effects_by_proposal(
            &self,
            id: &str,
        ) -> Result<Vec<InstitutionalEffectRecord>, String> {
            let e = self.effects.lock().unwrap();
            Ok(e.iter().filter(|r| r.proposal_id == id).cloned().collect())
        }
        // Keyed by the index (the queried id), like the store's by_proposal index, so a
        // corrupt index entry can surface a mandate whose body names another proposal.
        fn get_mandate_by_proposal(&self, id: &str) -> Result<Option<Mandate>, String> {
            if let Some(copy) = self.by_proposal_copy.lock().unwrap().clone() {
                return Ok(Some(copy));
            }
            let m = self.mandates.lock().unwrap();
            Ok(m.iter()
                .find(|x| {
                    x.decision.proposal_id == id
                        || x.decision.proposal_id == "INDEXED:".to_owned() + id
                })
                .cloned())
        }
        fn list_mandates_by_decision(
            &self,
            h: &icn_kernel_api::Hash,
        ) -> Result<Vec<Mandate>, String> {
            if self.no_enumerate {
                return Ok(vec![]);
            }
            let m = self.mandates.lock().unwrap();
            Ok(m.iter()
                .filter(|x| x.decision.decision_hash == *h)
                .cloned()
                .collect())
        }
        fn flush(&self) -> Result<(), String> {
            if self.flush_fails {
                Err("fsync failed".into())
            } else {
                Ok(())
            }
        }
    }

    const DOM: &str = "legacy.coop.example";

    fn proposal(id: &str, payload: ProposalPayload) -> Proposal {
        let mut p = Proposal::new(
            GovernanceDomainId(DOM.into()),
            icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone(),
            "t".into(),
            "d".into(),
            payload,
        );
        p.id = ProposalId(id.into());
        p
    }

    fn charter() -> ProposalPayload {
        ProposalPayload::Charter {
            charter_id: "c".into(),
            charter_yaml: "schema_version: v0".into(),
        }
    }

    pub(crate) fn budget() -> ProposalPayload {
        ProposalPayload::Budget {
            amount: 5,
            currency: "hours".into(),
            recipient: icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone(),
            purpose: "p".into(),
        }
    }

    fn receipt(p: &Proposal) -> GovernanceDecisionReceipt {
        GovernanceDecisionReceipt::new(
            p.id.0.clone(),
            DOM.into(),
            ProofOutcome::Accepted,
            VoteTally::new(3, 0, 0),
            &[],
        )
    }

    pub(crate) fn mandate(p: &Proposal, decision_hash: icn_kernel_api::Hash) -> Mandate {
        Mandate::new_pending_grants(
            DecisionProvenance {
                proposal_id: p.id.0.clone(),
                decision_hash,
            },
            hash_proposal_payload(&p.payload).unwrap(),
            None,
            None,
            10,
        )
    }

    /// The complete durable intent the actor's close writes for `p`.
    pub(crate) fn durable(b: &Backend, p: &Proposal) -> icn_kernel_api::Hash {
        let r = receipt(p);
        let h = r.decision_hash;
        b.receipts.lock().unwrap().push(r);
        b.mandates.lock().unwrap().push(mandate(p, h));
        if let Some(ier) = record_from_accepted_payload(&p.id.0, DOM, Some(h), &p.payload, 10) {
            b.effects.lock().unwrap().push(ier);
        }
        h
    }

    fn err(r: anyhow::Result<bool>) -> SealEvidenceError {
        r.unwrap_err()
            .downcast::<SealEvidenceError>()
            .expect("a SealEvidenceError")
    }

    #[test]
    fn complete_durable_intent_is_durable_with_and_without_an_effect_record() {
        let b = Backend::default();
        let c = proposal("c", charter());
        let m = proposal("m", budget());
        durable(&b, &c);
        durable(&b, &m);
        assert!(
            closure_durable_from(&b, &c).unwrap(),
            "mandate + receipt + IER"
        );
        // Budget has no institutional-effect record (AllocationReceipt covers it): mandate and
        // decision receipt are the durable intent; no IER is required.
        assert!(record_from_accepted_payload("m", DOM, None, &m.payload, 0).is_none());
        assert!(closure_durable_from(&b, &m).unwrap());
    }

    #[test]
    fn missing_artifacts_read_as_not_durable_and_an_unwired_backend_never_proves_durability() {
        // No mandate: not durable.
        let b = Backend::default();
        let c = proposal("c", charter());
        assert!(!closure_durable_from(&b, &c).unwrap());
        // Mandate and decision receipt but no IER for an effect-producing payload.
        let h = receipt(&c).decision_hash;
        b.receipts.lock().unwrap().push(receipt(&c));
        b.mandates.lock().unwrap().push(mandate(&c, h));
        assert!(!closure_durable_from(&b, &c).unwrap());
        // An IER of another kind does not count.
        let mut other = record_from_accepted_payload("c", DOM, Some(h), &c.payload, 1).unwrap();
        other.effect_kind = "freeze_member".into();
        b.effects.lock().unwrap().push(other);
        assert!(!closure_durable_from(&b, &c).unwrap());
        // A backend that keeps the trait defaults for mandates answers "no mandate": the Seal
        // is refused (not durable), never granted.
        struct Unwired;
        impl GovernanceReceiptBackend for Unwired {
            fn put_governance(&self, _: &GovernanceDecisionReceipt) -> Result<(), String> {
                Ok(())
            }
            fn get_governance_by_proposal(
                &self,
                _: &str,
            ) -> Result<Option<GovernanceDecisionReceipt>, String> {
                Ok(None)
            }
            fn put_allocation(
                &self,
                _: &icn_kernel_api::AllocationReceipt,
            ) -> Result<icn_kernel_api::Hash, String> {
                Ok([0; 32])
            }
            fn get_governance_by_decision(
                &self,
                _: &icn_kernel_api::Hash,
            ) -> Result<Option<GovernanceDecisionReceipt>, String> {
                Ok(None)
            }
            fn list_allocations_by_decision(
                &self,
                _: &icn_kernel_api::Hash,
            ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
                Ok(vec![])
            }
        }
        assert!(!closure_durable_from(&Unwired, &proposal("m", budget())).unwrap());
    }

    #[test]
    fn a_backend_that_cannot_enumerate_mandates_is_refused() {
        let b = Backend {
            no_enumerate: true,
            ..Backend::default()
        };
        let m = proposal("m", budget());
        durable(&b, &m);
        assert_eq!(
            err(closure_durable_from(&b, &m)),
            SealEvidenceError::MandatesNotEnumerable("m".into())
        );
    }

    #[test]
    fn contradictory_mandates_are_refused_but_consistent_duplicates_are_not() {
        let b = Backend::default();
        let m = proposal("m", budget());
        let h = durable(&b, &m);
        // A second, consistent mandate for the same decision (get-then-put race, or a future
        // multi-mandate decision) is not a contradiction.
        b.mandates.lock().unwrap().push(mandate(&m, h));
        assert!(closure_durable_from(&b, &m).unwrap());
        // One binding a different payload is.
        let mut wrong = mandate(&m, h);
        wrong.payload_hash = [0xAB; 32];
        b.mandates.lock().unwrap().push(wrong);
        assert_eq!(
            err(closure_durable_from(&b, &m)),
            SealEvidenceError::MandatePayloadMismatch("m".into())
        );
        // A mandate under the same decision naming another proposal.
        let b = Backend::default();
        let h = durable(&b, &m);
        let mut foreign = mandate(&proposal("z", budget()), h);
        foreign.payload_hash = hash_proposal_payload(&m.payload).unwrap();
        b.mandates.lock().unwrap().push(foreign);
        assert_eq!(
            err(closure_durable_from(&b, &m)),
            SealEvidenceError::MandateDecisionShared("m".into())
        );
    }

    #[test]
    fn a_mandate_for_another_proposal_or_payload_is_refused() {
        // The index surfaces a mandate whose body names another proposal.
        let b = Backend::default();
        let m = proposal("m", budget());
        let h = durable(&b, &m);
        b.mandates.lock().unwrap().clear();
        let mut skewed = mandate(&m, h);
        skewed.decision.proposal_id = "INDEXED:m".into();
        b.mandates.lock().unwrap().push(skewed);
        assert_eq!(
            err(closure_durable_from(&b, &m)),
            SealEvidenceError::MandateForeignProposal("m".into())
        );
        // The stored mandate binds a payload other than the proposal's.
        let b = Backend::default();
        let h = durable(&b, &m);
        b.mandates.lock().unwrap()[0].payload_hash = [1; 32];
        let _ = h;
        assert_eq!(
            err(closure_durable_from(&b, &m)),
            SealEvidenceError::MandatePayloadMismatch("m".into())
        );
    }

    #[test]
    fn a_by_proposal_copy_that_disagrees_with_the_decision_listing_is_refused() {
        // Kills E02: the primary mandate's own payload binding is checked, not only the copy
        // the decision listing returns.
        let b = Backend::default();
        let m = proposal("m", budget());
        durable(&b, &m);
        let mut copy = b.mandates.lock().unwrap()[0].clone();
        copy.payload_hash = [7; 32];
        *b.by_proposal_copy.lock().unwrap() = Some(copy);
        assert_eq!(
            err(closure_durable_from(&b, &m)),
            SealEvidenceError::MandatePayloadMismatch("m".into())
        );
    }

    #[test]
    fn decision_evidence_must_exist_and_match() {
        let m = proposal("m", budget());
        let b = Backend::default();
        durable(&b, &m);
        b.receipts.lock().unwrap().clear();
        assert_eq!(
            err(closure_durable_from(&b, &m)),
            SealEvidenceError::DecisionMissing("m".into())
        );
        for corrupt in [0u8, 1, 2] {
            let b = Backend::default();
            durable(&b, &m);
            let mut r = b.receipts.lock().unwrap();
            match corrupt {
                0 => r[0].proposal_id = "z".into(),
                1 => r[0].domain_id = "other.coop".into(),
                _ => r[0].outcome = ProofOutcome::Rejected,
            }
            drop(r);
            assert_eq!(
                err(closure_durable_from(&b, &m)),
                SealEvidenceError::DecisionMismatch("m".into()),
                "corruption {corrupt}"
            );
        }
    }

    #[test]
    fn effect_records_must_be_unique_and_bound_to_the_proposal_and_decision() {
        let c = proposal("c", charter());
        // Two distinct records of the kind.
        let b = Backend::default();
        let h = durable(&b, &c);
        let dup = record_from_accepted_payload("c", DOM, Some(h), &c.payload, 2).unwrap();
        b.effects.lock().unwrap().push(dup);
        assert_eq!(
            err(closure_durable_from(&b, &c)),
            SealEvidenceError::EffectDuplicate("c".into())
        );
        // Foreign domain, and a different decision.
        for corrupt in [0u8, 1] {
            let b = Backend::default();
            durable(&b, &c);
            let mut e = b.effects.lock().unwrap();
            match corrupt {
                0 => e[0].domain_id = "other.coop".into(),
                _ => e[0].decision_hash = Some([9; 32]),
            }
            drop(e);
            assert_eq!(
                err(closure_durable_from(&b, &c)),
                SealEvidenceError::EffectForeign("c".into()),
                "corruption {corrupt}"
            );
        }
        // A record without a decision hash (written before the receipt was available) is
        // accepted when everything else matches.
        let b = Backend::default();
        durable(&b, &c);
        b.effects.lock().unwrap()[0].decision_hash = None;
        assert!(closure_durable_from(&b, &c).unwrap());
    }

    #[test]
    fn the_bound_source_flushes_the_backend_and_reports_its_failure() {
        let ok = BoundSealEvidence::receipts(Arc::new(Backend::default()));
        assert!(ok.get().flush().is_ok());
        let failing = BoundSealEvidence::receipts(Arc::new(Backend {
            flush_fails: true,
            ..Backend::default()
        }));
        assert!(failing.get().flush().is_err());
    }

    #[test]
    fn the_cutover_basis_predicate_is_conservative() {
        let d = || {
            icn_identity::IdentityBundle::generate()
                .unwrap()
                .did()
                .clone()
        };
        for p in [
            ProposalPayload::FreezeMember {
                member: d(),
                reason: "r".into(),
                duration_seconds: None,
            },
            ProposalPayload::UnfreezeMember {
                member: d(),
                reason: "r".into(),
            },
            charter(),
        ] {
            assert!(changes_cutover_basis(&p), "{}", p.type_name());
        }
        for p in [budget(), ProposalPayload::Text { body: "b".into() }] {
            assert!(!changes_cutover_basis(&p), "{}", p.type_name());
        }
    }
}
