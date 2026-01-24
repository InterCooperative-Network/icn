//! Witness validation operations for the ledger
//!
//! This module contains witness signature validation and management operations.
//! These functions handle multi-signature witness requirements for high-value
//! transactions as specified in Issue #676.
//!
//! # Testing
//!
//! Tests for these functions are located in the main `ledger.rs` test module
//! (`crates/icn-ledger/src/ledger.rs`). The tests use the public `Ledger` API
//! which delegates to these functions, ensuring the public interface remains
//! stable while allowing internal reorganization.
//!
//! Key test functions:
//! - `test_witness_signature_timestamp_future_rejected` - tests timestamp validation
//! - `test_witness_signature_timestamp_expired_rejected` - tests expiration
//! - `test_witness_signature_timestamp_valid_accepted` - tests valid signatures
//! - `test_duplicate_witness_signatures_deduplicated` - tests deduplication
//! - `test_author_as_witness_not_double_counted` - tests signer counting
//! - Integration tests in `crates/icn-ledger/tests/` also exercise these functions

use crate::ledger::{Ledger, WITNESS_PREFIX};
use crate::types::{ContentHash, JournalEntry};
use anyhow::{Context, Result};
use tracing::debug;

/// Validates witness signatures on an entry
///
/// Verifies that all signatures are:
/// 1. Valid Ed25519 signatures over the entry hash
/// 2. Not timestamped in the future (with small tolerance for clock skew)
/// 3. Not expired (if `WitnessConfig::collection_timeout_secs` is configured)
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `witnessed` - The witnessed entry to validate
///
/// # Errors
/// Returns error if any signature is invalid, expired, or from the future
pub(crate) fn validate_witness_signatures(
    ledger: &Ledger,
    witnessed: &crate::types::WitnessedEntry,
) -> Result<()> {
    use ed25519_dalek::{Signature, Verifier};

    let entry_hash = witnessed
        .entry
        .id
        .as_ref()
        .context("Entry must have hash for witness validation")?;

    // Get current time for timestamp validation, failing closed on clock errors
    // to prevent accepting invalid timestamps when the system clock is misconfigured
    let current_time = icn_time::try_current_timestamp_secs()
        .context("Failed to get current time for witness validation")?;

    // Maximum allowed clock skew for witness signatures (5 seconds).
    // This is intentionally stricter than icn-time::MAX_CLOCK_SKEW (300s) because
    // ledger security requires tighter timestamp validation to limit the replay
    // attack window. Witness signatures should be created and validated promptly.
    const MAX_CLOCK_SKEW_SECS: u64 = 5;

    for sig in &witnessed.witness_signatures {
        // Extract public key from witness DID using icn-identity
        let verifying_key = sig
            .witness
            .to_verifying_key()
            .context("Failed to extract verifying key from witness DID")?;

        let signature =
            Signature::from_slice(&sig.signature).context("Invalid Ed25519 signature format")?;

        verifying_key
            .verify(entry_hash.as_bytes(), &signature)
            .with_context(|| {
                format!("Witness signature verification failed for {}", sig.witness)
            })?;

        // Validate timestamp is not in the future (with clock skew tolerance)
        if sig.signed_at > current_time + MAX_CLOCK_SKEW_SECS {
            anyhow::bail!(
                "Witness signature timestamp is in the future: {} (signed_at: {}, current: {})",
                sig.witness,
                sig.signed_at,
                current_time
            );
        }

        // Validate timestamp is not expired based on witness config
        if let Some(config) = &ledger.witness_config {
            let age = current_time.saturating_sub(sig.signed_at);
            if age > config.collection_timeout_secs {
                anyhow::bail!(
                    "Witness signature expired for {} (age: {}s, max: {}s)",
                    sig.witness,
                    age,
                    config.collection_timeout_secs
                );
            }
        }

        debug!(
            witness = %sig.witness,
            signed_at = sig.signed_at,
            "Witness signature verified"
        );
    }

    Ok(())
}

/// Store witness signatures for an entry (for fork resolution)
///
/// Persists witness signatures alongside entries so that fork resolution
/// can count co-signers when comparing conflicting entries.
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `entry_hash` - The content hash of the entry
/// * `witnesses` - The witness signatures to store
pub(crate) fn store_witness_signatures(
    ledger: &Ledger,
    entry_hash: &ContentHash,
    witnesses: &[crate::types::WitnessSignature],
) -> Result<()> {
    let key = format!("{}{}", WITNESS_PREFIX, entry_hash.to_hex());
    let value = serde_json::to_vec(witnesses)?;
    ledger.store.put(key.as_bytes(), &value)?;
    debug!(
        entry_hash = %entry_hash,
        witness_count = witnesses.len(),
        "Persisted witness signatures for fork resolution"
    );
    Ok(())
}

/// Load witness signatures for an entry (for fork resolution)
///
/// Returns the stored witness signatures, or an empty vec if none exist.
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `entry_hash` - The content hash of the entry
///
/// # Returns
/// Vector of witness signatures (empty if none stored)
pub(crate) fn load_witness_signatures(
    ledger: &Ledger,
    entry_hash: &ContentHash,
) -> Result<Vec<crate::types::WitnessSignature>> {
    let key = format!("{}{}", WITNESS_PREFIX, entry_hash.to_hex());
    match ledger.store.get(key.as_bytes())? {
        Some(data) => {
            let witnesses: Vec<crate::types::WitnessSignature> = serde_json::from_slice(&data)
                .context("Failed to deserialize witness signatures")?;
            Ok(witnesses)
        }
        None => Ok(Vec::new()),
    }
}

/// Count unique signers for an entry (author + witnesses)
///
/// Loads witness signatures from storage and counts unique DIDs.
/// Returns 1 (author only) if no witness signatures are stored.
/// Note: Author is always counted once; witnesses matching the author DID are filtered.
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `entry` - The journal entry to count signers for
///
/// # Returns
/// Number of unique signers (minimum 1)
pub(crate) fn count_entry_signers(ledger: &Ledger, entry: &JournalEntry) -> usize {
    let entry_hash = match &entry.id {
        Some(h) => h,
        None => return 1, // No hash, just count author
    };

    match load_witness_signatures(ledger, entry_hash) {
        Ok(witnesses) => {
            // Count unique witness DIDs excluding author (to prevent double-counting)
            // then add 1 for the author
            let author_did = &entry.author;
            let unique_witnesses: std::collections::HashSet<_> = witnesses
                .iter()
                .map(|w| &w.witness)
                .filter(|did| *did != author_did)
                .collect();
            unique_witnesses.len() + 1
        }
        Err(e) => {
            debug!(
                entry_hash = %entry_hash,
                error = %e,
                "Failed to load witness signatures, counting author only"
            );
            1
        }
    }
}

/// Calculate the total absolute value of an entry (for threshold comparison)
///
/// Uses only debits to determine transaction value. In a balanced double-entry
/// system, total debits equal total credits, so this correctly represents the
/// transaction size without double-counting.
///
/// # Arguments
/// * `entry` - The journal entry to calculate value for
///
/// # Returns
/// Total transaction value as u64
pub(crate) fn calculate_entry_value(_ledger: &Ledger, entry: &JournalEntry) -> u64 {
    entry
        .accounts
        .iter()
        .filter_map(|delta| delta.debit)
        .map(|d| d.unsigned_abs())
        .sum()
}

/// Check if an entry requires witness signatures based on current config
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `entry` - The journal entry to check
///
/// # Returns
/// true if witnesses are required, false otherwise
pub(crate) fn requires_witnesses(ledger: &Ledger, entry: &JournalEntry) -> bool {
    match &ledger.witness_config {
        None => false,
        Some(config) => {
            let value = calculate_entry_value(ledger, entry);
            !matches!(
                config.effective_policy(value),
                crate::types::WitnessPolicy::None
            )
        }
    }
}

/// Get the effective witness policy for an entry
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `entry` - The journal entry to get policy for
///
/// # Returns
/// The effective witness policy for this entry
pub(crate) fn effective_witness_policy(
    ledger: &Ledger,
    entry: &JournalEntry,
) -> crate::types::WitnessPolicy {
    match &ledger.witness_config {
        None => crate::types::WitnessPolicy::None,
        Some(config) => {
            let value = calculate_entry_value(ledger, entry);
            config.effective_policy(value).clone()
        }
    }
}
