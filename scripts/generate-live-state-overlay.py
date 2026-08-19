#!/usr/bin/env python3
"""Generate an owner-derived ICN live-state overlay.

The overlay is an on-demand orientation report, never a committed truth root. It
contains no curated active-issue list, subsystem maturity narrative, identity
model, deployment claim, or handoff-derived current state.

Usage:
    python3 scripts/generate-live-state-overlay.py
    python3 scripts/generate-live-state-overlay.py --format json
    python3 scripts/generate-live-state-overlay.py --no-gh
    python3 scripts/generate-live-state-overlay.py --check --no-gh
    python3 scripts/generate-live-state-overlay.py --output /tmp/icn-overlay.md
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def repo_root() -> Path:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=Path(__file__).resolve().parent,
            capture_output=True,
            text=True,
            timeout=5,
        )
        if result.returncode == 0 and result.stdout.strip():
            return Path(result.stdout.strip())
    except Exception:
        pass
    return Path(__file__).resolve().parents[1]


ROOT = repo_root()

REQUIRED_SECTIONS = {
    "provenance",
    "checkout",
    "truth_owners",
    "merge_policy",
    "live_pull_requests",
    "live_issues",
    "agent_registry",
    "skill_registry",
    "generated_context",
    "memory_pointer",
    "agent_start_rules",
}

# Module-level constants this generator is allowed to hold. Anything else is a
# frozen-worldview regression: curated issue lists, subsystem maturity tables,
# identity models, and deployment claims all entered the old overlay that way.
# The scan in validate() is anchored to column 0, so it matches real assignments
# and never this allowlist's own entries. (The previous two-name substring
# blocklist matched its own search terms and could therefore never pass.)
ALLOWED_MODULE_CONSTANTS = frozenset(
    {
        "ROOT",
        "REQUIRED_SECTIONS",
        "AGENT_START_RULES",
        "ALLOWED_MODULE_CONSTANTS",
        "HANDOFF_DATE_RE",
    }
)

# Handoffs are named handoff-YYYY-MM-DD-<topic>.md. The embedded date is the only
# checkout-independent ordering signal: git does not preserve mtimes, so in a
# fresh clone or worktree every handoff shares one mtime and "newest by mtime"
# is arbitrary.
HANDOFF_DATE_RE = re.compile(r"^handoff-(\d{4}-\d{2}-\d{2})-")

AGENT_START_RULES = [
    "Verify repo root, branch, HEAD, working-tree state, and origin/main when freshness matters.",
    "Read AGENTS.md and ops/state/truth/sources.json.",
    "Resolve the task's fact domains, then read only the relevant registered owners.",
    "Query the specific live issue/PR/reviews/checks needed for the task.",
    "Use the Agent Context Spine path brief when it helps narrow code, docs, and checks.",
    "Verify the claimed gap with the smallest relevant implementation evidence.",
    "Record a compact session frame and plan the bounded mutation before editing.",
    "Treat handoffs as historical memory; reverify every volatile claim before acting.",
]


def run(command: list[str], cwd: Path = ROOT, timeout: int = 10) -> tuple[int, str, str]:
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode, result.stdout.strip(), result.stderr.strip()
    except Exception as exc:
        return 127, "", str(exc)


def load_json(relative: str) -> dict[str, Any]:
    path = ROOT / relative
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        return {"_error": f"{relative}: {exc}"}
    if not isinstance(data, dict):
        return {"_error": f"{relative}: expected a JSON object"}
    return data


def git_value(*args: str) -> str | None:
    rc, out, _ = run(["git", *args])
    return out if rc == 0 and out else None


def git_status_lines() -> list[str] | None:
    """Working-tree entries, or None when git could not answer.

    `git status --short` prints nothing for a clean tree, so a helper that
    collapses "empty output" and "command failed" into None would report an
    unknown tree as clean. Keep the two apart.
    """
    rc, out, _ = run(["git", "status", "--short"])
    return out.splitlines() if rc == 0 else None


def github_list(kind: str, no_gh: bool) -> dict[str, Any]:
    if no_gh:
        return {
            "source": "github-api",
            "status": "offline-unavailable",
            "items": [],
        }

    if kind == "pr":
        command = [
            "gh",
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "20",
            "--json",
            "number,title,headRefName,baseRefName,isDraft,mergeStateStatus,updatedAt,url",
        ]
    elif kind == "issue":
        command = [
            "gh",
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            "20",
            "--json",
            "number,title,state,updatedAt,url",
        ]
    else:  # pragma: no cover - internal misuse
        raise ValueError(kind)

    rc, out, err = run(command, timeout=20)
    if rc != 0:
        return {
            "source": "github-api",
            "status": "unavailable",
            "items": [],
            "error": err or f"gh {kind} list failed with exit {rc}",
        }

    try:
        items = json.loads(out or "[]")
    except json.JSONDecodeError as exc:
        return {
            "source": "github-api",
            "status": "unavailable",
            "items": [],
            "error": f"invalid gh JSON: {exc}",
        }

    return {
        "source": "github-api",
        "status": "live-observed",
        "observed_at": datetime.now(timezone.utc).isoformat(),
        "items": items,
    }


def _handoff_order(path: Path) -> tuple[str, str]:
    """Order handoffs by their filename date, then name. Never by mtime."""
    match = HANDOFF_DATE_RE.match(path.name)
    return (match.group(1) if match else "", path.name)


def newest_handoff() -> dict[str, Any]:
    paths = list((ROOT / "docs" / "dev").glob("handoff-*.md"))
    if not paths:
        return {
            "path": None,
            "authority": "memory-only",
            "instruction": "No handoff found. Nothing is inferred from absence.",
        }

    newest = max(paths, key=_handoff_order)
    dated = HANDOFF_DATE_RE.match(newest.name)
    return {
        "path": str(newest.relative_to(ROOT)),
        "authority": "memory-only",
        "selected_by": (
            "handoff filename date" if dated else "filename order (no date in filename)"
        ),
        "instruction": (
            "Historical/resume context only. Reverify branch, PR, CI, issue, blocker, "
            "runtime, and next-work claims before acting."
        ),
    }


def generated_context() -> dict[str, Any]:
    relative = "docs/reference/project-index/generated/agent-context-spine.json"
    path = ROOT / relative
    if not path.exists():
        return {
            "source": relative,
            "status": "missing",
            "authority": "generated-navigation-only",
        }

    data = load_json(relative)
    if "_error" in data:
        return {
            "source": relative,
            "status": "invalid",
            "authority": "generated-navigation-only",
            "error": data["_error"],
        }

    nodes = data.get("nodes")
    edges = data.get("edges")
    return {
        "source": relative,
        "status": "present",
        "authority": "generated-navigation-only",
        "schema": data.get("schema"),
        "generated": data.get("generated"),
        "source_commit": data.get("source_commit"),
        "node_count": len(nodes) if isinstance(nodes, (list, dict)) else None,
        "edge_count": len(edges) if isinstance(edges, (list, dict)) else None,
        "brief_command": "python3 scripts/generate-agent-context-spine.py --brief <paths>",
    }


def collect(no_gh: bool) -> dict[str, Any]:
    sources = load_json("ops/state/truth/sources.json")
    policy = load_json("ops/state/truth/policy.json")
    agents = load_json("ops/state/truth/agents.json")
    skills = load_json("ops/state/truth/skills.json")

    domains = sources.get("domains", {}) if "_error" not in sources else {}
    truth_owners = []
    if isinstance(domains, dict):
        for domain, entry in sorted(domains.items()):
            if not isinstance(entry, dict):
                entry = {}
            truth_owners.append(
                {
                    "domain": domain,
                    "owner": entry.get("owner"),
                    "stability": entry.get("stability"),
                    "description": entry.get("description"),
                }
            )

    merge = policy.get("merge", {}) if "_error" not in policy else {}
    branch = policy.get("branch", {}) if "_error" not in policy else {}

    branch_name = git_value("branch", "--show-current")
    head = git_value("rev-parse", "HEAD")
    status_lines = git_status_lines()
    origin_main = git_value("rev-parse", "origin/main")

    registered_agents = agents.get("agents", []) if "_error" not in agents else []
    agent_names = [
        item.get("name")
        for item in registered_agents
        if isinstance(item, dict) and item.get("name")
    ]

    canonical_skill_sources = (
        skills.get("canonical_skill_sources", {}) if "_error" not in skills else {}
    )

    return {
        "provenance": {
            "generated_at": datetime.now(timezone.utc).isoformat(),
            "repo_root": str(ROOT),
            "authority": "orientation-only",
            "workflow": "docs/ai/WORKFLOW_ARCHITECTURE.md",
        },
        "checkout": {
            "source": "git",
            "branch": branch_name,
            "head": head,
            "working_tree": (
                "unavailable"
                if status_lines is None
                else "dirty"
                if status_lines
                else "clean"
            ),
            "working_tree_entries": status_lines or [],
            "origin_main": origin_main,
        },
        "truth_owners": {
            "source": "ops/state/truth/sources.json",
            "status": "invalid" if "_error" in sources else "loaded",
            "error": sources.get("_error"),
            "domains": truth_owners,
            "forbidden": sources.get("forbidden", []) if "_error" not in sources else [],
        },
        "merge_policy": {
            "source": "ops/state/truth/policy.json",
            "status": "invalid" if "_error" in policy else "loaded",
            "error": policy.get("_error"),
            "primary_branch": branch.get("primary") if isinstance(branch, dict) else None,
            "default_strategy": merge.get("default_strategy") if isinstance(merge, dict) else None,
            "required_checks": merge.get("required_checks", []) if isinstance(merge, dict) else [],
            "non_blocking_checks": merge.get("non_blocking_checks", []) if isinstance(merge, dict) else [],
            "note": "Reconfirm live branch protection before a merge decision.",
        },
        "live_pull_requests": github_list("pr", no_gh),
        "live_issues": github_list("issue", no_gh),
        "agent_registry": {
            "source": "ops/state/truth/agents.json",
            "status": "invalid" if "_error" in agents else "loaded",
            "error": agents.get("_error"),
            "count": len(agent_names),
            "agents": agent_names,
        },
        "skill_registry": {
            "source": "ops/state/truth/skills.json",
            "status": "invalid" if "_error" in skills else "loaded",
            "error": skills.get("_error"),
            "canonical_skill_sources": canonical_skill_sources,
        },
        "generated_context": generated_context(),
        "memory_pointer": newest_handoff(),
        "agent_start_rules": AGENT_START_RULES,
    }


def render_markdown(data: dict[str, Any]) -> str:
    lines: list[str] = []
    provenance = data["provenance"]
    checkout = data["checkout"]

    lines.extend(
        [
            "# ICN Live State Overlay",
            "",
            "> Orientation only. Domain owners, implementation evidence, and live Git/GitHub remain authoritative for their respective claims.",
            "",
            f"Generated: `{provenance['generated_at']}`",
            f"Repository: `{provenance['repo_root']}`",
            "",
            "## Checkout",
            "",
            f"- branch: `{checkout.get('branch') or 'UNKNOWN'}`",
            f"- HEAD: `{checkout.get('head') or 'UNKNOWN'}`",
            f"- working tree: **{checkout.get('working_tree')}**",
            f"- observed `origin/main`: `{checkout.get('origin_main') or 'NOT AVAILABLE'}`",
            "",
            "## Truth Owners",
            "",
            "| Domain | Owner | Stability |",
            "|---|---|---|",
        ]
    )

    for entry in data["truth_owners"]["domains"]:
        lines.append(
            f"| `{entry.get('domain')}` | `{entry.get('owner') or 'MISSING'}` | {entry.get('stability') or ''} |"
        )

    merge = data["merge_policy"]
    lines.extend(
        [
            "",
            "## Merge Policy",
            "",
            f"Source: `{merge['source']}`. Reconfirm live branch protection before merge.",
            "",
            f"- primary branch: `{merge.get('primary_branch') or 'UNKNOWN'}`",
            f"- default strategy: `{merge.get('default_strategy') or 'UNKNOWN'}`",
            f"- required checks from policy owner: {len(merge.get('required_checks', []))}",
            "",
            "## Live Pull Requests",
            "",
        ]
    )
    lines.extend(render_live_items(data["live_pull_requests"], "PR"))

    lines.extend(["", "## Live Issues", ""])
    lines.extend(render_live_items(data["live_issues"], "Issue"))

    agents = data["agent_registry"]
    lines.extend(
        [
            "",
            "## Agent Registry",
            "",
            f"Source: `{agents['source']}`. Registered specialists: **{agents.get('count', 0)}**.",
            "",
        ]
    )
    if agents.get("agents"):
        lines.append(", ".join(f"`{name}`" for name in agents["agents"]))

    skills = data["skill_registry"]
    lines.extend(["", "## Skill Registry", "", f"Source: `{skills['source']}`.", ""])
    for kind, path in (skills.get("canonical_skill_sources") or {}).items():
        lines.append(f"- {kind}: `{path}`")

    generated = data["generated_context"]
    lines.extend(
        [
            "",
            "## Generated Context",
            "",
            f"- Agent Context Spine: **{generated.get('status')}** (`{generated.get('source')}`)",
            f"- authority: `{generated.get('authority')}`",
            f"- path brief: `{generated.get('brief_command', 'unavailable')}`",
            "",
            "## Memory Pointer",
            "",
        ]
    )

    memory = data["memory_pointer"]
    if memory.get("path"):
        lines.append(f"- newest handoff pointer: `{memory['path']}`")
    else:
        lines.append("- newest handoff pointer: none")
    lines.append(f"- authority: **{memory['authority']}**")
    lines.append(f"- rule: {memory['instruction']}")

    lines.extend(["", "## Agent Start Rules", ""])
    for index, rule in enumerate(data["agent_start_rules"], 1):
        lines.append(f"{index}. {rule}")

    return "\n".join(lines) + "\n"


def render_live_items(section: dict[str, Any], label: str) -> list[str]:
    status = section.get("status")
    if status != "live-observed":
        text = f"{status or 'unavailable'}"
        if section.get("error"):
            text += f": {section['error']}"
        return [f"Live {label.lower()} state **{text}**. Nothing inferred."]

    items = section.get("items") or []
    if not items:
        return [f"No open {label.lower()} items returned by the live query."]

    rendered: list[str] = []
    for item in items:
        number = item.get("number", "?")
        title = item.get("title", "")
        updated = item.get("updatedAt")
        suffix = f" (updated {updated})" if updated else ""
        rendered.append(f"- {label} #{number}: {title}{suffix}")
    return rendered


def validate(data: dict[str, Any], no_gh: bool) -> list[str]:
    errors: list[str] = []
    missing = REQUIRED_SECTIONS - set(data)
    if missing:
        errors.append(f"missing sections: {sorted(missing)}")

    owners = data.get("truth_owners", {})
    if owners.get("status") != "loaded":
        errors.append(f"truth-owner registry not loaded: {owners.get('error')}")
    for entry in owners.get("domains", []):
        if not entry.get("owner"):
            errors.append(f"truth domain has no owner: {entry.get('domain')}")

    if data.get("memory_pointer", {}).get("authority") != "memory-only":
        errors.append("memory pointer must be labeled memory-only")

    if no_gh:
        for section in ("live_pull_requests", "live_issues"):
            live = data.get(section, {})
            if live.get("status") != "offline-unavailable" or live.get("items"):
                errors.append(f"{section} invented state in --no-gh mode")

    for section in ("merge_policy", "agent_registry", "skill_registry"):
        if data.get(section, {}).get("status") != "loaded":
            errors.append(f"{section} registry failed to load")

    try:
        json.loads(json.dumps(data))
    except (TypeError, ValueError) as exc:  # pragma: no cover - defensive
        errors.append(f"JSON round-trip failed: {exc}")

    source = Path(__file__).read_text(encoding="utf-8")
    declared = set(re.findall(r"^([A-Z][A-Z0-9_]*)\s*=", source, re.MULTILINE))
    for name in sorted(declared - ALLOWED_MODULE_CONSTANTS):
        errors.append(
            "unregistered module-level constant may freeze doctrine into the "
            f"generator: {name}"
        )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=("markdown", "json"), default="markdown")
    parser.add_argument("--no-gh", action="store_true", help="Do not query GitHub")
    parser.add_argument("--check", action="store_true", help="Validate structure and exit")
    parser.add_argument("--output", type=Path, help="Write output to this path")
    args = parser.parse_args()

    data = collect(args.no_gh)
    errors = validate(data, args.no_gh)

    if args.check:
        if errors:
            for error in errors:
                print(f"ERROR: {error}", file=sys.stderr)
            return 1
        print(
            "Live-state overlay self-check: OK "
            f"({len(data['truth_owners']['domains'])} truth domains, memory non-authoritative)"
        )
        return 0

    if errors:
        for error in errors:
            print(f"WARNING: {error}", file=sys.stderr)

    output = (
        json.dumps(data, indent=2, sort_keys=False) + "\n"
        if args.format == "json"
        else render_markdown(data)
    )

    if args.output:
        args.output.write_text(output, encoding="utf-8")
    else:
        sys.stdout.write(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
