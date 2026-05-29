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
/// rejection mirrors [`require_scope`] applied to the first (preferred) entry —
/// distinguishing "no scope present" from "not in granted scopes". Called with
/// an empty `required_scopes` is a programmer error and returns
/// `ApiError::Internal`.
pub fn require_any_scope<C: ClaimsLike>(
    req: &HttpRequest,
    required_scopes: &[&str],
) -> Result<C, ApiError> {
    let Some((&preferred, fallbacks)) = required_scopes.split_first() else {
        return Err(ApiError::Internal(
            "require_any_scope called with no candidate scopes".to_string(),
        ));
    };
    let claims = get_claims::<C>(req).ok_or(ApiError::Unauthenticated)?;

    // Accept on the first matching fallback candidate.
    if fallbacks
        .iter()
        .any(|scope| check_scope(&claims, scope).is_ok())
    {
        return Ok(claims);
    }
    // No fallback matched — defer to the preferred scope's own check so the
    // rejection mirrors `require_scope` exactly (preserving the "no scope
    // present" vs "not in granted scopes" distinction) and names the
    // preferred (narrowed) scope.
    check_scope(&claims, preferred)?;
    Ok(claims)
}

/// Like [`require_any_scope`], but also returns **which** candidate scope
/// authorized the request.
///
/// The returned scope is the first entry of `required_scopes` (in listed
/// preference order) that the claims actually grant. Callers that record the
/// accepted scope as evidence — e.g. a receipt's `capability_scope_presented`
/// — must use this rather than assuming the preferred class scope: during a
/// capability-decomposition migration a request may be accepted via a legacy
/// broad scope, and the evidence must say so truthfully. List the narrowed
/// class scope first so it is preferred when both are present.
///
/// Rejection behavior mirrors [`require_any_scope`] / [`require_scope`]: an
/// empty `required_scopes` is a programmer error (`ApiError::Internal`), and a
/// caller with none of the scopes gets the canonical rejection keyed on the
/// preferred (first) scope.
pub fn require_any_scope_matched<C: ClaimsLike>(
    req: &HttpRequest,
    required_scopes: &[&str],
) -> Result<(C, String), ApiError> {
    let Some((&preferred, _)) = required_scopes.split_first() else {
        return Err(ApiError::Internal(
            "require_any_scope_matched called with no candidate scopes".to_string(),
        ));
    };
    let claims = get_claims::<C>(req).ok_or(ApiError::Unauthenticated)?;
    for &scope in required_scopes {
        if check_scope(&claims, scope).is_ok() {
            return Ok((claims, scope.to_string()));
        }
    }
    // No candidate matched. Defer to the preferred scope's own check, which
    // returns the canonical Forbidden/Unauthenticated error. `preferred` is
    // among the candidates just tried, so this always returns Err and `?`
    // propagates; the trailing Err is unreachable but keeps the function
    // total without an unwrap/expect/panic.
    check_scope(&claims, preferred)?;
    Err(ApiError::Internal(
        "require_any_scope_matched: candidate rejected but no error produced".to_string(),
    ))
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
        // Mirrors `require_scope`: distinguishes "no scope present" from the
        // ordinary "not in granted scopes" rejection, naming the preferred scope.
        match err {
            ApiError::Forbidden(msg) => {
                assert!(
                    msg.contains("no scope present") && msg.contains(CHARTER),
                    "expected a 'no scope present' rejection naming {CHARTER}, got: {msg}"
                );
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn require_any_scope_empty_candidates_is_internal_error() {
        // Empty candidate list is a programmer error, not an auth rejection.
        let req = req_with_scope(Some(CHARTER));
        let err = require_any_scope::<BasicClaims>(&req, &[]).unwrap_err();
        assert!(matches!(err, ApiError::Internal(_)));
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

    #[test]
    fn require_any_scope_matched_returns_narrow_class_scope() {
        let req = req_with_scope(Some(CHARTER));
        let (_claims, matched) =
            require_any_scope_matched::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap();
        assert_eq!(matched, CHARTER);
    }

    #[test]
    fn require_any_scope_matched_returns_legacy_broad_scope_when_only_that_is_present() {
        // Evidence integrity: a request accepted via the legacy broad scope must
        // report `governance:write`, not the preferred narrowed class scope.
        let req = req_with_scope(Some(BROAD));
        let (_claims, matched) =
            require_any_scope_matched::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap();
        assert_eq!(matched, BROAD);
    }

    #[test]
    fn require_any_scope_matched_prefers_first_listed_when_both_present() {
        // Both scopes granted → the first-listed (narrowed) candidate wins.
        let req = req_with_scope(Some(&format!("{CHARTER} {BROAD}")));
        let (_claims, matched) =
            require_any_scope_matched::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap();
        assert_eq!(matched, CHARTER);
    }

    #[test]
    fn require_any_scope_matched_rejects_unrelated_scope() {
        let req = req_with_scope(Some("ledger:write"));
        let err = require_any_scope_matched::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn require_any_scope_matched_empty_candidates_is_internal_error() {
        let req = req_with_scope(Some(CHARTER));
        let err = require_any_scope_matched::<BasicClaims>(&req, &[]).unwrap_err();
        assert!(matches!(err, ApiError::Internal(_)));
    }
}
