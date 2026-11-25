//! A durable recovery journal for one multi-key write (meaning-neutral).
//!
//! [`crate::Store::compare_and_write`] is atomic on success, but an error after its commit began
//! leaves the outcome unknown, even after restart (see [`crate::fault`]). A caller that must never
//! act on unknown state wraps each such write in a [`RecoveryJournal`]:
//!
//! 1. [`RecoveryJournal::begin`] writes a [`RecoveryIntent`] (stable id, the exact expected prior
//!    values, the exact intended values, an integrity digest) with a create-if-absent, then
//!    **flushes**. Only after that flush returns `Ok` is the intent durable, and only then may the
//!    write run.
//! 2. [`RecoveryJournal::apply`] runs the write as one `compare_and_write` over the expected bytes,
//!    then flushes.
//! 3. [`RecoveryJournal::complete`] deletes the intent, then flushes.
//!
//! After a crash or an ambiguous error (which faults the store until reopen), the intent may
//! remain. On reopen, [`RecoveryJournal::classify`] reads each remaining intent and the current
//! values of its keys and reports one of:
//!
//! * [`Outcome::NotApplied`]: every key still holds its expected value. The write never happened.
//! * [`Outcome::Applied`]: every key holds its intended value. The write happened, but completion
//!   was not acknowledged.
//! * [`Outcome::Contradiction`]: anything else. Some keys were applied and others not, or a value
//!   is neither expected nor intended.
//! * [`Outcome::Corrupt`]: the intent fails its integrity check or does not decode.
//!
//! [`RecoveryJournal::require_clean`] refuses while any intent remains, so authoritative access
//! cannot resume before the caller has resolved every one. The store never chooses: replaying a
//! `NotApplied` intent ([`RecoveryJournal::apply`] is idempotent under its exact expectations),
//! finishing an `Applied` one, or reconciling a contradiction is the **caller's** decision, made
//! with whatever authority its domain requires. There is no rollback: the backend cannot provide
//! one.
//!
//! Limits: local to one process and one database; it does not detect a privileged raw edit that
//! leaves every journaled key at its expected or intended value; it relies on `flush` meaning
//! durable (sled's contract).

use sha2::{Digest, Sha256};

use crate::{CompareAndPut, CompareAndWrite, Expectation, Mutation, Store};

const INTENT_TAG: &[u8] = b"icn.store.recovery-intent.v1";
const INTENT_VERSION: u16 = 1;

/// One key's value in an intent: `None` means the key is absent.
pub type Entry = (Vec<u8>, Option<Vec<u8>>);

/// The durable description of one multi-key write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryIntent {
    /// Stable identity, chosen by the caller and unique within the journal.
    pub txid: [u8; 16],
    /// The exact values the write requires beforehand.
    pub expected: Vec<Entry>,
    /// The exact values the write leaves (`None` deletes).
    pub intended: Vec<Entry>,
}

/// What a remaining intent says about its write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No key changed: the write did not happen.
    NotApplied(RecoveryIntent),
    /// Every intended key holds its intended value: the write happened, unacknowledged.
    Applied(RecoveryIntent),
    /// A mix, or a value that is neither expected nor intended. Never resolved by the store.
    Contradiction(RecoveryIntent),
    /// The stored intent does not decode or fails its digest. `key` is its journal key.
    Corrupt { key: Vec<u8> },
}

/// Why the journal refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    #[error("an intent with this id is already journaled")]
    DuplicateTxid,
    #[error(
        "an intent must write at least one key, expect every key it writes, name no key twice, \
         and stay within bounds"
    )]
    InvalidIntent,
    #[error("{0} unresolved recovery intent(s); authoritative access stays closed until resolved")]
    RecoveryRequired(usize),
}

/// A journal under one key prefix of one store.
pub struct RecoveryJournal<'a> {
    store: &'a dyn Store,
    prefix: Vec<u8>,
}

impl<'a> RecoveryJournal<'a> {
    /// A journal whose intents live under `prefix` (which the caller reserves for it).
    pub fn new(store: &'a dyn Store, prefix: &[u8]) -> Self {
        Self {
            store,
            prefix: prefix.to_vec(),
        }
    }

    fn key(&self, txid: &[u8; 16]) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.extend_from_slice(txid);
        k
    }

    /// Make `intent` durable before its write runs. `Ok` means the intent is on disk.
    pub fn begin(&self, intent: &RecoveryIntent) -> anyhow::Result<()> {
        validate(intent)?;
        match self
            .store
            .compare_and_put(&self.key(&intent.txid), None, &encode(intent))?
        {
            CompareAndPut::Applied => {}
            CompareAndPut::Conflict { .. } => return Err(RecoveryError::DuplicateTxid.into()),
        }
        self.store.flush()
    }

    /// Run the journaled write and make it durable. `Ok(Conflict)` means nothing was written
    /// (the expectations no longer hold); the intent stays for the caller to resolve.
    pub fn apply(&self, intent: &RecoveryIntent) -> anyhow::Result<CompareAndWrite> {
        validate(intent)?;
        let expect: Vec<Expectation<'_>> = intent
            .expected
            .iter()
            .map(|(k, v)| Expectation {
                key: k,
                value: v.as_deref(),
            })
            .collect();
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

    /// Retire a resolved intent durably.
    pub fn complete(&self, txid: &[u8; 16]) -> anyhow::Result<()> {
        self.store.delete(&self.key(txid))?;
        self.store.flush()
    }

    /// Classify every remaining intent against the store's current values.
    pub fn classify(&self) -> anyhow::Result<Vec<Outcome>> {
        let mut out = Vec::new();
        for (key, bytes) in self.store.scan(&self.prefix)? {
            let intent = match decode(&bytes) {
                Some(i) if self.key(&i.txid) == key => i,
                _ => {
                    out.push(Outcome::Corrupt { key });
                    continue;
                }
            };
            // Every intended key is also expected (enforced by `validate`), so each key's prior
            // value is known. A key is "before" (its expected value), "after" (its intended
            // value), or neither. Expected-only keys are guards: they must still be "before".
            let mut none_applied = true;
            let mut all_applied = true;
            for (k, before) in &intent.expected {
                let current = self.store.get(k)?;
                let after = intent
                    .intended
                    .iter()
                    .find(|(ik, _)| ik == k)
                    .map(|(_, v)| v);
                let is_before = current.as_ref() == before.as_ref();
                let is_after = after.is_some_and(|a| current.as_ref() == a.as_ref());
                match after {
                    None => {
                        if !is_before {
                            none_applied = false;
                            all_applied = false;
                        }
                    }
                    Some(_) => {
                        if !is_before {
                            none_applied = false;
                        }
                        if !is_after {
                            all_applied = false;
                        }
                    }
                }
            }
            out.push(if none_applied {
                Outcome::NotApplied(intent)
            } else if all_applied {
                Outcome::Applied(intent)
            } else {
                Outcome::Contradiction(intent)
            });
        }
        Ok(out)
    }

    /// Refuse while any intent remains.
    pub fn require_clean(&self) -> anyhow::Result<()> {
        let n = self.store.scan_count(&self.prefix)?;
        if n > 0 {
            return Err(RecoveryError::RecoveryRequired(n).into());
        }
        Ok(())
    }
}

fn validate(intent: &RecoveryIntent) -> Result<(), RecoveryError> {
    let distinct = |entries: &[Entry]| {
        let mut keys: Vec<&Vec<u8>> = entries.iter().map(|(k, _)| k).collect();
        keys.sort();
        keys.windows(2).all(|w| w[0] != w[1])
    };
    let bounded = |entries: &[Entry]| entries.len() <= crate::MAX_COMPARE_AND_WRITE_ENTRIES;
    // Every written key must also be expected, so recovery knows its prior value.
    let covered = intent
        .intended
        .iter()
        .all(|(k, _)| intent.expected.iter().any(|(ek, _)| ek == k));
    if intent.intended.is_empty()
        || !covered
        || !distinct(&intent.expected)
        || !distinct(&intent.intended)
        || !bounded(&intent.expected)
        || !bounded(&intent.intended)
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
        if n > crate::MAX_COMPARE_AND_WRITE_ENTRIES {
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

    //! Crash boundaries are simulated: "applied" states are produced by writing through the store,
    //! "partial" states by writing a subset directly, and restarts by reopening the same directory.
    //! None of this reproduces sled's own non-latching partial commit; it exercises the journal's
    //! classification of the states such a failure can leave.

    use super::*;
    use crate::SledStore;

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

    fn seed(s: &dyn Store) {
        s.put(b"a", b"a0").unwrap();
        s.put(b"guard", b"g").unwrap();
    }

    fn one(j: &RecoveryJournal<'_>) -> Outcome {
        let mut v = j.classify().unwrap();
        assert_eq!(v.len(), 1, "{v:?}");
        v.remove(0)
    }

    #[test]
    fn the_happy_path_leaves_no_intent() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        let i = intent(1);
        j.begin(&i).unwrap();
        assert!(j.require_clean().is_err(), "an open intent closes access");
        assert_eq!(j.apply(&i).unwrap(), CompareAndWrite::Applied);
        j.complete(&i.txid).unwrap();
        j.require_clean().unwrap();
        assert!(j.classify().unwrap().is_empty());
        assert_eq!(s.get(b"b").unwrap(), Some(b"b1".to_vec()));
    }

    #[test]
    fn an_intent_survives_restart_and_keeps_access_closed() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = SledStore::open(dir.path()).unwrap();
            seed(&s);
            RecoveryJournal::new(&s, P).begin(&intent(1)).unwrap();
            // Crash before apply: drop without completing.
        }
        let s = SledStore::open(dir.path()).unwrap();
        let j = RecoveryJournal::new(&s, P);
        let e = j.require_clean().unwrap_err();
        assert_eq!(
            e.downcast_ref::<RecoveryError>(),
            Some(&RecoveryError::RecoveryRequired(1))
        );
        assert_eq!(one(&j), Outcome::NotApplied(intent(1)));
        // The caller may replay it (its expectations are exact), then retire it.
        assert_eq!(j.apply(&intent(1)).unwrap(), CompareAndWrite::Applied);
        assert_eq!(one(&j), Outcome::Applied(intent(1)));
        j.complete(&[1; 16]).unwrap();
        j.require_clean().unwrap();
    }

    #[test]
    fn a_write_applied_without_acknowledgement_is_reported_applied_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = SledStore::open(dir.path()).unwrap();
            seed(&s);
            let j = RecoveryJournal::new(&s, P);
            j.begin(&intent(1)).unwrap();
            j.apply(&intent(1)).unwrap();
            // Crash before complete.
        }
        let s = SledStore::open(dir.path()).unwrap();
        let j = RecoveryJournal::new(&s, P);
        assert_eq!(one(&j), Outcome::Applied(intent(1)));
        // Replaying it is refused by its own expectations: nothing is written twice.
        assert!(matches!(
            j.apply(&intent(1)).unwrap(),
            CompareAndWrite::Conflict { .. }
        ));
        assert!(j.require_clean().is_err());
    }

    #[test]
    fn a_partially_applied_write_is_a_contradiction_and_is_never_resolved_by_the_store() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = SledStore::open(dir.path()).unwrap();
            seed(&s);
            RecoveryJournal::new(&s, P).begin(&intent(1)).unwrap();
            // Simulated prefix: only "a" landed.
            s.put(b"a", b"a1").unwrap();
            s.flush().unwrap();
        }
        let s = SledStore::open(dir.path()).unwrap();
        let j = RecoveryJournal::new(&s, P);
        assert_eq!(one(&j), Outcome::Contradiction(intent(1)));
        // Classifying again changes nothing; access stays closed; values are untouched.
        assert_eq!(one(&j), Outcome::Contradiction(intent(1)));
        assert!(j.require_clean().is_err());
        assert_eq!(s.get(b"a").unwrap(), Some(b"a1".to_vec()));
        assert_eq!(s.get(b"b").unwrap(), None);
    }

    #[test]
    fn a_changed_guard_or_a_foreign_value_is_a_contradiction() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        j.begin(&intent(1)).unwrap();
        s.put(b"guard", b"moved").unwrap();
        assert_eq!(one(&j), Outcome::Contradiction(intent(1)));
        s.put(b"guard", b"g").unwrap();
        s.put(b"b", b"someone else").unwrap();
        assert_eq!(one(&j), Outcome::Contradiction(intent(1)));
    }

    #[test]
    fn a_corrupt_or_misfiled_intent_is_reported_corrupt() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        j.begin(&intent(1)).unwrap();
        let key = j.key(&[1; 16]);
        let mut bytes = s.get(&key).unwrap().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        s.put(&key, &bytes).unwrap();
        assert_eq!(one(&j), Outcome::Corrupt { key: key.clone() });
        // A valid intent filed under another id's key.
        s.put(&key, &encode(&intent(2))).unwrap();
        assert_eq!(one(&j), Outcome::Corrupt { key });
    }

    #[test]
    fn begin_refuses_a_duplicate_id_and_invalid_intents() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        let j = RecoveryJournal::new(&s, P);
        j.begin(&intent(1)).unwrap();
        let e = j.begin(&intent(1)).unwrap_err();
        assert_eq!(
            e.downcast_ref::<RecoveryError>(),
            Some(&RecoveryError::DuplicateTxid)
        );
        let mut uncovered = intent(3);
        uncovered
            .intended
            .push((b"c".to_vec(), Some(b"c1".to_vec())));
        let mut empty = intent(4);
        empty.intended.clear();
        let mut dup = intent(5);
        dup.expected.push((b"a".to_vec(), None));
        for bad in [uncovered, empty, dup] {
            let e = j.begin(&bad).unwrap_err();
            assert_eq!(
                e.downcast_ref::<RecoveryError>(),
                Some(&RecoveryError::InvalidIntent)
            );
        }
    }

    #[test]
    fn a_failed_begin_flush_faults_the_store_so_the_write_cannot_run() {
        let s = SledStore::temporary().unwrap();
        seed(&s);
        // Simulate the durability barrier failing: fault the domain as a failed flush would.
        let _ = s.fault.write_single("flush", || {
            Err::<(), _>(anyhow::anyhow!("injected fsync error"))
        });
        let j = RecoveryJournal::new(&s, P);
        assert!(j.begin(&intent(1)).is_err());
        assert!(j.apply(&intent(1)).is_err());
        assert!(j.require_clean().is_err(), "a faulted store admits nothing");
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
