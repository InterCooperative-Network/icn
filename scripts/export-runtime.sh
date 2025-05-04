#!/bin/bash
set -euo pipefail

# ICN Runtime Export Script
# This script extracts the runtime component from the ICN monorepo
# into a standalone repository for deployment

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXPORT_DIR="${REPO_ROOT}/export/icn-runtime"
TEMP_DIR="${EXPORT_DIR}_temp"

echo "🚀 Exporting runtime to standalone repository..."

# Create temp directory
rm -rf "${TEMP_DIR}" || true
mkdir -p "${TEMP_DIR}"

# Copy runtime files
echo "📂 Copying runtime files..."
cp -r "${REPO_ROOT}/runtime" "${TEMP_DIR}/"

# Copy Dockerfiles and config
echo "🐳 Copying Docker configuration..."
cp "${REPO_ROOT}/runtime/Dockerfile" "${TEMP_DIR}/" 2>/dev/null || true
cp -r "${REPO_ROOT}/runtime/config" "${TEMP_DIR}/" 2>/dev/null || true

# Copy shared dependencies if needed
echo "🧩 Copying shared dependencies..."
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
  "runtime/crates/*",
  "runtime/cli",
]

# Add shared dependencies if they exist
members_fallback = []
EOF

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
wasmtime = "18.0"
EOF

# Create README.md
echo "📝 Creating README.md..."
cat > "${TEMP_DIR}/README.md" << EOF
# ICN Runtime

The ICN Runtime is a stable, federated WASM execution engine with DAG-based governance, scoped economics, and cryptographic identity.

## Features

* **WASM Execution Engine**: Secure sandboxed execution of WebAssembly modules
* **DAG-based Governance**: Directed Acyclic Graph for tracking and managing proposals and decisions
* **Scoped Economics**: Resource token system for metering and accounting
* **Cryptographic Identity**: DID-based identity system with verifiable credentials

## Development

\`\`\`bash
# Build all runtime components
cargo build

# Run tests
cargo test

# Start a development node
./run_integration_node.sh
\`\`\`

## Docker Deployment

\`\`\`bash
# Build Docker image
docker build -t icn-runtime .

# Run container
docker run -p 8080:8080 icn-runtime
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
  docker:
    runs-on: ubuntu-latest
    needs: build
    if: github.ref == 'refs/heads/main'
    steps:
    - uses: actions/checkout@v3
    - name: Build Docker image
      run: docker build -t icn-runtime .
    - name: Run Docker container tests
      run: docker run icn-runtime cargo test --verbose
EOF

# Finalize the export
echo "✅ Creating final export..."
rm -rf "${EXPORT_DIR}" || true
mv "${TEMP_DIR}" "${EXPORT_DIR}"

echo "✨ Runtime export complete! Repository available at: ${EXPORT_DIR}" 