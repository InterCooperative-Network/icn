//! Service Discovery API endpoints (Epic 3, Issue #936)
//!
//! RESTful API for service endpoint discovery and management.

use actix_web::{delete, get, post, web, HttpResponse};
use icn_kernel_api::naming::{ServiceEndpoint, ServiceType};
use icn_kernel_api::scope::{CellId, ScopeLevel};
use icn_kernel_api::types::{Endpoint, Signature};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::service_discovery_mgr::ServiceDiscoveryManager;

// ============================================================================
// Request / Response types
// ============================================================================

/// Request to announce a service endpoint.
///
/// The `created_at` and `signature` fields are part of the signed payload: the
/// provider sets `created_at` before signing, and the gateway verifies the
/// Ed25519 signature over the canonical payload (which includes `created_at`).
/// This means the gateway trusts the provider's clock for TTL expiry.
///
/// **Trust model:** A malicious provider could set `created_at` far in the
/// future to extend effective TTL. This is acceptable because providers can
/// already re-announce endpoints. The TTL mechanism protects against *stale*
/// endpoints from providers that have gone offline, not against active abuse.
#[derive(Debug, Deserialize)]
pub struct AnnounceRequest {
    /// Unique service identifier
    pub service_id: String,
    /// DID of the service provider
    pub provider: String,
    /// Endpoint type: "quic", "http", "grpc", or "websocket"
    #[serde(default = "default_endpoint_type")]
    pub endpoint_type: String,
    /// Service type name (e.g., "ledger")
    pub service_type: String,
    /// Service type version (e.g., "1.0")
    pub service_version: String,
    /// Network endpoints
    pub endpoints: Vec<EndpointRequest>,
    /// String addresses (multiaddr or similar)
    #[serde(default)]
    pub addresses: Vec<String>,
    /// Capabilities offered
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Minimum trust score to access
    #[serde(default)]
    pub trust_threshold: f64,
    /// Scope visibility level
    #[serde(default = "default_scope")]
    pub scope_visibility: String,
    /// Optional cell ID for cell-scoped services
    #[serde(default)]
    pub cell_id: Option<String>,
    /// TTL in seconds
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
    /// Unix timestamp of creation (included in the signed payload).
    /// Set by the provider before signing; the gateway does not override this
    /// value because doing so would invalidate the signature.
    pub created_at: u64,
    /// Unix timestamp of last update (included in the signed payload).
    /// For initial announcements this should equal `created_at`.
    pub updated_at: u64,
    /// Ed25519 signature (hex-encoded) over the canonical signing payload
    pub signature: String,
}

fn default_endpoint_type() -> String {
    "http".to_string()
}

fn default_scope() -> String {
    "org".to_string()
}

fn default_ttl() -> u64 {
    3600
}

/// Network endpoint in API request format.
#[derive(Debug, Deserialize)]
pub struct EndpointRequest {
    pub protocol: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub path: Option<String>,
}

/// Response after announcing a service.
#[derive(Debug, Serialize)]
pub struct AnnounceResponse {
    pub service_id: String,
    pub status: String,
}

/// Query parameters for service discovery.
#[derive(Debug, Deserialize)]
pub struct DiscoverQuery {
    /// Service type name to filter by
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    /// Service version to filter by
    pub version: Option<String>,
    /// Maximum scope level
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Comma-separated required capabilities
    pub capabilities: Option<String>,
}

/// Query parameters for service endpoint query API (Issue #936).
#[derive(Debug, Deserialize)]
pub struct QueryServicesParams {
    /// Optional service ID to filter by
    pub service_id: Option<String>,
    /// Maximum scope level to query (defaults to "org")
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Minimum trust threshold (defaults to 0.0)
    #[serde(default)]
    pub min_trust: f64,
}

/// Response for service discovery.
#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub endpoints: Vec<ServiceEndpointResponse>,
    pub count: usize,
}

/// Service endpoint in API response format.
#[derive(Debug, Serialize)]
pub struct ServiceEndpointResponse {
    pub service_id: String,
    pub provider: String,
    pub service_type: String,
    pub service_version: String,
    pub endpoints: Vec<EndpointResponse>,
    pub capabilities: Vec<String>,
    pub trust_threshold: f64,
    pub scope_visibility: String,
    pub ttl_secs: u64,
    pub created_at: u64,
}

/// Network endpoint in API response format.
#[derive(Debug, Serialize)]
pub struct EndpointResponse {
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub path: Option<String>,
}

// ============================================================================
// Conversion helpers
// ============================================================================

fn parse_scope(s: &str) -> ScopeLevel {
    match s.to_lowercase().as_str() {
        "local" => ScopeLevel::Local,
        "cell" => ScopeLevel::Cell,
        "org" => ScopeLevel::Org,
        "federation" => ScopeLevel::Federation,
        "commons" => ScopeLevel::Commons,
        _ => ScopeLevel::Org,
    }
}

fn scope_to_string(scope: ScopeLevel) -> String {
    scope.to_string()
}

fn to_response(ep: &ServiceEndpoint) -> ServiceEndpointResponse {
    ServiceEndpointResponse {
        service_id: ep.service_id.clone(),
        provider: ep.provider.clone(),
        service_type: ep.service_type.name.clone(),
        service_version: ep.service_type.version.clone(),
        endpoints: ep
            .endpoints
            .iter()
            .map(|e| EndpointResponse {
                protocol: e.protocol.clone(),
                host: e.host.clone(),
                port: e.port,
                path: e.path.clone(),
            })
            .collect(),
        capabilities: ep.capabilities.clone(),
        trust_threshold: ep.trust_threshold,
        scope_visibility: scope_to_string(ep.scope_visibility),
        ttl_secs: ep.ttl_secs,
        created_at: ep.created_at,
    }
}

// ============================================================================
// Endpoints
// ============================================================================

/// POST /services/announce - Register a service endpoint
#[post("/announce")]
pub async fn announce_service(
    mgr: web::Data<Arc<ServiceDiscoveryManager>>,
    req: web::Json<AnnounceRequest>,
) -> crate::error::Result<HttpResponse> {
    let sig_bytes = hex::decode(&req.signature).map_err(|_| {
        crate::error::GatewayError::BadRequest("Invalid signature hex encoding".to_string())
    })?;
    if sig_bytes.len() != 64 {
        return Err(crate::error::GatewayError::BadRequest(format!(
            "Signature must be 64 bytes, got {}",
            sig_bytes.len()
        )));
    }

    // Validate timestamp consistency
    if req.updated_at < req.created_at {
        return Err(crate::error::GatewayError::BadRequest(
            "updated_at cannot be before created_at".to_string(),
        ));
    }

    // Prevent absurd future timestamps (allow 1 hour clock skew)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| crate::error::GatewayError::InternalError("System time error".to_string()))?
        .as_secs();
    let max_future = now + 3600;
    if req.created_at > max_future || req.updated_at > max_future {
        return Err(crate::error::GatewayError::BadRequest(
            "Timestamps too far in the future".to_string(),
        ));
    }

    let endpoint_type = match req.endpoint_type.as_str() {
        "quic" => icn_kernel_api::naming::EndpointType::Quic,
        "http" => icn_kernel_api::naming::EndpointType::Http,
        "grpc" => icn_kernel_api::naming::EndpointType::Grpc,
        "websocket" => icn_kernel_api::naming::EndpointType::WebSocket,
        other => {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Invalid endpoint_type: {other}"
            )));
        }
    };

    let endpoint = ServiceEndpoint {
        service_id: req.service_id.clone(),
        provider: req.provider.clone(),
        endpoint_type,
        service_type: ServiceType {
            name: req.service_type.clone(),
            version: req.service_version.clone(),
        },
        endpoints: req
            .endpoints
            .iter()
            .map(|e| {
                let mut ep = Endpoint::new(&e.protocol, &e.host, e.port);
                if let Some(ref path) = e.path {
                    ep = ep.with_path(path);
                }
                ep
            })
            .collect(),
        addresses: req.addresses.clone(),
        capabilities: req.capabilities.clone(),
        trust_threshold: req.trust_threshold,
        scope_visibility: parse_scope(&req.scope_visibility),
        cell_id: req
            .cell_id
            .as_ref()
            .map(|id| {
                let bytes = hex::decode(id).map_err(|_| {
                    crate::error::GatewayError::BadRequest("Invalid cell_id hex encoding".into())
                })?;
                if bytes.len() != 32 {
                    return Err(crate::error::GatewayError::BadRequest(format!(
                        "cell_id must be 32 bytes, got {}",
                        bytes.len()
                    )));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Ok(CellId(arr))
            })
            .transpose()?,
        ttl_secs: req.ttl_secs,
        signature: Signature::new(sig_bytes),
        created_at: req.created_at,
        updated_at: req.updated_at,
    };

    // Verify Ed25519 signature before accepting the endpoint
    icn_gossip::verify_service_endpoint(&endpoint).map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid endpoint signature: {e}"))
    })?;

    mgr.announce(endpoint)
        .await
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    Ok(HttpResponse::Ok().json(AnnounceResponse {
        service_id: req.service_id.clone(),
        status: "announced".to_string(),
    }))
}

/// DELETE /services/{service_id} - Withdraw a service endpoint
#[delete("/{service_id}")]
pub async fn withdraw_service(
    mgr: web::Data<Arc<ServiceDiscoveryManager>>,
    path: web::Path<String>,
    query: web::Query<WithdrawQuery>,
) -> crate::error::Result<HttpResponse> {
    let service_id = path.into_inner();

    mgr.withdraw(&service_id, &query.provider)
        .await
        .map_err(|e| match e {
            icn_kernel_api::naming::NamingError::NotFound(_) => {
                crate::error::GatewayError::NotFound(format!("Service not found: {service_id}"))
            }
            icn_kernel_api::naming::NamingError::Unauthorized(_) => {
                crate::error::GatewayError::Forbidden("Not authorized to withdraw".to_string())
            }
            other => crate::error::GatewayError::InternalError(other.to_string()),
        })?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "service_id": service_id,
        "status": "withdrawn"
    })))
}

/// Query params for withdraw endpoint.
#[derive(Debug, Deserialize)]
pub struct WithdrawQuery {
    pub provider: String,
}

/// GET /services/discover - Discover services with filters
#[get("/discover")]
pub async fn discover_services(
    mgr: web::Data<Arc<ServiceDiscoveryManager>>,
    query: web::Query<DiscoverQuery>,
) -> crate::error::Result<HttpResponse> {
    let scope = parse_scope(&query.scope);

    let service_type = query.service_type.as_ref().map(|name| ServiceType {
        name: name.clone(),
        version: query.version.clone().unwrap_or_default(),
    });

    let capabilities: Vec<String> = query
        .capabilities
        .as_ref()
        .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let results = mgr
        .discover(scope, service_type.as_ref(), &capabilities)
        .await;

    let response_endpoints: Vec<ServiceEndpointResponse> =
        results.iter().map(to_response).collect();
    let count = response_endpoints.len();

    Ok(HttpResponse::Ok().json(DiscoverResponse {
        endpoints: response_endpoints,
        count,
    }))
}

/// GET /services - Query service endpoints with flexible filtering (Issue #936)
///
/// Supports filtering by:
/// - `service_id` (optional): filter to specific service
/// - `scope`: maximum scope level (default: "org")
/// - `min_trust`: minimum trust threshold (default: 0.0)
///
/// Returns endpoints sorted by:
/// 1. Scope proximity (narrower scopes preferred)
/// 2. Trust threshold (higher trust preferred)
///
/// Automatically excludes stale/expired endpoints.
#[get("")]
pub async fn query_services(
    mgr: web::Data<Arc<ServiceDiscoveryManager>>,
    query: web::Query<QueryServicesParams>,
) -> crate::error::Result<HttpResponse> {
    let scope = parse_scope(&query.scope);
    let raw_min_trust = query.min_trust;

    // Validate min_trust to catch obvious client bugs early.
    // Trust scores are expected to be within [0.0, 1.0] and finite.
    if !raw_min_trust.is_finite() || !(0.0..=1.0).contains(&raw_min_trust) {
        return Ok(HttpResponse::BadRequest()
            .body("invalid min_trust: must be a finite value between 0.0 and 1.0"));
    }

    let min_trust = raw_min_trust;

    // Get all endpoints at the requested scope (already filters by scope and excludes stale)
    let mut results = mgr.discover(scope, None, &[]).await;

    // Apply service_id filter if specified
    if let Some(ref service_id) = query.service_id {
        results.retain(|ep| ep.service_id == *service_id);
    }

    // Apply trust threshold filter
    results.retain(|ep| ep.trust_threshold >= min_trust);

    // Sort by scope proximity (narrower first), then by trust threshold (descending)
    results.sort_by(|a, b| {
        // First compare by scope level (narrower is "smaller" and comes first)
        let scope_cmp = a.scope_visibility.cmp(&b.scope_visibility);
        if scope_cmp != std::cmp::Ordering::Equal {
            return scope_cmp;
        }
        // Then by trust threshold (higher trust first, so reverse order)
        // Note: NaN or otherwise invalid trust_threshold values (if present) are treated
        // as equal here; partial_cmp may return None, and unwrap_or provides a safe fallback.
        b.trust_threshold
            .partial_cmp(&a.trust_threshold)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let response_endpoints: Vec<ServiceEndpointResponse> =
        results.iter().map(to_response).collect();
    let count = response_endpoints.len();

    Ok(HttpResponse::Ok().json(DiscoverResponse {
        endpoints: response_endpoints,
        count,
    }))
}

/// GET /services/{service_id} - Get a specific service
#[get("/{service_id}")]
pub async fn get_service(
    mgr: web::Data<Arc<ServiceDiscoveryManager>>,
    path: web::Path<String>,
) -> crate::error::Result<HttpResponse> {
    let service_id = path.into_inner();

    match mgr.get(&service_id).await {
        Some(ep) => Ok(HttpResponse::Ok().json(to_response(&ep))),
        None => Err(crate::error::GatewayError::NotFound(format!(
            "Service not found: {service_id}"
        ))),
    }
}

/// Configure service discovery routes.
///
/// Routes:
/// - `POST /announce` - Register a service
/// - `GET /discover` - Discover services (type-based: filters by service type name)
/// - `GET /` - Query services with flexible filtering (preferred: filters by scope, trust, service_id)
/// - `GET /{service_id}` - Get specific service
/// - `DELETE /{service_id}` - Withdraw a service
///
/// **Usage Guidelines**:
/// - Use `/discover` for type-based discovery by service type name (e.g., "ledger", "governance")
/// - Use `/` (query API) when filtering by scope level, trust threshold, or service ID
pub fn configure(cfg: &mut web::ServiceConfig) {
    // Route registration order matters for actix-web:
    // 1. `discover_services` ("/discover") - specific path, must come before root
    // 2. `query_services` ("/") - root path
    // 3. `get_service` ("/{service_id}") - path parameter, matches after specific paths
    // 4. `withdraw_service` ("/{service_id}") - DELETE method, same pattern as get_service
    cfg.service(announce_service)
        .service(discover_services)
        .service(query_services)
        .service(get_service)
        .service(withdraw_service);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scope() {
        assert_eq!(parse_scope("local"), ScopeLevel::Local);
        assert_eq!(parse_scope("cell"), ScopeLevel::Cell);
        assert_eq!(parse_scope("org"), ScopeLevel::Org);
        assert_eq!(parse_scope("federation"), ScopeLevel::Federation);
        assert_eq!(parse_scope("commons"), ScopeLevel::Commons);
        assert_eq!(parse_scope("unknown"), ScopeLevel::Org);
        assert_eq!(parse_scope("ORG"), ScopeLevel::Org);
    }

    #[test]
    fn test_default_scope_and_ttl() {
        assert_eq!(default_scope(), "org");
        assert_eq!(default_ttl(), 3600);
    }

    #[test]
    fn test_query_services_params_defaults() {
        // Test that default values work correctly for QueryServicesParams
        let json = r#"{"service_id": "test-svc"}"#;
        let params: QueryServicesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.service_id, Some("test-svc".to_string()));
        assert_eq!(params.scope, "org");
        assert_eq!(params.min_trust, 0.0);
    }

    #[test]
    fn test_query_services_params_custom() {
        let json = r#"{"service_id": "test-svc", "scope": "federation", "min_trust": 0.5}"#;
        let params: QueryServicesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.service_id, Some("test-svc".to_string()));
        assert_eq!(params.scope, "federation");
        assert_eq!(params.min_trust, 0.5);
    }

    #[test]
    fn test_query_services_params_no_service_id() {
        let json = r#"{"scope": "cell", "min_trust": 0.3}"#;
        let params: QueryServicesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.service_id, None);
        assert_eq!(params.scope, "cell");
        assert_eq!(params.min_trust, 0.3);
    }
}
