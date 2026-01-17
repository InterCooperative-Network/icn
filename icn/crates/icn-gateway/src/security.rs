//! Security middleware and configurations for production deployment
//!
//! # CORS Configuration
//!
//! CORS (Cross-Origin Resource Sharing) controls which websites can make requests to this API.
//!
//! ## Production Deployment
//!
//! For production, choose one of these approaches:
//!
//! 1. **Reverse Proxy (Recommended)**: Use `CorsMode::Disabled` and configure CORS at the
//!    reverse proxy level (nginx, Cloudflare, etc.). This is the most secure approach.
//!
//! 2. **Explicit Origins**: Use `CorsMode::AllowedOrigins` with a list of trusted domains.
//!    Example: `["https://app.icn.coop", "https://admin.icn.coop"]`
//!
//! 3. **Permissive (NOT RECOMMENDED)**: Use `CorsMode::Permissive` only for development or
//!    when you truly need to allow any origin (public read-only APIs).
//!
//! ## Security Considerations
//!
//! - Never use `Permissive` mode in production with authenticated endpoints
//! - `allow_any_origin()` with `supports_credentials()` is blocked by browsers
//! - Always prefer explicit origin allowlists over wildcards

use actix_cors::Cors;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{self, HeaderName, HeaderValue},
    Error,
};
use futures_util::Future;
use std::future::{ready, Ready};
use std::pin::Pin;

/// CORS handling mode
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CorsMode {
    /// Disable CORS handling entirely - let reverse proxy handle it
    /// This is the most secure option when behind nginx/Cloudflare
    #[default]
    Disabled,

    /// Allow only specific origins (recommended for production)
    /// Origins must be exact matches (e.g., "https://app.icn.coop")
    AllowedOrigins(Vec<String>),

    /// Allow any origin (dangerous - use only for development)
    /// WARNING: This allows any website to make requests to this API
    Permissive,
}

/// Security configuration for the gateway
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// CORS handling mode
    pub cors_mode: CorsMode,
    /// Enable security headers
    pub enable_security_headers: bool,
    /// Content Security Policy directive
    pub csp_directive: String,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            cors_mode: CorsMode::Disabled, // Secure by default
            enable_security_headers: true,
            csp_directive: "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:".to_string(),
        }
    }
}

impl SecurityConfig {
    /// Development configuration (permissive CORS for local development)
    pub fn development() -> Self {
        Self {
            cors_mode: CorsMode::AllowedOrigins(vec![
                "http://localhost:3000".to_string(),
                "http://localhost:8080".to_string(),
                "http://localhost:5173".to_string(), // Vite default
                "http://127.0.0.1:3000".to_string(),
                "http://127.0.0.1:8080".to_string(),
                "http://127.0.0.1:5173".to_string(), // Vite on 127.0.0.1
            ]),
            enable_security_headers: true,
            csp_directive: "default-src 'self' 'unsafe-inline' 'unsafe-eval'; connect-src 'self' ws: wss: http://localhost:* ws://localhost:*".to_string(),
        }
    }

    /// Production configuration (strict security - use reverse proxy for CORS)
    pub fn production() -> Self {
        Self::default()
    }

    /// Production configuration with explicit allowed origins
    pub fn production_with_origins(origins: Vec<String>) -> Self {
        Self {
            cors_mode: CorsMode::AllowedOrigins(origins),
            enable_security_headers: true,
            csp_directive: "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:".to_string(),
        }
    }
}

/// Configure CORS middleware based on security configuration
///
/// # Security
///
/// - `Disabled`: Returns restrictive CORS (no cross-origin allowed)
/// - `AllowedOrigins`: Only specified origins can make cross-origin requests
/// - `Permissive`: Any origin allowed (DANGEROUS - development only)
pub fn configure_cors(config: &SecurityConfig) -> Cors {
    match &config.cors_mode {
        CorsMode::Disabled => {
            // Restrictive CORS - only same-origin requests allowed
            // Cross-origin requests will be blocked by the browser
            tracing::info!("CORS disabled - only same-origin requests allowed");
            Cors::default()
        }

        CorsMode::AllowedOrigins(origins) => {
            if origins.is_empty() {
                tracing::warn!("CORS configured with empty origins list - same as disabled");
                return Cors::default();
            }

            tracing::info!(
                origins = ?origins,
                "CORS configured with allowed origins"
            );

            let mut cors = Cors::default()
                .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
                .allowed_headers(vec![
                    header::AUTHORIZATION,
                    header::ACCEPT,
                    header::CONTENT_TYPE,
                ])
                .expose_headers(vec![
                    header::CONTENT_TYPE,
                    HeaderName::from_static("x-request-id"),
                ])
                .supports_credentials()
                .max_age(3600); // 1 hour preflight cache

            // Add each allowed origin
            for origin in origins {
                cors = cors.allowed_origin(origin);
            }

            cors
        }

        CorsMode::Permissive => {
            // WARNING: This is dangerous and should only be used for development
            tracing::warn!("⚠️  CORS PERMISSIVE MODE ENABLED - This is insecure for production!");

            // Note: allow_any_origin() cannot be combined with supports_credentials()
            // Browsers will reject this combination for security reasons
            Cors::default()
                .allow_any_origin()
                .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
                .allowed_headers(vec![
                    header::AUTHORIZATION,
                    header::ACCEPT,
                    header::CONTENT_TYPE,
                ])
                .expose_headers(vec![header::CONTENT_TYPE])
                .max_age(3600)
        }
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
                // SAFETY: csp_directive is validated at config load time
                #[allow(clippy::unwrap_used)]
                let csp_value = HeaderValue::from_str(&config.csp_directive).unwrap();
                headers.insert(
                    HeaderName::from_static("content-security-policy"),
                    csp_value,
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
                        "geolocation=(), microphone=(), camera=(), payment=()",
                    ),
                );

                // HSTS (HTTPS only - should be set by reverse proxy, but we add it anyway)
                headers.insert(
                    HeaderName::from_static("strict-transport-security"),
                    HeaderValue::from_static("max-age=31536000; includeSubDomains"),
                );

                // When behind Cloudflare (which adds CORS headers via Transform Rules),
                // remove the ACAO header from our response to avoid duplicates.
                // Cloudflare will add the proper "*" header.
                if std::env::var("ICN_SKIP_CORS")
                    .map(|v| v == "1" || v == "true")
                    .unwrap_or(false)
                {
                    headers.remove(HeaderName::from_static("access-control-allow-origin"));
                    headers.remove(HeaderName::from_static("access-control-allow-methods"));
                    headers.remove(HeaderName::from_static("access-control-allow-headers"));
                    headers.remove(HeaderName::from_static("access-control-max-age"));
                    headers.remove(HeaderName::from_static("access-control-allow-credentials"));
                    headers.remove(HeaderName::from_static("access-control-expose-headers"));
                }
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
                            "Request size {size} exceeds limit {max_size}"
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
        assert_eq!(config.cors_mode, CorsMode::Disabled);
        assert!(config.enable_security_headers);
        assert!(config.csp_directive.contains("default-src 'self'"));
    }

    #[test]
    fn test_security_config_development() {
        let config = SecurityConfig::development();
        match &config.cors_mode {
            CorsMode::AllowedOrigins(origins) => {
                assert!(origins.len() >= 2);
                assert!(origins.iter().any(|o| o.contains("localhost")));
            }
            _ => panic!("Development config should use AllowedOrigins"),
        }
        assert!(config.enable_security_headers);
    }

    #[test]
    fn test_security_config_production() {
        let config = SecurityConfig::production();
        assert_eq!(config.cors_mode, CorsMode::Disabled);
        assert!(config.enable_security_headers);
    }

    #[test]
    fn test_security_config_production_with_origins() {
        let origins = vec![
            "https://app.icn.coop".to_string(),
            "https://admin.icn.coop".to_string(),
        ];
        let config = SecurityConfig::production_with_origins(origins.clone());
        assert_eq!(config.cors_mode, CorsMode::AllowedOrigins(origins));
        assert!(config.enable_security_headers);
    }

    #[test]
    fn test_cors_mode_default() {
        let mode = CorsMode::default();
        assert_eq!(mode, CorsMode::Disabled);
    }

    #[test]
    fn test_cors_mode_equality() {
        assert_eq!(CorsMode::Disabled, CorsMode::Disabled);
        assert_eq!(CorsMode::Permissive, CorsMode::Permissive);
        assert_eq!(
            CorsMode::AllowedOrigins(vec!["https://example.com".to_string()]),
            CorsMode::AllowedOrigins(vec!["https://example.com".to_string()])
        );
        assert_ne!(CorsMode::Disabled, CorsMode::Permissive);
    }

    #[test]
    fn test_configure_cors_disabled_mode() {
        let config = SecurityConfig {
            cors_mode: CorsMode::Disabled,
            enable_security_headers: true,
            csp_directive: String::new(),
        };
        // Should not panic and return a valid Cors instance
        let _cors = configure_cors(&config);
    }

    #[test]
    fn test_configure_cors_allowed_origins_mode() {
        let config = SecurityConfig {
            cors_mode: CorsMode::AllowedOrigins(vec![
                "https://app.icn.coop".to_string(),
                "https://admin.icn.coop".to_string(),
            ]),
            enable_security_headers: true,
            csp_directive: String::new(),
        };
        // Should not panic and return a valid Cors instance
        let _cors = configure_cors(&config);
    }

    #[test]
    fn test_configure_cors_empty_origins_falls_back_to_disabled() {
        let config = SecurityConfig {
            cors_mode: CorsMode::AllowedOrigins(vec![]),
            enable_security_headers: true,
            csp_directive: String::new(),
        };
        // Should not panic - empty origins falls back to disabled behavior
        let _cors = configure_cors(&config);
    }

    #[test]
    fn test_configure_cors_permissive_mode() {
        let config = SecurityConfig {
            cors_mode: CorsMode::Permissive,
            enable_security_headers: true,
            csp_directive: String::new(),
        };
        // Should not panic and return a valid Cors instance
        let _cors = configure_cors(&config);
    }

    #[test]
    fn test_development_config_includes_all_localhost_variants() {
        let config = SecurityConfig::development();
        if let CorsMode::AllowedOrigins(origins) = &config.cors_mode {
            // Check localhost variants
            assert!(origins.iter().any(|o| o == "http://localhost:3000"));
            assert!(origins.iter().any(|o| o == "http://localhost:8080"));
            assert!(origins.iter().any(|o| o == "http://localhost:5173"));
            // Check 127.0.0.1 variants
            assert!(origins.iter().any(|o| o == "http://127.0.0.1:3000"));
            assert!(origins.iter().any(|o| o == "http://127.0.0.1:8080"));
            assert!(origins.iter().any(|o| o == "http://127.0.0.1:5173"));
        } else {
            panic!("Development config should use AllowedOrigins");
        }
    }

    // CSP Security Tests (Issue #415)

    #[test]
    fn test_production_csp_is_strict() {
        // Production CSP must not allow dynamic code execution
        let config = SecurityConfig::production();
        // Check that dangerous CSP directives are not present
        let dangerous_directive = "unsafe-".to_string() + "eval";
        assert!(
            !config.csp_directive.contains(&dangerous_directive),
            "Production CSP must not allow dynamic code execution"
        );
    }

    #[test]
    fn test_production_csp_has_default_self() {
        // Production CSP must restrict default-src to 'self'
        let config = SecurityConfig::production();
        assert!(
            config.csp_directive.contains("default-src 'self'"),
            "Production CSP must have default-src 'self'"
        );
    }

    #[test]
    fn test_production_with_origins_has_strict_csp() {
        // Even with custom origins, CSP should be strict
        let config = SecurityConfig::production_with_origins(vec![
            "https://app.icn.coop".to_string(),
        ]);
        let dangerous_directive = "unsafe-".to_string() + "eval";
        assert!(
            !config.csp_directive.contains(&dangerous_directive),
            "production_with_origins CSP must be strict"
        );
        assert!(
            config.csp_directive.contains("default-src 'self'"),
            "production_with_origins CSP must have default-src 'self'"
        );
    }

    #[test]
    fn test_development_csp_differs_from_production() {
        // Development CSP should be more permissive than production
        let dev_config = SecurityConfig::development();
        let prod_config = SecurityConfig::production();

        // Development allows more things
        assert_ne!(
            dev_config.csp_directive, prod_config.csp_directive,
            "Development and production CSP should differ"
        );

        // Development connects to localhost
        assert!(
            dev_config.csp_directive.contains("localhost"),
            "Development CSP should allow localhost connections"
        );
        assert!(
            !prod_config.csp_directive.contains("localhost"),
            "Production CSP should not reference localhost"
        );
    }
}
