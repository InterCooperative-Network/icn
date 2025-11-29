//! Clearing Agreement Types (Phase F3)
//!
//! Types for bilateral clearing agreements and cross-cooperative transfers.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::current_timestamp;

/// A bilateral clearing agreement between two cooperatives
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BilateralClearingAgreement {
    /// Unique ID for the agreement
    pub agreement_id: String,

    /// First cooperative's ID
    pub coop_a: String,

    /// First cooperative's DID
    pub coop_a_did: Did,

    /// Second cooperative's ID
    pub coop_b: String,

    /// Second cooperative's DID
    pub coop_b_did: Did,

    /// Exchange rates: "from_currency:to_currency" -> rate
    /// Key format is "from:to" (e.g., "hours:USD")
    pub exchange_rates: HashMap<String, f64>,

    /// How often to settle
    pub settlement_interval: SettlementInterval,

    /// Maximum imbalance allowed before requiring settlement
    pub max_imbalance: i64,

    /// When the agreement was created
    pub created_at: u64,

    /// Signatures from both cooperatives: (DID, signature bytes)
    pub signatures: Vec<(Did, Vec<u8>)>,
}

impl BilateralClearingAgreement {
    /// Create a new clearing agreement (unsigned)
    pub fn new(
        agreement_id: String,
        coop_a: String,
        coop_a_did: Did,
        coop_b: String,
        coop_b_did: Did,
    ) -> Self {
        Self {
            agreement_id,
            coop_a,
            coop_a_did,
            coop_b,
            coop_b_did,
            exchange_rates: HashMap::new(),
            settlement_interval: SettlementInterval::Weekly,
            max_imbalance: 10000,
            created_at: current_timestamp(),
            signatures: Vec::new(),
        }
    }

    /// Set an exchange rate
    pub fn with_rate(mut self, from: &str, to: &str, rate: f64) -> Self {
        let key = format!("{from}:{to}");
        self.exchange_rates.insert(key, rate);
        self
    }

    /// Set the settlement interval
    pub fn with_interval(mut self, interval: SettlementInterval) -> Self {
        self.settlement_interval = interval;
        self
    }

    /// Set the maximum imbalance
    pub fn with_max_imbalance(mut self, max: i64) -> Self {
        self.max_imbalance = max;
        self
    }

    /// Get exchange rate for a currency pair
    pub fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        let key = format!("{from}:{to}");
        self.exchange_rates.get(&key).copied()
    }

    /// Check if both parties have signed
    pub fn is_fully_signed(&self) -> bool {
        self.signatures.len() >= 2
    }

    /// Check if a specific cooperative has signed
    pub fn has_signed(&self, coop_did: &Did) -> bool {
        self.signatures.iter().any(|(did, _)| did == coop_did)
    }
}

/// How often to settle clearing positions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SettlementInterval {
    Daily,
    #[default]
    Weekly,
    Monthly,
    Manual,
}

impl SettlementInterval {
    /// Get the interval in seconds
    pub fn seconds(&self) -> Option<u64> {
        match self {
            SettlementInterval::Daily => Some(24 * 60 * 60),
            SettlementInterval::Weekly => Some(7 * 24 * 60 * 60),
            SettlementInterval::Monthly => Some(30 * 24 * 60 * 60),
            SettlementInterval::Manual => None,
        }
    }
}

/// Current clearing position between two cooperatives
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClearingPosition {
    /// The agreement this position is for
    pub agreement_id: String,

    /// Amount coop_a owes to coop_b
    pub coop_a_owes_b: i64,

    /// Amount coop_b owes to coop_a
    pub coop_b_owes_a: i64,

    /// When the last settlement occurred
    pub last_settlement: u64,

    /// Pending transfers not yet settled
    pub pending_transfers: Vec<CrossCoopTransfer>,
}

impl ClearingPosition {
    /// Create a new clearing position
    pub fn new(agreement_id: String) -> Self {
        Self {
            agreement_id,
            coop_a_owes_b: 0,
            coop_b_owes_a: 0,
            last_settlement: current_timestamp(),
            pending_transfers: Vec::new(),
        }
    }

    /// Get the net position (positive = coop_a owes coop_b)
    pub fn net_position(&self) -> i64 {
        self.coop_a_owes_b - self.coop_b_owes_a
    }

    /// Check if the position exceeds the imbalance limit
    pub fn exceeds_limit(&self, max_imbalance: i64) -> bool {
        self.net_position().abs() > max_imbalance
    }

    /// Add a pending transfer
    pub fn add_transfer(&mut self, transfer: CrossCoopTransfer) {
        self.pending_transfers.push(transfer);
    }
}

/// A cross-cooperative transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossCoopTransfer {
    /// Unique transfer ID
    pub id: String,

    /// Sending cooperative
    pub from_coop: String,

    /// Sending member's DID
    pub from_did: Did,

    /// Receiving cooperative
    pub to_coop: String,

    /// Receiving member's DID
    pub to_did: Did,

    /// Amount in source currency
    pub source_amount: i64,

    /// Source currency
    pub source_currency: String,

    /// Amount in destination currency
    pub dest_amount: i64,

    /// Destination currency
    pub dest_currency: String,

    /// When the transfer was created
    pub timestamp: u64,

    /// Current status
    pub status: TransferStatus,
}

impl CrossCoopTransfer {
    /// Create a new pending transfer
    pub fn new(
        id: String,
        from_coop: String,
        from_did: Did,
        to_coop: String,
        to_did: Did,
        source_amount: i64,
        source_currency: String,
        dest_amount: i64,
        dest_currency: String,
    ) -> Self {
        Self {
            id,
            from_coop,
            from_did,
            to_coop,
            to_did,
            source_amount,
            source_currency,
            dest_amount,
            dest_currency,
            timestamp: current_timestamp(),
            status: TransferStatus::Pending,
        }
    }

    /// Mark as confirmed
    pub fn confirm(&mut self) {
        self.status = TransferStatus::Confirmed;
    }

    /// Mark as settled
    pub fn settle(&mut self) {
        self.status = TransferStatus::Settled;
    }

    /// Mark as disputed
    pub fn dispute(&mut self) {
        self.status = TransferStatus::Disputed;
    }
}

/// Status of a cross-cooperative transfer
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    /// Transfer proposed but not confirmed by recipient
    #[default]
    Pending,
    /// Transfer confirmed by recipient
    Confirmed,
    /// Transfer included in settlement
    Settled,
    /// Transfer is disputed
    Disputed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_clearing_agreement() {
        let agreement = BilateralClearingAgreement::new(
            "agreement-1".to_string(),
            "food-coop".to_string(),
            test_did(),
            "tech-coop".to_string(),
            test_did(),
        )
        .with_rate("hours", "USD", 25.0)
        .with_interval(SettlementInterval::Weekly)
        .with_max_imbalance(5000);

        assert_eq!(agreement.get_rate("hours", "USD"), Some(25.0));
        assert_eq!(agreement.max_imbalance, 5000);
        assert!(!agreement.is_fully_signed());
    }

    #[test]
    fn test_clearing_position() {
        let mut position = ClearingPosition::new("agreement-1".to_string());

        position.coop_a_owes_b = 1000;
        position.coop_b_owes_a = 300;

        assert_eq!(position.net_position(), 700);
        assert!(!position.exceeds_limit(1000));
        assert!(position.exceeds_limit(500));
    }

    #[test]
    fn test_cross_coop_transfer() {
        let mut transfer = CrossCoopTransfer::new(
            "transfer-1".to_string(),
            "food-coop".to_string(),
            test_did(),
            "tech-coop".to_string(),
            test_did(),
            100,
            "hours".to_string(),
            2500,
            "USD".to_string(),
        );

        assert_eq!(transfer.status, TransferStatus::Pending);

        transfer.confirm();
        assert_eq!(transfer.status, TransferStatus::Confirmed);

        transfer.settle();
        assert_eq!(transfer.status, TransferStatus::Settled);
    }
}
