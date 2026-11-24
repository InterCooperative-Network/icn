//! Derive a [`BindingDerivedAnchorV1`] from the **stored** institutional authority binding
//! (design spec §2.2 "EXTEND: anchor derivation", roadmap row P3). EXPERIMENTAL, unratified.
//!
//! This is a read path only. It reads the binding through [`InstitutionalAuthorityStore`] and
//! hands it, with the caller's founding evidence, to the pure verifier
//! [`BindingDerivedAnchorV1::from_binding`]. It writes nothing and mints no
//! [`crate::institutional_authority_store::VerifiedAuthorityBindingV1`].
//!
//! Every non-success is a refusal, and the store's states stay distinct:
//!
//! - no binding stored for the domain: [`StoredAnchorError::MissingBinding`];
//! - a stored value that is not a valid binding for exactly that domain (undecodable, or a
//!   transplanted record naming another domain): [`StoredAnchorError::Binding`] carrying
//!   [`AuthorityBindingError::Corrupt`];
//! - an id that cannot name a binding, or a backend failure: [`StoredAnchorError::Binding`];
//! - a well-formed record that the founding evidence does not support:
//!   [`StoredAnchorError::Derivation`].
//!
//! The stored record alone never yields an anchor: the inception and the founder-signed genesis
//! are always re-verified against it.

use std::fmt;

use icn_governance::authz_threshold::ThresholdRuleV0;
use icn_governance::institutional_admission::RecognitionRequirement;
use icn_governance::institutional_domain::anchor_derivation::{
    AnchorDerivationError, BindingDerivedAnchorV1, ContextInception, LogCheckpointV0,
    SignedLogEntryV0,
};
use icn_governance::GovernanceDomainId;

use crate::institutional_authority_store::{AuthorityBindingError, InstitutionalAuthorityStore};

/// The founding evidence and relying-party configuration for one derivation.
#[derive(Debug, Clone)]
pub struct AnchorDerivationInput<'a> {
    /// The bound context's inception (must be V1).
    pub inception: &'a ContextInception,
    /// The bound context's founder-signed genesis entry.
    pub genesis: &'a SignedLogEntryV0,
    /// The relying party's lowest acceptable position (`>= 1`).
    pub min_seq: u64,
    /// A checkpoint the relying party retained, if any.
    pub pinned: Option<LogCheckpointV0>,
    /// The governing rules the relying party evaluates.
    pub rules: Vec<ThresholdRuleV0>,
    /// Which recognition bases the relying party accepts.
    pub recognition: RecognitionRequirement,
}

/// Why no anchor was derived from the store.
#[derive(Debug)]
pub enum StoredAnchorError {
    /// No binding is stored for the domain.
    MissingBinding(GovernanceDomainId),
    /// The stored binding could not be read as a binding for the domain.
    Binding(AuthorityBindingError),
    /// The stored binding is not supported by the founding evidence.
    Derivation(AnchorDerivationError),
}

impl fmt::Display for StoredAnchorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoredAnchorError::MissingBinding(domain) => write!(
                f,
                "no institutional authority binding is stored for governance domain {:?}",
                domain.0
            ),
            StoredAnchorError::Binding(e) => write!(f, "{e}"),
            StoredAnchorError::Derivation(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoredAnchorError {}

/// Read `domain`'s stored binding and derive an anchor from it and `input`.
pub fn derive_anchor_from_store(
    store: &InstitutionalAuthorityStore,
    domain: &GovernanceDomainId,
    input: AnchorDerivationInput<'_>,
) -> Result<BindingDerivedAnchorV1, StoredAnchorError> {
    let binding = store
        .get_authority_binding(domain)
        .map_err(StoredAnchorError::Binding)?
        .ok_or_else(|| StoredAnchorError::MissingBinding(domain.clone()))?;
    BindingDerivedAnchorV1::from_binding(
        &binding,
        input.inception,
        input.genesis,
        input.min_seq,
        input.pinned,
        input.rules,
        input.recognition,
    )
    .map_err(StoredAnchorError::Derivation)
}
