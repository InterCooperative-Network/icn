# Follow-Up Issues for HSM/TPM Backend Implementation

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

This document tracks the follow-up issues needed after completing the IdentityBundle refactor (Phase A).

## Issue Template

Each issue below should be created in the GitHub repository as a separate issue.

---

## Issue 1: Add Optional Signing Backend to IdentityBundle

**Title:** `feat(identity): Add optional DidSigner backend to IdentityBundle`

**Labels:** `enhancement`, `identity`, `hsm-tpm`

**Description:**

### Summary

IdentityBundle needs an optional signing backend to support hardware-backed signing (HSM/TPM) where the private key never exists in application memory.

### Current State

After Phase A refactor, IdentityBundle uses `DidKey` enum but still expects to sign directly. Hardware keys cannot sign without external hardware access.

### Proposed Solution

Add optional signer field:

```rust
pub struct IdentityBundle {
    // ... existing fields ...
    
    /// Optional signing backend for hardware keys
    /// None for software keys (use did_key directly)
    /// Some for hardware keys (delegate to HSM/TPM)
    signer: Option<Arc<dyn DidSigner>>,
}
```

### Implementation Tasks

- [ ] Add `signer` field to `IdentityBundle`
- [ ] Update constructors to accept optional signer
- [ ] Implement `sign()` method that delegates correctly
- [ ] Add tests for software and hardware signing paths
- [ ] Update documentation

### Acceptance Criteria

- Software keys continue working without signer
- Hardware keys require and use signer
- Signing fails gracefully if hardware key has no signer
- All existing tests pass

### Dependencies

- Requires Phase A (IdentityBundle refactor) to be complete

---

## Issue 2: PKCS#11 Backend Returns Hardware DidKey

**Title:** `feat(hsm): Make PKCS#11 backend return Hardware DidKey`

**Labels:** `enhancement`, `hsm`, `pkcs11`

**Description:**

### Summary

The PKCS#11 HSM backend needs to create and return `DidKey::Hardware` instead of failing with "IdentityBundle refactor required".

### Current State

`Pkcs11Backend::init()` currently returns an error stating that IdentityBundle refactor is required. This was the correct behavior until Phase A completed.

### Proposed Solution

1. Generate key pair in HSM
2. Extract public key only
3. Create `DidKey::Hardware` with:
   - `verifying_key`: Public key from HSM
   - `backend_type`: "pkcs11"
   - `hardware_id`: Slot ID + object handle
4. Return bundle with hardware DidKey

### Implementation Tasks

- [ ] Update `Pkcs11Backend::init()` to create hardware DidKey
- [ ] Implement public key extraction from HSM
- [ ] Add slot/handle tracking
- [ ] Create `Pkcs11DidSigner` implementation
- [ ] Add integration test with SoftHSM2
- [ ] Document HSM setup procedure

### Acceptance Criteria

- Key generation in HSM succeeds
- Public key extraction works
- DidKey::Hardware is created correctly
- Signing delegates to HSM
- Integration test with SoftHSM2 passes

### Dependencies

- Requires Issue #1 (Optional signer) to be complete
- Requires SoftHSM2 for testing

---

## Issue 3: TPM Backend Phase 1 - Core Sealing/Unsealing

**Title:** `feat(tpm): Implement TPM 2.0 key sealing and unsealing`

**Labels:** `enhancement`, `tpm`, `phase-1`

**Description:**

### Summary

Implement actual TPM 2.0 key sealing and unsealing to replace the placeholder implementation.

### Current State

`TpmBackend` is gated behind `tpm-experimental` flag and all operations return errors. This is scaffolding only.

### Proposed Solution

Follow Phase 1 of TPM implementation plan (`docs/tpm-implementation-plan.md`):

1. Implement key sealing with TPM
2. Implement key unsealing with TPM
3. Store sealed blob on disk
4. Verify PCR policy on unseal
5. Return hardware DidKey

### Implementation Tasks

- [ ] Initialize TPM context with `tss-esapi`
- [ ] Implement Ed25519 key generation
- [ ] Seal key to TPM with PCR policy
- [ ] Store sealed blob encrypted with Age
- [ ] Implement unsealing with PCR verification
- [ ] Create `TpmDidSigner` implementation
- [ ] Add integration tests with swtpm
- [ ] Document TPM setup procedure

### Acceptance Criteria

- Key sealing to TPM succeeds
- Unsealing verifies PCR values
- Sealed key persists across restarts
- PCR policy prevents unauthorized unsealing
- Integration tests with swtpm pass

### Dependencies

- Requires Issue #1 (Optional signer) to be complete
- Requires Issue #2 or independent path for hardware DidKey
- Requires TPM 2.0 hardware or swtpm for testing

### Security Considerations

- PCR values must be validated
- Sealed blob must be encrypted at rest
- No private key material in memory outside TPM

---

## Issue 4: Gateway Authentication with Hardware Keys

**Title:** `feat(gateway): Support hardware-backed signing in authentication`

**Labels:** `enhancement`, `gateway`, `auth`

**Description:**

### Summary

Gateway authentication middleware needs to support hardware-backed signing where calling `bundle.keypair().sign()` is not possible.

### Current State

Gateway code assumes it can call `bundle.keypair().sign()` directly. This fails for hardware keys.

### Proposed Solution

Update authentication code to use:

```rust
// OLD:
bundle.keypair().sign(message)

// NEW:
bundle.sign(message)?  // Delegates to signer if needed
```

### Implementation Tasks

- [ ] Audit all `keypair().sign()` calls in gateway
- [ ] Replace with `bundle.sign()` calls
- [ ] Add error handling for missing signers
- [ ] Update tests to cover both software and hardware paths
- [ ] Document authentication flow

### Affected Files

- `icn-gateway/src/auth.rs`
- `icn-gateway/src/api/auth.rs`
- `icn-gateway/src/middleware.rs`
- Gateway integration tests

### Acceptance Criteria

- Software keys continue working
- Hardware keys work with signer
- Graceful error if hardware key has no signer
- All gateway tests pass

### Dependencies

- Requires Issue #1 (Optional signer) to be complete

---

## Issue 5: State Snapshot Compatibility with Hardware Keys

**Title:** `feat(snapshot): Support hardware keys in state snapshots`

**Labels:** `enhancement`, `snapshot`

**Description:**

### Summary

State snapshot system needs to handle hardware keys which cannot serialize private key material.

### Current State

Snapshots may attempt to serialize `KeyPair` which includes private keys. This won't work for hardware keys.

### Proposed Solution

1. Detect hardware vs software keys
2. For software: Serialize secret key material (existing behavior)
3. For hardware: Serialize only public key + backend info
4. On restore: Reconnect to hardware backend

### Implementation Tasks

- [ ] Update snapshot serialization for DidKey enum
- [ ] Add hardware key detection
- [ ] Implement public-only serialization for hardware
- [ ] Add backend reconnection on restore
- [ ] Update tests for both key types
- [ ] Document snapshot behavior

### Affected Files

- `icn-snapshot/src/protocol.rs`
- `icn-snapshot/src/coordinator.rs`

### Acceptance Criteria

- Software keys snapshot with private material
- Hardware keys snapshot with public only
- Restore reconnects to hardware backend
- Tests cover both scenarios
- Documentation explains limitations

### Dependencies

- Requires Issue #1 (Optional signer) to be complete
- Requires Issue #2 or #3 for hardware backend

---

## Issue 6: Backend Factory and Configuration

**Title:** `feat(identity): Add backend factory for HSM/TPM selection`

**Labels:** `enhancement`, `identity`, `config`

**Description:**

### Summary

Users need a way to select and configure which keystore backend (Age/PKCS#11/TPM) to use via configuration file.

### Current State

No configuration mechanism exists. Backends are feature-gated but not selectable at runtime.

### Proposed Solution

Add backend configuration:

```toml
[identity]
backend = "age"  # or "pkcs11" or "tpm"

[identity.pkcs11]
library_path = "/usr/lib/libsofthsm2.so"
slot_id = 0
pin = "${PKCS11_PIN}"  # Environment variable

[identity.tpm]
pcr_policy = [0, 1, 2, 3, 7]
owner_auth = "${TPM_OWNER_AUTH}"
```

Factory pattern:

```rust
pub fn open_keystore(config: &IdentityConfig) -> Result<Box<dyn KeyStore>> {
    match config.backend.as_str() {
        "age" => Ok(Box::new(AgeKeyStore::new()?)),
        "pkcs11" => Ok(Box::new(Pkcs11Backend::new(&config.pkcs11)?)),
        "tpm" => Ok(Box::new(TpmBackend::new(&config.tpm)?)),
        _ => bail!("Unknown backend: {}", config.backend),
    }
}
```

### Implementation Tasks

- [ ] Add configuration types
- [ ] Implement backend factory
- [ ] Add environment variable expansion
- [ ] Update icnd to use factory
- [ ] Add icnctl commands for backend selection
- [ ] Document configuration options
- [ ] Add validation and error handling

### Acceptance Criteria

- Config file selects backend
- Factory creates correct backend instance
- Environment variables work for secrets
- icnd uses configured backend
- Documentation complete

### Dependencies

- Requires Issue #2 (PKCS#11) and/or #3 (TPM) for hardware backends
- Can be done in parallel with other issues

---

## Implementation Order

Recommended sequence:

1. **Issue #1** - Add optional signer (prerequisite for all hardware backends)
2. **Issue #6** - Backend factory (enables configuration)
3. **Issue #2** - PKCS#11 integration (smaller scope, SoftHSM2 available)
4. **Issue #4** - Gateway auth updates (required for production use)
5. **Issue #3** - TPM Phase 1 (larger scope, hardware dependent)
6. **Issue #5** - Snapshot compatibility (nice-to-have)

## Timeline Estimates

- Issue #1: ~1 week
- Issue #2: ~1-2 weeks
- Issue #3: ~2-3 weeks
- Issue #4: ~3-5 days
- Issue #5: ~3-5 days
- Issue #6: ~1 week

**Total: ~6-8 weeks** (aligns with TPM implementation plan)

## Testing Strategy

Each issue should include:
- Unit tests for new functionality
- Integration tests with real/simulated hardware
- Backward compatibility tests
- Documentation updates

## Security Review

Before merging to main:
- [ ] Code review by security-focused reviewer
- [ ] Threat model validation
- [ ] HSM/TPM best practices check
- [ ] Documentation security review
