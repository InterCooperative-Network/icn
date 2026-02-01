//! Shared trust computation algorithms
//!
//! This module contains the core trust score computation logic that is shared
//! between TrustGraph (actor-backed mode) and standalone implementations
//! (e.g., TrustPolicyOracle in gateway).
//!
//! ## Algorithm
//!
//! Trust scores are computed using a simplified PageRank-like algorithm:
//!
//! ```text
//! TrustScore(from -> to) =
//!     DirectTrust(from -> to) * direct_weight +
//!     TransitiveTrust(from -> intermediate -> to) * transitive_weight
//! ```
//!
//! Where:
//! - **Direct trust**: The trust edge directly from `from` to `to`
//! - **Transitive trust**: Average of weighted paths through intermediaries
//! - **Weights**: Configurable via `ScoringWeights` (must sum to 1.0)
//!
//! ## Transitive Computation
//!
//! Transitive trust is computed as the average of all weighted 2-hop paths:
//!
//! ```text
//! transitive_score = Σ(trust(from->intermediate) * trust(intermediate->to)) / count
//! ```
//!
//! This is a 1-hop transitive computation (2 edges total). Multi-hop algorithms
//! are not currently implemented to balance accuracy with computational cost.

use crate::types::ScoringWeights;

/// Compute trust score from direct and transitive components
///
/// This is the core trust computation algorithm used throughout ICN.
/// It combines direct trust (explicit edge) with transitive trust
/// (paths through intermediaries) using configurable weights.
///
/// # Arguments
///
/// * `direct_score` - Trust score from direct edge (0.0 if no edge exists)
/// * `intermediates` - Iterator of (intermediate_trust, indirect_trust) pairs
///   where:
///   - `intermediate_trust`: trust from source to intermediate
///   - `indirect_trust`: trust from intermediate to target
/// * `weights` - Weighting for direct vs transitive trust (must sum to 1.0)
///
/// # Returns
///
/// Trust score in range [0.0, 1.0]
///
/// # Examples
///
/// ```ignore
/// use icn_trust::{ScoringWeights, computation::compute_trust_score};
///
/// // Direct edge only
/// let score = compute_trust_score(
///     0.8,
///     std::iter::empty(),
///     ScoringWeights::legacy()
/// );
/// assert!((score - 0.56).abs() < 0.001); // 0.8 * 0.7 = 0.56
///
/// // With transitive paths
/// let intermediates = vec![
///     (0.9, 0.7), // Alice -> Bob -> Target
///     (0.8, 0.6), // Alice -> Carol -> Target
/// ];
/// let score = compute_trust_score(
///     0.0, // No direct edge
///     intermediates.into_iter(),
///     ScoringWeights::legacy()
/// );
/// // transitive = ((0.9*0.7) + (0.8*0.6)) / 2 = 0.555
/// // final = 0.0 * 0.7 + 0.555 * 0.3 = 0.1665
/// assert!((score - 0.1665).abs() < 0.001);
/// ```
pub fn compute_trust_score<I>(direct_score: f64, intermediates: I, weights: ScoringWeights) -> f64
where
    I: Iterator<Item = (f64, f64)>,
{
    // Compute transitive trust
    let mut transitive_sum = 0.0;
    let mut transitive_count = 0;

    for (intermediate_trust, indirect_trust) in intermediates {
        let weight = intermediate_trust * indirect_trust;
        transitive_sum += weight;
        transitive_count += 1;
    }

    let transitive_score = if transitive_count > 0 {
        transitive_sum / transitive_count as f64
    } else {
        0.0
    };

    // Combine using provided weights and clamp to [0.0, 1.0]
    (direct_score * weights.direct + transitive_score * weights.transitive).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_only() {
        let score = compute_trust_score(
            0.8,
            std::iter::empty(),
            ScoringWeights::legacy(), // 0.7/0.3
        );
        // 0.8 * 0.7 + 0.0 * 0.3 = 0.56
        assert!((score - 0.56).abs() < 0.001);
    }

    #[test]
    fn test_transitive_only() {
        let intermediates = vec![
            (0.9, 0.7), // 0.63
            (0.8, 0.6), // 0.48
        ];
        let score = compute_trust_score(
            0.0, // No direct edge
            intermediates.into_iter(),
            ScoringWeights::legacy(), // 0.7/0.3
        );
        // transitive = (0.63 + 0.48) / 2 = 0.555
        // final = 0.0 * 0.7 + 0.555 * 0.3 = 0.1665
        assert!((score - 0.1665).abs() < 0.001);
    }

    #[test]
    fn test_combined() {
        let intermediates = vec![(0.8, 0.6)];
        let score = compute_trust_score(
            0.6, // Direct edge
            intermediates.into_iter(),
            ScoringWeights::legacy(), // 0.7/0.3
        );
        // direct = 0.6 * 0.7 = 0.42
        // transitive = 0.48 * 0.3 = 0.144
        // final = 0.42 + 0.144 = 0.564
        assert!((score - 0.564).abs() < 0.001);
    }

    #[test]
    fn test_custom_weights() {
        let weights = ScoringWeights::new(0.8, 0.2);
        let intermediates = vec![(0.9, 0.7)];
        let score = compute_trust_score(0.5, intermediates.into_iter(), weights);
        // direct = 0.5 * 0.8 = 0.4
        // transitive = 0.63 * 0.2 = 0.126
        // final = 0.4 + 0.126 = 0.526
        assert!((score - 0.526).abs() < 0.001);
    }

    #[test]
    fn test_clamped_to_one() {
        // If computation exceeds 1.0, it should be clamped
        let intermediates = vec![(1.0, 1.0)];
        let score = compute_trust_score(1.0, intermediates.into_iter(), ScoringWeights::legacy());
        // direct = 1.0 * 0.7 = 0.7
        // transitive = 1.0 * 0.3 = 0.3
        // final = 0.7 + 0.3 = 1.0 (already at limit)
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_zero_score() {
        let score = compute_trust_score(0.0, std::iter::empty(), ScoringWeights::legacy());
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_single_intermediate() {
        let intermediates = vec![(0.8, 0.6)];
        let score = compute_trust_score(0.0, intermediates.into_iter(), ScoringWeights::legacy());
        // transitive = 0.48
        // final = 0.0 * 0.7 + 0.48 * 0.3 = 0.144
        assert!((score - 0.144).abs() < 0.001);
    }

    #[test]
    fn test_multiple_intermediates_averaging() {
        let intermediates = vec![
            (1.0, 1.0), // Weight: 1.0
            (0.5, 0.5), // Weight: 0.25
            (0.6, 0.6), // Weight: 0.36
        ];
        let score = compute_trust_score(0.0, intermediates.into_iter(), ScoringWeights::legacy());
        // transitive = (1.0 + 0.25 + 0.36) / 3 = 0.5366...
        // final = 0.0 * 0.7 + 0.5366 * 0.3 = 0.161
        assert!((score - 0.161).abs() < 0.001);
    }
}
