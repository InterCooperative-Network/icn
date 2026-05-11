//! Executable baseline-lock loop fixture (single-node, no networking).
//!
//! See `docs/manual/FOUNDATIONAL_MANUAL_EXECUTABLE_BASELINE_LOOP.md`.
#![allow(clippy::expect_used)]

mod capsule;
mod constants;
mod evidence;
mod host;
mod projector;
mod receipt_emit;
mod receipt_types;

pub use capsule::{build_capsule, StateResolutionCapsule};
pub use constants::*;
pub use evidence::{action_card_allocation_authorized, ActionCardView, EvidencePacket};
pub use host::{evaluate_guest, validate_hostile_output, HostError};
pub use projector::{project, BaselineFixtureAuthority, ProjectedState, ProjectorError};
pub use receipt_emit::BaselineProcessGateResultReceipt;
pub use receipt_types::{
    build_receipt_chain, signing_key_for_label, BaselineReceipt, BaselineReceiptBody, FixtureKeys,
    FixtureOptions,
};

pub use icn_boundary::{
    CanonicalFacts, DeterminismClass, ExecutionInputEnvelopeV1, ExecutionOutputEnvelopeV1,
    FinalityClass, Hash, PrivacyClass, ResultKind,
};
