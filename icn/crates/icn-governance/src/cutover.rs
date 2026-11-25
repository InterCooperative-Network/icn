//! Successor-domain cutover commitment (ADR-0092 §3; design-spec §2.3 as amended in session 17).
//!
//! **EXPERIMENTAL and unratified.** A [`CutoverCommitmentV1`] is what a legacy domain `L` accepts
//! when it consents to hand its decision space to a successor `S` served by the Context Authority
//! Log context `C`. It binds exactly:
//!
//! - `L` and `S`, the predecessor and successor [`GovernanceDomainId`]s;
//! - `C`, the successor context, and `h1`, the head of `C`'s genesis entry (sequence 1);
//! - `R`, the [`genesis_roster_commitment`] over `C`'s genesis standing;
//! - `M`, a commitment to the private recognition map (legacy principal to `SubjectId`).
//!
//! # What a valid commitment is not
//!
//! It is **not** verified institutional succession. It says what the predecessor's decision names;
//! it does not show that `L` accepted it (the legacy decision binding), that `L` sealed (P5), that
//! `C`'s genesis with head `h1` really carries the standing `R` commits to, or that `M` is a
//! well-formed injective map (P6). A verifier must recompute `R` from the genesis entry it has
//! checked against `h1`; `R` is a consistency check, never a trusted input.
//!
//! # `M` is opaque here
//!
//! This module neither constructs nor verifies `M`. Its row format, hiding and injectivity belong
//! to GEN-B (ADR-0092 seams B5/B6, decision D-M1). `M` is carried as 32 committed bytes; only the
//! all-zero placeholder is refused. No actor-mode byte is committed: the successor's
//! `HumanActorModel` is a property of `S`'s own domain record (decision D-R0, amendment A1).
//!
//! # `R` pins genesis
//!
//! `R` commits to the genesis standing exactly as the CAL encodes it: `(SubjectId, role)` pairs,
//! strictly ascending, at most [`MAX_INPUTS`] pairs, each role at most [`MAX_ROLE_LEN`] bytes. It
//! contains no legacy `Did`, no founder `PrincipalKey` and no recognition row. Later
//! `GrantStanding`/`RevokeStanding` transitions never change it. One `SubjectId` may hold several
//! distinct roles, as in genesis.

use icn_authz::effect_v0::{ContextRef, MAX_INPUTS, MAX_ROLE_LEN};
use icn_identity::authority_log::SubjectId;
use sha2::{Digest, Sha256};

use crate::domain::GovernanceDomainId;
use crate::institutional_domain::authority_binding::MAX_BINDING_DOMAIN_ID_BYTES;

/// Largest `L` or `S`, in UTF-8 bytes, that a cutover commitment accepts (decision D-S20-11).
///
/// This is the same bound as the P2 authority binding's, by definition rather than by copy, so
/// one constant governs every versioned institutional representation that names a domain: the
/// binding, this commitment and the P5b lifecycle record. A commitment the lifecycle record could
/// not store can therefore never be proposed. [`GovernanceDomainId`] itself stays unbounded, so a
/// longer legacy domain keeps working; it just cannot enter a cutover. Ids are bytes as given:
/// nothing is normalized or truncated.
pub const MAX_CUTOVER_DOMAIN_ID_BYTES: usize = MAX_BINDING_DOMAIN_ID_BYTES;

// The length frame is `u32be`, so the cap must fit in it (it does, by a wide margin).
const _: () = assert!(MAX_CUTOVER_DOMAIN_ID_BYTES <= u32::MAX as usize);

/// Domain tag of [`genesis_roster_commitment`] (decision D-R6).
pub const CUTOVER_ROSTER_DOMAIN: &[u8] = b"icn.governance.cutover-roster.v1";
/// Domain tag of [`CutoverCommitmentV1`]'s canonical encoding and commitment.
pub const CUTOVER_DOMAIN: &[u8] = b"icn.governance.cutover.v1";
/// Version of both preimages defined here.
pub const CUTOVER_VERSION: u16 = 1;

/// Why a cutover commitment or its roster commitment was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CutoverError {
    #[error("genesis standing has more than {MAX_INPUTS} pairs")]
    TooManyPairs,
    #[error("a genesis role is longer than {MAX_ROLE_LEN} bytes")]
    RoleTooLong,
    #[error("genesis standing is not strictly ascending (unsorted or duplicate pair)")]
    NotStrictlyAscending,
    #[error("predecessor or successor domain id is empty")]
    EmptyDomain,
    #[error("predecessor and successor domain ids are equal")]
    SameDomain,
    #[error(
        "a predecessor or successor domain id is longer than {MAX_CUTOVER_DOMAIN_ID_BYTES} bytes"
    )]
    DomainTooLong,
    #[error("the roster commitment is the empty roster's: a cutover needs a founding electorate")]
    EmptyRoster,
    #[error("the recognition-map commitment is the all-zero placeholder")]
    ZeroRecognitionMap,
    #[error("encoding: {0}")]
    Encoding(&'static str),
}

/// `R`: the commitment to a successor context's genesis standing (decisions D-R1, D-R2, D-R3).
///
/// `SHA-256( lp(tag) || u16be(1) || C || u32be(n) || for each pair: subject || lp(role) )`, where
/// `lp(x) = u32be(len x) || x`. The input must already be in the genesis order; it is refused,
/// never sorted. An empty standing is accepted, as genesis accepts it; a cutover refuses the
/// empty roster's commitment separately ([`CutoverCommitmentV1::validate`]).
pub fn genesis_roster_commitment(
    context: ContextRef,
    standing: &[(SubjectId, String)],
) -> Result<[u8; 32], CutoverError> {
    if standing.len() > MAX_INPUTS {
        return Err(CutoverError::TooManyPairs);
    }
    if standing.iter().any(|(_, role)| role.len() > MAX_ROLE_LEN) {
        return Err(CutoverError::RoleTooLong);
    }
    // The genesis decoder's own rule (`effect_v0::log`): strictly ascending pairs.
    if !standing.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(CutoverError::NotStrictlyAscending);
    }
    let mut h = Sha256::new();
    h.update(be32(CUTOVER_ROSTER_DOMAIN.len()));
    h.update(CUTOVER_ROSTER_DOMAIN);
    h.update(CUTOVER_VERSION.to_be_bytes());
    h.update(context.0);
    h.update(be32(standing.len()));
    for (subject, role) in standing {
        h.update(subject.as_bytes());
        h.update(be32(role.len()));
        h.update(role.as_bytes());
    }
    Ok(h.finalize().into())
}

/// The canonical commitment a legacy domain's cutover decision names (amendment A1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutoverCommitmentV1 {
    /// `L`, the predecessor decision space.
    pub predecessor: GovernanceDomainId,
    /// `S`, the successor decision space.
    pub successor: GovernanceDomainId,
    /// `C`, the successor's Context Authority Log context.
    pub context: ContextRef,
    /// `h1`, the head of `C`'s genesis entry (sequence 1).
    pub genesis_head: [u8; 32],
    /// `R`, [`genesis_roster_commitment`] over `C`'s genesis standing.
    pub roster_commitment: [u8; 32],
    /// `M`, the commitment to the private recognition map. Opaque here (D-M1).
    pub recognition_map_commitment: [u8; 32],
}

impl CutoverCommitmentV1 {
    /// Construct and validate.
    pub fn new(
        predecessor: GovernanceDomainId,
        successor: GovernanceDomainId,
        context: ContextRef,
        genesis_head: [u8; 32],
        roster_commitment: [u8; 32],
        recognition_map_commitment: [u8; 32],
    ) -> Result<Self, CutoverError> {
        let value = Self {
            predecessor,
            successor,
            context,
            genesis_head,
            roster_commitment,
            recognition_map_commitment,
        };
        value.validate()?;
        Ok(value)
    }

    /// The structural rules every commitment must meet. They do not establish succession.
    ///
    /// The empty-roster and all-zero-`M` refusals catch placeholders only; they prove nothing
    /// about an electorate or a map. Only `M` gets a placeholder check because it is the one field
    /// this slice cannot recompute or relate to anything: `C` and `h1` are checked by P6 against
    /// the inception and genesis evidence, and `R` is recomputed from that genesis.
    pub fn validate(&self) -> Result<(), CutoverError> {
        if self.predecessor.0.is_empty() || self.successor.0.is_empty() {
            return Err(CutoverError::EmptyDomain);
        }
        if self.predecessor.0.len() > MAX_CUTOVER_DOMAIN_ID_BYTES
            || self.successor.0.len() > MAX_CUTOVER_DOMAIN_ID_BYTES
        {
            return Err(CutoverError::DomainTooLong);
        }
        if self.predecessor.0.as_bytes() == self.successor.0.as_bytes() {
            return Err(CutoverError::SameDomain);
        }
        if self.roster_commitment == genesis_roster_commitment(self.context, &[])? {
            return Err(CutoverError::EmptyRoster);
        }
        if self.recognition_map_commitment == [0; 32] {
            return Err(CutoverError::ZeroRecognitionMap);
        }
        Ok(())
    }

    /// `lp(tag) || u16be(1) || lp(L) || lp(S) || C || h1 || R || M`.
    pub fn encode(&self) -> Result<Vec<u8>, CutoverError> {
        self.validate()?;
        let (l, s) = (self.predecessor.0.as_bytes(), self.successor.0.as_bytes());
        let mut out =
            Vec::with_capacity(4 + CUTOVER_DOMAIN.len() + 2 + 8 + l.len() + s.len() + 128);
        out.extend_from_slice(&be32(CUTOVER_DOMAIN.len()));
        out.extend_from_slice(CUTOVER_DOMAIN);
        out.extend_from_slice(&CUTOVER_VERSION.to_be_bytes());
        out.extend_from_slice(&be32(l.len()));
        out.extend_from_slice(l);
        out.extend_from_slice(&be32(s.len()));
        out.extend_from_slice(s);
        out.extend_from_slice(&self.context.0);
        out.extend_from_slice(&self.genesis_head);
        out.extend_from_slice(&self.roster_commitment);
        out.extend_from_slice(&self.recognition_map_commitment);
        Ok(out)
    }

    /// Strict inverse of [`Self::encode`]: exact tag and version, bounded lengths, valid UTF-8,
    /// no truncation and no trailing bytes, then [`Self::validate`].
    pub fn decode(bytes: &[u8]) -> Result<Self, CutoverError> {
        let mut r = Reader { bytes, pos: 0 };
        if r.lp()? != CUTOVER_DOMAIN {
            return Err(CutoverError::Encoding("wrong domain tag"));
        }
        let version = u16::from_be_bytes(r.array::<2>()?);
        if version != CUTOVER_VERSION {
            return Err(CutoverError::Encoding("unsupported version"));
        }
        let predecessor = r.text()?;
        let successor = r.text()?;
        let context = ContextRef(r.array::<32>()?);
        let genesis_head = r.array::<32>()?;
        let roster_commitment = r.array::<32>()?;
        let recognition_map_commitment = r.array::<32>()?;
        if r.pos != bytes.len() {
            return Err(CutoverError::Encoding("trailing bytes"));
        }
        Self::new(
            GovernanceDomainId(predecessor),
            GovernanceDomainId(successor),
            context,
            genesis_head,
            roster_commitment,
            recognition_map_commitment,
        )
    }

    /// `SHA-256(encode())`: the value the predecessor's decision commits to.
    pub fn commitment(&self) -> Result<[u8; 32], CutoverError> {
        Ok(Sha256::digest(self.encode()?).into())
    }
}

fn be32(len: usize) -> [u8; 4] {
    // Every caller has bounded `len` (tags, roles <= 64, pairs <= 1024, validated domain ids).
    (len as u32).to_be_bytes()
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CutoverError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(CutoverError::Encoding("truncated"))?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CutoverError> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn lp(&mut self) -> Result<&'a [u8], CutoverError> {
        let len = u32::from_be_bytes(self.array::<4>()?) as usize;
        self.take(len)
    }

    fn text(&mut self) -> Result<String, CutoverError> {
        String::from_utf8(self.lp()?.to_vec()).map_err(|_| CutoverError::Encoding("invalid UTF-8"))
    }
}
