# TPM 2.0 Setup Guide

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

This guide explains how to set up and use the TPM 2.0 backend for ICN identity management.

## Overview

The ICN TPM backend seals Ed25519 private keys to TPM 2.0 hardware, providing hardware-backed security for identity management. Since TPM 2.0 does not natively support Ed25519, we use TPM for sealed storage and perform signing operations in software with unsealed key material.

## Security Properties

- **Hardware-backed storage**: Private keys are sealed to TPM
- **Platform binding**: Keys can be bound to PCR values (optional)
- **Secure unsealing**: Keys can only be unsealed on the same platform
- **Zeroization**: Unsealed keys are zeroized after use

## Requirements

### Hardware Requirements

- TPM 2.0 chip (or software TPM simulator)
- Linux kernel 4.12+ with TPM 2.0 support
- TPM resource manager (`/dev/tpmrm0` or `/dev/tpm0`)

### Software Requirements

```bash
# Ubuntu/Debian
sudo apt-get install -y \
    libtss2-dev \
    tpm2-tools \
    tpm2-abrmd

# Fedora/RHEL
sudo dnf install -y \
    tss2-devel \
    tpm2-tools \
    tpm2-abrmd
```

## Using Software TPM (swtpm) for Testing

For development and testing without hardware TPM, use swtpm:

### Installation

```bash
# Ubuntu/Debian
sudo apt-get install -y swtpm swtpm-tools

# Fedora/RHEL
sudo dnf install -y swtpm swtpm-tools

# macOS
brew install swtpm
```

### Running swtpm

```bash
# Create TPM state directory
mkdir -p /tmp/tpm-state

# Start swtpm
swtpm socket \
    --tpmstate dir=/tmp/tpm-state \
    --tpm2 \
    --ctrl type=tcp,port=2322 \
    --server type=tcp,port=2321 \
    --flags not-need-init

# In another terminal, initialize the TPM
tpm2_startup -c -T swtpm:host=localhost,port=2321
```

## Building ICN with TPM Support

The TPM backend is gated behind the `tpm-experimental` feature flag:

```bash
cd icn
cargo build --features tpm-experimental
cargo test --features tpm-experimental
```

## Running Tests

### Unit Tests

```bash
cd icn
cargo test -p icn-identity --features tpm-experimental
```

### Integration Tests (Requires TPM/swtpm)

```bash
# Start swtpm first (see above)

# Run ignored tests that require TPM
cargo test -p icn-identity --features tpm-experimental -- --ignored --test-threads=1
```

Available integration tests:
- `test_tpm_seal_unseal_cycle` - Test complete seal/unseal workflow
- `test_tpm_with_pcr_binding` - Test PCR binding (when implemented)
- `test_tpm_persistent_across_restarts` - Test persistence

## Using TPM Backend in Code

```rust
use icn_identity::{TpmBackend, TpmConfig};

// Create TPM configuration
let config = TpmConfig {
    device_path: "/dev/tpmrm0".to_string(), // or "swtpm:host=localhost,port=2321"
    key_handle: 0x81000001,                 // Persistent handle
    platform_binding: true,                  // Enable PCR binding
    attestation: false,                      // Enable attestation (future)
};

// Create TPM backend
let mut backend = TpmBackend::new(config)?;

// Initialize with new identity
let bundle = backend.init(&[])?;

// Sign a message
let message = b"Hello, ICN!";
let signature = bundle.sign(message)?;

// Lock the backend
backend.lock();

// Unlock later
backend.unlock(&[])?;
```

## Current Implementation Status

### Phase 1: Basic Scaffolding ✅

- [x] Key generation
- [x] Backend trait implementation
- [x] TpmDidSigner implementation
- [x] Integration tests

### Phase 2: Real TPM Operations ✅

- [x] Actual TPM sealing with `tss-esapi::Context::create()`
- [x] Actual TPM unsealing with `tss-esapi::Context::unseal()`
- [x] Marshall/Unmarshall TPM structures for persistence
- [x] Lazy TPM context initialization
- [ ] PCR policy binding (planned)
- [ ] PCR verification on unseal (planned)
- [ ] Persistent handle management (planned)

### Phase 3: Advanced Features (Future)

- [ ] TPM attestation
- [ ] Multiple identity support
- [ ] Backup and recovery
- [ ] Migration between TPMs

## Known Limitations

**Current Phase 2 Implementation**:

- Keys are sealed to TPM hardware using real TPM2_Create/TPM2_Unseal operations
- PCR policy binding is not yet implemented - keys can be unsealed without platform integrity verification
- Persistent handle management is not yet implemented - sealed blobs are stored in files

## Troubleshooting

### TPM device not found

```bash
# Check if TPM device exists
ls -l /dev/tpm* /dev/tpmrm*

# Check TPM info
tpm2_getcap properties-fixed

# Check kernel modules
lsmod | grep tpm
```

### Permission denied

```bash
# Add user to tss group (log out and back in after this)
sudo usermod -a -G tss $USER

# Ensure TPM device is owned by tss group with restrictive permissions
# Check current permissions:
ls -l /dev/tpmrm0

# If needed, set group ownership and mode (do NOT use 666):
sudo chgrp tss /dev/tpmrm0
sudo chmod 660 /dev/tpmrm0   # Group-only access, not world-writable
```

**Security Note**: Never use `chmod 666` on TPM devices as this makes them world-readable and world-writable, allowing any local user or process to access TPM-protected keys.

### swtpm connection refused

```bash
# Check if swtpm is running
ps aux | grep swtpm

# Check port availability
netstat -tuln | grep 2321

# Restart swtpm
killall swtpm
# Then start again (see above)
```

### Build errors

```bash
# Ensure libtss2-dev is installed
pkg-config --modversion tss2-sys

# Should output version >= 2.4.6
```

## Security Considerations

### Threat Model

**Mitigated by TPM sealing:**
- ✅ Memory extraction attacks (keys sealed in TPM)
- ✅ Offline key extraction (PCR binding)
- ✅ Cold boot attacks (keys not in DRAM long-term)
- ✅ Key material in swap (never in cleartext)

**Not mitigated:**
- ❌ Runtime attacks after unsealing
- ❌ TPM physical access (advanced tools)
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
   - Maintain encrypted backups of sealed blobs
   - Store backups off-system
   - Test recovery procedures

## References

- [TPM 2.0 Specification](https://trustedcomputinggroup.org/resource/tpm-library-specification/)
- [tss-esapi Documentation](https://docs.rs/tss-esapi/latest/tss_esapi/)
- [TCG TPM2 Software Stack](https://github.com/tpm2-software/tpm2-tss)
- [swtpm Software TPM](https://github.com/stefanberger/swtpm)
- [TPM2 Tools](https://github.com/tpm2-software/tpm2-tools)
- [ICN TPM Implementation Plan](./tpm-implementation-plan.md)

## Support

For issues or questions:
1. Check troubleshooting section above
2. Review [TPM Implementation Plan](./tpm-implementation-plan.md)
3. Open an issue on GitHub with:
   - ICN version
   - TPM type (hardware/swtpm)
   - `tpm2_getcap properties-fixed` output
   - Error messages and logs
