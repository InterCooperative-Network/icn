//! RPC Context for request handling
//!
//! Provides authenticated context information for RPC handlers,
//! including caller identity and coop membership for access control.

use icn_identity::Did;

use crate::error_codes::RpcErrorCode;
use crate::types::RpcResponse;

/// Context for an authenticated RPC request.
///
/// This struct provides the caller's identity and cooperative membership
/// information to RPC handlers. It enables coop isolation by allowing
/// handlers to verify the caller has access to the requested cooperative.
#[derive(Debug, Clone)]
pub struct RpcContext {
    /// The authenticated caller's DID
    pub caller_did: Did,
    /// The cooperative ID from the JWT claims (if any)
    pub coop_id: Option<String>,
    /// The scopes granted to this token
    pub scopes: Vec<String>,
}

impl RpcContext {
    /// Create a new RPC context.
    pub fn new(caller_did: Did, coop_id: Option<String>, scopes: Vec<String>) -> Self {
        Self {
            caller_did,
            coop_id,
            scopes,
        }
    }

    /// Verify the caller has access to the specified cooperative.
    ///
    /// Returns Ok(()) if the caller's token coop_id matches the requested coop_id.
    /// Returns an error if:
    /// - The caller has no coop_id in their token (AuthenticationRequired)
    /// - The caller's coop_id doesn't match the requested coop_id (CoopAccessDenied)
    ///
    /// # Security
    ///
    /// This function logs all cross-coop access attempts for security audit.
    /// Successful access is logged at DEBUG level, denied access at WARN level.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctx: &RpcContext = ...;
    /// if let Err(e) = ctx.require_coop("coop-123") {
    ///     return e.to_default_response(id);
    /// }
    /// ```
    pub fn require_coop(&self, coop_id: &str) -> Result<(), RpcErrorCode> {
        match &self.coop_id {
            Some(c) if c == coop_id => {
                // Access granted - log for audit trail
                tracing::debug!(
                    caller = %self.caller_did,
                    coop_id = coop_id,
                    "Coop access granted"
                );
                Ok(())
            }
            Some(caller_coop) => {
                // Cross-coop access attempt blocked - log security event
                tracing::warn!(
                    caller = %self.caller_did,
                    caller_coop = caller_coop,
                    requested_coop = coop_id,
                    "Cross-coop access denied: token is for different cooperative"
                );
                // Increment security metric
                metrics::counter!(
                    "icn_rpc_coop_access_denied_total",
                    "caller_coop" => caller_coop.clone(),
                    "requested_coop" => coop_id.to_string()
                )
                .increment(1);
                Err(RpcErrorCode::CoopAccessDenied)
            }
            None => {
                // No coop_id in token - log security event
                tracing::warn!(
                    caller = %self.caller_did,
                    requested_coop = coop_id,
                    "Coop access denied: token missing coop_id claim"
                );
                // Increment security metric
                metrics::counter!(
                    "icn_rpc_coop_auth_required_total",
                    "requested_coop" => coop_id.to_string()
                )
                .increment(1);
                Err(RpcErrorCode::AuthenticationRequired)
            }
        }
    }

    /// Verify the caller has access to the specified cooperative (if coop_id is provided).
    ///
    /// This is a convenience wrapper for `require_coop` that only enforces isolation
    /// when a coop_id is actually provided. Use this for operations that can be
    /// either global or coop-scoped.
    ///
    /// Returns Ok(()) if:
    /// - No coop_id was provided (allows global access)
    /// - The coop_id matches the caller's token coop_id
    ///
    /// Returns an error if coop_id is provided but doesn't match.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Ledger operations can be global or coop-scoped
    /// if let Err(e) = ctx.require_coop_if_provided(params.coop_id.as_deref()) {
    ///     return e.to_default_response(id);
    /// }
    /// ```
    pub fn require_coop_if_provided(&self, coop_id: Option<&str>) -> Result<(), RpcErrorCode> {
        match coop_id {
            Some(id) => self.require_coop(id),
            None => Ok(()), // No coop_id specified, allow global access
        }
    }

    /// Get the cooperative ID from the context, if any.
    pub fn coop_id(&self) -> Option<&str> {
        self.coop_id.as_deref()
    }

    /// Check if the context has a specific scope.
    pub fn has_scope(&self, scope: &str) -> bool {
        // Wildcard scope grants everything
        if self.scopes.contains(&"*".to_string()) {
            return true;
        }

        // Check exact match
        if self.scopes.contains(&scope.to_string()) {
            return true;
        }

        // Check namespace wildcard (e.g., "compute:*" grants "compute:submit")
        let parts: Vec<&str> = scope.split(':').collect();
        if parts.len() == 2 {
            let wildcard = format!("{}:*", parts[0]);
            if self.scopes.contains(&wildcard) {
                return true;
            }
        }

        false
    }
}

/// Helper to build an error response from an RpcErrorCode.
///
/// This provides a consistent way to convert context validation errors
/// into RPC responses with appropriate error codes and messages.
///
/// Primarily intended for coop-related errors from `RpcContext::require_coop()`:
/// - `CoopAccessDenied` - Cross-coop isolation violation
/// - `AuthenticationRequired` - Token missing coop_id claim
/// - `CoopNotFound` - Invalid cooperative reference
///
/// Other error codes fall back to their default messages via `default_message()`.
impl RpcErrorCode {
    /// Convert to an RPC error response with a context-appropriate message.
    ///
    /// For coop-related errors, provides user-friendly messages explaining
    /// the access control failure. For other errors, uses the default message.
    pub fn to_context_response(&self, id: u64) -> RpcResponse {
        let message = match self {
            RpcErrorCode::CoopAccessDenied => {
                "Access denied: you do not have permission to access this cooperative".to_string()
            }
            RpcErrorCode::AuthenticationRequired => {
                "Authentication required: token must include coop_id claim".to_string()
            }
            RpcErrorCode::CoopNotFound => "Cooperative not found".to_string(),
            other => other.default_message().to_string(),
        };
        RpcResponse::error(id, self.code(), message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        // Generate a valid test keypair
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_require_coop_success() {
        let ctx = RpcContext::new(test_did(), Some("coop-123".to_string()), vec![]);
        assert!(ctx.require_coop("coop-123").is_ok());
    }

    #[test]
    fn test_require_coop_wrong_coop() {
        let ctx = RpcContext::new(test_did(), Some("coop-123".to_string()), vec![]);
        let result = ctx.require_coop("coop-456");
        assert!(matches!(result, Err(RpcErrorCode::CoopAccessDenied)));
    }

    #[test]
    fn test_require_coop_no_coop() {
        let ctx = RpcContext::new(test_did(), None, vec![]);
        let result = ctx.require_coop("coop-123");
        assert!(matches!(result, Err(RpcErrorCode::AuthenticationRequired)));
    }

    #[test]
    fn test_require_coop_if_provided_none() {
        let ctx = RpcContext::new(test_did(), Some("coop-123".to_string()), vec![]);
        // No coop_id provided - should allow access
        assert!(ctx.require_coop_if_provided(None).is_ok());
    }

    #[test]
    fn test_require_coop_if_provided_match() {
        let ctx = RpcContext::new(test_did(), Some("coop-123".to_string()), vec![]);
        // Matching coop_id - should allow access
        assert!(ctx.require_coop_if_provided(Some("coop-123")).is_ok());
    }

    #[test]
    fn test_require_coop_if_provided_mismatch() {
        let ctx = RpcContext::new(test_did(), Some("coop-123".to_string()), vec![]);
        // Non-matching coop_id - should deny access
        let result = ctx.require_coop_if_provided(Some("coop-456"));
        assert!(matches!(result, Err(RpcErrorCode::CoopAccessDenied)));
    }

    #[test]
    fn test_has_scope() {
        let ctx = RpcContext::new(
            test_did(),
            None,
            vec!["ledger:read".to_string(), "compute:*".to_string()],
        );

        assert!(ctx.has_scope("ledger:read"));
        assert!(!ctx.has_scope("ledger:write"));
        assert!(ctx.has_scope("compute:submit"));
        assert!(ctx.has_scope("compute:status"));
    }

    #[test]
    fn test_wildcard_scope() {
        let ctx = RpcContext::new(test_did(), None, vec!["*".to_string()]);
        assert!(ctx.has_scope("anything"));
        assert!(ctx.has_scope("ledger:write"));
    }
}
