#!/bin/bash
set -e

# Use this test script once you've built the CLI
# This script assumes that the CLI has already been built 
# and a viable WASM file is available in the core-vm tests

# Find a suitable WASM file to test with
TEST_WASM_FILE="$(find . -name '*.wasm' -type f | head -n 1)"

if [ -z "$TEST_WASM_FILE" ]; then
    echo "Error: Could not find a WASM file to test with."
    echo "Building a simple test WASM file..."
    
    # Create a temporary directory for the test WASM project
    TMP_DIR=$(mktemp -d)
    
    # Create a simple Rust project
    echo "Creating test WASM project in $TMP_DIR"
    mkdir -p "$TMP_DIR/src"
    
    cat > "$TMP_DIR/Cargo.toml" << EOF
[package]
name = "test-wasm"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
EOF

    cat > "$TMP_DIR/src/lib.rs" << EOF
#[no_mangle]
pub extern "C" fn _start() {
    unsafe {
        let message = "Hello from test WASM!";
        let ptr = message.as_ptr() as i32;
        let len = message.len() as i32;
        log_message(1, ptr, len);
    }
}

#[link(wasm_import_module = "env")]
extern "C" {
    #[link_name = "host_log_message"]
    fn log_message(level: i32, ptr: i32, len: i32);
}
EOF

    pushd "$TMP_DIR"
    cargo build --target wasm32-unknown-unknown --release || {
        echo "Failed to build test WASM."
        exit 1
    }
    popd
    
    # Copy the test WASM to a location in our project
    if [ -f "$TMP_DIR/target/wasm32-unknown-unknown/release/test_wasm.wasm" ]; then
        mkdir -p target/wasm
        cp "$TMP_DIR/target/wasm32-unknown-unknown/release/test_wasm.wasm" "target/wasm/test.wasm"
        TEST_WASM_FILE="target/wasm/test.wasm"
        echo "Test WASM file created at $TEST_WASM_FILE"
    else
        echo "Error: WASM build failed, no WASM available for testing."
        exit 1
    fi
    
    # Clean up
    rm -rf "$TMP_DIR"
else
    echo "Found test WASM file: $TEST_WASM_FILE"
fi

# Define paths to CCL files
CCL_DIR="examples"
COOP_CCL="$CCL_DIR/cooperative_bylaws.ccl"
COMMUNITY_CCL="$CCL_DIR/simple_community_charter.ccl"

# Check if CCL files exist, print error and exit if not
if [ ! -f "$COOP_CCL" ]; then
    echo "Error: Could not find cooperative_bylaws.ccl at $COOP_CCL"
    exit 1
fi

if [ ! -f "$COMMUNITY_CCL" ]; then
    echo "Error: Could not find simple_community_charter.ccl at $COMMUNITY_CCL"
    exit 1
fi

# Run with cooperative_bylaws.ccl
echo "-------------------------------------"
echo "Running execute test with cooperative_bylaws.ccl (should produce rich authorizations)..."
echo "-------------------------------------"
cargo run -- execute \
  --proposal-payload "$TEST_WASM_FILE" \
  --constitution "$COOP_CCL" \
  --identity "did:icn:test-user" \
  --scope "Cooperative" \
  --verbose

# Run with simple_community_charter.ccl
echo "-------------------------------------"
echo "Running execute test with simple_community_charter.ccl (might produce different authorizations)..."
echo "-------------------------------------"
cargo run -- execute \
  --proposal-payload "$TEST_WASM_FILE" \
  --constitution "$COMMUNITY_CCL" \
  --identity "did:icn:test-user" \
  --scope "Community" \
  --verbose

echo "-------------------------------------"
echo "Tests complete!" 