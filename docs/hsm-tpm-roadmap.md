# HSM/TPM Backend Roadmap

⚠️ **PLANNED FEATURE - NOT YET IMPLEMENTED**

This document outlines the planned HSM/TPM backend support for ICN keystore. These features are **not currently available** in icnd or icnctl.

## Current Status

### Implemented (Scaffolding Only)
- ✅ `DidSigner` trait for hardware/software signing abstraction
- ✅ `DidKey` enum (Software/Hardware variants)
- ✅ Type safety: hardware keys cannot expose secrets
- ✅ PKCS#11 backend scaffold (returns error, requires IdentityBundle refactor)
- ✅ TPM backend scaffold (gated behind `tpm-experimental` flag)

### Not Implemented
- ❌ CLI commands for backend selection (`icnctl id init --backend`)
- ❌ Configuration file backend selection
- ❌ IdentityBundle refactoring to use DidKey
- ❌ Backend factory pattern
- ❌ Integration with AgeKeyStore
- ❌ Working PKCS#11 implementation
- ❌ Working TPM implementation
- ❌ Any daemon/CLI integration

## Architecture Plan

### Type System (✅ Complete)

```rust
// Honest type representation - hardware keys don't expose secrets
pub trait DidSigner: Send + Sync {
    fn did(&self) -> &Did;
    fn verifying_key(&self) -> &VerifyingKey;
    fn sign(&self, msg: &[u8]) -> Result<Signature>;
    fn is_hardware_backed(&self) -> bool;
}

pub enum DidKey {
    Software { 
        secret_bytes: Zeroizing<[u8; 32]>, 
        verifying_key: VerifyingKey, 
        did: Did 
    },
    Hardware { 
        did: Did, 
        verifying_key: VerifyingKey, 
        backend_type: String,
        hardware_id: String 
    },
}
```

### Next Steps (Not Yet Started)

1. **Refactor IdentityBundle** (Critical Path)
   - Currently embeds `KeyPair` (requires secret material)
   - Must be changed to use `DidKey` or split into:
     - DID signing key (DidKey - software or hardware)
     - TLS key + cert (always software)
     - X25519 encryption key (always software)

2. **Backend Integration**
   - Create backend factory: `BackendConfig -> Box<dyn Backend>`
   - Integrate with existing AgeKeyStore
   - Add config file parsing for backend selection

3. **PKCS#11 Implementation**
   - Currently returns error "IdentityBundle refactor required"
   - Once IdentityBundle supports DidKey, implement:
     - Key generation in HSM
     - Signing delegation to HSM
     - Session management
   - Test **only** with SoftHSM2 initially
   - Document tested HSMs explicitly

4. **TPM Implementation**
   - Currently gated behind `tpm-experimental` (compile error)
   - Must implement:
     - Ed25519 key sealing to TPM
     - Key unsealing with PCR verification
     - Software signing with unsealed key
   - Or: remove from this PR and open separate issue

5. **CLI Integration**
   - Add `--backend` flag to `icnctl id init`
   - Add backend configuration to icn.toml
   - Wire into icnd startup

6. **Documentation**
   - Once features are implemented, document:
     - HSM setup procedures (per-vendor)
     - TPM setup procedures
     - Migration from Age keystore
     - Security considerations

## Why This Approach?

### Problem with Previous Attempt

The initial implementation created a "dummy KeyPair" for HSM keys:
```rust
// ❌ DANGEROUS - Type lie
let dummy_secret = [0u8; 32];
KeyPair::from_bytes(&dummy_secret, &verifying_key.to_bytes())
```

This violated type safety:
- Code expecting a real KeyPair could receive one with invalid secrets
- Downstream code could accidentally call `.sign()` expecting software signing
- Security footgun waiting to happen

### Current Approach

Hardware keys cannot be used until IdentityBundle refactor is complete:
```rust
// ✅ HONEST - Explicit error
pub fn init(&mut self, _pin: &[u8]) -> Result<IdentityBundle> {
    anyhow::bail!(
        "PKCS#11 backend init is not yet implemented: IdentityBundle \
         currently requires KeyPair with secret material, but HSM keys \
         never expose secrets. IdentityBundle must be refactored to use \
         DidKey::Hardware before this backend can be used."
    )
}
```

This ensures:
- No type lies in the codebase
- Clear error messages explaining what's needed
- Impossible to accidentally use non-functional code

## Testing Strategy

### Unit Tests (✅ Complete)
- DidKey enum behaviors
- Software/hardware type distinction
- Cannot extract secrets from hardware keys

### Integration Tests (Planned)
- SoftHSM2 integration test (real PKCS#11 operations)
- TPM simulator integration test
- Backend factory tests
- Migration tests (Age → HSM/TPM)

### Manual Testing (Planned)
- YubiHSM2 physical device
- AWS CloudHSM
- Real TPM 2.0 hardware

## Timeline

This is architectural scaffolding only. Actual implementation timeline TBD.

Suggested incremental approach:
1. **PR A** (Next): IdentityBundle refactor to use DidKey
2. **PR B** (After A): PKCS#11 backend with SoftHSM2 tests
3. **PR C** (After B): CLI/daemon integration
4. **PR D** (Separate): TPM implementation (if pursuing)

## References

- Issue #481: feat(identity): Add HSM/TPM backend for keystore
- PKCS#11 v2.40 Specification: http://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/
- TPM 2.0 Specification: https://trustedcomputinggroup.org/resource/tpm-library-specification/

## Security Considerations

### NOT Production Ready

**This scaffolding provides NO security guarantees.**

The "key never leaves hardware boundary" guarantee will only be claimed after:
1. IdentityBundle refactor is complete
2. Backend implementation is tested with real hardware
3. Integration tests validate end-to-end flows
4. Security audit is performed

### Algorithm Constraints

**PKCS#11:**
- Ed25519 support is module-dependent
- Different HSMs encode keys differently
- Testing required per vendor/model

**TPM 2.0:**
- Does not natively support Ed25519
- Requires software sealing approach
- Performance limitations (~50-200ms per sign operation)

## Contributing

If you want to work on HSM/TPM implementation:

1. Start with IdentityBundle refactor (blocks everything else)
2. Test with SoftHSM2 / TPM simulator (no physical hardware needed initially)
3. Document tested configurations explicitly
4. Do NOT claim compatibility with untested HSMs

For questions, see:
- `icn-identity/src/did_signer.rs` - Type system foundation
- `icn-identity/src/keystore_pkcs11.rs` - PKCS#11 scaffold
- `icn-identity/src/keystore_tpm.rs` - TPM scaffold (experimental)
