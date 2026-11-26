//! P5b slice 1a: the stored predecessor-lifecycle record, its write-once witness, and the basis
//! commitment (EXPERIMENTAL; not ratified).
//!
//! This module is a **pure codec**: it encodes, decodes and cross-checks byte strings. It performs
//! no I/O, establishes nothing about institutional legitimacy, and nothing in the daemon writes
//! these records yet. The layouts follow the S20 P5b contract (§1-§2) as amended by the S24
//! maintainer decisions:
//!
//! ```text
//! lp(x) := u32be(len x) ‖ x
//!
//! record := lp("icn.governance.predecessor-lifecycle.v1") ‖ u16be(2) ‖ lp(L) ‖ u64be(revision)
//!           ‖ K_first[32] ‖ u8(state) ‖ body          (record layout version 2, S26)
//!   0x01 Legacy   : u64be(last_attempt) ‖ u8(aborted_flag) ‖ [aborted_K[32] iff flag = 1]
//!   0x02 Pending  : u64be(attempt) ‖ lp(C) ‖ K[32] ‖ basis[32] ‖ E[32]
//!   0x03 Sealed   : u64be(attempt) ‖ lp(C) ‖ K[32] ‖ basis[32] ‖ E[32] ‖ F[32]
//!
//! witness := lp("icn.governance.predecessor-lifecycle-witness.v1") ‖ u16be(1) ‖ lp(L)
//!            ‖ u64be(first_attempt) ‖ K_first[32]
//!
//! lifecycle_key(L) := "gov:predecessor-lifecycle:v1:" ‖ u32be(len L) ‖ L
//! witness_key(L)   := "gov:predecessor-lifecycle-witness:v1:" ‖ u32be(len L) ‖ L
//!
//! pin   := SHA-256( lp("icn.governance.predecessor-domain-record-pin.v1") ‖ lp(stored domain bytes) )
//! basis := SHA-256( lp("icn.governance.predecessor-lifecycle.basis.v1") ‖ u16be(1) ‖ lp(L)
//!                   ‖ u64be(r_begin) ‖ u32be(2) ‖ u8(0x01) ‖ E ‖ u8(0x02) ‖ pin )
//! ```
//!
//! * `C` is the exact canonical [`CutoverCommitmentV1`] encoding and `K = SHA-256(C)`.
//! * `E` is the electorate commitment. **Here it is an opaque 32-byte value.** This module cannot
//!   and does not verify that it commits to the institutionally authorized electorate for the
//!   succession decision; that is the job of the (future, gated) electorate constructor, whose
//!   canonical principal encoding belongs to the identity owner. Nothing here turns caller bytes
//!   into verified electorate evidence.
//! * `pin` binds the exact stored domain-record bytes. It is a **store-local consistency pin**,
//!   not portable legitimacy evidence: it detects a raw edit of L's record between Begin and Seal
//!   and nothing more.
//! * The witness is written once, with L's first Pending record, and never rewritten. A record and
//!   a witness that do not correspond are refused ([`check_witness`]). Simultaneous deletion or
//!   rollback of **both** cannot be detected without independent durable evidence.

use sha2::{Digest, Sha256};

use crate::cutover::{CutoverCommitmentV1, CUTOVER_DOMAIN, MAX_CUTOVER_DOMAIN_ID_BYTES};
use crate::domain::GovernanceDomainId;

/// Tag of the lifecycle record.
pub const LIFECYCLE_RECORD_TAG: &[u8] = b"icn.governance.predecessor-lifecycle.v1";
/// Tag of the witness.
pub const LIFECYCLE_WITNESS_TAG: &[u8] = b"icn.governance.predecessor-lifecycle-witness.v1";
/// Tag of the basis commitment.
pub const LIFECYCLE_BASIS_TAG: &[u8] = b"icn.governance.predecessor-lifecycle.basis.v1";
/// Tag of the store-local domain-record pin.
pub const DOMAIN_RECORD_PIN_TAG: &[u8] = b"icn.governance.predecessor-domain-record-pin.v1";
/// Version of every layout in this module.
pub const LIFECYCLE_VERSION: u16 = 1;
/// Version of the lifecycle **record** layout. Version 2 (S26) adds `K_first`, the first cutover
/// commitment, carried by every stored state so the witness can be cross-checked across every
/// attempt. Version 1 records were never written outside tests (no writer exists) and are
/// refused as unsupported. The witness and the basis stay at [`LIFECYCLE_VERSION`].
pub const LIFECYCLE_RECORD_VERSION: u16 = 2;
/// Key prefix of lifecycle records.
pub const LIFECYCLE_KEY_PREFIX: &[u8] = b"gov:predecessor-lifecycle:v1:";
/// Key prefix of witnesses (not under the record prefix: `-` follows `lifecycle`, not `:`).
pub const WITNESS_KEY_PREFIX: &[u8] = b"gov:predecessor-lifecycle-witness:v1:";
/// Basis category: the electorate commitment `E`.
pub const BASIS_CATEGORY_ELECTORATE: u8 = 0x01;
/// Basis category: the store-local domain-record pin.
pub const BASIS_CATEGORY_RECORD_PIN: u8 = 0x02;
/// Largest canonical cutover encoding a record may carry: the tag, version, two capped domain
/// ids and four 32-byte fields.
pub const MAX_COMMITMENT_LEN: usize =
    4 + CUTOVER_DOMAIN.len() + 2 + 2 * (4 + MAX_CUTOVER_DOMAIN_ID_BYTES) + 4 * 32;

const STATE_LEGACY: u8 = 0x01;
const STATE_PENDING: u8 = 0x02;
const STATE_SEALED: u8 = 0x03;

/// Why a record, witness, key or basis input was refused. Every refusal is final: decoding never
/// normalizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleCodecError {
    #[error("wrong tag")]
    WrongTag,
    #[error("unsupported version")]
    UnsupportedVersion,
    #[error("truncated")]
    Truncated,
    #[error("trailing bytes")]
    TrailingBytes,
    #[error("domain id is empty, not UTF-8, or longer than {MAX_CUTOVER_DOMAIN_ID_BYTES} bytes")]
    InvalidDomain,
    #[error("the record names a different predecessor than its key")]
    WrongDomain,
    #[error("revision 0 is reserved for an absent record")]
    ZeroRevision,
    #[error("unknown state tag")]
    UnknownState,
    #[error("legacy record: aborted flag is not 0 or 1")]
    InvalidAbortFlag,
    #[error("legacy record: last_attempt and aborted evidence disagree")]
    AttemptFlagMismatch,
    #[error("Legacy with no attempt is never stored")]
    InitialStateNotStored,
    #[error("attempt 0 cannot be pending or sealed")]
    ZeroAttempt,
    #[error("cutover commitment is longer than the cap")]
    CommitmentTooLong,
    #[error("cutover commitment does not decode or is not canonical")]
    InvalidCommitment,
    #[error("K is not SHA-256 of the cutover commitment")]
    CommitmentHashMismatch,
    #[error("the cutover commitment names a different predecessor")]
    ForeignCommitment,
    #[error("witness: first attempt must be at least 1")]
    WitnessZeroAttempt,
    #[error("record and witness do not correspond")]
    WitnessMismatch,
    #[error("the storage revision would overflow")]
    RevisionOverflow,
    #[error("the succession attempt would overflow")]
    AttemptOverflow,
    #[error("the first attempt's commitment does not equal the recorded first commitment")]
    FirstCommitmentMismatch,
}

/// The revision after `revision`: refuses overflow rather than wrapping. A record at
/// `u64::MAX` decodes but can never be written again (frozen).
pub fn next_revision(revision: u64) -> Result<u64> {
    revision
        .checked_add(1)
        .ok_or(LifecycleCodecError::RevisionOverflow)
}

/// The attempt after `attempt`: refuses overflow rather than wrapping.
pub fn next_attempt(attempt: u64) -> Result<u64> {
    attempt
        .checked_add(1)
        .ok_or(LifecycleCodecError::AttemptOverflow)
}

type Result<T> = std::result::Result<T, LifecycleCodecError>;

/// The stored lifecycle state. `K` is not a field: it is always `commitment.commitment()`, stored
/// redundantly and checked on decode.
// One commitment per value; values are stored singly.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredLifecycle {
    /// Ordinary operation after at least one attempt (`last_attempt ≥ 1`). `aborted` is that
    /// attempt's `K` when it was aborted. The initial state (`last_attempt = 0`) is never stored.
    Legacy {
        last_attempt: u64,
        aborted: Option<[u8; 32]>,
    },
    /// Attempt `attempt` is draining under `commitment`.
    Pending {
        attempt: u64,
        commitment: CutoverCommitmentV1,
        basis: [u8; 32],
        electorate: [u8; 32],
    },
    /// Attempt `attempt` completed. Terminal.
    Sealed {
        attempt: u64,
        commitment: CutoverCommitmentV1,
        basis: [u8; 32],
        electorate: [u8; 32],
        final_set: [u8; 32],
    },
}

impl StoredLifecycle {
    /// The latest attempt this state records.
    pub fn attempt(&self) -> u64 {
        match self {
            StoredLifecycle::Legacy { last_attempt, .. } => *last_attempt,
            StoredLifecycle::Pending { attempt, .. } | StoredLifecycle::Sealed { attempt, .. } => {
                *attempt
            }
        }
    }

    /// The `K` this state carries for its latest attempt, if any.
    fn latest_k(&self) -> Result<Option<[u8; 32]>> {
        match self {
            StoredLifecycle::Legacy { aborted, .. } => Ok(*aborted),
            StoredLifecycle::Pending { commitment, .. }
            | StoredLifecycle::Sealed { commitment, .. } => commitment
                .commitment()
                .map(Some)
                .map_err(|_| LifecycleCodecError::InvalidCommitment),
        }
    }
}

/// One stored lifecycle record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRecordV1 {
    /// `L`, the predecessor domain.
    pub predecessor: GovernanceDomainId,
    /// Storage concurrency revision (≥ 1), independent of the succession attempt.
    pub revision: u64,
    /// `K_first`: the commitment hash of L's first cutover attempt. Set by the first Begin and
    /// never changed by any later transition; it must equal the witness's on every load.
    pub first_commitment_hash: [u8; 32],
    /// The lifecycle state.
    pub state: StoredLifecycle,
}

/// The write-once witness of L's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleWitnessV1 {
    /// `L`, inside the value as well as the key, so a copied witness is refused.
    pub predecessor: GovernanceDomainId,
    /// The first succession attempt ever begun for L (≥ 1).
    pub first_attempt: u64,
    /// `K` of that attempt.
    pub first_commitment_hash: [u8; 32],
}

fn check_domain(l: &GovernanceDomainId) -> Result<()> {
    if l.0.is_empty() || l.0.len() > MAX_CUTOVER_DOMAIN_ID_BYTES {
        return Err(LifecycleCodecError::InvalidDomain);
    }
    Ok(())
}

fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
    // Every caller passes a capped domain id, a tag, or a capped commitment.
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn keyed(prefix: &[u8], l: &GovernanceDomainId) -> Result<Vec<u8>> {
    check_domain(l)?;
    let mut key = prefix.to_vec();
    lp(&mut key, l.0.as_bytes());
    Ok(key)
}

/// The store key of L's lifecycle record.
pub fn lifecycle_key(l: &GovernanceDomainId) -> Result<Vec<u8>> {
    keyed(LIFECYCLE_KEY_PREFIX, l)
}

/// The store key of L's witness.
pub fn witness_key(l: &GovernanceDomainId) -> Result<Vec<u8>> {
    keyed(WITNESS_KEY_PREFIX, l)
}

/// The store-local pin of L's exact stored domain-record bytes. Not legitimacy evidence.
pub fn domain_record_pin(stored_domain_record: &[u8]) -> [u8; 32] {
    let mut pre = Vec::with_capacity(8 + DOMAIN_RECORD_PIN_TAG.len() + stored_domain_record.len());
    lp(&mut pre, DOMAIN_RECORD_PIN_TAG);
    lp(&mut pre, stored_domain_record);
    Sha256::digest(&pre).into()
}

/// The basis commitment of a Begin fenced on lifecycle revision `r_begin`, binding the electorate
/// commitment `E` and the record `pin` under distinct category bytes.
pub fn lifecycle_basis(
    l: &GovernanceDomainId,
    r_begin: u64,
    electorate: &[u8; 32],
    pin: &[u8; 32],
) -> Result<[u8; 32]> {
    check_domain(l)?;
    let mut pre =
        Vec::with_capacity(4 + LIFECYCLE_BASIS_TAG.len() + 2 + 4 + l.0.len() + 8 + 4 + 66);
    lp(&mut pre, LIFECYCLE_BASIS_TAG);
    pre.extend_from_slice(&LIFECYCLE_VERSION.to_be_bytes());
    lp(&mut pre, l.0.as_bytes());
    pre.extend_from_slice(&r_begin.to_be_bytes());
    pre.extend_from_slice(&2u32.to_be_bytes());
    pre.push(BASIS_CATEGORY_ELECTORATE);
    pre.extend_from_slice(electorate);
    pre.push(BASIS_CATEGORY_RECORD_PIN);
    pre.extend_from_slice(pin);
    Ok(Sha256::digest(&pre).into())
}

impl LifecycleRecordV1 {
    /// Every structural rule the decoder enforces; the encoder refuses what the decoder would.
    pub fn validate(&self) -> Result<()> {
        check_domain(&self.predecessor)?;
        if self.revision == 0 {
            return Err(LifecycleCodecError::ZeroRevision);
        }
        match &self.state {
            StoredLifecycle::Legacy {
                last_attempt,
                aborted,
            } => {
                if *last_attempt == 0 {
                    return Err(LifecycleCodecError::InitialStateNotStored);
                }
                if aborted.is_none() {
                    // A stored Legacy record exists only after an attempt, which ends by abort.
                    return Err(LifecycleCodecError::AttemptFlagMismatch);
                }
            }
            StoredLifecycle::Pending {
                attempt,
                commitment,
                ..
            }
            | StoredLifecycle::Sealed {
                attempt,
                commitment,
                ..
            } => {
                if *attempt == 0 {
                    return Err(LifecycleCodecError::ZeroAttempt);
                }
                let c = commitment
                    .encode()
                    .map_err(|_| LifecycleCodecError::InvalidCommitment)?;
                if c.len() > MAX_COMMITMENT_LEN {
                    return Err(LifecycleCodecError::CommitmentTooLong);
                }
                if commitment.predecessor != self.predecessor {
                    return Err(LifecycleCodecError::ForeignCommitment);
                }
            }
        }
        // Attempts start at 1 (P5a), so while the record is at attempt 1 its current K is K_first.
        if self.state.attempt() == 1 {
            if let Some(k) = self.state.latest_k()? {
                if k != self.first_commitment_hash {
                    return Err(LifecycleCodecError::FirstCommitmentMismatch);
                }
            }
        }
        Ok(())
    }

    /// The canonical encoding.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::new();
        lp(&mut out, LIFECYCLE_RECORD_TAG);
        out.extend_from_slice(&LIFECYCLE_RECORD_VERSION.to_be_bytes());
        lp(&mut out, self.predecessor.0.as_bytes());
        out.extend_from_slice(&self.revision.to_be_bytes());
        out.extend_from_slice(&self.first_commitment_hash);
        match &self.state {
            StoredLifecycle::Legacy {
                last_attempt,
                aborted,
            } => {
                out.push(STATE_LEGACY);
                out.extend_from_slice(&last_attempt.to_be_bytes());
                match aborted {
                    Some(k) => {
                        out.push(1);
                        out.extend_from_slice(k);
                    }
                    None => out.push(0),
                }
            }
            StoredLifecycle::Pending {
                attempt,
                commitment,
                basis,
                electorate,
            } => {
                out.push(STATE_PENDING);
                encode_attempt_body(&mut out, *attempt, commitment, basis, electorate)?;
            }
            StoredLifecycle::Sealed {
                attempt,
                commitment,
                basis,
                electorate,
                final_set,
            } => {
                out.push(STATE_SEALED);
                encode_attempt_body(&mut out, *attempt, commitment, basis, electorate)?;
                out.extend_from_slice(final_set);
            }
        }
        Ok(out)
    }

    /// Decode the record stored under L's key. `expected` is the L the key names; a record for any
    /// other domain is refused.
    pub fn decode_for(expected: &GovernanceDomainId, bytes: &[u8]) -> Result<Self> {
        check_domain(expected)?;
        let mut r = Reader { bytes, pos: 0 };
        if r.lp()? != LIFECYCLE_RECORD_TAG {
            return Err(LifecycleCodecError::WrongTag);
        }
        if r.u16()? != LIFECYCLE_RECORD_VERSION {
            return Err(LifecycleCodecError::UnsupportedVersion);
        }
        let predecessor = r.domain()?;
        if predecessor != *expected {
            return Err(LifecycleCodecError::WrongDomain);
        }
        let revision = r.u64()?;
        let first_commitment_hash: [u8; 32] = r.array()?;
        let state = match r.u8()? {
            STATE_LEGACY => {
                let last_attempt = r.u64()?;
                let aborted = match r.u8()? {
                    0 => None,
                    1 => Some(r.array()?),
                    _ => return Err(LifecycleCodecError::InvalidAbortFlag),
                };
                StoredLifecycle::Legacy {
                    last_attempt,
                    aborted,
                }
            }
            STATE_PENDING => {
                let (attempt, commitment, basis, electorate) = r.attempt_body()?;
                StoredLifecycle::Pending {
                    attempt,
                    commitment,
                    basis,
                    electorate,
                }
            }
            STATE_SEALED => {
                let (attempt, commitment, basis, electorate) = r.attempt_body()?;
                StoredLifecycle::Sealed {
                    attempt,
                    commitment,
                    basis,
                    electorate,
                    final_set: r.array()?,
                }
            }
            _ => return Err(LifecycleCodecError::UnknownState),
        };
        if r.pos != bytes.len() {
            return Err(LifecycleCodecError::TrailingBytes);
        }
        let record = LifecycleRecordV1 {
            predecessor,
            revision,
            first_commitment_hash,
            state,
        };
        record.validate()?;
        Ok(record)
    }
}

fn encode_attempt_body(
    out: &mut Vec<u8>,
    attempt: u64,
    commitment: &CutoverCommitmentV1,
    basis: &[u8; 32],
    electorate: &[u8; 32],
) -> Result<()> {
    let c = commitment
        .encode()
        .map_err(|_| LifecycleCodecError::InvalidCommitment)?;
    let k = commitment
        .commitment()
        .map_err(|_| LifecycleCodecError::InvalidCommitment)?;
    out.extend_from_slice(&attempt.to_be_bytes());
    lp(out, &c);
    out.extend_from_slice(&k);
    out.extend_from_slice(basis);
    out.extend_from_slice(electorate);
    Ok(())
}

impl LifecycleWitnessV1 {
    /// The canonical encoding.
    pub fn encode(&self) -> Result<Vec<u8>> {
        check_domain(&self.predecessor)?;
        if self.first_attempt == 0 {
            return Err(LifecycleCodecError::WitnessZeroAttempt);
        }
        let mut out = Vec::new();
        lp(&mut out, LIFECYCLE_WITNESS_TAG);
        out.extend_from_slice(&LIFECYCLE_VERSION.to_be_bytes());
        lp(&mut out, self.predecessor.0.as_bytes());
        out.extend_from_slice(&self.first_attempt.to_be_bytes());
        out.extend_from_slice(&self.first_commitment_hash);
        Ok(out)
    }

    /// Decode the witness stored under L's witness key; a witness naming another domain (a
    /// transplant) is refused.
    pub fn decode_for(expected: &GovernanceDomainId, bytes: &[u8]) -> Result<Self> {
        check_domain(expected)?;
        let mut r = Reader { bytes, pos: 0 };
        if r.lp()? != LIFECYCLE_WITNESS_TAG {
            return Err(LifecycleCodecError::WrongTag);
        }
        if r.u16()? != LIFECYCLE_VERSION {
            return Err(LifecycleCodecError::UnsupportedVersion);
        }
        let predecessor = r.domain()?;
        if predecessor != *expected {
            return Err(LifecycleCodecError::WrongDomain);
        }
        let first_attempt = r.u64()?;
        if first_attempt == 0 {
            return Err(LifecycleCodecError::WitnessZeroAttempt);
        }
        let first_commitment_hash = r.array()?;
        if r.pos != bytes.len() {
            return Err(LifecycleCodecError::TrailingBytes);
        }
        Ok(LifecycleWitnessV1 {
            predecessor,
            first_attempt,
            first_commitment_hash,
        })
    }

    /// The witness written with L's first Pending record.
    pub fn for_first_pending(record: &LifecycleRecordV1) -> Result<Self> {
        record.validate()?;
        match &record.state {
            StoredLifecycle::Pending {
                attempt,
                commitment,
                ..
            } => {
                let k = commitment
                    .commitment()
                    .map_err(|_| LifecycleCodecError::InvalidCommitment)?;
                if k != record.first_commitment_hash {
                    return Err(LifecycleCodecError::FirstCommitmentMismatch);
                }
                Ok(LifecycleWitnessV1 {
                    predecessor: record.predecessor.clone(),
                    first_attempt: *attempt,
                    first_commitment_hash: k,
                })
            }
            _ => Err(LifecycleCodecError::WitnessMismatch),
        }
    }
}

/// What the presence of L's record and witness means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecyclePresence {
    /// Neither exists: L has never begun a cutover.
    NeverBegun,
    /// Both exist and correspond.
    Established(Box<LifecycleRecordV1>),
}

/// Decode and cross-check L's record and witness as read together. Any missing, malformed,
/// transplanted or contradictory combination is refused; a missing record is never Legacy.
pub fn check_witness(
    l: &GovernanceDomainId,
    record: Option<&[u8]>,
    witness: Option<&[u8]>,
) -> Result<LifecyclePresence> {
    match (record, witness) {
        (None, None) => {
            check_domain(l)?;
            Ok(LifecyclePresence::NeverBegun)
        }
        (Some(_), None) | (None, Some(_)) => Err(LifecycleCodecError::WitnessMismatch),
        (Some(rb), Some(wb)) => {
            let record = LifecycleRecordV1::decode_for(l, rb)?;
            let witness = LifecycleWitnessV1::decode_for(l, wb)?;
            // The record retains K_first across every attempt, so the witness is checked on every
            // load, not only during the first attempt.
            if record.first_commitment_hash != witness.first_commitment_hash {
                return Err(LifecycleCodecError::WitnessMismatch);
            }
            let attempt = record.state.attempt();
            if attempt < witness.first_attempt {
                return Err(LifecycleCodecError::WitnessMismatch);
            }
            if attempt == witness.first_attempt {
                if let Some(k) = record.state.latest_k()? {
                    if k != witness.first_commitment_hash {
                        return Err(LifecycleCodecError::WitnessMismatch);
                    }
                }
            }
            Ok(LifecyclePresence::Established(Box::new(record)))
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(LifecycleCodecError::Truncated)?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    fn lp(&mut self) -> Result<&'a [u8]> {
        let n = u32::from_be_bytes(self.array()?) as usize;
        self.take(n)
    }
    fn domain(&mut self) -> Result<GovernanceDomainId> {
        let n = u32::from_be_bytes(self.array()?) as usize;
        if n == 0 || n > MAX_CUTOVER_DOMAIN_ID_BYTES {
            return Err(LifecycleCodecError::InvalidDomain);
        }
        let raw = self.take(n)?;
        let s = std::str::from_utf8(raw).map_err(|_| LifecycleCodecError::InvalidDomain)?;
        Ok(GovernanceDomainId(s.to_string()))
    }
    fn attempt_body(&mut self) -> Result<(u64, CutoverCommitmentV1, [u8; 32], [u8; 32])> {
        let attempt = self.u64()?;
        let n = u32::from_be_bytes(self.array()?) as usize;
        if n > MAX_COMMITMENT_LEN {
            return Err(LifecycleCodecError::CommitmentTooLong);
        }
        let c = self.take(n)?;
        let commitment =
            CutoverCommitmentV1::decode(c).map_err(|_| LifecycleCodecError::InvalidCommitment)?;
        // Canonical: the stored bytes are exactly what the commitment encodes to.
        if commitment.encode().ok().as_deref() != Some(c) {
            return Err(LifecycleCodecError::InvalidCommitment);
        }
        let k: [u8; 32] = self.array()?;
        if Sha256::digest(c).as_slice() != k {
            return Err(LifecycleCodecError::CommitmentHashMismatch);
        }
        Ok((attempt, commitment, self.array()?, self.array()?))
    }
}
