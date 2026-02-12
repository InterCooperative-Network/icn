# Gateway Content Security Policy

This document describes the Content Security Policy (CSP) configuration for the ICN Gateway API.

## Overview

Content Security Policy is a security header that helps prevent XSS attacks, clickjacking, and other code injection attacks by controlling which resources the browser is allowed to load.

## CSP Configurations

The gateway provides different CSP configurations for development and production environments.

### Production CSP (Default)

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
img-src 'self' data:;
connect-src 'self' ws: wss:;
```

**Note:** The CSP is sent as a single-line HTTP header. The formatting above is for readability.

| Directive | Value | Purpose |
|-----------|-------|---------|
| `default-src` | `'self'` | Only load resources from same origin |
| `script-src` | `'self'` | Scripts only from same origin (no inline) |
| `style-src` | `'self' 'unsafe-inline'` | Styles from same origin + inline styles |
| `img-src` | `'self' data:` | Images from same origin + data URIs |
| `connect-src` | `'self' ws: wss:` | API calls to same origin + WebSocket |

**Note:** `'unsafe-inline'` in `style-src` is intentionally allowed for production. This is a common pattern because:
- Many UI frameworks require inline styles for dynamic styling
- Inline styles are lower risk than inline scripts
- The alternative (nonces or hashes) adds significant complexity

### Development CSP

The development configuration includes relaxed CSP directives to support hot module replacement (HMR) and development tooling:

- Allows inline scripts and styles
- Allows dynamic code execution for HMR
- Permits connections to any localhost port

**Security Warning:** The development CSP includes directives that would be dangerous in production. These are required for development tooling but **MUST NOT** be used in production deployments.

## Configuration Usage

### Code Examples

```rust
use icn_gateway::security::SecurityConfig;

// Production (recommended)
let config = SecurityConfig::production();

// Production with custom origins
let config = SecurityConfig::production_with_origins(vec![
    "https://app.icn.coop".to_string(),
    "https://admin.icn.coop".to_string(),
]);

// Development only
let config = SecurityConfig::development();
```

### Environment Detection

The gateway does NOT automatically detect environment. You must explicitly choose:

```rust
let config = if cfg!(debug_assertions) {
    SecurityConfig::development()
} else {
    SecurityConfig::production()
};
```

## Security Checklist

Before deploying to production, verify:

- [ ] Using `SecurityConfig::production()` or `production_with_origins()`
- [ ] CSP does not include dangerous directives (check for development config leakage)
- [ ] CORS properly configured (preferably `CorsMode::Disabled` with reverse proxy handling)
- [ ] HTTPS enforced (HSTS header is set by default)
- [ ] Security headers enabled (`enable_security_headers: true`)

## Other Security Headers

The gateway also sets these security headers (enabled by default):

| Header | Value | Purpose |
|--------|-------|---------|
| `X-Frame-Options` | `DENY` | Prevent clickjacking |
| `X-Content-Type-Options` | `nosniff` | Prevent MIME sniffing |
| `X-XSS-Protection` | `1; mode=block` | Legacy XSS protection |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Limit referrer leakage |
| `Permissions-Policy` | Restrictive | Disable geolocation, mic, camera, payment |
| `Strict-Transport-Security` | `max-age=31536000; includeSubDomains` | Enforce HTTPS |

## Testing CSP

### Manual Verification

Check response headers in browser DevTools (Network tab) or curl:

```bash
curl -I https://your-gateway/api/health
```

Look for `Content-Security-Policy` header.

### Automated Verification

The gateway includes tests to verify CSP configuration is secure for production.

## Related Files

- `icn-gateway/src/security.rs` - Security middleware implementation
- `icn-core/src/config/gateway.rs` - Gateway configuration
- `docs/production-hardening.md` - Overall security hardening guide

## References

- [MDN: Content Security Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/CSP)
- [OWASP: Content Security Policy Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Content_Security_Policy_Cheat_Sheet.html)
