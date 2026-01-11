//! Labor share gossip messages (Issue #391)
//!
//! Message types for propagating labor share events across the network:
//! - Surplus allocations
//! - Bond issuance announcements
//! - Bond payment notifications

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Labor share gossip message types
///
/// These messages propagate on the `labor-shares:allocations` topic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LaborShareMessage {
    /// Announce a surplus allocation has been approved
    ///
    /// Sent when a cooperative approves periodic surplus distribution.
    /// Subscribers can request details if they're affected.
    AllocationAnnouncement {
        /// Cooperative that made the allocation
        cooperative_id: String,
        /// Period identifier (e.g., "2026-Q1")
        period: String,
        /// Total surplus amount allocated
        total_surplus: i64,
        /// Currency of the allocation
        currency: String,
        /// Number of shares receiving allocation
        share_count: u32,
        /// Unix timestamp of announcement
        timestamp: u64,
    },

    /// Request allocation details for a specific member
    ///
    /// Members can request their individual allocation details.
    AllocationRequest {
        /// Cooperative to query
        cooperative_id: String,
        /// Period to query
        period: String,
        /// Member requesting their allocation
        member: Did,
    },

    /// Response with allocation details for a member
    AllocationDetails {
        /// Cooperative that made the allocation
        cooperative_id: String,
        /// Period of allocation
        period: String,
        /// Member's DID
        member: Did,
        /// Amount allocated to this member
        amount: i64,
        /// Currency
        currency: String,
        /// Basis for allocation (e.g., "hours_worked", "tenure", "equal")
        basis: String,
        /// Raw value used in calculation (e.g., hours worked)
        basis_value: f64,
    },

    /// Announce a share redemption has been approved
    ///
    /// Sent when a departing member's shares are being redeemed.
    RedemptionAnnouncement {
        /// Cooperative processing the redemption
        cooperative_id: String,
        /// Member whose shares are being redeemed
        member: Did,
        /// Number of shares being redeemed
        share_count: u32,
        /// Total redemption value
        total_value: i64,
        /// Currency
        currency: String,
        /// Reason for redemption
        reason: String,
    },
}

/// Bond-related gossip message types
///
/// These messages propagate on `bonds:issuance` and `bonds:payments` topics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BondMessage {
    /// Announce a new bond offering is open for subscription
    ///
    /// Propagates on `bonds:issuance` topic (`TrustClass::Known` - Known+).
    BondOfferingAnnouncement {
        /// Unique bond offering ID
        bond_id: String,
        /// Cooperative issuing the bond
        issuing_cooperative: String,
        /// Purpose of the bond (project funding, equipment, etc.)
        purpose: String,
        /// Principal amount requested
        principal_requested: i64,
        /// Currency
        currency: String,
        /// Interest rate in basis points (100 = 1%)
        interest_rate_bps: u32,
        /// Term in days
        term_days: u32,
        /// Minimum subscription amount
        min_subscription: i64,
        /// Maximum subscription amount (per investor)
        max_subscription: Option<i64>,
        /// Deadline for subscription (Unix timestamp)
        subscription_deadline: u64,
        /// Collateral description (if any)
        collateral_description: Option<String>,
    },

    /// Announce bond subscription is now filled
    ///
    /// Sent when a bond offering reaches its target.
    BondFilled {
        /// Bond ID that was filled
        bond_id: String,
        /// Total amount subscribed
        amount_subscribed: i64,
        /// Number of subscribers
        subscriber_count: u32,
        /// Timestamp
        timestamp: u64,
    },

    /// Announce a bond subscription by a cooperative/member
    ///
    /// Allows tracking of bond subscription activity.
    BondSubscription {
        /// Bond ID subscribed to
        bond_id: String,
        /// Subscriber (cooperative or individual DID)
        subscriber: Did,
        /// Amount subscribed
        amount: i64,
        /// Timestamp
        timestamp: u64,
    },

    /// Bond payment notification (trust-gated)
    ///
    /// Propagates on `bonds:payments` topic (`TrustClass::Partner` - Partner+).
    /// Only sent to subscribers with sufficient trust.
    PaymentNotification {
        /// Bond ID
        bond_id: String,
        /// Payment type
        payment_type: BondPaymentType,
        /// Payment amount
        amount: i64,
        /// Currency
        currency: String,
        /// Due date (Unix timestamp)
        due_date: u64,
        /// Whether payment was made on time
        on_time: bool,
        /// Timestamp of notification
        timestamp: u64,
    },

    /// Bond maturity notification
    ///
    /// Sent when a bond reaches maturity.
    BondMatured {
        /// Bond ID
        bond_id: String,
        /// Issuing cooperative
        issuing_cooperative: String,
        /// Total principal repaid
        principal_repaid: i64,
        /// Total interest paid
        total_interest_paid: i64,
        /// Currency
        currency: String,
        /// Timestamp
        timestamp: u64,
    },
}

/// Type of bond payment (mirrors icn_ledger::BondPaymentType)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BondPaymentType {
    /// Regular interest payment
    Interest,
    /// Principal repayment
    Principal,
    /// Combined interest and principal (at maturity)
    InterestAndPrincipal,
}

/// Topic names for labor share gossip
pub mod topics {
    /// Surplus allocation announcements (`TrustClass::Known` - Known+)
    pub const LABOR_SHARES_ALLOCATIONS: &str = "labor-shares:allocations";

    /// Bond issuance announcements (`TrustClass::Known` - Known+)
    pub const BONDS_ISSUANCE: &str = "bonds:issuance";

    /// Bond payment notifications (`TrustClass::Partner` - Partner+)
    pub const BONDS_PAYMENTS: &str = "bonds:payments";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_announcement_serialization() {
        let msg = LaborShareMessage::AllocationAnnouncement {
            cooperative_id: "coop-123".to_string(),
            period: "2026-Q1".to_string(),
            total_surplus: 100_000,
            currency: "ICN".to_string(),
            share_count: 50,
            timestamp: 1735862400,
        };

        // Use binary serialization (gossip messages are binary)
        let serialized = icn_encoding::encode_bincode_legacy(&msg).expect("serialize");
        let deserialized: LaborShareMessage =
            icn_encoding::decode_bincode_legacy(&serialized).expect("deserialize");

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_bond_offering_serialization() {
        let msg = BondMessage::BondOfferingAnnouncement {
            bond_id: "bond-abc".to_string(),
            issuing_cooperative: "worker-coop".to_string(),
            purpose: "Equipment purchase".to_string(),
            principal_requested: 50_000,
            currency: "ICN".to_string(),
            interest_rate_bps: 500, // 5%
            term_days: 365,
            min_subscription: 100,
            max_subscription: Some(10_000),
            subscription_deadline: 1738454400,
            collateral_description: Some("Equipment lien".to_string()),
        };

        let serialized = icn_encoding::encode_bincode_legacy(&msg).expect("serialize");
        let deserialized: BondMessage =
            icn_encoding::decode_bincode_legacy(&serialized).expect("deserialize");

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_payment_notification_serialization() {
        let msg = BondMessage::PaymentNotification {
            bond_id: "bond-xyz".to_string(),
            payment_type: BondPaymentType::Interest,
            amount: 250,
            currency: "ICN".to_string(),
            due_date: 1736467200,
            on_time: true,
            timestamp: 1736467100,
        };

        let serialized = icn_encoding::encode_bincode_legacy(&msg).expect("serialize");
        let deserialized: BondMessage =
            icn_encoding::decode_bincode_legacy(&serialized).expect("deserialize");

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_topic_names() {
        assert_eq!(topics::LABOR_SHARES_ALLOCATIONS, "labor-shares:allocations");
        assert_eq!(topics::BONDS_ISSUANCE, "bonds:issuance");
        assert_eq!(topics::BONDS_PAYMENTS, "bonds:payments");
    }
}
