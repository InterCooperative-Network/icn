#!/bin/bash
# Hook: PreToolUse guard for meaning firewall violations
# Blocks Edit/Write to kernel crates if the content introduces domain imports
#
# Receives JSON on stdin with tool_input containing file_path and content

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Only check Rust source files in kernel crates
if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

# Define kernel crate paths
KERNEL_CRATES="icn-net|icn-gateway|icn-gossip|icn-ledger|icn-core|icn-store|icn-kernel-api"

# Check if file is in a kernel crate
if ! echo "$FILE_PATH" | grep -qE "crates/(${KERNEL_CRATES})/"; then
  exit 0
fi

# Only check .rs files
if [[ "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# Check new content for domain imports
NEW_CONTENT=$(echo "$INPUT" | jq -r '.tool_input.new_string // .tool_input.content // empty')

if [[ -z "$NEW_CONTENT" ]]; then
  exit 0
fi

# Check for forbidden domain imports
VIOLATIONS=""

if echo "$NEW_CONTENT" | grep -qE 'use icn_trust::'; then
  VIOLATIONS="${VIOLATIONS}\n  - imports icn_trust (domain crate) in kernel code"
fi

if echo "$NEW_CONTENT" | grep -qE 'use icn_governance::'; then
  VIOLATIONS="${VIOLATIONS}\n  - imports icn_governance (domain crate) in kernel code"
fi

if echo "$NEW_CONTENT" | grep -qE 'use icn_ccl::'; then
  VIOLATIONS="${VIOLATIONS}\n  - imports icn_ccl (domain crate) in kernel code"
fi

if echo "$NEW_CONTENT" | grep -qE 'use icn_coop::'; then
  VIOLATIONS="${VIOLATIONS}\n  - imports icn_coop (domain crate) in kernel code"
fi

if echo "$NEW_CONTENT" | grep -qE 'TrustClass|TrustGraph|GovernanceRole'; then
  VIOLATIONS="${VIOLATIONS}\n  - references domain types (TrustClass/TrustGraph/GovernanceRole) in kernel code"
fi

if [[ -n "$VIOLATIONS" ]]; then
  echo "MEANING FIREWALL VIOLATION in ${FILE_PATH}:${VIOLATIONS}" >&2
  echo "Kernel crates must not reference domain types. Use ConstraintSet and PolicyOracle instead." >&2
  exit 2
fi

exit 0
