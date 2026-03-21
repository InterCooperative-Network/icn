#!/usr/bin/env python3
"""
Check documentation freshness against staleness thresholds

Reads freshness.toml and status.toml, checks:
1. Auto-generated sections (skip, handled by generate script)
2. Human-asserted sections: check git log for dependency changes
3. Status subsystems: flag if last_verified > 60 days old

Outputs: Freshness report suitable for CI
Exit codes:
    0: All fresh
    1: Stale sections found

Usage:
    python3 freshness-check.py
    python3 freshness-check.py --freshness path/to/freshness.toml --status path/to/status.toml --repo /path/to/git/repo

Requires: Python 3.11+ and git installed
"""

import sys
import subprocess
from datetime import datetime, timedelta
from pathlib import Path
import argparse
from typing import Dict, Any, List, Tuple

# Try tomllib (Python 3.11+) first, fall back to manual TOML parsing if needed
try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib
    except ImportError:
        print("ERROR: Python 3.11+ required or 'tomli' package needed", file=sys.stderr)
        sys.exit(1)


def load_toml(path: str) -> Dict[str, Any]:
    """Load TOML file."""
    with open(path, "rb") as f:
        return tomllib.load(f)


def strip_tz(dt: datetime) -> datetime:
    """Strip timezone info to get naive datetime for comparison."""
    if dt.tzinfo is not None:
        return dt.replace(tzinfo=None)
    return dt


def get_git_log_date(file_globs: List[str], repo_path: str) -> datetime:
    """Get most recent commit date for any file matching globs (naive UTC)."""
    try:
        # Use git log to find most recent change to any of the files
        cmd = [
            "git", "-C", repo_path, "log", "-1", "--format=%aI"
        ] + file_globs

        result = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
        if result.returncode == 0 and result.stdout.strip():
            date_str = result.stdout.strip()
            return strip_tz(datetime.fromisoformat(date_str))
    except Exception as e:
        print(f"Warning: Could not get git log for {file_globs}: {e}", file=sys.stderr)

    # Return epoch if we can't get git info
    return datetime(1970, 1, 1)


def parse_date(date_str: str) -> datetime:
    """Parse date string to naive datetime.

    Returns datetime.min on failure so invalid timestamps appear maximally
    stale rather than silently appearing fresh.
    """
    try:
        return strip_tz(datetime.fromisoformat(date_str))
    except Exception:
        return datetime.min


def check_freshness(
    freshness_path: str,
    status_path: str,
    repo_path: str
) -> Tuple[List[str], bool]:
    """Check freshness and return (report_lines, all_fresh)."""
    lines = []
    all_fresh = True

    freshness = load_toml(freshness_path)
    status = load_toml(status_path)

    lines.append("=== DOCUMENTATION FRESHNESS REPORT ===")
    lines.append("")
    lines.append(f"Generated: {datetime.now().isoformat()}")
    lines.append("")

    # Check sections
    lines.append("## SECTION FRESHNESS")
    lines.append("")

    sections = freshness.get("sections", {})
    for section_name, section_data in sorted(sections.items()):
        evidence_type = section_data.get("evidence_type", "human-asserted")
        last_updated_str = section_data.get("last_updated", "")

        # Skip auto-generated sections — accept either evidence_type = "auto"
        # or last_updated = "auto" as the skip signal (freshness.toml uses both)
        if evidence_type == "auto" or last_updated_str == "auto":
            lines.append(f"⊕ {section_name}: auto-generated (fresh by definition)")
            continue
        threshold_days = section_data.get("staleness_threshold_days", 30)

        if not last_updated_str:
            lines.append(f"⚠ {section_name}: NO TIMESTAMP")
            all_fresh = False
            continue

        last_updated = parse_date(last_updated_str)
        now = datetime.now()
        age = now - last_updated
        threshold = timedelta(days=threshold_days)

        # Check dependencies for changes
        depends_on = section_data.get("depends_on", [])
        if depends_on and evidence_type == "human-asserted":
            git_date = get_git_log_date(depends_on, repo_path)
            if git_date > last_updated:
                lines.append(
                    f"⚠ {section_name}: DEPENDENCY CHANGED (verified {age.days} days ago)"
                )
                all_fresh = False
                continue

        # Check age against threshold
        if age > threshold:
            days_overdue = (age - threshold).days
            lines.append(
                f"✗ {section_name}: STALE ({age.days} days old, threshold {threshold_days}d)"
            )
            all_fresh = False
        else:
            days_remaining = (threshold - age).days
            lines.append(
                f"✓ {section_name}: fresh ({age.days}d, {days_remaining}d remaining)"
            )

    # Check subsystems
    lines.append("")
    lines.append("## SUBSYSTEM VERIFICATION")
    lines.append("")

    subsystems = status.get("subsystems", {})
    for sub_name, sub_data in sorted(subsystems.items()):
        last_verified_str = sub_data.get("last_verified", "")
        if not last_verified_str:
            lines.append(f"⚠ {sub_name}: NO VERIFICATION DATE")
            all_fresh = False
            continue

        last_verified = parse_date(last_verified_str)
        now = datetime.now()
        age = now - last_verified
        threshold = timedelta(days=60)  # Hard coded 60-day threshold for subsystems

        if age > threshold:
            days_overdue = (age - threshold).days
            lines.append(f"✗ {sub_name}: STALE (verified {age.days} days ago)")
            all_fresh = False
        else:
            days_remaining = (threshold - age).days
            lines.append(f"✓ {sub_name}: verified {age.days}d ago ({days_remaining}d remaining)")

    # Summary
    lines.append("")
    lines.append("=== SUMMARY ===")
    if all_fresh:
        lines.append("✓ All documentation is fresh")
    else:
        lines.append("✗ STALE SECTIONS FOUND - Review and update required")

    return lines, all_fresh


def main():
    parser = argparse.ArgumentParser(description="Check documentation freshness")
    parser.add_argument(
        "--freshness",
        default="freshness.toml",
        help="Path to freshness.toml"
    )
    parser.add_argument(
        "--status",
        default="status.toml",
        help="Path to status.toml"
    )
    parser.add_argument(
        "--repo",
        default=".",
        help="Path to git repository root"
    )

    args = parser.parse_args()

    # Check files exist
    for path in [args.freshness, args.status]:
        if not Path(path).exists():
            print(f"ERROR: {path} not found", file=sys.stderr)
            sys.exit(1)

    # Check freshness
    report_lines, all_fresh = check_freshness(args.freshness, args.status, args.repo)

    for line in report_lines:
        print(line)

    sys.exit(0 if all_fresh else 1)


if __name__ == "__main__":
    main()
