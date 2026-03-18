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
