//! The N1 event taxonomy and its canonical codec.
//!
//! Every type here is deliberately small and hard to misuse. In particular [`SubjectId`] and
//! [`PrincipalKey`] are separate types with **no conversion between them**, so a subject can
//! never be named where a raw authority principal is required (§9.2.1: `derive` is pure only if
//! authority principals are keys).

use std::collections::BTreeSet;

use ed25519_dalek::VerifyingKey;

use super::encoding::{CodecError, Reader, Writer};
use super::{kind, sha256, DOMAIN, PRINCIPAL_TAG_ED25519, PROTOCOL_VERSION, SIGNATURE_DOMAIN};
use crate::Did;

/// A 32-byte body digest: `event_id = SHA-256(canonical_body)`.
///
/// Signatures are outside this digest. See the module documentation for why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId([u8; 32]);

impl EventId {
    /// Wrap raw digest bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        EventId(bytes)
    }

    /// The raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// A subject identifier: `σ = event_id(inception body)`.
///
/// A subject is **not** a principal. There is intentionally no conversion from `SubjectId` into
/// [`PrincipalKey`]; placing subject bytes in a principal field is a wire-level decode error
/// because principals carry a distinct tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubjectId([u8; 32]);

impl SubjectId {
    /// Wrap raw subject bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        SubjectId(bytes)
    }

    /// The raw subject bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for SubjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// A pre-rotation commitment `C_g`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Commitment([u8; 32]);

impl Commitment {
    /// The sentinel meaning "no further establishment is armed".
    ///
    /// A state carrying this value authorizes no establishment body at all, so the authority set
    /// is frozen for the remaining life of the subject.
    pub const TERMINAL: Commitment = Commitment([0u8; 32]);

    /// Wrap raw commitment bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Commitment(bytes)
    }

    /// The raw commitment bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Whether this is [`Commitment::TERMINAL`].
    pub fn is_terminal(&self) -> bool {
        *self == Commitment::TERMINAL
    }
}

impl std::fmt::Display for Commitment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// The pre-subject context nonce carried in an inception body.
///
/// This exists to break the genesis fixed point. The initial authority key is derived from
/// `(continuity root, nonce, generation 0)` — never from the subject identifier, which does not
/// exist until the inception body it appears in has been encoded and hashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextNonce([u8; 32]);

impl ContextNonce {
    /// Wrap raw nonce bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        ContextNonce(bytes)
    }

    /// The raw nonce bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A raw cryptographic principal: an Ed25519 public key, and nothing else.
///
/// **Canonicalization (absorbs O10).** A [`Did`] is a string, and `Did` equality is string
/// equality, so two multibase encodings of one key are two unequal `Did`s. A `PrincipalKey`
/// accepts only a canonical RFC 8032 key encoding and rejects weak points before it can enter a
/// body, commitment, or event identifier.
#[derive(Clone, Copy)]
pub struct PrincipalKey(VerifyingKey);

impl PrincipalKey {
    /// Build a principal from an Ed25519 verifying key.
    ///
    /// This applies the same canonical-encoding and weak-point checks as wire decoding. N1 does
    /// not inherit the legacy [`Did`] parser's more permissive ZIP-215 acceptance rules.
    pub fn try_from_verifying_key(key: VerifyingKey) -> Result<Self, CodecError> {
        PrincipalKey::try_from_bytes(key.to_bytes())
    }

    /// Build a principal from a [`Did`], decoding it to its underlying key.
    ///
    /// This fails for any `Did` whose payload is not a valid Ed25519 public key — including DIDs
    /// minted from hash-derived values rather than from keys.
    pub fn from_did(did: &Did) -> Result<Self, CodecError> {
        let key = did
            .to_verifying_key()
            .map_err(|_| CodecError::InvalidPublicKey)?;
        PrincipalKey::try_from_bytes(key.to_bytes())
    }

    /// Build a principal from raw key bytes, validating that they are a real Ed25519 point.
    pub fn try_from_bytes(bytes: [u8; 32]) -> Result<Self, CodecError> {
        if !canonical_edwards_y(&bytes) {
            return Err(CodecError::NonCanonicalPublicKey);
        }
        let key = VerifyingKey::from_bytes(&bytes).map_err(|_| CodecError::InvalidPublicKey)?;
        if key.is_weak() {
            return Err(CodecError::WeakPublicKey);
        }
        Ok(PrincipalKey(key))
    }

    /// Construct from a locally derived signing key.
    ///
    /// Ed25519 signing keys produce canonical, prime-subgroup public keys. Keeping this constructor
    /// crate-private prevents untrusted `VerifyingKey` values from bypassing [`Self::try_from_bytes`].
    pub(crate) fn from_signing_key(key: &ed25519_dalek::SigningKey) -> Self {
        PrincipalKey(key.verifying_key())
    }

    /// The canonical 32-byte key encoding.
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// The underlying verifying key, for signature checks.
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.0
    }

    /// Render this principal as a `did:icn:` string.
    pub fn to_did(&self) -> Did {
        Did::from_public_key(&self.0)
    }

    fn encode(&self, w: &mut Writer) {
        w.u8(PRINCIPAL_TAG_ED25519);
        w.b32(&self.as_bytes());
    }

    fn decode(r: &mut Reader<'_>, field: &'static str) -> Result<Self, CodecError> {
        let tag = r.u8(field)?;
        if tag != PRINCIPAL_TAG_ED25519 {
            return Err(CodecError::BadPrincipalTag(tag));
        }
        let bytes = r.b32(field)?;
        PrincipalKey::try_from_bytes(bytes)
    }
}

/// RFC 8032 encodes the Edwards `y` coordinate as a little-endian integer below
/// `p = 2^255 - 19`, with the high bit reserved for the sign of `x`.
///
/// `ed25519-dalek` deliberately accepts ZIP-215 non-canonical encodings, so N1 must enforce this
/// protocol invariant before constructing a [`PrincipalKey`].
fn canonical_edwards_y(bytes: &[u8; 32]) -> bool {
    const FIELD_MODULUS: [u8; 32] = [
        0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x7f,
    ];

    let mut y = *bytes;
    y[31] &= 0x7f;
    for index in (0..32).rev() {
        match y[index].cmp(&FIELD_MODULUS[index]) {
            std::cmp::Ordering::Less => return true,
            std::cmp::Ordering::Greater => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    false
}

impl PartialEq for PrincipalKey {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for PrincipalKey {}

impl PartialOrd for PrincipalKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrincipalKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bytes().cmp(&other.as_bytes())
    }
}

impl std::hash::Hash for PrincipalKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl std::fmt::Debug for PrincipalKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrincipalKey({})", hex::encode(self.as_bytes()))
    }
}

/// The exactly-one logical writer for an establishment generation.
///
/// The counted-set wire shape is retained by the N1 encoding, but cardinality is a protocol
/// invariant: several independently signing keys would be several writers and would reintroduce
/// consensus at ordinary-event positions. A future threshold/group public key is still one
/// [`PrincipalKey`] here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrincipalSet(BTreeSet<PrincipalKey>);

impl PrincipalSet {
    /// Build an authority set, requiring exactly one logical writer.
    pub fn new(
        members: impl IntoIterator<Item = PrincipalKey>,
        field: &'static str,
    ) -> Result<Self, CodecError> {
        let set: BTreeSet<PrincipalKey> = members.into_iter().collect();
        if set.is_empty() {
            return Err(CodecError::EmptySet { field });
        }
        if set.len() != 1 {
            return Err(CodecError::MultipleAuthorityPrincipals {
                field,
                count: set.len(),
            });
        }
        Ok(PrincipalSet(set))
    }

    /// Convenience constructor for the common single-key case.
    pub fn single(key: PrincipalKey) -> Self {
        let mut set = BTreeSet::new();
        set.insert(key);
        PrincipalSet(set)
    }

    /// The members, in canonical ascending order.
    pub fn members(&self) -> &BTreeSet<PrincipalKey> {
        &self.0
    }

    /// The sole logical writer.
    ///
    /// Used as the signer rule for establishment bodies: the only revealed writer must be the
    /// inline signer. This is not key ordering and never chooses between candidates. It returns an
    /// `Option` rather than unwrapping because protocol paths do not rely on panics.
    pub fn canonical_signer(&self) -> Option<PrincipalKey> {
        self.0.iter().next().copied()
    }

    fn encode(&self, w: &mut Writer) {
        w.u32(self.0.len() as u32);
        for member in &self.0 {
            member.encode(w);
        }
    }

    fn decode(r: &mut Reader<'_>, field: &'static str) -> Result<Self, CodecError> {
        // Each principal is 33 bytes on the wire (tag + key).
        let count = r.count(33, field)?;
        if count == 0 {
            return Err(CodecError::EmptySet { field });
        }
        if count != 1 {
            return Err(CodecError::MultipleAuthorityPrincipals {
                field,
                count: count as usize,
            });
        }
        let mut set = BTreeSet::new();
        let mut previous: Option<[u8; 32]> = None;
        for _ in 0..count {
            let member = PrincipalKey::decode(r, field)?;
            let bytes = member.as_bytes();
            if let Some(prev) = previous {
                if bytes <= prev {
                    return Err(CodecError::UnorderedSet { field });
                }
            }
            previous = Some(bytes);
            set.insert(member);
        }
        Ok(PrincipalSet(set))
    }
}

/// A device capability recorded by an `authorize` event.
///
/// These are app-layer labels the kernel never interprets. **No capability value confers
/// log-writing or establishment authority**: [`DeviceCapability::Recover`] exists here
/// deliberately, and a body carrying it is still an ordinary non-establishment event. If a
/// capability bit could mint establishment priority, a compromised current key would defeat
/// [`super::supersede`] (finding F13/F13a, one layer up).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceCapability {
    /// The device may sign application-layer acts.
    Sign,
    /// The device may decrypt content addressed to the subject.
    Encrypt,
    /// The device may present credentials on the subject's behalf.
    Present,
    /// An app-layer recovery role. **Inert** with respect to establishment authority.
    Recover,
}

impl DeviceCapability {
    /// The wire tag. Never renumber these.
    pub const fn tag(self) -> u8 {
        match self {
            DeviceCapability::Sign => 0x01,
            DeviceCapability::Encrypt => 0x02,
            DeviceCapability::Present => 0x03,
            DeviceCapability::Recover => 0x04,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, CodecError> {
        match tag {
            0x01 => Ok(DeviceCapability::Sign),
            0x02 => Ok(DeviceCapability::Encrypt),
            0x03 => Ok(DeviceCapability::Present),
            0x04 => Ok(DeviceCapability::Recover),
            other => Err(CodecError::UnknownCapability(other)),
        }
    }
}

/// A canonically ordered, possibly empty capability set.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilitySet(BTreeSet<DeviceCapability>);

impl CapabilitySet {
    /// Build a capability set.
    pub fn new(caps: impl IntoIterator<Item = DeviceCapability>) -> Self {
        CapabilitySet(caps.into_iter().collect())
    }

    /// The members, in canonical ascending order.
    pub fn members(&self) -> &BTreeSet<DeviceCapability> {
        &self.0
    }

    /// Whether the set contains `cap`.
    pub fn contains(&self, cap: DeviceCapability) -> bool {
        self.0.contains(&cap)
    }

    fn encode(&self, w: &mut Writer) {
        w.u32(self.0.len() as u32);
        for cap in &self.0 {
            w.u8(cap.tag());
        }
    }

    fn decode(r: &mut Reader<'_>, field: &'static str) -> Result<Self, CodecError> {
        let count = r.count(1, field)?;
        let mut set = BTreeSet::new();
        let mut previous: Option<u8> = None;
        for _ in 0..count {
            let tag = r.u8(field)?;
            if let Some(prev) = previous {
                if tag <= prev {
                    return Err(CodecError::UnorderedSet { field });
                }
            }
            previous = Some(tag);
            set.insert(DeviceCapability::from_tag(tag)?);
        }
        Ok(CapabilitySet(set))
    }
}

/// A **position-denominated** validity span, inclusive at both ends.
///
/// Spans are denominated in log positions, never in wall-clock time: nothing in this module may
/// read a receiver clock (§19.2 obligation 21). O-N1 — whether an act's evaluation position can
/// be bound to something the signer does not control — remains open and is not answered here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValiditySpan {
    /// First position (inclusive) at which the grant is in force.
    pub from_position: u64,
    /// Last position (inclusive) at which the grant is in force.
    pub until_position: u64,
}

impl ValiditySpan {
    /// Build a span, rejecting an inverted one.
    pub fn new(from_position: u64, until_position: u64) -> Result<Self, CodecError> {
        if until_position < from_position {
            return Err(CodecError::InvertedSpan {
                from: from_position,
                until: until_position,
            });
        }
        Ok(ValiditySpan {
            from_position,
            until_position,
        })
    }

    /// Whether `position` falls inside the span.
    pub fn contains(&self, position: u64) -> bool {
        position >= self.from_position && position <= self.until_position
    }
}

/// The header shared by every non-inception body.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventHeader {
    /// The subject this body belongs to.
    pub subject: SubjectId,
    /// The `+1`-monotone position. Never `0`, which is reserved for inception.
    pub position: u64,
    /// `event_id` of the parent **body**. Never a digest of a signed event.
    pub prev_digest: EventId,
    /// The key that signed this body, carried inline so admissibility is state-independent.
    pub signer: PrincipalKey,
}

impl EventHeader {
    fn encode(&self, w: &mut Writer) {
        w.b32(self.subject.as_bytes());
        w.u64(self.position);
        w.b32(self.prev_digest.as_bytes());
        self.signer.encode(w);
    }

    fn decode(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let subject = SubjectId::from_bytes(r.b32("subject")?);
        let position = r.u64("position")?;
        if position == 0 {
            return Err(CodecError::ReservedPosition);
        }
        let prev_digest = EventId::from_bytes(r.b32("prev_digest")?);
        let signer = PrincipalKey::decode(r, "signer")?;
        Ok(EventHeader {
            subject,
            position,
            prev_digest,
            signer,
        })
    }
}

/// The root of a subject's authority log. Its digest **is** the subject identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InceptionBody {
    /// The key that signed this body. Must be the sole member of `initial_authority`.
    pub signer: PrincipalKey,
    /// The pre-subject context nonce that breaks the genesis fixed point.
    pub context_nonce: ContextNonce,
    /// The initial authority set `A_0`, containing exactly one logical log writer.
    pub initial_authority: PrincipalSet,
    /// The pre-rotation commitment `C_1` to the next establishment.
    pub next_commitment: Commitment,
}

/// A `rotate` or `recover` body: the two establishment kinds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EstablishmentBody {
    /// Common header.
    pub header: EventHeader,
    /// The revealed pre-committed authority set `A_g`, containing one logical log writer.
    pub revealed_authority: PrincipalSet,
    /// The pre-rotation commitment `C_{g+1}` to the establishment after this one.
    pub next_commitment: Commitment,
}

/// An `authorize` body: grants a device a capability set, optionally over a position span.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthorizeBody {
    /// Common header.
    pub header: EventHeader,
    /// The device principal being granted capabilities. A device is not a log writer.
    pub device: PrincipalKey,
    /// The granted capabilities.
    pub capabilities: CapabilitySet,
    /// Optional position-denominated validity span.
    pub validity: Option<ValiditySpan>,
}

/// A `revoke` body: removes a device grant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevokeBody {
    /// Common header.
    pub header: EventHeader,
    /// The device principal whose grant is removed.
    pub device: PrincipalKey,
}

/// A canonical authority-log body.
///
/// Equality, ordering and hashing all go through [`AuthorityBody::canonical_bytes`], so the set
/// semantics of the durable layer are exactly the semantics of the canonical encoding.
#[derive(Debug, Clone)]
pub enum AuthorityBody {
    /// The log root.
    Inception(InceptionBody),
    /// A planned establishment. Preserves device grants.
    Rotate(EstablishmentBody),
    /// A device grant.
    Authorize(AuthorizeBody),
    /// A device revocation.
    Revoke(RevokeBody),
    /// A recovery establishment. Replaces rather than extends: device grants are cleared.
    Recover(EstablishmentBody),
}

impl AuthorityBody {
    /// The wire kind tag.
    pub fn kind_tag(&self) -> u8 {
        match self {
            AuthorityBody::Inception(_) => kind::INCEPTION,
            AuthorityBody::Rotate(_) => kind::ROTATE,
            AuthorityBody::Authorize(_) => kind::AUTHORIZE,
            AuthorityBody::Revoke(_) => kind::REVOKE,
            AuthorityBody::Recover(_) => kind::RECOVER,
        }
    }

    /// Whether this body is an **establishment** event.
    ///
    /// Establishment is defined by the authorization rule, not by this discriminant: a body is an
    /// establishment event **iff** candidate authorization requires it to reveal pre-committed
    /// authority material. The two are kept in step by
    /// `authority_log_derived::establishment_class_matches_authorization_rule`, which flips
    /// `state.next_commitment` and asserts that exactly the establishment kinds change verdict.
    pub fn is_establishment(&self) -> bool {
        matches!(self, AuthorityBody::Rotate(_) | AuthorityBody::Recover(_))
    }

    /// The subject this body belongs to.
    ///
    /// For an inception body the subject **is** its own `event_id` — the self-addressing rule, so
    /// admissibility rule 3 ("for an inception body, `σ == event_id(b)`") holds by representation
    /// and cannot be violated by a hostile encoder.
    pub fn subject(&self) -> SubjectId {
        match self {
            AuthorityBody::Inception(_) => SubjectId::from_bytes(*self.event_id().as_bytes()),
            AuthorityBody::Rotate(b) | AuthorityBody::Recover(b) => b.header.subject,
            AuthorityBody::Authorize(b) => b.header.subject,
            AuthorityBody::Revoke(b) => b.header.subject,
        }
    }

    /// The log position. Inception is always `0`.
    pub fn position(&self) -> u64 {
        match self {
            AuthorityBody::Inception(_) => 0,
            AuthorityBody::Rotate(b) | AuthorityBody::Recover(b) => b.header.position,
            AuthorityBody::Authorize(b) => b.header.position,
            AuthorityBody::Revoke(b) => b.header.position,
        }
    }

    /// The parent body digest, absent for inception.
    pub fn prev_digest(&self) -> Option<EventId> {
        match self {
            AuthorityBody::Inception(_) => None,
            AuthorityBody::Rotate(b) | AuthorityBody::Recover(b) => Some(b.header.prev_digest),
            AuthorityBody::Authorize(b) => Some(b.header.prev_digest),
            AuthorityBody::Revoke(b) => Some(b.header.prev_digest),
        }
    }

    /// The key that signed this body, carried inline.
    pub fn signer(&self) -> PrincipalKey {
        match self {
            AuthorityBody::Inception(b) => b.signer,
            AuthorityBody::Rotate(b) | AuthorityBody::Recover(b) => b.header.signer,
            AuthorityBody::Authorize(b) => b.header.signer,
            AuthorityBody::Revoke(b) => b.header.signer,
        }
    }

    /// The canonical byte encoding. Deterministic, versioned, domain-separated, length-prefixed.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(DOMAIN);
        w.u16(PROTOCOL_VERSION);
        w.u8(self.kind_tag());
        match self {
            AuthorityBody::Inception(b) => {
                b.signer.encode(&mut w);
                w.b32(b.context_nonce.as_bytes());
                b.initial_authority.encode(&mut w);
                w.b32(b.next_commitment.as_bytes());
            }
            AuthorityBody::Rotate(b) | AuthorityBody::Recover(b) => {
                b.header.encode(&mut w);
                b.revealed_authority.encode(&mut w);
                w.b32(b.next_commitment.as_bytes());
            }
            AuthorityBody::Authorize(b) => {
                b.header.encode(&mut w);
                b.device.encode(&mut w);
                b.capabilities.encode(&mut w);
                match b.validity {
                    None => w.u8(0x00),
                    Some(span) => {
                        w.u8(0x01);
                        w.u64(span.from_position);
                        w.u64(span.until_position);
                    }
                }
            }
            AuthorityBody::Revoke(b) => {
                b.header.encode(&mut w);
                b.device.encode(&mut w);
            }
        }
        w.finish()
    }

    /// `event_id = SHA-256(canonical_body)`. Signatures are outside this digest.
    pub fn event_id(&self) -> EventId {
        EventId::from_bytes(sha256(&self.canonical_bytes()))
    }

    /// The bytes a signer signs: `LP(SIGNATURE_DOMAIN) || canonical_body`.
    ///
    /// Separately domain-separated from the digest preimage so an authority-log signature can
    /// never be reinterpreted in another context.
    pub fn signature_preimage(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(SIGNATURE_DOMAIN);
        let mut out = w.finish();
        out.extend_from_slice(&self.canonical_bytes());
        out
    }

    /// Decode a canonical body, rejecting anything non-canonical.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut r = Reader::new(bytes);
        r.expect_lp(DOMAIN, "domain")?;
        let version = r.u16("version")?;
        if version != PROTOCOL_VERSION {
            return Err(CodecError::UnsupportedVersion(version));
        }
        let tag = r.u8("kind")?;
        let body = match tag {
            kind::INCEPTION => {
                let signer = PrincipalKey::decode(&mut r, "signer")?;
                let context_nonce = ContextNonce::from_bytes(r.b32("context_nonce")?);
                let initial_authority = PrincipalSet::decode(&mut r, "initial_authority")?;
                let next_commitment = Commitment::from_bytes(r.b32("next_commitment")?);
                if !initial_authority.members().contains(&signer) {
                    return Err(CodecError::SignerNotInInitialAuthority);
                }
                AuthorityBody::Inception(InceptionBody {
                    signer,
                    context_nonce,
                    initial_authority,
                    next_commitment,
                })
            }
            kind::ROTATE | kind::RECOVER => {
                let header = EventHeader::decode(&mut r)?;
                let revealed_authority = PrincipalSet::decode(&mut r, "revealed_authority")?;
                let next_commitment = Commitment::from_bytes(r.b32("next_commitment")?);
                let establishment = EstablishmentBody {
                    header,
                    revealed_authority,
                    next_commitment,
                };
                if tag == kind::ROTATE {
                    AuthorityBody::Rotate(establishment)
                } else {
                    AuthorityBody::Recover(establishment)
                }
            }
            kind::AUTHORIZE => {
                let header = EventHeader::decode(&mut r)?;
                let device = PrincipalKey::decode(&mut r, "device")?;
                let capabilities = CapabilitySet::decode(&mut r, "capabilities")?;
                let validity = match r.u8("validity_tag")? {
                    0x00 => None,
                    0x01 => {
                        let from = r.u64("validity_from")?;
                        let until = r.u64("validity_until")?;
                        Some(ValiditySpan::new(from, until)?)
                    }
                    other => return Err(CodecError::BadOptionTag(other)),
                };
                AuthorityBody::Authorize(AuthorizeBody {
                    header,
                    device,
                    capabilities,
                    validity,
                })
            }
            kind::REVOKE => {
                let header = EventHeader::decode(&mut r)?;
                let device = PrincipalKey::decode(&mut r, "device")?;
                AuthorityBody::Revoke(RevokeBody { header, device })
            }
            other => return Err(CodecError::UnknownKind(other)),
        };
        r.finish()?;
        Ok(body)
    }
}

impl PartialEq for AuthorityBody {
    fn eq(&self, other: &Self) -> bool {
        self.canonical_bytes() == other.canonical_bytes()
    }
}

impl Eq for AuthorityBody {}

impl PartialOrd for AuthorityBody {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AuthorityBody {
    /// Ordering exists so bodies can live in a [`std::collections::BTreeSet`] and so diagnostics
    /// are stable. **It is never a selection rule.** See [`super::resolve`].
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.canonical_bytes().cmp(&other.canonical_bytes())
    }
}

impl std::hash::Hash for AuthorityBody {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.canonical_bytes().hash(state);
    }
}
