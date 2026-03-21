#!/usr/bin/env python3
"""
Generate docs/INDEX.md from registry.toml

Reads registry.toml, groups docs by category, and generates a clean markdown index
with status badges, descriptions, audiences, and last-updated timestamps.

Usage:
    python3 generate-index.py > docs/INDEX.md
    python3 generate-index.py --registry path/to/registry.toml

Requires: Python 3.11+ (uses tomllib)
"""

import sys
import argparse
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Any

# Try tomllib (Python 3.11+) first, fall back to manual TOML parsing if needed
try:
    import tomllib
except ImportError:
    # Fallback for Python < 3.11
    try:
        import tomli as tomllib
    except ImportError:
        print("ERROR: Python 3.11+ required or 'tomli' package needed", file=sys.stderr)
        sys.exit(1)


def load_registry(path: str) -> Dict[str, Any]:
    """Load registry.toml and return parsed content."""
    with open(path, "rb") as f:
        return tomllib.load(f)


def group_by_category(docs: Dict[str, Any]) -> Dict[str, List[tuple]]:
    """Group documents by category."""
    grouped = {}
    for doc_path, doc_data in docs.items():
        category = doc_data.get("category", "uncategorized")
        if category not in grouped:
            grouped[category] = []
        grouped[category].append((doc_path, doc_data))

    # Sort within each category by doc path
    for category in grouped:
        grouped[category].sort(key=lambda x: x[0])

    return grouped


def status_badge(status: str) -> str:
    """Return markdown badge for document status."""
    badges = {
        "canonical": "🔒 **Canonical**",
        "living": "📝 **Living**",
        "draft": "📋 **Draft**",
        "archived": "⏮️ **Archived**",
        "superseded": "❌ **Superseded**",
    }
    return badges.get(status, f"⚪ **{status}**")


def format_audiences(audiences: List[str]) -> str:
    """Format audience list as comma-separated tags."""
    return ", ".join(f"`{a}`" for a in audiences)


def generate_index(registry: Dict[str, Any]) -> str:
    """Generate markdown index content."""
    lines = []

    # Header
    lines.append("# ICN Documentation Index")
    lines.append("")
    lines.append("Canonical index of all ICN documentation, organized by category.")
    lines.append(f"*Generated: {datetime.now().strftime('%Y-%m-%d %H:%M:%S UTC')}*")
    lines.append("")
    lines.append("---")
    lines.append("")

    # Table of contents
    lines.append("## Quick Navigation")
    lines.append("")
    grouped = group_by_category(registry.get("docs", {}))
    for category in sorted(grouped.keys()):
        # Slugify category for anchor
        slug = category.lower().replace(" ", "-")
        lines.append(f"- [{category.title()}](#{slug})")
    lines.append("")
    lines.append("---")
    lines.append("")

    # Documents by category
    for category in sorted(grouped.keys()):
        slug = category.lower().replace(" ", "-")
        lines.append(f"## {category.title()}")
        lines.append("")

        for doc_path, doc_data in grouped[category]:
            # Document entry
            title = doc_data.get("title", doc_path)
            status = doc_data.get("status", "unknown")
            description = doc_data.get("description", "")
            audiences = doc_data.get("audiences", [])
            last_updated = doc_data.get("last_updated", "unknown")

            # Build entry
            lines.append(f"### {status_badge(status)} [{title}]({doc_path})")
            lines.append("")

            if description:
                lines.append(f"{description}")
                lines.append("")

            # Metadata row
            meta_parts = []
            if audiences:
                meta_parts.append(f"**For:** {format_audiences(audiences)}")
            if last_updated != "unknown":
                meta_parts.append(f"**Updated:** {last_updated}")

            if meta_parts:
                lines.append(" | ".join(meta_parts))
                lines.append("")

            # Supersession info if applicable
            if status == "superseded":
                superseded_by = doc_data.get("superseded_by", "unknown")
                reason = doc_data.get("reason", "")
                lines.append(f"> Superseded by [{superseded_by}]({superseded_by})")
                if reason:
                    lines.append(f"> Reason: {reason}")
                lines.append("")

        lines.append("")

    # Summary
    docs_dict = registry.get("docs", {})
    total = len(docs_dict)
    by_status = {}
    for doc_data in docs_dict.values():
        status = doc_data.get("status", "unknown")
        by_status[status] = by_status.get(status, 0) + 1

    lines.append("---")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(f"**Total documents:** {total}")
    lines.append("")
    lines.append("**By status:**")
    for status in sorted(by_status.keys()):
        lines.append(f"- {status.title()}: {by_status[status]}")

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description="Generate docs/INDEX.md from registry.toml"
    )
    parser.add_argument(
        "--registry",
        default="registry.toml",
        help="Path to registry.toml (default: registry.toml in cwd)"
    )

    args = parser.parse_args()

    # Check if registry exists
    if not Path(args.registry).exists():
        print(f"ERROR: {args.registry} not found", file=sys.stderr)
        sys.exit(1)

    # Load and generate
    try:
        registry = load_registry(args.registry)
        index = generate_index(registry)
        print(index)
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
