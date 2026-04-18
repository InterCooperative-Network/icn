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

// Governance actor milestone/program models (for schema registration and doc stubs)
use icn_governance_actor::http::models::{
    CreateMilestoneRequest, MilestoneResponse, ProgramResponse, SetMilestoneGateRequest,
    UpdateMilestoneStatusRequest, UpdateProgramStatusRequest,
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

/// Non-generic documentation stubs for milestone endpoints.
///
/// The real handlers are generic over `GovernanceEventEmitter` which prevents
/// `#[utoipa::path]` from being referenced in `paths(...)` directly. These
/// stubs carry the path metadata; the types are resolved via schema registration.
mod milestone_paths {
    use icn_governance_actor::http::models::{
        CreateMilestoneRequest, MilestoneResponse, ProgramResponse, SetMilestoneGateRequest,
        UpdateMilestoneStatusRequest, UpdateProgramStatusRequest,
    };

    /// POST /gov/programs/{program_id}/milestones — create a milestone.
    #[utoipa::path(
        post,
        path = "/gov/programs/{program_id}/milestones",
        tag = "milestones",
        params(
            ("program_id" = String, Path, description = "Program ID")
        ),
        request_body = CreateMilestoneRequest,
        responses(
            (status = 201, body = MilestoneResponse, description = "Milestone created"),
            (status = 400, description = "Bad request — invalid gate expression or empty name"),
            (status = 401, description = "Unauthorized"),
            (status = 403, description = "Caller is not a member of the program's domain"),
            (status = 404, description = "Program not found"),
        ),
        security(("bearer_auth" = []))
    )]
    #[allow(dead_code)]
    pub(super) async fn create_milestone() {}

    /// GET /gov/milestones/{milestone_id} — get a specific milestone.
    #[utoipa::path(
        get,
        path = "/gov/milestones/{milestone_id}",
        tag = "milestones",
        params(
            ("milestone_id" = String, Path, description = "Milestone ID")
        ),
        responses(
            (status = 200, body = MilestoneResponse, description = "Milestone detail"),
            (status = 401, description = "Unauthorized"),
            (status = 404, description = "Milestone not found"),
        ),
        security(("bearer_auth" = []))
    )]
    #[allow(dead_code)]
    pub(super) async fn get_milestone() {}

    /// GET /gov/programs/{program_id}/milestones — list milestones for a program.
    #[utoipa::path(
        get,
        path = "/gov/programs/{program_id}/milestones",
        tag = "milestones",
        params(
            ("program_id" = String, Path, description = "Program ID")
        ),
        responses(
            (status = 200, body = Vec<MilestoneResponse>, description = "Ordered milestone list"),
            (status = 401, description = "Unauthorized"),
            (status = 404, description = "Program not found"),
        ),
        security(("bearer_auth" = []))
    )]
    #[allow(dead_code)]
    pub(super) async fn list_milestones_by_program() {}

    /// PATCH /gov/milestones/{milestone_id} — update milestone status.
    #[utoipa::path(
        patch,
        path = "/gov/milestones/{milestone_id}",
        tag = "milestones",
        params(
            ("milestone_id" = String, Path, description = "Milestone ID")
        ),
        request_body = UpdateMilestoneStatusRequest,
        responses(
            (status = 200, body = MilestoneResponse, description = "Milestone updated"),
            (status = 400, description = "Invalid status or gate blocked completion"),
            (status = 401, description = "Unauthorized"),
            (status = 403, description = "Caller is not a member of the program's domain"),
            (status = 404, description = "Milestone not found"),
        ),
        security(("bearer_auth" = []))
    )]
    #[allow(dead_code)]
    pub(super) async fn update_milestone_status() {}

    /// PATCH /gov/programs/{program_id}/status — advance or revert program lifecycle status.
    #[utoipa::path(
        patch,
        path = "/gov/programs/{program_id}/status",
        tag = "milestones",
        params(
            ("program_id" = String, Path, description = "Program ID")
        ),
        request_body = UpdateProgramStatusRequest,
        responses(
            (status = 200, body = ProgramResponse, description = "Program with updated status"),
            (status = 400, description = "Unknown status string"),
            (status = 401, description = "Unauthorized"),
            (status = 403, description = "Caller is not a member of the program's domain"),
            (status = 404, description = "Program not found"),
        ),
        security(("bearer_auth" = []))
    )]
    #[allow(dead_code)]
    pub(super) async fn update_program_status() {}

    /// PUT /gov/milestones/{milestone_id}/gate — set or clear the CCL completion gate.
    ///
    /// Accepts a JSON-encoded `icn_ccl::Expr` object to install a gate, or `null` /
    /// an absent field to remove any existing gate. The expression is validated
    /// immediately; a malformed expression is rejected with 400.
    #[utoipa::path(
        put,
        path = "/gov/milestones/{milestone_id}/gate",
        tag = "milestones",
        params(
            ("milestone_id" = String, Path, description = "Milestone ID")
        ),
        request_body = SetMilestoneGateRequest,
        responses(
            (status = 200, body = MilestoneResponse, description = "Gate installed or cleared"),
            (status = 400, description = "Invalid CCL gate expression"),
            (status = 401, description = "Unauthorized"),
            (status = 403, description = "Caller is not a member of the program's domain"),
            (status = 404, description = "Milestone not found"),
        ),
        security(("bearer_auth" = []))
    )]
    #[allow(dead_code)]
    pub(super) async fn set_milestone_gate() {}
}

/// OpenAPI documentation for ICN Gateway
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::federation::propose_clearing_adoption,
        milestone_paths::update_program_status,
        milestone_paths::create_milestone,
        milestone_paths::get_milestone,
        milestone_paths::list_milestones_by_program,
        milestone_paths::update_milestone_status,
        milestone_paths::set_milestone_gate,
    ),
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
            // Shared types
            DeviceInfo, Platform, InAppNotification,
            // Milestones and program status
            CreateMilestoneRequest, UpdateMilestoneStatusRequest,
            SetMilestoneGateRequest, MilestoneResponse,
            UpdateProgramStatusRequest, ProgramResponse,
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
        (name = "milestones", description = "Program milestone tracking with CCL completion gates"),
    )
)]
pub struct ApiDoc;
