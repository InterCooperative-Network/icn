//! Trusted token-issuance authority boundary (#2080 PR2, RFC-0018 migration).
//!
//! #2080 PR1 (PR #2111) added the *rail*: an optional, non-enforcing
//! `entity_id`/`entity_type` claim plus the [`AuthManager::issue_entity_token`] mint
//! seam. It deliberately left open the *source of trust* that decides whether a
//! token may carry that typed entity context. This module draws that boundary
//! explicitly — without making any new authority claim.
//!
//! ## Two responsibilities, kept apart
//!
//! - **Minting** ([`AuthManager`]): assemble and sign a JWT. Mechanical.
//! - **Authority** ([`TokenAuthoritySource`]): prove that a subject DID may receive
//!   a token binding a cooperative [`EntityId`] with a set of scopes. Institutional.
//!
//! A token is not the source of legitimacy. The token *carries* authority context;
//! a [`TokenAuthoritySource`] is what proves that context is legitimate. The core
//! invariants this seam encodes: **mapping is not authority; a DID is not
//! membership; a token is not voting power.**
//!
//! ## Fail-closed by construction
//!
//! The only source shipped in this PR is [`DenyUntilWired`], which denies *every*
//! request with [`IssuanceDenialReason::NoTrustedSourceWired`]. It never reads
//! claims and never derives authority from a DID, a `coop_id`, an [`EntityId`], or
//! scopes alone. Denial is modeled as a *decision value*, not an `Err` — mirroring
//! the in-crate observe-mode precedent ([`crate::authority`]'s
//! `EntityAccessObservation`) — so a future caller can never mistake an error for an
//! allow.
//!
//! ## Explicitly NOT in this PR
//!
//! No production caller is wired to any source: [`AuthManager::issue_entity_token`]
//! with `Some(..)` remains a raw mint primitive, and no issuance path (`/auth/verify`,
//! invites, sessions, SDIS enrollment, bootstrap) consults a [`TokenAuthoritySource`]
//! yet. A real *positive* source — likely backed by `icn-entity` membership, so that
//! trusted issuance converges on the same source `require_entity_access` already
//! enforces against — lands in a follow-up, once the gateway has a wired
//! `coop_id → EntityId` resolver and the bootstrap/governance path can populate or
//! bridge membership safely. See RFC-0018 and ADR-0035 ("claim vs lookup — we choose
//! lookup; fail-closed only on a wired resolver").

use icn_entity::EntityId;
use icn_identity::Did;

use crate::auth::AuthManager;
use crate::error::{GatewayError, Result};

/// The outcome of asking a [`TokenAuthoritySource`] whether a token may be issued
/// with typed entity context.
///
/// A denial is a first-class *decision*, not an error: an unwired or unhealthy
/// source must `Deny`, never surface an `Err` a caller might treat as success.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "an issuance authority decision must be acted on (allow vs deny)"]
pub enum IssuanceAuthorityDecision {
    /// The source positively confirms issuance, citing the basis of the authority.
    Allow {
        /// Which authority surface proved the issuance legitimate.
        basis: IssuanceAuthorityBasis,
    },
    /// The source refuses issuance, citing a machine-readable reason.
    Deny {
        /// Why issuance was refused.
        reason: IssuanceDenialReason,
    },
}

impl IssuanceAuthorityDecision {
    /// `true` only for [`IssuanceAuthorityDecision::Allow`].
    #[must_use]
    pub fn is_allow(&self) -> bool {
        matches!(self, IssuanceAuthorityDecision::Allow { .. })
    }
}

/// The authority surface that justified an [`IssuanceAuthorityDecision::Allow`].
///
/// This is the forward-looking vocabulary of *where* trusted issuance may eventually
/// read authority from. In this PR only [`IssuanceAuthorityBasis::TestOnly`] is ever
/// constructed (by test doubles); the production source ([`DenyUntilWired`]) allows
/// nothing. The remaining variants name the substrates a later PR will wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuanceAuthorityBasis {
    /// Proven by `icn-entity` membership (the enforcement-aligned source).
    Membership,
    /// Proven by `icn-governance` standing / role assignment.
    Standing,
    /// Proven by a privileged first-admin / institution bootstrap path.
    Bootstrap,
    /// Proven by a constitutional-memory authority grant / mandate.
    AuthorityGrant,
    /// A test-only positive decision. Never produced in production.
    TestOnly,
}

/// Why a [`TokenAuthoritySource`] refused to authorize typed-entity issuance.
///
/// Stable, machine-readable labels (see [`IssuanceDenialReason::label`]) so the
/// reason can flow into logs/metrics without leaking internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuanceDenialReason {
    /// No trusted authority source is wired — the fail-closed default for this PR.
    NoTrustedSourceWired,
    /// The subject is not a member of the target cooperative entity.
    MissingMembership,
    /// The subject lacks the standing/role required for this issuance.
    MissingStanding,
    /// The subject's membership/standing lacks the capability the scopes require.
    MissingCapability,
    /// The requested `entity_id` does not match the proven authority.
    EntityMismatch,
    /// The requested `coop_id` does not match the proven authority.
    CoopMismatch,
    /// One or more requested scopes are not permitted by the proven authority.
    UnsupportedScope,
}

impl IssuanceDenialReason {
    /// Stable, lowercase label for logs/metrics.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            IssuanceDenialReason::NoTrustedSourceWired => "no_trusted_source_wired",
            IssuanceDenialReason::MissingMembership => "missing_membership",
            IssuanceDenialReason::MissingStanding => "missing_standing",
            IssuanceDenialReason::MissingCapability => "missing_capability",
            IssuanceDenialReason::EntityMismatch => "entity_mismatch",
            IssuanceDenialReason::CoopMismatch => "coop_mismatch",
            IssuanceDenialReason::UnsupportedScope => "unsupported_scope",
        }
    }
}

/// The trust boundary a trusted issuance path consults before minting a token that
/// carries typed entity context.
///
/// Implementations answer: *may `subject` (a DID) receive a token binding `coop_id`
/// to `entity_id` with these `scopes`?* The method is `async` because real sources
/// resolve authority from an entity/standing store (the same async shape as
/// [`crate::authority`]'s `require_entity_access`); the fail-closed default resolves
/// without I/O.
///
/// Returning the decision **by value** (not `Result`) is deliberate: a deny — or an
/// unhealthy/unreachable source mapped to a deny — must never be confusable with a
/// successful mint.
#[async_trait::async_trait]
pub trait TokenAuthoritySource: Send + Sync {
    /// Decide whether `subject` may receive a token binding `coop_id`/`entity_id`
    /// with `scopes`. Never mints; only decides.
    async fn can_issue_entity_token(
        &self,
        subject: &Did,
        coop_id: &str,
        entity_id: &EntityId,
        scopes: &[String],
    ) -> IssuanceAuthorityDecision;
}

/// The fail-closed default [`TokenAuthoritySource`]: denies all issuance.
///
/// Until a real authority source is wired (a later PR), this is the only source the
/// gateway ships. It always returns [`IssuanceDenialReason::NoTrustedSourceWired`]
/// and reads none of its inputs — no DID, `coop_id`, `entity_id`, or scope can, alone
/// or together, produce an allow.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyUntilWired;

#[async_trait::async_trait]
impl TokenAuthoritySource for DenyUntilWired {
    async fn can_issue_entity_token(
        &self,
        _subject: &Did,
        _coop_id: &str,
        _entity_id: &EntityId,
        _scopes: &[String],
    ) -> IssuanceAuthorityDecision {
        IssuanceAuthorityDecision::Deny {
            reason: IssuanceDenialReason::NoTrustedSourceWired,
        }
    }
}

impl AuthManager {
    /// Issue an entity-context token **only** if `authority` returns
    /// [`IssuanceAuthorityDecision::Allow`].
    ///
    /// This is the checked counterpart to the raw [`AuthManager::issue_entity_token`]
    /// mint seam: trusted callers should route entity-context issuance through here
    /// once real authority sources are wired, so that the typed `entity_id`/
    /// `entity_type` claims are minted only behind a positive authority decision. On
    /// [`IssuanceAuthorityDecision::Deny`] it mints nothing and returns
    /// [`GatewayError::AuthorizationFailed`] (403).
    ///
    /// In this PR the only production source is [`DenyUntilWired`], so this helper is
    /// fail-closed for every input; it is not yet wired into any issuance route.
    pub async fn issue_entity_token_checked<A>(
        &self,
        authority: &A,
        did: &Did,
        coop_id: &str,
        entity_id: &EntityId,
        scopes: Vec<String>,
    ) -> Result<String>
    where
        A: TokenAuthoritySource + ?Sized,
    {
        match authority
            .can_issue_entity_token(did, coop_id, entity_id, &scopes)
            .await
        {
            IssuanceAuthorityDecision::Allow { .. } => {
                self.issue_entity_token(did, coop_id, Some(entity_id.clone()), scopes)
            }
            IssuanceAuthorityDecision::Deny { reason } => Err(GatewayError::AuthorizationFailed(
                format!("token issuance denied: {}", reason.label()),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;

    fn coop_entity() -> EntityId {
        EntityId::cooperative("food-coop").expect("valid cooperative slug")
    }

    /// Test-only positive source. Proves the decision type can express `Allow` and
    /// that the checked helper mints behind a positive decision. Never compiled into
    /// production (`#[cfg(test)]`).
    struct AllowForTests;

    #[async_trait::async_trait]
    impl TokenAuthoritySource for AllowForTests {
        async fn can_issue_entity_token(
            &self,
            _subject: &Did,
            _coop_id: &str,
            _entity_id: &EntityId,
            _scopes: &[String],
        ) -> IssuanceAuthorityDecision {
            IssuanceAuthorityDecision::Allow {
                basis: IssuanceAuthorityBasis::TestOnly,
            }
        }
    }

    /// `DenyUntilWired` denies issuance and cites the fail-closed reason.
    #[tokio::test]
    async fn deny_until_wired_denies_all_issuance() {
        let bundle = IdentityBundle::generate().unwrap();
        let decision = DenyUntilWired
            .can_issue_entity_token(
                bundle.did(),
                "food-coop",
                &coop_entity(),
                &["ledger:read".to_string()],
            )
            .await;
        assert_eq!(
            decision,
            IssuanceAuthorityDecision::Deny {
                reason: IssuanceDenialReason::NoTrustedSourceWired
            }
        );
        assert!(!decision.is_allow());
    }

    /// A subject DID, by itself, is not authority: a valid DID still yields a deny.
    #[tokio::test]
    async fn did_alone_is_not_authority() {
        let bundle = IdentityBundle::generate().unwrap();
        let decision = DenyUntilWired
            .can_issue_entity_token(bundle.did(), "food-coop", &coop_entity(), &[])
            .await;
        assert!(!decision.is_allow());
    }

    /// A matching `coop_id` string is not authority. Even when the request's
    /// `coop_id` equals the entity's slug — the comparison `require_coop_access`
    /// would allow — issuance is denied.
    #[tokio::test]
    async fn coop_id_alone_is_not_authority() {
        let bundle = IdentityBundle::generate().unwrap();
        let decision = DenyUntilWired
            .can_issue_entity_token(
                bundle.did(),
                "food-coop",
                &coop_entity(),
                &["coop:read".to_string()],
            )
            .await;
        assert!(!decision.is_allow());
    }

    /// A well-formed `EntityId` (a `coop_id → EntityId` mapping result) is not
    /// authority: mapping is not authority.
    #[tokio::test]
    async fn entity_id_mapping_alone_is_not_authority() {
        let bundle = IdentityBundle::generate().unwrap();
        let decision = DenyUntilWired
            .can_issue_entity_token(bundle.did(), "food-coop", &coop_entity(), &[])
            .await;
        assert!(!decision.is_allow());
    }

    /// High-privilege scopes are not authority: a token is not voting power.
    #[tokio::test]
    async fn scopes_alone_are_not_authority() {
        let bundle = IdentityBundle::generate().unwrap();
        let decision = DenyUntilWired
            .can_issue_entity_token(
                bundle.did(),
                "food-coop",
                &coop_entity(),
                &["governance:write".to_string(), "treasury:write".to_string()],
            )
            .await;
        assert!(!decision.is_allow());
    }

    /// Checked issuance fails closed against `DenyUntilWired`: it returns an error
    /// and mints no token.
    #[tokio::test]
    async fn checked_issuance_fails_closed_with_deny_until_wired() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();
        let result = auth
            .issue_entity_token_checked(
                &DenyUntilWired,
                bundle.did(),
                "food-coop",
                &coop_entity(),
                vec!["ledger:read".to_string()],
            )
            .await;
        assert!(result.is_err(), "DenyUntilWired must mint no token");
        match result {
            Err(GatewayError::AuthorizationFailed(msg)) => {
                assert!(msg.contains("no_trusted_source_wired"), "got: {msg}");
            }
            other => panic!("expected AuthorizationFailed, got: {other:?}"),
        }
    }

    /// Checked issuance mints **only** through a positive decision: a test source
    /// that allows lets the helper mint a token carrying the typed entity context.
    #[tokio::test]
    async fn checked_issuance_allows_only_through_positive_decision() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();
        let token = auth
            .issue_entity_token_checked(
                &AllowForTests,
                bundle.did(),
                "food-coop",
                &coop_entity(),
                vec!["ledger:read".to_string()],
            )
            .await
            .expect("positive decision permits issuance");
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(
            claims.entity_id.as_deref(),
            Some("entity:icn:cooperative:food-coop")
        );
        assert_eq!(claims.entity_type.as_deref(), Some("cooperative"));
        assert_eq!(claims.coop_id, "food-coop");
    }
}
