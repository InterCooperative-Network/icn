//! OpenAPI documentation for ICN Gateway API
//!
//! This module provides comprehensive API documentation using utoipa.

use utoipa::OpenApi;

// Re-export models for schema registration
use crate::models::{
    AccountDeltaResponse, AddMemberRequest, BalanceResponse, CastVoteRequest, ChallengeRequest,
    ChallengeResponse, ComponentHealth, CreateCoopRequest, CreateDomainRequest,
    CreateInviteRequest, CreatePaymentRequest, CreateProposalRequest, CreateSessionRequest,
    CreateSessionResponse, DetailedComponentHealth, DetailedHealthResponse, HealthResponse,
    HealthStatus, InviteInfo, InviteListResponse, InviteResponse, JoinRequest, JoinResponse,
    OpenProposalRequest, PaginationInfo, ProposalPayloadRequest, SessionQrData,
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

/// OpenAPI documentation for ICN Gateway
#[derive(OpenApi)]
#[openapi(
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
            CreatePaymentRequest, BalanceResponse, AccountDeltaResponse,
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
