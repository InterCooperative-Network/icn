//! The owner of a [`JournalEntry`]'s **entry-intrinsic** validity.
//!
//! # Why this is its own module
//!
//! [`Ledger::validate_entry`](crate::Ledger) answers two different questions in
//! one method:
//!
//! 1. **Is this entry well formed on its own terms?** It carries at least one
//!    account delta, and within the entry each currency's debits and credits
//!    cancel under checked `i64`. This is a function of the entry *value* and
//!    nothing else.
//! 2. **May this entry be appended to *this* ledger, *now*?** The author and
//!    accounts are not frozen; no account breaches a credit limit or a
//!    progressive POPLevel limit. These read mutable ledger state — balances,
//!    trust scores, cleared volume — and `SystemTime::now()`.
//!
//! Only the first question has an answer for an entry sitting in a backup, and
//! only the first question is owned here.
//!
//! # The drift this exists to stop
//!
//! `icnctl verify-backup --verify-ledger` re-implemented question 1 rather than
//! calling it, and the copy diverged twice before anyone noticed (icn#2717):
//! it summed balances across the whole journal where the ledger sums per entry,
//! so a `+60` row and a `-60` row cancelled and both were certified; and it
//! accumulated in a widened `i128` where the ledger uses checked `i64`, so a row
//! the ledger rejects as an overflow reached zero and was certified. Both wore
//! the ledger's own words while asking a weaker question.
//!
//! Those two were patched. A third was already latent: an entry with an empty
//! `accounts` array satisfies a per-currency balance check *vacuously*, so the
//! mirrored code accepted what `validate_entry` rejects on its first line.
//!
//! Patching them one at a time is what produced them. This module is the single
//! owner both callers now consult (icn#2736): [`Ledger::validate_entry`] on the
//! append path, and `icnctl verify-backup` on a restored journal.
//!
//! # Scope of this ownership, stated exactly
//!
//! This module is the owner for the two consumers named above:
//! [`Ledger::validate_entry`](crate::Ledger) on the append path, and `icnctl
//! verify-backup` on a restored journal. It is **not** yet the owner for entry
//! *construction*: [`crate::entry::JournalEntryBuilder::build`] still applies its
//! own `validate_double_entry` and `validate_positive_amounts`, and those diverge
//! from this module in three ways — the builder accumulates with unchecked `+=`
//! rather than `checked_add`, compares Σdebits to Σcredits rather than summing
//! `net_change`, and does not reject an empty `accounts` array. Folding the
//! builder in is its own bounded change; claiming that ownership here before it
//! happens would be the same kind of overclaim this module exists to stop.
//!
//! The builder also owns a check this module does not have at all: amount signs.
//! A caller reporting on what "icn-ledger's entry validation" established must
//! therefore say so, rather than let the crate's name imply the union.
//!
//! # What this module deliberately does NOT own
//!
//! Freeze state, credit limits and progressive limits stay on `Ledger`. They are
//! not merely inconvenient to call from a verifier — they are **wrong** to call
//! there. A credit limit is computed against the balance a ledger holds now and
//! against the wall clock now, so re-evaluating a historical journal against
//! today's limits would reject entries that were valid when they were appended.
//! That is a false-rejection engine, and a backup verifier that produces one is
//! worse than one that admits the check is out of scope.

use std::collections::{BTreeMap, BTreeSet};

use crate::types::JournalEntry;
use crate::LedgerError;

/// A way in which a [`JournalEntry`] is invalid on its own terms.
///
/// Rendered straight into operator-facing diagnostics, so the wording is part of
/// the contract: it has to read correctly both as a ledger rejection on append
/// and as a row-level finding in a backup report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryDefect {
    /// The entry carries no account deltas at all.
    ///
    /// A journal entry that moves nothing is not a balanced entry — it is an
    /// entry with nothing to balance, which a per-currency sum accepts
    /// vacuously. Named as its own defect so no caller can mistake the two.
    NoAccountDeltas,

    /// A single delta's own `debit - credit` overflowed `i64`.
    DeltaOverflow {
        /// The message from [`crate::AccountDelta::net_change`].
        detail: String,
    },

    /// The running per-currency total overflowed `i64`.
    ///
    /// Distinct from an imbalance: the sum could not be computed at all. A
    /// verifier that widened this accumulator to `i128` would find such a row
    /// balanced and certify exactly what the ledger refuses.
    CurrencyOverflow {
        /// The currency whose accumulator overflowed.
        currency: String,
        /// The running total before the offending delta.
        running: i64,
        /// The delta that could not be added.
        change: i64,
    },

    /// A currency's deltas do not sum to zero **within this entry**.
    CurrencyImbalance {
        /// The currency that does not balance.
        currency: String,
        /// The non-zero sum.
        sum: i64,
    },
}

impl std::fmt::Display for EntryDefect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryDefect::NoAccountDeltas => write!(f, "entry has no account deltas"),
            EntryDefect::DeltaOverflow { detail } => write!(f, "{detail}"),
            EntryDefect::CurrencyOverflow {
                currency,
                running,
                change,
            } => write!(
                f,
                "arithmetic overflow in double-entry check: {running} + {change} \
                 for currency {currency}"
            ),
            EntryDefect::CurrencyImbalance { currency, sum } => write!(
                f,
                "currency {currency} sums to {sum}, so the entry does not balance"
            ),
        }
    }
}

impl From<EntryDefect> for LedgerError {
    /// Overflow stays [`LedgerError::ArithmeticOverflow`].
    ///
    /// `validate_entry` has always surfaced a failed accumulator as
    /// `ArithmeticOverflow` rather than as an invalid entry, and that
    /// distinction is what lets a caller tell "these numbers do not add up" from
    /// "these numbers cannot be added". Collapsing both into `InvalidEntry`
    /// while moving the code would be a silent narrowing.
    fn from(defect: EntryDefect) -> Self {
        let message = defect.to_string();
        match defect {
            EntryDefect::DeltaOverflow { .. } | EntryDefect::CurrencyOverflow { .. } => {
                LedgerError::ArithmeticOverflow(message)
            }
            EntryDefect::NoAccountDeltas | EntryDefect::CurrencyImbalance { .. } => {
                LedgerError::InvalidEntry(message)
            }
        }
    }
}

/// What [`inspect_entry`] established about one entry.
///
/// Carries the evidence, not just a verdict, because a caller that reports to an
/// operator must be able to say *which* currencies it actually balanced. A
/// summary that infers "the double-entry invariant holds" from the mere absence
/// of an error is the overclaim icn#2717 was about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryIntrinsics {
    currencies_balanced: BTreeSet<String>,
    defects: Vec<EntryDefect>,
}

impl EntryIntrinsics {
    /// Every defect found, in a deterministic order.
    pub fn defects(&self) -> &[EntryDefect] {
        &self.defects
    }

    /// The currencies whose per-entry sum was computed **and found to be zero**.
    ///
    /// Empty when a defect stopped the sum from being computed, so a caller
    /// cannot report coverage it did not obtain.
    pub fn currencies_balanced(&self) -> &BTreeSet<String> {
        &self.currencies_balanced
    }

    /// Whether the entry is valid on its own terms.
    pub fn is_valid(&self) -> bool {
        self.defects.is_empty()
    }

    /// The single-error view: the balanced currencies, or the first defect.
    ///
    /// This is the shape the append path wants, where any defect is a rejection
    /// and only the first one needs a message.
    pub fn into_result(self) -> Result<BTreeSet<String>, EntryDefect> {
        match self.defects.into_iter().next() {
            Some(defect) => Err(defect),
            None => Ok(self.currencies_balanced),
        }
    }
}

/// Validate a [`JournalEntry`] against every rule that is a function of the
/// entry alone.
///
/// Side-effect free and state-free by construction: it takes `&JournalEntry`,
/// touches no ledger, opens no store, reads no clock. That is what makes it
/// callable from `icnctl verify-backup`, which must not disturb the tree it is
/// inspecting.
///
/// Reports **all** currency imbalances rather than the first, because a backup
/// report lists findings per row; arithmetic failures stop the scan, matching
/// the append path's `?`, since a sum that could not be computed makes every
/// later sum for that entry meaningless.
pub fn inspect_entry(entry: &JournalEntry) -> EntryIntrinsics {
    let mut out = EntryIntrinsics::default();

    if entry.accounts.is_empty() {
        out.defects.push(EntryDefect::NoAccountDeltas);
        return out;
    }

    // `BTreeMap`, not `HashMap`: these sums become operator-facing findings, and
    // a hash-ordered walk makes two runs over the same corrupt row disagree
    // about which currency to name first.
    let mut sums: BTreeMap<String, i64> = BTreeMap::new();

    for delta in &entry.accounts {
        let change = match delta.net_change() {
            Ok(change) => change,
            Err(e) => {
                out.defects.push(EntryDefect::DeltaOverflow {
                    detail: e.to_string(),
                });
                return out;
            }
        };

        let running = sums.entry(delta.currency.clone()).or_insert(0);
        match running.checked_add(change) {
            Some(next) => *running = next,
            None => {
                out.defects.push(EntryDefect::CurrencyOverflow {
                    currency: delta.currency.clone(),
                    running: *running,
                    change,
                });
                return out;
            }
        }
    }

    for (currency, sum) in sums {
        if sum == 0 {
            out.currencies_balanced.insert(currency);
        } else {
            out.defects
                .push(EntryDefect::CurrencyImbalance { currency, sum });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AccountDelta, ProvenanceRef};

    /// Build an entry from `(currency, debit, credit)` triples.
    fn entry(deltas: &[(&str, i64, i64)]) -> JournalEntry {
        let author = icn_identity::KeyPair::generate().unwrap().did().clone();
        JournalEntry {
            id: None,
            timestamp: 1_700_000_000,
            author: author.clone(),
            contract_ref: None,
            accounts: deltas
                .iter()
                .map(|(currency, debit, credit)| AccountDelta {
                    account_id: author.clone(),
                    currency: (*currency).to_string(),
                    debit: if *debit == 0 { None } else { Some(*debit) },
                    credit: if *credit == 0 { None } else { Some(*credit) },
                })
                .collect(),
            parents: Vec::new(),
            signature: None,
            nonce: None,
            provenance: ProvenanceRef::SystemGenerated {
                reason: "icn#2736 unit fixture".to_string(),
            },
        }
    }

    #[test]
    fn a_balanced_entry_reports_the_currencies_it_actually_balanced() {
        let report = inspect_entry(&entry(&[
            ("hours", 60, 0),
            ("hours", 0, 60),
            ("kwh", 5, 0),
            ("kwh", 0, 5),
        ]));

        assert!(report.is_valid(), "defects: {:?}", report.defects());
        // Naming the set, not just the count: a caller reports coverage from
        // this, so "two currencies" must mean these two.
        assert_eq!(
            report
                .currencies_balanced()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["hours".to_string(), "kwh".to_string()],
        );
    }

    /// The defect the mirrored implementation structurally could not see.
    ///
    /// An empty `accounts` array makes every per-currency sum vacuously zero, so
    /// a balance-only check calls it valid. `validate_entry` rejects it on its
    /// first line, and so must this owner (icn#2736).
    #[test]
    fn an_entry_with_no_account_deltas_is_a_defect_not_a_vacuous_pass() {
        let report = inspect_entry(&entry(&[]));

        assert_eq!(report.defects(), &[EntryDefect::NoAccountDeltas]);
        assert!(
            report.currencies_balanced().is_empty(),
            "nothing was summed, so no currency may be reported as balanced"
        );
    }

    #[test]
    fn an_imbalanced_currency_is_reported_with_its_sum() {
        let report = inspect_entry(&entry(&[("hours", 100, 0), ("hours", 0, 40)]));

        assert_eq!(
            report.defects(),
            &[EntryDefect::CurrencyImbalance {
                currency: "hours".to_string(),
                sum: 60,
            }]
        );
    }

    /// Every imbalanced currency is reported, not just the first.
    ///
    /// A backup report lists findings per row; stopping at the first would
    /// under-report the corruption an operator is being asked to act on.
    #[test]
    fn all_imbalanced_currencies_are_reported_in_a_deterministic_order() {
        let report = inspect_entry(&entry(&[("kwh", 7, 0), ("hours", 100, 0)]));

        assert_eq!(
            report.defects(),
            &[
                EntryDefect::CurrencyImbalance {
                    currency: "hours".to_string(),
                    sum: 100,
                },
                EntryDefect::CurrencyImbalance {
                    currency: "kwh".to_string(),
                    sum: 7,
                },
            ],
            "findings must be ordered by currency, not by hash iteration order"
        );
    }

    /// A balanced currency alongside an imbalanced one is still not coverage.
    #[test]
    fn a_partially_balanced_entry_reports_the_defect_and_the_currency_it_did_balance() {
        let report = inspect_entry(&entry(&[("hours", 60, 0), ("hours", 0, 60), ("kwh", 7, 0)]));

        assert!(!report.is_valid());
        assert_eq!(
            report
                .currencies_balanced()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["hours".to_string()],
        );
    }

    /// The i128 trap, pinned at the owner.
    ///
    /// These deltas cancel in any wider accumulator. Under checked `i64` the
    /// running total overflows and the entry is refused, which is what the
    /// ledger does on append.
    #[test]
    fn a_running_total_that_overflows_i64_is_a_defect_not_a_zero_sum() {
        let report = inspect_entry(&entry(&[
            ("hours", i64::MAX, 0),
            ("hours", 1, 0),
            ("hours", 0, i64::MAX),
            ("hours", 0, 1),
        ]));

        assert!(matches!(
            report.defects(),
            [EntryDefect::CurrencyOverflow { currency, .. }] if currency == "hours"
        ));
        assert!(
            report.currencies_balanced().is_empty(),
            "the sum never completed, so the currency was not balanced"
        );
    }

    /// A single delta whose own `debit - credit` cannot be computed.
    #[test]
    fn a_delta_whose_net_change_overflows_is_a_defect() {
        let report = inspect_entry(&entry(&[("hours", i64::MIN, 1)]));

        assert!(
            matches!(report.defects(), [EntryDefect::DeltaOverflow { .. }]),
            "got {:?}",
            report.defects()
        );
    }

    /// Overflow keeps its own error class through the conversion.
    ///
    /// The append path distinguishes "these numbers do not add up" from "these
    /// numbers cannot be added"; folding both into `InvalidEntry` while moving
    /// the code would be a silent narrowing.
    #[test]
    fn overflow_converts_to_arithmetic_overflow_and_imbalance_to_invalid_entry() {
        assert!(matches!(
            LedgerError::from(EntryDefect::CurrencyOverflow {
                currency: "hours".to_string(),
                running: 1,
                change: 2,
            }),
            LedgerError::ArithmeticOverflow(_)
        ));
        assert!(matches!(
            LedgerError::from(EntryDefect::DeltaOverflow {
                detail: "x".to_string()
            }),
            LedgerError::ArithmeticOverflow(_)
        ));
        assert!(matches!(
            LedgerError::from(EntryDefect::NoAccountDeltas),
            LedgerError::InvalidEntry(_)
        ));
        assert!(matches!(
            LedgerError::from(EntryDefect::CurrencyImbalance {
                currency: "hours".to_string(),
                sum: 60,
            }),
            LedgerError::InvalidEntry(_)
        ));
    }

    #[test]
    fn into_result_surfaces_the_first_defect_and_otherwise_the_balanced_set() {
        assert_eq!(
            inspect_entry(&entry(&[])).into_result(),
            Err(EntryDefect::NoAccountDeltas)
        );
        assert_eq!(
            inspect_entry(&entry(&[("hours", 60, 0), ("hours", 0, 60)]))
                .into_result()
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["hours".to_string()],
        );
    }
}
