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

#[cfg(test)]
mod tests {
    use super::*;

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
}
