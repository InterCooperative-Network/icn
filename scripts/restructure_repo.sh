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

# 8. Copy README.md to the root if it exists
if [ -f "README.md" ]; then
  cp README.md ./
fi

# 9. Update Cargo.toml
cp docs/example-Cargo.toml Cargo.toml

echo "Cleaning up temporary files..."
# Only remove original files after confirming copies worked
# rm -rf health_check.rs
# rm -rf health_check
# rm -rf agoranet/health_check
# etc.

echo "ICN monorepo restructuring complete!"
echo "Please review the changes and then run 'cargo check' to verify everything works." 