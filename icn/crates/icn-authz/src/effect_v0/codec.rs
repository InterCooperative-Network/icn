//! Strict canonical framing for the v0 authority-to-effect formats.
//!
//! The same shape as the N1/N4 codec in `icn-identity` (big-endian integers, `u32be`
//! length-prefixed byte strings, a leading domain string and a version), re-stated here because
//! that codec is crate-private. Nothing here is serde-derived: the bytes are the contract.

use thiserror::Error;

/// Why v0 bytes did not decode.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthzCodecError {
    /// Input ended inside a field.
    #[error("unexpected end of input in {field}")]
    UnexpectedEnd {
        /// The field being read.
        field: &'static str,
    },
    /// Bytes remained after the last field.
    #[error("trailing bytes after the encoded value")]
    TrailingBytes,
    /// The leading domain string was not the expected one.
    #[error("wrong domain string in {field}")]
    WrongDomain {
        /// The field being read.
        field: &'static str,
    },
    /// The version is not one this decoder accepts.
    #[error("unsupported version {0}")]
    UnsupportedVersion(u16),
    /// A length or count exceeded its bound.
    #[error("{field}: length {len} exceeds bound {max}")]
    TooLong {
        /// The field being read.
        field: &'static str,
        /// The declared length.
        len: usize,
        /// The bound.
        max: usize,
    },
    /// A sorted list was not strictly ascending (unsorted, or a duplicate).
    #[error("{field} is not strictly ascending")]
    NotStrictlyAscending {
        /// The list.
        field: &'static str,
    },
    /// 32 bytes in a principal slot are not a canonical, non-weak Ed25519 key.
    #[error("{field} is not a valid principal key")]
    InvalidPrincipal {
        /// The field being read.
        field: &'static str,
    },
    /// A text field is not UTF-8.
    #[error("{field} is not UTF-8")]
    InvalidUtf8 {
        /// The field being read.
        field: &'static str,
    },
    /// A field held a value outside its defined range.
    #[error("{field} holds an invalid value")]
    InvalidValue {
        /// The field being read.
        field: &'static str,
    },
    /// A carried N4 evidence bundle did not decode.
    #[error("vote bundle {index} did not decode: {reason}")]
    VoteBundle {
        /// Position of the bundle in the evidence.
        index: usize,
        /// The N4 decoder's reason.
        reason: String,
    },
}

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

    pub(crate) fn b64(&mut self, value: &[u8; 64]) {
        self.buf.extend_from_slice(value);
    }

    /// `u32be(len) || bytes`. Callers bound `len` (every v0 field has a bound far below 4 GiB and
    /// is checked before encoding or at decode).
    pub(crate) fn lp(&mut self, value: &[u8]) {
        self.u32(value.len() as u32);
        self.buf.extend_from_slice(value);
    }

    /// A count prefix for a list.
    pub(crate) fn count(&mut self, n: usize) {
        self.u32(n as u32);
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.buf
    }
}

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], AuthzCodecError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(AuthzCodecError::UnexpectedEnd { field })?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(AuthzCodecError::UnexpectedEnd { field })?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self, field: &'static str) -> Result<[u8; N], AuthzCodecError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N, field)?);
        Ok(out)
    }

    pub(crate) fn u8(&mut self, field: &'static str) -> Result<u8, AuthzCodecError> {
        Ok(self.array::<1>(field)?[0])
    }

    pub(crate) fn u16(&mut self, field: &'static str) -> Result<u16, AuthzCodecError> {
        Ok(u16::from_be_bytes(self.array(field)?))
    }

    pub(crate) fn u32(&mut self, field: &'static str) -> Result<u32, AuthzCodecError> {
        Ok(u32::from_be_bytes(self.array(field)?))
    }

    pub(crate) fn u64(&mut self, field: &'static str) -> Result<u64, AuthzCodecError> {
        Ok(u64::from_be_bytes(self.array(field)?))
    }

    pub(crate) fn b32(&mut self, field: &'static str) -> Result<[u8; 32], AuthzCodecError> {
        self.array(field)
    }

    pub(crate) fn b64(&mut self, field: &'static str) -> Result<[u8; 64], AuthzCodecError> {
        self.array(field)
    }

    /// A length-prefixed byte string of at most `max` bytes.
    pub(crate) fn lp(
        &mut self,
        field: &'static str,
        max: usize,
    ) -> Result<&'a [u8], AuthzCodecError> {
        let len = self.u32(field)? as usize;
        if len > max {
            return Err(AuthzCodecError::TooLong { field, len, max });
        }
        self.take(len, field)
    }

    /// A list count of at most `max` elements.
    pub(crate) fn count(
        &mut self,
        field: &'static str,
        max: usize,
    ) -> Result<usize, AuthzCodecError> {
        let len = self.u32(field)? as usize;
        if len > max {
            return Err(AuthzCodecError::TooLong { field, len, max });
        }
        Ok(len)
    }

    /// Expect the exact length-prefixed `domain`.
    pub(crate) fn expect_domain(
        &mut self,
        domain: &[u8],
        field: &'static str,
    ) -> Result<(), AuthzCodecError> {
        let got = self.lp(field, domain.len())?;
        if got != domain {
            return Err(AuthzCodecError::WrongDomain { field });
        }
        Ok(())
    }

    /// Expect the v0 version number.
    pub(crate) fn expect_version(&mut self, version: u16) -> Result<(), AuthzCodecError> {
        let got = self.u16("version")?;
        if got != version {
            return Err(AuthzCodecError::UnsupportedVersion(got));
        }
        Ok(())
    }

    /// UTF-8 text of at most `max` bytes.
    pub(crate) fn text(
        &mut self,
        field: &'static str,
        max: usize,
    ) -> Result<String, AuthzCodecError> {
        let bytes = self.lp(field, max)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| AuthzCodecError::InvalidUtf8 { field })
    }

    pub(crate) fn finish(self) -> Result<(), AuthzCodecError> {
        if self.pos != self.buf.len() {
            return Err(AuthzCodecError::TrailingBytes);
        }
        Ok(())
    }
}
