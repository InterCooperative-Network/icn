# Identity Backend Configuration

This document describes how to configure ICN's identity keystore backend selection.

## Overview

ICN supports multiple keystore backends for identity storage:

- **Age** (default): Software-based encryption using the Age encryption format
- **PKCS#11** (scaffolding): Hardware Security Module (HSM) support
- **TPM 2.0** (scaffolding): Trusted Platform Module support

## Configuration

Backend selection is configured in the `[identity]` section of your `icnd` config file (commonly `config.toml`):

```toml
[identity]
backend = "age"  # Options: "age", "pkcs11", "tpm"
```

### Age Backend (Default)

The Age backend stores encrypted keys in a file at `{data_dir}/identity.age` (for default `data_dir`, typically `~/.local/share/icn/identity.age` on Linux). This is the recommended backend for most users and is the most mature backend in current ICN deployments.

**Configuration:**

```toml
[identity]
backend = "age"
```

No additional configuration required. The keystore file location is determined by the `data_dir` setting.

**Features:**
- ✅ Mature and widely used in current deployments
- ✅ Cross-platform
- ✅ Simple deployment
- ✅ Passphrase protection
- ✅ Key rotation support

### PKCS#11 Backend (Scaffolding Only)

**⚠️ NOT PRODUCTION READY - SCAFFOLDING ONLY ⚠️**

The PKCS#11 backend provides integration with Hardware Security Modules (HSMs) via the PKCS#11 standard. This backend is currently scaffolding only and will refuse to operate at runtime.

**Status:** Issue #744 tracks full implementation. The backend requires `IdentityBundle` refactoring to support hardware-backed keys with `DidKey::Hardware`.

**Example Configuration (non-functional):**

```toml
[identity]
backend = "pkcs11"

[identity.pkcs11]
library_path = "/usr/lib/softhsm/libsofthsm2.so"
slot_id = 0
key_label = "icn-identity"
token_label = "softhsm"  # Optional
```

**Configuration Options:**
- `library_path`: Path to PKCS#11 library (e.g., SoftHSM, YubiKey)
- `slot_id`: HSM slot number
- `key_label`: Label for the keypair in the HSM
- `token_label`: Optional token label filter

**Attempting to use this backend will result in:**
```
Error: PKCS#11 backend is scaffolding only and not yet functional.
IdentityBundle must be hardware-keyed before enabling.
```

### TPM 2.0 Backend (Scaffolding Only)

**⚠️ NOT PRODUCTION READY - EXPERIMENTAL SCAFFOLDING ONLY ⚠️**

The TPM backend seals keys to a Trusted Platform Module 2.0, providing hardware-based key protection and platform binding. This backend is currently scaffolding only and will refuse to operate at runtime.

**Status:** Issue #745 tracks full implementation. The backend requires key sealing/unsealing implementation and `IdentityBundle` refactoring for hardware-backed keys.

**Example Configuration (non-functional):**

```toml
[identity]
backend = "tpm"

[identity.tpm]
device_path = "/dev/tpmrm0"
key_handle = 0x81000000
platform_binding = true
attestation = false
```

**Configuration Options:**
- `device_path`: TPM device path (default: `/dev/tpmrm0`)
- `key_handle`: Persistent handle for the key (must be in range 0x81000000-0x81FFFFFF)
- `platform_binding`: Seal key to platform PCRs
- `attestation`: Enable TPM attestation support

**Attempting to use this backend will result in:**
```
Error: TPM backend is scaffolding only and not yet functional.
Key sealing/unsealing + hardware-keyed IdentityBundle required.
```

## Backend Selection at Runtime

The backend is selected when `icnd` starts:

1. Configuration is loaded from the file passed to `--config` (or from runtime defaults if `--config` is omitted)
2. The `identity.backend` value is validated
3. The appropriate backend is initialized
4. For Age: keystore file is opened
5. For hardware backends: clear error is returned

Backend selection is logged during startup:
```
INFO Identity keystore found at: "/home/user/.local/share/icn/identity.age"
INFO Using identity backend: age
```

## Migration

Currently, only the Age backend is functional, so no migration is needed. When hardware backends become available:

1. The Age keystore will remain the default
2. Users can opt-in to hardware backends by changing configuration
3. Migration tools will be provided to transfer keys to hardware
4. Key rotation can be used to switch backends

## Security Considerations

### Age Backend
- **Passphrase strength:** Use a strong passphrase (32+ characters recommended)
- **Environment variables:** Set `ICN_KEYSTORE_PASSPHRASE` for automated deployments
- **File permissions:** Ensure `identity.age` is readable only by the ICN process owner
- **Backup:** Securely backup the keystore file and passphrase

### Hardware Backends (Future)
- **PIN security:** PINs should be strong and stored securely (not in config files)
- **Physical security:** HSMs and TPMs provide tamper resistance
- **Platform binding:** TPM keys are bound to specific hardware
- **Attestation:** TPM attestation proves key origin and platform state

## Validation

Configuration validation runs on startup. Common errors:

**Unknown backend:**
```
Error: Unknown identity backend 'foo' (valid: age, pkcs11, tpm)
```

**Missing hardware backend config:**
```
Error: identity.backend=pkcs11 requires [identity.pkcs11] config section
```

**Invalid TPM handle:**
```
Error: Invalid key_handle 0x1234 (must be in range 0x81000000-0x81FFFFFF)
```

Use `icnd --validate-config` to check configuration without starting the daemon:

```bash
$ icnd --config /path/to/config.toml --validate-config
Validating configuration...

✓ Configuration is valid

Warnings:
  ⚠ TPM backend is scaffolding only and not yet functional.
    Key sealing/unsealing + hardware-keyed IdentityBundle required.
```

## Development

### Adding a New Backend

To add a new keystore backend:

1. Implement the backend in `icn-identity/src/keystore_<name>.rs`
2. Add backend configuration to `icn-core/src/config/identity.rs`
3. Update the factory in `icn-identity/src/backend_factory.rs`
4. Add feature flag if needed
5. Update validation logic
6. Add tests
7. Document the backend

### Testing Hardware Backends

Hardware backends can be tested with:
- **SoftHSM:** Software HSM for PKCS#11 testing
- **TPM Simulator:** Software TPM 2.0 for development

See issue #744 (PKCS#11) and #745 (TPM) for implementation details.

## Future Work

Tracked in issues:
- #744: Complete PKCS#11 HSM backend implementation
- #745: Complete TPM 2.0 backend implementation
- #741: HSM/TPM Backend Implementation (parent issue)

Key requirements:
- [ ] Refactor `IdentityBundle` to support `DidKey::Hardware`
- [ ] Implement hardware signing delegation
- [ ] Add key migration tools
- [ ] Comprehensive hardware backend testing
- [ ] Security audit for hardware integration
- [ ] Production deployment guide

## References

- [Age encryption specification](https://age-encryption.org/)
- [PKCS#11 standard](https://docs.oasis-open.org/pkcs11/pkcs11-base/v3.0/pkcs11-base-v3.0.html)
- [TPM 2.0 specification](https://trustedcomputinggroup.org/resource/tpm-library-specification/)
- [ICN Identity Architecture](../../ARCHITECTURE.md#identity-layer)
