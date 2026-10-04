//! N4-C — the enrollment request: what a new device presents, and what approval produces.
//!
//! The N4 ceremony (#2599) moves a *request* from a new device to the person's authority edge
//! and an N1 `Authorize` fact back. N1 already defines the fact (`construct::authorize_event`)
//! and N4-A/N4-B define how a relying party evaluates it. What was missing is the request: the
//! one object a device may present before it holds any authority at all. This module defines
//! it, and the approval step that turns it into the N1 fact, so the ceremony is executable
//! end to end in a fixture.
//!
//! # What is deliberately true here
//!
//! - **A request is possession, not authority.** It is self-signed by the device key it names,
//!   which proves only that whoever built it holds that key. It carries no certificate, no
//!   claim of authority, no secret, and nothing about the human beyond the `SubjectId` it asks
//!   to join. [`verify_enrollment_request`] answers "does this presenter hold this key", and
//!   nothing more; a verified request that was never approved is refused by N4-A exactly like
//!   any stranger (pinned).
//! - **Approval attenuates.** The approver chooses what to grant; the grant must be a subset
//!   of what was requested, must be non-empty, and may **never** include
//!   [`DeviceCapability::Recover`] (which no device requests and no approval grants: recovery
//!   authority is the person's own establishment authority, never delegated by this path).
//! - **The establishment authority is never a device.** A request naming the key that signs the
//!   Subject's log is refused at approval, as GEN-A refuses it at genesis and N4-A at evaluation.
//! - **The approver names the Subject.** The request *asks* for a Subject; approval refuses a
//!   request for any Subject other than the one the approver is acting for.
//! - **Rejection is silence; expiry is the approver's.** A rejected request produces no fact. N1
//!   has no clock, so the request carries a nonce rather than a deadline: an approver that wants
//!   requests to expire enforces that at its edge, and a device whose request lapsed simply
//!   builds a new one. Nothing about an unapproved request is ever durable.
//! - **Per-context keys.** A device presents the Principal it holds *for this context*
//!   (`HUMAN_IDENTITY_ARCHITECTURE.md` §11.1). Nothing here derives that key; the device does,
//!   from its own secret, and the derivation must not take the `SubjectId` as input.
//!
//! # What is deliberately not here
//!
//! No transport for the request (QR, link, network: the carrier's business). No approval UI.
//! No persistence of pending requests. No delivery of the resulting fact back to the device or
//! to relying parties (N3/N4 transport). No rotation, replacement or lost-device flow. No
//! capability beyond N1's four. No context kind. Fixture identities only in the tests.
//!
//! # Canonical request bytes
//!
//! N1 framing, reused: `LP(x) := u32be(len(x)) || x`, `b32` raw bytes, integers big-endian,
//! principals and capability sets encoded exactly as in an N1 body.
//!
//! ```text
//! enrollment_request_v1 :=
//!       LP(ENROLLMENT_REQUEST_DOMAIN)
//!    || u16be(ENROLLMENT_REQUEST_VERSION)
//!    || b32(subject)                         -- the Subject the device asks to join
//!    || u8(principal_tag) || b32(device_key) -- the device's per-context Principal
//!    || u32be(n) || n × u8(capability_tag)   -- requested capabilities, strictly ascending
//!    || LP(label)                            -- UTF-8, ≤ MAX_ENROLLMENT_LABEL bytes, for humans
//!    || b32(nonce)                           -- chosen by the device; freshness is the approver's
//!
//! request_id := SHA-256(enrollment_request_v1)
//! signature  := Ed25519(device_key, enrollment_request_v1)
//! ```

use ed25519_dalek::{Signature, Signer, SigningKey};
use thiserror::Error;

use crate::authority_log::encoding::{Reader, Writer};
use crate::authority_log::{
    authorize_event, sha256, CapabilitySet, CodecError, DeviceCapability, EventId, PrincipalKey,
    SignedAuthorityEvent, SubjectId, ValiditySpan,
};

/// Domain separator that opens every canonical enrollment request.
pub const ENROLLMENT_REQUEST_DOMAIN: &[u8] = b"icn.n4.enrollment-request";

/// Canonical request version. Decoding rejects any other value.
pub const ENROLLMENT_REQUEST_VERSION: u16 = 1;

/// Upper bound on the human-readable label, in bytes. A protocol constant.
pub const MAX_ENROLLMENT_LABEL: usize = 64;

/// Why an [`EnrollmentRequestV1`] could not be constructed, decoded or signed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnrollmentRequestError {
    /// The label exceeds [`MAX_ENROLLMENT_LABEL`] bytes.
    #[error("label of {0} bytes exceeds the {MAX_ENROLLMENT_LABEL}-byte bound")]
    LabelTooLong(usize),
    /// The label is not valid UTF-8.
    #[error("label is not valid UTF-8")]
    LabelNotUtf8,
    /// The request asks for nothing.
    #[error("a request must ask for at least one capability")]
    EmptyRequest,
    /// The request asks for `Recover`, which no device may request.
    #[error("a device never requests Recover")]
    RecoverRequested,
    /// The signing key offered is not the device Principal the request names.
    #[error("signing key is not the device principal named by the request")]
    SignerIsNotDevice,
    /// The bytes are not a strict canonical request.
    #[error("canonical enrollment request is malformed: {0}")]
    Codec(#[from] CodecError),
}

/// What a new device presents to the person's authority edge. Public protocol data, all of it
/// bound under the device's own signature. Construct through [`EnrollmentRequestV1::new`] or
/// [`EnrollmentRequestV1::decode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentRequestV1 {
    /// The Subject the device asks to be authorized under. Asked, not asserted.
    pub subject: SubjectId,
    /// The device's per-context Principal.
    pub device: PrincipalKey,
    /// The capabilities the device asks for. Never contains `Recover`.
    pub requested: CapabilitySet,
    /// A label for the approving human ("Katie desk Pi"). Interpreted by nobody else.
    pub label: String,
    /// Device-chosen bytes that make two otherwise identical requests distinct. Freshness
    /// policy belongs to the approver.
    pub nonce: [u8; 32],
}

impl EnrollmentRequestV1 {
    /// Build a request, enforcing the label bound, a non-empty ask, and no `Recover`.
    pub fn new(
        subject: SubjectId,
        device: PrincipalKey,
        requested: CapabilitySet,
        label: &str,
        nonce: [u8; 32],
    ) -> Result<Self, EnrollmentRequestError> {
        if label.len() > MAX_ENROLLMENT_LABEL {
            return Err(EnrollmentRequestError::LabelTooLong(label.len()));
        }
        if requested.members().is_empty() {
            return Err(EnrollmentRequestError::EmptyRequest);
        }
        if requested.contains(DeviceCapability::Recover) {
            return Err(EnrollmentRequestError::RecoverRequested);
        }
        Ok(EnrollmentRequestV1 {
            subject,
            device,
            requested,
            label: label.to_owned(),
            nonce,
        })
    }

    /// The canonical byte encoding — the exact bytes that are signed and hashed.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(ENROLLMENT_REQUEST_DOMAIN);
        w.u16(ENROLLMENT_REQUEST_VERSION);
        w.b32(self.subject.as_bytes());
        self.device.encode(&mut w);
        self.requested.encode(&mut w);
        w.lp(self.label.as_bytes());
        w.b32(&self.nonce);
        w.finish()
    }

    /// Strict decode: wrong domain or version, a non-principal in the device slot, an unordered
    /// or unknown capability, an over-long or non-UTF-8 label, `Recover`, an empty ask or
    /// trailing bytes are all errors.
    pub fn decode(bytes: &[u8]) -> Result<Self, EnrollmentRequestError> {
        let mut r = Reader::new(bytes);
        r.expect_lp(ENROLLMENT_REQUEST_DOMAIN, "domain")?;
        let version = r.u16("version")?;
        if version != ENROLLMENT_REQUEST_VERSION {
            return Err(CodecError::UnsupportedVersion(version).into());
        }
        let subject = SubjectId::from_bytes(r.b32("subject")?);
        let device = PrincipalKey::decode(&mut r, "device")?;
        let requested = CapabilitySet::decode(&mut r, "requested")?;
        let label_bytes = r.lp("label")?;
        let label =
            std::str::from_utf8(label_bytes).map_err(|_| EnrollmentRequestError::LabelNotUtf8)?;
        let nonce = r.b32("nonce")?;
        r.finish()?;
        EnrollmentRequestV1::new(subject, device, requested, label, nonce)
    }

    /// `SHA-256(canonical_bytes)`: the request's identity, signature-independent.
    pub fn request_id(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }
}

/// An Ed25519 signature over [`EnrollmentRequestV1::canonical_bytes`], by the device key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnrollmentRequestSignature([u8; 64]);

impl EnrollmentRequestSignature {
    /// Wrap raw signature bytes.
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        EnrollmentRequestSignature(bytes)
    }

    /// The raw signature bytes.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

/// A request with the device's own signature over it: proof of possession of the device key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedEnrollmentRequest {
    /// The request.
    pub request: EnrollmentRequestV1,
    /// The device's signature over the request's canonical bytes.
    pub signature: EnrollmentRequestSignature,
}

/// Sign a request with the device key it names. Refuses any other key.
pub fn sign_enrollment_request(
    request: &EnrollmentRequestV1,
    device_key: &SigningKey,
) -> Result<SignedEnrollmentRequest, EnrollmentRequestError> {
    if PrincipalKey::from_signing_key(device_key) != request.device {
        return Err(EnrollmentRequestError::SignerIsNotDevice);
    }
    let signature = device_key.sign(&request.canonical_bytes());
    Ok(SignedEnrollmentRequest {
        request: request.clone(),
        signature: EnrollmentRequestSignature::from_bytes(signature.to_bytes()),
    })
}

/// Why a signed request is not even a valid *claim of possession*.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnrollmentVerifyError {
    /// The signature does not verify under the device key the request names.
    #[error("request signature does not verify under the named device principal")]
    BadSignature,
}

/// Check that the presenter holds the device key the request names. **This is not
/// authorization**: a device whose request verifies holds no authority until an approver
/// records an `Authorize` fact, and N4-A refuses it until then.
pub fn verify_enrollment_request(
    signed: &SignedEnrollmentRequest,
) -> Result<(), EnrollmentVerifyError> {
    let signature = Signature::from_bytes(signed.signature.as_bytes());
    signed
        .request
        .device
        .verifying_key()
        .verify_strict(&signed.request.canonical_bytes(), &signature)
        .map_err(|_| EnrollmentVerifyError::BadSignature)
}

/// Why an approver refused to turn a request into a grant.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnrollmentApprovalError {
    /// The request does not prove possession of the device key.
    #[error("{0}")]
    Possession(#[from] EnrollmentVerifyError),
    /// The request asks for a Subject other than the one the approver acts for.
    #[error("request names a different subject than the approver acts for")]
    RequestSubjectMismatch,
    /// The approver tried to grant something the device did not ask for.
    #[error("grant is not a subset of what the device requested")]
    GrantExceedsRequest,
    /// The approver tried to grant nothing.
    #[error("a grant must carry at least one capability")]
    EmptyGrant,
    /// The approver tried to grant `Recover`, which this path never does.
    #[error("Recover is never granted by enrollment")]
    RecoverNeverGranted,
    /// The device is the Subject's establishment authority at this generation.
    #[error("the establishment authority cannot be enrolled as a device")]
    DeviceIsEstablishmentAuthority,
}

/// Turn a request into the N1 `Authorize` fact that grants it — attenuated to `granted`.
///
/// `establishment_key` is the Subject's current establishment signing key, held by the
/// person's authority edge and nowhere else; `position` and `prev_digest` are the next slot in
/// the Subject's log as that edge knows it. The returned fact is what every relying party will
/// later admit; delivering it to them is transport, not this function.
#[allow(clippy::too_many_arguments)]
pub fn approve_enrollment(
    signed: &SignedEnrollmentRequest,
    establishment_key: &SigningKey,
    subject: SubjectId,
    position: u64,
    prev_digest: EventId,
    granted: CapabilitySet,
    validity: Option<ValiditySpan>,
) -> Result<SignedAuthorityEvent, EnrollmentApprovalError> {
    verify_enrollment_request(signed)?;
    let request = &signed.request;
    if request.subject != subject {
        return Err(EnrollmentApprovalError::RequestSubjectMismatch);
    }
    if granted.contains(DeviceCapability::Recover) {
        return Err(EnrollmentApprovalError::RecoverNeverGranted);
    }
    if granted.members().is_empty() {
        return Err(EnrollmentApprovalError::EmptyGrant);
    }
    if !granted.members().is_subset(request.requested.members()) {
        return Err(EnrollmentApprovalError::GrantExceedsRequest);
    }
    if PrincipalKey::from_signing_key(establishment_key) == request.device {
        return Err(EnrollmentApprovalError::DeviceIsEstablishmentAuthority);
    }
    Ok(authorize_event(
        establishment_key,
        subject,
        position,
        prev_digest,
        request.device,
        granted,
        validity,
    ))
}
