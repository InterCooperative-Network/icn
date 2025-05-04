/*!
 * ICN Federation Receipt Verification Server
 *
 * HTTP server for verifying execution receipts across federations.
 */

use std::sync::Arc;
use axum::{
    routing::{get, post},
    Router,
    extract::{State, Path},
    response::{IntoResponse, Response, Json},
    http::StatusCode,
};
use serde::{Serialize, Deserialize};
use serde_json::json;
use tracing::{info, error, debug};
use tower_http::trace::TraceLayer;
use url::Url;
use std::net::SocketAddr;

use crate::{ReceiptVerifier, VerifierConfig, VerifyBundleRequest, VerificationResult, VerifierError};
use icn_wallet_core::dag::DagStorageManager;

/// Application state
pub struct AppState<D: DagStorageManager> {
    /// The receipt verifier
    verifier: Arc<ReceiptVerifier<D>>,
}

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host to bind to
    pub host: String,
    
    /// Port to bind to
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }
    }
}

/// Response for bundle verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    /// Whether the request was successful
    pub success: bool,
    
    /// Verification result, if successful
    pub result: Option<VerificationResult>,
    
    /// Error message, if unsuccessful
    pub error: Option<String>,
}

impl IntoResponse for VerifyResponse {
    fn into_response(self) -> Response {
        if self.success {
            (StatusCode::OK, Json(json!({
                "success": self.success,
                "result": self.result
            }))).into_response()
        } else {
            (StatusCode::BAD_REQUEST, Json(json!({
                "success": self.success,
                "error": self.error
            }))).into_response()
        }
    }
}

/// Start the verifier server
pub async fn start_server<D: DagStorageManager + 'static>(
    config: ServerConfig,
    verifier: Arc<ReceiptVerifier<D>>,
) -> anyhow::Result<()> {
    // Create the application state
    let state = Arc::new(AppState {
        verifier,
    });
    
    // Create the router with routes
    let app = Router::new()
        .route("/", get(|| async { "ICN Federation Receipt Verification Service" }))
        .route("/verify", post(verify_bundle::<D>))
        .route("/verify-bundle", post(verify_bundle::<D>))
        .route("/verify-link", get(verify_link::<D>))
        .route("/verify-link/:bundle", get(verify_link::<D>))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    
    // Bind to the address and start the server
    let addr = format!("{}:{}", config.host, config.port).parse::<SocketAddr>()?;
    info!("Starting verifier server on {}", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    
    Ok(())
}

/// Handle verification of receipt bundles
async fn verify_bundle<D: DagStorageManager>(
    State(state): State<Arc<AppState<D>>>,
    Json(request): Json<VerifyBundleRequest>,
) -> VerifyResponse {
    debug!("Received verification request");
    
    match state.verifier.verify_bundle(request).await {
        Ok(result) => {
            info!(
                success = %result.success,
                receipts = %result.receipt_count,
                verified = %result.verified_count,
                sender = %result.sender_federation,
                "Verification complete"
            );
            
            VerifyResponse {
                success: true,
                result: Some(result),
                error: None,
            }
        },
        Err(err) => {
            error!(error = %err, "Verification failed");
            
            VerifyResponse {
                success: false,
                result: None,
                error: Some(format!("Verification failed: {}", err)),
            }
        }
    }
}

/// Handle verification from a share link
async fn verify_link<D: DagStorageManager>(
    State(state): State<Arc<AppState<D>>>,
    Path(bundle): Path<String>,
) -> VerifyResponse {
    debug!("Received verification link request");
    
    // Create a verification request from the bundle
    let request = VerifyBundleRequest {
        bundle,
        sender_federation: None,
    };
    
    verify_bundle(State(state), Json(request)).await
}

/// Handle the redirect from an ICN URI scheme
pub async fn handle_icn_uri(uri: &str, verifier_url: &str) -> anyhow::Result<String> {
    if !uri.starts_with("icn://") {
        return Err(anyhow::anyhow!("Invalid ICN URI: {}", uri));
    }
    
    // Parse the URI
    let uri = uri.replace("icn://", "https://");
    let url = Url::parse(&uri)?;
    
    // Extract the bundle from the query params
    let bundle = url.query_pairs()
        .find(|(k, _)| k == "bundle")
        .map(|(_, v)| v.to_string())
        .ok_or_else(|| anyhow::anyhow!("Missing bundle parameter"))?;
    
    // Construct the URL for the verifier
    let mut verifier_url = verifier_url.to_string();
    if !verifier_url.ends_with('/') {
        verifier_url.push('/');
    }
    
    let redirect_url = format!("{}verify-link/{}", verifier_url, bundle);
    
    Ok(redirect_url)
} 