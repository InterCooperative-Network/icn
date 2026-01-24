//! Fork detection and resolution operations for the ledger
//!
//! This module contains fork detection and resolution operations extracted from
//! the main Ledger implementation. These functions handle:
//! - Fork detection via `ForkDetector`
//! - Fork resolution via `ForkResolver`
//! - Quarantining of forked entries
//! - Fork statistics and indexing
//!
//! # Fork Detection
//!
//! A fork occurs when two entries reference the same parent(s) but have different
//! content hashes. The `ForkDetector` maintains a parent → children index to detect
//! when multiple entries reference the same parent.
//!
//! # Fork Resolution
//!
//! The `ForkResolver` applies strategies (timestamp preference, trust-weighted, etc.)
//! to determine which entry should be canonical. Losing entries are quarantined.
//!
//! # Testing
//!
//! Tests for these functions are located in the main `ledger.rs` test module
//! (`crates/icn-ledger/src/ledger.rs`). The tests use the public `Ledger` API
//! which delegates to these functions.

use crate::fork_resolution::{Fork, ForkResolution};
use crate::ledger::{Ledger, JOURNAL_PREFIX, JOURNAL_TS_PREFIX};
use crate::merge::QuarantineItem;
use crate::types::{ContentHash, JournalEntry, QuarantineReason};
use anyhow::{Context, Result};
use std::time::SystemTime;
use tracing::{debug, info, instrument, warn};

/// Statistics about forks in the ledger
#[derive(Debug, Clone)]
pub struct ForkStats {
    /// Total number of detected forks
    pub total_forks: usize,
    /// Parent hashes that have forks
    pub parents_with_forks: Vec<ContentHash>,
}

/// Detect any forks in the ledger
///
/// Returns a list of (parent_hash, child_hashes) tuples where a parent has multiple children.
///
/// # Arguments
/// * `ledger` - The ledger instance
///
/// # Returns
/// Vector of tuples containing parent hash and its multiple child hashes
pub(crate) fn detect_forks(ledger: &Ledger) -> Vec<(ContentHash, Vec<ContentHash>)> {
    ledger.fork_detector.detect_forks()
}

/// Check if a specific parent has a fork
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `parent` - The parent hash to check
///
/// # Returns
/// `true` if the parent has multiple children (fork detected), `false` otherwise
pub(crate) fn has_fork(ledger: &Ledger, parent: &ContentHash) -> bool {
    ledger.fork_detector.has_fork(parent)
}

/// Detect and resolve all forks in the ledger
///
/// Returns a list of resolved forks with their resolutions.
/// Entries that should be discarded are quarantined.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable to allow quarantining)
///
/// # Returns
/// Vector of (Fork, ForkResolution) tuples for all resolved forks
#[instrument(skip(ledger))]
pub(crate) fn detect_and_resolve_forks(ledger: &mut Ledger) -> Result<Vec<(Fork, ForkResolution)>> {
    let forks = ledger.fork_detector.detect_forks();

    if forks.is_empty() {
        debug!("No forks detected in ledger");
        return Ok(vec![]);
    }

    info!(
        fork_count = forks.len(),
        "Detected forks in ledger, attempting resolution"
    );

    let mut resolutions = Vec::new();

    for (parent, children) in forks {
        // Get the actual entries for comparison
        let mut entries = Vec::new();
        for child_hash in &children {
            if let Some(entry) = ledger.get_entry(child_hash)? {
                entries.push(entry);
            }
        }

        // Handle N-way forks using tournament-style resolution
        // Compare entries pairwise: winner of round 1 vs entry 3, winner vs entry 4, etc.
        if entries.len() >= 2 {
            let is_nway = entries.len() > 2;
            let entry_count = entries.len();

            // Track winning entry index and all losers
            let mut winner_idx = 0;
            let mut losers: Vec<usize> = Vec::new();
            let mut requires_manual = false;
            let mut manual_reason = String::new();

            // Tournament: compare current winner against each subsequent entry
            for challenger_idx in 1..entries.len() {
                let fork = Fork {
                    common_parents: vec![parent.clone()],
                    entry1: entries[winner_idx].clone(),
                    entry2: entries[challenger_idx].clone(),
                    detected_at: SystemTime::now(),
                };

                match ledger.fork_resolver.resolve_fork(&fork) {
                    Ok(resolution) => {
                        match &resolution {
                            ForkResolution::KeepFirst => {
                                // Current winner stays, challenger loses
                                losers.push(challenger_idx);
                                debug!(
                                    round = challenger_idx,
                                    winner = winner_idx,
                                    "Tournament round: keeping current winner"
                                );
                            }
                            ForkResolution::KeepSecond => {
                                // Challenger wins, previous winner loses
                                losers.push(winner_idx);
                                winner_idx = challenger_idx;
                                debug!(
                                    round = challenger_idx,
                                    new_winner = winner_idx,
                                    "Tournament round: challenger wins"
                                );
                            }
                            ForkResolution::RequiresManual { reason } => {
                                requires_manual = true;
                                manual_reason = reason.clone();
                                warn!(
                                    parent = %parent.to_hex(),
                                    round = challenger_idx,
                                    reason = reason,
                                    "Fork requires manual resolution, stopping tournament"
                                );
                                break;
                            }
                        }

                        // Store the last resolution for reporting
                        if challenger_idx == entries.len() - 1 && !requires_manual {
                            resolutions.push((fork, resolution));
                        }
                    }
                    Err(e) => {
                        warn!(
                            parent = %parent.to_hex(),
                            round = challenger_idx,
                            error = %e,
                            "Failed to resolve fork round"
                        );
                    }
                }
            }

            // Handle manual resolution requirement
            if requires_manual {
                icn_obs::metrics::ledger_forks::manual_resolution_required_inc(&manual_reason);
                continue;
            }

            // Quarantine all losing entries
            for loser_idx in &losers {
                let loser_entry = &entries[*loser_idx];
                if let Some(hash) = &loser_entry.id {
                    quarantine_forked_entry(
                        ledger,
                        loser_entry,
                        if is_nway {
                            "Lost N-way fork resolution"
                        } else {
                            "Lost fork resolution"
                        },
                    )?;
                    debug!(
                        quarantined = %hash.to_hex(),
                        entry_index = loser_idx,
                        "Quarantined losing fork entry"
                    );
                }
            }

            // Record metrics
            icn_obs::metrics::ledger_forks::resolved_inc("hybrid");
            if is_nway {
                icn_obs::metrics::ledger_forks::nway_fork_resolved_inc(entry_count);
                info!(
                    parent = %parent.to_hex(),
                    entry_count = entry_count,
                    losers_quarantined = losers.len(),
                    winner_idx = winner_idx,
                    "Resolved N-way fork via tournament"
                );
            } else {
                info!(
                    parent = %parent.to_hex(),
                    "Resolved 2-way fork"
                );
            }
        }
    }

    Ok(resolutions)
}

/// Quarantine an entry that lost fork resolution
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
/// * `entry` - The entry to quarantine
/// * `reason` - Reason for quarantining (e.g., "Lost fork resolution")
///
/// # Returns
/// `Ok(())` if successful, error otherwise
pub(crate) fn quarantine_forked_entry(
    ledger: &mut Ledger,
    entry: &JournalEntry,
    reason: &str,
) -> Result<()> {
    let hash = entry.id.as_ref().context("Entry missing hash")?;

    // Remove from main store
    let key = format!("{}{}", JOURNAL_PREFIX, hash.to_hex());
    ledger.store.delete(key.as_bytes())?;

    // Remove from timestamp index
    let ts_key = format!(
        "{}{:020}:{}",
        JOURNAL_TS_PREFIX,
        entry.timestamp,
        hash.to_hex()
    );
    ledger.store.delete(ts_key.as_bytes())?;

    // Add to quarantine
    let item = QuarantineItem::new(
        hash.clone(),
        QuarantineReason::ForkConflict(reason.to_string()),
        entry.author.clone(),
    );
    ledger.quarantine.add(entry.clone(), item)?;

    // Record Byzantine violation for conflicting ledger entries (Phase 18)
    if let Some(ref detector) = ledger.misbehavior_detector {
        // Find the conflicting entry hash from the first parent
        let conflicting_hash = entry
            .parents
            .first()
            .cloned()
            .unwrap_or_else(|| hash.clone());

        let violation = icn_security::Violation::ConflictingLedgerEntries {
            entry1: hash.as_bytes().try_into().unwrap_or([0u8; 32]),
            entry2: conflicting_hash.as_bytes().try_into().unwrap_or([0u8; 32]),
        };

        // SAFETY: Use block_in_place to report violation from sync context.
        // This may be called from tokio runtime; block_in_place moves other tasks off this thread.
        let detector_clone = detector.clone();
        let author = entry.author.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                detector_clone
                    .write()
                    .await
                    .record_violation(&author, violation, vec![]);
            })
        });
    }

    // Recompute balances (expensive but necessary for correctness)
    ledger.recompute_balances()?;

    Ok(())
}

/// Get fork resolution statistics
///
/// # Arguments
/// * `ledger` - The ledger instance
///
/// # Returns
/// ForkStats with total fork count and parent hashes
pub(crate) fn get_fork_stats(ledger: &Ledger) -> ForkStats {
    let forks = ledger.fork_detector.detect_forks();
    ForkStats {
        total_forks: forks.len(),
        parents_with_forks: forks.iter().map(|(p, _)| p.clone()).collect(),
    }
}

/// Rebuild the fork detection index
///
/// This re-indexes all entries in the ledger for fork detection.
/// Called during ledger initialization or after major operations.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
///
/// # Returns
/// `Ok(())` if successful, error otherwise
pub(crate) fn rebuild_fork_index(ledger: &mut Ledger) -> Result<()> {
    let entries = ledger.get_all_entries()?;
    let entry_count = entries.len();

    for entry in entries {
        ledger.fork_detector.index_entry(&entry);
    }

    info!(entry_count = entry_count, "Rebuilt fork detection index");

    Ok(())
}

/// Ensure timestamp index exists for efficient pagination
///
/// This migrates existing ledgers that were created before the timestamp
/// index was introduced. Only runs if the index is empty but entries exist.
///
/// # Arguments
/// * `ledger` - The ledger instance
///
/// # Returns
/// `Ok(())` if successful, error otherwise
pub(crate) fn ensure_timestamp_index(ledger: &Ledger) -> Result<()> {
    let ts_prefix = JOURNAL_TS_PREFIX.as_bytes();
    let ts_count = ledger.store.scan_count(ts_prefix)?;

    // If timestamp index already has entries, no migration needed
    if ts_count > 0 {
        return Ok(());
    }

    // Check if we have journal entries that need indexing
    let journal_prefix = JOURNAL_PREFIX.as_bytes();
    let pairs = ledger.store.scan(journal_prefix)?;

    if pairs.is_empty() {
        return Ok(());
    }

    info!(
        entry_count = pairs.len(),
        "Migrating ledger: building timestamp index"
    );

    for (key, value) in pairs {
        // Extract hash from key (after prefix)
        let key_str = String::from_utf8_lossy(&key);
        let hash_hex = key_str.trim_start_matches(JOURNAL_PREFIX);

        // Deserialize entry to get timestamp
        let entry: JournalEntry = serde_json::from_slice(&value)?;

        // Write to timestamp index
        let ts_key = format!("{}{:020}:{}", JOURNAL_TS_PREFIX, entry.timestamp, hash_hex);
        if let Ok(hash_bytes) = hex::decode(hash_hex) {
            ledger.store.put(ts_key.as_bytes(), &hash_bytes)?;
        }
    }

    info!("Timestamp index migration complete");
    Ok(())
}
