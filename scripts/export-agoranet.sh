#!/bin/bash
set -euo pipefail

# ICN AgoraNet Export Script
# This script extracts the AgoraNet component from the ICN monorepo
# into a standalone repository for deployment

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXPORT_DIR="${REPO_ROOT}/export/icn-agoranet"
TEMP_DIR="${EXPORT_DIR}_temp"

echo "🚀 Exporting AgoraNet to standalone repository..."

# Create temp directory
rm -rf "${TEMP_DIR}" || true
mkdir -p "${TEMP_DIR}"

# Copy AgoraNet files
echo "📂 Copying AgoraNet files..."
cp -r "${REPO_ROOT}/agoranet" "${TEMP_DIR}/"

# Copy database setup scripts if they exist
if [ -f "${REPO_ROOT}/scripts/setup_agoranet_db.sh" ]; then
  echo "🗃️ Copying database setup scripts..."
  mkdir -p "${TEMP_DIR}/scripts"
  cp "${REPO_ROOT}/scripts/setup_agoranet_db.sh" "${TEMP_DIR}/scripts/"
fi

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
  "agoranet"
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
axum = "0.7"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "uuid", "time", "migrate"] }
tokio = { version = "1.34", features = ["full"] }
tracing = "0.1"
uuid = { version = "1.6", features = ["v4", "serde"] }
EOF

# Create Docker Compose file for local development
echo "🐳 Creating Docker Compose configuration..."
cat > "${TEMP_DIR}/docker-compose.yml" << EOF
version: '3.8'

services:
  db:
    image: postgres:16
    environment:
      POSTGRES_USER: agoranet
      POSTGRES_PASSWORD: agoranet
      POSTGRES_DB: agoranet
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

  agoranet:
    build: .
    ports:
      - "3000:3000"
    environment:
      - DATABASE_URL=postgres://agoranet:agoranet@db:5432/agoranet
    depends_on:
      - db

volumes:
  postgres_data:
EOF

# Create a simple Dockerfile
echo "🐳 Creating Dockerfile..."
cat > "${TEMP_DIR}/Dockerfile" << EOF
FROM rust:1.76 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/agoranet /app/agoranet
COPY --from=builder /app/agoranet/migrations /app/migrations

ENV DATABASE_URL=postgres://agoranet:agoranet@db:5432/agoranet

EXPOSE 3000

CMD ["/app/agoranet"]
EOF

# Create README.md
echo "📝 Creating README.md..."
cat > "${TEMP_DIR}/README.md" << EOF
# ICN AgoraNet

AgoraNet is a federated deliberation system supporting threads, proposal linking, federation syncing, and public discussion.

## Features

* **Thread Management**: Create, view, and respond to discussion threads
* **Proposal Linking**: Connect discussions to on-chain proposals
* **Federation Sync**: Synchronize data across federated instances
* **Public Discussion**: Open, accessible forums for community input

## Development

### Prerequisites

* Rust 1.76 or higher
* PostgreSQL 16

### Setup

\`\`\`bash
# Setup the database
./scripts/setup_agoranet_db.sh

# Build the project
cargo build

# Run database migrations
cargo run --bin agoranet -- migrate

# Start the server
cargo run
\`\`\`

### Using Docker Compose

\`\`\`bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f
\`\`\`

## API Documentation

The API documentation is available at http://localhost:3000/api/docs when the server is running.

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
.env
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
  SQLX_OFFLINE: true

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
      run: docker build -t icn-agoranet .
EOF

# Finalize the export
echo "✅ Creating final export..."
rm -rf "${EXPORT_DIR}" || true
mv "${TEMP_DIR}" "${EXPORT_DIR}"

echo "✨ AgoraNet export complete! Repository available at: ${EXPORT_DIR}" 