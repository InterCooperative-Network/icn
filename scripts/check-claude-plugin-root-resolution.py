#!/usr/bin/env python3
"""Branch tests for the icn-agent-pack ICN-root resolver (`bin/icn-find-root`).

Imports `resolve_root` from the plugin's resolver and exercises every resolution
branch against temporary fixtures — repo root, a subdirectory, a parent directory
with ICN_ROOT, a path containing spaces, an invalid ICN_ROOT, ambiguous candidates,
the git branch, and the no-root case.

Dependency-free (standard library only). No network. Mutates nothing outside
fresh temporary directories. Exits 0 if all scenarios behave as expected, else 1.

Run:
    python3 scripts/check-claude-plugin-root-resolution.py
"""

from __future__ import annotations

import importlib.util
import os
import sys
import tempfile
from importlib.machinery import SourceFileLoader
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FINDER = ROOT / "tools" / "claude-code" / "plugins" / "icn-agent-pack" / "bin" / "icn-find-root"

if not FINDER.is_file():  # pragma: no cover
    print(f"cannot load resolver — missing {FINDER}", file=sys.stderr)
    raise SystemExit(1)
# Loading the resolver must not litter a __pycache__ into the plugin's bin/.
sys.dont_write_bytecode = True
# icn-find-root has no .py extension, so use an explicit source-file loader.
_loader = SourceFileLoader("icn_find_root", str(FINDER))
_spec = importlib.util.spec_from_loader(_loader.name, _loader)
_mod = importlib.util.module_from_spec(_spec)
_loader.exec_module(_mod)
resolve_root = _mod.resolve_root

NO_GIT = lambda _start: None  # noqa: E731 - disable git for deterministic fixtures


def make_root(base: Path) -> Path:
    """Create a minimal valid ICN root (ops/mcp/package.json + docs/STATE.md)."""
    (base / "ops" / "mcp").mkdir(parents=True, exist_ok=True)
    (base / "ops" / "mcp" / "package.json").write_text("{}\n", encoding="utf-8")
    (base / "docs").mkdir(parents=True, exist_ok=True)
    (base / "docs" / "STATE.md").write_text("# state\n", encoding="utf-8")
    return base


def real(p) -> str:
    return os.path.realpath(str(p))


# Each scenario returns (ok: bool, detail: str).

def s_repo_root() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = make_root(Path(t) / "repo")
        root, err = resolve_root(pwd=str(r), git_toplevel=NO_GIT)
        return (err is None and root == real(r), f"root={root} err={err}")


def s_subdir() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = make_root(Path(t) / "repo")
        sub = r / "docs"
        root, err = resolve_root(pwd=str(sub), git_toplevel=NO_GIT)
        return (err is None and root == real(r), f"root={root} err={err}")


def s_parent_with_icn_root() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = make_root(Path(t) / "repo")
        parent = Path(t)
        root, err = resolve_root(icn_root_env=str(r), pwd=str(parent), git_toplevel=NO_GIT)
        return (err is None and root == real(r), f"root={root} err={err}")


def s_path_with_spaces() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory(prefix="has space ") as t:
        r = make_root(Path(t) / "icn repo")
        sub = r / "docs"
        root, err = resolve_root(pwd=str(sub), git_toplevel=NO_GIT)
        return (err is None and root == real(r), f"root={root} err={err}")


def s_invalid_icn_root() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        bad = Path(t) / "not-icn"
        bad.mkdir()
        root, err = resolve_root(icn_root_env=str(bad), pwd=str(t), git_toplevel=NO_GIT)
        return (root is None and bool(err) and "ICN_ROOT" in err, f"root={root} err={err}")


def s_ambiguous_layout() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as home:
        wt = Path(home) / "icn-dev" / "worktrees" / "icn"
        make_root(wt / "a")
        make_root(wt / "b")
        neutral = Path(home) / "elsewhere"
        neutral.mkdir()
        root, err = resolve_root(pwd=str(neutral), home=home, git_toplevel=NO_GIT)
        return (root is None and bool(err) and "Multiple candidate" in err, f"root={root} err={err}")


def s_higher_level_unique_layout() -> tuple[bool, str]:
    # Launched from a higher-level dir with exactly one candidate and no ICN_ROOT:
    #   /tmp/<fixture>/workspace/icn  is the only valid root; resolve from workspace/.
    with tempfile.TemporaryDirectory() as t:
        workspace = Path(t) / "workspace"
        r = make_root(workspace / "icn")
        root, err = resolve_root(pwd=str(workspace), git_toplevel=NO_GIT)
        return (err is None and root == real(r), f"root={root} err={err}")


def s_git_branch() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = make_root(Path(t) / "repo")
        # pwd has no markers; git reports the valid root.
        root, err = resolve_root(pwd=str(Path(t) / "anywhere"), git_toplevel=lambda _s: str(r))
        return (err is None and root == real(r), f"root={root} err={err}")


def s_no_root() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        empty = Path(t) / "empty"
        empty.mkdir()
        root, err = resolve_root(pwd=str(empty), home=str(empty), git_toplevel=NO_GIT)
        return (root is None and bool(err) and "Could not resolve ICN root" in err, f"root={root} err={err}")


SCENARIOS = [
    ("1. from repo root", s_repo_root),
    ("2. from repo subdirectory", s_subdir),
    ("3. from parent dir with ICN_ROOT", s_parent_with_icn_root),
    ("4. path containing spaces", s_path_with_spaces),
    ("5. invalid ICN_ROOT fails clearly", s_invalid_icn_root),
    ("6. ambiguous candidates fail", s_ambiguous_layout),
    ("7. higher-level dir, unique ./icn layout (no ICN_ROOT)", s_higher_level_unique_layout),
    ("8. git toplevel branch", s_git_branch),
    ("9. no root found fails clearly", s_no_root),
]


def main() -> int:
    failures = 0
    for label, fn in SCENARIOS:
        try:
            ok, detail = fn()
        except Exception as exc:  # pragma: no cover - test harness safety
            ok, detail = False, f"raised {exc!r}"
        status = "PASS" if ok else "FAIL"
        print(f"  {status}  {label}")
        if not ok:
            print(f"         -> {detail}")
            failures += 1
    if failures:
        print(f"root-resolution check FAILED: {failures}/{len(SCENARIOS)} scenarios", file=sys.stderr)
        return 1
    print(f"root-resolution check passed: {len(SCENARIOS)}/{len(SCENARIOS)} scenarios")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
