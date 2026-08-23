//! Live sender evidence, indexed by signing key (#2644, #2646).
//!
//! # Why this is not a view over `peer_connections`
//!
//! The replay guard needs one question answered on every signed message: does this sender
//! *currently* prove `DURABLE_SIGNING_SEQUENCE`? The obvious place to look is
//! `peer_connections`, and it is the wrong place twice over.
//!
//! It cannot answer *currently*. Nothing removes a row when a connection ends — the connection
//! handler returns on both the application-close and the error path without touching the map,
//! and the only removal anywhere is the administrative `NetworkHandle::disconnect_peer`.
//! `NetworkHandle::restore_state` goes further and recreates rows from a snapshot at startup,
//! capability bits intact, before any Hello has happened in this process. A row is therefore a
//! record of something a peer once proved, which is a different claim from what it is proving
//! now, and treating the two as one let a peer that had rolled back keep a durable regime it no
//! longer had.
//!
//! It cannot answer *cheaply* either. One signing key has many accepted textual spellings and
//! that map is keyed by spelling, so answering by key means walking it — on a hot path, over a
//! structure with no upper bound, once per envelope. A peer that authenticates under many
//! one-off DIDs grows it permanently, and every other peer's traffic pays for that.
//!
//! # The shape that fixes both
//!
//! Evidence is created by exactly one event — a connection authenticating a DID and advertising
//! the capability — and destroyed by exactly one event: that connection going away. So it is
//! held as a **lease** whose lifetime *is* the connection's, and indexed by
//! [`SenderPrincipal`] so the lookup is one hash.
//!
//! The lease is the load-bearing part. A second index that some writer has to remember to
//! update desynchronises the first time somebody adds an exit path; a lease dropped by the
//! connection handler's own stack frame is released on every exit path there is, including the
//! ones nobody thought about. This mirrors [`crate::preauth_admission::AdmissionGuard`], which
//! is held the same way in the same struct for the same reason.
//!
//! # Reference counted, because one key can hold several connections
//!
//! A key holder may authenticate under more than one spelling of itself, and simultaneous
//! cross-dialling gives one pair two connections at once. Each is its own claim with its own
//! lease, so the registry counts them: the principal proves the capability while *any* live
//! connection is claiming it. That is the same disjunction the spelling-keyed scan computed,
//! with the currency it could not express — and it cannot be suppressed by adding a claim,
//! which matters because adding claims is exactly what a key holder can do.
//!
//! # What this deliberately does not do
//!
//! It holds no capability a connection did not advertise, caches nothing across a restart, and
//! has no removal path other than `Drop`.
//!
//! What that buys is a bound with a *cost* behind it. Cardinality here is the number of
//! connections currently claiming, and each of those is a live QUIC connection the peer has to
//! keep standing up. The structure this replaced grew with every DID ever authenticated and
//! kept the entry after the connection was gone, so growing it needed no ongoing resource at
//! all. Note this is not the #2547 bound: pre-authentication admission releases its slot the
//! moment a connection authenticates, so it limits how many connections a source may hold
//! *while anonymous*, and says nothing about how many authenticated ones it may hold.

//! # Two kinds of evidence, one lifetime rule
//!
//! Everything above is about a **boolean capability**, and its join across several live
//! connections is `any`. [`LiveCapabilityRegistry::current_pq_key`] answers a different shape
//! of question about the same principal — *which ML-DSA public key*, a **value** — and a value
//! has no join that is both non-suppressible and sound (#2646).
//!
//! `any-that-verifies` is not suppressible, but it means "any key this principal has vouched
//! for on a live connection", which quietly picks the permissive reading of a rotation the
//! protocol does not authenticate: a peer that opened a new connection to move off a
//! compromised key would still be verified against the old one. First-wins, last-wins and
//! lexical-minimum all pick an answer out of a set the protocol never ordered.
//!
//! So this half does not join. It **reports the disagreement** —
//! [`CurrentPqKey::ConflictingCurrentKeys`] — and the caller fails closed.
//!
//! What makes fail-closed affordable here, in a way it would not be for the capability bit, is
//! the authority required to reach the state at all: installing a claim for a principal
//! requires authenticating a connection with that principal's Ed25519 key (the three #2520
//! DID-TLS checks) *and* a `PqBindingProof` signed by the advertised ML-DSA key. Both are
//! pinned by `handlers::hello`'s authentication-ordering suite. So a principal's evidence can
//! only be contradicted by a party already able to authenticate as that principal, and the
//! state is recoverable — closing one of the disagreeing connections restores a unique answer.
//!
//! This is a statement about who can install *contradictory* evidence, not a claim that the
//! absence of evidence is unreachable by other means; see [`CurrentPqKey::NoCurrentKey`].
//!
//! # Why not the key advertised on *this* connection
//!
//! A signed envelope's sender is not necessarily the peer at the other end of the connection it
//! arrived on — gossip relays. Resolving against the receiving connection's own Hello would
//! verify relayed traffic against the relay's key. The index has to be node-global and keyed by
//! the sender's principal, which is what makes the lease, rather than the connection object,
//! the thing that carries currency.

use crate::replay_guard::SenderPrincipal;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Which signing keys are, right now, being claimed as durable by a live connection.
#[derive(Debug, Default)]
pub struct LiveCapabilityRegistry {
    /// How many live authenticated connections currently claim `DURABLE_SIGNING_SEQUENCE` for
    /// each key. An absent key and a zero count mean the same thing, and zero counts are
    /// removed rather than kept, so the map's size is bounded by *claiming* connections.
    ///
    /// A `std` lock rather than a `tokio` one: [`LiveCapabilityClaim::drop`] runs in a
    /// synchronous destructor and cannot await. Nothing awaits while holding it — every
    /// critical section here is a hash lookup — so it cannot be held across a yield point.
    durable_claims: RwLock<HashMap<SenderPrincipal, usize>>,

    /// Which ML-DSA public keys live authenticated connections are currently advertising for
    /// each signing key.
    ///
    /// Same lifetime rule as `durable_claims` and the same reason for it, but the inner value
    /// is a *set* rather than a count, because the question is which key rather than whether.
    /// Reference-counted per distinct key so several connections advertising the same bytes
    /// compose to one answer, and so closing one of them does not retract what the others are
    /// still saying.
    pq_keys: RwLock<HashMap<SenderPrincipal, PqKeyClaims>>,
}

/// The distinct ML-DSA keys currently advertised for one principal, each with a live-claim
/// count.
///
/// Aggregated here, at claim time, rather than recomputed per envelope: the hot path must not
/// pay for however many connections a peer happens to be holding.
#[derive(Debug, Default)]
struct PqKeyClaims {
    /// Distinct key bytes -> how many live connections are advertising exactly those bytes.
    /// An empty map cannot exist; the principal's entry is removed instead.
    by_key: HashMap<Arc<[u8]>, usize>,
}

/// What live connections currently prove about one principal's ML-DSA public key.
///
/// Deliberately three outcomes rather than `Option<Key>`. Collapsing the third into `None`
/// would turn "live connections disagree about this key" into "this peer has no PQ key", which
/// is precisely the classical-only fallback a disagreement must never reach (#2646).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentPqKey {
    /// No live authenticated connection is advertising an ML-DSA key for this principal.
    ///
    /// A peer that has gone away, a peer that never advertised one, and a legacy peer whose key
    /// was discarded for want of a `PqBindingProof` are all this, and are indistinguishable
    /// from here on purpose.
    ///
    /// What #2646 established about this state is narrow and worth stating exactly: it can no
    /// longer be *selected by textual representation* — every accepted spelling of one signing
    /// key resolves identically. It is not a claim that no party can bring about the absence of
    /// evidence by other means (disrupting the peer's connections, for instance), which this
    /// work did not examine.
    NoCurrentKey,
    /// Exactly one distinct key, advertised by one or more live connections.
    UniqueCurrentKey(Arc<[u8]>),
    /// Live connections disagree about the bytes. Never a licence to verify classically.
    ConflictingCurrentKeys,
}

impl LiveCapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Does a live connection currently claim the durable signing sequence for this key?
    ///
    /// One hash lookup, independent of how many peers have ever been seen. This is the whole
    /// reason the registry exists rather than a scan.
    pub fn proves_durable_signing_sequence(&self, principal: &SenderPrincipal) -> bool {
        self.read().get(principal).is_some_and(|count| *count > 0)
    }

    /// Record that one live connection claims the durable signing sequence for `principal`.
    ///
    /// The claim stands until the returned lease is dropped, which the connection handler does
    /// by returning. Callers must **hold** the lease for as long as the claim is true; dropping
    /// it immediately is the same as never having made it.
    #[must_use = "the claim lasts exactly as long as this lease is held"]
    pub fn claim_durable(self: &Arc<Self>, principal: SenderPrincipal) -> LiveCapabilityClaim {
        *self.write().entry(principal).or_insert(0) += 1;
        LiveCapabilityClaim {
            registry: Arc::clone(self),
            principal,
        }
    }

    /// Which ML-DSA public key, if any, live connections currently agree this principal is
    /// using (#2646).
    ///
    /// Principal-local: one hash lookup for the principal, then at most two steps into a map
    /// whose size is the number of *distinct keys its own live connections* are advertising —
    /// one, for every peer that is not disagreeing with itself. Nothing here is proportional to
    /// how many peers this node has ever seen, which is the property the spelling-keyed
    /// `peer_connections` lookup this replaced could not offer once it had to answer by key.
    pub fn current_pq_key(&self, principal: &SenderPrincipal) -> CurrentPqKey {
        let claims = self.read_pq();
        let Some(claims) = claims.get(principal) else {
            return CurrentPqKey::NoCurrentKey;
        };
        let mut keys = claims.by_key.keys();
        match (keys.next(), keys.next()) {
            // Unreachable by construction — an entry with no keys is removed rather than kept —
            // but written as the safe answer rather than as an `unreachable!`, because a panic
            // on a protocol path is not an option and "no key" is what an empty set means.
            (None, _) => CurrentPqKey::NoCurrentKey,
            (Some(only), None) => CurrentPqKey::UniqueCurrentKey(Arc::clone(only)),
            (Some(_), Some(_)) => CurrentPqKey::ConflictingCurrentKeys,
        }
    }

    /// Record that one live authenticated connection advertises `key` for `principal`.
    ///
    /// Call this only from the Hello path and only after the advertised key has passed every
    /// check that makes it *this principal's* key: the three #2520 DID-TLS facts, and a
    /// `PqBindingProof` signed by the advertised key over this DID. Installing a claim before
    /// those hold would let a connection name a principal it cannot authenticate as, and choose
    /// the key that principal's traffic is verified against.
    ///
    /// The claim stands until the returned lease is dropped. The registry does not validate the
    /// bytes; the ingestion boundary already refuses a key that does not parse, and this type's
    /// job is lifetime, not format.
    #[must_use = "the claim lasts exactly as long as this lease is held"]
    pub fn claim_pq_key(
        self: &Arc<Self>,
        principal: SenderPrincipal,
        key: Arc<[u8]>,
    ) -> LivePqKeyClaim {
        *self
            .write_pq()
            .entry(principal)
            .or_default()
            .by_key
            .entry(Arc::clone(&key))
            .or_insert(0) += 1;
        LivePqKeyClaim {
            registry: Arc::clone(self),
            principal,
            key,
        }
    }

    /// How many principals currently have any ML-DSA claim — the PQ map's cardinality.
    ///
    /// Same purpose as [`Self::claimed_keys`]: lets tests assert the bound, that connections
    /// which have gone away leave nothing behind. Not a production signal.
    #[cfg(test)]
    pub(crate) fn pq_claimed_principals(&self) -> usize {
        self.read_pq().len()
    }

    /// How many distinct keys are currently claimed — the map's cardinality.
    ///
    /// Exists so tests can assert the bound this type is for: that the structure does not grow
    /// with peers that have gone away. Not a production signal.
    #[cfg(test)]
    pub(crate) fn claimed_keys(&self) -> usize {
        self.read().len()
    }

    /// Poisoning is recovered from rather than propagated, following
    /// `PreAuthAdmission::lock_state`: a panic somewhere else must not turn every subsequent
    /// signed message into a panic, and the invariant this map holds is a count that
    /// [`LiveCapabilityClaim::drop`] repairs on the way out anyway.
    fn read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<SenderPrincipal, usize>> {
        self.durable_claims
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<SenderPrincipal, usize>> {
        self.durable_claims
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Poisoning recovered from for the same reason as `read`: a panic elsewhere must not turn
    /// every subsequent hybrid envelope into a panic on a protocol path.
    fn read_pq(&self) -> std::sync::RwLockReadGuard<'_, HashMap<SenderPrincipal, PqKeyClaims>> {
        self.pq_keys
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_pq(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<SenderPrincipal, PqKeyClaims>> {
        self.pq_keys
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// One connection's live claim that its authenticated key has a durable signing sequence.
///
/// Releasing on `Drop` is what makes "live" mean live: the connection handler owns this through
/// its [`crate::handlers::ConnectionContext`], so normal close, peer error, and any early
/// return all release it without a release call existing anywhere to be forgotten.
#[derive(Debug)]
pub struct LiveCapabilityClaim {
    registry: Arc<LiveCapabilityRegistry>,
    principal: SenderPrincipal,
}

impl LiveCapabilityClaim {
    /// The key this claim is about.
    ///
    /// Lets a caller decide whether a fresh Hello on the same connection is re-stating the
    /// claim it already holds or replacing it with a different key's.
    pub fn principal(&self) -> &SenderPrincipal {
        &self.principal
    }
}

impl Drop for LiveCapabilityClaim {
    fn drop(&mut self) {
        let mut claims = self.registry.write();
        if let Some(count) = claims.get_mut(&self.principal) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                // The entry's only purpose was to carry a non-zero count. Removing it here is
                // what bounds the map by *claiming* connections rather than by keys ever seen —
                // the unbounded growth this type replaced.
                claims.remove(&self.principal);
            }
        }
    }
}

/// One connection's live claim that its authenticated principal is using this ML-DSA key.
///
/// Released on `Drop`, exactly like [`LiveCapabilityClaim`] and for the same reason: the
/// connection handler owns it through its [`crate::handlers::ConnectionContext`], so normal
/// close, remote close, stream EOF, transport error, handler error, authentication failure and
/// task cancellation all release it without any exit path having to remember to.
#[derive(Debug)]
pub struct LivePqKeyClaim {
    registry: Arc<LiveCapabilityRegistry>,
    principal: SenderPrincipal,
    key: Arc<[u8]>,
}

impl Drop for LivePqKeyClaim {
    fn drop(&mut self) {
        use std::collections::hash_map::Entry;

        let mut all = self.registry.write_pq();
        let Entry::Occupied(mut principal_entry) = all.entry(self.principal) else {
            return;
        };
        if let Entry::Occupied(mut key_entry) = principal_entry
            .get_mut()
            .by_key
            .entry(Arc::clone(&self.key))
        {
            let count = key_entry.get_mut();
            *count = count.saturating_sub(1);
            if *count == 0 {
                // Removing the *key* rather than the principal is what makes a conflict
                // resolve back to a unique answer when one of two disagreeing connections
                // closes, instead of the principal keeping a phantom second key forever.
                key_entry.remove();
            }
        }
        if principal_entry.get().by_key.is_empty() {
            // And removing the principal is what bounds the map by live claims, so a peer that
            // has gone away leaves nothing to be read as current.
            principal_entry.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::{Did, KeyPair};

    fn principal_of(keypair: &KeyPair) -> SenderPrincipal {
        SenderPrincipal::from_did(keypair.did()).expect("a generated DID decodes")
    }

    /// The base16-lower spelling of the same key: a different string, the same principal.
    fn alias_of(did: &Did) -> Did {
        let key = did.to_verifying_key().expect("canonical DID decodes");
        let alias = Did::from_str(&format!(
            "did:icn:{}",
            multibase::encode(multibase::Base::Base16Lower, key.as_bytes())
        ))
        .expect("base16-lower is an accepted spelling");
        assert_ne!(alias.as_str(), did.as_str(), "CONTROL: a different string");
        alias
    }

    #[test]
    fn an_unclaimed_key_proves_nothing() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        assert!(!registry.proves_durable_signing_sequence(&principal_of(&peer)));
        assert_eq!(registry.claimed_keys(), 0);
    }

    #[test]
    fn a_held_claim_proves_the_capability_and_a_dropped_one_stops() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        let claim = registry.claim_durable(principal);
        assert!(registry.proves_durable_signing_sequence(&principal));

        drop(claim);
        assert!(
            !registry.proves_durable_signing_sequence(&principal),
            "a claim outliving the connection that made it is the whole defect (#2644)"
        );
        assert_eq!(
            registry.claimed_keys(),
            0,
            "and the entry must be removed, or the map grows with every peer ever seen"
        );
    }

    /// Two connections, one key: the principal stays proved until the *last* one goes.
    ///
    /// A key holder can authenticate under several spellings of itself, and cross-dialling
    /// gives one pair two connections at once. Releasing on the first drop would let a peer
    /// cancel its own live proof by closing an unrelated connection.
    #[test]
    fn a_second_live_claim_keeps_the_key_proved_after_the_first_is_dropped() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        let first = registry.claim_durable(principal);
        let second = registry.claim_durable(principal);
        assert_eq!(registry.claimed_keys(), 1, "one key, not one per claim");

        drop(first);
        assert!(
            registry.proves_durable_signing_sequence(&principal),
            "the second connection is still claiming it"
        );
        drop(second);
        assert!(!registry.proves_durable_signing_sequence(&principal));
        assert_eq!(registry.claimed_keys(), 0);
    }

    /// Claims made under different spellings of one key are claims about one principal.
    ///
    /// This is #2640's equivalence class reaching the registry: the index is the decoded key,
    /// so which base the peer spelled its DID in cannot select a different entry.
    #[test]
    fn spellings_of_one_key_share_a_single_entry() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let canonical = principal_of(&peer);
        let aliased = SenderPrincipal::from_did(&alias_of(peer.did())).expect("alias decodes");
        assert_eq!(canonical, aliased, "CONTROL: one principal, two spellings");

        let under_alias = registry.claim_durable(aliased);
        assert!(
            registry.proves_durable_signing_sequence(&canonical),
            "a claim made under one spelling answers for the key, not for the string"
        );
        assert_eq!(registry.claimed_keys(), 1);

        drop(under_alias);
        assert!(!registry.proves_durable_signing_sequence(&canonical));
    }

    /// One key's claim says nothing about another's.
    #[test]
    fn a_claim_does_not_bleed_to_another_key() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let claiming = KeyPair::generate().unwrap();
        let other = KeyPair::generate().unwrap();

        let _claim = registry.claim_durable(principal_of(&claiming));
        assert!(!registry.proves_durable_signing_sequence(&principal_of(&other)));
    }

    /// The bound this type exists for: peers that have gone away leave nothing behind.
    ///
    /// The structure it replaced grew with every DID ever authenticated and was walked once per
    /// envelope, so a peer reconnecting under fresh one-off DIDs permanently amplified every
    /// other peer's packet-processing cost.
    #[test]
    fn churning_peers_leave_no_residue() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        for _ in 0..1_000 {
            let peer = KeyPair::generate().unwrap();
            let claim = registry.claim_durable(principal_of(&peer));
            assert_eq!(registry.claimed_keys(), 1);
            drop(claim);
        }
        assert_eq!(
            registry.claimed_keys(),
            0,
            "a thousand one-off authenticated DIDs must cost nothing once they are gone"
        );
    }
    // -----------------------------------------------------------------------------------
    // #2646 — ML-DSA key claims. A value, not a bit, so the resolution has a third outcome.
    // -----------------------------------------------------------------------------------

    fn key(byte: u8) -> Arc<[u8]> {
        Arc::from(vec![byte; 32].as_slice())
    }

    #[test]
    fn an_unclaimed_principal_has_no_current_pq_key() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        assert_eq!(
            registry.current_pq_key(&principal_of(&peer)),
            CurrentPqKey::NoCurrentKey
        );
        assert_eq!(registry.pq_claimed_principals(), 0);
    }

    #[test]
    fn a_held_pq_claim_is_the_current_key_and_a_dropped_one_is_not() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        let claim = registry.claim_pq_key(principal, key(1));
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::UniqueCurrentKey(key(1))
        );

        drop(claim);
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::NoCurrentKey,
            "a key outliving the connection advertising it is the second half of #2646"
        );
        assert_eq!(
            registry.pq_claimed_principals(),
            0,
            "and the entry must go, or the index grows with every peer ever seen"
        );
    }

    /// Two connections advertising the same bytes are one answer, and the last one out
    /// releases it.
    #[test]
    fn same_key_claims_reference_count() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        let first = registry.claim_pq_key(principal, key(1));
        let second = registry.claim_pq_key(principal, key(1));
        assert_eq!(
            registry.pq_claimed_principals(),
            1,
            "one principal, not one entry per claim"
        );

        drop(first);
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::UniqueCurrentKey(key(1)),
            "the second connection is still advertising it; dropping on the first would let a \
             peer cancel its own live evidence by closing an unrelated connection"
        );
        drop(second);
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::NoCurrentKey
        );
    }

    /// Different bytes are a disagreement, and a disagreement is its own answer.
    ///
    /// Never `NoCurrentKey`: the caller's fallback for that is classical-only verification, and
    /// "we cannot tell which key protects this sender" must not select the weakest check.
    #[test]
    fn different_keys_for_one_principal_are_a_conflict() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        let _a = registry.claim_pq_key(principal, key(1));
        let _b = registry.claim_pq_key(principal, key(2));
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::ConflictingCurrentKeys
        );
    }

    /// A conflict resolves when the disagreeing claim goes, whichever it was.
    ///
    /// Pins that `Drop` removes the *key* rather than the principal's entry: removing the
    /// entry would take the survivor with it, and keeping the key would make the conflict
    /// permanent.
    #[test]
    fn a_conflict_resolves_to_whichever_claim_survives() {
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        for (survivor, dropped) in [(1u8, 2u8), (2u8, 1u8)] {
            let registry = Arc::new(LiveCapabilityRegistry::new());
            let kept = registry.claim_pq_key(principal, key(survivor));
            let going = registry.claim_pq_key(principal, key(dropped));
            assert_eq!(
                registry.current_pq_key(&principal),
                CurrentPqKey::ConflictingCurrentKeys
            );

            drop(going);
            assert_eq!(
                registry.current_pq_key(&principal),
                CurrentPqKey::UniqueCurrentKey(key(survivor)),
                "closing the disagreeing connection must leave the survivor's key unique"
            );
            drop(kept);
            assert_eq!(registry.pq_claimed_principals(), 0);
        }
    }

    /// Three-way: two of one key and one of another is still a conflict until the odd one out
    /// goes, and dropping only one of the pair does not resolve it.
    #[test]
    fn a_conflict_needs_every_disagreeing_claim_gone() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        let a1 = registry.claim_pq_key(principal, key(1));
        let _a2 = registry.claim_pq_key(principal, key(1));
        let b = registry.claim_pq_key(principal, key(2));

        drop(a1);
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::ConflictingCurrentKeys,
            "one of the two agreeing claims going does not end the disagreement"
        );
        drop(b);
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::UniqueCurrentKey(key(1))
        );
    }

    /// Claims made under different spellings of one key are claims about one principal.
    #[test]
    fn pq_claims_are_indexed_by_key_not_by_spelling() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let canonical = principal_of(&peer);
        let aliased = SenderPrincipal::from_did(&alias_of(peer.did())).expect("alias decodes");
        assert_eq!(canonical, aliased, "CONTROL: one principal, two spellings");

        let _under_alias = registry.claim_pq_key(aliased, key(3));
        assert_eq!(
            registry.current_pq_key(&canonical),
            CurrentPqKey::UniqueCurrentKey(key(3)),
            "a claim made under one spelling answers for the key, not for the string — this is \
             the lookup #2646 is about"
        );
    }

    /// One principal's key says nothing about another's.
    #[test]
    fn a_pq_claim_does_not_bleed_to_another_principal() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let claiming = KeyPair::generate().unwrap();
        let other = KeyPair::generate().unwrap();

        let _claim = registry.claim_pq_key(principal_of(&claiming), key(4));
        assert_eq!(
            registry.current_pq_key(&principal_of(&other)),
            CurrentPqKey::NoCurrentKey
        );
    }

    /// The two evidence kinds are independent: one does not imply or suppress the other.
    #[test]
    fn durable_and_pq_evidence_are_independent() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        let peer = KeyPair::generate().unwrap();
        let principal = principal_of(&peer);

        let durable = registry.claim_durable(principal);
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::NoCurrentKey,
            "claiming a durable sequence says nothing about an ML-DSA key"
        );

        let pq = registry.claim_pq_key(principal, key(5));
        assert!(registry.proves_durable_signing_sequence(&principal));
        drop(durable);
        assert_eq!(
            registry.current_pq_key(&principal),
            CurrentPqKey::UniqueCurrentKey(key(5)),
            "and releasing one must not release the other"
        );
        drop(pq);
        assert!(!registry.proves_durable_signing_sequence(&principal));
    }

    /// The bound: peers that have gone away leave nothing for the hot path to walk.
    #[test]
    fn churning_peers_leave_no_pq_residue() {
        let registry = Arc::new(LiveCapabilityRegistry::new());
        for _ in 0..1_000 {
            let peer = KeyPair::generate().unwrap();
            let claim = registry.claim_pq_key(principal_of(&peer), key(6));
            assert_eq!(registry.pq_claimed_principals(), 1);
            drop(claim);
        }
        assert_eq!(registry.pq_claimed_principals(), 0);
    }
}
