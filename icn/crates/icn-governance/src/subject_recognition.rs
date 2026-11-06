//! GEN-B — `SubjectRecognitionV1`: an immutable, deterministic institutional recognition fact.
//!
//! Byte-level contract: `docs/architecture/GEN_SUBJECT_RECOGNITION.md`. Read that first.
//!
//! # What this is
//!
//! A governance domain records that one context-scoped human **Subject** (GEN-A) is recognised
//! at the standing class [`StandingClass::Member`], under a declared recognition basis. The fact
//! is the hash of its committed fields and nothing else:
//!
//! ```text
//! domain_id + SubjectId + standing_class + recognition_basis_kind + recognition_basis_hash
//!     -> recognition_hash
//! ```
//!
//! # What this is not
//!
//! It is not a vote, a permission grant, a session token or an authorization decision. It does
//! not prove membership legitimacy, voting eligibility, economic authority, or that any device is
//! *currently* authorized (GEN-A verification is a historical fact, not current authority). The
//! only supported basis in v1 is [`RecognitionBasisKind::BootstrapRehearsalV1`], which is
//! explicitly **non-production**: no production institution's decision is represented by it.
//!
//! # Why the constructor takes the genesis bundle
//!
//! `VerifiedSubjectContext` does not carry the context kind or context id, and its fields are
//! public, so a value of it cannot prove which domain it was verified for. The constructor
//! therefore takes the [`SubjectContextGenesisV1`] bundle and re-runs
//! `verify_subject_context_genesis_v1` against the recognising domain's exact id bytes. A
//! recognition cannot be assembled from a Subject and a domain that were never verified together.

use icn_identity::authority_log::SubjectId;
use icn_identity::subject_context::{
    verify_subject_context_genesis_v1, GenesisVerifyError, SubjectContextGenesisV1,
    SubjectContextKind, SubjectContextRef,
};
use icn_kernel_api::canonical::{CanonicalHasher, DomainTag};
use thiserror::Error;

use crate::domain::GovernanceDomainId;

/// Domain-separation tag of the recognition hash (`CANONICAL_ENCODING.md` §4.1.3).
pub const SUBJECT_RECOGNITION_TAG: DomainTag = DomainTag::new(b"icn:gov:subject-recognition:v1");

/// Domain-separation tag of the bootstrap-rehearsal basis hash.
pub const BOOTSTRAP_REHEARSAL_BASIS_TAG: DomainTag =
    DomainTag::new(b"icn:gov:recognition-basis:bootstrap-rehearsal:v1");

/// The standing a domain recognises a Subject at. Only `Member` exists in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandingClass {
    /// Member standing. Wire tag `0x01`.
    Member,
}

impl StandingClass {
    /// The committed one-byte tag.
    pub const fn tag(self) -> u8 {
        match self {
            StandingClass::Member => 0x01,
        }
    }

    /// Parse a committed tag; anything but `0x01` is refused.
    pub fn from_tag(tag: u8) -> Result<Self, RecognitionError> {
        match tag {
            0x01 => Ok(StandingClass::Member),
            other => Err(RecognitionError::UnsupportedStandingClass(other)),
        }
    }
}

/// The kind of evidence a recognition rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecognitionBasisKind {
    /// A non-production bootstrap-rehearsal basis for a fresh Alpha Subject. Wire tag `0x01`.
    BootstrapRehearsalV1,
}

impl RecognitionBasisKind {
    /// The committed one-byte tag.
    pub const fn tag(self) -> u8 {
        match self {
            RecognitionBasisKind::BootstrapRehearsalV1 => 0x01,
        }
    }

    /// Parse a committed tag; anything but `0x01` is refused.
    pub fn from_tag(tag: u8) -> Result<Self, RecognitionError> {
        match tag {
            0x01 => Ok(RecognitionBasisKind::BootstrapRehearsalV1),
            other => Err(RecognitionError::UnsupportedBasisKind(other)),
        }
    }

    /// Whether this basis may stand in for a production institution's authorized decision.
    ///
    /// Always `false` for every v1 kind. Consumers that must not treat rehearsal recognition as
    /// real membership should branch on this rather than on the enum variant.
    pub const fn is_production_grade(self) -> bool {
        match self {
            RecognitionBasisKind::BootstrapRehearsalV1 => false,
        }
    }
}

/// A 32-byte basis hash. Opaque; produced by a basis-specific derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecognitionBasisHash([u8; 32]);

impl RecognitionBasisHash {
    /// The raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The recognition hash: the identity of a recognition fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecognitionHash([u8; 32]);

impl RecognitionHash {
    /// The raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Why a recognition could not be constructed or verified. Every variant is a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecognitionError {
    /// The recognising domain id is empty.
    #[error("recognising domain id must not be empty")]
    EmptyDomain,
    /// The genesis bundle did not verify for this domain (wrong domain, kind, bad evidence...).
    #[error("subject-context genesis does not verify for this domain: {0}")]
    Genesis(#[from] GenesisVerifyError),
    /// A standing-class tag outside the v1 set.
    #[error("unsupported standing class tag {0:#04x}")]
    UnsupportedStandingClass(u8),
    /// A basis-kind tag outside the v1 set.
    #[error("unsupported recognition basis kind tag {0:#04x}")]
    UnsupportedBasisKind(u8),
    /// The recognition is not the one this genesis and domain produce.
    #[error("recognition does not match the supplied genesis and domain")]
    Mismatch,
}

/// The bootstrap-rehearsal basis hash. Exposed so vectors can pin the layout.
///
/// Commits the domain, the Subject and the GEN-A `subject_context_ref`, so a basis cannot be
/// replayed for another domain, another Subject, or another context incarnation.
///
/// Layout: `tag || u64le(len(domain)) || domain_utf8 || b32(subject) || b32(subject_context_ref)`.
pub fn bootstrap_rehearsal_basis_hash(
    domain_id: &str,
    subject: &SubjectId,
    context_ref: &SubjectContextRef,
) -> RecognitionBasisHash {
    let mut h = CanonicalHasher::new(BOOTSTRAP_REHEARSAL_BASIS_TAG);
    h.str(domain_id)
        .digest(subject.as_bytes())
        .digest(context_ref.as_bytes());
    RecognitionBasisHash(h.finish())
}

/// The recognition hash over already-committed fields. Exposed so vectors can pin the layout.
///
/// Layout (§4.1.3, every variable field length-prefixed, every fixed field raw):
/// `tag || u64le(len(domain)) || domain_utf8 || b32(subject) || u8(standing) || u8(basis_kind)
/// || b32(basis_hash)`.
pub fn compute_recognition_hash(
    domain_id: &str,
    subject: &SubjectId,
    standing: StandingClass,
    basis_kind: RecognitionBasisKind,
    basis_hash: &RecognitionBasisHash,
) -> RecognitionHash {
    let mut h = CanonicalHasher::new(SUBJECT_RECOGNITION_TAG);
    h.str(domain_id)
        .digest(subject.as_bytes())
        .fixed(&[standing.tag()])
        .fixed(&[basis_kind.tag()])
        .digest(basis_hash.as_bytes());
    RecognitionHash(h.finish())
}

/// An immutable institutional recognition of one Subject. Fields are private; there is no serde
/// or wire form in v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRecognitionV1 {
    domain_id: GovernanceDomainId,
    subject: SubjectId,
    standing_class: StandingClass,
    basis_kind: RecognitionBasisKind,
    basis_hash: RecognitionBasisHash,
    recognition_hash: RecognitionHash,
}

impl SubjectRecognitionV1 {
    /// Recognise the Subject of `genesis` as a `Member` of `domain` under the non-production
    /// bootstrap-rehearsal basis.
    ///
    /// The genesis is verified against the domain's exact id bytes (kind must be
    /// `GovernanceDomainV1`). The Subject is *derived* from that verification; it is never
    /// supplied by the caller, so no Principal, legacy DID or foreign Subject can be substituted.
    pub fn recognize_bootstrap_rehearsal(
        domain: &GovernanceDomainId,
        genesis: &SubjectContextGenesisV1,
    ) -> Result<Self, RecognitionError> {
        if domain.0.is_empty() {
            return Err(RecognitionError::EmptyDomain);
        }
        let verified = verify_subject_context_genesis_v1(
            genesis,
            SubjectContextKind::GovernanceDomainV1,
            &domain.0,
        )?;
        let basis_hash = bootstrap_rehearsal_basis_hash(
            &domain.0,
            &verified.subject,
            &verified.subject_context_ref,
        );
        let standing_class = StandingClass::Member;
        let basis_kind = RecognitionBasisKind::BootstrapRehearsalV1;
        let recognition_hash = compute_recognition_hash(
            &domain.0,
            &verified.subject,
            standing_class,
            basis_kind,
            &basis_hash,
        );
        Ok(Self {
            domain_id: domain.clone(),
            subject: verified.subject,
            standing_class,
            basis_kind,
            basis_hash,
            recognition_hash,
        })
    }

    /// Re-derive the recognition from `genesis` and the recognition's own domain, and require it
    /// to equal `self` in every committed field. This is the check a relying party runs.
    ///
    /// It proves the recognition was *derivable* from that genesis. It says nothing about whether
    /// the device is currently authorized or whether any institution actually decided anything.
    pub fn verify_against_genesis(
        &self,
        genesis: &SubjectContextGenesisV1,
    ) -> Result<(), RecognitionError> {
        let expected = Self::recognize_bootstrap_rehearsal(&self.domain_id, genesis)?;
        if &expected == self {
            Ok(())
        } else {
            Err(RecognitionError::Mismatch)
        }
    }

    /// The recognising domain.
    pub fn domain_id(&self) -> &GovernanceDomainId {
        &self.domain_id
    }

    /// The recognised Subject (never a device Principal).
    pub fn subject(&self) -> &SubjectId {
        &self.subject
    }

    /// The recognised standing.
    pub fn standing_class(&self) -> StandingClass {
        self.standing_class
    }

    /// The basis kind.
    pub fn basis_kind(&self) -> RecognitionBasisKind {
        self.basis_kind
    }

    /// The basis hash.
    pub fn basis_hash(&self) -> &RecognitionBasisHash {
        &self.basis_hash
    }

    /// The recognition hash.
    pub fn recognition_hash(&self) -> &RecognitionHash {
        &self.recognition_hash
    }
}
