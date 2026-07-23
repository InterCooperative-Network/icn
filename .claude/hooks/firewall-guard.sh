#!/bin/bash
# Hook: PreToolUse guard for meaning firewall violations
# Blocks Edit/Write to kernel crates if the content introduces domain imports
#
# Crate lists come from kernel-crates.conf, which is sync-checked against the
# single-source taxonomy (scripts/firewall-taxonomy.toml) in CI.
#
# Receives JSON on stdin with tool_input containing file_path and content

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Only check Rust source files in kernel crates
if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

# Load kernel + domain crate lists from centralized config, with hardcoded fallback
HOOK_DIR="$(cd "$(dirname "$0")" && pwd)"
CONF_FILE="${HOOK_DIR}/kernel-crates.conf"
FALLBACK_KERNEL_CRATES="icn-core|icn-net|icn-gossip|icn-store|icn-kernel-api"
FALLBACK_DOMAIN_CRATES="icn-trust|icn-governance|icn-ledger|icn-ccl|icn-compute|icn-entity|icn-community|icn-federation|icn-steward|icn-coop|icn-commons|icn-zkp"
if [[ -f "$CONF_FILE" ]]; then
  KERNEL_CRATES=$(sed -n '/^\[kernel\]/,/^\[/p' "$CONF_FILE" | grep -v '^\[' | grep -v '^#' | grep -v '^$' | tr '\n' '|' | sed 's/|$//')
  DOMAIN_CRATES=$(sed -n '/^\[domain\]/,/^\[/p' "$CONF_FILE" | grep -v '^\[' | grep -v '^#' | grep -v '^$' | tr '\n' '|' | sed 's/|$//')
fi
# Fallback if config absent or sections empty/malformed
KERNEL_CRATES="${KERNEL_CRATES:-$FALLBACK_KERNEL_CRATES}"
DOMAIN_CRATES="${DOMAIN_CRATES:-$FALLBACK_DOMAIN_CRATES}"

# Check if file is in a kernel crate
if ! echo "$FILE_PATH" | grep -qE "crates/(${KERNEL_CRATES})/"; then
  exit 0
fi

# Only check .rs files
if [[ "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# Exclude integration-test paths — they legitimately use domain crates as
# dev-dependencies. (In-file #[cfg(test)] modules cannot be detected from the
# path; the ratchet tests own that surface.)
if echo "$FILE_PATH" | grep -qE '/tests/'; then
  exit 0
fi

# Exclude the ratchet-test file itself — it necessarily contains forbidden
# patterns as string literals (same exclusion its own counter applies).
if [[ "$(basename "$FILE_PATH")" == "meaning_firewall.rs" ]]; then
  exit 0
fi

# Check new content for domain imports
NEW_CONTENT=$(echo "$INPUT" | jq -r '.tool_input.new_string // .tool_input.content // empty')

if [[ -z "$NEW_CONTENT" ]]; then
  exit 0
fi

# Check for forbidden domain imports — one pattern per [domain] crate.
VIOLATIONS=""

IFS='|' read -ra DOMAIN_LIST <<< "$DOMAIN_CRATES"
for crate in "${DOMAIN_LIST[@]}"; do
  module="${crate//-/_}"
  if echo "$NEW_CONTENT" | grep -qE "use ${module}::"; then
    VIOLATIONS="${VIOLATIONS}\n  - imports ${module} (domain crate) in kernel code"
  fi
done

if echo "$NEW_CONTENT" | grep -qE 'TrustClass|TrustGraph|GovernanceRole'; then
  VIOLATIONS="${VIOLATIONS}\n  - references domain types (TrustClass/TrustGraph/GovernanceRole) in kernel code"
fi

if [[ -n "$VIOLATIONS" ]]; then
  echo "MEANING FIREWALL VIOLATION in ${FILE_PATH}:${VIOLATIONS}" >&2
  echo "Kernel crates must not reference domain types. Use ConstraintSet and PolicyOracle instead." >&2
  echo "Crate classes: scripts/firewall-taxonomy.toml (single source of truth)." >&2
  exit 2
fi

exit 0
