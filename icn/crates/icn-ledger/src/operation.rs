//! Operation identity: one economic operation, as opposed to one journal record.
//!
//! A journal entry id is a digest of the entry's content, including a millisecond timestamp
//! (`hash.rs`). It answers "is this the same record?". It cannot answer "is this the same
//! operation?":
//!
//! - a retry of one operation built a millisecond later is a different record;
//! - two separate operations of one shape built in the same millisecond are the same record.
//!
//! An [`OperationId`] is the caller's durable name for one operation. It travels in the entry's
//! existing `nonce` field, so it is already part of the hashed content and setting it changes no
//! encoding. [`Ledger::append_operation`](crate::Ledger::append_operation) gives it meaning: at
//! most one journal entry is ever appended for an operation id, a retry is answered with the
//! entry that was appended the first time, and the same id with different content is refused.
//!
//! # What this is and is not
//!
//! - **Opt-in.** [`Ledger::append_entry`](crate::Ledger::append_entry) is unchanged. An author gets
//!   these guarantees only by appending through `append_operation`.
//! - **One id, one entry.** An operation that needs several entries derives one id per entry. The
//!   commons settlement pair predates this contract and shares one nonce across two entries; it
//!   keeps working through `append_entry` and is not an operation in this sense.
//! - **The journal is the record.** An operation is applied exactly when a journal entry carrying
//!   its id exists. The index below is rebuilt from the journal on open and every row it yields
//!   is checked against the journal before it is believed, so it introduces no persisted format
//!   and cannot report an entry that is no longer journaled.
//! - **Journaled is not complete.** [`OperationAppend::Replayed`] says the operation's entry is in
//!   the journal. It does not say that every projection derived from it (cached balances, budget
//!   records kept by a caller) was written.
//! - **No derivation is defined here.** How an author derives an id from its own durable source
//!   (a receipt hash, an escrow id, a client idempotency key) is a canonical encoding decided per
//!   operation class. A value that changes on every attempt, such as a timestamp or a random
//!   number, gives distinct entries but never a recognisable retry.

use std::collections::HashMap;
use std::fmt;

use anyhow::{Context, Result};

use crate::hash::compute_entry_hash;
use crate::ledger::{Ledger, JOURNAL_PREFIX};
use crate::types::{ContentHash, JournalEntry};

/// The durable identity of one economic operation.
///
/// Carried in [`JournalEntry::nonce`]. See the [module documentation](self).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId([u8; 32]);

impl OperationId {
    /// Wrap 32 bytes that the caller derived from a durable source identity.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The bytes stored in the entry's `nonce` field.
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Lowercase hex, for logs and error messages.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Debug for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OperationId({})", self.to_hex())
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// What [`Ledger::append_operation`](crate::Ledger::append_operation) did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationAppend {
    /// The operation was new. This entry was appended for it.
    Applied(ContentHash),
    /// The operation's entry was already journaled. Nothing was appended; this is the id of the
    /// entry appended the first time, which can differ from the id of the entry offered now.
    Replayed(ContentHash),
}

impl OperationAppend {
    /// The id of the journal entry that carries the operation.
    pub fn entry_id(&self) -> &ContentHash {
        match self {
            Self::Applied(id) | Self::Replayed(id) => id,
        }
    }

    /// True when nothing was appended because the operation was already journaled.
    pub fn is_replay(&self) -> bool {
        matches!(self, Self::Replayed(_))
    }
}

/// Digest of everything the entry id covers except the timestamp.
///
/// Two entries are the same operation content exactly when they would have the same id if they
/// had been built in the same millisecond. Reusing the id hash keeps that statement literal: the
/// comparison can never drift from what the id covers. The value is compared in memory only and
/// is never stored.
pub(crate) fn content_digest(entry: &JournalEntry) -> Result<ContentHash> {
    let mut timeless = entry.clone();
    timeless.timestamp = 0;
    compute_entry_hash(&timeless)
}

/// In-memory index from a nonce to the journal entries that carry it.
///
/// Derived state: rebuilt from the journal on open, extended on every append, never persisted.
/// It indexes every nonce-bearing entry, including ones appended through the plain path, so that
/// an operation id already used by more than one entry is seen as ambiguous instead of being
/// matched against whichever entry happened to be first.
///
/// Rows are hints. An entry can leave the journal without this index hearing of it, so
/// [`Ledger::operation_entry`](crate::Ledger::operation_entry) reads each indexed entry back
/// from the journal and ignores a row whose entry is gone.
#[derive(Default)]
pub(crate) struct OperationIndex {
    by_nonce: HashMap<[u8; 32], Vec<ContentHash>>,
}

impl OperationIndex {
    /// Record that the journal entry `id` carries `nonce`. Recording a pair twice is a no-op.
    pub(crate) fn record(&mut self, nonce: [u8; 32], id: &ContentHash) {
        let ids = self.by_nonce.entry(nonce).or_default();
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }

    /// Ids of the entries recorded under `nonce`, in the order they were recorded.
    pub(crate) fn entries_for(&self, nonce: &[u8; 32]) -> &[ContentHash] {
        self.by_nonce.get(nonce).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Rebuild the operation index from the journal.
///
/// The entry id comes from the journal key, not from the stored value: a stored entry does not
/// carry its id (`JournalEntry::id` is not serialized), and the key is what
/// [`Ledger::get_entry`](crate::Ledger::get_entry) resolves. A key under the journal prefix that
/// is not an entry id means the journal is not what this code understands, so the rebuild fails
/// instead of skipping a record it could later miss.
pub(crate) fn rebuild_index(ledger: &mut Ledger) -> Result<()> {
    const PAGE: usize = 1000;
    let prefix = JOURNAL_PREFIX.as_bytes();
    let mut index = OperationIndex::default();
    let mut cursor: Option<Vec<u8>> = None;

    loop {
        let page = ledger.store.scan_after(prefix, cursor.as_deref(), PAGE)?;
        let page_len = page.len();
        cursor = page.last().map(|(key, _)| key.clone());

        for (key, value) in page {
            let entry: JournalEntry = serde_json::from_slice(&value)?;
            let Some(nonce) = entry.nonce else {
                continue;
            };
            let id = entry_id_from_journal_key(&key)?;
            index.record(nonce, &id);
        }

        if page_len < PAGE {
            break;
        }
    }

    ledger.operation_index = index;
    Ok(())
}

/// The entry id a journal key names: the 64 hex characters after the journal prefix.
fn entry_id_from_journal_key(key: &[u8]) -> Result<ContentHash> {
    let hex_id = key
        .strip_prefix(JOURNAL_PREFIX.as_bytes())
        .context("journal key does not start with the journal prefix")?;
    let mut id = [0u8; 32];
    hex::decode_to_slice(hex_id, &mut id).with_context(|| {
        format!(
            "journal key {:?} does not name an entry id",
            String::from_utf8_lossy(key)
        )
    })?;
    Ok(ContentHash::from_bytes(id))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::{AccountDelta, ProvenanceRef};
    use icn_identity::IdentityBundle;

    fn entry_between(
        payer: &icn_identity::Did,
        payee: &icn_identity::Did,
        timestamp: u64,
        nonce: Option<[u8; 32]>,
        amount: i64,
    ) -> JournalEntry {
        let mut entry = JournalEntry {
            id: None,
            timestamp,
            author: payer.clone(),
            contract_ref: None,
            accounts: vec![
                AccountDelta::debit(payer.clone(), "hours".to_string(), amount),
                AccountDelta::credit(payee.clone(), "hours".to_string(), amount),
            ],
            parents: vec![],
            signature: None,
            nonce,
            provenance: ProvenanceRef::SystemGenerated {
                reason: "test".to_string(),
            },
        };
        entry.compute_hash().unwrap();
        entry
    }

    #[test]
    fn content_digest_ignores_only_the_timestamp() {
        let payer = IdentityBundle::generate().unwrap().did().clone();
        let payee = IdentityBundle::generate().unwrap().did().clone();
        let first = entry_between(&payer, &payee, 1_000, Some([7; 32]), 5);
        let later = entry_between(&payer, &payee, 2_000, Some([7; 32]), 5);
        assert_ne!(
            first.id, later.id,
            "a later millisecond is a different record"
        );
        assert_eq!(
            content_digest(&first).unwrap(),
            content_digest(&later).unwrap()
        );

        let other_amount = entry_between(&payer, &payee, 1_000, Some([7; 32]), 6);
        assert_ne!(
            content_digest(&first).unwrap(),
            content_digest(&other_amount).unwrap()
        );
        let other_nonce = entry_between(&payer, &payee, 1_000, Some([8; 32]), 5);
        assert_ne!(
            content_digest(&first).unwrap(),
            content_digest(&other_nonce).unwrap()
        );
        let other_payee = entry_between(&payer, &payer, 1_000, Some([7; 32]), 5);
        assert_ne!(
            content_digest(&first).unwrap(),
            content_digest(&other_payee).unwrap()
        );
    }

    #[test]
    fn index_records_a_pair_once_and_keeps_order() {
        let mut index = OperationIndex::default();
        let nonce = [9; 32];
        let a = ContentHash::from_bytes([1; 32]);
        let b = ContentHash::from_bytes([2; 32]);

        index.record(nonce, &a);
        index.record(nonce, &a);
        assert_eq!(index.entries_for(&nonce), std::slice::from_ref(&a));

        index.record(nonce, &b);
        assert_eq!(index.entries_for(&nonce), &[a, b]);
        assert!(index.entries_for(&[8; 32]).is_empty());
    }

    #[test]
    fn journal_key_names_the_entry_id() {
        let id = ContentHash::from_bytes([0xAB; 32]);
        let key = format!("{JOURNAL_PREFIX}{}", id.to_hex());
        assert_eq!(entry_id_from_journal_key(key.as_bytes()).unwrap(), id);

        for bad in [
            format!("{JOURNAL_PREFIX}not-hex"),
            format!("{JOURNAL_PREFIX}{}", "ab".repeat(31)),
            format!("{JOURNAL_PREFIX}{}00", id.to_hex()),
            format!("other:{}", id.to_hex()),
        ] {
            assert!(
                entry_id_from_journal_key(bad.as_bytes()).is_err(),
                "{bad} must not be read as an entry id"
            );
        }
    }
}
