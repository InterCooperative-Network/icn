#!/bin/bash
set -e

# Colors for better output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting monorepo fixing script${NC}"

# 1. Clean any previous build artifacts
echo -e "${YELLOW}Cleaning previous build artifacts...${NC}"
cargo clean || echo "Couldn't clean, continuing anyway"

# 2. First fix the wallet components circular dependency

# Check if wallet-types exists
if [ -d "wallet/crates/wallet-types" ]; then
  echo -e "${GREEN}wallet-types directory exists, continuing...${NC}"
else
  echo -e "${RED}wallet-types directory doesn't exist. Creating it...${NC}"
  mkdir -p wallet/crates/wallet-types/src
  
  # Create a basic Cargo.toml for wallet-types
  cat > wallet/crates/wallet-types/Cargo.toml << 'EOF'
[package]
name = "wallet-types"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
cid = { version = "0.10.1", features = ["serde"] }
multihash = "0.16.3"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4", "serde"] }
EOF

  # Create a basic lib.rs for wallet-types
  cat > wallet/crates/wallet-types/src/lib.rs << 'EOF'
//! Common types shared between wallet components

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

pub mod error;
pub mod network;
pub mod dag;

/// Re-exports
pub use dag::DagNode;
pub use dag::DagThread;
pub use network::NodeSubmissionResponse;
pub use network::NetworkStatus;

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
}
EOF

  # Create common error types module
  cat > wallet/crates/wallet-types/src/error.rs << 'EOF'
//! Common error types for wallet components

use thiserror::Error;

/// Common wallet error type
#[derive(Debug, Error)]
pub enum WalletError {
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Sync error: {0}")]
    SyncError(String),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type WalletResult<T> = Result<T, WalletError>;
EOF

  # Create network types
  cat > wallet/crates/wallet-types/src/network.rs << 'EOF'
//! Network-related types

use serde::{Deserialize, Serialize};

/// Status of the network connection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkStatus {
    Connected,
    Disconnected,
    Reconnecting,
    Error(String),
}

/// Response from node when submitting data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSubmissionResponse {
    pub success: bool,
    pub message: String,
    pub cid: Option<String>,
    pub timestamp: Option<u64>,
}
EOF

  # Create DAG types
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
    
    /// Node links (for IPLD compatibility)
    pub links: Vec<String>,
    
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
}
EOF

  echo -e "${GREEN}Successfully created wallet-types crate${NC}"
fi

# 3. Fix wallet-sync to use wallet-types and remove circular dependency
if grep -q "wallet_agent" "wallet/crates/wallet-sync/Cargo.toml"; then
  echo -e "${YELLOW}Fixing wallet-sync Cargo.toml...${NC}"
  
  # Backup original
  cp wallet/crates/wallet-sync/Cargo.toml wallet/crates/wallet-sync/Cargo.toml.bak
  
  # Update dependency to use wallet-types instead of wallet-agent
  sed -i 's/wallet-agent/wallet-types/g' wallet/crates/wallet-sync/Cargo.toml
  
  echo -e "${GREEN}Updated wallet-sync Cargo.toml${NC}"
fi

# 4. Fix the wallet-agent crate if it exists
if [ -d "wallet/crates/wallet-agent" ]; then
  echo -e "${YELLOW}Checking wallet-agent Cargo.toml...${NC}"
  
  # Backup original
  cp wallet/crates/wallet-agent/Cargo.toml wallet/crates/wallet-agent/Cargo.toml.bak
  
  # Make sure it depends on wallet-types
  if ! grep -q "wallet-types" "wallet/crates/wallet-agent/Cargo.toml"; then
    echo -e "${YELLOW}Adding wallet-types dependency to wallet-agent${NC}"
    sed -i '/\[dependencies\]/a wallet-types = { path = "../wallet-types", version = "0.1.0" }' wallet/crates/wallet-agent/Cargo.toml
  fi
  
  echo -e "${GREEN}Checked wallet-agent Cargo.toml${NC}"
fi

# 5. Update the main workspace Cargo.toml to ensure correct dependency versions
echo -e "${YELLOW}Ensuring workspace dependencies are correct...${NC}"

# Add backoff dependency to the workspace
if ! grep -q "backoff" "Cargo.toml"; then
  echo -e "${YELLOW}Adding backoff dependency to workspace${NC}"
  sed -i '/libipld-core = "0.13.1"/a backoff = "0.4.0"' Cargo.toml
fi

echo -e "${GREEN}Fixed monorepo structure! Try building a specific component now:${NC}"
echo -e "${YELLOW}cargo build -p icn-agoranet${NC}" 