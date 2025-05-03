#!/bin/bash
set -e

# Location of this script
SCRIPT_DIR=$(dirname "$0")
ROOT_DIR=$(realpath "$SCRIPT_DIR/..")
WASM_DIR="$ROOT_DIR/tests/fixtures"
CCL_DIR="$ROOT_DIR/examples"

# Ensure fixtures directory exists
mkdir -p "$WASM_DIR"

# Function to create a simple test WASM that checks resource authorizations
create_test_wasm() {
    # Create a simple Rust WASM file that just logs success
    mkdir -p "$WASM_DIR/src"
    cat > "$WASM_DIR/Cargo.toml" << 'EOF'
[package]
name = "test-wasm"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
EOF

    cat > "$WASM_DIR/src/lib.rs" << 'EOF'
#[no_mangle]
pub extern "C" fn _start() {
    log_message(1, "Test WASM executed successfully!");
}

#[link(wasm_import_module = "env")]
extern "C" {
    #[link_name = "host_log_message"]
    fn log_raw(level: i32, ptr: i32, len: i32);
}

fn log_message(level: i32, message: &str) {
    unsafe {
        let ptr = message.as_ptr() as i32;
        let len = message.len() as i32;
        log_raw(level, ptr, len);
    }
}
EOF

    # Move to the WASM directory and build the test WASM
    pushd "$WASM_DIR"
    if ! cargo build --target wasm32-unknown-unknown --release; then
        echo "Failed to build test WASM. Please make sure you have the wasm32-unknown-unknown target installed."
        echo "You can install it with: rustup target add wasm32-unknown-unknown"
        exit 1
    fi
    popd

    # Copy the built WASM to the fixtures directory
    cp "$WASM_DIR/target/wasm32-unknown-unknown/release/test_wasm.wasm" "$WASM_DIR/test_wasm.wasm"
    echo "Created test WASM at $WASM_DIR/test_wasm.wasm"
}

# Create our test WASM if it doesn't exist
if [ ! -f "$WASM_DIR/test_wasm.wasm" ]; then
    echo "Creating test WASM file..."
    create_test_wasm
fi

# Create cooperative_bylaws.ccl if it doesn't exist in examples yet
if [ ! -f "$CCL_DIR/cooperative_bylaws.ccl" ]; then
    mkdir -p "$CCL_DIR"
    cat > "$CCL_DIR/cooperative_bylaws.ccl" << 'EOF'
coop_bylaws {
    "name": "Test Cooperative",
    "description": "A cooperative for testing CCL interpretation",
    "founding_date": "2023-01-01",
    "mission_statement": "To build a better world through shared ownership",
    
    "governance": {
        "decision_making": "consent",
        "quorum": 0.75,
        "majority": 0.66,
        "roles": [
            {
                "name": "coordinator",
                "permissions": ["administrate", "create_proposals"]
            },
            {
                "name": "facilitator",
                "permissions": ["moderate_content", "facilitate_meetings"]
            }
        ]
    },
    
    "membership": {
        "onboarding": {
            "requires_sponsor": true,
            "trial_period_days": 90
        },
        "dues": {
            "amount": 50,
            "frequency": "monthly"
        }
    },
    
    "economic_model": {
        "surplus_distribution": "patronage",
        "compensation_policy": {
            "hourly_rates": {
                "programming": 50,
                "design": 45,
                "documentation": 40
            },
            "track_hours": true
        }
    },
    
    "working_groups": {
        "formation_threshold": 3,
        "resource_allocation": {
            "default_budget": 5000,
            "requires_approval": true
        }
    },
    
    "dispute_resolution": {
        "process": [
            "direct_conversation",
            "facilitated_mediation",
            "binding_arbitration"
        ],
        "committee_size": 3
    }
}
EOF
fi

# Run the CLI with our test
echo "-------------------------------------"
echo "Running execute test with cooperative_bylaws.ccl (should produce rich authorizations)..."
echo "-------------------------------------"
cargo run -- execute \
  --proposal-payload "$WASM_DIR/test_wasm.wasm" \
  --constitution "$CCL_DIR/cooperative_bylaws.ccl" \
  --identity "did:icn:test-user" \
  --scope "Cooperative" \
  --verbose

# Run with simple_community_charter.ccl if it exists
if [ -f "$CCL_DIR/simple_community_charter.ccl" ]; then
    echo "-------------------------------------"
    echo "Running execute test with simple_community_charter.ccl (might produce different authorizations)..."
    echo "-------------------------------------"
    cargo run -- execute \
      --proposal-payload "$WASM_DIR/test_wasm.wasm" \
      --constitution "$CCL_DIR/simple_community_charter.ccl" \
      --identity "did:icn:test-user" \
      --scope "Community" \
      --verbose
fi

echo "-------------------------------------"
echo "Tests complete!" 