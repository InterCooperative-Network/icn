//! # Authority-to-effect spine, v0 (EXPERIMENTAL formats)
//!
//! Signed action → verified attribution → contextual standing → adopted-rule evaluation → signed
//! authorization → target-side admission or refusal → outcome receipt.
//!
//! Attribution is **reused unchanged** from `icn-identity` (N1 authority log, N4 `DeviceActV1`,
//! N4 portable evidence bundle). This module adds only what sits between "this device signed these
//! bytes for this Subject" and "this target may act":
//!
//! - [`ContextInceptionV0`] / [`ContextRef`] — a self-certifying context and its founders;
//! - [`WriterDesignationV0`] — founders (≥ threshold) designate the epoch's single writer
//!   (**provisional**: single-writer-per-context is unvalidated, T9 not run);
//! - [`StandingV0`], [`RuleAdoptionV0`], [`AuthorizationV0`] — writer-signed statements;
//! - [`AdoptedRule`] — a pure rule supplied by a domain crate (governance meaning is not here);
//! - [`verify_authorization`] — re-derives the verdict; the writer's signature is never trusted
//!   for truth, only for order and attribution;
//! - [`admit`] / [`ConsumedSet`] — target, expiry, epoch and one-time consumption;
//! - [`OutcomeReceiptV0`] — the custodian's signed outcome boundary (no real effect runs);
//! - [`log`] — the Context Authority Log: standing, rules and writer derived from authorized,
//!   sequenced transitions instead of writer assertions, with relying-party frontiers and fork
//!   proofs ([`verify_checkpointed_authorization`]).
//!
//! ## Status
//!
//! Formats are v0 experimental (`icn.authz.v0.*` domains, version 0), not gossiped, not persisted
//! by any daemon, and droppable without migration. No context-kind byte is introduced.
//! [`InMemoryConsumedSet`] is not durable replay protection.

mod codec;
pub mod log;
mod statements;
mod verify;

pub use codec::AuthzCodecError;
pub use log::{
    apply_entry, find_log_fork, replay_context_log, sign_checkpointed_authorization,
    transition_subject_matter, verify_checkpointed_authorization, CheckpointedAuthorizationV0,
    CheckpointedEvidenceV0, ContextStateV0, DecisionAuthorityV0, EntryAuthorityV0, LogCheckpointV0,
    LogEntryV0, LogForkProofV0, LogRefusal, RelyingFrontierV0, RuleResolver, RuleSlot,
    SignedLogEntryV0, TransitionV0, VerifiedCheckpointedAuthorization, VerifiedContextLog,
    CHECKPOINTED_AUTHORIZATION_DOMAIN, LOG_ENTRY_DOMAIN, LOG_TRANSITION_DOMAIN,
};
pub use statements::{
    roster_hash, target_id, AuthorizationEvidenceV0, AuthorizationV0, CanonicalV0,
    ContextInceptionV0, ContextRef, EffectV0, FounderSignature, InputActV0, OutcomeReceiptV0,
    RuleAdoptionV0, Signed, SignedDesignation, StandingV0, Verdict, WriterDesignationV0,
    AUTHORIZATION_DOMAIN, AUTHZ_V0_VERSION, CONTEXT_REF_DOMAIN, DESIGNATION_DOMAIN,
    EVIDENCE_DOMAIN, INCEPTION_DOMAIN, MAX_ACTION_LEN, MAX_FOUNDERS, MAX_INPUTS, MAX_ROLE_LEN,
    MAX_RULE_ARTIFACT, MAX_VOTE_BUNDLE, OUTCOME_DOMAIN, ROSTER_DOMAIN, RULE_ADOPTION_DOMAIN,
    STANDING_DOMAIN, SUBJECT_MATTER_DOMAIN, TARGET_DOMAIN,
};
pub use verify::{
    admit, designation_equivocators, issue_outcome_receipt, verify_authorization,
    verify_authorization_bytes, verify_outcome_receipt, Admission, AdmissionRefusal, AdoptedRule,
    AuthzRefusal, ConsumedEntry, ConsumedSet, ConsumedSetError, EquivocationProof,
    InMemoryConsumedSet, ReceiptRefusal, RuleBinding, RuleError, StatementKind,
    VerifiedAuthorization,
};
