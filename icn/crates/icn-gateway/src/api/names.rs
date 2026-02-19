//! Name resolution API endpoints.
//!
//! Provides Flow B name lookup via the kernel `NamingService` trait.

use actix_web::{get, web, HttpResponse};
use icn_kernel_api::naming::{NameRecord, NamingError, NamingService, ResolveOptions, Target};
use icn_kernel_api::types::{Endpoint, Name};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::GatewayError;

#[derive(Debug, Deserialize)]
pub struct ResolveNameQuery {
    #[serde(default)]
    pub verify_signatures: Option<bool>,
    #[serde(default)]
    pub max_depth: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ResolveNameResponse {
    pub name: String,
    pub resolved_name: String,
    pub target: NameTargetResponse,
    pub authority: String,
    pub ttl_secs: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NameTargetResponse {
    Service {
        endpoint: EndpointResponse,
    },
    Blob {
        hash: String,
    },
    Namespace {
        org: String,
        app: String,
        sub: Option<String>,
    },
    Alias {
        name: String,
    },
    MultiService {
        endpoints: Vec<EndpointResponse>,
    },
}

#[derive(Debug, Serialize)]
pub struct EndpointResponse {
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub path: Option<String>,
}

fn map_endpoint(endpoint: &Endpoint) -> EndpointResponse {
    EndpointResponse {
        protocol: endpoint.protocol.clone(),
        host: endpoint.host.clone(),
        port: endpoint.port,
        path: endpoint.path.clone(),
    }
}

fn map_target(target: &Target) -> NameTargetResponse {
    match target {
        Target::Service { endpoint } => NameTargetResponse::Service {
            endpoint: map_endpoint(endpoint),
        },
        Target::Blob { hash } => NameTargetResponse::Blob {
            hash: hex::encode(hash),
        },
        Target::Namespace { ns } => NameTargetResponse::Namespace {
            org: ns.org.clone(),
            app: ns.app.clone(),
            sub: ns.sub.clone(),
        },
        Target::Alias { name } => NameTargetResponse::Alias {
            name: name.as_str().to_string(),
        },
        Target::MultiService { endpoints } => NameTargetResponse::MultiService {
            endpoints: endpoints.iter().map(map_endpoint).collect(),
        },
    }
}

fn map_record_response(request_name: &Name, resolved: NameRecord) -> ResolveNameResponse {
    ResolveNameResponse {
        name: request_name.as_str().to_string(),
        resolved_name: resolved.name.as_str().to_string(),
        target: map_target(&resolved.target),
        authority: resolved.authority,
        ttl_secs: resolved.ttl.as_secs(),
        created_at: resolved.created_at,
        updated_at: resolved.updated_at,
        metadata: resolved.metadata,
    }
}

fn normalize_name_path(raw_path: &str) -> Result<Name, GatewayError> {
    if raw_path.is_empty() {
        return Err(GatewayError::BadRequest(
            "name path must not be empty".to_string(),
        ));
    }
    if raw_path.starts_with('/') {
        Ok(Name::new(raw_path.to_string()))
    } else {
        Ok(Name::new(format!("/{raw_path}")))
    }
}

fn map_naming_error(error: NamingError) -> GatewayError {
    match error {
        NamingError::NotFound(message) => GatewayError::NotFound(message),
        NamingError::ServiceNotFound(message) => GatewayError::NotFound(message),
        NamingError::InvalidName(message) => GatewayError::BadRequest(message),
        NamingError::TooManyRedirects(depth) => {
            GatewayError::BadRequest(format!("name resolution exceeded max depth: {depth}"))
        }
        NamingError::Unauthorized(message) => GatewayError::Forbidden(message),
        NamingError::InvalidSignature(message) => GatewayError::BadRequest(message),
        NamingError::AlreadyExists(message) => GatewayError::Conflict(message),
        NamingError::RegistryFull(message) => GatewayError::ServiceUnavailable(message),
        NamingError::Timeout => GatewayError::ServiceUnavailable("name resolution timeout".into()),
        NamingError::Internal(message) => GatewayError::InternalError(message),
    }
}

/// GET /names/{path} - Resolve a hierarchical name.
#[get("/{path:.*}")]
pub async fn resolve_name(
    naming: web::Data<Arc<dyn NamingService>>,
    path: web::Path<String>,
    query: web::Query<ResolveNameQuery>,
) -> crate::error::Result<HttpResponse> {
    let name = normalize_name_path(&path.into_inner())?;
    let mut options = ResolveOptions::new();
    if let Some(max_depth) = query.max_depth {
        options = options.with_max_depth(max_depth);
    }
    if matches!(query.verify_signatures, Some(false)) {
        options = options.without_signature_verification();
    }

    let (_, record) = naming
        .resolve_with_options(&name, options)
        .map_err(map_naming_error)?;
    Ok(HttpResponse::Ok().json(map_record_response(&name, record)))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(resolve_name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_adds_leading_slash() {
        let normalized = normalize_name_path("org/demo/service").unwrap();
        assert_eq!(normalized.as_str(), "/org/demo/service");
    }

    #[test]
    fn normalize_path_rejects_empty() {
        let err = normalize_name_path("").unwrap_err();
        assert!(matches!(err, GatewayError::BadRequest(_)));
    }
}
