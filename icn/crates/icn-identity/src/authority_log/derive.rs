//! The derived authority view — a **pure function** of `Bodies(σ)` (§9.2.1).
//!
//! `derive` reads nothing but the body set and the subject identifier. No wall clock, no
//! randomness, no network state, no arrival order, no mutable receiver history, no local policy,
//! no other subject's log. Same `Bodies(σ)` ⇒ same verdict, on every replica, forever.
//!
//! The derived view **may regress** when more facts arrive (`Live → Halted`, or
//! `Live(branch A) → Live(branch B)`). The durable state may not. That asymmetry is deliberate:
//! a verified prefix is a *conclusion*, not durable state.

#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use super::body::{
    AuthorityBody, CapabilitySet, Commitment, EventId, InceptionBody, PrincipalKey, PrincipalSet,
    SubjectId, ValiditySpan,
};
use super::encoding::Writer;
use super::store::AuthorityStore;
use super::{sha256, COMMITMENT_DOMAIN, MAX_POSITION};

/// A device grant recorded by an `authorize` event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceGrant {
    /// The granted capabilities.
    pub capabilities: CapabilitySet,
    /// Optional position-denominated validity span.
    pub validity: Option<ValiditySpan>,
    /// The log position at which the grant was made. A position, never a timestamp.
    pub granted_at: u64,
}

impl DeviceGrant {
    /// Whether the grant is in force at `position`.
    pub fn in_force_at(&self, position: u64) -> bool {
        if position < self.granted_at {
            return false;
        }
        match self.validity {
            None => true,
            Some(span) => span.contains(position),
        }
    }
}

/// The authority state produced by folding a chain of chosen bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityState {
    /// The exactly-one logical writer of this log. Only establishment events change this set.
    ///
    /// The set representation mirrors the canonical body, but every admitted establishment body
    /// is required to contain one principal.
    pub authority: BTreeSet<PrincipalKey>,
    /// The pre-rotation commitment for the next establishment.
    pub next_commitment: Commitment,
    /// The establishment generation. Inception is `0`.
    pub generation: u64,
    /// Device grants — the relying-party-facing output. Devices are **not** log writers.
    pub devices: BTreeMap<PrincipalKey, DeviceGrant>,
}

impl AuthorityState {
    /// Whether `device` holds a grant that is in force at `position`.
    pub fn device_in_force_at(&self, device: &PrincipalKey, position: u64) -> bool {
        self.devices
            .get(device)
            .is_some_and(|grant| grant.in_force_at(position))
    }
}

/// The derived verdict for a subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityView {
    /// No inception body for this subject is durably known.
    Unknown,
    /// The chain advanced to `frontier`, where no authorized candidate exists (yet).
    Live {
        /// Folded authority state.
        state: AuthorityState,
        /// The first position with no authorized candidate.
        frontier: u64,
    },
    /// Two or more equally-authorized candidates exist at `disputed_at`. The subject is halted
    /// until a superseding establishment event lands **at that position**.
    Halted {
        /// Folded authority state, as of `disputed_at - 1`.
        state: AuthorityState,
        /// The disputed position.
        disputed_at: u64,
        /// The surviving candidates. Reported, never chosen between.
        candidates: BTreeSet<AuthorityBody>,
    },
}

impl AuthorityView {
    /// The folded state, if any.
    pub fn state(&self) -> Option<&AuthorityState> {
        match self {
            AuthorityView::Unknown => None,
            AuthorityView::Live { state, .. } | AuthorityView::Halted { state, .. } => Some(state),
        }
    }

    /// Whether the verdict is [`AuthorityView::Halted`].
    pub fn is_halted(&self) -> bool {
        matches!(self, AuthorityView::Halted { .. })
    }
}

/// Compute the pre-rotation commitment for a candidate establishment.
///
/// ```text
/// C = SHA-256(LP(COMMITMENT_DOMAIN) || u8(kind) || PS(authority) || b32(next_commitment))
/// ```
///
/// The commitment covers the **next** commitment as well as the authority set and the
/// establishment kind, so it pins every field of the establishment body that the signer would
/// otherwise be free to choose.
pub(crate) fn commitment_for(
    kind_tag: u8,
    authority: &PrincipalSet,
    next_commitment: &Commitment,
) -> Commitment {
    let mut w = Writer::new();
    w.lp(COMMITMENT_DOMAIN);
    w.u8(kind_tag);
    w.u32(authority.members().len() as u32);
    for member in authority.members() {
        w.u8(super::PRINCIPAL_TAG_ED25519);
        w.b32(&member.as_bytes());
    }
    w.b32(next_commitment.as_bytes());
    Commitment::from_bytes(sha256(&w.finish()))
}

/// Is `body` authorized against `state`?
///
/// **Establishment bodies** (`rotate`, `recover`) are authorized **iff** they reveal the
/// pre-committed material. This is the only place the pre-image reveal happens, and it is what
/// makes those two kinds establishment events — not a capability bit, and not a delegation.
/// The check *enforces* canonical derivation rather than merely permitting it:
///
/// * the recomputed commitment must equal `state.next_commitment`, which pins the kind, the
///   revealed authority set and the next commitment;
/// * the signer must be the sole revealed authority principal, which pins the last remaining free
///   field.
///
/// `subject`, `position` and `prev_digest` are pinned by the caller's position in the chain.
/// Every field is therefore determined, so **at most one authorized establishment body can exist
/// at a position** (§9.2.1 constraints 2 and 3). A malicious quorum, or an honest retry with
/// fresh randomness, produces a non-canonical body that fails here and never enters the
/// candidate set.
///
/// **Non-establishment bodies** are authorized iff their signer is in the current authority set.
/// Note what is *not* checked: no capability, no flag, nothing that a compromised current key
/// could use to claim establishment priority.
///
/// An inception body is never authorized by this function — it is selected by
/// `σ == event_id(b)`, which is a different mechanism.
fn authorized_by_state(body: &AuthorityBody, state: &AuthorityState) -> bool {
    match body {
        AuthorityBody::Inception(_) => false,
        AuthorityBody::Rotate(establishment) | AuthorityBody::Recover(establishment) => {
            if state.next_commitment.is_terminal() {
                // Nothing is armed; no establishment can be authorized.
                return false;
            }
            let recomputed = commitment_for(
                body.kind_tag(),
                &establishment.revealed_authority,
                &establishment.next_commitment,
            );
            if recomputed != state.next_commitment {
                return false;
            }
            establishment.revealed_authority.canonical_signer() == Some(establishment.header.signer)
        }
        AuthorityBody::Authorize(authorize) => {
            state.authority.len() == 1 && state.authority.contains(&authorize.header.signer)
        }
        AuthorityBody::Revoke(revoke) => {
            state.authority.len() == 1 && state.authority.contains(&revoke.header.signer)
        }
    }
}

/// Whether `body` is an authorized candidate at one exact chain location.
///
/// This internal boundary binds the state-only authority predicate to the subject, position, and
/// parent digest fixed by the derived prefix. It is deliberately not exported: an external caller
/// must derive through an admitted [`AuthorityStore`] rather than pair arbitrary state and bodies.
fn candidate_is_authorized(
    body: &AuthorityBody,
    state: &AuthorityState,
    subject: SubjectId,
    position: u64,
    parent: EventId,
) -> bool {
    body.subject() == subject
        && body.position() == position
        && body.prev_digest() == Some(parent)
        && authorized_by_state(body, state)
}

/// The **only** permitted selector (§9.2.1).
///
/// ```text
/// supersede(C) = if C contains any establishment event
///                then { b ∈ C : b is an establishment event }
///                else C
/// ```
///
/// It selects by **authority class**, never by content. There is no lowest-hash, first-seen,
/// greatest-`event_id`, longest-chain, lexicographic, arrival-order, random or "stable" tiebreak
/// here or anywhere else in this module. Where authority class cannot distinguish candidates the
/// caller **halts**.
pub fn supersede(candidates: BTreeSet<AuthorityBody>) -> BTreeSet<AuthorityBody> {
    if candidates.iter().any(AuthorityBody::is_establishment) {
        candidates
            .into_iter()
            .filter(AuthorityBody::is_establishment)
            .collect()
    } else {
        candidates
    }
}

/// What a candidate set resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// No authorized candidate — the chain has reached its frontier.
    Exhausted,
    /// Exactly one authorized candidate — advance.
    Advance(Box<AuthorityBody>),
    /// Two or more equally-authorized candidates — halt. **Never choose.**
    Halt(BTreeSet<AuthorityBody>),
}

/// Apply [`supersede`] and decide.
///
/// This is the single decision point of the derived layer, deliberately factored out so the
/// "`|C| ≥ 2` halts, it does not choose" rule is directly testable and so no future change can
/// smuggle a tiebreak into the loop body. `derive` calls exactly this function.
///
/// Note that the `BTreeSet` iteration order is used **only** to take the sole element of a
/// singleton. It is never used to pick between two.
pub fn resolve(candidates: BTreeSet<AuthorityBody>) -> Resolution {
    let survivors = supersede(candidates);
    match survivors.len() {
        0 => Resolution::Exhausted,
        1 => match survivors.into_iter().next() {
            Some(body) => Resolution::Advance(Box::new(body)),
            // Unreachable: length was just observed to be 1.
            None => Resolution::Exhausted,
        },
        _ => Resolution::Halt(survivors),
    }
}

/// Fold an inception body into the initial state.
pub(crate) fn apply_inception(body: &InceptionBody) -> AuthorityState {
    AuthorityState {
        authority: body.initial_authority.members().clone(),
        next_commitment: body.next_commitment,
        generation: 0,
        devices: BTreeMap::new(),
    }
}

/// Fold a chosen body into the state.
///
/// `rotate` and `recover` both **replace** the authority set with the revealed set, so a
/// superseding establishment actually removes a compromised key rather than extending around it
/// — which is why superseding recovery terminates (§9.2.2). They differ only in their effect on
/// device grants: `rotate` preserves them, `recover` clears them.
fn apply_transition(state: &AuthorityState, body: &AuthorityBody) -> AuthorityState {
    let mut next = state.clone();
    match body {
        AuthorityBody::Inception(inception) => {
            return apply_inception(inception);
        }
        AuthorityBody::Rotate(establishment) => {
            next.authority = establishment.revealed_authority.members().clone();
            next.next_commitment = establishment.next_commitment;
            next.generation = next.generation.saturating_add(1);
        }
        AuthorityBody::Recover(establishment) => {
            next.authority = establishment.revealed_authority.members().clone();
            next.next_commitment = establishment.next_commitment;
            next.generation = next.generation.saturating_add(1);
            next.devices.clear();
        }
        AuthorityBody::Authorize(authorize) => {
            next.devices.insert(
                authorize.device,
                DeviceGrant {
                    capabilities: authorize.capabilities.clone(),
                    validity: authorize.validity,
                    granted_at: authorize.header.position,
                },
            );
        }
        AuthorityBody::Revoke(revoke) => {
            next.devices.remove(&revoke.device);
        }
    }
    next
}

/// `Bodies(σ)`, arranged once by the chain coordinates the fold looks bodies up by.
///
/// The fold advances one position at a time, and at every step the candidate set is pinned to a
/// single `(position, parent)` pair. Re-scanning `Bodies(σ)` at each step would be `O(p·n)` — and
/// worse than merely slow, it would be *attacker-amplified*: `admissible` is deliberately cheap and
/// state-independent (§9.2.1), so anyone can mint unlimited well-formed bodies for a public σ. Such
/// bodies can never enter `C` — they are unauthorized — but under a per-position rescan each one
/// would still cost work at *every* position forever after. Grouping once bounds the total
/// authorization work by `|Bodies(σ)|` regardless of how the spam is shaped, and confines a flood
/// aimed at one position to the one bucket it was aimed at.
///
/// **This index is ephemeral to a single derivation.** It is not durable state, it takes no part in
/// the join, it is never persisted, and it holds no verdict — it is the same bodies in a different
/// arrangement. Derivation stays a pure function of `Bodies(σ)`: building the index twice from the
/// same set yields the same lookups and the same view.
struct CandidateIndex<'a> {
    /// The inception body, if `Bodies(σ)` holds one.
    ///
    /// Kept separate because inception is selected by `σ == event_id(b)` — a different mechanism
    /// from the fold, and the reason an inception body carries no `prev_digest` and never lands in
    /// a bucket.
    root: Option<(&'a AuthorityBody, &'a InceptionBody)>,
    /// `(position, prev_digest) → the bodies at that exact chain location`.
    ///
    /// Every non-inception body for σ appears in exactly one bucket, so the bucket sizes sum to the
    /// body count: the fold can never examine more bodies in total than the subject durably has.
    buckets: BTreeMap<(u64, EventId), Vec<&'a AuthorityBody>>,
    /// Test-only tally of bodies actually handed to the authorization predicate, so the resource
    /// property is *asserted* rather than argued from the shape of the source. The field does not
    /// exist in non-test builds.
    #[cfg(test)]
    examined: Cell<usize>,
}

impl<'a> CandidateIndex<'a> {
    /// The one grouping pass over `Bodies(σ)`.
    ///
    /// Bodies belonging to another subject are dropped here rather than at lookup time, which keeps
    /// the cross-subject guarantee even if a caller hands over an unfiltered set.
    fn build(subject: SubjectId, bodies: &'a BTreeSet<AuthorityBody>) -> Self {
        let mut root = None;
        let mut buckets: BTreeMap<(u64, EventId), Vec<&'a AuthorityBody>> = BTreeMap::new();

        for body in bodies {
            if body.subject() != subject {
                continue;
            }
            match (body, body.prev_digest()) {
                // Structurally unique: only an inception body reports its own digest as its
                // subject, and a distinct inception body has a distinct digest. The first-wins
                // guard preserves the previous `find_map` behaviour exactly.
                (AuthorityBody::Inception(inner), _) => {
                    if root.is_none() {
                        root = Some((body, inner));
                    }
                }
                (_, Some(parent)) => {
                    buckets
                        .entry((body.position(), parent))
                        .or_default()
                        .push(body);
                }
                // A non-inception body without a parent digest is not admissible and cannot be at
                // any chain location, so there is no bucket for it.
                (_, None) => {}
            }
        }

        CandidateIndex {
            root,
            buckets,
            #[cfg(test)]
            examined: Cell::new(0),
        }
    }

    /// Collect the authorized candidate set `C` at `position`.
    ///
    /// ```text
    /// C = { b ∈ Bodies(σ) : b.position = p
    ///                     ∧ b.prev_digest = event_id(chosen(p−1))
    ///                     ∧ authorized(b, state) }
    /// ```
    ///
    /// The definition is unchanged; only the set it is applied to is narrowed. The first two
    /// conjuncts are the bucket key, so [`candidate_is_authorized`] re-checks them redundantly —
    /// kept deliberately, so that the predicate a reviewer reads is still the whole predicate and a
    /// future mis-keying of the index cannot widen `C` behind it.
    fn candidates_at(
        &self,
        subject: SubjectId,
        position: u64,
        parent: EventId,
        state: &AuthorityState,
    ) -> BTreeSet<AuthorityBody> {
        let bucket = match self.buckets.get(&(position, parent)) {
            Some(bucket) => bucket,
            None => return BTreeSet::new(),
        };

        #[cfg(test)]
        self.examined.set(self.examined.get() + bucket.len());

        bucket
            .iter()
            .filter(|body| candidate_is_authorized(body, state, subject, position, parent))
            .map(|body| (*body).clone())
            .collect()
    }
}

/// Derive the authority view for `subject` from a durable body set.
///
/// Pure. Same set, same verdict — across permutations, across replicas, across processes.
///
/// **Inception cannot fork:** two different inception bodies hash to two different subjects, so
/// they are two different subjects, not two roots of one.
///
/// **The recursion is well-founded:** the loop advances one position at a time and only after
/// `|C| = 1`, so `chosen(p−1)` is always defined when `C` at `p` is computed; if `p−1` halts the
/// function has already returned.
pub fn derive(subject: SubjectId, store: &AuthorityStore) -> AuthorityView {
    let bodies = store.bodies_for(subject);
    derive_bodies(subject, &bodies)
}

/// Body-only fold behind the admitted-store boundary.
fn derive_bodies(subject: SubjectId, bodies: &BTreeSet<AuthorityBody>) -> AuthorityView {
    derive_indexed(subject, &CandidateIndex::build(subject, bodies))
}

/// The fold itself, over an already-grouped body set.
///
/// Split from [`derive_bodies`] so the single grouping pass is visibly outside the loop: nothing
/// below this line touches `Bodies(σ)` again.
fn derive_indexed(subject: SubjectId, index: &CandidateIndex<'_>) -> AuthorityView {
    // b₀ ← the unique body with σ = event_id(b). Uniqueness is structural: only an inception body
    // reports its own digest as its subject, and a distinct inception body has a distinct digest.
    let (root_body, root) = match index.root {
        Some(found) => found,
        None => return AuthorityView::Unknown,
    };

    let mut state = apply_inception(root);
    let mut chosen = root_body.event_id();
    let mut position: u64 = 1;

    loop {
        // The absolute position bound also bounds this loop: no body above `MAX_POSITION` is
        // admissible, so `C` is necessarily empty there.
        if position > MAX_POSITION {
            return AuthorityView::Live {
                state,
                frontier: position,
            };
        }

        let candidates = index.candidates_at(subject, position, chosen, &state);

        match resolve(candidates) {
            Resolution::Exhausted => {
                return AuthorityView::Live {
                    state,
                    frontier: position,
                };
            }
            Resolution::Advance(body) => {
                state = apply_transition(&state, &body);
                chosen = body.event_id();
                position = position.saturating_add(1);
            }
            Resolution::Halt(candidates) => {
                return AuthorityView::Halted {
                    state,
                    disputed_at: position,
                    candidates,
                };
            }
        }
    }
}

/// Resource obligations for the derivation fold.
///
/// These live inline rather than in `tests/` because the property being pinned is *internal*: how
/// many bodies the fold hands to the authorization predicate. An integration test can only observe
/// the verdict, and the verdict is identical whether the fold looks at each body once or `p` times.
/// The counter is `#[cfg(test)]`-gated, so it exists only for these assertions.
///
/// The bound asserted throughout is **linear**: `examined ≤ |Bodies(σ)| + positions`. A fold that
/// re-scanned the body set per position would report `positions × |Bodies(σ)|` and fail here.
#[cfg(test)]
mod resource_obligations {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::authority_log::body::{ContextNonce, DeviceCapability};
    use crate::authority_log::construct::{authorize_event, ContinuityRoot};
    use crate::authority_log::store::SignedAuthorityEvent;

    /// A deterministic signing key from a 16-bit label. Distinct labels give distinct keys, so
    /// distinct spam bodies — no accidental dedup inflating or deflating a count.
    fn key(label: u16) -> SigningKey {
        let mut seed = [0x5au8; 32];
        seed[0] = (label & 0xff) as u8;
        seed[1] = (label >> 8) as u8;
        SigningKey::from_bytes(&seed)
    }

    fn caps() -> CapabilitySet {
        CapabilitySet::new([DeviceCapability::Sign])
    }

    /// A subject plus its inception event, from one seed.
    fn subject_of(seed: u8) -> (ContinuityRoot, SubjectId, SignedAuthorityEvent) {
        let root = ContinuityRoot::new([seed; 32], ContextNonce::from_bytes([seed ^ 0xa5; 32]), 4);
        let inception = root.incept().expect("inception must construct");
        let subject = inception.body.subject();
        (root, subject, inception)
    }

    /// An ordinary authorized chain of `positions` `authorize` events, plus its inception.
    fn ordinary_chain(seed: u8, positions: u64) -> (SubjectId, Vec<SignedAuthorityEvent>) {
        let (root, subject, inception) = subject_of(seed);
        let authority = root.authority_signing_key(0);
        let mut prev = inception.body.event_id();
        let mut events = vec![inception];
        for position in 1..=positions {
            let event = authorize_event(
                &authority,
                subject,
                position,
                prev,
                PrincipalKey::from_signing_key(&key(position as u16)),
                caps(),
                None,
            );
            prev = event.body.event_id();
            events.push(event);
        }
        (subject, events)
    }

    /// Ingest into a store, derive through the index, and report `(view, bodies examined)`.
    fn derive_counted(
        subject: SubjectId,
        events: &[SignedAuthorityEvent],
    ) -> (AuthorityView, usize) {
        let mut store = AuthorityStore::new();
        store.ingest_all(events.iter());
        let bodies = store.bodies_for(subject);
        let index = CandidateIndex::build(subject, &bodies);
        let view = derive_indexed(subject, &index);
        let examined = index.examined.get();
        (view, examined)
    }

    /// A long ordinary chain must not rescan the retained set once per position.
    ///
    /// With one authorized body per position and no spam, the fold has exactly one candidate to
    /// weigh at each step, so the count is *exactly* the chain length. The previous shape would
    /// have reported `128 × 129 = 16512`.
    #[test]
    fn ordinary_chain_does_not_rescan_the_body_set_per_position() {
        const POSITIONS: u64 = 128;
        let (subject, events) = ordinary_chain(1, POSITIONS);
        let (view, examined) = derive_counted(subject, &events);

        assert_eq!(
            view.state().map(|state| state.devices.len()),
            Some(POSITIONS as usize),
            "the whole chain must fold"
        );
        assert_eq!(
            examined, POSITIONS as usize,
            "one candidate per position — not one full-set scan per position"
        );
        assert!(
            examined < (POSITIONS as usize) * events.len(),
            "a per-position rescan would examine {} bodies",
            (POSITIONS as usize) * events.len()
        );
    }

    /// Admissible-but-unauthorized bodies must not multiply *every* frontier step.
    ///
    /// Spam is confined to the bucket it names. It is paid for once, when the fold passes that
    /// position, and never again — so total work stays linear in the retained set rather than
    /// linear in `spam × positions`.
    #[test]
    fn unauthorized_spam_is_paid_for_once_not_at_every_position() {
        const POSITIONS: u64 = 64;
        const SPAM: u16 = 512;

        let (subject, mut events) = ordinary_chain(2, POSITIONS);
        let genesis = events[0].body.event_id();
        let (_, clean_examined) = derive_counted(subject, &events);

        // Every spam body is well-formed, correctly names the verified prefix, and is signed by a
        // key with no authority. All of them are admitted; none of them can ever enter `C`.
        for label in 0..SPAM {
            events.push(authorize_event(
                &key(label ^ 0x8000),
                subject,
                1,
                genesis,
                PrincipalKey::from_signing_key(&key(label ^ 0x4000)),
                caps(),
                None,
            ));
        }

        let (clean_view, _) = derive_counted(subject, &events[..=POSITIONS as usize]);
        let (spammed_view, spammed_examined) = derive_counted(subject, &events);

        assert_eq!(
            spammed_view, clean_view,
            "spam changes the cost model, never the verdict"
        );
        assert_eq!(
            spammed_examined,
            clean_examined + SPAM as usize,
            "the flood is examined exactly once, at the position it named"
        );
        assert!(
            spammed_examined <= events.len() + POSITIONS as usize,
            "total work must stay linear in the retained body set"
        );
    }

    /// Grouping must be permutation invariant: same bodies, same buckets, same count, same verdict.
    #[test]
    fn candidate_indexing_is_permutation_invariant() {
        let (subject, events) = ordinary_chain(3, 12);
        let (baseline_view, baseline_examined) = derive_counted(subject, &events);

        // Reversed, and rotated by a stride coprime with the length — two orders that share no
        // prefix with the original.
        let reversed: Vec<_> = events.iter().rev().cloned().collect();
        let rotated: Vec<_> = (0..events.len())
            .map(|i| events[(i * 5) % events.len()].clone())
            .collect();

        for (label, order) in [("reversed", reversed), ("rotated", rotated)] {
            let (view, examined) = derive_counted(subject, &order);
            assert_eq!(view, baseline_view, "{label} order changed the verdict");
            assert_eq!(
                examined, baseline_examined,
                "{label} order changed the cost"
            );
        }
    }

    /// Bucketing must not hide a conflict. Two distinct authorized bodies at one position share a
    /// bucket, so both reach `resolve` and the subject halts — it is never resolved by grouping.
    #[test]
    fn same_position_conflict_still_halts_through_the_index() {
        let (root, subject, inception) = subject_of(4);
        let authority = root.authority_signing_key(0);
        let genesis = inception.body.event_id();

        let left = authorize_event(
            &authority,
            subject,
            1,
            genesis,
            PrincipalKey::from_signing_key(&key(1)),
            caps(),
            None,
        );
        let right = authorize_event(
            &authority,
            subject,
            1,
            genesis,
            PrincipalKey::from_signing_key(&key(2)),
            caps(),
            None,
        );

        let (view, examined) = derive_counted(subject, &[inception, left, right]);
        match view {
            AuthorityView::Halted {
                disputed_at,
                candidates,
                ..
            } => {
                assert_eq!(disputed_at, 1);
                assert_eq!(candidates.len(), 2, "both conflicting bodies must survive");
            }
            other => panic!("one bucket, two authorized bodies must halt, got {other:?}"),
        }
        assert_eq!(examined, 2, "both were weighed, neither was dropped");
    }

    /// A gap must still buffer. The bodies beyond the gap are indexed and retained, but their
    /// bucket is never reached, so the frontier stops at the missing position.
    #[test]
    fn gaps_still_buffer_through_the_index() {
        let (subject, events) = ordinary_chain(5, 6);
        // Drop position 3 (index 3: inception, p1, p2, p3, …).
        let with_gap: Vec<_> = events
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 3)
            .map(|(_, event)| event.clone())
            .collect();

        let (view, examined) = derive_counted(subject, &with_gap);
        match view {
            AuthorityView::Live { frontier, .. } => assert_eq!(
                frontier, 3,
                "the fold must stop at the gap, not skip or wedge"
            ),
            other => panic!("a gap must leave the subject live, got {other:?}"),
        }
        assert_eq!(
            examined, 2,
            "positions 1 and 2 only; the buffered suffix is never examined"
        );
    }

    /// A superseding establishment orphans the suffix. The orphaned bodies stay indexed under the
    /// parent they named, which no longer sits on the chain, so their bucket is never reached.
    #[test]
    fn orphaned_suffix_stays_indexed_but_unreachable() {
        let (root, subject, inception) = subject_of(6);
        let authority = root.authority_signing_key(0);
        let genesis = inception.body.event_id();

        let ordinary = authorize_event(
            &authority,
            subject,
            1,
            genesis,
            PrincipalKey::from_signing_key(&key(1)),
            caps(),
            None,
        );
        // A suffix built on the ordinary event at position 1.
        let suffix = authorize_event(
            &authority,
            subject,
            2,
            ordinary.body.event_id(),
            PrincipalKey::from_signing_key(&key(2)),
            caps(),
            None,
        );
        // A superseding establishment at the *same* position as the ordinary event.
        let establishment = root
            .establish(1, subject, 1, genesis)
            .expect("armed establishment must construct");

        let events = vec![inception, ordinary, suffix.clone(), establishment];
        let (view, _) = derive_counted(subject, &events);

        assert_eq!(
            view.state().map(|state| state.generation),
            Some(1),
            "the establishment supersedes the ordinary event at position 1"
        );
        match view {
            AuthorityView::Live { frontier, .. } => assert_eq!(
                frontier, 2,
                "the suffix is orphaned: it names a parent no longer on the chain"
            ),
            other => panic!("superseding establishment must advance, got {other:?}"),
        }

        let mut store = AuthorityStore::new();
        store.ingest_all(events.iter());
        assert!(
            store.contains_body(&suffix.body),
            "orphaning is a derived conclusion — the body stays durably known"
        );
    }

    /// Re-signing one canonical body must not multiply candidates. Bodies are the set elements, so
    /// a second witness over the same body adds a witness and leaves the bucket at one.
    #[test]
    fn resigning_does_not_multiply_body_candidates() {
        let (subject, events) = ordinary_chain(7, 3);
        let (baseline_view, baseline_examined) = derive_counted(subject, &events);

        // The same canonical body, carried by a second signed event.
        let mut resigned = events.clone();
        resigned.push(SignedAuthorityEvent::new(
            events[1].body.clone(),
            events[1].signature,
        ));

        let (view, examined) = derive_counted(subject, &resigned);
        assert_eq!(view, baseline_view, "a re-signing is not a fork");
        assert_eq!(
            examined, baseline_examined,
            "one body, one candidate — re-signings collapse before the fold sees them"
        );
    }

    /// Every non-inception body for the subject lands in exactly one bucket, which is what makes
    /// the total examinable set equal to the retained set rather than a multiple of it.
    #[test]
    fn buckets_partition_the_non_inception_bodies() {
        const POSITIONS: u64 = 20;
        let (subject, events) = ordinary_chain(8, POSITIONS);
        let mut store = AuthorityStore::new();
        store.ingest_all(events.iter());
        let bodies = store.bodies_for(subject);
        let index = CandidateIndex::build(subject, &bodies);

        let indexed: usize = index.buckets.values().map(Vec::len).sum();
        assert_eq!(
            indexed,
            bodies.len() - 1,
            "every body except the inception is indexed exactly once"
        );
        assert!(index.root.is_some(), "the inception is held separately");
    }
}
