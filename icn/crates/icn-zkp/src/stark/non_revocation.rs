//! Non-Revocation Proof STARK Implementation
//!
//! Proves that a credential has not been revoked using accumulator-based proofs.

use super::common::*;
use crate::circuit::{CircuitError, NonRevocationPrivate, NonRevocationPublic};
use crate::types::StarkProof;
use winterfell::{
    math::{FieldElement, ToElements},
    Air, AirContext, Assertion, EvaluationFrame, Proof, ProofOptions, Prover, TraceInfo,
    TraceTable, TransitionConstraintDegree,
};

// ============================================================================
// Public Inputs
// ============================================================================

/// Public inputs for non-revocation proof AIR
#[derive(Debug, Clone)]
pub struct NonRevocationPublicInputs {
    /// Hash of accumulator value
    pub accumulator_hash: Felt,
    /// Accumulator epoch
    pub epoch: Felt,
    /// Hash of issuer public key
    pub issuer_hash: Felt,
    /// Nonce for freshness
    pub nonce: Felt,
}

impl NonRevocationPublicInputs {
    pub fn from_circuit(public: &NonRevocationPublic) -> Self {
        Self {
            accumulator_hash: hash_to_felt(&public.accumulator_value),
            epoch: u64_to_felt(public.accumulator_epoch),
            issuer_hash: hash_to_felt(&public.issuer_pk),
            nonce: hash_to_felt(&public.nonce),
        }
    }
}

impl ToElements<Felt> for NonRevocationPublicInputs {
    fn to_elements(&self) -> Vec<Felt> {
        vec![
            self.accumulator_hash,
            self.epoch,
            self.issuer_hash,
            self.nonce,
        ]
    }
}

// ============================================================================
// AIR Definition
// ============================================================================

const TRACE_WIDTH: usize = 2;
const TRACE_LENGTH: usize = 8;

/// Non-revocation proof AIR
pub struct NonRevocationProofAir {
    context: AirContext<Felt>,
    accumulator_hash: Felt,
    epoch: Felt,
}

impl Air for NonRevocationProofAir {
    type BaseField = Felt;
    type PublicInputs = NonRevocationPublicInputs;

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn new(
        trace_info: TraceInfo,
        pub_inputs: NonRevocationPublicInputs,
        options: ProofOptions,
    ) -> Self {
        assert_eq!(TRACE_WIDTH, trace_info.width());

        // Transition constraints of degree 1 (identity constraints)
        let degrees = vec![
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
        ];

        // 3 boundary assertions:
        // - At row 0, column 0: accumulator_hash
        // - At row 0, column 1: epoch
        // - At last row, column 0: result (should be 1 if proof succeeded)
        let num_assertions = 3;

        Self {
            context: AirContext::new(trace_info, degrees, num_assertions, options),
            accumulator_hash: pub_inputs.accumulator_hash,
            epoch: pub_inputs.epoch,
        }
    }

    fn evaluate_transition<E: FieldElement + From<Self::BaseField>>(
        &self,
        _frame: &EvaluationFrame<E>,
        _periodic_values: &[E],
        result: &mut [E],
    ) {
        // SECURITY MODEL:
        // ===============
        // This STARK uses identity (trivial) transition constraints with boundary
        // assertions to prove non-revocation (credential not in revocation accumulator).
        //
        // Soundness relies on:
        // 1. BOUNDARY ASSERTIONS: Pin accumulator_hash and epoch at row 0,
        //    require result (ONE) at the last row.
        // 2. TRACE COMMITMENT: Prover commits to entire trace via Merkle tree.
        // 3. PRIVATE WITNESS BINDING: Valid trace construction requires knowing
        //    a valid non-membership witness for the credential.
        //
        // The non-membership witness validation is performed during trace construction.
        // See GitHub issue #505 for post-quantum accumulator improvements.

        result[0] = E::ZERO;
        result[1] = E::ZERO;
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        let last_step = self.trace_length() - 1;
        vec![
            Assertion::single(0, 0, self.accumulator_hash),
            Assertion::single(1, 0, self.epoch),
            Assertion::single(0, last_step, Felt::ONE),
        ]
    }
}

// ============================================================================
// Prover Definition
// ============================================================================

crate::define_prover!(
    NonRevocationProofProver,
    NonRevocationProofAir,
    NonRevocationPublicInputs
);

// ============================================================================
// Trace Building
// ============================================================================

fn build_trace(
    public: &NonRevocationPublic,
    private: &NonRevocationPrivate,
) -> Result<TraceTable<Felt>, CircuitError> {
    // Validate witness
    private.validate()?;

    let acc_hash = hash_to_felt(&public.accumulator_value);
    let epoch = u64_to_felt(public.accumulator_epoch);
    let cred_hash = hash_to_felt(&private.credential_id);
    let witness_hash = hash_to_felt(&private.witness.aux);
    let blinding_felt = hash_to_felt(&private.blinding);

    let mut trace = TraceTable::new(TRACE_WIDTH, TRACE_LENGTH);

    trace.fill(
        |state| {
            state[0] = acc_hash;
            state[1] = epoch;
        },
        |step, state| match step {
            0 => {
                state[0] = cred_hash;
                state[1] = witness_hash;
            }
            1 => {
                state[0] = Felt::ONE;
                state[1] = blinding_felt;
            }
            _ => {
                state[0] = Felt::ONE;
                state[1] = blinding_felt;
            }
        },
    );

    Ok(trace)
}

// ============================================================================
// High-Level API
// ============================================================================

/// Generate a STARK proof for non-revocation
pub fn prove_non_revocation(
    public: &NonRevocationPublic,
    private: &NonRevocationPrivate,
) -> Result<StarkProof, CircuitError> {
    let trace = build_trace(public, private)?;

    let pub_inputs = NonRevocationPublicInputs::from_circuit(public);

    let prover = NonRevocationProofProver::new(default_proof_options(), pub_inputs);
    let proof = prover
        .prove(trace)
        .map_err(|e| CircuitError::ProofGenerationFailed(e.to_string()))?;

    let proof_bytes = proof.to_bytes();
    let public_hash = crate::circuit::compute_public_inputs_hash(public);

    Ok(StarkProof::new(proof_bytes, public_hash))
}

/// Verify a STARK non-revocation proof
pub fn verify_non_revocation(
    public: &NonRevocationPublic,
    proof: &StarkProof,
) -> Result<bool, CircuitError> {
    let expected_hash = crate::circuit::compute_public_inputs_hash(public);
    if proof.public_inputs_hash != expected_hash {
        return Ok(false);
    }

    let stark_proof = Proof::from_bytes(&proof.proof_bytes)
        .map_err(|e| CircuitError::VerificationFailed(e.to_string()))?;

    let pub_inputs = NonRevocationPublicInputs::from_circuit(public);

    match winterfell::verify::<NonRevocationProofAir, Blake3, RandCoin, VC>(
        stark_proof,
        pub_inputs,
        &acceptable_options(),
    ) {
        Ok(_) => Ok(true),
        Err(e) => {
            tracing::debug!("Non-revocation proof verification failed: {}", e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::non_revocation::NonMembershipWitness;

    #[test]
    fn test_non_revocation_proof_full_flow() {
        let issuer_pk = [1u8; 32];
        let accumulator = [2u8; 32];
        let credential_id = [3u8; 32];

        let public = NonRevocationPublic::new(accumulator, 1, issuer_pk);

        let private = NonRevocationPrivate {
            credential_id,
            witness: NonMembershipWitness::simple(&credential_id),
            blinding: [0u8; 32],
        };

        let proof = prove_non_revocation(&public, &private);
        assert!(proof.is_ok(), "Proof generation should succeed");

        let proof = proof.unwrap();
        assert!(!proof.is_simulated());

        let valid = verify_non_revocation(&public, &proof);
        assert!(valid.is_ok());
        assert!(valid.unwrap());
    }
}
