//! The durable replicated state: two grow-only sets joined by union (§9.2.1).
//!
//! Nothing here is a map whose value needs a merge rule. `Bodies` is a set of **bodies**, and
//! `Witness` is a set of `(event_id, signature)` pairs. Ed25519 admits more than one valid
//! signature over one message, so if whole signed events were the elements then two signatures
//! over one body would be two elements at one position — manufacturing duplicity that halts the
//! subject — and collapsing them would require *choosing* a signature, which is a content
//! tiebreak that destroys commutativity and idempotence.
//!
//! With bodies as elements, re-signings collapse automatically and `Witness` simply accumulates.
//! **Nothing in this module ever compares two signatures.**

use std::collections::BTreeSet;

use super::admission::{admissible, AdmissionError};
use super::body::{AuthorityBody, EventId, SubjectId};

/// A raw Ed25519 signature over a body's signature preimage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WitnessSignature([u8; 64]);

impl WitnessSignature {
    /// Wrap raw signature bytes.
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        WitnessSignature(bytes)
    }

    /// The raw signature bytes.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

/// A `(body digest, signature)` pair.
///
/// Several witnesses may name one body. That is not duplicity — it is a re-signing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Witness {
    /// The digest of the **body** that was signed.
    pub event_id: EventId,
    /// The signature.
    pub signature: WitnessSignature,
}

/// A body together with one signature over it, as received on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedAuthorityEvent {
    /// The canonical body.
    pub body: AuthorityBody,
    /// One valid signature over [`AuthorityBody::signature_preimage`].
    pub signature: WitnessSignature,
}

impl SignedAuthorityEvent {
    /// Pair a body with a signature.
    pub fn new(body: AuthorityBody, signature: WitnessSignature) -> Self {
        SignedAuthorityEvent { body, signature }
    }

    /// The witness entry this event contributes.
    pub fn witness(&self) -> Witness {
        Witness {
            event_id: self.body.event_id(),
            signature: self.signature,
        }
    }
}

/// The durable replicated state for any number of subjects.
///
/// `Bodies(σ)` and `Witness(σ)` are recovered by filtering; because the join is set union,
/// filtering by subject commutes with it, so a single global store and a per-subject store have
/// identical semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthorityStore {
    bodies: BTreeSet<AuthorityBody>,
    witnesses: BTreeSet<Witness>,
}

impl AuthorityStore {
    /// An empty store — the join identity.
    pub fn new() -> Self {
        AuthorityStore::default()
    }

    /// Admit an event, or reject it.
    ///
    /// The decision comes from [`admissible`], a free function of the body and signature only.
    /// This method is the *only* ingest path, which is what makes the state-independence property
    /// falsifiable: a future implementation that consulted `self` here would be caught by
    /// `authority_log_durable::admissibility_is_state_independent`.
    pub fn ingest(&mut self, event: &SignedAuthorityEvent) -> Result<(), AdmissionError> {
        admissible(&event.body, &event.signature)?;
        self.bodies.insert(event.body.clone());
        self.witnesses.insert(event.witness());
        Ok(())
    }

    /// Ingest a batch, returning the number admitted. Rejections are silently dropped, exactly as
    /// a replica would drop them.
    pub fn ingest_all<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a SignedAuthorityEvent>,
    ) -> usize {
        events
            .into_iter()
            .filter(|event| self.ingest(event).is_ok())
            .count()
    }

    /// The componentwise union — the join of the durable lattice.
    ///
    /// Associative, commutative, idempotent and monotone, all inherited from set union. Receipt
    /// order cannot change the result because union discards order.
    pub fn join(left: &AuthorityStore, right: &AuthorityStore) -> AuthorityStore {
        let mut out = left.clone();
        out.join_in_place(right);
        out
    }

    /// In-place [`AuthorityStore::join`].
    pub fn join_in_place(&mut self, other: &AuthorityStore) {
        self.bodies.extend(other.bodies.iter().cloned());
        self.witnesses.extend(other.witnesses.iter().copied());
    }

    /// All durably known bodies.
    pub fn bodies(&self) -> &BTreeSet<AuthorityBody> {
        &self.bodies
    }

    /// All durably known witnesses.
    pub fn witnesses(&self) -> &BTreeSet<Witness> {
        &self.witnesses
    }

    /// `Bodies(σ)`.
    pub fn bodies_for(&self, subject: SubjectId) -> BTreeSet<AuthorityBody> {
        self.bodies
            .iter()
            .filter(|body| body.subject() == subject)
            .cloned()
            .collect()
    }

    /// Every witness naming `event_id`. More than one is a re-signing, not duplicity.
    pub fn witnesses_for(&self, event_id: EventId) -> BTreeSet<Witness> {
        self.witnesses
            .iter()
            .filter(|witness| witness.event_id == event_id)
            .copied()
            .collect()
    }

    /// Whether the store holds this exact body.
    pub fn contains_body(&self, body: &AuthorityBody) -> bool {
        self.bodies.contains(body)
    }

    /// Number of durably known bodies.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Number of durably known witnesses.
    pub fn witness_count(&self) -> usize {
        self.witnesses.len()
    }
}
