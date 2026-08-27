#!/usr/bin/env python3
"""Install icn-merge-pr into a user-level location OUTSIDE any ICN worktree.

WHY THIS EXISTS
A program executed from the PR's own worktree is candidate-controlled: the change being evaluated
would be supplying the code that decides whether to merge it. Moving the evaluator to a user-level
install is only half the answer, though — an installer that copies whatever is in front of it just
relocates the same problem. So installation REFUSES unless the source is demonstrably the live
default-branch tip:

  1. repository identity comes from the source checkout's `origin` remote;
  2. the default branch and its head OID are resolved from EXTERNAL GitHub metadata, which the
     checkout cannot alter;
  3. the checkout must be ON that branch;
  4. `git fetch` runs, so the comparison is against current remote state;
  5. local HEAD must equal the externally reported default-branch OID;
  6. every file being installed must be clean — no local modification, no untracked addition;
  7. the proved commit is recorded next to the installed code.

There is no --force. A flag that skipped these checks would be the whole vulnerability, wearing a
name that made it sound deliberate.

Usage:
    python3 tools/icn-merge-pr/install.py [--source PATH] [--prefix PATH] [--dry-run]

Default prefix is ~/.local, matching scripts/install.sh: the executable lands at
<prefix>/bin/icn-merge-pr and its code at <prefix>/lib/icn/merge-pr/.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone

TOOL = "icn-merge-pr"
PACKAGE = "icn_merge_pr"
LIB_SUFFIX = pathlib.PurePath("lib", "icn", "merge-pr")
BIN_NAME = "icn-merge-pr"
VALIDATOR_SOURCE = pathlib.PurePath("scripts", "check-merge-policy-schema.py")
VALIDATOR_INSTALLED = "_policy_schema.py"
TOOL_DIR = pathlib.PurePath("tools", "icn-merge-pr")
MIN_PYTHON = (3, 9)

_REMOTE = re.compile(r"^(?:https://github\.com/|git@github\.com:|ssh://git@github\.com/)"
                     r"(?P<owner>[^/]+)/(?P<name>.+?)(?:\.git)?/?$")


class InstallRefused(Exception):
    """Installation refused. The message says which trust condition failed."""


def _git(source: pathlib.Path, *args: str) -> str:
    try:
        proc = subprocess.run(["git", "-C", str(source), *args], capture_output=True, text=True,
                              timeout=120, check=False)
    except (OSError, subprocess.SubprocessError) as exc:
        raise InstallRefused(f"git {' '.join(args)} failed to run: {exc}") from exc
    if proc.returncode != 0:
        raise InstallRefused(f"git {' '.join(args)} exited {proc.returncode}: "
                             f"{(proc.stderr or proc.stdout).strip()[:300]}")
    return proc.stdout.strip()


def github_default_branch(owner: str, name: str) -> dict:
    """EXTERNAL trust root: the default branch and its head OID, straight from GitHub.

    Replaced in tests. Nothing derived from the checkout may substitute for this — that is the
    circularity the whole design exists to remove.
    """
    query = ('{repository(owner:"%s",name:"%s"){defaultBranchRef{name target{oid}}}}'
             % (owner, name))
    try:
        proc = subprocess.run(["gh", "api", "graphql", "-f", f"query={query}"],
                              capture_output=True, text=True, timeout=120, check=False)
    except (OSError, subprocess.SubprocessError) as exc:
        raise InstallRefused(f"could not reach GitHub for repository metadata: {exc}") from exc
    if proc.returncode != 0:
        raise InstallRefused("could not read repository metadata from GitHub: "
                             f"{(proc.stderr or proc.stdout).strip()[:300]}")
    try:
        ref = json.loads(proc.stdout)["data"]["repository"]["defaultBranchRef"]
        return {"branch": ref["name"], "oid": ref["target"]["oid"]}
    except (ValueError, KeyError, TypeError) as exc:
        raise InstallRefused(f"GitHub returned no usable default branch: {exc}") from exc


def repository_identity(source: pathlib.Path) -> tuple[str, str]:
    url = _git(source, "remote", "get-url", "origin")
    match = _REMOTE.match(url)
    if not match:
        raise InstallRefused(f"origin {url!r} is not a recognised github.com remote")
    return match.group("owner"), match.group("name")


def installed_files(source: pathlib.Path) -> list[tuple[pathlib.Path, str]]:
    """(absolute source path, path relative to the install lib root) for everything installed."""
    package_dir = source / TOOL_DIR / PACKAGE
    if not package_dir.is_dir():
        raise InstallRefused(f"{package_dir} does not exist; --source is not an ICN checkout")
    files = [(path, str(pathlib.PurePath(PACKAGE, path.name)))
             for path in sorted(package_dir.glob("*.py"))]
    # The schema validator is VENDORED rather than re-implemented: one owner for the rule, and the
    # installed program never reaches into a checkout to find it.
    files.append((source / VALIDATOR_SOURCE, str(pathlib.PurePath(PACKAGE, VALIDATOR_INSTALLED))))
    for path, _ in files:
        if not path.is_file():
            raise InstallRefused(f"required source file is missing: {path}")
    return files


def assert_clean(source: pathlib.Path) -> None:
    """Nothing being installed may be modified or untracked in the source checkout."""
    paths = [str(TOOL_DIR), str(VALIDATOR_SOURCE)]
    dirty = _git(source, "status", "--porcelain", "--untracked-files=all", "--", *paths)
    if dirty:
        listing = "\n  ".join(dirty.splitlines()[:20])
        raise InstallRefused(
            "the evaluator source is not clean, so the installed code would not be the commit "
            f"this install claims:\n  {listing}")


def verify_provenance(source: pathlib.Path) -> dict:
    """Prove the source is the live default-branch tip, or refuse. Returns the trusted record."""
    owner, name = repository_identity(source)
    external = github_default_branch(owner, name)

    branch = _git(source, "rev-parse", "--abbrev-ref", "HEAD")
    if branch != external["branch"]:
        raise InstallRefused(
            f"refusing to install from branch {branch!r}: the repository's default branch is "
            f"{external['branch']!r}. A feature branch is candidate-controlled code, and this "
            "program is exactly the thing that must not come from one.")

    _git(source, "fetch", "origin", external["branch"])

    # The tip can move between resolving it and fetching. Comparing HEAD against the OID read
    # BEFORE the fetch would then accept a checkout the fetch itself has already left behind, and
    # a stale evaluator would install looking perfectly clean. Re-read the external tip afterwards
    # and require agreement with what the fetch actually obtained.
    external = github_default_branch(owner, name)
    fetched = _git(source, "rev-parse", f"refs/remotes/origin/{external['branch']}")
    head = _git(source, "rev-parse", "HEAD")
    if head != external["oid"] or head != fetched:
        raise InstallRefused(
            f"refusing to install from a stale checkout: HEAD is {head[:12]}, the fetched "
            f"origin/{external['branch']} is {fetched[:12]}, and GitHub reports "
            f"{external['oid'][:12]}. Pull until all three agree, then install — an older "
            "default-branch commit is still not the code review approved, and a tip that moved "
            "mid-install is not one this checkout has.")

    assert_clean(source)
    return {"repository": f"{owner}/{name}", "default_branch": external["branch"],
            "source_commit": head}


def _sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


LAUNCHER = """#!/bin/sh
# Generated by tools/icn-merge-pr/install.py from {commit}. Do not edit.
# -E drops PYTHONPATH, -s drops the per-user site directory: the installed package is the only
# icn_merge_pr that can be imported, whatever checkout the operator is standing in.
set -eu
exec "{python}" -E -s "{entry}" "$@"
"""


def install(source: pathlib.Path, prefix: pathlib.Path, dry_run: bool = False) -> dict:
    if sys.version_info < MIN_PYTHON:
        raise InstallRefused(f"python >= {'.'.join(map(str, MIN_PYTHON))} is required")
    record = verify_provenance(source)
    files = installed_files(source)

    lib = prefix / LIB_SUFFIX
    binary = prefix / "bin" / BIN_NAME
    entry = lib / PACKAGE / "__main__.py"
    record.update({
        "tool": TOOL,
        "installed_from": str(source),
        "installed_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "python": sys.executable,
        "lib": str(lib),
        "bin": str(binary),
        "files": {relative: _sha256(path) for path, relative in files},
    })
    if dry_run:
        return record

    # A fresh package directory: a file removed upstream must not survive as a live import.
    package_dir = lib / PACKAGE
    if package_dir.exists():
        shutil.rmtree(package_dir)
    package_dir.mkdir(parents=True, exist_ok=True)
    for path, relative in files:
        shutil.copyfile(path, lib / relative)

    (lib / "provenance.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n",
                                         encoding="utf-8")
    binary.parent.mkdir(parents=True, exist_ok=True)
    binary.write_text(LAUNCHER.format(commit=record["source_commit"][:12], python=sys.executable,
                                      entry=entry), encoding="utf-8")
    binary.chmod(0o755)
    return record


def main(argv: list[str] | None = None) -> int:
    here = pathlib.Path(__file__).resolve()
    parser = argparse.ArgumentParser(description=f"install {TOOL} from a trusted default-branch "
                                                 "checkout")
    parser.add_argument("--source", type=pathlib.Path, default=here.parents[2],
                        help="ICN checkout to install from (default: the one containing this file)")
    parser.add_argument("--prefix", type=pathlib.Path,
                        default=pathlib.Path(os.path.expanduser("~/.local")),
                        help="install prefix (default: ~/.local)")
    parser.add_argument("--dry-run", action="store_true",
                        help="run every trust check and report, without writing anything")
    args = parser.parse_args(argv)

    try:
        record = install(args.source.resolve(), args.prefix.expanduser(), args.dry_run)
    except InstallRefused as exc:
        print(f"{TOOL}: REFUSED — {exc}", file=sys.stderr)
        return 1
    verb = "would install" if args.dry_run else "installed"
    print(f"{TOOL}: {verb} {record['repository']} @ {record['source_commit'][:12]} "
          f"({record['default_branch']}) -> {record['bin']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
