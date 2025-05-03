#!/bin/bash
set -euo pipefail

# ICN Wallet Export Script
# This script extracts the wallet component from the ICN monorepo
# into a standalone repository for deployment

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXPORT_DIR="${REPO_ROOT}/export/icn-wallet"
TEMP_DIR="${EXPORT_DIR}_temp"

echo "🚀 Exporting wallet to standalone repository..."

# Create temp directory
rm -rf "${TEMP_DIR}" || true
mkdir -p "${TEMP_DIR}"

# Copy wallet files
echo "📂 Copying wallet files..."
cp -r "${REPO_ROOT}/wallet" "${TEMP_DIR}/"
cp -r "${REPO_ROOT}/wallet-types" "${TEMP_DIR}/" 2>/dev/null || true

# Copy shared dependencies needed by wallet
echo "🧩 Copying shared dependencies..."
mkdir -p "${TEMP_DIR}/shared"
if [ -d "${REPO_ROOT}/crates/dag-core" ]; then
  mkdir -p "${TEMP_DIR}/shared/dag-core"
  cp -r "${REPO_ROOT}/crates/dag-core" "${TEMP_DIR}/shared/"
fi

# Create root Cargo.toml
echo "📄 Creating root Cargo.toml..."
cat > "${TEMP_DIR}/Cargo.toml" << EOF
[workspace]
resolver = "2"
members = [
  "wallet/crates/*",
]

# Add shared dependencies if they exist
members_fallback = []
EOF

if [ -d "${TEMP_DIR}/wallet-types" ]; then
  echo '  "wallet-types",' >> "${TEMP_DIR}/Cargo.toml"
fi

if [ -d "${TEMP_DIR}/shared/dag-core" ]; then
  echo '  "shared/dag-core",' >> "${TEMP_DIR}/Cargo.toml"
fi

# Finish Cargo.toml
cat >> "${TEMP_DIR}/Cargo.toml" << EOF

[workspace.dependencies]
# Add common dependencies here
anyhow = "1.0"
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.34", features = ["full"] }
tracing = "0.1"
uuid = { version = "1.6", features = ["v4", "serde"] }
EOF

# Create README.md
echo "📝 Creating README.md..."
cat > "${TEMP_DIR}/README.md" << EOF
# ICN Wallet

The ICN Wallet is a mobile-first agent for offline-capable DAG interaction, DID/VC handling, scoped token usage, and proposal syncing.

## Features

* **DID & VC Support**: \`did:key\` Ed25519 identities, Verifiable Credential storage and issuance, Selective disclosure
* **Resource Token System**: Scoped token minting, transfer, and metering
* **Offline-Capable DAG Agent**: Local DAG thread cache, Action queueing with signature + replay
* **Secure Storage**: Platform-native secure storage with encryption fallbacks

## Development

\`\`\`bash
# Build all wallet components
cargo build

# Run tests
cargo test
\`\`\`

## License

Copyright (c) InterCooperative Network Contributors
Licensed under the Apache License, Version 2.0
EOF

# Create .gitignore
echo "🔍 Creating .gitignore..."
cat > "${TEMP_DIR}/.gitignore" << EOF
/target
**/*.rs.bk
Cargo.lock
.DS_Store
.idea/
.vscode/
*.iml
.keys/
EOF

# Setup GitHub Actions workflow
echo "🔄 Setting up CI workflow..."
mkdir -p "${TEMP_DIR}/.github/workflows"
cat > "${TEMP_DIR}/.github/workflows/ci.yml" << EOF
name: CI

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    - name: Build
      run: cargo build --verbose
    - name: Run tests
      run: cargo test --verbose
EOF

# Finalize the export
echo "✅ Creating final export..."
rm -rf "${EXPORT_DIR}" || true
mv "${TEMP_DIR}" "${EXPORT_DIR}"

echo "✨ Wallet export complete! Repository available at: ${EXPORT_DIR}" 