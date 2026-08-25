#!/usr/bin/env python3
"""check-agent-capabilities.py — the committed capability manifest must be true.

This is the mechanism that makes "add a capability in its canonical place and agents discover
it" an enforced property rather than an aspiration. It regenerates the manifest from the same
canonical sources and fails if the committed copy differs.

Because generation introspects the LIVE MCP server, a drift failure means one of:
  - a tool/skill/hook/helper was added or removed without regenerating;
  - the committed manifest claims a tool the runtime cannot actually expose;
  - the runtime exposes a tool the manifest does not mention.

All three are the same bug from an agent's point of view: it was told the wrong thing about
what it can do.

Usage:
    python3 scripts/check-agent-capabilities.py [--repo-root PATH]

Exit 0 = manifest is true. Exit 1 = drift. Refs icn#2653.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path




def load_generator(scripts_dir: Path):
    """Import the generator by path — its filename is not a valid module name."""
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "icn_capability_generator", scripts_dir / "generate-agent-capabilities.py"
    )
    if spec is None or spec.loader is None:
        raise SystemExit("could not load scripts/generate-agent-capabilities.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def diff_lists(kind: str, key: str, committed: list, fresh: list) -> list[str]:
    c = {json.dumps(x, sort_keys=True) for x in committed}
    f = {json.dumps(x, sort_keys=True) for x in fresh}
    problems = []
    for missing in sorted(f - c):
        problems.append(f"  {kind}: present in the runtime but NOT in the manifest: {missing}")
    for extra in sorted(c - f):
        problems.append(f"  {kind}: claimed by the manifest but NOT in the runtime: {extra}")
    return problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo-root", default=None)
    args = ap.parse_args()

    root = Path(args.repo_root) if args.repo_root else Path(
        subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                       text=True, check=True).stdout.strip()
    )

    generator = load_generator(root / "scripts")
    out = root / generator.OUTPUT

    if not out.is_file():
        print(f"FAIL: {generator.OUTPUT} is missing.", file=sys.stderr)
        print("      Run: python3 scripts/generate-agent-capabilities.py --write", file=sys.stderr)
        return 1

    try:
        committed = json.loads(out.read_text(encoding="utf-8"))
    except ValueError as exc:
        print(f"FAIL: {generator.OUTPUT} is not valid JSON: {exc}", file=sys.stderr)
        return 1

    fresh = generator.build(root)

    problems: list[str] = []
    if committed.get("schema") != fresh["schema"]:
        problems.append(
            f"  schema: committed {committed.get('schema')!r} != {fresh['schema']!r}"
        )
    for kind, key in (
        ("mcp_tool", "mcp_tools"),
        ("skill", "skills"),
        ("hook", "hooks"),
        ("helper", "helpers"),
        ("truth_domain", "truth_domains"),
    ):
        problems.extend(diff_lists(kind, key, committed.get(key, []), fresh[key]))

    if problems:
        print("FAIL: the committed capability manifest does not match the real runtime surface.",
              file=sys.stderr)
        print("\n".join(problems), file=sys.stderr)
        print("\n      Fix: python3 scripts/generate-agent-capabilities.py --write",
              file=sys.stderr)
        return 1

    print(
        f"ok: capability manifest is true "
        f"({len(fresh['mcp_tools'])} MCP tools verified against the live server, "
        f"{len(fresh['skills'])} skills, {len(fresh['hooks'])} hooks, "
        f"{len(fresh['helpers'])} helpers)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
