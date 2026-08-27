#!/usr/bin/env python3
"""Provenance controls for the icn-merge-pr installer (icn#2651 stage B).

The installer is the trust boundary. Everything the evaluator proves about a pull request is
worth nothing if the evaluator itself can be supplied by that pull request, so these cases are
about ONE question: can code that is not the live default-branch tip end up deciding a merge.

Each MUST-REFUSE case is a way in. A feature branch is the direct attack; a stale default-branch
checkout is the one that looks harmless; a dirty file is the one that needs no git history at all.
The last case is the runtime half — an installed program standing inside a hostile checkout, with
that checkout on PYTHONPATH, still running its own code.

Only the two genuinely external lookups are substituted: GitHub's report of the default branch,
and the parse of a github.com remote URL (unit-tested separately against a table). `git` itself is
real throughout — real branches, real fetch, real status.

Run: python3 scripts/tests/test_icn_merge_pr_install.py
"""

from __future__ import annotations

import importlib.util
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "icn_merge_pr_install", ROOT / "tools" / "icn-merge-pr" / "install.py")
install = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(install)

failures: list[str] = []


def check(desc: str, cond: bool, extra: str = "") -> None:
    print(f"  {'ok  ' if cond else 'FAIL'} {desc}{'' if cond else f'  ({extra})'}")
    if not cond:
        failures.append(desc)


def git(cwd: pathlib.Path, *args: str) -> str:
    proc = subprocess.run(["git", "-C", str(cwd), *args], capture_output=True, text=True,
                          check=True)
    return proc.stdout.strip()


def build_source(tmp: pathlib.Path) -> pathlib.Path:
    """A miniature ICN checkout with a real local origin, holding the real evaluator source."""
    origin = tmp / "origin.git"
    subprocess.run(["git", "init", "--bare", "-b", "main", str(origin)], check=True,
                   capture_output=True)
    source = tmp / "source"
    source.mkdir()
    subprocess.run(["git", "init", "-b", "main", str(source)], check=True, capture_output=True)
    git(source, "config", "user.email", "test@example.invalid")
    git(source, "config", "user.name", "provenance test")
    shutil.copytree(ROOT / "tools" / "icn-merge-pr", source / "tools" / "icn-merge-pr")
    (source / "scripts").mkdir()
    shutil.copyfile(ROOT / "scripts" / "check-merge-policy-schema.py",
                    source / "scripts" / "check-merge-policy-schema.py")
    git(source, "add", "-A")
    git(source, "commit", "-m", "evaluator source")
    git(source, "remote", "add", "origin", str(origin))
    git(source, "push", "-u", "origin", "main")
    return source


def stub_github(oid: str, branch: str = "main"):
    install.github_default_branch = lambda owner, name: {"branch": branch, "oid": oid}


def stub_identity() -> None:
    install.repository_identity = lambda source: ("example", "icn")


def refusal(source: pathlib.Path, prefix: pathlib.Path) -> str:
    try:
        install.install(source, prefix)
    except install.InstallRefused as exc:
        return str(exc)
    return ""


# --- remote identity is parsed, not guessed ----------------------------------------------------
print("repository identity comes from a github.com remote")
for url, expected in (
    ("https://github.com/InterCooperative-Network/icn.git", ("InterCooperative-Network", "icn")),
    ("https://github.com/InterCooperative-Network/icn", ("InterCooperative-Network", "icn")),
    ("git@github.com:InterCooperative-Network/icn.git", ("InterCooperative-Network", "icn")),
    ("ssh://git@github.com/InterCooperative-Network/icn.git", ("InterCooperative-Network", "icn")),
):
    got = install._REMOTE.match(url)
    check(f"{url} parses", bool(got) and (got.group("owner"), got.group("name")) == expected,
          str(got and got.groupdict()))
for url in ("https://gitlab.com/x/y.git", "/srv/mirrors/icn.git", "https://evil.example/github.com/x/y"):
    check(f"{url} is not accepted as a github.com remote", install._REMOTE.match(url) is None)

# --- the install flow --------------------------------------------------------------------------
with tempfile.TemporaryDirectory(prefix="icn-merge-pr-install-") as raw:
    tmp = pathlib.Path(raw)
    source = build_source(tmp)
    head = git(source, "rev-parse", "HEAD")
    stub_identity()

    print("installation refuses anything that is not the live default-branch tip")
    stub_github(head)
    git(source, "checkout", "-q", "-b", "feat/candidate")
    message = refusal(source, tmp / "prefix-feature")
    check("a feature branch is refused", "feat/candidate" in message and "default branch" in message,
          message[:160])
    check("nothing was installed from the feature branch",
          not (tmp / "prefix-feature").exists())
    git(source, "checkout", "-q", "main")

    stub_github("f" * 40)
    message = refusal(source, tmp / "prefix-stale")
    check("a default-branch checkout behind the remote is refused",
          "stale" in message and head[:12] in message, message[:160])
    check("nothing was installed from the stale checkout", not (tmp / "prefix-stale").exists())

    stub_github(head)
    evaluator = source / "tools" / "icn-merge-pr" / "icn_merge_pr" / "evaluate.py"
    original = evaluator.read_text(encoding="utf-8")
    evaluator.write_text(original + "\n# locally modified\n", encoding="utf-8")
    message = refusal(source, tmp / "prefix-dirty")
    check("a locally modified evaluator source is refused", "not clean" in message, message[:160])
    check("nothing was installed from the dirty checkout", not (tmp / "prefix-dirty").exists())
    evaluator.write_text(original, encoding="utf-8")

    smuggled = source / "tools" / "icn-merge-pr" / "icn_merge_pr" / "extra.py"
    smuggled.write_text("# untracked addition\n", encoding="utf-8")
    message = refusal(source, tmp / "prefix-untracked")
    check("an untracked file inside the evaluator source is refused",
          "not clean" in message, message[:160])
    smuggled.unlink()

    print("a clean, current default-branch checkout installs")
    prefix = tmp / "prefix"
    record = install.install(source, prefix)
    binary = prefix / "bin" / "icn-merge-pr"
    lib = prefix / "lib" / "icn" / "merge-pr"
    check("the launcher was written and is executable", binary.is_file() and binary.stat().st_mode & 0o111)
    check("the install location is outside any ICN worktree", str(ROOT) not in str(lib), str(lib))
    check("the proved source commit is recorded", record["source_commit"] == head)
    provenance = json.loads((lib / "provenance.json").read_text(encoding="utf-8"))
    check("the provenance record names the repository, branch and commit",
          provenance["repository"] == "example/icn" and provenance["default_branch"] == "main"
          and provenance["source_commit"] == head)
    check("every installed file is recorded with its digest",
          all(len(digest) == 64 for digest in provenance["files"].values())
          and len(provenance["files"]) >= 8, str(len(provenance["files"])))
    check("the policy schema validator was vendored beside the package",
          (lib / "icn_merge_pr" / "_policy_schema.py").is_file())

    proc = subprocess.run([str(binary), "--help"], capture_output=True, text=True)
    check("the installed launcher runs", proc.returncode == 0
          and "ordinary-merge primitive" in proc.stdout, proc.stderr[:160])
    proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True)
    check("the installed program can report its own provenance",
          proc.returncode == 0 and head in proc.stdout, proc.stderr[:160])
    proc = subprocess.run([str(binary), "merge", "1", "--authorize", "--admin"],
                          capture_output=True, text=True)
    check("the installed program refuses a privileged option",
          proc.returncode == 2 and "REFUSED_FORBIDDEN_OPTION" in proc.stdout, proc.stdout[:160])

    print("the installed runtime does not execute candidate-worktree code")
    candidate = tmp / "candidate-worktree"
    hostile = candidate / "icn_merge_pr"
    hostile.mkdir(parents=True)
    (hostile / "__init__.py").write_text("", encoding="utf-8")
    (hostile / "cli.py").write_text(
        "def main(argv):\n    print('HOSTILE-EVALUATOR-RAN')\n    return 0\n", encoding="utf-8")
    (hostile / "__main__.py").write_text(
        "print('HOSTILE-EVALUATOR-RAN')\n", encoding="utf-8")
    for args in (["--help"], ["provenance"]):
        proc = subprocess.run([str(binary), *args], capture_output=True, text=True,
                              cwd=str(candidate),
                              env={"PATH": "/usr/bin:/bin", "HOME": str(tmp),
                                   "PYTHONPATH": str(candidate)})
        check(f"`icn-merge-pr {' '.join(args)}` inside a hostile checkout runs the INSTALLED code",
              "HOSTILE-EVALUATOR-RAN" not in proc.stdout and proc.returncode == 0,
              proc.stdout[:120])

    print("an installed runtime will not fall back to a checkout for its validator")
    script = ("import icn_merge_pr.policy as p\n"
              "try:\n"
              "    print('RESOLVED', p._validator_path())\n"
              "except Exception as exc:\n"
              "    print('REFUSED', type(exc).__name__)\n")
    proc = subprocess.run([sys.executable, "-E", "-s", "-c", script], cwd=str(lib),
                          capture_output=True, text=True)
    check("an installed runtime resolves the vendored validator",
          "RESOLVED" in proc.stdout and "_policy_schema.py" in proc.stdout,
          proc.stdout[:160] + proc.stderr[:160])
    (lib / "icn_merge_pr" / "_policy_schema.py").unlink()
    proc = subprocess.run([sys.executable, "-E", "-s", "-c", script], cwd=str(lib),
                          capture_output=True, text=True)
    check("an installed runtime missing its vendored validator refuses rather than searching",
          "REFUSED EvidenceUnavailable" in proc.stdout, proc.stdout[:160] + proc.stderr[:160])

    print("a dry run proves the trust checks without writing")
    stub_github(head)
    dry = install.install(source, tmp / "prefix-dry", dry_run=True)
    check("a dry run reports the proved commit", dry["source_commit"] == head)
    check("a dry run writes nothing", not (tmp / "prefix-dry").exists())

print()
if failures:
    print(f"icn-merge-pr installer tests: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("icn-merge-pr installer tests: clean")
