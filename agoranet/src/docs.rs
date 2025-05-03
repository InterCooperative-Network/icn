use utoipa::{
    openapi::security::{SecurityRequirement, SecurityScheme, SecuritySchemeType},
    Modify, OpenApi,
};

use crate::types::{
    thread::{ThreadResponse, Thread},
    message::{Message, MessageResponse, CreateMessageRequest}, 
    reaction::{ReactionRequest, ReactionResponse},
    credential::{CredentialLink, CredentialLinkResponse},
};

use crate::routes::messages::MessageQueryParams;
use crate::health::HealthResponse;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        // Add Bearer token authentication
        let components = openapi.components.get_or_insert_with(Default::default);
        components.security_schemes.insert(
            "bearer_auth".to_string(),
            SecurityScheme {
                scheme_type: SecuritySchemeType::Http,
                scheme: Some("bearer".to_string()),
                bearer_format: Some("JWT".to_string()),
                description: Some("DID authentication using JWT-like token format".to_string()),
                ..Default::default()
            },
        );

        // Add global security requirement
        openapi.security = vec![
            SecurityRequirement::new("bearer_auth", Vec::new())
        ];
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        // Thread endpoints
        crate::routes::threads::list_threads,
        crate::routes::threads::get_thread,
        crate::routes::threads::create_thread,
        
        // Message endpoints
        crate::routes::messages::list_messages,
        crate::routes::messages::get_message,
        crate::routes::messages::create_message,
        crate::routes::messages::delete_message,
        
        // Reaction endpoints
        crate::routes::messages::list_reactions,
        crate::routes::messages::add_reaction,
        crate::routes::messages::remove_reaction,
        
        // Credential link endpoints
        crate::routes::credentials::list_credential_links,
        crate::routes::credentials::link_credential,
        
        // Health endpoint
        crate::health::health_check,
    ),
    components(
        schemas(
            Thread,
            ThreadResponse,
            Message,
            MessageResponse,
            CreateMessageRequest,
            MessageQueryParams,
            ReactionRequest,
            ReactionResponse,
            CredentialLink,
            CredentialLinkResponse,
            HealthResponse,
        )
    ),
    tags(
        (name = "AgoraNet API", description = "ICN Deliberation Layer API"),
        (name = "Health", description = "API Health Check"),
    ),
    modifiers(&SecurityAddon),
    info(
        title = "AgoraNet API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Intercooperative Network Deliberation Layer API",
        contact(
            name = "ICN Team",
            url = "https://icn.xyz"
        ),
    )
)]
pub struct ApiDoc; 