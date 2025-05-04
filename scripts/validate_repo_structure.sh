#!/bin/bash

# ICN Monorepo Structure Validation Script
# This script verifies that the repository structure meets the defined standards

set -e  # Exit on error

echo "Starting ICN monorepo structure validation..."

# Define expected top-level directories
TOP_LEVEL_DIRS=("agoranet" "docs" "frontend" "runtime" "scripts" "tools" "wallet")

# Define expected crate directories
RUNTIME_CRATES=("common" "core-vm" "dag" "economics" "federation" "governance-kernel" "storage")
WALLET_CRATES=("actions" "api" "ffi" "identity" "storage" "sync" "wallet-agent" "wallet-core" "wallet-ffi" "wallet-types")
TOOLS=("health_check" "icn-verifier")

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Error and warning counters
errors=0
warnings=0

# Function to check if a directory exists
check_dir() {
  if [ ! -d "$1" ]; then
    echo -e "${RED}ERROR: Directory $1 does not exist${NC}"
    ((errors++))
    return 1
  else
    echo -e "${GREEN}✓ Directory $1 exists${NC}"
    return 0
  fi
}

# Function to check if a file exists
check_file() {
  if [ ! -f "$1" ]; then
    echo -e "${RED}ERROR: File $1 does not exist${NC}"
    ((errors++))
    return 1
  else
    echo -e "${GREEN}✓ File $1 exists${NC}"
    return 0
  fi
}

# Function to check if a Cargo.toml includes workspace dependencies
check_workspace_deps() {
  if grep -q "workspace = true" "$1"; then
    echo -e "${GREEN}✓ $1 uses workspace dependencies${NC}"
    return 0
  else
    echo -e "${YELLOW}WARNING: $1 may not be using workspace dependencies${NC}"
    ((warnings++))
    return 1
  fi
}

# 1. Check that all expected directories exist
echo -e "\n=== Checking top-level directory structure ==="
for dir in "${TOP_LEVEL_DIRS[@]}"; do
  check_dir "./$dir"
done

# 2. Check for Cargo.toml at root
echo -e "\n=== Checking root Cargo.toml ==="
check_file "./Cargo.toml"

# 3. Check for README.md at root
echo -e "\n=== Checking root README.md ==="
check_file "./README.md"

# 4. Check runtime crates
echo -e "\n=== Checking runtime crates ==="
check_dir "./runtime/crates"
for crate in "${RUNTIME_CRATES[@]}"; do
  if check_dir "./runtime/crates/$crate"; then
    check_file "./runtime/crates/$crate/Cargo.toml"
    check_workspace_deps "./runtime/crates/$crate/Cargo.toml"
  fi
done

# 5. Check wallet crates
echo -e "\n=== Checking wallet crates ==="
check_dir "./wallet/crates"
for crate in "${WALLET_CRATES[@]}"; do
  if check_dir "./wallet/crates/$crate"; then
    check_file "./wallet/crates/$crate/Cargo.toml"
    check_workspace_deps "./wallet/crates/$crate/Cargo.toml"
  fi
done

# 6. Check tools
echo -e "\n=== Checking tools ==="
check_dir "./tools"
for tool in "${TOOLS[@]}"; do
  if check_dir "./tools/$tool"; then
    check_file "./tools/$tool/Cargo.toml"
    check_workspace_deps "./tools/$tool/Cargo.toml"
  fi
done

# 7. Check for orphaned files in root
echo -e "\n=== Checking for orphaned files in root ==="
orphaned_rs=$(find . -maxdepth 1 -name "*.rs" | wc -l)
orphaned_md=$(find . -maxdepth 1 -name "*.md" -not -name "README.md" | wc -l)

if [ "$orphaned_rs" -gt 0 ]; then
  echo -e "${RED}ERROR: Found $orphaned_rs orphaned .rs files in root:${NC}"
  find . -maxdepth 1 -name "*.rs" -exec echo "  - {}" \;
  ((errors++))
else
  echo -e "${GREEN}✓ No orphaned .rs files in root${NC}"
fi

if [ "$orphaned_md" -gt 0 ]; then
  echo -e "${YELLOW}WARNING: Found $orphaned_md .md files in root (excluding README.md):${NC}"
  find . -maxdepth 1 -name "*.md" -not -name "README.md" -exec echo "  - {}" \;
  ((warnings++))
else
  echo -e "${GREEN}✓ No orphaned .md files in root${NC}"
fi

# 8. Check if Cargo.toml workspace members match actual directory structure
echo -e "\n=== Checking workspace member consistency ==="
workspace_members=$(grep -E "^\s+\"[^\"]+\",$" Cargo.toml | sed 's/[ ",]//g')
for member in $workspace_members; do
  # Skip wildcard entries like runtime/crates/*
  if [[ $member == *"*"* ]]; then
    continue
  fi
  
  if [ ! -d "$member" ]; then
    echo -e "${RED}ERROR: Workspace member $member declared in Cargo.toml but directory not found${NC}"
    ((errors++))
  else
    echo -e "${GREEN}✓ Workspace member $member exists${NC}"
  fi
done

# 9. Check for CI configuration
echo -e "\n=== Checking CI configuration ==="
if [ -d ".github/workflows" ]; then
  echo -e "${GREEN}✓ GitHub Actions workflows directory exists${NC}"
else
  echo -e "${YELLOW}WARNING: No GitHub Actions workflows directory found${NC}"
  ((warnings++))
fi

# Summary
echo -e "\n=== Validation Summary ==="
if [ $errors -eq 0 ] && [ $warnings -eq 0 ]; then
  echo -e "${GREEN}✅ All checks passed successfully!${NC}"
elif [ $errors -eq 0 ]; then
  echo -e "${YELLOW}⚠️ Validation complete with $warnings warnings but no errors.${NC}"
  echo -e "Review warnings to improve repository structure."
else
  echo -e "${RED}❌ Validation failed with $errors errors and $warnings warnings.${NC}"
  echo -e "Please fix the errors before committing."
  exit 1
fi

# Suggest next steps
if [ $errors -eq 0 ]; then
  echo -e "\n=== Recommended Next Steps ==="
  echo "1. Run 'cargo check --workspace' to verify all dependencies"
  echo "2. Run 'cargo test --workspace' to ensure functionality is preserved"
  echo "3. Run 'cargo update' to regenerate Cargo.lock cleanly"
  echo "4. Review changes with 'git status' and 'git diff'"
  echo "5. Commit with 'git add . && git commit -m \"Restructure ICN monorepo for modular federation architecture\"'"
fi

exit $errors 