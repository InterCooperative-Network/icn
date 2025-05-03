# ICN Monorepo Export Scripts

This directory contains utility scripts for the ICN monorepo.

## LLM Export Scripts

These scripts export the ICN monorepo in a format optimized for LLM (Large Language Model) ingestion.

### Shell Script Version

```bash
./export_for_llm.sh
```

This Bash script generates a Markdown file (`icn_knowledge_base.md`) containing:
- Repository structure 
- Documentation files
- Code files
- Configuration files

**Features:**
- Simple and fast execution
- Requires only standard Unix tools
- Hierarchical organization

### Python Script Version

```bash
./export_for_llm.py [options]
```

This Python script provides more advanced features:

**Options:**
- `--repo, -r`: Repository root directory (default: current directory)
- `--output, -o`: Output file path (default: icn_knowledge_base.md)
- `--max-size, -m`: Maximum file size in bytes (default: 1MB)

**Features:**
- Smart component detection (finds Cargo workspaces automatically)
- Component relationship detection
- Documentation extraction from code
- Skips binary and non-UTF8 files
- Extracts dependencies between files
- Statistics for codebase analysis

**Example:**
```bash
# Generate full repository knowledge base
./export_for_llm.py

# Generate knowledge base for a specific subdirectory
./export_for_llm.py --repo ./wallet --output wallet_knowledge.md

# Generate with larger file size limit (5MB)
./export_for_llm.py --max-size 5242880
```

## Output Format

Both scripts generate a Markdown file structured as follows:

1. **Repository overview** - General statistics about the codebase
2. **Components** - Major logical components in the codebase
3. **Component details** - Files organized by component and type
   - Documentation files
   - Code files
   - Configuration files

The Python script adds additional context like:
- Component relationships
- Dependency information
- Code structure analysis

## Requirements

- Shell script: Bash and standard Unix tools
- Python script: Python 3.7+ and standard libraries 