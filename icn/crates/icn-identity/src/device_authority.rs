//! N4-A — relying-party evaluation of delegated device authority over the N1 authority log.
//!
//! This module answers exactly one question, for exactly one relying party at a time:
//!
//! > At the log position **I** have chosen to evaluate against, does this device Principal hold
//! > a grant from this Subject that covers this capability — and did that device sign this act?
//!
//! It is the first N4 tranche (#2599) and rung 5 of the #2694 convergence ladder. It implements
//! the Device Principal contract of `docs/architecture/IDENTITY_SEMANTICS.md` §2.7 as a
//! **relying-party** primitive: the derived view says which Principals a Subject has delegated
//! to; this module says whether one particular signed act is covered by that delegation at one
//! particular position.
//!
//! # What is deliberately true here
//!
//! - **The evaluation position is the relying party's, never the signer's.** A
//!   [`DeviceActV1`] binds the position its signer claims, but [`verify_device_act`] refuses
//!   unless that claim equals the position the relying party supplies. No signed field can
//!   extend its own authority window (N4 invariant 6).
//! - **Authority is a function of the retained prefix, not the frontier.** Evaluation runs
//!   [`derive_prefix`] through the evaluation position, so a later `revoke` does not erase
//!   authorship that was authorized at an earlier position, and an earlier `revoke` is not
//!   hidden by a later `authorize`.
//! - **Verification returns a grant, not a bool** (I5). Success yields
//!   [`DeviceAuthorityEvidence`] naming the Subject, the device, the capability, the position, the
//!   grant that covered it and the establishment generation it was evaluated under.
//! - **Forks and gaps fail closed.** A `Halted` view at or below the position, or a prefix
//!   whose frontier does not reach the position, is a refusal. There is no branch selection and no
//!   "probably still authorized".
//! - **A device is never the Subject.** The act names the Subject it acts *for*; the Subject has
//!   no key; the device's signature proves authorship of the act and nothing more until the grant
//!   is checked. An act by a Principal that is the Subject's own establishment authority is
//!   refused as a device act, for the reason GEN-A refuses it at genesis: a credential that is also
//!   the log-writer key has no bounded scope.
//! - **No clock, no network, no registry, no randomness.** Same store, same act, same position ⇒
//!   same verdict, on every replica.
//!
//! # What is deliberately not here
//!
//! No wire container for a store or an act (the act's canonical bytes are defined; a transport
//! envelope is not). No replay slot — the one-ballot rule of #2694 §9 belongs to the action
//! family that uses it, and `act_id` is provided for that purpose. No session or JWT
//! coupling: a session proves a request may enter a handler now; this proves durable authorship.
//! No attenuation *enforcement* beyond what N1 records (capability set plus optional position
//! span): N1 has no caveat or resource field, and inventing one here would be a contract change.
//! No gateway route, no persistence, no replication, no recovery, no production wiring.
//!
//! # Canonical act bytes
//!
//! All framing is N1's, reused rather than reimplemented: `LP(x) := u32be(len(x)) || x`,
//! `b32(x) := 32 raw bytes`, integers big-endian, principals tagged exactly as in an N1 body.
//!
//! ```text
//! device_act_v1 :=
//!       LP(DEVICE_ACT_DOMAIN)
//!    || u16be(DEVICE_ACT_VERSION)
//!    || b32(subject)
//!    || u8(principal_tag) || b32(device_key)
//!    || u64be(evaluation_position)
//!    || u8(capability_tag)
//!    || LP(payload)
//!
//! act_id    := SHA-256(device_act_v1)
//! signature := Ed25519(device_key, device_act_v1)
//! ```
//!
//! The payload is opaque to this layer. An action family that signs through this envelope puts
//! its own domain-separated canonical bytes in `payload`; this module never parses them.
//! [`DEVICE_ACT_DOMAIN`] is distinct from every N1 and GEN domain separator, so act bytes can never
//! be mistaken for an authority body, a signature preimage, a commitment, a KDF input or a GEN
//! reference, and vice versa.

use ed25519_dalek::{Signature, Signer, SigningKey};
use thiserror::Error;

use crate::authority_log::encoding::{Reader, Writer};
use crate::authority_log::{
    derive_prefix, sha256, AuthorityStore, AuthorityView, CodecError, DeviceCapability,
    DeviceGrant, PrincipalKey, SubjectId, ValiditySpan, MAX_POSITION,
};

/// Domain separator that opens every canonical device act. Length-prefixed in the encoding.
pub const DEVICE_ACT_DOMAIN: &[u8] = b"icn.n4.device-act";

/// Canonical device-act version. Decoding rejects any other value.
pub const DEVICE_ACT_VERSION: u16 = 1;

/// Upper bound on an act payload. A protocol constant, not a tunable: it bounds what a verifier
/// must hash and what an untrusted act can make a verifier allocate.
pub const MAX_DEVICE_ACT_PAYLOAD: usize = 64 * 1024;

/// Why a [`DeviceActV1`] could not be constructed, decoded or signed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeviceActError {
    /// The evaluation position is `0` (reserved for inception) or above [`MAX_POSITION`].
    #[error("evaluation position {0} is outside 1..=MAX_POSITION")]
    PositionOutOfRange(u64),
    /// The payload exceeds [`MAX_DEVICE_ACT_PAYLOAD`].
    #[error("act payload of {0} bytes exceeds the {MAX_DEVICE_ACT_PAYLOAD}-byte bound")]
    PayloadTooLarge(usize),
    /// The signing key offered does not belong to the device Principal the act names.
    #[error("signing key is not the device principal named by the act")]
    SignerIsNotDevice,
    /// The bytes are not a strict canonical device act.
    #[error("canonical device act is malformed: {0}")]
    Codec(#[from] CodecError),
}

/// One application-layer act, signed by a device, naming the Subject it acts for.
///
/// The fields are public because every one of them is public protocol data bound under the
/// signature. Construct through [`DeviceActV1::new`] or [`DeviceActV1::decode`], both of which
/// enforce the position and payload bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceActV1 {
    /// The Subject the device acts for. A Subject has no key; it is named, never a signer.
    pub subject: SubjectId,
    /// The device Principal that signs. Device-ness is in the grant, not the identifier.
    pub device: PrincipalKey,
    /// The capability the act exercises.
    pub capability: DeviceCapability,
    /// The log position the signer asks to be evaluated at. **Advisory until a relying party
    /// agrees**: [`verify_device_act`] refuses any act whose claim differs from the position the
    /// relying party supplies.
    pub evaluation_position: u64,
    /// Opaque application bytes. Never interpreted here.
    pub payload: Vec<u8>,
}

impl DeviceActV1 {
    /// Build an act, enforcing the position and payload bounds.
    pub fn new(
        subject: SubjectId,
        device: PrincipalKey,
        capability: DeviceCapability,
        evaluation_position: u64,
        payload: Vec<u8>,
    ) -> Result<Self, DeviceActError> {
        check_position(evaluation_position)?;
        if payload.len() > MAX_DEVICE_ACT_PAYLOAD {
            return Err(DeviceActError::PayloadTooLarge(payload.len()));
        }
        Ok(DeviceActV1 {
            subject,
            device,
            capability,
            evaluation_position,
            payload,
        })
    }

    /// The canonical byte encoding — the exact bytes that are signed and hashed.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(DEVICE_ACT_DOMAIN);
        w.u16(DEVICE_ACT_VERSION);
        w.b32(self.subject.as_bytes());
        self.device.encode(&mut w);
        w.u64(self.evaluation_position);
        w.u8(self.capability.tag());
        w.lp(&self.payload);
        w.finish()
    }

    /// Strict decode: wrong domain, wrong version, a non-principal in the device slot, an
    /// out-of-range position, an oversized payload or trailing bytes are all errors.
    pub fn decode(bytes: &[u8]) -> Result<Self, DeviceActError> {
        let mut r = Reader::new(bytes);
        r.expect_lp(DEVICE_ACT_DOMAIN, "domain")?;
        let version = r.u16("version")?;
        if version != DEVICE_ACT_VERSION {
            return Err(CodecError::UnsupportedVersion(version).into());
        }
        let subject = SubjectId::from_bytes(r.b32("subject")?);
        let device = PrincipalKey::decode(&mut r, "device")?;
        let evaluation_position = r.u64("evaluation_position")?;
        let capability = DeviceCapability::from_tag(r.u8("capability")?)?;
        let payload = r.lp("payload")?.to_vec();
        r.finish()?;
        DeviceActV1::new(subject, device, capability, evaluation_position, payload)
    }

    /// `SHA-256(canonical_bytes)`. Witness-independent: two signatures over one act share one id.
    /// This is the identity an action family should key a replay slot by.
    pub fn act_id(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }
}

/// An Ed25519 signature over [`DeviceActV1::canonical_bytes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceActSignature([u8; 64]);

impl DeviceActSignature {
    /// Wrap raw signature bytes.
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        DeviceActSignature(bytes)
    }

    /// The raw signature bytes.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

/// An act together with its device signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedDeviceAct {
    /// The act.
    pub act: DeviceActV1,
    /// The device's signature over the act's canonical bytes.
    pub signature: DeviceActSignature,
}

/// Sign `act` with the device key it names.
///
/// Refuses a key that is not the named device: an act that names one Principal and is signed by
/// another would be rejected at verification anyway, and refusing here keeps a client from
/// producing bytes it cannot use.
pub fn sign_device_act(
    act: &DeviceActV1,
    device_key: &SigningKey,
) -> Result<SignedDeviceAct, DeviceActError> {
    if PrincipalKey::from_signing_key(device_key) != act.device {
        return Err(DeviceActError::SignerIsNotDevice);
    }
    let signature = device_key.sign(&act.canonical_bytes());
    Ok(SignedDeviceAct {
        act: act.clone(),
        signature: DeviceActSignature::from_bytes(signature.to_bytes()),
    })
}

/// Why device authority could not be established at the evaluated position.
///
/// Every variant is a refusal. Variants carry positions and spans only — never payload bytes.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeviceAuthorityRefusal {
    /// The position is `0` or above [`MAX_POSITION`].
    #[error("evaluation position {0} is outside 1..=MAX_POSITION")]
    PositionOutOfRange(u64),
    /// No inception body for this Subject is durably known.
    #[error("subject is unknown to this store")]
    SubjectUnknown,
    /// Two or more equally-authorized candidates exist at or below the evaluated position.
    #[error("subject authority is halted at position {disputed_at}")]
    AuthorityHalted {
        /// The disputed position.
        disputed_at: u64,
    },
    /// The retained prefix does not reach the evaluated position.
    #[error("retained prefix reaches only position {frontier}, evaluation requires {required}")]
    PrefixIncomplete {
        /// The first position in the prefix with no authorized candidate.
        frontier: u64,
        /// The position the relying party asked to evaluate at.
        required: u64,
    },
    /// The Principal is the Subject's establishment authority at this position. It may well sign
    /// — but not as a bounded device.
    #[error("principal is the subject's establishment authority, not a device")]
    DeviceIsEstablishmentAuthority,
    /// No grant names this device as of the evaluated position.
    #[error("device holds no grant from this subject at the evaluated position")]
    DeviceNotAuthorized,
    /// A grant exists but is not in force at the evaluated position.
    #[error("device grant made at {granted_at} is not in force at the evaluated position")]
    GrantNotInForce {
        /// The position the grant was made at.
        granted_at: u64,
        /// The grant's validity span, if any.
        validity: Option<ValiditySpan>,
    },
    /// The grant is in force but does not carry the requested capability.
    #[error("device grant does not carry capability {0:?}")]
    CapabilityNotGranted(DeviceCapability),
}

/// What a successful evaluation establishes. A grant, not a bool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAuthorityEvidence {
    /// The Subject whose log was evaluated.
    pub subject: SubjectId,
    /// The device whose grant was found.
    pub device: PrincipalKey,
    /// The capability that was checked.
    pub capability: DeviceCapability,
    /// The relying party's evaluation position.
    pub evaluation_position: u64,
    /// The grant that covered the act at that position.
    pub grant: DeviceGrant,
    /// The establishment generation the prefix was in at that position.
    pub generation: u64,
}

/// Evaluate whether `device` holds `capability` from `subject` at `position`, over the retained
/// prefix of `store` through `position`.
///
/// `position` is the relying party's choice — the last position it relies on, typically the
/// position of the last fact it has itself observed and retained. The prefix must contain exactly
/// one authorized candidate at every position `1..=position`; a gap or a fork fails closed.
pub fn evaluate_device_authority(
    subject: SubjectId,
    store: &AuthorityStore,
    device: PrincipalKey,
    capability: DeviceCapability,
    position: u64,
) -> Result<DeviceAuthorityEvidence, DeviceAuthorityRefusal> {
    if position == 0 || position > MAX_POSITION {
        return Err(DeviceAuthorityRefusal::PositionOutOfRange(position));
    }
    let (state, frontier) = match derive_prefix(subject, store, position) {
        AuthorityView::Unknown => return Err(DeviceAuthorityRefusal::SubjectUnknown),
        AuthorityView::Halted { disputed_at, .. } => {
            return Err(DeviceAuthorityRefusal::AuthorityHalted { disputed_at });
        }
        AuthorityView::Live { state, frontier } => (state, frontier),
    };
    if frontier <= position {
        return Err(DeviceAuthorityRefusal::PrefixIncomplete {
            frontier,
            required: position,
        });
    }
    if state.authority.contains(&device) {
        return Err(DeviceAuthorityRefusal::DeviceIsEstablishmentAuthority);
    }
    let grant = state
        .devices
        .get(&device)
        .ok_or(DeviceAuthorityRefusal::DeviceNotAuthorized)?;
    if !grant.in_force_at(position) {
        return Err(DeviceAuthorityRefusal::GrantNotInForce {
            granted_at: grant.granted_at,
            validity: grant.validity,
        });
    }
    if !grant.capabilities.contains(capability) {
        return Err(DeviceAuthorityRefusal::CapabilityNotGranted(capability));
    }
    Ok(DeviceAuthorityEvidence {
        subject,
        device,
        capability,
        evaluation_position: position,
        grant: grant.clone(),
        generation: state.generation,
    })
}

/// Why a [`SignedDeviceAct`] was rejected. Every variant is a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeviceActVerifyError {
    /// The act names a Subject other than the one the relying party is evaluating for.
    #[error("act names a different subject than the relying party expects")]
    ActSubjectMismatch,
    /// The act's claimed evaluation position is not the relying party's.
    #[error("act claims evaluation position {act}, relying party evaluates at {relying}")]
    EvaluationPositionMismatch {
        /// The position bound in the act.
        act: u64,
        /// The position the relying party supplied.
        relying: u64,
    },
    /// The signature does not verify under the device key the act names.
    #[error("act signature does not verify under the named device principal")]
    BadSignature,
    /// The signature verifies, but the device holds no covering authority at the position.
    #[error("device authority refused: {0}")]
    Refused(#[source] DeviceAuthorityRefusal),
}

/// Verify a signed device act for `subject` at the relying party's `position`.
///
/// Order of checks, each fail-closed:
///
/// 1. the act names `subject`;
/// 2. the act's bound position equals `position` (the signer does not choose the window);
/// 3. the signature verifies under the act's device key — authorship, state-independent;
/// 4. [`evaluate_device_authority`] — authorization, over the retained prefix.
///
/// Steps 3 and 4 are two checks of two different things and are never merged: a verifying
/// signature proves that key signed these bytes, never that the key was authorized.
pub fn verify_device_act(
    signed: &SignedDeviceAct,
    subject: SubjectId,
    store: &AuthorityStore,
    position: u64,
) -> Result<DeviceAuthorityEvidence, DeviceActVerifyError> {
    let act = &signed.act;
    if act.subject != subject {
        return Err(DeviceActVerifyError::ActSubjectMismatch);
    }
    if act.evaluation_position != position {
        return Err(DeviceActVerifyError::EvaluationPositionMismatch {
            act: act.evaluation_position,
            relying: position,
        });
    }
    let signature = Signature::from_bytes(signed.signature.as_bytes());
    act.device
        .verifying_key()
        .verify_strict(&act.canonical_bytes(), &signature)
        .map_err(|_| DeviceActVerifyError::BadSignature)?;
    evaluate_device_authority(subject, store, act.device, act.capability, position)
        .map_err(DeviceActVerifyError::Refused)
}

fn check_position(position: u64) -> Result<(), DeviceActError> {
    if position == 0 || position > MAX_POSITION {
        return Err(DeviceActError::PositionOutOfRange(position));
    }
    Ok(())
}
