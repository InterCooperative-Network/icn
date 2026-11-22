//! Portable governance proof bundle: export and offline verification (icn#2465, first slice).
//!
//! The daemon already persists, for every actor-closed proposal, the exact `serde_json` bytes of a
//! [`GovernanceProofV2`]: the canonical V1 [`GovernanceDecisionReceipt`] plus Ed25519
//! [`GovernanceDecisionAttestation`]s by the node identity key. This module adds nothing to sign.
//! It carries those stored bytes, unchanged, in a versioned bundle, and verifies them with no
//! network, no daemon and no store — only the bundle bytes and a signer the *verifier* pins.
//!
//! Four results are kept apart, because each proves something different:
//!
//! 1. **Integrity** — the carried proof bytes hash to the bundle's `proof_sha256`. This only shows
//!    the bytes were not changed *after* export by someone who did not also update the checksum;
//!    it is not authentication (the checksum travels with the bundle).
//! 2. **Decision consistency** — the receipt's `decision_hash` recomputes from its committed fields
//!    (blake3, domain tag `icn:gov:decision:v1`), and the attestation commits to that hash.
//! 3. **Authenticity** — the attestation's Ed25519 signature verifies under a public key.
//! 4. **Trust** — that key is the one decoded from the DID the verifier pinned. A valid signature
//!    by any other key is reported as untrusted, never as a pass.
//!
//! What a successful verification proves is bounded: *the pinned node attested this decision hash
//! for this proposal and outcome*. It does not prove that an institution authorized the decision —
//! a node identity cannot sign for an institution — and it does not re-derive the tally (votes are
//! committed only through `vote_hash`). Those are owner decisions recorded on icn#2465.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::proof::{GovernanceProofV2, ProofOutcome};
use crate::{MembershipAction, ProposalPayload};

/// Schema identifier of version 1 of the bundle. A bundle naming any other schema is refused.
pub const GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1: &str = "urn:icn:contract:governance-proof-bundle:v1";

/// Version 1 of the portable bundle.
///
/// Field order is the serialization order, and serialization is compact JSON with a trailing
/// newline, so the same stored proof bytes always export to the same bundle bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceProofBundleV1 {
    /// Always [`GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1`].
    pub schema: String,
    /// The proposal the carried proof is about; must equal the receipt's `proposal_id`.
    pub proposal_id: String,
    /// Lowercase hex SHA-256 of the decoded `proof_bytes_hex`.
    pub proof_sha256: String,
    /// The exact stored `GovernanceProofV2` bytes, lowercase hex. Never re-serialized.
    pub proof_bytes_hex: String,
}

/// Why a bundle could not be exported.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BundleExportError {
    /// The stored bytes are not a `GovernanceProofV2`.
    #[error("stored proof is not a GovernanceProofV2: {0}")]
    NotAProofV2(String),
    /// The stored proof is about a different proposal than the one requested.
    #[error("stored proof is for proposal {found}, not {requested}")]
    ProposalMismatch { requested: String, found: String },
    /// The bundle could not be serialized.
    #[error("could not encode bundle: {0}")]
    Encode(String),
}

/// Why a bundle failed verification. Each variant is a distinct, stable reason; none is a soft
/// pass, and the first failing check is reported.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BundleVerifyError {
    /// The bundle is not a well-formed v1 bundle (unparseable, missing or unknown fields).
    #[error("malformed bundle: {0}")]
    Malformed(String),
    /// The bundle names a schema this verifier does not implement.
    #[error("unsupported bundle schema {found:?} (expected {expected:?})")]
    SchemaMismatch {
        found: String,
        expected: &'static str,
    },
    /// The pinned signer is not a usable DID.
    #[error("pinned signer is not a usable DID: {0}")]
    InvalidPinnedSigner(String),
    /// `proof_bytes_hex` is not lowercase hex (one bundle has exactly one byte form).
    #[error("proof bytes are not lowercase hex")]
    ProofEncoding,
    /// The carried bytes do not hash to `proof_sha256`.
    #[error("integrity: proof bytes do not match proof_sha256")]
    IntegrityMismatch,
    /// The carried bytes are not a `GovernanceProofV2`.
    #[error("proof bytes are not a GovernanceProofV2: {0}")]
    ProofMalformed(String),
    /// The bundle's `proposal_id` differs from the receipt's.
    #[error("bundle proposal_id {bundle} does not match receipt proposal_id {receipt}")]
    ProposalIdMismatch { bundle: String, receipt: String },
    /// The receipt's `decision_hash` does not recompute from its committed fields.
    #[error("decision: receipt decision_hash does not match its committed fields")]
    DecisionHashMismatch,
    /// The proof carries no attestation at all (e.g. a legacy-converted proof): nothing signs it.
    #[error("authenticity: proof carries no attestation")]
    NoAttestation,
    /// No attestation is by the pinned signer's key.
    #[error("trust: no attestation by the pinned signer {pinned}")]
    UntrustedSigner { pinned: String },
    /// The pinned signer's attestation commits to a different decision hash.
    #[error("decision: the pinned signer's attestation is for a different decision_hash")]
    AttestationDecisionMismatch,
    /// The pinned signer's attestation signature does not verify under the pinned key.
    #[error("authenticity: the pinned signer's attestation signature is invalid")]
    InvalidSignature,
    /// A payload binding is present but does not belong to this decision: a different decision
    /// hash, a non-accepted outcome, or a signer other than the pinned one.
    #[error("decision: payload binding does not belong to this decision ({0})")]
    PayloadBindingMismatch(String),
    /// The payload binding's signature does not verify under the pinned key (e.g. the payload
    /// text was changed after signing).
    #[error("authenticity: payload binding signature is invalid")]
    PayloadBindingInvalidSignature,
    /// A subject was expected, but the proof binds no payload (a non-accepted outcome, or a proof
    /// written before bindings existed): it cannot say what was decided.
    #[error("subject: proof binds no payload, so it cannot show what was decided")]
    PayloadUnbound,
    /// The bound payload is not the expected membership change.
    #[error("subject: bound payload is {found}, expected {expected}")]
    SubjectMismatch { expected: String, found: String },
}

impl BundleVerifyError {
    /// A stable machine-readable code for each reason.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "malformed",
            Self::SchemaMismatch { .. } => "schema_mismatch",
            Self::InvalidPinnedSigner(_) => "invalid_pinned_signer",
            Self::ProofEncoding => "proof_encoding",
            Self::IntegrityMismatch => "integrity_mismatch",
            Self::ProofMalformed(_) => "proof_malformed",
            Self::ProposalIdMismatch { .. } => "proposal_id_mismatch",
            Self::DecisionHashMismatch => "decision_hash_mismatch",
            Self::NoAttestation => "no_attestation",
            Self::UntrustedSigner { .. } => "untrusted_signer",
            Self::AttestationDecisionMismatch => "attestation_decision_mismatch",
            Self::InvalidSignature => "invalid_signature",
            Self::PayloadBindingMismatch(_) => "payload_binding_mismatch",
            Self::PayloadBindingInvalidSignature => "payload_binding_invalid_signature",
            Self::PayloadUnbound => "payload_unbound",
            Self::SubjectMismatch { .. } => "subject_mismatch",
        }
    }
}

/// What a successfully verified bundle establishes — and, by its `claim`, what it does not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VerifiedGovernanceProof {
    pub schema: &'static str,
    pub proposal_id: String,
    pub domain_id: String,
    pub outcome: ProofOutcome,
    /// Lowercase hex of the verified `decision_hash`.
    pub decision_hash: String,
    /// The pinned signer, as given to the verifier.
    pub attested_by: String,
    /// The attestation's own timestamp (signed, but not part of `decision_hash`).
    pub attested_at: u64,
    pub proof_sha256: String,
    /// The bounded meaning of a pass.
    pub claim: &'static str,
    /// The decided payload, when the pinned node bound it to this decision (accepted outcomes
    /// from binding-aware producers). Parsed from the exact signed text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    /// Lowercase hex blake3 of the exact signed payload text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_hash: Option<String>,
    /// Unix seconds at which the pinned node closed the decision and bound the payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_at: Option<u64>,
    /// What the payload binding adds to `claim`, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_claim: Option<&'static str>,
}

/// What a verified payload binding adds.
pub const VERIFIED_PAYLOAD_CLAIM: &str = "the same pinned node states that this exact payload is \
     what the decision in domain_id accepted; it does not show the change is still current (a \
     later removal is a separate decision, and no expiry exists), who may decide is the domain's \
     membership rule, and no institutional key signs it";

/// The bounded meaning of a successful verification.
pub const VERIFIED_CLAIM: &str = "the pinned node identity attested this governance decision hash \
     for this proposal and outcome; this is not proof that an institution authorized the \
     decision, and the vote tally is committed only through vote_hash";

/// Key under which the governance store (`{data_dir}/store/governance`) persists the
/// `GovernanceProofV2` bytes of a closed proposal. The governance actor writes it; exporters read
/// it. One definition, so the two cannot drift.
pub fn stored_proof_key(proposal_id: &str) -> Vec<u8> {
    format!("governance:proof:{proposal_id}").into_bytes()
}

/// Export stored `GovernanceProofV2` bytes, unchanged, as a v1 bundle.
///
/// Refuses bytes that are not a proof for `proposal_id`, so an export never ships something the
/// verifier would reject for being the wrong artifact. Deterministic: same inputs, same bytes.
pub fn export_bundle(
    proposal_id: &str,
    stored_proof_bytes: &[u8],
) -> Result<Vec<u8>, BundleExportError> {
    let proof: GovernanceProofV2 = serde_json::from_slice(stored_proof_bytes)
        .map_err(|e| BundleExportError::NotAProofV2(e.to_string()))?;
    if proof.receipt.proposal_id != proposal_id {
        return Err(BundleExportError::ProposalMismatch {
            requested: proposal_id.to_string(),
            found: proof.receipt.proposal_id,
        });
    }
    let bundle = GovernanceProofBundleV1 {
        schema: GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1.to_string(),
        proposal_id: proposal_id.to_string(),
        proof_sha256: hex::encode(Sha256::digest(stored_proof_bytes)),
        proof_bytes_hex: hex::encode(stored_proof_bytes),
    };
    let mut bytes =
        serde_json::to_vec(&bundle).map_err(|e| BundleExportError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Verify a bundle offline against a signer the verifier pins.
///
/// Uses only `bundle_bytes` and `pinned_signer`: no network, no daemon, no store.
pub fn verify_bundle(
    bundle_bytes: &[u8],
    pinned_signer: &str,
) -> Result<VerifiedGovernanceProof, BundleVerifyError> {
    // Schema first, so a future version is reported as such rather than as a field error.
    let value: serde_json::Value = serde_json::from_slice(bundle_bytes)
        .map_err(|e| BundleVerifyError::Malformed(e.to_string()))?;
    match value.get("schema").and_then(|s| s.as_str()) {
        Some(GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1) => {}
        Some(other) => {
            return Err(BundleVerifyError::SchemaMismatch {
                found: other.to_string(),
                expected: GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1,
            })
        }
        None => {
            return Err(BundleVerifyError::Malformed(
                "missing field `schema`".into(),
            ))
        }
    }
    // Typed parse from the original bytes, not from `value`: a `Value` keeps the last of
    // duplicate keys silently, so one file could carry two bundles and different verifiers
    // would report different decisions. The derived deserializer refuses duplicate fields.
    let bundle: GovernanceProofBundleV1 = serde_json::from_slice(bundle_bytes)
        .map_err(|e| BundleVerifyError::Malformed(e.to_string()))?;
    if !is_lower_hex(&bundle.proof_sha256) || bundle.proof_sha256.len() != 64 {
        return Err(BundleVerifyError::Malformed(
            "proof_sha256 is not 64 lowercase hex digits".into(),
        ));
    }

    let pinned_key = Did::from_str(pinned_signer)
        .and_then(|did| did.to_verifying_key())
        .map_err(|e| BundleVerifyError::InvalidPinnedSigner(e.to_string()))?;

    // 1. Integrity of the carried bytes.
    if !is_lower_hex(&bundle.proof_bytes_hex) {
        return Err(BundleVerifyError::ProofEncoding);
    }
    let proof_bytes =
        hex::decode(&bundle.proof_bytes_hex).map_err(|_| BundleVerifyError::ProofEncoding)?;
    if hex::encode(Sha256::digest(&proof_bytes)) != bundle.proof_sha256 {
        return Err(BundleVerifyError::IntegrityMismatch);
    }
    let proof: GovernanceProofV2 = serde_json::from_slice(&proof_bytes)
        .map_err(|e| BundleVerifyError::ProofMalformed(e.to_string()))?;

    // 2. Decision consistency.
    if proof.receipt.proposal_id != bundle.proposal_id {
        return Err(BundleVerifyError::ProposalIdMismatch {
            bundle: bundle.proposal_id,
            receipt: proof.receipt.proposal_id,
        });
    }
    if !proof.verify_receipt() {
        return Err(BundleVerifyError::DecisionHashMismatch);
    }

    // 3 + 4. Authenticity under the pinned key, and only the pinned key.
    if proof.attestations.is_empty() {
        return Err(BundleVerifyError::NoAttestation);
    }
    let pinned_bytes = pinned_key.to_bytes();
    let attestation = proof
        .attestations
        .iter()
        .find(|a| {
            Did::from_str(&a.signer_did)
                .and_then(|did| did.to_verifying_key())
                .map(|key| key.to_bytes() == pinned_bytes)
                .unwrap_or(false)
        })
        .ok_or_else(|| BundleVerifyError::UntrustedSigner {
            pinned: pinned_signer.to_string(),
        })?;
    if attestation.decision_hash != proof.receipt.decision_hash {
        return Err(BundleVerifyError::AttestationDecisionMismatch);
    }
    if !attestation.verify(&pinned_key) {
        return Err(BundleVerifyError::InvalidSignature);
    }

    // 5. The payload binding. Only a binding by the pinned key is this verifier's evidence; one
    //    by any other key is ignored (the proof is unbound here), never trusted and never fatal.
    //    A pinned binding must belong to this decision and verify, or the bundle is refused.
    //    Absence is not a failure here; `expect_membership` makes it one.
    let pinned_binding = proof.payload_binding.as_ref().filter(|binding| {
        Did::from_str(&binding.signer_did)
            .and_then(|did| did.to_verifying_key())
            .map(|key| key.to_bytes() == pinned_bytes)
            .unwrap_or(false)
    });
    let (payload, payload_hash, bound_at, payload_claim) = match pinned_binding {
        None => (None, None, None, None),
        Some(binding) => {
            if binding.decision_hash != proof.receipt.decision_hash {
                return Err(BundleVerifyError::PayloadBindingMismatch(
                    "different decision_hash".into(),
                ));
            }
            if proof.receipt.outcome != ProofOutcome::Accepted {
                return Err(BundleVerifyError::PayloadBindingMismatch(
                    "binding on a decision that was not accepted".into(),
                ));
            }
            if !binding.verify(&pinned_key) {
                return Err(BundleVerifyError::PayloadBindingInvalidSignature);
            }
            // Strict typed parse of the exact signed text: the derived deserializer refuses
            // duplicate fields, so no lenient reader can find a different member in it. A payload
            // this build cannot parse (e.g. a newer variant) leaves the proof unbound for this
            // verifier rather than failing a plain verify; `expect_membership` then refuses it.
            match serde_json::from_str::<ProposalPayload>(&binding.payload_json) {
                Err(_) => (None, None, None, None),
                Ok(parsed) => match serde_json::to_value(&parsed) {
                    Err(_) => (None, None, None, None),
                    Ok(value) => (
                        Some(value),
                        Some(hex::encode(binding.payload_hash())),
                        Some(binding.decided_at),
                        Some(VERIFIED_PAYLOAD_CLAIM),
                    ),
                },
            }
        }
    };

    Ok(VerifiedGovernanceProof {
        schema: GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1,
        proposal_id: proof.receipt.proposal_id.clone(),
        domain_id: proof.receipt.domain_id.clone(),
        outcome: proof.receipt.outcome,
        decision_hash: hex::encode(proof.receipt.decision_hash),
        attested_by: pinned_signer.to_string(),
        attested_at: attestation.timestamp,
        proof_sha256: bundle.proof_sha256,
        claim: VERIFIED_CLAIM,
        payload,
        payload_hash,
        bound_at,
        payload_claim,
    })
}

/// Require that a verified proof binds exactly this membership change.
///
/// `member` is compared by key equality, so any spelling of the expected DID matches; an
/// unusable expected DID is reported as a mismatch, never a pass.
pub fn expect_membership(
    verified: &VerifiedGovernanceProof,
    action: MembershipAction,
    member: &str,
    domain: Option<&str>,
) -> Result<(), BundleVerifyError> {
    let expected = format!("membership {action:?} {member}");
    // The membership payload names no domain; the domain is committed by the decision hash.
    // Without pinning it, an admission into any domain the node votes in would pass.
    if let Some(domain) = domain {
        if verified.domain_id != domain {
            return Err(BundleVerifyError::SubjectMismatch {
                expected: format!("{expected} in domain {domain}"),
                found: format!("a decision in domain {}", verified.domain_id),
            });
        }
    }
    let payload = verified
        .payload
        .as_ref()
        .ok_or(BundleVerifyError::PayloadUnbound)?;
    let parsed: ProposalPayload = serde_json::from_value(payload.clone())
        .map_err(|e| BundleVerifyError::ProofMalformed(format!("bound payload: {e}")))?;
    let ProposalPayload::Membership {
        action: found_action,
        member: found_member,
    } = parsed
    else {
        return Err(BundleVerifyError::SubjectMismatch {
            expected,
            found: "a non-membership payload".into(),
        });
    };
    let expected_did = Did::from_str(member).map_err(|_| BundleVerifyError::SubjectMismatch {
        expected: expected.clone(),
        found: "(expected member is not a usable DID)".into(),
    })?;
    if found_action != action || found_member != expected_did {
        return Err(BundleVerifyError::SubjectMismatch {
            expected,
            found: format!("membership {found_action:?} {found_member}"),
        });
    }
    Ok(())
}
