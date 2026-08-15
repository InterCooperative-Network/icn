//! Shared fixtures for the N1 authority-log test obligations.
//!
//! Everything here is deterministic: seeds in, identical bytes out. No test in this suite may
//! depend on randomness, wall-clock time, or iteration order.

#![allow(dead_code)]

use ed25519_dalek::SigningKey;

use icn_identity::authority_log::{
    authorize_event, revoke_event, AuthorityBody, AuthorityStore, CapabilitySet, ContextNonce,
    ContinuityRoot, DeviceCapability, EstablishmentKind, EventId, PrincipalKey,
    SignedAuthorityEvent, SubjectId, ValiditySpan,
};

/// A subject with its continuity root and inception event, all derived from one seed byte.
pub struct Subject {
    pub root: ContinuityRoot,
    pub subject: SubjectId,
    pub inception: SignedAuthorityEvent,
}

impl Subject {
    /// The `event_id` of the inception body — the parent of position 1.
    pub fn genesis(&self) -> EventId {
        self.inception.body.event_id()
    }
}

/// Build a deterministic subject from a seed, arming `horizon` rotations.
pub fn subject(seed: u8, horizon: usize) -> Subject {
    subject_with_plan(seed, vec![EstablishmentKind::Rotate; horizon])
}

/// Build a deterministic subject with an explicit establishment plan.
pub fn subject_with_plan(seed: u8, plan: Vec<EstablishmentKind>) -> Subject {
    let root = ContinuityRoot::with_plan(
        [seed; 32],
        ContextNonce::from_bytes([seed ^ 0xa5; 32]),
        plan,
    );
    let inception = root.incept().expect("inception must construct");
    let subject = inception.body.subject();
    Subject {
        root,
        subject,
        inception,
    }
}

/// A deterministic keypair that is *not* part of any authority chain.
pub fn stranger(seed: u8) -> (SigningKey, PrincipalKey) {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let principal = principal(&key);
    (key, principal)
}

/// Convert a deterministic test signing key into a validated N1 principal.
pub fn principal(key: &SigningKey) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(key.verifying_key())
        .expect("test key is canonical and strong")
}

/// A deterministic device principal.
pub fn device(seed: u8) -> PrincipalKey {
    stranger(seed).1
}

/// The default capability set used across the suite.
pub fn caps() -> CapabilitySet {
    CapabilitySet::new([DeviceCapability::Sign])
}

/// A capability set that *includes* `Recover`, used to prove the bit is inert.
pub fn caps_with_recover() -> CapabilitySet {
    CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Recover])
}

/// Author an `authorize` event signed by the authority key of `generation`.
pub fn authorize_at(
    s: &Subject,
    generation: u64,
    position: u64,
    prev: EventId,
    dev: PrincipalKey,
) -> SignedAuthorityEvent {
    authorize_event(
        &s.root.authority_signing_key(generation),
        s.subject,
        position,
        prev,
        dev,
        caps(),
        None,
    )
}

/// Author an `authorize` event carrying a position-denominated validity span.
pub fn authorize_span_at(
    s: &Subject,
    generation: u64,
    position: u64,
    prev: EventId,
    dev: PrincipalKey,
    span: ValiditySpan,
) -> SignedAuthorityEvent {
    authorize_event(
        &s.root.authority_signing_key(generation),
        s.subject,
        position,
        prev,
        dev,
        caps(),
        Some(span),
    )
}

/// Author a `revoke` event signed by the authority key of `generation`.
pub fn revoke_at(
    s: &Subject,
    generation: u64,
    position: u64,
    prev: EventId,
    dev: PrincipalKey,
) -> SignedAuthorityEvent {
    revoke_event(
        &s.root.authority_signing_key(generation),
        s.subject,
        position,
        prev,
        dev,
    )
}

/// A store holding exactly the given events, ingested in the given order.
pub fn store_of(events: &[SignedAuthorityEvent]) -> AuthorityStore {
    let mut store = AuthorityStore::new();
    store.ingest_all(events.iter());
    store
}

/// Every permutation of `items`, for small slices. Used to prove order-independence directly
/// rather than sampling it.
pub fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(index);
        for mut tail in permutations(&rest) {
            let mut candidate = vec![item.clone()];
            candidate.append(&mut tail);
            out.push(candidate);
        }
    }
    out
}

/// Convenience: the body set for a subject held by a store.
pub fn bodies_of(
    store: &AuthorityStore,
    s: SubjectId,
) -> std::collections::BTreeSet<AuthorityBody> {
    store.bodies_for(s)
}
