# Keystore Backend Migration Guide

This guide explains how to migrate from Age-encrypted keystores to HSM or TPM backends.

## Overview

ICN supports three keystore backends:

| Backend | Security | Performance | Cost | Use Case |
|---------|----------|-------------|------|----------|
| **Age** | Software encryption | Fast (no hardware) | Free | Development, low-value nodes |
| **PKCS#11 HSM** | Hardware-backed | Medium (HSM latency) | $$$ | Production, high-throughput |
| **TPM 2.0** | Platform-bound | Slow (TPM limitations) | Free (built-in) | Single-node, platform integrity |

## Why Migrate?

### Benefits of Hardware Backends

**Security:**
- Keys never exist in application memory
- Protection against memory dumps and malware
- Physical tamper resistance (HSM)
- Platform binding and attestation (TPM)

**Compliance:**
- FIPS 140-2 certification (HSM)
- Common Criteria (HSM/TPM)
- Regulatory requirements for financial services

**Operational:**
- Centralized key management
- Hardware audit logging
- Key ceremony procedures

### When to Migrate

Migrate to HSM when:
- Handling high-value transactions
- Regulatory compliance required
- Multi-node deployments
- High signing throughput (>100 ops/sec)

Migrate to TPM when:
- Single-node deployment
- Platform integrity is critical
- Cost is a concern
- Low signing volume (<10 ops/sec)

## Migration Process

### Phase 1: Planning

1. **Assess Requirements**:
   - What security level is needed?
   - What is the signing volume?
   - Budget for hardware?
   - Compliance requirements?

2. **Choose Backend**:
   - HSM for high-security, high-throughput
   - TPM for single-node, cost-effective
   - Age for development/testing

3. **Test Environment**:
   - Set up HSM (SoftHSM2) or TPM simulator
   - Test migration procedure
   - Verify functionality

### Phase 2: Preparation

#### Backup Current Identity

```bash
# Export DID and public keys (NOT private key!)
icnctl id export --did-only > identity-backup.json

# Verify export
cat identity-backup.json
# Should contain: DID, public keys, DID document

# Store backup securely
chmod 600 identity-backup.json
```

#### Document Current State

```bash
# Record current identity
icnctl id show > pre-migration-state.txt

# Record network connections
icnctl network peers > peers-before.txt

# Record trust relationships
icnctl trust list > trust-before.txt
```

#### Install Hardware

**HSM:**
```bash
# Install PKCS#11 library
sudo apt-get install softhsm2  # For testing
# or install YubiHSM2 software for production

# Initialize token
softhsm2-util --init-token --slot 0 \
  --label "icn-production" \
  --pin 1234 --so-pin 5678
```

**TPM:**
```bash
# Verify TPM availability
ls /dev/tpm*

# Install TPM tools
sudo apt-get install tpm2-tools tpm2-abrmd

# Enable resource manager
sudo systemctl enable tpm2-abrmd
sudo systemctl start tpm2-abrmd
```

### Phase 3: Migration

#### Stop ICN Daemon

```bash
# Stop daemon gracefully
sudo systemctl stop icnd

# Verify stopped
ps aux | grep icnd
```

#### Backup Configuration

```bash
# Backup current config
cp /etc/icn/icn.toml /etc/icn/icn.toml.backup

# Backup current keystore
cp ~/.icn/keystore.age ~/.icn/keystore.age.backup
```

#### Update Configuration

**For HSM:**
```bash
cat >> /etc/icn/icn.toml <<EOF

[identity]
backend = "pkcs11"

[identity.pkcs11]
library_path = "/usr/lib/softhsm/libsofthsm2.so"
slot_id = 0
key_label = "icn-node-1"
token_label = "icn-production"
EOF
```

**For TPM:**
```bash
cat >> /etc/icn/icn.toml <<EOF

[identity]
backend = "tpm"

[identity.tpm]
device_path = "/dev/tpmrm0"
key_handle = 0x81000001
platform_binding = true
attestation = true
pcr_indices = "0,1,7"
EOF
```

#### Initialize New Backend

**HSM:**
```bash
# Initialize identity in HSM
icnctl id init --backend pkcs11

# Verify
icnctl id show
# Should show:
#   Backend: pkcs11
#   Hardware-backed: yes
```

**TPM:**
```bash
# Initialize identity in TPM
icnctl id init --backend tpm

# Verify
icnctl id show
# Should show:
#   Backend: tpm
#   Hardware-backed: yes
#   Platform binding: enabled
```

#### Publish Key Rotation

```bash
# Generate rotation event
icnctl id rotate --publish

# This:
# 1. Creates rotation record signed by old key
# 2. Signs rotation record with new key
# 3. Broadcasts to network
# 4. Peers update trust relationships
```

### Phase 4: Verification

#### Test Signing

```bash
# Sign test message
echo "test" | icnctl sign -

# Verify signature
icnctl verify --message test --signature <sig>
```

#### Test Network Connectivity

```bash
# Start daemon
sudo systemctl start icnd

# Check peers
icnctl network peers

# Should show existing peers
# May take a few minutes for key rotation to propagate
```

#### Verify Identity

```bash
# Check identity status
icnctl id show

# Compare DID (should be different!)
# Compare with backup
diff <(jq -r .did identity-backup.json) \
     <(icnctl id show --format json | jq -r .did)

# Verify trust relationships
icnctl trust list > trust-after.txt
diff trust-before.txt trust-after.txt
```

### Phase 5: Cleanup

#### Secure Old Keystore

**Option 1: Archive** (Recommended)
```bash
# Encrypt old keystore
tar czf keystore-backup.tar.gz ~/.icn/keystore.age.backup
gpg --encrypt --recipient you@example.com keystore-backup.tar.gz

# Store encrypted backup offsite
# Delete unencrypted backup
shred -u keystore-backup.tar.gz ~/.icn/keystore.age.backup
```

**Option 2: Delete** (High Security)
```bash
# Securely delete old keystore
shred -vfz -n 5 ~/.icn/keystore.age.backup

# Verify deletion
ls -la ~/.icn/
```

#### Update Documentation

Document the migration:

```markdown
# Migration Log

- Date: 2026-01-20
- Old DID: did:icn:z6Mk...
- New DID: did:icn:z6Mm...
- Backend: PKCS#11 (YubiHSM2)
- Slot: 0
- Key Label: icn-prod-node-1
- Migration performed by: Alice
- Verified by: Bob
```

## Rollback Procedure

If migration fails, rollback:

### Quick Rollback (Emergency)

```bash
# Stop new daemon
sudo systemctl stop icnd

# Restore old config
cp /etc/icn/icn.toml.backup /etc/icn/icn.toml

# Restore old keystore
cp ~/.icn/keystore.age.backup ~/.icn/keystore.age

# Start daemon
sudo systemctl start icnd

# Verify
icnctl id show
```

### Proper Rollback

```bash
# Generate reverse rotation event
icnctl id rotate --reverse

# Restore Age backend
icnctl id init --backend age --restore identity-backup.json

# Publish rollback
icnctl id rotate --publish --reason "Rollback from HSM migration"

# Verify network connectivity
icnctl network peers
```

## Migration Strategies

### Strategy 1: Blue-Green Deployment

For high-availability:

1. **Setup new node** with HSM/TPM backend
2. **Establish trust** between old and new node
3. **Migrate workload** to new node
4. **Decommission old node** after verification

### Strategy 2: Gradual Migration

For large deployments:

1. **Pilot group**: Migrate 1-2 nodes first
2. **Monitor** for issues (1 week)
3. **Batch migration**: Migrate nodes in waves
4. **Final cleanup**: Retire old keystores

### Strategy 3: Atomic Swap

For single-node:

1. **Stop services**
2. **Migrate identity**
3. **Start services**
4. **Verify** within maintenance window

## Troubleshooting

### Migration Failed: Key Not Found

```bash
# Check HSM/TPM connection
# HSM:
pkcs11-tool --list-slots

# TPM:
tpm2_getcap properties-fixed

# Verify key label
pkcs11-tool --list-objects

# Re-initialize if needed
icnctl id init --backend pkcs11 --force
```

### Network Not Recognizing New Key

```bash
# Check rotation event was published
icnctl network messages --topic identity:rotation

# Manually broadcast rotation
icnctl id rotate --publish --broadcast

# Wait for propagation (up to 5 minutes)
# Verify peers received update
icnctl network peers --verbose
```

### Performance Issues After Migration

```bash
# Check signing latency
time icnctl sign --message "test"

# Expected latencies:
# Age: <1ms
# HSM: 50-200ms
# TPM: 100-500ms

# If too slow:
# 1. Check hardware connection
# 2. Enable hardware caching (if supported)
# 3. Consider batching operations
# 4. For TPM: Check resource manager is running
```

### Identity Lost After Migration

This is a critical failure. Use backup to recover:

```bash
# Restore from backup
icnctl id import identity-backup.json

# Re-initialize with Age backend
icnctl id init --backend age --restore

# Notify network of emergency key change
icnctl id emergency-rotation

# Investigation required:
# 1. Check HSM/TPM logs
# 2. Verify hardware functionality
# 3. Test in dev environment before retry
```

## Best Practices

### Pre-Migration

- [ ] Test in development environment
- [ ] Document current state
- [ ] Backup all identity information
- [ ] Schedule maintenance window
- [ ] Notify network participants
- [ ] Verify hardware functionality

### During Migration

- [ ] Follow checklist strictly
- [ ] Verify each step before proceeding
- [ ] Keep old keystore until fully verified
- [ ] Monitor logs continuously
- [ ] Have rollback plan ready

### Post-Migration

- [ ] Monitor for 24-48 hours
- [ ] Verify all network connections
- [ ] Test signing operations
- [ ] Update documentation
- [ ] Archive old keystore securely
- [ ] Update disaster recovery plan

## Security Considerations

### Key Rotation Event Security

The key rotation event is critical:

1. **Signed by old key**: Proves authorization
2. **Signed by new key**: Proves possession
3. **Includes timestamp**: Prevents replay
4. **Broadcast to network**: Ensures propagation

Attackers cannot:
- Forge rotation events (no old private key)
- Replay old events (timestamp check)
- Intercept and modify (cryptographic signatures)

### Hardware Security

**HSM:**
- Store PINs in secure secret management
- Use strong PINs (16+ characters)
- Enable audit logging
- Physical security for USB HSMs

**TPM:**
- Enable Secure Boot
- Bind to critical PCRs (0,1,7)
- Monitor for PCR changes
- Test unsealing after firmware updates

### Network Security

During migration:
- Existing peers gradually learn new key
- Old key remains trusted during transition
- Full propagation takes ~5 minutes
- Monitor for peers not updating

## Support

For migration assistance:

- **Documentation**: See `docs/hsm-setup.md` and `docs/tpm-setup.md`
- **Community**: ICN Discord #keystore-migration channel
- **Issues**: https://github.com/InterCooperative-Network/icn/issues

## Migration Checklist

Print and follow:

```
Pre-Migration:
[ ] Backup current identity
[ ] Test in development
[ ] Schedule maintenance window
[ ] Notify network participants
[ ] Install and test hardware

Migration:
[ ] Stop ICN daemon
[ ] Backup configuration
[ ] Update configuration
[ ] Initialize new backend
[ ] Publish key rotation
[ ] Start ICN daemon

Verification:
[ ] Test signing operations
[ ] Verify network connectivity
[ ] Check trust relationships
[ ] Monitor for 24 hours

Post-Migration:
[ ] Archive old keystore
[ ] Update documentation
[ ] Update disaster recovery plan
[ ] Notify stakeholders of completion
```
