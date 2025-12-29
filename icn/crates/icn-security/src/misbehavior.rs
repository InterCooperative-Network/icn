//! Byzantine Fault Detection - Misbehavior tracking and reputation management
//!
//! Phase 18 Week 1-2: Byzantine Fault Detection
//!
//! This module implements automatic detection of malicious behavior, reputation scoring,
//! and quarantine/ban mechanisms to protect the network from Byzantine actors.

use icn_identity::Did;
use icn_obs::metrics::misbehavior as metrics;
use icn_store::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

/// Types of misbehavior that can be detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Violation {
    /// Invalid signature detected on a message
    InvalidSignature { message_hash: ContentHash },

    /// Conflicting ledger entries from same author
    ConflictingLedgerEntries {
        entry1: ContentHash,
        entry2: ContentHash,
    },

    /// Compute result verification failed
    FailedComputeVerification {
        task_hash: ContentHash,
        expected: ContentHash,
        actual: ContentHash,
    },

    /// Excessive resource usage (rate limiting violation)
    ExcessiveResourceUse {
        metric: String,
        observed: u64,
        limit: u64,
    },

    /// Trust graph spam (excessive attestation publishing)
    TrustGraphSpam { rate_per_hour: f64, threshold: f64 },

    /// Conflicting signed statements (generic Byzantine behavior)
    ConflictingSignedStatements {
        statement1: ContentHash,
        statement2: ContentHash,
        conflict_type: String,
    },

    /// Replay attack detected (sequence number reuse)
    ReplayAttack {
        message_hash: ContentHash,
        sequence: u64,
    },
}

impl Violation {
    /// Get a severity score for this violation type
    /// Higher score = more severe
    pub fn severity(&self) -> u32 {
        match self {
            // Critical violations (10 points) - clear malicious intent
            Violation::ConflictingLedgerEntries { .. } => 10,
            Violation::ConflictingSignedStatements { .. } => 10,
            Violation::ReplayAttack { .. } => 10,

            // Major violations (5 points) - likely malicious
            Violation::FailedComputeVerification { .. } => 5,
            Violation::InvalidSignature { .. } => 5,

            // Minor violations (1 point) - might be accidental
            Violation::ExcessiveResourceUse { .. } => 1,
            Violation::TrustGraphSpam { .. } => 1,
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            Violation::InvalidSignature { message_hash } => {
                format!("Invalid signature on message {}", hex::encode(message_hash))
            }
            Violation::ConflictingLedgerEntries { entry1, entry2 } => {
                format!(
                    "Conflicting ledger entries: {} vs {}",
                    hex::encode(entry1),
                    hex::encode(entry2)
                )
            }
            Violation::FailedComputeVerification { task_hash, .. } => {
                format!(
                    "Compute verification failed for task {}",
                    hex::encode(task_hash)
                )
            }
            Violation::ExcessiveResourceUse {
                metric,
                observed,
                limit,
            } => {
                format!("{metric}: {observed} (limit: {limit})")
            }
            Violation::TrustGraphSpam {
                rate_per_hour,
                threshold,
            } => {
                format!("Trust graph spam: {rate_per_hour:.1}/hr (threshold: {threshold:.1})")
            }
            Violation::ConflictingSignedStatements { conflict_type, .. } => {
                format!("Conflicting signed statements: {conflict_type}")
            }
            Violation::ReplayAttack { sequence, .. } => {
                format!("Replay attack detected (sequence: {sequence})")
            }
        }
    }

    /// Should this violation trigger immediate ban?
    pub fn is_auto_ban(&self) -> bool {
        matches!(
            self,
            Violation::ConflictingLedgerEntries { .. }
                | Violation::ConflictingSignedStatements { .. }
                | Violation::ReplayAttack { .. }
        )
    }
}

/// Violation record with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationRecord {
    pub violation: Violation,
    pub detected_at: SystemTime,
    pub evidence: Vec<u8>,
}

/// Reputation score for a DID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationScore {
    /// Current score (0.0 = banned, 1.0 = pristine)
    pub score: f64,

    /// Total violations detected
    pub total_violations: usize,

    /// Severity points accumulated
    pub severity_points: u32,

    /// Last violation timestamp
    pub last_violation: Option<SystemTime>,

    /// When the score was last updated
    pub updated_at: SystemTime,
}

impl ReputationScore {
    /// Create a new pristine reputation score
    pub fn new() -> Self {
        Self {
            score: 1.0,
            total_violations: 0,
            severity_points: 0,
            last_violation: None,
            updated_at: SystemTime::now(),
        }
    }

    /// Apply a violation penalty
    pub fn apply_penalty(&mut self, violation: &Violation, decay_rate: f64) {
        let severity = violation.severity();

        // Decay existing score based on time since last violation
        if let Some(last) = self.last_violation {
            if let Ok(elapsed) = SystemTime::now().duration_since(last) {
                let hours = elapsed.as_secs() as f64 / 3600.0;
                self.score = (self.score + decay_rate * hours).min(1.0);
            }
        }

        // Apply penalty (severity directly reduces score)
        let penalty = severity as f64 * 0.05; // 5% per severity point
        self.score = (self.score - penalty).max(0.0);

        // Update counters
        self.total_violations += 1;
        self.severity_points += severity;
        self.last_violation = Some(SystemTime::now());
        self.updated_at = SystemTime::now();

        debug!(
            "Applied penalty: severity={}, new_score={:.2}",
            severity, self.score
        );
    }

    /// Is this DID quarantined based on threshold?
    pub fn is_quarantined(&self, threshold: f64) -> bool {
        self.score < threshold
    }

    /// Is this DID banned based on threshold?
    pub fn is_banned(&self, threshold: f64) -> bool {
        self.score <= threshold
    }
}

impl Default for ReputationScore {
    fn default() -> Self {
        Self::new()
    }
}

/// Thresholds for misbehavior detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MisbehaviorThresholds {
    /// Reputation score below which a peer is quarantined
    pub quarantine_threshold: f64,

    /// Reputation score at which a peer is banned
    pub ban_threshold: f64,

    /// Maximum violations per hour before auto-quarantine
    pub max_violations_per_hour: usize,

    /// Reputation decay rate (points per hour)
    pub decay_rate: f64,

    /// How long to keep violation history (seconds)
    pub violation_retention_secs: u64,
}

impl Default for MisbehaviorThresholds {
    fn default() -> Self {
        Self {
            quarantine_threshold: 0.5,
            ban_threshold: 0.0,
            max_violations_per_hour: 10,
            decay_rate: 0.01,                        // 1% per hour
            violation_retention_secs: 7 * 24 * 3600, // 7 days
        }
    }
}

/// Byzantine fault detector
/// Callback to update trust graph when reputation changes
pub type TrustPenaltyCallback = std::sync::Arc<dyn Fn(&Did, f64) + Send + Sync>;

pub struct MisbehaviorDetector {
    /// Violation records per DID
    violations: HashMap<Did, Vec<ViolationRecord>>,

    /// Reputation scores per DID
    reputation_scores: HashMap<Did, ReputationScore>,

    /// Detection thresholds
    thresholds: MisbehaviorThresholds,

    /// Quarantined DIDs
    quarantined: HashMap<Did, SystemTime>,

    /// Banned DIDs
    banned: HashMap<Did, SystemTime>,

    /// Callback to update trust graph when reputation changes (Phase 18)
    trust_penalty_callback: Option<TrustPenaltyCallback>,
}

impl MisbehaviorDetector {
    /// Create a new misbehavior detector
    pub fn new(thresholds: MisbehaviorThresholds) -> Self {
        Self {
            violations: HashMap::new(),
            reputation_scores: HashMap::new(),
            thresholds,
            quarantined: HashMap::new(),
            banned: HashMap::new(),
            trust_penalty_callback: None,
        }
    }

    /// Set callback to update trust graph when reputation changes (Phase 18)
    pub fn set_trust_penalty_callback(&mut self, callback: TrustPenaltyCallback) {
        self.trust_penalty_callback = Some(callback);
    }

    /// Record a violation for a DID
    pub fn record_violation(&mut self, did: &Did, violation: Violation, evidence: Vec<u8>) {
        info!(
            "Recording violation for {}: {}",
            did,
            violation.description()
        );

        // Create violation record
        let record = ViolationRecord {
            violation: violation.clone(),
            detected_at: SystemTime::now(),
            evidence,
        };

        // Add to violation history
        self.violations.entry(did.clone()).or_default().push(record);

        // Update reputation score
        let score = self.reputation_scores.entry(did.clone()).or_default();

        score.apply_penalty(&violation, self.thresholds.decay_rate);

        // Emit metrics
        metrics::violations_inc(&did.to_string(), &violation.description());

        // Update trust graph if callback is set (Phase 18)
        if let Some(ref callback) = self.trust_penalty_callback {
            callback(did, score.score);
        }

        // Check for auto-quarantine/ban
        if violation.is_auto_ban() || score.is_banned(self.thresholds.ban_threshold) {
            self.ban_peer(did);
        } else if score.is_quarantined(self.thresholds.quarantine_threshold) {
            self.quarantine_peer(did);
        }

        // Check rate-based quarantine
        if self.should_rate_limit_quarantine(did) {
            warn!(
                "Rate-limiting violation threshold exceeded for {}, quarantining",
                did
            );
            self.quarantine_peer(did);
        }

        // Cleanup old violations
        self.cleanup_old_violations();
    }

    /// Get violations for a DID
    pub fn get_violations(&self, did: &Did) -> Vec<&ViolationRecord> {
        self.violations
            .get(did)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Get reputation score for a DID
    pub fn get_reputation(&self, did: &Did) -> Option<&ReputationScore> {
        self.reputation_scores.get(did)
    }

    /// Is a DID quarantined?
    pub fn is_quarantined(&self, did: &Did) -> bool {
        self.quarantined.contains_key(did)
    }

    /// Is a DID banned?
    pub fn is_banned(&self, did: &Did) -> bool {
        self.banned.contains_key(did)
    }

    /// Quarantine a peer
    fn quarantine_peer(&mut self, did: &Did) {
        if self.quarantined.contains_key(did) {
            return; // Already quarantined
        }

        warn!("Quarantining peer {}", did);
        self.quarantined.insert(did.clone(), SystemTime::now());

        metrics::quarantined_inc();
    }

    /// Ban a peer permanently
    fn ban_peer(&mut self, did: &Did) {
        warn!("BANNING peer {} for severe violation", did);

        self.banned.insert(did.clone(), SystemTime::now());

        // If peer was quarantined, remove from quarantine and decrement metric
        if self.quarantined.remove(did).is_some() {
            metrics::quarantined_dec();
        }

        // Set reputation to zero
        if let Some(score) = self.reputation_scores.get_mut(did) {
            score.score = 0.0;
        }

        metrics::banned_inc();
    }

    /// Check if a DID should be quarantined based on violation rate
    fn should_rate_limit_quarantine(&self, did: &Did) -> bool {
        if let Some(violations) = self.violations.get(did) {
            let now = SystemTime::now();
            let one_hour_ago = now - Duration::from_secs(3600);

            let recent_count = violations
                .iter()
                .filter(|v| v.detected_at > one_hour_ago)
                .count();

            recent_count > self.thresholds.max_violations_per_hour
        } else {
            false
        }
    }

    /// Cleanup old violation records
    fn cleanup_old_violations(&mut self) {
        let now = SystemTime::now();
        let retention_duration = Duration::from_secs(self.thresholds.violation_retention_secs);
        let cutoff = now - retention_duration;

        for violations in self.violations.values_mut() {
            violations.retain(|v| v.detected_at > cutoff);
        }

        // Remove DIDs with no violations
        self.violations.retain(|_, v| !v.is_empty());
    }

    /// Check and release peers whose reputation has recovered above quarantine threshold.
    ///
    /// Should be called periodically to restore baseline trust for rehabilitated peers.
    /// Returns the list of DIDs that were released from quarantine.
    pub fn check_quarantine_releases(&mut self) -> Vec<Did> {
        let mut released = Vec::new();

        // Apply reputation decay first to update scores
        for (_did, score) in self.reputation_scores.iter_mut() {
            if let Some(last) = score.last_violation {
                if let Ok(elapsed) = SystemTime::now().duration_since(last) {
                    let hours = elapsed.as_secs() as f64 / 3600.0;
                    score.score = (score.score + self.thresholds.decay_rate * hours).min(1.0);
                }
            }
        }

        // Check for quarantine release
        let quarantine_threshold = self.thresholds.quarantine_threshold;
        let release_threshold = quarantine_threshold + 0.1; // Must exceed threshold by 0.1

        let dids_to_check: Vec<Did> = self.quarantined.keys().cloned().collect();

        for did in dids_to_check {
            if let Some(score) = self.reputation_scores.get(&did) {
                if score.score >= release_threshold {
                    self.release_from_quarantine(&did);
                    released.push(did);
                }
            }
        }

        if !released.is_empty() {
            info!("Released {} peers from quarantine", released.len());
        }

        released
    }

    /// Release a peer from quarantine, restoring baseline trust.
    ///
    /// This should only be called when a peer's reputation has sufficiently recovered.
    pub fn release_from_quarantine(&mut self, did: &Did) {
        if self.quarantined.remove(did).is_some() {
            info!("Released {} from quarantine (reputation recovered)", did);
            metrics::quarantined_dec();

            // Update trust graph if callback is set
            if let Some(ref callback) = self.trust_penalty_callback {
                if let Some(score) = self.reputation_scores.get(did) {
                    callback(did, score.score);
                }
            }
        }
    }

    /// Baseline reputation score for administrative release (0.6 = moderate trust)
    const BASELINE_REPUTATION_SCORE: f64 = 0.6;

    /// Manually force release from quarantine (administrative action).
    ///
    /// This bypasses reputation checks and resets to baseline trust.
    pub fn force_release_from_quarantine(&mut self, did: &Did) {
        if self.quarantined.remove(did).is_some() {
            warn!("Administratively released {} from quarantine", did);
            metrics::quarantined_dec();

            // Reset reputation to baseline
            if let Some(score) = self.reputation_scores.get_mut(did) {
                score.score = Self::BASELINE_REPUTATION_SCORE;
                score.total_violations = 0;
                score.severity_points = 0;
            }

            // Update trust graph if callback is set
            if let Some(ref callback) = self.trust_penalty_callback {
                if let Some(score) = self.reputation_scores.get(did) {
                    callback(did, score.score);
                }
            }
        }
    }

    /// Get statistics for monitoring
    pub fn get_stats(&self) -> MisbehaviorStats {
        MisbehaviorStats {
            total_tracked_dids: self.reputation_scores.len(),
            quarantined_count: self.quarantined.len(),
            banned_count: self.banned.len(),
            total_violations: self.violations.values().map(|v| v.len()).sum(),
        }
    }
}

/// Statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MisbehaviorStats {
    pub total_tracked_dids: usize,
    pub quarantined_count: usize,
    pub banned_count: usize,
    pub total_violations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_violation_severity() {
        let critical = Violation::ConflictingLedgerEntries {
            entry1: [0u8; 32],
            entry2: [1u8; 32],
        };
        assert_eq!(critical.severity(), 10);
        assert!(critical.is_auto_ban());

        let minor = Violation::ExcessiveResourceUse {
            metric: "messages".to_string(),
            observed: 100,
            limit: 50,
        };
        assert_eq!(minor.severity(), 1);
        assert!(!minor.is_auto_ban());
    }

    #[test]
    fn test_reputation_score_new() {
        let score = ReputationScore::new();
        let thresholds = MisbehaviorThresholds::default();

        assert_eq!(score.score, 1.0);
        assert_eq!(score.total_violations, 0);
        assert!(!score.is_quarantined(thresholds.quarantine_threshold));
        assert!(!score.is_banned(thresholds.ban_threshold));
    }

    #[test]
    fn test_reputation_penalty() {
        let mut score = ReputationScore::new();

        // Apply a minor violation
        let violation = Violation::ExcessiveResourceUse {
            metric: "test".to_string(),
            observed: 10,
            limit: 5,
        };

        score.apply_penalty(&violation, 0.01);

        assert_eq!(score.total_violations, 1);
        assert!(score.score < 1.0);
        assert!(score.score > 0.9); // Minor violation, small penalty
    }

    #[test]
    fn test_reputation_quarantine_threshold() {
        let mut score = ReputationScore::new();
        let thresholds = MisbehaviorThresholds::default();

        // Apply critical violations to drop below quarantine threshold
        for _ in 0..6 {
            let violation = Violation::InvalidSignature {
                message_hash: [0u8; 32],
            };
            score.apply_penalty(&violation, 0.01);
        }

        assert!(
            score.is_quarantined(thresholds.quarantine_threshold),
            "Score should be below threshold ({}): actual score = {}",
            thresholds.quarantine_threshold,
            score.score
        );
    }

    #[test]
    fn test_detector_record_violation() {
        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());
        let did = test_did();

        let violation = Violation::ExcessiveResourceUse {
            metric: "test".to_string(),
            observed: 100,
            limit: 50,
        };

        detector.record_violation(&did, violation.clone(), vec![]);

        assert_eq!(detector.get_violations(&did).len(), 1);
        assert!(detector.get_reputation(&did).is_some());
    }

    #[test]
    fn test_detector_auto_ban() {
        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());
        let did = test_did();

        // Critical violation should trigger auto-ban
        let violation = Violation::ConflictingLedgerEntries {
            entry1: [0u8; 32],
            entry2: [1u8; 32],
        };

        detector.record_violation(&did, violation, vec![]);

        assert!(detector.is_banned(&did), "DID should be auto-banned");
        assert!(
            !detector.is_quarantined(&did),
            "Banned DIDs not quarantined"
        );
    }

    #[test]
    fn test_detector_rate_limit_quarantine() {
        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds {
            max_violations_per_hour: 3,
            ..Default::default()
        });

        let did = test_did();

        // Exceed rate limit
        for _ in 0..4 {
            let violation = Violation::ExcessiveResourceUse {
                metric: "test".to_string(),
                observed: 10,
                limit: 5,
            };
            detector.record_violation(&did, violation, vec![]);
        }

        assert!(
            detector.is_quarantined(&did),
            "DID should be quarantined for rate limiting"
        );
    }

    #[test]
    fn test_detector_stats() {
        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());

        let did1 = test_did();
        let did2 = test_did();

        detector.record_violation(
            &did1,
            Violation::ExcessiveResourceUse {
                metric: "test".to_string(),
                observed: 10,
                limit: 5,
            },
            vec![],
        );

        detector.record_violation(
            &did2,
            Violation::ConflictingLedgerEntries {
                entry1: [0u8; 32],
                entry2: [1u8; 32],
            },
            vec![],
        );

        let stats = detector.get_stats();
        assert_eq!(stats.total_tracked_dids, 2);
        assert_eq!(stats.total_violations, 2);
        assert_eq!(stats.banned_count, 1); // did2 auto-banned
    }

    #[test]
    fn test_quarantine_release_with_high_score() {
        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());
        let did = test_did();

        // First quarantine the peer via rate limiting
        for _ in 0..15 {
            let violation = Violation::ExcessiveResourceUse {
                metric: "test".to_string(),
                observed: 10,
                limit: 5,
            };
            detector.record_violation(&did, violation, vec![]);
        }

        assert!(detector.is_quarantined(&did), "DID should be quarantined");

        // Manually set reputation just above release threshold (quarantine_threshold + 0.1)
        if let Some(score) = detector.reputation_scores.get_mut(&did) {
            score.score = 0.61; // Slightly above 0.5 + 0.1 = 0.6 release threshold
        }

        let released = detector.check_quarantine_releases();
        assert!(released.contains(&did), "DID should be released");
        assert!(
            !detector.is_quarantined(&did),
            "DID should no longer be quarantined"
        );
    }

    #[test]
    fn test_force_release_from_quarantine() {
        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());
        let did = test_did();

        // Quarantine the peer
        for _ in 0..15 {
            let violation = Violation::ExcessiveResourceUse {
                metric: "test".to_string(),
                observed: 10,
                limit: 5,
            };
            detector.record_violation(&did, violation, vec![]);
        }

        assert!(detector.is_quarantined(&did), "DID should be quarantined");

        // Force release
        detector.force_release_from_quarantine(&did);

        assert!(
            !detector.is_quarantined(&did),
            "DID should no longer be quarantined"
        );

        // Reputation should be reset to baseline
        let rep = detector.get_reputation(&did).unwrap().score;
        assert!(
            (rep - 0.6).abs() < 0.01,
            "Reputation should be reset to 0.6 baseline"
        );
    }

    #[test]
    fn test_trust_penalty_callback_called() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());

        // Set up callback
        let callback: TrustPenaltyCallback = Arc::new(move |_did, _score| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        });
        detector.set_trust_penalty_callback(callback);

        let did = test_did();
        let violation = Violation::ExcessiveResourceUse {
            metric: "test".to_string(),
            observed: 10,
            limit: 5,
        };

        detector.record_violation(&did, violation, vec![]);

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Trust penalty callback should be called once"
        );
    }

    #[test]
    fn test_trust_penalty_callback_receives_correct_score() {
        use std::sync::{Arc, Mutex};

        let received_score = Arc::new(Mutex::new(None));
        let received_score_clone = received_score.clone();

        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());

        // Set up callback to capture score
        let callback: TrustPenaltyCallback = Arc::new(move |_did, score| {
            *received_score_clone.lock().unwrap() = Some(score);
        });
        detector.set_trust_penalty_callback(callback);

        let did = test_did();
        let violation = Violation::InvalidSignature {
            message_hash: [0u8; 32],
        };

        detector.record_violation(&did, violation, vec![]);

        let score = received_score.lock().unwrap().expect("Score should be set");
        // InvalidSignature has severity 5, penalty = 5 * 0.05 = 0.25
        // Starting from 1.0, new score = 1.0 - 0.25 = 0.75
        assert!(
            (score - 0.75).abs() < 0.01,
            "Score should be approximately 0.75, got {score}"
        );
    }

    #[test]
    fn test_release_from_quarantine_calls_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());

        let callback: TrustPenaltyCallback = Arc::new(move |_did, _score| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        });
        detector.set_trust_penalty_callback(callback);

        let did = test_did();

        // Quarantine via violations
        for _ in 0..15 {
            detector.record_violation(
                &did,
                Violation::ExcessiveResourceUse {
                    metric: "test".to_string(),
                    observed: 10,
                    limit: 5,
                },
                vec![],
            );
        }

        // Reset call count after initial violations
        call_count.store(0, Ordering::SeqCst);

        // Set reputation high enough for release
        if let Some(score) = detector.reputation_scores.get_mut(&did) {
            score.score = 0.65;
        }

        detector.release_from_quarantine(&did);

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Callback should be called on quarantine release"
        );
    }
}
