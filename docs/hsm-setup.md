# HSM Setup Guide

This guide explains how to configure ICN to use Hardware Security Modules (HSMs) for key storage.

## Overview

ICN supports PKCS#11-compliant HSMs for production deployments where enhanced key security is required. HSMs provide:

- **Hardware key generation**: Keys are generated within the secure hardware boundary
- **Protected key storage**: Private keys never leave the HSM
- **Tamper resistance**: Physical security against key extraction
- **Audit logging**: Track all key operations
- **FIPS compliance**: Many HSMs are FIPS 140-2 Level 2+ certified

## Supported HSMs

ICN's PKCS#11 backend supports any PKCS#11-compliant HSM, including:

- **YubiHSM2**: Affordable USB HSM ($650)
- **AWS CloudHSM**: Cloud-based HSM service
- **Google Cloud HSM**: GCP's managed HSM
- **Thales Luna**: Enterprise HSM
- **Utimaco**: High-performance HSM
- **SoftHSM2**: Software HSM for testing (NOT for production)

## Prerequisites

1. **HSM device or service**:
   - Physical HSM connected to the server
   - Or cloud HSM service credentials
   - Or SoftHSM2 for testing

2. **PKCS#11 library**:
   - YubiHSM2: `yubihsm-pkcs11` library
   - CloudHSM: AWS CloudHSM client
   - SoftHSM2: `libsofthsm2.so`

3. **ICN compiled with HSM support**:
   ```bash
   cd icn
   cargo build --release --features hsm
   ```

## Setup Steps

### 1. Install HSM Software

#### YubiHSM2

```bash
# Ubuntu/Debian
sudo apt-get install yubihsm-shell yubihsm-connector

# macOS
brew install yubihsm-shell yubihsm-connector

# Start connector
yubihsm-connector -d
```

#### SoftHSM2 (Testing Only)

```bash
# Ubuntu/Debian
sudo apt-get install softhsm2

# macOS
brew install softhsm

# Initialize token
softhsm2-util --init-token --slot 0 \
  --label "icn-test" \
  --pin 1234 \
  --so-pin 5678
```

#### AWS CloudHSM

```bash
# Install CloudHSM client
wget https://s3.amazonaws.com/cloudhsmv2-software/CloudHsmClient/EL7/cloudhsm-client-latest.el7.x86_64.rpm
sudo yum install -y ./cloudhsm-client-latest.el7.x86_64.rpm

# Configure cluster
sudo /opt/cloudhsm/bin/configure -a <cluster-id>.cloudhsm.<region>.amazonaws.com
```

### 2. Configure ICN

Create or update `icn.toml`:

```toml
[identity]
# Use PKCS#11 HSM backend
backend = "pkcs11"

[identity.pkcs11]
# Path to PKCS#11 library
library_path = "/usr/lib/softhsm/libsofthsm2.so"

# HSM slot ID (use pkcs11-tool to list slots)
slot_id = 0

# Key label (unique identifier for this identity)
key_label = "icn-node-1"

# Optional: Token label
token_label = "icn-production"
```

#### YubiHSM2 Configuration

```toml
[identity.pkcs11]
library_path = "/usr/lib/x86_64-linux-gnu/pkcs11/yubihsm_pkcs11.so"
slot_id = 0
key_label = "icn-production-key"
```

#### AWS CloudHSM Configuration

```toml
[identity.pkcs11]
library_path = "/opt/cloudhsm/lib/libcloudhsm_pkcs11.so"
slot_id = 1
key_label = "icn-node-prod"
```

### 3. Initialize Identity

When starting ICN for the first time with HSM:

```bash
# Initialize new identity in HSM
icnctl id init --backend pkcs11

# You will be prompted for HSM PIN
Enter HSM PIN: ****

# ICN generates keypair in HSM
✓ Generated Ed25519 keypair in HSM
✓ Created identity bundle
✓ DID: did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
```

### 4. Verify HSM Usage

```bash
# Check identity status
icnctl id show

# Output should show:
Backend: pkcs11
Hardware-backed: yes
Slot: 0
Key label: icn-node-1
```

### 5. Start ICN Daemon

```bash
# Start with HSM backend
icnd --config icn.toml

# You will be prompted for HSM PIN
Enter HSM PIN: ****

# Daemon will unlock HSM and load identity
✓ Unlocked PKCS#11 backend
✓ Loaded identity: did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
```

## HSM Operations

### Key Generation

Keys are generated **inside the HSM** and marked as non-extractable:

```rust
use icn_identity::{Pkcs11Backend, Pkcs11Config};

let config = Pkcs11Config {
    library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
    slot_id: 0,
    key_label: "my-key".to_string(),
    token_label: None,
};

let mut backend = Pkcs11Backend::new(config)?;
let bundle = backend.init(b"1234")?; // PIN

// Private key never leaves HSM
```

### Signing Operations

All signing is delegated to the HSM:

```rust
// Unlock backend
backend.unlock(b"1234")?;

// Get signing backend
let signing_backend = backend.signing_backend()?;

// Sign message (signing happens in HSM)
let message = b"transaction data";
let signature = signing_backend.sign(message)?;

// Private key was never exposed to application
```

### Key Rotation

ICN supports rotating the HSM key to a new key:

```bash
# Generate new key in HSM
icnctl id rotate --backend pkcs11

# Old key is kept for verification
# New key becomes active
```

## Security Best Practices

### 1. PIN Management

**DO:**
- Use strong PINs (16+ characters)
- Store PIN in secure secret management (Vault, AWS Secrets Manager)
- Use different PINs for different environments
- Rotate PINs regularly

**DON'T:**
- Hard-code PINs in config files
- Use default PINs (1234, password, etc.)
- Share PINs across nodes
- Store PINs in version control

### 2. Access Control

**DO:**
- Restrict HSM access to ICN daemon user only
- Use file permissions on PKCS#11 library (chmod 600)
- Enable HSM audit logging
- Monitor HSM access logs

**DON'T:**
- Run ICN as root
- Allow multiple processes to access same HSM slot
- Disable HSM authentication

### 3. Backup and Recovery

**DO:**
- Backup HSM configuration (not keys!)
- Document HSM setup procedures
- Test disaster recovery procedures
- Use HSM's built-in backup features

**DON'T:**
- Export private keys from HSM
- Store unencrypted backups
- Skip backup testing

### 4. Monitoring

Enable audit logging:

```toml
[identity.pkcs11]
audit_log = "/var/log/icn/hsm-audit.log"
log_all_operations = true
```

Monitor for:
- Failed authentication attempts
- Unexpected key access patterns
- HSM connection failures
- Abnormal signing rates

## Troubleshooting

### Cannot find PKCS#11 library

```
Error: Failed to load PKCS#11 library
```

**Solution:**
```bash
# Find library location
find /usr -name "*pkcs11*.so" 2>/dev/null

# Update config with correct path
library_path = "/path/to/libpkcs11.so"
```

### HSM authentication failure

```
Error: Failed to login to HSM
```

**Solution:**
- Verify PIN is correct
- Check token is initialized
- Verify slot ID: `pkcs11-tool --list-slots`
- Check HSM is connected and powered

### Key not found

```
Error: Private key not found in HSM
```

**Solution:**
- List keys: `pkcs11-tool --list-objects`
- Verify key_label matches HSM object label
- Re-initialize identity if key was deleted

### Performance issues

HSMs have limited operation rates. For high-throughput:

1. Use session pooling
2. Enable HSM caching (if supported)
3. Consider load balancing across multiple HSMs
4. Monitor HSM operation latency

## Compliance and Certification

### FIPS 140-2

For FIPS 140-2 compliance:

1. **Use FIPS-certified HSM**:
   - YubiHSM2: FIPS 140-2 Level 2
   - AWS CloudHSM: FIPS 140-2 Level 3
   - Luna: FIPS 140-2 Level 3

2. **Enable FIPS mode**:
   ```toml
   [identity.pkcs11]
   fips_mode = true
   ```

3. **Document key lifecycle**:
   - Key generation procedures
   - Key usage policies
   - Key destruction procedures

### Common Criteria

For Common Criteria (CC) compliance:

1. Use CC-certified HSM
2. Follow vendor's CC configuration guide
3. Enable audit logging
4. Document security policies

## Cloud HSM Setup

### AWS CloudHSM

1. **Create CloudHSM cluster**:
   ```bash
   aws cloudhsmv2 create-cluster \
     --hsm-type hsm1.medium \
     --subnet-ids subnet-xxxxx
   ```

2. **Initialize cluster**:
   ```bash
   aws cloudhsmv2 initialize-cluster \
     --cluster-id cluster-xxxxx \
     --signed-cert file://cluster.cert \
     --trust-anchor file://ca.cert
   ```

3. **Configure ICN**:
   ```toml
   [identity.pkcs11]
   library_path = "/opt/cloudhsm/lib/libcloudhsm_pkcs11.so"
   slot_id = 1
   key_label = "icn-prod-node-1"
   ```

### Google Cloud HSM

1. **Create key ring**:
   ```bash
   gcloud kms keyrings create icn-prod \
     --location us-east1
   ```

2. **Create HSM key**:
   ```bash
   gcloud kms keys create icn-node-1 \
     --keyring icn-prod \
     --location us-east1 \
     --purpose asymmetric-signing \
     --default-algorithm ec-sign-ed25519
   ```

3. **Configure PKCS#11 bridge**:
   See Google's Cloud KMS PKCS#11 documentation

## Testing with SoftHSM2

For development and testing:

```bash
# Initialize test token
softhsm2-util --init-token --slot 0 \
  --label "icn-test" \
  --pin 1234 \
  --so-pin 5678

# Configure ICN for testing
cat > icn.toml <<EOF
[identity]
backend = "pkcs11"

[identity.pkcs11]
library_path = "/usr/lib/softhsm/libsofthsm2.so"
slot_id = 0
key_label = "test-key"
token_label = "icn-test"
EOF

# Run tests
cargo test --features hsm
```

## Migration from Age Keystore

To migrate from Age-encrypted keystore to HSM:

```bash
# 1. Export existing identity (DID only, not private key)
icnctl id export --did-only > identity.json

# 2. Generate new key in HSM
icnctl id init --backend pkcs11

# 3. Update DID document to reference new key
icnctl id update-document --import identity.json

# 4. Publish key rotation event to network
icnctl id rotate --publish
```

**Note:** The old Age keystore should be securely destroyed after migration.

## Support

For HSM-related issues:

- **YubiHSM2**: https://developers.yubico.com/YubiHSM2/
- **AWS CloudHSM**: AWS Support
- **Google Cloud HSM**: Google Cloud Support
- **ICN Issues**: https://github.com/InterCooperative-Network/icn/issues

## References

- PKCS#11 v2.40 Specification: http://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/
- FIPS 140-2: https://csrc.nist.gov/publications/detail/fips/140/2/final
- YubiHSM2 Documentation: https://developers.yubico.com/YubiHSM2/
- AWS CloudHSM User Guide: https://docs.aws.amazon.com/cloudhsm/
