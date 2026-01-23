//! Query operations for the ledger
//!
//! This module contains read-only query operations that retrieve journal entries
//! from storage. These functions are extracted from the main Ledger implementation
//! for better code organization.

use crate::ledger::{ArchiveRecord, Ledger, PaginationCursor, ARCHIVE_PREFIX, JOURNAL_PREFIX, JOURNAL_TS_PREFIX};
use crate::types::{ContentHash, JournalEntry};
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
pub(crate) fn get_archived_entries(ledger: &Ledger, archive_timestamp: u64) -> Result<Vec<JournalEntry>> {
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
