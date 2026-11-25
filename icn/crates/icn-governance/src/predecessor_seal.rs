//! Predecessor-seal lifecycle model (ADR-0092 §2; design-spec §2.3 as amended in session 19).
//!
//! **EXPERIMENTAL, unratified, and PURE.** This module validates the lifecycle of a legacy
//! predecessor domain `L` that hands its decision space to a successor through a
//! [`CutoverCommitmentV1`]:
//!
//! ```text
//! Legacy{n} --begin(n+1, K)--> CutoverPending{n+1, K} --seal(n+1, K, F)--> Sealed{n+1, K, F}
//!     ^                               |
//!     +----- abort(n+1, K) -----------+      (Legacy keeps n+1; the next begin is n+2)
//! ```
//!
//! The attempt counter never moves backward and is never reused. `Sealed` has no exit through
//! ordinary operations (exceptional recovery is P8).
//!
//! # What this module does not establish
//!
//! It performs no I/O, reads no clock, writes no storage and is wired to no live path. A
//! [`TransitionOutcome`] is a **validated** transition, not a durable one: nothing here makes a
//! predecessor sealed. [`evaluate_drain`] decides over **caller-supplied** proposal evidence and
//! cannot prove that the evidence lists every proposal of `L`, that a closure artifact is really
//! durable, or that no writer races the seal. The authoritative store, the fence and complete
//! enumeration are P5b; live rails and the operational proposal variant are P5c. A sealed
//! predecessor is also not, by itself, verified institutional succession (P6).

use sha2::{Digest, Sha256};

use crate::cutover::{CutoverCommitmentV1, CutoverError, MAX_CUTOVER_DOMAIN_ID_BYTES};
use crate::domain::GovernanceDomainId;
use crate::proposal::{ExecutionClosureClass, ProposalPayload, ProposalState};
use crate::ProposalOutcome;

/// Domain tag of the final-set commitment `F`.
pub const PREDECESSOR_FINAL_SET_DOMAIN: &[u8] = b"icn.governance.predecessor-final-set.v1";
/// Version of the final-set preimage.
pub const PREDECESSOR_FINAL_SET_VERSION: u16 = 1;
/// Upper bound on entries in one final set (experimental).
pub const MAX_FINAL_SET: usize = 1 << 20;
/// Upper bound on one proposal id's UTF-8 length (experimental; generated ids are 36 bytes).
pub const MAX_PROPOSAL_ID_LEN: usize = 256;

/// The lifecycle state of a legacy predecessor domain.
// Variants carry one cutover commitment at most; values are transient or stored singly.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredecessorLifecycle {
    /// Ordinary legacy operation. `last_attempt` is the latest cutover attempt ever begun (0 if
    /// none); `aborted` is the commitment hash of that attempt when it was aborted, so an exact
    /// replay of the abort is recognisable. `aborted` is `None` iff `last_attempt == 0`.
    Legacy {
        last_attempt: u64,
        aborted: Option<[u8; 32]>,
    },
    /// Attempt `attempt` is draining under the exact approved `commitment`.
    CutoverPending {
        attempt: u64,
        commitment: CutoverCommitmentV1,
    },
    /// Attempt `attempt` completed under `commitment` with final-set commitment `final_set`.
    Sealed {
        attempt: u64,
        commitment: CutoverCommitmentV1,
        final_set: [u8; 32],
    },
}

impl PredecessorLifecycle {
    /// The state of every domain before any cutover.
    pub const fn initial() -> Self {
        PredecessorLifecycle::Legacy {
            last_attempt: 0,
            aborted: None,
        }
    }

    /// The latest attempt this lifecycle has seen.
    pub fn attempt(&self) -> u64 {
        match self {
            PredecessorLifecycle::Legacy { last_attempt, .. } => *last_attempt,
            PredecessorLifecycle::CutoverPending { attempt, .. }
            | PredecessorLifecycle::Sealed { attempt, .. } => *attempt,
        }
    }

    /// Structural consistency of a stored state for predecessor `domain`.
    pub fn validate(&self, domain: &GovernanceDomainId) -> Result<(), SealRefusal> {
        match self {
            PredecessorLifecycle::Legacy {
                last_attempt,
                aborted,
            } => {
                if (*last_attempt == 0) != aborted.is_none() {
                    return Err(SealRefusal::MalformedState);
                }
                Ok(())
            }
            PredecessorLifecycle::CutoverPending {
                attempt,
                commitment,
            }
            | PredecessorLifecycle::Sealed {
                attempt,
                commitment,
                ..
            } => {
                if *attempt == 0 {
                    return Err(SealRefusal::MalformedState);
                }
                commitment.validate().map_err(SealRefusal::Commitment)?;
                if commitment.predecessor != *domain {
                    return Err(SealRefusal::ForeignPredecessor);
                }
                Ok(())
            }
        }
    }
}

/// A requested lifecycle transition. `commitment` / `commitment_hash` name the exact cutover the
/// transition applies to; `K = CutoverCommitmentV1::commitment()`.
// A transient request value, never stored in bulk; boxing the commitment would only complicate
// equality checks.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutoverTransition {
    Begin {
        attempt: u64,
        commitment: CutoverCommitmentV1,
    },
    Abort {
        attempt: u64,
        commitment_hash: [u8; 32],
    },
    Seal {
        attempt: u64,
        commitment_hash: [u8; 32],
        final_set: [u8; 32],
    },
}

/// A validated (not durable) transition result.
// Variants carry one cutover commitment at most; values are transient or stored singly.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionOutcome {
    /// The transition is valid and yields this next state.
    Validated(PredecessorLifecycle),
    /// The transition exactly repeats the one that produced the current state; nothing changes.
    Replayed,
}

/// Why a transition, final set or drain was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SealRefusal {
    #[error("stored lifecycle state is malformed")]
    MalformedState,
    #[error("cutover commitment is invalid: {0}")]
    Commitment(CutoverError),
    #[error("the cutover commitment names another predecessor domain")]
    ForeignPredecessor,
    #[error("attempt {attempt} is not newer than the latest attempt {latest}")]
    StaleAttempt { attempt: u64, latest: u64 },
    #[error("attempt {attempt} skips past the next attempt {expected}")]
    AttemptGap { attempt: u64, expected: u64 },
    #[error("the cutover attempt counter is exhausted")]
    AttemptOverflow,
    #[error("another cutover attempt is pending")]
    AlreadyPending,
    #[error("the transition does not match the pending attempt and commitment")]
    MismatchedAttempt,
    #[error("no cutover is pending")]
    NotPending,
    #[error("the predecessor is sealed; a seal cannot be aborted, reopened or replaced")]
    SealedIsFinal,
    #[error("final set: {0}")]
    FinalSet(&'static str),
    #[error("proposal {0} is still active")]
    ActiveProposal(String),
    #[error("proposal {0} was accepted but its execution is not durably closed")]
    UnresolvedEffects(String),
    #[error("proposal {0} changes a condition the cutover relies on; reconcile or abort")]
    ChangesCutoverBasis(String),
}

/// Validate `transition` against the stored `state` of predecessor `domain`.
///
/// A `Seal` carries a caller-supplied `final_set`, which this function records but cannot
/// recompute (it has no proposal set). Use [`evaluate_drain`] to produce a `Seal`; the
/// authoritative store (P5b) must recompute `F` from the enumerated set under its fence.
pub fn validate_transition(
    domain: &GovernanceDomainId,
    state: &PredecessorLifecycle,
    transition: &CutoverTransition,
) -> Result<TransitionOutcome, SealRefusal> {
    state.validate(domain)?;
    match (state, transition) {
        (
            PredecessorLifecycle::Legacy { last_attempt, .. },
            CutoverTransition::Begin {
                attempt,
                commitment,
            },
        ) => {
            let expected = last_attempt
                .checked_add(1)
                .ok_or(SealRefusal::AttemptOverflow)?;
            if *attempt <= *last_attempt {
                return Err(SealRefusal::StaleAttempt {
                    attempt: *attempt,
                    latest: *last_attempt,
                });
            }
            if *attempt != expected {
                return Err(SealRefusal::AttemptGap {
                    attempt: *attempt,
                    expected,
                });
            }
            commitment.validate().map_err(SealRefusal::Commitment)?;
            if commitment.predecessor != *domain {
                return Err(SealRefusal::ForeignPredecessor);
            }
            Ok(TransitionOutcome::Validated(
                PredecessorLifecycle::CutoverPending {
                    attempt: *attempt,
                    commitment: commitment.clone(),
                },
            ))
        }
        (
            PredecessorLifecycle::Legacy {
                last_attempt,
                aborted,
            },
            CutoverTransition::Abort {
                attempt,
                commitment_hash,
            },
        ) => {
            if *attempt == *last_attempt && aborted.as_ref() == Some(commitment_hash) {
                Ok(TransitionOutcome::Replayed)
            } else if *attempt <= *last_attempt {
                Err(SealRefusal::StaleAttempt {
                    attempt: *attempt,
                    latest: *last_attempt,
                })
            } else {
                Err(SealRefusal::NotPending)
            }
        }
        (PredecessorLifecycle::Legacy { .. }, CutoverTransition::Seal { .. }) => {
            Err(SealRefusal::NotPending)
        }
        (
            PredecessorLifecycle::CutoverPending {
                attempt: pending,
                commitment,
            },
            CutoverTransition::Begin {
                attempt,
                commitment: requested,
            },
        ) => {
            if attempt == pending && requested == commitment {
                Ok(TransitionOutcome::Replayed)
            } else if attempt < pending {
                Err(SealRefusal::StaleAttempt {
                    attempt: *attempt,
                    latest: *pending,
                })
            } else {
                Err(SealRefusal::AlreadyPending)
            }
        }
        (
            PredecessorLifecycle::CutoverPending {
                attempt: pending,
                commitment,
            },
            CutoverTransition::Abort {
                attempt,
                commitment_hash,
            },
        ) => {
            let hash = commitment.commitment().map_err(SealRefusal::Commitment)?;
            if attempt == pending && *commitment_hash == hash {
                Ok(TransitionOutcome::Validated(PredecessorLifecycle::Legacy {
                    last_attempt: *pending,
                    aborted: Some(hash),
                }))
            } else if attempt < pending {
                Err(SealRefusal::StaleAttempt {
                    attempt: *attempt,
                    latest: *pending,
                })
            } else {
                Err(SealRefusal::MismatchedAttempt)
            }
        }
        (
            PredecessorLifecycle::CutoverPending {
                attempt: pending,
                commitment,
            },
            CutoverTransition::Seal {
                attempt,
                commitment_hash,
                final_set,
            },
        ) => {
            let hash = commitment.commitment().map_err(SealRefusal::Commitment)?;
            if attempt == pending && *commitment_hash == hash {
                Ok(TransitionOutcome::Validated(PredecessorLifecycle::Sealed {
                    attempt: *pending,
                    commitment: commitment.clone(),
                    final_set: *final_set,
                }))
            } else if attempt < pending {
                Err(SealRefusal::StaleAttempt {
                    attempt: *attempt,
                    latest: *pending,
                })
            } else {
                Err(SealRefusal::MismatchedAttempt)
            }
        }
        (
            PredecessorLifecycle::Sealed {
                attempt: sealed,
                commitment,
                final_set: stored,
            },
            CutoverTransition::Seal {
                attempt,
                commitment_hash,
                final_set,
            },
        ) => {
            let hash = commitment.commitment().map_err(SealRefusal::Commitment)?;
            if attempt == sealed && *commitment_hash == hash && final_set == stored {
                Ok(TransitionOutcome::Replayed)
            } else {
                Err(SealRefusal::SealedIsFinal)
            }
        }
        (PredecessorLifecycle::Sealed { .. }, _) => Err(SealRefusal::SealedIsFinal),
    }
}

/// `F`: the commitment to the terminal proposal ids of predecessor `domain` at the seal of
/// `attempt` under cutover commitment hash `commitment_hash` (S19 layout):
///
/// `SHA-256( lp(tag) || u16be(1) || lp(L) || u64be(attempt) || K || u32be(n) || lp(id)* )`
///
/// with ids strictly ascending by UTF-8 bytes, each non-empty and at most
/// [`MAX_PROPOSAL_ID_LEN`] bytes, and `1 <= n <= MAX_FINAL_SET`. Refused, never sorted.
/// `L` is non-empty and at most [`MAX_CUTOVER_DOMAIN_ID_BYTES`] (1024) UTF-8 bytes: the same
/// bound as the cutover commitment that names it (D-S20-11).
pub fn final_set_commitment(
    domain: &GovernanceDomainId,
    attempt: u64,
    commitment_hash: &[u8; 32],
    ids: &[&str],
) -> Result<[u8; 32], SealRefusal> {
    if domain.0.is_empty() || domain.0.len() > MAX_CUTOVER_DOMAIN_ID_BYTES {
        return Err(SealRefusal::FinalSet("invalid predecessor domain id"));
    }
    if attempt == 0 {
        return Err(SealRefusal::FinalSet("attempt 0 cannot be sealed"));
    }
    if ids.is_empty() {
        return Err(SealRefusal::FinalSet("empty final set"));
    }
    if ids.len() > MAX_FINAL_SET {
        return Err(SealRefusal::FinalSet("too many entries"));
    }
    if ids
        .iter()
        .any(|id| id.is_empty() || id.len() > MAX_PROPOSAL_ID_LEN)
    {
        return Err(SealRefusal::FinalSet("empty or oversize proposal id"));
    }
    if !ids
        .windows(2)
        .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
    {
        return Err(SealRefusal::FinalSet(
            "ids are not strictly ascending (unsorted or duplicate)",
        ));
    }
    let mut h = Sha256::new();
    h.update((PREDECESSOR_FINAL_SET_DOMAIN.len() as u32).to_be_bytes());
    h.update(PREDECESSOR_FINAL_SET_DOMAIN);
    h.update(PREDECESSOR_FINAL_SET_VERSION.to_be_bytes());
    h.update((domain.0.len() as u32).to_be_bytes());
    h.update(domain.0.as_bytes());
    h.update(attempt.to_be_bytes());
    h.update(commitment_hash);
    h.update((ids.len() as u32).to_be_bytes());
    for id in ids {
        h.update((id.len() as u32).to_be_bytes());
        h.update(id.as_bytes());
    }
    Ok(h.finalize().into())
}

/// How one proposal of the predecessor stands for the drain (caller-derived; see [`classify`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainStatus {
    /// Never activated. Preserved; does not block the seal; not in `F`. P5c must keep it from
    /// becoming active while draining or after the seal.
    InactiveDraft,
    /// In deliberation or open for voting: blocks the seal until it reaches a terminal outcome.
    /// An existing deliberating proposal may still move to voting while draining; the refused
    /// "activation" is only leaving `Draft` (P5b design, D-P5b-3).
    Active,
    /// Closed, with no outstanding effect (declarative, not accepted, or durably closed).
    Terminal,
    /// Accepted and execution-required, but its closure artifact is not durable: blocks the seal.
    TerminalEffectsPending,
    /// Accepted and changes a condition the cutover relies on: blocks the seal until reconciled or
    /// the attempt is aborted.
    TerminalChangesCutoverBasis,
}

/// Classify a proposal from its real state and payload (whose
/// [`ProposalPayload::execution_closure_class`] decides whether acceptance needs a closure
/// artifact) plus two caller-supplied facts that this pure module cannot verify: whether that
/// closure artifact is durable, and whether the proposal changes a condition the cutover relies on
/// (authority, membership, governance configuration).
pub fn classify(
    state: &ProposalState,
    payload: &ProposalPayload,
    closure_durable: bool,
    changes_cutover_basis: bool,
) -> DrainStatus {
    let accepted = match state {
        ProposalState::Draft => return DrainStatus::InactiveDraft,
        ProposalState::Deliberation { .. } | ProposalState::Open { .. } => {
            return DrainStatus::Active
        }
        ProposalState::Accepted { .. } => true,
        ProposalState::ForceClosed { outcome, .. } => *outcome == ProposalOutcome::Accepted,
        ProposalState::Rejected { .. }
        | ProposalState::NoQuorum { .. }
        | ProposalState::Cancelled { .. }
        | ProposalState::Vetoed { .. } => false,
    };
    if !accepted {
        DrainStatus::Terminal
    } else if payload.execution_closure_class() == ExecutionClosureClass::ExecutionRequired
        && !closure_durable
    {
        DrainStatus::TerminalEffectsPending
    } else if changes_cutover_basis {
        DrainStatus::TerminalChangesCutoverBasis
    } else {
        DrainStatus::Terminal
    }
}

/// Decide whether the supplied evidence permits sealing `attempt` of `domain` under the pending
/// `commitment`, and if so produce the [`CutoverTransition::Seal`] (with `F` over the terminal
/// ids). Entries must be strictly ascending by id. Refuses on any active, effects-pending or
/// basis-changing proposal. The evidence is trusted to be complete: see the module docs.
pub fn evaluate_drain(
    domain: &GovernanceDomainId,
    state: &PredecessorLifecycle,
    evidence: &[(&str, DrainStatus)],
) -> Result<CutoverTransition, SealRefusal> {
    state.validate(domain)?;
    let (attempt, commitment) = match state {
        PredecessorLifecycle::CutoverPending {
            attempt,
            commitment,
        } => (*attempt, commitment),
        PredecessorLifecycle::Sealed { .. } => return Err(SealRefusal::SealedIsFinal),
        PredecessorLifecycle::Legacy { .. } => return Err(SealRefusal::NotPending),
    };
    if !evidence
        .windows(2)
        .all(|pair| pair[0].0.as_bytes() < pair[1].0.as_bytes())
    {
        return Err(SealRefusal::FinalSet(
            "evidence is not strictly ascending by id (unsorted or duplicate)",
        ));
    }
    let mut terminal = Vec::with_capacity(evidence.len());
    for (id, status) in evidence {
        match status {
            DrainStatus::InactiveDraft => {}
            DrainStatus::Active => return Err(SealRefusal::ActiveProposal((*id).to_string())),
            DrainStatus::TerminalEffectsPending => {
                return Err(SealRefusal::UnresolvedEffects((*id).to_string()))
            }
            DrainStatus::TerminalChangesCutoverBasis => {
                return Err(SealRefusal::ChangesCutoverBasis((*id).to_string()))
            }
            DrainStatus::Terminal => terminal.push(*id),
        }
    }
    let commitment_hash = commitment.commitment().map_err(SealRefusal::Commitment)?;
    let final_set = final_set_commitment(domain, attempt, &commitment_hash, &terminal)?;
    Ok(CutoverTransition::Seal {
        attempt,
        commitment_hash,
        final_set,
    })
}
