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
//!   IER uniqueness per (proposal, kind) is enforced by the receipt store since #2955; this
//!   module refuses duplicates independently rather than assuming it.
//! * **An unwired backend refuses.** The trait's default lookups return empty, which reads as
//!   "not durable": the Seal is refused, never granted.
//! * The P5c predicate is the **conservative compiled-in closed list** of S19 (D-P5c-1 open):
//!   until the cutover-control semantics are decided, every listed payload counts as
//!   basis-changing, including every protocol change, regardless of whether it alters the
//!   electorate. This refuses more, never less.

use std::sync::Arc;

use icn_governance::{ProofOutcome, Proposal, ProposalPayload};

use crate::grant_minting::hash_proposal_payload;
use crate::institutional_effect::record_from_accepted_payload;
use crate::receipt_backend::GovernanceReceiptBackend;

/// The facts the Seal needs that the governance store cannot establish itself. Crate-private:
/// implementations are this module's receipt-backed source and `#[cfg(test)]` fixtures only.
pub(crate) trait SealEvidence: Send + Sync {
    /// Is `p`'s durable intent present (only consulted for accepted execution-required
    /// proposals)? `Err` on any contradiction or backend failure.
    fn closure_durable(&self, p: &Proposal) -> anyhow::Result<bool>;
    /// Does accepting `p` change a condition the cutover relies on (P5c predicate)?
    fn changes_cutover_basis(&self, p: &Proposal) -> bool;
    /// Make every fact reported so far durable before the Seal commits.
    fn flush(&self) -> anyhow::Result<()>;
}

/// The Seal evidence a lifecycle owner is bound to at construction. Opaque: the only public
/// constructor reads the receipt backend.
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
    let Some(receipt) = backend.get_governance_by_decision(&decision_hash).map_err(be)? else {
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

/// The compiled-in P5c predicate: S19's closed list, read conservatively (D-P5c-1 open). Every
/// protocol change counts, because whether it alters the electorate or voting rules is not yet
/// decidable from the payload.
pub(crate) fn changes_cutover_basis(payload: &ProposalPayload) -> bool {
    matches!(
        payload,
        ProposalPayload::Membership { .. }
            | ProposalPayload::ConfigChange { .. }
            | ProposalPayload::FreezeMember { .. }
            | ProposalPayload::UnfreezeMember { .. }
            | ProposalPayload::ProtocolChange { .. }
            | ProposalPayload::ProtocolUpgrade { .. }
    )
}
