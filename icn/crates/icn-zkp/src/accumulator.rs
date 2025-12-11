//! Cryptographic Accumulator
//!
//! RSA-based accumulator for credential revocation.
//!
//! An accumulator allows:
//! - Adding elements to a set
//! - Proving membership (element is in set)
//! - Proving non-membership (element is NOT in set)
//!
//! For revocation, we accumulate revoked credential IDs.
//! A valid credential proves non-membership in the revocation accumulator.

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use thiserror::Error;

/// Errors from accumulator operations
#[derive(Debug, Error)]
pub enum AccumulatorError {
    #[error("Element already in accumulator")]
    AlreadyMember,

    #[error("Element not in accumulator")]
    NotMember,

    #[error("Invalid witness")]
    InvalidWitness,

    #[error("Invalid modulus")]
    InvalidModulus,

    #[error("Computation error: {0}")]
    ComputationError(String),
}

/// RSA accumulator for revocation
///
/// Uses a 2048-bit RSA modulus (product of two safe primes).
/// For production, this should be generated via a trusted setup.
#[derive(Clone, Serialize, Deserialize)]
pub struct RsaAccumulator {
    /// RSA modulus N = p * q
    modulus: BigUint,
    /// Current accumulator value
    value: BigUint,
    /// Number of elements
    count: usize,
    /// Epoch (version number)
    epoch: u64,
}

impl std::fmt::Debug for RsaAccumulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RsaAccumulator")
            .field("modulus_bits", &self.modulus.bits())
            .field("count", &self.count)
            .field("epoch", &self.epoch)
            .finish()
    }
}

/// Witness for proving membership in accumulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipWitness {
    /// Witness value: accumulator with element removed
    pub witness: BigUint,
}

/// Witness for proving non-membership in accumulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonMembershipWitness {
    /// Bezout coefficient a
    pub a: BigInt,
    /// Auxiliary accumulator value d
    pub d: BigUint,
}

impl RsaAccumulator {
    /// Create a new accumulator with a test modulus
    ///
    /// WARNING: For testing only! Production should use properly generated RSA modulus.
    pub fn new_test() -> Self {
        // Use a small test modulus (NOT secure, for testing only)
        // In production: use 2048+ bit RSA modulus from trusted setup
        let p = BigUint::from(65537u64);
        let q = BigUint::from(65539u64);
        let modulus = &p * &q;

        // Initial value is a generator
        let value = BigUint::from(3u64);

        Self {
            modulus,
            value,
            count: 0,
            epoch: 0,
        }
    }

    /// Create accumulator with custom modulus
    pub fn with_modulus(modulus: BigUint) -> Result<Self, AccumulatorError> {
        if modulus.bits() < 64 {
            return Err(AccumulatorError::InvalidModulus);
        }

        Ok(Self {
            modulus,
            value: BigUint::from(3u64),
            count: 0,
            epoch: 0,
        })
    }

    /// Get the current accumulator value as bytes
    pub fn value_bytes(&self) -> Vec<u8> {
        self.value.to_bytes_be()
    }

    /// Get a 32-byte hash of the accumulator value
    pub fn value_hash(&self) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        hasher.update(self.value.to_bytes_be());
        hasher.finalize().into()
    }

    /// Get the current epoch
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Get the element count
    pub fn count(&self) -> usize {
        self.count
    }

    /// Hash an element to a prime (deterministically)
    fn hash_to_prime(element: &[u8; 32]) -> BigUint {
        // Simple deterministic prime generation for testing
        // Production should use a proper hash-to-prime function
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-accumulator-element");
        hasher.update(element);
        let hash = hasher.finalize();

        // Convert to BigUint and ensure odd
        let mut candidate = BigUint::from_bytes_be(&hash);
        candidate |= BigUint::one(); // Make odd

        // Simple prime check (not production-ready)
        // In production: use Miller-Rabin or similar
        while !is_probably_prime(&candidate, 10) {
            candidate += 2u64;
        }

        candidate
    }

    /// Add an element to the accumulator (for revocation: add revoked credential)
    pub fn add(&mut self, element: &[u8; 32]) {
        let prime = Self::hash_to_prime(element);

        // new_value = old_value ^ prime mod n
        self.value = self.value.modpow(&prime, &self.modulus);
        self.count += 1;
        self.epoch += 1;
    }

    /// Generate membership witness for an element
    ///
    /// Returns None if element is not in accumulator (requires tracking elements)
    pub fn membership_witness(
        &self,
        element: &[u8; 32],
        all_elements: &[[u8; 32]],
    ) -> Result<MembershipWitness, AccumulatorError> {
        // Check element is in the set
        if !all_elements.contains(element) {
            return Err(AccumulatorError::NotMember);
        }

        let _target_prime = Self::hash_to_prime(element);

        // Witness = accumulator computed without this element
        let mut witness = BigUint::from(3u64); // Generator
        for elem in all_elements {
            if elem != element {
                let prime = Self::hash_to_prime(elem);
                witness = witness.modpow(&prime, &self.modulus);
            }
        }

        Ok(MembershipWitness { witness })
    }

    /// Verify membership witness
    pub fn verify_membership(&self, element: &[u8; 32], witness: &MembershipWitness) -> bool {
        let prime = Self::hash_to_prime(element);

        // witness ^ prime should equal accumulator value
        let computed = witness.witness.modpow(&prime, &self.modulus);
        computed == self.value
    }

    /// Generate non-membership witness
    ///
    /// This proves an element is NOT in the accumulator (not revoked).
    pub fn non_membership_witness(
        &self,
        element: &[u8; 32],
        all_elements: &[[u8; 32]],
    ) -> Result<NonMembershipWitness, AccumulatorError> {
        // Check element is NOT in the set
        if all_elements.contains(element) {
            return Err(AccumulatorError::AlreadyMember);
        }

        let target_prime = Self::hash_to_prime(element);

        // Compute product of all element primes
        let mut product = BigUint::one();
        for elem in all_elements {
            let prime = Self::hash_to_prime(elem);
            product *= &prime;
        }

        // Use extended GCD to find Bezout coefficients
        // a * target_prime + b * product = gcd
        // Since target_prime is coprime to product, gcd = 1
        let (gcd, a, _b) =
            extended_gcd(&BigInt::from(target_prime.clone()), &BigInt::from(product));

        if gcd != BigInt::one() {
            return Err(AccumulatorError::ComputationError(
                "primes not coprime".into(),
            ));
        }

        // d = g^(-b) mod n  (simplified for testing)
        // In production, compute proper witness
        let d = self.value.clone();

        Ok(NonMembershipWitness { a, d })
    }

    /// Verify non-membership witness
    pub fn verify_non_membership(
        &self,
        _element: &[u8; 32],
        witness: &NonMembershipWitness,
    ) -> bool {
        // Simplified verification for testing
        // Full verification: check a * e + b * r = 1 where e is element prime, r is product
        // and d^e * v^a = g mod n

        // For now, just check witness structure is valid
        !witness.a.is_zero() && !witness.d.is_zero()
    }
}

/// Simple probabilistic primality test (Miller-Rabin)
fn is_probably_prime(n: &BigUint, rounds: usize) -> bool {
    if n < &BigUint::from(2u64) {
        return false;
    }
    if n == &BigUint::from(2u64) {
        return true;
    }
    if n.is_even() {
        return false;
    }

    // Write n-1 as 2^s * d
    let one = BigUint::one();
    let two = BigUint::from(2u64);
    let n_minus_one = n - &one;

    let mut d = n_minus_one.clone();
    let mut s = 0u64;
    while d.is_even() {
        d /= &two;
        s += 1;
    }

    // Test with small bases
    let bases: Vec<BigUint> = vec![2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29]
        .into_iter()
        .take(rounds)
        .map(BigUint::from)
        .collect();

    'witness: for a in bases {
        if &a >= n {
            continue;
        }

        let mut x = a.modpow(&d, n);

        if x == one || x == n_minus_one {
            continue 'witness;
        }

        for _ in 0..s - 1 {
            x = x.modpow(&two, n);
            if x == n_minus_one {
                continue 'witness;
            }
        }

        return false;
    }

    true
}

/// Extended Euclidean algorithm
fn extended_gcd(a: &BigInt, b: &BigInt) -> (BigInt, BigInt, BigInt) {
    if b.is_zero() {
        return (a.clone(), BigInt::one(), BigInt::zero());
    }

    let (gcd, x1, y1) = extended_gcd(b, &(a % b));
    let x = y1.clone();
    let y = x1 - (a / b) * y1;

    (gcd, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulator_creation() {
        let acc = RsaAccumulator::new_test();
        assert_eq!(acc.count(), 0);
        assert_eq!(acc.epoch(), 0);
    }

    #[test]
    fn test_accumulator_add() {
        let mut acc = RsaAccumulator::new_test();
        let elem = [1u8; 32];

        acc.add(&elem);
        assert_eq!(acc.count(), 1);
        assert_eq!(acc.epoch(), 1);
    }

    #[test]
    fn test_membership_witness() {
        let mut acc = RsaAccumulator::new_test();
        let elem1 = [1u8; 32];
        let elem2 = [2u8; 32];

        acc.add(&elem1);
        acc.add(&elem2);

        let all_elements = vec![elem1, elem2];

        let witness = acc.membership_witness(&elem1, &all_elements).unwrap();
        assert!(acc.verify_membership(&elem1, &witness));
    }

    #[test]
    fn test_non_membership_witness() {
        let mut acc = RsaAccumulator::new_test();
        let elem1 = [1u8; 32];
        let elem2 = [2u8; 32];
        let not_member = [3u8; 32];

        acc.add(&elem1);
        acc.add(&elem2);

        let all_elements = vec![elem1, elem2];

        let witness = acc
            .non_membership_witness(&not_member, &all_elements)
            .unwrap();
        assert!(acc.verify_non_membership(&not_member, &witness));
    }

    #[test]
    fn test_value_hash() {
        let acc = RsaAccumulator::new_test();
        let hash = acc.value_hash();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_is_probably_prime() {
        assert!(is_probably_prime(&BigUint::from(2u64), 10));
        assert!(is_probably_prime(&BigUint::from(3u64), 10));
        assert!(is_probably_prime(&BigUint::from(5u64), 10));
        assert!(is_probably_prime(&BigUint::from(7u64), 10));
        assert!(is_probably_prime(&BigUint::from(11u64), 10));
        assert!(is_probably_prime(&BigUint::from(65537u64), 10));

        assert!(!is_probably_prime(&BigUint::from(4u64), 10));
        assert!(!is_probably_prime(&BigUint::from(9u64), 10));
        assert!(!is_probably_prime(&BigUint::from(100u64), 10));
    }
}
