//! The derived authority view — a **pure function** of `Bodies(σ)` (§9.2.1).
//!
//! `derive` reads nothing but the body set and the subject identifier. No wall clock, no
//! randomness, no network state, no arrival order, no mutable receiver history, no local policy,
//! no other subject's log. Same `Bodies(σ)` ⇒ same verdict, on every replica, forever.
//!
//! The derived view **may regress** when more facts arrive (`Live → Halted`, or
//! `Live(branch A) → Live(branch B)`). The durable state may not. That asymmetry is deliberate:
//! a verified prefix is a *conclusion*, not durable state.

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
    /// The keys authorized to **write** this log. Only establishment events change this set.
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
/// * the signer must be the canonically first revealed principal, which pins the last remaining
///   free field.
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
        AuthorityBody::Authorize(authorize) => state.authority.contains(&authorize.header.signer),
        AuthorityBody::Revoke(revoke) => state.authority.contains(&revoke.header.signer),
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

/// Collect the authorized candidate set `C` at `position`.
///
/// ```text
/// C = { b ∈ Bodies(σ) : b.position = p
///                     ∧ b.prev_digest = event_id(chosen(p−1))
///                     ∧ authorized(b, state) }
/// ```
///
/// Factored so the candidate-set definition remains explicit and reviewable.
pub(crate) fn candidates_at(
    bodies: &BTreeSet<AuthorityBody>,
    subject: SubjectId,
    position: u64,
    parent: EventId,
    state: &AuthorityState,
) -> BTreeSet<AuthorityBody> {
    bodies
        .iter()
        .filter(|body| candidate_is_authorized(body, state, subject, position, parent))
        .cloned()
        .collect()
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
    // b₀ ← the unique body with σ = event_id(b). Uniqueness is structural: only an inception body
    // reports its own digest as its subject, and a distinct inception body has a distinct digest.
    let inception = bodies.iter().find_map(|body| match body {
        AuthorityBody::Inception(inner) if body.subject() == subject => Some((body, inner)),
        _ => None,
    });

    let (root_body, root) = match inception {
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

        let candidates = candidates_at(bodies, subject, position, chosen, &state);

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
