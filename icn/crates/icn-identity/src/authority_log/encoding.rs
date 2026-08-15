//! Deterministic, length-prefixed canonical encoding primitives.
//!
//! Every variable-length field is prefixed with a big-endian `u32` length; every fixed-width
//! field has a single fixed width. There is no self-describing framing, no map ordering, and no
//! floating point — so an encoder has no freedom and a decoder has no ambiguity.

use thiserror::Error;

/// Errors produced while decoding a canonical body.
///
/// Every variant is a *rejection*: a body that produces any of these is not admissible, and
/// because the decision uses only the bytes, every replica rejects it identically.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CodecError {
    /// The input ended before a field could be read.
    #[error("canonical body ended before field '{field}' could be read")]
    UnexpectedEnd {
        /// Name of the field that could not be read.
        field: &'static str,
    },
    /// Bytes remain after a complete body was decoded. Strictness matters: trailing bytes would
    /// give one logical body more than one encoding.
    #[error("canonical body has {0} trailing byte(s)")]
    TrailingBytes(usize),
    /// The leading domain separator is absent or wrong.
    #[error("canonical body does not carry the authority-log domain separator")]
    BadDomain,
    /// The protocol version is not [`super::PROTOCOL_VERSION`].
    #[error("unsupported authority-log protocol version {0}")]
    UnsupportedVersion(u16),
    /// The body kind tag is not one of the five defined kinds.
    #[error("unknown authority-log body kind tag 0x{0:02x}")]
    UnknownKind(u8),
    /// A principal field did not carry [`super::PRINCIPAL_TAG_ED25519`].
    ///
    /// This is the check that rejects a `SubjectId` (or any other 32-byte value that is not a
    /// key) placed where a raw authority principal is required.
    #[error("expected an Ed25519 principal tag, found 0x{0:02x}")]
    BadPrincipalTag(u8),
    /// The 32 bytes in a principal field are not a valid Ed25519 public key.
    #[error("principal field does not contain a valid Ed25519 public key")]
    InvalidPublicKey,
    /// The public-key bytes encode an Edwards point non-canonically.
    ///
    /// Accepting two encodings of one point would give one mathematical principal two protocol
    /// identities, commitments, and event IDs.
    #[error("principal field contains a non-canonical Ed25519 public-key encoding")]
    NonCanonicalPublicKey,
    /// The public key is a small-order Edwards point.
    ///
    /// Such keys admit signatures that do not provide the authority proof required by this log.
    #[error("principal field contains a weak Ed25519 public key")]
    WeakPublicKey,
    /// A set was not encoded in strictly ascending order, or contained a duplicate.
    #[error("set field '{field}' is not strictly ascending (duplicate or reordered member)")]
    UnorderedSet {
        /// Name of the offending set field.
        field: &'static str,
    },
    /// A set that must be non-empty was empty.
    #[error("set field '{field}' must be non-empty")]
    EmptySet {
        /// Name of the offending set field.
        field: &'static str,
    },
    /// A capability tag is not a defined capability.
    #[error("unknown device capability tag 0x{0:02x}")]
    UnknownCapability(u8),
    /// A validity span presence tag was neither `0x00` nor `0x01`.
    #[error("invalid optional-field presence tag 0x{0:02x}")]
    BadOptionTag(u8),
    /// A validity span had `until_position < from_position`.
    #[error("validity span is inverted ({from} > {until})")]
    InvertedSpan {
        /// Inclusive lower bound.
        from: u64,
        /// Inclusive upper bound.
        until: u64,
    },
    /// A non-inception body claimed position `0`, which is reserved for inception.
    #[error("position 0 is reserved for the inception body")]
    ReservedPosition,
    /// An inception body named a signer that is not a member of its own initial authority set.
    #[error("inception signer is not a member of the initial authority set")]
    SignerNotInInitialAuthority,
    /// A declared element count exceeds what the remaining input could possibly hold.
    #[error("declared element count {0} exceeds the remaining input")]
    CountOverflow(u32),
}

/// Append-only canonical encoder.
pub(crate) struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Writer { buf: Vec::new() }
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn b32(&mut self, value: &[u8; 32]) {
        self.buf.extend_from_slice(value);
    }

    /// Write a length-prefixed byte string: `u32be(len) || bytes`.
    pub(crate) fn lp(&mut self, value: &[u8]) {
        // Domain separators and every other length-prefixed field in this module are short,
        // fixed, compile-time constants; a `u32` length is more than sufficient.
        self.u32(value.len() as u32);
        self.buf.extend_from_slice(value);
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.buf
    }
}

/// Strict canonical decoder.
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], CodecError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(CodecError::UnexpectedEnd { field })?;
        if end > self.buf.len() {
            return Err(CodecError::UnexpectedEnd { field });
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub(crate) fn u8(&mut self, field: &'static str) -> Result<u8, CodecError> {
        Ok(self.take(1, field)?[0])
    }

    pub(crate) fn u16(&mut self, field: &'static str) -> Result<u16, CodecError> {
        let bytes = self.take(2, field)?;
        let mut out = [0u8; 2];
        out.copy_from_slice(bytes);
        Ok(u16::from_be_bytes(out))
    }

    pub(crate) fn u32(&mut self, field: &'static str) -> Result<u32, CodecError> {
        let bytes = self.take(4, field)?;
        let mut out = [0u8; 4];
        out.copy_from_slice(bytes);
        Ok(u32::from_be_bytes(out))
    }

    pub(crate) fn u64(&mut self, field: &'static str) -> Result<u64, CodecError> {
        let bytes = self.take(8, field)?;
        let mut out = [0u8; 8];
        out.copy_from_slice(bytes);
        Ok(u64::from_be_bytes(out))
    }

    pub(crate) fn b32(&mut self, field: &'static str) -> Result<[u8; 32], CodecError> {
        let bytes = self.take(32, field)?;
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    /// Read a length-prefixed byte string and require it to equal `expected`.
    pub(crate) fn expect_lp(
        &mut self,
        expected: &[u8],
        field: &'static str,
    ) -> Result<(), CodecError> {
        let len = self.u32(field)? as usize;
        let actual = self.take(len, field)?;
        if actual != expected {
            return Err(CodecError::BadDomain);
        }
        Ok(())
    }

    /// Read an element count, rejecting counts that could not possibly fit in the remaining
    /// input. Without this a hostile `u32::MAX` count would drive a huge speculative allocation
    /// before the read failed.
    pub(crate) fn count(
        &mut self,
        min_element_len: usize,
        field: &'static str,
    ) -> Result<u32, CodecError> {
        let n = self.u32(field)?;
        let remaining = self.buf.len().saturating_sub(self.pos);
        let needed = (n as usize).saturating_mul(min_element_len);
        if needed > remaining {
            return Err(CodecError::CountOverflow(n));
        }
        Ok(n)
    }

    /// Assert the input is fully consumed.
    pub(crate) fn finish(self) -> Result<(), CodecError> {
        let remaining = self.buf.len().saturating_sub(self.pos);
        if remaining != 0 {
            return Err(CodecError::TrailingBytes(remaining));
        }
        Ok(())
    }
}
