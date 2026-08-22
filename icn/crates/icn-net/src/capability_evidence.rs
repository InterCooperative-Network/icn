//! Live sender-capability evidence, indexed by signing key (#2644).
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
}
