# TPM 2.0 Setup Guide

This guide explains how to configure ICN to use Trusted Platform Module (TPM) 2.0 for key storage and platform binding.

## Overview

ICN supports TPM 2.0 for secure key storage with platform binding. TPMs provide:

- **Platform binding**: Keys sealed to specific hardware/firmware state
- **Attestation**: Cryptographic proof of platform integrity
- **Tamper detection**: Keys unusable if platform state changes
- **Hardware security**: Dedicated security chip on motherboard
- **Cost-effective**: TPM chips included in most modern servers/laptops

## TPM 2.0 vs HSM

| Feature | TPM 2.0 | HSM (PKCS#11) |
|---------|---------|---------------|
| **Hardware location** | Motherboard chip | External device/USB/PCI |
| **Key operations/sec** | 10-50 | 1000+ |
| **Platform binding** | Yes | No |
| **Attestation** | Yes | Limited |
| **Physical security** | Moderate | High |
| **Cost** | $0 (included) | $500-$10,000+ |
| **Use case** | Single-node binding | High-volume signing |

**When to use TPM:**
- Single-node deployments
- Platform integrity is critical
- Cost-sensitive deployments
- Low signing volume (<10 ops/sec)

**When to use HSM:**
- Multi-node clusters
- High signing volume
- Regulatory compliance (FIPS)
- Physical security is critical

## Prerequisites

1. **TPM 2.0 chip**:
   - Most servers manufactured after 2016
   - Verify: `ls /dev/tpm*` should show `/dev/tpm0` or `/dev/tpmrm0`

2. **TPM 2.0 tools**:
   ```bash
   # Ubuntu/Debian
   sudo apt-get install tpm2-tools tpm2-abrmd

   # Fedora/RHEL
   sudo dnf install tpm2-tools tpm2-abrmd

   # Arch
   sudo pacman -S tpm2-tools tpm2-abrmd
   ```

3. **TPM Resource Manager**:
   ```bash
   # Enable tpm2-abrmd
   sudo systemctl enable tpm2-abrmd
   sudo systemctl start tpm2-abrmd
   ```

4. **ICN compiled with TPM support**:
   ```bash
   cd icn
   cargo build --release --features tpm
   ```

## Verify TPM Availability

```bash
# Check TPM device
ls -l /dev/tpm*

# Expected output:
# crw-rw---- 1 tss tss 10, 224 Jan 20 10:00 /dev/tpm0
# crw-rw---- 1 tss tss 10, 225 Jan 20 10:00 /dev/tpmrm0

# Test TPM communication
tpm2_getcap properties-fixed

# Should show TPM manufacturer, version, etc.
```

## Setup Steps

### 1. Clear TPM (Optional)

**⚠️ WARNING**: This erases all TPM data. Only do this on new systems or if resetting.

```bash
# Clear TPM (requires physical presence on many systems)
tpm2_clear -c p

# May require BIOS confirmation on reboot
```

### 2. Take Ownership

Set TPM owner password (required for some operations):

```bash
# Set owner password
tpm2_changeauth -c owner newownerpassword

# Set endorsement password  
tpm2_changeauth -c endorsement newendorsementpass

# Set lockout password
tpm2_changeauth -c lockout newlockoutpass
```

**Important**: Store these passwords securely (e.g., in a password manager).

### 3. Configure ICN

Create or update `icn.toml`:

```toml
[identity]
# Use TPM 2.0 backend
backend = "tpm"

[identity.tpm]
# TPM device path (use tpmrm0 for resource manager)
device_path = "/dev/tpmrm0"

# Persistent handle for the key (must be in persistent range: 0x81000000 - 0x81FFFFFF)
key_handle = 0x81000001

# Enable platform binding (seal key to PCR values)
platform_binding = true

# Enable attestation support
attestation = true

# PCR indices to bind to (comma-separated)
# PCR 0: BIOS/UEFI code
# PCR 1: BIOS/UEFI config
# PCR 7: Secure Boot state
pcr_indices = "0,1,7"
```

### 4. Initialize Identity

```bash
# Initialize new identity in TPM
icnctl id init --backend tpm

# ICN generates keypair and seals to TPM
✓ Generated Ed25519 keypair
✓ Sealed key to TPM (bound to PCRs 0,1,7)
✓ Created identity bundle
✓ DID: did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
```

### 5. Verify TPM Usage

```bash
# Check identity status
icnctl id show

# Output should show:
Backend: tpm
Hardware-backed: yes
Device: /dev/tpmrm0
Key handle: 0x81000001
Platform binding: enabled
PCR-bound: 0,1,7
Attestation: enabled
```

### 6. Start ICN Daemon

```bash
# Start with TPM backend
icnd --config icn.toml

# TPM will unseal key (verifying PCR values)
✓ Unlocked TPM backend
✓ Unsealed key from TPM
✓ Verified platform state (PCRs match)
✓ Loaded identity: did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
```

## Platform Binding

### Understanding PCR Binding

Platform Configuration Registers (PCRs) measure platform state:

| PCR | Measures | Security Impact |
|-----|----------|-----------------|
| 0 | BIOS/UEFI code | High - detects firmware changes |
| 1 | BIOS/UEFI config | High - detects config tampering |
| 2 | Option ROM code | Medium - detects device changes |
| 3 | Option ROM config | Medium |
| 4 | Boot loader | High - detects boot tampering |
| 5 | Boot config | High |
| 6 | Resume from sleep | Low |
| 7 | Secure Boot state | High - detects boot bypass |
| 8-15 | OS/bootloader use | Varies |

### Choosing PCR Indices

**Conservative (tightest binding)**:
```toml
pcr_indices = "0,1,2,3,4,5,7"
```
- Key unusable after firmware update
- Key unusable after hardware changes
- Maximum security, minimum flexibility

**Balanced (recommended)**:
```toml
pcr_indices = "0,1,7"
```
- Key survives firmware updates (with TPM clear)
- Key survives minor hardware changes
- Good security, reasonable flexibility

**Permissive (loosest binding)**:
```toml
pcr_indices = "7"
```
- Only binds to Secure Boot state
- Key survives most system changes
- Minimum security, maximum flexibility

### Reading PCR Values

```bash
# Read all PCR values
tpm2_pcrread

# Read specific PCRs
tpm2_pcrread sha256:0,1,7

# Example output:
#   sha256:
#     0 : 0x3D6772B4F84ED47595D72A2C4C5FBE8F...
#     1 : 0x5A7E92A1C2B3D4E5F6A7B8C9D0E1F2A3...
#     7 : 0x0000000000000000000000000000000000...
```

## Attestation

### Generate Attestation Quote

Prove platform integrity to remote verifiers:

```rust
use icn_identity::{TpmBackend, TpmConfig};

let config = TpmConfig {
    device_path: "/dev/tpmrm0".to_string(),
    key_handle: 0x81000001,
    platform_binding: true,
    attestation: true,
};

let mut backend = TpmBackend::new(config)?;
backend.unlock(b"")?;

// Generate attestation quote
let quote = backend.generate_attestation()?;

// Quote contains:
// - Signed PCR values
// - TPM-signed statement
// - Attestation key certificate
```

### Verify Attestation

Remote verifiers can check:

1. **TPM authenticity**: Verify endorsement key certificate
2. **Platform state**: Check PCR values against expected values
3. **Quote signature**: Verify TPM signed the quote
4. **Freshness**: Check quote timestamp/nonce

```bash
# Verify quote (remote verifier)
icnctl attestation verify \
  --quote quote.bin \
  --pcr-policy expected-pcrs.json \
  --ek-cert tpm-ek-cert.pem
```

## Key Operations

### Sealing Keys to TPM

Keys are sealed during initialization:

```rust
// During init, key is sealed to PCR values
let mut backend = TpmBackend::new(config)?;
let bundle = backend.init(b"")?;

// Key is now sealed to TPM and can only be unsealed
// when PCR values match those at sealing time
```

### Unsealing Keys

Keys are automatically unsealed when unlocking:

```rust
// Unlock unseals key (verifies PCRs)
backend.unlock(b"")?;

// If PCRs don't match sealing values, unlock fails
```

### Signing Operations

```rust
// Get signing backend
let signing_backend = backend.signing_backend()?;

// Sign message (using TPM-unsealed key)
let message = b"transaction data";
let signature = signing_backend.sign(message)?;
```

## Handling Platform Changes

### Firmware Updates

After firmware updates, PCRs will change:

```bash
# Before update: backup identity info (DID only)
icnctl id export --did-only > identity.json

# After update: PCR values changed
icnd --config icn.toml
# Error: Failed to unseal key (PCR mismatch)

# Solution 1: Update PCR bindings
icnctl id update-pcrs --extend

# Solution 2: Re-seal to new PCRs
icnctl id reseal --verify-identity identity.json
```

### Hardware Changes

If bound to hardware PCRs (2,3), hardware changes break unsealing:

```bash
# After hardware change
icnd --config icn.toml
# Error: Failed to unseal key (PCR mismatch)

# Solution: Re-initialize with new hardware
# (requires backup/recovery procedure)
```

## Security Best Practices

### 1. PCR Selection

**DO:**
- Bind to firmware PCRs (0,1) for tamper detection
- Bind to Secure Boot (7) to prevent bootkit attacks
- Document your PCR policy
- Test unsealing after updates

**DON'T:**
- Bind to volatile PCRs (6)
- Bind to all PCRs (too brittle)
- Skip PCR binding entirely (defeats purpose)

### 2. Access Control

**DO:**
- Run ICN daemon as dedicated user
- Restrict TPM device permissions: `chown tss:tss /dev/tpm*`
- Use TPM resource manager (`/dev/tpmrm0`)
- Enable TPM owner password

**DON'T:**
- Run as root unnecessarily
- Allow world-readable TPM device
- Share TPM between untrusted applications

### 3. Backup and Recovery

**DO:**
- Export DID and public keys (not private key!)
- Document PCR policy
- Test recovery procedures
- Keep endorsement key certificate

**DON'T:**
- Export TPM-sealed blobs to untrusted storage
- Store unsealed keys anywhere
- Skip backup procedures

### 4. Monitoring

Monitor for:

```bash
# Failed unseal attempts (potential tampering)
journalctl -u icnd | grep "Failed to unseal"

# PCR changes (unexpected firmware updates)
tpm2_pcrread | tee current-pcrs.txt
diff expected-pcrs.txt current-pcrs.txt

# TPM errors
tpm2_eventlog /sys/kernel/security/tpm0/binary_bios_measurements
```

## Troubleshooting

### TPM device not found

```
Error: Failed to connect to TPM device
```

**Solution:**
```bash
# Check TPM is enabled in BIOS
# Verify device exists
ls -l /dev/tpm*

# Check permissions
sudo chmod 660 /dev/tpm0
sudo chown tss:tss /dev/tpm0

# Start resource manager
sudo systemctl restart tpm2-abrmd
```

### PCR mismatch on unseal

```
Error: Failed to unseal key (PCR mismatch)
```

**Solution:**
```bash
# Check current PCR values
tpm2_pcrread sha256:0,1,7

# Compare to expected values
# If firmware updated: extend PCR policy
icnctl id update-pcrs

# If hardware changed: may need to re-initialize
```

### TPM ownership required

```
Error: TPM ownership required
```

**Solution:**
```bash
# Take ownership
tpm2_changeauth -c owner newpassword

# Configure ICN with owner password
[identity.tpm]
owner_password = "newpassword"
```

### Performance issues

TPM operations are slower than software:

- **Key generation**: 1-5 seconds
- **Signing**: 50-200ms per operation
- **Unsealing**: 100-500ms

For high-throughput scenarios:
1. Cache unsealed key in memory
2. Batch signing operations
3. Consider HSM for >100 ops/sec

## TPM Simulator for Development

For development without TPM hardware:

```bash
# Install TPM simulator
git clone https://github.com/stefanberger/swtpm
cd swtpm
./autogen.sh --prefix=/usr
make
sudo make install

# Start simulator
mkdir /tmp/myvtpm
swtpm socket --tpmstate dir=/tmp/myvtpm \
  --ctrl type=tcp,port=2322 \
  --server type=tcp,port=2321 \
  --flags not-need-init

# Configure ICN for simulator
[identity.tpm]
device_path = "swtpm:host=127.0.0.1,port=2321"
```

## Integration with Secure Boot

For complete platform integrity:

1. **Enable Secure Boot** in BIOS
2. **Configure shim/bootloader** to measure boot chain
3. **Bind keys to PCR 7** (Secure Boot state)
4. **Verify TPM eventlog** matches expected boot sequence

```bash
# Check Secure Boot status
mokutil --sb-state

# Read TPM event log
tpm2_eventlog /sys/kernel/security/tpm0/binary_bios_measurements

# Verify boot chain measurements
tpm2_pcrread sha256:4,5,7
```

## Compliance

### TCG (Trusted Computing Group)

TPM 2.0 compliance:
- Implements TPM 2.0 specification
- Supports SHA-256 PCR banks
- Provides endorsement key (EK) certificate

### FIPS 140-2

Some TPM modules are FIPS 140-2 certified:
- Check TPM manufacturer's certification status
- Enable FIPS mode in BIOS if available
- Use FIPS-approved algorithms only

## Migration from Age Keystore

To migrate from Age to TPM:

```bash
# 1. Export existing identity
icnctl id export --did-only > identity.json

# 2. Initialize TPM backend
icnctl id init --backend tpm

# 3. Update network with new key
icnctl id rotate --publish
```

## Support

For TPM-related issues:

- **TPM 2.0 Specification**: https://trustedcomputinggroup.org/
- **tpm2-tools**: https://github.com/tpm2-software/tpm2-tools
- **ICN Issues**: https://github.com/InterCooperative-Network/icn/issues

## References

- TPM 2.0 Library Specification: https://trustedcomputinggroup.org/resource/tpm-library-specification/
- TCG PC Client Platform TPM Profile: https://trustedcomputinggroup.org/resource/pc-client-specific-platform-tpm-profile-for-tpm-2-0/
- Linux TPM2 Software Stack: https://github.com/tpm2-software
- Secure Boot and TPM: https://wiki.archlinux.org/title/Unified_Extensible_Firmware_Interface/Secure_Boot
