//! Query operations for the ledger
//!
//! This module contains read-only query operations that retrieve journal entries
//! from storage. These functions are extracted from the main Ledger implementation
//! for better code organization.
//!
//! # Testing
//!
//! Tests for these functions are located in the main `ledger.rs` test module
//! (`crates/icn-ledger/src/ledger.rs`). The tests use the public `Ledger` API
//! which delegates to these functions, ensuring the public interface remains
//! stable while allowing internal reorganization.
//!
//! Key test functions:
//! - `test_append_and_retrieve_entry` - tests `get_entry()`
//! - `test_pagination_basic`, `test_pagination_last_page`, etc. - tests pagination functions
//! - Integration tests in `crates/icn-ledger/tests/` also exercise these functions

use crate::ledger::{
    ArchiveRecord, Ledger, PaginationCursor, ARCHIVE_PREFIX, DECISION_INDEX_PREFIX, JOURNAL_PREFIX,
    JOURNAL_TS_PREFIX,
};
use crate::types::{ContentHash, JournalEntry, ProvenanceRef};
use anyhow::Result;
use icn_identity::Did;
use tracing::warn;

/// Get a specific journal entry by its content hash
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `hash` - The content hash of the entry to retrieve
///
/// # Returns
/// Some(entry) if found, None otherwise
pub(crate) fn get_entry(ledger: &Ledger, hash: &ContentHash) -> Result<Option<JournalEntry>> {
    let key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
    let value = ledger.store.get(key.as_bytes())?;

    match value {
        Some(bytes) => {
            let entry: JournalEntry = serde_json::from_slice(&bytes)?;
            Ok(Some(entry))
        }
        None => Ok(None),
    }
}

/// Get all journal entries
pub(crate) fn get_all_entries(ledger: &Ledger) -> Result<Vec<JournalEntry>> {
    let prefix = JOURNAL_PREFIX.as_bytes();
    let pairs = ledger.store.scan(prefix)?;

    let mut entries = Vec::new();
    for (_key, value) in pairs {
        let entry: JournalEntry = serde_json::from_slice(&value)?;
        entries.push(entry);
    }

    // Sort by timestamp for deterministic ordering
    entries.sort_by_key(|e| e.timestamp);

    Ok(entries)
}

/// Count the total number of journal entries
///
/// More efficient than `get_all_entries().len()` as it doesn't
/// deserialize entries.
pub(crate) fn count_entries(ledger: &Ledger) -> Result<usize> {
    let prefix = JOURNAL_PREFIX.as_bytes();
    ledger.store.scan_count(prefix)
}

/// Get journal entries with pagination (newest first)
///
/// Returns entries in reverse chronological order (most recent first),
/// which is the typical use case for displaying transaction history.
///
/// Uses a timestamp-based secondary index for O(1) count and O(limit)
/// entry retrieval, making this efficient for large ledgers.
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `offset` - Number of entries to skip (0-based)
/// * `limit` - Maximum number of entries to return
///
/// # Returns
/// Tuple of (entries, total_count)
pub(crate) fn get_entries_paginated(
    ledger: &Ledger,
    offset: usize,
    limit: usize,
) -> Result<(Vec<JournalEntry>, usize)> {
    // Use timestamp index for efficient pagination (entries sorted by timestamp)
    let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();
    let total = ledger.store.scan_count(ts_prefix)?;

    // Early return if offset is beyond total
    if offset >= total {
        return Ok((Vec::new(), total));
    }

    // For descending order (newest first), we need to read from the end
    // Calculate the starting position from the ascending index
    let items_from_end = offset + limit;
    let skip_from_start = total.saturating_sub(items_from_end);
    let take_count = limit.min(total.saturating_sub(offset));

    // Scan the timestamp index with calculated offset
    let (ts_pairs, _) = ledger
        .store
        .scan_paginated(ts_prefix, skip_from_start, take_count)?;

    // Look up entries by hash (values in timestamp index are hashes)
    let mut entries = Vec::with_capacity(ts_pairs.len());
    for (_key, hash_bytes) in ts_pairs {
        let hash_hex = hex::encode(&hash_bytes);
        let entry_key = format!("{}{}", JOURNAL_PREFIX, &hash_hex);
        if let Some(entry_data) = ledger.store.get(entry_key.as_bytes())? {
            let mut entry: JournalEntry = serde_json::from_slice(&entry_data)?;
            // Restore the id from the hash (not serialized due to #[serde(skip)])
            if let Ok(hash_arr) = <[u8; 32]>::try_from(hash_bytes.as_slice()) {
                entry.id = Some(ContentHash::from_bytes(hash_arr));
            }
            entries.push(entry);
        }
    }

    // Reverse to get descending order (newest first)
    entries.reverse();

    Ok((entries, total))
}

/// Get journal entries with pagination (oldest first)
///
/// Returns entries in chronological order (oldest first).
/// Useful for auditing and sequential processing.
///
/// Uses a timestamp-based secondary index for O(1) count and O(limit)
/// entry retrieval, making this efficient for large ledgers.
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `offset` - Number of entries to skip (0-based)
/// * `limit` - Maximum number of entries to return
///
/// # Returns
/// Tuple of (entries, total_count)
pub(crate) fn get_entries_paginated_asc(
    ledger: &Ledger,
    offset: usize,
    limit: usize,
) -> Result<(Vec<JournalEntry>, usize)> {
    // Use timestamp index for efficient pagination (already sorted by timestamp ASC)
    let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();

    // Scan with pagination - this efficiently skips and takes from the sorted index
    let (ts_pairs, total) = ledger.store.scan_paginated(ts_prefix, offset, limit)?;

    // Early return if offset is beyond total
    if offset >= total {
        return Ok((Vec::new(), total));
    }

    // Look up entries by hash (values in timestamp index are hashes)
    let mut entries = Vec::with_capacity(ts_pairs.len());
    for (_key, hash_bytes) in ts_pairs {
        let hash_hex = hex::encode(&hash_bytes);
        let entry_key = format!("{}{}", JOURNAL_PREFIX, &hash_hex);
        if let Some(entry_data) = ledger.store.get(entry_key.as_bytes())? {
            let mut entry: JournalEntry = serde_json::from_slice(&entry_data)?;
            // Restore the id from the hash (not serialized due to #[serde(skip)])
            if let Ok(hash_arr) = <[u8; 32]>::try_from(hash_bytes.as_slice()) {
                entry.id = Some(ContentHash::from_bytes(hash_arr));
            }
            entries.push(entry);
        }
    }

    Ok((entries, total))
}

/// Get journal entries with filtered pagination (oldest first)
///
/// This is a memory-efficient version of filtered queries. Instead of loading
/// all entries and then filtering, it streams through entries one by one,
/// applying the filter during iteration and stopping once `limit` matches found.
///
/// Uses cursor-based pagination: pass the timestamp from the last entry of
/// the previous page to continue from that point.
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `filter_did` - Optional DID to filter by (entry must involve this DID)
/// * `cursor` - Optional timestamp to start after (exclusive)
/// * `limit` - Maximum number of entries to return
///
/// # Returns
/// Tuple of (entries, next_cursor_timestamp)
/// The next_cursor is Some if there may be more entries to fetch.
///
/// # Performance Notes
/// - Memory bounded to O(limit) instead of O(total_entries)
/// - Uses N+1 query pattern: one index scan + one lookup per entry
/// - Acceptable for local Sled storage; would need batch fetching for network storage
/// - Filtered queries scan all index entries but only deserialize matching entries
///
/// # Cursor Format
/// The cursor is a tuple of (timestamp, optional_hash) for proper tie-breaking.
/// When multiple entries share the same timestamp, the hash provides deterministic ordering.
pub(crate) fn get_entries_filtered_paginated(
    ledger: &Ledger,
    filter_did: Option<&Did>,
    cursor: Option<(u64, Option<String>)>,
    limit: usize,
) -> Result<(Vec<JournalEntry>, Option<PaginationCursor>)> {
    // Fast path: No filter AND no cursor - use efficient offset-based pagination
    if filter_did.is_none() && cursor.is_none() {
        let (entries, _total) = get_entries_paginated_asc(ledger, 0, limit + 1)?;
        let has_more = entries.len() > limit;
        let entries: Vec<_> = entries.into_iter().take(limit).collect();
        let next_cursor = if has_more {
            entries.last().map(|e| {
                let hash = e.id.as_ref().map(|h| h.to_hex()).unwrap_or_default();
                (e.timestamp, hash)
            })
        } else {
            None
        };
        return Ok((entries, next_cursor));
    }

    // Extract cursor components
    let cursor_ts = cursor.as_ref().map(|(ts, _)| *ts);
    let cursor_hash = cursor.as_ref().and_then(|(_, h)| h.clone());

    // Streaming path: Either has filter OR has cursor (or both)
    // Stream through timestamp index, applying filter and cursor, stopping early
    let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();
    let ts_pairs = ledger.store.scan(ts_prefix)?;

    let mut entries = Vec::with_capacity(limit);

    for (key, hash_bytes) in ts_pairs {
        // Extract timestamp and hash from key
        // Key format: "ledger:journal_ts:{timestamp:020}:{hash}"
        // Timestamp is stored as zero-padded decimal (not hex)
        let key_str = String::from_utf8_lossy(&key);
        let entry_hash = hex::encode(&hash_bytes);

        // Parse timestamp from key - log errors instead of silently skipping
        let entry_ts = match key_str.strip_prefix(JOURNAL_TS_PREFIX) {
            Some(ts_str) => match ts_str.split(':').next() {
                Some(ts_decimal) => match ts_decimal.parse::<u64>() {
                    Ok(ts) => ts,
                    Err(e) => {
                        warn!(
                            key = %key_str,
                            error = %e,
                            "Malformed timestamp in journal index key - skipping entry"
                        );
                        continue;
                    }
                },
                None => {
                    warn!(
                        key = %key_str,
                        "Missing timestamp component in journal index key - skipping entry"
                    );
                    continue;
                }
            },
            None => {
                warn!(
                    key = %key_str,
                    expected_prefix = JOURNAL_TS_PREFIX,
                    "Unexpected key format in journal timestamp index - skipping entry"
                );
                continue;
            }
        };

        // Skip entries at or before the cursor (using hash for tie-breaking)
        // Entries are sorted by (timestamp ASC, hash ASC) in the index.
        //
        // The cursor points to the LAST entry returned in the previous page.
        // We use <= (not <) for the hash comparison because:
        // - If entry_hash == cursor_hash: this is the same entry, skip to avoid duplicates
        // - If entry_hash < cursor_hash: this entry was already returned, skip it
        // - If entry_hash > cursor_hash: this is a new entry, include it
        if let Some(cursor) = cursor_ts {
            if entry_ts < cursor {
                continue;
            }
            if entry_ts == cursor {
                // Same timestamp - use hash for tie-breaking
                if let Some(ref cursor_h) = cursor_hash {
                    if entry_hash <= *cursor_h {
                        continue;
                    }
                }
            }
        }

        // Look up the full entry
        let entry_key = format!("{}{}", JOURNAL_PREFIX, &entry_hash);
        if let Some(entry_data) = ledger.store.get(entry_key.as_bytes())? {
            let mut entry: JournalEntry = serde_json::from_slice(&entry_data)?;
            // Restore the id from the hash (not serialized due to #[serde(skip)])
            if let Ok(hash_arr) = <[u8; 32]>::try_from(hash_bytes.as_slice()) {
                entry.id = Some(ContentHash::from_bytes(hash_arr));
            }

            // Apply DID filter if present
            let matches_filter = match filter_did {
                Some(did) => entry.accounts.iter().any(|delta| &delta.account_id == did),
                None => true, // No filter means all entries match
            };

            if matches_filter {
                entries.push(entry);

                // Stop once we have enough + 1 (to check if there are more)
                if entries.len() > limit {
                    break;
                }
            }
        }
    }

    // Check if there are more entries
    let has_more = entries.len() > limit;
    if has_more {
        entries.pop(); // Remove the extra entry
    }

    // Next cursor includes both timestamp and hash for proper tie-breaking
    let next_cursor = if has_more {
        entries.last().map(|e| {
            let hash = e.id.as_ref().map(|h| h.to_hex()).unwrap_or_default();
            (e.timestamp, hash)
        })
    } else {
        None
    };

    Ok((entries, next_cursor))
}

/// Get archived entries for a specific rollback timestamp
pub(crate) fn get_archived_entries(
    ledger: &Ledger,
    archive_timestamp: u64,
) -> Result<Vec<JournalEntry>> {
    let prefix = format!("{ARCHIVE_PREFIX}{archive_timestamp}:");
    let pairs = ledger.store.scan(prefix.as_bytes())?;

    let mut entries = Vec::new();
    for (_key, value) in pairs {
        let record: ArchiveRecord = serde_json::from_slice(&value)?;
        entries.push(record.entry);
    }

    Ok(entries)
}

/// List all rollback timestamps (for recovery purposes)
pub(crate) fn list_rollback_timestamps(ledger: &Ledger) -> Result<Vec<u64>> {
    let prefix = ARCHIVE_PREFIX.as_bytes();
    let pairs = ledger.store.scan(prefix)?;

    let mut timestamps = std::collections::HashSet::new();
    for (key, _value) in pairs {
        // Key format: "ledger:archive:{timestamp}:{hash}"
        let key_str = String::from_utf8_lossy(&key);
        if let Some(rest) = key_str.strip_prefix(ARCHIVE_PREFIX) {
            if let Some(ts_str) = rest.split(':').next() {
                if let Ok(ts) = ts_str.parse::<u64>() {
                    timestamps.insert(ts);
                }
            }
        }
    }

    let mut sorted: Vec<_> = timestamps.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

/// Get governance-authorized entries for a `decision_hash` via the secondary
/// index. See [`crate::ledger::Ledger::get_entries_by_decision_hash`].
///
/// The index (`ledger:decision_idx:{decision_hash}` -> `Vec<entry_hash_hex>`) is
/// only a hint; the journal entry is the source of truth. Each indexed hash is
/// resolved against the live journal and its provenance re-verified, so rows for
/// entries removed by archival/quarantine — or otherwise stale — are skipped.
/// This makes the result identical to the old full-journal scan (which also read
/// only live `journal:`/`journal_ts:` rows) while costing O(matches), not
/// O(ledger).
pub(crate) fn get_entries_by_decision_hash(
    ledger: &Ledger,
    decision_hash: &str,
    offset: usize,
    limit: usize,
) -> Result<(Vec<JournalEntry>, usize)> {
    // Per-entry index keys: `ledger:decision_idx:{decision_hash}:{entry_hash_hex}`.
    // Scanning this prefix touches only this decision's keys, so the lookup stays
    // O(matches). The provenance re-check below makes the trailing `:` delimiter
    // safe even if a `decision_hash` value were a prefix of another.
    let scan_prefix = format!("{}{}:", DECISION_INDEX_PREFIX, decision_hash);
    let rows = ledger.store.scan(scan_prefix.as_bytes())?;

    let mut matches: Vec<JournalEntry> = Vec::with_capacity(rows.len());
    for (key, _value) in rows {
        let key_str = String::from_utf8_lossy(&key);
        let Some(hash_hex) = key_str.strip_prefix(scan_prefix.as_str()) else {
            continue;
        };
        // Decode the locator; a malformed key simply can't resolve, so skip it.
        let Ok(raw) = hex::decode(hash_hex) else {
            continue;
        };
        let Ok(arr) = <[u8; 32]>::try_from(raw.as_slice()) else {
            continue;
        };

        let entry_key = format!("{}{}", JOURNAL_PREFIX, hash_hex);
        let Some(bytes) = ledger.store.get(entry_key.as_bytes())? else {
            // Entry removed by archival/quarantine, or a stale index row.
            continue;
        };

        let mut entry: JournalEntry = serde_json::from_slice(&bytes)?;
        // Re-verify against the journal: the entry, not the index, is authoritative.
        let still_matches = matches!(
            &entry.provenance,
            ProvenanceRef::Governance { decision_hash: dh, .. } if dh == decision_hash
        );
        if !still_matches {
            continue;
        }

        // Restore id from the hash (not serialized; `#[serde(skip)]`).
        entry.id = Some(ContentHash::from_bytes(arr));
        matches.push(entry);
    }

    // Deterministic order: timestamp ascending, ties broken by hash bytes — the
    // same ordering the chronological journal scan produced. Comparing the raw
    // `ContentHash` bytes avoids a per-comparison hex allocation.
    matches.sort_by(|a, b| {
        a.timestamp.cmp(&b.timestamp).then_with(|| {
            a.id.as_ref()
                .map(|h| h.as_bytes())
                .cmp(&b.id.as_ref().map(|h| h.as_bytes()))
        })
    });

    let total = matches.len();
    let page = matches.into_iter().skip(offset).take(limit).collect();
    Ok((page, total))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod decision_index_tests {
    use crate::entry::JournalEntryBuilder;
    use crate::ledger::{Ledger, DECISION_INDEX_BUILT_KEY, DECISION_INDEX_PREFIX, JOURNAL_PREFIX};
    use crate::types::JournalEntry;
    use icn_identity::{Did, KeyPair};
    use icn_store::{SledStore, Store};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    /// Self-balancing governance entry; the unique `amount` makes the content
    /// hash distinct so appends don't collide/overwrite.
    fn gov_entry(author: &Did, peer: &Did, amount: i64, decision_hash: &str) -> JournalEntry {
        JournalEntryBuilder::new(author.clone())
            .debit(author.clone(), "hours".to_string(), amount)
            .credit(peer.clone(), "hours".to_string(), amount)
            .with_governance_provenance(format!("receipt-{amount}"), decision_hash)
            .build()
            .unwrap()
    }

    /// Test B: paging over indexed matches is deterministic with no skip/dup.
    #[tokio::test]
    async fn decision_index_pages_deterministically_without_skip_or_dup() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(tmp.path()).unwrap());
        let mut ledger = Ledger::new(store).unwrap();
        let (a, b) = (did(), did());
        let dh = "dh-paging";

        for i in 0..5 {
            ledger
                .append_entry(gov_entry(&a, &b, (i as i64) + 1, dh))
                .await
                .unwrap();
        }
        // An unrelated decision's entry must never leak into this lookup.
        ledger
            .append_entry(gov_entry(&a, &b, 100, "dh-other"))
            .await
            .unwrap();

        let (p1, total) = ledger.get_entries_by_decision_hash(dh, 0, 2).unwrap();
        assert_eq!(total, 5, "total counts all live matches");
        assert_eq!(p1.len(), 2);
        let (p2, _) = ledger.get_entries_by_decision_hash(dh, 2, 2).unwrap();
        assert_eq!(p2.len(), 2);
        let (p3, _) = ledger.get_entries_by_decision_hash(dh, 4, 2).unwrap();
        assert_eq!(p3.len(), 1, "final partial page");

        // Pages cover all 5 matches exactly once (no skip, no duplicate).
        let mut ids: Vec<String> = p1
            .iter()
            .chain(&p2)
            .chain(&p3)
            .map(|e| e.id.as_ref().unwrap().to_hex())
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 5);

        // Deterministic chronological order across the full sequence.
        let seq: Vec<u64> = p1
            .iter()
            .chain(&p2)
            .chain(&p3)
            .map(|e| e.timestamp)
            .collect();
        assert!(seq.windows(2).all(|w| w[0] <= w[1]));
    }

    /// Test D: non-governance entries create no index rows and never match.
    #[tokio::test]
    async fn non_governance_entries_create_no_decision_index_rows() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(tmp.path()).unwrap());
        let mut ledger = Ledger::new(store.clone()).unwrap();
        let (a, b) = (did(), did());

        for i in 0..3 {
            let e = JournalEntryBuilder::new(a.clone())
                .debit(a.clone(), "hours".to_string(), (i as i64) + 1)
                .credit(b.clone(), "hours".to_string(), (i as i64) + 1)
                .with_system_provenance("filler")
                .build()
                .unwrap();
            ledger.append_entry(e).await.unwrap();
        }

        assert_eq!(
            store.scan_count(DECISION_INDEX_PREFIX.as_bytes()).unwrap(),
            0,
            "no bogus index rows for non-governance entries"
        );
        let (entries, total) = ledger
            .get_entries_by_decision_hash("anything", 0, 10)
            .unwrap();
        assert!(entries.is_empty());
        assert_eq!(total, 0);
    }

    /// Test E: rows for removed entries (archival/quarantine) and stale rows are
    /// skipped — the journal, not the index, is authoritative.
    #[tokio::test]
    async fn lookup_skips_removed_and_stale_index_rows() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(tmp.path()).unwrap());
        let mut ledger = Ledger::new(store.clone()).unwrap();
        let (a, b) = (did(), did());
        let dh = "dh-stale";

        let hash = ledger.append_entry(gov_entry(&a, &b, 1, dh)).await.unwrap();
        assert_eq!(ledger.get_entries_by_decision_hash(dh, 0, 10).unwrap().1, 1);

        // (a) Simulate archival/quarantine: delete the entry from the journal but
        // leave the index row behind. The lookup must skip it.
        let jkey = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
        store.delete(jkey.as_bytes()).unwrap();
        let (entries, total) = ledger.get_entries_by_decision_hash(dh, 0, 10).unwrap();
        assert!(entries.is_empty(), "removed entry is not surfaced");
        assert_eq!(total, 0);

        // (b) A stale per-entry row pointing at a non-existent hash is skipped
        // (no panic).
        let fake = "00".repeat(32);
        let key = format!("{}{}:{}", DECISION_INDEX_PREFIX, "dh-bogus", fake);
        store.put(key.as_bytes(), &[]).unwrap();
        let (entries, total) = ledger
            .get_entries_by_decision_hash("dh-bogus", 0, 10)
            .unwrap();
        assert!(entries.is_empty());
        assert_eq!(total, 0);
    }

    /// Test F: a store written before the index existed is migrated on open, so
    /// old stores never return incomplete results.
    #[tokio::test]
    async fn decision_index_rebuilds_for_pre_index_store() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(tmp.path()).unwrap());
        let dh = "dh-migrate";
        let (a, b) = (did(), did());

        {
            let mut ledger = Ledger::new(store.clone()).unwrap();
            for i in 0..3 {
                ledger
                    .append_entry(gov_entry(&a, &b, (i as i64) + 1, dh))
                    .await
                    .unwrap();
            }
            assert_eq!(ledger.get_entries_by_decision_hash(dh, 0, 10).unwrap().1, 3);
        }

        // Strip the index rows and the migration marker, keeping the journal —
        // the shape of a store written before this index existed.
        for (k, _) in store.scan(DECISION_INDEX_PREFIX.as_bytes()).unwrap() {
            store.delete(&k).unwrap();
        }
        store.delete(DECISION_INDEX_BUILT_KEY.as_bytes()).unwrap();
        assert_eq!(
            store.scan_count(DECISION_INDEX_PREFIX.as_bytes()).unwrap(),
            0
        );

        // Reopening rebuilds the index from the journal (ensure_decision_index).
        let ledger2 = Ledger::new(store.clone()).unwrap();
        let (entries, total) = ledger2.get_entries_by_decision_hash(dh, 0, 10).unwrap();
        assert_eq!(total, 3, "migration backfilled the decision index");
        assert_eq!(entries.len(), 3);
    }
}
