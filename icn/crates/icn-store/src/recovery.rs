//! A durable recovery journal for one multi-key write (meaning-neutral).
//!
//! [`crate::Store::compare_and_write`] is atomic on success, but an error after its commit began
//! leaves the outcome unknown, even after restart (see [`crate::fault`]). A caller that must never
//! act on unknown state brackets each such write with a [`RecoveryJournal`]:
//!
//! 1. [`RecoveryJournal::begin`]: one `compare_and_write` that writes the [`RecoveryIntent`] (stable
//!    id, the exact prior value of every key it touches, the exact intended values, an integrity
//!    digest) **and a lock entry per touched key**, expecting all of them absent, then `flush`.
//!    Overlapping intents and duplicate ids are therefore an atomic conflict.
//! 2. [`RecoveryJournal::apply`]: one `compare_and_write` that expects the journaled intent bytes,
//!    **every one of its locks**, and every prior value, writes the intended values, then
//!    `flush`. A write can land only while its whole intent is on record, never on a torn one.
//! 3. Retirement ([`RecoveryJournal::complete`] after an applied write,
//!    [`RecoveryJournal::abandon`] after one that never landed,
//!    [`RecoveryJournal::retire_reconciled`] after the caller's own reconciliation): classify,
//!    `flush` so the relied-on state is durable, then remove the intent and its locks in one
//!    `compare_and_write` that **also expects every touched key at the exact value classification
//!    read**, then `flush`. An intent therefore never disappears before the state it describes is
//!    durable. A write landing between classification and retirement turns retirement into a
//!    conflict.
//!
//! The journal's own writes obey the same failure contract. A torn intent or lock set is
//! reported, never trusted.
//!
//! | Outcome | Meaning | Resolution |
//! |---|---|---|
//! | [`Outcome::NotApplied`] | every key at its prior value | `apply` or `abandon` |
//! | [`Outcome::Applied`] | every written key at its intended value, guards unchanged | `complete` |
//! | [`Outcome::Partial`] | every key at its prior or intended value, but mixed | caller reconciles, then `retire_reconciled` |
//! | [`Outcome::Foreign`] | some key matches neither | caller reconciles, then `retire_reconciled` |
//! | [`Outcome::Torn`] | a decodable intent missing some of its locks (an interrupted `begin` or retirement) | caller inspects, then `retire_torn` |
//! | [`Outcome::Corrupt`] | an intent that fails its digest, does not decode, or is misfiled | none in-store: needs out-of-band repair |
//! | [`Outcome::OrphanLock`] | a lock with no intent | `clear_orphan_locks`, refused while any intent is Corrupt |
//!
//! The store never chooses an outcome for the caller and offers no rollback.
//! [`RecoveryJournal::admit`] returns an [`Admission`] only when nothing remains. The token is
//! branded with the journal's lifetime and proves the journal was empty *when checked*. An owning
//! store must take it by value in its only constructor, expose no raw store, and route every later
//! multi-key write through this journal. Nothing in this crate prevents a holder of the raw store
//! from bypassing it.
//!
//! Limits: local to one process and one database; relies on `flush` meaning durable (sled's
//! contract); cannot detect a privileged raw edit that leaves every key at a prior or intended
//! value; the digest guards integrity, not authenticity.

use std::marker::PhantomData;

use sha2::{Digest, Sha256};

use crate::{CompareAndWrite, Expectation, Mutation, Store, MAX_COMPARE_AND_WRITE_ENTRIES};

const INTENT_TAG: &[u8] = b"icn.store.recovery-intent.v1";
const INTENT_VERSION: u16 = 1;
const INTENT_SUBSPACE: u8 = b'i';
const LOCK_SUBSPACE: u8 = b'l';

/// One key's value in an intent: `None` means the key is absent.
pub type Entry = (Vec<u8>, Option<Vec<u8>>);

/// The durable description of one multi-key write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryIntent {
    /// Stable identity, chosen by the caller and unique within the journal.
    pub txid: [u8; 16],
    /// The exact prior value of every key the write touches (written keys and guards).
    pub expected: Vec<Entry>,
    /// The exact values the write leaves (`None` deletes); every key here is also in `expected`.
    pub intended: Vec<Entry>,
}

/// What the journal records about one intent or lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    NotApplied(RecoveryIntent),
    Applied(RecoveryIntent),
    Partial(RecoveryIntent),
    Foreign(RecoveryIntent),
    Torn(RecoveryIntent),
    Corrupt { key: Vec<u8> },
    OrphanLock { key: Vec<u8> },
}

/// Why the journal refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    #[error("the journal prefix must be non-empty")]
    InvalidPrefix,
    #[error("an intent with this id, or one covering one of its keys, is already journaled")]
    Overlap,
    #[error(
        "an intent must change at least one key, expect every key it writes, name no key twice, \
         stay out of the journal's own prefix, and fit one compare-and-write"
    )]
    InvalidIntent,
    #[error("no such intent")]
    UnknownIntent,
    #[error("the intent is not (or no longer) in the state this retirement requires")]
    WrongState,
    #[error("a corrupt intent exists; orphan locks may belong to it and are not cleared")]
    CorruptIntentPresent,
    #[error("{0} unresolved recovery record(s); authoritative access stays closed until resolved")]
    RecoveryRequired(usize),
}

/// Proof that the journal held no intent and no lock when it was checked. Only
/// [`RecoveryJournal::admit`] constructs one; it borrows the journal's lifetime.
#[derive(Debug)]
pub struct Admission<'j> {
    _journal: PhantomData<&'j ()>,
}

/// A journal under one key prefix of one store.
pub struct RecoveryJournal<'a> {
    store: &'a dyn Store,
    prefix: Vec<u8>,
}

/// What classification read, so retirement can expect exactly that.
struct Classified {
    outcome: Outcome,
    intent_bytes: Vec<u8>,
    observed: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    locks_present: Vec<Vec<u8>>,
}

impl<'a> RecoveryJournal<'a> {
    /// A journal under `prefix`, which the caller reserves for it. The prefix must be non-empty.
    pub fn new(store: &'a dyn Store, prefix: &[u8]) -> Result<Self, RecoveryError> {
        if prefix.is_empty() {
            return Err(RecoveryError::InvalidPrefix);
        }
        Ok(Self {
            store,
            prefix: prefix.to_vec(),
        })
    }

    fn sub(&self, space: u8) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.push(space);
        k
    }

    fn intent_key(&self, txid: &[u8; 16]) -> Vec<u8> {
        let mut k = self.sub(INTENT_SUBSPACE);
        k.extend_from_slice(txid);
        k
    }

    fn lock_key(&self, key: &[u8]) -> Vec<u8> {
        let mut k = self.sub(LOCK_SUBSPACE);
        // Keys are bounded by `validate`, so the length fits u32.
        k.extend_from_slice(&(key.len() as u32).to_be_bytes());
        k.extend_from_slice(key);
        k
    }

    fn validate(&self, intent: &RecoveryIntent) -> Result<(), RecoveryError> {
        let distinct = |entries: &[Entry]| {
            let mut keys: Vec<&Vec<u8>> = entries.iter().map(|(k, _)| k).collect();
            keys.sort();
            keys.windows(2).all(|w| w[0] != w[1])
        };
        let prior = |k: &Vec<u8>| intent.expected.iter().find(|(ek, _)| ek == k);
        let covered = intent.intended.iter().all(|(k, _)| prior(k).is_some());
        let changes = intent
            .intended
            .iter()
            .any(|(k, v)| prior(k).is_some_and(|(_, p)| p != v));
        let outside_journal = intent
            .expected
            .iter()
            .chain(intent.intended.iter())
            .all(|(k, _)| !k.starts_with(&self.prefix));
        let framable = intent
            .expected
            .iter()
            .chain(intent.intended.iter())
            .all(|(k, v)| {
                u32::try_from(k.len()).is_ok()
                    && v.as_ref().is_none_or(|v| u32::try_from(v.len()).is_ok())
            });
        // apply and retire expect the intent, every lock and every key: 2n + 1 entries.
        let fits = intent
            .expected
            .len()
            .checked_mul(2)
            .and_then(|x| x.checked_add(1))
            .is_some_and(|n| n <= MAX_COMPARE_AND_WRITE_ENTRIES);
        if intent.intended.is_empty()
            || !covered
            || !changes
            || !distinct(&intent.expected)
            || !distinct(&intent.intended)
            || !outside_journal
            || !framable
            || !fits
        {
            return Err(RecoveryError::InvalidIntent);
        }
        Ok(())
    }

    /// Journal `intent` and lock its keys, durably. `Ok` means the intent is on disk.
    pub fn begin(&self, intent: &RecoveryIntent) -> anyhow::Result<()> {
        self.validate(intent)?;
        let ikey = self.intent_key(&intent.txid);
        let bytes = encode(intent);
        let locks: Vec<Vec<u8>> = intent
            .expected
            .iter()
            .map(|(k, _)| self.lock_key(k))
            .collect();
        let mut expect = vec![Expectation {
            key: &ikey,
            value: None,
        }];
        let mut writes = vec![Mutation {
            key: &ikey,
            value: Some(&bytes),
        }];
        for l in &locks {
            expect.push(Expectation {
                key: l,
                value: None,
            });
            writes.push(Mutation {
                key: l,
                value: Some(&intent.txid),
            });
        }
        match self.store.compare_and_write(&expect, &writes)? {
            CompareAndWrite::Applied => self.store.flush(),
            CompareAndWrite::Conflict { .. } => Err(RecoveryError::Overlap.into()),
        }
    }

    /// Run the journaled write while its whole intent (record and every lock) is on record,
    /// durably. `Ok(Conflict)` means nothing was written.
    pub fn apply(&self, intent: &RecoveryIntent) -> anyhow::Result<CompareAndWrite> {
        self.validate(intent)?;
        let ikey = self.intent_key(&intent.txid);
        let bytes = encode(intent);
        let locks: Vec<Vec<u8>> = intent
            .expected
            .iter()
            .map(|(k, _)| self.lock_key(k))
            .collect();
        let mut expect = vec![Expectation {
            key: &ikey,
            value: Some(&bytes),
        }];
        for l in &locks {
            expect.push(Expectation {
                key: l,
                value: Some(&intent.txid),
            });
        }
        expect.extend(intent.expected.iter().map(|(k, v)| Expectation {
            key: k,
            value: v.as_deref(),
        }));
        let writes: Vec<Mutation<'_>> = intent
            .intended
            .iter()
            .map(|(k, v)| Mutation {
                key: k,
                value: v.as_deref(),
            })
            .collect();
        let outcome = self.store.compare_and_write(&expect, &writes)?;
        if outcome == CompareAndWrite::Applied {
            self.store.flush()?;
        }
        Ok(outcome)
    }

    /// Retire an intent whose write is fully applied.
    pub fn complete(&self, txid: &[u8; 16]) -> anyhow::Result<()> {
        self.retire(txid, |o| matches!(o, Outcome::Applied(_)))
    }

    /// Retire an intent whose write never landed.
    pub fn abandon(&self, txid: &[u8; 16]) -> anyhow::Result<()> {
        self.retire(txid, |o| matches!(o, Outcome::NotApplied(_)))
    }

    /// Retire an intent after the caller has reconciled a `Partial` or `Foreign` outcome under its
    /// own authority. Only journal records are removed; no authoritative key is written.
    pub fn retire_reconciled(&self, txid: &[u8; 16]) -> anyhow::Result<()> {
        self.retire(txid, |o| {
            matches!(o, Outcome::Partial(_) | Outcome::Foreign(_))
        })
    }

    /// Retire a `Torn` intent (an interrupted `begin` or retirement) after the caller has
    /// inspected it. Its write cannot have landed through `apply`, which requires every lock.
    pub fn retire_torn(&self, txid: &[u8; 16]) -> anyhow::Result<()> {
        self.retire(txid, |o| matches!(o, Outcome::Torn(_)))
    }

    fn retire(&self, txid: &[u8; 16], allowed: impl Fn(&Outcome) -> bool) -> anyhow::Result<()> {
        let ikey = self.intent_key(txid);
        let bytes = self.store.get(&ikey)?.ok_or(RecoveryError::UnknownIntent)?;
        let c = self.classify_one(&ikey, &bytes)?;
        if !allowed(&c.outcome) {
            return Err(RecoveryError::WrongState.into());
        }
        // The state this retirement relies on must be durable before its record can disappear.
        self.store.flush()?;
        let mut expect = vec![Expectation {
            key: &ikey,
            value: Some(&c.intent_bytes),
        }];
        let mut writes = vec![Mutation {
            key: &ikey,
            value: None,
        }];
        for l in &c.locks_present {
            expect.push(Expectation {
                key: l,
                value: Some(txid),
            });
            writes.push(Mutation {
                key: l,
                value: None,
            });
        }
        // Retire only the state that was classified: a write landing in between conflicts.
        expect.extend(c.observed.iter().map(|(k, v)| Expectation {
            key: k,
            value: v.as_deref(),
        }));
        match self.store.compare_and_write(&expect, &writes)? {
            CompareAndWrite::Applied => self.store.flush(),
            CompareAndWrite::Conflict { .. } => Err(RecoveryError::WrongState.into()),
        }
    }

    fn classify_one(&self, key: &[u8], bytes: &[u8]) -> anyhow::Result<Classified> {
        let corrupt = || Classified {
            outcome: Outcome::Corrupt { key: key.to_vec() },
            intent_bytes: bytes.to_vec(),
            observed: Vec::new(),
            locks_present: Vec::new(),
        };
        let intent = match decode(bytes) {
            Some(i) if self.intent_key(&i.txid) == key => i,
            _ => return Ok(corrupt()),
        };
        let mut locks_present = Vec::new();
        let mut torn = false;
        for (k, _) in &intent.expected {
            let lk = self.lock_key(k);
            match self.store.get(&lk)? {
                Some(v) if v.as_slice() == intent.txid => locks_present.push(lk),
                _ => torn = true,
            }
        }
        let mut observed = Vec::with_capacity(intent.expected.len());
        let (mut any_before_only, mut any_after_only, mut any_neither) = (false, false, false);
        for (k, before) in &intent.expected {
            let current = self.store.get(k)?;
            let after = intent
                .intended
                .iter()
                .find(|(ik, _)| ik == k)
                .map(|(_, v)| v);
            let is_before = current.as_ref() == before.as_ref();
            let is_after = after.is_some_and(|a| current.as_ref() == a.as_ref());
            match (is_before, is_after) {
                (false, false) => any_neither = true,
                (true, false) if after.is_some() => any_before_only = true,
                (false, true) => any_after_only = true,
                _ => {} // guard unchanged, or a key whose prior equals its intended value
            }
            observed.push((k.clone(), current));
        }
        let outcome = if torn {
            Outcome::Torn(intent)
        } else if any_neither {
            Outcome::Foreign(intent)
        } else if any_before_only && any_after_only {
            Outcome::Partial(intent)
        } else if any_after_only {
            Outcome::Applied(intent)
        } else {
            Outcome::NotApplied(intent)
        };
        Ok(Classified {
            outcome,
            intent_bytes: bytes.to_vec(),
            observed,
            locks_present,
        })
    }

    /// Classify every recorded intent and every orphaned lock.
    pub fn classify(&self) -> anyhow::Result<Vec<Outcome>> {
        let (outcomes, _) = self.classify_all()?;
        Ok(outcomes)
    }

    #[allow(clippy::type_complexity)]
    fn classify_all(&self) -> anyhow::Result<(Vec<Outcome>, Vec<(Vec<u8>, Vec<u8>)>)> {
        let mut out = Vec::new();
        let mut live_txids = Vec::new();
        for (key, bytes) in self.store.scan(&self.sub(INTENT_SUBSPACE))? {
            let c = self.classify_one(&key, &bytes)?;
            if let Some(i) = decode(&bytes) {
                live_txids.push(i.txid);
            }
            out.push(c.outcome);
        }
        let mut orphans = Vec::new();
        for (key, value) in self.store.scan(&self.sub(LOCK_SUBSPACE))? {
            let owner: Option<[u8; 16]> = value.as_slice().try_into().ok();
            if !owner.is_some_and(|t| live_txids.contains(&t)) {
                out.push(Outcome::OrphanLock { key: key.clone() });
                orphans.push((key, value));
            }
        }
        Ok((out, orphans))
    }

    /// Remove locks that belong to no intent. Refused while any intent is corrupt, because a
    /// corrupt intent's own locks look orphaned.
    pub fn clear_orphan_locks(&self) -> anyhow::Result<usize> {
        let (outcomes, orphans) = self.classify_all()?;
        if outcomes
            .iter()
            .any(|o| matches!(o, Outcome::Corrupt { .. }))
        {
            return Err(RecoveryError::CorruptIntentPresent.into());
        }
        let mut n = 0;
        for chunk in orphans.chunks(MAX_COMPARE_AND_WRITE_ENTRIES) {
            let expect: Vec<Expectation<'_>> = chunk
                .iter()
                .map(|(k, v)| Expectation {
                    key: k,
                    value: Some(v),
                })
                .collect();
            let writes: Vec<Mutation<'_>> = chunk
                .iter()
                .map(|(k, _)| Mutation {
                    key: k,
                    value: None,
                })
                .collect();
            match self.store.compare_and_write(&expect, &writes)? {
                CompareAndWrite::Applied => n += chunk.len(),
                CompareAndWrite::Conflict { .. } => return Err(RecoveryError::WrongState.into()),
            }
        }
        self.store.flush()?;
        Ok(n)
    }

    /// Admit authoritative access only if no intent and no lock remains.
    pub fn admit(&self) -> anyhow::Result<Admission<'a>> {
        let n = self.store.scan_count(&self.sub(INTENT_SUBSPACE))?
            + self.store.scan_count(&self.sub(LOCK_SUBSPACE))?;
        if n > 0 {
            return Err(RecoveryError::RecoveryRequired(n).into());
        }
        Ok(Admission {
            _journal: PhantomData,
        })
    }
}

fn lp(out: &mut Vec<u8>, b: &[u8]) {
    // Callers pass validated (u32-framable) keys/values or the fixed tag.
    out.extend_from_slice(&(b.len() as u32).to_be_bytes());
    out.extend_from_slice(b);
}

fn put_entries(out: &mut Vec<u8>, entries: &[Entry]) {
    out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for (k, v) in entries {
        lp(out, k);
        match v {
            Some(v) => {
                out.push(1);
                lp(out, v);
            }
            None => out.push(0),
        }
    }
}

/// `lp(tag) ‖ u16be(1) ‖ txid ‖ entries(expected) ‖ entries(intended) ‖ SHA-256(all preceding)`.
pub fn encode(intent: &RecoveryIntent) -> Vec<u8> {
    let mut out = Vec::new();
    lp(&mut out, INTENT_TAG);
    out.extend_from_slice(&INTENT_VERSION.to_be_bytes());
    out.extend_from_slice(&intent.txid);
    put_entries(&mut out, &intent.expected);
    put_entries(&mut out, &intent.intended);
    let digest = Sha256::digest(&out);
    out.extend_from_slice(&digest);
    out
}

/// Strict decode; `None` on any malformation or digest mismatch.
pub fn decode(bytes: &[u8]) -> Option<RecoveryIntent> {
    let body_len = bytes.len().checked_sub(32)?;
    let (body, digest) = bytes.split_at(body_len);
    if Sha256::digest(body).as_slice() != digest {
        return None;
    }
    let mut pos = 0usize;
    let take = |pos: &mut usize, n: usize| -> Option<&[u8]> {
        let end = pos.checked_add(n).filter(|e| *e <= body.len())?;
        let s = &body[*pos..end];
        *pos = end;
        Some(s)
    };
    let u32_at = |pos: &mut usize| -> Option<usize> {
        let b = take(pos, 4)?;
        Some(u32::from_be_bytes(b.try_into().ok()?) as usize)
    };
    let tag_len = u32_at(&mut pos)?;
    if take(&mut pos, tag_len)? != INTENT_TAG {
        return None;
    }
    if take(&mut pos, 2)? != INTENT_VERSION.to_be_bytes() {
        return None;
    }
    let txid: [u8; 16] = take(&mut pos, 16)?.try_into().ok()?;
    let entries = |pos: &mut usize| -> Option<Vec<Entry>> {
        let n = u32_at(pos)?;
        if n > MAX_COMPARE_AND_WRITE_ENTRIES {
            return None;
        }
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            let kl = u32_at(pos)?;
            let k = take(pos, kl)?.to_vec();
            let val = match take(pos, 1)?[0] {
                0 => None,
                1 => {
                    let vl = u32_at(pos)?;
                    Some(take(pos, vl)?.to_vec())
                }
                _ => return None,
            };
            v.push((k, val));
        }
        Some(v)
    };
    let expected = entries(&mut pos)?;
    let intended = entries(&mut pos)?;
    if pos != body.len() {
        return None;
    }
    Some(RecoveryIntent {
        txid,
        expected,
        intended,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    //! Crash boundaries are simulated: applied states come from writing through the store,
    //! partial/foreign/torn states from writing directly, restarts from reopening the same
    //! directory, and races from a deterministic hook. None of this reproduces sled's own
    //! non-latching partial commit; it exercises the journal's handling of the states such a
    //! failure can leave.

    use super::*;
    use crate::{CompareAndPut, SledStore};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    const P: &[u8] = b"journal:";

    type Hook = Box<dyn FnOnce(&SledStore) + Send>;

    fn intent(txid: u8) -> RecoveryIntent {
        RecoveryIntent {
            txid: [txid; 16],
            expected: vec![
                (b"a".to_vec(), Some(b"a0".to_vec())),
                (b"b".to_vec(), None),
                (b"guard".to_vec(), Some(b"g".to_vec())),
            ],
            intended: vec![
                (b"a".to_vec(), Some(b"a1".to_vec())),
                (b"b".to_vec(), Some(b"b1".to_vec())),
            ],
        }
    }

    fn other_keys(txid: u8) -> RecoveryIntent {
        RecoveryIntent {
            txid: [txid; 16],
            expected: vec![(b"x".to_vec(), None)],
            intended: vec![(b"x".to_vec(), Some(b"x1".to_vec()))],
        }
    }

    fn seed(s: &dyn Store) {
        s.put(b"a", b"a0").unwrap();
        s.put(b"guard", b"g").unwrap();
    }

    fn j(s: &dyn Store) -> RecoveryJournal<'_> {
        RecoveryJournal::new(s, P).unwrap()
    }

    fn one(j: &RecoveryJournal<'_>) -> Outcome {
        let mut v = j.classify().unwrap();
        assert_eq!(v.len(), 1, "{v:?}");
        v.remove(0)
    }

    fn err(e: anyhow::Error) -> RecoveryError {
        e.downcast::<RecoveryError>().expect("typed")
    }

    #[test]
    fn the_happy_path_leaves_nothing_and_admits() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = j(&s);
        let i = intent(1);
        j.begin(&i).unwrap();
        assert_eq!(
            err(j.admit().unwrap_err()),
            RecoveryError::RecoveryRequired(4)
        );
        assert_eq!(j.apply(&i).unwrap(), CompareAndWrite::Applied);
        j.complete(&i.txid).unwrap();
        j.admit().unwrap();
        assert!(j.classify().unwrap().is_empty());
        assert_eq!(s.get(b"b").unwrap(), Some(b"b1".to_vec()));
    }

    #[test]
    fn intents_survive_restart_and_keep_admission_closed_until_resolved() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = SledStore::open(dir.path()).unwrap();
            seed(&s);
            let j = j(&s);
            j.begin(&intent(1)).unwrap();
            j.begin(&other_keys(2)).unwrap();
        }
        let s = SledStore::open(dir.path()).unwrap();
        let j = j(&s);
        assert!(j.admit().is_err());
        let got = j.classify().unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|o| matches!(o, Outcome::NotApplied(_))));
        assert_eq!(
            j.classify().unwrap().len(),
            2,
            "repeated recovery changes nothing"
        );
        j.abandon(&[2; 16]).unwrap();
        assert!(j.admit().is_err(), "one intent still open");
        assert_eq!(j.apply(&intent(1)).unwrap(), CompareAndWrite::Applied);
        j.complete(&[1; 16]).unwrap();
        j.admit().unwrap();
    }

    #[test]
    fn a_write_applied_without_acknowledgement_is_applied_after_restart_and_not_replayed() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = SledStore::open(dir.path()).unwrap();
            seed(&s);
            let j = j(&s);
            j.begin(&intent(1)).unwrap();
            j.apply(&intent(1)).unwrap();
        }
        let s = SledStore::open(dir.path()).unwrap();
        let j = j(&s);
        assert_eq!(one(&j), Outcome::Applied(intent(1)));
        assert!(matches!(
            j.apply(&intent(1)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert_eq!(
            err(j.abandon(&[1; 16]).unwrap_err()),
            RecoveryError::WrongState
        );
        assert_eq!(
            err(j.retire_reconciled(&[1; 16]).unwrap_err()),
            RecoveryError::WrongState
        );
        j.complete(&[1; 16]).unwrap();
        j.admit().unwrap();
    }

    #[test]
    fn partial_and_foreign_are_distinguished_and_only_reconciliation_retires_them() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = j(&s);
        j.begin(&intent(1)).unwrap();
        assert_eq!(
            err(j.retire_reconciled(&[1; 16]).unwrap_err()),
            RecoveryError::WrongState,
            "a NotApplied intent is not reconcilable"
        );
        s.put(b"a", b"a1").unwrap();
        assert_eq!(one(&j), Outcome::Partial(intent(1)));
        assert_eq!(
            err(j.complete(&[1; 16]).unwrap_err()),
            RecoveryError::WrongState
        );
        assert_eq!(
            err(j.abandon(&[1; 16]).unwrap_err()),
            RecoveryError::WrongState
        );
        s.put(b"b", b"someone else").unwrap();
        assert_eq!(one(&j), Outcome::Foreign(intent(1)));
        s.put(b"b", b"b1").unwrap();
        s.put(b"guard", b"moved").unwrap();
        assert_eq!(one(&j), Outcome::Foreign(intent(1)));
        assert_eq!(s.get(b"a").unwrap(), Some(b"a1".to_vec()));
        j.retire_reconciled(&[1; 16]).unwrap();
        j.admit().unwrap();
    }

    #[test]
    fn a_delete_intent_is_classified_and_completed() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = j(&s);
        let del = RecoveryIntent {
            txid: [6; 16],
            expected: vec![(b"a".to_vec(), Some(b"a0".to_vec()))],
            intended: vec![(b"a".to_vec(), None)],
        };
        j.begin(&del).unwrap();
        assert_eq!(one(&j), Outcome::NotApplied(del.clone()));
        j.apply(&del).unwrap();
        assert_eq!(one(&j), Outcome::Applied(del));
        j.complete(&[6; 16]).unwrap();
        assert_eq!(s.get(b"a").unwrap(), None);
    }

    #[test]
    fn overlapping_and_duplicate_intents_are_refused_atomically() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = j(&s);
        j.begin(&intent(1)).unwrap();
        assert_eq!(
            err(j.begin(&intent(1)).unwrap_err()),
            RecoveryError::Overlap
        );
        let mut overlapping = other_keys(3);
        overlapping
            .expected
            .push((b"a".to_vec(), Some(b"a0".to_vec())));
        assert_eq!(
            err(j.begin(&overlapping).unwrap_err()),
            RecoveryError::Overlap
        );
        j.begin(&other_keys(4)).unwrap();
    }

    #[test]
    fn concurrent_begins_over_one_key_have_one_winner_and_the_rest_overlap() {
        let s = std::sync::Arc::new(SledStore::temporary().unwrap());
        let handles: Vec<_> = (0..8u8)
            .map(|t| {
                let s = s.clone();
                std::thread::spawn(move || {
                    RecoveryJournal::new(s.as_ref(), P)
                        .unwrap()
                        .begin(&other_keys(t + 10))
                        .map_err(err)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "{results:?}"
        );
        assert!(results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| *e == RecoveryError::Overlap));
        assert_eq!(j(s.as_ref()).classify().unwrap().len(), 1);
    }

    #[test]
    fn torn_begins_and_retirements_are_reported_and_resolvable() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = j(&s);
        j.begin(&intent(1)).unwrap();
        // Intent without one of its locks: apply cannot land on it.
        s.delete(&j.lock_key(b"b")).unwrap();
        assert_eq!(one(&j), Outcome::Torn(intent(1)));
        assert!(matches!(
            j.apply(&intent(1)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert_eq!(s.get(b"b").unwrap(), None);
        j.retire_torn(&[1; 16]).unwrap();
        j.admit().unwrap();
        // Locks without an intent.
        j.begin(&intent(2)).unwrap();
        s.delete(&j.intent_key(&[2; 16])).unwrap();
        let got = j.classify().unwrap();
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(|o| matches!(o, Outcome::OrphanLock { .. })));
        assert!(j.admit().is_err());
        assert_eq!(j.clear_orphan_locks().unwrap(), 3);
        j.admit().unwrap();
    }

    #[test]
    fn a_corrupt_intent_blocks_orphan_clearing_and_admission() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = j(&s);
        j.begin(&intent(1)).unwrap();
        let key = j.intent_key(&[1; 16]);
        let mut bytes = s.get(&key).unwrap().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        s.put(&key, &bytes).unwrap();
        let got = j.classify().unwrap();
        assert!(got.contains(&Outcome::Corrupt { key: key.clone() }));
        assert_eq!(
            err(j.clear_orphan_locks().unwrap_err()),
            RecoveryError::CorruptIntentPresent
        );
        s.put(&key, &encode(&intent(2))).unwrap();
        assert!(j.classify().unwrap().contains(&Outcome::Corrupt { key }));
        assert!(j.admit().is_err());
    }

    #[test]
    fn apply_only_lands_while_its_intent_is_on_record() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = j(&s);
        assert!(matches!(
            j.apply(&intent(1)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert_eq!(s.get(b"b").unwrap(), None);
    }

    #[test]
    fn invalid_prefixes_intents_and_retirements_are_refused() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        assert!(matches!(
            RecoveryJournal::new(&s, b""),
            Err(RecoveryError::InvalidPrefix)
        ));
        let j = j(&s);
        let mut uncovered = intent(3);
        uncovered
            .intended
            .push((b"c".to_vec(), Some(b"c1".to_vec())));
        let mut empty = intent(4);
        empty.intended.clear();
        let mut dup = intent(5);
        dup.expected.push((b"a".to_vec(), None));
        let noop = RecoveryIntent {
            txid: [7; 16],
            expected: vec![(b"a".to_vec(), Some(b"a0".to_vec()))],
            intended: vec![(b"a".to_vec(), Some(b"a0".to_vec()))],
        };
        let mut inside = other_keys(8);
        inside.expected.push((b"journal:i-forged".to_vec(), None));
        inside
            .intended
            .push((b"journal:i-forged".to_vec(), Some(b"x".to_vec())));
        let mut too_big = other_keys(9);
        for n in 0..MAX_COMPARE_AND_WRITE_ENTRIES / 2 {
            too_big.expected.push((format!("k{n}").into_bytes(), None));
        }
        for bad in [uncovered, empty, dup, noop, inside, too_big] {
            assert_eq!(
                err(j.begin(&bad).unwrap_err()),
                RecoveryError::InvalidIntent
            );
        }
        assert_eq!(
            err(j.complete(&[9; 16]).unwrap_err()),
            RecoveryError::UnknownIntent
        );
    }

    /// Forwards to a real store, records mutating calls and flushes, can fail the Nth flush
    /// without faulting, and can run a one-shot hook before the next compare-and-write.
    struct Harness {
        inner: SledStore,
        log: Mutex<Vec<&'static str>>,
        fail_flush_at: AtomicUsize,
        flushes: AtomicUsize,
        hook: Mutex<Option<Hook>>,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                inner: SledStore::temporary().unwrap(),
                log: Mutex::new(Vec::new()),
                fail_flush_at: AtomicUsize::new(usize::MAX),
                flushes: AtomicUsize::new(0),
                hook: Mutex::new(None),
            }
        }
        fn take(&self) -> Vec<&'static str> {
            std::mem::take(&mut *self.log.lock().unwrap())
        }
        fn note(&self, op: &'static str) {
            self.log.lock().unwrap().push(op);
        }
    }

    impl Store for Harness {
        fn get(&self, k: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            self.inner.get(k)
        }
        fn put(&self, k: &[u8], v: &[u8]) -> anyhow::Result<()> {
            self.note("put");
            self.inner.put(k, v)
        }
        fn delete(&self, k: &[u8]) -> anyhow::Result<()> {
            self.note("delete");
            self.inner.delete(k)
        }
        fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
            self.note("delete_atomic");
            self.inner.delete_atomic(keys)
        }
        fn compare_and_put(
            &self,
            k: &[u8],
            e: Option<&[u8]>,
            n: &[u8],
        ) -> anyhow::Result<CompareAndPut> {
            self.note("compare_and_put");
            self.inner.compare_and_put(k, e, n)
        }
        fn compare_and_write(
            &self,
            e: &[Expectation<'_>],
            w: &[Mutation<'_>],
        ) -> anyhow::Result<CompareAndWrite> {
            let hook = self.hook.lock().unwrap().take();
            if let Some(h) = hook {
                h(&self.inner);
            }
            self.note("compare_and_write");
            self.inner.compare_and_write(e, w)
        }
        fn flush(&self) -> anyhow::Result<()> {
            self.note("flush");
            let n = self.flushes.fetch_add(1, Ordering::SeqCst) + 1;
            if n == self.fail_flush_at.load(Ordering::SeqCst) {
                anyhow::bail!("injected flush failure");
            }
            Store::flush(&self.inner)
        }
        fn scan(&self, p: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan(p)
        }
        fn scan_count(&self, p: &[u8]) -> anyhow::Result<usize> {
            self.inner.scan_count(p)
        }
        fn get_replica_metadata(
            &self,
            h: &crate::ContentHash,
        ) -> anyhow::Result<Option<crate::ReplicaMetadata>> {
            self.inner.get_replica_metadata(h)
        }
        fn put_replica_metadata(&self, m: &crate::ReplicaMetadata) -> anyhow::Result<()> {
            self.inner.put_replica_metadata(m)
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<crate::ContentHash>> {
            self.inner.list_replica_hashes()
        }
    }

    #[test]
    fn a_write_landing_between_classification_and_retirement_blocks_the_retirement() {
        // abandon classifies NotApplied; before its retirement CAW, the write lands (as a
        // concurrent apply would). The retirement must conflict, not retire a landed write.
        let s = Harness::new();
        seed(&s);
        let j = j(&s);
        j.begin(&intent(1)).unwrap();
        *s.hook.lock().unwrap() = Some(Box::new(|inner: &SledStore| {
            inner.put(b"a", b"a1").unwrap();
            inner.put(b"b", b"b1").unwrap();
        }));
        assert_eq!(
            err(j.abandon(&[1; 16]).unwrap_err()),
            RecoveryError::WrongState
        );
        assert_eq!(one(&j), Outcome::Applied(intent(1)), "the record survives");
        j.complete(&[1; 16]).unwrap();
    }

    #[test]
    fn each_steps_own_flush_failure_is_reported_and_retirement_keeps_its_record() {
        let s = Harness::new();
        seed(&s);
        let j = j(&s);
        // begin: its flush (#1) fails; the intent's durability is unknown, it is reported.
        s.fail_flush_at.store(1, Ordering::SeqCst);
        assert!(j.begin(&intent(1)).is_err());
        assert_eq!(one(&j), Outcome::NotApplied(intent(1)));
        // apply: its flush (#2) fails after the write.
        s.fail_flush_at.store(2, Ordering::SeqCst);
        assert!(j.apply(&intent(1)).is_err());
        assert_eq!(one(&j), Outcome::Applied(intent(1)));
        // retire: the pre-retirement flush (#3) fails, so the record is not removed.
        s.fail_flush_at.store(3, Ordering::SeqCst);
        assert!(j.complete(&[1; 16]).is_err());
        assert_eq!(one(&j), Outcome::Applied(intent(1)));
        s.fail_flush_at.store(usize::MAX, Ordering::SeqCst);
        j.complete(&[1; 16]).unwrap();
        j.admit().unwrap();
    }

    #[test]
    fn a_faulted_store_admits_nothing() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let _ = s.fault.write_single("flush", || {
            Err::<(), _>(anyhow::anyhow!("injected fsync error"))
        });
        let j = j(&s);
        assert!(j.begin(&intent(1)).is_err());
        assert!(j.admit().is_err());
    }

    #[test]
    fn each_step_is_durable_before_the_next_and_retirement_never_precedes_its_state() {
        let s = Harness::new();
        seed(&s);
        s.take();
        let j = j(&s);
        let i = intent(1);
        j.begin(&i).unwrap();
        assert_eq!(
            s.take(),
            ["compare_and_write", "flush"],
            "intent+locks durable first"
        );
        j.apply(&i).unwrap();
        assert_eq!(s.take(), ["compare_and_write", "flush"], "write durable");
        j.complete(&i.txid).unwrap();
        assert_eq!(
            s.take(),
            ["flush", "compare_and_write", "flush"],
            "state flushed before the record goes, removal durable"
        );
        j.begin(&intent(2)).unwrap();
        s.take();
        s.inner.put(b"a", b"moved").unwrap();
        assert!(matches!(
            j.apply(&intent(2)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert_eq!(
            s.take(),
            ["compare_and_write"],
            "a conflict flushes nothing"
        );
    }

    #[test]
    fn the_intent_encoding_round_trips_and_every_truncation_is_refused() {
        let i = intent(7);
        let b = encode(&i);
        assert_eq!(decode(&b), Some(i));
        for cut in 0..b.len() {
            assert_eq!(decode(&b[..cut]), None, "prefix {cut}");
        }
        let mut t = b.clone();
        t.push(0);
        assert_eq!(decode(&t), None);
    }
}
