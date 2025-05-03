#!/bin/bash
set -e

# Script to generate mobile platform bindings from the wallet-ffi crate

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
TARGET_DIR="$ROOT_DIR/target"
OUTPUT_DIR="$ROOT_DIR/mobile-bindings"

# Ensure output directory exists
mkdir -p "$OUTPUT_DIR/android"
mkdir -p "$OUTPUT_DIR/ios"

# Build the wallet-ffi crate
echo "Building wallet-ffi crate..."
cargo build --package wallet-ffi

# Path to uniffi-bindgen
UNIFFI_BINDGEN="$TARGET_DIR/debug/uniffi-bindgen"

# If uniffi-bindgen doesn't exist, build it
if [ ! -f "$UNIFFI_BINDGEN" ]; then
    echo "Building uniffi-bindgen..."
    cargo install uniffi_bindgen
    UNIFFI_BINDGEN="$(which uniffi-bindgen)"
fi

# Path to the UDL file
UDL_FILE="$ROOT_DIR/crates/wallet-ffi/src/wallet.udl"

# Generate Kotlin bindings
echo "Generating Kotlin bindings..."
$UNIFFI_BINDGEN generate "$UDL_FILE" --language kotlin --out-dir "$OUTPUT_DIR/android"

# Generate Swift bindings
echo "Generating Swift bindings..."
$UNIFFI_BINDGEN generate "$UDL_FILE" --language swift --out-dir "$OUTPUT_DIR/ios"

echo "Generating Kotlin class wrapper..."
cat > "$OUTPUT_DIR/android/WalletWrapper.kt" << 'EOF'
package wallet

/**
 * Wrapper class for the wallet API to provide more idiomatic Kotlin usage
 */
class WalletWrapper(config: WalletConfig? = null) {
    private val api: WalletApi

    init {
        api = WalletApi(config)
    }

    /**
     * Creates a new identity
     */
    fun createIdentity(scope: String, metadata: Map<String, String>): String {
        return api.createIdentity(scope, metadata)
    }

    /**
     * Lists all identities
     */
    fun listIdentities(): List<IdentityInfo> {
        return api.listIdentities()
    }

    /**
     * Gets details for an identity
     */
    fun getIdentity(id: String): IdentityDetails {
        return api.getIdentity(id)
    }

    /**
     * Queues an action for processing
     */
    fun queueAction(creatorId: String, type: String, payload: Map<String, String>): String {
        return api.queueAction(creatorId, type, payload)
    }

    /**
     * Processes a queued action
     */
    fun processAction(actionId: String) {
        api.processAction(actionId)
    }

    /**
     * Triggers synchronization with federation nodes
     */
    fun triggerSync() {
        api.triggerSync()
    }

    /**
     * Gets the current sync status
     */
    fun getSyncStatus(): SyncStatusInfo {
        return api.getSyncStatus()
    }
}
EOF

echo "Generating Swift class wrapper..."
cat > "$OUTPUT_DIR/ios/WalletWrapper.swift" << 'EOF'
import Foundation
import wallet_ffi

/**
 * Wrapper class for the wallet API to provide more idiomatic Swift usage
 */
public class WalletWrapper {
    private let api: WalletApi

    public init(config: WalletConfig? = nil) throws {
        api = try WalletApi(config: config)
    }

    /**
     * Creates a new identity
     */
    public func createIdentity(scope: String, metadata: [String: String]) throws -> String {
        return try api.createIdentity(scope: scope, metadata: metadata)
    }

    /**
     * Lists all identities
     */
    public func listIdentities() throws -> [IdentityInfo] {
        return try api.listIdentities()
    }

    /**
     * Gets details for an identity
     */
    public func getIdentity(id: String) throws -> IdentityDetails {
        return try api.getIdentity(id: id)
    }

    /**
     * Queues an action for processing
     */
    public func queueAction(creatorId: String, type: String, payload: [String: String]) throws -> String {
        return try api.queueAction(creatorId: creatorId, type: type, payload: payload)
    }

    /**
     * Processes a queued action
     */
    public func processAction(actionId: String) throws {
        try api.processAction(actionId: actionId)
    }

    /**
     * Triggers synchronization with federation nodes
     */
    public func triggerSync() throws {
        try api.triggerSync()
    }

    /**
     * Gets the current sync status
     */
    public func getSyncStatus() throws -> SyncStatusInfo {
        return try api.getSyncStatus()
    }
}
EOF

echo "Making script executable..."
chmod +x "$OUTPUT_DIR/android/run-uniffi-bindgen.sh" 2>/dev/null || true
chmod +x "$OUTPUT_DIR/ios/run-uniffi-bindgen.sh" 2>/dev/null || true

echo "Done! Mobile bindings generated in $OUTPUT_DIR"
echo "Android: $OUTPUT_DIR/android"
echo "iOS: $OUTPUT_DIR/ios" 