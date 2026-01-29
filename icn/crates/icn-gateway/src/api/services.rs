//! Service Discovery API endpoints (Epic 3, Issue #936)
//!
//! RESTful API for service endpoint discovery and management.

use actix_web::{delete, get, post, web, HttpResponse};
use icn_kernel_api::naming::{ServiceEndpoint, ServiceType};
use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::types::{Endpoint, Signature};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::service_discovery_mgr::ServiceDiscoveryManager;

// ============================================================================
// Request / Response types
// ============================================================================

/// Request to announce a service endpoint.
#[derive(Debug, Deserialize)]
pub struct AnnounceRequest {
    /// Unique service identifier
    pub service_id: String,
    /// DID of the service provider
    pub provider: String,
    /// Service type name (e.g., "ledger")
    pub service_type: String,
    /// Service type version (e.g., "1.0")
    pub service_version: String,
    /// Network endpoints
    pub endpoints: Vec<EndpointRequest>,
    /// Capabilities offered
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Minimum trust score to access
    #[serde(default)]
    pub trust_threshold: f64,
    /// Scope visibility level
    #[serde(default = "default_scope")]
    pub scope_visibility: String,
    /// TTL in seconds
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
    /// Ed25519 signature (hex-encoded)
    #[serde(default)]
    pub signature: String,
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let sig_bytes = hex::decode(&req.signature).unwrap_or_default();

    let endpoint = ServiceEndpoint {
        service_id: req.service_id.clone(),
        provider: req.provider.clone(),
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
        capabilities: req.capabilities.clone(),
        trust_threshold: req.trust_threshold,
        scope_visibility: parse_scope(&req.scope_visibility),
        ttl_secs: req.ttl_secs,
        signature: Signature::new(sig_bytes),
        created_at: now,
    };

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
/// - `GET /discover` - Discover services (must be registered before `{service_id}`)
/// - `GET /{service_id}` - Get specific service
/// - `DELETE /{service_id}` - Withdraw a service
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(announce_service)
        .service(discover_services)
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
}
