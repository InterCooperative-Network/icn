//! Principal-identity guards for the ledger's principal-keyed persistence.
//!
//! # Why this module exists
//!
//! I7 (#2686) moved `Did` equality and hashing onto the 32 identifier bytes a
//! `did:icn:` spelling decodes to. Every persisted ledger key, however, is still
//! built from a *spelling* — `serde_json::to_string` for `ledger:balance:`,
//! `Display` for `ledger:cleared_volume:` and `ledger:frozen:`. The two regimes
//! disagree exactly when one principal has been stored under two spellings, and
//! `did:icn:` identifiers are multibase, so a principal has as many valid
//! spellings as `multibase` has bases.
//!
//! A rebuild that reads such rows into a `HashMap<Did, _>` collapses them. That
//! collapse is not a merge: `HashMap::insert` replaces the *value* while
//! retaining the *first-inserted key*, so the surviving value comes from the
//! byte-greatest row while the surviving key spelling comes from the byte-least
//! one. The subsequent write-back then stores the one row's value under the
//! other row's key and leaves the loser on disk holding stale state, which the
//! next start reads back — the divergence is permanent and silent.
//!
//! # Why it refuses instead of merging
//!
//! `icn-ledger/{balance,cleared_volume,frozen}` are three of the six keyspaces
//! carrying `RuleBasis::AwaitingDomainSignOff` in the N2-A scanner registry
//! (`icn_store::did_collision_scan::n2a_keyspaces`). Summing two balances or
//! unioning two freezes is *plausible* and is written down in
//! `docs/architecture/n2-a-migration-gate.md` §4, but it is not authorized: no
//! economics owner has approved it. Storage code has no standing to settle an
//! economic question by choosing a survivor, so this module detects and refuses.
//!
//! Detection is complete before any row reaches an in-memory map, so an
//! ambiguous keyspace is refused whole rather than half-applied.
//!
//! The refusal names the keyspace and a truncated principal fingerprint, never
//! a spelling or a stored value. `did-collision-scan` is the row-level evidence
//! tool; this module is the guard that stops a rebuild the tool would have
//! blocked, whether or not anyone ran it.

use icn_identity::identifier_bytes_of_spelling;
use std::collections::{BTreeMap, BTreeSet};

/// Registry name of the `ledger:balance:` keyspace, matching the descriptor in
/// `icn_store::did_collision_scan::n2a_keyspaces`.
pub const BALANCE_KEYSPACE: &str = "icn-ledger/balance";

/// Registry name of the `ledger:cleared_volume:` keyspace.
pub const CLEARED_VOLUME_KEYSPACE: &str = "icn-ledger/cleared_volume";

/// Registry name of the `ledger:frozen:` keyspace.
pub const FROZEN_KEYSPACE: &str = "icn-ledger/frozen";

/// How many identifier bytes a fingerprint shows.
///
/// Eight bytes correlate a refusal with a `did-collision-scan` report without
/// reproducing the identifier, so a refusal is safe to log where a DID is not.
const FINGERPRINT_BYTES: usize = 8;

/// A truncated, non-reversing name for one principal.
fn fingerprint(identifier: &[u8; 32]) -> String {
    hex::encode(&identifier[..FINGERPRINT_BYTES])
}

/// One set of persisted rows that name a single principal under several
/// spellings, and therefore have no single value the loader may adopt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasGroup {
    /// First [`FINGERPRINT_BYTES`] identifier bytes, hex-encoded.
    pub principal_fingerprint: String,
    /// The non-principal remainder of the key these rows share — the currency
    /// for `ledger:cleared_volume:`, empty where the principal is the whole key.
    pub discriminator: String,
    /// How many distinct spellings named that principal within the group.
    pub spellings: usize,
}

/// Why a principal-keyed keyspace could not be rebuilt.
///
/// Carried through `anyhow` so callers keep their existing signatures; recover
/// it with `anyhow::Error::downcast_ref::<PrincipalRowsRefusal>()`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrincipalRowsRefusal {
    /// Two or more spellings of one principal hold rows in the same keyspace.
    #[error(
        "{keyspace}: {} persisted row group(s) name one principal under several \
         did:icn: spellings, and this keyspace has no domain-authorized merge \
         rule (RuleBasis::AwaitingDomainSignOff); refusing to rebuild. \
         Groups: {}. Run `did-collision-scan` on this data directory for the \
         row-level report.",
        .groups.len(),
        describe_groups(.groups)
    )]
    AliasCollision {
        /// Registry name of the keyspace.
        keyspace: &'static str,
        /// One entry per colliding group.
        groups: Vec<AliasGroup>,
    },

    /// A stored key does not name a principal at all.
    ///
    /// Refused rather than skipped: were the unreadable row the only one for an
    /// account, skipping it would rebuild a balance that silently omits it. An
    /// unreadable row is evidence, not absence (§2.6).
    #[error(
        "{keyspace}: {rows} persisted row key(s) name no principal (not a \
         decodable did:icn: identifier); refusing to rebuild, because skipping \
         a row would turn unreadable state into absent state."
    )]
    UnreadableKey {
        /// Registry name of the keyspace.
        keyspace: &'static str,
        /// How many keys failed to decode.
        rows: usize,
    },

    /// A row's key spells a principal differently from the row's own contents.
    ///
    /// Nothing writes such a row deliberately. It is the fingerprint of a
    /// rebuild that already collapsed two spellings and wrote the survivor's
    /// value under the loser's key.
    #[error(
        "{keyspace}: {rows} persisted row(s) whose key spelling differs from the \
         spelling stored inside the row; refusing to rebuild, because a previous \
         write-back has already stored one row's value under another row's key."
    )]
    KeyValueSpellingMismatch {
        /// Registry name of the keyspace.
        keyspace: &'static str,
        /// How many rows disagree with their own key.
        rows: usize,
    },
}

/// Render colliding groups for the error text: fingerprint, discriminator and
/// spelling count only.
fn describe_groups(groups: &[AliasGroup]) -> String {
    groups
        .iter()
        .map(|g| {
            if g.discriminator.is_empty() {
                format!("{}×{}", g.principal_fingerprint, g.spellings)
            } else {
                format!(
                    "{}[{}]×{}",
                    g.principal_fingerprint, g.discriminator, g.spellings
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Decide whether a keyspace's stored keys name each principal exactly once.
///
/// `rows` yields one `(principal_spelling, discriminator)` pair per stored row.
/// The caller does the splitting because only it knows its key shape; the
/// discriminator keeps rows apart that merely share a principal — two
/// currencies of one account are two accounts' worth of state, not a collision.
///
/// Returns the first refusal that applies, having classified **every** row: no
/// caller may have mutated anything by the time this returns.
pub fn refuse_unless_one_spelling_per_principal<'a>(
    keyspace: &'static str,
    rows: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), PrincipalRowsRefusal> {
    let mut by_principal: BTreeMap<([u8; 32], &str), BTreeSet<&str>> = BTreeMap::new();
    let mut unreadable = 0usize;

    for (spelling, discriminator) in rows {
        match identifier_bytes_of_spelling(spelling) {
            Ok(identifier) => {
                by_principal
                    .entry((identifier, discriminator))
                    .or_default()
                    .insert(spelling);
            }
            Err(_) => unreadable += 1,
        }
    }

    // Unreadability is reported first: it means the classification below was
    // computed over an incomplete view, so its silence proves nothing.
    if unreadable > 0 {
        return Err(PrincipalRowsRefusal::UnreadableKey {
            keyspace,
            rows: unreadable,
        });
    }

    let groups: Vec<AliasGroup> = by_principal
        .into_iter()
        .filter(|(_, spellings)| spellings.len() > 1)
        .map(|((identifier, discriminator), spellings)| AliasGroup {
            principal_fingerprint: fingerprint(&identifier),
            discriminator: discriminator.to_string(),
            spellings: spellings.len(),
        })
        .collect();

    if !groups.is_empty() {
        return Err(PrincipalRowsRefusal::AliasCollision { keyspace, groups });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::{Did, KeyPair};

    fn a_principal() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    /// A second, equally valid textual encoding of the principal `did` names.
    ///
    /// `did:icn:` identifiers are multibase, so the same 32 bytes have a
    /// base58btc spelling and a base16 spelling. Both parse; both decode to one
    /// identifier.
    fn alternate_spelling(did: &Did) -> Did {
        let bytes = did.identifier_bytes().unwrap();
        let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes))).unwrap();
        assert_ne!(
            did.as_str(),
            alias.as_str(),
            "the two spellings must differ, or the test proves nothing"
        );
        alias
    }

    #[test]
    fn one_spelling_per_principal_is_accepted() {
        let a = a_principal();
        let b = a_principal();
        let rows = vec![(a.as_str(), ""), (b.as_str(), "")];
        assert!(refuse_unless_one_spelling_per_principal("test", rows).is_ok());
    }

    #[test]
    fn two_spellings_of_one_principal_are_refused() {
        let a = a_principal();
        let alias = alternate_spelling(&a);
        let rows = vec![(a.as_str(), ""), (alias.as_str(), "")];

        let refusal = refuse_unless_one_spelling_per_principal("test", rows)
            .expect_err("two spellings of one principal must refuse");

        match refusal {
            PrincipalRowsRefusal::AliasCollision { groups, .. } => {
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].spellings, 2);
            }
            other => panic!("expected AliasCollision, got {other:?}"),
        }
    }

    #[test]
    fn two_genuinely_different_principals_are_not_a_collision() {
        // Distinct keypairs, so the guard must stay silent however many rows
        // share the keyspace. This is the case that breaks if the guard ever
        // groups by something coarser than the decoded identifier.
        let principals: Vec<Did> = (0..4).map(|_| a_principal()).collect();
        let rows: Vec<(&str, &str)> = principals.iter().map(|d| (d.as_str(), "")).collect();
        assert!(refuse_unless_one_spelling_per_principal("test", rows).is_ok());
    }

    #[test]
    fn a_discriminator_keeps_two_currencies_of_one_account_apart() {
        let a = a_principal();
        let rows = vec![(a.as_str(), "USD"), (a.as_str(), "EUR")];
        assert!(
            refuse_unless_one_spelling_per_principal("test", rows).is_ok(),
            "one account's two currencies are two rows of state, not a collision"
        );
    }

    #[test]
    fn two_spellings_under_one_discriminator_are_refused() {
        let a = a_principal();
        let alias = alternate_spelling(&a);
        let rows = vec![(a.as_str(), "USD"), (alias.as_str(), "USD")];

        let refusal = refuse_unless_one_spelling_per_principal("test", rows)
            .expect_err("one account, one currency, two spellings must refuse");

        match refusal {
            PrincipalRowsRefusal::AliasCollision { groups, .. } => {
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].discriminator, "USD");
            }
            other => panic!("expected AliasCollision, got {other:?}"),
        }
    }

    #[test]
    fn a_key_naming_no_principal_is_refused_not_skipped() {
        let a = a_principal();
        let rows = vec![(a.as_str(), ""), ("did:icn:not-a-multibase-identifier", "")];

        let refusal = refuse_unless_one_spelling_per_principal("test", rows)
            .expect_err("an undecodable key must refuse");

        assert!(matches!(
            refusal,
            PrincipalRowsRefusal::UnreadableKey { rows: 1, .. }
        ));
    }

    #[test]
    fn unreadability_outranks_collision_because_it_bounds_the_evidence() {
        let a = a_principal();
        let alias = alternate_spelling(&a);
        let rows = vec![
            (a.as_str(), ""),
            (alias.as_str(), ""),
            ("did:icn:garbage!", ""),
        ];

        let refusal =
            refuse_unless_one_spelling_per_principal("test", rows).expect_err("must refuse");

        assert!(
            matches!(refusal, PrincipalRowsRefusal::UnreadableKey { .. }),
            "an incomplete view must be reported as incomplete, not as a collision count"
        );
    }

    #[test]
    fn the_refusal_text_carries_no_spelling_and_no_value() {
        let a = a_principal();
        let alias = alternate_spelling(&a);
        let rows = vec![(a.as_str(), ""), (alias.as_str(), "")];

        let refusal = refuse_unless_one_spelling_per_principal(BALANCE_KEYSPACE, rows)
            .expect_err("must refuse");
        let text = refusal.to_string();

        assert!(!text.contains(a.as_str()), "refusal leaked a spelling");
        assert!(!text.contains(alias.as_str()), "refusal leaked a spelling");
        assert!(text.contains(BALANCE_KEYSPACE));
    }
}
