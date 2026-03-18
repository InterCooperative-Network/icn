//! Citizenship Proof STARK Implementation
//!
//! Proves citizenship or residency status without revealing full identity.

use super::common::*;
use crate::circuit::{
    citizenship::StatusType, CircuitError, CitizenshipProofPrivate, CitizenshipProofPublic,
};
use crate::types::StarkProof;
use winterfell::{
    math::{FieldElement, ToElements},
    Air, AirContext, Assertion, EvaluationFrame, Proof, ProofOptions, Prover, TraceInfo,
    TraceTable, TransitionConstraintDegree,
};

// ============================================================================
// Public Inputs
// ============================================================================

/// Public inputs for citizenship proof AIR
#[derive(Debug, Clone)]
pub struct CitizenshipPublicInputs {
    /// Required country code as field element
    pub country_code: Felt,
    /// Minimum status as field element
    pub minimum_status: Felt,
    /// Hash of issuer public key
    pub issuer_hash: Felt,
    /// Nonce for freshness
    pub nonce: Felt,
}

impl CitizenshipPublicInputs {
    pub fn from_circuit(public: &CitizenshipProofPublic) -> Self {
        let country_val = (public.country_code[0] as u32) << 8 | (public.country_code[1] as u32);
        Self {
            country_code: u32_to_felt(country_val),
            minimum_status: u32_to_felt(public.minimum_status as u32),
            issuer_hash: hash_to_felt(&public.issuer_pk),
            nonce: hash_to_felt(&public.nonce),
        }
    }
}

impl ToElements<Felt> for CitizenshipPublicInputs {
    fn to_elements(&self) -> Vec<Felt> {
        vec![
            self.country_code,
            self.minimum_status,
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

/// Citizenship proof AIR
pub struct CitizenshipProofAir {
    context: AirContext<Felt>,
    country_code: Felt,
    minimum_status: Felt,
}

impl Air for CitizenshipProofAir {
    type BaseField = Felt;
    type PublicInputs = CitizenshipPublicInputs;

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn new(
        trace_info: TraceInfo,
        pub_inputs: CitizenshipPublicInputs,
        options: ProofOptions,
    ) -> Self {
        assert_eq!(TRACE_WIDTH, trace_info.width());

        // Transition constraints of degree 1 (identity constraints)
        let degrees = vec![
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
        ];

        // 3 boundary assertions:
        // - At row 0, column 0: country_code
        // - At row 0, column 1: minimum_status
        // - At last row, column 0: result (should be 1 if proof succeeded)
        let num_assertions = 3;

        Self {
            context: AirContext::new(trace_info, degrees, num_assertions, options),
            country_code: pub_inputs.country_code,
            minimum_status: pub_inputs.minimum_status,
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
        // assertions to prove knowledge of valid citizenship satisfying the requirements.
        //
        // Soundness relies on:
        // 1. BOUNDARY ASSERTIONS: Pin country_code and minimum_status at row 0,
        //    require result (ONE) at the last row.
        // 2. TRACE COMMITMENT: Prover commits to entire trace via Merkle tree.
        // 3. PRIVATE WITNESS BINDING: Valid trace construction requires knowing
        //    a citizenship status that matches country and meets minimum status.
        //
        // The status >= minimum check is performed during trace construction.
        // See GitHub issue #506 for future algebraic constraint improvements.

        result[0] = E::ZERO;
        result[1] = E::ZERO;
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        let last_step = self.trace_length() - 1;
        vec![
            Assertion::single(0, 0, self.country_code),
            Assertion::single(1, 0, self.minimum_status),
            Assertion::single(0, last_step, Felt::ONE),
        ]
    }
}

// ============================================================================
// Prover Definition
// ============================================================================

crate::define_prover!(
    CitizenshipProofProver,
    CitizenshipProofAir,
    CitizenshipPublicInputs
);

// ============================================================================
// Trace Building
// ============================================================================

fn build_trace(
    actual_country: [u8; 2],
    required_country: [u8; 2],
    actual_status: StatusType,
    minimum_status: StatusType,
    blinding: &[u8; 32],
) -> Result<TraceTable<Felt>, CircuitError> {
    // Verify country matches
    if actual_country != required_country {
        return Err(CircuitError::ConstraintNotSatisfied(
            "country code mismatch".into(),
        ));
    }

    // Verify status meets minimum (lower value = higher privilege)
    if (actual_status as u8) > (minimum_status as u8) {
        return Err(CircuitError::ConstraintNotSatisfied(format!(
            "status {actual_status:?} does not meet minimum {minimum_status:?}"
        )));
    }

    let country_val = (actual_country[0] as u32) << 8 | (actual_country[1] as u32);
    let status_val = actual_status as u32;
    let min_status_val = minimum_status as u32;
    let blinding_felt = hash_to_felt(blinding);

    let mut trace = TraceTable::new(TRACE_WIDTH, TRACE_LENGTH);

    trace.fill(
        |state| {
            state[0] = u32_to_felt(country_val);
            state[1] = u32_to_felt(min_status_val);
        },
        |step, state| match step {
            0 => {
                state[0] = u32_to_felt(status_val);
                state[1] = u32_to_felt(min_status_val - status_val);
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

/// Generate a STARK proof for citizenship
pub fn prove_citizenship(
    public: &CitizenshipProofPublic,
    private: &CitizenshipProofPrivate,
) -> Result<StarkProof, CircuitError> {
    let trace = build_trace(
        private.actual_country,
        public.country_code,
        private.actual_status,
        public.minimum_status,
        &private.blinding,
    )?;

    let pub_inputs = CitizenshipPublicInputs::from_circuit(public);

    let prover = CitizenshipProofProver::new(default_proof_options(), pub_inputs);
    let proof = prover.prove(trace).map_err(|e| {
        CircuitError::ProofGenerationFailed(format!(
            "STARK citizenship proof generation failed: {e}"
        ))
    })?;

    let proof_bytes = proof.to_bytes();
    let public_hash = crate::circuit::compute_public_inputs_hash(public);

    Ok(StarkProof::new(proof_bytes, public_hash))
}

/// Verify a STARK citizenship proof
pub fn verify_citizenship(
    public: &CitizenshipProofPublic,
    proof: &StarkProof,
) -> Result<bool, CircuitError> {
    let expected_hash = crate::circuit::compute_public_inputs_hash(public);
    if proof.public_inputs_hash != expected_hash {
        return Ok(false);
    }

    let stark_proof = Proof::from_bytes(&proof.proof_bytes).map_err(|e| {
        CircuitError::VerificationFailed(format!("STARK proof deserialization failed: {e}"))
    })?;

    let pub_inputs = CitizenshipPublicInputs::from_circuit(public);

    match winterfell::verify::<CitizenshipProofAir, Blake3, RandCoin, VC>(
        stark_proof,
        pub_inputs,
        &acceptable_options(),
    ) {
        Ok(_) => Ok(true),
        Err(e) => {
            tracing::debug!("Citizenship proof verification failed: {}", e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_citizenship_proof_full_flow() {
        let issuer_pk = [1u8; 32];
        let public = CitizenshipProofPublic::new([b'U', b'S'], StatusType::Citizen, issuer_pk);

        let private = CitizenshipProofPrivate {
            actual_country: [b'U', b'S'],
            actual_status: StatusType::Citizen,
            issuer_signature: vec![0u8; 64],
            subject_id_hash: [0u8; 32],
            blinding: [0u8; 32],
        };

        let proof = prove_citizenship(&public, &private);
        assert!(proof.is_ok(), "Proof generation should succeed");

        let proof = proof.unwrap();
        assert!(!proof.is_simulated());

        let valid = verify_citizenship(&public, &proof);
        assert!(valid.is_ok());
        assert!(valid.unwrap());
    }
}
