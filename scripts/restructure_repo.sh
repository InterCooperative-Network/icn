#!/bin/bash

# ICN Monorepo Restructuring Script
# This script implements the migration plan for restructuring the ICN monorepo.

set -e  # Exit on error

echo "Starting ICN monorepo restructuring..."

# 1. Create the directory structure
mkdir -p runtime/crates
mkdir -p wallet/crates
mkdir -p agoranet/crates
mkdir -p frontend
mkdir -p docs/{runtime,wallet,agoranet}
mkdir -p scripts/{deployment,development,testing}
mkdir -p tools/{health_check/src,icn-verifier/src}

# 2. Move wallet-related components
echo "Moving wallet components..."
if [ -d "wallet-ffi" ]; then
  mv wallet-ffi wallet/crates/wallet-ffi
fi
if [ -d "wallet-core" ]; then
  mv wallet-core wallet/crates/wallet-core
fi
if [ -d "wallet-agent" ]; then
  mv wallet-agent wallet/crates/wallet-agent
fi

# 3. Consolidate health check
echo "Consolidating health_check..."
if [ -f "health_check.rs" ]; then
  cp health_check.rs tools/health_check/src/main.rs
fi
if [ -d "health_check" ]; then
  cp -r health_check/* tools/health_check/
fi
if [ -d "agoranet/health_check" ]; then
  cp -r agoranet/health_check/* tools/health_check/
fi

# 4. Move dashboard
echo "Moving dashboard..."
if [ -d "dashboard" ]; then
  mkdir -p frontend/dashboard
  cp -r dashboard/* frontend/dashboard/
fi
if [ -d "agoranet/dashboard" ]; then
  mkdir -p frontend/agoranet-dashboard
  cp -r agoranet/dashboard/* frontend/agoranet-dashboard/
fi

# 5. Move verification tool
echo "Moving verification tool..."
if [ -d "icn-verifier" ]; then
  cp -r icn-verifier/* tools/icn-verifier/
fi

# 6. Centralize documentation
echo "Centralizing documentation..."
if [ -d "runtime" ]; then
  find runtime -name "*.md" -exec cp {} docs/runtime/ \;
fi
if [ -d "wallet" ]; then
  find wallet -name "*.md" -exec cp {} docs/wallet/ \;
fi
if [ -d "agoranet/agoranet-redesign" ]; then
  cp -r agoranet/agoranet-redesign/* docs/agoranet/
fi
if [ -f "refactoring-report.md" ]; then
  cp refactoring-report.md docs/
fi

# 7. Gather scripts
echo "Gathering scripts..."
if [ -d "runtime" ]; then
  find runtime -name "*.sh" -exec cp {} scripts/testing/ \;
fi
if [ -d "agoranet" ]; then
  find agoranet -name "*.sh" -exec cp {} scripts/development/ \;
fi
if [ -f "generate_llm_dump.sh" ]; then
  cp generate_llm_dump.sh scripts/development/
fi

# 8. Generate updated top-level files
echo "Generating updated top-level files..."
cat > Cargo.toml << 'EOF'
[workspace]
resolver = "2"
members = [
  "runtime",
  "runtime/crates/*",
  "wallet",
  "wallet/crates/*",
  "agoranet",
  "agoranet/crates/*",
  "tools/*",
]

exclude = [
  "frontend/*",
  "wallet/crates/sync",
  "runtime/crates/agoranet-integration"
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

# IPLD related dependencies
libipld = { version = "0.14", features = ["derive"] }
libipld-core = "0.13.1"
serde_ipld_dagcbor = "0.4"
ipld-core = "0.3"

# Identity and security
ssi = { version = "0.7", features = ["ed25519", "rsa"] }

# Runtime-specific dependencies
wasmer = "3.1"
wasmer-wasi = "3.1"
did-method-key = "0.2"
hashbrown = "0.14"
merkle-cbt = "0.3"
backoff = "0.4.0"

# Additional commonly used dependencies
base64 = { version = "0.21", features = ["std"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.3", features = ["v4", "serde"] }
rand = "0.8"
sha2 = "0.10"
hex = "0.4"
reqwest = { version = "0.11", features = ["json"] }
ed25519-dalek = "1.0"
axum = "0.7.9"
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-native-tls", "migrate"] }
dotenv = "0.15.0"
tokio-stream = "0.1"

# Web and frontend integration
tower = "0.4"
tower-http = { version = "0.4", features = ["trace", "cors"] }
hyper = { version = "0.14", features = ["full"] }
url = "2.3"
EOF

# If README.md exists, copy it to the root; otherwise create a placeholder
if [ -f "README.md" ]; then
  cp README.md ./
else
  cat > README.md << 'EOF'
# ICN (Identity-Centric Network)

ICN is a decentralized federation network focused on identity management, data synchronization, and governance.

## Repository Structure

This monorepo contains the following components:

- `runtime/`: Core federation logic
- `wallet/`: Identity and sync agent
- `agoranet/`: Deliberation layer
- `tools/`: Standalone utilities
- `frontend/`: User interfaces
- `scripts/`: Utility scripts
- `docs/`: Documentation

For detailed information about the repository structure, see [docs/REPO_STRUCTURE.md](docs/REPO_STRUCTURE.md).

## Getting Started

[Instructions on building and running ICN components...]
EOF
fi

# 9. Clean up and verify
echo "Cleaning up and verifying..."
# Only remove original files after confirming copies worked
# Note: These are commented out by default to ensure safety
# rm -rf health_check.rs
# rm -rf health_check
# rm -rf agoranet/health_check
# rm -f wallet-ffi wallet-core wallet-agent
# rm -rf dashboard
# rm -rf icn-verifier

# 10. Generate .gitignore files if needed
if [ ! -f ".gitignore" ]; then
  cat > .gitignore << 'EOF'
/target
**/*.rs.bk
Cargo.lock
.DS_Store
.env
.env.*
!.env.example
*.swp
*.swo
*.log
node_modules/
.idea/
.vscode/
EOF
fi

echo "ICN monorepo restructuring complete!"
echo "Please review the changes and then run 'cargo check' to verify everything works."
echo "After validation, you may want to regenerate Cargo.lock with 'cargo build --workspace'." 