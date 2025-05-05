#!/bin/bash
set -e

echo "ICN Project Cleanup Script"
echo "=========================="

# Function to clean up a directory
cleanup_dir() {
    local dir=$1
    echo "Cleaning up directory: $dir"
    
    # Remove target directories
    find "$dir" -name "target" -type d -exec rm -rf {} +
    
    # Remove Cargo.lock files (except in the workspace root)
    if [ "$dir" != "/home/matt/dev/icn" ]; then
        find "$dir" -name "Cargo.lock" -type f -exec rm -f {} +
    fi
    
    # Remove temporary files
    find "$dir" -name "*.rs.bk" -type f -exec rm -f {} +
    find "$dir" -name "*.swp" -type f -exec rm -f {} +
    find "$dir" -name "*.swo" -type f -exec rm -f {} +
    find "$dir" -name "*~" -type f -exec rm -f {} +
    
    echo "Cleanup completed for $dir"
}

# Function to reset the database (if requested)
reset_database() {
    echo "Resetting AgoraNet database..."
    
    # Database configuration
    DB_HOST=${PGHOST:-localhost}
    DB_PORT=${PGPORT:-5432}
    DB_USER=${PGUSER:-postgres}
    DB_PASSWORD=${PGPASSWORD:-postgres}
    DB_NAME=${PGDATABASE:-agoranet}
    
    # Drop and recreate the database
    PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U $DB_USER -c "DROP DATABASE IF EXISTS $DB_NAME;"
    PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U $DB_USER -c "CREATE DATABASE $DB_NAME;"
    
    echo "Database reset complete."
}

# Main script execution
ICN_ROOT="/home/matt/dev/icn"

# Clean up each module directory
cleanup_dir "$ICN_ROOT"
cleanup_dir "$ICN_ROOT/icn-wallet-root"
cleanup_dir "$ICN_ROOT/icn-runtime-root"
cleanup_dir "$ICN_ROOT/agoranet"

# Reset database if requested
if [ "$1" == "--reset-db" ]; then
    reset_database
fi

echo "Running cargo clean on workspace..."
cd "$ICN_ROOT"
cargo clean

echo ""
echo "All cleanup tasks completed!"
echo "To rebuild the project, run: cargo build --workspace" 