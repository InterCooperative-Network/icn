//! ZK Prover
//!
//! High-level prover interface for generating zero-knowledge proofs.

use crate::accumulator::RsaAccumulator;
use crate::circuit::{
    AgeProofCircuit, AgeProofPrivate, AgeProofPublic, Circuit, CircuitError,
    CitizenshipProofCircuit, CitizenshipProofPrivate, CitizenshipProofPublic,
    MembershipProofCircuit, MembershipProofPrivate, MembershipProofPublic, NonRevocationCircuit,
    NonRevocationPrivate, NonRevocationPublic,
};
use crate::types::{
    AgeAttestation, Attestation, CitizenshipAttestation, CompoundProof, MembershipAttestation,
    ProofContext, ProofType, StarkProof,
};
use icn_identity::Did;
use thiserror::Error;

/// Errors from proof generation
#[derive(Debug, Error)]
pub enum ProverError {
    #[error("Circuit error: {0}")]
    Circuit(#[from] CircuitError),

    #[error("Invalid attestation: {0}")]
    InvalidAttestation(String),

    #[error("Attestation expired")]
    AttestationExpired,

    #[error("Missing required attestation")]
    MissingAttestation,

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// ZK prover for generating attribute proofs
pub struct ZkProver {
    /// Optional issuer public key for validation
    trusted_issuers: Vec<[u8; 32]>,
}

impl Default for ZkProver {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkProver {
    /// Create a new prover
    pub fn new() -> Self {
        Self {
            trusted_issuers: Vec::new(),
        }
    }

    /// Add a trusted issuer public key
    pub fn add_trusted_issuer(&mut self, pk: [u8; 32]) {
        if !self.trusted_issuers.contains(&pk) {
            self.trusted_issuers.push(pk);
        }
    }

    /// Generate an age proof
    pub fn prove_age(
        &self,
        threshold: u8,
        attestation: &AgeAttestation,
        issuer_pk: [u8; 32],
        context: &ProofContext,
    ) -> Result<StarkProof, ProverError> {
        let public = AgeProofPublic {
            threshold,
            current_date: (context.timestamp / 86400) as u32,
            issuer_pk,
            nonce: context.nonce,
        };

        let private = AgeProofPrivate {
            birthdate_days: attestation.birthdate_days,
            issuer_signature: attestation.signature.clone(),
            blinding: rand::random(),
        };

        Ok(AgeProofCircuit::prove(&public, &private)?)
    }

    /// Generate a citizenship proof
    pub fn prove_citizenship(
        &self,
        country_code: [u8; 2],
        attestation: &CitizenshipAttestation,
        issuer_pk: [u8; 32],
        context: &ProofContext,
    ) -> Result<StarkProof, ProverError> {
        use crate::circuit::citizenship::StatusType;

        let status_type = match attestation.status_type {
            crate::types::CitizenshipStatus::Citizen => StatusType::Citizen,
            crate::types::CitizenshipStatus::PermanentResident => StatusType::PermanentResident,
            crate::types::CitizenshipStatus::TemporaryResident => StatusType::TemporaryResident,
            crate::types::CitizenshipStatus::VisaHolder => StatusType::VisaHolder,
        };

        let public = CitizenshipProofPublic {
            country_code,
            minimum_status: StatusType::Citizen, // Prove citizenship specifically
            issuer_pk,
            nonce: context.nonce,
        };

        let private = CitizenshipProofPrivate {
            actual_country: attestation.country_code,
            actual_status: status_type,
            issuer_signature: attestation.signature.clone(),
            subject_id_hash: rand::random(),
            blinding: rand::random(),
        };

        Ok(CitizenshipProofCircuit::prove(&public, &private)?)
    }

    /// Generate a membership proof
    pub fn prove_membership(
        &self,
        org_did: Did,
        attestation: &MembershipAttestation,
        member_did: Did,
        context: &ProofContext,
    ) -> Result<StarkProof, ProverError> {
        let public = MembershipProofPublic {
            org_did: org_did.clone(),
            required_role: None,
            current_time: context.timestamp,
            nonce: context.nonce,
        };

        let private = MembershipProofPrivate {
            member_did,
            actual_role: attestation.role.clone(),
            member_since: attestation.member_since,
            expires_at: None,
            org_signature: attestation.signature.clone(),
            blinding: rand::random(),
        };

        Ok(MembershipProofCircuit::prove(&public, &private)?)
    }

    /// Generate a non-revocation proof
    pub fn prove_non_revocation(
        &self,
        credential_id: [u8; 32],
        accumulator: &RsaAccumulator,
        revoked_credentials: &[[u8; 32]],
        issuer_pk: [u8; 32],
        context: &ProofContext,
    ) -> Result<StarkProof, ProverError> {
        // Get non-membership witness from accumulator
        let witness = accumulator
            .non_membership_witness(&credential_id, revoked_credentials)
            .map_err(|e| ProverError::InvalidAttestation(e.to_string()))?;

        let public = NonRevocationPublic {
            accumulator_value: accumulator.value_hash(),
            accumulator_epoch: accumulator.epoch(),
            issuer_pk,
            nonce: context.nonce,
        };

        let private = NonRevocationPrivate {
            credential_id,
            witness: crate::circuit::non_revocation::NonMembershipWitness {
                a: bincode::serialize(&witness.a).unwrap_or_default(),
                b: bincode::serialize(&witness.d).unwrap_or_default(),
                aux: credential_id,
            },
            blinding: rand::random(),
        };

        Ok(NonRevocationCircuit::prove(&public, &private)?)
    }

    /// Generate a compound proof (attribute + non-revocation)
    pub fn prove_compound(
        &self,
        proof_type: ProofType,
        attestation: &Attestation,
        credential_id: [u8; 32],
        accumulator: &RsaAccumulator,
        revoked_credentials: &[[u8; 32]],
    ) -> Result<CompoundProof, ProverError> {
        if attestation.is_expired() {
            return Err(ProverError::AttestationExpired);
        }

        let context = ProofContext::new(None);

        // Get issuer's public key (from attestation or lookup)
        let issuer_pk = [0u8; 32]; // TODO: Extract from attestation

        // Generate attribute proof based on type
        let attribute_proof = match &proof_type {
            ProofType::AgeAtLeast { threshold } => {
                let age_att: AgeAttestation =
                    bincode::deserialize(&attestation.payload).map_err(|e| {
                        ProverError::InvalidAttestation(format!("age attestation: {e}"))
                    })?;
                self.prove_age(*threshold, &age_att, issuer_pk, &context)?
            }
            ProofType::Citizenship { country_code } => {
                let cit_att: CitizenshipAttestation = bincode::deserialize(&attestation.payload)
                    .map_err(|e| {
                        ProverError::InvalidAttestation(format!("citizenship attestation: {e}"))
                    })?;
                self.prove_citizenship(*country_code, &cit_att, issuer_pk, &context)?
            }
            ProofType::Membership { org_did } => {
                let mem_att: MembershipAttestation = bincode::deserialize(&attestation.payload)
                    .map_err(|e| {
                        ProverError::InvalidAttestation(format!("membership attestation: {e}"))
                    })?;
                let holder_did = Did::from_anchor_id(&[0u8; 32]); // TODO: Get from keybundle
                self.prove_membership(org_did.clone(), &mem_att, holder_did, &context)?
            }
            ProofType::NonRevocation => self.prove_non_revocation(
                credential_id,
                accumulator,
                revoked_credentials,
                issuer_pk,
                &context,
            )?,
            ProofType::Custom { .. } => {
                return Err(ProverError::InvalidAttestation(
                    "custom circuits not yet supported".into(),
                ));
            }
        };

        // Generate non-revocation proof
        let non_revocation_proof = Some(self.prove_non_revocation(
            credential_id,
            accumulator,
            revoked_credentials,
            issuer_pk,
            &context,
        )?);

        Ok(CompoundProof {
            attribute_proof,
            non_revocation_proof,
            context,
            proof_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prover_creation() {
        let prover = ZkProver::new();
        assert!(prover.trusted_issuers.is_empty());
    }

    #[test]
    fn test_add_trusted_issuer() {
        let mut prover = ZkProver::new();
        let pk = [1u8; 32];

        prover.add_trusted_issuer(pk);
        assert_eq!(prover.trusted_issuers.len(), 1);

        // Adding same issuer shouldn't duplicate
        prover.add_trusted_issuer(pk);
        assert_eq!(prover.trusted_issuers.len(), 1);
    }

    #[test]
    fn test_prove_age() {
        let prover = ZkProver::new();
        let context = ProofContext::new(None);
        let issuer_pk = [1u8; 32];

        // Someone born 20 years ago
        let birthdate_days = ((context.timestamp / 86400) as u32) - (20 * 365);
        let attestation = AgeAttestation {
            birthdate_days,
            signature: vec![0u8; 64],
        };

        let proof = prover.prove_age(18, &attestation, issuer_pk, &context);
        assert!(proof.is_ok());
    }

    #[test]
    fn test_prove_non_revocation() {
        let prover = ZkProver::new();
        let context = ProofContext::new(None);
        let issuer_pk = [1u8; 32];

        let mut accumulator = RsaAccumulator::new_test();
        let revoked = [1u8; 32];
        accumulator.add(&revoked);

        let not_revoked = [2u8; 32];
        let revoked_list = vec![revoked];

        let proof = prover.prove_non_revocation(
            not_revoked,
            &accumulator,
            &revoked_list,
            issuer_pk,
            &context,
        );
        assert!(proof.is_ok());
    }
}
