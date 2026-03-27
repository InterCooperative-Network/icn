#!/usr/bin/env bash
# PostToolUse hook: Warns when gateway route files are edited
# Reminds to regenerate OpenAPI spec and TypeScript types
#
# Triggered by: Edit/Write on icn-gateway/src/routes/**

set -euo pipefail

# Extract file path from tool input (stdin JSON)
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

# Only trigger for gateway route files
if [[ "$FILE_PATH" == *"icn-gateway/src/routes"* ]] || \
   [[ "$FILE_PATH" == *"icn-gateway/src/api"* ]] || \
   [[ "$FILE_PATH" == *"icn-api/src"*"handler"* ]]; then
    echo "Gateway API route changed. Before pushing, regenerate:"
    echo "  1. cd icn && cargo build -p icnctl"
    echo "  2. ./target/debug/icnctl api export-openapi > ../docs/api/openapi.generated.yaml"
    echo "  3. cd ../sdk/typescript && npm run generate-types"
    exit 0
fi

exit 0
