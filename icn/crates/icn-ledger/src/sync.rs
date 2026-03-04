//! Ledger synchronization via gossip

use crate::types::{ContentHash, JournalEntry, WitnessedEntry};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Message types for ledger synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LedgerSyncMessage {
    /// Announce a new journal entry (without witness signatures)
    NewEntry {
        hash: ContentHash,
        entry: JournalEntry,
    },

    /// Announce a new witnessed entry with signatures (Issue #676)
    ///
    /// Used when the ledger requires witness signatures for entries above
    /// a certain threshold. Receivers validate signatures before accepting.
    NewWitnessedEntry {
        hash: ContentHash,
        witnessed: WitnessedEntry,
    },

    /// Request a journal entry by hash
    RequestEntry { hash: ContentHash },

    /// Response with requested entry
    EntryResponse {
        hash: ContentHash,
        entry: Option<JournalEntry>,
    },

    /// Rollback notification from governance (emergency recovery)
    ///
    /// This message instructs all nodes to roll back to a specific entry.
    /// Only valid when originating from an accepted governance proposal.
    RollbackNotification {
        /// Hash of the entry to roll back to (all entries after this are archived)
        target_hash: ContentHash,
        /// Hashes of entries that were archived (for verification)
        archived_entries: Vec<ContentHash>,
        /// Reason for rollback (from governance proposal)
        reason: String,
        /// Unix timestamp when rollback was executed
        executed_at: u64,
    },
}

/// Get the topic name for a currency's ledger sync
pub fn ledger_topic(currency: &str) -> String {
    format!("ledger:{currency}")
}

/// Serialize a sync message for gossip transmission
pub fn serialize_sync_message(msg: &LedgerSyncMessage) -> Result<Vec<u8>> {
    serde_json::to_vec(msg).context("Failed to serialize ledger sync message")
}

/// Deserialize a sync message from gossip
pub fn deserialize_sync_message(data: &[u8]) -> Result<LedgerSyncMessage> {
    serde_json::from_slice(data).context("Failed to deserialize ledger sync message")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::JournalEntryBuilder;
    use icn_identity::KeyPair;

    #[test]
    fn test_topic_naming() {
        assert_eq!(ledger_topic("hours"), "ledger:hours");
        assert_eq!(ledger_topic("USD"), "ledger:USD");
    }

    #[test]
    fn test_message_serialization() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .with_system_provenance("test")
            .build()
            .unwrap();

        let hash = entry.id.clone().unwrap();

        let msg = LedgerSyncMessage::NewEntry {
            hash: hash.clone(),
            entry: entry.clone(),
        };

        // Serialize
        let data = serialize_sync_message(&msg).unwrap();

        // Deserialize
        let msg2 = deserialize_sync_message(&data).unwrap();

        match msg2 {
            LedgerSyncMessage::NewEntry {
                hash: h2,
                entry: e2,
            } => {
                assert_eq!(h2, hash);
                assert_eq!(e2.accounts.len(), 2);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
