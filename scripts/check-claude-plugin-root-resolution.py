#!/usr/bin/env python3
"""Branch tests for the icn-agent-pack ICN-root resolver (`bin/icn-find-root`).

Imports `resolve_root` from the plugin's resolver and exercises every resolution
branch against temporary fixtures — repo root, a subdirectory, a parent directory
with ICN_ROOT, a path containing spaces, an invalid ICN_ROOT, ambiguous candidates,
the git branch, and the no-root case.

It also runs the plugin's `bin/icn-ops-mcp` launcher against fixture roots, with a
fake `npm` on PATH, to pin its startup policy: opening a session never compiles,
an explicit build compiles only a checkout's own (non-symlinked) dist, and the
registry DB is pinned to the repository's git common dir.

Dependency-free (standard library only). No network. Mutates nothing outside
fresh temporary directories. Exits 0 if all scenarios behave as expected, else 1.

Run:
    python3 scripts/check-claude-plugin-root-resolution.py
"""

from __future__ import annotations

import importlib.util
import os
import sys
import subprocess
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


# --- launcher policy: bin/icn-ops-mcp -------------------------------------------------------

LAUNCHER = FINDER.parent / "icn-ops-mcp"


def run_launcher(t: Path, root: Path, extra_env: dict[str, str] | None = None):
    """Run the real launcher with a fake npm that records argv/env instead of starting anything."""
    fakebin = t / "fakebin"
    fakebin.mkdir(exist_ok=True)
    log = t / "npm.log"
    npm = fakebin / "npm"
    npm.write_text(
        "#!/bin/sh\n"
        f'printf "%s\\n" "ICN_OPS_DB=$ICN_OPS_DB" "ICN_ROOT=$ICN_ROOT" "$@" > "{log}"\n',
        encoding="utf-8",
    )
    npm.chmod(0o755)
    env = {k: v for k, v in os.environ.items() if k not in ("ICN_ROOT", "ICN_OPS_DB", "ICN_OPS_MCP_BUILD_ON_START")}
    env.update({"PATH": f"{fakebin}{os.pathsep}{env.get('PATH', '')}", "ICN_ROOT": str(root)})
    env.update(extra_env or {})
    proc = subprocess.run([sys.executable, str(LAUNCHER)], env=env, cwd=str(t),
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    calls = log.read_text(encoding="utf-8").splitlines() if log.exists() else None
    return proc, calls


def built(root: Path) -> Path:
    (root / "ops" / "mcp" / "dist").mkdir(parents=True, exist_ok=True)
    (root / "ops" / "mcp" / "dist" / "index.js").write_text("// built\n", encoding="utf-8")
    return root


def s_launch_starts_without_compiling() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = built(make_root(Path(t) / "repo"))
        proc, calls = run_launcher(Path(t), r)
        ok = proc.returncode == 0 and calls is not None and calls[2:] == [
            "--prefix", str(Path(real(r)) / "ops" / "mcp"), "run", "--silent", "start"]
        return (ok and "start:stdio" not in calls and "build" not in calls, f"rc={proc.returncode} calls={calls}")


def s_launch_without_build_refuses() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = make_root(Path(t) / "repo")
        proc, calls = run_launcher(Path(t), r)
        return (proc.returncode != 0 and calls is None and "never compiles" in proc.stderr,
                f"rc={proc.returncode} calls={calls} err={proc.stderr.strip()}")


def s_launch_never_compiles_shared_dist() -> tuple[bool, str]:
    # NEGATIVE BOUNDARY: a lane whose dist is a symlink into a shared build — even when the
    # developer explicitly asks for build-on-start — must not compile.
    with tempfile.TemporaryDirectory() as t:
        shared = built(make_root(Path(t) / "shared-build"))
        r = make_root(Path(t) / "lane")
        (r / "ops" / "mcp" / "dist").symlink_to(shared / "ops" / "mcp" / "dist")
        default_proc, default_calls = run_launcher(Path(t), r)
        default_ok = default_calls is not None and "start:stdio" not in default_calls
        (Path(t) / "npm.log").unlink(missing_ok=True)
        proc, calls = run_launcher(Path(t), r, {"ICN_OPS_MCP_BUILD_ON_START": "1"})
        return (default_ok and proc.returncode != 0 and calls is None and "shared" in proc.stderr,
                f"default={default_calls} rc={proc.returncode} calls={calls} err={proc.stderr.strip()}")


def s_launch_explicit_build_compiles_own_tree() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = make_root(Path(t) / "repo")
        proc, calls = run_launcher(Path(t), r, {"ICN_OPS_MCP_BUILD_ON_START": "1"})
        return (proc.returncode == 0 and calls is not None and calls[-1] == "start:stdio", f"calls={calls}")


def s_launch_pins_repository_db() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as t:
        r = built(make_root(Path(t) / "repo"))
        subprocess.run(["git", "init", "-q", str(r)], check=True)
        _, calls = run_launcher(Path(t), r)
        want = f"ICN_OPS_DB={Path(real(r)) / '.git' / 'icn-ops.db'}"
        (Path(t) / "npm.log").unlink(missing_ok=True)
        _, kept = run_launcher(Path(t), r, {"ICN_OPS_DB": "/explicit/db"})
        return (calls is not None and calls[0] == want and kept is not None and kept[0] == "ICN_OPS_DB=/explicit/db",
                f"default={calls and calls[0]} explicit={kept and kept[0]} want={want}")


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
    ("10. launcher starts a built server without compiling", s_launch_starts_without_compiling),
    ("11. launcher with no build fails instead of compiling", s_launch_without_build_refuses),
    ("12. launcher never compiles into a shared (symlinked) dist", s_launch_never_compiles_shared_dist),
    ("13. explicit build-on-start compiles the checkout's own dist", s_launch_explicit_build_compiles_own_tree),
    ("14. launcher pins ICN_OPS_DB to the repo's git common dir", s_launch_pins_repository_db),
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
