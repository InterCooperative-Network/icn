//! Security middleware and configurations for production deployment

use actix_cors::Cors;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{self, HeaderName, HeaderValue},
    Error,
};
use std::future::{ready, Ready};
use std::pin::Pin;
use futures_util::Future;

/// Security configuration for the gateway
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Enable CORS (only for development - use reverse proxy in production)
    pub enable_cors: bool,
    /// Allowed CORS origins (if enable_cors is true)
    pub cors_origins: Vec<String>,
    /// Enable security headers
    pub enable_security_headers: bool,
    /// Content Security Policy directive
    pub csp_directive: String,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_cors: false, // Disabled by default - use reverse proxy
            cors_origins: vec![],
            enable_security_headers: true,
            csp_directive: "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:".to_string(),
        }
    }
}

impl SecurityConfig {
    /// Development configuration (permissive CORS for local development)
    pub fn development() -> Self {
        Self {
            enable_cors: true,
            cors_origins: vec!["http://localhost:3000".to_string(), "http://localhost:8080".to_string()],
            enable_security_headers: true,
            csp_directive: "default-src 'self' 'unsafe-inline' 'unsafe-eval'; connect-src 'self' ws: wss: http://localhost:* ws://localhost:*".to_string(),
        }
    }

    /// Production configuration (strict security, no CORS - use reverse proxy)
    pub fn production() -> Self {
        Self::default()
    }
}

/// Configure CORS middleware
pub fn configure_cors(config: &SecurityConfig) -> Cors {
    if config.enable_cors {
        let mut cors = Cors::default()
            .allow_any_method()
            .allowed_headers(vec![
                header::AUTHORIZATION,
                header::ACCEPT,
                header::CONTENT_TYPE,
            ])
            .supports_credentials()
            .max_age(3600);

        // Add allowed origins
        for origin in &config.cors_origins {
            cors = cors.allowed_origin(origin);
        }

        cors
    } else {
        // Strict CORS - only same origin
        Cors::default()
            .allow_any_method()
            .allowed_headers(vec![
                header::AUTHORIZATION,
                header::ACCEPT,
                header::CONTENT_TYPE,
            ])
    }
}

/// Security headers middleware
pub struct SecurityHeaders {
    config: SecurityConfig,
}

impl SecurityHeaders {
    pub fn new(config: SecurityConfig) -> Self {
        Self { config }
    }
}

impl<S, B> Transform<S, ServiceRequest> for SecurityHeaders
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = SecurityHeadersMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(SecurityHeadersMiddleware {
            service,
            config: self.config.clone(),
        }))
    }
}

pub struct SecurityHeadersMiddleware<S> {
    service: S,
    config: SecurityConfig,
}

impl<S, B> Service<ServiceRequest> for SecurityHeadersMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);
        let config = self.config.clone();

        Box::pin(async move {
            let mut res = fut.await?;

            if config.enable_security_headers {
                let headers = res.headers_mut();

                // Content Security Policy
                headers.insert(
                    HeaderName::from_static("content-security-policy"),
                    HeaderValue::from_str(&config.csp_directive).unwrap(),
                );

                // Prevent clickjacking
                headers.insert(
                    HeaderName::from_static("x-frame-options"),
                    HeaderValue::from_static("DENY"),
                );

                // Prevent MIME sniffing
                headers.insert(
                    HeaderName::from_static("x-content-type-options"),
                    HeaderValue::from_static("nosniff"),
                );

                // Enable XSS protection (legacy but still useful)
                headers.insert(
                    HeaderName::from_static("x-xss-protection"),
                    HeaderValue::from_static("1; mode=block"),
                );

                // Referrer policy (don't leak URLs to third parties)
                headers.insert(
                    HeaderName::from_static("referrer-policy"),
                    HeaderValue::from_static("strict-origin-when-cross-origin"),
                );

                // Permissions policy (restrict browser features)
                headers.insert(
                    HeaderName::from_static("permissions-policy"),
                    HeaderValue::from_static(
                        "geolocation=(), microphone=(), camera=(), payment=()"
                    ),
                );

                // HSTS (HTTPS only - should be set by reverse proxy, but we add it anyway)
                headers.insert(
                    HeaderName::from_static("strict-transport-security"),
                    HeaderValue::from_static("max-age=31536000; includeSubDomains"),
                );
            }

            Ok(res)
        })
    }
}

/// Request size limit checker
pub async fn check_request_size(
    req: ServiceRequest,
    max_size: usize,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    if let Some(content_length) = req.headers().get(header::CONTENT_LENGTH) {
        if let Ok(size_str) = content_length.to_str() {
            if let Ok(size) = size_str.parse::<usize>() {
                if size > max_size {
                    return Err((
                        actix_web::error::ErrorPayloadTooLarge(format!(
                            "Request size {} exceeds limit {}",
                            size, max_size
                        )),
                        req,
                    ));
                }
            }
        }
    }
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_config_default() {
        let config = SecurityConfig::default();
        assert!(!config.enable_cors);
        assert!(config.enable_security_headers);
        assert!(config.csp_directive.contains("default-src 'self'"));
    }

    #[test]
    fn test_security_config_development() {
        let config = SecurityConfig::development();
        assert!(config.enable_cors);
        assert_eq!(config.cors_origins.len(), 2);
        assert!(config.enable_security_headers);
    }

    #[test]
    fn test_security_config_production() {
        let config = SecurityConfig::production();
        assert!(!config.enable_cors);
        assert!(config.enable_security_headers);
    }
}
