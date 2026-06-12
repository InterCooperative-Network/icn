//! OpenAPI documentation for ICN Gateway API
//!
//! This module provides comprehensive API documentation using utoipa.

use utoipa::OpenApi;

// Re-export models for schema registration
use crate::models::{
    AccountDeltaResponse, AccountStateResponse, AddMemberRequest, CastVoteRequest,
    ChallengeRequest, ChallengeResponse, ComponentHealth, CreateCoopRequest, CreateDomainRequest,
    CreateInviteRequest, CreateProposalRequest, CreateSessionRequest, CreateSessionResponse,
    DetailedComponentHealth, DetailedHealthResponse, HealthResponse, HealthStatus, InviteInfo,
    InviteListResponse, InviteResponse, JoinRequest, JoinResponse, OpenProposalRequest,
    PaginationInfo, ProposalPayloadRequest, RecordTransferRequest, SessionQrData,
    SessionStatusResponse, TokenResponse, TransactionHistoryEntry, TransactionHistoryResponse,
    UpdateRoleRequest, UpdateSettingsRequest, VerifyRequest, VoteChoiceResponse,
};

// API module types
use crate::api::charter::{
    CharterDetailResponse, CharterSummaryResponse, CreateCharterRequest, FounderDetailResponse,
    FounderResponse, FoundersResponse, SignCharterRequest, TimelineEvent, TimelineResponse,
    UpdateCharterStatusRequest,
};
use crate::api::devices::{
    ApiListDevicesResponse, ApiRegisterDeviceRequest, ApiRegisterDeviceResponse,
    ApiRevokeDeviceRequest,
};
use crate::api::federation::{ProposeAdoptionRequest, ProposeAdoptionResponse};
use crate::api::governance_dashboard::{
    ActivityEvent, AmendmentsBreakdown, AppealsBreakdown, GovernanceDashboard,
};
use crate::api::membership::{
    ApplyMembershipRequest, BanMemberRequest, CapabilityRequest, MemberResponse,
    MembershipActionRequest, RevokeMembershipRequest, RoleRequest,
};
use crate::api::notifications::{
    ListNotificationsResponse, MarkReadResponse, NotificationCountResponse,
    RegisterDeviceRequest as NotifRegisterDeviceRequest, RegisterDeviceResponse,
};
use crate::api::steward::{
    BondOperationRequest, ExtendTermRequest, RegisterStewardRequest, StewardDetailResponse,
    StewardSummaryResponse, UpdateStatusRequest,
};
use crate::identity_mgr::DeviceInfo;
use crate::notification_store::{InAppNotification, Platform};

// Member-shell surface (handlers live in the governance app crate, mounted
// by this gateway under /v1/gov — see configure_governance in server.rs).
// Receipt types come via the app crate's re-export, not icn_governance
// directly: the meaning-firewall ratchets pin this crate's direct
// icn_governance references (see icn-core/src/meaning_firewall.rs).
use icn_governance_actor::{ActionItemCompletionReceipt, ActionItemTransition};

use icn_governance_actor::http::models::{
    ActionCard, ActionCardActionKind, ActionCardRiskLevel, ActionCardScope, ActionCardSourceKind,
    ActionCardsResponse, StandingDomainMembership, StandingResponse, StandingRoleAssignment,
};

/// OpenAPI documentation for ICN Gateway
#[derive(OpenApi)]
#[openapi(
    servers((
        url = "/v1",
        description = "Gateway API base path (all routes are mounted under /v1)"
    )),
    paths(
        crate::api::federation::propose_clearing_adoption,
        // Member-shell surface (docs/dev/openapi-member-surface-gaps.md):
        // the routes a member-facing shell needs for the core loop
        // standing → action card → discharge → receipt.
        icn_governance_actor::http::handlers::get_my_standing,
        icn_governance_actor::http::handlers::get_my_action_cards,
        icn_governance_actor::http::handlers::get_action_item_completion_receipt,
    ),
    modifiers(&SecurityAddon),
    info(
        title = "ICN Gateway API",
        version = "0.1.0",
        description = "REST and WebSocket API for the Intercooperative Network\n\n\
        ## Overview\n\n\
        The ICN Gateway provides a RESTful API for cooperative applications to interact with \
        the Intercooperative Network. Features include:\n\n\
        - **Identity**: DID-based identity management with multi-device support\n\
        - **Cooperatives**: Create and manage cooperatives with member management\n\
        - **Ledger**: Mutual credit accounting with double-entry bookkeeping\n\
        - **Trust**: Social trust graph with transitive trust computation\n\
        - **Governance**: Democratic proposals and voting\n\
        - **Compute**: Distributed task execution with trust-gated access\n\
        - **Federation**: Cross-federation message routing\n\
        - **Notifications**: Real-time push notifications\n\n\
        ## Authentication\n\n\
        Most endpoints require JWT authentication. Obtain a token via `/v1/auth/challenge` \
        and `/v1/auth/verify` endpoints. Include the token in the `Authorization: Bearer <token>` header.\n\n\
        ## WebSocket\n\n\
        Real-time events are available via WebSocket at `/v1/websocket`. After connecting, \
        send authentication message with your JWT token.",
        contact(
            name = "ICN Project",
            url = "https://github.com/InterCooperative-Network/icn"
        ),
        license(
            name = "MIT OR Apache-2.0",
            url = "https://opensource.org/licenses/MIT"
        )
    ),
    components(
        schemas(
            // Core models
            HealthResponse, ComponentHealth, DetailedHealthResponse, DetailedComponentHealth, HealthStatus,
            // Auth
            ChallengeRequest, ChallengeResponse, VerifyRequest, TokenResponse,
            // Coops
            CreateCoopRequest, AddMemberRequest, UpdateRoleRequest, UpdateSettingsRequest,
            // Ledger
            RecordTransferRequest, AccountStateResponse, AccountDeltaResponse,
            TransactionHistoryEntry, TransactionHistoryResponse, PaginationInfo,
            // Governance
            CreateDomainRequest, CreateProposalRequest, ProposalPayloadRequest,
            OpenProposalRequest, CastVoteRequest, VoteChoiceResponse,
            // Sessions
            CreateSessionRequest, CreateSessionResponse, SessionQrData, SessionStatusResponse,
            // Invites
            CreateInviteRequest, InviteResponse, InviteInfo, InviteListResponse,
            JoinRequest, JoinResponse,
            // Membership
            ApplyMembershipRequest, MembershipActionRequest, CapabilityRequest, RoleRequest,
            BanMemberRequest, RevokeMembershipRequest, MemberResponse,
            // Charter
            CharterSummaryResponse, CharterDetailResponse, FounderResponse, CreateCharterRequest,
            SignCharterRequest, UpdateCharterStatusRequest, FounderDetailResponse, FoundersResponse,
            TimelineEvent, TimelineResponse,
            // Steward
            StewardSummaryResponse, StewardDetailResponse, RegisterStewardRequest, UpdateStatusRequest,
            ExtendTermRequest, BondOperationRequest,
            // Governance Dashboard
            GovernanceDashboard, AmendmentsBreakdown, AppealsBreakdown, ActivityEvent,
            // Devices
            ApiRegisterDeviceRequest, ApiRevokeDeviceRequest, ApiRegisterDeviceResponse, ApiListDevicesResponse,
            // Notifications
            NotifRegisterDeviceRequest, RegisterDeviceResponse,
            ListNotificationsResponse, NotificationCountResponse, MarkReadResponse,
            // Federation
            ProposeAdoptionRequest, ProposeAdoptionResponse,
            // Member-shell surface (standing, action cards, completion receipt)
            StandingResponse, StandingDomainMembership, StandingRoleAssignment,
            ActionCardsResponse, ActionCard, ActionCardSourceKind, ActionCardActionKind,
            ActionCardScope, ActionCardRiskLevel,
            ActionItemCompletionReceipt, ActionItemTransition,
            // Shared types
            DeviceInfo, Platform, InAppNotification,
        )
    ),
    tags(
        (name = "health", description = "Health check and readiness endpoints"),
        (name = "auth", description = "Authentication and authorization"),
        (name = "identity", description = "DID-based identity management"),
        (name = "cooperatives", description = "Cooperative creation and membership"),
        (name = "ledger", description = "Mutual credit ledger operations"),
        (name = "trust", description = "Trust graph management"),
        (name = "governance", description = "Democratic governance and voting"),
        (name = "compute", description = "Distributed compute task execution"),
        (name = "federation", description = "Cross-federation routing"),
        (name = "notifications", description = "Push notifications and alerts"),
        (name = "websocket", description = "Real-time event streaming"),
        (name = "membership", description = "Commons membership management"),
        (name = "charter", description = "Charter creation and management"),
        (name = "steward", description = "Steward management and attestations"),
        (name = "devices", description = "Multi-device management"),
    )
)]
pub struct ApiDoc;

/// Registers the `bearer_auth` JWT security scheme that annotated paths
/// reference via `security(("bearer_auth" = []))`. Without this component
/// those references point at an undeclared scheme.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The member-shell surface must stay in the served OpenAPI document
    /// (`/api-docs/openapi.json` is built from this same `ApiDoc` object —
    /// see server.rs). Guards the annotation wiring across the gateway and
    /// the governance app crate.
    #[test]
    fn member_shell_surface_is_documented() {
        let doc = ApiDoc::openapi();
        let paths = &doc.paths.paths;
        for expected in [
            "/gov/me/standing",
            "/gov/me/action-cards",
            "/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt",
        ] {
            assert!(
                paths.contains_key(expected),
                "missing documented path: {expected}"
            );
        }

        let schemas = doc
            .components
            .as_ref()
            .expect("components present")
            .schemas
            .clone();
        for expected in [
            "StandingResponse",
            "ActionCardsResponse",
            "ActionCard",
            "ActionItemCompletionReceipt",
        ] {
            assert!(
                schemas.contains_key(expected),
                "missing registered schema: {expected}"
            );
        }

        let security_schemes = &doc
            .components
            .as_ref()
            .expect("components present")
            .security_schemes;
        assert!(
            security_schemes.contains_key("bearer_auth"),
            "bearer_auth security scheme must be registered"
        );

        // Paths are documented relative to the /v1 mount (Actix scope in
        // server.rs); the server base keeps generated clients on the right
        // URLs.
        let servers = doc.servers.as_ref().expect("servers present");
        assert!(
            servers.iter().any(|s| s.url == "/v1"),
            "server base url /v1 must be declared"
        );
    }
}
