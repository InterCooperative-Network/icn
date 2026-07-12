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

/// Whether a space-separated `raw` scope string grants `required`.
///
/// Matching rule: some whitespace-split entry is either an exact match or a
/// sub-scope of `required` (e.g. `governance:write:admin` grants
/// `governance:write`). This is the single source of truth for scope matching;
/// both [`check_scope`] (which enforces) and [`classify_scope_admission`]
/// (which only observes) use it, so an observation can never disagree with the
/// gate it describes.
fn scope_str_grants(raw: &str, required: &str) -> bool {
    raw.split_whitespace().any(|s| {
        // Exact match, or a colon-delimited sub-scope (e.g. `governance:write:admin`
        // grants `governance:write`) — never a bare prefix like `governance:writer`.
        // `strip_prefix` avoids allocating a `format!("{required}:")` per token.
        s.strip_prefix(required)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(':'))
    })
}

/// Centralized scope checking. Split and normalize once, not in every handler.
///
/// A missing scope claim and a whitespace-only (tokenless) scope string are both
/// treated as "no scope present": a whitespace-only string carries no usable
/// scope, so it takes the same rejection path as an absent claim. This keeps the
/// gate aligned with [`classify_scope_admission`], which classifies both `None`
/// and whitespace-only as [`ScopeAdmission::RejectedMissing`]. Only the error
/// *message* changes for the whitespace-only edge; the `ApiError` variant, HTTP
/// status, and allow/deny outcome are unchanged (both were already `Forbidden`).
fn check_scope<C: ClaimsLike>(claims: &C, required: &str) -> Result<(), ApiError> {
    let raw = claims.raw_scope().unwrap_or_default();
    if raw.split_whitespace().next().is_none() {
        return Err(ApiError::Forbidden(format!(
            "scope '{required}' required but no scope present"
        )));
    }

    if scope_str_grants(raw, required) {
        Ok(())
    } else {
        Err(ApiError::Forbidden(format!(
            "required scope '{required}' not in granted scopes"
        )))
    }
}

/// How a class-first, accepted-also scope gate was satisfied — or why it would
/// be rejected.
///
/// This is **observability only**. It classifies the *same* scope decision the
/// gate helpers ([`require_any_scope`] / [`require_any_scope_matched`]) already
/// make; it never authorizes, rejects, or alters a request. It is the stable
/// in-code vocabulary a future privacy-safe observability slice can count
/// without capturing token contents, subjects, or resource identity. See
/// `docs/design/governance-broad-fallback-observability.md` §3.
///
/// A "class-first accepted-also" gate lists a narrowed class scope first and a
/// broad compatibility scope as a fallback (e.g.
/// `["governance:charter:write", "governance:write"]`). The classification
/// distinguishes admission via the class scope from admission via the broad
/// fallback, so migration off the fallback can eventually be measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeAdmission {
    /// Accepted by the required class scope; the broad fallback was not needed.
    Class,
    /// Accepted only by the broad fallback scope.
    Fallback,
    /// Both the class scope and the broad fallback are present; the class scope
    /// is the preferred/classified match.
    ClassPreferred,
    /// Rejected: a *different* class scope (from the sibling set) is present,
    /// but neither the required class scope nor the broad fallback.
    RejectedSibling,
    /// Rejected: only unrelated scopes are present.
    RejectedUnrelated,
    /// Rejected: no usable scope is present at all.
    RejectedMissing,
}

impl ScopeAdmission {
    /// Whether this classification corresponds to an accepted request.
    ///
    /// The accepted variants ([`Class`](Self::Class),
    /// [`Fallback`](Self::Fallback), [`ClassPreferred`](Self::ClassPreferred))
    /// match exactly the requests the gate helpers accept; the `Rejected*`
    /// variants match those they reject. Provided so an observer can assert its
    /// classification agrees with the gate outcome without re-deriving it.
    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Class | Self::Fallback | Self::ClassPreferred)
    }
}

/// Classify how a class-first, accepted-also scope gate was satisfied, from the
/// request's granted scope string alone.
///
/// Observability only — this makes no authorization decision and must never be
/// wired to change one. It applies the same whitespace/sub-scope rule the gate
/// uses ([`scope_str_grants`]), so the classification cannot disagree with the
/// gate it describes.
///
/// - `granted_scope`: the request's space-separated scope string
///   ([`ClaimsLike::raw_scope`]), or `None` when no scope claim is present.
/// - `class_scope`: the narrowed class scope this surface prefers
///   (e.g. `"governance:charter:write"`).
/// - `broad_scope`: the broad compatibility fallback (e.g. `"governance:write"`).
/// - `sibling_scopes`: the other class scopes in the same family, used only to
///   tell [`RejectedSibling`](ScopeAdmission::RejectedSibling) from
///   [`RejectedUnrelated`](ScopeAdmission::RejectedUnrelated). Passed as a
///   parameter (not hard-coded) so this crate stays domain-agnostic.
///
/// Privacy: only scope strings are inspected. No subject, DID, domain, entity,
/// resource identifier, or payload is read, returned, or retained.
pub fn classify_scope_admission(
    granted_scope: Option<&str>,
    class_scope: &str,
    broad_scope: &str,
    sibling_scopes: &[&str],
) -> ScopeAdmission {
    let Some(raw) = granted_scope else {
        return ScopeAdmission::RejectedMissing;
    };

    let has_class = scope_str_grants(raw, class_scope);
    let has_broad = scope_str_grants(raw, broad_scope);

    match (has_class, has_broad) {
        (true, true) => ScopeAdmission::ClassPreferred,
        (true, false) => ScopeAdmission::Class,
        (false, true) => ScopeAdmission::Fallback,
        (false, false) => {
            if raw.split_whitespace().next().is_none() {
                ScopeAdmission::RejectedMissing
            } else if sibling_scopes
                .iter()
                .any(|sibling| scope_str_grants(raw, sibling))
            {
                ScopeAdmission::RejectedSibling
            } else {
                ScopeAdmission::RejectedUnrelated
            }
        }
    }
}

/// [`classify_scope_admission`] applied to a request's already-validated claims.
///
/// Convenience wrapper that reads [`ClaimsLike::raw_scope`]; identical semantics
/// and the same privacy guarantee — only the scope string is inspected.
pub fn classify_scope_admission_for_claims<C: ClaimsLike>(
    claims: &C,
    class_scope: &str,
    broad_scope: &str,
    sibling_scopes: &[&str],
) -> ScopeAdmission {
    classify_scope_admission(claims.raw_scope(), class_scope, broad_scope, sibling_scopes)
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

    /// Least-privilege boundary for a narrow read + single-class-write session
    /// token (the shape the appliance demo hands the browser:
    /// `governance:read governance:meeting:write`). It must authorize the
    /// member-shell's read and action-item-completion routes and be denied
    /// everything broader — entity mutation, cooperative admin, and the broad or
    /// sibling governance write classes. This guards the credential split against
    /// silently regranting broad authority to a browser session.
    #[test]
    fn narrow_read_plus_meeting_write_cannot_reach_admin_or_broad_routes() {
        let req = req_with_scope(Some("governance:read governance:meeting:write"));

        // MUST satisfy the member-shell live routes:
        //   standing / action-cards / pending-publish / completion-receipt (read)
        assert!(require_scope::<BasicClaims>(&req, "governance:read").is_ok());
        //   action-item completion PUT (accepts the narrow meeting class)
        assert!(
            require_any_scope::<BasicClaims>(&req, &["governance:meeting:write", BROAD]).is_ok()
        );

        // MUST be denied broader authority (representative admin / broad-mutation
        // routes): create entity, create domain, cooperative admin — and the
        // narrow meeting class MUST NOT be silently escalated to the broad write.
        for denied in ["entity:write", "coop:admin", "coop:write", BROAD, CHARTER] {
            assert!(
                matches!(
                    require_scope::<BasicClaims>(&req, denied).unwrap_err(),
                    ApiError::Forbidden(_)
                ),
                "narrow session token must NOT grant {denied}"
            );
        }
        // create_domain / assign_role use require_any_scope([class, broad]) — the
        // narrow session has neither, so it is rejected there too.
        assert!(matches!(
            require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap_err(),
            ApiError::Forbidden(_)
        ));
    }

    /// Least-privilege boundary for the completion-only browser credential shape
    /// (`governance:read governance:action-item:complete`, #2400). It must satisfy
    /// the action-item completion candidate list and the read surface, and be
    /// denied everything broader — including the `governance:meeting:write` class
    /// that authorizes *creating* items/meetings, the broad/sibling governance
    /// writes, entity mutation, and cooperative admin. Guards the credential split
    /// against a browser session silently regaining create/admin authority.
    #[test]
    fn narrow_read_plus_action_item_complete_cannot_create_or_reach_broad_routes() {
        const COMPLETE: &str = "governance:action-item:complete";
        let req = req_with_scope(Some("governance:read governance:action-item:complete"));

        // MUST satisfy the member-shell read surface and the completion PUT
        // (the completion candidate list is narrowest-first).
        assert!(require_scope::<BasicClaims>(&req, "governance:read").is_ok());
        assert!(
            require_any_scope::<BasicClaims>(&req, &[COMPLETE, MEETING, BROAD]).is_ok(),
            "completion candidate list must accept the completion-only scope"
        );
        // Evidence must record the narrow completion scope, not a broad fallback.
        let (_c, matched) =
            require_any_scope_matched::<BasicClaims>(&req, &[COMPLETE, MEETING, BROAD]).unwrap();
        assert_eq!(
            matched, COMPLETE,
            "evidence must record the completion-only scope"
        );

        // MUST NOT reach creation / broader authority. The completion scope is a
        // sibling of meeting:write and governance:write — it neither implies nor
        // is implied by them (no accidental sub-scope escalation).
        for denied in [
            MEETING,
            BROAD,
            CHARTER,
            "entity:write",
            "coop:admin",
            "coop:write",
        ] {
            assert!(
                matches!(
                    require_scope::<BasicClaims>(&req, denied).unwrap_err(),
                    ApiError::Forbidden(_)
                ),
                "completion-only session must NOT grant {denied}"
            );
        }
        // The create / non-completion-transition gate (meeting:write | write)
        // must reject the completion-only session outright.
        assert!(matches!(
            require_any_scope::<BasicClaims>(&req, &[MEETING, BROAD]).unwrap_err(),
            ApiError::Forbidden(_)
        ));
    }

    /// Least-privilege boundary for the rehearsal ORGANIZER credential shape
    /// (`governance:read governance:pending-publish:review
    /// governance:pending-publish:confirm`, #1726/#2386). It must satisfy the
    /// rehearsal review and confirm candidate lists and the read surface, and
    /// be denied every broader capability — creation via `meeting:write`, the
    /// completion-only member capability, broad/sibling governance writes,
    /// entity mutation, and cooperative admin. Also pins that review and
    /// confirm are mutually non-implying siblings: a review-only credential
    /// must fail the confirm gate and vice versa.
    #[test]
    fn narrow_organizer_rehearsal_shape_cannot_reach_broad_or_member_routes() {
        const REVIEW: &str = "governance:pending-publish:review";
        const CONFIRM: &str = "governance:pending-publish:confirm";
        const COMPLETE: &str = "governance:action-item:complete";
        let req = req_with_scope(Some(
            "governance:read governance:pending-publish:review governance:pending-publish:confirm",
        ));

        assert!(require_scope::<BasicClaims>(&req, "governance:read").is_ok());
        let (_c, matched) =
            require_any_scope_matched::<BasicClaims>(&req, &[REVIEW, BROAD]).unwrap();
        assert_eq!(
            matched, REVIEW,
            "evidence must record the review capability"
        );
        let (_c, matched) =
            require_any_scope_matched::<BasicClaims>(&req, &[CONFIRM, BROAD]).unwrap();
        assert_eq!(
            matched, CONFIRM,
            "evidence must record the confirm capability"
        );

        // The organizer shape must NOT hold member-completion, creation,
        // fixture setup (designation/binding), or any broader authority.
        // Siblings never imply each other.
        for denied in [
            COMPLETE,
            MEETING,
            BROAD,
            CHARTER,
            "governance:rehearsal:setup",
            "entity:write",
            "coop:admin",
            "coop:write",
        ] {
            assert!(
                matches!(
                    require_scope::<BasicClaims>(&req, denied).unwrap_err(),
                    ApiError::Forbidden(_)
                ),
                "organizer rehearsal session must NOT grant {denied}"
            );
        }

        // Review-only cannot confirm; confirm-only cannot review.
        let review_only = req_with_scope(Some("governance:read governance:pending-publish:review"));
        assert!(matches!(
            require_any_scope::<BasicClaims>(&review_only, &[CONFIRM, BROAD]).unwrap_err(),
            ApiError::Forbidden(_)
        ));
        let confirm_only =
            req_with_scope(Some("governance:read governance:pending-publish:confirm"));
        assert!(matches!(
            require_any_scope::<BasicClaims>(&confirm_only, &[REVIEW, BROAD]).unwrap_err(),
            ApiError::Forbidden(_)
        ));
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

    // --- ScopeAdmission classification (observability only) ---

    const MEETING: &str = "governance:meeting:write";
    // A representative sibling-class set. The real set is
    // `icn_rpc::auth::scopes::GOVERNANCE_CLASS_WRITE`, supplied by callers;
    // `icn-http-kit` stays domain-agnostic and takes it as a parameter.
    const SIBLINGS: &[&str] = &[CHARTER, MEETING, "governance:proposal:write"];

    #[test]
    fn classify_class_only_is_class() {
        assert_eq!(
            classify_scope_admission(Some(CHARTER), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::Class
        );
    }

    #[test]
    fn classify_broad_only_is_fallback() {
        assert_eq!(
            classify_scope_admission(Some(BROAD), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::Fallback
        );
    }

    #[test]
    fn classify_class_and_broad_is_class_preferred() {
        let both = format!("{CHARTER} {BROAD}");
        assert_eq!(
            classify_scope_admission(Some(&both), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::ClassPreferred
        );
    }

    #[test]
    fn classify_sibling_class_scope_is_rejected_sibling() {
        // A different governance class scope, but neither the required class nor
        // the broad fallback.
        assert_eq!(
            classify_scope_admission(Some(MEETING), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::RejectedSibling
        );
    }

    #[test]
    fn classify_unrelated_scope_is_rejected_unrelated() {
        assert_eq!(
            classify_scope_admission(Some("ledger:write"), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::RejectedUnrelated
        );
    }

    #[test]
    fn classify_missing_scope_is_rejected_missing() {
        assert_eq!(
            classify_scope_admission(None, CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::RejectedMissing
        );
        // An empty / whitespace-only scope string is "no usable scope" too.
        assert_eq!(
            classify_scope_admission(Some("   "), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::RejectedMissing
        );
    }

    #[test]
    fn classify_honors_sub_scope_match_for_broad_fallback() {
        // `governance:write:admin` satisfies the broad `governance:write` fallback.
        assert_eq!(
            classify_scope_admission(Some("governance:write:admin"), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::Fallback
        );
    }

    #[test]
    fn scope_prefix_without_colon_does_not_grant() {
        // A bare prefix like `governance:writer` must NOT satisfy `governance:write`
        // — only an exact match or a colon-delimited sub-scope grants. Guards the
        // `strip_prefix`-based matcher against false prefix positives, for both the
        // gate (`require_scope`) and the classifier.
        let req = req_with_scope(Some("governance:writer"));
        assert!(
            require_scope::<BasicClaims>(&req, BROAD).is_err(),
            "bare prefix `governance:writer` must not grant `governance:write`"
        );
        assert_eq!(
            classify_scope_admission(Some("governance:writer"), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::RejectedUnrelated
        );
    }

    #[test]
    fn whitespace_only_scope_takes_the_no_scope_present_path() {
        // Post-#2344 alignment: the gate treats a whitespace-only (tokenless) scope
        // as "no scope present" — the same rejection class as a missing claim —
        // matching the classifier's `RejectedMissing`. The `ApiError` variant and
        // HTTP status are unchanged (`Forbidden` either way); only the message text
        // changes, so this is not an authorization-behavior change.
        let req = req_with_scope(Some("   "));
        match require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).unwrap_err() {
            ApiError::Forbidden(msg) => assert!(
                msg.contains("no scope present"),
                "whitespace-only scope should use the no-scope-present message, got: {msg}"
            ),
            other => panic!("expected Forbidden, got {other:?}"),
        }
        // Gate and classifier now agree this is the "missing" case.
        assert_eq!(
            classify_scope_admission(Some("   "), CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::RejectedMissing
        );
    }

    #[test]
    fn classify_for_claims_matches_pure_classifier() {
        let broad_claims = get_claims::<BasicClaims>(&req_with_scope(Some(BROAD))).unwrap();
        assert_eq!(
            classify_scope_admission_for_claims(&broad_claims, CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::Fallback
        );
        let no_claims = get_claims::<BasicClaims>(&req_with_scope(None)).unwrap();
        assert_eq!(
            classify_scope_admission_for_claims(&no_claims, CHARTER, BROAD, SIBLINGS),
            ScopeAdmission::RejectedMissing
        );
    }

    #[test]
    fn classification_accept_set_agrees_with_require_any_scope() {
        // The observer must never disagree with the gate it describes: accepted
        // classifications correspond exactly to gate acceptance, rejected ones to
        // gate rejection, for the same `[class, broad]` candidate list.
        let both = format!("{CHARTER} {BROAD}");
        let cases: [(Option<&str>, bool); 7] = [
            (Some(CHARTER), true),
            (Some(BROAD), true),
            (Some(both.as_str()), true),
            (Some(MEETING), false),
            (Some("ledger:write"), false),
            (Some("   "), false),
            (None, false),
        ];
        for (scope, gate_accepts) in cases {
            let req = req_with_scope(scope);
            let gate_ok = require_any_scope::<BasicClaims>(&req, &[CHARTER, BROAD]).is_ok();
            assert_eq!(
                gate_ok, gate_accepts,
                "gate acceptance mismatch for {scope:?}"
            );
            let admission = classify_scope_admission(scope, CHARTER, BROAD, SIBLINGS);
            assert_eq!(
                admission.is_accepted(),
                gate_accepts,
                "classifier/gate disagree for {scope:?}: {admission:?}"
            );
        }
    }
}
