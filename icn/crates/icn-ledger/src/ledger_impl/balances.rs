//! Balance operations for the ledger
//!
//! This module contains balance management operations including balance queries,
//! balance recomputation, and cleared volume tracking. These functions are extracted
//! from the main Ledger implementation for better code organization.
//!
//! # Testing
//!
//! Tests for these functions are located in the main `ledger.rs` test module
//! (`crates/icn-ledger/src/ledger.rs`). The tests use the public `Ledger` API
//! which delegates to these functions, ensuring the public interface remains
//! stable while allowing internal reorganization.
//!
//! Key test functions:
//! - `test_balance_tracking` - tests balance computation
//! - `test_recompute_balances` - tests balance recomputation
//! - Integration tests in `crates/icn-ledger/tests/` also exercise these functions

use crate::balance::compute_all_balances;
use crate::ledger::{Ledger, BALANCE_PREFIX, CLEARED_VOLUME_PREFIX};
use crate::principal_rows::{
    refuse_unless_one_spelling_per_principal, PrincipalRowsRefusal, BALANCE_KEYSPACE,
    CLEARED_VOLUME_KEYSPACE,
};
use crate::types::AccountBalances;
use anyhow::Result;
use icn_identity::Did;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Get balance for a specific account and currency
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `account_id` - The account DID
/// * `currency` - The currency code
///
/// # Returns
/// The balance as an i64 (positive = credit, negative = debit)
pub(crate) fn get_balance(ledger: &Ledger, account_id: &Did, currency: &str) -> i64 {
    ledger
        .cached_balances
        .get(account_id)
        .map(|b| b.get(currency))
        .unwrap_or(0)
}

/// Get all balances for an account
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `account_id` - The account DID
///
/// # Returns
/// AccountBalances for the account (empty if account has no balances)
pub(crate) fn get_account_balances(ledger: &Ledger, account_id: &Did) -> AccountBalances {
    ledger
        .cached_balances
        .get(account_id)
        .cloned()
        .unwrap_or_else(|| AccountBalances::new(account_id.clone()))
}

/// Get all balances across all accounts
///
/// # Arguments
/// * `ledger` - The ledger instance
///
/// # Returns
/// HashMap of account DID to AccountBalances
pub(crate) fn get_all_balances(ledger: &Ledger) -> HashMap<Did, AccountBalances> {
    ledger.cached_balances.clone()
}

/// Get total cleared volume for an account in a currency (O(1) lookup)
///
/// This returns the total credits received by the account in the specified currency,
/// which represents their total historical contributions to the system.
/// Used for calculating credit limit bonuses based on transaction history.
///
/// Example: If Alice has received 500 hours of credits over time,
/// this returns 500 (even if her current balance is lower due to debits).
///
/// Performance: O(1) - uses the pre-computed cleared volume index
///
/// # Arguments
/// * `ledger` - The ledger instance
/// * `account_id` - The account DID
/// * `currency` - The currency code
///
/// # Returns
/// The total cleared volume as an i64
pub(crate) fn total_cleared_by(ledger: &Ledger, account_id: &Did, currency: &str) -> Result<i64> {
    let key = (account_id.clone(), currency.to_string());
    Ok(*ledger.cleared_volume_index.get(&key).unwrap_or(&0))
}

/// Recompute all balances and cleared volumes from journal entries (for verification)
///
/// This method uses snapshot isolation to prevent race conditions (M7 fix):
/// 1. Capture the journal version at snapshot time
/// 2. Compute new balances from the snapshot
/// 3. Validate the version hasn't changed before applying
/// 4. If version changed, return error (caller should retry)
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
///
/// # Returns
/// Ok(()) if successful, Err if journal was modified during recomputation
pub(crate) fn recompute_balances(ledger: &mut Ledger) -> Result<()> {
    // M7 Fix: Capture journal version at snapshot time for isolation
    let snapshot_version = ledger.journal_version;

    info!(
        cached_account_count = ledger.cached_balances.len(),
        cleared_volume_count = ledger.cleared_volume_index.len(),
        snapshot_version,
        "Recomputing all balances and cleared volumes from journal"
    );

    // Take snapshot of entries
    let entries = ledger.get_all_entries()?;
    let balances = compute_all_balances(&entries)?;

    // Also recompute cleared volume index
    let mut cleared_volumes: HashMap<(Did, String), i64> = HashMap::new();
    for entry in &entries {
        for delta in &entry.accounts {
            if let Some(credit) = delta.credit {
                let key = (delta.account_id.clone(), delta.currency.clone());
                *cleared_volumes.entry(key).or_insert(0) += credit;
            }
        }
    }

    // M7 Fix: Validate journal version hasn't changed during recomputation
    // This prevents the race condition where concurrent entry appends are lost
    if ledger.journal_version != snapshot_version {
        warn!(
            snapshot_version,
            current_version = ledger.journal_version,
            "Journal modified during balance recomputation - aborting to prevent data loss"
        );
        icn_obs::metrics::ledger::recompute_aborted_version_mismatch_inc();
        anyhow::bail!(
            "Journal modified during balance recomputation (version {} -> {}). \
             Retry the operation to ensure data consistency.",
            snapshot_version,
            ledger.journal_version
        );
    }

    // Safe to apply - journal hasn't changed during our computation.
    //
    // The recomputed maps are keyed by the spelling the *journal entries*
    // carry, which need not be the spelling the persisted row uses. Writing
    // them straight back would put the same principal's state under a second
    // key and strand the first row, manufacturing the collision the loader
    // refuses. Adopt the stored spelling wherever the store already has one.
    ledger.cached_balances = adopt_stored_balance_spellings(&ledger.cached_balances, balances);
    ledger.cleared_volume_index =
        adopt_stored_volume_spellings(&ledger.cleared_volume_index, cleared_volumes);
    save_cached_balances(ledger)?;
    save_cleared_volume_index(ledger)?;

    info!(
        entry_count = entries.len(),
        account_count = ledger.cached_balances.len(),
        cleared_volume_count = ledger.cleared_volume_index.len(),
        snapshot_version,
        "Balance and cleared volume recomputation complete"
    );
    Ok(())
}

/// Recompute balances with automatic retry on version mismatch
///
/// This is a convenience wrapper around `recompute_balances` that handles
/// the race condition by retrying up to `max_retries` times.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
/// * `max_retries` - Maximum number of retry attempts
///
/// # Returns
/// Ok(()) if successful, Err if all retries exhausted
pub(crate) fn recompute_balances_with_retry(ledger: &mut Ledger, max_retries: usize) -> Result<()> {
    for attempt in 0..=max_retries {
        match recompute_balances(ledger) {
            Ok(()) => return Ok(()),
            Err(e) if e.to_string().contains("Journal modified") => {
                if attempt < max_retries {
                    warn!(
                        attempt = attempt + 1,
                        max_retries, "Balance recomputation retry due to concurrent modification"
                    );
                    // Small delay to reduce contention
                    std::thread::sleep(std::time::Duration::from_millis(10));
                } else {
                    // All retries exhausted with retryable error
                    return Err(anyhow::anyhow!(
                        "Balance recomputation failed after {max_retries} retries due to concurrent modifications"
                    ));
                }
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("All code paths should return within the loop")
}

/// Load cached balances from storage
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
///
/// # Returns
/// Ok(()) if successful
pub(crate) fn load_cached_balances(ledger: &mut Ledger) -> Result<()> {
    let prefix = BALANCE_PREFIX.as_bytes();
    let pairs = ledger.store.scan(prefix)?;

    // Read every row, and classify the whole keyspace, before one of them
    // reaches `cached_balances`. `HashMap::insert` would otherwise collapse two
    // spellings of one principal into a single entry whose key came from one
    // row and whose value came from another — see `crate::principal_rows`.
    let mut rows = Vec::with_capacity(pairs.len());
    for (key, value) in &pairs {
        let balances: AccountBalances = serde_json::from_slice(value)?;
        rows.push((balance_key_spelling(key), balances));
    }

    refuse_unless_one_spelling_per_principal(
        BALANCE_KEYSPACE,
        rows.iter().map(|(spelling, _)| (spelling.as_str(), "")),
    )?;

    // A row whose key names the account differently from the row's own
    // `account_id` is the residue of a rebuild that already collapsed two
    // spellings and wrote the survivor's balances under the loser's key.
    // Loading it would adopt one row's money under another row's name.
    let strayed = rows
        .iter()
        .filter(|(spelling, balances)| balances.account_id.as_str() != spelling)
        .count();
    if strayed > 0 {
        return Err(PrincipalRowsRefusal::KeyValueSpellingMismatch {
            keyspace: BALANCE_KEYSPACE,
            rows: strayed,
        }
        .into());
    }

    for (_spelling, balances) in rows {
        ledger
            .cached_balances
            .insert(balances.account_id.clone(), balances);
    }

    debug!("Loaded {} cached balances", ledger.cached_balances.len());
    Ok(())
}

/// Re-key a recomputed balance map onto the spellings the store already holds.
///
/// `HashMap::get_key_value` finds the entry by principal (I7 equality) and
/// hands back the *stored* key, so the write-back that follows addresses the
/// row that already exists instead of opening a second one. The row's own
/// `account_id` is moved with it, because a row whose key and contents spell
/// the account differently is exactly what `load_cached_balances` refuses.
fn adopt_stored_balance_spellings(
    stored: &HashMap<Did, AccountBalances>,
    computed: HashMap<Did, AccountBalances>,
) -> HashMap<Did, AccountBalances> {
    computed
        .into_iter()
        .map(|(did, mut balances)| match stored.get_key_value(&did) {
            Some((stored_did, _)) => {
                balances.account_id = stored_did.clone();
                (stored_did.clone(), balances)
            }
            None => (did, balances),
        })
        .collect()
}

/// The cleared-volume counterpart of [`adopt_stored_balance_spellings`].
///
/// The currency travels with the principal because it is part of the row
/// identity, not part of the account.
fn adopt_stored_volume_spellings(
    stored: &HashMap<(Did, String), i64>,
    computed: HashMap<(Did, String), i64>,
) -> HashMap<(Did, String), i64> {
    computed
        .into_iter()
        .map(|(key, volume)| match stored.get_key_value(&key) {
            Some((stored_key, _)) => (stored_key.clone(), volume),
            None => (key, volume),
        })
        .collect()
}

/// The DID spelling inside a `ledger:balance:` key.
///
/// `save_cached_balances` builds the key with `serde_json::to_string`, so the
/// remainder is a JSON string. Anything that is not — a truncated or foreign
/// key — is returned as-is so the guard classifies it as unreadable rather than
/// this function silently deciding a row does not exist.
fn balance_key_spelling(key: &[u8]) -> String {
    let key = String::from_utf8_lossy(key);
    let remainder = key.strip_prefix(BALANCE_PREFIX).unwrap_or(&key);
    serde_json::from_str::<String>(remainder).unwrap_or_else(|_| remainder.to_string())
}

/// Save cached balances to storage
///
/// # Arguments
/// * `ledger` - The ledger instance
///
/// # Returns
/// Ok(()) if successful
pub(crate) fn save_cached_balances(ledger: &Ledger) -> Result<()> {
    for (account_id, balances) in &ledger.cached_balances {
        let key = format!("{}{}", BALANCE_PREFIX, serde_json::to_string(account_id)?);
        let value = serde_json::to_vec(balances)?;
        ledger.store.put(key.as_bytes(), &value)?;
    }

    Ok(())
}

/// Load cleared volume index from storage
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
///
/// # Returns
/// Ok(()) if successful
pub(crate) fn load_cleared_volume_index(ledger: &mut Ledger) -> Result<()> {
    let prefix = CLEARED_VOLUME_PREFIX.as_bytes();
    let pairs = ledger.store.scan(prefix)?;

    // Split every key before adopting any row, so the keyspace is classified
    // whole. A row that cannot be split is kept with an empty currency and an
    // unusable spelling, which the guard reports as unreadable — the previous
    // code dropped such rows silently, turning a corrupt key into an account
    // with no cleared volume.
    let mut rows = Vec::with_capacity(pairs.len());
    for (key, value) in &pairs {
        // Key format: "ledger:cleared_volume:{did}:{currency}". The DID itself
        // contains colons, so the *last* colon is the currency separator.
        let key_str = String::from_utf8_lossy(key);
        let rest = key_str
            .strip_prefix(CLEARED_VOLUME_PREFIX)
            .unwrap_or(&key_str);
        let (did_str, currency) = match rest.rfind(':') {
            Some(last_colon) => (&rest[..last_colon], &rest[last_colon + 1..]),
            None => (rest, ""),
        };
        rows.push((did_str.to_string(), currency.to_string(), value));
    }

    refuse_unless_one_spelling_per_principal(
        CLEARED_VOLUME_KEYSPACE,
        rows.iter()
            .map(|(did_str, currency, _)| (did_str.as_str(), currency.as_str())),
    )?;

    for (did_str, currency, value) in rows {
        let did: Did = serde_json::from_str(&format!("\"{did_str}\""))?;
        let volume: i64 = serde_json::from_slice(value)?;
        ledger.cleared_volume_index.insert((did, currency), volume);
    }

    debug!(
        "Loaded {} cleared volume entries",
        ledger.cleared_volume_index.len()
    );
    Ok(())
}

/// Save cleared volume index to storage
///
/// # Arguments
/// * `ledger` - The ledger instance
///
/// # Returns
/// Ok(()) if successful
pub(crate) fn save_cleared_volume_index(ledger: &Ledger) -> Result<()> {
    for ((account_id, currency), volume) in &ledger.cleared_volume_index {
        // Store with composite key: "{prefix}{did}:{currency}"
        let key = format!("{CLEARED_VOLUME_PREFIX}{account_id}:{currency}");
        let value = serde_json::to_vec(volume)?;
        ledger.store.put(key.as_bytes(), &value)?;
    }

    Ok(())
}
