#!/usr/bin/env python3
"""check-truth-spine.py — validate the truth spine's own integrity (warning-mode).

Companion to ops/scripts/drift-check.sh and scripts/check-preflight-consistency.sh.
Checks that ops/state/truth/sources.json — the arbiter of truth ownership — points at
things that exist, does not double-assign owners, and that the org-repo coordination
registry (ops/state/config/repo-map.json#org_repos) stays consistent with the
ecosystem index (ops/state/ecosystem.json).

Warning-mode by default: exits 0 with warnings printed unless --strict is passed
(future ratchet, mirroring the readiness-overclaim linter's warning->blocking path).
The only unconditional failure is an unreadable/unparseable sources.json.

Deliberately NOT checked (v1): section-level owner overlap (cluster_topology is a
documented pointer view into repo-map.json#infrastructure); content freshness of
downstream lock files (needs private-repo access — VM-session concern, not CI).
"""

import argparse
import datetime
import json
import sys
from pathlib import Path

# Owners that are not files in this repo: live git/API state or downstream repos.
NONFILE_OWNERS = {"git", "github-api", "downstream-repos"}

# Staleness warning thresholds by declared stability class (days).
STALENESS_DAYS = {"volatile": 14, "slow-changing": 120}

# Date-ish keys we look for in JSON owner files, in preference order.
DATE_KEYS = ("last_reviewed", "reviewed_at", "start_date")

warnings: list[str] = []


def warn(msg: str) -> None:
    warnings.append(msg)
    print(f"  !!  {msg}")


def ok(msg: str) -> None:
    print(f"  ok  {msg}")


def parse_date(value: str) -> datetime.date | None:
    try:
        return datetime.date.fromisoformat(str(value)[:10])
    except ValueError:
        return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo-root", default=".", help="repo root to validate (default: .)")
    ap.add_argument("--strict", action="store_true", help="exit 1 if any warnings (ratchet mode)")
    args = ap.parse_args()
    root = Path(args.repo_root)

    print("check-truth-spine")

    sources_path = root / "ops/state/truth/sources.json"
    try:
        sources = json.loads(sources_path.read_text())
    except (OSError, json.JSONDecodeError) as e:
        print(f"  FAIL  cannot read/parse {sources_path}: {e}")
        return 1

    domains = sources.get("domains", {})
    today = datetime.date.today()

    # 1. Every file-owner must exist; machine_view files must exist and parse.
    seen_owners: dict[str, str] = {}
    for name, dom in domains.items():
        owner = dom.get("owner", "")
        if owner in NONFILE_OWNERS or " " in owner:
            ok(f"{name}: non-file owner ({owner!r}) — skipped existence check")
        else:
            owner_path = root / owner.split("#", 1)[0]
            if not owner_path.exists():
                warn(f"{name}: owner path missing on disk: {owner}")
            else:
                ok(f"{name}: owner exists ({owner})")

        # Duplicate-owner rule applies to file owners only: live-query owners
        # (git / github-api) legitimately serve multiple volatile domains.
        if owner not in NONFILE_OWNERS:
            if owner in seen_owners:
                warn(
                    f"duplicate owner: {name} and {seen_owners[owner]} both claim {owner!r} "
                    "(one source per domain — sources.json's own rule)"
                )
            else:
                seen_owners[owner] = name

        mv = dom.get("machine_view")
        if mv:
            mv_path = root / mv
            if not mv_path.exists():
                warn(f"{name}: machine_view missing: {mv}")
            elif mv_path.suffix == ".json":
                try:
                    json.loads(mv_path.read_text())
                except json.JSONDecodeError as e:
                    warn(f"{name}: machine_view unparseable: {mv} ({e})")

        # 2. Staleness by stability class, where the owner file carries a date.
        threshold = STALENESS_DAYS.get(dom.get("stability", ""))
        if threshold and owner not in NONFILE_OWNERS and " " not in owner:
            owner_file = root / owner.split("#", 1)[0]
            if owner_file.is_file() and owner_file.suffix == ".json":
                try:
                    data = json.loads(owner_file.read_text())
                except json.JSONDecodeError:
                    data = None
                if isinstance(data, dict):
                    for key in DATE_KEYS:
                        if key in data:
                            d = parse_date(data[key])
                            if d and (today - d).days > threshold:
                                warn(
                                    f"{name}: {owner} {key}={data[key]} is "
                                    f"{(today - d).days}d old (> {threshold}d for "
                                    f"{dom.get('stability')})"
                                )
                            break

    # 3. Ecosystem index vs org-repo registry consistency.
    eco_path = root / "ops/state/ecosystem.json"
    map_path = root / "ops/state/config/repo-map.json"
    try:
        eco = json.loads(eco_path.read_text())
        repo_map = json.loads(map_path.read_text())
    except (OSError, json.JSONDecodeError) as e:
        warn(f"cannot cross-check ecosystem vs registry: {e}")
        eco, repo_map = {}, {}

    eco_repos = eco.get("repos", {})
    org_repos = repo_map.get("org_repos", {}).get("repos", {})
    if eco_repos and org_repos:
        # icn is "this repo" in ecosystem.json; homelab-inventory lives in #repos.
        expected = set(eco_repos) - {"icn", "homelab-inventory"}
        missing = expected - set(org_repos)
        for r in sorted(missing):
            warn(f"ecosystem.json names repo {r!r} but repo-map.json#org_repos does not register it")
        if not missing:
            ok(f"registry covers all {len(expected)} ecosystem repos (extras allowed)")

        for r in sorted(expected & set(org_repos)):
            ev = eco_repos[r].get("visibility")
            rv = org_repos[r].get("visibility")
            if ev and rv and ev != rv:
                warn(f"visibility disagrees for {r}: ecosystem.json={ev} registry={rv}")

    # Result
    if warnings:
        print(f"check-truth-spine: {len(warnings)} warning(s)")
        return 1 if args.strict else 0
    print("check-truth-spine: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
