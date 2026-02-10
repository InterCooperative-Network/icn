# TPM 2.0 Implementation Plan

**Status:** Planned - Not Yet Implemented  
**Target:** Production-ready TPM key sealing and signing

## Overview

This document outlines the complete implementation plan for TPM 2.0 backend support in ICN. The goal is to enable production nodes to seal Ed25519 private keys inside TPM hardware, preventing memory extraction and providing platform binding.

## Background: Why TPM for Ed25519?

TPM 2.0 does not natively support Ed25519. However, we can use TPM for:
1. **Sealed storage** - Key sealed to TPM, bound to platform PCRs
2. **Software signing** - After unsealing, sign in software with extra protections
3. **Attestation** - Prove the key is TPM-sealed and platform-bound

This provides significant security benefits over pure software storage:
- Keys cannot be extracted without TPM hardware access
- Platform binding ensures keys only work on the original machine
- PCR measurement provides boot-time integrity verification

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│  Application Layer                                   │
│  ┌─────────────────────────────────────────────┐   │
│  │ IdentityBundle (uses DidKey::Hardware)      │   │
│  └──────────────┬──────────────────────────────┘   │
│                 │                                     │
│  ┌──────────────▼───────────────────────────────┐   │
│  │ TpmDidSigner (implements DidSigner trait)    │   │
│  │  - Unseals key from TPM on demand           │   │
│  │  - Signs in software with unsealed key      │   │
│  │  - Re-seals after signing (optional)        │   │
│  └──────────────┬───────────────────────────────┘   │
│                 │                                     │
├─────────────────┼─────────────────────────────────  │
│  TPM Layer      │                                     │
│  ┌──────────────▼───────────────────────────────┐   │
│  │ TpmBackend (implements KeyStoreBackend)      │   │
│  │  - init(): Seal new key to TPM               │   │
│  │  - unlock(): Unseal key from TPM             │   │
│  │  - signing_backend(): Return TpmDidSigner    │   │
│  └──────────────┬───────────────────────────────┘   │
│                 │                                     │
│  ┌──────────────▼───────────────────────────────┐   │
│  │ tss-esapi (TPM Software Stack)               │   │
│  │  - SealedData API                            │   │
│  │  - PCR binding                               │   │
│  │  - Attestation                               │   │
│  └──────────────┬───────────────────────────────┘   │
└─────────────────┼─────────────────────────────────  ┘
                  │
         ┌────────▼────────┐
         │  TPM 2.0 Hardware│
         └─────────────────┘
```

## Implementation Phases

### Phase 1: Core TPM Sealing/Unsealing ✅ (Prerequisite: IdentityBundle refactor)

**Blockers:**
- IdentityBundle must use `DidKey` enum instead of embedding `KeyPair`
- This is being implemented in Phase A (IdentityBundle refactor)

**Tasks:**
1. Implement TPM context initialization
   - Connect to TPM resource manager (`/dev/tpmrm0`)
   - Handle TCTI (TPM Command Transmission Interface) setup
   - Error handling for missing TPM hardware

2. Implement key sealing
   ```rust
   fn seal_key(
       context: &mut Context,
       secret_bytes: &[u8; 32],
       pcr_selection: &[u32],
   ) -> Result<SealedData>
   ```
   - Generate sealing key in TPM
   - Seal Ed25519 private key with TPM
   - Bind to platform PCRs (0, 7 for boot integrity)
   - Store sealed blob to disk

3. Implement key unsealing
   ```rust
   fn unseal_key(
       context: &mut Context,
       sealed_data: &SealedData,
   ) -> Result<Zeroizing<[u8; 32]>>
   ```
   - Load sealed data from disk
   - Verify PCR values match sealing time
   - Unseal key using TPM
   - Return private key (zeroized)

4. Add PCR management
   - Read current PCR values
   - Verify against expected measurements
   - Handle PCR mismatch (unsealing fails)

**Deliverables:**
- `TpmContext` struct wrapping tss-esapi context
- Seal/unseal functions with PCR binding
- Unit tests with TPM simulator (swtpm)
- Error handling for missing TPM hardware

**Testing:**
- Use swtpm (software TPM simulator) for CI
- Manual testing with real TPM 2.0 hardware
- Test PCR binding (unseal fails after PCR change)
- Test persistent storage (seal, reboot, unseal)

### Phase 2: TpmDidSigner Implementation

**Tasks:**
1. Implement `TpmDidSigner` struct
   ```rust
   pub struct TpmDidSigner {
       did: Did,
       verifying_key: VerifyingKey,
       tpm_context: Arc<Mutex<TpmContext>>,
       sealed_data_path: PathBuf,
       unsealed_key: Option<Zeroizing<[u8; 32]>>, // Cached unsealed key
   }
   ```

2. Implement `DidSigner` trait
   ```rust
   impl DidSigner for TpmDidSigner {
       fn did(&self) -> &Did;
       fn verifying_key(&self) -> &VerifyingKey;
       fn sign(&self, message: &[u8]) -> Result<Signature>;
       fn is_hardware_backed(&self) -> bool { true }
       fn backend_type(&self) -> &str { "tpm" }
   }
   ```

3. Implement signing flow
   - Check if key is unsealed (cached)
   - If not, unseal from TPM
   - Sign with unsealed key in software
   - Optionally re-seal (or keep cached for session)
   - Zeroize unsealed key on drop

4. Add session management
   - Keep key unsealed for duration of session
   - Lock/unlock API for explicit control
   - Auto-lock on timeout (configurable)

**Deliverables:**
- `TpmDidSigner` implementing `DidSigner`
- Session management for unsealed keys
- Integration tests with sign operations
- Performance benchmarks (seal/unseal overhead)

**Testing:**
- Sign messages with TPM-backed key
- Verify signatures with public key
- Test session caching (unseal once, sign many)
- Test auto-lock timeout

### Phase 3: TpmBackend Integration

**Tasks:**
1. Refactor `TpmBackend` to use real sealing
   ```rust
   impl KeyStoreBackend for TpmBackend {
       fn init(&mut self, passphrase: &[u8]) -> Result<()> {
           // Generate new Ed25519 keypair
           // Seal private key to TPM
           // Store sealed data to disk
           // Return DidKey::Hardware with public key
       }

       fn unlock(&mut self, passphrase: &[u8]) -> Result<()> {
           // Load sealed data from disk
           // Unseal key from TPM (PCR verification)
           // Cache unsealed key in TpmDidSigner
       }

       fn signing_backend(&self) -> Option<&dyn DidSigner> {
           // Return TpmDidSigner instance
       }
   }
   ```

2. Add persistent storage
   - Store sealed data blob to `~/.icn/tpm-sealed.dat`
   - Store TPM handle/context info
   - Handle multiple identities (one sealed blob per identity)

3. Add configuration
   ```toml
   [identity.tpm]
   enabled = true
   pcrs = [0, 7]  # Boot integrity measurements
   device = "/dev/tpmrm0"
   auto_lock_timeout_secs = 300
   ```

4. Error handling
   - TPM not available → fallback to software or error
   - PCR mismatch → require manual intervention
   - Sealed data corruption → recovery via backup

**Deliverables:**
- Complete `TpmBackend` implementation
- Configuration file support
- Persistent storage for sealed data
- Error handling and recovery procedures

**Testing:**
- End-to-end: init → unlock → sign → verify
- Persistence: seal → restart → unseal
- PCR binding: seal → modify PCR → unseal fails
- Multiple identities (if supported)

### Phase 4: Platform Binding & Attestation

**Tasks:**
1. Implement PCR measurement
   - Read TPM PCRs during sealing
   - Store expected PCR values with sealed data
   - Verify PCRs match during unsealing

2. Add attestation support
   ```rust
   pub struct TpmAttestation {
       pub pcr_values: HashMap<u32, Vec<u8>>,
       pub quote: Vec<u8>,  // TPM quote signature
       pub quote_sig: Vec<u8>,
       pub attestation_key: Vec<u8>,
   }
   
   fn attest_identity(tpm: &TpmContext, did: &Did) -> Result<TpmAttestation>
   ```
   - Generate TPM quote over PCRs
   - Sign quote with attestation key
   - Return attestation proof

3. Add verification API
   ```rust
   fn verify_attestation(
       attestation: &TpmAttestation,
       expected_pcrs: &HashMap<u32, Vec<u8>>,
   ) -> Result<()>
   ```

4. Document attestation flow
   - When to request attestation
   - How to verify attestation
   - Trust model for attestation keys

**Deliverables:**
- TPM attestation generation
- Attestation verification
- Documentation for attestation usage
- Integration with network handshake (optional)

**Testing:**
- Generate attestation for TPM-sealed identity
- Verify attestation with expected PCRs
- Test attestation with modified PCRs (should fail)

### Phase 5: Production Hardening

**Tasks:**
1. Add comprehensive error handling
   - TPM hardware failures
   - PCR measurement changes
   - Sealed data corruption
   - Resource exhaustion

2. Add monitoring & observability
   - Metrics: seal/unseal operations, latency
   - Logging: TPM operations, PCR values
   - Alerts: PCR mismatches, unsealing failures

3. Add recovery procedures
   - Backup sealed data (encrypted)
   - Emergency unsealing procedure
   - PCR update workflow (BIOS updates)
   - Identity migration (old TPM → new TPM)

4. Security audit
   - Code review for key material handling
   - Verify zeroization of unsealed keys
   - Test for timing attacks (constant-time operations)
   - Fuzz testing for malformed sealed data

5. Performance optimization
   - Minimize unseal operations (session caching)
   - Batch signing operations when possible
   - Profile TPM command latency

**Deliverables:**
- Production-ready error handling
- Comprehensive documentation
- Monitoring integration
- Recovery procedures
- Security audit report

**Testing:**
- Fault injection (TPM failures, PCR changes)
- Load testing (many sign operations)
- Recovery testing (backup restore)
- Security testing (fuzzing, timing attacks)

### Phase 6: Documentation & Examples

**Tasks:**
1. Write setup guide
   - Hardware requirements (TPM 2.0)
   - Linux kernel configuration
   - TPM software stack installation
   - swtpm setup for testing

2. Write usage guide
   - Initializing TPM-backed identity
   - Signing operations
   - PCR management
   - Backup and recovery

3. Write migration guide
   - Age-encrypted → TPM-sealed migration
   - TPM → TPM migration (hardware replacement)
   - Emergency recovery procedures

4. Add examples
   - Simple TPM identity creation
   - Signing and verification
   - Attestation generation
   - Configuration examples

**Deliverables:**
- `docs/tpm-setup.md` - Setup guide
- `docs/tpm-usage.md` - Usage guide
- `docs/tpm-migration.md` - Migration guide
- `examples/tpm-identity.rs` - Example code
- Configuration templates

## Dependencies

### Rust Crates
- `tss-esapi` (^7.4) - TPM2 Software Stack bindings
- `zeroize` (^1.7) - Secure memory wiping

### System Requirements
- TPM 2.0 hardware (or swtpm for testing)
- Linux kernel 4.12+ with TPM 2.0 support
- `/dev/tpmrm0` device (TPM resource manager)

### Optional for Development
- `swtpm` - Software TPM simulator for CI
- `tpm2-tools` - Command-line TPM utilities
- `ibmswtpm2` - IBM TPM 2.0 simulator

## Security Considerations

### Threat Model

**Attacks TPM Sealing Mitigates:**
- ✅ Memory extraction (keys sealed in TPM)
- ✅ Offline key extraction (PCR binding)
- ✅ Key material in swap (never in memory long)
- ✅ Cold boot attacks (keys not in DRAM)

**Attacks TPM Sealing Does NOT Mitigate:**
- ❌ Runtime attacks (after unsealing)
- ❌ TPM physical access (with advanced tools)
- ❌ Supply chain attacks (compromised TPM)
- ❌ Side-channel attacks on unsealing

### Best Practices

1. **PCR Selection**
   - PCR 0: BIOS/UEFI firmware
   - PCR 7: Secure Boot policy
   - Avoid PCRs that change frequently

2. **Key Material Handling**
   - Zeroize unsealed keys immediately after use
   - Minimize time key is unsealed
   - Use session caching sparingly

3. **Backup Strategy**
   - Encrypt sealed data backup
   - Store backup off-system
   - Test recovery procedures

4. **PCR Management**
   - Document expected PCR values
   - Test unsealing before BIOS updates
   - Have recovery procedure for PCR changes

## Testing Strategy

### Unit Tests
- TPM context initialization
- Key sealing/unsealing
- PCR binding
- Error handling

### Integration Tests
- End-to-end signing flow
- Persistence across restarts
- Multiple identity management
- Session caching

### System Tests
- Real TPM hardware testing
- Performance benchmarks
- Fault injection
- Security testing

### CI/CD
- Use swtpm in CI for automated testing
- Manual hardware testing on release
- Security scanning (fuzzing, static analysis)

## Success Criteria

1. **Functionality**
   - ✅ Keys sealed to TPM 2.0 hardware
   - ✅ PCR binding for platform integrity
   - ✅ Signing operations work correctly
   - ✅ Attestation generation

2. **Performance**
   - ✅ Seal operation < 1 second
   - ✅ Unseal operation < 1 second
   - ✅ Sign operation < 100ms (after unseal)
   - ✅ Session caching reduces unseal overhead

3. **Security**
   - ✅ Keys never in cleartext on disk
   - ✅ PCR mismatch prevents unsealing
   - ✅ Zeroization of unsealed keys
   - ✅ Constant-time operations

4. **Reliability**
   - ✅ Graceful handling of TPM failures
   - ✅ Recovery from sealed data corruption
   - ✅ Migration support for hardware changes
   - ✅ Comprehensive error messages

5. **Usability**
   - ✅ Clear documentation
   - ✅ Simple configuration
   - ✅ Good error messages
   - ✅ Example code

## Timeline Estimate

- Phase 1: Core Sealing/Unsealing - 2 weeks
- Phase 2: TpmDidSigner - 1 week
- Phase 3: Backend Integration - 1 week
- Phase 4: Attestation - 1 week
- Phase 5: Production Hardening - 2 weeks
- Phase 6: Documentation - 1 week

**Total: ~8 weeks** (with one developer)

## Current Status

**Phase 0: Scaffolding** ✅ Complete
- DidSigner trait defined
- DidKey enum defined
- TpmBackend placeholder exists
- Feature flag `tpm-experimental` in place

**Next: Phase A (Prerequisite)** 🔄 In Progress
- Refactor IdentityBundle to use DidKey
- Remove embedded KeyPair from IdentityBundle
- Enable hardware backend support

**Next: Phase 1** ⏳ Not Started
- Implement real TPM sealing/unsealing
- Depends on Phase A completion

## Open Questions

1. **PCR Selection:** Which PCRs should we bind to by default?
   - Recommendation: PCR 0 (firmware) + PCR 7 (secure boot)

2. **Session Management:** How long should we cache unsealed keys?
   - Recommendation: 5 minutes default, configurable

3. **Backup Strategy:** How should users backup TPM-sealed keys?
   - Recommendation: Encrypted backup with recovery passphrase

4. **Migration:** How to migrate between TPM hardware?
   - Recommendation: Export/import with intermediate encryption

5. **Attestation:** Should attestation be required for network handshake?
   - Recommendation: Optional, policy-driven

## References

- [TPM 2.0 Specification](https://trustedcomputinggroup.org/resource/tpm-library-specification/)
- [tss-esapi Documentation](https://docs.rs/tss-esapi/latest/tss_esapi/)
- [TCG TPM2 Software Stack](https://github.com/tpm2-software/tpm2-tss)
- [Intel TPM2 Tools](https://github.com/tpm2-software/tpm2-tools)
- [swtpm Software TPM](https://github.com/stefanberger/swtpm)

## Pathway from Current Scaffolding

The current TPM scaffolding (marked `tpm-experimental`) provides:
1. Backend trait abstraction ✅
2. Basic structure (TpmBackend, TpmSigningBackend) ✅
3. Placeholder implementations (all bail!) ✅
4. Feature flag isolation ✅

**To reach Phase 1, we need:**
1. IdentityBundle refactor (Phase A) - **In Progress**
2. Replace placeholder sealing with real tss-esapi calls
3. Implement PCR binding logic
4. Add persistent storage for sealed data
5. Handle TPM initialization errors gracefully

**This document serves as the roadmap from scaffolding to production.**
