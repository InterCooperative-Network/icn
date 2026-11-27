//! Decision-commitment verification for mandate authority.
//!
//! A stored [`Mandate`] is not, by itself, evidence that its decision was
//! legitimately made. A mandate written by an older binary, or by a close that
//! failed after writing it, can outlive the decision it names. The mandate
//! gate ([`crate::mandate_gate::DefaultMandateGate`]) therefore asks a
//! [`DecisionCommitmentSource`] what the governance owner has actually
//! committed for the mandate's proposal, and authorizes only when the mandate's
//! decision hash **and** payload hash equal that commitment.
//!
//! This module is app-layer governance meaning (it reads proposals, close
//! intents and v1 decision receipts) and stays out of the kernel.
//!
//! # Verdicts
//! [`classify`] is a pure function over the evidence a source collects:
//!
//! | Evidence | Verdict |
//! |---|---|
//! | durable close intent that contradicts the stored terminal state | `Contradictory` |
//! | durable accepted close intent with a valid hash, no receipt naming another hash | `AcceptedIntent` |
//! | durable close intent for a non-accepted outcome | `Rejected` |
//! | no proposal record and no intent | `Unknown` |
//! | proposal not closed | `Incomplete` |
//! | proposal closed with a non-accepted outcome | `Rejected` |
//! | accepted proposal, exactly one decision hash among its receipts, Accepted, same domain | `FinalizedAccepted` |
//! | accepted proposal with no receipt | `Unknown` |
//! | accepted proposal with several distinct receipt hashes, or a non-accepted / foreign-domain receipt | `Contradictory` |
//!
//! Only `FinalizedAccepted` and `AcceptedIntent` can authorize, and only for a
//! mandate bound to the same decision and payload ([`DecisionCommitment::authorizes`]).
//!
//! # Why receipts alone are not enough
//! A close that wrote its receipts and mandate and then failed leaves exactly
//! one accepted receipt matching the orphan mandate. Only the proposal record
//! (or its durable close intent) distinguishes that from a completed decision,
//! so a missing proposal record is `Unknown`, never "trust the receipt".
//! A force-accepted proposal persists no v1 receipt yet, so once its intent is
//! cleared it is also `Unknown` (force-accept has no production sender today).
//!
//! # Concurrency
//! A source reads without the governance actor's lock. Every accepting verdict
//! rests on durable facts that are never retracted (a fsynced intent, a terminal
//! proposal, its receipt), and a close writes them in the order intent →
//! receipts/mandate → terminal proposal → intent removal. A read racing that
//! sequence can only observe an earlier stage, which yields `Incomplete`,
//! `AcceptedIntent` or `FinalizedAccepted` for the same decision — never
//! acceptance of a different one.

use std::sync::Arc;

use icn_governance::proof::{Hash, ProofOutcome};
use icn_governance::{GovernanceDecisionReceipt, Mandate, Proposal, ProposalId};

use crate::close_journal::{detect_conflict, is_accepted, CloseJournalEntry};
use crate::grant_minting::hash_proposal_payload;
use crate::receipt_backend::GovernanceReceiptBackend;
use crate::state_store::GovernanceStateStore;

/// The decision a proposal has committed to: the decision hash that keys its
/// receipts and mandate, and the hash of the payload it accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommittedDecision {
    /// Blake3 decision hash of the accepted decision.
    pub decision_hash: Hash,
    /// Hash of the accepted proposal payload, computed the way the mandate
    /// seam computes `Mandate::payload_hash`.
    pub payload_hash: Hash,
}

/// What the governance owner has committed for one proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionCommitment {
    /// The proposal is terminal-accepted with exactly one consistent decision receipt.
    FinalizedAccepted(CommittedDecision),
    /// A durable (fsynced) accepted close intent exists; the decision is made
    /// and recovery will complete it.
    AcceptedIntent(CommittedDecision),
    /// No decision yet (draft, deliberation or open).
    Incomplete,
    /// The decision is (or is durably being) closed with a non-accepted outcome.
    Rejected,
    /// The evidence disagrees with itself; only an authorized reconciliation
    /// can resolve it.
    Contradictory(String),
    /// The evidence needed to establish a commitment is missing.
    Unknown(String),
}

impl DecisionCommitment {
    /// True only when this commitment is an accepted decision and `mandate`
    /// is bound to exactly that decision and payload.
    pub fn authorizes(&self, mandate: &Mandate) -> bool {
        match self {
            DecisionCommitment::FinalizedAccepted(c) | DecisionCommitment::AcceptedIntent(c) => {
                c.decision_hash == mandate.decision.decision_hash
                    && c.payload_hash == mandate.payload_hash
            }
            DecisionCommitment::Incomplete
            | DecisionCommitment::Rejected
            | DecisionCommitment::Contradictory(_)
            | DecisionCommitment::Unknown(_) => false,
        }
    }

    /// Short, stable label for logs and display (no payload data).
    pub fn label(&self) -> &'static str {
        match self {
            DecisionCommitment::FinalizedAccepted(_) => "finalized_accepted",
            DecisionCommitment::AcceptedIntent(_) => "accepted_intent",
            DecisionCommitment::Incomplete => "incomplete",
            DecisionCommitment::Rejected => "rejected",
            DecisionCommitment::Contradictory(_) => "contradictory",
            DecisionCommitment::Unknown(_) => "unknown",
        }
    }
}

/// Supplies the committed decision for a proposal.
///
/// `Err` means the evidence could not be read (a backend fault); the gate turns
/// it into a fail-closed backend error, never a verdict.
pub trait DecisionCommitmentSource: Send + Sync {
    /// The commitment for `proposal_id`.
    fn decision_commitment(&self, proposal_id: &str) -> Result<DecisionCommitment, String>;
}

fn parse_hash(hex_hash: Option<&str>) -> Option<Hash> {
    let bytes = hex::decode(hex_hash?).ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

/// Classify the evidence for one proposal. Pure; see the module table.
///
/// `receipts` must be the **complete** set of v1 decision receipts stored for
/// the proposal ([`GovernanceReceiptBackend::list_governance_by_proposal`]).
pub fn classify(
    stored: Option<&Proposal>,
    intent: Option<&CloseJournalEntry>,
    receipts: &[GovernanceDecisionReceipt],
) -> DecisionCommitment {
    let mut hashes: Vec<Hash> = receipts.iter().map(|r| r.decision_hash).collect();
    hashes.sort_unstable();
    hashes.dedup();

    if let Some(entry) = intent {
        match detect_conflict(entry, stored) {
            Ok(None) => {}
            Ok(Some(conflict)) => return DecisionCommitment::Contradictory(conflict.to_string()),
            Err(e) => {
                return DecisionCommitment::Unknown(format!(
                    "close intent could not be compared with the stored proposal: {e}"
                ))
            }
        }
        if !is_accepted(&entry.proposal.state) {
            return if entry.proposal.state.is_closed() {
                DecisionCommitment::Rejected
            } else {
                DecisionCommitment::Contradictory(
                    "close intent records a non-terminal proposal state".into(),
                )
            };
        }
        let Some(decision_hash) = parse_hash(entry.governance_decision_hash.as_deref()) else {
            return DecisionCommitment::Contradictory(
                "accepted close intent carries no valid decision hash".into(),
            );
        };
        if hashes.iter().any(|h| *h != decision_hash) {
            return DecisionCommitment::Contradictory(
                "a stored decision receipt names a different decision than the close intent".into(),
            );
        }
        return match hash_proposal_payload(&entry.proposal.payload) {
            Ok(payload_hash) => DecisionCommitment::AcceptedIntent(CommittedDecision {
                decision_hash,
                payload_hash,
            }),
            Err(e) => DecisionCommitment::Unknown(format!("payload hash unavailable: {e}")),
        };
    }

    let Some(proposal) = stored else {
        return DecisionCommitment::Unknown("no proposal record".into());
    };
    if !proposal.state.is_closed() {
        return DecisionCommitment::Incomplete;
    }
    if !is_accepted(&proposal.state) {
        return DecisionCommitment::Rejected;
    }
    let receipt = match (hashes.len(), receipts.first()) {
        (1, Some(receipt)) => receipt,
        (0, _) => {
            return DecisionCommitment::Unknown("accepted proposal has no decision receipt".into())
        }
        _ => {
            return DecisionCommitment::Contradictory(format!(
                "accepted proposal has {} distinct decision receipts",
                hashes.len()
            ))
        }
    };
    if receipt.outcome != ProofOutcome::Accepted {
        return DecisionCommitment::Contradictory(
            "accepted proposal's decision receipt records another outcome".into(),
        );
    }
    if receipt.domain_id != proposal.domain_id.0 || receipt.proposal_id != proposal.id.0 {
        return DecisionCommitment::Contradictory(
            "decision receipt names another proposal or domain".into(),
        );
    }
    match hash_proposal_payload(&proposal.payload) {
        Ok(payload_hash) => DecisionCommitment::FinalizedAccepted(CommittedDecision {
            decision_hash: receipt.decision_hash,
            payload_hash,
        }),
        Err(e) => DecisionCommitment::Unknown(format!("payload hash unavailable: {e}")),
    }
}

/// Commitment source over durable proposal records: the governance state store
/// (proposals and close intents) plus the receipt store.
///
/// A proposal the state store does not hold is `Unknown`.
pub struct StoredDecisionCommitments {
    state: Arc<dyn GovernanceStateStore>,
    receipts: Arc<dyn GovernanceReceiptBackend>,
}

impl StoredDecisionCommitments {
    /// A source reading `state` and `receipts`.
    pub fn new(
        state: Arc<dyn GovernanceStateStore>,
        receipts: Arc<dyn GovernanceReceiptBackend>,
    ) -> Self {
        Self { state, receipts }
    }
}

/// Read the evidence for `proposal_id` and classify it. `held` is a proposal
/// record the caller already holds (the standalone manager's in-memory copy);
/// when present it is used instead of the state store's proposal row.
pub(crate) fn commitment_from_stores(
    proposal_id: &str,
    held: Option<Proposal>,
    state: Option<&dyn GovernanceStateStore>,
    receipts: &dyn GovernanceReceiptBackend,
) -> Result<DecisionCommitment, String> {
    let pid = ProposalId::new(proposal_id);
    let (stored, intent) = match state {
        Some(state) => {
            let intent = state
                .get_close_intent(&pid)
                .map_err(|e| format!("close intent for '{proposal_id}' unreadable: {e}"))?;
            let stored = match held {
                Some(p) => Some(p),
                None => state
                    .get_proposal(&pid)
                    .map_err(|e| format!("proposal '{proposal_id}' unreadable: {e}"))?,
            };
            (stored, intent)
        }
        None => (held, None),
    };
    let receipts = receipts.list_governance_by_proposal(proposal_id)?;
    Ok(classify(stored.as_ref(), intent.as_ref(), &receipts))
}

impl DecisionCommitmentSource for StoredDecisionCommitments {
    fn decision_commitment(&self, proposal_id: &str) -> Result<DecisionCommitment, String> {
        commitment_from_stores(
            proposal_id,
            None,
            Some(self.state.as_ref()),
            self.receipts.as_ref(),
        )
    }
}

/// Commitment source for a [`crate::manager::GovernanceManager`]: the
/// proposal the manager holds in memory (standalone mode keeps proposals
/// there), otherwise its attached state store's proposal row, plus that
/// store's close intent and the manager's receipt backend.
///
/// With neither an in-memory proposal nor a stored one the verdict is
/// `Unknown`, so standalone authority does not survive a restart that drops
/// the in-memory proposal: receipts alone cannot tell a completed decision
/// from a close that failed after writing them.
pub(crate) struct ManagerDecisionCommitments<'m> {
    pub(crate) manager: &'m crate::manager::GovernanceManager,
    pub(crate) receipts: Arc<dyn GovernanceReceiptBackend>,
}

impl DecisionCommitmentSource for ManagerDecisionCommitments<'_> {
    fn decision_commitment(&self, proposal_id: &str) -> Result<DecisionCommitment, String> {
        // An actor-backed manager answers from the actor's authoritative
        // state; it never falls back to its own (empty) standalone view.
        if let Some(source) = self.manager.authoritative_commitments() {
            return source.decision_commitment(proposal_id);
        }
        if self.manager.is_actor_backed() {
            return Err(
                "actor-backed governance manager has no authoritative commitment source wired"
                    .to_string(),
            );
        }
        let held = self.manager.held_proposal(proposal_id)?;
        let state = self.manager.domain_state_store();
        commitment_from_stores(proposal_id, held, state.as_deref(), self.receipts.as_ref())
    }
}

/// The governance actor's commitment source: the actor's own durable state
/// store (proposals and close intents) and the receipt backend installed
/// into the actor. This is the authoritative evidence for every deployment
/// that runs the governance actor; it exposes verdicts only, never the stores.
///
/// Created at actor spawn, before any receipt backend exists. Until
/// `GovernanceHandle::install_receipt_store` runs, every lookup is an error
/// (fail closed), never a verdict.
#[derive(Clone)]
pub struct ActorDecisionCommitments {
    state: Arc<dyn GovernanceStateStore>,
    receipts: Arc<std::sync::RwLock<Option<Arc<dyn GovernanceReceiptBackend>>>>,
}

impl ActorDecisionCommitments {
    pub(crate) fn new(state: Arc<dyn GovernanceStateStore>) -> Self {
        Self {
            state,
            receipts: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Bind the receipt backend the actor writes its decisions to.
    pub(crate) fn install_receipts(&self, receipts: Arc<dyn GovernanceReceiptBackend>) {
        match self.receipts.write() {
            Ok(mut slot) => *slot = Some(receipts),
            Err(poisoned) => *poisoned.into_inner() = Some(receipts),
        }
    }
}

impl DecisionCommitmentSource for ActorDecisionCommitments {
    fn decision_commitment(&self, proposal_id: &str) -> Result<DecisionCommitment, String> {
        let receipts = self
            .receipts
            .read()
            .map_err(|e| format!("commitment receipt slot poisoned: {e}"))?
            .clone()
            .ok_or_else(|| {
                "governance actor has no receipt store installed: decision commitment unavailable"
                    .to_string()
            })?;
        commitment_from_stores(
            proposal_id,
            None,
            Some(self.state.as_ref()),
            receipts.as_ref(),
        )
    }
}

/// An owned [`ManagerDecisionCommitments`] over a shared manager, for a
/// long-lived gate. The manager's receipt backend is read on each call; with
/// none wired, every lookup is an error (fail closed).
pub struct SharedManagerCommitments {
    manager: Arc<crate::manager::GovernanceManager>,
}

impl SharedManagerCommitments {
    /// A source over `manager`'s held proposals, state store and receipt store.
    pub fn new(manager: Arc<crate::manager::GovernanceManager>) -> Self {
        Self { manager }
    }
}

impl DecisionCommitmentSource for SharedManagerCommitments {
    fn decision_commitment(&self, proposal_id: &str) -> Result<DecisionCommitment, String> {
        let receipts = self.manager.receipt_backend().ok_or_else(|| {
            "no receipt backend wired: decision commitment unavailable".to_string()
        })?;
        ManagerDecisionCommitments {
            manager: &self.manager,
            receipts,
        }
        .decision_commitment(proposal_id)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use icn_governance::{
        DecisionProvenance, GovernanceDomainId, ProposalPayload, ProposalState, VoteTally,
    };
    use icn_identity::Did;

    const H: Hash = [7u8; 32];

    fn proposal(state: ProposalState) -> Proposal {
        let mut p = Proposal::new(
            GovernanceDomainId::new("gov-alpha"),
            Did::from_anchor_id(&[1u8; 32]),
            "t".into(),
            "d".into(),
            ProposalPayload::Text { body: "b".into() },
        );
        p.id = ProposalId::new("prop-1");
        p.state = state;
        p
    }
    fn accepted() -> Proposal {
        proposal(ProposalState::Accepted { closed_at: 50 })
    }
    fn receipt(hash: Hash, outcome: ProofOutcome) -> GovernanceDecisionReceipt {
        GovernanceDecisionReceipt {
            proposal_id: "prop-1".into(),
            domain_id: "gov-alpha".into(),
            outcome,
            vote_tally: VoteTally::new(1, 0, 0),
            vote_hash: [0u8; 32],
            decision_hash: hash,
        }
    }
    fn intent(p: Proposal, hash: Option<Hash>) -> CloseJournalEntry {
        CloseJournalEntry {
            proposal: p,
            proof_bytes: None,
            receipts: Default::default(),
            decided_at: 50,
            governance_decision_hash: hash.map(hex::encode),
            execution_closure: true,
        }
    }
    fn committed() -> CommittedDecision {
        CommittedDecision {
            decision_hash: H,
            payload_hash: hash_proposal_payload(&accepted().payload).unwrap(),
        }
    }
    fn mandate(hash: Hash, payload: Hash) -> Mandate {
        Mandate::new(
            DecisionProvenance {
                proposal_id: "prop-1".into(),
                decision_hash: hash,
            },
            payload,
            vec![icn_governance::AuthorityGrantId::new()],
            None,
            None,
            0,
        )
        .unwrap()
    }
    fn is_contradictory(c: &DecisionCommitment) -> bool {
        matches!(c, DecisionCommitment::Contradictory(_))
    }
    fn is_unknown(c: &DecisionCommitment) -> bool {
        matches!(c, DecisionCommitment::Unknown(_))
    }

    #[test]
    fn finalized_accepted_with_one_consistent_receipt() {
        let c = classify(
            Some(&accepted()),
            None,
            &[receipt(H, ProofOutcome::Accepted)],
        );
        assert_eq!(c, DecisionCommitment::FinalizedAccepted(committed()));
        // A duplicate of the same receipt is still one decision.
        let c = classify(
            Some(&accepted()),
            None,
            &[
                receipt(H, ProofOutcome::Accepted),
                receipt(H, ProofOutcome::Accepted),
            ],
        );
        assert_eq!(c, DecisionCommitment::FinalizedAccepted(committed()));
    }

    #[test]
    fn accepted_without_receipt_or_record_is_unknown() {
        assert!(is_unknown(&classify(Some(&accepted()), None, &[])));
        assert!(is_unknown(&classify(
            None,
            None,
            &[receipt(H, ProofOutcome::Accepted)]
        )));
    }

    #[test]
    fn undecided_and_rejected_states() {
        for open in [
            ProposalState::Draft,
            ProposalState::Open {
                opened_at: 1,
                closes_at: 2,
            },
        ] {
            let c = classify(
                Some(&proposal(open)),
                None,
                &[receipt(H, ProofOutcome::Accepted)],
            );
            assert_eq!(c, DecisionCommitment::Incomplete);
        }
        for closed in [
            ProposalState::Rejected { closed_at: 5 },
            ProposalState::NoQuorum { closed_at: 5 },
            ProposalState::Cancelled { cancelled_at: 5 },
            ProposalState::Vetoed {
                vetoed_at: 5,
                reason: "r".into(),
            },
        ] {
            let c = classify(
                Some(&proposal(closed)),
                None,
                &[receipt(H, ProofOutcome::Accepted)],
            );
            assert_eq!(c, DecisionCommitment::Rejected);
        }
    }

    #[test]
    fn contradictory_receipt_sets() {
        let two = [
            receipt(H, ProofOutcome::Accepted),
            receipt([8u8; 32], ProofOutcome::Accepted),
        ];
        assert!(is_contradictory(&classify(Some(&accepted()), None, &two)));
        assert!(is_contradictory(&classify(
            Some(&accepted()),
            None,
            &[receipt(H, ProofOutcome::Rejected)]
        )));
        let mut foreign = receipt(H, ProofOutcome::Accepted);
        foreign.domain_id = "gov-beta".into();
        assert!(is_contradictory(&classify(
            Some(&accepted()),
            None,
            &[foreign]
        )));
        let mut other = receipt(H, ProofOutcome::Accepted);
        other.proposal_id = "prop-2".into();
        assert!(is_contradictory(&classify(
            Some(&accepted()),
            None,
            &[other]
        )));
    }

    #[test]
    fn durable_intent_verdicts() {
        let open = proposal(ProposalState::Open {
            opened_at: 1,
            closes_at: 2,
        });
        // Accepted intent over an Open row: committed, before the terminal save.
        let c = classify(Some(&open), Some(&intent(accepted(), Some(H))), &[]);
        assert_eq!(c, DecisionCommitment::AcceptedIntent(committed()));
        // Its own receipt already written: still the same commitment.
        let c = classify(
            Some(&open),
            Some(&intent(accepted(), Some(H))),
            &[receipt(H, ProofOutcome::Accepted)],
        );
        assert_eq!(c, DecisionCommitment::AcceptedIntent(committed()));
        // A receipt naming another decision contradicts the intent.
        assert!(is_contradictory(&classify(
            Some(&open),
            Some(&intent(accepted(), Some(H))),
            &[receipt([8u8; 32], ProofOutcome::Accepted)],
        )));
        // Accepted intent without a valid hash.
        assert!(is_contradictory(&classify(
            Some(&open),
            Some(&intent(accepted(), None)),
            &[]
        )));
        // A non-accepted terminal intent.
        let rejected = proposal(ProposalState::Rejected { closed_at: 5 });
        assert_eq!(
            classify(Some(&open), Some(&intent(rejected.clone(), None)), &[]),
            DecisionCommitment::Rejected
        );
        // An intent contradicting the stored terminal state.
        assert!(is_contradictory(&classify(
            Some(&rejected),
            Some(&intent(accepted(), Some(H))),
            &[]
        )));
        // An intent recording a non-terminal state.
        assert!(is_contradictory(&classify(
            Some(&open),
            Some(&intent(open.clone(), None)),
            &[]
        )));
    }

    #[test]
    fn authorizes_only_the_committed_decision_and_payload() {
        let payload = committed().payload_hash;
        for c in [
            DecisionCommitment::FinalizedAccepted(committed()),
            DecisionCommitment::AcceptedIntent(committed()),
        ] {
            assert!(c.authorizes(&mandate(H, payload)));
            assert!(
                !c.authorizes(&mandate([8u8; 32], payload)),
                "other decision"
            );
            assert!(!c.authorizes(&mandate(H, [9u8; 32])), "other payload");
        }
        for c in [
            DecisionCommitment::Incomplete,
            DecisionCommitment::Rejected,
            DecisionCommitment::Contradictory("x".into()),
            DecisionCommitment::Unknown("x".into()),
        ] {
            assert!(!c.authorizes(&mandate(H, payload)), "{}", c.label());
        }
    }
}
