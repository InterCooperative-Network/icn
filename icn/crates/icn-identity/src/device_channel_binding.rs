//! N4-D — binding a transport connection to a device Principal, by composition.
//!
//! A Home runtime terminating a connection must be able to conclude "this connection is acting
//! as device Principal `K` for Subject `S`" without trusting an IP address, a hostname, a
//! remote-desktop login, ownership of a TLS certificate, or possession of an enrollment request.
//! ICN binds a Principal to a TLS certificate only for **nodes** (`BindingInfo` in the network
//! Hello); that object is node-scoped and must not be repurposed for devices.
//!
//! This module adds **no kernel primitive**. A channel binding is a [`DeviceActV1`] with
//! capability [`DeviceCapability::Present`] whose payload is a domain-separated
//! [`ChannelBindingV1`]: the digest of the channel the device believes it is on (for TLS, the
//! certificate the client saw, or a `tls-exporter` value) and a nonce. The relying party that
//! owns the other end verifies the act with N4-A exactly as any act — authorship, then
//! authorization over the facts it retains, at the position it chooses — and then checks a single
//! equality: the bound channel digest is the channel it sees. Nothing else is consulted.
//!
//! # What this keeps apart
//!
//! | layer | answers | never answers |
//! |---|---|---|
//! | the transport (TLS, pairing, local network) | "is this channel private and the same at both ends?" | who is acting, under what authority |
//! | this binding | "the device Principal that signed this is on *this* channel" | whether that device is authorized |
//! | N4-A over retained facts | "is that device authorized for `Present` by `S` at `E`?" | whether the human approved anything today |
//! | the human, at the edge | approvals (N4-C) | — |
//!
//! A binding whose channel digest differs from what the relying party sees is refused **even if
//! the device is fully authorized**: that is the man-in-the-middle case, and it is a transport
//! fact, not an authority fact. A binding by an unauthorized or revoked device is refused by N4-A
//! before the channel is even compared.
//!
//! # Canonical payload bytes
//!
//! ```text
//! channel_binding_v1 := LP("icn.n4.channel-binding") || u16be(1) || b32(channel) || b32(nonce)
//! ```
//!
//! What `channel` is the digest *of* is the transport's business and is stated by the transport
//! profile that uses this binding; this module only requires that both ends compute the same
//! 32 bytes for the same channel and different bytes for different channels.

use ed25519_dalek::SigningKey;
use thiserror::Error;

use crate::authority_log::encoding::{Reader, Writer};
use crate::authority_log::{AuthorityStore, CodecError, DeviceCapability, PrincipalKey, SubjectId};
use crate::device_authority::{
    sign_device_act, verify_device_act, DeviceActError, DeviceActV1, DeviceActVerifyError,
    DeviceAuthorityEvidence, SignedDeviceAct,
};

/// Domain separator that opens every canonical channel binding. Length-prefixed in the encoding.
pub const CHANNEL_BINDING_DOMAIN: &[u8] = b"icn.n4.channel-binding";

/// Canonical channel-binding version. Decoding rejects any other value.
pub const CHANNEL_BINDING_VERSION: u16 = 1;

/// The payload of a channel-binding act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelBindingV1 {
    /// The 32-byte digest of the channel the device believes it is on.
    pub channel: [u8; 32],
    /// Device-chosen bytes; replay handling is the transport profile's (it may require the
    /// nonce to be its own fresh challenge).
    pub nonce: [u8; 32],
}

impl ChannelBindingV1 {
    /// Build a binding.
    pub const fn new(channel: [u8; 32], nonce: [u8; 32]) -> Self {
        ChannelBindingV1 { channel, nonce }
    }

    /// The canonical byte encoding, which becomes the act's payload.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(CHANNEL_BINDING_DOMAIN);
        w.u16(CHANNEL_BINDING_VERSION);
        w.b32(&self.channel);
        w.b32(&self.nonce);
        w.finish()
    }

    /// Strict decode: wrong domain or version, short input or trailing bytes are errors.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut r = Reader::new(bytes);
        r.expect_lp(CHANNEL_BINDING_DOMAIN, "domain")?;
        let version = r.u16("version")?;
        if version != CHANNEL_BINDING_VERSION {
            return Err(CodecError::UnsupportedVersion(version));
        }
        let channel = r.b32("channel")?;
        let nonce = r.b32("nonce")?;
        r.finish()?;
        Ok(ChannelBindingV1 { channel, nonce })
    }
}

/// Why a signed act did not bind this connection to an authorized device.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChannelBindingError {
    /// The act's capability is not `Present`: it may be a perfectly good act, but not a binding.
    #[error("a channel binding is a Present act; this act claims another capability")]
    NotAPresentation,
    /// The act's payload is not a canonical channel binding.
    #[error("act payload is not a channel binding: {0}")]
    Payload(#[from] CodecError),
    /// N4-A refused the act: bad authorship, or no covering authority at the position.
    #[error("{0}")]
    Act(#[from] DeviceActVerifyError),
    /// The device bound a channel other than the one this relying party sees.
    #[error("the bound channel is not the channel this relying party sees")]
    ChannelMismatch,
}

/// Build and sign a channel-binding act for `device` on `channel`.
pub fn bind_channel(
    subject: SubjectId,
    device: PrincipalKey,
    device_key: &SigningKey,
    position: u64,
    channel: [u8; 32],
    nonce: [u8; 32],
) -> Result<SignedDeviceAct, DeviceActError> {
    let act = DeviceActV1::new(
        subject,
        device,
        DeviceCapability::Present,
        position,
        ChannelBindingV1::new(channel, nonce).canonical_bytes(),
    )?;
    sign_device_act(&act, device_key)
}

/// Verify that `signed` binds `expected_channel` to a device authorized by `subject` at
/// `position`. Order, each fail-closed: the act is a presentation; its payload is a channel
/// binding; N4-A accepts the act (authorship, then authorization over `store`); the bound
/// channel equals the one the relying party sees.
pub fn verify_channel_binding(
    signed: &SignedDeviceAct,
    subject: SubjectId,
    store: &AuthorityStore,
    position: u64,
    expected_channel: &[u8; 32],
) -> Result<DeviceAuthorityEvidence, ChannelBindingError> {
    if signed.act.capability != DeviceCapability::Present {
        return Err(ChannelBindingError::NotAPresentation);
    }
    let binding = ChannelBindingV1::decode(&signed.act.payload)?;
    let evidence = verify_device_act(signed, subject, store, position)?;
    if binding.channel != *expected_channel {
        return Err(ChannelBindingError::ChannelMismatch);
    }
    Ok(evidence)
}
