#!/usr/bin/env python3
"""
ICN Monorepo Exporter for LLM Ingestion

This script exports the entire ICN monorepo into a structured format designed
for easy ingestion by a Large Language Model (LLM).

Features:
- Preserves directory structure for context
- Extracts and highlights documentation comments
- Links related files through references
- Smart filtering of binary and large files
- Organizes code by component and subsystem
- Creates contextual summaries
"""

import os
import sys
import re
import glob
import json
import argparse
import datetime
import subprocess
from pathlib import Path
from dataclasses import dataclass, field
from typing import List, Dict, Set, Optional, Tuple, Any

# Configure colors for terminal output
class Colors:
    GREEN = '\033[0;32m'
    BLUE = '\033[0;34m'
    YELLOW = '\033[0;33m'
    RED = '\033[0;31m'
    NC = '\033[0m'  # No Color

@dataclass
class FileInfo:
    """Represents a processed file with metadata."""
    path: str
    relative_path: str
    extension: str
    size: int
    lines: int
    is_binary: bool = False
    is_doc: bool = False
    is_code: bool = False
    is_config: bool = False
    docstring_ratio: float = 0.0
    dependencies: List[str] = field(default_factory=list)
    references: List[str] = field(default_factory=list)
    content: Optional[str] = None
    summary: Optional[str] = None

@dataclass
class ComponentInfo:
    """Represents a logical component in the codebase."""
    name: str
    description: Optional[str] = None
    files: List[FileInfo] = field(default_factory=list)
    related_components: List[str] = field(default_factory=list)

class MonorepoExporter:
    """Exports the monorepo in a format optimized for LLM ingestion."""
    
    # File categorization
    CODE_EXTENSIONS = {
        'rs', 'ts', 'js', 'go', 'py', 'c', 'cpp', 'h', 'hpp', 
        'java', 'kt', 'sh', 'toml', 'dart', 'swift', 'kt'
    }
    DOC_EXTENSIONS = {'md', 'txt', 'rst', 'adoc', 'pdf', 'docx'}
    CONFIG_EXTENSIONS = {'json', 'yaml', 'yml', 'xml', 'ini', 'conf', 'properties'}
    BINARY_EXTENSIONS = {
        'png', 'jpg', 'jpeg', 'gif', 'bmp', 'svg', 'ico', 'webp',
        'mp3', 'wav', 'mp4', 'mov', 'avi', 'webm',
        'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx',
        'zip', 'tar', 'gz', 'bz2', 'xz', '7z', 'jar', 'war',
        'class', 'so', 'dll', 'exe', 'bin', 'dat'
    }
    
    # Directories to exclude
    EXCLUDE_DIRS = {'.git', 'target', 'node_modules', '.keys', 'dist', 'out', 'build'}
    
    def __init__(self, repo_root: str, output_file: str, max_file_size: int = 1024 * 1024):
        self.repo_root = Path(repo_root).absolute()
        self.output_file = output_file
        self.max_file_size = max_file_size
        self.files: List[FileInfo] = []
        self.components: Dict[str, ComponentInfo] = {}
        
        # Ensure repo root exists
        if not self.repo_root.exists() or not self.repo_root.is_dir():
            print(f"{Colors.RED}Error: Repository root '{self.repo_root}' does not exist or is not a directory{Colors.NC}")
            sys.exit(1)
    
    def get_components(self) -> Dict[str, ComponentInfo]:
        """Identify logical components in the repo."""
        components = {}
        
        # Check if this is a cargo workspace
        cargo_toml = self.repo_root / "Cargo.toml"
        if cargo_toml.exists():
            try:
                # Read Cargo.toml to identify workspace members
                with open(cargo_toml, 'r') as f:
                    cargo_content = f.read()
                
                # Extract workspace members
                members_match = re.search(r'\[workspace\]\s*(?:members\s*=\s*\[(.*?)\]|members\s*=\s*\{(.*?)\})', 
                                          cargo_content, re.DOTALL)
                
                if members_match:
                    members_str = members_match.group(1) or members_match.group(2)
                    members = re.findall(r'"([^"]+)"|\'([^\']+)\'', members_str)
                    members = [m[0] or m[1] for m in members]
                    
                    # Create components for each member
                    for member in members:
                        if '*' in member:
                            # Handle glob patterns like "crates/*"
                            base_dir = member.split('*')[0].rstrip('/')
                            for path in glob.glob(str(self.repo_root / member)):
                                if os.path.isdir(path):
                                    name = os.path.basename(path)
                                    components[name] = ComponentInfo(name=name)
                        else:
                            name = os.path.basename(member)
                            components[name] = ComponentInfo(name=name)
            except Exception as e:
                print(f"{Colors.YELLOW}Warning: Failed to parse Cargo.toml: {e}{Colors.NC}")
        
        # Default component structure based on top-level directories
        if not components:
            for item in os.listdir(self.repo_root):
                if (item not in self.EXCLUDE_DIRS and 
                    not item.startswith('.') and
                    os.path.isdir(self.repo_root / item)):
                    components[item] = ComponentInfo(name=item)
        
        return components
    
    def process_file(self, file_path: Path) -> Optional[FileInfo]:
        """Process a single file."""
        if not file_path.exists():
            return None
        
        # Get relative path for display
        rel_path = str(file_path.relative_to(self.repo_root))
        
        # Skip excluded directories
        if any(part in self.EXCLUDE_DIRS for part in file_path.parts):
            return None
        
        # Get file info
        try:
            size = file_path.stat().st_size
            extension = file_path.suffix.lstrip('.').lower()
            
            # Skip files that are too large
            if size > self.max_file_size:
                print(f"{Colors.YELLOW}Skipping large file: {rel_path} ({size//1024}KB){Colors.NC}")
                return None
            
            # Detect if file is binary
            is_binary = False
            if extension in self.BINARY_EXTENSIONS:
                is_binary = True
            else:
                # Use file command for more reliable binary detection
                try:
                    output = subprocess.check_output(['file', '--mime', str(file_path)], 
                                                    universal_newlines=True)
                    is_binary = 'charset=binary' in output
                except (subprocess.SubprocessError, FileNotFoundError):
                    # Try a simple heuristic if 'file' command fails
                    try:
                        with open(file_path, 'rb') as f:
                            chunk = f.read(1024)
                            is_binary = b'\0' in chunk
                    except:
                        is_binary = False
            
            # Skip binary files
            if is_binary:
                print(f"{Colors.YELLOW}Skipping binary file: {rel_path}{Colors.NC}")
                return None
            
            # Read file content
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    content = f.read()
                    lines = content.count('\n') + 1
            except UnicodeDecodeError:
                print(f"{Colors.YELLOW}Skipping non-UTF8 file: {rel_path}{Colors.NC}")
                return None
            
            # Categorize file
            is_doc = extension in self.DOC_EXTENSIONS
            is_code = extension in self.CODE_EXTENSIONS
            is_config = extension in self.CONFIG_EXTENSIONS
            
            # Calculate docstring ratio for code files
            docstring_ratio = 0.0
            if is_code and lines > 0:
                comment_lines = 0
                
                # Count comments based on file type
                if extension in ('rs', 'c', 'cpp', 'h', 'hpp', 'java', 'js', 'ts'):
                    # C-style comments: // and /* */
                    comment_lines += content.count('//')
                    comment_lines += content.count('/*')
                elif extension in ('py', 'sh'):
                    # Python/Shell-style comments: #
                    comment_lines += content.count('#')
                
                docstring_ratio = comment_lines / lines
            
            # Extract dependencies
            dependencies = []
            if is_code:
                # Look for import/use/require/include statements based on language
                if extension == 'rs':
                    dependencies = re.findall(r'(?:use|extern crate)\s+([^;{]+)', content)
                elif extension in ('js', 'ts'):
                    dependencies = re.findall(r'(?:import|require)\s*\([\'"](.+?)[\'"]\)', content)
                    dependencies.extend(re.findall(r'import.+?from\s+[\'"](.+?)[\'"]', content))
                elif extension == 'py':
                    dependencies = re.findall(r'(?:import|from)\s+([^\s;]+)', content)
                
                # Clean up dependencies
                dependencies = [d.strip() for d in dependencies]
                dependencies = [d for d in dependencies if d]
            
            # Create file info
            file_info = FileInfo(
                path=str(file_path),
                relative_path=rel_path,
                extension=extension,
                size=size,
                lines=lines,
                is_binary=is_binary,
                is_doc=is_doc,
                is_code=is_code,
                is_config=is_config,
                docstring_ratio=docstring_ratio,
                dependencies=dependencies,
                content=content
            )
            
            print(f"{Colors.GREEN}Processed: {rel_path}{Colors.NC}")
            return file_info
            
        except Exception as e:
            print(f"{Colors.RED}Error processing {rel_path}: {e}{Colors.NC}")
            return None
    
    def find_all_files(self) -> List[FileInfo]:
        """Find and process all files in the repository."""
        files = []
        
        # Walk through the repository
        for root, dirs, filenames in os.walk(self.repo_root):
            # Skip excluded directories
            dirs[:] = [d for d in dirs if d not in self.EXCLUDE_DIRS and not d.startswith('.')]
            
            for filename in filenames:
                file_path = Path(root) / filename
                file_info = self.process_file(file_path)
                if file_info:
                    files.append(file_info)
        
        return files
    
    def categorize_files(self):
        """Categorize files into components."""
        self.components = self.get_components()
        
        # Default component for files that don't match any specific component
        other_component = ComponentInfo(name="other")
        
        for file_info in self.files:
            # Try to find the component this file belongs to
            assigned = False
            rel_path = file_info.relative_path
            
            for name, component in self.components.items():
                if rel_path.startswith(name + '/') or rel_path == name:
                    component.files.append(file_info)
                    assigned = True
                    break
            
            # Check for crates pattern
            if not assigned and 'crates/' in rel_path:
                crate_name = rel_path.split('crates/')[1].split('/')[0]
                if crate_name in self.components:
                    self.components[crate_name].files.append(file_info)
                    assigned = True
            
            # Assign to "other" if no match found
            if not assigned:
                other_component.files.append(file_info)
        
        # Only add other component if it has files
        if other_component.files:
            self.components["other"] = other_component
    
    def find_component_relationships(self):
        """Find relationships between components based on dependencies."""
        # Create mapping from file paths to components
        file_to_component = {}
        for name, component in self.components.items():
            for file_info in component.files:
                file_to_component[file_info.relative_path] = name
        
        # Find inter-component dependencies
        for component_name, component in self.components.items():
            related = set()
            
            for file_info in component.files:
                for ref in file_info.references:
                    if ref in file_to_component and file_to_component[ref] != component_name:
                        related.add(file_to_component[ref])
            
            component.related_components = list(related)
    
    def export(self):
        """Export the repository to the formatted output."""
        print(f"{Colors.BLUE}Finding files in {self.repo_root}...{Colors.NC}")
        self.files = self.find_all_files()
        
        print(f"{Colors.BLUE}Categorizing files into components...{Colors.NC}")
        self.categorize_files()
        
        # Find inter-component relationships
        print(f"{Colors.BLUE}Finding component relationships...{Colors.NC}")
        self.find_component_relationships()
        
        # Write output
        print(f"{Colors.BLUE}Writing output to {self.output_file}...{Colors.NC}")
        with open(self.output_file, 'w', encoding='utf-8') as f:
            # Write header
            f.write(f"# ICN Monorepo Knowledge Base\n\n")
            f.write(f"Generated on: {datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n\n")
            
            # Write table of contents
            f.write("## Table of Contents\n\n")
            f.write("1. [Repository Overview](#repository-overview)\n")
            f.write("2. [Components](#components)\n")
            
            # Number for component sections
            i = 3
            for name in sorted(self.components.keys()):
                component = self.components[name]
                if component.files:  # Only include non-empty components
                    f.write(f"{i}. [{name}](#{name.lower().replace(' ', '-')})\n")
                    i += 1
            
            f.write("\n")
            
            # Write repository overview
            f.write("## Repository Overview\n\n")
            
            # Count files by type
            code_files = sum(1 for file in self.files if file.is_code)
            doc_files = sum(1 for file in self.files if file.is_doc)
            config_files = sum(1 for file in self.files if file.is_config)
            other_files = len(self.files) - code_files - doc_files - config_files
            
            f.write(f"Total files: {len(self.files)}\n")
            f.write(f"- Code files: {code_files}\n")
            f.write(f"- Documentation files: {doc_files}\n")
            f.write(f"- Configuration files: {config_files}\n")
            f.write(f"- Other files: {other_files}\n\n")
            
            # Write components summary
            f.write("## Components\n\n")
            f.write("| Component | Files | Description |\n")
            f.write("|-----------|-------|-------------|\n")
            
            for name in sorted(self.components.keys()):
                component = self.components[name]
                file_count = len(component.files)
                description = component.description or ""
                f.write(f"| {name} | {file_count} | {description} |\n")
            
            f.write("\n")
            
            # Write detailed component sections
            for name in sorted(self.components.keys()):
                component = self.components[name]
                if not component.files:  # Skip empty components
                    continue
                    
                f.write(f"## {name}\n\n")
                
                # Write component metadata
                if component.description:
                    f.write(f"{component.description}\n\n")
                
                if component.related_components:
                    f.write("Related components: ")
                    f.write(", ".join(component.related_components))
                    f.write("\n\n")
                
                # Group files by type
                code_files = [file for file in component.files if file.is_code]
                doc_files = [file for file in component.files if file.is_doc]
                config_files = [file for file in component.files if file.is_config]
                other_files = [file for file in component.files 
                               if not file.is_code and not file.is_doc and not file.is_config]
                
                # First write documentation files
                if doc_files:
                    f.write("### Documentation\n\n")
                    for file in sorted(doc_files, key=lambda x: x.relative_path):
                        f.write(f"#### {file.relative_path}\n\n")
                        f.write(f"**File Info:** {file.lines} lines, {file.size} bytes\n\n")
                        
                        if file.content:
                            f.write("```" + file.extension + "\n")
                            f.write(file.content)
                            f.write("\n```\n\n")
                
                # Then write code files
                if code_files:
                    f.write("### Code\n\n")
                    for file in sorted(code_files, key=lambda x: x.relative_path):
                        f.write(f"#### {file.relative_path}\n\n")
                        f.write(f"**File Info:** {file.lines} lines, {file.size} bytes")
                        
                        if file.dependencies:
                            f.write(", Dependencies: " + ", ".join(file.dependencies))
                        
                        f.write("\n\n")
                        
                        if file.content:
                            f.write("```" + file.extension + "\n")
                            f.write(file.content)
                            f.write("\n```\n\n")
                
                # Finally write configuration files
                if config_files:
                    f.write("### Configuration\n\n")
                    for file in sorted(config_files, key=lambda x: x.relative_path):
                        f.write(f"#### {file.relative_path}\n\n")
                        f.write(f"**File Info:** {file.lines} lines, {file.size} bytes\n\n")
                        
                        if file.content:
                            f.write("```" + file.extension + "\n")
                            f.write(file.content)
                            f.write("\n```\n\n")
        
        # Report stats
        file_size = os.path.getsize(self.output_file)
        file_size_mb = file_size / (1024 * 1024)
        
        print(f"{Colors.BLUE}========================================={Colors.NC}")
        print(f"{Colors.GREEN}Export complete!{Colors.NC}")
        print(f"Output file: {Colors.YELLOW}{self.output_file}{Colors.NC}")
        print(f"Size: {Colors.YELLOW}{file_size_mb:.2f} MB{Colors.NC}")
        print(f"Processed {Colors.YELLOW}{len(self.files)}{Colors.NC} files")
        print(f"{Colors.BLUE}========================================={Colors.NC}")

def main():
    parser = argparse.ArgumentParser(description='Export ICN monorepo for LLM ingestion')
    parser.add_argument('--repo', '-r', default='.', 
                        help='Repository root directory (default: current directory)')
    parser.add_argument('--output', '-o', default='icn_knowledge_base.md',
                        help='Output file path (default: icn_knowledge_base.md)')
    parser.add_argument('--max-size', '-m', type=int, default=1024*1024,
                        help='Maximum file size in bytes (default: 1MB)')
    
    args = parser.parse_args()
    
    # Find repository root (try to find .git directory)
    repo_root = args.repo
    while not os.path.exists(os.path.join(repo_root, '.git')) and repo_root != '/':
        parent = os.path.dirname(repo_root)
        if parent == repo_root:  # Reached root directory
            break
        repo_root = parent
    
    exporter = MonorepoExporter(
        repo_root=repo_root,
        output_file=args.output,
        max_file_size=args.max_size
    )
    
    exporter.export()

if __name__ == '__main__':
    main() 