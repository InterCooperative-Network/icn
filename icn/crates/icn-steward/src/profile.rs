//! Steward Profile - Identity and status of steward nodes
//!
//! Stewards are trusted nodes in the SDIS network that:
//! - Hold pepper shares for VUI computation
//! - Issue enrollment tokens via blind signatures
//! - Participate in recovery ceremonies
//! - Maintain VUI registry replicas

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Profile of a steward in the SDIS network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardProfile {
    /// The steward's DID (node identity)
    pub steward_did: Did,

    /// The citizen DID of the person operating this steward
    pub citizen_did: Did,

    /// Current operational status
    pub status: StewardStatus,

    /// Jurisdiction tier determining verification requirements
    pub jurisdiction_tier: JurisdictionTier,

    /// Bond amount in credits (stake for honest behavior)
    pub bond_amount: i64,

    /// Geographic region (ISO 3166-1 alpha-2 or custom)
    pub region: String,

    /// Unix timestamp when term started
    pub term_start: u64,

    /// Unix timestamp when term ends (None = indefinite)
    pub term_end: Option<u64>,

    /// Operational statistics
    pub stats: StewardStats,

    /// Public key for enrollment token signatures
    pub token_signing_key: Vec<u8>,

    /// Commitment to pepper share (for verification without revealing)
    pub pepper_share_commitment: [u8; 32],
}

/// Steward operational statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StewardStats {
    /// Number of POPs (Proof of Personhood) issued
    pub pops_issued: u64,

    /// Number of fraud investigations participated in
    pub fraud_investigations: u64,

    /// Number of proven fraud cases
    pub proven_fraud: u64,

    /// Number of recovery ceremonies participated in
    pub recoveries_participated: u64,

    /// Number of VUI checks processed
    pub vui_checks: u64,

    /// Total uptime in seconds
    pub uptime_seconds: u64,

    /// Last heartbeat timestamp
    pub last_heartbeat: u64,

    /// Daily vouch tracking (Issue #396 - sybil resistance)
    /// Tuple: (day_start_timestamp, vouch_count_for_day)
    #[serde(default)]
    pub daily_vouches: (u64, u32),
}

/// Steward operational status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StewardStatus {
    /// Actively serving enrollment and recovery requests
    Active,

    /// Temporarily suspended (can be reinstated)
    Suspended {
        /// Reason for suspension
        reason: String,
        /// Unix timestamp when suspension ends (None = indefinite)
        until: Option<u64>,
    },

    /// Permanently revoked (cannot be reinstated)
    Revoked {
        /// Reason for revocation
        reason: String,
        /// Unix timestamp when revoked
        at: u64,
    },

    /// Term has expired (requires renewal)
    TermExpired {
        /// Unix timestamp when term expired
        expired_at: u64,
    },

    /// Pending activation (newly joined, awaiting bond)
    Pending {
        /// When the application was submitted
        applied_at: u64,
    },
}

/// Jurisdiction tier determining verification requirements
///
/// Higher tiers require more scrutiny for enrollments from that jurisdiction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum JurisdictionTier {
    /// Standard jurisdiction - normal verification
    #[default]
    Tier1,

    /// Enhanced monitoring - additional document checks
    Tier2,

    /// High-risk - requires co-signing by another steward
    Tier3,
}

impl StewardProfile {
    /// Create a new pending steward profile
    pub fn new_pending(
        steward_did: Did,
        citizen_did: Did,
        region: String,
        token_signing_key: Vec<u8>,
        pepper_share_commitment: [u8; 32],
    ) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            steward_did,
            citizen_did,
            status: StewardStatus::Pending { applied_at: now },
            jurisdiction_tier: JurisdictionTier::Tier1,
            bond_amount: 0,
            region,
            term_start: 0,
            term_end: None,
            stats: StewardStats::default(),
            token_signing_key,
            pepper_share_commitment,
        }
    }

    /// Check if steward is currently active
    pub fn is_active(&self) -> bool {
        matches!(self.status, StewardStatus::Active)
    }

    /// Check if steward can issue enrollment tokens
    pub fn can_issue_tokens(&self) -> bool {
        self.is_active() && !self.is_term_expired()
    }

    /// Check if term has expired
    pub fn is_term_expired(&self) -> bool {
        if let Some(term_end) = self.term_end {
            let now = icn_time::current_timestamp_secs();
            now > term_end
        } else {
            false
        }
    }

    /// Activate the steward (called after bond is posted)
    pub fn activate(&mut self, bond_amount: i64, term_duration_secs: Option<u64>) {
        let now = icn_time::current_timestamp_secs();

        self.status = StewardStatus::Active;
        self.bond_amount = bond_amount;
        self.term_start = now;
        self.term_end = term_duration_secs.map(|d| now + d);
    }

    /// Suspend the steward
    pub fn suspend(&mut self, reason: String, duration_secs: Option<u64>) {
        let until = duration_secs.map(|d| icn_time::current_timestamp_secs() + d);

        self.status = StewardStatus::Suspended { reason, until };
    }

    /// Revoke the steward permanently
    pub fn revoke(&mut self, reason: String) {
        let now = icn_time::current_timestamp_secs();

        self.status = StewardStatus::Revoked { reason, at: now };
    }

    /// Record a heartbeat
    pub fn record_heartbeat(&mut self) {
        self.stats.last_heartbeat = icn_time::current_timestamp_secs();
    }

    /// Increment POP issued counter
    pub fn record_pop_issued(&mut self) {
        self.stats.pops_issued += 1;
    }

    /// Increment VUI check counter
    pub fn record_vui_check(&mut self) {
        self.stats.vui_checks += 1;
    }

    /// Increment recovery participation counter
    pub fn record_recovery_participation(&mut self) {
        self.stats.recoveries_participated += 1;
    }

    /// Seconds per UTC day for vouch rate limiting calculations
    const SECONDS_PER_DAY: u64 = 86400;

    /// Check if steward can vouch today (Issue #396 - sybil resistance)
    ///
    /// Returns the number of vouches remaining for today, or 0 if limit reached.
    pub fn vouches_remaining_today(&self, max_per_day: u32) -> u32 {
        let now = icn_time::current_timestamp_secs();
        let day_start = now - (now % Self::SECONDS_PER_DAY); // Start of current UTC day

        let (last_day, count) = self.stats.daily_vouches;
        if last_day == day_start {
            max_per_day.saturating_sub(count)
        } else {
            // New day, reset count
            max_per_day
        }
    }

    /// Record a vouch and check if within daily limit (Issue #396 - sybil resistance)
    ///
    /// Returns `Some(remaining)` if vouch was allowed, `None` if limit exceeded.
    pub fn record_vouch(&mut self, max_per_day: u32) -> Option<u32> {
        let now = icn_time::current_timestamp_secs();
        let day_start = now - (now % Self::SECONDS_PER_DAY); // Start of current UTC day

        let (last_day, count) = self.stats.daily_vouches;
        let new_count = if last_day == day_start {
            // Same day - increment
            if count >= max_per_day {
                return None;
            }
            count + 1
        } else {
            // New day - reset to 1
            1
        };

        self.stats.daily_vouches = (day_start, new_count);
        Some(max_per_day.saturating_sub(new_count))
    }
}

impl StewardStats {
    /// Create new empty stats
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        // Create a test DID
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_steward_profile_creation() {
        let steward_did = test_did();
        let citizen_did = test_did();

        let profile = StewardProfile::new_pending(
            steward_did.clone(),
            citizen_did,
            "US".to_string(),
            vec![1, 2, 3],
            [42u8; 32],
        );

        assert!(!profile.is_active());
        assert!(!profile.can_issue_tokens());
        assert_eq!(profile.region, "US");
    }

    #[test]
    fn test_steward_activation() {
        let steward_did = test_did();
        let citizen_did = test_did();

        let mut profile = StewardProfile::new_pending(
            steward_did,
            citizen_did,
            "US".to_string(),
            vec![1, 2, 3],
            [42u8; 32],
        );

        profile.activate(1000, Some(86400 * 365)); // 1 year term

        assert!(profile.is_active());
        assert!(profile.can_issue_tokens());
        assert_eq!(profile.bond_amount, 1000);
        assert!(profile.term_end.is_some());
    }

    #[test]
    fn test_steward_suspension() {
        let steward_did = test_did();
        let citizen_did = test_did();

        let mut profile = StewardProfile::new_pending(
            steward_did,
            citizen_did,
            "US".to_string(),
            vec![1, 2, 3],
            [42u8; 32],
        );

        profile.activate(1000, None);
        assert!(profile.is_active());

        profile.suspend("Investigation pending".to_string(), Some(86400));

        assert!(!profile.is_active());
        assert!(!profile.can_issue_tokens());
        assert!(matches!(profile.status, StewardStatus::Suspended { .. }));
    }

    #[test]
    fn test_steward_revocation() {
        let steward_did = test_did();
        let citizen_did = test_did();

        let mut profile = StewardProfile::new_pending(
            steward_did,
            citizen_did,
            "US".to_string(),
            vec![1, 2, 3],
            [42u8; 32],
        );

        profile.activate(1000, None);
        profile.revoke("Fraud detected".to_string());

        assert!(!profile.is_active());
        assert!(matches!(profile.status, StewardStatus::Revoked { .. }));
    }

    #[test]
    fn test_stats_recording() {
        let steward_did = test_did();
        let citizen_did = test_did();

        let mut profile = StewardProfile::new_pending(
            steward_did,
            citizen_did,
            "US".to_string(),
            vec![1, 2, 3],
            [42u8; 32],
        );

        profile.activate(1000, None);
        profile.record_pop_issued();
        profile.record_pop_issued();
        profile.record_vui_check();
        profile.record_heartbeat();

        assert_eq!(profile.stats.pops_issued, 2);
        assert_eq!(profile.stats.vui_checks, 1);
        assert!(profile.stats.last_heartbeat > 0);
    }

    #[test]
    fn test_jurisdiction_tiers() {
        assert_eq!(JurisdictionTier::default(), JurisdictionTier::Tier1);

        let tier1 = JurisdictionTier::Tier1;
        let tier2 = JurisdictionTier::Tier2;
        let tier3 = JurisdictionTier::Tier3;

        assert_ne!(tier1, tier2);
        assert_ne!(tier2, tier3);
    }

    #[test]
    fn test_vouch_rate_limiting() {
        // Issue #396 - test daily vouch limits
        let steward_did = test_did();
        let citizen_did = test_did();

        let mut profile = StewardProfile::new_pending(
            steward_did,
            citizen_did,
            "US".to_string(),
            vec![1, 2, 3],
            [42u8; 32],
        );

        profile.activate(1000, None);

        let max_per_day = 3;

        // Initially should have all vouches available
        assert_eq!(profile.vouches_remaining_today(max_per_day), 3);

        // Record vouches
        assert_eq!(profile.record_vouch(max_per_day), Some(2)); // 2 remaining
        assert_eq!(profile.record_vouch(max_per_day), Some(1)); // 1 remaining
        assert_eq!(profile.record_vouch(max_per_day), Some(0)); // 0 remaining

        // Should be at limit now
        assert_eq!(profile.vouches_remaining_today(max_per_day), 0);

        // Should reject additional vouches
        assert_eq!(profile.record_vouch(max_per_day), None);
        assert_eq!(profile.record_vouch(max_per_day), None);

        // Still at 0 remaining
        assert_eq!(profile.vouches_remaining_today(max_per_day), 0);
    }

    #[test]
    fn test_vouch_limit_resets_daily() {
        // Issue #396 - test that vouch limit resets at day boundary
        let steward_did = test_did();
        let citizen_did = test_did();

        let mut profile = StewardProfile::new_pending(
            steward_did,
            citizen_did,
            "US".to_string(),
            vec![1, 2, 3],
            [42u8; 32],
        );

        profile.activate(1000, None);

        // Simulate yesterday's vouches
        const SECONDS_PER_DAY: u64 = 86400;
        let yesterday = icn_time::current_timestamp_secs() - SECONDS_PER_DAY;
        let yesterday_day_start = yesterday - (yesterday % SECONDS_PER_DAY);
        profile.stats.daily_vouches = (yesterday_day_start, 10);

        // Should have full quota today since it's a new day
        let max_per_day = 5;
        assert_eq!(profile.vouches_remaining_today(max_per_day), 5);

        // First vouch of the new day should succeed
        assert_eq!(profile.record_vouch(max_per_day), Some(4));
    }
}
