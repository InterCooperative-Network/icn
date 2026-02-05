//! Trust attestations for network propagation
//!
//! This module provides signed trust attestations that can be broadcast
//! over the gossip network to build distributed trust webs.

use crate::types::TrustGraphType;
use crate::{TrustEdge, TrustScore};
use anyhow::Result;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hex;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Maximum number of labels allowed in an attestation (DoS prevention).
///
/// Labels are metadata tags (e.g., "partner", "validator") that can be
/// attached to attestations. We limit this to prevent malicious nodes from
/// creating attestations with thousands of labels that would consume
/// excessive memory and network bandwidth.
///
/// Rationale: Typical attestations use 1-3 labels. 10 allows for rich
/// multi-role attestations while preventing abuse. At ~128 bytes per label,
/// 10 labels ≈ 1.3KB which is negligible per gossip message.
pub const MAX_LABELS: usize = 10;

/// Maximum number of evidence references allowed in an attestation (DoS prevention).
///
/// Evidence references are content hashes that support an attestation.
/// We limit this to prevent memory exhaustion attacks while still allowing
/// reasonable evidence documentation.
///
/// Rationale: Evidence entries are typically content-addressed hashes (32-64 bytes).
/// 20 entries allows thorough provenance documentation. At ~256 bytes per entry,
/// 20 entries ≈ 5KB which fits comfortably within gossip message size limits.
pub const MAX_EVIDENCE: usize = 20;

/// Maximum length in bytes for a single label string (DoS prevention).
///
/// Prevents memory exhaustion via a few very long label strings.
/// Gossip message size limits provide primary protection, but this is
/// defense-in-depth for well-formed payloads with oversized individual fields.
pub const MAX_LABEL_LENGTH: usize = 128;

/// Maximum length in bytes for a single evidence reference string (DoS prevention).
///
/// Evidence strings are typically content-addressed hashes (hex or base58).
/// 256 bytes allows generous formatting while preventing abuse.
pub const MAX_EVIDENCE_LENGTH: usize = 256;

/// A signed trust attestation that can be broadcast over the network
///
/// This represents a cryptographically signed statement: "I (issuer) trust
/// (subject) with score X, valid until TTL expires."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustAttestation {
    /// The DID making the trust statement (same as trust edge source)
    pub issuer: Did,

    /// The DID being trusted (same as trust edge target)
    pub subject: Did,

    /// Trust score (0.0-1.0)
    pub score: f64,

    /// Optional labels (e.g., "partner", "validator", "service-provider")
    #[serde(default)]
    pub labels: Vec<String>,

    /// Optional evidence references (content hashes)
    #[serde(default)]
    pub evidence: Vec<String>,

    /// Time-to-live in seconds (how long this attestation is valid)
    /// Default: 30 days (2,592,000 seconds)
    pub ttl_seconds: u64,

    /// Unix timestamp when this attestation was created
    pub created_at: u64,

    /// Monotonic sequence number for replay protection
    ///
    /// Each issuer maintains a monotonic counter. Receivers track the last
    /// seen sequence per issuer and reject attestations with stale sequences.
    ///
    /// **Migration**: Legacy attestations without this field are treated as
    /// sequence 0 during deserialization for backward compatibility.
    /// Attestations with `sequence == 0` bypass replay protection to allow
    /// a grace period for nodes to upgrade.  A future release will reject
    /// `sequence == 0` after a migration deadline.
    #[serde(default)]
    pub sequence: u64,

    /// Ed25519 signature over the canonical representation
    ///
    /// V3 (current): issuer || subject || score || ttl || created_at || sequence || graph_type || labels || evidence
    /// V2 (legacy):  issuer || subject || score || ttl || created_at || graph_type || labels || evidence
    /// V1 (legacy):  issuer || subject || score || ttl || created_at || labels || evidence
    ///
    /// The verify() method supports all formats for backward compatibility.
    pub signature: Vec<u8>,

    /// The type of trust graph this attestation belongs to
    ///
    /// Defaults to `Social` for backward compatibility with attestations
    /// created before multi-graph support was added.
    #[serde(default)]
    pub graph_type: TrustGraphType,
}

/// A signed trust revocation that can be broadcast over the network
///
/// This represents a cryptographically signed statement: "I (issuer) revoke
/// my previous trust attestation for (subject) as of this timestamp."
///
/// Revocations win over any attestation with an earlier timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustRevocation {
    /// The DID revoking the trust statement
    pub issuer: Did,

    /// The DID whose trust is being revoked
    pub subject: Did,

    /// Unix timestamp when this revocation was created
    pub revoked_at: u64,

    /// Optional reason for revocation (for auditing)
    #[serde(default)]
    pub reason: Option<String>,

    /// The type of trust graph this revocation applies to
    #[serde(default)]
    pub graph_type: TrustGraphType,

    /// Ed25519 signature over the canonical representation
    /// Payload: issuer || subject || revoked_at || graph_type || reason (if present)
    pub signature: Vec<u8>,
}

impl TrustAttestation {
    /// Large TTL used for non-expiring edges converted from storage.
    /// `u64::MAX / 2` avoids overflow in arithmetic while remaining far
    /// larger than any realistic expiration.
    pub const NON_EXPIRING_TTL: u64 = u64::MAX / 2;

    /// Create a new unsigned attestation (defaults to Social graph type)
    ///
    /// Note: This returns an attestation with an empty signature.
    /// You must call `sign()` before broadcasting.
    pub fn new(issuer: Did, subject: Did, score: f64) -> Self {
        Self::new_typed(issuer, subject, score, TrustGraphType::Social)
    }

    /// Create a new unsigned attestation with explicit graph type
    ///
    /// Note: This returns an attestation with an empty signature.
    /// You must call `sign()` before broadcasting.
    pub fn new_typed(issuer: Did, subject: Did, score: f64, graph_type: TrustGraphType) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            issuer,
            subject,
            score,
            labels: Vec::new(),
            evidence: Vec::new(),
            ttl_seconds: 30 * 24 * 60 * 60, // 30 days default
            created_at: now,
            sequence: 0, // Will be set by the caller before signing
            signature: Vec::new(),
            graph_type,
        }
    }

    /// Add a label to this attestation
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }

    /// Add evidence to this attestation
    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence.push(evidence.into());
        self
    }

    /// Set custom TTL
    pub fn with_ttl(mut self, ttl_seconds: u64) -> Self {
        self.ttl_seconds = ttl_seconds;
        self
    }

    /// Set the graph type
    pub fn with_graph_type(mut self, graph_type: TrustGraphType) -> Self {
        self.graph_type = graph_type;
        self
    }

    /// Set the sequence number
    ///
    /// The sequence number should be set before signing to enable replay protection.
    /// Typically, the issuer increments a counter stored in persistent storage.
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    /// Get the canonical signing payload (v3 format, includes sequence)
    ///
    /// The signature covers the core attestation fields in a deterministic order:
    /// issuer || subject || score || ttl || created_at || sequence || graph_type || labels || evidence
    fn signing_payload(&self) -> Vec<u8> {
        self.signing_payload_v3()
    }

    /// V3 signing payload - includes sequence (replay protection)
    fn signing_payload_v3(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();

        // Core fields
        hasher.update(self.issuer.as_str().as_bytes());
        hasher.update(self.subject.as_str().as_bytes());
        hasher.update(self.score.to_le_bytes());
        hasher.update(self.ttl_seconds.to_le_bytes());
        hasher.update(self.created_at.to_le_bytes());

        // Sequence number (added for replay protection)
        hasher.update(self.sequence.to_le_bytes());

        // Graph type (added for multi-graph support)
        hasher.update(self.graph_type.as_str().as_bytes());

        // Labels (sorted for determinism)
        let mut sorted_labels = self.labels.clone();
        sorted_labels.sort();
        for label in sorted_labels {
            hasher.update(label.as_bytes());
        }

        // Evidence (sorted for determinism)
        let mut sorted_evidence = self.evidence.clone();
        sorted_evidence.sort();
        for ev in sorted_evidence {
            hasher.update(ev.as_bytes());
        }

        hasher.finalize().to_vec()
    }

    /// V2 signing payload - includes graph_type (multi-graph support)
    fn signing_payload_v2(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();

        // Core fields
        hasher.update(self.issuer.as_str().as_bytes());
        hasher.update(self.subject.as_str().as_bytes());
        hasher.update(self.score.to_le_bytes());
        hasher.update(self.ttl_seconds.to_le_bytes());
        hasher.update(self.created_at.to_le_bytes());

        // Note: No sequence in v2 format

        // Graph type (added for multi-graph support)
        hasher.update(self.graph_type.as_str().as_bytes());

        // Labels (sorted for determinism)
        let mut sorted_labels = self.labels.clone();
        sorted_labels.sort();
        for label in sorted_labels {
            hasher.update(label.as_bytes());
        }

        // Evidence (sorted for determinism)
        let mut sorted_evidence = self.evidence.clone();
        sorted_evidence.sort();
        for ev in sorted_evidence {
            hasher.update(ev.as_bytes());
        }

        hasher.finalize().to_vec()
    }

    /// V1 signing payload - legacy format without graph_type (backward compatibility)
    fn signing_payload_v1(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();

        // Core fields (same as v2 but without graph_type)
        hasher.update(self.issuer.as_str().as_bytes());
        hasher.update(self.subject.as_str().as_bytes());
        hasher.update(self.score.to_le_bytes());
        hasher.update(self.ttl_seconds.to_le_bytes());
        hasher.update(self.created_at.to_le_bytes());

        // Note: No sequence or graph_type in v1 format

        // Labels (sorted for determinism)
        let mut sorted_labels = self.labels.clone();
        sorted_labels.sort();
        for label in sorted_labels {
            hasher.update(label.as_bytes());
        }

        // Evidence (sorted for determinism)
        let mut sorted_evidence = self.evidence.clone();
        sorted_evidence.sort();
        for ev in sorted_evidence {
            hasher.update(ev.as_bytes());
        }

        hasher.finalize().to_vec()
    }

    /// Sign this attestation with a keypair
    ///
    /// This creates an Ed25519 signature over the canonical representation
    /// and stores it in the `signature` field.
    pub fn sign(&mut self, keypair: &icn_identity::KeyPair) -> Result<()> {
        if keypair.did() != &self.issuer {
            anyhow::bail!(
                "Keypair DID ({}) does not match attestation issuer ({})",
                keypair.did(),
                self.issuer
            );
        }

        let payload = self.signing_payload();
        let sig = keypair.sign(&payload);
        self.signature = sig.to_bytes().to_vec();

        Ok(())
    }

    /// Verify the signature on this attestation
    ///
    /// Extracts the verifying key from the issuer DID and checks the signature.
    ///
    /// For backward compatibility, this method tries v3 (with sequence), v2 (with
    /// graph_type), and v1 (legacy) signing formats. This ensures that attestations
    /// signed before replay protection or multi-graph support will still verify.
    pub fn verify(&self) -> Result<()> {
        if self.signature.is_empty() {
            anyhow::bail!("Attestation has no signature");
        }

        if self.signature.len() != 64 {
            anyhow::bail!(
                "Invalid signature length: {} (expected 64)",
                self.signature.len()
            );
        }

        // Extract public key from issuer DID
        let verifying_key = self.extract_verifying_key_from_did(&self.issuer)?;

        // Parse signature
        let sig_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert signature to array"))?;
        let signature = Signature::from_bytes(&sig_bytes);

        // Try v3 format first (with sequence)
        let payload_v3 = self.signing_payload_v3();
        if verifying_key.verify(&payload_v3, &signature).is_ok() {
            return Ok(());
        }

        // Fall back to v2 format (with graph_type, no sequence)
        let payload_v2 = self.signing_payload_v2();
        if verifying_key.verify(&payload_v2, &signature).is_ok() {
            return Ok(());
        }

        // Fall back to v1 format for backward compatibility (without graph_type or sequence)
        // This handles attestations signed before multi-graph support was added
        let payload_v1 = self.signing_payload_v1();
        verifying_key
            .verify(&payload_v1, &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {e}"))?;

        Ok(())
    }

    /// Check if this attestation has expired
    pub fn is_expired(&self, now: u64) -> bool {
        now > self.created_at + self.ttl_seconds
    }

    /// Validate attestation size limits (DoS prevention)
    ///
    /// Checks that labels and evidence arrays are within acceptable bounds
    /// to prevent memory exhaustion attacks. Returns an error if limits are exceeded.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `labels.len() > MAX_LABELS` (default: 10)
    /// - `evidence.len() > MAX_EVIDENCE` (default: 20)
    pub fn validate_size_limits(&self) -> Result<()> {
        if self.labels.len() > MAX_LABELS {
            return Err(anyhow::anyhow!(
                "Too many labels: {} (max: {})",
                self.labels.len(),
                MAX_LABELS
            ));
        }
        if self.evidence.len() > MAX_EVIDENCE {
            return Err(anyhow::anyhow!(
                "Too many evidence entries: {} (max: {})",
                self.evidence.len(),
                MAX_EVIDENCE
            ));
        }
        // Check individual string lengths (defense-in-depth against oversized fields)
        for (i, label) in self.labels.iter().enumerate() {
            if label.len() > MAX_LABEL_LENGTH {
                return Err(anyhow::anyhow!(
                    "Label[{}] too long: {} bytes (max: {})",
                    i,
                    label.len(),
                    MAX_LABEL_LENGTH
                ));
            }
        }
        for (i, evidence) in self.evidence.iter().enumerate() {
            if evidence.len() > MAX_EVIDENCE_LENGTH {
                return Err(anyhow::anyhow!(
                    "Evidence[{}] too long: {} bytes (max: {})",
                    i,
                    evidence.len(),
                    MAX_EVIDENCE_LENGTH
                ));
            }
        }
        Ok(())
    }

    /// Calculate the expiration timestamp
    pub fn expires_at(&self) -> u64 {
        self.created_at + self.ttl_seconds
    }

    /// Check if this attestation should supersede another attestation
    ///
    /// This method implements replay protection by considering BOTH timestamp and score
    /// in a two-tier ordering: timestamp takes precedence (outside clock skew tolerance),
    /// with score as a tiebreaker (within tolerance window).
    ///
    /// ## Ordering Semantics
    ///
    /// The supersession logic follows these rules in order:
    ///
    /// 1. **Clearly newer** (time difference > 5 minutes): Accept, regardless of score
    /// 2. **Clearly older** (time difference < -5 minutes): Reject, regardless of score
    /// 3. **Within tolerance window** (time difference within ±5 minutes):
    ///    - If scores differ significantly (> 0.001): Higher score wins
    ///    - If scores are equal (within 0.001): Newer timestamp wins
    ///
    /// ## Clock Skew Tolerance
    ///
    /// Distributed systems face clock synchronization challenges. If Node A's clock is
    /// +5 minutes ahead and Node B's clock is -5 minutes behind, a 10-minute ordering
    /// window exists where conflicting attestations could arrive out of order.
    ///
    /// **Solution:**
    /// - Accept attestation if `created_at` is newer by more than tolerance (5 minutes)
    /// - Within tolerance window: use trust score as tiebreaker (higher trust wins)
    /// - This prevents clock skew from causing incorrect rejections while maintaining
    ///   general chronological ordering
    ///
    /// ## Replay Attack Protection
    ///
    /// **Primary Defense:** Monotonic timestamp checking prevents replay of old attestations
    /// with high scores. An old attestation will NOT supersede a newer attestation, even if
    /// the old attestation has a higher trust score.
    ///
    /// **Example Scenario:**
    /// - Attacker records: `attestation(score=0.9, created_at=1000)`
    /// - Victim creates: `attestation(score=0.1, created_at=10000)` (revocation ~2.5 hours later)
    /// - Old attestation has `time_diff = 1000 - 10000 = -9000 seconds`
    /// - Since `-9000 < -300` (clock skew tolerance), the old attestation is rejected
    /// - Result: ✅ Replay attack prevented by timestamp checking
    ///
    /// **Vulnerability Window:** Within the 5-minute clock skew tolerance window, a higher-score
    /// attestation CAN supersede a lower-score one. This is by design to handle:
    /// - Race conditions where nodes update trust simultaneously
    /// - Clock skew between nodes causing near-simultaneous attestations to arrive out of order
    ///
    /// **Mitigations for tolerance window vulnerability:**
    /// 1. Short TTLs (default 30 days) - old attestations expire
    /// 2. Active re-attestation requirement - trust must be continuously maintained
    /// 3. Key rotation (future) - invalidates old signatures after rotation
    /// 4. Narrow tolerance window (5 minutes) - limits replay opportunity
    ///
    /// ## Parameters
    ///
    /// - `other_created_at`: Timestamp of existing attestation (Unix seconds)
    /// - `other_score`: Trust score of existing attestation (0.0-1.0)
    ///
    /// ## Returns
    ///
    /// `true` if this attestation should supersede the other, `false` otherwise
    pub fn should_supersede(&self, other_created_at: u64, other_score: f64) -> bool {
        const CLOCK_SKEW_TOLERANCE_SECS: i64 = 300; // 5 minutes
        /// Score difference below which two scores are considered equal.
        /// Chosen to absorb floating-point rounding while remaining well below
        /// any meaningful trust difference.
        const SCORE_EPSILON: f64 = 0.001;

        let time_diff = self.created_at as i64 - other_created_at as i64;

        // Clearly newer: accept
        if time_diff > CLOCK_SKEW_TOLERANCE_SECS {
            return true;
        }

        // Clearly older: reject
        if time_diff < -CLOCK_SKEW_TOLERANCE_SECS {
            return false;
        }

        // Within tolerance window: use score as tiebreaker
        // Higher trust wins. If scores are equal, accept the newer one (even if within tolerance)
        if (self.score - other_score).abs() < SCORE_EPSILON {
            // Scores effectively equal - accept newer timestamp
            time_diff >= 0
        } else {
            // Use score to break tie
            self.score >= other_score
        }
    }

    /// Convert this attestation to a TrustEdge for storage
    ///
    /// Note: Attestation string evidence is converted to `TrustEvidence::Legacy`
    /// during this conversion. For typed evidence, use `to_trust_edge_with_evidence()`.
    pub fn to_trust_edge(&self) -> TrustEdge {
        use crate::evidence::TrustEvidence;

        // Convert string evidence to legacy evidence type
        let typed_evidence: Vec<TrustEvidence> = self
            .evidence
            .iter()
            .map(|e| TrustEvidence::from_legacy_string(e.clone()))
            .collect();

        TrustEdge {
            source: self.issuer.clone(),
            target: self.subject.clone(),
            labels: self.labels.clone(),
            score: TrustScore::unchecked(self.score),
            evidence: typed_evidence,
            legacy_evidence: Vec::new(), // Converted to typed evidence above
            expires_at: Some(self.expires_at()),
            created_at: self.created_at,
            graph_type: self.graph_type,
            scope_id: None, // Attestations default to global scope
        }
    }

    /// Convert this attestation to a TrustEdge with provided typed evidence
    ///
    /// Use this when you have properly typed evidence to associate with the edge.
    pub fn to_trust_edge_with_evidence(
        &self,
        evidence: Vec<crate::evidence::TrustEvidence>,
    ) -> TrustEdge {
        TrustEdge {
            source: self.issuer.clone(),
            target: self.subject.clone(),
            labels: self.labels.clone(),
            score: TrustScore::unchecked(self.score),
            evidence,
            legacy_evidence: Vec::new(),
            expires_at: Some(self.expires_at()),
            created_at: self.created_at,
            graph_type: self.graph_type,
            scope_id: None, // Attestations default to global scope
        }
    }

    /// Create an attestation from a TrustEdge (before signing)
    ///
    /// Note: Typed evidence is converted to string references for the attestation
    /// wire format. Legacy evidence strings are extracted from `TrustEvidence::Legacy`.
    pub fn from_trust_edge(edge: &TrustEdge) -> Self {
        use crate::evidence::TrustEvidence;

        let ttl_seconds =
            edge.expires_at
                .map(|exp| {
                    // Validate expiration time is in the future
                    // Warn on extreme clock skew (>30 days in the past)
                    if exp < edge.created_at {
                        tracing::warn!(
                        "Trust edge expires_at ({}) is before created_at ({}), treating as expired",
                        exp, edge.created_at
                    );
                        0 // Already expired
                    } else {
                        let ttl = exp.saturating_sub(edge.created_at);
                        // Warn on suspiciously long TTL (>10 years)
                        const MAX_REASONABLE_TTL: u64 = 10 * 365 * 24 * 60 * 60;
                        if ttl > MAX_REASONABLE_TTL {
                            tracing::warn!(
                                "Trust edge has suspiciously long TTL: {} seconds ({} years)",
                                ttl,
                                ttl / (365 * 24 * 60 * 60)
                            );
                        }
                        ttl
                    }
                })
                // Large TTL for non-expiring edges so they survive is_expired() checks.
                .unwrap_or(Self::NON_EXPIRING_TTL);

        // Convert typed evidence to string references for attestation
        let evidence: Vec<String> = edge
            .evidence
            .iter()
            .filter_map(|e| match e {
                TrustEvidence::Legacy { reference, .. } => Some(reference.clone()),
                TrustEvidence::ContractExecution {
                    contract_id,
                    agreement_id,
                    ..
                } => agreement_id
                    .clone()
                    .or_else(|| Some(hex::encode(contract_id))),
                TrustEvidence::LedgerTransaction { entry_hash, .. } => {
                    Some(hex::encode(entry_hash))
                }
                TrustEvidence::GovernanceVote { proposal_id, .. } => Some(proposal_id.clone()),
                TrustEvidence::ExternalAttestation { attestation_id, .. } => {
                    Some(attestation_id.clone())
                }
                TrustEvidence::PeerEndorsement { endorser, .. } => Some(endorser.to_string()),
                TrustEvidence::TechnicalObservation {
                    observer,
                    metric_type,
                    ..
                } => Some(format!("{}:{}", observer, metric_type.as_str())),
            })
            .collect();

        Self {
            issuer: edge.source.clone(),
            subject: edge.target.clone(),
            score: edge.score.value(),
            labels: edge.labels.clone(),
            evidence,
            ttl_seconds,
            created_at: edge.created_at,
            sequence: 0, // Will be set by the caller before signing
            signature: Vec::new(),
            graph_type: edge.graph_type,
        }
    }

    /// Extract verifying key from a DID
    fn extract_verifying_key_from_did(&self, did: &Did) -> Result<VerifyingKey> {
        extract_verifying_key_from_did(did)
    }
}

/// Shared helper to extract an Ed25519 verifying key from a DID.
///
/// DIDs are in the format: `did:icn:<multibase-encoded-pubkey>`
fn extract_verifying_key_from_did(did: &Did) -> Result<VerifyingKey> {
    let did_str = did.as_str();

    if !did_str.starts_with("did:icn:") {
        anyhow::bail!("Invalid DID format: {did_str}");
    }

    let encoded = &did_str[8..]; // Skip "did:icn:"
    let (_base, decoded) = multibase::decode(encoded)?;

    if decoded.len() != 32 {
        anyhow::bail!("Invalid public key length: {} (expected 32)", decoded.len());
    }

    let key_bytes: [u8; 32] = decoded
        .try_into()
        .map_err(|_| anyhow::anyhow!("Failed to convert to 32-byte array"))?;

    Ok(VerifyingKey::from_bytes(&key_bytes)?)
}

impl TrustRevocation {
    /// Create a new unsigned revocation
    ///
    /// Note: This returns a revocation with an empty signature.
    /// You must call `sign()` before broadcasting.
    pub fn new(issuer: Did, subject: Did) -> Self {
        Self::new_typed(issuer, subject, TrustGraphType::Social)
    }

    /// Create a new unsigned revocation with explicit graph type
    pub fn new_typed(issuer: Did, subject: Did, graph_type: TrustGraphType) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            issuer,
            subject,
            revoked_at: now,
            reason: None,
            graph_type,
            signature: Vec::new(),
        }
    }

    /// Add a reason for this revocation
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the graph type
    pub fn with_graph_type(mut self, graph_type: TrustGraphType) -> Self {
        self.graph_type = graph_type;
        self
    }

    /// Get the canonical signing payload
    ///
    /// The signature covers: issuer || subject || revoked_at || graph_type || reason (if present)
    fn signing_payload(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();

        hasher.update(self.issuer.as_str().as_bytes());
        hasher.update(self.subject.as_str().as_bytes());
        hasher.update(self.revoked_at.to_le_bytes());
        hasher.update(self.graph_type.as_str().as_bytes());

        if let Some(ref reason) = self.reason {
            hasher.update(reason.as_bytes());
        }

        hasher.finalize().to_vec()
    }

    /// Sign this revocation with a keypair
    ///
    /// This creates an Ed25519 signature over the canonical representation
    /// and stores it in the `signature` field.
    pub fn sign(&mut self, keypair: &icn_identity::KeyPair) -> Result<()> {
        if keypair.did() != &self.issuer {
            anyhow::bail!(
                "Keypair DID ({}) does not match revocation issuer ({})",
                keypair.did(),
                self.issuer
            );
        }

        let payload = self.signing_payload();
        let sig = keypair.sign(&payload);
        self.signature = sig.to_bytes().to_vec();

        Ok(())
    }

    /// Verify the signature on this revocation
    ///
    /// Extracts the verifying key from the issuer DID and checks the signature.
    pub fn verify(&self) -> Result<()> {
        if self.signature.is_empty() {
            anyhow::bail!("Revocation has no signature");
        }

        if self.signature.len() != 64 {
            anyhow::bail!(
                "Invalid signature length: {} (expected 64)",
                self.signature.len()
            );
        }

        // Extract public key from issuer DID
        let verifying_key = self.extract_verifying_key_from_did(&self.issuer)?;

        // Parse signature
        let sig_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert signature to array"))?;
        let signature = Signature::from_bytes(&sig_bytes);

        // Verify signature
        let payload = self.signing_payload();
        verifying_key
            .verify(&payload, &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {e}"))?;

        Ok(())
    }

    /// Check if this revocation should supersede an attestation
    ///
    /// A revocation supersedes any attestation with an earlier timestamp.
    /// This provides strong revocation semantics: once revoked, previous
    /// attestations are invalid regardless of their trust score.
    pub fn supersedes_attestation(&self, attestation_created_at: u64) -> bool {
        self.revoked_at >= attestation_created_at
    }

    /// Extract verifying key from a DID
    fn extract_verifying_key_from_did(&self, did: &Did) -> Result<VerifyingKey> {
        extract_verifying_key_from_did(did)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_attestation_creation() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.75)
            .with_label("partner")
            .with_evidence("contract_xyz");

        assert_eq!(attestation.issuer, *alice.did());
        assert_eq!(attestation.subject, *bob.did());
        assert_eq!(attestation.score, 0.75);
        assert_eq!(attestation.labels, vec!["partner"]);
        assert_eq!(attestation.evidence, vec!["contract_xyz"]);
        assert_eq!(attestation.ttl_seconds, 30 * 24 * 60 * 60);
    }

    #[test]
    fn test_attestation_sign_and_verify() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        // Sign
        attestation.sign(&alice).unwrap();

        // Verify
        assert!(attestation.verify().is_ok());
    }

    #[test]
    fn test_attestation_verify_fails_wrong_signature() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let eve = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        // Sign with Eve's key (wrong issuer)
        attestation.issuer = eve.did().clone();
        attestation.sign(&eve).unwrap();

        // Change issuer back to Alice
        attestation.issuer = alice.did().clone();

        // Verify should fail
        assert!(attestation.verify().is_err());
    }

    #[test]
    fn test_attestation_verify_fails_tampered_score() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        attestation.sign(&alice).unwrap();

        // Tamper with score
        attestation.score = 0.9;

        // Verify should fail
        assert!(attestation.verify().is_err());
    }

    #[test]
    fn test_attestation_expiry() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let attestation =
            TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5).with_ttl(3600); // 1 hour

        let now = attestation.created_at;

        assert!(!attestation.is_expired(now));
        assert!(!attestation.is_expired(now + 1800)); // 30 minutes later
        assert!(attestation.is_expired(now + 3601)); // 1 hour + 1 second later
    }

    #[test]
    fn test_attestation_to_trust_edge() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.6)
            .with_label("validator")
            .with_ttl(7200);

        let edge = attestation.to_trust_edge();

        assert_eq!(edge.source, attestation.issuer);
        assert_eq!(edge.target, attestation.subject);
        assert_eq!(edge.score.value(), attestation.score);
        assert_eq!(edge.labels, attestation.labels);
        assert_eq!(edge.expires_at, Some(attestation.expires_at()));
    }

    #[test]
    fn test_attestation_from_trust_edge() {
        use crate::evidence::TrustEvidence;

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Create a typed evidence item
        let evidence = TrustEvidence::ContractExecution {
            contract_id: [0u8; 32],
            agreement_id: Some("collaboration_proof".to_string()),
            executed_at: 12345,
        };

        let mut edge = TrustEdge::new(
            alice.did().clone(),
            bob.did().clone(),
            TrustScore::unchecked(0.8),
        )
        .with_label("partner")
        .with_evidence(evidence);

        let expiry = edge.created_at + 86400; // 1 day
        edge = edge.with_expiry(expiry);

        let attestation = TrustAttestation::from_trust_edge(&edge);

        assert_eq!(attestation.issuer, edge.source);
        assert_eq!(attestation.subject, edge.target);
        assert_eq!(attestation.score, edge.score.value());
        assert_eq!(attestation.labels, edge.labels);
        // Attestation extracts string references from typed evidence
        assert_eq!(attestation.evidence.len(), 1);
        assert_eq!(attestation.evidence[0], "collaboration_proof");
        assert_eq!(attestation.ttl_seconds, 86400);
    }

    #[test]
    fn test_signing_payload_determinism() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Create two attestations with same data but labels in different order
        let mut att1 = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);
        att1.labels = vec!["b".to_string(), "a".to_string()];
        att1.created_at = 1000;

        let mut att2 = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);
        att2.labels = vec!["a".to_string(), "b".to_string()];
        att2.created_at = 1000;

        // Signing payloads should be identical (labels get sorted)
        assert_eq!(att1.signing_payload(), att2.signing_payload());
    }

    #[test]
    fn test_sign_wrong_keypair() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let eve = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        // Try to sign with Eve's keypair (should fail)
        let result = attestation.sign(&eve);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not match attestation issuer"));
    }

    #[test]
    fn test_should_supersede_clearly_newer() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 1000u64;

        let old_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.5,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let new_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.5,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 400, // 6.67 minutes newer (> 5 min tolerance)
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        assert!(new_att.should_supersede(old_att.created_at, old_att.score));
        assert!(!old_att.should_supersede(new_att.created_at, new_att.score));
    }

    #[test]
    fn test_should_supersede_within_tolerance_higher_score() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 1000u64;

        let low_trust_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.3,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let high_trust_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.8,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 60, // 1 minute newer (within tolerance)
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        // Higher trust should supersede even if only slightly newer
        assert!(high_trust_att.should_supersede(low_trust_att.created_at, low_trust_att.score));

        // Lower trust should NOT supersede even if slightly newer
        assert!(!low_trust_att.should_supersede(high_trust_att.created_at, high_trust_att.score));
    }

    #[test]
    fn test_should_supersede_within_tolerance_equal_scores() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 1000u64;

        let old_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.5,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let new_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.5, // Same score
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 60, // 1 minute newer (within tolerance)
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        // With equal scores, newer timestamp wins
        assert!(new_att.should_supersede(old_att.created_at, old_att.score));
        assert!(!old_att.should_supersede(new_att.created_at, new_att.score));
    }

    #[test]
    fn test_should_supersede_clock_skew_scenario() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Simulate Node A with clock +5 minutes ahead
        let node_a_time = 1000u64 + 300; // +5 minutes

        // Simulate Node B with clock -5 minutes behind
        let node_b_time = 1000u64 - 300; // -5 minutes

        let att_from_node_a = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.7,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: node_a_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let att_from_node_b = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.6,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: node_b_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        // Node A's attestation appears 10 minutes newer, but that's within 2x tolerance
        // The 10-minute gap is exactly at the boundary (2 * 5 minutes)
        // With these specific timestamps, neither clearly supersedes by time alone
        // So score should be the tiebreaker

        // Node A (higher score 0.7) should supersede Node B (lower score 0.6)
        assert!(att_from_node_a.should_supersede(att_from_node_b.created_at, att_from_node_b.score));
    }

    #[test]
    fn test_should_supersede_trust_downgrade() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 1000u64;

        // Alice initially trusts Bob highly
        let high_trust = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.9,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        // Alice downgrades trust significantly
        let low_trust = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.2,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 3600, // 1 hour later (well outside tolerance)
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        // Downgrade should supersede because it's much newer
        assert!(low_trust.should_supersede(high_trust.created_at, high_trust.score));

        // Old high trust should not supersede newer low trust
        assert!(!high_trust.should_supersede(low_trust.created_at, low_trust.score));
    }

    // ====================================================================
    // REPLAY PROTECTION TESTS (Issue #877, PR #987 follow-up)
    // ====================================================================

    #[test]
    fn test_replay_protection_old_high_score_rejected() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 10_000u64;

        let old_high_trust = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.95,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let new_low_trust = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.05,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 7200,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        assert!(
            !old_high_trust.should_supersede(new_low_trust.created_at, new_low_trust.score),
            "OLD attestation with higher score MUST NOT supersede NEWER attestation (replay protection)"
        );

        assert!(
            new_low_trust.should_supersede(old_high_trust.created_at, old_high_trust.score),
            "NEWER attestation should supersede OLDER attestation"
        );
    }

    #[test]
    fn test_replay_protection_outside_tolerance_window() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 20_000u64;

        let old_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.9,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let test_cases = vec![
            (400, 0.1),
            (600, 0.2),
            (1800, 0.3),
            (3600, 0.05),
            (86400, 0.0),
        ];

        for (time_delta, new_score) in test_cases {
            let new_att = TrustAttestation {
                issuer: alice.did().clone(),
                subject: bob.did().clone(),
                score: new_score,
                labels: vec![],
                evidence: vec![],
                ttl_seconds: 86400,
                created_at: base_time + time_delta,
                sequence: 0,
                signature: vec![],
                graph_type: TrustGraphType::default(),
            };

            assert!(
                !old_att.should_supersede(new_att.created_at, new_att.score),
                "Old attestation (score={}) should NOT supersede newer attestation (created +{}s, score={})",
                old_att.score, time_delta, new_score
            );

            assert!(
                new_att.should_supersede(old_att.created_at, old_att.score),
                "Newer attestation (created +{}s, score={}) SHOULD supersede old attestation (score={})",
                time_delta, new_score, old_att.score
            );
        }
    }

    #[test]
    fn test_identical_timestamp_behavior() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let timestamp = 30_000u64;

        let low_score_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.3,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: timestamp,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let high_score_att = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.8,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: timestamp,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        assert!(
            high_score_att.should_supersede(low_score_att.created_at, low_score_att.score),
            "Higher score should supersede lower score with identical timestamp"
        );

        assert!(
            !low_score_att.should_supersede(high_score_att.created_at, high_score_att.score),
            "Lower score should NOT supersede higher score with identical timestamp"
        );

        let att_a = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.5,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: timestamp,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let att_b = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.5,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: timestamp,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        assert!(
            att_a.should_supersede(att_b.created_at, att_b.score),
            "With identical timestamp and score, first should supersede"
        );

        assert!(
            att_b.should_supersede(att_a.created_at, att_a.score),
            "With identical timestamp and score, both return true (time_diff = 0)"
        );
    }

    #[test]
    fn test_within_tolerance_score_tiebreaker() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 40_000u64;

        let older_high_score = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.9,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let newer_low_score = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.3,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 120,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        assert!(
            older_high_score.should_supersede(newer_low_score.created_at, newer_low_score.score),
            "Within tolerance window, older high score should supersede newer low score"
        );

        assert!(
            !newer_low_score.should_supersede(older_high_score.created_at, older_high_score.score),
            "Within tolerance window, newer low score should NOT supersede older high score"
        );

        let boundary_high_score = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.85,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let boundary_low_score = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.4,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 300,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        assert!(
            boundary_high_score
                .should_supersede(boundary_low_score.created_at, boundary_low_score.score),
            "At tolerance boundary, higher score should supersede"
        );

        let just_outside = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.1,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time + 301,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let older_high = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.9,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 0,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        assert!(
            just_outside.should_supersede(older_high.created_at, older_high.score),
            "Just outside tolerance (301s), newer should win even with lower score"
        );

        assert!(
            !older_high.should_supersede(just_outside.created_at, just_outside.score),
            "Just outside tolerance (301s), older should NOT supersede newer"
        );
    }

    // ============================================================================
    // Sequence Number Tests (Replay Protection)
    // ============================================================================

    #[test]
    fn test_attestation_with_sequence() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation =
            TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5).with_sequence(42);

        attestation.sign(&alice).unwrap();

        assert!(attestation.verify().is_ok());
        assert_eq!(attestation.sequence, 42);
    }

    #[test]
    fn test_attestation_sequence_in_signature() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut att1 =
            TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5).with_sequence(1);
        att1.sign(&alice).unwrap();

        let mut att2 =
            TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5).with_sequence(2);
        att2.sign(&alice).unwrap();

        assert_ne!(att1.signature, att2.signature);
        assert!(att1.verify().is_ok());
        assert!(att2.verify().is_ok());

        let mut att1_tampered = att1.clone();
        att1_tampered.sequence = 999;
        assert!(att1_tampered.verify().is_err());
    }

    #[test]
    fn test_attestation_backward_compatibility_no_sequence() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);
        assert_eq!(attestation.sequence, 0);

        attestation.sign(&alice).unwrap();
        assert!(attestation.verify().is_ok());
    }

    // ============================================================================
    // Trust Revocation Tests
    // ============================================================================

    #[test]
    fn test_revocation_creation() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let revocation = TrustRevocation::new(alice.did().clone(), bob.did().clone())
            .with_reason("Breach of contract");

        assert_eq!(revocation.issuer, *alice.did());
        assert_eq!(revocation.subject, *bob.did());
        assert_eq!(revocation.reason, Some("Breach of contract".to_string()));
        assert_eq!(revocation.graph_type, TrustGraphType::Social);
    }

    #[test]
    fn test_revocation_sign_and_verify() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut revocation = TrustRevocation::new(alice.did().clone(), bob.did().clone());

        revocation.sign(&alice).unwrap();
        assert!(!revocation.signature.is_empty());
        assert!(revocation.verify().is_ok());
    }

    #[test]
    fn test_revocation_verify_fails_wrong_signature() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let eve = KeyPair::generate().unwrap();

        let mut revocation = TrustRevocation::new(alice.did().clone(), bob.did().clone());
        revocation.issuer = eve.did().clone();
        revocation.sign(&eve).unwrap();
        revocation.issuer = alice.did().clone();
        assert!(revocation.verify().is_err());
    }

    #[test]
    fn test_revocation_verify_fails_tampered_subject() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let carol = KeyPair::generate().unwrap();

        let mut revocation = TrustRevocation::new(alice.did().clone(), bob.did().clone());
        revocation.sign(&alice).unwrap();
        revocation.subject = carol.did().clone();
        assert!(revocation.verify().is_err());
    }

    #[test]
    fn test_revocation_supersedes_attestation() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let base_time = 1000u64;

        let attestation = TrustAttestation {
            issuer: alice.did().clone(),
            subject: bob.did().clone(),
            score: 0.9,
            labels: vec![],
            evidence: vec![],
            ttl_seconds: 86400,
            created_at: base_time,
            sequence: 1,
            signature: vec![],
            graph_type: TrustGraphType::default(),
        };

        let mut revocation = TrustRevocation::new(alice.did().clone(), bob.did().clone());
        revocation.revoked_at = base_time + 3600;

        assert!(revocation.supersedes_attestation(attestation.created_at));

        revocation.revoked_at = base_time;
        assert!(revocation.supersedes_attestation(attestation.created_at));

        revocation.revoked_at = base_time - 1;
        assert!(!revocation.supersedes_attestation(attestation.created_at));
    }

    #[test]
    fn test_revocation_with_graph_type() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut revocation = TrustRevocation::new_typed(
            alice.did().clone(),
            bob.did().clone(),
            TrustGraphType::EconomicReliability,
        );

        revocation.sign(&alice).unwrap();
        assert!(revocation.verify().is_ok());
        assert_eq!(revocation.graph_type, TrustGraphType::EconomicReliability);
    }

    #[test]
    fn test_revocation_sign_wrong_keypair() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let eve = KeyPair::generate().unwrap();

        let mut revocation = TrustRevocation::new(alice.did().clone(), bob.did().clone());

        let result = revocation.sign(&eve);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not match revocation issuer"));
    }

    #[test]
    fn test_validate_size_limits_accepts_within_limits() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        // Add MAX_LABELS labels
        for i in 0..MAX_LABELS {
            attestation.labels.push(format!("label_{i}"));
        }

        // Add MAX_EVIDENCE evidence entries
        for i in 0..MAX_EVIDENCE {
            attestation.evidence.push(format!("evidence_{i}"));
        }

        let result = attestation.validate_size_limits();
        assert!(result.is_ok(), "Should accept attestation at size limits");
    }

    #[test]
    fn test_validate_size_limits_rejects_too_many_labels() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        // Add MAX_LABELS + 1 labels
        for i in 0..=MAX_LABELS {
            attestation.labels.push(format!("label_{i}"));
        }

        let result = attestation.validate_size_limits();
        assert!(
            result.is_err(),
            "Should reject attestation with too many labels"
        );
        assert!(result.unwrap_err().to_string().contains("Too many labels"));
    }

    #[test]
    fn test_validate_size_limits_rejects_too_many_evidence() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        // Add MAX_EVIDENCE + 1 evidence entries
        for i in 0..=MAX_EVIDENCE {
            attestation.evidence.push(format!("evidence_{i}"));
        }

        let result = attestation.validate_size_limits();
        assert!(
            result.is_err(),
            "Should reject attestation with too many evidence entries"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Too many evidence"));
    }

    #[test]
    fn test_validate_size_limits_empty_arrays() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);

        let result = attestation.validate_size_limits();
        assert!(
            result.is_ok(),
            "Should accept attestation with empty arrays"
        );
    }

    #[test]
    fn test_validate_size_limits_rejects_long_label() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);
        attestation
            .labels
            .push("x".repeat(super::MAX_LABEL_LENGTH + 1));

        let result = attestation.validate_size_limits();
        assert!(result.is_err(), "Should reject attestation with long label");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Label[0] too long"));
    }

    #[test]
    fn test_validate_size_limits_rejects_long_evidence() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.5);
        attestation
            .evidence
            .push("x".repeat(super::MAX_EVIDENCE_LENGTH + 1));

        let result = attestation.validate_size_limits();
        assert!(
            result.is_err(),
            "Should reject attestation with long evidence"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Evidence[0] too long"));
    }
}
