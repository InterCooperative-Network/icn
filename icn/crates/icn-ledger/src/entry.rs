//! Journal entry creation and validation

use crate::types::{AccountDelta, ContentHash, JournalEntry, ProvenanceRef, Signature};
use anyhow::{bail, Result};
use icn_identity::{Did, KeyPair};
use std::collections::HashMap;

/// Sign a journal entry's content hash using the author's Ed25519 keypair.
///
/// Signs `entry.id` (the 32-byte SHA-256 content hash) and stores the 64-byte
/// Ed25519 signature in `entry.signature`.  The signed bytes are identical to
/// the bytes used by witness signatures, so verification is consistent.
///
/// # Prerequisites
///
/// - The entry must have a computed content hash (`entry.id` must be `Some`).
///   Call `JournalEntryBuilder::build()` before calling this function.
/// - `keypair.did()` should equal `entry.author` for the signature to be
///   verifiable via `verify_entry_signature()`.
///
/// # Key access gap (issue #1435)
///
/// The gateway's direct-member settlement path does not currently have access to
/// the author's private key — JWT auth proves identity but the key never reaches
/// the server.  This function is the substrate enabling step: once a signing
/// capability is threaded into `LedgerManager::create_settlement`, this is the
/// call that produces the content signature.
pub fn sign_entry(entry: &mut JournalEntry, keypair: &KeyPair) -> Result<()> {
    let hash = entry.id.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Entry must have a computed hash before signing; call build() first")
    })?;

    let sig = keypair.sign(hash.as_bytes());
    entry.signature = Some(Signature(sig.to_bytes().to_vec()));
    Ok(())
}

/// Verify the Ed25519 content signature on a journal entry.
///
/// Extracts the verifying key from `entry.author` (the DID's embedded Ed25519
/// public key) and verifies the stored signature against `entry.id` (the
/// content hash).
///
/// This is publicly verifiable: any party with the author's DID can verify
/// without any shared secret.
///
/// Returns:
/// - `Ok(true)` — signature is present and verifies against author's public key
/// - `Ok(false)` — no signature stored (`entry.signature` is `None`)
/// - `Err(...)` — DID is malformed, signature bytes are wrong length,
///   or content hash is missing
pub fn verify_entry_signature(entry: &JournalEntry) -> Result<bool> {
    let Some(ref stored_sig) = entry.signature else {
        return Ok(false);
    };

    let hash = entry
        .id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Entry has no content hash; cannot verify signature"))?;

    let verifying_key = entry.author.to_verifying_key()?;

    let sig_bytes: [u8; 64] = stored_sig.0.as_slice().try_into().map_err(|_| {
        anyhow::anyhow!(
            "Invalid signature length: {} bytes (expected 64)",
            stored_sig.0.len()
        )
    })?;

    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    use ed25519_dalek::Verifier;
    Ok(verifying_key.verify(hash.as_bytes(), &signature).is_ok())
}

/// Builder for creating valid journal entries
pub struct JournalEntryBuilder {
    author: Did,
    contract_ref: Option<ContentHash>,
    accounts: Vec<AccountDelta>,
    parents: Vec<ContentHash>,
    nonce: Option<[u8; 32]>,
    provenance: Option<ProvenanceRef>,
}

impl JournalEntryBuilder {
    /// Create a new builder with the given author
    pub fn new(author: Did) -> Self {
        JournalEntryBuilder {
            author,
            contract_ref: None,
            accounts: Vec::new(),
            parents: Vec::new(),
            nonce: None,
            provenance: None,
        }
    }

    /// Set the contract reference
    pub fn contract_ref(mut self, contract_ref: ContentHash) -> Self {
        self.contract_ref = Some(contract_ref);
        self
    }

    /// Add a debit to an account
    pub fn debit(mut self, account_id: Did, currency: String, amount: i64) -> Self {
        self.accounts
            .push(AccountDelta::debit(account_id, currency, amount));
        self
    }

    /// Add a credit to an account
    pub fn credit(mut self, account_id: Did, currency: String, amount: i64) -> Self {
        self.accounts
            .push(AccountDelta::credit(account_id, currency, amount));
        self
    }

    /// Add an account delta
    pub fn add_delta(mut self, delta: AccountDelta) -> Self {
        self.accounts.push(delta);
        self
    }

    /// Add a parent entry (for Merkle-DAG)
    pub fn add_parent(mut self, parent: ContentHash) -> Self {
        self.parents.push(parent);
        self
    }

    /// Set a nonce for replay protection.
    ///
    /// When set, the nonce is included in the content hash, ensuring
    /// entries with identical content produce distinct hashes. Use this
    /// for commons credit entries where the nonce is the `receipt_id`.
    pub fn nonce(mut self, nonce: [u8; 32]) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Set governance decision provenance.
    ///
    /// Use when this entry was authorized by a governance proposal/vote.
    ///
    /// # Arguments
    /// * `receipt_id` - Node-local decision receipt ID
    /// * `decision_hash` - Canonical cross-node hash anchor
    pub fn with_governance_provenance(
        mut self,
        receipt_id: impl Into<String>,
        decision_hash: impl Into<String>,
    ) -> Self {
        self.provenance = Some(ProvenanceRef::Governance {
            receipt_id: receipt_id.into(),
            decision_hash: decision_hash.into(),
        });
        self
    }

    /// Set member-direct provenance.
    ///
    /// Use when a cooperative member directly authorizes a transfer without a
    /// formal governance proposal — e.g., a member-to-member credit grant.
    ///
    /// # Arguments
    /// * `did`        - DID of the authorizing member
    /// * `auth_proof` - Stored authorization evidence for this action.
    ///   On the HTTP direct-member path, pass `"jwt-sig:<segment>"` where
    ///   `<segment>` is the HMAC-SHA256 third segment of the JWT from the
    ///   `Authorization` header.  For system-executed paths (escrow, recurring
    ///   settlements), pass `"system-executed"`.
    ///   This is NOT an Ed25519 signature over entry content (see issue #1435).
    pub fn with_direct_member_provenance(
        mut self,
        did: Did,
        auth_proof: impl Into<String>,
    ) -> Self {
        self.provenance = Some(ProvenanceRef::DirectMember {
            did,
            auth_proof: auth_proof.into(),
        });
        self
    }

    /// Set system-generated provenance.
    ///
    /// Use for internal operations: FX transfers, DID migration, mint/burn.
    pub fn with_system_provenance(mut self, reason: impl Into<String>) -> Self {
        self.provenance = Some(ProvenanceRef::SystemGenerated {
            reason: reason.into(),
        });
        self
    }

    /// Build and validate the journal entry.
    ///
    /// Returns `Err` if:
    /// - Double-entry invariant is violated (Σ debits ≠ Σ credits per currency)
    /// - Any amount is negative
    /// - No provenance was set (use `with_governance_provenance`, `with_direct_member_provenance`, or `with_system_provenance`)
    /// - The system clock is unavailable
    pub fn build(self) -> Result<JournalEntry> {
        // Validate double-entry invariant: Σ debits == Σ credits per currency
        validate_double_entry(&self.accounts)?;

        // Validate that amounts are positive
        validate_positive_amounts(&self.accounts)?;

        // Require provenance — every entry must be traceable
        let provenance = self.provenance.ok_or_else(|| {
            anyhow::anyhow!(
                "JournalEntry requires provenance; call with_governance_provenance(), \
                 with_direct_member_provenance(), or with_system_provenance()"
            )
        })?;

        // Get current timestamp in milliseconds (security-critical: reject if clock is invalid)
        let timestamp = icn_time::try_current_timestamp_millis()
            .map_err(|e| anyhow::anyhow!("Cannot create journal entry: {e}"))?;

        let mut entry = JournalEntry {
            id: None,
            timestamp,
            author: self.author,
            contract_ref: self.contract_ref,
            accounts: self.accounts,
            parents: self.parents,
            signature: None, // Will be set by caller
            nonce: self.nonce,
            provenance,
        };

        // Compute the content hash
        entry.compute_hash()?;

        Ok(entry)
    }
}

/// Validate double-entry bookkeeping invariant
/// For each currency, the sum of debits must equal the sum of credits
fn validate_double_entry(accounts: &[AccountDelta]) -> Result<()> {
    // Group by currency and sum debits/credits
    let mut currency_totals: HashMap<String, (i64, i64)> = HashMap::new();

    for delta in accounts {
        let (total_debits, total_credits) = currency_totals
            .entry(delta.currency.clone())
            .or_insert((0, 0));

        *total_debits += delta.debit.unwrap_or(0);
        *total_credits += delta.credit.unwrap_or(0);
    }

    // Check that debits == credits for each currency
    for (currency, (debits, credits)) in currency_totals.iter() {
        if debits != credits {
            bail!(
                "Double-entry invariant violated for currency '{currency}': debits={debits}, credits={credits}"
            );
        }
    }

    Ok(())
}

/// Validate that all amounts are positive
fn validate_positive_amounts(accounts: &[AccountDelta]) -> Result<()> {
    for delta in accounts {
        if let Some(debit) = delta.debit {
            if debit < 0 {
                bail!(
                    "Negative debit amount not allowed: {} for account {}",
                    debit,
                    delta.account_id
                );
            }
        }

        if let Some(credit) = delta.credit {
            if credit < 0 {
                bail!(
                    "Negative credit amount not allowed: {} for account {}",
                    credit,
                    delta.account_id
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_valid_entry_creation() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .with_system_provenance("test")
            .build();

        assert!(entry.is_ok(), "Valid entry should build successfully");

        let entry = entry.unwrap();
        assert!(entry.id.is_some(), "Entry should have a computed hash");
        assert_eq!(entry.accounts.len(), 2);
    }

    #[test]
    fn test_unbalanced_entry_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 5) // Unbalanced!
            .with_system_provenance("test")
            .build();

        assert!(entry.is_err(), "Unbalanced entry should fail validation");
        assert!(entry
            .unwrap_err()
            .to_string()
            .contains("Double-entry invariant violated"));
    }

    #[test]
    fn test_negative_amount_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), -10) // Negative!
            .credit(bob.clone(), "hours".to_string(), -10) // Negative!
            .with_system_provenance("test")
            .build();

        assert!(entry.is_err(), "Negative amounts should fail validation");
        assert!(entry
            .unwrap_err()
            .to_string()
            .contains("Negative debit amount"));
    }

    #[test]
    fn test_multi_currency_entry() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .debit(alice.clone(), "USD".to_string(), 100)
            .credit(bob.clone(), "USD".to_string(), 100)
            .with_system_provenance("test")
            .build();

        assert!(
            entry.is_ok(),
            "Multi-currency entry should build successfully"
        );

        let entry = entry.unwrap();
        assert_eq!(entry.accounts.len(), 4);
    }

    #[test]
    fn test_multi_currency_unbalanced_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .debit(alice.clone(), "USD".to_string(), 100)
            .credit(bob.clone(), "USD".to_string(), 50) // Unbalanced USD!
            .with_system_provenance("test")
            .build();

        assert!(
            entry.is_err(),
            "Unbalanced multi-currency entry should fail"
        );
        assert!(entry.unwrap_err().to_string().contains("USD"));
    }

    /// Golden-path test: JournalEntry with governance provenance
    ///
    /// Verifies:
    /// - `ProvenanceRef::Governance` is preserved through build
    /// - Provenance round-trips through JSON serialization
    #[test]
    fn test_entry_with_governance_provenance() {
        let keypair = KeyPair::generate().unwrap();
        let treasury = keypair.did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let receipt_id = "gov:proposal:2024-001:receipt:abc123";
        let decision_hash = "sha256:def456789...";

        let entry = JournalEntryBuilder::new(treasury.clone())
            .debit(treasury.clone(), "HOURS".to_string(), 2500)
            .credit(recipient.clone(), "HOURS".to_string(), 2500)
            .with_governance_provenance(receipt_id, decision_hash)
            .build()
            .expect("Entry with governance provenance should build successfully");

        // Verify provenance variant
        match &entry.provenance {
            ProvenanceRef::Governance {
                receipt_id: r,
                decision_hash: h,
            } => {
                assert_eq!(r, receipt_id);
                assert_eq!(h, decision_hash);
            }
            other => panic!("Expected Governance provenance, got {other:?}"),
        }

        assert!(entry.id.is_some(), "Entry should have computed hash");

        // Round-trip through JSON
        let json = serde_json::to_string(&entry).expect("should serialize");
        assert!(json.contains(receipt_id), "JSON should contain receipt_id");
        assert!(
            json.contains(decision_hash),
            "JSON should contain decision_hash"
        );
        let deserialized: JournalEntry = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized.provenance, entry.provenance);
    }

    /// Test that build() fails when no provenance is set.
    #[test]
    fn test_missing_provenance_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let result = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build();

        assert!(result.is_err(), "build() without provenance must fail");
        assert!(
            result.unwrap_err().to_string().contains("provenance"),
            "error message should mention provenance"
        );
    }

    /// Test that SystemGenerated provenance round-trips correctly.
    #[test]
    fn test_system_provenance_roundtrip() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .with_system_provenance("fx-transfer")
            .build()
            .expect("system provenance entry should build");

        match &entry.provenance {
            ProvenanceRef::SystemGenerated { reason } => assert_eq!(reason, "fx-transfer"),
            other => panic!("Expected SystemGenerated, got {other:?}"),
        }

        let json = serde_json::to_string(&entry).unwrap();
        let de: JournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(de.provenance, entry.provenance);
    }

    /// Test that DirectMember provenance round-trips correctly with the new
    /// `auth_proof` field name.
    #[test]
    fn test_direct_member_provenance_roundtrip() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let proof = "jwt-sig:aHR0cHM6Ly9leGFtcGxlLmNvbQ";

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 3)
            .credit(bob.clone(), "hours".to_string(), 3)
            .with_direct_member_provenance(alice.clone(), proof)
            .build()
            .expect("direct member provenance entry should build");

        match &entry.provenance {
            ProvenanceRef::DirectMember { did, auth_proof } => {
                assert_eq!(did, &alice);
                assert_eq!(auth_proof, proof);
            }
            other => panic!("Expected DirectMember, got {other:?}"),
        }

        // New entries serialize with "auth_proof" key.
        let json = serde_json::to_string(&entry).unwrap();
        assert!(
            json.contains("\"auth_proof\""),
            "new entries must serialize with auth_proof key, got: {json}"
        );
        assert!(json.contains(proof), "JSON must contain the proof value");
        let de: JournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(de.provenance, entry.provenance);
    }

    /// Backward-compatibility: on-disk Sled data written before the rename
    /// used `"signature"` as the JSON key.  The serde alias must still
    /// deserialize those entries without error.
    #[test]
    fn test_direct_member_provenance_backward_compat_signature_alias() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();

        // Simulate a pre-rename journal entry where the field was `"signature"`.
        let old_json = format!(
            r#"{{
                "timestamp": 1000000000,
                "author": {did_json},
                "contract_ref": null,
                "accounts": [],
                "parents": [],
                "signature": null,
                "nonce": null,
                "provenance": {{"DirectMember": {{"did": {did_json}, "signature": "jwt-sig:oldsig"}}}}
            }}"#,
            did_json = serde_json::to_string(&alice).unwrap()
        );

        let entry: JournalEntry =
            serde_json::from_str(&old_json).expect("old signature key must deserialize via alias");

        match &entry.provenance {
            ProvenanceRef::DirectMember { auth_proof, .. } => {
                assert_eq!(
                    auth_proof, "jwt-sig:oldsig",
                    "backward-compat alias must preserve the stored value"
                );
            }
            other => panic!("Expected DirectMember, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // sign_entry / verify_entry_signature tests
    // -------------------------------------------------------------------------

    /// Golden path: sign → verify with the same keypair returns true.
    ///
    /// Proves that `sign_entry` + `verify_entry_signature` form a valid
    /// round-trip using only the author's DID (public info) for verification.
    #[test]
    fn test_sign_and_verify_entry() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .with_direct_member_provenance(alice.clone(), "jwt-sig:testsig")
            .build()
            .expect("entry should build");

        // Entry has a hash but no signature yet.
        assert!(entry.id.is_some(), "build() must compute the content hash");
        assert!(
            entry.signature.is_none(),
            "signature not set before sign_entry"
        );

        sign_entry(&mut entry, &keypair).expect("sign_entry must succeed");

        assert!(
            entry.signature.is_some(),
            "signature must be set after sign_entry"
        );

        // Verification uses only the author DID (public key embedded in the DID string).
        let result = verify_entry_signature(&entry).expect("verify must not error");
        assert!(result, "signature from correct keypair must verify");
    }

    /// Verify returns false for an entry with no signature.
    #[test]
    fn test_verify_unsigned_entry_returns_false() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 3)
            .credit(bob.clone(), "hours".to_string(), 3)
            .with_system_provenance("test")
            .build()
            .expect("entry should build");

        let result = verify_entry_signature(&entry).expect("verify must not error");
        assert!(!result, "unsigned entry must return false, not error");
    }

    /// Verify fails when a different keypair signed the entry.
    ///
    /// Proves that the signature is bound to the specific keypair / DID:
    /// signing with a key that doesn't match `entry.author` is detected.
    #[test]
    fn test_verify_wrong_key_fails() {
        // alice is the entry author
        let alice_kp = KeyPair::generate().unwrap();
        let alice = alice_kp.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .with_direct_member_provenance(alice.clone(), "jwt-sig:test")
            .build()
            .expect("entry should build");

        // Sign with a DIFFERENT keypair (not alice's).
        let wrong_kp = KeyPair::generate().unwrap();
        sign_entry(&mut entry, &wrong_kp).expect("sign_entry with wrong key must not panic");

        // Verification checks against entry.author (alice's DID), so must fail.
        let result = verify_entry_signature(&entry).expect("verify must not error");
        assert!(
            !result,
            "signature from wrong keypair must not verify against author's DID"
        );
    }

    /// Verify fails after content mutation (hash changes).
    ///
    /// Proves that `sign_entry` binds to the content hash at signing time.
    /// Any change to content-hashed fields invalidates the signature.
    #[test]
    fn test_verify_fails_after_content_mutation() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .with_direct_member_provenance(alice.clone(), "jwt-sig:test")
            .build()
            .expect("entry should build");

        sign_entry(&mut entry, &keypair).expect("sign must succeed");

        // Mutate the content hash to simulate tampered data.
        // (In a real attack, the attacker would modify accounts and then re-hash,
        // but here we directly corrupt the id to prove signature coverage.)
        entry.id = Some(crate::types::ContentHash::from_bytes([0xde; 32]));

        let result = verify_entry_signature(&entry).expect("verify must not error");
        assert!(
            !result,
            "mutated content hash must break signature verification"
        );
    }
}
