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
//! `canonical_payload_hash`, BLAKE3 over `serde_json::to_string` of the payload's
//! `serde_json::Value`, which no contract defines as a canonical encoding. This commitment is the byte-exact, versioned replacement a
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
//!   (`Did::to_string()`), not as decoded principals. `Did` equality (I7) identifies several
//!   multibase spellings of one principal. For `treasury_did` the spelling is itself
//!   effect-relevant: the ledger keys the treasury nonce by the treasury string, so two spellings
//!   advance different nonce slots, and a principal-level commitment would let two different
//!   effects share one hash. For `recipient` the ledger parses to a `Did` and credits by
//!   principal, so an alias spelling is the same economic effect with a different commitment: a
//!   false rejection, never a false acceptance.
//! * `currency` and `memo` are committed verbatim. No `CurrencyId` normalisation is applied: the
//!   ledger keys balances by the exact currency string with no case folding, so `HOURS` and
//!   `hours` are different balances and normalising would merge distinct effects.
//! * `amount` must be strictly positive, then is committed as its u64 value.
//!
//! # Status: provisional, not frozen
//!
//! The bytes above are unambiguous for every input that can be committed. They are **not yet to
//! be treated as a frozen durable format**, because three items are open: two owner decisions
//! where the owner documents contradict the code, and one ledger alias hazard:
//!
//! 1. **DID spelling.** `CANONICAL_ENCODING.md` §5.1 requires `did:icn:<base58btc>`, but `Did`
//!    accepts any multibase spelling (e.g. `did:icn:f<hex>`) and N2 (I7) treats them as one
//!    principal. Choose: accept every spelling `Did` accepts (this v1), or refuse any
//!    non-Base58btc spelling at commit time. Do not convert `treasury_did` to the decoded
//!    principal while the treasury nonce is keyed by the spelling (item 3).
//! 2. **Currency.** §5.2 describes a normalised `<scope>:<SYMBOL>` `CurrencyId`; no such type
//!    exists in code and the rehearsed action uses bare symbols such as `HOURS`. A later mandate
//!    to normalise changes the bytes and so needs a new tag, or a refusal here.
//! 3. **Treasury nonce keying.** The ledger keys `ledger:treasury_nonce:` by the raw treasury
//!    string; that keyspace is not among the N2 alias-scan keyspaces (`n2a_keyspaces()`), and the
//!    ledger always debits its configured treasury rather than the payload's `treasury_did`. Two
//!    spellings of one treasury therefore get independent nonce slots against one account. This
//!    is an unregistered alias hazard in the ledger, not settled design. If owners key the nonce
//!    by principal, the spelling rationale for `treasury_did` lapses; committing the spelling then
//!    stays safe, only stricter.
//!
//! §4.1.3 itself imposes no identifier normalisation on field strings; whether §5.1/§5.2 apply to
//! them is unspecified.

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
