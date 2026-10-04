//! N4-E — current admission: the relying party's evaluation position, from its own facts.
//!
//! N4-A (`device_authority`) answers a **historical** question for a position the relying
//! party supplies: *was this device covered at `E`?* That must stay answerable, and this module
//! does not change it. What N4-A deliberately leaves to the relying party is *which* `E` to
//! supply (N4-A §7.1). For a class-2 act admitted **now** — a connection binding, a request to
//! open a session, anything settled at admission — the choice is not free: if the device could
//! pick any `E` at which it was once authorized, a device revoked at 3 would keep signing acts
//! bound to 2 forever, and every relying party would honestly confirm that history.
//!
//! The rule this module executes (N4-A §7.1, row 2; HIA §9.3 for class 2):
//!
//! > The admission position is the last position the relying party holds a clean prefix
//! > through — `frontier − 1` of **its own** retained facts — and the act must bind exactly
//! > that position.
//!
//! # What is deliberately true here
//!
//! - **The position comes from the store, never from the act.** An act bound below the
//!   admission position is *stale* (`StaleAct`): the device is behind, or replaying a position
//!   it was still authorized at. An act bound above it means *this relying party* is behind
//!   (`RelyingPartyBehind`): it must obtain facts, never guess, fetch or fall back.
//! - **Retained current position is local evidence, not universal finality.** A relying party
//!   whose facts stop before a revocation will admit, correctly under what it holds, at its old
//!   tip. Nothing here claims otherwise; obtaining facts (N3) is the only cure, and no clock,
//!   registrar or network lookup is consulted.
//! - **Unknown, halted and inception-only Subjects admit nothing.** No inception: `SubjectUnknown`.
//!   A fork in the retained prefix: `AuthorityHalted` — current authority is disputed, so there
//!   is no current admission (history below the fork stays answerable through N4-A). Inception
//!   only: `NoDelegationYet` — position 0 is inception, never an evaluation position.
//! - **N4-A is called unchanged** once the position is fixed. Authorship and authorization are
//!   its two separate checks, exactly as before.
//! - **Class 1 acts do not use this.** A deferred decision pins its own `E` (G1-A, #2694 §6);
//!   evaluating each ballot at a receiver-local tip would reintroduce arrival-order divergence.
//!
//! No clock, no network, no filesystem, no randomness.

use thiserror::Error;

use crate::authority_log::{derive, AuthorityStore, AuthorityView, SubjectId};
use crate::device_authority::{
    verify_device_act, DeviceActVerifyError, DeviceAuthorityEvidence, SignedDeviceAct,
};

/// Why no current admission is possible, or why this act is not admitted now.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdmissionRefusal {
    /// No inception body for the Subject is retained.
    #[error("subject is unknown to this relying party")]
    SubjectUnknown,
    /// The retained prefix forks at `disputed_at`: current authority is disputed.
    #[error("subject authority is halted at position {disputed_at}; no current admission")]
    AuthorityHalted {
        /// The disputed position.
        disputed_at: u64,
    },
    /// Only the inception is retained: no delegation exists yet.
    #[error("no delegation position retained yet; inception alone admits nothing")]
    NoDelegationYet,
    /// The act binds a position below this relying party's admission position.
    #[error("act binds position {act}, below the current admission position {admission}: stale")]
    StaleAct {
        /// The position bound in the act.
        act: u64,
        /// This relying party's admission position.
        admission: u64,
    },
    /// The act binds a position above this relying party's admission position: this relying
    /// party is behind and must obtain facts.
    #[error("act binds position {act}, above the current admission position {admission}: this relying party is behind")]
    RelyingPartyBehind {
        /// The position bound in the act.
        act: u64,
        /// This relying party's admission position.
        admission: u64,
    },
    /// The act was evaluated at the admission position and N4-A refused it.
    #[error("{0}")]
    Act(#[from] DeviceActVerifyError),
}

/// The class-2 admission position for `subject`: `frontier − 1` of the clean prefix this
/// relying party retains. Never read from an act; never a timestamp; never fetched.
pub fn admission_position(
    subject: SubjectId,
    store: &AuthorityStore,
) -> Result<u64, AdmissionRefusal> {
    match derive(subject, store) {
        AuthorityView::Unknown => Err(AdmissionRefusal::SubjectUnknown),
        AuthorityView::Halted { disputed_at, .. } => {
            Err(AdmissionRefusal::AuthorityHalted { disputed_at })
        }
        AuthorityView::Live { frontier, .. } => {
            if frontier < 2 {
                return Err(AdmissionRefusal::NoDelegationYet);
            }
            Ok(frontier - 1)
        }
    }
}

/// Admit `signed` now: compute the admission position from `store`, require the act to bind
/// exactly it, then run N4-A unchanged at that position.
pub fn verify_device_act_at_admission(
    signed: &SignedDeviceAct,
    subject: SubjectId,
    store: &AuthorityStore,
) -> Result<DeviceAuthorityEvidence, AdmissionRefusal> {
    let admission = admission_position(subject, store)?;
    let act = signed.act.evaluation_position;
    if act < admission {
        return Err(AdmissionRefusal::StaleAct { act, admission });
    }
    if act > admission {
        return Err(AdmissionRefusal::RelyingPartyBehind { act, admission });
    }
    verify_device_act(signed, subject, store, admission).map_err(AdmissionRefusal::Act)
}
