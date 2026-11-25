//! A durable recovery journal for one multi-key write (meaning-neutral).
//!
//! [`crate::Store::compare_and_write`] is atomic on success, but an error after its commit began
//! leaves the outcome unknown, even after restart (see [`crate::fault`]). A caller that must never
//! act on unknown state brackets each such write with a [`RecoveryJournal`]:
//!
//! 1. [`RecoveryJournal::begin`]: one `compare_and_write` that writes the [`RecoveryIntent`] (stable
//!    id, the exact prior value of every key it touches, the exact intended values, an integrity
//!    digest) **and a lock entry for every touched key**, expecting all of them absent, then
//!    `flush`. Two intents can therefore never cover the same key, and a racing `begin` is an
//!    ordinary conflict. The intent is durable only once that flush returns `Ok`.
//! 2. [`RecoveryJournal::apply`]: one `compare_and_write` that expects the journaled intent bytes
//!    *and* every prior value, and writes the intended values, then `flush`. The write can land
//!    only while its intent is on record.
//! 3. [`RecoveryJournal::complete`] (after an applied write) or [`RecoveryJournal::abandon`]
//!    (after a write that never landed) re-checks the state, flushes so the state it relies on is
//!    durable, then removes the intent and its locks in one `compare_and_write`, then `flush`. An
//!    intent therefore never disappears durably before the state it describes is durable.
//!
//! The journal's own writes obey the same failure contract. An ambiguous error faults the store,
//! and after reopening a partially written intent/lock set is reported, never trusted.
//!
//! [`RecoveryJournal::classify`] reports, per intent:
//!
//! | Outcome | Meaning |
//! |---|---|
//! | [`Outcome::NotApplied`] | every key still at its prior value |
//! | [`Outcome::Applied`] | every written key at its intended value, every guard unchanged |
//! | [`Outcome::Partial`] | every key is at its prior or its intended value, but not uniformly |
//! | [`Outcome::Foreign`] | at least one key matches neither |
//! | [`Outcome::Corrupt`] | an intent that fails its digest, does not decode, is misfiled, or lacks its locks |
//! | [`Outcome::OrphanLock`] | a lock with no intent (an interrupted `begin` or retirement) |
//!
//! The store never chooses between them and offers no rollback. Replaying, finishing, abandoning
//! or reconciling is the caller's decision, under its own authority;
//! [`RecoveryJournal::retire_reconciled`] removes an intent only after the caller has done that.
//! [`RecoveryJournal::admit`] returns an [`Admission`] token, which cannot be constructed any other
//! way, only when nothing remains. An owning store can require that token, so authoritative
//! handles cannot exist before recovery is complete.
//!
//! Limits: local to one process and one database. It relies on `flush` meaning durable (sled's
//! contract). It cannot detect a privileged raw edit that leaves every key at a prior or intended
//! value.

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

/// What the journal records about one intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    NotApplied(RecoveryIntent),
    Applied(RecoveryIntent),
    Partial(RecoveryIntent),
    Foreign(RecoveryIntent),
    Corrupt { key: Vec<u8> },
    OrphanLock { key: Vec<u8> },
}

/// Why the journal refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    #[error("an intent with this id, or one covering one of its keys, is already journaled")]
    Overlap,
    #[error(
        "an intent must write at least one key, expect every key it writes, name no key twice, \
         and stay within bounds"
    )]
    InvalidIntent,
    #[error("no such intent")]
    UnknownIntent,
    #[error("the intent is not in the state this retirement requires")]
    WrongState,
    #[error("{0} unresolved recovery record(s); authoritative access stays closed until resolved")]
    RecoveryRequired(usize),
}

/// Proof that the journal held no intent and no lock when it was checked. Only
/// [`RecoveryJournal::admit`] can construct one.
#[derive(Debug)]
pub struct Admission {
    _private: (),
}

/// A journal under one key prefix of one store.
pub struct RecoveryJournal<'a> {
    store: &'a dyn Store,
    prefix: Vec<u8>,
}

impl<'a> RecoveryJournal<'a> {
    /// A journal under `prefix` (which the caller reserves for it).
    pub fn new(store: &'a dyn Store, prefix: &[u8]) -> Self {
        Self {
            store,
            prefix: prefix.to_vec(),
        }
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
        lp(&mut k, key);
        k
    }

    /// Journal `intent` and lock its keys, durably. `Ok` means the intent is on disk.
    pub fn begin(&self, intent: &RecoveryIntent) -> anyhow::Result<()> {
        validate(intent)?;
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

    /// Run the journaled write while its intent is on record, durably. `Ok(Conflict)` means
    /// nothing was written.
    pub fn apply(&self, intent: &RecoveryIntent) -> anyhow::Result<CompareAndWrite> {
        validate(intent)?;
        let ikey = self.intent_key(&intent.txid);
        let bytes = encode(intent);
        let mut expect = vec![Expectation {
            key: &ikey,
            value: Some(&bytes),
        }];
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
    /// own authority. The store does not judge that reconciliation; it only removes the record.
    pub fn retire_reconciled(&self, txid: &[u8; 16]) -> anyhow::Result<()> {
        self.retire(txid, |o| {
            matches!(o, Outcome::Partial(_) | Outcome::Foreign(_))
        })
    }

    fn retire(&self, txid: &[u8; 16], allowed: impl Fn(&Outcome) -> bool) -> anyhow::Result<()> {
        let ikey = self.intent_key(txid);
        let bytes = self.store.get(&ikey)?.ok_or(RecoveryError::UnknownIntent)?;
        let outcome = self.classify_one(&ikey, &bytes)?;
        if !allowed(&outcome) {
            return Err(RecoveryError::WrongState.into());
        }
        let intent = match outcome {
            Outcome::NotApplied(i)
            | Outcome::Applied(i)
            | Outcome::Partial(i)
            | Outcome::Foreign(i) => i,
            _ => return Err(RecoveryError::WrongState.into()),
        };
        // The state this retirement relies on must be durable before its record can disappear.
        self.store.flush()?;
        let locks: Vec<Vec<u8>> = intent
            .expected
            .iter()
            .map(|(k, _)| self.lock_key(k))
            .collect();
        let mut expect = vec![Expectation {
            key: &ikey,
            value: Some(&bytes),
        }];
        let mut writes = vec![Mutation {
            key: &ikey,
            value: None,
        }];
        for l in &locks {
            expect.push(Expectation {
                key: l,
                value: Some(txid),
            });
            writes.push(Mutation {
                key: l,
                value: None,
            });
        }
        match self.store.compare_and_write(&expect, &writes)? {
            CompareAndWrite::Applied => self.store.flush(),
            CompareAndWrite::Conflict { .. } => Err(RecoveryError::WrongState.into()),
        }
    }

    fn classify_one(&self, key: &[u8], bytes: &[u8]) -> anyhow::Result<Outcome> {
        let corrupt = || Outcome::Corrupt { key: key.to_vec() };
        let intent = match decode(bytes) {
            Some(i) if self.intent_key(&i.txid) == key => i,
            _ => return Ok(corrupt()),
        };
        // An intent without all of its locks was not written as one unit: unverifiable.
        for (k, _) in &intent.expected {
            if self.store.get(&self.lock_key(k))?.as_deref() != Some(&intent.txid[..]) {
                return Ok(corrupt());
            }
        }
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
                _ => {} // guard unchanged, or before == after
            }
        }
        Ok(if any_neither {
            Outcome::Foreign(intent)
        } else if any_before_only && any_after_only {
            Outcome::Partial(intent)
        } else if any_after_only {
            Outcome::Applied(intent)
        } else {
            // Nothing moved: not applied (including a write whose values equal its priors).
            Outcome::NotApplied(intent)
        })
    }

    /// Classify every recorded intent and every orphaned lock.
    pub fn classify(&self) -> anyhow::Result<Vec<Outcome>> {
        let mut out = Vec::new();
        let mut live_txids = Vec::new();
        for (key, bytes) in self.store.scan(&self.sub(INTENT_SUBSPACE))? {
            let o = self.classify_one(&key, &bytes)?;
            if let Some(i) = decode(&bytes) {
                live_txids.push(i.txid);
            }
            out.push(o);
        }
        for (key, value) in self.store.scan(&self.sub(LOCK_SUBSPACE))? {
            let owner: Option<[u8; 16]> = value.as_slice().try_into().ok();
            if !owner.is_some_and(|t| live_txids.contains(&t)) {
                out.push(Outcome::OrphanLock { key });
            }
        }
        Ok(out)
    }

    /// Admit authoritative access only if no intent and no lock remains.
    pub fn admit(&self) -> anyhow::Result<Admission> {
        let n = self.store.scan_count(&self.sub(INTENT_SUBSPACE))?
            + self.store.scan_count(&self.sub(LOCK_SUBSPACE))?;
        if n > 0 {
            return Err(RecoveryError::RecoveryRequired(n).into());
        }
        Ok(Admission { _private: () })
    }
}

fn validate(intent: &RecoveryIntent) -> Result<(), RecoveryError> {
    let distinct = |entries: &[Entry]| {
        let mut keys: Vec<&Vec<u8>> = entries.iter().map(|(k, _)| k).collect();
        keys.sort();
        keys.windows(2).all(|w| w[0] != w[1])
    };
    let covered = intent
        .intended
        .iter()
        .all(|(k, _)| intent.expected.iter().any(|(ek, _)| ek == k));
    // begin/retire write the intent plus one lock per expected key; apply expects the intent plus
    // every expected key. Both must fit one compare_and_write.
    if intent.intended.is_empty()
        || !covered
        || !distinct(&intent.expected)
        || !distinct(&intent.intended)
        || intent.expected.len() + 1 > MAX_COMPARE_AND_WRITE_ENTRIES
    {
        return Err(RecoveryError::InvalidIntent);
    }
    Ok(())
}

fn lp(out: &mut Vec<u8>, b: &[u8]) {
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

    //! Crash boundaries are simulated: "applied" states come from writing through the store,
    //! partial and foreign states from writing directly, and restarts from reopening the same
    //! directory. None of this reproduces sled's own non-latching partial commit; it exercises the
    //! journal's handling of the states such a failure can leave.

    use super::*;
    use crate::{CompareAndPut, SledStore};

    const P: &[u8] = b"journal:";

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
        let j = RecoveryJournal::new(&s, P);
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
            let j = RecoveryJournal::new(&s, P);
            j.begin(&intent(1)).unwrap();
            j.begin(&other_keys(2)).unwrap();
        }
        let s = SledStore::open(dir.path()).unwrap();
        let j = RecoveryJournal::new(&s, P);
        assert!(j.admit().is_err());
        let mut got = j.classify().unwrap();
        got.sort_by_key(|o| format!("{o:?}"));
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|o| matches!(o, Outcome::NotApplied(_))));
        // Repeated recovery attempts change nothing until the caller acts.
        assert_eq!(j.classify().unwrap().len(), 2);
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
            let j = RecoveryJournal::new(&s, P);
            j.begin(&intent(1)).unwrap();
            j.apply(&intent(1)).unwrap();
        }
        let s = SledStore::open(dir.path()).unwrap();
        let j = RecoveryJournal::new(&s, P);
        assert_eq!(one(&j), Outcome::Applied(intent(1)));
        assert!(matches!(
            j.apply(&intent(1)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        // Not abandonable: it landed.
        assert_eq!(
            err(j.abandon(&[1; 16]).unwrap_err()),
            RecoveryError::WrongState
        );
        j.complete(&[1; 16]).unwrap();
        j.admit().unwrap();
    }

    #[test]
    fn partial_and_foreign_are_distinguished_and_never_resolved_by_the_store() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        j.begin(&intent(1)).unwrap();
        // Simulated prefix: only "a" landed; every key is at a prior or intended value.
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
        // A value that is neither prior nor intended.
        s.put(b"b", b"someone else").unwrap();
        assert_eq!(one(&j), Outcome::Foreign(intent(1)));
        // A changed guard is foreign too.
        s.put(b"b", b"b1").unwrap();
        s.put(b"guard", b"moved").unwrap();
        assert_eq!(one(&j), Outcome::Foreign(intent(1)));
        // Values are untouched by classification; only explicit reconciliation retires it.
        assert_eq!(s.get(b"a").unwrap(), Some(b"a1".to_vec()));
        j.retire_reconciled(&[1; 16]).unwrap();
        j.admit().unwrap();
    }

    #[test]
    fn overlapping_and_duplicate_intents_are_refused_atomically() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
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
        // A disjoint intent is fine.
        j.begin(&other_keys(4)).unwrap();
    }

    #[test]
    fn concurrent_begins_over_one_key_have_exactly_one_winner() {
        let s = std::sync::Arc::new(SledStore::temporary().unwrap());
        let handles: Vec<_> = (0..8u8)
            .map(|t| {
                let s = s.clone();
                std::thread::spawn(move || {
                    RecoveryJournal::new(s.as_ref(), P)
                        .begin(&other_keys(t + 10))
                        .is_ok()
                })
            })
            .collect();
        let winners = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|w| *w)
            .count();
        assert_eq!(winners, 1);
        assert_eq!(
            RecoveryJournal::new(s.as_ref(), P)
                .classify()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn an_interrupted_begin_or_retirement_is_reported_not_trusted() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        j.begin(&intent(1)).unwrap();
        // Intent without one of its locks (a torn begin).
        s.delete(&j.lock_key(b"b")).unwrap();
        assert_eq!(
            j.classify().unwrap()[0],
            Outcome::Corrupt {
                key: j.intent_key(&[1; 16])
            }
        );
        // Locks without their intent (a torn retirement).
        s.delete(&j.intent_key(&[1; 16])).unwrap();
        let got = j.classify().unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|o| matches!(o, Outcome::OrphanLock { .. })));
        assert!(j.admit().is_err());
    }

    #[test]
    fn a_corrupt_or_misfiled_intent_is_reported_corrupt() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        j.begin(&intent(1)).unwrap();
        let key = j.intent_key(&[1; 16]);
        let mut bytes = s.get(&key).unwrap().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        s.put(&key, &bytes).unwrap();
        assert_eq!(
            j.classify().unwrap()[0],
            Outcome::Corrupt { key: key.clone() }
        );
        s.put(&key, &encode(&intent(2))).unwrap();
        assert_eq!(j.classify().unwrap()[0], Outcome::Corrupt { key });
    }

    #[test]
    fn apply_only_lands_while_its_intent_is_on_record() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        // Never journaled: refused, nothing written.
        assert!(matches!(
            j.apply(&intent(1)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert_eq!(s.get(b"b").unwrap(), None);
    }

    #[test]
    fn invalid_intents_and_unknown_retirements_are_refused() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        let mut uncovered = intent(3);
        uncovered
            .intended
            .push((b"c".to_vec(), Some(b"c1".to_vec())));
        let mut empty = intent(4);
        empty.intended.clear();
        let mut dup = intent(5);
        dup.expected.push((b"a".to_vec(), None));
        for bad in [uncovered, empty, dup] {
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

    #[test]
    fn a_failed_flush_faults_the_store_so_nothing_proceeds() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let _ = s.fault.write_single("flush", || {
            Err::<(), _>(anyhow::anyhow!("injected fsync error"))
        });
        let j = RecoveryJournal::new(&s, P);
        assert!(j.begin(&intent(1)).is_err());
        assert!(j.apply(&intent(1)).is_err());
        assert!(j.admit().is_err(), "a faulted store admits nothing");
    }

    /// Forwards to a real store and records the order of mutating calls and flushes.
    struct Recording {
        inner: SledStore,
        log: std::sync::Mutex<Vec<&'static str>>,
    }

    impl Recording {
        fn take(&self) -> Vec<&'static str> {
            std::mem::take(&mut *self.log.lock().unwrap())
        }
        fn note(&self, op: &'static str) {
            self.log.lock().unwrap().push(op);
        }
    }

    impl Store for Recording {
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
            self.note("compare_and_write");
            self.inner.compare_and_write(e, w)
        }
        fn flush(&self) -> anyhow::Result<()> {
            self.note("flush");
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
    fn each_step_is_durable_before_the_next_and_retirement_never_precedes_its_state() {
        let s = Recording {
            inner: SledStore::temporary().unwrap(),
            log: std::sync::Mutex::new(Vec::new()),
        };
        seed(&s);
        s.take();
        let j = RecoveryJournal::new(&s, P);
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
            "the state is flushed before the record goes, and its removal is durable"
        );
        j.begin(&intent(2)).unwrap();
        s.take();
        // A conflicting apply writes nothing and flushes nothing.
        s.inner.put(b"a", b"moved").unwrap();
        assert!(matches!(
            j.apply(&intent(2)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert_eq!(s.take(), ["compare_and_write"]);
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
