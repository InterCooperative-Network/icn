//! G1-A: a canonical commitment to the exact bounded resource action a proposal asks to authorize.
//!
//! [`SemanticProposalCommitmentV1`] covers **one** action kind: the treasury spend
//! (`ProposalPayload::Treasury { operation: TreasuryProposalOperation::Spend { .. } }`), the action
//! the `treasury_spend_governance_provenance` rehearsal drives to a journal entry. Every other
//! payload, and every other treasury operation, is refused with a typed error: no encoding is
//! invented for an action no owner contract has pinned.
//!
//! # Why this exists
//!
//! `GovernanceDecisionReceipt::decision_hash` covers proposal id, domain id, outcome, tally and vote
//! hash, but not the payload. The only payload digest on the accept path today is
//! `canonical_payload_hash`, BLAKE3 over `serde_json::to_string(&payload)`, which no contract
//! defines as a canonical encoding. This commitment is the byte-exact, versioned replacement a
//! future frozen `VotingProcessSnapshotV1` can bind (domain id, proposal id, **this hash**), so an
//! accepted decision names the exact action voters approved.
//!
//! # Not in scope
//!
//! Nothing here is wired into vote, close, receipt or effect handling. A commitment is not an
//! authorization, not a signature, and not evidence of who proposed anything. Subject standing,
//! recognition and electorate live in other layers.
//!
//! # Canonical encoding (v1)
//!
//! The `CANONICAL_ENCODING.md` §4.1.3 field-wise recipe, via
//! [`icn_kernel_api::canonical::CanonicalHasher`]'s byte layout. All integers are u64
//! little-endian; `STR(x)` is `u64le(len(utf8(x))) || utf8(x)`:
//!
//! ```text
//! TAG = "icn:gov:proposal-commitment:treasury-spend:v1"
//! preimage = TAG
//!         || STR(domain_id) || STR(proposal_id)
//!         || STR(treasury_did) || STR(recipient)
//!         || STR(currency) || STR(memo)
//!         || u64le(amount) || u64le(nonce)
//! commitment_hash = BLAKE3(preimage)
//! ```
//!
//! * One tag binds exactly this field sequence. The action kind is part of the tag; a new kind or
//!   field set needs a new tag/version, never a reinterpretation of this one.
//! * There are no optional fields in a `Spend`, so no absence encoding exists to get wrong.
//! * `treasury_did` and `recipient` are committed as the **exact spelling** the executor passes on
//!   (`Did::to_string()`), not as decoded principals. `Did` equality may identify several spellings
//!   of one principal; committing the spelling is the fail-safe direction (a different spelling is
//!   a different commitment).
//! * `currency` and `memo` are committed verbatim. No `CurrencyId` normalisation is applied: the
//!   executor passes the string through unchanged, and the rehearsed code uses bare symbols that
//!   `CANONICAL_ENCODING.md` §5.2 would not accept (an open owner decision: whether committed currency must be normalised first).
//! * `amount` must be strictly positive, then is committed as its u64 value.

use icn_kernel_api::canonical::DomainTag;
use thiserror::Error;

use crate::proposal::{ProposalPayload, TreasuryProposalOperation};

/// Domain tag of the v1 treasury-spend commitment. Registered in the workspace domain audit.
pub const TREASURY_SPEND_COMMITMENT_TAG: DomainTag =
    DomainTag::new(b"icn:gov:proposal-commitment:treasury-spend:v1");

/// Why a commitment could not be formed or did not verify. Every case fails closed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum CommitmentRefusal {
    /// The governance domain id is empty.
    #[error("domain id must not be empty")]
    EmptyDomainId,
    /// The proposal id is empty.
    #[error("proposal id must not be empty")]
    EmptyProposalId,
    /// This payload kind has no pinned commitment encoding.
    #[error("no commitment encoding for payload kind `{kind}`")]
    UnsupportedPayload {
        /// `ProposalPayload::type_name()` of the refused payload.
        kind: &'static str,
    },
    /// This treasury operation has no pinned commitment encoding.
    #[error("no commitment encoding for treasury operation `{operation}`")]
    UnsupportedTreasuryOperation {
        /// Stable name of the refused operation.
        operation: &'static str,
    },
    /// The spend amount is zero or negative.
    #[error("spend amount must be positive, got {0}")]
    NonPositiveAmount(i64),
    /// The currency string is empty.
    #[error("currency must not be empty")]
    EmptyCurrency,
    /// The memo is empty.
    #[error("memo must not be empty")]
    EmptyMemo,
    /// The supplied (domain, proposal, payload) is not the one this commitment was formed over.
    #[error("payload, domain or proposal id does not match the commitment")]
    Mismatch,
}

/// A canonical commitment to one treasury-spend action within one domain and proposal.
///
/// Fields are private: the only way to hold one is [`Self::commit`], so the bytes and hash always
/// agree and always come from a validated action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticProposalCommitmentV1 {
    bytes: Vec<u8>,
    hash: [u8; 32],
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    // usize is at most 64 bits on every target ICN builds for: lossless.
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn operation_name(op: &TreasuryProposalOperation) -> &'static str {
    match op {
        TreasuryProposalOperation::CreateBudget { .. } => "create_budget",
        TreasuryProposalOperation::ModifySpendingRule { .. } => "modify_spending_rule",
        TreasuryProposalOperation::Withdraw { .. } => "withdraw",
        TreasuryProposalOperation::TransferBetweenBudgets { .. } => "transfer_between_budgets",
        TreasuryProposalOperation::CancelBudget { .. } => "cancel_budget",
        TreasuryProposalOperation::ReclaimBudget { .. } => "reclaim_budget",
        TreasuryProposalOperation::Spend { .. } => "spend",
    }
}

impl SemanticProposalCommitmentV1 {
    /// Commit to `payload` as the action of proposal `proposal_id` in governance `domain_id`.
    ///
    /// # Errors
    ///
    /// A [`CommitmentRefusal`] for an empty id, an unsupported payload or treasury operation, a
    /// non-positive amount, or an empty currency or memo.
    pub fn commit(
        domain_id: &str,
        proposal_id: &str,
        payload: &ProposalPayload,
    ) -> Result<Self, CommitmentRefusal> {
        if domain_id.is_empty() {
            return Err(CommitmentRefusal::EmptyDomainId);
        }
        if proposal_id.is_empty() {
            return Err(CommitmentRefusal::EmptyProposalId);
        }
        let operation = match payload {
            ProposalPayload::Treasury { operation } => operation,
            other => {
                return Err(CommitmentRefusal::UnsupportedPayload {
                    kind: other.type_name(),
                })
            }
        };
        let (treasury_did, amount, currency, recipient, memo, nonce) = match operation {
            TreasuryProposalOperation::Spend {
                treasury_did,
                amount,
                currency,
                recipient,
                memo,
                nonce,
            } => (treasury_did, *amount, currency, recipient, memo, *nonce),
            other => {
                return Err(CommitmentRefusal::UnsupportedTreasuryOperation {
                    operation: operation_name(other),
                })
            }
        };
        if amount <= 0 {
            return Err(CommitmentRefusal::NonPositiveAmount(amount));
        }
        if currency.is_empty() {
            return Err(CommitmentRefusal::EmptyCurrency);
        }
        if memo.is_empty() {
            return Err(CommitmentRefusal::EmptyMemo);
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(TREASURY_SPEND_COMMITMENT_TAG.as_bytes());
        put_str(&mut bytes, domain_id);
        put_str(&mut bytes, proposal_id);
        put_str(&mut bytes, &treasury_did.to_string());
        put_str(&mut bytes, &recipient.to_string());
        put_str(&mut bytes, currency);
        put_str(&mut bytes, memo);
        bytes.extend_from_slice(&(amount as u64).to_le_bytes());
        bytes.extend_from_slice(&nonce.to_le_bytes());

        let hash = *blake3::hash(&bytes).as_bytes();
        Ok(Self { bytes, hash })
    }

    /// The exact preimage bytes (layout in the module docs).
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// `BLAKE3(canonical_bytes)`.
    pub fn commitment_hash(&self) -> &[u8; 32] {
        &self.hash
    }

    /// Recompute the commitment over `(domain_id, proposal_id, payload)` and require it to equal
    /// this one. Catches payload substitution, domain replay and proposal-id reuse.
    ///
    /// # Errors
    ///
    /// [`CommitmentRefusal::Mismatch`] when the recomputed commitment differs; any other refusal
    /// when the supplied triple cannot be committed to at all.
    pub fn verify(
        &self,
        domain_id: &str,
        proposal_id: &str,
        payload: &ProposalPayload,
    ) -> Result<(), CommitmentRefusal> {
        let recomputed = Self::commit(domain_id, proposal_id, payload)?;
        if recomputed == *self {
            Ok(())
        } else {
            Err(CommitmentRefusal::Mismatch)
        }
    }
}
