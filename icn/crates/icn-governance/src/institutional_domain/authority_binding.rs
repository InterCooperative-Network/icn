//! The institutional authority binding: the set-once record saying that the Context Authority
//! Log context `C` serves the governance decision space `D` (design spec §2.2, A-form = A-2).
//! EXPERIMENTAL, unratified.
//!
//! # A sibling record, not a field
//!
//! The binding is **not** a field on [`super::InstitutionalDomain`]. That struct is lenient JSON,
//! so a binary that predates the field would silently drop it on its next save. The binding is a
//! separate, strictly encoded record under its own store key, which an older binary never names
//! and therefore never rewrites. [`super::InstitutionalDomain`]'s serialized form is unchanged.
//!
//! # What a binding is, and what it is not
//!
//! A [`InstitutionalAuthorityBindingV1`] names three distinct identifiers and derives none of
//! them from another (design spec Part 3, ruling E):
//!
//! - `domain`: a [`GovernanceDomainId`], the decision space;
//! - `context`: a [`ContextRef`], the authority log;
//! - no `EntityId`. The binding says nothing about which institution owns `D`.
//!
//! A decoded or stored binding is **a record, not evidence.** Constructing one checks only its
//! internal consistency (`served_subject == served_domain_commitment(domain)`, non-zero hashes,
//! known basis). It does not check that `C`'s founders exist, that `C`'s inception really commits
//! to `D`, or that the basis decisions were taken. Those are the verifiers' job (design spec
//! §2.3, §2.4, P3 and P6), and nothing in this module authorizes anything.
//!
//! # Canonical encoding
//!
//! ```text
//! lp(AUTHORITY_BINDING_DOMAIN) || u16be(1) || lp(utf8 D) || C || h1 || served || u8 basis
//!   || (basis 1: lp(utf8 L) || decision_hash || cutover_commitment
//!    | basis 2: declaration_decision_hash || mandate_id)
//! ```
//!
//! where `lp(x) = u32be(len x) || x`. Decoding is strict: a wrong tag or version, an unknown
//! basis, an empty or over-long id, invalid UTF-8, a zero hash, trailing bytes, or
//! `served != served_domain_commitment(D)` is an error, never a default. Ids are compared and
//! committed as their exact UTF-8 bytes; there is no Unicode normalization.
//!
//! There is no timestamp: establishment time is not authority evidence.

use crate::domain::{served_domain_commitment, GovernanceDomainId, ServedDomainError};

pub use icn_authz::effect_v0::ContextRef;

/// Leading domain tag of a canonical [`InstitutionalAuthorityBindingV1`] (EXPERIMENTAL).
pub const AUTHORITY_BINDING_DOMAIN: &[u8] = b"icn.governance.institutional-authority-binding.v1";

/// Version number following the tag.
pub const AUTHORITY_BINDING_VERSION: u16 = 1;

/// Fixed prefix of a binding's store key. The full key is
/// `prefix || u32be(len utf8 D) || utf8 D` (see [`authority_binding_key`]).
pub const AUTHORITY_BINDING_KEY_PREFIX: &[u8] = b"gov:institutional-authority:v1:";

/// Largest accepted governance domain id, in UTF-8 bytes, for either `D` or a predecessor `L`.
///
/// A bound on what a binding may name, so decoding never allocates an attacker-chosen length.
/// [`GovernanceDomainId`] itself is unbounded, so a longer id (which `served_domain_commitment`
/// accepts) can never be bound: establishment fails closed with
/// [`AuthorityBindingCodecError::IdTooLong`]. A liveness limit, not a safety one.
pub const MAX_BINDING_DOMAIN_ID_BYTES: usize = 1024;

const BASIS_PREDECESSOR_CUTOVER: u8 = 1;
const BASIS_DECLARED_GENESIS: u8 = 2;

/// Why a binding could not be built, encoded or decoded.
///
/// For a stored record, every variant means **corrupt**, never "unbound".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthorityBindingCodecError {
    /// The input ended inside `field`.
    #[error("binding ended inside {field}")]
    UnexpectedEnd {
        /// The field being read.
        field: &'static str,
    },
    /// The leading tag is not [`AUTHORITY_BINDING_DOMAIN`].
    #[error("not an institutional authority binding (wrong domain tag)")]
    WrongDomain,
    /// The version is not [`AUTHORITY_BINDING_VERSION`].
    #[error("unsupported binding version {0}")]
    UnsupportedVersion(u16),
    /// A governance domain id is empty.
    #[error("{field} is empty")]
    EmptyId {
        /// Which id.
        field: &'static str,
    },
    /// A governance domain id exceeds [`MAX_BINDING_DOMAIN_ID_BYTES`].
    #[error("{field} is {len} bytes, over the {max}-byte bound")]
    IdTooLong {
        /// Which id.
        field: &'static str,
        /// Its length.
        len: usize,
        /// The bound.
        max: usize,
    },
    /// A governance domain id is not UTF-8.
    #[error("{field} is not UTF-8")]
    InvalidUtf8 {
        /// Which id.
        field: &'static str,
    },
    /// A 32-byte hash or commitment is all zeros, which no real value is.
    #[error("{field} is all zeros")]
    ZeroHash {
        /// Which field.
        field: &'static str,
    },
    /// `served_subject` is not `served_domain_commitment(domain)`.
    #[error("served_subject does not commit to the bound domain")]
    ServedMismatch,
    /// The basis byte names no known basis.
    #[error("unknown binding basis {0}")]
    UnknownBasis(u8),
    /// A successor names itself as its own predecessor.
    #[error("a domain cannot be its own predecessor")]
    SelfPredecessor,
    /// Bytes remain after a complete binding.
    #[error("{0} trailing bytes after the binding")]
    TrailingBytes(usize),
}

/// Why `D` is bound to its context: the basis decision(s), each carried as an opaque hash.
///
/// The hashes are not verified here; the verifiers that mint a write capability do that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingBasisV1 {
    /// `D` succeeds the sealed predecessor `L` (ruling C).
    PredecessorCutover {
        /// The predecessor decision space `L`. Non-empty, and not `D` itself.
        predecessor: GovernanceDomainId,
        /// `L`'s accepted receipt `decision_hash`.
        decision_hash: [u8; 32],
        /// The cutover commitment `L` decided.
        cutover_commitment: [u8; 32],
    },
    /// `D` is a genuinely new decision space with no predecessor (design spec §2.4).
    DeclaredGenesis {
        /// The declaring decision's hash.
        declaration_decision_hash: [u8; 32],
        /// The mandate under which it was declared.
        mandate_id: [u8; 32],
    },
}

/// The set-once binding of decision space `D` to authority context `C` (EXPERIMENTAL).
///
/// Every instance is internally consistent: fields are private and only [`Self::new`] and
/// [`Self::decode`] build one, and both run the same checks. See the module docs for what that
/// consistency does **not** establish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstitutionalAuthorityBindingV1 {
    domain: GovernanceDomainId,
    context: ContextRef,
    genesis_head: [u8; 32],
    served_subject: [u8; 32],
    basis: BindingBasisV1,
}

impl InstitutionalAuthorityBindingV1 {
    /// Build a binding, computing `served_subject = served_domain_commitment(domain)`.
    pub fn new(
        domain: GovernanceDomainId,
        context: ContextRef,
        genesis_head: [u8; 32],
        basis: BindingBasisV1,
    ) -> Result<Self, AuthorityBindingCodecError> {
        let served_subject = commitment_of(&domain, "domain")?;
        let binding = Self {
            domain,
            context,
            genesis_head,
            served_subject,
            basis,
        };
        binding.validate()?;
        Ok(binding)
    }

    /// The decision space `D`.
    pub fn domain(&self) -> &GovernanceDomainId {
        &self.domain
    }

    /// The authority context `C`.
    pub fn context(&self) -> ContextRef {
        self.context
    }

    /// The head of `C`'s genesis entry (`seq` 1).
    pub fn genesis_head(&self) -> [u8; 32] {
        self.genesis_head
    }

    /// `served_domain_commitment(domain)`, which `C`'s inception must carry.
    pub fn served_subject(&self) -> [u8; 32] {
        self.served_subject
    }

    /// Why the binding exists.
    pub fn basis(&self) -> &BindingBasisV1 {
        &self.basis
    }

    fn validate(&self) -> Result<(), AuthorityBindingCodecError> {
        check_id(&self.domain, "domain")?;
        if self.served_subject != commitment_of(&self.domain, "domain")? {
            return Err(AuthorityBindingCodecError::ServedMismatch);
        }
        non_zero(&self.context.0, "context")?;
        non_zero(&self.genesis_head, "genesis_head")?;
        match &self.basis {
            BindingBasisV1::PredecessorCutover {
                predecessor,
                decision_hash,
                cutover_commitment,
            } => {
                check_id(predecessor, "predecessor")?;
                if predecessor.0.as_bytes() == self.domain.0.as_bytes() {
                    return Err(AuthorityBindingCodecError::SelfPredecessor);
                }
                non_zero(decision_hash, "decision_hash")?;
                non_zero(cutover_commitment, "cutover_commitment")?;
            }
            BindingBasisV1::DeclaredGenesis {
                declaration_decision_hash,
                mandate_id,
            } => {
                non_zero(declaration_decision_hash, "declaration_decision_hash")?;
                non_zero(mandate_id, "mandate_id")?;
            }
        }
        Ok(())
    }

    /// The canonical bytes (see the module docs).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_lp(&mut out, AUTHORITY_BINDING_DOMAIN);
        out.extend_from_slice(&AUTHORITY_BINDING_VERSION.to_be_bytes());
        put_lp(&mut out, self.domain.0.as_bytes());
        out.extend_from_slice(&self.context.0);
        out.extend_from_slice(&self.genesis_head);
        out.extend_from_slice(&self.served_subject);
        match &self.basis {
            BindingBasisV1::PredecessorCutover {
                predecessor,
                decision_hash,
                cutover_commitment,
            } => {
                out.push(BASIS_PREDECESSOR_CUTOVER);
                put_lp(&mut out, predecessor.0.as_bytes());
                out.extend_from_slice(decision_hash);
                out.extend_from_slice(cutover_commitment);
            }
            BindingBasisV1::DeclaredGenesis {
                declaration_decision_hash,
                mandate_id,
            } => {
                out.push(BASIS_DECLARED_GENESIS);
                out.extend_from_slice(declaration_decision_hash);
                out.extend_from_slice(mandate_id);
            }
        }
        out
    }

    /// Strictly decode canonical bytes. Any deviation is an error (see the module docs).
    pub fn decode(bytes: &[u8]) -> Result<Self, AuthorityBindingCodecError> {
        let mut r = Reader { buf: bytes, pos: 0 };
        let tag = r.lp("domain tag", AUTHORITY_BINDING_DOMAIN.len())?;
        if tag != AUTHORITY_BINDING_DOMAIN {
            return Err(AuthorityBindingCodecError::WrongDomain);
        }
        let version = u16::from_be_bytes(r.array("version")?);
        if version != AUTHORITY_BINDING_VERSION {
            return Err(AuthorityBindingCodecError::UnsupportedVersion(version));
        }
        let domain = r.id("domain")?;
        let context = ContextRef(r.array("context")?);
        let genesis_head = r.array("genesis_head")?;
        let served_subject = r.array("served_subject")?;
        let basis = match r.array::<1>("basis")?[0] {
            BASIS_PREDECESSOR_CUTOVER => BindingBasisV1::PredecessorCutover {
                predecessor: r.id("predecessor")?,
                decision_hash: r.array("decision_hash")?,
                cutover_commitment: r.array("cutover_commitment")?,
            },
            BASIS_DECLARED_GENESIS => BindingBasisV1::DeclaredGenesis {
                declaration_decision_hash: r.array("declaration_decision_hash")?,
                mandate_id: r.array("mandate_id")?,
            },
            other => return Err(AuthorityBindingCodecError::UnknownBasis(other)),
        };
        let rest = bytes.len().saturating_sub(r.pos);
        if rest != 0 {
            return Err(AuthorityBindingCodecError::TrailingBytes(rest));
        }
        let binding = Self {
            domain,
            context,
            genesis_head,
            served_subject,
            basis,
        };
        binding.validate()?;
        Ok(binding)
    }
}

/// The store key of `domain`'s binding: `AUTHORITY_BINDING_KEY_PREFIX || u32be(len) || utf8 D`.
///
/// Length-framed, so no id (including one containing `:`) can produce another id's key or a key
/// under another id's prefix. Refuses an id a binding could not hold.
pub fn authority_binding_key(
    domain: &GovernanceDomainId,
) -> Result<Vec<u8>, AuthorityBindingCodecError> {
    check_id(domain, "domain")?;
    let mut key = AUTHORITY_BINDING_KEY_PREFIX.to_vec();
    put_lp(&mut key, domain.0.as_bytes());
    Ok(key)
}

fn check_id(
    id: &GovernanceDomainId,
    field: &'static str,
) -> Result<(), AuthorityBindingCodecError> {
    let len = id.0.len();
    if len == 0 {
        return Err(AuthorityBindingCodecError::EmptyId { field });
    }
    if len > MAX_BINDING_DOMAIN_ID_BYTES {
        return Err(AuthorityBindingCodecError::IdTooLong {
            field,
            len,
            max: MAX_BINDING_DOMAIN_ID_BYTES,
        });
    }
    Ok(())
}

fn commitment_of(
    id: &GovernanceDomainId,
    field: &'static str,
) -> Result<[u8; 32], AuthorityBindingCodecError> {
    check_id(id, field)?;
    served_domain_commitment(id).map_err(|e| match e {
        ServedDomainError::Empty => AuthorityBindingCodecError::EmptyId { field },
        ServedDomainError::TooLong => AuthorityBindingCodecError::IdTooLong {
            field,
            len: id.0.len(),
            max: MAX_BINDING_DOMAIN_ID_BYTES,
        },
    })
}

fn non_zero(value: &[u8; 32], field: &'static str) -> Result<(), AuthorityBindingCodecError> {
    if value.iter().all(|b| *b == 0) {
        return Err(AuthorityBindingCodecError::ZeroHash { field });
    }
    Ok(())
}

/// `u32be(len) || bytes`. Every caller passes at most [`MAX_BINDING_DOMAIN_ID_BYTES`] or a tag.
fn put_lp(out: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(
        &mut self,
        n: usize,
        field: &'static str,
    ) -> Result<&'a [u8], AuthorityBindingCodecError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(AuthorityBindingCodecError::UnexpectedEnd { field })?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(AuthorityBindingCodecError::UnexpectedEnd { field })?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(
        &mut self,
        field: &'static str,
    ) -> Result<[u8; N], AuthorityBindingCodecError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N, field)?);
        Ok(out)
    }

    /// A length-prefixed byte string of at most `max` bytes. An over-long length is refused
    /// before anything is read.
    fn lp(
        &mut self,
        field: &'static str,
        max: usize,
    ) -> Result<&'a [u8], AuthorityBindingCodecError> {
        let len = u32::from_be_bytes(self.array(field)?) as usize;
        if len > max {
            return Err(match field {
                "domain tag" => AuthorityBindingCodecError::WrongDomain,
                _ => AuthorityBindingCodecError::IdTooLong { field, len, max },
            });
        }
        self.take(len, field)
    }

    fn id(
        &mut self,
        field: &'static str,
    ) -> Result<GovernanceDomainId, AuthorityBindingCodecError> {
        let raw = self.lp(field, MAX_BINDING_DOMAIN_ID_BYTES)?;
        if raw.is_empty() {
            return Err(AuthorityBindingCodecError::EmptyId { field });
        }
        let text = std::str::from_utf8(raw)
            .map_err(|_| AuthorityBindingCodecError::InvalidUtf8 { field })?;
        Ok(GovernanceDomainId(text.to_owned()))
    }
}
