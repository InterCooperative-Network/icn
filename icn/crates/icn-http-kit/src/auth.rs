//! Generic JWT claims extraction for actix-web handlers.
//!
//! Provides a `ClaimsLike` trait so that app crates can define their own
//! claims shape (or use `BasicClaims`) without depending on gateway internals.
//! Scope parsing is centralized here to avoid N apps reinventing the rules.

use actix_web::{HttpMessage, HttpRequest};

use crate::error::ApiError;

/// Trait for types that carry authenticated identity + optional OAuth-style scope.
///
/// Implementors are inserted into request extensions by the JWT middleware and
/// extracted by `get_claims<C>` in handlers.
pub trait ClaimsLike: Clone + Send + Sync + 'static {
    /// The authenticated subject (DID or username).
    fn subject(&self) -> &str;

    /// Raw scope string (space-separated OAuth scopes), if present.
    fn raw_scope(&self) -> Option<&str>;
}

/// A minimal claims type — use as `type GovernanceClaims = BasicClaims` or
/// define your own that implements `ClaimsLike`.
#[derive(Clone, Debug)]
pub struct BasicClaims {
    pub sub: String,
    pub scope: Option<String>,
}

impl ClaimsLike for BasicClaims {
    fn subject(&self) -> &str {
        &self.sub
    }

    fn raw_scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }
}

/// Extract already-validated claims from the request extension map.
///
/// Returns `None` if the middleware did not insert claims (e.g. unauthenticated
/// endpoint, test stub that skips auth).
pub fn get_claims<C: ClaimsLike>(req: &HttpRequest) -> Option<C> {
    req.extensions().get::<C>().cloned()
}

/// Require that the request carries valid claims that include `required_scope`.
///
/// Scope matching: the request's raw scope string is split on whitespace; a
/// sub-scope match is also accepted (e.g. `"governance:write"` is satisfied by
/// `"governance:write:admin"`).
pub fn require_scope<C: ClaimsLike>(
    req: &HttpRequest,
    required_scope: &str,
) -> Result<C, ApiError> {
    let claims = get_claims::<C>(req).ok_or(ApiError::Unauthenticated)?;
    check_scope(&claims, required_scope)?;
    Ok(claims)
}

/// Require that the request carries valid claims satisfying **at least one** of
/// `required_scopes`.
///
/// This supports capability-decomposition migrations: a handler can require a
/// narrowed class scope (e.g. `"governance:charter:write"`) while still
/// accepting a legacy broad scope (e.g. `"governance:write"`) as an
/// accepted-also fallback until the broad scope is retired. Each candidate is
/// matched with the same rule as [`require_scope`] (exact or sub-scope).
///
/// Returns the claims on the first candidate that matches. If none match, the
/// `Forbidden` error names the first (preferred) entry in `required_scopes`.
pub fn require_any_scope<C: ClaimsLike>(
    req: &HttpRequest,
    required_scopes: &[&str],
) -> Result<C, ApiError> {
    let claims = get_claims::<C>(req).ok_or(ApiError::Unauthenticated)?;
    if required_scopes
        .iter()
        .any(|scope| check_scope(&claims, scope).is_ok())
    {
        return Ok(claims);
    }
    let preferred = required_scopes.first().copied().unwrap_or("");
    Err(ApiError::Forbidden(format!(
        "required scope '{preferred}' not in granted scopes"
    )))
}

/// Centralized scope checking. Split and normalize once, not in every handler.
fn check_scope<C: ClaimsLike>(claims: &C, required: &str) -> Result<(), ApiError> {
    let Some(raw) = claims.raw_scope() else {
        return Err(ApiError::Forbidden(format!(
            "scope '{required}' required but no scope present"
        )));
    };

    let granted = raw.split_whitespace().any(|s| {
        // Exact match or sub-scope (governance:write:admin satisfies governance:write)
        s == required || s.starts_with(&format!("{required}:"))
    });

    if granted {
        Ok(())
    } else {
        Err(ApiError::Forbidden(format!(
            "required scope '{required}' not in granted scopes"
        )))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use actix_web::test::TestRequest;

    const CHARTER: &str = "governance:charter:write";
    const BROAD: &str = "governance:write";

    fn req_with_scope(scope: Option<&str>) -> HttpRequest {
        let req = TestRequest::default().to_http_request();
        req.extensions_mut().insert(BasicClaims {
            sub: "did:icn:test".to_string(),
            scope: scope.map(str::to_string),
        });
        req
    }

    #[test]
    fn require_any_scope_accepts_class_scope() {
        let req = req_with_scope(Some(CHARTER));
        assert!(require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).is_ok());
    }

    #[test]
    fn require_any_scope_accepts_legacy_broad_scope() {
        // Accepted-also fallback: tokens minted before the migration still work.
        let req = req_with_scope(Some(BROAD));
        assert!(require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).is_ok());
    }

    #[test]
    fn require_any_scope_rejects_unrelated_scope() {
        let req = req_with_scope(Some("ledger:write"));
        let err = require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn require_any_scope_rejects_when_no_scope_present() {
        let req = req_with_scope(None);
        let err = require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn require_any_scope_unauthenticated_without_claims() {
        let req = TestRequest::default().to_http_request();
        let err = require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap_err();
        assert!(matches!(err, ApiError::Unauthenticated));
    }

    #[test]
    fn require_any_scope_honors_sub_scope_match() {
        // `governance:write:admin` satisfies the broad `governance:write` candidate.
        let req = req_with_scope(Some("governance:write:admin"));
        assert!(require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).is_ok());
    }
}
