//! Trust graph type definitions
//!
//! This module defines the three orthogonal trust dimensions used by ICN:
//! - Social: Peer endorsements and community participation
//! - EconomicReliability: Payment history and credit behavior
//! - TechnicalReliability: Node uptime and task success rates

use serde::{Deserialize, Serialize};
use std::fmt;

/// Three orthogonal trust dimensions
///
/// Each dimension tracks a different aspect of trustworthiness:
/// - **Social**: "I know you, we've worked together"
/// - **EconomicReliability**: "You have a consistent record of clearing obligations"
/// - **TechnicalReliability**: "Your node behaves correctly under load"
///
/// This separation prevents any single clique from gaining cross-domain influence.
/// A node can be socially popular but economically unreliable, or technically
/// excellent but socially unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrustGraphType {
    /// Social trust: "I know you, we've worked together"
    ///
    /// Evidence sources:
    /// - Peer endorsements
    /// - Shared governance participation
    /// - Community activity
    /// - Organization membership
    ///
    /// Used by:
    /// - Connection priority in networking
    /// - Gossip bandwidth allocation
    /// - Topic access control
    /// - Governance membership resolution
    #[default]
    Social,

    /// Economic reliability: "You have a consistent record of clearing obligations"
    ///
    /// Evidence sources:
    /// - Cleared transactions
    /// - On-time payments
    /// - Debt history
    /// - Dispute outcomes
    ///
    /// Used by:
    /// - Credit limit calculations
    /// - Dispute weighting
    /// - Federation trade limits
    EconomicReliability,

    /// Technical reliability: "Your node behaves correctly under load"
    ///
    /// Evidence sources:
    /// - Node uptime percentage
    /// - Compute task success rate
    /// - Byzantine violation history
    /// - Storage reliability metrics
    ///
    /// Used by:
    /// - Compute task scheduling
    /// - Contract execution priority
    /// - Storage replica selection
    TechnicalReliability,
}

impl TrustGraphType {
    /// Returns all graph types in canonical order
    pub fn all() -> &'static [TrustGraphType] {
        &[
            Self::Social,
            Self::EconomicReliability,
            Self::TechnicalReliability,
        ]
    }

    /// Returns the storage key prefix for this graph type
    ///
    /// Each graph type uses a separate namespace to ensure isolation:
    /// - Social: `trust/social/edges/{source}:{target}`
    /// - Economic: `trust/economic/edges/{source}:{target}`
    /// - Technical: `trust/technical/edges/{source}:{target}`
    pub fn storage_prefix(&self) -> &'static str {
        match self {
            Self::Social => "trust/social",
            Self::EconomicReliability => "trust/economic",
            Self::TechnicalReliability => "trust/technical",
        }
    }

    /// Returns the default scoring weights for this graph type
    ///
    /// Different dimensions use different direct/transitive weight balances:
    /// - **Social**: 60/40 - Reputation spreads through networks
    /// - **Economic**: 80/20 - Your payment history matters most
    /// - **Technical**: 90/10 - Your node's performance is yours
    pub fn default_weights(&self) -> ScoringWeights {
        match self {
            // Social: More transitive (reputation spreads through networks)
            Self::Social => ScoringWeights {
                direct: 0.6,
                transitive: 0.4,
            },
            // Economic: More direct (your payment history matters most)
            Self::EconomicReliability => ScoringWeights {
                direct: 0.8,
                transitive: 0.2,
            },
            // Technical: Heavily direct (your node's performance is yours)
            Self::TechnicalReliability => ScoringWeights {
                direct: 0.9,
                transitive: 0.1,
            },
        }
    }

    /// Returns a short string identifier for this graph type
    ///
    /// Used in metrics labels and logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Social => "social",
            Self::EconomicReliability => "economic",
            Self::TechnicalReliability => "technical",
        }
    }
}

impl fmt::Display for TrustGraphType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Weights for computing combined trust scores
///
/// Trust scores are computed as:
/// `score = (direct_score * direct) + (transitive_score * transitive)`
///
/// The weights should sum to 1.0 for normalized scores.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoringWeights {
    /// Weight for direct trust (edge from self to target)
    pub direct: f64,
    /// Weight for transitive trust (through intermediaries)
    pub transitive: f64,
}

impl ScoringWeights {
    /// Create new scoring weights
    ///
    /// # Panics
    /// Panics in debug mode if weights don't sum to approximately 1.0
    pub fn new(direct: f64, transitive: f64) -> Self {
        debug_assert!(
            (direct + transitive - 1.0).abs() < 0.001,
            "Scoring weights should sum to 1.0, got {}",
            direct + transitive
        );
        Self { direct, transitive }
    }

    /// Returns the legacy scoring weights (70% direct, 30% transitive)
    ///
    /// This matches the original single-graph scoring algorithm.
    pub fn legacy() -> Self {
        Self {
            direct: 0.7,
            transitive: 0.3,
        }
    }
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self::legacy()
    }
}

/// Error type for invalid trust scores
///
/// This error is returned when attempting to create a `TrustScore` with an
/// invalid value.
#[derive(Debug, Clone, PartialEq)]
pub enum TrustScoreError {
    /// The value was NaN
    NaN,
    /// The value was outside the valid range [0.0, 1.0]
    OutOfRange {
        /// The invalid value
        value: f64,
        /// Minimum valid value (0.0)
        min: f64,
        /// Maximum valid value (1.0)
        max: f64,
    },
}

impl fmt::Display for TrustScoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NaN => write!(f, "Trust score cannot be NaN"),
            Self::OutOfRange { value, min, max } => {
                write!(
                    f,
                    "Trust score must be in range [{min}, {max}], got {value}"
                )
            }
        }
    }
}

impl std::error::Error for TrustScoreError {}

/// A trust score in the range [0.0, 1.0]
///
/// This newtype wrapper provides compile-time type safety for trust scores,
/// preventing them from being mixed with arbitrary floats. All trust scores
/// are validated to be in the valid range and not NaN.
///
/// # Examples
///
/// ```
/// use icn_trust::TrustScore;
///
/// // Create a valid trust score
/// let score = TrustScore::new(0.8).unwrap();
/// assert_eq!(score.value(), 0.8);
///
/// // Convert to trust class
/// let class = score.to_class();
///
/// // Invalid scores are rejected
/// assert!(TrustScore::new(1.5).is_err());
/// assert!(TrustScore::new(-0.1).is_err());
/// assert!(TrustScore::new(f64::NAN).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrustScore(f64);

impl TrustScore {
    /// The minimum valid trust score
    pub const MIN: f64 = 0.0;
    /// The maximum valid trust score
    pub const MAX: f64 = 1.0;

    /// Threshold for Known trust class (>= 0.1)
    pub const KNOWN_THRESHOLD: f64 = 0.1;
    /// Threshold for Partner trust class (>= 0.4)
    pub const PARTNER_THRESHOLD: f64 = 0.4;
    /// Threshold for Federated trust class (>= 0.7)
    pub const FEDERATED_THRESHOLD: f64 = 0.7;

    /// Create a new trust score with validation
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `value` is less than 0.0
    /// - `value` is greater than 1.0
    /// - `value` is NaN
    ///
    /// # Examples
    ///
    /// ```
    /// use icn_trust::TrustScore;
    ///
    /// assert!(TrustScore::new(0.5).is_ok());
    /// assert!(TrustScore::new(1.5).is_err());
    /// assert!(TrustScore::new(-0.1).is_err());
    /// ```
    pub fn new(value: f64) -> Result<Self, TrustScoreError> {
        if value.is_nan() {
            Err(TrustScoreError::NaN)
        } else if !(Self::MIN..=Self::MAX).contains(&value) {
            Err(TrustScoreError::OutOfRange {
                value,
                min: Self::MIN,
                max: Self::MAX,
            })
        } else {
            Ok(Self(value))
        }
    }

    /// Create a trust score without validation (for constants and trusted sources)
    ///
    /// # Correctness requirements
    ///
    /// The caller must ensure the value is in range [0.0, 1.0] and not NaN.
    /// This is intended for use with constants where the value is known to be valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use icn_trust::TrustScore;
    ///
    /// const PARTNER_TRUST: TrustScore = TrustScore::unchecked(0.4);
    /// ```
    pub const fn unchecked(value: f64) -> Self {
        Self(value)
    }

    /// Get the inner f64 value
    ///
    /// # Examples
    ///
    /// ```
    /// use icn_trust::TrustScore;
    ///
    /// let score = TrustScore::new(0.7).unwrap();
    /// assert_eq!(score.value(), 0.7);
    /// ```
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Convert this score to a trust class
    ///
    /// # Examples
    ///
    /// ```
    /// use icn_trust::{TrustScore, TrustClass};
    ///
    /// let score = TrustScore::new(0.5).unwrap();
    /// assert_eq!(score.to_class(), TrustClass::Partner);
    /// ```
    pub fn to_class(&self) -> crate::TrustClass {
        crate::TrustClass::from_score(self.0)
    }

    /// Check if this score meets the isolated threshold (>= 0.0)
    pub fn is_isolated_or_better(&self) -> bool {
        self.0 >= Self::MIN
    }

    /// Check if this score meets the known threshold (>= 0.1)
    pub fn is_known_or_better(&self) -> bool {
        self.0 >= Self::KNOWN_THRESHOLD
    }

    /// Check if this score meets the partner threshold (>= 0.4)
    pub fn is_partner_or_better(&self) -> bool {
        self.0 >= Self::PARTNER_THRESHOLD
    }

    /// Check if this score meets the federated threshold (>= 0.7)
    pub fn is_federated(&self) -> bool {
        self.0 >= Self::FEDERATED_THRESHOLD
    }
}

impl fmt::Display for TrustScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

impl From<TrustScore> for f64 {
    fn from(score: TrustScore) -> Self {
        score.0
    }
}

impl TryFrom<f64> for TrustScore {
    type Error = TrustScoreError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl std::ops::Sub<f64> for TrustScore {
    type Output = f64;

    fn sub(self, rhs: f64) -> Self::Output {
        self.0 - rhs
    }
}

impl std::ops::Sub<TrustScore> for f64 {
    type Output = f64;

    fn sub(self, rhs: TrustScore) -> Self::Output {
        self - rhs.0
    }
}

impl std::ops::Sub<TrustScore> for TrustScore {
    type Output = f64;

    fn sub(self, rhs: TrustScore) -> Self::Output {
        self.0 - rhs.0
    }
}

impl std::ops::Mul<f64> for TrustScore {
    type Output = f64;

    fn mul(self, rhs: f64) -> Self::Output {
        self.0 * rhs
    }
}

impl std::ops::Mul<TrustScore> for f64 {
    type Output = f64;

    fn mul(self, rhs: TrustScore) -> Self::Output {
        self * rhs.0
    }
}

impl std::ops::Mul<TrustScore> for TrustScore {
    type Output = f64;

    fn mul(self, rhs: TrustScore) -> Self::Output {
        self.0 * rhs.0
    }
}

impl std::ops::Add<TrustScore> for f64 {
    type Output = f64;

    fn add(self, rhs: TrustScore) -> Self::Output {
        self + rhs.0
    }
}

impl std::ops::AddAssign<TrustScore> for f64 {
    fn add_assign(&mut self, rhs: TrustScore) {
        *self += rhs.0;
    }
}

impl PartialEq<TrustScore> for f64 {
    fn eq(&self, other: &TrustScore) -> bool {
        *self == other.0
    }
}

impl PartialEq<f64> for TrustScore {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<TrustScore> for f64 {
    fn partial_cmp(&self, other: &TrustScore) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&other.0)
    }
}

impl PartialOrd<f64> for TrustScore {
    fn partial_cmp(&self, other: &f64) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

// Safety: NaN is rejected at construction time, so TrustScore always contains
// a valid f64 that satisfies the requirements for Eq (reflexive equality).
impl Eq for TrustScore {}

// Safety: NaN is rejected at construction time, so TrustScore always contains
// a valid f64 that satisfies the requirements for Ord (total ordering).
impl Ord for TrustScore {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Since NaN is rejected at construction, partial_cmp will never return None.
        // We use total_cmp for a branchless comparison that handles all f64 edge cases.
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for TrustScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::iter::Sum<TrustScore> for f64 {
    fn sum<I: Iterator<Item = TrustScore>>(iter: I) -> Self {
        iter.map(|s| s.0).sum()
    }
}

impl Serialize for TrustScore {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for TrustScore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        TrustScore::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrustClass;

    #[test]
    fn test_trust_graph_type_all() {
        let all = TrustGraphType::all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], TrustGraphType::Social);
        assert_eq!(all[1], TrustGraphType::EconomicReliability);
        assert_eq!(all[2], TrustGraphType::TechnicalReliability);
    }

    #[test]
    fn test_trust_graph_type_storage_prefix() {
        assert_eq!(TrustGraphType::Social.storage_prefix(), "trust/social");
        assert_eq!(
            TrustGraphType::EconomicReliability.storage_prefix(),
            "trust/economic"
        );
        assert_eq!(
            TrustGraphType::TechnicalReliability.storage_prefix(),
            "trust/technical"
        );
    }

    #[test]
    fn test_trust_graph_type_default_weights() {
        let social = TrustGraphType::Social.default_weights();
        assert!((social.direct - 0.6).abs() < 0.001);
        assert!((social.transitive - 0.4).abs() < 0.001);

        let economic = TrustGraphType::EconomicReliability.default_weights();
        assert!((economic.direct - 0.8).abs() < 0.001);
        assert!((economic.transitive - 0.2).abs() < 0.001);

        let technical = TrustGraphType::TechnicalReliability.default_weights();
        assert!((technical.direct - 0.9).abs() < 0.001);
        assert!((technical.transitive - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_trust_graph_type_as_str() {
        assert_eq!(TrustGraphType::Social.as_str(), "social");
        assert_eq!(TrustGraphType::EconomicReliability.as_str(), "economic");
        assert_eq!(TrustGraphType::TechnicalReliability.as_str(), "technical");
    }

    #[test]
    fn test_trust_graph_type_display() {
        assert_eq!(format!("{}", TrustGraphType::Social), "social");
        assert_eq!(
            format!("{}", TrustGraphType::EconomicReliability),
            "economic"
        );
        assert_eq!(
            format!("{}", TrustGraphType::TechnicalReliability),
            "technical"
        );
    }

    #[test]
    fn test_trust_graph_type_default() {
        assert_eq!(TrustGraphType::default(), TrustGraphType::Social);
    }

    #[test]
    fn test_trust_graph_type_serialization() {
        let social = TrustGraphType::Social;
        let json = serde_json::to_string(&social).unwrap();
        assert_eq!(json, "\"social\"");

        let economic = TrustGraphType::EconomicReliability;
        let json = serde_json::to_string(&economic).unwrap();
        assert_eq!(json, "\"economic_reliability\"");

        let technical = TrustGraphType::TechnicalReliability;
        let json = serde_json::to_string(&technical).unwrap();
        assert_eq!(json, "\"technical_reliability\"");
    }

    #[test]
    fn test_trust_graph_type_deserialization() {
        let social: TrustGraphType = serde_json::from_str("\"social\"").unwrap();
        assert_eq!(social, TrustGraphType::Social);

        let economic: TrustGraphType = serde_json::from_str("\"economic_reliability\"").unwrap();
        assert_eq!(economic, TrustGraphType::EconomicReliability);

        let technical: TrustGraphType = serde_json::from_str("\"technical_reliability\"").unwrap();
        assert_eq!(technical, TrustGraphType::TechnicalReliability);
    }

    #[test]
    fn test_scoring_weights_new() {
        let weights = ScoringWeights::new(0.7, 0.3);
        assert!((weights.direct - 0.7).abs() < 0.001);
        assert!((weights.transitive - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_scoring_weights_legacy() {
        let weights = ScoringWeights::legacy();
        assert!((weights.direct - 0.7).abs() < 0.001);
        assert!((weights.transitive - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_scoring_weights_default() {
        let weights = ScoringWeights::default();
        assert!((weights.direct - 0.7).abs() < 0.001);
        assert!((weights.transitive - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_weights_sum_to_one() {
        for graph_type in TrustGraphType::all() {
            let weights = graph_type.default_weights();
            let sum = weights.direct + weights.transitive;
            assert!(
                (sum - 1.0).abs() < 0.001,
                "{graph_type:?} weights sum to {sum} instead of 1.0"
            );
        }
    }

    // TrustScore tests
    #[test]
    fn test_trust_score_valid_range() {
        // Valid scores
        assert!(TrustScore::new(0.0).is_ok());
        assert!(TrustScore::new(0.5).is_ok());
        assert!(TrustScore::new(1.0).is_ok());

        // Invalid scores
        assert!(TrustScore::new(-0.1).is_err());
        assert!(TrustScore::new(1.1).is_err());
        assert!(TrustScore::new(f64::NAN).is_err());
        assert!(TrustScore::new(f64::INFINITY).is_err());
        assert!(TrustScore::new(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn test_trust_score_value() {
        let score = TrustScore::new(0.7).unwrap();
        assert_eq!(score.value(), 0.7);
    }

    #[test]
    fn test_trust_score_to_class() {
        assert_eq!(
            TrustScore::new(0.0).unwrap().to_class(),
            TrustClass::Isolated
        );
        assert_eq!(
            TrustScore::new(0.05).unwrap().to_class(),
            TrustClass::Isolated
        );
        assert_eq!(TrustScore::new(0.2).unwrap().to_class(), TrustClass::Known);
        assert_eq!(
            TrustScore::new(0.5).unwrap().to_class(),
            TrustClass::Partner
        );
        assert_eq!(
            TrustScore::new(0.9).unwrap().to_class(),
            TrustClass::Federated
        );
    }

    #[test]
    fn test_trust_score_threshold_checks() {
        let isolated = TrustScore::new(0.0).unwrap();
        assert!(isolated.is_isolated_or_better());
        assert!(!isolated.is_known_or_better());
        assert!(!isolated.is_partner_or_better());
        assert!(!isolated.is_federated());

        let known = TrustScore::new(0.2).unwrap();
        assert!(known.is_isolated_or_better());
        assert!(known.is_known_or_better());
        assert!(!known.is_partner_or_better());
        assert!(!known.is_federated());

        let partner = TrustScore::new(0.5).unwrap();
        assert!(partner.is_isolated_or_better());
        assert!(partner.is_known_or_better());
        assert!(partner.is_partner_or_better());
        assert!(!partner.is_federated());

        let federated = TrustScore::new(0.9).unwrap();
        assert!(federated.is_isolated_or_better());
        assert!(federated.is_known_or_better());
        assert!(federated.is_partner_or_better());
        assert!(federated.is_federated());
    }

    #[test]
    fn test_trust_score_display() {
        let score = TrustScore::new(0.7).unwrap();
        assert_eq!(format!("{}", score), "0.70");

        let score = TrustScore::new(0.123).unwrap();
        assert_eq!(format!("{}", score), "0.12");
    }

    #[test]
    fn test_trust_score_from_into() {
        let score = TrustScore::new(0.8).unwrap();
        let val: f64 = score.into();
        assert_eq!(val, 0.8);
    }

    #[test]
    fn test_trust_score_try_from() {
        let result: Result<TrustScore, _> = 0.5.try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value(), 0.5);

        let result: Result<TrustScore, _> = 1.5.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_trust_score_serde() {
        let score = TrustScore::new(0.6).unwrap();
        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(json, "0.6");

        let deserialized: TrustScore = serde_json::from_str("0.6").unwrap();
        assert_eq!(deserialized.value(), 0.6);

        // Invalid values should fail deserialization
        let result: Result<TrustScore, _> = serde_json::from_str("1.5");
        assert!(result.is_err());
    }

    #[test]
    fn test_trust_score_ordering() {
        let low = TrustScore::new(0.3).unwrap();
        let high = TrustScore::new(0.7).unwrap();

        assert!(low < high);
        assert!(high > low);
        assert_eq!(low, low);
    }

    #[test]
    fn test_trust_score_unchecked() {
        const PARTNER_TRUST: TrustScore = TrustScore::unchecked(0.4);
        assert_eq!(PARTNER_TRUST.value(), 0.4);
    }
}
