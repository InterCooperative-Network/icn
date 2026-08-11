//! Invite API endpoints
//!
//! RESTful API for managing cooperative invitations.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::commons_mgr::CommonsManager;
use crate::error::Result;
use crate::invite::InviteManager;
use crate::middleware::{get_claims, require_coop_access, require_scope};
use crate::models::{
    CreateInviteRequest, InviteInfo, InviteListResponse, InviteResponse, JoinRequest, JoinResponse,
};
use crate::session_authority::SessionAuthority;
use crate::validation;
use icn_identity::Did;
use icn_obs::metrics::gateway;

// ============================================================================
// Invite Endpoints
// ============================================================================

/// POST /invites - Create a new invite code
#[post("")]
pub async fn create_invite(
    http_req: HttpRequest,
    invite_mgr: web::Data<Arc<InviteManager>>,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    req: web::Json<CreateInviteRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let creator_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate inputs
    validation::validate_coop_id(&req.coop_id)?;
    // Flat coop-namespace guard (NOT entity/community/federation hierarchy — see
    // #2061): bind the body-supplied coop_id to the caller's token namespace.
    // `coop:admin` is requestable per namespace, so without this a coop A admin could
    // mint invites (which grant membership) for coop B. Prevent a cross-namespace write.
    require_coop_access(&http_req, &req.coop_id)?;
    validation::validate_role(&req.role)?;

    // Default to 7 days if not specified
    let expires_in = req.expires_in_seconds.unwrap_or(7 * 24 * 3600);

    // Validate expiration (max 30 days)
    if expires_in > 30 * 24 * 3600 {
        return Err(crate::error::GatewayError::BadRequest(
            "Expiration cannot exceed 30 days".to_string(),
        ));
    }

    // Create invite
    let invite = invite_mgr
        .create_invite(
            req.coop_id.clone(),
            req.role.clone(),
            creator_did,
            expires_in,
        )
        .await
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to create invite: {e}"))
        })?;

    // Get coop name from charter
    let coop_name = match commons_mgr.get_charter_by_domain(&req.coop_id).await {
        Ok(Some(charter)) => charter.name,
        _ => format!("Coop {}", req.coop_id), // Fallback if charter not found
    };

    // Construct invite URL — base resolved by `invite_base_url()` below.
    //
    // This link is a *member-facing UI* destination, not a gateway API origin — see
    // `invite_base_url` for why the two configurations are deliberately separate (#2569).
    let invite_url = format!("{}/join?code={}", invite_base_url(), invite.code);

    // Record metrics
    gateway::invites_created(&req.coop_id);

    Ok(HttpResponse::Created().json(InviteResponse {
        code: invite.code,
        coop_id: invite.coop_id,
        coop_name,
        role: invite.role,
        expires_at: invite.expires_at,
        invite_url,
    }))
}

/// Operator-set base for the human-facing `{base}/join?code=…` invite link.
pub(crate) const INVITE_BASE_URL_ENV: &str = "ICN_INVITE_BASE_URL";

/// Local-development default: `deploy/docker-compose.yml` serves the pilot UI on port 3000.
/// It is honest only where a member UI actually runs there, which is why the doc below is
/// explicit that no shipped profile serves this route.
const INVITE_BASE_URL_DEFAULT: &str = "http://localhost:3000";

/// Base URL for the human-facing `{base}/join?code=…` invite link.
///
/// # Why this is not `GATEWAY_BASE_URL`
///
/// One variable was carrying two incompatible authorities (#2569):
///
/// - **A — gateway API origin.** Where a *device* sends authenticated traffic. Owned by
///   [`crate::advertised_origin`], fails closed when absent, and must stay operator-controlled.
/// - **B — member UI origin.** Where a *person* opens a join link. That is this value.
///
/// Reading A for B coupled them, and wiring A into a deployment then corrupted invite links two
/// different ways. Both were reproduced: with `GATEWAY_BASE_URL=` present-but-empty (what
/// Compose exports for an unconfigured `${VAR:-}`) `std::env::var` returns `Ok("")`, so an
/// `unwrap_or_else` that only fires on `Err` yielded a hostless `/join?code=…`; and with it
/// set to a real API origin the link became `{gateway API}/join?code=…`, a route the gateway
/// does not serve. Setting the QR origin must not be able to move a UI link, so this reads its
/// own variable and never consults `GATEWAY_BASE_URL`.
///
/// # Honest status of this link
///
/// **No composition in this repository serves `GET /join?code=…`.** The only invite redemption
/// path is `POST /v1/invites/join`, which carries the code in the request body; `web/pilot-ui`
/// collects it from a typed form and posts there. Nothing reads `code` from a URL query.
/// So this string is advisory: an operator who runs a member UI implementing such a route
/// points `ICN_INVITE_BASE_URL` at it, and otherwise the link addresses nothing.
///
/// The default is retained for local development only, and is **not** claimed to be reachable
/// in any deployment profile — on the devnet profile port 3000 is Grafana, and no member UI is
/// served at all. Empty and whitespace-only are treated as absent so a misconfigured value
/// degrades to that default rather than to a hostless path.
fn invite_base_url() -> String {
    resolve_invite_base(configured_invite_base())
}

/// Pure resolution, so the policy is testable without touching any ambient state.
fn resolve_invite_base(configured: Option<String>) -> String {
    configured
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| INVITE_BASE_URL_DEFAULT.to_string())
}

#[cfg(not(test))]
fn configured_invite_base() -> Option<String> {
    std::env::var(INVITE_BASE_URL_ENV).ok()
}

#[cfg(test)]
fn configured_invite_base() -> Option<String> {
    test_env::configured()
}

/// Per-test UI-origin configuration, for the same reason [`crate::advertised_origin::test_env`]
/// exists: every `#[cfg(test)]` module lands in one lib test binary, so a process-global read
/// here would let two tests configuring different origins race. libtest gives each test its own
/// thread, so a thread-local makes them order-independent by construction rather than by a lock
/// every future author must remember.
#[cfg(test)]
pub(crate) mod test_env {
    use std::cell::RefCell;

    thread_local! {
        static CONFIGURED: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    pub(super) fn configured() -> Option<String> {
        CONFIGURED.with(|slot| slot.borrow().clone())
    }

    /// Pins the member-UI origin for this thread, restoring the prior value on drop.
    pub(crate) struct UiOriginGuard {
        prior: Option<String>,
    }

    impl UiOriginGuard {
        pub(crate) fn acquire(value: Option<&str>) -> Self {
            let prior = CONFIGURED.with(|slot| slot.replace(value.map(str::to_owned)));
            Self { prior }
        }
    }

    impl Drop for UiOriginGuard {
        fn drop(&mut self) {
            let prior = self.prior.take();
            CONFIGURED.with(|slot| *slot.borrow_mut() = prior);
        }
    }
}

/// GET /invites - List all invites for a cooperative
#[get("")]
pub async fn list_invites(
    http_req: HttpRequest,
    invite_mgr: web::Data<Arc<InviteManager>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:read")?;

    // Get coop_id from query params
    let coop_id = query.get("coop_id").ok_or_else(|| {
        crate::error::GatewayError::BadRequest("Missing coop_id parameter".to_string())
    })?;

    validation::validate_coop_id(coop_id)?;
    // Invite codes are membership-granting bearer secrets; binding the queried
    // coop_id to the caller's token namespace stops a coop A reader from listing (and
    // then redeeming) coop B's invites. Flat coop-namespace guard, not entity
    // hierarchy (#2061). Prevent cross-namespace exposure.
    require_coop_access(&http_req, coop_id)?;

    // List invites
    let invites = invite_mgr.list_invites(coop_id).await.map_err(|e| {
        crate::error::GatewayError::InternalError(format!("Failed to list invites: {e}"))
    })?;

    // Convert to response format
    let invite_infos: Vec<InviteInfo> = invites
        .into_iter()
        .map(|i| InviteInfo {
            code: i.code,
            role: i.role,
            created_by: i.created_by.to_string(),
            created_at: i.created_at,
            expires_at: i.expires_at,
            used: i.used,
        })
        .collect();

    Ok(HttpResponse::Ok().json(InviteListResponse {
        invites: invite_infos,
    }))
}

/// POST /invites/join - Join a cooperative via invite code
#[post("/join")]
pub async fn join_via_invite(
    invite_mgr: web::Data<Arc<InviteManager>>,
    authority: web::Data<Arc<SessionAuthority>>,
    req: web::Json<JoinRequest>,
) -> Result<HttpResponse> {
    // Validate invite code
    let invite = invite_mgr
        .validate_invite(&req.invite_code)
        .await
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid invite: {e}")))?;

    // Validate the provided DID
    let did: Did = req
        .did
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Mark invite as used
    invite_mgr
        .mark_used(&req.invite_code, did.clone())
        .await
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to mark invite as used: {e}"))
        })?;

    // Generate capability token
    let scopes = vec![
        "coop:read".to_string(),
        "coop:write".to_string(),
        "ledger:read".to_string(),
        "ledger:transact".to_string(),
    ];

    let token = authority
        .auth_manager()
        .issue_token(&did, &invite.coop_id, scopes)
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to generate token: {e}"))
        })?;

    // Record metrics
    gateway::invites_used(&invite.coop_id);

    Ok(HttpResponse::Ok().json(JoinResponse {
        did: did.to_string(),
        token,
        token_expires_in: authority.lifetime().ttl().as_secs(),
        coop_id: invite.coop_id,
        role: invite.role,
        private_key: String::new(), // Client generates their own keypair
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthManager, TokenClaims};
    use crate::commons_mgr::CommonsManager;
    use crate::invite::InviteManager;
    use actix_web::http::StatusCode;
    use actix_web::{test as actix_test, App, HttpMessage};

    /// Absent, empty and whitespace-only all mean "unconfigured", and never a hostless link.
    ///
    /// The empty case is the one that bit: Compose exports an unconfigured `${VAR:-}` as a
    /// present-but-empty variable, so `std::env::var` returns `Ok("")` and an `unwrap_or_else`
    /// that only fires on `Err` produced a relative `/join?code=…` with no host at all.
    #[test]
    fn unconfigured_invite_base_degrades_to_the_default_not_to_a_hostless_link() {
        for configured in [None, Some(""), Some("   "), Some("\t\n")] {
            let base = resolve_invite_base(configured.map(str::to_string));
            assert_eq!(
                base, INVITE_BASE_URL_DEFAULT,
                "{configured:?} must resolve to the default, got {base:?}"
            );
            assert!(
                base.starts_with("http"),
                "invite link must carry a host, got {base:?}"
            );
        }
    }

    /// The operator-facing variable name is a documented contract (`deploy/icnd.env.example`,
    /// `docs/guides/onboarding-runbook.md`), so renaming it should break a test, not just docs.
    #[test]
    fn invite_base_url_env_name_is_the_documented_one() {
        assert_eq!(INVITE_BASE_URL_ENV, "ICN_INVITE_BASE_URL");
    }

    /// A configured member-UI origin is used verbatim, with surrounding whitespace trimmed.
    #[test]
    fn configured_invite_base_is_used_and_trimmed() {
        assert_eq!(
            resolve_invite_base(Some("https://members.example.coop".to_string())),
            "https://members.example.coop"
        );
        assert_eq!(
            resolve_invite_base(Some("  https://members.example.coop  ".to_string())),
            "https://members.example.coop"
        );
    }

    /// **The independence proof.** Setting the gateway API origin alone must not move the
    /// invite link — driven through the real handler, not the helper.
    ///
    /// This is the half of the review finding that treating empty-as-absent did not fix: an
    /// operator following the QR-origin instructions set `GATEWAY_BASE_URL` to a real API
    /// origin, and invite links silently became `{gateway API}/join?code=…`, a route the
    /// gateway does not serve. The two authorities are now separate variables, so the QR
    /// origin has no reachable path to this value.
    ///
    /// The gateway API origin is pinned through the authority that actually owns it, and the
    /// member-UI origin is left unconfigured. Neither test touches the process environment, so
    /// the two independence cases below cannot race each other.
    #[actix_web::test]
    async fn setting_the_gateway_api_origin_cannot_move_the_invite_link() {
        let _api = crate::advertised_origin::test_env::EnvGuard::acquire(Some(
            "http://gateway.example:8080",
        ));
        let _ui = test_env::UiOriginGuard::acquire(None);

        let invite_url = created_invite_url().await;

        assert!(
            !invite_url.contains("gateway.example"),
            "the gateway API origin leaked into the invite link: {invite_url}"
        );
        assert!(
            invite_url.starts_with(INVITE_BASE_URL_DEFAULT),
            "expected the member-UI default, got {invite_url}"
        );
    }

    /// With both configured to different values, each authority stays in its own lane.
    #[actix_web::test]
    async fn api_origin_and_invite_origin_are_independent_when_both_are_set() {
        let _api = crate::advertised_origin::test_env::EnvGuard::acquire(Some(
            "http://gateway.example:8080",
        ));
        let _ui = test_env::UiOriginGuard::acquire(Some("https://members.example.coop"));

        let invite_url = created_invite_url().await;

        assert!(
            invite_url.starts_with("https://members.example.coop/join?code="),
            "invite link must use the member-UI origin only, got {invite_url}"
        );
        assert!(
            !invite_url.contains("gateway.example"),
            "the gateway API origin leaked into the invite link: {invite_url}"
        );
    }

    /// Drives the real `create_invite` handler and returns the `invite_url` it produced.
    async fn created_invite_url() -> String {
        let invite_mgr = Arc::new(InviteManager::new());
        let commons_mgr = Arc::new(CommonsManager::new());
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(invite_mgr))
                .app_data(web::Data::new(commons_mgr))
                .service(web::scope("/invites").service(create_invite)),
        )
        .await;

        let sub = icn_identity::KeyPair::generate().unwrap().did().to_string();
        let req = actix_test::TestRequest::post()
            .uri("/invites")
            .set_json(serde_json::json!({ "coop_id": "coopA", "role": "member" }))
            .to_request();
        req.extensions_mut().insert(admin_claims("coopA", &sub));

        let body: InviteResponse = actix_test::call_and_read_body_json(&app, req).await;
        body.invite_url
    }

    fn admin_claims(coop: &str, sub: &str) -> TokenClaims {
        TokenClaims {
            entity_id: None,
            entity_type: None,
            sub: sub.to_string(),
            iat: 1_000_000_000,
            exp: 9_999_999_999,
            coop_id: coop.to_string(),
            scopes: vec!["coop:admin".to_string(), "coop:read".to_string()],
            jti: None,
        }
    }

    /// `coop:admin` is requestable per coop, so `create_invite` binds the body
    /// coop_id to the token's coop. A same-coop admin mints an invite (201) that is
    /// then listed; a cross-coop admin is rejected (403) and creates nothing under
    /// the target coop. Invites grant membership, so this is a privilege-escalation
    /// boundary. (coop_ids avoid ':' to satisfy validate_coop_id.)
    #[actix_web::test]
    async fn create_invite_rejects_cross_coop_write() {
        async fn run(
            token_coop: &str,
            body_coop: &str,
            sub: &str,
        ) -> (StatusCode, Arc<InviteManager>) {
            let invite_mgr = Arc::new(InviteManager::new());
            let commons_mgr = Arc::new(CommonsManager::new());
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(invite_mgr.clone()))
                    .app_data(web::Data::new(commons_mgr))
                    .service(web::scope("/invites").service(create_invite)),
            )
            .await;
            let req = actix_test::TestRequest::post()
                .uri("/invites")
                .set_json(serde_json::json!({ "coop_id": body_coop, "role": "member" }))
                .to_request();
            req.extensions_mut().insert(admin_claims(token_coop, sub));
            let status = actix_test::call_service(&app, req).await.status();
            (status, invite_mgr)
        }

        // The success path parses claims.sub into a Did, so use a real DID subject.
        let sub = icn_identity::KeyPair::generate().unwrap().did().to_string();

        let (same, same_mgr) = run("coopA", "coopA", &sub).await;
        assert_eq!(same, StatusCode::CREATED);
        assert_eq!(
            same_mgr.list_invites("coopA").await.unwrap().len(),
            1,
            "same-coop admin should create exactly one invite"
        );

        let (cross, cross_mgr) = run("coopA", "coopB", &sub).await;
        assert_eq!(cross, StatusCode::FORBIDDEN);
        assert!(
            cross_mgr.list_invites("coopB").await.unwrap().is_empty(),
            "cross-coop reject must not create an invite under coopB"
        );
    }

    /// Invite codes are membership-granting bearer secrets; `list_invites` binds the
    /// queried coop_id to the token's coop so a coopA reader cannot enumerate (and
    /// then redeem) coopB's invites. Same-coop read succeeds (200); cross-coop is 403.
    #[actix_web::test]
    async fn list_invites_rejects_cross_coop_read() {
        async fn run(token_coop: &str, query_coop: &str) -> StatusCode {
            let invite_mgr = Arc::new(InviteManager::new());
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(invite_mgr))
                    .service(web::scope("/invites").service(list_invites)),
            )
            .await;
            let req = actix_test::TestRequest::get()
                .uri(&format!("/invites?coop_id={query_coop}"))
                .to_request();
            req.extensions_mut()
                .insert(admin_claims(token_coop, "did:icn:reader"));
            actix_test::call_service(&app, req).await.status()
        }

        assert_eq!(run("coopA", "coopA").await, StatusCode::OK);
        assert_eq!(run("coopA", "coopB").await, StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn join_reports_the_installed_non_default_token_lifetime() {
        let invite_mgr = Arc::new(InviteManager::new());
        let creator = icn_identity::KeyPair::generate().unwrap().did().clone();
        let invite = invite_mgr
            .create_invite("coopA".to_string(), "member".to_string(), creator, 3600)
            .await
            .unwrap();
        let auth = Arc::new(
            AuthManager::new(b"invite-lifetime-test-secret-32b".to_vec())
                .with_token_ttl(std::time::Duration::from_secs(2 * 3600)),
        );
        let authority = Arc::new(SessionAuthority::evaluator(auth.clone()));
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(invite_mgr))
                .app_data(web::Data::new(authority))
                .service(join_via_invite),
        )
        .await;
        let member = icn_identity::KeyPair::generate().unwrap().did().clone();
        let request = actix_test::TestRequest::post()
            .uri("/join")
            .set_json(JoinRequest {
                invite_code: invite.code,
                did: member.to_string(),
            })
            .to_request();

        let response = actix_test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let response: JoinResponse = actix_test::read_body_json(response).await;

        assert_eq!(response.token_expires_in, 2 * 3600);
        let claims = auth.verify_token(&response.token).unwrap();
        assert_eq!(claims.exp - claims.iat, response.token_expires_in);
    }
}
