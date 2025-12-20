//! Security audit logging
//!
//! Logs security-relevant events for compliance, forensics, and monitoring:
//! - Authentication attempts (success/failure)
//! - Authorization failures
//! - Rate limit violations
//! - Invalid scope requests
//! - TLS handshake failures
//! - Suspicious activity patterns

use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

/// Security event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEvent {
    /// Authentication attempt
    AuthAttempt {
        did: String,
        success: bool,
        reason: Option<String>,
        ip: String,
    },
    /// Authorization failure (insufficient permissions)
    AuthzFailure {
        did: String,
        resource: String,
        required_scope: String,
        ip: String,
    },
    /// Rate limit exceeded
    RateLimitExceeded {
        did: Option<String>,
        ip: String,
        endpoint: String,
    },
    /// Invalid scope request (potential privilege escalation attempt)
    InvalidScopeRequest {
        did: String,
        invalid_scopes: Vec<String>,
        ip: String,
    },
    /// TLS handshake failure (untrusted peer)
    TlsHandshakeFailure {
        peer_did: Option<String>,
        peer_ip: String,
        reason: String,
    },
    /// Suspicious activity detected
    SuspiciousActivity {
        did: Option<String>,
        ip: String,
        activity_type: String,
        details: String,
    },
}

/// Audit logger for security events
pub struct AuditLogger;

impl AuditLogger {
    /// Log authentication attempt
    pub fn log_auth_attempt(did: &str, success: bool, reason: Option<&str>, ip: &str) {
        let event = SecurityEvent::AuthAttempt {
            did: did.to_string(),
            success,
            reason: reason.map(String::from),
            ip: ip.to_string(),
        };

        if success {
            info!(
                event_type = "auth_attempt",
                did = did,
                success = true,
                ip = ip,
                "Authentication successful"
            );
        } else {
            warn!(
                event_type = "auth_attempt",
                did = did,
                success = false,
                reason = reason.unwrap_or("unknown"),
                ip = ip,
                "Authentication failed"
            );
        }

        // Send to structured logging
        Self::emit_event(event);
    }

    /// Log authorization failure
    pub fn log_authz_failure(did: &str, resource: &str, required_scope: &str, ip: &str) {
        let event = SecurityEvent::AuthzFailure {
            did: did.to_string(),
            resource: resource.to_string(),
            required_scope: required_scope.to_string(),
            ip: ip.to_string(),
        };

        warn!(
            event_type = "authz_failure",
            did = did,
            resource = resource,
            required_scope = required_scope,
            ip = ip,
            "Authorization failed: insufficient permissions"
        );

        Self::emit_event(event);
    }

    /// Log rate limit exceeded
    pub fn log_rate_limit_exceeded(did: Option<&str>, ip: &str, endpoint: &str) {
        let event = SecurityEvent::RateLimitExceeded {
            did: did.map(String::from),
            ip: ip.to_string(),
            endpoint: endpoint.to_string(),
        };

        warn!(
            event_type = "rate_limit_exceeded",
            did = did.unwrap_or("unauthenticated"),
            ip = ip,
            endpoint = endpoint,
            "Rate limit exceeded"
        );

        Self::emit_event(event);
    }

    /// Log invalid scope request (privilege escalation attempt)
    pub fn log_invalid_scope_request(did: &str, invalid_scopes: &[String], ip: &str) {
        let event = SecurityEvent::InvalidScopeRequest {
            did: did.to_string(),
            invalid_scopes: invalid_scopes.to_vec(),
            ip: ip.to_string(),
        };

        error!(
            event_type = "invalid_scope_request",
            did = did,
            invalid_scopes = ?invalid_scopes,
            ip = ip,
            "SECURITY: Invalid scope request detected (potential privilege escalation)"
        );

        Self::emit_event(event);
    }

    /// Log TLS handshake failure
    pub fn log_tls_handshake_failure(peer_did: Option<&str>, peer_ip: &str, reason: &str) {
        let event = SecurityEvent::TlsHandshakeFailure {
            peer_did: peer_did.map(String::from),
            peer_ip: peer_ip.to_string(),
            reason: reason.to_string(),
        };

        warn!(
            event_type = "tls_handshake_failure",
            peer_did = peer_did.unwrap_or("unknown"),
            peer_ip = peer_ip,
            reason = reason,
            "TLS handshake failed"
        );

        Self::emit_event(event);
    }

    /// Log suspicious activity
    pub fn log_suspicious_activity(
        did: Option<&str>,
        ip: &str,
        activity_type: &str,
        details: &str,
    ) {
        let event = SecurityEvent::SuspiciousActivity {
            did: did.map(String::from),
            ip: ip.to_string(),
            activity_type: activity_type.to_string(),
            details: details.to_string(),
        };

        error!(
            event_type = "suspicious_activity",
            did = did.unwrap_or("unknown"),
            ip = ip,
            activity_type = activity_type,
            details = details,
            "SECURITY: Suspicious activity detected"
        );

        Self::emit_event(event);
    }

    /// Emit event to structured logging backend
    /// In production, this could send to:
    /// - Elasticsearch
    /// - Splunk
    /// - CloudWatch Logs
    /// - Datadog
    /// - Custom SIEM
    fn emit_event(event: SecurityEvent) {
        // For now, just ensure it's in structured logs
        // In production, you'd send to your audit log backend
        match serde_json::to_string(&event) {
            Ok(json) => {
                info!(
                    audit_event = true,
                    event_json = json,
                    "Security audit event"
                );
            }
            Err(e) => {
                error!("Failed to serialize security event: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_logger_formats() {
        // Just verify events can be created and serialized
        let event = SecurityEvent::AuthAttempt {
            did: "did:icn:alice".to_string(),
            success: false,
            reason: Some("invalid signature".to_string()),
            ip: "192.168.1.1".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("did:icn:alice"));
        assert!(json.contains("invalid signature"));
    }

    #[test]
    fn test_invalid_scope_event() {
        let event = SecurityEvent::InvalidScopeRequest {
            did: "did:icn:mallory".to_string(),
            invalid_scopes: vec!["admin".to_string(), "root".to_string()],
            ip: "10.0.0.1".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("mallory"));
        assert!(json.contains("admin"));
        assert!(json.contains("root"));
    }
}
