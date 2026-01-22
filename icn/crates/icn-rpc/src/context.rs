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
    /// # Example
    ///
    /// ```ignore
    /// let ctx: &RpcContext = ...;
    /// if let Err(e) = ctx.require_coop("coop-123") {
    ///     return e.to_response(id);
    /// }
    /// ```
    pub fn require_coop(&self, coop_id: &str) -> Result<(), RpcErrorCode> {
        match &self.coop_id {
            Some(c) if c == coop_id => Ok(()),
            Some(_) => Err(RpcErrorCode::CoopAccessDenied),
            None => Err(RpcErrorCode::AuthenticationRequired),
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
impl RpcErrorCode {
    /// Convert to an RPC error response with a context-appropriate message.
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
