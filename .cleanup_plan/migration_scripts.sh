#!/bin/bash
set -e

# Function to migrate a component to the new structure
# Usage: migrate_component source_dir target_dir
migrate_component() {
    local source_dir=$1
    local target_dir=$2
    local name=$(basename "$source_dir")
    
    echo "Migrating $source_dir to $target_dir/$name"
    
    # Create target directory
    mkdir -p "$target_dir/$name"
    
    # Copy files (excluding target directory itself to avoid recursion)
    rsync -av --exclude=".git" --exclude="target" "$source_dir/" "$target_dir/$name/"
    
    echo "Migration of $name completed"
}

# Migrate runtime components
migrate_runtime_components() {
    echo "Migrating runtime components..."
    
    # Skip components with -old suffix
    for component in $(find icn-runtime-root/crates -maxdepth 1 -type d -not -name "*-old" | grep -v "^\.$" | grep -v "^\.\."); do
        if [[ $(basename $component) != "crates" && ! $(basename $component) =~ -old$ ]]; then
            migrate_component "$component" "crates/runtime"
        fi
    done
    
    echo "Runtime components migration completed"
}

# Migrate wallet components
migrate_wallet_components() {
    echo "Migrating wallet components..."
    
    for component in $(find icn-wallet-root/crates -maxdepth 1 -type d | grep -v "^\.$" | grep -v "^\.\."); do
        if [[ $(basename $component) != "crates" ]]; then
            migrate_component "$component" "crates/wallet"
        fi
    done
    
    echo "Wallet components migration completed"
}

# Migrate AgoraNet components
migrate_agoranet_components() {
    echo "Migrating AgoraNet components..."
    
    # AgoraNet main component
    migrate_component "agoranet/src" "crates/agoranet/agoranet-core"
    
    # AgoraNet subcrates if they exist
    if [ -d "agoranet/crates" ]; then
        for component in $(find agoranet/crates -maxdepth 1 -type d | grep -v "^\.$" | grep -v "^\.\."); do
            if [[ $(basename $component) != "crates" ]]; then
                migrate_component "$component" "crates/agoranet"
            fi
        done
    fi
    
    echo "AgoraNet components migration completed"
}

# Migrate Mesh components
migrate_mesh_components() {
    echo "Migrating Mesh components..."
    
    for component in $(find mesh/crates -maxdepth 1 -type d | grep -v "^\.$" | grep -v "^\.\."); do
        if [[ $(basename $component) != "crates" ]]; then
            migrate_component "$component" "crates/mesh"
        fi
    done
    
    # Migrate meshctl
    if [ -d "mesh/meshctl" ]; then
        migrate_component "mesh/meshctl" "crates/mesh"
    fi
    
    echo "Mesh components migration completed"
}

# Identify and extract common code
extract_common_code() {
    echo "Extracting common code..."
    
    # Migrate icn-common to common/icn-common
    if [ -d "icn-runtime-root/crates/icn-common" ]; then
        migrate_component "icn-runtime-root/crates/icn-common" "crates/common"
    fi
    
    # Create common types library
    mkdir -p crates/common/common-types/src
    touch crates/common/common-types/src/lib.rs
    
    echo "Common code extraction completed"
}

# Update Cargo.toml files to reflect new paths
update_cargo_files() {
    echo "Updating Cargo.toml files..."
    
    find crates -name "Cargo.toml" -type f -exec sed -i 's|"icn-runtime-root/crates/|"../../../crates/|g' {} \;
    find crates -name "Cargo.toml" -type f -exec sed -i 's|"icn-wallet-root/crates/|"../../../crates/|g' {} \;
    find crates -name "Cargo.toml" -type f -exec sed -i 's|"agoranet/|"../../../crates/|g' {} \;
    find crates -name "Cargo.toml" -type f -exec sed -i 's|"mesh/|"../../../crates/|g' {} \;
    
    echo "Cargo.toml files updated"
}

# Run all migration steps
run_all_migrations() {
    migrate_runtime_components
    migrate_wallet_components
    migrate_agoranet_components
    migrate_mesh_components
    extract_common_code
    update_cargo_files
}

# Display usage information
usage() {
    echo "Usage: $0 [command]"
    echo "Commands:"
    echo "  runtime    - Migrate runtime components"
    echo "  wallet     - Migrate wallet components"
    echo "  agoranet   - Migrate agoranet components"
    echo "  mesh       - Migrate mesh components"
    echo "  common     - Extract common code"
    echo "  cargo      - Update Cargo.toml files"
    echo "  all        - Run all migration steps"
    echo "  help       - Display this help message"
}

# Main script execution
case "$1" in
    runtime)
        migrate_runtime_components
        ;;
    wallet)
        migrate_wallet_components
        ;;
    agoranet)
        migrate_agoranet_components
        ;;
    mesh)
        migrate_mesh_components
        ;;
    common)
        extract_common_code
        ;;
    cargo)
        update_cargo_files
        ;;
    all)
        run_all_migrations
        ;;
    help|*)
        usage
        ;;
esac 