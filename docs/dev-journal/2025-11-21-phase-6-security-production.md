# Phase 6: Security & Production Hardening

**Date**: 2025-11-21
**Phase**: 6 - Security & Production
**Status**: In Progress

---

## Overview

Adding production-ready security features to the ICN Gateway to prepare for pilot deployment. This phase focuses on defense-in-depth: multiple layers of security controls to protect cooperative data and infrastructure.

## Goals

1. **Gateway Security Hardening**
   - CORS configuration (development vs production)
   - Security headers (CSP, X-Frame-Options, HSTS, etc.)
   - Request size limits
   - Timeout configurations

2. **Logging Improvements**
   - Structured logging everywhere
   - Sensitive data redaction
   - Log level configuration
   - Request ID tracking

3. **Health Check Enhancements**
   - Liveness vs readiness probes
   - Dependency health checks
   - Graceful degradation signals

4. **Metrics Expansion**
   - Percentile histograms (p50, p95, p99)
   - Detailed error metrics
   - Business metrics (active users, transactions/day)

---

## Implementation

### 1. Security Module (COMPLETE ✓)

**File**: `icn/crates/icn-gateway/src/security.rs`

**Features Implemented:**

#### SecurityConfig
Three configuration profiles:
- **Default/Production**: Strict security, no CORS (use reverse proxy)
- **Development**: Permissive CORS for localhost, relaxed CSP
- **Custom**: Configurable origins and CSP directives

```rust
pub struct SecurityConfig {
    pub enable_cors: bool,
    pub cors_origins: Vec<String>,
    pub enable_security_headers: bool,
    pub csp_directive: String,
}
```

#### CORS Configuration
- `configure_cors()` - Creates actix-cors middleware
- Development mode: Allow localhost:3000, localhost:8080
- Production mode: Strict same-origin only
- Supports credentials for authenticated requests

#### Security Headers Middleware
Adds comprehensive security headers to all responses:

1. **Content-Security-Policy**
   - Prevents XSS attacks
   - Controls resource loading (scripts, styles, images)
   - Configurable per-environment

2. **X-Frame-Options: DENY**
   - Prevents clickjacking attacks
   - Blocks embedding in iframes

3. **X-Content-Type-Options: nosniff**
   - Prevents MIME-sniffing vulnerabilities
   - Forces browsers to respect Content-Type

4. **X-XSS-Protection: 1; mode=block**
   - Legacy XSS filter (still useful for older browsers)

5. **Referrer-Policy: strict-origin-when-cross-origin**
   - Prevents URL leakage to third parties
   - Protects user privacy

6. **Permissions-Policy**
   - Restricts browser features (geolocation, camera, microphone, payment)
   - Reduces attack surface

7. **Strict-Transport-Security**
   - Forces HTTPS (max-age=1 year, includeSubDomains)
   - Prevents downgrade attacks

#### Request Size Limiter
- `check_request_size()` helper function
- Validates Content-Length header
- Prevents DoS via large requests
- Returns HTTP 413 Payload Too Large

**Files Changed:**
- `icn/crates/icn-gateway/src/security.rs` (new - 267 lines)
- `icn/crates/icn-gateway/src/lib.rs` (added security module)

**Test Coverage:**
- 3 unit tests for SecurityConfig (default, development, production)

---

## Security Architecture

### Defense in Depth Layers

**Layer 1: Network (Reverse Proxy)**
- TLS termination
- DDoS protection
- Geographic filtering
- *ICN provides: HSTS header*

**Layer 2: Application (Gateway)**
- CORS policies
- Security headers
- Request size limits
- Rate limiting (already implemented in Phase 14)
- *ICN provides: All of the above*

**Layer 3: Authentication (JWT)**
- Challenge-response flow
- Scope-based authorization
- Token expiration
- *Already implemented in Phase 14*

**Layer 4: Authorization (Per-Request)**
- Scope enforcement
- Resource ownership validation
- *Already implemented in Phase 14*

### Security Headers Explained

**Content-Security-Policy (CSP)**:
- Most important modern security header
- Prevents XSS by controlling what resources can load
- Production: `default-src 'self'` (only same-origin)
- Development: Relaxed for hot-reload tools

**Why X-Frame-Options: DENY**:
- Prevents clickjacking (embedding ICN UI in malicious iframes)
- Cooperatives handle sensitive data (financial, membership)
- No legitimate use case for iframe embedding

**Why Referrer-Policy**:
- URLs may contain sensitive IDs (cooperative IDs, proposal IDs)
- Prevents leakage to analytics services
- Protects user privacy when clicking external links

**Why Permissions-Policy**:
- ICN doesn't need geolocation, camera, microphone
- Reducing permissions = reducing attack surface
- Prevents malicious scripts from accessing sensors

---

## Production Deployment Recommendations

### Reverse Proxy Configuration

**Nginx Example:**
```nginx
server {
    listen 443 ssl http2;
    server_name icn.example.com;

    # TLS certificates (Let's Encrypt recommended)
    ssl_certificate /etc/letsencrypt/live/icn.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/icn.example.com/privkey.pem;

    # Proxy to ICN Gateway
    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }

    # Rate limiting at nginx level (additional layer)
    limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
    limit_req zone=api burst=20 nodelay;
}
```

**Caddy Example (simpler):**
```
icn.example.com {
    reverse_proxy localhost:8080
}
```

### Environment Configuration

**Development:**
```toml
[gateway.security]
enable_cors = true
cors_origins = ["http://localhost:3000"]
enable_security_headers = true
csp_directive = "default-src 'self' 'unsafe-inline' 'unsafe-eval'; connect-src 'self' ws://localhost:*"
```

**Production:**
```toml
[gateway.security]
enable_cors = false  # Use reverse proxy CORS if needed
enable_security_headers = true
csp_directive = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' wss://icn.example.com"
```

---

## Testing

### Manual Testing

**1. Security Headers:**
```bash
curl -I https://icn.example.com/v1/health

# Should see:
# Content-Security-Policy: default-src 'self'...
# X-Frame-Options: DENY
# X-Content-Type-Options: nosniff
# Strict-Transport-Security: max-age=31536000...
```

**2. CORS (Development):**
```bash
curl -H "Origin: http://localhost:3000" \
     -H "Access-Control-Request-Method: POST" \
     -X OPTIONS \
     https://icn.example.com/v1/auth/challenge

# Should see:
# Access-Control-Allow-Origin: http://localhost:3000
# Access-Control-Allow-Credentials: true
```

**3. Request Size Limit:**
```bash
# Create large payload
dd if=/dev/zero of=/tmp/large.dat bs=1M count=10

# Try to upload
curl -X POST https://icn.example.com/v1/coops \
     --data-binary @/tmp/large.dat

# Should get: 413 Payload Too Large
```

### Security Scanning

```bash
# OWASP ZAP scan
docker run -t owasp/zap2docker-stable zap-baseline.py \
    -t https://icn.example.com

# Mozilla Observatory
curl https://observatory.mozilla.org/analyze/icn.example.com

# Security Headers Check
curl https://securityheaders.com/?q=icn.example.com&hide=on&followRedirects=on
```

---

## Next Steps

### Immediate (This Session)
- [x] Create security module
- [ ] Integrate security middleware into server.rs
- [ ] Add configuration options to GatewayServer
- [ ] Update documentation
- [ ] Add integration tests

### Phase 6 Remaining
- [ ] Logging improvements (structured logging, redaction)
- [ ] Health check enhancements (liveness/readiness)
- [ ] Metrics expansion (percentiles, business metrics)

### Before Pilot Launch
- [ ] Security audit by third party (if budget allows)
- [ ] Penetration testing
- [ ] Load testing with realistic workload
- [ ] Review logs for information leakage

---

## Security Considerations

### What This Protects Against

✅ **XSS (Cross-Site Scripting)**
- CSP prevents inline scripts
- X-XSS-Protection adds legacy protection

✅ **Clickjacking**
- X-Frame-Options prevents iframe embedding

✅ **MIME Sniffing Attacks**
- X-Content-Type-Options forces content type respect

✅ **Man-in-the-Middle**
- HSTS forces HTTPS
- Reverse proxy enforces TLS

✅ **Referrer Leakage**
- Referrer-Policy prevents URL leakage

✅ **Excessive Permissions**
- Permissions-Policy disables unused browser features

✅ **DoS via Large Requests**
- Request size limits (256KB JSON, configurable others)

### What This Doesn't Protect Against

❌ **SQL Injection** - ICN doesn't use SQL (uses Sled)
❌ **DDoS** - Reverse proxy should handle (nginx/Cloudflare)
❌ **Zero-Days** - Keep dependencies updated
❌ **Social Engineering** - User education required
❌ **Insider Threats** - Audit logs + access controls help

---

## Lessons Learned

1. **Defense in Depth Works**
   - Multiple layers catch what one layer misses
   - Security headers are cheap insurance

2. **CSP is Powerful but Complex**
   - Start strict, relax if needed
   - Test thoroughly in development

3. **CORS is Tricky**
   - Production: Let reverse proxy handle it
   - Development: Be explicit about origins

4. **Security vs Usability**
   - Development mode should be permissive
   - Production mode should be strict
   - Make it easy to switch

---

## References

- [OWASP Secure Headers Project](https://owasp.org/www-project-secure-headers/)
- [MDN Web Security](https://developer.mozilla.org/en-US/docs/Web/Security)
- [Content Security Policy Reference](https://content-security-policy.com/)
- [Mozilla Observatory](https://observatory.mozilla.org/)
- [Security Headers](https://securityheaders.com/)

---

*Last Updated: 2025-11-21*
*Status: Security module complete, integration pending*
