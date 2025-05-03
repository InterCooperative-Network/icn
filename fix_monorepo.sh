#!/bin/bash
set -e

# Colors for better output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Help message
show_help() {
  echo "Usage: $0 [options]"
  echo ""
  echo "Options:"
  echo "  --all                 Fix and build all components"
  echo "  --runtime             Fix and build only runtime components"
  echo "  --wallet              Fix and build only wallet components"
  echo "  --agoranet            Fix and build AgoraNet components (requires PostgreSQL)"
  echo "  --db-setup            Setup PostgreSQL database for AgoraNet"
  echo "  --help                Show this help message"
  echo ""
  echo "Example: $0 --runtime --wallet"
}

# Default options
FIX_RUNTIME=false
FIX_WALLET=false
FIX_AGORANET=false
SETUP_DB=false

# Parse command line arguments
if [ $# -eq 0 ]; then
  # If no arguments, show help and exit
  show_help
  exit 0
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --all)
      FIX_RUNTIME=true
      FIX_WALLET=true
      FIX_AGORANET=true
      shift
      ;;
    --runtime)
      FIX_RUNTIME=true
      shift
      ;;
    --wallet)
      FIX_WALLET=true
      shift
      ;;
    --agoranet)
      FIX_AGORANET=true
      shift
      ;;
    --db-setup)
      SETUP_DB=true
      shift
      ;;
    --help)
      show_help
      exit 0
      ;;
    *)
      echo -e "${RED}Unknown option: $1${NC}"
      show_help
      exit 1
      ;;
  esac
done

echo -e "${GREEN}Starting monorepo fixing script${NC}"

# 1. Clean any previous build artifacts
echo -e "${YELLOW}Cleaning previous build artifacts...${NC}"
cargo clean || echo "Couldn't clean, continuing anyway"

# 2. Fix root Cargo.toml workspace configuration
echo -e "${YELLOW}Fixing workspace configuration...${NC}"
cat > Cargo.toml << 'EOF'
[workspace]
resolver = "2"
members = [
    "runtime",
    "wallet",
    "runtime/crates/*",
    "wallet/crates/*"
]

# Temporarily excluded components with issues
exclude = [
    "wallet/crates/wallet-sync",
    "agoranet"
]

[workspace.dependencies]
# Common dependencies
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
futures = "0.3"
clap = { version = "4.4", features = ["derive"] }

# Network and storage
libp2p = "0.53"
multihash = { version = "0.16.3", features = ["sha2"] }
cid = { version = "0.10.1", features = ["serde"] }

# Runtime-specific dependencies
wasmer = "3.1"
wasmer-wasi = "3.1"
did-method-key = "0.2"
hashbrown = "0.14"
merkle-cbt = "0.3"
libipld-core = "0.13.1"
backoff = "0.4.0"
EOF

# 3. Fix runtime Cargo.toml if requested
if [ "$FIX_RUNTIME" = true ]; then
  echo -e "${YELLOW}Fixing runtime Cargo.toml...${NC}"
  cat > runtime/Cargo.toml << 'EOF'
[package]
name = "icn-runtime-root"
version = "0.1.0"
edition = "2021"
publish = false

# Runtime package dependencies
[dependencies]
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
futures = { workspace = true }
EOF
fi

# 4. Fix wallet Cargo.toml if requested
if [ "$FIX_WALLET" = true ]; then
  echo -e "${YELLOW}Fixing wallet Cargo.toml...${NC}"
  cat > wallet/Cargo.toml << 'EOF'
[package]
name = "icn-wallet-root"
version = "0.1.0"
edition = "2021"
publish = false

# Wallet package dependencies
[dependencies]
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
futures = { workspace = true }
EOF

  # Create wallet-types if it doesn't exist
  if [ ! -d "wallet/crates/wallet-types" ]; then
    echo -e "${YELLOW}Creating wallet-types directory...${NC}"
    mkdir -p wallet/crates/wallet-types/src
  fi

  # Fix wallet-types Cargo.toml
  echo -e "${YELLOW}Fixing wallet-types Cargo.toml...${NC}"
  cat > wallet/crates/wallet-types/Cargo.toml << 'EOF'
[package]
name = "wallet-types"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
chrono = { version = "0.4", features = ["serde"] }
EOF

  # Create essential modules for wallet-types
  echo -e "${YELLOW}Creating essential wallet-types modules...${NC}"
  
  # Create lib.rs
  cat > wallet/crates/wallet-types/src/lib.rs << 'EOF'
//! Common types shared between wallet components

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

pub mod error;
pub mod action;
pub mod network;
pub mod dag;

/// Re-exports
pub use error::{SharedError, SharedResult};
pub use action::{ActionType, ActionStatus};
pub use network::{NetworkStatus, NodeSubmissionResponse};
pub use dag::DagNode;
pub use dag::DagThread;

/// Trust bundle for federation governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    pub id: String,
    pub epoch: u64,
    pub guardians: Vec<String>,
    pub members: Vec<String>,
    pub policies: HashMap<String, String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Bundle expiration timestamp (optional)
    pub valid_until: Option<SystemTime>,
    /// Federation ID
    pub federation_id: String,
    /// Version number
    pub version: u32,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Whether this bundle is active
    pub active: bool,
    /// Signature threshold
    pub threshold: u32,
    /// Signatures map
    pub signatures: HashMap<String, String>,
    /// Links to related resources
    #[serde(default)]
    pub links: HashMap<String, String>,
}
EOF

  # Create error.rs
  cat > wallet/crates/wallet-types/src/error.rs << 'EOF'
use thiserror::Error;
use std::io;

/// Common error type used across wallet components
#[derive(Error, Debug)]
pub enum SharedError {
    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    /// Resource not found
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    
    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// Timeout error
    #[error("Timeout error: {0}")]
    TimeoutError(String),
    
    /// Generic error
    #[error("{0}")]
    GenericError(String),
}

/// Convenient Result type alias using SharedError
pub type SharedResult<T> = Result<T, SharedError>;
EOF

  # Create network.rs
  cat > wallet/crates/wallet-types/src/network.rs << 'EOF'
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Network status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    /// Whether the network is online
    pub online: bool,
    
    /// Network type (e.g., "testnet", "mainnet")
    pub network_type: String,
    
    /// Number of connected peers
    pub peer_count: u32,
    
    /// Current block height
    pub block_height: u64,
    
    /// Network latency in milliseconds
    pub latency_ms: u64,
    
    /// Sync status percentage (0-100)
    pub sync_percent: u8,
    
    /// Additional status information
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Response from node after submitting data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSubmissionResponse {
    /// Success status
    pub success: bool,
    
    /// Transaction or submission ID
    pub id: String,
    
    /// Timestamp of the submission
    pub timestamp: String,
    
    /// Block number (if applicable)
    pub block_number: Option<u64>,
    
    /// Error message (if any)
    pub error: Option<String>,
    
    /// Additional response data
    #[serde(default)]
    pub data: HashMap<String, String>,
}
EOF

  # Create action.rs
  cat > wallet/crates/wallet-types/src/action.rs << 'EOF'
use serde::{Deserialize, Serialize};

/// Types of actions that can be performed by wallet components
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    /// Create a new item
    Create,
    
    /// Update an existing item
    Update,
    
    /// Delete an item
    Delete,
    
    /// Submit data to the network
    Submit,
    
    /// Query or fetch data
    Query,
    
    /// Synchronize data
    Sync,
    
    /// Import data
    Import,
    
    /// Export data
    Export,
    
    /// Sign data
    Sign,
    
    /// Verify signature
    Verify,
    
    /// Approve an action
    Approve,
    
    /// Reject an action
    Reject,
}

/// Status of an action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStatus {
    /// Action is pending
    Pending,
    
    /// Action is in progress
    InProgress,
    
    /// Action completed successfully
    Completed,
    
    /// Action failed
    Failed,
    
    /// Action was cancelled
    Cancelled,
    
    /// Action requires approval
    RequiresApproval,
    
    /// Action is expired
    Expired,
}
EOF

  # Create dag.rs
  cat > wallet/crates/wallet-types/src/dag.rs << 'EOF'
//! DAG-related data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// DAG node structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// CID of the node
    pub cid: String,
    
    /// Parent CIDs
    pub parents: Vec<String>,
    
    /// Epoch number
    pub epoch: u64,
    
    /// Creator DID
    pub creator: String,
    
    /// Timestamp
    pub timestamp: SystemTime,
    
    /// Content type
    pub content_type: String,
    
    /// Node content (JSON)
    pub content: serde_json::Value,
    
    /// Signatures map
    pub signatures: HashMap<String, String>,
    
    /// Binary data for the node (if applicable)
    pub data: Option<Vec<u8>>,
    
    /// Node links (for IPLD compatibility) - map of name to CID
    pub links: HashMap<String, String>,
    
    /// Created time for the node
    pub created_at: Option<SystemTime>,
}

/// DAG Thread structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagThread {
    /// Thread ID
    pub id: String,
    
    /// Thread type
    pub thread_type: String,
    
    /// Root node CID
    pub root_cid: String,
    
    /// List of node CIDs in this thread
    pub nodes: Vec<String>,
    
    /// Last updated timestamp
    pub last_updated: SystemTime,
    
    /// The latest CID in the thread
    pub latest_cid: String,
}
EOF
fi

# 5. Setup AgoraNet database if requested
if [ "$SETUP_DB" = true ]; then
  if [ -f "./setup_agoranet_db.sh" ]; then
    echo -e "${YELLOW}Setting up AgoraNet database...${NC}"
    ./setup_agoranet_db.sh
  else
    echo -e "${RED}Database setup script (setup_agoranet_db.sh) not found!${NC}"
    exit 1
  fi
fi

# 6. Fix AgoraNet configuration if requested
if [ "$FIX_AGORANET" = true ]; then
  # First, update root Cargo.toml to include AgoraNet
  sed -i 's/\(members = \[\)/\1\n    "agoranet",/g' Cargo.toml
  
  echo -e "${YELLOW}AgoraNet setup requires manual configuration.${NC}"
  echo -e "${YELLOW}Please run ./setup_agoranet_db.sh to set up the database.${NC}"
fi

# 7. Build the monorepo
echo -e "${YELLOW}Building the monorepo...${NC}"

OPTIONS=""
if [ "$FIX_RUNTIME" = true ]; then
  OPTIONS="$OPTIONS -p icn-runtime-root -p icn-dag -p icn-core-vm"
fi

if [ "$FIX_WALLET" = true ]; then
  OPTIONS="$OPTIONS -p icn-wallet-root -p wallet-types"
fi

if [ "$FIX_AGORANET" = true ]; then
  OPTIONS="$OPTIONS -p icn-agoranet"
  # Export DATABASE_URL for AgoraNet build
  export DATABASE_URL=postgres://postgres:postgres@localhost:5432/icn_agoranet
  export SQLX_OFFLINE=true
fi

if [ -n "$OPTIONS" ]; then
  echo -e "${YELLOW}Running: cargo build $OPTIONS${NC}"
  cargo build $OPTIONS
else
  echo -e "${YELLOW}Running: cargo build${NC}"
  cargo build
fi

echo -e "${GREEN}Monorepo build complete!${NC}"
echo -e "${GREEN}You can now use the verify_build.sh script to verify the build status.${NC}" 