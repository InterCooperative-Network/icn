#!/bin/bash

# sync-docs.sh
# Copies markdown documentation from icn-docs to icn-website/src/content/docs/

# Exit immediately if a command exits with a non-zero status.
set -e

# Define source and destination paths
# SCRIPT_DIR is the directory where this script resides (icn-website/scripts)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBSITE_ROOT_DIR="$(dirname "$SCRIPT_DIR")" # icn-website
DOCS_SOURCE_DIR="$WEBSITE_ROOT_DIR/../icn-docs" # ../icn-docs relative to website root
DOCS_DEST_DIR="$WEBSITE_ROOT_DIR/src/content/docs"

# Ensure the destination directory exists
mkdir -p "$DOCS_DEST_DIR"

echo "Starting documentation sync from $DOCS_SOURCE_DIR to $DOCS_DEST_DIR"

# Define specific file mappings from icn-docs structure to website content
# Format: "source_path:dest_file"
declare -A file_mappings=(
    ["api/core-crates.md"]="api.md"
    ["architecture/system-overview.md"]="architecture.md"
    ["economy/README.md"]="economy.md"
    ["governance/README.md"]="governance.md"
    ["networking/README.md"]="networking.md"
    ["observability/README.md"]="observability.md"
    ["philosophy/README.md"]="philosophy.md"
    ["rfcs/README.md"]="rfcs.md"
    ["onboarding/README.md"]="onboarding.md"
    ["cli/README.md"]="cli.md"
)

# Counter for found files
found_count=0
missing_count=0

# Process each mapping
for source_path in "${!file_mappings[@]}"; do
    dest_file="${file_mappings[$source_path]}"
    full_source_path="$DOCS_SOURCE_DIR/$source_path"
    full_dest_path="$DOCS_DEST_DIR/$dest_file"
    
    echo "Processing: $source_path -> $dest_file"
    
    if [ -f "$full_source_path" ]; then
        echo "  ✅ Found $full_source_path. Copying to $full_dest_path"
        cp "$full_source_path" "$full_dest_path"
        found_count=$((found_count + 1))
    else
        echo "  ❌ Missing: $full_source_path"
        missing_count=$((missing_count + 1))
        
        # Create a placeholder file indicating the content is coming soon
        cat > "$full_dest_path" << EOF
# ${dest_file%.*} Documentation

This documentation section is currently being developed.

Please check back soon or visit the [main documentation repository](https://github.com/InterCooperative-Network/icn-docs) for the latest updates.

## Coming Soon

We're actively working on comprehensive documentation for this section of the InterCooperative Network.
EOF
        echo "  📝 Created placeholder for $dest_file"
    fi
done

echo ""
echo "=============================================="
echo "Documentation sync complete!"
echo "✅ Successfully copied: $found_count files"
if [ $missing_count -gt 0 ]; then
    echo "📝 Created placeholders: $missing_count files"
fi
echo "=============================================="

# Additional helpful output
echo ""
echo "Next steps:"
echo "1. Review the synced files in: $DOCS_DEST_DIR"
echo "2. Test the website: npm run dev"
echo "3. Build for production: npm run build"

# Make the script executable from the command line
# chmod +x scripts/sync-docs.sh
echo "Run 'chmod +x scripts/sync-docs.sh' to make this script executable." 