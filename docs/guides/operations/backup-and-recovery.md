# Backup and Recovery Guide

This guide covers backup and recovery procedures for ICN nodes, focusing on identity preservation and operational continuity.

## Overview

ICN backup and restore functionality allows you to:

- **Create encrypted backups** of your ICN data directory
- **Restore identities** to new devices or recover from failures
- **Migrate identities** between environments
- **Verify integrity** of backups with SHA256 checksums

## What Gets Backed Up

The backup includes all data in your ICN data directory (default: `~/.icn`):

- **Identity keystore** (`identity.age`) - Your Ed25519 signing key, X25519 encryption key, and TLS certificate
- **DID Document** - Your multi-device identity configuration
- **Rotation chain** - Complete audit trail of device additions/revocations
- **Trust graph data** (if present) - Your local trust relationships
- **Ledger database** (if present) - Your mutual credit ledger state
- **Configuration files** (if present)

**Note**: The backup does NOT include:
- Gossip state (reconstructed from peers on restart)
- Active network connections
- In-memory caches

## Creating a Backup

### Basic Usage

```bash
icnctl backup /path/to/backup.tar
```

This creates a tarball backup with:
- All files from your data directory
- Metadata file with ICN version, timestamp, and checksum
- SHA256 integrity verification

### Example Output

```
Creating backup of /home/user/.icn...
Archiving data directory...
Calculating checksum...
✓ Backup created successfully
  Output: /home/user/backups/icn-2025-01-14.tar
  ICN version: 0.1.0
  Checksum: a455524be49ebd31071587200bb3cde9c2827517696b40935ded6efaf423f2fd

IMPORTANT: Store this backup securely. It contains your identity keystore.
```

### Custom Data Directory

```bash
icnctl --data-dir /custom/path backup /path/to/backup.tar
```

## Restoring from Backup

### Basic Restore

```bash
icnctl restore /path/to/backup.tar
```

This will:
1. Verify the backup file exists
2. Check if data directory already exists (fails if it does)
3. Extract backup metadata and display information
4. Extract all files to the data directory
5. Verify integrity with SHA256 checksum

### Example Output

```
Restoring backup from /home/user/backups/icn-2025-01-14.tar...
Reading backup metadata...
Backup information:
  ICN version: 0.1.0
  Created: 2025-01-14T10:23:45+00:00
  Checksum: a455524be49ebd31071587200bb3cde9c2827517696b40935ded6efaf423f2fd

Extracting backup...
Verifying checksum...
✓ Backup restored successfully
  Restored to: /home/user/.icn
  Checksum verified: a455524be49ebd31071587200bb3cde9c2827517696b40935ded6efaf423f2fd

You can now use 'icnctl id show' to verify your restored identity.
```

### Force Restore (Overwrite Existing)

If your data directory already exists, use `--force` to overwrite:

```bash
icnctl restore /path/to/backup.tar --force
```

This will:
1. Take the data directory's locks. Restore is refused while `icnd` or another `icnctl` command holds the directory, so stop the daemon first. On a systemd install, use `sudo systemctl disable --now icnd` rather than just `stop`, so the daemon doesn't keep trying to start (it refuses while a restore is incomplete, see below). Re-enable it once the restored node is verified.
2. Move the existing *contents* (not the directory itself) into `<data-dir>.backup-<archive-timestamp>`, beside the real data directory (symlinks resolved). That directory is owner-only (`0700`) whatever your umask, so files that a `0700` data directory protected stay protected.
3. Extract the backup into the data directory and verify its checksum.
4. Leave the old data in the `.backup-` directory in case you need it. Remove it once the restored node is verified.

The data directory's two lock files (`.icn-data-dir.lock` and `.icn-config.lock`) stay where they are. They are coordination files, not state. A data directory that holds nothing else has nothing to move aside, so no `.backup-` directory is created.

Restore checks everything it can before moving anything. It refuses, saying `Nothing has been moved`, when:

- a `.backup-` directory of that name already exists and isn't empty (an earlier restore of the same archive). It may be the only copy of what the data directory held before, so move it somewhere safe rather than deleting it;
- the data directory is a mount point or btrfs subvolume, or contains one. The `.backup-` directory would be on a different filesystem, and contents can't be renamed across filesystems;
- the account running restore can't create the `.backup-` directory (see the native install below).

**If the move fails part-way, restore puts everything back.** An entry can turn out to be unmovable only once restore tries to move it, for example a directory the account can't write or an immutable file. Restore then moves every entry it had already moved back into the data directory, removes the `.backup-` directory it created, and reports `Nothing has been moved; <data-dir> is unchanged`. Restore doesn't put an entry back over something now at its original name. If an entry can't be put back, restore says the data directory is split, and names each entry still in the `.backup-` directory and why.

#### Native install (`/var/lib/icn`): supported, with one administrator step

`deploy/install.sh` gives the `icn` service account `/var/lib/icn` but not its parent `/var/lib`. Restore must run as the account that owns the data directory: it refuses to run as root over a directory the service account owns, because files it created would belong to root. So the first attempt refuses, having moved nothing, and prints the one administrator step that makes the restore possible: creating the empty `.backup-` directory for the `icn` account.

Restore prints that step as a command only when every directory from the parent up to `/` belongs to root and nobody else can write it, as `/var/lib`, `/var` and `/` do. In any other layout someone else could plant a link at that name, or swap a directory on the way to it, before you ran it. So restore describes the directory instead.

```bash
sudo systemctl disable --now icnd
sudo -u icn icnctl --data-dir /var/lib/icn restore /path/to/backup.tar --force
#   Refusing to move the existing data aside: this account cannot create
#   /var/lib/icn.backup-<timestamp> beside the data directory ... Nothing has been moved.
#       sudo mkdir -m 0700 -- '/var/lib/icn.backup-<timestamp>' && sudo chown -h +<uid>:+<gid> -- '/var/lib/icn.backup-<timestamp>'

# Run the exact line it printed, then the same restore again:
sudo mkdir -m 0700 -- '/var/lib/icn.backup-<timestamp>' && sudo chown -h +<uid>:+<gid> -- '/var/lib/icn.backup-<timestamp>'
sudo -u icn icnctl --data-dir /var/lib/icn restore /path/to/backup.tar --force
```

The command uses `mkdir` and `chown -h` rather than `install -d` because `install -d` follows a symlink that already exists at that name and re-owns its target. Restore accepts the directory only if it is a real directory (not a symlink), owned by the data directory's account, on the same filesystem, and empty. It makes the directory owner-only before moving anything into it. The `icn` account must be able to read the archive.

#### Mount-point data directory (container volume, PVC): not supported in place

When the data directory is itself a mount point, as with the Kubernetes and Helm deployments, Compose volumes and a PVC, there is nowhere on the same filesystem beside it to move the contents to. Restore refuses with `Nothing has been moved`. To restore one:

1. Stop the node, so nothing holds the data directory.
2. Move or copy the contents off the volume yourself, to storage you control, including dotfiles. That copy is your only backup of the previous state, so verify it before going on.
3. Leave nothing in the directory except the two lock files. On an ext4-formatted volume that includes removing `lost+found`, which `fsck` recreates when it needs it.
4. Run `icnctl restore /path/to/backup.tar --force`. A data directory with nothing in it needs no `.backup-` directory.

#### If a restore does not finish

Before it changes anything, restore writes a marker into the data directory, `.icn-restore-incomplete`, and flushes it to disk. The marker records what the directory held and where restore was moving it. Restore removes it only once the directory is verified: the restore finished with its data on disk, or a failed move put back exactly what it took.

A restore that is stopped part-way (Ctrl-C, `kill`, power loss) or fails after moving the old contents aside leaves the marker in place. **While it is there, nothing opens the data directory's stores.** `icnd`, every `icnctl` command that uses them, `icnctl backup`, and another restore all refuse and name the marker. That holds across a reboot too, so a daemon that starts on its own refuses instead of coming up with its stores empty.

To recover, run, as the data directory's account and with nothing else running against it:

```bash
icnctl --data-dir '<data-dir>' restore --recover-incomplete
```

Recovery puts the previous contents back from `<data-dir>.backup-<timestamp>`, but only the same objects restore recorded, never copies and never over anything. It then checks that the data directory holds exactly what it held before the restore began, and only then removes the marker. It deletes nothing, and it stops and says why in two cases:

- **Entries that were not there before.** These came from the archive, or from somewhere else. Remove exactly the entries it lists, for example `rm -rf -- '<data-dir>/<name>'` for each one. The archive still has them.
- **Entries that carry an earlier name but are not the same objects.** One may be the only copy of that entry, changed in place. Compare it with what you expect, and do not delete it.

It runs only as the account that owns the data directory. It refuses if every account can write the directory, or can replace entries in its parent, because then its record could have been written by someone else. In those cases, recover by hand.

Then run the recovery again. It is safe to run any number of times, including after it was itself interrupted, because it works from where things are now. Once it succeeds, the empty `.backup-` directory can stay; a later restore accepts it.

If recovery can't read the marker, or you recover the directory some other way, move the `.backup-` directory's entries back without overwriting:

```bash
find '<data-dir>.backup-<timestamp>' -mindepth 1 -maxdepth 1 -exec mv -n -t '<data-dir>' -- {} +
```

When you are certain the data directory is whole, remove `.icn-restore-incomplete` yourself. Nothing removes it automatically.

While the marker is there, `icnd --init`, `icnctl id init` and anything that writes the configuration refuse as well, so nothing mints a fresh identity or configuration into a half-restored directory.

### Restore to Custom Location

```bash
icnctl --data-dir /new/location restore /path/to/backup.tar
```

## Best Practices

### 1. Regular Backups

**Recommendation**: Create backups after significant identity changes:
- After initial identity creation
- After adding new devices
- After rotating keys
- Before major upgrades
- Weekly for active nodes

**Automation Example**:
```bash
#!/bin/bash
# Daily backup script
DATE=$(date +%Y-%m-%d)
BACKUP_DIR="$HOME/icn-backups"
mkdir -p "$BACKUP_DIR"

icnctl backup "$BACKUP_DIR/icn-backup-$DATE.tar"

# Keep only last 30 days
find "$BACKUP_DIR" -name "icn-backup-*.tar" -mtime +30 -delete
```

### 2. Secure Storage

**Passphrase Protection**: The keystore itself is encrypted with Age using your passphrase. The backup tarball does NOT add additional encryption.

**Storage Options**:
- ✅ **Encrypted external drive** - Good for local backups
- ✅ **Encrypted cloud storage** (with additional encryption layer)
- ✅ **Hardware security module** - Best for production
- ❌ **Unencrypted cloud storage** - Never store keystores here
- ❌ **Public repositories** - Absolutely never

**Additional Encryption** (recommended for cloud storage):
```bash
# Encrypt the backup tarball with GPG
gpg -c icn-backup.tar

# Upload encrypted file
aws s3 cp icn-backup.tar.gpg s3://your-bucket/backups/

# To restore:
aws s3 cp s3://your-bucket/backups/icn-backup.tar.gpg .
gpg -d icn-backup.tar.gpg > icn-backup.tar
icnctl restore icn-backup.tar
```

### 3. Verify Backups

**Test your backups regularly**:
```bash
# Restore to temporary directory
icnctl --data-dir /tmp/test-restore restore icn-backup.tar

# Verify identity
export ICN_PASSPHRASE="your-passphrase"
icnctl --data-dir /tmp/test-restore id show

# Clean up
rm -rf /tmp/test-restore
```

### 4. Multiple Copies

Follow the **3-2-1 backup rule**:
- **3** copies of your data
- **2** different storage media types
- **1** off-site copy

Example:
1. Primary copy: Active node data directory
2. Local backup: External encrypted drive
3. Remote backup: Encrypted cloud storage

### 5. Document Recovery Procedures

Store recovery instructions separately from backups:

```
# ICN Recovery Procedure

1. Install ICN on new system
2. Retrieve backup from secure storage
3. Decrypt if encrypted: gpg -d icn-backup.tar.gpg > icn-backup.tar
4. Restore: icnctl restore icn-backup.tar
5. Verify identity: icnctl id show
6. Verify passphrase: Enter it when prompted
7. Check device list: icnctl device list
8. Revoke compromised devices if needed
```

## Recovery Scenarios

### Scenario 1: Lost Device

**Situation**: Laptop with ICN identity was lost/stolen.

**Steps**:
1. On trusted device with backup:
   ```bash
   icnctl restore /path/to/backup.tar
   ```

2. Revoke the lost device:
   ```bash
   icnctl device list  # Find lost device ID
   icnctl device revoke device-abc123 --reason lost
   ```

3. Add new device:
   ```bash
   # On new device
   icnctl device add new-laptop

   # On trusted device
   icnctl device approve new-laptop-request.json
   ```

### Scenario 2: Corrupted Data Directory

**Situation**: Data directory corrupted due to disk failure.

**Steps**:
1. Stop ICNd if running:
   ```bash
   pkill icnd
   ```

2. Restore from backup:
   ```bash
   icnctl restore /path/to/backup.tar --force
   ```

3. Verify restoration:
   ```bash
   icnctl id show
   ```

4. Restart ICNd:
   ```bash
   icnd
   ```

### Scenario 3: Migrate to New Server

**Situation**: Moving ICN identity to new server.

**Steps**:
1. On old server, create backup:
   ```bash
   icnctl backup /tmp/icn-migration.tar
   ```

2. Transfer to new server securely:
   ```bash
   scp /tmp/icn-migration.tar new-server:/tmp/
   ```

3. On new server, restore:
   ```bash
   icnctl restore /tmp/icn-migration.tar
   ```

4. Verify identity:
   ```bash
   icnctl id show
   ```

5. Update network configuration if needed

### Scenario 4: Forgotten Passphrase

**Situation**: Lost passphrase for encrypted keystore.

**Unfortunately**: **Passphrase cannot be recovered**. This is by design for security.

**Prevention**:
- Store passphrase in password manager (e.g., 1Password, Bitwarden)
- Write down on paper and store in physical safe
- Use social recovery (future feature - Phase 11.6)

**If passphrase is lost**:
1. Identity is permanently inaccessible
2. Must create new identity with `icnctl id init`
3. Inform cooperative members of new DID
4. Rebuild trust relationships

## Integrity Verification

### Understanding Checksums

Each backup includes a SHA256 checksum of all files in the data directory. This detects:
- File corruption during backup/restore
- Tampering with backup contents
- Incomplete extractions

### Manual Verification

To verify a backup without restoring:

```bash
# Extract metadata
tar -xf icn-backup.tar backup_metadata.json
cat backup_metadata.json

# Output:
{
  "icn_version": "0.1.0",
  "created_at": 1736860986,
  "checksum": "a455524be49ebd31071587200bb3cde9c2827517696b40935ded6efaf423f2fd"
}
```

### Checksum Mismatch

If you see `Checksum mismatch` error during restore:
1. **Do not use the restored data**
2. Try restoring from a different backup
3. Check backup file integrity:
   ```bash
   sha256sum icn-backup.tar
   ```
4. Verify backup was not corrupted during transfer
5. Contact support if issue persists

## Security Considerations

### Keystore Encryption

- Keystore is encrypted with Age using your passphrase
- Uses scrypt for key derivation (memory-hard, resistant to brute-force)
- Passphrase is zeroized from memory after use

### Backup Security

- Backup tarball does **NOT** add additional encryption
- Keystore inside backup is still Age-encrypted
- Anyone with backup + passphrase can access your identity
- Treat backups as **highly sensitive**

### Threat Model

**Protects against**:
- ✅ Hardware failure
- ✅ Accidental deletion
- ✅ Data corruption
- ✅ Physical device loss (if passphrase secure)

**Does NOT protect against**:
- ❌ Compromised passphrase
- ❌ Malware on backup system
- ❌ Stolen backup + passphrase

## Troubleshooting

### "Backup file not found"

**Cause**: File path incorrect or file deleted.

**Solution**: Verify file exists with `ls -l /path/to/backup.tar`

### "Data directory already exists"

**Cause**: Trying to restore to existing directory without `--force`.

**Solutions**:
- Use `--force` flag to overwrite
- Restore to different directory with `--data-dir`
- Manually backup and remove existing directory

### "Failed to extract backup"

**Cause**: Corrupted tarball or permission issues.

**Solutions**:
- Verify file integrity: `tar -tf backup.tar`
- Check disk space: `df -h`
- Verify permissions on target directory

### "Checksum mismatch"

**Cause**: Backup corrupted or tampered with.

**Solutions**:
- Use a different backup from earlier date
- Verify backup file wasn't partially transferred
- Check for disk errors

## Command Reference

### Backup Command

```
icnctl backup <OUTPUT>

Arguments:
  <OUTPUT>  Path for backup archive (e.g., /backup/icn.tar)

Flags:
  --data-dir <DIR>  Data directory to backup (default: ~/.icn)
  -h, --help        Print help
```

### Restore Command

```
icnctl restore <INPUT> [--force]

Arguments:
  <INPUT>  Path to backup archive

Flags:
  --force          Overwrite existing data directory
  --data-dir <DIR> Target directory for restore (default: ~/.icn)
  -h, --help       Print help
```

## See Also

- [Multi-Device Identity Guide](../../development/sessions/2025-11/2025-11-14-phase-11-multi-device-identity.md) - Device management
- [Deployment Guide](../../operations/deployment/deployment-guide.md) - Production deployment
- [ARCHITECTURE.md](../../ARCHITECTURE.md) - System architecture
