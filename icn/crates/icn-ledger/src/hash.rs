//! Content hashing for Merkle-DAG structure

use crate::types::{ContentHash, JournalEntry};
use anyhow::Result;
use sha2::{Digest, Sha256};

/// Compute content hash for a journal entry
/// Uses canonical JSON serialization to ensure deterministic hashing
pub fn compute_entry_hash(entry: &JournalEntry) -> Result<ContentHash> {
    // Create a hashable version without the id and signature fields
    // to avoid circular dependencies
    let hashable = HashableEntry {
        timestamp: entry.timestamp,
        author: &entry.author,
        contract_ref: entry.contract_ref.as_ref(),
        accounts: &entry.accounts,
        parents: &entry.parents,
    };

    // Serialize to canonical JSON (sorted keys)
    let json = serde_json::to_string(&hashable)?;

    // Hash the canonical JSON
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let hash_bytes = hasher.finalize();

    // Convert to fixed-size array
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash_bytes);

    Ok(ContentHash::from_bytes(result))
}

/// Helper struct for hashing (excludes id and signature)
#[derive(serde::Serialize)]
struct HashableEntry<'a> {
    timestamp: u64,
    author: &'a icn_identity::Did,
    contract_ref: Option<&'a ContentHash>,
    accounts: &'a [crate::types::AccountDelta],
    parents: &'a [ContentHash],
}

impl JournalEntry {
    /// Compute and set the content hash for this entry
    pub fn compute_hash(&mut self) -> Result<ContentHash> {
        let hash = compute_entry_hash(self)?;
        self.id = Some(hash.clone());
        Ok(hash)
    }

    /// Get the content hash (computes if not already set)
    pub fn get_hash(&mut self) -> Result<ContentHash> {
        if let Some(ref hash) = self.id {
            Ok(hash.clone())
        } else {
            self.compute_hash()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{AccountDelta, JournalEntry};
    use icn_identity::KeyPair;

    #[test]
    fn test_deterministic_hashing() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut entry1 = JournalEntry {
            id: None,
            timestamp: 1234567890,
            author: did.clone(),
            contract_ref: None,
            accounts: vec![
                AccountDelta::debit(did.clone(), "hours".to_string(), 10),
                AccountDelta::credit(did.clone(), "hours".to_string(), 10),
            ],
            parents: vec![],
            signature: None,
        };

        let mut entry2 = entry1.clone();

        let hash1 = entry1.compute_hash().unwrap();
        let hash2 = entry2.compute_hash().unwrap();

        assert_eq!(hash1, hash2, "Same entry should produce same hash");
    }

    #[test]
    fn test_different_content_different_hash() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut entry1 = JournalEntry {
            id: None,
            timestamp: 1234567890,
            author: did.clone(),
            contract_ref: None,
            accounts: vec![
                AccountDelta::debit(did.clone(), "hours".to_string(), 10),
                AccountDelta::credit(did.clone(), "hours".to_string(), 10),
            ],
            parents: vec![],
            signature: None,
        };

        let mut entry2 = entry1.clone();
        entry2.timestamp = 9999999999; // Different timestamp

        let hash1 = entry1.compute_hash().unwrap();
        let hash2 = entry2.compute_hash().unwrap();

        assert_ne!(
            hash1, hash2,
            "Different content should produce different hash"
        );
    }
}
