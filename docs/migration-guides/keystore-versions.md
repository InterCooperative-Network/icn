# Keystore Version Migration Guide

This guide covers migrating between keystore format versions in ICN.

## Version History

| Version | Date | Changes | Status |
|---------|------|---------|--------|
| v1 | 2025-01 | Ed25519 keypair only | Legacy |
| v2 | 2025-01 | + TLS certificate with DID-TLS binding | Legacy |
| v2.1 | 2025-01 | + X25519 keys for end-to-end encryption | Current |

---

## Automatic Migration

**ICN automatically migrates your keystore** when you unlock it for the first time after upgrading.

### What Happens

1. **Unlock with existing passphrase**:
   ```bash
   icnctl id show
   # or
   icnd
   ```

2. **Automatic migration**:
   - Detects old format (v1 or v2)
   - Generates missing components:
     - v1→v2.1: TLS certificate + X25519 keys
     - v2→v2.1: X25519 keys (reuses TLS cert)
   - Saves upgraded keystore immediately
   - Creates backup: `keystore.age.backup-v1-<timestamp>`

3. **Verification**:
   ```bash
   icnctl id show
   ```
   Look for:
   - ✅ DID (same as before)
   - ✅ TLS certificate persisted
   - ✅ X25519 encryption key present

### Migration Log

You'll see this in the logs:
```
✅ Successfully migrated and saved v2.1 keystore with persistent TLS binding and X25519 keys
```

---

## Manual Migration

If automatic migration fails or you want manual control:

### Step 1: Backup Current Keystore

**CRITICAL**: Always backup before manual migration!

```bash
cp ~/.icn/keystore.age ~/.icn/keystore.age.backup-manual
# Or use built-in backup
icnctl backup ~/icn-backup-before-migration.tar.gz.age
```

### Step 2: Check Current Version

```bash
# Check keystore format (requires reading the file)
# The version is detected on unlock, so just try to unlock:
icnctl id show
```

If you see warnings about missing TLS or X25519 keys, you have an old version.

### Step 3: Trigger Migration

Simply unlock the keystore:
```bash
icnctl id show
# Enter your passphrase when prompted
```

Migration happens automatically on unlock.

### Step 4: Verify Migration

```bash
# Check that DID is unchanged
icnctl id show

# Verify keystore file was updated
ls -lh ~/.icn/keystore.age*

# You should see:
# keystore.age              (new v2.1)
# keystore.age.backup-v1-*  (old version backup)
```

---

## Troubleshooting

### "Failed to unlock keystore"

**Cause**: Wrong passphrase or corrupted keystore

**Solution**:
1. Double-check your passphrase
2. Restore from backup:
   ```bash
   icnctl restore ~/icn-backup.tar.gz.age
   ```
3. If still failing, your keystore may be corrupted - restore from backup

### "Migration succeeded but TLS/X25519 keys missing"

**Cause**: Migration didn't save properly

**Solution**:
1. Restore from backup
2. Ensure `~/.icn/` is writable:
   ```bash
   ls -ld ~/.icn/
   # Should show: drwx------
   ```
3. Re-run migration with debug logging:
   ```bash
   RUST_LOG=debug icnctl id show
   ```

### "DID changed after migration"

**This should NEVER happen!** DIDs are derived from Ed25519 public keys, which never change.

**If this happens**:
1. **STOP IMMEDIATELY**
2. Restore from backup:
   ```bash
   icnctl restore ~/icn-backup.tar.gz.age --force
   ```
3. Report bug: https://github.com/InterCooperative-Network/icn/issues

### "Backup file is huge"

**Cause**: Backup includes entire data directory (ledger, trust graph, gossip state)

**This is normal.** Backups include:
- Keystore (~1 KB)
- Trust graph (~100 KB for 50 members)
- Ledger state (~1 MB for 1000 transactions)
- Gossip state (~500 KB)
- Snapshots (~500 KB)

**To backup only keystore**:
```bash
tar -czf ~/keystore-only-backup.tar.gz -C ~/.icn keystore.age
```

---

## Version-Specific Details

### v1 → v2.1 Migration

**What happens:**
1. Loads Ed25519 keypair (unchanged)
2. Generates TLS certificate from Ed25519 key
3. Signs DID-TLS binding proof
4. Generates X25519 keypair for encryption
5. Creates v2.1 bundle
6. Encrypts with your passphrase
7. Saves to disk (overwrites `keystore.age`)
8. Creates backup of old keystore

**Time**: < 1 second

**Compatibility**:
- ✅ Old nodes can still verify your identity (DID unchanged)
- ✅ New features (E2E encryption, persistent TLS) now available

### v2 → v2.1 Migration

**What happens:**
1. Loads Ed25519 keypair + TLS certificate (unchanged)
2. Generates X25519 keypair for encryption
3. Creates v2.1 bundle
4. Encrypts with your passphrase
5. Saves to disk
6. Creates backup of v2 keystore

**Time**: < 1 second

**Compatibility**:
- ✅ TLS certificate reused (no new binding signature needed)
- ✅ E2E encryption now available

---

## Multi-Device Considerations

If you use multi-device identity:

1. **Migrate primary device first**:
   ```bash
   # On Device 1 (primary)
   icnctl id show  # Triggers migration
   ```

2. **Export device credentials**:
   ```bash
   icnctl device export my-phone > phone-creds.json
   ```

3. **Import on other devices**:
   ```bash
   # On Device 2
   icnctl device import phone-creds.json
   ```

4. **Verify all devices work**:
   ```bash
   icnctl device list
   ```

See [multi-device-identity-design.md](../design/multi-device-identity-design.md) for details.

---

## Downgrading (Not Recommended)

**You cannot downgrade keystore versions.** v2.1 keystores cannot be read by older ICN versions.

**If you must use an older ICN version**:
1. Restore from pre-migration backup
2. Use the old ICN version
3. Do NOT upgrade to new ICN until ready to migrate

**Why downgrade is not supported:**
- Newer formats include more security features (X25519, persistent TLS)
- Removing features would compromise security
- Old versions lack code to parse new formats

---

## Best Practices

1. **Always backup before upgrading ICN**:
   ```bash
   icnctl backup ~/icn-backup-before-upgrade.tar.gz.age
   ```

2. **Test migration on non-production node first** (if possible)

3. **Keep multiple backups**:
   - Local: `~/icn-backups/`
   - Off-site: Cloud storage, USB drive, etc.
   - Frequency: Weekly for active nodes

4. **Verify migration succeeded**:
   ```bash
   icnctl id show
   icnctl network status  # Ensure peers still connect
   ```

5. **Monitor logs during migration**:
   ```bash
   journalctl -u icnd -f
   ```

---

## FAQ

**Q: Will migration change my DID?**
A: No. DIDs are derived from Ed25519 public keys, which never change during migration.

**Q: Will I lose my ledger history?**
A: No. Migration only affects the keystore. Ledger state is separate.

**Q: Do I need to re-establish trust relationships?**
A: No. Trust graph is unchanged. Peers will recognize your DID.

**Q: Can I migrate while icnd is running?**
A: No. Stop icnd first: `sudo systemctl stop icnd`

**Q: How long does migration take?**
A: < 1 second. Cryptographic operations are fast.

**Q: Can I migrate back to v1/v2?**
A: No. Downgrading is not supported.

**Q: What if I have multiple keystores (test/prod)?**
A: Migrate each separately. Set `ICN_DATA_DIR` for non-default locations:
```bash
ICN_DATA_DIR=/path/to/test icnctl id show
```

---

## Related Documentation

- [Getting Started Guide](../GETTING_STARTED.md) - Initial setup
- [Multi-Device Identity](../design/multi-device-identity-design.md) - Multiple devices
- [Backup & Recovery](../guides/operations/backup-and-recovery.md) - Backup strategies
- [Operations Guide](../guides/operations/operations-guide.md) - Day-to-day management

---

*Last Updated: 2025-11-21*
