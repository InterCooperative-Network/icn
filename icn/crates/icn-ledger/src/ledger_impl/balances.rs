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
    let mut unsplittable = 0usize;
    for (key, value) in &pairs {
        let balances: AccountBalances = serde_json::from_slice(value)?;
        match balance_key_spelling(key) {
            Some(spelling) => rows.push((spelling, balances)),
            None => unsplittable += 1,
        }
    }

    // A key that is not the shape `save_cached_balances` writes is evidence
    // of corruption or tampering, and a classification computed without it
    // proves nothing; refuse before the guard runs.
    if unsplittable > 0 {
        return Err(PrincipalRowsRefusal::UnreadableKey {
            keyspace: BALANCE_KEYSPACE,
            rows: unsplittable,
        }
        .into());
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

/// The DID spelling inside a `ledger:balance:` key, or `None` for a key the
/// writer could not have produced.
///
/// `save_cached_balances` builds the key with `serde_json::to_string`, so the
/// remainder is exactly one JSON string. A remainder that is not — a bare
/// spelling without the quotes, a truncated or foreign key — is not this
/// keyspace's shape. It used to be handed back as-is so the guard would call
/// it unreadable, but a bare spelling *decodes*, so the guard would have
/// adopted a row nothing in this crate wrote; the caller now counts it and
/// refuses instead.
fn balance_key_spelling(key: &[u8]) -> Option<String> {
    let remainder = key.strip_prefix(BALANCE_PREFIX.as_bytes())?;
    let remainder = std::str::from_utf8(remainder).ok()?;
    serde_json::from_str::<String>(remainder).ok()
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

/// The multibase Identity sigil: the identifier body that follows it is the
/// raw 32 identifier bytes, unencoded.
const MULTIBASE_IDENTITY_SIGIL: u8 = 0x00;

/// Split a `ledger:cleared_volume:<did>:<currency>` key into its spelling and
/// currency, exactly, or `None` for a key the writer could not have produced.
///
/// `save_cleared_volume_index` writes `{did}:{currency}` with `Display`, and a
/// currency is not validated against a charset anywhere in this crate, so the
/// currency may hold anything — including `:`. The boundary is therefore found
/// from the DID side. A `did:icn:` spelling is the scheme followed by one
/// multibase sigil and a body, and every accepted base has a colon-free
/// alphabet **except** the Identity base, whose body is the raw 32 identifier
/// bytes and may contain any byte. So: after the scheme, an Identity sigil
/// means the spelling is exactly `8 + 1 + 32` bytes long; any other sigil means
/// the spelling ends at the first `:`. In both cases the byte at the boundary
/// must be the `:` the writer put there — a key that ends at the spelling, or
/// whose spelling or currency is not UTF-8, is not this keyspace's shape and is
/// handed back as unreadable rather than adopted under an invented currency.
/// The parse is byte-identical to the old first-colon split for every
/// non-Identity spelling, so existing rows re-read unchanged.
pub(crate) fn split_cleared_volume_key(key: &[u8]) -> Option<(&str, &str)> {
    const DID_SCHEME: &[u8] = b"did:icn:";
    const IDENTIFIER_BYTES: usize = 32;

    let rest = key.strip_prefix(CLEARED_VOLUME_PREFIX.as_bytes())?;
    let body = rest.strip_prefix(DID_SCHEME)?;
    let body_len = match body.first()? {
        &MULTIBASE_IDENTITY_SIGIL => 1 + IDENTIFIER_BYTES,
        _ => body.iter().position(|&b| b == b':')?,
    };
    let split = DID_SCHEME.len() + body_len;
    if rest.get(split) != Some(&b':') {
        return None;
    }
    let spelling = std::str::from_utf8(&rest[..split]).ok()?;
    let currency = std::str::from_utf8(&rest[split + 1..]).ok()?;
    Some((spelling, currency))
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
    // whole. A key that cannot be split into the shape the writer produces is
    // counted and refused: the previous code adopted such a row under an
    // empty currency, turning a corrupt key into an account with a phantom
    // currency, and before that dropped it silently.
    let mut rows = Vec::with_capacity(pairs.len());
    let mut unsplittable = 0usize;
    for (key, value) in &pairs {
        match split_cleared_volume_key(key) {
            Some((did_str, currency)) => {
                rows.push((did_str.to_string(), currency.to_string(), value))
            }
            None => unsplittable += 1,
        }
    }

    // Reported before the guard runs: a keyspace holding a row the writer
    // could not have produced is evidence of corruption or tampering, and a
    // classification computed without that row proves nothing.
    if unsplittable > 0 {
        return Err(PrincipalRowsRefusal::UnreadableKey {
            keyspace: CLEARED_VOLUME_KEYSPACE,
            rows: unsplittable,
        }
        .into());
    }

    refuse_unless_one_spelling_per_principal(
        CLEARED_VOLUME_KEYSPACE,
        rows.iter()
            .map(|(did_str, currency, _)| (did_str.as_str(), currency.as_str())),
    )?;

    for (did_str, currency, value) in rows {
        // Parsed directly, not through a JSON round trip: an Identity-base
        // spelling carries a raw NUL sigil, which JSON forbids inside a string
        // even though `Did::from_str` accepts it.
        let did = Did::from_str(did_str.as_str())?;
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

#[cfg(test)]
mod key_shape_tests {
    use super::*;

    fn key(rest: &str) -> Vec<u8> {
        format!("{CLEARED_VOLUME_PREFIX}{rest}").into_bytes()
    }

    #[test]
    fn an_ordinary_spelling_splits_at_the_currency_delimiter() {
        let k = key("did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK:USD");
        let (spelling, currency) = split_cleared_volume_key(&k).unwrap();
        assert_eq!(
            spelling,
            "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
        );
        assert_eq!(currency, "USD");
    }

    #[test]
    fn a_colon_bearing_currency_keeps_its_colons() {
        let k = key("did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK:USD:SPOT:T+2");
        let (_, currency) = split_cleared_volume_key(&k).unwrap();
        assert_eq!(currency, "USD:SPOT:T+2");
    }

    #[test]
    fn an_identity_base_body_containing_colons_is_split_after_its_32_bytes() {
        // Identity base: the sigil is NUL and the body is the raw identifier.
        let body = "ab:cd:ef:gh:ij:kl:mn:op:qr:st:uv";
        assert_eq!(body.len(), 32);
        let k = key(&format!("did:icn:\u{0}{body}:EUR:SPOT"));
        let (spelling, currency) = split_cleared_volume_key(&k).unwrap();
        assert_eq!(spelling, format!("did:icn:\u{0}{body}"));
        assert_eq!(currency, "EUR:SPOT");
    }

    #[test]
    fn a_key_that_ends_at_the_spelling_is_not_this_keyspaces_shape() {
        let k = key("did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
        assert!(split_cleared_volume_key(&k).is_none());
        let identity = key(&format!(
            "did:icn:\u{0}{}",
            "ab:cd:ef:gh:ij:kl:mn:op:qr:st:uv"
        ));
        assert!(split_cleared_volume_key(&identity).is_none());
    }

    #[test]
    fn an_identity_body_not_followed_by_the_delimiter_is_not_split() {
        let k = key(&format!(
            "did:icn:\u{0}{}",
            "ab:cd:ef:gh:ij:kl:mn:op:qr:st:uvXUSD"
        ));
        assert!(split_cleared_volume_key(&k).is_none());
    }

    #[test]
    fn a_key_without_the_scheme_or_with_invalid_utf8_is_not_split() {
        assert!(split_cleared_volume_key(&key("garbage:USD")).is_none());
        let mut bad_currency = key("did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK:");
        bad_currency.extend([0xff, 0xfe]);
        assert!(split_cleared_volume_key(&bad_currency).is_none());
    }
}
