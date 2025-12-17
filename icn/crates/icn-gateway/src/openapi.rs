//! OpenAPI documentation for ICN Gateway API
//!
//! This module provides comprehensive API documentation using utoipa.

use utoipa::OpenApi;

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
    )
)]
pub struct ApiDoc;
