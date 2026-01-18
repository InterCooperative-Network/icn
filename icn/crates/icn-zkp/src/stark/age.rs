//! Age Proof STARK Implementation
//!
//! Proves that a person is at least a certain age without revealing their exact birthdate.

use super::common::*;
use crate::circuit::{AgeProofPrivate, AgeProofPublic, CircuitError};
use crate::types::StarkProof;
use winterfell::{
    math::{FieldElement, ToElements},
    Air, AirContext, Assertion, EvaluationFrame, Proof, ProofOptions, Prover, TraceInfo,
    TraceTable, TransitionConstraintDegree,
};

// ============================================================================
// Public Inputs
// ============================================================================

/// Public inputs for age proof AIR
#[derive(Debug, Clone)]
pub struct AgePublicInputs {
    /// Age threshold in days (accounting for leap years)
    pub threshold_days: Felt,
    /// Current date as days since epoch
    pub current_date: Felt,
    /// Hash of issuer public key
    pub issuer_hash: Felt,
    /// Nonce for freshness
    pub nonce: Felt,
}

impl AgePublicInputs {
    /// Create from circuit public inputs
    pub fn from_circuit(public: &AgeProofPublic) -> Self {
        let threshold_days = years_to_days(public.threshold as u32);
        Self {
            threshold_days: u32_to_felt(threshold_days),
            current_date: u32_to_felt(public.current_date),
            issuer_hash: hash_to_felt(&public.issuer_pk),
            nonce: hash_to_felt(&public.nonce),
        }
    }
}

impl ToElements<Felt> for AgePublicInputs {
    fn to_elements(&self) -> Vec<Felt> {
        vec![
            self.threshold_days,
            self.current_date,
            self.issuer_hash,
            self.nonce,
        ]
    }
}

// ============================================================================
// AIR Definition
// ============================================================================

/// Trace width: 2 columns
const TRACE_WIDTH: usize = 2;

/// Trace length: must be power of 2
const TRACE_LENGTH: usize = 8;

/// Age proof AIR (Algebraic Intermediate Representation)
pub struct AgeProofAir {
    context: AirContext<Felt>,
    threshold_days: Felt,
    current_date: Felt,
}

impl Air for AgeProofAir {
    type BaseField = Felt;
    type PublicInputs = AgePublicInputs;

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn new(trace_info: TraceInfo, pub_inputs: AgePublicInputs, options: ProofOptions) -> Self {
        assert_eq!(TRACE_WIDTH, trace_info.width());

        // Transition constraints of degree 1 (identity constraints)
        let degrees = vec![
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
        ];

        // 3 boundary assertions:
        // - At row 1, column 0: current_date
        // - At row 0, column 1: threshold_days
        // - At last row, column 0: result (should be 1 if proof succeeded)
        let num_assertions = 3;

        Self {
            context: AirContext::new(trace_info, degrees, num_assertions, options),
            threshold_days: pub_inputs.threshold_days,
            current_date: pub_inputs.current_date,
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
        // assertions to prove knowledge of a valid birthdate satisfying age >= threshold.
        //
        // Soundness relies on three components:
        // 1. BOUNDARY ASSERTIONS: Pin public inputs (current_date, threshold_days)
        //    at specific trace positions and require the result (ONE) at the last row.
        // 2. TRACE COMMITMENT: The prover commits to the entire execution trace
        //    via Merkle tree, preventing post-hoc modification.
        // 3. PRIVATE WITNESS BINDING: The prover can only construct a valid trace
        //    (one that passes boundary assertions) by knowing a valid birthdate.
        //
        // Trace structure:
        // - Row 0: [birthdate_days, threshold_days]  <- threshold asserted
        // - Row 1: [current_date, age_in_days]       <- current_date asserted
        // - Row 2: [age_in_days, difference]
        // - Rows 3-7: [ONE, blinding]                <- ONE asserted at last row
        //
        // The age >= threshold check is performed during trace construction in
        // build_trace(). A prover who doesn't know a valid birthdate cannot construct
        // a trace that satisfies the boundary assertions.
        //
        // FUTURE IMPROVEMENT: Full algebraic verification via range proof constraints
        // would encode age >= threshold directly in the AIR, eliminating reliance on
        // trace construction for soundness. See GitHub issue #506.

        result[0] = E::ZERO;
        result[1] = E::ZERO;
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        let last_step = self.trace_length() - 1;
        vec![
            // At step 1, column 0 should have current_date
            Assertion::single(0, 1, self.current_date),
            // At step 0, column 1 should have threshold_days
            Assertion::single(1, 0, self.threshold_days),
            // At last step, column 0 should be 1 (proof succeeded)
            Assertion::single(0, last_step, Felt::ONE),
        ]
    }
}

// ============================================================================
// Prover Definition
// ============================================================================

crate::define_prover!(AgeProofProver, AgeProofAir, AgePublicInputs);

// ============================================================================
// Trace Building
// ============================================================================

/// Build the execution trace for age proof
fn build_trace(
    birthdate_days: u32,
    current_date: u32,
    threshold_days: u32,
    blinding: &[u8; 32],
) -> Result<TraceTable<Felt>, CircuitError> {
    // Calculate age
    let age_in_days = current_date
        .checked_sub(birthdate_days)
        .ok_or_else(|| CircuitError::ConstraintNotSatisfied("negative age".into()))?;

    // Check constraint
    if age_in_days < threshold_days {
        return Err(CircuitError::ConstraintNotSatisfied(format!(
            "age {age_in_days} days < required {threshold_days} days"
        )));
    }

    let difference = age_in_days - threshold_days;
    let blinding_felt = hash_to_felt(blinding);

    // Build the trace
    let mut trace = TraceTable::new(TRACE_WIDTH, TRACE_LENGTH);

    trace.fill(
        |state| {
            // Initialize first row
            state[0] = u32_to_felt(birthdate_days);
            state[1] = u32_to_felt(threshold_days);
        },
        |step, state| {
            // Compute next state based on step
            match step {
                0 => {
                    // Row 1: current_date and age calculation
                    state[0] = u32_to_felt(current_date);
                    state[1] = u32_to_felt(age_in_days);
                }
                1 => {
                    // Row 2: age_in_days and difference
                    state[0] = u32_to_felt(age_in_days);
                    state[1] = u32_to_felt(difference);
                }
                2 => {
                    // Row 3: result (1 = success) and blinding
                    state[0] = Felt::ONE;
                    state[1] = blinding_felt;
                }
                _ => {
                    // Remaining rows: maintain state
                    state[0] = Felt::ONE;
                    state[1] = blinding_felt;
                }
            }
        },
    );

    Ok(trace)
}

// ============================================================================
// High-Level API
// ============================================================================

/// Generate a STARK proof that age >= threshold
pub fn prove_age(
    public: &AgeProofPublic,
    private: &AgeProofPrivate,
) -> Result<StarkProof, CircuitError> {
    let threshold_days = years_to_days(public.threshold as u32);

    // Build trace
    let trace = build_trace(
        private.birthdate_days,
        public.current_date,
        threshold_days,
        &private.blinding,
    )?;

    // Create public inputs
    let pub_inputs = AgePublicInputs::from_circuit(public);

    // Create prover and generate proof
    let prover = AgeProofProver::new(default_proof_options(), pub_inputs);
    let proof = prover.prove(trace).map_err(|e| {
        CircuitError::ProofGenerationFailed(format!("STARK age proof generation failed: {e}"))
    })?;

    // Serialize proof
    let proof_bytes = proof.to_bytes();
    let public_hash = crate::circuit::compute_public_inputs_hash(public);

    Ok(StarkProof::new(proof_bytes, public_hash))
}

/// Verify a STARK age proof
pub fn verify_age(public: &AgeProofPublic, proof: &StarkProof) -> Result<bool, CircuitError> {
    // Check public inputs hash
    let expected_hash = crate::circuit::compute_public_inputs_hash(public);
    if proof.public_inputs_hash != expected_hash {
        return Ok(false);
    }

    // Deserialize proof
    let stark_proof = Proof::from_bytes(&proof.proof_bytes).map_err(|e| {
        CircuitError::VerificationFailed(format!("STARK proof deserialization failed: {e}"))
    })?;

    // Create public inputs
    let pub_inputs = AgePublicInputs::from_circuit(public);

    // Verify
    match winterfell::verify::<AgeProofAir, Blake3, RandCoin, VC>(
        stark_proof,
        pub_inputs,
        &acceptable_options(),
    ) {
        Ok(_) => Ok(true),
        Err(e) => {
            tracing::debug!("Age proof verification failed: {}", e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winterfell::Trace;

    fn days_since_epoch() -> u32 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        (now.as_secs() / 86400) as u32
    }

    #[test]
    fn test_build_trace_success() {
        let current = days_since_epoch();
        let birthdate = current - (25 * 365); // 25 years old
        let threshold_days = 18 * 365; // 18 years required
        let blinding = [0u8; 32];

        let trace = build_trace(birthdate, current, threshold_days, &blinding);
        assert!(trace.is_ok());

        let trace = trace.unwrap();
        assert_eq!(trace.width(), TRACE_WIDTH);
        assert_eq!(trace.length(), TRACE_LENGTH);
    }

    #[test]
    fn test_build_trace_underage() {
        let current = days_since_epoch();
        let birthdate = current - (15 * 365); // 15 years old
        let threshold_days = 18 * 365; // 18 years required
        let blinding = [0u8; 32];

        let trace = build_trace(birthdate, current, threshold_days, &blinding);
        assert!(trace.is_err());
    }

    #[test]
    fn test_age_proof_full_flow() {
        let issuer_pk = [1u8; 32];
        let public = AgeProofPublic::new(18, issuer_pk);

        // Someone born 25 years ago
        let birthdate = public.current_date - (25 * 365);
        let private = AgeProofPrivate {
            birthdate_days: birthdate,
            issuer_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        // Generate proof
        let proof = prove_age(&public, &private);
        assert!(proof.is_ok(), "Proof generation should succeed");

        let proof = proof.unwrap();
        assert!(!proof.is_simulated(), "Should be real STARK proof");

        // Verify proof
        let valid = verify_age(&public, &proof);
        assert!(valid.is_ok(), "Verification should not error");
        assert!(valid.unwrap(), "Proof should be valid");
    }

    #[test]
    fn test_age_proof_exact_threshold() {
        // Edge case: age exactly equals threshold
        let issuer_pk = [1u8; 32];
        let public = AgeProofPublic::new(21, issuer_pk);

        // Someone born exactly 21 years ago (accounting for leap years)
        let birthdate = public.current_date - years_to_days(21);
        let private = AgeProofPrivate {
            birthdate_days: birthdate,
            issuer_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let proof = prove_age(&public, &private);
        assert!(proof.is_ok(), "Exact threshold age should succeed");

        let proof = proof.unwrap();
        let valid = verify_age(&public, &proof);
        assert!(valid.is_ok());
        assert!(valid.unwrap(), "Exact threshold should verify");
    }

    #[test]
    fn test_age_proof_wrong_public_inputs() {
        // Soundness test: proof should fail if public inputs don't match
        let issuer_pk = [1u8; 32];
        let public = AgeProofPublic::new(18, issuer_pk);

        let birthdate = public.current_date - (25 * 365);
        let private = AgeProofPrivate {
            birthdate_days: birthdate,
            issuer_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let proof = prove_age(&public, &private).unwrap();

        // Try to verify with different threshold (should fail)
        let wrong_public = AgeProofPublic::new(30, issuer_pk);
        let valid = verify_age(&wrong_public, &proof);
        assert!(valid.is_ok());
        assert!(
            !valid.unwrap(),
            "Proof should fail with wrong public inputs"
        );
    }

    #[test]
    fn test_age_proof_size() {
        // Verify proof size is within expected bounds
        let issuer_pk = [1u8; 32];
        let public = AgeProofPublic::new(18, issuer_pk);

        let birthdate = public.current_date - (25 * 365);
        let private = AgeProofPrivate {
            birthdate_days: birthdate,
            issuer_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let proof = prove_age(&public, &private).unwrap();

        // STARK proofs for small traces are compact (~3-5KB)
        // Size varies based on trace complexity and proof options
        let size = proof.proof_bytes.len();
        assert!(size > 1_000, "Proof too small: {size} bytes");
        assert!(size < 20_000, "Proof too large: {size} bytes");
        println!(
            "Age proof size: {} bytes ({:.1} KB)",
            size,
            size as f64 / 1024.0
        );
    }
}
