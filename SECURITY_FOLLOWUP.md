# Security Follow-up Tasks

## Completed Security Fixes (Session 2025-12-18)

✅ **DID-TLS Binding Verification**: Added explicit verification of DID-TLS binding during Hello message handling in `icn-net/src/actor.rs`. The server now verifies that the binding_info in the Hello message matches the peer's TLS certificate.

✅ **Gateway Scope Validation**: Added proper scope validation and authorization checks in `icn-gateway/src/validation.rs` and `icn-gateway/src/api/auth.rs`. Tokens can no longer request arbitrary scopes.

✅ **Comprehensive Security Tests**: Added extensive test coverage in:
- `icn-net/tests/client_cert_verification_integration.rs`
- `icn-gateway/tests/scope_validation_integration.rs`

## Pending Security Enhancements

### 1. Full Mutual TLS with Client Certificates (High Priority)

**Current Status**: Partially implemented but disabled due to test failures.

**Issue**: The current implementation adds client certificate verification on the server side (`DidCertificateVerifier`), but this breaks bidirectional communication in multi-node tests. The problem is that when Node A dials Node B, Node B needs to send messages back to Node A, but if Node B doesn't have a pre-existing trust relationship with Node A, the TLS handshake times out or is rejected.

**Files Modified**:
- `icn-net/src/tls.rs`: Added `DidCertificateVerifier` for both client and server cert verification
- `icn-net/src/session.rs`: Server config conditionally enables client cert verification based on trust graph presence

**Test Failures**:
- `test_did_tls_binding_verified_on_hello`: TLS handshake timeout
- `test_dev_mode_no_client_cert_verification`: Stream closed by peer
- Multiple tests in `did_tls_binding_integration.rs`: Stream closure issues

**Next Steps**:
1. Investigate why TLS handshakes are timing out even when trust scores meet thresholds
2. Consider implementing "trust-on-first-use" (TOFU) semantics for initial connections
3. Update all integration tests to properly set up bidirectional trust relationships
4. Add metrics/logging to track TLS handshake failures and reasons
5. Document the trust setup requirements for production deployments

**Architecture Decision Needed**:
Should ICN require mutual trust BEFORE allowing QUIC connections, or should it allow connections and then gate specific operations based on trust? Current implementation attempts the former, but tests suggest the latter might be more practical.

### 2. Replay Protection Cleanup (Medium Priority)

**Issue**: The `ReplayGuard` uses Bloom filters that can saturate over long-running sessions, potentially causing false positives and DoS. The `cleanup()` method exists but isn't actively called.

**Location**: `icn-net/src/replay_guard.rs`

**Recommendation**: Implement periodic cleanup task in NetworkActor to prune old entries and rotate Bloom filters.

### 3. Rate Limiter Pre-Verification Check (Low Priority)

**Issue**: Rate limiter checks `message.from` before signature verification in `actor.rs:1452`. While not a critical vulnerability (unsigned messages are rejected later), it could allow minor resource exhaustion.

**Recommendation**: Move rate limit check after signature verification, or implement a two-tier rate limiter (loose pre-auth, strict post-auth).

## Testing Strategy

When re-enabling client certificate verification:

1. Start with simple two-node tests where both nodes explicitly trust each other
2. Gradually add complexity (three nodes, partial trust, untrusted peers)
3. Test connection resilience (reconnection after trust changes)
4. Performance test: measure TLS handshake overhead with cert verification

## Documentation Needs

- Update `docs/ARCHITECTURE.md` with TLS security model
- Add deployment guide for setting up trust relationships
- Document the trust threshold semantics (0.0 = allow all authenticated, 0.5 = moderate trust, etc.)
- Create troubleshooting guide for TLS handshake failures

## Metrics to Add

- `icn_net_tls_handshake_duration_seconds`: Histogram of TLS handshake times
- `icn_net_tls_cert_verification_failures_total`: Counter of cert verification failures by reason
- `icn_net_client_cert_presented_total`: Counter of successful client cert presentations

## Related Issues

- Sequence number persistence for replay protection (#TBD)
- Trust graph persistence and synchronization (#TBD)
- Gateway token scope allowlist configuration (#TBD)

---

Last Updated: 2025-12-18
Contributors: GitHub Copilot CLI Security Review
