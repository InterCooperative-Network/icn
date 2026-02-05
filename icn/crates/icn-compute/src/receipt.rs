//! Execution receipts for compute task metering and settlement.
//!
//! An [`ExecutionReceipt`] captures the resource usage of a completed task
//! and carries a chain of Ed25519 signatures: executor → submitter → attester.
//! Each signature uses a domain-separated payload to prevent cross-role reuse.

use ed25519_dalek::Signer;
use icn_kernel_api::ScopeLevel;
use serde::{Deserialize, Serialize};

use crate::error::ComputeError;

/// Blake3 hash identifying a receipt by its content (non-signature fields).
pub type ReceiptHash = [u8; 32];

// ---------------------------------------------------------------------------
// Domain-separation prefixes (byte constants, single source of truth)
// ---------------------------------------------------------------------------

/// Prefix for executor signing payload.
pub const SIGN_PREFIX_EXECUTOR: &[u8] = b"icn-receipt-executor-v1:";

/// Prefix for submitter acknowledgement payload.
pub const SIGN_PREFIX_SUBMITTER: &[u8] = b"icn-receipt-submitter-v1:";

/// Prefix for attester signing payload.
pub const SIGN_PREFIX_ATTESTER: &[u8] = b"icn-receipt-attester-v1:";

// ---------------------------------------------------------------------------
// SignatureBytes — fixed-size Ed25519 signature with serde support
// ---------------------------------------------------------------------------

/// A 64-byte Ed25519 signature with hex-based serde serialization.
///
/// Using a newtype instead of raw `[u8; 64]` gives us:
/// - Type-level length enforcement (no accidental truncation)
/// - Deterministic hex serialization (readable in JSON, stable across platforms)
/// - Clean `From`/`Into` conversions with `ed25519_dalek::Signature`
///
/// `Copy` is intentionally **not** derived — signatures should not be
/// implicitly copied, only explicitly cloned or borrowed via [`as_bytes()`].
///
/// The inner field is private to enforce that all access goes through
/// explicit methods, preserving the non-Copy invariant.
#[derive(Clone, PartialEq, Eq)]
pub struct SignatureBytes([u8; 64]);

impl SignatureBytes {
    /// View as a reference to the underlying 64-byte array.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Return an owned copy of the underlying 64-byte array.
    ///
    /// This makes creating owned copies an explicit operation, preserving
    /// the intent behind not implementing `Copy` for `SignatureBytes`.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0
    }
}

impl std::fmt::Debug for SignatureBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sig({}..)", hex::encode(&self.0[..4]))
    }
}

impl Serialize for SignatureBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&hex::encode(self.0))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for SignatureBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
            let arr: [u8; 64] = bytes
                .try_into()
                .map_err(|_| serde::de::Error::custom("expected 64 bytes for Ed25519 signature"))?;
            Ok(SignatureBytes(arr))
        } else {
            let bytes = <Vec<u8>>::deserialize(deserializer)?;
            let arr: [u8; 64] = bytes
                .try_into()
                .map_err(|_| serde::de::Error::custom("expected 64 bytes for Ed25519 signature"))?;
            Ok(SignatureBytes(arr))
        }
    }
}

impl From<[u8; 64]> for SignatureBytes {
    fn from(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

impl From<ed25519_dalek::Signature> for SignatureBytes {
    fn from(sig: ed25519_dalek::Signature) -> Self {
        Self(sig.to_bytes())
    }
}

// ---------------------------------------------------------------------------
// Protocol-stable scope encoding
// ---------------------------------------------------------------------------

/// Encode [`ScopeLevel`] as a protocol-stable byte.
///
/// This explicit match decouples the receipt wire format from the enum's
/// `#[repr(u8)]` discriminants, preventing silent breakage if kernel-api
/// reorders or adds variants.
fn scope_to_wire(scope: ScopeLevel) -> u8 {
    match scope {
        ScopeLevel::Local => 0,
        ScopeLevel::Cell => 1,
        ScopeLevel::Org => 2,
        ScopeLevel::Federation => 3,
        ScopeLevel::Commons => 4,
    }
}

// ---------------------------------------------------------------------------
// ExecutionReceipt
// ---------------------------------------------------------------------------

/// Maximum valid CPU time: 24 hours in milliseconds.
const MAX_CPU_MILLIS: u64 = 86_400_000;

/// Maximum valid memory: 1 TB-second in megabyte-milliseconds (1_000_000 MB * 1000 ms).
const MAX_MEMORY_MB_MILLIS: u64 = 1_000_000_000;

/// Maximum valid storage: 1 TB.
const MAX_STORAGE_BYTES: u64 = 1_099_511_627_776;

/// Maximum valid egress: 100 GB.
const MAX_EGRESS_BYTES: u64 = 107_374_182_400;

/// Minimum plausible Unix timestamp (2020-09-13).
const MIN_VALID_TIMESTAMP: u64 = 1_600_000_000;

/// Maximum allowed clock skew into the future, in seconds.
const MAX_CLOCK_SKEW_SECS: u64 = 300;

/// A signed attestation of resource consumption for a completed compute task.
///
/// The receipt binds metering data to a chain of three signatures:
///
/// 1. **Executor** — the node that ran the task, signs the raw metering.
/// 2. **Submitter** — the node that submitted the task, acknowledges the cost.
/// 3. **Attester** — an independent witness, vouches for the receipt.
///
/// All metering values use deterministic integer units to guarantee
/// bit-exact signing payloads across architectures.
///
/// The `receipt_id` field is a random 32-byte nonce that provides replay
/// protection. Even if all other fields are identical, a new nonce
/// produces a unique signing payload and receipt hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionReceipt {
    // -- Identity --
    /// Random 32-byte nonce for replay protection.
    ///
    /// Must be unique per receipt. Ensures that identical tasks produce
    /// distinct receipts and signing payloads.
    pub receipt_id: [u8; 32],
    /// Blake3 hash of the original [`ComputeTask`].
    pub task_hash: [u8; 32],
    /// DID of the executor node (e.g. `did:icn:z6Mk...`).
    pub executor: String,
    /// DID of the task submitter.
    pub submitter: String,
    /// Scope at which this task was executed.
    pub scope: ScopeLevel,

    // -- Metering (deterministic integer units) --
    /// CPU time consumed, in milliseconds.
    pub cpu_millis: u64,
    /// Memory usage, in megabyte-milliseconds.
    pub memory_mb_millis: u64,
    /// Persistent storage consumed, in bytes.
    pub storage_bytes: u64,
    /// Network egress, in bytes.
    pub egress_bytes: u64,

    // -- Cost --
    /// Fuel consumed during execution.
    pub fuel_used: u64,
    /// Total cost in the settlement currency's smallest unit.
    pub cost: u64,

    // -- Signatures (Ed25519 = 64 bytes fixed) --
    /// Executor's signature over the base signing payload.
    pub executor_signature: Option<SignatureBytes>,
    /// Submitter's acknowledgement signature (includes executor sig in payload).
    pub submitter_ack: Option<SignatureBytes>,
    /// DID of the independent attester, if present.
    pub attester: Option<String>,
    /// Attester's signature (includes both prior sigs in payload).
    pub attester_signature: Option<SignatureBytes>,

    // -- Timestamp --
    /// Unix timestamp (seconds) when the receipt was created.
    pub created_at: u64,
}

impl ExecutionReceipt {
    /// Deterministic byte payload of all non-signature fields.
    ///
    /// This is the content that signatures bind to. Field order is fixed and
    /// all variable-length strings are length-prefixed to prevent ambiguity.
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);

        // Nonce (replay protection)
        buf.extend_from_slice(&self.receipt_id);

        // Identity
        buf.extend_from_slice(&self.task_hash);
        buf.extend_from_slice(&(self.executor.len() as u32).to_le_bytes());
        buf.extend_from_slice(self.executor.as_bytes());
        buf.extend_from_slice(&(self.submitter.len() as u32).to_le_bytes());
        buf.extend_from_slice(self.submitter.as_bytes());
        buf.push(scope_to_wire(self.scope));

        // Metering
        buf.extend_from_slice(&self.cpu_millis.to_le_bytes());
        buf.extend_from_slice(&self.memory_mb_millis.to_le_bytes());
        buf.extend_from_slice(&self.storage_bytes.to_le_bytes());
        buf.extend_from_slice(&self.egress_bytes.to_le_bytes());

        // Cost
        buf.extend_from_slice(&self.fuel_used.to_le_bytes());
        buf.extend_from_slice(&self.cost.to_le_bytes());

        // Timestamp
        buf.extend_from_slice(&self.created_at.to_le_bytes());

        buf
    }

    /// Content hash of the receipt (blake3 over the signing payload).
    ///
    /// Includes `receipt_id` nonce, so identical tasks with different nonces
    /// produce different hashes. Used as the deduplication key for settlement.
    pub fn receipt_hash(&self) -> ReceiptHash {
        *blake3::hash(&self.signing_payload()).as_bytes()
    }

    /// Returns `true` if all three signatures are present.
    pub fn is_fully_attested(&self) -> bool {
        self.executor_signature.is_some()
            && self.submitter_ack.is_some()
            && self.attester.is_some()
            && self.attester_signature.is_some()
    }

    /// Validate structural invariants (DID format, timestamp, metering bounds).
    pub fn validate(&self) -> Result<(), ComputeError> {
        // DID format
        if !self.executor.starts_with("did:icn:") {
            return Err(ComputeError::InvalidInput(format!(
                "executor DID has invalid format: {}",
                self.executor
            )));
        }
        if !self.submitter.starts_with("did:icn:") {
            return Err(ComputeError::InvalidInput(format!(
                "submitter DID has invalid format: {}",
                self.submitter
            )));
        }
        if let Some(ref attester) = self.attester {
            if !attester.starts_with("did:icn:") {
                return Err(ComputeError::InvalidInput(format!(
                    "attester DID has invalid format: {attester}"
                )));
            }
        }

        // Timestamp bounds
        if self.created_at < MIN_VALID_TIMESTAMP {
            return Err(ComputeError::InvalidInput(format!(
                "created_at {} is before minimum valid timestamp {}",
                self.created_at, MIN_VALID_TIMESTAMP
            )));
        }
        let now = icn_time::current_timestamp_secs();
        if self.created_at > now + MAX_CLOCK_SKEW_SECS {
            return Err(ComputeError::InvalidInput(format!(
                "created_at {} is more than {}s in the future (now={})",
                self.created_at, MAX_CLOCK_SKEW_SECS, now
            )));
        }

        // Metering bounds
        if self.cpu_millis > MAX_CPU_MILLIS {
            return Err(ComputeError::InvalidInput(format!(
                "cpu_millis {} exceeds maximum {}",
                self.cpu_millis, MAX_CPU_MILLIS
            )));
        }
        if self.memory_mb_millis > MAX_MEMORY_MB_MILLIS {
            return Err(ComputeError::InvalidInput(format!(
                "memory_mb_millis {} exceeds maximum {}",
                self.memory_mb_millis, MAX_MEMORY_MB_MILLIS
            )));
        }
        if self.storage_bytes > MAX_STORAGE_BYTES {
            return Err(ComputeError::InvalidInput(format!(
                "storage_bytes {} exceeds maximum {}",
                self.storage_bytes, MAX_STORAGE_BYTES
            )));
        }
        if self.egress_bytes > MAX_EGRESS_BYTES {
            return Err(ComputeError::InvalidInput(format!(
                "egress_bytes {} exceeds maximum {}",
                self.egress_bytes, MAX_EGRESS_BYTES
            )));
        }

        Ok(())
    }

    // -- Signing (builder-style; executor infallible, later steps return Result) --

    /// Sign as executor. Sets `executor_signature`.
    ///
    /// This step is infallible and returns `Self` for ergonomic chaining.
    ///
    /// Payload: `SIGN_PREFIX_EXECUTOR || signing_payload()`
    pub fn sign_executor(mut self, key: &ed25519_dalek::SigningKey) -> Self {
        let payload = self.executor_sign_payload();
        let sig = key.sign(&payload);
        self.executor_signature = Some(SignatureBytes::from(sig));
        self
    }

    /// Sign as submitter. Sets `submitter_ack`.
    ///
    /// Payload: `SIGN_PREFIX_SUBMITTER || signing_payload() || executor_signature`
    ///
    /// Returns `Err` if `executor_signature` is not present — the chain
    /// must be built in order.
    pub fn sign_submitter_ack(
        mut self,
        key: &ed25519_dalek::SigningKey,
    ) -> Result<Self, ComputeError> {
        if self.executor_signature.is_none() {
            return Err(ComputeError::InvalidInput(
                "cannot sign submitter ack: executor_signature missing".into(),
            ));
        }
        let payload = self.submitter_sign_payload();
        let sig = key.sign(&payload);
        self.submitter_ack = Some(SignatureBytes::from(sig));
        Ok(self)
    }

    /// Sign as attester. Sets `attester` and `attester_signature`.
    ///
    /// Payload: `SIGN_PREFIX_ATTESTER || signing_payload() || executor_signature || submitter_ack`
    ///
    /// Returns `Err` if either prior signature is missing.
    pub fn sign_attester(
        mut self,
        attester_did: String,
        key: &ed25519_dalek::SigningKey,
    ) -> Result<Self, ComputeError> {
        if self.executor_signature.is_none() {
            return Err(ComputeError::InvalidInput(
                "cannot sign attester: executor_signature missing".into(),
            ));
        }
        if self.submitter_ack.is_none() {
            return Err(ComputeError::InvalidInput(
                "cannot sign attester: submitter_ack missing".into(),
            ));
        }
        self.attester = Some(attester_did);
        let payload = self.attester_sign_payload();
        let sig = key.sign(&payload);
        self.attester_signature = Some(SignatureBytes::from(sig));
        Ok(self)
    }

    // -- Verification -------------------------------------------------------

    /// Verify the executor's signature against the given DID.
    ///
    /// Enforces that `executor_did` matches the receipt's `executor` field,
    /// preventing verification against a DID that doesn't match the claimed identity.
    /// All error messages include the hex-encoded receipt hash for correlation
    /// with settlement disputes.
    pub fn verify_executor(&self, executor_did: &icn_identity::Did) -> Result<(), ComputeError> {
        let hash_hex = self.receipt_hash_hex();
        if executor_did.to_string() != self.executor {
            return Err(ComputeError::InvalidSignature(format!(
                "DID mismatch: receipt {hash_hex} claims executor={}, but verifying against {executor_did}",
                self.executor
            )));
        }
        let sig = self.executor_signature.as_ref().ok_or_else(|| {
            ComputeError::InvalidSignature(format!("executor signature missing (receipt {hash_hex})"))
        })?;
        self.verify_sig(executor_did, sig, &self.executor_sign_payload(), &hash_hex)
    }

    /// Verify the submitter's acknowledgement signature against the given DID.
    ///
    /// Enforces that:
    /// - `submitter_did` matches the receipt's `submitter` field
    /// - `executor_signature` is present (chain prerequisite)
    ///
    /// All error messages include the hex-encoded receipt hash for correlation
    /// with settlement disputes.
    pub fn verify_submitter_ack(
        &self,
        submitter_did: &icn_identity::Did,
    ) -> Result<(), ComputeError> {
        let hash_hex = self.receipt_hash_hex();
        if submitter_did.to_string() != self.submitter {
            return Err(ComputeError::InvalidSignature(format!(
                "DID mismatch: receipt {hash_hex} claims submitter={}, but verifying against {submitter_did}",
                self.submitter
            )));
        }
        if self.executor_signature.is_none() {
            return Err(ComputeError::InvalidSignature(format!(
                "cannot verify submitter ack: executor signature missing (receipt {hash_hex})"
            )));
        }
        let sig = self.submitter_ack.as_ref().ok_or_else(|| {
            ComputeError::InvalidSignature(format!("submitter ack missing (receipt {hash_hex})"))
        })?;
        self.verify_sig(
            submitter_did,
            sig,
            &self.submitter_sign_payload(),
            &hash_hex,
        )
    }

    /// Verify the attester's signature against the given DID.
    ///
    /// Enforces that:
    /// - `self.attester` is `Some` and matches `attester_did`
    /// - Both prior signatures are present (chain prerequisite)
    ///
    /// All error messages include the hex-encoded receipt hash for correlation
    /// with settlement disputes.
    pub fn verify_attester(&self, attester_did: &icn_identity::Did) -> Result<(), ComputeError> {
        let hash_hex = self.receipt_hash_hex();
        match &self.attester {
            None => {
                return Err(ComputeError::InvalidSignature(format!(
                    "cannot verify attester: attester DID not set on receipt {hash_hex}"
                )));
            }
            Some(claimed) if claimed != &attester_did.to_string() => {
                return Err(ComputeError::InvalidSignature(format!(
                    "DID mismatch: receipt {hash_hex} claims attester={claimed}, but verifying against {attester_did}"
                )));
            }
            _ => {}
        }
        if self.executor_signature.is_none() || self.submitter_ack.is_none() {
            return Err(ComputeError::InvalidSignature(format!(
                "cannot verify attester: prior signatures missing (receipt {hash_hex})"
            )));
        }
        let sig = self.attester_signature.as_ref().ok_or_else(|| {
            ComputeError::InvalidSignature(format!("attester signature missing (receipt {hash_hex})"))
        })?;
        self.verify_sig(attester_did, sig, &self.attester_sign_payload(), &hash_hex)
    }

    // -- Internal helpers ---------------------------------------------------

    /// Hex-encoded receipt hash for use in error messages and log correlation.
    ///
    /// This hash uniquely identifies a receipt and appears in all verification
    /// error messages, enabling operators to correlate failures with specific
    /// receipts during settlement dispute investigation. The same hash is used
    /// as the key in `SettlementMessage::Dispute`, so grep-ing logs for a
    /// dispute's `receipt_hash` will surface the corresponding verification error.
    fn receipt_hash_hex(&self) -> String {
        hex::encode(self.receipt_hash())
    }

    fn executor_sign_payload(&self) -> Vec<u8> {
        let base = self.signing_payload();
        let mut buf = Vec::with_capacity(SIGN_PREFIX_EXECUTOR.len() + base.len());
        buf.extend_from_slice(SIGN_PREFIX_EXECUTOR);
        buf.extend_from_slice(&base);
        buf
    }

    fn submitter_sign_payload(&self) -> Vec<u8> {
        let base = self.signing_payload();
        let exec_sig: &[u8] = self
            .executor_signature
            .as_ref()
            .map(|s| s.as_bytes().as_slice())
            .unwrap_or(&[]);
        let mut buf = Vec::with_capacity(SIGN_PREFIX_SUBMITTER.len() + base.len() + exec_sig.len());
        buf.extend_from_slice(SIGN_PREFIX_SUBMITTER);
        buf.extend_from_slice(&base);
        buf.extend_from_slice(exec_sig);
        buf
    }

    fn attester_sign_payload(&self) -> Vec<u8> {
        let base = self.signing_payload();
        let exec_sig: &[u8] = self
            .executor_signature
            .as_ref()
            .map(|s| s.as_bytes().as_slice())
            .unwrap_or(&[]);
        let sub_sig: &[u8] = self
            .submitter_ack
            .as_ref()
            .map(|s| s.as_bytes().as_slice())
            .unwrap_or(&[]);
        let mut buf = Vec::with_capacity(
            SIGN_PREFIX_ATTESTER.len() + base.len() + exec_sig.len() + sub_sig.len(),
        );
        buf.extend_from_slice(SIGN_PREFIX_ATTESTER);
        buf.extend_from_slice(&base);
        buf.extend_from_slice(exec_sig);
        buf.extend_from_slice(sub_sig);
        buf
    }

    fn verify_sig(
        &self,
        did: &icn_identity::Did,
        sig_bytes: &SignatureBytes,
        payload: &[u8],
        receipt_hash_hex: &str,
    ) -> Result<(), ComputeError> {
        use ed25519_dalek::Verifier;

        let verifying_key = did.to_verifying_key().map_err(|e| {
            ComputeError::InvalidSignature(format!(
                "cannot extract public key from DID: {e} (receipt {receipt_hash_hex})"
            ))
        })?;

        let signature = ed25519_dalek::Signature::from_bytes(sig_bytes.as_bytes());

        verifying_key.verify(payload, &signature).map_err(|e| {
            ComputeError::InvalidSignature(format!(
                "signature verification failed: {e} (receipt {receipt_hash_hex})"
            ))
        })?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SettlementMessage — gossip envelope for receipt distribution
// ---------------------------------------------------------------------------

/// Messages sent over gossip for settlement coordination.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SettlementMessage {
    /// A fully-attested execution receipt ready for settlement.
    Receipt(Box<ExecutionReceipt>),
    /// A dispute raised against a previously submitted receipt.
    Dispute {
        receipt_hash: ReceiptHash,
        disputer: String,
        reason: String,
        timestamp: u64,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal valid receipt with no signatures.
    fn sample_receipt() -> ExecutionReceipt {
        ExecutionReceipt {
            receipt_id: [0x01; 32],
            task_hash: [0xAB; 32],
            executor: "did:icn:z6MkExecutor".to_string(),
            submitter: "did:icn:z6MkSubmitter".to_string(),
            scope: ScopeLevel::Cell,
            cpu_millis: 1500,
            memory_mb_millis: 4096,
            storage_bytes: 2048,
            egress_bytes: 512,
            fuel_used: 100,
            cost: 50,
            executor_signature: None,
            submitter_ack: None,
            attester: None,
            attester_signature: None,
            created_at: 1_700_000_000,
        }
    }

    // -- Step 1: Determinism and structural tests ---

    #[test]
    fn test_signing_payload_deterministic() {
        let r1 = sample_receipt();
        let r2 = sample_receipt();
        assert_eq!(r1.signing_payload(), r2.signing_payload());
    }

    #[test]
    fn test_signing_payload_changes_on_field_mutation() {
        let base = sample_receipt();
        let base_payload = base.signing_payload();

        let mut r = sample_receipt();
        r.receipt_id[0] = 0xFF;
        assert_ne!(r.signing_payload(), base_payload, "receipt_id mutation");

        let mut r = sample_receipt();
        r.task_hash[0] = 0x00;
        assert_ne!(r.signing_payload(), base_payload, "task_hash mutation");

        let mut r = sample_receipt();
        r.executor = "did:icn:z6MkOtherExecutor".to_string();
        assert_ne!(r.signing_payload(), base_payload, "executor mutation");

        let mut r = sample_receipt();
        r.submitter = "did:icn:z6MkOtherSubmitter".to_string();
        assert_ne!(r.signing_payload(), base_payload, "submitter mutation");

        let mut r = sample_receipt();
        r.scope = ScopeLevel::Federation;
        assert_ne!(r.signing_payload(), base_payload, "scope mutation");

        let mut r = sample_receipt();
        r.cpu_millis = 9999;
        assert_ne!(r.signing_payload(), base_payload, "cpu_millis mutation");

        let mut r = sample_receipt();
        r.memory_mb_millis = 9999;
        assert_ne!(
            r.signing_payload(),
            base_payload,
            "memory_mb_millis mutation"
        );

        let mut r = sample_receipt();
        r.storage_bytes = 9999;
        assert_ne!(r.signing_payload(), base_payload, "storage_bytes mutation");

        let mut r = sample_receipt();
        r.egress_bytes = 9999;
        assert_ne!(r.signing_payload(), base_payload, "egress_bytes mutation");

        let mut r = sample_receipt();
        r.fuel_used = 9999;
        assert_ne!(r.signing_payload(), base_payload, "fuel_used mutation");

        let mut r = sample_receipt();
        r.cost = 9999;
        assert_ne!(r.signing_payload(), base_payload, "cost mutation");

        let mut r = sample_receipt();
        r.created_at = 9999;
        assert_ne!(r.signing_payload(), base_payload, "created_at mutation");
    }

    #[test]
    fn test_signing_payload_length_prefix_prevents_collision() {
        let mut r1 = sample_receipt();
        r1.executor = "did:icn:ab".to_string();
        r1.submitter = "did:icn:cd".to_string();

        let mut r2 = sample_receipt();
        r2.executor = "did:icn:abc".to_string();
        r2.submitter = "did:icn:d".to_string();

        assert_ne!(
            r1.signing_payload(),
            r2.signing_payload(),
            "length-prefix must disambiguate string boundaries"
        );
    }

    #[test]
    fn test_receipt_hash_deterministic() {
        let r1 = sample_receipt();
        let r2 = sample_receipt();
        assert_eq!(r1.receipt_hash(), r2.receipt_hash());
    }

    #[test]
    fn test_receipt_hash_changes_on_mutation() {
        let base = sample_receipt();
        let mut r = sample_receipt();
        r.cost = 9999;
        assert_ne!(r.receipt_hash(), base.receipt_hash());
    }

    #[test]
    fn test_is_fully_attested() {
        let mut r = sample_receipt();
        assert!(!r.is_fully_attested());

        r.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        assert!(!r.is_fully_attested());

        r.submitter_ack = Some(SignatureBytes::from([0x22; 64]));
        assert!(!r.is_fully_attested(), "missing attester DID + sig");

        r.attester = Some("did:icn:z6MkAttester".to_string());
        assert!(!r.is_fully_attested(), "missing attester signature");

        r.attester_signature = Some(SignatureBytes::from([0x33; 64]));
        assert!(r.is_fully_attested());
    }

    #[test]
    fn test_validate_rejects_bad_executor_did() {
        let mut r = sample_receipt();
        r.executor = "not-a-did".to_string();
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("executor DID"));
    }

    #[test]
    fn test_validate_rejects_bad_submitter_did() {
        let mut r = sample_receipt();
        r.submitter = "bad-format".to_string();
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("submitter DID"));
    }

    #[test]
    fn test_validate_rejects_bad_attester_did() {
        let mut r = sample_receipt();
        r.attester = Some("garbage".to_string());
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("attester DID"));
    }

    #[test]
    fn test_validate_accepts_valid_receipt() {
        let r = sample_receipt();
        assert!(r.validate().is_ok());
    }

    #[test]
    fn test_signature_bytes_serde_roundtrip() {
        let sig = SignatureBytes::from([0xAA; 64]);
        let json = serde_json::to_string(&sig).unwrap();
        let deserialized: SignatureBytes = serde_json::from_str(&json).unwrap();
        assert_eq!(sig, deserialized);
    }

    #[test]
    fn test_receipt_serde_roundtrip() {
        let mut r = sample_receipt();
        r.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        let json = serde_json::to_string(&r).unwrap();
        let deserialized: ExecutionReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(r, deserialized);
    }

    // -- Step 2: Signing and verification tests ---

    /// Construct an `ed25519_dalek::SigningKey` from a `KeyPair`.
    fn to_signing_key(kp: &icn_identity::KeyPair) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&kp.to_signing_key_bytes())
    }

    /// Build a receipt with executor DID matching a real keypair.
    fn receipt_with_real_keys() -> (ExecutionReceipt, icn_identity::KeyPair) {
        let executor_kp = icn_identity::KeyPair::generate().unwrap();
        let r = ExecutionReceipt {
            receipt_id: [0x02; 32],
            task_hash: [0xAB; 32],
            executor: executor_kp.did().to_string(),
            submitter: "did:icn:z6MkSubmitter".to_string(),
            scope: ScopeLevel::Cell,
            cpu_millis: 1500,
            memory_mb_millis: 4096,
            storage_bytes: 2048,
            egress_bytes: 512,
            fuel_used: 100,
            cost: 50,
            executor_signature: None,
            submitter_ack: None,
            attester: None,
            attester_signature: None,
            created_at: 1_700_000_000,
        };
        (r, executor_kp)
    }

    #[test]
    fn test_sign_verify_executor() {
        let (r, kp) = receipt_with_real_keys();
        let r = r.sign_executor(&to_signing_key(&kp));
        assert!(r.executor_signature.is_some());
        assert!(r.verify_executor(kp.did()).is_ok());
    }

    #[test]
    fn test_sign_verify_submitter_ack() {
        let (r, executor_kp) = receipt_with_real_keys();
        let submitter_kp = icn_identity::KeyPair::generate().unwrap();
        let mut r = r.sign_executor(&to_signing_key(&executor_kp));
        r.submitter = submitter_kp.did().to_string();
        let r = r
            .sign_submitter_ack(&to_signing_key(&submitter_kp))
            .unwrap();
        assert!(r.submitter_ack.is_some());
        assert!(r.verify_submitter_ack(submitter_kp.did()).is_ok());
    }

    #[test]
    fn test_sign_verify_attester() {
        let (r, executor_kp) = receipt_with_real_keys();
        let submitter_kp = icn_identity::KeyPair::generate().unwrap();
        let attester_kp = icn_identity::KeyPair::generate().unwrap();

        let mut r = r.sign_executor(&to_signing_key(&executor_kp));
        r.submitter = submitter_kp.did().to_string();
        let r = r
            .sign_submitter_ack(&to_signing_key(&submitter_kp))
            .unwrap()
            .sign_attester(attester_kp.did().to_string(), &to_signing_key(&attester_kp))
            .unwrap();

        assert!(r.is_fully_attested());
        assert!(r.verify_attester(attester_kp.did()).is_ok());
    }

    #[test]
    fn test_verify_executor_wrong_did_rejected() {
        let (r, executor_kp) = receipt_with_real_keys();
        let r = r.sign_executor(&to_signing_key(&executor_kp));

        let wrong_kp = icn_identity::KeyPair::generate().unwrap();
        let err = r.verify_executor(wrong_kp.did()).unwrap_err();
        assert!(
            err.to_string().contains("DID mismatch"),
            "should reject DID that doesn't match receipt's executor field, got: {err}"
        );
    }

    #[test]
    fn test_verify_executor_tampered() {
        let (r, executor_kp) = receipt_with_real_keys();
        let mut r = r.sign_executor(&to_signing_key(&executor_kp));

        // Tamper with cost after signing
        r.cost = 9999;
        let err = r.verify_executor(executor_kp.did()).unwrap_err();
        assert!(err.to_string().contains("verification failed"));
    }

    #[test]
    fn test_submitter_payload_includes_executor_sig() {
        // Identical base fields, only vary executor signature bytes.
        // This directly proves the submitter payload binds to the executor sig.
        let mut r1 = sample_receipt();
        r1.executor_signature = Some(SignatureBytes::from([0x11; 64]));

        let mut r2 = sample_receipt();
        r2.executor_signature = Some(SignatureBytes::from([0x22; 64]));

        assert_ne!(
            r1.submitter_sign_payload(),
            r2.submitter_sign_payload(),
            "submitter payload must bind to executor signature bytes"
        );
    }

    #[test]
    fn test_attester_payload_includes_both_prior_sigs() {
        let mut r1 = sample_receipt();
        r1.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        r1.submitter_ack = Some(SignatureBytes::from([0xAA; 64]));

        let mut r2 = sample_receipt();
        r2.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        r2.submitter_ack = Some(SignatureBytes::from([0xBB; 64]));

        assert_ne!(
            r1.attester_sign_payload(),
            r2.attester_sign_payload(),
            "attester payload must bind to submitter ack bytes"
        );
    }

    #[test]
    fn test_builder_chain() {
        let executor_kp = icn_identity::KeyPair::generate().unwrap();
        let submitter_kp = icn_identity::KeyPair::generate().unwrap();
        let attester_kp = icn_identity::KeyPair::generate().unwrap();

        let r = ExecutionReceipt {
            receipt_id: [0x03; 32],
            task_hash: [0xCC; 32],
            executor: executor_kp.did().to_string(),
            submitter: submitter_kp.did().to_string(),
            scope: ScopeLevel::Org,
            cpu_millis: 500,
            memory_mb_millis: 1024,
            storage_bytes: 0,
            egress_bytes: 0,
            fuel_used: 10,
            cost: 5,
            executor_signature: None,
            submitter_ack: None,
            attester: None,
            attester_signature: None,
            created_at: 1_700_000_001,
        };

        let r = r
            .sign_executor(&to_signing_key(&executor_kp))
            .sign_submitter_ack(&to_signing_key(&submitter_kp))
            .unwrap()
            .sign_attester(attester_kp.did().to_string(), &to_signing_key(&attester_kp))
            .unwrap();

        assert!(r.is_fully_attested());
        assert!(r.verify_executor(executor_kp.did()).is_ok());
        assert!(r.verify_submitter_ack(submitter_kp.did()).is_ok());
        assert!(r.verify_attester(attester_kp.did()).is_ok());
    }

    #[test]
    fn test_settlement_message_serde() {
        let r = sample_receipt();
        let msg = SettlementMessage::Receipt(Box::new(r));
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: SettlementMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);

        let dispute = SettlementMessage::Dispute {
            receipt_hash: [0xFF; 32],
            disputer: "did:icn:z6MkDisputer".to_string(),
            reason: "incorrect metering".to_string(),
            timestamp: 1_700_000_100,
        };
        let json = serde_json::to_string(&dispute).unwrap();
        let deserialized: SettlementMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(dispute, deserialized);
    }

    // -- Signature prerequisite enforcement tests ---

    #[test]
    fn test_verify_missing_executor_sig() {
        let (r, kp) = receipt_with_real_keys();
        let err = r.verify_executor(kp.did()).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_verify_missing_submitter_ack() {
        let (r, executor_kp) = receipt_with_real_keys();
        let submitter_kp = icn_identity::KeyPair::generate().unwrap();
        // Set submitter DID to match the keypair we'll verify against
        let mut r = r.sign_executor(&to_signing_key(&executor_kp));
        r.submitter = submitter_kp.did().to_string();
        let err = r.verify_submitter_ack(submitter_kp.did()).unwrap_err();
        assert!(
            err.to_string().contains("submitter ack missing"),
            "got: {err}"
        );
    }

    #[test]
    fn test_verify_submitter_rejects_missing_executor_sig() {
        // A receipt with submitter_ack but no executor_signature must fail
        // verification — the chain is broken.
        let submitter_kp = icn_identity::KeyPair::generate().unwrap();
        let mut r = sample_receipt();
        r.submitter = submitter_kp.did().to_string();
        r.submitter_ack = Some(SignatureBytes::from([0xAA; 64])); // fake, but present
        let err = r.verify_submitter_ack(submitter_kp.did()).unwrap_err();
        assert!(
            err.to_string().contains("executor signature missing"),
            "should enforce chain prerequisite, got: {err}"
        );
    }

    #[test]
    fn test_verify_attester_rejects_missing_prior_sigs() {
        let attester_kp = icn_identity::KeyPair::generate().unwrap();
        let mut r = sample_receipt();
        r.attester = Some(attester_kp.did().to_string());
        r.attester_signature = Some(SignatureBytes::from([0xBB; 64])); // fake, but present
        let err = r.verify_attester(attester_kp.did()).unwrap_err();
        assert!(
            err.to_string().contains("prior signatures missing"),
            "should enforce chain prerequisite, got: {err}"
        );
    }

    #[test]
    fn test_sign_submitter_without_executor_returns_error() {
        let r = sample_receipt();
        let kp = icn_identity::KeyPair::generate().unwrap();
        let err = r.sign_submitter_ack(&to_signing_key(&kp)).unwrap_err();
        assert!(err.to_string().contains("executor_signature missing"));
    }

    #[test]
    fn test_sign_attester_without_submitter_returns_error() {
        let (r, executor_kp) = receipt_with_real_keys();
        let r = r.sign_executor(&to_signing_key(&executor_kp));
        let kp = icn_identity::KeyPair::generate().unwrap();
        let err = r
            .sign_attester(kp.did().to_string(), &to_signing_key(&kp))
            .unwrap_err();
        assert!(err.to_string().contains("submitter_ack missing"));
    }

    #[test]
    fn test_sign_attester_without_executor_returns_error() {
        let r = sample_receipt();
        let kp = icn_identity::KeyPair::generate().unwrap();
        let err = r
            .sign_attester(kp.did().to_string(), &to_signing_key(&kp))
            .unwrap_err();
        assert!(err.to_string().contains("executor_signature missing"));
    }

    // -- Replay protection tests ---

    #[test]
    fn test_different_receipt_id_produces_different_payload() {
        let mut r1 = sample_receipt();
        r1.receipt_id = [0xAA; 32];
        let mut r2 = sample_receipt();
        r2.receipt_id = [0xBB; 32];

        assert_ne!(
            r1.signing_payload(),
            r2.signing_payload(),
            "different receipt_id must produce different signing payloads"
        );
        assert_ne!(
            r1.receipt_hash(),
            r2.receipt_hash(),
            "different receipt_id must produce different receipt hashes"
        );
    }

    // -- Timestamp validation tests ---

    #[test]
    fn test_validate_rejects_ancient_timestamp() {
        let mut r = sample_receipt();
        r.created_at = 1_000_000; // Way before MIN_VALID_TIMESTAMP
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("before minimum"), "got: {err}");
    }

    #[test]
    fn test_validate_rejects_future_timestamp() {
        let mut r = sample_receipt();
        r.created_at = icn_time::current_timestamp_secs() + MAX_CLOCK_SKEW_SECS + 1000;
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("in the future"), "got: {err}");
    }

    #[test]
    fn test_validate_accepts_recent_timestamp() {
        let mut r = sample_receipt();
        // Use a timestamp that's clearly in the past but after MIN_VALID_TIMESTAMP
        r.created_at = icn_time::current_timestamp_secs() - 60;
        assert!(r.validate().is_ok());
    }

    // -- Metering bounds tests ---

    #[test]
    fn test_validate_rejects_excessive_cpu_millis() {
        let mut r = sample_receipt();
        r.cpu_millis = MAX_CPU_MILLIS + 1;
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("cpu_millis"), "got: {err}");
    }

    #[test]
    fn test_validate_rejects_excessive_memory() {
        let mut r = sample_receipt();
        r.memory_mb_millis = MAX_MEMORY_MB_MILLIS + 1;
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("memory_mb_millis"), "got: {err}");
    }

    #[test]
    fn test_validate_rejects_excessive_storage() {
        let mut r = sample_receipt();
        r.storage_bytes = MAX_STORAGE_BYTES + 1;
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("storage_bytes"), "got: {err}");
    }

    #[test]
    fn test_validate_rejects_excessive_egress() {
        let mut r = sample_receipt();
        r.egress_bytes = MAX_EGRESS_BYTES + 1;
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("egress_bytes"), "got: {err}");
    }

    // -- DID identity binding tests ---

    #[test]
    fn test_verify_submitter_wrong_did_rejected() {
        let (r, executor_kp) = receipt_with_real_keys();
        let submitter_kp = icn_identity::KeyPair::generate().unwrap();
        let mut r = r.sign_executor(&to_signing_key(&executor_kp));
        r.submitter = submitter_kp.did().to_string();
        let r = r
            .sign_submitter_ack(&to_signing_key(&submitter_kp))
            .unwrap();

        let wrong_kp = icn_identity::KeyPair::generate().unwrap();
        let err = r.verify_submitter_ack(wrong_kp.did()).unwrap_err();
        assert!(
            err.to_string().contains("DID mismatch"),
            "should reject DID that doesn't match receipt's submitter field, got: {err}"
        );
    }

    #[test]
    fn test_verify_attester_wrong_did_rejected() {
        let (r, executor_kp) = receipt_with_real_keys();
        let submitter_kp = icn_identity::KeyPair::generate().unwrap();
        let attester_kp = icn_identity::KeyPair::generate().unwrap();

        let mut r = r.sign_executor(&to_signing_key(&executor_kp));
        r.submitter = submitter_kp.did().to_string();
        let r = r
            .sign_submitter_ack(&to_signing_key(&submitter_kp))
            .unwrap()
            .sign_attester(attester_kp.did().to_string(), &to_signing_key(&attester_kp))
            .unwrap();

        let wrong_kp = icn_identity::KeyPair::generate().unwrap();
        let err = r.verify_attester(wrong_kp.did()).unwrap_err();
        assert!(
            err.to_string().contains("DID mismatch"),
            "should reject DID that doesn't match receipt's attester field, got: {err}"
        );
    }

    #[test]
    fn test_verify_attester_rejects_missing_attester_did() {
        // Receipt has attester_signature but no attester DID set
        let mut r = sample_receipt();
        r.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        r.submitter_ack = Some(SignatureBytes::from([0x22; 64]));
        r.attester_signature = Some(SignatureBytes::from([0x33; 64]));
        // attester is None
        let kp = icn_identity::KeyPair::generate().unwrap();
        let err = r.verify_attester(kp.did()).unwrap_err();
        assert!(
            err.to_string().contains("attester DID not set"),
            "should reject when receipt has no attester DID, got: {err}"
        );
    }

    // -- Wire format tests ---

    #[test]
    fn test_verification_errors_include_receipt_hash() {
        let mut r = sample_receipt();
        let kp = icn_identity::KeyPair::generate().unwrap();
        let hash_hex = hex::encode(r.receipt_hash());

        // Missing executor signature
        let err = r.verify_executor(kp.did()).unwrap_err();
        assert!(
            err.to_string().contains(&hash_hex),
            "executor error should contain receipt hash, got: {err}"
        );

        // Set fake executor sig so we can test submitter path
        r.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        let err = r.verify_submitter_ack(kp.did()).unwrap_err();
        assert!(
            err.to_string().contains(&hash_hex),
            "submitter error should contain receipt hash, got: {err}"
        );

        // Missing attester DID
        r.submitter_ack = Some(SignatureBytes::from([0x22; 64]));
        let err = r.verify_attester(kp.did()).unwrap_err();
        assert!(
            err.to_string().contains(&hash_hex),
            "attester error should contain receipt hash, got: {err}"
        );
    }

    #[test]
    fn test_receipt_hash_stable_across_signature_changes() {
        // Receipt hash is computed from non-signature fields only.
        // Adding/changing signatures must not alter the hash.
        let r = sample_receipt();
        let hash_before = r.receipt_hash();

        let mut r_signed = r.clone();
        r_signed.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        r_signed.submitter_ack = Some(SignatureBytes::from([0x22; 64]));
        r_signed.attester = Some("did:icn:z6MkAttester".to_string());
        r_signed.attester_signature = Some(SignatureBytes::from([0x33; 64]));

        assert_eq!(
            hash_before,
            r_signed.receipt_hash(),
            "receipt hash must be stable regardless of signature state"
        );
    }

    #[test]
    fn test_settlement_message_icn_encoding_roundtrip() {
        let r = sample_receipt();
        let msg = SettlementMessage::Receipt(Box::new(r));
        let encoded = icn_encoding::encode(&msg).unwrap();
        let decoded: SettlementMessage = icn_encoding::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_receipt_icn_encoding_roundtrip() {
        let mut r = sample_receipt();
        r.executor_signature = Some(SignatureBytes::from([0x11; 64]));
        let encoded = icn_encoding::encode(&r).unwrap();
        let decoded: ExecutionReceipt = icn_encoding::decode(&encoded).unwrap();
        assert_eq!(r, decoded);
    }

    #[test]
    fn test_signature_bytes_binary_serde_compact() {
        // Verify that binary serialization is more compact than hex JSON
        let sig = SignatureBytes::from([0xAA; 64]);
        let json_bytes = serde_json::to_vec(&sig).unwrap();
        let postcard_bytes = icn_encoding::encode(&sig).unwrap();
        assert!(
            postcard_bytes.len() < json_bytes.len(),
            "binary encoding ({} bytes) should be smaller than JSON ({} bytes)",
            postcard_bytes.len(),
            json_bytes.len()
        );
    }
}
