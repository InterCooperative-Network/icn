//! Authenticated governance replication envelope (#2469, slice 1).
//!
//! # What this module is
//!
//! A [`SignedGovernanceOp`] binds a governance operation to the DID that authored it, to
//! the governance domain it affects, and to the membership snapshot under which that DID
//! held authority. It is the content-layer primitive for authenticated governance
//! replication.
//!
//! **This module is deliberately unwired.** Nothing here reads or writes governance state,
//! subscribes to gossip, or lifts the #2470 containment. It provides the type, its
//! canonical encoding, signing and verification; ingress wiring is a later slice.
//!
//! # Why the signature lives here and not at the transport layer
//!
//! `SignedEnvelope` (`icn-net`) authenticates a *hop*: a relaying node re-serves a stored
//! entry verbatim under its own envelope signature, so the original author's identity does
//! not survive the relay. Authenticating the author of replicated content therefore
//! requires a signature *inside* the replicated bytes, which is what this type is.
//!
//! # Why `op_bytes` is opaque
//!
//! The signature covers the operation's bytes exactly as transmitted, and the receiver
//! verifies over the bytes it received rather than re-serializing. Serialization
//! determinism is therefore never relied upon — a strictly stronger position than
//! canonicalizing [`GovernanceMessage`], and the same discipline `SignedEnvelope` uses for
//! its payload. The envelope's *own* fields, by contrast, are canonically encoded here
//! (§[`SignedGovernanceOp::canonical_body`]).
//!
//! See `docs/design/authenticated-governance-replication.md`.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{GovernanceDomainId, GovernanceMessage};
use icn_identity::Did;

/// Frame magic and version tag. Also the domain separator for the signed body, so a
/// signature over an envelope can never be reinterpreted under another protocol.
pub const GOV_OP_MAGIC: &[u8] = b"ICN-GOV-OP-v1\0";

/// Envelope version understood by this implementation.
pub const GOV_OP_V1: u16 = 1;

/// Domain separator for the static-membership authority snapshot.
const MEMBERSHIP_HASH_DOMAIN: &[u8] = b"ICN-GOV-STATIC-MEMBERSHIP-v1\0";

/// Errors from encoding, decoding or verifying a [`SignedGovernanceOp`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplicationEnvelopeError {
    /// The frame did not begin with [`GOV_OP_MAGIC`].
    #[error("not a governance replication frame: bad magic")]
    BadMagic,

    /// The frame declared a version this implementation does not understand.
    #[error("unsupported governance replication envelope version: {0}")]
    UnsupportedVersion(u16),

    /// The frame ended in the middle of a field.
    #[error("truncated governance replication frame")]
    Truncated,

    /// A length prefix exceeded the remaining frame, or a field was malformed.
    #[error("malformed governance replication frame: {0}")]
    Malformed(String),

    /// The authority discriminant is not one this version defines.
    #[error("unknown replication authority discriminant: {0}")]
    UnknownAuthority(u8),

    /// The operation-kind discriminant is not one this version defines.
    #[error("unknown governance operation kind discriminant: {0}")]
    UnknownOpKind(u8),

    /// The author DID does not yield a usable Ed25519 verifying key.
    #[error("author DID is not a usable verifying key: {0}")]
    UnusableAuthorKey(String),

    /// The signature is not 64 bytes, or does not verify under the author's key.
    #[error("signature does not verify for the declared author")]
    SignatureInvalid,

    /// `op_bytes` did not decode as a [`GovernanceMessage`].
    #[error("operation payload did not decode: {0}")]
    PayloadUndecodable(String),

    /// The decoded payload is a different variant than `op_kind` declares.
    #[error("operation kind {declared:?} disagrees with payload variant {actual}")]
    OpKindMismatch {
        /// The kind named in the signed envelope.
        declared: GovernanceOpKind,
        /// The variant the payload actually decoded to.
        actual: &'static str,
    },

    /// The message variant cannot be carried in a replication envelope.
    #[error("governance message variant {0} is not a replicable state mutation")]
    NotReplicable(&'static str),
}

/// The basis on which an author claims authority over a domain.
///
/// One variant today. It exists as an enum so that a future basis (a membership witness
/// set, an `AuthorityGrant`, a charter binding) is a *new discriminant* that this
/// version's verifier rejects explicitly, rather than a reinterpretation of these bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationAuthority {
    /// Authority derives from membership in a `MembershipSource::StaticList` domain,
    /// pinned to the exact member set via [`static_membership_hash`].
    StaticMembership {
        /// Canonical hash of the domain's member set at authoring time.
        membership_hash: [u8; 32],
    },
}

impl ReplicationAuthority {
    /// Wire discriminant. Stable across versions; never reuse a retired value.
    const fn discriminant(&self) -> u8 {
        match self {
            Self::StaticMembership { .. } => 1,
        }
    }
}

/// The state-mutating governance operations a replication envelope may carry.
///
/// Deliberately not a mirror of every [`GovernanceMessage`] variant: only operations that
/// mutate durable governance state can ever be replicated, and the pre-containment ingress
/// persisted exactly these eight. Comment, deliberation and reaction messages were never
/// persisted by the replication path and are therefore not replicable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GovernanceOpKind {
    /// [`GovernanceMessage::DomainCreated`]
    DomainCreated,
    /// [`GovernanceMessage::DomainUpdated`]
    DomainUpdated,
    /// [`GovernanceMessage::ProposalCreated`]
    ProposalCreated,
    /// [`GovernanceMessage::ProposalOpened`]
    ProposalOpened,
    /// [`GovernanceMessage::VoteCast`]
    VoteCast,
    /// [`GovernanceMessage::ProposalClosed`]
    ProposalClosed,
    /// [`GovernanceMessage::DelegationCreated`]
    DelegationCreated,
    /// [`GovernanceMessage::DelegationRevoked`]
    DelegationRevoked,
}

/// Operation kinds authenticated replication v1 is permitted to apply.
///
/// Only `VoteCast`: its storage key `gov:vote:{proposal}:{voter}` already embeds the voter
/// DID, so once the voter is authenticated a cross-author key collision is impossible by
/// construction. Every other kind either has no authority root (`DomainCreated`,
/// `DomainUpdated`), has a caller-chosen identifier that permits cross-author collision
/// (`ProposalCreated`, `DelegationCreated`), or has an undecided state-transition authority
/// (`ProposalOpened`, `ProposalClosed`).
///
/// This is a **declaration**, not an enforcement point — enforcement lands with the ingress
/// slice. It lives here so the decision is version-controlled next to the wire format.
pub const V1_RESTORABLE_OP_KINDS: &[GovernanceOpKind] = &[GovernanceOpKind::VoteCast];

impl GovernanceOpKind {
    /// Wire discriminant. Stable across versions; never reuse a retired value.
    const fn discriminant(&self) -> u8 {
        match self {
            Self::DomainCreated => 1,
            Self::DomainUpdated => 2,
            Self::ProposalCreated => 3,
            Self::ProposalOpened => 4,
            Self::VoteCast => 5,
            Self::ProposalClosed => 6,
            Self::DelegationCreated => 7,
            Self::DelegationRevoked => 8,
        }
    }

    const fn from_discriminant(d: u8) -> Option<Self> {
        match d {
            1 => Some(Self::DomainCreated),
            2 => Some(Self::DomainUpdated),
            3 => Some(Self::ProposalCreated),
            4 => Some(Self::ProposalOpened),
            5 => Some(Self::VoteCast),
            6 => Some(Self::ProposalClosed),
            7 => Some(Self::DelegationCreated),
            8 => Some(Self::DelegationRevoked),
            _ => None,
        }
    }

    /// Classify a message, or `None` if it is not a replicable state mutation.
    pub fn from_message(msg: &GovernanceMessage) -> Option<Self> {
        match msg {
            GovernanceMessage::DomainCreated { .. } => Some(Self::DomainCreated),
            GovernanceMessage::DomainUpdated { .. } => Some(Self::DomainUpdated),
            GovernanceMessage::ProposalCreated { .. } => Some(Self::ProposalCreated),
            GovernanceMessage::ProposalOpened { .. } => Some(Self::ProposalOpened),
            GovernanceMessage::VoteCast { .. } => Some(Self::VoteCast),
            GovernanceMessage::ProposalClosed { .. } => Some(Self::ProposalClosed),
            GovernanceMessage::DelegationCreated { .. } => Some(Self::DelegationCreated),
            GovernanceMessage::DelegationRevoked { .. } => Some(Self::DelegationRevoked),
            _ => None,
        }
    }

    /// Whether authenticated replication v1 may apply this kind.
    pub fn is_restorable_in_v1(&self) -> bool {
        V1_RESTORABLE_OP_KINDS.contains(self)
    }
}

/// Canonical hash of a `StaticList` domain's member set.
///
/// The member DIDs are sorted bytewise and deduplicated, so the hash is a function of the
/// *set* and not of the order the caller happened to supply. `domain_id` is bound inside
/// the hash so a snapshot cannot be lifted between two domains that share a member set.
///
/// Length-prefixed throughout: no member DID can be split or joined across a boundary to
/// forge a different set with the same hash.
pub fn static_membership_hash(domain_id: &GovernanceDomainId, members: &[Did]) -> [u8; 32] {
    let mut canonical: Vec<&str> = members.iter().map(Did::as_str).collect();
    canonical.sort_unstable();
    canonical.dedup();

    let mut hasher = Sha256::new();
    hasher.update(MEMBERSHIP_HASH_DOMAIN);
    push_lp(&mut hasher, domain_id.0.as_bytes());
    hasher.update((canonical.len() as u32).to_be_bytes());
    for did in canonical {
        push_lp(&mut hasher, did.as_bytes());
    }
    hasher.finalize().into()
}

/// A governance operation bound to its author, its domain, and the membership snapshot
/// under which that author held authority.
///
/// Construct with [`SignedGovernanceOp::sign`]; validate with
/// [`SignedGovernanceOp::verify`]. Verifying establishes **content and author binding
/// only** — that these exact bytes were authored by this DID. It does *not* establish that
/// the DID held authority; that requires resolving the domain against local state and is
/// the ingress slice's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedGovernanceOp {
    version: u16,
    author: Did,
    domain_id: GovernanceDomainId,
    authority: ReplicationAuthority,
    seq: u64,
    op_kind: GovernanceOpKind,
    op_bytes: Vec<u8>,
    signature: Vec<u8>,
}

impl SignedGovernanceOp {
    /// Sign a governance operation.
    ///
    /// `seq` orders the author's own operations within `domain_id`. It is a **comparator
    /// for same-key conflicts, never an acceptance gate**: a receiver must not reject an
    /// operation for having a lower `seq` than one already seen, because gossip delivers
    /// out of order and doing so would permanently discard legitimate operations.
    pub fn sign(
        signing_key: &SigningKey,
        author: Did,
        domain_id: GovernanceDomainId,
        authority: ReplicationAuthority,
        seq: u64,
        op: &GovernanceMessage,
    ) -> Result<Self, ReplicationEnvelopeError> {
        let op_kind = GovernanceOpKind::from_message(op)
            .ok_or(ReplicationEnvelopeError::NotReplicable(op.message_type()))?;

        let op_bytes = op
            .to_bytes()
            .map_err(|e| ReplicationEnvelopeError::Malformed(e.to_string()))?;

        let mut envelope = Self {
            version: GOV_OP_V1,
            author,
            domain_id,
            authority,
            seq,
            op_kind,
            op_bytes,
            signature: Vec::new(),
        };

        let signature: Signature = signing_key.sign(&envelope.canonical_body());
        envelope.signature = signature.to_bytes().to_vec();
        Ok(envelope)
    }

    /// The exact byte range the signature covers.
    ///
    /// Every variable-length field is length-prefixed, so no field boundary is ambiguous
    /// and no two distinct field assignments can produce the same body. This is also the
    /// frame's literal prefix: an encoded envelope is `canonical_body() ‖ lp(signature)`.
    fn canonical_body(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(GOV_OP_MAGIC);
        out.extend_from_slice(&self.version.to_be_bytes());
        push_lp_vec(&mut out, self.author.as_str().as_bytes());
        push_lp_vec(&mut out, self.domain_id.0.as_bytes());
        out.push(self.authority.discriminant());
        match self.authority {
            ReplicationAuthority::StaticMembership { membership_hash } => {
                out.extend_from_slice(&membership_hash);
            }
        }
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.push(self.op_kind.discriminant());
        push_lp_vec(&mut out, &self.op_bytes);
        out
    }

    /// Verify content and author binding.
    ///
    /// Establishes that these exact bytes were signed by `author`'s key. Establishes
    /// **nothing** about authority over the domain.
    pub fn verify(&self) -> Result<(), ReplicationEnvelopeError> {
        if self.version != GOV_OP_V1 {
            return Err(ReplicationEnvelopeError::UnsupportedVersion(self.version));
        }

        let verifying_key = self
            .author
            .to_verifying_key()
            .map_err(|e| ReplicationEnvelopeError::UnusableAuthorKey(e.to_string()))?;

        let sig_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| ReplicationEnvelopeError::SignatureInvalid)?;

        verifying_key
            .verify(&self.canonical_body(), &Signature::from_bytes(&sig_bytes))
            .map_err(|_| ReplicationEnvelopeError::SignatureInvalid)
    }

    /// Replay identity: the SHA-256 of the signed body.
    ///
    /// Derived rather than carried, so it cannot disagree with the content. A receiver
    /// deduplicates on this value; because it is order-independent it never causes a
    /// legitimate unseen operation to be discarded.
    pub fn op_id(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_body()).into()
    }

    /// Decode the carried operation, checking it against the signed `op_kind`.
    ///
    /// Call only after [`verify`](Self::verify) — decoding unverified attacker bytes is
    /// exactly the ordering mistake this envelope exists to prevent.
    pub fn decode_op(&self) -> Result<GovernanceMessage, ReplicationEnvelopeError> {
        let msg = GovernanceMessage::from_bytes(&self.op_bytes)
            .map_err(|e| ReplicationEnvelopeError::PayloadUndecodable(e.to_string()))?;

        match GovernanceOpKind::from_message(&msg) {
            Some(actual) if actual == self.op_kind => Ok(msg),
            _ => Err(ReplicationEnvelopeError::OpKindMismatch {
                declared: self.op_kind,
                actual: msg.message_type(),
            }),
        }
    }

    /// Serialize to the wire frame: `canonical_body() ‖ lp(signature)`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.canonical_body();
        push_lp_vec(&mut out, &self.signature);
        out
    }

    /// Parse a wire frame. Does **not** verify the signature; call
    /// [`verify`](Self::verify) next.
    pub fn decode(frame: &[u8]) -> Result<Self, ReplicationEnvelopeError> {
        let mut cur = Cursor::new(frame);

        if cur.take(GOV_OP_MAGIC.len())? != GOV_OP_MAGIC {
            return Err(ReplicationEnvelopeError::BadMagic);
        }

        let version = u16::from_be_bytes(
            cur.take(2)?
                .try_into()
                .map_err(|_| ReplicationEnvelopeError::Truncated)?,
        );
        if version != GOV_OP_V1 {
            return Err(ReplicationEnvelopeError::UnsupportedVersion(version));
        }

        let author_bytes = cur.take_lp()?;
        let author_str = std::str::from_utf8(author_bytes)
            .map_err(|e| ReplicationEnvelopeError::Malformed(e.to_string()))?;
        let author = Did::from_str(author_str)
            .map_err(|e| ReplicationEnvelopeError::Malformed(e.to_string()))?;

        let domain_bytes = cur.take_lp()?;
        let domain_id = GovernanceDomainId::new(
            std::str::from_utf8(domain_bytes)
                .map_err(|e| ReplicationEnvelopeError::Malformed(e.to_string()))?,
        );

        let authority_disc = cur.take_u8()?;
        let authority = match authority_disc {
            1 => {
                let hash: [u8; 32] = cur
                    .take(32)?
                    .try_into()
                    .map_err(|_| ReplicationEnvelopeError::Truncated)?;
                ReplicationAuthority::StaticMembership {
                    membership_hash: hash,
                }
            }
            other => return Err(ReplicationEnvelopeError::UnknownAuthority(other)),
        };

        let seq = u64::from_be_bytes(
            cur.take(8)?
                .try_into()
                .map_err(|_| ReplicationEnvelopeError::Truncated)?,
        );

        let kind_disc = cur.take_u8()?;
        let op_kind = GovernanceOpKind::from_discriminant(kind_disc)
            .ok_or(ReplicationEnvelopeError::UnknownOpKind(kind_disc))?;

        let op_bytes = cur.take_lp()?.to_vec();
        let signature = cur.take_lp()?.to_vec();

        if !cur.is_empty() {
            return Err(ReplicationEnvelopeError::Malformed(
                "trailing bytes after frame".to_string(),
            ));
        }

        Ok(Self {
            version,
            author,
            domain_id,
            authority,
            seq,
            op_kind,
            op_bytes,
            signature,
        })
    }

    /// Envelope version.
    pub fn version(&self) -> u16 {
        self.version
    }
    /// The DID whose key signed this envelope.
    pub fn author(&self) -> &Did {
        &self.author
    }
    /// The governance domain this operation affects.
    pub fn domain_id(&self) -> &GovernanceDomainId {
        &self.domain_id
    }
    /// The authority basis claimed by the author.
    pub fn authority(&self) -> &ReplicationAuthority {
        &self.authority
    }
    /// The author's per-domain ordering comparator. Not an acceptance gate.
    pub fn seq(&self) -> u64 {
        self.seq
    }
    /// The declared operation kind.
    pub fn op_kind(&self) -> GovernanceOpKind {
        self.op_kind
    }
    /// The carried operation bytes, exactly as signed.
    pub fn op_bytes(&self) -> &[u8] {
        &self.op_bytes
    }
}

// ---- Encoding helpers ----

fn push_lp_vec(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn push_lp(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u32).to_be_bytes());
    hasher.update(bytes);
}

/// Bounds-checked forward reader. Every read is length-checked against the remaining
/// frame, so a truncated or over-declared frame is an error rather than a panic.
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ReplicationEnvelopeError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(ReplicationEnvelopeError::Truncated)?;
        if end > self.buf.len() {
            return Err(ReplicationEnvelopeError::Truncated);
        }
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn take_u8(&mut self) -> Result<u8, ReplicationEnvelopeError> {
        Ok(self.take(1)?[0])
    }

    fn take_lp(&mut self) -> Result<&'a [u8], ReplicationEnvelopeError> {
        let len = u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| ReplicationEnvelopeError::Truncated)?,
        ) as usize;
        self.take(len)
    }

    fn is_empty(&self) -> bool {
        self.pos == self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProposalId, Vote, VoteChoice};
    use icn_identity::KeyPair;

    fn keypair() -> KeyPair {
        KeyPair::generate().unwrap()
    }

    fn signing_key(kp: &KeyPair) -> SigningKey {
        SigningKey::from_bytes(&kp.to_signing_key_bytes())
    }

    fn domain() -> GovernanceDomainId {
        GovernanceDomainId::new("domain-alpha")
    }

    fn authority() -> ReplicationAuthority {
        ReplicationAuthority::StaticMembership {
            membership_hash: [7u8; 32],
        }
    }

    fn vote_msg(voter: &Did) -> GovernanceMessage {
        let vote = Vote::new(ProposalId::new("prop-1"), voter.clone(), VoteChoice::For);
        GovernanceMessage::vote_cast(vote, None)
    }

    fn signed(kp: &KeyPair) -> SignedGovernanceOp {
        SignedGovernanceOp::sign(
            &signing_key(kp),
            kp.did().clone(),
            domain(),
            authority(),
            1,
            &vote_msg(kp.did()),
        )
        .unwrap()
    }

    // ---- Content and author binding ----

    #[test]
    fn a_signed_envelope_verifies_under_its_author() {
        let kp = keypair();
        assert!(signed(&kp).verify().is_ok());
    }

    #[test]
    fn a_signature_from_a_different_key_does_not_verify() {
        let alice = keypair();
        let mallory = keypair();

        // Mallory signs, but the envelope names Alice as author.
        let env = SignedGovernanceOp::sign(
            &signing_key(&mallory),
            alice.did().clone(),
            domain(),
            authority(),
            1,
            &vote_msg(alice.did()),
        )
        .unwrap();

        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    #[test]
    fn mutating_a_single_payload_byte_breaks_verification() {
        let kp = keypair();
        let mut env = signed(&kp);
        env.op_bytes[0] ^= 0x01;
        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    #[test]
    fn retargeting_the_domain_breaks_verification() {
        let kp = keypair();
        let mut env = signed(&kp);
        env.domain_id = GovernanceDomainId::new("domain-beta");
        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    #[test]
    fn reassigning_the_author_breaks_verification() {
        let kp = keypair();
        let other = keypair();
        let mut env = signed(&kp);
        env.author = other.did().clone();
        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    #[test]
    fn altering_the_sequence_breaks_verification() {
        let kp = keypair();
        let mut env = signed(&kp);
        env.seq += 1;
        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    #[test]
    fn altering_the_authority_snapshot_breaks_verification() {
        let kp = keypair();
        let mut env = signed(&kp);
        env.authority = ReplicationAuthority::StaticMembership {
            membership_hash: [9u8; 32],
        };
        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    #[test]
    fn altering_the_op_kind_breaks_verification() {
        let kp = keypair();
        let mut env = signed(&kp);
        env.op_kind = GovernanceOpKind::ProposalCreated;
        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    #[test]
    fn a_truncated_signature_is_rejected_rather_than_panicking() {
        let kp = keypair();
        let mut env = signed(&kp);
        env.signature.truncate(63);
        assert_eq!(
            env.verify(),
            Err(ReplicationEnvelopeError::SignatureInvalid)
        );
    }

    // ---- op_id ----

    #[test]
    fn op_id_is_stable_across_encode_decode() {
        let kp = keypair();
        let env = signed(&kp);
        let round = SignedGovernanceOp::decode(&env.encode()).unwrap();
        assert_eq!(env.op_id(), round.op_id());
    }

    #[test]
    fn op_id_changes_with_every_signed_field() {
        let kp = keypair();
        let base = signed(&kp);
        let baseline = base.op_id();

        let mut d = base.clone();
        d.domain_id = GovernanceDomainId::new("domain-beta");
        assert_ne!(baseline, d.op_id(), "domain_id must affect op_id");

        let mut s = base.clone();
        s.seq += 1;
        assert_ne!(baseline, s.op_id(), "seq must affect op_id");

        let mut a = base.clone();
        a.authority = ReplicationAuthority::StaticMembership {
            membership_hash: [9u8; 32],
        };
        assert_ne!(baseline, a.op_id(), "authority must affect op_id");

        let mut k = base.clone();
        k.op_kind = GovernanceOpKind::ProposalCreated;
        assert_ne!(baseline, k.op_id(), "op_kind must affect op_id");

        let mut p = base.clone();
        p.op_bytes[0] ^= 0x01;
        assert_ne!(baseline, p.op_id(), "op_bytes must affect op_id");

        let other = keypair();
        let mut au = base.clone();
        au.author = other.did().clone();
        assert_ne!(baseline, au.op_id(), "author must affect op_id");
    }

    #[test]
    fn op_id_ignores_the_signature_itself() {
        // The signature is not part of the signed body, so a re-signature of identical
        // content yields the same replay identity. Ed25519 is deterministic, so this also
        // documents that assumption explicitly rather than relying on it silently.
        let kp = keypair();
        let a = signed(&kp);
        let mut b = a.clone();
        b.signature = vec![0u8; 64];
        assert_eq!(a.op_id(), b.op_id());
    }

    // ---- Membership hash ----

    #[test]
    fn membership_hash_ignores_member_order() {
        let a = keypair().did().clone();
        let b = keypair().did().clone();
        let d = domain();
        assert_eq!(
            static_membership_hash(&d, &[a.clone(), b.clone()]),
            static_membership_hash(&d, &[b, a])
        );
    }

    #[test]
    fn membership_hash_ignores_duplicates() {
        let a = keypair().did().clone();
        let b = keypair().did().clone();
        let d = domain();
        assert_eq!(
            static_membership_hash(&d, &[a.clone(), b.clone()]),
            static_membership_hash(&d, &[a.clone(), b, a])
        );
    }

    #[test]
    fn membership_hash_changes_when_the_member_set_changes() {
        let a = keypair().did().clone();
        let b = keypair().did().clone();
        let d = domain();
        assert_ne!(
            static_membership_hash(&d, std::slice::from_ref(&a)),
            static_membership_hash(&d, &[a, b])
        );
    }

    #[test]
    fn membership_hash_is_domain_separated() {
        let a = keypair().did().clone();
        let members = [a];
        assert_ne!(
            static_membership_hash(&GovernanceDomainId::new("domain-alpha"), &members),
            static_membership_hash(&GovernanceDomainId::new("domain-beta"), &members)
        );
    }

    #[test]
    fn an_empty_member_set_is_still_bound_to_its_domain() {
        // The degenerate case must not collapse to a constant: two domains with no
        // members must still hash differently, or an empty snapshot would be portable
        // between domains.
        assert_ne!(
            static_membership_hash(&GovernanceDomainId::new("domain-alpha"), &[]),
            static_membership_hash(&GovernanceDomainId::new("domain-beta"), &[])
        );
    }

    #[test]
    fn the_domain_member_boundary_cannot_be_shifted() {
        // Without a length prefix on domain_id, domain "ab" + member set M would hash
        // identically to domain "a" + a set whose encoding began with "b". The prefix is
        // what forbids that; assert the boundary is real rather than trusting it.
        let m = keypair().did().clone();
        assert_ne!(
            static_membership_hash(&GovernanceDomainId::new("ab"), std::slice::from_ref(&m)),
            static_membership_hash(&GovernanceDomainId::new("a"), std::slice::from_ref(&m))
        );
    }

    // ---- Frame encoding ----

    #[test]
    fn encode_decode_round_trips_every_field() {
        let kp = keypair();
        let env = signed(&kp);
        let round = SignedGovernanceOp::decode(&env.encode()).unwrap();
        assert_eq!(env, round);
        assert!(round.verify().is_ok());
    }

    #[test]
    fn the_signed_body_is_a_prefix_of_the_frame() {
        let kp = keypair();
        let env = signed(&kp);
        assert!(env.encode().starts_with(&env.canonical_body()));
    }

    #[test]
    fn a_frame_without_the_magic_is_rejected() {
        let kp = keypair();
        let mut frame = signed(&kp).encode();
        frame[0] ^= 0xff;
        assert_eq!(
            SignedGovernanceOp::decode(&frame),
            Err(ReplicationEnvelopeError::BadMagic)
        );
    }

    #[test]
    fn a_bare_governance_message_is_not_mistaken_for_a_frame() {
        // Migration depends on this: legacy payloads are bare JSON and must be
        // distinguishable by magic, never by trial decoding.
        let kp = keypair();
        let legacy = vote_msg(kp.did()).to_bytes().unwrap();
        assert_eq!(
            SignedGovernanceOp::decode(&legacy),
            Err(ReplicationEnvelopeError::BadMagic)
        );
    }

    #[test]
    fn an_unknown_version_is_rejected() {
        let kp = keypair();
        let mut frame = signed(&kp).encode();
        let v = GOV_OP_MAGIC.len();
        frame[v..v + 2].copy_from_slice(&99u16.to_be_bytes());
        assert_eq!(
            SignedGovernanceOp::decode(&frame),
            Err(ReplicationEnvelopeError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn a_truncated_frame_is_rejected_at_every_length() {
        let kp = keypair();
        let frame = signed(&kp).encode();
        for cut in 0..frame.len() {
            assert!(
                SignedGovernanceOp::decode(&frame[..cut]).is_err(),
                "prefix of length {cut} must not decode"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let kp = keypair();
        let mut frame = signed(&kp).encode();
        frame.push(0x00);
        assert!(matches!(
            SignedGovernanceOp::decode(&frame),
            Err(ReplicationEnvelopeError::Malformed(_))
        ));
    }

    #[test]
    fn an_over_declared_length_prefix_is_rejected() {
        let kp = keypair();
        let mut frame = signed(&kp).encode();
        // Author length prefix sits immediately after magic + version.
        let at = GOV_OP_MAGIC.len() + 2;
        frame[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            SignedGovernanceOp::decode(&frame),
            Err(ReplicationEnvelopeError::Truncated)
        );
    }

    #[test]
    fn an_unknown_authority_discriminant_is_rejected() {
        let kp = keypair();
        let env = signed(&kp);
        let mut frame = env.encode();
        let at = GOV_OP_MAGIC.len() + 2 + 4 + env.author.as_str().len() + 4 + env.domain_id.0.len();
        frame[at] = 0xfe;
        assert_eq!(
            SignedGovernanceOp::decode(&frame),
            Err(ReplicationEnvelopeError::UnknownAuthority(0xfe))
        );
    }

    #[test]
    fn an_unknown_op_kind_discriminant_is_rejected() {
        let kp = keypair();
        let env = signed(&kp);
        let mut frame = env.encode();
        let at = GOV_OP_MAGIC.len()
            + 2
            + 4
            + env.author.as_str().len()
            + 4
            + env.domain_id.0.len()
            + 1
            + 32
            + 8;
        frame[at] = 0xfd;
        assert_eq!(
            SignedGovernanceOp::decode(&frame),
            Err(ReplicationEnvelopeError::UnknownOpKind(0xfd))
        );
    }

    // ---- Payload / kind agreement ----

    #[test]
    fn decode_op_returns_the_carried_operation() {
        let kp = keypair();
        let env = signed(&kp);
        let msg = env.decode_op().unwrap();
        assert_eq!(msg.message_type(), "VoteCast");
    }

    #[test]
    fn a_payload_disagreeing_with_the_declared_kind_is_rejected() {
        let kp = keypair();
        let mut env = signed(&kp);
        // Re-sign so the signature is valid but the kind lies about the payload.
        env.op_kind = GovernanceOpKind::DelegationRevoked;
        let sig: Signature = signing_key(&kp).sign(&env.canonical_body());
        env.signature = sig.to_bytes().to_vec();

        assert!(env.verify().is_ok(), "signature must still be valid");
        assert!(matches!(
            env.decode_op(),
            Err(ReplicationEnvelopeError::OpKindMismatch { .. })
        ));
    }

    #[test]
    fn a_non_mutating_message_cannot_be_wrapped() {
        let kp = keypair();
        let msg = GovernanceMessage::CommentCreated {
            proposal_id: ProposalId::new("prop-1"),
            comment_id: crate::CommentId::from_string("c-1"),
            author: kp.did().clone(),
            parent_id: None,
            created_at: 0,
        };
        assert!(matches!(
            SignedGovernanceOp::sign(
                &signing_key(&kp),
                kp.did().clone(),
                domain(),
                authority(),
                1,
                &msg,
            ),
            Err(ReplicationEnvelopeError::NotReplicable(_))
        ));
    }

    // ---- v1 policy declaration ----

    #[test]
    fn only_vote_cast_is_restorable_in_v1() {
        assert!(GovernanceOpKind::VoteCast.is_restorable_in_v1());
        for kind in [
            GovernanceOpKind::DomainCreated,
            GovernanceOpKind::DomainUpdated,
            GovernanceOpKind::ProposalCreated,
            GovernanceOpKind::ProposalOpened,
            GovernanceOpKind::ProposalClosed,
            GovernanceOpKind::DelegationCreated,
            GovernanceOpKind::DelegationRevoked,
        ] {
            assert!(
                !kind.is_restorable_in_v1(),
                "{kind:?} must stay contained in v1"
            );
        }
    }

    #[test]
    fn op_kind_discriminants_are_stable() {
        // Wire discriminants are permanent. Changing one silently reinterprets every
        // previously signed envelope, so pin them.
        for (kind, disc) in [
            (GovernanceOpKind::DomainCreated, 1u8),
            (GovernanceOpKind::DomainUpdated, 2),
            (GovernanceOpKind::ProposalCreated, 3),
            (GovernanceOpKind::ProposalOpened, 4),
            (GovernanceOpKind::VoteCast, 5),
            (GovernanceOpKind::ProposalClosed, 6),
            (GovernanceOpKind::DelegationCreated, 7),
            (GovernanceOpKind::DelegationRevoked, 8),
        ] {
            assert_eq!(kind.discriminant(), disc);
            assert_eq!(GovernanceOpKind::from_discriminant(disc), Some(kind));
        }
    }
}
