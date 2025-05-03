use crate::error::{AgentError, AgentResult};
use wallet_core::identity::IdentityWallet;
use tokio::task;

pub async fn generate_auth_token(
    identity: &IdentityWallet,
    expires_in_minutes: i64,
    thread_scope: Option<String>
) -> AgentResult<String> {
    let identity = identity.clone();
    
    task::spawn_blocking(move || {
        wallet_core::auth::generate_auth_token(&identity, expires_in_minutes, thread_scope)
            .map_err(|e| AgentError::AuthError(format!("Failed to generate auth token: {}", e)))
    }).await?
} 