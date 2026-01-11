//! Shamir's Secret Sharing implementation
//!
//! This module provides a k-of-n threshold secret sharing scheme based on
//! Shamir's Secret Sharing over GF(256). It allows splitting a secret into
//! n shares where any k shares can reconstruct the original secret.
//!
//! # Security Properties
//!
//! - **Information-theoretic security**: With fewer than k shares, no information
//!   about the secret is revealed (assuming random coefficients).
//! - **Perfect secrecy**: k-1 shares give zero information about the secret.
//! - **Fault tolerance**: Any k of n shares suffice for reconstruction.
//!
//! # Implementation Details
//!
//! Uses GF(256) (the Galois Field with 256 elements) with the AES/Rijndael
//! irreducible polynomial: x^8 + x^4 + x^3 + x + 1 (0x11B).
//!
//! Each byte of the secret is independently split using Shamir's scheme.
//! This approach is standard and used in production systems like SLIP-0039.
//!
//! # Example
//!
//! ```rust
//! use icn_crypto_pq::shamir::ShamirSecretSharing;
//!
//! // Split a 32-byte secret with 3-of-5 threshold
//! let secret = [42u8; 32];
//! let result = ShamirSecretSharing::split(&secret, 3, 5).unwrap();
//!
//! // Any 3 shares can reconstruct
//! let reconstructed = ShamirSecretSharing::reconstruct(&result.shares[0..3]).unwrap();
//! assert_eq!(reconstructed, secret);
//!
//! // Different subset also works
//! let reconstructed2 = ShamirSecretSharing::reconstruct(&[
//!     result.shares[1].clone(),
//!     result.shares[3].clone(),
//!     result.shares[4].clone(),
//! ]).unwrap();
//! assert_eq!(reconstructed2, secret);
//! ```
//!
//! # References
//!
//! - Shamir, Adi. "How to share a secret." Communications of the ACM 22.11 (1979): 612-613.
//! - SLIP-0039: Shamir's Secret-Sharing for Mnemonic Codes

use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, Result};

// =============================================================================
// GF(256) Field Arithmetic
// =============================================================================

/// GF(256) using the AES irreducible polynomial: x^8 + x^4 + x^3 + x + 1
mod gf256 {
    /// Logarithm table for GF(256) multiplication
    /// LOG[x] = discrete log base 3 of x (for x != 0)
    const LOG: [u8; 256] = [
        0x00, 0x00, 0x19, 0x01, 0x32, 0x02, 0x1a, 0xc6, 0x4b, 0xc7, 0x1b, 0x68, 0x33, 0xee, 0xdf,
        0x03, 0x64, 0x04, 0xe0, 0x0e, 0x34, 0x8d, 0x81, 0xef, 0x4c, 0x71, 0x08, 0xc8, 0xf8, 0x69,
        0x1c, 0xc1, 0x7d, 0xc2, 0x1d, 0xb5, 0xf9, 0xb9, 0x27, 0x6a, 0x4d, 0xe4, 0xa6, 0x72, 0x9a,
        0xc9, 0x09, 0x78, 0x65, 0x2f, 0x8a, 0x05, 0x21, 0x0f, 0xe1, 0x24, 0x12, 0xf0, 0x82, 0x45,
        0x35, 0x93, 0xda, 0x8e, 0x96, 0x8f, 0xdb, 0xbd, 0x36, 0xd0, 0xce, 0x94, 0x13, 0x5c, 0xd2,
        0xf1, 0x40, 0x46, 0x83, 0x38, 0x66, 0xdd, 0xfd, 0x30, 0xbf, 0x06, 0x8b, 0x62, 0xb3, 0x25,
        0xe2, 0x98, 0x22, 0x88, 0x91, 0x10, 0x7e, 0x6e, 0x48, 0xc3, 0xa3, 0xb6, 0x1e, 0x42, 0x3a,
        0x6b, 0x28, 0x54, 0xfa, 0x85, 0x3d, 0xba, 0x2b, 0x79, 0x0a, 0x15, 0x9b, 0x9f, 0x5e, 0xca,
        0x4e, 0xd4, 0xac, 0xe5, 0xf3, 0x73, 0xa7, 0x57, 0xaf, 0x58, 0xa8, 0x50, 0xf4, 0xea, 0xd6,
        0x74, 0x4f, 0xae, 0xe9, 0xd5, 0xe7, 0xe6, 0xad, 0xe8, 0x2c, 0xd7, 0x75, 0x7a, 0xeb, 0x16,
        0x0b, 0xf5, 0x59, 0xcb, 0x5f, 0xb0, 0x9c, 0xa9, 0x51, 0xa0, 0x7f, 0x0c, 0xf6, 0x6f, 0x17,
        0xc4, 0x49, 0xec, 0xd8, 0x43, 0x1f, 0x2d, 0xa4, 0x76, 0x7b, 0xb7, 0xcc, 0xbb, 0x3e, 0x5a,
        0xfb, 0x60, 0xb1, 0x86, 0x3b, 0x52, 0xa1, 0x6c, 0xaa, 0x55, 0x29, 0x9d, 0x97, 0xb2, 0x87,
        0x90, 0x61, 0xbe, 0xdc, 0xfc, 0xbc, 0x95, 0xcf, 0xcd, 0x37, 0x3f, 0x5b, 0xd1, 0x53, 0x39,
        0x84, 0x3c, 0x41, 0xa2, 0x6d, 0x47, 0x14, 0x2a, 0x9e, 0x5d, 0x56, 0xf2, 0xd3, 0xab, 0x44,
        0x11, 0x92, 0xd9, 0x23, 0x20, 0x2e, 0x89, 0xb4, 0x7c, 0xb8, 0x26, 0x77, 0x99, 0xe3, 0xa5,
        0x67, 0x4a, 0xed, 0xde, 0xc5, 0x31, 0xfe, 0x18, 0x0d, 0x63, 0x8c, 0x80, 0xc0, 0xf7, 0x70,
        0x07,
    ];

    /// Exponentiation table for GF(256)
    /// EXP[x] = 3^x mod the irreducible polynomial
    const EXP: [u8; 256] = [
        0x01, 0x03, 0x05, 0x0f, 0x11, 0x33, 0x55, 0xff, 0x1a, 0x2e, 0x72, 0x96, 0xa1, 0xf8, 0x13,
        0x35, 0x5f, 0xe1, 0x38, 0x48, 0xd8, 0x73, 0x95, 0xa4, 0xf7, 0x02, 0x06, 0x0a, 0x1e, 0x22,
        0x66, 0xaa, 0xe5, 0x34, 0x5c, 0xe4, 0x37, 0x59, 0xeb, 0x26, 0x6a, 0xbe, 0xd9, 0x70, 0x90,
        0xab, 0xe6, 0x31, 0x53, 0xf5, 0x04, 0x0c, 0x14, 0x3c, 0x44, 0xcc, 0x4f, 0xd1, 0x68, 0xb8,
        0xd3, 0x6e, 0xb2, 0xcd, 0x4c, 0xd4, 0x67, 0xa9, 0xe0, 0x3b, 0x4d, 0xd7, 0x62, 0xa6, 0xf1,
        0x08, 0x18, 0x28, 0x78, 0x88, 0x83, 0x9e, 0xb9, 0xd0, 0x6b, 0xbd, 0xdc, 0x7f, 0x81, 0x98,
        0xb3, 0xce, 0x49, 0xdb, 0x76, 0x9a, 0xb5, 0xc4, 0x57, 0xf9, 0x10, 0x30, 0x50, 0xf0, 0x0b,
        0x1d, 0x27, 0x69, 0xbb, 0xd6, 0x61, 0xa3, 0xfe, 0x19, 0x2b, 0x7d, 0x87, 0x92, 0xad, 0xec,
        0x2f, 0x71, 0x93, 0xae, 0xe9, 0x20, 0x60, 0xa0, 0xfb, 0x16, 0x3a, 0x4e, 0xd2, 0x6d, 0xb7,
        0xc2, 0x5d, 0xe7, 0x32, 0x56, 0xfa, 0x15, 0x3f, 0x41, 0xc3, 0x5e, 0xe2, 0x3d, 0x47, 0xc9,
        0x40, 0xc0, 0x5b, 0xed, 0x2c, 0x74, 0x9c, 0xbf, 0xda, 0x75, 0x9f, 0xba, 0xd5, 0x64, 0xac,
        0xef, 0x2a, 0x7e, 0x82, 0x9d, 0xbc, 0xdf, 0x7a, 0x8e, 0x89, 0x80, 0x9b, 0xb6, 0xc1, 0x58,
        0xe8, 0x23, 0x65, 0xaf, 0xea, 0x25, 0x6f, 0xb1, 0xc8, 0x43, 0xc5, 0x54, 0xfc, 0x1f, 0x21,
        0x63, 0xa5, 0xf4, 0x07, 0x09, 0x1b, 0x2d, 0x77, 0x99, 0xb0, 0xcb, 0x46, 0xca, 0x45, 0xcf,
        0x4a, 0xde, 0x79, 0x8b, 0x86, 0x91, 0xa8, 0xe3, 0x3e, 0x42, 0xc6, 0x51, 0xf3, 0x0e, 0x12,
        0x36, 0x5a, 0xee, 0x29, 0x7b, 0x8d, 0x8c, 0x8f, 0x8a, 0x85, 0x94, 0xa7, 0xf2, 0x0d, 0x17,
        0x39, 0x4b, 0xdd, 0x7c, 0x84, 0x97, 0xa2, 0xfd, 0x1c, 0x24, 0x6c, 0xb4, 0xc7, 0x52, 0xf6,
        0x01,
    ];

    /// Addition in GF(256) is XOR
    #[inline]
    pub fn add(a: u8, b: u8) -> u8 {
        a ^ b
    }

    /// Subtraction in GF(256) is the same as addition (XOR)
    #[inline]
    pub fn sub(a: u8, b: u8) -> u8 {
        a ^ b
    }

    /// Multiplication in GF(256) using log/exp tables
    #[inline]
    pub fn mul(a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        let log_a = LOG[a as usize] as usize;
        let log_b = LOG[b as usize] as usize;
        let log_result = (log_a + log_b) % 255;
        EXP[log_result]
    }

    /// Division in GF(256): a / b = a * b^(-1)
    #[inline]
    pub fn div(a: u8, b: u8) -> u8 {
        debug_assert!(b != 0, "Division by zero in GF(256)");
        if a == 0 {
            return 0;
        }
        let log_a = LOG[a as usize] as usize;
        let log_b = LOG[b as usize] as usize;
        // (log_a - log_b) mod 255, handling underflow
        let log_result = (log_a + 255 - log_b) % 255;
        EXP[log_result]
    }

    /// Evaluate polynomial at x in GF(256)
    /// coefficients[0] is the constant term (the secret)
    pub fn eval_polynomial(coefficients: &[u8], x: u8) -> u8 {
        if x == 0 {
            return coefficients[0];
        }

        // Horner's method for efficient evaluation
        let mut result = 0u8;
        for &coeff in coefficients.iter().rev() {
            result = add(mul(result, x), coeff);
        }
        result
    }

    /// Lagrange interpolation to find f(0) given points
    ///
    /// Points are (x_i, y_i) pairs where x_i are the share indices.
    ///
    /// # Panics
    ///
    /// Debug builds will panic if duplicate x-coordinates are present,
    /// as this would cause division by zero.
    pub fn interpolate_at_zero(points: &[(u8, u8)]) -> u8 {
        let mut result = 0u8;

        for (i, &(x_i, y_i)) in points.iter().enumerate() {
            // Calculate Lagrange basis polynomial L_i(0)
            // L_i(0) = product over j != i of (0 - x_j) / (x_i - x_j)
            //        = product over j != i of x_j / (x_i - x_j)
            let mut numerator = 1u8;
            let mut denominator = 1u8;

            for (j, &(x_j, _)) in points.iter().enumerate() {
                if i != j {
                    // Check for duplicate indices which would cause division by zero
                    debug_assert!(
                        x_i != x_j,
                        "Duplicate x-coordinates in interpolation: x[{i}] = x[{j}] = {x_i}"
                    );
                    // numerator *= x_j (since we're evaluating at 0)
                    numerator = mul(numerator, x_j);
                    // denominator *= (x_i - x_j) = (x_i XOR x_j) in GF(256)
                    denominator = mul(denominator, sub(x_i, x_j));
                }
            }

            // Add y_i * L_i(0) to result
            let basis = div(numerator, denominator);
            result = add(result, mul(y_i, basis));
        }

        result
    }
}

// =============================================================================
// Shamir Secret Sharing Types
// =============================================================================

/// A share from Shamir's Secret Sharing scheme
///
/// Each share contains an index (x-coordinate) and value (y-coordinate).
/// The index must be non-zero and unique among shares.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ShamirShare {
    /// Share index (1-255, non-zero for GF(256))
    #[zeroize(skip)]
    pub index: u8,
    /// The share value (32 bytes, one for each byte of the secret)
    pub value: [u8; 32],
}

impl ShamirShare {
    /// Create a new Shamir share
    pub fn new(index: u8, value: [u8; 32]) -> Self {
        Self { index, value }
    }

    /// Get the share index
    pub fn index(&self) -> u8 {
        self.index
    }

    /// Get the share value
    pub fn value(&self) -> &[u8; 32] {
        &self.value
    }
}

impl std::fmt::Debug for ShamirShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShamirShare")
            .field("index", &self.index)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

/// Result of splitting a secret with Shamir's Secret Sharing
pub struct ShamirSplitResult {
    /// The threshold (k) - minimum shares needed to reconstruct
    pub threshold: u8,
    /// Total number of shares (n)
    pub total: u8,
    /// The generated shares
    pub shares: Vec<ShamirShare>,
}

/// Shamir's Secret Sharing implementation
///
/// Provides k-of-n threshold secret sharing where:
/// - k is the threshold (minimum shares needed)
/// - n is the total number of shares
/// - Any k shares can reconstruct the secret
/// - k-1 or fewer shares reveal nothing about the secret
pub struct ShamirSecretSharing;

impl ShamirSecretSharing {
    /// Maximum number of shares (limited by GF(256) - need non-zero indices)
    pub const MAX_SHARES: u8 = 255;

    /// Split a 32-byte secret into n shares with threshold k
    ///
    /// # Arguments
    ///
    /// * `secret` - The 32-byte secret to split
    /// * `threshold` - Minimum shares needed to reconstruct (k)
    /// * `total` - Total number of shares to generate (n)
    ///
    /// # Returns
    ///
    /// A `ShamirSplitResult` containing the shares and parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - threshold < 2 (need at least 2 for meaningful threshold)
    /// - total < threshold (can't have fewer shares than threshold)
    /// - total > 255 (GF(256) limitation)
    ///
    /// # Security
    ///
    /// Uses `OsRng` for cryptographically secure random coefficient generation.
    pub fn split(secret: &[u8; 32], threshold: u8, total: u8) -> Result<ShamirSplitResult> {
        // Validate parameters
        if threshold < 2 {
            return Err(CryptoError::InvalidKey(
                "Threshold must be at least 2".to_string(),
            ));
        }
        if total < threshold {
            return Err(CryptoError::InvalidKey(format!(
                "Total shares ({total}) must be >= threshold ({threshold})"
            )));
        }
        // Note: total is u8 so it can't exceed MAX_SHARES (255).
        // The type system enforces this constraint.

        let mut shares: Vec<ShamirShare> = (1..=total)
            .map(|idx| ShamirShare::new(idx, [0u8; 32]))
            .collect();
        let mut rng = rand::rngs::OsRng;

        // For each byte of the secret, generate ONE polynomial and evaluate at each share index
        for (byte_idx, &secret_byte) in secret.iter().enumerate() {
            // Generate random polynomial coefficients (same for all shares of this byte)
            // coefficients[0] = secret_byte (the secret is the constant term)
            // coefficients[1..threshold] = random
            let mut coefficients = vec![0u8; threshold as usize];
            coefficients[0] = secret_byte;

            // Generate random coefficients for degree 1 to threshold-1
            let mut random_bytes = vec![0u8; (threshold - 1) as usize];
            rng.fill_bytes(&mut random_bytes);
            coefficients[1..].copy_from_slice(&random_bytes);

            // Evaluate the SAME polynomial at each share index
            for (share_idx, share) in shares.iter_mut().enumerate() {
                let x = (share_idx + 1) as u8; // 1-indexed
                share.value[byte_idx] = gf256::eval_polynomial(&coefficients, x);
            }
        }

        Ok(ShamirSplitResult {
            threshold,
            total,
            shares,
        })
    }

    /// Reconstruct a secret from k or more shares
    ///
    /// # Arguments
    ///
    /// * `shares` - At least k shares from the original split
    ///
    /// # Returns
    ///
    /// The reconstructed 32-byte secret.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No shares provided
    /// - Duplicate share indices exist
    ///
    /// # Note
    ///
    /// This function does not verify that enough shares were provided.
    /// If fewer than k shares are given, the result will be incorrect
    /// (but won't error). The caller must ensure sufficient shares.
    pub fn reconstruct(shares: &[ShamirShare]) -> Result<[u8; 32]> {
        if shares.is_empty() {
            return Err(CryptoError::InvalidKey(
                "At least one share is required".to_string(),
            ));
        }

        // Check for duplicate indices
        let mut seen_indices = std::collections::HashSet::new();
        for share in shares {
            if share.index == 0 {
                return Err(CryptoError::InvalidKey(
                    "Share index must be non-zero".to_string(),
                ));
            }
            if !seen_indices.insert(share.index) {
                return Err(CryptoError::InvalidKey(format!(
                    "Duplicate share index: {}",
                    share.index
                )));
            }
        }

        let mut secret = [0u8; 32];

        // Reconstruct each byte independently
        for (byte_idx, secret_byte) in secret.iter_mut().enumerate() {
            // Collect (x, y) points for this byte position
            let points: Vec<(u8, u8)> = shares
                .iter()
                .map(|s| (s.index, s.value[byte_idx]))
                .collect();

            // Lagrange interpolation at x=0 gives us the secret byte
            *secret_byte = gf256::interpolate_at_zero(&points);
        }

        Ok(secret)
    }

    /// Verify that a set of shares is consistent
    ///
    /// Checks if shares could have come from the same secret by verifying
    /// that subsets reconstruct to the same value.
    ///
    /// # Arguments
    ///
    /// * `shares` - The shares to verify
    /// * `threshold` - The threshold (k) used during splitting
    ///
    /// # Returns
    ///
    /// `true` if the checked shares are consistent, `false` otherwise.
    ///
    /// # Limitations
    ///
    /// This function only checks a subset of possible combinations to avoid
    /// exponential complexity. Specifically, it verifies that:
    /// - The first k shares reconstruct correctly
    /// - The last k shares produce the same secret as the first k
    ///
    /// This means tampering in middle shares may not be detected if those
    /// shares are not included in the first or last k. For comprehensive
    /// verification, callers should check multiple random subsets or use
    /// verifiable secret sharing (VSS) schemes.
    pub fn verify_consistency(shares: &[ShamirShare], threshold: u8) -> bool {
        if shares.len() < threshold as usize {
            return true; // Can't verify with fewer than threshold shares
        }

        // Reconstruct with first k shares
        let first_k: Vec<ShamirShare> = shares.iter().take(threshold as usize).cloned().collect();

        let Ok(first_secret) = Self::reconstruct(&first_k) else {
            return false;
        };

        // Verify other possible k-subsets give same result
        // (Only check a few to avoid exponential blowup)
        if shares.len() > threshold as usize {
            // Try with last k shares
            let last_k: Vec<ShamirShare> = shares
                .iter()
                .rev()
                .take(threshold as usize)
                .cloned()
                .collect();

            if let Ok(last_secret) = Self::reconstruct(&last_k) {
                if first_secret != last_secret {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

// =============================================================================
// Integration with existing PepperShare type
// =============================================================================

impl From<ShamirShare> for crate::threshold::PepperShare {
    fn from(share: ShamirShare) -> Self {
        crate::threshold::PepperShare::new(share.index, share.value)
    }
}

impl From<crate::threshold::PepperShare> for ShamirShare {
    fn from(share: crate::threshold::PepperShare) -> Self {
        ShamirShare::new(share.index, share.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gf256_addition() {
        // Addition is XOR
        assert_eq!(gf256::add(0, 0), 0);
        assert_eq!(gf256::add(1, 1), 0);
        assert_eq!(gf256::add(0x12, 0x34), 0x26);
        assert_eq!(gf256::add(0xff, 0xff), 0);
    }

    #[test]
    fn test_gf256_multiplication() {
        // Multiplication properties
        assert_eq!(gf256::mul(0, 5), 0); // 0 * x = 0
        assert_eq!(gf256::mul(1, 5), 5); // 1 * x = x
        assert_eq!(gf256::mul(5, 1), 5); // x * 1 = x

        // Commutativity
        assert_eq!(gf256::mul(3, 7), gf256::mul(7, 3));

        // Known values from AES GF(256)
        assert_eq!(gf256::mul(0x53, 0xca), 0x01); // multiplicative inverse pair
    }

    #[test]
    fn test_gf256_division() {
        // Division by 1
        assert_eq!(gf256::div(5, 1), 5);

        // a / a = 1 for a != 0
        for a in 1u8..=255 {
            assert_eq!(gf256::div(a, a), 1);
        }

        // (a * b) / b = a
        assert_eq!(gf256::div(gf256::mul(7, 13), 13), 7);
    }

    #[test]
    fn test_polynomial_evaluation() {
        // f(x) = 5 (constant polynomial)
        assert_eq!(gf256::eval_polynomial(&[5], 0), 5);
        assert_eq!(gf256::eval_polynomial(&[5], 1), 5);
        assert_eq!(gf256::eval_polynomial(&[5], 7), 5);

        // f(x) = 3 + 2x (linear polynomial)
        // f(0) = 3, f(1) = 3 XOR 2 = 1
        let coeffs = [3, 2];
        assert_eq!(gf256::eval_polynomial(&coeffs, 0), 3);
        assert_eq!(gf256::eval_polynomial(&coeffs, 1), gf256::add(3, 2));
    }

    #[test]
    fn test_lagrange_interpolation() {
        // Test with a simple case: f(x) = 42 (constant)
        // Points: (1, 42), (2, 42)
        let points = vec![(1, 42), (2, 42)];
        assert_eq!(gf256::interpolate_at_zero(&points), 42);
    }

    #[test]
    fn test_split_basic() {
        let secret = [42u8; 32];
        let result = ShamirSecretSharing::split(&secret, 2, 3).unwrap();

        assert_eq!(result.threshold, 2);
        assert_eq!(result.total, 3);
        assert_eq!(result.shares.len(), 3);

        // Each share should have unique index 1, 2, 3
        assert_eq!(result.shares[0].index, 1);
        assert_eq!(result.shares[1].index, 2);
        assert_eq!(result.shares[2].index, 3);
    }

    #[test]
    fn test_split_reconstruct_2_of_3() {
        let secret = [42u8; 32];
        let result = ShamirSecretSharing::split(&secret, 2, 3).unwrap();

        // Any 2 shares should reconstruct
        let r1 =
            ShamirSecretSharing::reconstruct(&[result.shares[0].clone(), result.shares[1].clone()])
                .unwrap();
        let r2 =
            ShamirSecretSharing::reconstruct(&[result.shares[0].clone(), result.shares[2].clone()])
                .unwrap();
        let r3 =
            ShamirSecretSharing::reconstruct(&[result.shares[1].clone(), result.shares[2].clone()])
                .unwrap();

        assert_eq!(r1, secret);
        assert_eq!(r2, secret);
        assert_eq!(r3, secret);

        // All 3 shares should also work
        let r_all = ShamirSecretSharing::reconstruct(&result.shares).unwrap();
        assert_eq!(r_all, secret);
    }

    #[test]
    fn test_split_reconstruct_3_of_5() {
        let secret: [u8; 32] = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
            0xcc, 0xdd, 0xee, 0xff,
        ];

        let result = ShamirSecretSharing::split(&secret, 3, 5).unwrap();
        assert_eq!(result.shares.len(), 5);

        // Try different combinations of 3 shares
        let r1 = ShamirSecretSharing::reconstruct(&[
            result.shares[0].clone(),
            result.shares[1].clone(),
            result.shares[2].clone(),
        ])
        .unwrap();

        let r2 = ShamirSecretSharing::reconstruct(&[
            result.shares[0].clone(),
            result.shares[2].clone(),
            result.shares[4].clone(),
        ])
        .unwrap();

        let r3 = ShamirSecretSharing::reconstruct(&[
            result.shares[1].clone(),
            result.shares[3].clone(),
            result.shares[4].clone(),
        ])
        .unwrap();

        assert_eq!(r1, secret);
        assert_eq!(r2, secret);
        assert_eq!(r3, secret);
    }

    #[test]
    fn test_insufficient_shares_wrong_result() {
        let secret = [42u8; 32];
        let result = ShamirSecretSharing::split(&secret, 3, 5).unwrap();

        // Only 2 shares when threshold is 3 - should give wrong result
        let wrong =
            ShamirSecretSharing::reconstruct(&[result.shares[0].clone(), result.shares[1].clone()])
                .unwrap();

        // The result should NOT match the secret (with overwhelming probability)
        assert_ne!(wrong, secret);
    }

    #[test]
    fn test_parameter_validation() {
        let secret = [42u8; 32];

        // Threshold < 2
        assert!(ShamirSecretSharing::split(&secret, 1, 3).is_err());

        // Total < threshold
        assert!(ShamirSecretSharing::split(&secret, 5, 3).is_err());

        // Works with edge cases
        assert!(ShamirSecretSharing::split(&secret, 2, 2).is_ok());
        assert!(ShamirSecretSharing::split(&secret, 255, 255).is_ok());
    }

    #[test]
    fn test_duplicate_index_detection() {
        let share1 = ShamirShare::new(1, [1u8; 32]);
        let share2 = ShamirShare::new(1, [2u8; 32]); // Duplicate index!

        let result = ShamirSecretSharing::reconstruct(&[share1, share2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_index_rejection() {
        let share = ShamirShare::new(0, [1u8; 32]); // Invalid index!
        let result = ShamirSecretSharing::reconstruct(&[share]);
        assert!(result.is_err());
    }

    #[test]
    fn test_consistency_verification() {
        let secret = [42u8; 32];
        let result = ShamirSecretSharing::split(&secret, 3, 5).unwrap();

        // Valid shares should be consistent
        assert!(ShamirSecretSharing::verify_consistency(
            &result.shares,
            result.threshold
        ));

        // Tamper with the first share (which is always in the "first k" subset)
        // This should definitely be detected
        let mut tampered_first = result.shares.clone();
        tampered_first[0].value[0] ^= 0xFF;
        assert!(
            !ShamirSecretSharing::verify_consistency(&tampered_first, result.threshold),
            "Tampering with first share should be detected"
        );

        // Tamper with the last share (which is always in the "last k" subset)
        // This should definitely be detected
        let mut tampered_last = result.shares.clone();
        tampered_last[4].value[0] ^= 0xFF;
        assert!(
            !ShamirSecretSharing::verify_consistency(&tampered_last, result.threshold),
            "Tampering with last share should be detected"
        );
    }

    #[test]
    fn test_consistency_limitation_documented() {
        // This test documents the known limitation of verify_consistency:
        // tampering in middle shares may not be detected if they're not
        // in the first k or last k shares.
        let secret = [42u8; 32];
        let result = ShamirSecretSharing::split(&secret, 3, 5).unwrap();

        // Tamper with middle share (index 2, which is share 3)
        // With 3-of-5 threshold:
        // - First 3 shares: indices 0, 1, 2 (includes tampered share)
        // - Last 3 shares: indices 4, 3, 2 (includes tampered share)
        // So this WILL be detected because share 2 is in both subsets
        let mut tampered_middle = result.shares.clone();
        tampered_middle[2].value[0] ^= 0xFF;
        // The tampered share IS checked, so inconsistency is detected
        assert!(
            !ShamirSecretSharing::verify_consistency(&tampered_middle, result.threshold),
            "Tampering with middle share (in both subsets) should be detected"
        );
    }

    #[test]
    fn test_randomness_different_splits() {
        let secret = [42u8; 32];

        let result1 = ShamirSecretSharing::split(&secret, 2, 3).unwrap();
        let result2 = ShamirSecretSharing::split(&secret, 2, 3).unwrap();

        // Shares should be different (different random coefficients)
        let different = result1
            .shares
            .iter()
            .zip(result2.shares.iter())
            .any(|(a, b)| a.value != b.value);
        assert!(different, "Two splits should produce different shares");

        // But both should reconstruct to the same secret
        let r1 = ShamirSecretSharing::reconstruct(&result1.shares).unwrap();
        let r2 = ShamirSecretSharing::reconstruct(&result2.shares).unwrap();
        assert_eq!(r1, secret);
        assert_eq!(r2, secret);
    }

    #[test]
    fn test_pepper_share_conversion() {
        let shamir_share = ShamirShare::new(5, [0xAB; 32]);

        // Convert to PepperShare
        let pepper_share: crate::threshold::PepperShare = shamir_share.clone().into();
        assert_eq!(pepper_share.index, 5);
        assert_eq!(pepper_share.value, [0xAB; 32]);

        // Convert back
        let back: ShamirShare = pepper_share.into();
        assert_eq!(back.index, 5);
        assert_eq!(back.value, [0xAB; 32]);
    }

    #[test]
    fn test_all_zeros_secret() {
        let secret = [0u8; 32];
        let result = ShamirSecretSharing::split(&secret, 2, 3).unwrap();
        let reconstructed = ShamirSecretSharing::reconstruct(&result.shares).unwrap();
        assert_eq!(reconstructed, secret);
    }

    #[test]
    fn test_all_ones_secret() {
        let secret = [0xFF; 32];
        let result = ShamirSecretSharing::split(&secret, 2, 3).unwrap();
        let reconstructed = ShamirSecretSharing::reconstruct(&result.shares).unwrap();
        assert_eq!(reconstructed, secret);
    }

    #[test]
    fn test_high_threshold() {
        let secret = [0x42; 32];
        // 10-of-10 threshold
        let result = ShamirSecretSharing::split(&secret, 10, 10).unwrap();
        let reconstructed = ShamirSecretSharing::reconstruct(&result.shares).unwrap();
        assert_eq!(reconstructed, secret);
    }
}
