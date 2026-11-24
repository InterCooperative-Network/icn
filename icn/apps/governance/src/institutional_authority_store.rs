//! The only typed writer of institutional authority bindings (design spec §2.2, A-form = A-2).
//! EXPERIMENTAL, unratified.
//!
//! A binding ([`InstitutionalAuthorityBindingV1`]) is written **once** per governance domain, by a
//! single create-if-absent `compare_and_put(key, None, bytes)`. That CAS is the linearization
//! point: of any number of racing establishments for one domain, exactly one is
//! [`BindingOutcome::Established`]; the others see the winner's bytes and are
//! [`BindingOutcome::AlreadyEstablished`] (identical bytes) or
//! [`AuthorityBindingError::BindingConflict`] (anything else). A stored value that is not a valid
//! binding for that exact domain is [`AuthorityBindingError::Corrupt`], never "unbound".
//!
//! There is **no update and no delete API.** Replacing a domain's authority is a new binding for a
//! new domain (ruling D), never a rewrite of this record.
//!
//! # Not proof of legitimacy
//!
//! A stored binding is local durability of a record, nothing more. Holders of the raw
//! `Arc<dyn Store>` (and `import_snapshot`, and file restore) can write any bytes under the key,
//! as `tests/institutional_authority_binding_store.rs` pins. Nothing here authorizes an action:
//! the consumer that derives an anchor from a binding (P3) re-checks it against the context's
//! inception, and the verifier that admits a binding for writing is P6's.
//!
//! # The write capability
//!
//! [`InstitutionalAuthorityStore::establish_authority_binding`] takes a
//! [`VerifiedAuthorityBindingV1`], whose field is private and which has **no constructor at
//! all** in this slice: only this module's own child `tests` module can build one (by struct
//! literal, through Rust's module privacy). So **no production path can write a binding yet**,
//! and none can appear by deleting a `cfg` attribute. The
//! verifiers that mint it (successor bundle and declared genesis, design spec §2.3/§2.4) and the
//! operator `MandateGate` check arrive with P6.
//!
//! A dependent crate can read a verified binding (this compiles):
//!
//! ```
//! use icn_governance::institutional_domain::authority_binding::InstitutionalAuthorityBindingV1;
//! use icn_governance_actor::institutional_authority_store::VerifiedAuthorityBindingV1;
//! fn read(v: &VerifiedAuthorityBindingV1) -> &InstitutionalAuthorityBindingV1 {
//!     v.binding()
//! }
//! ```
//!
//! but cannot mint one, because the field is private (this does not):
//!
//! ```compile_fail
//! use icn_governance::institutional_domain::authority_binding::InstitutionalAuthorityBindingV1;
//! use icn_governance_actor::institutional_authority_store::VerifiedAuthorityBindingV1;
//! fn mint(binding: InstitutionalAuthorityBindingV1) -> VerifiedAuthorityBindingV1 {
//!     VerifiedAuthorityBindingV1 { binding }
//! }
//! ```
//!
//! # Coexistence with `InstitutionalDomain`
//!
//! The binding key (`gov:institutional-authority:v1:` + length-framed id) is disjoint from
//! `gov:institutional-domain:<id>` and from every prefix the [`crate::state_store`] scans. A
//! binary that predates this module never names the key, so saving an
//! [`icn_governance::InstitutionalDomain`] (lenient JSON) cannot erase or alter a binding.

use std::fmt;
use std::sync::Arc;

use icn_governance::institutional_domain::authority_binding::{
    authority_binding_key, AuthorityBindingCodecError, InstitutionalAuthorityBindingV1,
};
use icn_governance::GovernanceDomainId;
use icn_store::{CompareAndPut, Store};

/// A binding a verifier has admitted for writing. The only argument
/// [`InstitutionalAuthorityStore::establish_authority_binding`] accepts.
///
/// Has no constructor in this slice; only this module's tests build one (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAuthorityBindingV1 {
    binding: InstitutionalAuthorityBindingV1,
}

impl VerifiedAuthorityBindingV1 {
    /// The admitted binding.
    pub fn binding(&self) -> &InstitutionalAuthorityBindingV1 {
        &self.binding
    }
}

/// A successful establishment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingOutcome {
    /// This call created the binding.
    Established,
    /// The identical binding was already stored (idempotent replay).
    AlreadyEstablished,
}

/// Why a stored value under a binding key is not a binding for that domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorruptBinding {
    /// The bytes do not strictly decode.
    Undecodable(AuthorityBindingCodecError),
    /// The bytes decode to a valid binding for a **different** domain (a transplanted record).
    WrongDomain {
        /// The domain the stored record names.
        stored: GovernanceDomainId,
    },
}

impl fmt::Display for CorruptBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CorruptBinding::Undecodable(e) => write!(f, "undecodable: {e}"),
            CorruptBinding::WrongDomain { stored } => {
                write!(f, "record names another domain {:?}", stored.0)
            }
        }
    }
}

/// Why a binding was not read or established. Every variant leaves the store unchanged by this
/// call, except that [`Self::Store`] after the CAS may hide a write that landed (re-read).
#[derive(Debug)]
pub enum AuthorityBindingError {
    /// The domain is already bound to a different binding. Both are surfaced; nothing is
    /// overwritten.
    BindingConflict {
        /// The domain.
        domain: GovernanceDomainId,
        /// The binding already stored.
        existing: Box<InstitutionalAuthorityBindingV1>,
        /// The binding this call proposed.
        proposed: Box<InstitutionalAuthorityBindingV1>,
    },
    /// The value stored under the domain's key is not a valid binding for that domain.
    Corrupt {
        /// The domain.
        domain: GovernanceDomainId,
        /// What is wrong with it.
        reason: CorruptBinding,
    },
    /// The domain id cannot name a binding (empty or over-long).
    InvalidDomain(AuthorityBindingCodecError),
    /// The backend failed, or cannot make the create-if-absent write atomic.
    Store(anyhow::Error),
}

impl fmt::Display for AuthorityBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthorityBindingError::BindingConflict { domain, .. } => write!(
                f,
                "governance domain {:?} is already bound to a different authority binding",
                domain.0
            ),
            AuthorityBindingError::Corrupt { domain, reason } => write!(
                f,
                "stored authority binding for governance domain {:?} is corrupt: {reason}",
                domain.0
            ),
            AuthorityBindingError::InvalidDomain(e) => {
                write!(f, "governance domain id cannot be bound: {e}")
            }
            AuthorityBindingError::Store(e) => write!(f, "authority binding store failed: {e:#}"),
        }
    }
}

impl std::error::Error for AuthorityBindingError {}

/// Typed access to institutional authority bindings over a raw store.
pub struct InstitutionalAuthorityStore {
    store: Arc<dyn Store>,
}

impl InstitutionalAuthorityStore {
    /// Wrap a store. May share the `Arc` with a [`crate::state_store::SledGovernanceStateStore`].
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// The binding stored for `domain`, if any. A stored value that is not a valid binding for
    /// exactly `domain` is [`AuthorityBindingError::Corrupt`], never `Ok(None)`.
    ///
    /// The result is a record, not evidence of legitimacy (see the module docs).
    pub fn get_authority_binding(
        &self,
        domain: &GovernanceDomainId,
    ) -> Result<Option<InstitutionalAuthorityBindingV1>, AuthorityBindingError> {
        let key = authority_binding_key(domain).map_err(AuthorityBindingError::InvalidDomain)?;
        match self.store.get(&key).map_err(AuthorityBindingError::Store)? {
            None => Ok(None),
            Some(bytes) => decode_stored(domain, &bytes).map(Some),
        }
    }

    /// Establish `verified`'s binding by a single create-if-absent CAS, then flush.
    ///
    /// - absent: written, [`BindingOutcome::Established`];
    /// - identical bytes present: [`BindingOutcome::AlreadyEstablished`];
    /// - a different valid binding present: [`AuthorityBindingError::BindingConflict`];
    /// - anything else present: [`AuthorityBindingError::Corrupt`].
    ///
    /// Never overwrites. An outcome is returned only after [`Store::flush`] succeeds, so a
    /// reported establishment survives a restart **on a backend whose flush is durable** (sled
    /// today; the trait's default `flush` is a no-op, so a backend that keeps it promises no
    /// durability). A flush failure is an error even though the write may have landed; a retry
    /// then reports [`BindingOutcome::AlreadyEstablished`].
    pub fn establish_authority_binding(
        &self,
        verified: VerifiedAuthorityBindingV1,
    ) -> Result<BindingOutcome, AuthorityBindingError> {
        let proposed = verified.binding;
        let domain = proposed.domain().clone();
        let key = authority_binding_key(&domain).map_err(AuthorityBindingError::InvalidDomain)?;
        let bytes = proposed.encode();

        let outcome = match self
            .store
            .compare_and_put(&key, None, &bytes)
            .map_err(AuthorityBindingError::Store)?
        {
            CompareAndPut::Applied => BindingOutcome::Established,
            CompareAndPut::Conflict {
                current: Some(current),
            } => {
                if current == bytes {
                    BindingOutcome::AlreadyEstablished
                } else {
                    let existing = decode_stored(&domain, &current)?;
                    return Err(AuthorityBindingError::BindingConflict {
                        domain,
                        existing: Box::new(existing),
                        proposed: Box::new(proposed),
                    });
                }
            }
            // `expected = None` conflicting with an absent key is a backend contract violation.
            CompareAndPut::Conflict { current: None } => {
                return Err(AuthorityBindingError::Store(anyhow::anyhow!(
                    "create-if-absent compare_and_put reported a conflict with an absent key"
                )));
            }
        };
        self.store.flush().map_err(AuthorityBindingError::Store)?;
        Ok(outcome)
    }
}

fn decode_stored(
    domain: &GovernanceDomainId,
    bytes: &[u8],
) -> Result<InstitutionalAuthorityBindingV1, AuthorityBindingError> {
    let binding = InstitutionalAuthorityBindingV1::decode(bytes).map_err(|e| {
        AuthorityBindingError::Corrupt {
            domain: domain.clone(),
            reason: CorruptBinding::Undecodable(e),
        }
    })?;
    if binding.domain().0.as_bytes() != domain.0.as_bytes() {
        return Err(AuthorityBindingError::Corrupt {
            domain: domain.clone(),
            reason: CorruptBinding::WrongDomain {
                stored: binding.domain().clone(),
            },
        });
    }
    Ok(binding)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::state_store::test_support::HookStore;
    use crate::state_store::{GovernanceStateStore, SledGovernanceStateStore};
    use icn_governance::bootstrap::BootstrapEntityType;
    use icn_governance::institutional_domain::authority_binding::{BindingBasisV1, ContextRef};
    use icn_governance::InstitutionalDomain;
    use icn_store::SledStore;
    use std::sync::Barrier;

    fn genesis_basis(seed: u8) -> BindingBasisV1 {
        BindingBasisV1::DeclaredGenesis {
            declaration_decision_hash: [seed; 32],
            mandate_id: [seed.wrapping_add(1) | 1; 32],
        }
    }

    fn binding(domain: &str, ctx: u8) -> InstitutionalAuthorityBindingV1 {
        InstitutionalAuthorityBindingV1::new(
            GovernanceDomainId::new(domain),
            ContextRef([ctx; 32]),
            [ctx ^ 0x55; 32],
            genesis_basis(0x11),
        )
        .unwrap()
    }

    fn verified(b: &InstitutionalAuthorityBindingV1) -> VerifiedAuthorityBindingV1 {
        // Child-module access to the private field: the only mint in this slice.
        VerifiedAuthorityBindingV1 { binding: b.clone() }
    }

    fn fixture() -> (Arc<dyn Store>, InstitutionalAuthorityStore) {
        let raw: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        (raw.clone(), InstitutionalAuthorityStore::new(raw))
    }

    fn raw_at(raw: &Arc<dyn Store>, domain: &str) -> Option<Vec<u8>> {
        raw.get(&authority_binding_key(&GovernanceDomainId::new(domain)).unwrap())
            .unwrap()
    }

    #[test]
    fn establish_then_read_back() {
        let (raw, store) = fixture();
        let b = binding("coop-a", 1);
        assert!(store.get_authority_binding(b.domain()).unwrap().is_none());
        assert_eq!(
            store.establish_authority_binding(verified(&b)).unwrap(),
            BindingOutcome::Established
        );
        assert_eq!(
            store.get_authority_binding(b.domain()).unwrap(),
            Some(b.clone())
        );
        assert_eq!(raw_at(&raw, "coop-a"), Some(b.encode()));
    }

    #[test]
    fn identical_replay_is_idempotent_and_rewrites_nothing() {
        let (raw, store) = fixture();
        let b = binding("coop-a", 1);
        store.establish_authority_binding(verified(&b)).unwrap();
        for _ in 0..3 {
            assert_eq!(
                store.establish_authority_binding(verified(&b)).unwrap(),
                BindingOutcome::AlreadyEstablished
            );
        }
        assert_eq!(raw_at(&raw, "coop-a"), Some(b.encode()));
    }

    #[test]
    fn a_different_binding_for_the_same_domain_conflicts_and_surfaces_both() {
        let (raw, store) = fixture();
        let first = binding("coop-a", 1);
        store.establish_authority_binding(verified(&first)).unwrap();

        // Every field but the domain differs in turn; each is a conflict.
        let rivals = [
            binding("coop-a", 2),
            InstitutionalAuthorityBindingV1::new(
                GovernanceDomainId::new("coop-a"),
                first.context(),
                [0x77; 32],
                first.basis().clone(),
            )
            .unwrap(),
            InstitutionalAuthorityBindingV1::new(
                GovernanceDomainId::new("coop-a"),
                first.context(),
                first.genesis_head(),
                genesis_basis(0x33),
            )
            .unwrap(),
        ];
        for rival in rivals {
            match store.establish_authority_binding(verified(&rival)) {
                Err(AuthorityBindingError::BindingConflict {
                    domain,
                    existing,
                    proposed,
                }) => {
                    assert_eq!(domain.0, "coop-a");
                    assert_eq!(*existing, first);
                    assert_eq!(*proposed, rival);
                }
                other => panic!("expected BindingConflict, got {other:?}"),
            }
            assert_eq!(
                raw_at(&raw, "coop-a"),
                Some(first.encode()),
                "never overwritten"
            );
        }
    }

    #[test]
    fn corrupt_stored_bytes_are_refused_on_read_and_write_and_left_untouched() {
        let b = binding("coop-a", 1);
        let key = authority_binding_key(b.domain()).unwrap();
        let mut truncated = b.encode();
        truncated.pop();
        let mut trailing = b.encode();
        trailing.push(0);
        let mut retagged = b.encode();
        retagged[5] ^= 0x01;
        for bad in [Vec::new(), b"{}".to_vec(), truncated, trailing, retagged] {
            let (raw, store) = fixture();
            raw.put(&key, &bad).unwrap();
            match store.get_authority_binding(b.domain()) {
                Err(AuthorityBindingError::Corrupt {
                    reason: CorruptBinding::Undecodable(_),
                    ..
                }) => {}
                other => panic!("read of {bad:?}: expected Corrupt, got {other:?}"),
            }
            match store.establish_authority_binding(verified(&b)) {
                Err(AuthorityBindingError::Corrupt { .. }) => {}
                other => panic!("write over {bad:?}: expected Corrupt, got {other:?}"),
            }
            assert_eq!(
                raw.get(&key).unwrap(),
                Some(bad),
                "corrupt value left as found"
            );
        }
    }

    #[test]
    fn a_valid_binding_for_another_domain_under_this_key_is_corrupt() {
        let (raw, store) = fixture();
        let other = binding("coop-b", 9);
        let mine = binding("coop-a", 1);
        let key = authority_binding_key(mine.domain()).unwrap();
        raw.put(&key, &other.encode()).unwrap();
        match store.get_authority_binding(mine.domain()) {
            Err(AuthorityBindingError::Corrupt {
                reason: CorruptBinding::WrongDomain { stored },
                ..
            }) => assert_eq!(stored.0, "coop-b"),
            other => panic!("expected WrongDomain, got {other:?}"),
        }
        assert!(matches!(
            store.establish_authority_binding(verified(&mine)),
            Err(AuthorityBindingError::Corrupt {
                reason: CorruptBinding::WrongDomain { .. },
                ..
            })
        ));
        assert_eq!(raw.get(&key).unwrap(), Some(other.encode()));
    }

    #[test]
    fn racing_different_bindings_yield_exactly_one_establishment() {
        const N: usize = 12;
        let (raw, store) = fixture();
        let store = Arc::new(store);
        let barrier = Arc::new(Barrier::new(N));
        let handles: Vec<_> = (0..N)
            .map(|i| {
                let store = store.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let b = binding("coop-race", u8::try_from(i + 1).unwrap());
                    barrier.wait();
                    (b.clone(), store.establish_authority_binding(verified(&b)))
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners: Vec<_> = results
            .iter()
            .filter(|(_, r)| matches!(r, Ok(BindingOutcome::Established)))
            .map(|(b, _)| b.clone())
            .collect();
        assert_eq!(winners.len(), 1, "exactly one Established");
        let winner = &winners[0];
        for (b, r) in &results {
            if b == winner {
                continue;
            }
            match r {
                Err(AuthorityBindingError::BindingConflict { existing, .. }) => {
                    assert_eq!(**existing, *winner)
                }
                other => panic!("loser: expected BindingConflict, got {other:?}"),
            }
        }
        assert_eq!(raw_at(&raw, "coop-race"), Some(winner.encode()));
    }

    #[test]
    fn racing_identical_bindings_yield_one_established_and_the_rest_idempotent() {
        const N: usize = 12;
        let (raw, store) = fixture();
        let store = Arc::new(store);
        let barrier = Arc::new(Barrier::new(N));
        let b = binding("coop-race", 3);
        let handles: Vec<_> = (0..N)
            .map(|_| {
                let (store, barrier, b) = (store.clone(), barrier.clone(), b.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    store.establish_authority_binding(verified(&b)).unwrap()
                })
            })
            .collect();
        let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let established = outcomes
            .iter()
            .filter(|o| **o == BindingOutcome::Established)
            .count();
        assert_eq!(established, 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|o| **o == BindingOutcome::AlreadyEstablished)
                .count(),
            N - 1
        );
        assert_eq!(raw_at(&raw, "coop-race"), Some(b.encode()));
    }

    #[test]
    fn a_binding_survives_restart_and_stays_set_once() {
        let dir = tempfile::tempdir().unwrap();
        let b = binding("coop-a", 1);
        {
            let raw: Arc<dyn Store> = Arc::new(SledStore::open(dir.path()).unwrap());
            let store = InstitutionalAuthorityStore::new(raw);
            assert_eq!(
                store.establish_authority_binding(verified(&b)).unwrap(),
                BindingOutcome::Established
            );
        }
        let raw: Arc<dyn Store> = Arc::new(SledStore::open(dir.path()).unwrap());
        let store = InstitutionalAuthorityStore::new(raw);
        assert_eq!(
            store.get_authority_binding(b.domain()).unwrap(),
            Some(b.clone())
        );
        assert_eq!(
            store.establish_authority_binding(verified(&b)).unwrap(),
            BindingOutcome::AlreadyEstablished
        );
        assert!(matches!(
            store.establish_authority_binding(verified(&binding("coop-a", 2))),
            Err(AuthorityBindingError::BindingConflict { .. })
        ));
    }

    /// Older-binary simulation: code that knows only `InstitutionalDomain` (lenient JSON) loads and
    /// re-saves the domain, before and after the binding exists. The binding is untouched, and
    /// establishing the binding does not alter the domain's stored JSON.
    #[test]
    fn institutional_domain_saves_neither_erase_nor_see_the_binding() {
        let (raw, store) = fixture();
        let legacy = SledGovernanceStateStore::new(raw.clone());
        let id = GovernanceDomainId::new("coop-a");
        let domain_key = format!("gov:institutional-domain:{}", id.0).into_bytes();

        let domain = InstitutionalDomain::declare(id.clone(), BootstrapEntityType::Cooperative);
        legacy.save_institutional_domain(&domain).unwrap();
        let json_before = raw.get(&domain_key).unwrap().unwrap();

        let b = binding("coop-a", 1);
        store.establish_authority_binding(verified(&b)).unwrap();
        assert_eq!(
            raw.get(&domain_key).unwrap().unwrap(),
            json_before,
            "establishing the binding leaves the InstitutionalDomain JSON byte-identical"
        );

        // An "older binary" round-trip: load the domain and save it back, twice.
        for _ in 0..2 {
            let loaded = legacy.get_institutional_domain(&id).unwrap().unwrap();
            legacy.save_institutional_domain(&loaded).unwrap();
        }
        assert_eq!(raw_at(&raw, "coop-a"), Some(b.encode()));
        assert_eq!(store.get_authority_binding(&id).unwrap(), Some(b));
        // The JSON carries no binding: the struct's serialized form is unchanged by this slice.
        let json = String::from_utf8(raw.get(&domain_key).unwrap().unwrap()).unwrap();
        assert!(!json.contains("authority"), "unexpected field in {json}");
    }

    #[test]
    fn a_backend_without_atomic_cas_fails_closed_and_writes_nothing() {
        let hook = Arc::new(HookStore::new(false));
        let raw: Arc<dyn Store> = hook.clone();
        let store = InstitutionalAuthorityStore::new(raw.clone());
        let b = binding("coop-a", 1);
        assert!(matches!(
            store.establish_authority_binding(verified(&b)),
            Err(AuthorityBindingError::Store(_))
        ));
        assert_eq!(raw_at(&raw, "coop-a"), None);
    }

    #[test]
    fn unicode_forms_are_distinct_domains_with_distinct_keys_and_bindings() {
        // NFC "café" (U+00E9) and NFD "café" (e + U+0301) render alike but are different
        // byte strings. Exact-byte V1 behavior: no normalization, so they are separate decision
        // spaces, each with its own key and its own independent set-once binding.
        let nfc = "caf\u{e9}";
        let nfd = "cafe\u{301}";
        assert_ne!(nfc.as_bytes(), nfd.as_bytes());
        let (raw, store) = fixture();
        let a = binding(nfc, 1);
        let b = binding(nfd, 2);
        assert_ne!(
            authority_binding_key(a.domain()).unwrap(),
            authority_binding_key(b.domain()).unwrap()
        );
        assert_eq!(
            store.establish_authority_binding(verified(&a)).unwrap(),
            BindingOutcome::Established
        );
        assert_eq!(
            store.establish_authority_binding(verified(&b)).unwrap(),
            BindingOutcome::Established,
            "the other normalization form is NOT treated as already bound"
        );
        assert_eq!(store.get_authority_binding(a.domain()).unwrap(), Some(a));
        assert_eq!(store.get_authority_binding(b.domain()).unwrap(), Some(b));
        assert_eq!(raw_at(&raw, nfc).map(|v| v.is_empty()), Some(false));
    }

    #[test]
    fn an_unbindable_domain_id_is_refused_before_touching_the_store() {
        use icn_governance::institutional_domain::authority_binding::MAX_BINDING_DOMAIN_ID_BYTES;
        let (_raw, store) = fixture();
        let long = "x".repeat(MAX_BINDING_DOMAIN_ID_BYTES + 1);
        for bad in ["", long.as_str()] {
            assert!(matches!(
                store.get_authority_binding(&GovernanceDomainId::new(bad)),
                Err(AuthorityBindingError::InvalidDomain(_))
            ));
            // The write side cannot even be reached: no binding (so no verified binding) can
            // name an unbindable id.
            assert!(InstitutionalAuthorityBindingV1::new(
                GovernanceDomainId::new(bad),
                ContextRef([1; 32]),
                [2; 32],
                genesis_basis(0x11),
            )
            .is_err());
        }
    }

    /// Wraps a sled store and records, per call, whether `flush` ran after the last applied CAS.
    struct FlushProbe {
        inner: SledStore,
        flushed_since_write: std::sync::atomic::AtomicBool,
        fail_flush: std::sync::atomic::AtomicBool,
    }

    impl Store for FlushProbe {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            self.inner.get(key)
        }
        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            self.inner.put(key, value)
        }
        fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            self.inner.delete(key)
        }
        fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
            self.inner.delete_atomic(keys)
        }
        fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan(prefix)
        }
        fn get_replica_metadata(
            &self,
            h: &icn_store::ContentHash,
        ) -> anyhow::Result<Option<icn_store::ReplicaMetadata>> {
            self.inner.get_replica_metadata(h)
        }
        fn put_replica_metadata(&self, m: &icn_store::ReplicaMetadata) -> anyhow::Result<()> {
            self.inner.put_replica_metadata(m)
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<icn_store::ContentHash>> {
            self.inner.list_replica_hashes()
        }
        fn compare_and_put(
            &self,
            key: &[u8],
            expected: Option<&[u8]>,
            new: &[u8],
        ) -> anyhow::Result<CompareAndPut> {
            let r = self.inner.compare_and_put(key, expected, new)?;
            if matches!(r, CompareAndPut::Applied) {
                self.flushed_since_write
                    .store(false, std::sync::atomic::Ordering::SeqCst);
            }
            Ok(r)
        }
        fn flush(&self) -> anyhow::Result<()> {
            if self.fail_flush.load(std::sync::atomic::Ordering::SeqCst) {
                anyhow::bail!("injected flush failure");
            }
            Store::flush(&self.inner)?;
            self.flushed_since_write
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    fn probe(fail_flush: bool) -> Arc<FlushProbe> {
        Arc::new(FlushProbe {
            inner: SledStore::temporary().unwrap(),
            flushed_since_write: std::sync::atomic::AtomicBool::new(true),
            fail_flush: std::sync::atomic::AtomicBool::new(fail_flush),
        })
    }

    #[test]
    fn establishment_is_reported_only_after_a_flush() {
        let p = probe(false);
        let store = InstitutionalAuthorityStore::new(p.clone());
        assert_eq!(
            store
                .establish_authority_binding(verified(&binding("coop-a", 1)))
                .unwrap(),
            BindingOutcome::Established
        );
        assert!(
            p.flushed_since_write
                .load(std::sync::atomic::Ordering::SeqCst),
            "Established was returned before the write was flushed"
        );
    }

    #[test]
    fn a_flush_failure_is_an_error_and_a_retry_is_idempotent() {
        let p = probe(true);
        let store = InstitutionalAuthorityStore::new(p.clone());
        let b = binding("coop-a", 1);
        assert!(matches!(
            store.establish_authority_binding(verified(&b)),
            Err(AuthorityBindingError::Store(_))
        ));
        // The CAS landed before the flush failed: the honest answer on retry (with a working
        // flush) is AlreadyEstablished, never a second Established or a conflict.
        p.fail_flush
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            store.establish_authority_binding(verified(&b)).unwrap(),
            BindingOutcome::AlreadyEstablished
        );
    }
}
