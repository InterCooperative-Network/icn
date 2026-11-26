//! P5b slice 1b: transactional persistence of a predecessor domain's lifecycle (EXPERIMENTAL).
//!
//! **Operationally inactive.** Nothing in the daemon constructs a [`LifecycleStore`]. The only
//! constructor is crate-private, and a Begin needs an [`ElectorateCommitment`], which has no
//! production constructor: the electorate constructor (canonical principal encoding, identity
//! owner) is separate, gated work. This module establishes local storage consistency only. It is
//! not consensus, and it is not institutional legitimacy.
//!
//! # What it does
//! * [`LifecycleStore::load`] reads L's lifecycle record and witness together and cross-checks them
//!   (`check_witness`). Neither present means L never began a cutover (untouched legacy state);
//!   anything missing, malformed, transplanted or contradictory is refused, never read as Legacy.
//! * [`LifecycleStore::transition`] applies one P5a transition (`validate_transition` decides), as
//!   **one journaled multi-key write** through `icn_store::recovery`. The write expects the exact
//!   record and witness bytes that were loaded (so a stale view conflicts). A Begin also expects
//!   the exact stored domain bytes its basis pin was computed from. The write bumps the storage
//!   revision (overflow refused), and on the first Begin establishes the immutable witness in the
//!   same write. The journal flushes before acknowledging.
//! * A Seal's `F` is recomputed here from the enumerated final set; a caller's `F` is never taken.
//!   Seal carries the Pending record's basis and `E` unchanged. Sealed is terminal (P5a).
//! * [`LifecycleStore::open`] succeeds only when the recovery journal admits. [`recover`] resolves
//!   what is safe to resolve (a fully applied or a never-applied intent; each is fully described
//!   by its intent). It leaves partial, foreign, torn or corrupt records quarantined, and admission
//!   stays closed until an authorized reconciliation.
//!
//! # Witness correspondence (stated precisely)
//! The witness records the first attempt and its `K`. While L's record is at that first attempt
//! (Pending, Sealed, or Legacy after aborting it), `K_first` is cross-checked against the record's
//! `K`. After a later Begin, the record no longer carries the first attempt's `K`, so only
//! `attempt ≥ first_attempt` and L's identity remain checkable. `K_first` is then preserved but
//! **not re-verified**. Extending this would need the record to carry `K_first` (32 more bytes in
//! the slice 1a layout), an experimental change proposed, not made. Simultaneous deletion or
//! rollback of both record and witness cannot be detected without independent evidence.
//!
//! # Ownership boundary
//! The store must be the daemon's owning governance [`SledStore`], so its fault domain covers every
//! holder. Standalone gateway mode shares a raw database and must never construct this type.

use icn_governance::cutover::CutoverCommitmentV1;
use icn_governance::domain::GovernanceDomainId;
use icn_governance::predecessor_lifecycle::{
    check_witness, domain_record_pin, lifecycle_basis, lifecycle_key, next_revision, witness_key,
    LifecyclePresence, LifecycleRecordV1, LifecycleWitnessV1, StoredLifecycle,
};
use icn_governance::predecessor_seal::{
    final_set_commitment, validate_transition, CutoverTransition, PredecessorLifecycle,
    TransitionOutcome,
};
use icn_store::recovery::{Entry, Outcome, RecoveryError, RecoveryIntent, RecoveryJournal};
use icn_store::{CompareAndWrite, SledStore, Store};

use crate::state_store::domain_key;

/// The journal prefix for lifecycle writes. It sits outside every `gov:` prefix the governance
/// store scans: `lifecycle-journal` differs from `lifecycle:` and `lifecycle-witness:`.
pub const LIFECYCLE_JOURNAL_PREFIX: &[u8] = b"gov:predecessor-lifecycle-journal:v1:";

/// The electorate commitment `E` for a Begin. Opaque here, and with **no production
/// constructor**: only the gated electorate constructor may assert that bytes commit to the
/// institutionally authorized electorate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectorateCommitment([u8; 32]);

impl ElectorateCommitment {
    /// Test-only: unverified bytes.
    #[cfg(test)]
    pub(crate) fn unverified_for_tests(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Why a lifecycle operation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleStoreError {
    /// The predecessor domain record is missing; a cutover cannot begin.
    DomainMissing,
    /// The loaded lifecycle is stale: another write landed first.
    Stale,
    /// Another lifecycle write for this domain is in flight.
    Busy,
    /// A Begin needs an electorate commitment.
    MissingElectorate,
    /// The stored record does not carry the Pending basis needed to seal.
    Inconsistent,
    /// A Begin replay names a different electorate commitment than the one recorded.
    ReplayMismatch,
}

impl std::fmt::Display for LifecycleStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::DomainMissing => "predecessor domain record is missing; a cutover cannot begin",
            Self::Stale => "the loaded lifecycle is stale: another write landed first",
            Self::Busy => "another lifecycle write for this domain is in flight",
            Self::MissingElectorate => "a Begin needs an electorate commitment",
            Self::Inconsistent => {
                "the stored record does not carry the Pending basis needed to seal"
            }
            Self::ReplayMismatch => {
                "a Begin replay names a different electorate commitment than the one recorded"
            }
        })
    }
}

impl std::error::Error for LifecycleStoreError {}

/// L's lifecycle as loaded, with the exact bytes a transition must expect.
///
/// Every field is private and only [`LifecycleStore::load`] and [`LifecycleStore::transition`]
/// construct one, so the state a transition validates against always corresponds to the bytes
/// it expects. A caller cannot edit the state to pass validation over genuine bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedLifecycle {
    predecessor: GovernanceDomainId,
    state: PredecessorLifecycle,
    revision: u64,
    record_bytes: Option<Vec<u8>>,
    witness_bytes: Option<Vec<u8>>,
    stored: Option<LifecycleRecordV1>,
}

impl LoadedLifecycle {
    /// The predecessor domain.
    pub fn predecessor(&self) -> &GovernanceDomainId {
        &self.predecessor
    }
    /// The lifecycle state.
    pub fn state(&self) -> &PredecessorLifecycle {
        &self.state
    }
    /// The storage revision (0 when L never began a cutover).
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// One requested transition.
#[derive(Debug, Clone)]
pub enum LifecycleRequest<'r> {
    Begin {
        attempt: u64,
        commitment: CutoverCommitmentV1,
        electorate: ElectorateCommitment,
    },
    Abort {
        attempt: u64,
        commitment_hash: [u8; 32],
    },
    Seal {
        attempt: u64,
        commitment_hash: [u8; 32],
        final_ids: &'r [&'r str],
    },
}

/// What a transition did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Committed {
    /// The exact transition that produced the current state; nothing was written.
    Replayed,
    /// Written and durable.
    Written(Box<LoadedLifecycle>),
}

/// Test-only hook type (see `LifecycleStore::before_apply`).
#[cfg(test)]
type BeforeApplyHook<'a> = Box<dyn FnOnce(&SledStore) + 'a>;

/// The admitted, owning-store-only lifecycle persistence.
pub struct LifecycleStore<'a> {
    store: &'a SledStore,
    journal: RecoveryJournal<'a>,
    /// Held for the store's lifetime; admission is also re-checked before every transition.
    _admission: icn_store::recovery::Admission<'a>,
    /// Test-only: runs once between journaling an intent and applying it, to model a concurrent
    /// writer landing in that window.
    #[cfg(test)]
    before_apply: std::cell::RefCell<Option<BeforeApplyHook<'a>>>,
}

/// What [`recover`] did.
#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub completed: usize,
    pub abandoned: usize,
    /// Records left for an authorized reconciliation; admission stays closed while any remain.
    pub quarantined: Vec<Outcome>,
}

/// Resolve the lifecycle journal after a restart: complete fully applied intents and abandon
/// never-applied ones. Every other record is quarantined.
// Operationally inactive by design: no production call site until P5b slice 2 / activation.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn recover(store: &SledStore) -> anyhow::Result<RecoveryReport> {
    let journal = RecoveryJournal::new(store, LIFECYCLE_JOURNAL_PREFIX)?;
    let mut report = RecoveryReport::default();
    for outcome in journal.classify()? {
        match &outcome {
            Outcome::Applied(i) => {
                journal.complete(&i.txid)?;
                report.completed += 1;
            }
            Outcome::NotApplied(i) => {
                journal.abandon(&i.txid)?;
                report.abandoned += 1;
            }
            _ => report.quarantined.push(outcome),
        }
    }
    Ok(report)
}

impl<'a> LifecycleStore<'a> {
    /// The only constructor: over the owning governance store, and only once the recovery
    /// journal admits.
    // Operationally inactive by design: no production call site until P5b slice 2 / activation.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn open(store: &'a SledStore) -> anyhow::Result<Self> {
        let journal = RecoveryJournal::new(store, LIFECYCLE_JOURNAL_PREFIX)?;
        let admission = journal.admit()?;
        Ok(Self {
            store,
            journal,
            _admission: admission,
            #[cfg(test)]
            before_apply: std::cell::RefCell::new(None),
        })
    }

    /// Load and cross-check L's record and witness.
    pub fn load(&self, l: &GovernanceDomainId) -> anyhow::Result<LoadedLifecycle> {
        let record_bytes = self.store.get(&lifecycle_key(l)?)?;
        let witness_bytes = self.store.get(&witness_key(l)?)?;
        let presence = check_witness(l, record_bytes.as_deref(), witness_bytes.as_deref())?;
        let (state, revision, stored) = match presence {
            LifecyclePresence::NeverBegun => (PredecessorLifecycle::initial(), 0, None),
            LifecyclePresence::Established(record) => {
                let state = match &record.state {
                    StoredLifecycle::Legacy {
                        last_attempt,
                        aborted,
                    } => PredecessorLifecycle::Legacy {
                        last_attempt: *last_attempt,
                        aborted: *aborted,
                    },
                    StoredLifecycle::Pending {
                        attempt,
                        commitment,
                        ..
                    } => PredecessorLifecycle::CutoverPending {
                        attempt: *attempt,
                        commitment: commitment.clone(),
                    },
                    StoredLifecycle::Sealed {
                        attempt,
                        commitment,
                        final_set,
                        ..
                    } => PredecessorLifecycle::Sealed {
                        attempt: *attempt,
                        commitment: commitment.clone(),
                        final_set: *final_set,
                    },
                };
                (state, record.revision, Some(*record))
            }
        };
        Ok(LoadedLifecycle {
            predecessor: l.clone(),
            state,
            revision,
            record_bytes,
            witness_bytes,
            stored,
        })
    }

    /// Apply one transition against exactly `loaded`, durably.
    pub fn transition(
        &self,
        loaded: &LoadedLifecycle,
        request: LifecycleRequest<'_>,
        txid: [u8; 16],
    ) -> anyhow::Result<Committed> {
        let l = &loaded.predecessor;
        let (transition, electorate) = match request {
            LifecycleRequest::Begin {
                attempt,
                commitment,
                electorate,
            } => (
                CutoverTransition::Begin {
                    attempt,
                    commitment,
                },
                Some(electorate),
            ),
            LifecycleRequest::Abort {
                attempt,
                commitment_hash,
            } => (
                CutoverTransition::Abort {
                    attempt,
                    commitment_hash,
                },
                None,
            ),
            LifecycleRequest::Seal {
                attempt,
                commitment_hash,
                final_ids,
            } => (
                CutoverTransition::Seal {
                    attempt,
                    commitment_hash,
                    // Recomputed by the store from the enumerated set, never taken from a caller.
                    final_set: final_set_commitment(l, attempt, &commitment_hash, final_ids)?,
                },
                None,
            ),
        };
        let next = match validate_transition(l, &loaded.state, &transition)? {
            TransitionOutcome::Replayed => {
                // Acknowledge a replay only against the CURRENT stored bytes, and only if a Begin
                // replay names the electorate commitment that was recorded.
                if self.store.get(&lifecycle_key(l)?)? != loaded.record_bytes {
                    return Err(LifecycleStoreError::Stale.into());
                }
                if let (
                    Some(e),
                    Some(LifecycleRecordV1 {
                        state: StoredLifecycle::Pending { electorate, .. },
                        ..
                    }),
                ) = (electorate.as_ref(), loaded.stored.as_ref())
                {
                    if e.0 != *electorate {
                        return Err(LifecycleStoreError::ReplayMismatch.into());
                    }
                }
                return Ok(Committed::Replayed);
            }
            TransitionOutcome::Validated(next) => next,
        };
        let revision = next_revision(loaded.revision)?;
        let mut guards: Vec<Entry> = Vec::new();
        let state = match &next {
            PredecessorLifecycle::Legacy {
                last_attempt,
                aborted,
            } => StoredLifecycle::Legacy {
                last_attempt: *last_attempt,
                aborted: *aborted,
            },
            PredecessorLifecycle::CutoverPending {
                attempt,
                commitment,
            } => {
                let electorate = electorate.ok_or(LifecycleStoreError::MissingElectorate)?;
                let dkey = domain_key(l);
                let domain_bytes = self
                    .store
                    .get(&dkey)?
                    .ok_or(LifecycleStoreError::DomainMissing)?;
                let pin = domain_record_pin(&domain_bytes);
                let basis = lifecycle_basis(l, loaded.revision, &electorate.0, &pin)?;
                // The pinned bytes must still be the domain's at the write's linearization point.
                guards.push((dkey, Some(domain_bytes)));
                StoredLifecycle::Pending {
                    attempt: *attempt,
                    commitment: commitment.clone(),
                    basis,
                    electorate: electorate.0,
                }
            }
            PredecessorLifecycle::Sealed {
                attempt,
                commitment,
                final_set,
            } => {
                let (basis, electorate) = match &loaded.stored {
                    Some(LifecycleRecordV1 {
                        state:
                            StoredLifecycle::Pending {
                                basis, electorate, ..
                            },
                        ..
                    }) => (*basis, *electorate),
                    _ => return Err(LifecycleStoreError::Inconsistent.into()),
                };
                StoredLifecycle::Sealed {
                    attempt: *attempt,
                    commitment: commitment.clone(),
                    basis,
                    electorate,
                    final_set: *final_set,
                }
            }
        };
        let record = LifecycleRecordV1 {
            predecessor: l.clone(),
            revision,
            state,
        };
        let record_bytes = record.encode()?;
        let rkey = lifecycle_key(l)?;
        let wkey = witness_key(l)?;
        // Refuse an obviously stale view before journaling anything. The journaled write's own
        // expectations remain the authority if something lands after this check.
        if self.store.get(&rkey)? != loaded.record_bytes
            || self.store.get(&wkey)? != loaded.witness_bytes
        {
            return Err(LifecycleStoreError::Stale.into());
        }
        let mut expected = vec![
            (rkey.clone(), loaded.record_bytes.clone()),
            (wkey.clone(), loaded.witness_bytes.clone()),
        ];
        expected.extend(guards);
        let mut intended = vec![(rkey, Some(record_bytes.clone()))];
        let witness_bytes = match &loaded.witness_bytes {
            Some(existing) => existing.clone(),
            None => {
                // First Begin: the immutable witness is established in the same write.
                let w = LifecycleWitnessV1::for_first_pending(&record)?.encode()?;
                intended.push((wkey, Some(w.clone())));
                w
            }
        };
        let intent = RecoveryIntent {
            txid,
            expected,
            intended,
        };
        // Any unresolved journal record (on any domain) closes the store until recovery.
        self.journal.admit()?;
        if let Err(e) = self.journal.begin(&intent) {
            return Err(match e.downcast_ref::<RecoveryError>() {
                Some(RecoveryError::Overlap) => LifecycleStoreError::Busy.into(),
                _ => e,
            });
        }
        #[cfg(test)]
        if let Some(hook) = self.before_apply.borrow_mut().take() {
            hook(self.store);
        }
        match self.journal.apply(&intent)? {
            CompareAndWrite::Applied => self.journal.complete(&txid)?,
            CompareAndWrite::Conflict { .. } => {
                // A conflict is definitive: nothing was written. If the keys still hold our
                // priors the intent is NotApplied (abandon); if another writer moved them it
                // classifies as Foreign, and since we know our write did not land, the record is
                // retired as reconciled. Any storage error propagates (the store faults).
                if let Err(e) = self.journal.abandon(&txid) {
                    match e.downcast_ref::<RecoveryError>() {
                        Some(RecoveryError::WrongState) => self.journal.retire_reconciled(&txid)?,
                        _ => return Err(e),
                    }
                }
                return Err(LifecycleStoreError::Stale.into());
            }
        }
        Ok(Committed::Written(Box::new(LoadedLifecycle {
            predecessor: l.clone(),
            state: next,
            revision,
            record_bytes: Some(record_bytes),
            witness_bytes: Some(witness_bytes),
            stored: Some(record),
        })))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use icn_governance::predecessor_lifecycle::LifecycleCodecError;
    use icn_governance::predecessor_seal::SealRefusal;

    /// P4's golden cutover encoding (L = legacy.coop.example, S = successor.coop.example).
    const GOLDEN_C: &str = "0000001969636e2e676f7665726e616e63652e6375746f7665722e76310001000000136c65676163792e636f6f702e6578616d706c6500000016737563636573736f722e636f6f702e6578616d706c65c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1c76438699ba705283321e5fd451416db99e077315671aad3a0edffa98eb2fda34d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d";
    const L: &str = "legacy.coop.example";

    fn l() -> GovernanceDomainId {
        GovernanceDomainId(L.into())
    }

    /// The golden commitment, with its recognition-map bytes set to `m` (a distinct valid K).
    fn commitment(m: u8) -> CutoverCommitmentV1 {
        let mut b = hex::decode(GOLDEN_C).unwrap();
        let n = b.len();
        b[n - 32..].copy_from_slice(&[m; 32]);
        CutoverCommitmentV1::decode(&b).unwrap()
    }

    fn k(m: u8) -> [u8; 32] {
        commitment(m).commitment().unwrap()
    }

    fn e() -> ElectorateCommitment {
        ElectorateCommitment::unverified_for_tests([0xE1; 32])
    }

    const DOMAIN: &[u8] = br#"{"id":"legacy.coop.example"}"#;

    fn store() -> SledStore {
        let s = SledStore::temporary().unwrap();
        s.put(&domain_key(&l()), DOMAIN).unwrap();
        s
    }

    fn begin(attempt: u64, m: u8) -> LifecycleRequest<'static> {
        LifecycleRequest::Begin {
            attempt,
            commitment: commitment(m),
            electorate: e(),
        }
    }

    fn written(c: Committed) -> LoadedLifecycle {
        match c {
            Committed::Written(l) => *l,
            Committed::Replayed => panic!("expected a write"),
        }
    }

    fn admitted(s: &SledStore) {
        RecoveryJournal::new(s, LIFECYCLE_JOURNAL_PREFIX)
            .unwrap()
            .admit()
            .unwrap();
    }

    #[test]
    fn the_first_begin_establishes_record_and_witness_together() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let fresh = ls.load(&l()).unwrap();
        assert_eq!(fresh.state, PredecessorLifecycle::initial());
        assert_eq!(fresh.revision, 0);
        let after = written(ls.transition(&fresh, begin(1, 0x4D), [1; 16]).unwrap());
        assert_eq!(after.revision, 1);
        assert_eq!(ls.load(&l()).unwrap(), after);
        let w = LifecycleWitnessV1::decode_for(
            &l(),
            &s.get(&witness_key(&l()).unwrap()).unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!((w.first_attempt, w.first_commitment_hash), (1, k(0x4D)));
        // The basis binds E and the exact stored domain bytes at the Begin revision.
        match after.stored.unwrap().state {
            StoredLifecycle::Pending {
                basis, electorate, ..
            } => {
                assert_eq!(electorate, [0xE1; 32]);
                assert_eq!(
                    basis,
                    lifecycle_basis(&l(), 0, &[0xE1; 32], &domain_record_pin(DOMAIN)).unwrap()
                );
            }
            other => panic!("{other:?}"),
        }
        admitted(&s);
    }

    #[test]
    fn an_exact_replay_writes_nothing_and_a_different_begin_is_refused() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let before = s.get(&lifecycle_key(&l()).unwrap()).unwrap();
        assert_eq!(
            ls.transition(&p, begin(1, 0x4D), [2; 16]).unwrap(),
            Committed::Replayed
        );
        assert!(ls.transition(&p, begin(1, 0x4E), [3; 16]).is_err());
        assert_eq!(s.get(&lifecycle_key(&l()).unwrap()).unwrap(), before);
    }

    #[test]
    fn abort_then_retry_keeps_the_witness_and_advances_both_counters() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p1 = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let wbefore = s.get(&witness_key(&l()).unwrap()).unwrap();
        let aborted = written(
            ls.transition(
                &p1,
                LifecycleRequest::Abort {
                    attempt: 1,
                    commitment_hash: k(0x4D),
                },
                [2; 16],
            )
            .unwrap(),
        );
        assert_eq!(aborted.revision, 2);
        let p2 = written(ls.transition(&aborted, begin(2, 0x4E), [3; 16]).unwrap());
        assert_eq!((p2.revision, p2.state.attempt()), (3, 2));
        assert_eq!(
            s.get(&witness_key(&l()).unwrap()).unwrap(),
            wbefore,
            "witness immutable"
        );
        assert_eq!(ls.load(&l()).unwrap(), p2);
    }

    #[test]
    fn seal_recomputes_f_and_is_terminal() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let ids = ["p-a", "p-b"];
        let sealed = written(
            ls.transition(
                &p,
                LifecycleRequest::Seal {
                    attempt: 1,
                    commitment_hash: k(0x4D),
                    final_ids: &ids,
                },
                [2; 16],
            )
            .unwrap(),
        );
        let expected_f = final_set_commitment(&l(), 1, &k(0x4D), &ids).unwrap();
        assert!(
            matches!(&sealed.state, PredecessorLifecycle::Sealed { final_set, .. } if *final_set == expected_f)
        );
        // Basis and E carried unchanged from Pending.
        assert!(matches!(
            sealed.stored.as_ref().unwrap().state,
            StoredLifecycle::Sealed { electorate, .. } if electorate == [0xE1; 32]
        ));
        let e = ls.transition(&sealed, begin(2, 0x4E), [3; 16]).unwrap_err();
        assert!(e.downcast_ref::<SealRefusal>().is_some(), "{e:#}");
    }

    #[test]
    fn a_stale_view_conflicts_and_leaves_no_journal_record() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let fresh = ls.load(&l()).unwrap();
        written(ls.transition(&fresh, begin(1, 0x4D), [1; 16]).unwrap());
        let e = ls.transition(&fresh, begin(1, 0x4E), [2; 16]).unwrap_err();
        assert_eq!(
            e.downcast_ref::<LifecycleStoreError>(),
            Some(&LifecycleStoreError::Stale)
        );
        admitted(&s);
    }

    #[test]
    fn a_write_landing_after_the_journal_opened_is_stale_and_leaves_nothing_behind() {
        // Model the race the pre-check cannot see: our intent is journaled, then another write
        // moves the record before our apply. The apply conflicts, and the intent (now Foreign)
        // is retired because our write definitively did not land.
        let s = store();
        let rkey = lifecycle_key(&l()).unwrap();
        let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
        let ours = RecoveryIntent {
            txid: [7; 16],
            expected: vec![(rkey.clone(), None)],
            intended: vec![(rkey.clone(), Some(b"ours".to_vec()))],
        };
        journal.begin(&ours).unwrap();
        s.put(&rkey, b"theirs").unwrap();
        assert!(matches!(
            journal.apply(&ours).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert!(
            journal.abandon(&[7; 16]).is_err(),
            "Foreign: not abandonable"
        );
        journal.retire_reconciled(&[7; 16]).unwrap();
        admitted(&s);
        assert_eq!(s.get(&rkey).unwrap(), Some(b"theirs".to_vec()));
    }

    #[test]
    fn a_domain_change_between_load_and_write_is_caught_by_the_pin_guard() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let fresh = ls.load(&l()).unwrap();
        // The domain is read inside `transition`; a concurrent edit after that read is modelled
        // by changing it while the intent is open, then applying: the write must conflict.
        let rkey = lifecycle_key(&l()).unwrap();
        let wkey = witness_key(&l()).unwrap();
        let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
        let intent = RecoveryIntent {
            txid: [9; 16],
            expected: vec![
                (rkey.clone(), None),
                (wkey, None),
                (domain_key(&l()), Some(DOMAIN.to_vec())),
            ],
            intended: vec![(rkey, Some(b"x".to_vec()))],
        };
        journal.begin(&intent).unwrap();
        s.put(&domain_key(&l()), b"edited").unwrap();
        assert!(matches!(
            journal.apply(&intent).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        // Only the guard moved; every written key is at its prior: NotApplied, abandoned.
        journal.abandon(&[9; 16]).unwrap();
        // And a real Begin over an edited domain pins the edited bytes, not the old ones.
        let p = written(ls.transition(&fresh, begin(1, 0x4D), [1; 16]).unwrap());
        assert!(matches!(
            p.stored.unwrap().state,
            StoredLifecycle::Pending { basis, .. }
                if basis == lifecycle_basis(&l(), 0, &[0xE1; 32], &domain_record_pin(b"edited")).unwrap()
        ));
    }

    #[test]
    fn seal_carries_the_pending_basis_and_electorate_unchanged() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let (pb, pe) = match &p.stored.as_ref().unwrap().state {
            StoredLifecycle::Pending {
                basis, electorate, ..
            } => (*basis, *electorate),
            other => panic!("{other:?}"),
        };
        let ids = ["p-a"];
        let sealed = written(
            ls.transition(
                &p,
                LifecycleRequest::Seal {
                    attempt: 1,
                    commitment_hash: k(0x4D),
                    final_ids: &ids,
                },
                [2; 16],
            )
            .unwrap(),
        );
        assert!(matches!(
            sealed.stored.unwrap().state,
            StoredLifecycle::Sealed { basis, electorate, .. } if basis == pb && electorate == pe
        ));
    }

    #[test]
    fn a_domain_edit_after_begin_read_it_makes_the_begin_stale_and_leaves_nothing() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let fresh = ls.load(&l()).unwrap();
        *ls.before_apply.borrow_mut() = Some(Box::new(|st: &SledStore| {
            st.put(&domain_key(&l()), b"edited concurrently").unwrap();
        }));
        let e = ls.transition(&fresh, begin(1, 0x4D), [1; 16]).unwrap_err();
        assert_eq!(
            e.downcast_ref::<LifecycleStoreError>(),
            Some(&LifecycleStoreError::Stale)
        );
        assert_eq!(
            s.get(&lifecycle_key(&l()).unwrap()).unwrap(),
            None,
            "no record written"
        );
        assert_eq!(
            s.get(&witness_key(&l()).unwrap()).unwrap(),
            None,
            "no witness written"
        );
        admitted(&s);
    }

    #[test]
    fn a_record_write_landing_after_journaling_is_stale_and_retired_through_the_store() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let fresh = ls.load(&l()).unwrap();
        *ls.before_apply.borrow_mut() = Some(Box::new(|st: &SledStore| {
            st.put(&lifecycle_key(&l()).unwrap(), b"another writer")
                .unwrap();
        }));
        let e = ls.transition(&fresh, begin(1, 0x4D), [1; 16]).unwrap_err();
        assert_eq!(
            e.downcast_ref::<LifecycleStoreError>(),
            Some(&LifecycleStoreError::Stale)
        );
        assert_eq!(
            s.get(&lifecycle_key(&l()).unwrap()).unwrap(),
            Some(b"another writer".to_vec()),
            "the other write is left as it is"
        );
        admitted(&s);
    }

    #[test]
    fn a_begin_replay_with_a_different_electorate_is_refused() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let other_e = LifecycleRequest::Begin {
            attempt: 1,
            commitment: commitment(0x4D),
            electorate: ElectorateCommitment::unverified_for_tests([0xE2; 32]),
        };
        let e = ls.transition(&p, other_e, [2; 16]).unwrap_err();
        assert_eq!(
            e.downcast_ref::<LifecycleStoreError>(),
            Some(&LifecycleStoreError::ReplayMismatch)
        );
    }

    #[test]
    fn a_replay_against_a_stale_snapshot_is_refused() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        // Someone else aborts attempt 1 after our snapshot.
        written(
            ls.transition(
                &p,
                LifecycleRequest::Abort {
                    attempt: 1,
                    commitment_hash: k(0x4D),
                },
                [2; 16],
            )
            .unwrap(),
        );
        let e = ls.transition(&p, begin(1, 0x4D), [3; 16]).unwrap_err();
        assert_eq!(
            e.downcast_ref::<LifecycleStoreError>(),
            Some(&LifecycleStoreError::Stale)
        );
    }

    #[test]
    fn invalid_transitions_are_refused_and_write_nothing() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let fresh = ls.load(&l()).unwrap();
        let ids = ["p"];
        // Seal from the untouched legacy state; Begin with an attempt gap.
        assert!(ls
            .transition(
                &fresh,
                LifecycleRequest::Seal {
                    attempt: 1,
                    commitment_hash: k(0x4D),
                    final_ids: &ids
                },
                [1; 16],
            )
            .is_err());
        assert!(ls.transition(&fresh, begin(2, 0x4D), [2; 16]).is_err());
        assert_eq!(s.get(&lifecycle_key(&l()).unwrap()).unwrap(), None);
        let p = written(ls.transition(&fresh, begin(1, 0x4D), [3; 16]).unwrap());
        let before = s.get(&lifecycle_key(&l()).unwrap()).unwrap();
        // Abort with the wrong K; Seal at the wrong attempt or with the wrong K.
        for bad in [
            LifecycleRequest::Abort {
                attempt: 1,
                commitment_hash: k(0x4E),
            },
            LifecycleRequest::Seal {
                attempt: 2,
                commitment_hash: k(0x4D),
                final_ids: &ids,
            },
            LifecycleRequest::Seal {
                attempt: 1,
                commitment_hash: k(0x4E),
                final_ids: &ids,
            },
        ] {
            assert!(ls.transition(&p, bad, [4; 16]).is_err());
        }
        assert_eq!(s.get(&lifecycle_key(&l()).unwrap()).unwrap(), before);
        admitted(&s);
    }

    #[test]
    fn a_witness_tampered_after_abort_and_retry_is_refused() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p1 = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let a = written(
            ls.transition(
                &p1,
                LifecycleRequest::Abort {
                    attempt: 1,
                    commitment_hash: k(0x4D),
                },
                [2; 16],
            )
            .unwrap(),
        );
        written(ls.transition(&a, begin(2, 0x4E), [3; 16]).unwrap());
        // A witness claiming a later first attempt than the record's attempt is refused.
        let mut w = LifecycleWitnessV1::decode_for(
            &l(),
            &s.get(&witness_key(&l()).unwrap()).unwrap().unwrap(),
        )
        .unwrap();
        w.first_attempt = 3;
        s.put(&witness_key(&l()).unwrap(), &w.encode().unwrap())
            .unwrap();
        assert_eq!(
            ls.load(&l())
                .unwrap_err()
                .downcast_ref::<LifecycleCodecError>(),
            Some(&LifecycleCodecError::WitnessMismatch)
        );
    }

    #[test]
    fn an_unresolved_record_on_any_domain_closes_transitions() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let fresh = ls.load(&l()).unwrap();
        // Another domain's write left an unresolved intent after this store opened.
        let other = lifecycle_key(&GovernanceDomainId("other.coop".into())).unwrap();
        let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
        journal
            .begin(&RecoveryIntent {
                txid: [8; 16],
                expected: vec![(other.clone(), None)],
                intended: vec![(other, Some(b"o".to_vec()))],
            })
            .unwrap();
        assert!(ls.transition(&fresh, begin(1, 0x4D), [1; 16]).is_err());
        assert_eq!(s.get(&lifecycle_key(&l()).unwrap()).unwrap(), None);
    }

    #[test]
    fn concurrent_begins_have_one_winner() {
        let s = std::sync::Arc::new(store());
        let results: Vec<_> = (0..6u8)
            .map(|t| {
                let s = s.clone();
                std::thread::spawn(move || {
                    let ls = LifecycleStore::open(&s).unwrap();
                    let fresh = ls.load(&l()).unwrap();
                    ls.transition(&fresh, begin(1, 0x40 + t), [t + 1; 16])
                        .is_ok()
                })
            })
            .map(|h| h.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|ok| **ok).count(), 1, "{results:?}");
        admitted(&s);
    }

    #[test]
    fn a_missing_or_transplanted_record_or_witness_is_refused_never_legacy() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let wkey = witness_key(&l()).unwrap();
        let w = s.get(&wkey).unwrap().unwrap();
        s.delete(&wkey).unwrap();
        assert_eq!(
            ls.load(&l())
                .unwrap_err()
                .downcast_ref::<LifecycleCodecError>(),
            Some(&LifecycleCodecError::WitnessMismatch)
        );
        s.put(&wkey, &w).unwrap();
        let rkey = lifecycle_key(&l()).unwrap();
        s.delete(&rkey).unwrap();
        assert!(ls.load(&l()).is_err());
        // A witness copied under another domain's key.
        let other = GovernanceDomainId("other.coop".into());
        s.put(&witness_key(&other).unwrap(), &w).unwrap();
        assert!(ls.load(&other).is_err());
    }

    #[test]
    fn revision_exhaustion_is_refused() {
        let s = store();
        let ls = LifecycleStore::open(&s).unwrap();
        let p = written(
            ls.transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
                .unwrap(),
        );
        let mut rec = p.stored.clone().unwrap();
        rec.revision = u64::MAX;
        s.put(&lifecycle_key(&l()).unwrap(), &rec.encode().unwrap())
            .unwrap();
        let maxed = ls.load(&l()).unwrap();
        let e = ls
            .transition(
                &maxed,
                LifecycleRequest::Abort {
                    attempt: 1,
                    commitment_hash: k(0x4D),
                },
                [2; 16],
            )
            .unwrap_err();
        assert_eq!(
            e.downcast_ref::<LifecycleCodecError>(),
            Some(&LifecycleCodecError::RevisionOverflow)
        );
    }

    #[test]
    fn a_begin_without_the_domain_record_is_refused() {
        let s = SledStore::temporary().unwrap();
        let ls = LifecycleStore::open(&s).unwrap();
        let e = ls
            .transition(&ls.load(&l()).unwrap(), begin(1, 0x4D), [1; 16])
            .unwrap_err();
        assert_eq!(
            e.downcast_ref::<LifecycleStoreError>(),
            Some(&LifecycleStoreError::DomainMissing)
        );
    }

    #[test]
    fn interrupted_writes_keep_the_store_closed_until_recovery_resolves_them() {
        let dir = tempfile::tempdir().unwrap();
        let (intent_applied, intent_not) = {
            let s = SledStore::open(dir.path()).unwrap();
            s.put(&domain_key(&l()), DOMAIN).unwrap();
            let rkey = lifecycle_key(&l()).unwrap();
            let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
            // A write applied but never acknowledged...
            let applied = RecoveryIntent {
                txid: [1; 16],
                expected: vec![(rkey.clone(), None)],
                intended: vec![(rkey, Some(b"r".to_vec()))],
            };
            journal.begin(&applied).unwrap();
            journal.apply(&applied).unwrap();
            // ...and one journaled but never applied.
            let other = GovernanceDomainId("other.coop".into());
            let okey = lifecycle_key(&other).unwrap();
            let not = RecoveryIntent {
                txid: [2; 16],
                expected: vec![(okey.clone(), None)],
                intended: vec![(okey, Some(b"o".to_vec()))],
            };
            journal.begin(&not).unwrap();
            (applied, not)
        };
        let s = SledStore::open(dir.path()).unwrap();
        assert!(LifecycleStore::open(&s).is_err(), "closed after restart");
        let report = recover(&s).unwrap();
        assert_eq!((report.completed, report.abandoned), (1, 1));
        assert!(report.quarantined.is_empty());
        assert!(LifecycleStore::open(&s).is_ok());
        assert_eq!(
            s.get(&intent_applied.intended[0].0).unwrap(),
            Some(b"r".to_vec())
        );
        assert_eq!(s.get(&intent_not.intended[0].0).unwrap(), None);
    }

    #[test]
    fn a_partial_write_is_quarantined_and_admission_stays_closed() {
        let s = store();
        let rkey = lifecycle_key(&l()).unwrap();
        let wkey = witness_key(&l()).unwrap();
        let journal = RecoveryJournal::new(&s, LIFECYCLE_JOURNAL_PREFIX).unwrap();
        let intent = RecoveryIntent {
            txid: [1; 16],
            expected: vec![(rkey.clone(), None), (wkey.clone(), None)],
            intended: vec![
                (rkey.clone(), Some(b"r".to_vec())),
                (wkey, Some(b"w".to_vec())),
            ],
        };
        journal.begin(&intent).unwrap();
        // Simulated prefix of the commit: the record landed, the witness did not.
        s.put(&rkey, b"r").unwrap();
        let report = recover(&s).unwrap();
        assert_eq!(report.quarantined.len(), 1);
        assert!(matches!(report.quarantined[0], Outcome::Partial(_)));
        assert!(LifecycleStore::open(&s).is_err());
    }

    #[test]
    fn the_journal_prefix_is_outside_every_lifecycle_key() {
        let rk = lifecycle_key(&l()).unwrap();
        let wk = witness_key(&l()).unwrap();
        assert!(!rk.starts_with(LIFECYCLE_JOURNAL_PREFIX));
        assert!(!wk.starts_with(LIFECYCLE_JOURNAL_PREFIX));
        assert!(!domain_key(&l()).starts_with(LIFECYCLE_JOURNAL_PREFIX));
    }
}
