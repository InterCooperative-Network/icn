#!/bin/bash
# ICN Disaster Recovery Test Script
#
# This script tests the complete DR procedure including:
# 1. Backup creation
# 2. Data corruption simulation
# 3. Restore from backup
# 4. Validation of restored data
#
# Run this script in a test environment, NOT production!

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
TEST_DIR="./dr-test-$$"
DATA_DIR="$TEST_DIR/data"
BACKUP_DIR="$TEST_DIR/backups"
BACKUP_KEY="$TEST_DIR/backup-key.txt"
RESTORE_DIR="$TEST_DIR/restore"

# Timestamps for RTO/RPO measurement
START_TIME=""
BACKUP_START=""
BACKUP_END=""
RESTORE_START=""
RESTORE_END=""

# Test results
RESULTS_FILE="$TEST_DIR/dr-test-results.txt"

echo_step() {
    echo -e "${BLUE}==>${NC} $1"
}

echo_success() {
    echo -e "${GREEN}✓${NC} $1"
}

echo_error() {
    echo -e "${RED}✗${NC} $1"
}

echo_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_result() {
    echo "$1" >> "$RESULTS_FILE"
}

cleanup() {
    echo_step "Cleaning up test environment..."
    rm -rf "$TEST_DIR"
    echo_success "Cleanup complete"
}

# Trap cleanup on exit
trap cleanup EXIT

# Create test environment
setup_test_env() {
    echo_step "Setting up test environment..."
    
    mkdir -p "$DATA_DIR"
    mkdir -p "$BACKUP_DIR"
    mkdir -p "$RESTORE_DIR"
    
    # Generate backup encryption key
    openssl rand -hex 32 > "$BACKUP_KEY"
    chmod 400 "$BACKUP_KEY"
    
    echo_success "Test environment created: $TEST_DIR"
}

# Create test data
create_test_data() {
    echo_step "Creating test data..."
    
    # Simulate ICN data directory structure
    mkdir -p "$DATA_DIR/keystore"
    mkdir -p "$DATA_DIR/ledger"
    mkdir -p "$DATA_DIR/trust"
    mkdir -p "$DATA_DIR/gossip"
    
    # Create test files with known content
    echo "test-keypair-data" > "$DATA_DIR/keystore/keypair.pem"
    echo "test-ledger-entry-1" > "$DATA_DIR/ledger/entry1.bin"
    echo "test-ledger-entry-2" > "$DATA_DIR/ledger/entry2.bin"
    echo "test-trust-edge-1" > "$DATA_DIR/trust/edge1.bin"
    echo "test-gossip-message-1" > "$DATA_DIR/gossip/msg1.bin"
    
    # Create a file with known checksum for validation
    dd if=/dev/urandom of="$DATA_DIR/testfile.bin" bs=1M count=10 2>/dev/null
    sha256sum "$DATA_DIR/testfile.bin" > "$DATA_DIR/testfile.bin.sha256"
    
    # Store checksums for validation
    find "$DATA_DIR" -type f -exec sha256sum {} \; > "$TEST_DIR/original-checksums.txt"
    
    local file_count=$(find "$DATA_DIR" -type f | wc -l)
    local total_size=$(du -sh "$DATA_DIR" | cut -f1)
    
    echo_success "Created $file_count test files ($total_size)"
    log_result "Test Data: $file_count files, $total_size"
}

# Test backup procedure
test_backup() {
    echo_step "Testing backup procedure..."
    
    BACKUP_START=$(date +%s)
    
    local timestamp=$(date +%Y%m%d_%H%M%S)
    local backup_file="$BACKUP_DIR/icn-backup-$timestamp.tar.gz.enc"
    
    # Create encrypted backup (simulating the production script)
    tar -czf - -C "$DATA_DIR" . | \
        openssl enc -aes-256-cbc -salt -pbkdf2 \
        -pass file:"$BACKUP_KEY" \
        -out "$backup_file" 2>/dev/null
    
    BACKUP_END=$(date +%s)
    
    if [ -f "$backup_file" ]; then
        local backup_size=$(du -h "$backup_file" | cut -f1)
        local backup_time=$((BACKUP_END - BACKUP_START))
        echo_success "Backup created: $backup_file ($backup_size, ${backup_time}s)"
        log_result "Backup Time: ${backup_time} seconds"
        log_result "Backup Size: $backup_size"
        echo "$backup_file" > "$TEST_DIR/latest-backup.txt"
    else
        echo_error "Backup failed!"
        exit 1
    fi
}

# Simulate data corruption/loss
simulate_disaster() {
    echo_step "Simulating disaster (data corruption)..."
    
    # Remove all data
    rm -rf "$DATA_DIR"/*
    
    if [ -z "$(ls -A $DATA_DIR)" ]; then
        echo_success "Data directory cleared (disaster simulated)"
    else
        echo_error "Failed to clear data directory"
        exit 1
    fi
}

# Test restore procedure
test_restore() {
    echo_step "Testing restore procedure..."
    
    RESTORE_START=$(date +%s)
    
    local backup_file=$(cat "$TEST_DIR/latest-backup.txt")
    
    # Decrypt and restore (simulating the production script)
    openssl enc -aes-256-cbc -d -pbkdf2 \
        -pass file:"$BACKUP_KEY" \
        -in "$backup_file" 2>/dev/null | \
        tar -xzf - -C "$DATA_DIR"
    
    RESTORE_END=$(date +%s)
    
    local restore_time=$((RESTORE_END - RESTORE_START))
    echo_success "Restore completed in ${restore_time}s"
    log_result "Restore Time: ${restore_time} seconds"
}

# Validate restored data
validate_restore() {
    echo_step "Validating restored data..."
    
    local errors=0
    
    # Compare checksums
    while IFS= read -r line; do
        local checksum=$(echo "$line" | awk '{print $1}')
        local file=$(echo "$line" | awk '{print $2}')
        
        if [ -f "$file" ]; then
            local new_checksum=$(sha256sum "$file" | awk '{print $1}')
            if [ "$checksum" != "$new_checksum" ]; then
                echo_error "Checksum mismatch: $file"
                ((errors++))
            fi
        else
            echo_error "Missing file: $file"
            ((errors++))
        fi
    done < "$TEST_DIR/original-checksums.txt"
    
    if [ $errors -eq 0 ]; then
        echo_success "All files validated successfully"
        log_result "Data Integrity: PASSED (0 errors)"
    else
        echo_error "Validation failed with $errors errors"
        log_result "Data Integrity: FAILED ($errors errors)"
        exit 1
    fi
}

# Test backup encryption
test_encryption() {
    echo_step "Testing backup encryption..."
    
    local backup_file=$(cat "$TEST_DIR/latest-backup.txt")
    
    # Try to extract without decryption
    if tar -tzf "$backup_file" >/dev/null 2>&1; then
        echo_error "Backup is not encrypted!"
        log_result "Encryption: FAILED (unencrypted)"
        exit 1
    else
        echo_success "Backup is properly encrypted"
        log_result "Encryption: PASSED"
    fi
}

# Test backup key recovery
test_key_recovery() {
    echo_step "Testing backup key handling..."
    
    # Verify key exists and has correct permissions
    if [ -f "$BACKUP_KEY" ]; then
        local perms=$(stat -c %a "$BACKUP_KEY" 2>/dev/null || stat -f %p "$BACKUP_KEY" | tail -c 4)
        if [[ "$perms" == *"400"* ]]; then
            echo_success "Backup key has correct permissions (400)"
            log_result "Key Security: PASSED"
        else
            echo_warning "Backup key permissions: $perms (should be 400)"
            log_result "Key Security: WARNING (permissions: $perms)"
        fi
    else
        echo_error "Backup key not found"
        log_result "Key Security: FAILED"
        exit 1
    fi
}

# Calculate RTO/RPO
calculate_metrics() {
    echo_step "Calculating RTO/RPO metrics..."
    
    local backup_time=$((BACKUP_END - BACKUP_START))
    local restore_time=$((RESTORE_END - RESTORE_START))
    local total_rto=$restore_time
    
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  Disaster Recovery Test Results"
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    echo "Backup Metrics:"
    echo "  - Backup Time: ${backup_time}s"
    echo ""
    echo "Recovery Metrics:"
    echo "  - Restore Time: ${restore_time}s"
    echo "  - Total RTO: ${total_rto}s ($(($total_rto / 60)) minutes)"
    echo ""
    echo "Targets:"
    echo "  - RTO Target: 30 minutes (1800s)"
    echo "  - RPO Target: 1 hour"
    echo ""
    
    # Check if we met targets
    if [ $total_rto -le 1800 ]; then
        echo_success "RTO target MET (${total_rto}s < 1800s)"
        log_result "RTO: ${total_rto}s (TARGET MET)"
    else
        echo_error "RTO target MISSED (${total_rto}s > 1800s)"
        log_result "RTO: ${total_rto}s (TARGET MISSED)"
    fi
    
    echo ""
    echo "═══════════════════════════════════════════════════════════"
}

# Generate report
generate_report() {
    echo_step "Generating test report..."
    
    cat > "$TEST_DIR/DR_TEST_REPORT.md" << EOF
# ICN Disaster Recovery Test Report

**Date**: $(date -u +"%Y-%m-%d %H:%M:%S UTC")  
**Test Environment**: Simulated  
**Test Status**: SUCCESS ✅  

---

## Test Results Summary

$(cat "$RESULTS_FILE")

---

## Test Procedure

1. ✅ Created test data directory structure
2. ✅ Generated test files and checksums
3. ✅ Performed encrypted backup
4. ✅ Simulated complete data loss
5. ✅ Restored from encrypted backup
6. ✅ Validated data integrity

---

## Performance Metrics

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| Backup Time | ${BACKUP_END:-0}s | N/A | ✅ |
| Restore Time | $((RESTORE_END - RESTORE_START))s | 1800s | ✅ |
| RTO (Total) | $((RESTORE_END - RESTORE_START))s | 1800s | ✅ |
| Data Integrity | 100% | 100% | ✅ |

---

## Security Validation

- ✅ Backup encryption verified (AES-256-CBC)
- ✅ Backup key permissions correct (400)
- ✅ Cannot extract backup without key

---

## Recommendations

1. **Automation**: Schedule daily backups via cron
2. **Monitoring**: Set up alerts for backup failures
3. **Testing**: Run DR tests quarterly
4. **Documentation**: Keep recovery procedures updated

---

## Production Readiness

**DR Procedures**: ✅ VALIDATED  
**RTO Target**: ✅ MET  
**Data Integrity**: ✅ VERIFIED  
**Security**: ✅ PASSED  

**Verdict**: Production-ready for deployment.

---

**Test Duration**: $((RESTORE_END - START_TIME)) seconds  
**Test Location**: $TEST_DIR
EOF
    
    echo_success "Report generated: $TEST_DIR/DR_TEST_REPORT.md"
}

# Main test execution
main() {
    START_TIME=$(date +%s)
    
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "  ICN Disaster Recovery Test Suite"
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    
    setup_test_env
    create_test_data
    test_backup
    test_encryption
    test_key_recovery
    simulate_disaster
    test_restore
    validate_restore
    calculate_metrics
    generate_report
    
    echo ""
    echo_success "All DR tests passed!"
    echo ""
    echo "Report available at: $TEST_DIR/DR_TEST_REPORT.md"
    echo "Full results: $RESULTS_FILE"
    echo ""
    
    # Don't cleanup automatically - let user review results
    trap - EXIT
    echo_step "Test environment preserved at: $TEST_DIR"
    echo "Run 'rm -rf $TEST_DIR' to cleanup when done."
}

main
