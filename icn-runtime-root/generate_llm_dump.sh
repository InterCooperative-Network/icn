#!/bin/bash

# Script to generate a single text file dump of the ICN Runtime repository,
# optimized for Large Language Model (LLM) context ingestion.
# Includes source code, configs, documentation, examples, and relevant project files.

# Define the output file name
OUTPUT_FILE="llm_context_dump.txt"

# Define directories to explicitly exclude
EXCLUDE_DIRS=(
    "./.git"
    "./target"
    "./.vscode"
    "./.idea"
    # Add any other large or irrelevant directories if needed
    "./.cursor_journal" # Exclude the journal itself
    "./agent_journal"  # Exclude alternative journal name
)

# Define file patterns to explicitly include
# (Order matters less here, find handles it)
INCLUDE_PATTERNS=(
    "*.rs"                # Rust source code
    "*.toml"              # Cargo config, potentially others
    "*.md"                # Markdown documentation (README, docs/, CONTRIB, etc.)
    "*.ccl"               # Contract Chain Language examples/templates
    "*.dsl"               # DSL examples/scripts
    "Makefile"            # Build scripts
    "*.yml"               # GitHub Actions workflows, potentially others
    "*.sh"                # Shell scripts (like this one)
    ".gitignore"          # Git ignore rules
    "LICENSE*"            # License files (LICENSE, LICENSE.md, etc.)
    "CONTRIBUTING*"       # Contribution guidelines
    "CODE_OF_CONDUCT*"    # Code of Conduct
    "CHANGELOG*"          # Changelog file
    ".editorconfig"       # Editor configuration
    ".rustfmt.toml"       # Rust formatting configuration
    ".aicursor_context"   # AI Context pointer file
    "PROJECT_CONTEXT.md"  # Alternative AI context file name
    # Add other relevant text-based file types if needed
)

# Define specific files/patterns to explicitly exclude
EXCLUDE_FILES=(
    "./Cargo.lock"        # Lock file is noisy and generated
    # Add any other specific files to exclude
)

# --- Script Logic ---

# Build the find command exclusion part for directories
exclude_path_args=()
for dir in "${EXCLUDE_DIRS[@]}"; do
    exclude_path_args+=(-path "$dir" -prune -o)
done

# Build the find command exclusion part for specific files
for file_pattern in "${EXCLUDE_FILES[@]}"; do
    exclude_path_args+=(-path "$file_pattern" -prune -o)
done

# Build the find command inclusion part for file patterns
include_name_args=()
for pattern in "${INCLUDE_PATTERNS[@]}"; do
    # Use -name for simple patterns, -iname for case-insensitive if needed
    # For LICENSE*, CONTRIBUTING*, etc., -name is appropriate
    include_name_args+=(-name "$pattern" -o)
done
# Remove the last trailing '-o' if arguments were added
if [ ${#include_name_args[@]} -gt 0 ]; then
    unset 'include_name_args[${#include_name_args[@]}-1]'
fi

# --- File Generation ---

# Clear the output file or create it
echo "Generating Comprehensive LLM context dump in $OUTPUT_FILE..." > "$OUTPUT_FILE"
echo "Repository Root: $(pwd)" >> "$OUTPUT_FILE"
echo "Timestamp: $(date)" >> "$OUTPUT_FILE"
echo "========================================" >> "$OUTPUT_FILE"
echo "Included File Types: ${INCLUDE_PATTERNS[*]}" >> "$OUTPUT_FILE"
echo "Excluded Dirs: ${EXCLUDE_DIRS[*]}" >> "$OUTPUT_FILE"
echo "Excluded Files: ${EXCLUDE_FILES[*]}" >> "$OUTPUT_FILE"
echo "========================================" >> "$OUTPUT_FILE"


# Find relevant files using the constructed arguments and append content
# Using process substitution and a while loop for robustness
while IFS= read -r file; do
    # Skip if file is empty or doesn't exist (safety check)
    if [ -s "$file" ]; then
        echo -e "\n\n--- File: $file ---" >> "$OUTPUT_FILE"
        # Attempt to cat, handle potential errors gracefully
        cat "$file" >> "$OUTPUT_FILE" || echo "Error reading file: $file" >> "$OUTPUT_FILE"
    else
         echo -e "\n\n--- File (Skipped - Empty or Unreadable): $file ---" >> "$OUTPUT_FILE"
    fi
done < <(find . "${exclude_path_args[@]}" \( "${include_name_args[@]}" \) -type f -print)
# Note: Ensures only regular files (-type f) are included

echo "========================================" >> "$OUTPUT_FILE"
echo "LLM context dump generation complete: $OUTPUT_FILE"

exit 0