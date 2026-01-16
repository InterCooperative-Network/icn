//! Core ledger data types

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Content hash for Merkle-DAG addressing
/// SHA-256 hash of the canonical serialization of an entry
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Create from byte array
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ContentHash(bytes)
    }

    /// Get as byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Convert to hex string for display
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Cryptographic signature over journal entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(pub Vec<u8>);

/// Journal entry in the ledger's Merkle-DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Content hash (computed from canonical serialization)
    #[serde(skip)]
    pub id: Option<ContentHash>,

    /// Local timestamp (not consensus, for ordering)
    pub timestamp: u64,

    /// Author who created this entry
    pub author: Did,

    /// Optional contract that authorized this entry
    pub contract_ref: Option<ContentHash>,

    /// Account deltas (debits and credits)
    pub accounts: Vec<AccountDelta>,

    /// Previous entries (Merkle-DAG links)
    pub parents: Vec<ContentHash>,

    /// Signature by author
    pub signature: Option<Signature>,
}

/// Delta for a single account in a journal entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountDelta {
    /// Account identifier (DID of person, org, or resource)
    pub account_id: Did,

    /// Currency symbol (e.g., "hours", "USD", "kwh")
    pub currency: String,

    /// Debit amount (positive value, or None if credit-only)
    pub debit: Option<i64>,

    /// Credit amount (positive value, or None if debit-only)
    pub credit: Option<i64>,
}

impl AccountDelta {
    /// Create a debit-only delta
    pub fn debit(account_id: Did, currency: String, amount: i64) -> Self {
        AccountDelta {
            account_id,
            currency,
            debit: Some(amount),
            credit: None,
        }
    }

    /// Create a credit-only delta
    pub fn credit(account_id: Did, currency: String, amount: i64) -> Self {
        AccountDelta {
            account_id,
            currency,
            debit: None,
            credit: Some(amount),
        }
    }

    /// Get the net change for this delta (debit - credit)
    ///
    /// Uses checked arithmetic to prevent overflow. Returns error if
    /// the calculation would overflow.
    pub fn net_change(&self) -> Result<i64, crate::LedgerError> {
        let debit = self.debit.unwrap_or(0);
        let credit = self.credit.unwrap_or(0);
        debit.checked_sub(credit).ok_or_else(|| {
            crate::LedgerError::ArithmeticOverflow(format!(
                "arithmetic overflow in net_change: {debit} - {credit} for account {}",
                self.account_id
            ))
        })
    }

    /// Get the net change, saturating on overflow instead of erroring.
    ///
    /// Use this only when overflow is acceptable (e.g., display purposes).
    /// For ledger mutations, use `net_change()` which returns Result.
    pub fn net_change_saturating(&self) -> i64 {
        let debit = self.debit.unwrap_or(0);
        let credit = self.credit.unwrap_or(0);
        debit.saturating_sub(credit)
    }
}

/// Currency definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Currency {
    /// Currency symbol (e.g., "hours", "USD", "kwh")
    pub symbol: String,

    /// Decimal precision (e.g., 2 for cents)
    pub decimals: u8,

    /// Optional issuer (None = mutual credit)
    pub issuer: Option<Did>,
}

impl Currency {
    /// Create a mutual credit currency (no issuer)
    pub fn mutual_credit(symbol: String, decimals: u8) -> Self {
        Currency {
            symbol,
            decimals,
            issuer: None,
        }
    }

    /// Create an asset-backed currency with issuer
    pub fn asset_backed(symbol: String, decimals: u8, issuer: Did) -> Self {
        Currency {
            symbol,
            decimals,
            issuer: Some(issuer),
        }
    }
}

/// Account balance for a specific currency
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Balance {
    /// Currency symbol
    pub currency: String,

    /// Current balance (can be negative for mutual credit)
    pub amount: i64,
}

/// Cached balances for an account across all currencies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountBalances {
    /// Account DID
    pub account_id: Did,

    /// Map of currency -> balance
    pub balances: HashMap<String, i64>,
}

impl AccountBalances {
    /// Create new empty balances for an account
    pub fn new(account_id: Did) -> Self {
        AccountBalances {
            account_id,
            balances: HashMap::new(),
        }
    }

    /// Get balance for a specific currency
    pub fn get(&self, currency: &str) -> i64 {
        *self.balances.get(currency).unwrap_or(&0)
    }

    /// Apply a delta to the balances
    ///
    /// Uses checked arithmetic to prevent overflow. Returns error if
    /// the calculation would overflow.
    pub fn apply_delta(&mut self, delta: &AccountDelta) -> Result<(), crate::LedgerError> {
        if delta.account_id != self.account_id {
            return Ok(()); // Delta is for a different account
        }

        let current = self.get(&delta.currency);
        let change = delta.net_change()?;
        let new_balance = current.checked_add(change).ok_or_else(|| {
            crate::LedgerError::ArithmeticOverflow(format!(
                "overflow in apply_delta: {} + {} for account {} currency {}",
                current, change, self.account_id, delta.currency
            ))
        })?;
        self.balances.insert(delta.currency.clone(), new_balance);
        Ok(())
    }
}

/// Credit limit for a participant in a contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditLimit {
    /// Participant DID
    pub participant: Did,

    /// Currency symbol
    pub currency: String,

    /// Maximum negative balance allowed (how much they can owe)
    pub max_negative_balance: i64,

    /// Set by contract or mutual agreement
    pub set_by: Did,
}

/// Quarantine reason for conflict resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuarantineReason {
    /// Invariant violation (e.g., overdraft, unbalanced entry)
    InvariantViolation(String),

    /// Conflicting timestamp in Merkle-DAG
    ConflictingTimestamp,

    /// Invalid signature
    InvalidSignature,

    /// Exceeds credit limit
    ExceedsCreditLimit,

    /// Fork conflict - lost fork resolution (Phase 18 Week 5)
    ForkConflict(String),

    /// Charter rule violation - entry violates cooperative charter policies
    CharterViolation,
}

/// Quarantined entry awaiting resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantinedEntry {
    /// The problematic entry
    pub entry: JournalEntry,

    /// Why it was quarantined
    pub reason: QuarantineReason,

    /// Entries it conflicts with
    pub conflicts_with: Vec<ContentHash>,

    /// Optional resolution (human or contract decision)
    pub resolution: Option<Resolution>,
}

/// Resolution for a quarantined entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Resolution {
    /// Accept the entry despite issues
    Accept,

    /// Reject and discard
    Reject,

    /// Merge with modifications
    Merge(JournalEntry),
}

/// Dispute status for a journal entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DisputeStatus {
    /// Normal entry, no disputes
    Normal,

    /// Entry is being contested
    Contested {
        /// Who filed the dispute
        filed_by: Did,

        /// Reason for the dispute
        reason: String,

        /// When the dispute was filed (Unix timestamp)
        filed_at: u64,
    },

    /// Dispute has been resolved
    Resolved {
        /// Who mediated the resolution
        mediator: Did,

        /// Outcome of the dispute
        outcome: DisputeOutcome,

        /// When the dispute was resolved (Unix timestamp)
        resolved_at: u64,
    },

    /// Dispute has been escalated to governance for community vote
    Escalated {
        /// Governance proposal ID tracking the escalation
        proposal_id: String,

        /// Reason for escalation
        escalation_reason: String,

        /// When escalated (Unix timestamp)
        escalated_at: u64,
    },
}

/// Outcome of a dispute resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DisputeOutcome {
    /// Entry is valid, dispute was invalid
    Upheld,

    /// Entry is invalid, should be reversed
    Reversed,

    /// Partial settlement agreed upon
    Settlement {
        /// Description of the settlement
        terms: String,

        /// Optional replacement entry
        replacement_entry: Option<ContentHash>,
    },

    /// Write-off (debt forgiven or written off)
    WriteOff {
        /// Reason for write-off
        reason: String,
    },
}

/// Dispute record (stored separately from journal entries)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispute {
    /// Hash of the disputed entry
    pub entry_hash: ContentHash,

    /// Who filed the dispute
    pub filed_by: Did,

    /// Reason for the dispute
    pub reason: String,

    /// When the dispute was filed
    pub filed_at: u64,

    /// Current status
    pub status: DisputeStatus,

    /// Optional evidence or documentation
    pub evidence: Vec<String>,

    /// Optional mediator assigned
    pub mediator: Option<Did>,
}

// ============================================================================
// Witness Signature Types (BFT Hardening - Issue #676)
// ============================================================================

/// Policy for requiring witness signatures on ledger entries
///
/// Witness signatures provide Byzantine fault tolerance by requiring
/// multiple parties to co-sign transactions, preventing unilateral
/// manipulation by ledger authorizers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WitnessPolicy {
    /// No witnesses required (current behavior, suitable for low-value transactions)
    None,

    /// Require counterparty signature (the other party to the transaction)
    Counterparty,

    /// Require N-of-M designated witnesses to co-sign
    Quorum {
        /// Minimum signatures required
        required: u32,
        /// List of authorized witnesses
        witnesses: Vec<Did>,
    },

    /// Require all parties involved in the transaction to sign
    AllParties,
}

impl Default for WitnessPolicy {
    fn default() -> Self {
        WitnessPolicy::None
    }
}

/// Configuration for witness requirements on a ledger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessConfig {
    /// Default policy for all entries
    pub default_policy: WitnessPolicy,

    /// Value threshold above which witnesses are required
    /// If None, policy applies to all entries regardless of value
    pub threshold: Option<u64>,

    /// Grace period (in seconds) to collect witness signatures
    /// After this time, unsigned entries may be rejected
    pub collection_timeout_secs: u64,
}

impl Default for WitnessConfig {
    fn default() -> Self {
        WitnessConfig {
            default_policy: WitnessPolicy::None,
            threshold: None,
            collection_timeout_secs: 300, // 5 minutes default
        }
    }
}

impl WitnessConfig {
    /// Create config requiring counterparty signature above threshold
    pub fn counterparty_above(threshold: u64) -> Self {
        WitnessConfig {
            default_policy: WitnessPolicy::Counterparty,
            threshold: Some(threshold),
            collection_timeout_secs: 300,
        }
    }

    /// Create config requiring quorum for all entries
    pub fn quorum(required: u32, witnesses: Vec<Did>) -> Self {
        WitnessConfig {
            default_policy: WitnessPolicy::Quorum { required, witnesses },
            threshold: None,
            collection_timeout_secs: 300,
        }
    }

    /// Determine the effective policy for a given entry value
    pub fn effective_policy(&self, entry_value: u64) -> &WitnessPolicy {
        match self.threshold {
            Some(thresh) if entry_value < thresh => &WitnessPolicy::None,
            _ => &self.default_policy,
        }
    }
}

/// A witness signature on a ledger entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessSignature {
    /// DID of the witness who signed
    pub witness: Did,

    /// Ed25519 signature over the entry hash
    pub signature: Vec<u8>,

    /// Timestamp when signature was created
    pub signed_at: u64,
}

impl WitnessSignature {
    /// Create a new witness signature
    pub fn new(witness: Did, signature: Vec<u8>, signed_at: u64) -> Self {
        WitnessSignature {
            witness,
            signature,
            signed_at,
        }
    }
}

/// A journal entry with attached witness signatures
///
/// This wraps a JournalEntry with the required witness signatures
/// based on the ledger's WitnessPolicy. The entry hash is signed
/// by witnesses, not the full entry content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessedEntry {
    /// The underlying journal entry
    pub entry: JournalEntry,

    /// Witness signatures collected for this entry
    pub witness_signatures: Vec<WitnessSignature>,

    /// The policy that was applied when collecting signatures
    pub policy_applied: WitnessPolicy,
}

impl WitnessedEntry {
    /// Create a new witnessed entry with no signatures yet
    pub fn new(entry: JournalEntry, policy: WitnessPolicy) -> Self {
        WitnessedEntry {
            entry,
            witness_signatures: Vec::new(),
            policy_applied: policy,
        }
    }

    /// Add a witness signature
    pub fn add_signature(&mut self, sig: WitnessSignature) {
        self.witness_signatures.push(sig);
    }

    /// Check if the entry has sufficient signatures for the applied policy
    pub fn has_sufficient_signatures(&self) -> bool {
        match &self.policy_applied {
            WitnessPolicy::None => true,
            WitnessPolicy::Counterparty => !self.witness_signatures.is_empty(),
            WitnessPolicy::Quorum { required, .. } => {
                self.witness_signatures.len() >= *required as usize
            }
            WitnessPolicy::AllParties => {
                // Need to check against entry participants - caller must verify
                // For now, require at least one signature per non-author account
                let unique_accounts: std::collections::HashSet<_> = self
                    .entry
                    .accounts
                    .iter()
                    .map(|d| &d.account_id)
                    .filter(|id| **id != self.entry.author)
                    .collect();
                let signed_witnesses: std::collections::HashSet<_> =
                    self.witness_signatures.iter().map(|s| &s.witness).collect();
                unique_accounts.iter().all(|a| signed_witnesses.contains(a))
            }
        }
    }

    /// Get the entry hash that witnesses should sign
    pub fn entry_hash(&self) -> Option<&ContentHash> {
        self.entry.id.as_ref()
    }

    /// Count unique witnesses who have signed
    pub fn unique_witness_count(&self) -> usize {
        let witnesses: std::collections::HashSet<_> =
            self.witness_signatures.iter().map(|s| &s.witness).collect();
        witnesses.len()
    }
}
