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
import collections
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


# One input to the install: where it came from in the pinned commit, where it lands, the EXACT
# bytes of that Git blob, and the digest of those same bytes. Hashing one source and copying
# another is the defect this shape exists to make impossible.
InstallInput = collections.namedtuple("InstallInput", "repo_path relative data digest")


def _git_bytes(source: pathlib.Path, *args: str) -> bytes:
    """Run git and return raw stdout. Binary-safe, unlike the text helper beside it."""
    try:
        proc = subprocess.run(["git", "-C", str(source), *args], capture_output=True,
                              timeout=120, check=False)
    except (OSError, subprocess.SubprocessError) as exc:
        raise InstallRefused(f"git {' '.join(args)} failed to run: {exc}") from exc
    if proc.returncode != 0:
        raise InstallRefused(f"git {' '.join(args)} exited {proc.returncode}: "
                             f"{proc.stderr.decode('utf-8', 'replace').strip()[:300]}")
    return proc.stdout


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


def _tree_entries(source: pathlib.Path, tree_ish: str,
                  pathspec: str | None = None) -> list[tuple[str, str, str, str]]:
    """(mode, type, oid, name) for one Git tree, read from the object store."""
    argv = ["ls-tree", "-z", tree_ish] + (["--", pathspec] if pathspec else [])
    raw = _git_bytes(source, *argv).decode("utf-8", "strict")
    entries = []
    for record in raw.split("\0"):
        if not record:
            continue
        meta, _, name = record.partition("\t")
        parts = meta.split()
        if len(parts) != 3 or not name:
            raise InstallRefused(f"git ls-tree produced an entry this installer cannot read: "
                                 f"{record!r}")
        entries.append((parts[0], parts[1], parts[2], name))
    return entries


def _blob(source: pathlib.Path, mode: str, kind: str, oid: str, label: str) -> bytes:
    """The bytes of one regular-file blob, or a refusal.

    Modes are checked, not assumed. `120000` is a symlink and `160000` a submodule; neither is
    Python source, and either would install something other than the file it appears to be.
    """
    if kind != "blob" or mode not in ("100644", "100755"):
        raise InstallRefused(
            f"{label} is a {kind} with mode {mode} in the pinned commit, not a regular file; "
            "refusing to install something that is not the source it claims to be")
    return _git_bytes(source, "cat-file", "blob", oid)


def pinned_inputs(source: pathlib.Path, commit: str) -> list[InstallInput]:
    """Everything to install, enumerated and READ FROM the pinned commit's Git objects.

    The working tree identifies the repository, proves operator state and carries the fetch. It is
    NOT authoritative for the bytes: globbing it and hashing its paths meant another process could
    change a file after `assert_clean()` returned and before it was read, so provenance could name
    commit X while installing bytes that were never in X. The commit is the byte authority now, and
    the same `data` is hashed, recorded and written.
    """
    package_tree = f"{commit}:{TOOL_DIR.as_posix()}/{PACKAGE}"
    inputs = []
    for mode, kind, oid, name in sorted(_tree_entries(source, package_tree), key=lambda e: e[3]):
        if not name.endswith(".py"):
            continue          # the installed surface is exactly what it was: the package modules
        data = _blob(source, mode, kind, oid, f"{package_tree}/{name}")
        inputs.append(InstallInput(f"{TOOL_DIR.as_posix()}/{PACKAGE}/{name}",
                                   str(pathlib.PurePath(PACKAGE, name)),
                                   data, hashlib.sha256(data).hexdigest()))
    if not inputs:
        raise InstallRefused(f"{package_tree} holds no Python modules in the pinned commit")

    # The schema validator is VENDORED rather than re-implemented: one owner for the rule, and the
    # installed program never reaches into a checkout to find it.
    validator = VALIDATOR_SOURCE.as_posix()
    # A pathspec, so a path absent from the commit comes back as an empty listing rather than a
    # git error about an object name — the refusal below says what is wrong, git does not.
    found = [e for e in _tree_entries(source, commit, validator) if e[3] == validator]
    if not found:
        raise InstallRefused(f"{validator} is not present in {commit[:12]}")
    mode, kind, oid, _ = found[0]
    data = _blob(source, mode, kind, oid, validator)
    inputs.append(InstallInput(validator, str(pathlib.PurePath(PACKAGE, VALIDATOR_INSTALLED)),
                               data, hashlib.sha256(data).hexdigest()))
    return inputs


def assert_clean(source: pathlib.Path) -> None:
    """Operator hygiene: nothing being installed may be modified or untracked in the checkout.

    This is NOT what binds installed bytes to the commit any more — Git-object extraction in
    `pinned_inputs` is that boundary, because a cleanliness check is a point-in-time observation
    and another process can write the tree the moment after it returns. It stays because it is
    useful: it catches an operator who has forgotten uncommitted evaluator work, and it keeps an
    install from silently ignoring changes someone believes they are installing.
    """
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

    # The tip can move between resolving it and fetching, so it is re-read. The SECOND read is a
    # full refreshed trust root, not merely a fresher OID: the default branch can also be RENAMED,
    # and a rename that leaves the new name pointing at the same commit — with its tracking ref
    # already present locally — would otherwise pass every OID comparison while the checkout sat
    # on the old branch. Name first, then the checkout, then the ref, then the commit.
    refreshed = github_default_branch(owner, name)
    if refreshed["branch"] != external["branch"]:
        raise InstallRefused(
            f"the repository's default branch changed from {external['branch']!r} to "
            f"{refreshed['branch']!r} while installing. Refusing rather than reconciling by "
            "assumption: run the install again against the branch that is now default.")
    branch_now = _git(source, "rev-parse", "--abbrev-ref", "HEAD")
    if branch_now != refreshed["branch"]:
        raise InstallRefused(
            f"the checkout is on {branch_now!r} but the repository's default branch is "
            f"{refreshed['branch']!r}; the branch this install would take its code from is not "
            "the one GitHub calls default.")
    _git(source, "fetch", "origin", refreshed["branch"])
    fetched = _git(source, "rev-parse", f"refs/remotes/origin/{refreshed['branch']}")
    head = _git(source, "rev-parse", "HEAD")
    if head != refreshed["oid"] or head != fetched:
        raise InstallRefused(
            f"refusing to install from a stale checkout: HEAD is {head[:12]}, the fetched "
            f"origin/{refreshed['branch']} is {fetched[:12]}, and GitHub reports "
            f"{refreshed['oid'][:12]}. Pull until all three agree, then install — an older "
            "default-branch commit is still not the code review approved, and a tip that moved "
            "mid-install is not one this checkout has.")

    assert_clean(source)
    return {"repository": f"{owner}/{name}", "default_branch": external["branch"],
            "source_commit": head}


LAUNCHER = """#!/bin/sh
# Generated by tools/icn-merge-pr/install.py from {commit}. Do not edit.
# -I is isolated mode: it drops PYTHONPATH and the per-user site directory, and it leaves the
# script's own directory OFF sys.path, so the install tree cannot shadow the standard library
# before __main__.py has verified it. -B stops bytecode being written, which keeps the installed
# tree closed across runs instead of growing __pycache__ that a closure check would have to
# forgive. __main__.py re-asserts both from inside rather than trusting these flags.
set -eu
exec "{python}" -I -B "{entry}" "$@"
"""


def refuse_if_run_from_a_candidate_checkout() -> None:
    """Refuse when THIS FILE is being run out of a checkout that is not on the default branch.

    Read what this can and cannot do. It catches the honest accident — an operator or agent
    standing in the worktree under review and running the installer that happens to be in front of
    them. It CANNOT bind a tampered installer: a pull request that edits this file deletes this
    function along with every other check, which is why the operating instruction is to run the
    installer from the trusted default-branch ref rather than from the working tree. The check is
    worth having anyway, because the accident is the common case and the tamper is not.
    """
    here = pathlib.Path(__file__).resolve().parent
    for directory in (here, *here.parents):
        if not (directory / ".git").exists():
            continue
        try:
            branch = _git(directory, "rev-parse", "--abbrev-ref", "HEAD")
            owner, name = repository_identity(directory)
            default = github_default_branch(owner, name)["branch"]
        except InstallRefused:
            return                                  # not a checkout we can reason about
        if branch != default:
            raise InstallRefused(
                f"this installer is itself running out of {directory}, which is on {branch!r} "
                f"rather than the default branch {default!r}. An installer taken from the change "
                "under review cannot vouch for anything, including itself. Run the copy on the "
                f"default branch instead:\n"
                f"    git -C <repo> fetch origin\n"
                f"    d=\"$(mktemp -d)\"\n"
                f"    git -C <repo> show origin/{default}:tools/icn-merge-pr/install.py "
                f"> \"$d/install.py\"\n"
                f"    python3 \"$d/install.py\" --source <a checkout on {default}>")
        return


def install(source: pathlib.Path, prefix: pathlib.Path, dry_run: bool = False) -> dict:
    if sys.version_info < MIN_PYTHON:
        raise InstallRefused(f"python >= {'.'.join(map(str, MIN_PYTHON))} is required")
    refuse_if_run_from_a_candidate_checkout()
    record = verify_provenance(source)
    # Read from the PINNED COMMIT, after it has been proved, not from the working tree. The same
    # `data` is hashed here, recorded below and written out, so bytes hashed == bytes written ==
    # bytes in source_commit, and nothing that happens to the checkout in between can change that.
    inputs = pinned_inputs(source, record["source_commit"])

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
        "files": {item.relative: item.digest for item in inputs},
    })
    if dry_run:
        return record

    # A fresh package directory: a file removed upstream must not survive as a live import.
    package_dir = lib / PACKAGE
    if package_dir.exists():
        shutil.rmtree(package_dir)
    package_dir.mkdir(parents=True, exist_ok=True)
    for item in inputs:
        (lib / item.relative).write_bytes(item.data)

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
