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
//! `icn-ledger/{balance,cleared_volume,frozen}` are three of the seven keyspaces
//! carrying `RuleBasis::AwaitingDomainSignOff` in the N2-A scanner registry
//! (`icn_store::did_collision_scan::n2a_keyspaces`). Summing two balances or
//! unioning two freezes is *plausible* and is written down in
//! `docs/architecture/n2-a-migration-gate.md` §4, but it is not authorized: no
//! economics owner has approved it. Storage code has no standing to settle an
//! economic question by choosing a survivor, so this module detects and refuses.
//!
//! The treasury loader (`crate::treasury`, #2627 M1) is the fourth caller. Its
//! keyspace is registered `FailClosed` with `RuleBasis::Established`: the
//! refusal there is the established rule, not a wait for one, which is why the
//! alias refusal names the keyspace and not a basis.
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

/// Registry name of the primary `ledger:treasury:<did>` keyspace (#2627 M1).
///
/// The treasury loader (`crate::treasury`) classifies its primary rows through
/// [`refuse_unless_one_spelling_per_principal`] exactly as the three rebuilds
/// above do; the scanner descriptor of the same name claims only the primary
/// rows, never the budget, rule, audit, index or velocity-limit subspaces that
/// share the lexical parent `ledger:treasury:`.
pub const TREASURY_KEYSPACE: &str = "icn-ledger/treasury";

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
    ///
    /// The text names the keyspace and leaves its rule basis to the registry:
    /// the three `Ledger::new` rebuilds refuse under
    /// `RuleBasis::AwaitingDomainSignOff`, while the treasury loader (#2627 M1)
    /// refuses under `FailClosed`/`Established` — there the refusal *is* the
    /// established rule. Asserting one basis here would misstate the other.
    #[error(
        "{keyspace}: {} persisted row group(s) name one principal under several \
         did:icn: spellings, and no domain-authorized merge rule exists for this \
         keyspace (see its N2-A registry entry); refusing to rebuild. \
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

    /// A stored key cannot be read as one of this keyspace's rows: it names
    /// no principal, or it is not the key shape the keyspace's writer produces
    /// (a `ledger:cleared_volume:` key with no currency delimiter, say).
    ///
    /// Refused rather than skipped: were the unreadable row the only one for an
    /// account, skipping it would rebuild a balance that silently omits it, and
    /// adopting it under an invented shape would rebuild state the writer never
    /// wrote. An unreadable row is evidence, not absence (§2.6).
    #[error(
        "{keyspace}: {rows} persisted row key(s) cannot be read as this \
         keyspace's principal-keyed rows (no decodable did:icn: identifier, or \
         not the key shape the writer produces); refusing to rebuild, because \
         skipping a row would turn unreadable state into absent state."
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

/// Longest discriminator shown in a refusal, in characters after escaping.
///
/// A currency is not validated against a charset or a length anywhere in this
/// crate, so the discriminator is persisted, externally supplied text. It is
/// shown because it is what tells two groups of one principal apart, and it
/// is escaped and bounded because a refusal is the operator's evidence for a
/// deliberate failed start and must survive being logged whatever the row
/// held.
const DISCRIMINATOR_ERROR_CAP: usize = 32;

/// Most colliding groups listed in one refusal. The true count is always
/// stated; only the listing is cut, and the cut is stated too.
const GROUPS_ERROR_CAP: usize = 16;

/// Escape untrusted text for a one-line diagnostic and bound what is shown.
///
/// Every character goes through `char::escape_default`, so control characters
/// (newline, carriage return, tab, ANSI escape), quotes and non-ASCII become
/// escape sequences and the result cannot end the line or repaint a terminal.
/// The cap counts shown characters and is applied per source character, so a
/// cut never lands inside an escape sequence; a cut is marked with `…`.
fn escaped_bounded(text: &str, cap: usize) -> String {
    let mut out = String::new();
    let mut shown = 0usize;
    for c in text.chars() {
        let escaped: String = c.escape_default().collect();
        let width = escaped.chars().count();
        if shown + width > cap {
            out.push('…');
            return out;
        }
        shown += width;
        out.push_str(&escaped);
    }
    out
}

/// Render colliding groups for the error text: fingerprint, escaped and
/// bounded discriminator, and spelling count only. At most
/// [`GROUPS_ERROR_CAP`] groups are listed; the remainder is counted.
fn describe_groups(groups: &[AliasGroup]) -> String {
    let mut listed: Vec<String> = groups
        .iter()
        .take(GROUPS_ERROR_CAP)
        .map(|g| {
            if g.discriminator.is_empty() {
                format!("{}×{}", g.principal_fingerprint, g.spellings)
            } else {
                format!(
                    "{}[{}]×{}",
                    g.principal_fingerprint,
                    escaped_bounded(&g.discriminator, DISCRIMINATOR_ERROR_CAP),
                    g.spellings
                )
            }
        })
        .collect();
    if groups.len() > GROUPS_ERROR_CAP {
        listed.push(format!("and {} more", groups.len() - GROUPS_ERROR_CAP));
    }
    listed.join(", ")
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

    #[test]
    fn a_hostile_currency_cannot_forge_or_split_the_refusal_line() {
        // `ledger:cleared_volume:` carries the currency as the discriminator,
        // and nothing validates a currency against a charset, so an alias
        // collision can arrive wearing whatever text was persisted: a newline
        // that ends the log line, a forged entry after it, an ANSI escape that
        // repaints the terminal, and enough length to swamp the message.
        let a = a_principal();
        let alias = alternate_spelling(&a);
        let mut hostile = String::from("USD\n[ERROR] ledger clean, proceeding\r\x1b[31m\t");
        hostile.extend(std::iter::repeat_n('X', 600));
        let rows = vec![
            (a.as_str(), hostile.as_str()),
            (alias.as_str(), hostile.as_str()),
        ];

        let refusal = refuse_unless_one_spelling_per_principal(CLEARED_VOLUME_KEYSPACE, rows)
            .expect_err("the collision must still refuse");
        assert!(matches!(
            refusal,
            PrincipalRowsRefusal::AliasCollision { .. }
        ));
        let text = refusal.to_string();

        assert!(text.contains(CLEARED_VOLUME_KEYSPACE), "names the keyspace");
        assert!(
            text.contains("1 persisted row group"),
            "names the collision fact"
        );
        assert!(
            !text.chars().any(|c| c.is_control()),
            "a control character reached the diagnostic: {text:?}"
        );
        assert_eq!(text.lines().count(), 1, "one logical log line: {text:?}");
        assert!(
            !text
                .lines()
                .any(|line| line.trim_start().starts_with("[ERROR]")),
            "the forged entry must not begin a line: {text:?}"
        );
        assert!(
            text.contains("USD\\n[ERROR]"),
            "the forged entry stays glued to the escaped newline: {text}"
        );
        assert!(
            text.contains("USD\\n"),
            "escaped rather than dropped: {text}"
        );
        assert!(!text.contains(a.as_str()) && !text.contains(alias.as_str()));
        assert!(
            text.chars().count() < 400,
            "bounded: {} chars for a 600-char discriminator",
            text.chars().count()
        );
    }

    #[test]
    fn many_colliding_groups_still_render_as_one_bounded_line() {
        // A store can hold an alias pair for every account it has; the
        // refusal must stay a bounded line and still state the true count.
        let rows: Vec<(Did, Did)> = (0..40)
            .map(|_| {
                let a = a_principal();
                let alias = alternate_spelling(&a);
                (a, alias)
            })
            .collect();
        let flat: Vec<(&str, &str)> = rows
            .iter()
            .flat_map(|(a, alias)| [(a.as_str(), "USD"), (alias.as_str(), "USD")])
            .collect();

        let text = refuse_unless_one_spelling_per_principal(CLEARED_VOLUME_KEYSPACE, flat)
            .expect_err("must refuse")
            .to_string();
        assert!(text.contains("40 persisted row group"), "{text}");
        assert_eq!(text.lines().count(), 1);
        assert!(
            text.chars().count() < 1200,
            "bounded: {} chars",
            text.chars().count()
        );
        assert!(
            text.contains("more"),
            "the cut is stated, not silent: {text}"
        );
    }
}
