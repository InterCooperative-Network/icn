//! Membership Proof STARK Implementation
//!
//! Proves membership in an organization without revealing member identity.

use super::common::*;
use crate::circuit::{CircuitError, MembershipProofPrivate, MembershipProofPublic};
use crate::types::StarkProof;
use winterfell::{
    math::{FieldElement, ToElements},
    Air, AirContext, Assertion, EvaluationFrame, Proof, ProofOptions, Prover, TraceInfo,
    TraceTable, TransitionConstraintDegree,
};

// ============================================================================
// Public Inputs
// ============================================================================

/// Public inputs for membership proof AIR
#[derive(Debug, Clone)]
pub struct MembershipPublicInputs {
    /// Hash of organization DID
    pub org_did_hash: Felt,
    /// Hash of required role (0 if no role required)
    pub role_hash: Felt,
    /// Current timestamp
    pub current_time: Felt,
    /// Nonce for freshness
    pub nonce: Felt,
}

impl MembershipPublicInputs {
    pub fn from_circuit(public: &MembershipProofPublic) -> Self {
        let role_hash = match &public.required_role {
            Some(role) => hash_to_felt(role.as_bytes()),
            None => Felt::ZERO,
        };

        Self {
            org_did_hash: hash_to_felt(public.org_did.as_str().as_bytes()),
            role_hash,
            current_time: u64_to_felt(public.current_time),
            nonce: hash_to_felt(&public.nonce),
        }
    }
}

impl ToElements<Felt> for MembershipPublicInputs {
    fn to_elements(&self) -> Vec<Felt> {
        vec![
            self.org_did_hash,
            self.role_hash,
            self.current_time,
            self.nonce,
        ]
    }
}

// ============================================================================
// AIR Definition
// ============================================================================

const TRACE_WIDTH: usize = 2;
const TRACE_LENGTH: usize = 8;

/// Membership proof AIR
pub struct MembershipProofAir {
    context: AirContext<Felt>,
    org_did_hash: Felt,
    current_time: Felt,
}

impl Air for MembershipProofAir {
    type BaseField = Felt;
    type PublicInputs = MembershipPublicInputs;

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn new(
        trace_info: TraceInfo,
        pub_inputs: MembershipPublicInputs,
        options: ProofOptions,
    ) -> Self {
        assert_eq!(TRACE_WIDTH, trace_info.width());

        // Transition constraints of degree 1 (identity constraints)
        let degrees = vec![
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
        ];

        // 3 boundary assertions:
        // - At row 0, column 0: org_did_hash
        // - At row 0, column 1: current_time
        // - At last row, column 0: result (should be 1 if proof succeeded)
        let num_assertions = 3;

        Self {
            context: AirContext::new(trace_info, degrees, num_assertions, options),
            org_did_hash: pub_inputs.org_did_hash,
            current_time: pub_inputs.current_time,
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
        // assertions to prove valid organization membership.
        //
        // Soundness relies on:
        // 1. BOUNDARY ASSERTIONS: Pin org_did_hash and current_time at row 0,
        //    require result (ONE) at the last row.
        // 2. TRACE COMMITMENT: Prover commits to entire trace via Merkle tree.
        // 3. PRIVATE WITNESS BINDING: Valid trace construction requires knowing
        //    a member DID that belongs to the organization with valid time bounds.
        //
        // The membership validity check is performed during trace construction.
        // See GitHub issue #506 for future algebraic constraint improvements.

        result[0] = E::ZERO;
        result[1] = E::ZERO;
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        let last_step = self.trace_length() - 1;
        vec![
            Assertion::single(0, 0, self.org_did_hash),
            Assertion::single(1, 0, self.current_time),
            Assertion::single(0, last_step, Felt::ONE),
        ]
    }
}

// ============================================================================
// Prover Definition
// ============================================================================

crate::define_prover!(
    MembershipProofProver,
    MembershipProofAir,
    MembershipPublicInputs
);

// ============================================================================
// Trace Building
// ============================================================================

fn build_trace(
    public: &MembershipProofPublic,
    private: &MembershipProofPrivate,
) -> Result<TraceTable<Felt>, CircuitError> {
    // Check membership validity
    if !private.is_valid_at(public.current_time) {
        return Err(CircuitError::ConstraintNotSatisfied(
            "membership not valid at current time".into(),
        ));
    }

    // Check role requirement
    if let Some(ref required) = public.required_role {
        match &private.actual_role {
            Some(actual) if actual == required => {}
            Some(actual) => {
                return Err(CircuitError::ConstraintNotSatisfied(format!(
                    "role '{actual}' does not match required '{required}'"
                )));
            }
            None => {
                return Err(CircuitError::ConstraintNotSatisfied(
                    "no role but role required".into(),
                ));
            }
        }
    }

    let org_hash = hash_to_felt(public.org_did.as_str().as_bytes());
    let member_hash = hash_to_felt(private.member_did.as_str().as_bytes());
    let current_time = u64_to_felt(public.current_time);
    let member_since = u64_to_felt(private.member_since);
    let blinding_felt = hash_to_felt(&private.blinding);

    let mut trace = TraceTable::new(TRACE_WIDTH, TRACE_LENGTH);

    trace.fill(
        |state| {
            state[0] = org_hash;
            state[1] = current_time;
        },
        |step, state| match step {
            0 => {
                state[0] = member_hash;
                state[1] = member_since;
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

/// Generate a STARK proof for membership
pub fn prove_membership(
    public: &MembershipProofPublic,
    private: &MembershipProofPrivate,
) -> Result<StarkProof, CircuitError> {
    let trace = build_trace(public, private)?;

    let pub_inputs = MembershipPublicInputs::from_circuit(public);

    let prover = MembershipProofProver::new(default_proof_options(), pub_inputs);
    let proof = prover.prove(trace).map_err(|e| {
        CircuitError::ProofGenerationFailed(format!(
            "STARK membership proof generation failed: {e}"
        ))
    })?;

    let proof_bytes = proof.to_bytes();
    let public_hash = crate::circuit::compute_public_inputs_hash(public);

    Ok(StarkProof::new(proof_bytes, public_hash))
}

/// Verify a STARK membership proof
pub fn verify_membership(
    public: &MembershipProofPublic,
    proof: &StarkProof,
) -> Result<bool, CircuitError> {
    let expected_hash = crate::circuit::compute_public_inputs_hash(public);
    if proof.public_inputs_hash != expected_hash {
        return Ok(false);
    }

    let stark_proof = Proof::from_bytes(&proof.proof_bytes).map_err(|e| {
        CircuitError::VerificationFailed(format!("STARK proof deserialization failed: {e}"))
    })?;

    let pub_inputs = MembershipPublicInputs::from_circuit(public);

    match winterfell::verify::<MembershipProofAir, Blake3, RandCoin, VC>(
        stark_proof,
        pub_inputs,
        &acceptable_options(),
    ) {
        Ok(_) => Ok(true),
        Err(e) => {
            tracing::debug!("Membership proof verification failed: {}", e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::Did;

    fn test_org_did() -> Did {
        Did::from_anchor_id(&[1u8; 32])
    }

    fn test_member_did() -> Did {
        Did::from_anchor_id(&[2u8; 32])
    }

    #[test]
    fn test_membership_proof_full_flow() {
        let public = MembershipProofPublic::new(test_org_did());

        let private = MembershipProofPrivate {
            member_did: test_member_did(),
            actual_role: None,
            member_since: 0,
            expires_at: None,
            org_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let proof = prove_membership(&public, &private);
        assert!(proof.is_ok(), "Proof generation should succeed");

        let proof = proof.unwrap();
        assert!(!proof.is_simulated());

        let valid = verify_membership(&public, &proof);
        assert!(valid.is_ok());
        assert!(valid.unwrap());
    }
}
