#!/bin/bash
set -e

# Default to not failing on mismatches
FAIL_ON_MISMATCH=${1:-0}
MISMATCHES_FOUND=0

# Find all Cargo.toml files and check for name/directory mismatches
find . -name "Cargo.toml" | while read path; do
  crate_dir=$(dirname "$path")
  pkg_name=$(grep '^name *= *' "$path" | head -1 | sed -E 's/name *= *"([^"]+)"/\1/')
  
  # Skip if no package name found
  if [ -z "$pkg_name" ]; then
    continue
  fi
  
  # Skip workspace root Cargo.toml
  if [ "$crate_dir" = "." ]; then
    continue
  fi
  
  # Extract the last component of the directory path
  dir_name=$(basename "$crate_dir")
  
  # Check if directory name and package name match
  if [[ "$dir_name" != "$pkg_name" ]]; then
    echo "MISMATCH: Directory '$crate_dir' has package name '$pkg_name'"
    MISMATCHES_FOUND=1
  fi
done

# Exit with an error if mismatches were found and FAIL_ON_MISMATCH=1
if [ "$MISMATCHES_FOUND" -eq 1 ] && [ "$FAIL_ON_MISMATCH" -eq 1 ]; then
  echo "ERROR: Directory/package name mismatches found!"
  exit 1
fi

# If no mismatches or not failing on mismatches, exit success
if [ "$MISMATCHES_FOUND" -eq 0 ]; then
  echo "SUCCESS: All directories match their package names."
fi

exit 0 