# Security Fixes - December 18, 2025

## Overview

This patch addresses three critical security vulnerabilities identified during code review:

1. **Unauthenticated inbound QUIC connections**
2. **Missing DID-TLS binding verification**
3. **Gateway scope authorization bypass**

## Changes Made

### 1. Client Certificate Verification (icn-net)

**Files Modified:**
- `icn/crates/icn-net/src/tls.rs`
- `icn/crates/icn-net/src/session.rs`
- `icn/crates/icn-net/src/actor.rs`

**Changes:**
- Implemented `ClientCertVerifier` trait for `DidCertificateVerifier` to verify client certificates on inbound connections
- Updated `create_server_config()` to require client certificate verification with trust-gated validation
- Added `create_server_config_no_client_auth()` for development/testing (with clear warnings)
- Modified `SessionManager::start()` to conditionally enable client verification based on trust_graph availability
- Added explicit DID-TLS binding verification in `handle_connection()` Hello message handler

**Security Impact:**
- Server now requires and validates client certificates during TLS handshake
- Trust graph integration ensures only trusted peers can establish connections
- DID-TLS binding is explicitly verified before accepting peer capabilities

### 2. Gateway Scope Allowlist (icn-gateway)

**Files Modified:**
- `icn/crates/icn-gateway/src/validation.rs`

**Changes:**
- Added `ALLOWED_SCOPES` constant with explicit allowlist of valid scopes
- Updated `validate_scopes()` to check requested scopes against allowlist
- Prevents privilege escalation by rejecting arbitrary scope requests

**Allowed Scopes:**
```
ledger:read, ledger:write
coop:read, coop:write, coop:admin
gov:read, gov:write, governance:read
payments:read, payments:write
federation:read, federation:write, federation:admin
compute:read, compute:write
constitutional:read, constitutional:write, constitutional:admin
```

**Security Impact:**
- Clients can no longer request arbitrary scopes during authentication
- Token issuance is limited to predefined, authorized capabilities
- Eliminates privilege escalation via scope injection

## Testing

All existing tests pass with these changes:
- `cargo test -p icn-net` ✅
- `cargo test -p icn-gateway` ✅
- Specific tests verified:
  - `test_create_server_config` - Updated to use trust-gated configuration
  - `test_validate_scopes` - Enhanced to test scope allowlist enforcement

## Deployment Notes

### Production Configuration

**IMPORTANT:** Production deployments must provide a trust graph to `SessionManager::start()`:

```rust
session_manager.start(
    &keypair,
    listen_addr,
    Some(trust_graph),           // Required for client cert verification
    Some(0.1),                    // Minimum trust threshold
    stun_servers,
    turn_config,
).await?;
```

Without a trust graph, the system falls back to `create_server_config_no_client_auth()` and logs a warning. This is acceptable for local development but **NOT for production**.

### Development Mode

For development/testing without a trust graph:
```rust
// Warning will be logged: "Starting session manager WITHOUT client certificate verification"
session_manager.start(&keypair, listen_addr, None, None, None, None).await?;
```

## Security Model Improvements

### Before
1. ❌ Server accepted any QUIC client without verification
2. ❌ DID-TLS binding was assumed but never checked
3. ❌ Gateway tokens could request arbitrary scopes (including admin)

### After
1. ✅ Server requires client certificates and validates them via trust graph
2. ✅ DID-TLS binding is explicitly verified on Hello message receipt
3. ✅ Gateway tokens are limited to predefined, approved scopes

## Related Documentation

- Original security review findings in conversation history
- Trust-gated TLS verification: `icn/crates/icn-net/src/tls.rs` module documentation
- Gateway authentication flow: `icn/crates/icn-gateway/src/auth.rs`

## Authors

- Security review and fixes: GitHub Copilot CLI
- Date: December 18, 2025
