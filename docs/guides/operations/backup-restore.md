# Backup and Restore: Operator Recovery Procedure

This document covers the low-level recovery procedure for ICN node state,
focusing on the Sled database and keystore. For the `icnctl` CLI backup/restore
commands, see [backup-and-recovery.md](backup-and-recovery.md).

## What to Back Up

An ICN node's critical state lives in three places:

| Component | Default Location | Contents |
|-----------|-----------------|----------|
| **Keystore** | `~/.icn/identity.age` | Ed25519 signing key, X25519 encryption key, TLS cert |
| **Sled database** | `~/.icn/db/` | Ledger entries, replica metadata, trust graph, gossip state |
| **Configuration** | `~/.icn/config.toml` | Node settings, listen addresses, peers |

### Priority Order

1. **Keystore** -- Without it, the node's DID is lost. This is the most critical file.
2. **Sled database** -- Contains ledger history and trust relationships. Can be partially reconstructed from peers via gossip, but full recovery is faster from backup.
3. **Configuration** -- Can be recreated manually, but saves time.

### What is NOT backed up (reconstructed automatically)

- Gossip vector clocks (re-synced from peers)
- Active network connections (re-established on restart)
- In-memory caches (rebuilt on demand)

## How to Back Up

### Option 1: icnctl (recommended for operators)

```bash
icnctl backup /path/to/backup.tar
```

This creates a tarball with all data directory contents plus a SHA256 checksum.
See [backup-and-recovery.md](backup-and-recovery.md) for full details.

### Option 2: Direct filesystem copy (for automation / cron)

```bash
# Stop the daemon first to ensure consistent state
systemctl stop icnd

# Copy the data directory
cp -a ~/.icn /backup/icn-$(date +%Y%m%d)

# Restart
systemctl start icnd
```

### Option 3: Sled-level export (programmatic)

The `SledStore` exposes `export_snapshot()` and `import_snapshot()` methods
for creating portable database snapshots:

```rust
use icn_store::SledStore;

// Export
let store = SledStore::open("~/.icn/db")?;
let snapshot = store.export_snapshot()?;
// snapshot is Vec<(Vec<u8>, Vec<u8>)> -- serialize to disk

// Import
let new_store = SledStore::open("/restore/db")?;
new_store.import_snapshot(&snapshot)?;
```

These methods are used by the automated backup validation tests
(`icn-core/tests/backup_restore_integration.rs`).

## How to Restore

### Step 1: Stop the daemon

```bash
systemctl stop icnd
# or: pkill icnd
```

### Step 2: Restore files

**From icnctl backup:**
```bash
icnctl restore /path/to/backup.tar --force
```

**From filesystem copy:**
```bash
# Move corrupted data out of the way
mv ~/.icn ~/.icn.corrupted.$(date +%s)

# Restore from backup
cp -a /backup/icn-20260215 ~/.icn
```

### Step 3: Verify the restore

```bash
# Verify identity is intact
icnctl id show

# Expected output: your DID, key fingerprint, etc.
# If this fails, the keystore is corrupted -- try an older backup.
```

### Step 4: Restart the daemon

```bash
systemctl start icnd
# or: icnd --data-dir ~/.icn
```

### Step 5: Verify node health

```bash
# Check the daemon is running
icnctl status

# Verify ledger state
icnctl ledger balance

# Verify peer connections (may take a few seconds)
icnctl peers list
```

## Verification Checklist

After any restore, confirm:

- [ ] `icnctl id show` returns the correct DID
- [ ] Passphrase unlocks the keystore (prompted on `id show`)
- [ ] Daemon starts without errors
- [ ] Ledger balances match expected values
- [ ] Peers reconnect within 30 seconds
- [ ] Gossip sync resumes (check logs for "anti-entropy" messages)

## Automated Testing

The backup/restore integration tests in
`icn/crates/icn-core/tests/backup_restore_integration.rs` validate:

| Test | What it verifies |
|------|-----------------|
| `test_store_backup_and_restore_roundtrip` | Full store export/import with mixed data types |
| `test_backup_restore_ledger_entries` | Ledger entries survive backup/restore cycle |
| `test_backup_restore_keystore_identity` | Keypair backup/restore preserves DID and signing capability |
| `test_backup_restore_multiple_identities` | Multiple identity backup/restore in batch |
| `test_backup_restore_replica_metadata` | Replica tracking state survives restore |
| `test_backup_restore_empty_store` | Empty store backup is a safe no-op |
| `test_backup_restore_overwrites_existing` | Import correctly overwrites target data |
| `test_backup_restore_large_dataset` | Scalability with 1000 entries |

Run them with:
```bash
cd icn
cargo test -p icn-core --test backup_restore_integration -- --nocapture
```

## See Also

- [backup-and-recovery.md](backup-and-recovery.md) -- CLI backup commands and security guidance
- [troubleshooting.md](troubleshooting.md) -- Common operational issues
