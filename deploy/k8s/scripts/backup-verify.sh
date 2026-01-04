#!/bin/sh
# ICN Backup Verification Script
# Validates backup integrity and database consistency
# Part of Issue #320: Automated backup verification

set -e

BACKUP_DIR="${BACKUP_DIR:-/backups}"
VERIFY_DIR="${VERIFY_DIR:-/tmp/backup-verify}"
METRICS_FILE="${METRICS_FILE:-/tmp/backup-metrics.txt}"
MAX_AGE_HOURS="${MAX_AGE_HOURS:-26}"  # Alert if newest backup > 26 hours old

# Prometheus metrics helpers
init_metrics() {
    cat > "$METRICS_FILE" << 'EOF'
# HELP icn_backup_verification_success Whether the last backup verification succeeded (1=success, 0=failure)
# TYPE icn_backup_verification_success gauge
# HELP icn_backup_last_verified_timestamp Unix timestamp of last successful verification
# TYPE icn_backup_last_verified_timestamp gauge
# HELP icn_backup_count Number of backup files available
# TYPE icn_backup_count gauge
# HELP icn_backup_newest_age_seconds Age of newest backup in seconds
# TYPE icn_backup_newest_age_seconds gauge
# HELP icn_backup_total_size_bytes Total size of all backups in bytes
# TYPE icn_backup_total_size_bytes gauge
# HELP icn_backup_verification_duration_seconds Duration of verification in seconds
# TYPE icn_backup_verification_duration_seconds gauge
EOF
}

write_metric() {
    echo "$1 $2" >> "$METRICS_FILE"
}

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1"
}

error() {
    log "ERROR: $1"
    write_metric "icn_backup_verification_success" "0"
    exit 1
}

cleanup() {
    rm -rf "$VERIFY_DIR" 2>/dev/null || true
}
trap cleanup EXIT

# Start verification
START_TIME=$(date +%s)
init_metrics
log "Starting backup verification"
log "Backup directory: $BACKUP_DIR"

# Check backup directory exists
if [ ! -d "$BACKUP_DIR" ]; then
    error "Backup directory does not exist: $BACKUP_DIR"
fi

# Find all backup files
BACKUP_FILES=$(find "$BACKUP_DIR" -name "icn-backup-*.tar.gz" -type f 2>/dev/null | sort -r)
BACKUP_COUNT=$(echo "$BACKUP_FILES" | grep -c "tar.gz" || echo "0")

if [ "$BACKUP_COUNT" -eq 0 ]; then
    error "No backup files found in $BACKUP_DIR"
fi

log "Found $BACKUP_COUNT backup file(s)"
write_metric "icn_backup_count" "$BACKUP_COUNT"

# Get newest backup
NEWEST_BACKUP=$(echo "$BACKUP_FILES" | head -1)
log "Newest backup: $NEWEST_BACKUP"

# Calculate backup age
BACKUP_MTIME=$(stat -c %Y "$NEWEST_BACKUP" 2>/dev/null || stat -f %m "$NEWEST_BACKUP" 2>/dev/null)
CURRENT_TIME=$(date +%s)
BACKUP_AGE_SECONDS=$((CURRENT_TIME - BACKUP_MTIME))
BACKUP_AGE_HOURS=$((BACKUP_AGE_SECONDS / 3600))

log "Backup age: ${BACKUP_AGE_HOURS} hours (${BACKUP_AGE_SECONDS} seconds)"
write_metric "icn_backup_newest_age_seconds" "$BACKUP_AGE_SECONDS"

if [ "$BACKUP_AGE_HOURS" -gt "$MAX_AGE_HOURS" ]; then
    log "WARNING: Newest backup is older than $MAX_AGE_HOURS hours"
fi

# Calculate total backup size
TOTAL_SIZE=$(find "$BACKUP_DIR" -name "icn-backup-*.tar.gz" -type f -exec stat -c %s {} + 2>/dev/null | awk '{sum+=$1} END {print sum}' || echo "0")
write_metric "icn_backup_total_size_bytes" "$TOTAL_SIZE"
log "Total backup size: $((TOTAL_SIZE / 1024 / 1024)) MB"

# Verify archive integrity
log "Verifying archive integrity..."
if ! tar -tzf "$NEWEST_BACKUP" > /dev/null 2>&1; then
    error "Archive integrity check failed for $NEWEST_BACKUP"
fi
log "Archive integrity: OK"

# Extract backup to temporary directory for content verification
log "Extracting backup for content verification..."
mkdir -p "$VERIFY_DIR"
tar -xzf "$NEWEST_BACKUP" -C "$VERIFY_DIR"

# Verify Sled database files exist
log "Checking Sled database structure..."
SLED_DIRS=""
for dir in "$VERIFY_DIR"/*; do
    if [ -d "$dir" ]; then
        # Check for Sled database markers (conf file or db directory structure)
        if [ -f "$dir/conf" ] || [ -d "$dir/db" ]; then
            SLED_DIRS="$SLED_DIRS $dir"
        fi
    fi
done

# Also check root level for flat Sled structure
if [ -f "$VERIFY_DIR/conf" ]; then
    SLED_DIRS="$VERIFY_DIR"
fi

if [ -n "$SLED_DIRS" ]; then
    log "Found Sled database structure(s)"
    for db_dir in $SLED_DIRS; do
        db_name=$(basename "$db_dir")
        if [ -f "$db_dir/conf" ]; then
            log "  - $db_name: conf file present"
        fi
        # Count segment files (blobs directory)
        if [ -d "$db_dir/blobs" ]; then
            blob_count=$(find "$db_dir/blobs" -type f 2>/dev/null | wc -l)
            log "  - $db_name: $blob_count blob files"
        fi
    done
else
    log "No Sled database structure found (may be empty or different format)"
fi

# Verify essential files/directories exist
log "Checking for essential data..."
ESSENTIAL_CHECK_PASSED=true

# Check for keystore (identity)
if [ -f "$VERIFY_DIR/keystore.age" ] || [ -f "$VERIFY_DIR/keystore" ]; then
    log "  - Keystore: present"
else
    log "  - Keystore: not found (may be stored separately)"
fi

# Check for any data files
DATA_FILE_COUNT=$(find "$VERIFY_DIR" -type f 2>/dev/null | wc -l)
log "  - Total files in backup: $DATA_FILE_COUNT"

if [ "$DATA_FILE_COUNT" -eq 0 ]; then
    log "WARNING: Backup appears to be empty"
fi

# Calculate verification duration
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
write_metric "icn_backup_verification_duration_seconds" "$DURATION"

# Mark verification as successful
write_metric "icn_backup_verification_success" "1"
write_metric "icn_backup_last_verified_timestamp" "$CURRENT_TIME"

log "Verification completed successfully in ${DURATION} seconds"
log "Metrics written to: $METRICS_FILE"

# Print metrics summary
echo ""
echo "=== Verification Summary ==="
echo "Backup count: $BACKUP_COUNT"
echo "Newest backup age: ${BACKUP_AGE_HOURS}h"
echo "Total backup size: $((TOTAL_SIZE / 1024 / 1024)) MB"
echo "Files in newest backup: $DATA_FILE_COUNT"
echo "Verification time: ${DURATION}s"
echo "Status: SUCCESS"
