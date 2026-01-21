# IdentityBundle Refactor Plan

**Status:** In Progress  
**Goal:** Replace `KeyPair` with `DidKey` enum in `IdentityBundle` to enable hardware-backed signing

## Overview

The current `IdentityBundle` embeds a `KeyPair` which assumes software-based private key material exists in memory. This blocks hardware backends (HSM/TPM) which never expose private keys.

## Changes Required

### Phase A1: Update IdentityBundle Core

**File:** `icn-identity/src/bundle.rs`

1. **Replace field:**
   ```rust
   // OLD:
   did_keypair: KeyPair,
   
   // NEW:
   did_key: DidKey,
   ```

2. **Update constructors:**
   - `generate()` - Create software DidKey
   - `from_keypair()` - Convert KeyPair to software DidKey  
   - `from_stored()` - Reconstruct software DidKey from bytes
   - Add new: `from_did_key()` - Direct construction

3. **Update accessors:**
   - Keep `did()` - unchanged
   - Update `keypair()` - deprecate or return only for software keys
   - Add `did_key()` - return reference to DidKey
   - Add `sign()` - delegate to DidKey or separate signer

4. **Update signing:**
   - Current: Uses `did_keypair.sign()`
   - New: Handle both software and hardware paths
   - For software: Sign directly with secret bytes
   - For hardware: Require external signer implementation

### Phase A2: Update Keystore Integration

**File:** `icn-identity/src/keystore.rs`

1. **Update load methods** to create DidKey instead of KeyPair:
   - `load_v4()` line 936: `from_stored_with_kem()` 
   - `load_v3()` line 1020: `from_stored()`
   - `load_v2()` line 1086: `from_stored()`

2. **Keep backward compatibility:**
   - Still store secret bytes for software keys
   - Don't break existing keystores

### Phase A3: Update Downstream Usages

**Files to check:**
- `icn-identity/src/keystore_pkcs11.rs` - HSM backend
- `icn-identity/src/keystore_tpm.rs` - TPM backend  
- `icn-gateway/src/auth.rs` - Authentication
- `icn-gateway/src/api/*.rs` - API endpoints
- `icn-snapshot/src/*.rs` - State snapshots

**Common pattern:**
```rust
// OLD:
bundle.keypair().sign(message)

// NEW - Software:
bundle.sign(message)?  // or bundle.did_key().sign_software(message)?

// NEW - Hardware (requires signer):
signer.sign(message)?  // where signer implements DidSigner
```

### Phase A4: Testing

1. **Update existing tests:**
   - All tests in `bundle.rs`
   - Keystore tests
   - Gateway integration tests

2. **Add new tests:**
   - Software DidKey creation
   - Hardware DidKey creation (mock)
   - Signing with software keys
   - Error handling for hardware keys without signer

## Migration Path

### For Software Keys (Current Default)

No visible change to users. Backend converts KeyPair → DidKey automatically.

### For Hardware Keys (Future)

Will require:
1. Backend-specific initialization (HSM/TPM)
2. External signer implementation
3. Explicit "hardware mode" flag

## Follow-Up Issues

### Issue 1: Add Signing Backend to IdentityBundle

**Problem:** IdentityBundle needs optional signing backend  
**Solution:** Add `Option<Box<dyn DidSigner>>` field

### Issue 2: PKCS#11 Backend Integration

**Problem:** PKCS#11 backend needs to create hardware DidKey  
**Solution:** Update `init()` to return DidKey::Hardware

### Issue 3: TPM Backend Integration

**Problem:** TPM backend needs to create hardware DidKey  
**Solution:** Implement sealing then create DidKey::Hardware

### Issue 4: Gateway Authentication with Hardware Keys

**Problem:** Auth middleware uses `bundle.keypair().sign()`  
**Solution:** Use `bundle.sign()` or injected signer

### Issue 5: State Snapshot with Hardware Keys

**Problem:** Snapshots may try to serialize private keys  
**Solution:** Only serialize public keys for hardware mode

## Success Criteria

- [x] IdentityBundle uses DidKey instead of KeyPair
- [x] All tests pass
- [x] No breaking changes to software key flow
- [x] Hardware keys are supported (preparation)
- [x] Documentation updated
- [ ] Follow-up issues created

## Timeline

**Estimated:** 1-2 commits, ~2-3 hours

## Security Considerations

1. **Backward Compatible:** Existing software keystores continue working
2. **No Dummy Keys:** Hardware keys cannot be misused as software keys
3. **Type Safety:** Compiler prevents accessing non-existent secrets
4. **Zeroization:** Software key material remains zeroized on drop

## References

- TPM Implementation Plan: `docs/tpm-implementation-plan.md`
- DidSigner trait: `icn-identity/src/did_signer.rs`
- Current KeyPair: `icn-identity/src/keybundle.rs`
