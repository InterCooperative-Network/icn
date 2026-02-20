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
        // Cap to prevent unreasonably deep traversal via the REST API.
        options = options.with_max_depth(max_depth.min(32));
    }
    match query.verify_signatures {
        Some(false) => {
            options = options.without_signature_verification();
        }
        Some(true) | None => {
            // verify_signatures defaults to true; explicit true is a no-op.
        }
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
    use actix_web::{test, App};
    use icn_kernel_api::naming::{NameRecord, ResolveOptions, Target};
    use icn_kernel_api::types::{Endpoint, Name, Signature};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    /// Minimal NamingService mock backed by a closure for full flexibility.
    struct MockNaming<F>
    where
        F: Fn(&Name, ResolveOptions) -> Result<(Target, NameRecord), NamingError> + Send + Sync,
    {
        handler: F,
    }

    impl<F> NamingService for MockNaming<F>
    where
        F: Fn(&Name, ResolveOptions) -> Result<(Target, NameRecord), NamingError> + Send + Sync,
    {
        fn register(
            &self,
            _: &Name,
            _: Target,
            _: &icn_kernel_api::types::Did,
            _: &Signature,
            _: Duration,
        ) -> Result<NameRecord, NamingError> {
            unimplemented!()
        }

        fn resolve(&self, _: &Name) -> Result<Target, NamingError> {
            unimplemented!()
        }

        fn resolve_with_options(
            &self,
            name: &Name,
            options: ResolveOptions,
        ) -> Result<(Target, NameRecord), NamingError> {
            (self.handler)(name, options)
        }

        fn update(&self, _: &Name, _: Target, _: &Signature) -> Result<NameRecord, NamingError> {
            unimplemented!()
        }

        fn delete(&self, _: &Name, _: &Signature) -> Result<(), NamingError> {
            unimplemented!()
        }

        fn get_record(&self, _: &Name) -> Result<NameRecord, NamingError> {
            unimplemented!()
        }

        fn list(&self, _: &Name) -> Result<Vec<Name>, NamingError> {
            unimplemented!()
        }

        fn watch(&self, _: &Name) -> Result<icn_kernel_api::types::Subscription, NamingError> {
            unimplemented!()
        }

        fn verify(&self, _: &NameRecord) -> Result<bool, NamingError> {
            unimplemented!()
        }
    }

    fn stub_record(name: &str) -> NameRecord {
        NameRecord {
            name: Name::new(name.to_string()),
            target: Target::Service {
                endpoint: Endpoint {
                    protocol: "https".to_string(),
                    host: "localhost".to_string(),
                    port: 8080,
                    path: None,
                },
            },
            authority: "did:icn:testauthority".to_string(),
            signature: Signature(vec![]),
            ttl: Duration::from_secs(300),
            created_at: 1000,
            updated_at: 1000,
            metadata: HashMap::new(),
        }
    }

    #[actix_web::test]
    async fn normalize_path_adds_leading_slash() {
        let normalized = normalize_name_path("org/demo/service").unwrap();
        assert_eq!(normalized.as_str(), "/org/demo/service");
    }

    #[actix_web::test]
    async fn normalize_path_rejects_empty() {
        let err = normalize_name_path("").unwrap_err();
        assert!(matches!(err, GatewayError::BadRequest(_)));
    }

    #[actix_web::test]
    async fn test_resolve_name_success_returns_200() {
        let svc: Arc<dyn NamingService> = Arc::new(MockNaming {
            handler: |name, _opts| {
                let rec = stub_record(name.as_str());
                let target = rec.target.clone();
                Ok((target, rec))
            },
        });

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .service(web::scope("/names").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/names/org/demo/service")
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["target"]["kind"], "service");
        assert_eq!(body["target"]["endpoint"]["host"], "localhost");
        assert_eq!(body["authority"], "did:icn:testauthority");
    }

    #[actix_web::test]
    async fn test_resolve_name_not_found_returns_404() {
        let svc: Arc<dyn NamingService> = Arc::new(MockNaming {
            handler: |_, _| Err(NamingError::NotFound("no such name".into())),
        });

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .service(web::scope("/names").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/names/missing/path")
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn test_resolve_name_unauthorized_returns_403() {
        let svc: Arc<dyn NamingService> = Arc::new(MockNaming {
            handler: |_, _| Err(NamingError::Unauthorized("not your name".into())),
        });

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(svc))
                .service(web::scope("/names").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/names/protected/path")
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 403);
    }
}
