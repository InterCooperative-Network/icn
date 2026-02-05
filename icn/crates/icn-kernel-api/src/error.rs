//! Stable Protocol Error Codes
//!
//! Every rejection path in the ICN protocol MUST use one of these codes.
//! Gateway maps them to stable HTTP status codes. SDK maps them to typed exceptions.
//!
//! # Wire Format Stability
//!
//! `ErrCode` variants are serialized as lowercase strings (e.g., `"unauthorized"`).
//! Adding new variants is backwards-compatible. Removing or renaming is not.

use serde::{Deserialize, Serialize};

/// Stable error codes for the ICN protocol.
///
/// These codes are the canonical way to communicate rejection reasons across
/// all layers: kernel, gossip, gateway, and SDK. Each code maps to exactly
/// one HTTP status code.
///
/// # Usage
///
/// ```rust
/// use icn_kernel_api::error::{ErrCode, IcnError};
///
/// let err = IcnError::new(ErrCode::Unauthorized, "missing DID in request");
/// assert_eq!(err.http_status(), 401);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrCode {
    /// Missing or invalid identity (HTTP 401)
    Unauthorized,
    /// Signature verification failed (HTTP 401)
    InvalidSignature,
    /// Identity valid but action not permitted (HTTP 403)
    Forbidden,
    /// Message already processed - nonce/sequence replay (HTTP 409)
    Replay,
    /// Request or transfer past expiry (HTTP 410)
    Expired,
    /// Resource does not exist (HTTP 404)
    NotFound,
    /// PolicyOracle rate limit exceeded (HTTP 429)
    RateLimited,
    /// Payload exceeds size limit (HTTP 413)
    Oversize,
    /// Resource exhaustion - max in-flight, max reassemblies (HTTP 503)
    Busy,
    /// Malformed request - wrong format, bad parameters (HTTP 400)
    InvalidRequest,
}

impl ErrCode {
    /// Map this error code to an HTTP status code.
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::Unauthorized => 401,
            Self::InvalidSignature => 401,
            Self::Forbidden => 403,
            Self::Replay => 409,
            Self::Expired => 410,
            Self::NotFound => 404,
            Self::RateLimited => 429,
            Self::Oversize => 413,
            Self::Busy => 503,
            Self::InvalidRequest => 400,
        }
    }

    /// Wire-format string for this code (matches serde serialization).
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::InvalidSignature => "invalid_signature",
            Self::Forbidden => "forbidden",
            Self::Replay => "replay",
            Self::Expired => "expired",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::Oversize => "oversize",
            Self::Busy => "busy",
            Self::InvalidRequest => "invalid_request",
        }
    }
}

impl std::fmt::Display for ErrCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured protocol error with stable code and human-readable message.
///
/// This is the canonical error type for protocol-level rejections.
/// Gateway serializes it as `{"code": "...", "error": "..."}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IcnError {
    /// Stable error code
    pub code: ErrCode,
    /// Human-readable description (not stable, do not match on this)
    #[serde(rename = "error")]
    pub message: String,
    /// Optional structured details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl IcnError {
    /// Create a new protocol error.
    pub fn new(code: ErrCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Create a new protocol error with details.
    pub fn with_details(
        code: ErrCode,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details),
        }
    }

    /// Get the HTTP status code for this error.
    pub const fn http_status(&self) -> u16 {
        self.code.http_status()
    }
}

impl std::fmt::Display for IcnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for IcnError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errcode_serialization_stable() {
        // Verify serde roundtrip produces stable wire format
        let codes = [
            (ErrCode::Unauthorized, "\"unauthorized\""),
            (ErrCode::InvalidSignature, "\"invalid_signature\""),
            (ErrCode::Forbidden, "\"forbidden\""),
            (ErrCode::Replay, "\"replay\""),
            (ErrCode::Expired, "\"expired\""),
            (ErrCode::NotFound, "\"not_found\""),
            (ErrCode::RateLimited, "\"rate_limited\""),
            (ErrCode::Oversize, "\"oversize\""),
            (ErrCode::Busy, "\"busy\""),
            (ErrCode::InvalidRequest, "\"invalid_request\""),
        ];

        for (code, expected_json) in &codes {
            let json = serde_json::to_string(code).expect("serialize");
            assert_eq!(
                &json, expected_json,
                "serialization mismatch for {:?}",
                code
            );

            let roundtrip: ErrCode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&roundtrip, code, "roundtrip mismatch for {:?}", code);
        }
    }

    #[test]
    fn http_error_mapping_is_stable() {
        assert_eq!(ErrCode::Unauthorized.http_status(), 401);
        assert_eq!(ErrCode::InvalidSignature.http_status(), 401);
        assert_eq!(ErrCode::Forbidden.http_status(), 403);
        assert_eq!(ErrCode::Replay.http_status(), 409);
        assert_eq!(ErrCode::Expired.http_status(), 410);
        assert_eq!(ErrCode::NotFound.http_status(), 404);
        assert_eq!(ErrCode::RateLimited.http_status(), 429);
        assert_eq!(ErrCode::Oversize.http_status(), 413);
        assert_eq!(ErrCode::Busy.http_status(), 503);
        assert_eq!(ErrCode::InvalidRequest.http_status(), 400);
    }

    #[test]
    fn icn_error_serialization_roundtrip() {
        let err = IcnError::new(ErrCode::Replay, "message already processed");
        let json = serde_json::to_string(&err).expect("serialize");

        // Verify wire format has "code" and "error" fields
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(value["code"], "replay");
        assert_eq!(value["error"], "message already processed");
        assert!(value.get("details").is_none());

        let roundtrip: IcnError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(roundtrip.code, ErrCode::Replay);
        assert_eq!(roundtrip.message, "message already processed");
    }

    #[test]
    fn icn_error_with_details() {
        let err = IcnError::with_details(
            ErrCode::Oversize,
            "blob exceeds 10MB limit",
            serde_json::json!({ "size": 15_000_000, "max": 10_485_760 }),
        );
        let json = serde_json::to_string(&err).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");

        assert_eq!(value["code"], "oversize");
        assert_eq!(value["details"]["size"], 15_000_000);
        assert_eq!(value["details"]["max"], 10_485_760);
    }

    #[test]
    fn errcode_display_matches_serde() {
        for code in [
            ErrCode::Unauthorized,
            ErrCode::InvalidSignature,
            ErrCode::Forbidden,
            ErrCode::Replay,
            ErrCode::Expired,
            ErrCode::NotFound,
            ErrCode::RateLimited,
            ErrCode::Oversize,
            ErrCode::Busy,
            ErrCode::InvalidRequest,
        ] {
            let display = code.to_string();
            let serde_str = serde_json::to_string(&code)
                .expect("serialize")
                .trim_matches('"')
                .to_string();
            assert_eq!(
                display, serde_str,
                "Display and serde mismatch for {:?}",
                code
            );
        }
    }

    #[test]
    fn errcode_as_str_matches_display() {
        for code in [
            ErrCode::Unauthorized,
            ErrCode::InvalidSignature,
            ErrCode::Forbidden,
            ErrCode::Replay,
            ErrCode::Expired,
            ErrCode::NotFound,
            ErrCode::RateLimited,
            ErrCode::Oversize,
            ErrCode::Busy,
            ErrCode::InvalidRequest,
        ] {
            assert_eq!(code.as_str(), &code.to_string());
        }
    }
}
