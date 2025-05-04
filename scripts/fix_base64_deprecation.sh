#!/bin/bash
set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "Fixing deprecated base64::encode calls..."

# Find all Rust files that use base64::encode
FILES_WITH_DEPRECATED=$(grep -r "base64::encode" --include="*.rs" . | cut -d: -f1 | sort | uniq)

if [ -z "$FILES_WITH_DEPRECATED" ]; then
  echo "No files with deprecated base64::encode found."
  exit 0
fi

# Process each file
for file in $FILES_WITH_DEPRECATED; do
  echo "Processing $file..."
  
  # Check if the file already imports base64::engine
  if grep -q "use base64::engine::" "$file"; then
    # File already has the import, just need to update the encode calls
    sed -i 's/base64::encode(\([^)]*\))/base64::engine::general_purpose::STANDARD.encode(\1)/g' "$file"
  else
    # Need to add the import and update the encode calls
    
    # Create a temporary file
    tmp_file=$(mktemp)
    
    # First, find where the base64 import is
    base64_import_line=$(grep -n "use base64" "$file" | head -1 | cut -d: -f1)
    
    if [ -n "$base64_import_line" ]; then
      # Add engine import after the existing base64 import
      sed "${base64_import_line}a use base64::engine::general_purpose::STANDARD;" "$file" > "$tmp_file"
    else
      # No existing base64 import, check if there are other imports to add after
      import_lines=$(grep -n "^use " "$file" | tail -1 | cut -d: -f1)
      
      if [ -n "$import_lines" ]; then
        # Add imports after the last import
        sed "${import_lines}a use base64::engine::general_purpose::STANDARD;" "$file" > "$tmp_file"
      else
        # No imports found, add at the beginning of the file
        echo 'use base64::engine::general_purpose::STANDARD;' > "$tmp_file"
        cat "$file" >> "$tmp_file"
      fi
    fi
    
    # Now update the encode calls
    sed -i 's/base64::encode(\([^)]*\))/STANDARD.encode(\1)/g' "$tmp_file"
    
    # Replace the original file
    mv "$tmp_file" "$file"
  fi
  
  echo "Updated $file"
done

echo "All base64::encode calls have been fixed!"
echo "Note: You may need to update Cargo.toml to ensure base64 crate has the 'engine' feature enabled." 