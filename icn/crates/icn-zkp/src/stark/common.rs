//! Common utilities for STARK proof generation
//!
//! Shared types and functions used across all STARK circuits.

use winterfell::{
    crypto::{hashers::Blake3_256, DefaultRandomCoin, MerkleTree},
    math::{fields::f128::BaseElement, StarkField},
    AcceptableOptions, BatchingMethod, FieldExtension, ProofOptions,
};

/// The base field element type used in all our circuits
pub type Felt = BaseElement;

/// Hash function used for STARK proofs
pub type Blake3 = Blake3_256<Felt>;

/// Random coin type for Fiat-Shamir
pub type RandCoin = DefaultRandomCoin<Blake3>;

/// Vector commitment type (Merkle tree)
pub type VC = MerkleTree<Blake3>;

/// Default proof options for ~96-bit security
pub fn default_proof_options() -> ProofOptions {
    ProofOptions::new(
        32, // number of queries (higher = more secure but larger proof)
        8,  // blowup factor (must be power of 2, affects proof size)
        0,  // grinding factor (0 = no grinding, faster proofs)
        FieldExtension::None,
        8,                      // FRI folding factor
        127,                    // FRI remainder max degree
        BatchingMethod::Linear, // Trace commitment batching
        BatchingMethod::Linear, // Constraint commitment batching
    )
}

/// Acceptable options for proof verification
pub fn acceptable_options() -> AcceptableOptions {
    // Accept proofs with at least 95 bits of conjectured security
    AcceptableOptions::MinConjecturedSecurity(95)
}

/// Convert a u32 value to a field element
pub fn u32_to_felt(value: u32) -> Felt {
    Felt::from(value)
}

/// Convert a u64 value to a field element
pub fn u64_to_felt(value: u64) -> Felt {
    Felt::from(value)
}

/// Convert a byte array to field elements (each byte becomes one element)
pub fn bytes_to_felts(bytes: &[u8]) -> Vec<Felt> {
    bytes.iter().map(|&b| Felt::from(b as u32)).collect()
}

/// Convert field elements back to u32 (assumes values fit in u32)
pub fn felt_to_u32(felt: Felt) -> u32 {
    // BaseElement's inner value can be extracted via as_int()
    felt.as_int() as u32
}

/// Hash bytes to a single field element using SHA3
pub fn hash_to_felt(data: &[u8]) -> Felt {
    use sha3::{Digest, Sha3_256};
    let hash = Sha3_256::digest(data);
    // Take first 8 bytes as u64 for field element
    let bytes: [u8; 8] = hash[..8].try_into().unwrap_or([0u8; 8]);
    Felt::from(u64::from_le_bytes(bytes))
}

/// Macro to create a prover for a specific AIR type
#[macro_export]
macro_rules! define_prover {
    ($prover_name:ident, $air_type:ty, $public_inputs:ty) => {
        pub struct $prover_name {
            options: winterfell::ProofOptions,
            pub_inputs: $public_inputs,
        }

        impl $prover_name {
            pub fn new(options: winterfell::ProofOptions, pub_inputs: $public_inputs) -> Self {
                Self {
                    options,
                    pub_inputs,
                }
            }
        }

        impl winterfell::Prover for $prover_name {
            type BaseField = $crate::stark::common::Felt;
            type Air = $air_type;
            type Trace = winterfell::TraceTable<$crate::stark::common::Felt>;
            type HashFn = $crate::stark::common::Blake3;
            type VC = $crate::stark::common::VC;
            type RandomCoin = $crate::stark::common::RandCoin;
            type TraceLde<
                E: winterfell::math::FieldElement<BaseField = $crate::stark::common::Felt>,
            > = winterfell::DefaultTraceLde<E, Self::HashFn, Self::VC>;
            type ConstraintCommitment<
                E: winterfell::math::FieldElement<BaseField = $crate::stark::common::Felt>,
            > = winterfell::DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
            type ConstraintEvaluator<
                'a,
                E: winterfell::math::FieldElement<BaseField = $crate::stark::common::Felt>,
            > = winterfell::DefaultConstraintEvaluator<'a, Self::Air, E>;

            fn get_pub_inputs(&self, _trace: &Self::Trace) -> $public_inputs {
                self.pub_inputs.clone()
            }

            fn options(&self) -> &winterfell::ProofOptions {
                &self.options
            }

            fn new_trace_lde<
                E: winterfell::math::FieldElement<BaseField = $crate::stark::common::Felt>,
            >(
                &self,
                trace_info: &winterfell::TraceInfo,
                main_trace: &winterfell::matrix::ColMatrix<$crate::stark::common::Felt>,
                domain: &winterfell::StarkDomain<$crate::stark::common::Felt>,
                partition_options: winterfell::PartitionOptions,
            ) -> (Self::TraceLde<E>, winterfell::TracePolyTable<E>) {
                winterfell::DefaultTraceLde::new(trace_info, main_trace, domain, partition_options)
            }

            fn build_constraint_commitment<
                E: winterfell::math::FieldElement<BaseField = $crate::stark::common::Felt>,
            >(
                &self,
                composition_poly_trace: winterfell::CompositionPolyTrace<E>,
                num_trace_poly_columns: usize,
                domain: &winterfell::StarkDomain<$crate::stark::common::Felt>,
                partition_options: winterfell::PartitionOptions,
            ) -> (
                Self::ConstraintCommitment<E>,
                winterfell::CompositionPoly<E>,
            ) {
                winterfell::DefaultConstraintCommitment::new(
                    composition_poly_trace,
                    num_trace_poly_columns,
                    domain,
                    partition_options,
                )
            }

            fn new_evaluator<
                'a,
                E: winterfell::math::FieldElement<BaseField = $crate::stark::common::Felt>,
            >(
                &self,
                air: &'a Self::Air,
                aux_rand_elements: Option<winterfell::AuxRandElements<E>>,
                composition_coefficients: winterfell::ConstraintCompositionCoefficients<E>,
            ) -> Self::ConstraintEvaluator<'a, E> {
                winterfell::DefaultConstraintEvaluator::new(
                    air,
                    aux_rand_elements,
                    composition_coefficients,
                )
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u32_to_felt() {
        let felt = u32_to_felt(42);
        assert_eq!(felt, Felt::from(42u32));
    }

    #[test]
    fn test_felt_to_u32() {
        let felt = Felt::from(12345u32);
        assert_eq!(felt_to_u32(felt), 12345);
    }

    #[test]
    fn test_hash_to_felt_deterministic() {
        let h1 = hash_to_felt(b"hello");
        let h2 = hash_to_felt(b"hello");
        assert_eq!(h1, h2);

        let h3 = hash_to_felt(b"world");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_default_proof_options() {
        let opts = default_proof_options();
        // Just verify it doesn't panic and returns valid options
        assert!(opts.num_queries() > 0);
    }
}
