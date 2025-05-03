#!/usr/bin/env bash
# Script to export documentation and code from the ICN monorepo for LLM ingestion
set -e

# Configuration
OUTPUT_FILE="icn_knowledge_base.md"
TEMP_DIR=$(mktemp -d)
REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")
DATE=$(date "+%Y-%m-%d")

# Colors for terminal output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# File extension groupings
CODE_EXTENSIONS=("rs" "ts" "js" "go" "py" "toml" "sh")
DOC_EXTENSIONS=("md" "txt" "rst" "adoc")
CONFIG_EXTENSIONS=("json" "yaml" "yml" "ini" "conf" "xml")

# Directories to exclude
EXCLUDE_DIRS=(".git" "target" "node_modules" ".keys" "dist" "*.lock")
EXCLUDE_PATTERN=$(printf " -not -path \"*/%s/*\"" "${EXCLUDE_DIRS[@]}")

# Header for the output file
echo "# ICN Monorepo Knowledge Base" > "$OUTPUT_FILE"
echo "Generated on: $DATE" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"
echo "## Table of Contents" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Generate TOC later
TOC_PLACEHOLDER_LINE=$(wc -l < "$OUTPUT_FILE")

echo -e "${BLUE}Exporting repository structure...${NC}"

# 1. Repository Structure
echo "## Repository Structure" >> "$OUTPUT_FILE"
echo '```' >> "$OUTPUT_FILE"
find "$REPO_ROOT" -type d -not -path "*/\.*" $EXCLUDE_PATTERN | sort | \
  sed -e "s|$REPO_ROOT/||g" -e '/^$/d' | \
  awk '{for(i=1; i<length($0)-length($0)/gsub("/","/",&0)); i++) printf "  "; print "/"$0}' >> "$OUTPUT_FILE"
echo '```' >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Function to extract code and documentation from files
extract_files() {
    local section_title=$1
    local extensions=("${!2}")
    local file_count=0
    local section_anchor=$(echo "$section_title" | tr '[:upper:]' '[:lower:]' | tr ' ' '-')
    
    # Start section
    echo "## $section_title" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    
    # Create pattern for find command
    local pattern=$(printf " -o -name \"*.%s\"" "${extensions[@]}")
    pattern=${pattern:3}  # Remove initial " -o"
    
    # Find files with specified extensions
    local files=()
    while IFS= read -r file; do
        files+=("$file")
    done < <(find "$REPO_ROOT" -type f \( $pattern \) $EXCLUDE_PATTERN | sort)
    
    # Process each file
    for file in "${files[@]}"; do
        # Get relative path for display
        local rel_path=${file#"$REPO_ROOT/"}
        
        # Skip files that are too large (>1MB)
        local size=$(stat -c%s "$file" 2>/dev/null || stat -f%z "$file" 2>/dev/null)
        if (( size > 1000000 )); then
            echo -e "${YELLOW}Skipping large file: $rel_path ($((size/1024))KB)${NC}"
            echo "### $rel_path (skipped, too large: $((size/1024))KB)" >> "$OUTPUT_FILE"
            echo "" >> "$OUTPUT_FILE"
            continue
        fi
        
        echo -e "${GREEN}Processing: $rel_path${NC}"
        
        # File header
        echo "### $rel_path" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
        
        # Get file info
        local ext="${file##*.}"
        local lines=$(wc -l < "$file")
        
        # File metadata
        echo "**File Info:** $lines lines, $(du -h "$file" | cut -f1) bytes" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
        
        # Start code block with language highlight
        echo '```'"$ext" >> "$OUTPUT_FILE"
        cat "$file" >> "$OUTPUT_FILE"
        echo '```' >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
        
        file_count=$((file_count+1))
    done
    
    # Section summary
    echo "Processed $file_count files in $section_title section"
    echo "" >> "$OUTPUT_FILE"
    
    # Return the section information for TOC
    echo "$section_title:$section_anchor:$file_count"
}

# 2. Documentation Files
echo -e "${BLUE}Processing documentation files...${NC}"
DOC_STATS=$(extract_files "Documentation Files" DOC_EXTENSIONS[@])

# 3. Code files
echo -e "${BLUE}Processing code files...${NC}"
CODE_STATS=$(extract_files "Code Files" CODE_EXTENSIONS[@])

# 4. Configuration files
echo -e "${BLUE}Processing configuration files...${NC}"
CONFIG_STATS=$(extract_files "Configuration Files" CONFIG_EXTENSIONS[@])

# 5. Build TOC
echo -e "${BLUE}Building table of contents...${NC}"
TOC=""

# Parse stats and add to TOC
add_to_toc() {
    local stats=$1
    IFS=':' read -r title anchor count <<< "$stats"
    
    # Only add to TOC if there are files
    if [ "$count" -gt 0 ]; then
        TOC+="- [$title](#$anchor) ($count files)\n"
    fi
}

# Add repository structure to TOC
TOC+="- [Repository Structure](#repository-structure)\n"

# Add other sections to TOC
add_to_toc "$DOC_STATS"
add_to_toc "$CODE_STATS"
add_to_toc "$CONFIG_STATS"

# Insert TOC at the placeholder position
sed -i "${TOC_PLACEHOLDER_LINE}i\\${TOC}" "$OUTPUT_FILE"

# Final stats
TOTAL_SIZE=$(du -h "$OUTPUT_FILE" | cut -f1)
TOTAL_LINES=$(wc -l < "$OUTPUT_FILE")

echo -e "${BLUE}===========================================${NC}"
echo -e "${GREEN}Export complete!${NC}"
echo -e "Output file: ${YELLOW}$OUTPUT_FILE${NC}"
echo -e "Size: ${YELLOW}$TOTAL_SIZE${NC}"
echo -e "Lines: ${YELLOW}$TOTAL_LINES${NC}"
echo -e "${BLUE}===========================================${NC}" 