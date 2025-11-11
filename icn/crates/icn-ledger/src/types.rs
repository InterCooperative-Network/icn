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
    pub fn net_change(&self) -> i64 {
        self.debit.unwrap_or(0) - self.credit.unwrap_or(0)
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
    pub fn apply_delta(&mut self, delta: &AccountDelta) {
        if delta.account_id != self.account_id {
            return; // Delta is for a different account
        }

        let current = self.get(&delta.currency);
        let new_balance = current + delta.net_change();
        self.balances.insert(delta.currency.clone(), new_balance);
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
