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

# This module IS the copy in the checkout under test, which is normally a feature branch, so its
# own "am I being run out of a candidate checkout" guard would refuse every case below. It is
# neutralised here and tested separately, against a fixture copy whose branch we control.
install.refuse_if_run_from_a_candidate_checkout = lambda: None

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
    shutil.copytree(ROOT / "tools" / "icn-merge-pr", source / "tools" / "icn-merge-pr",
                    ignore=shutil.ignore_patterns("__pycache__"))
    (source / "scripts").mkdir()
    shutil.copyfile(ROOT / "scripts" / "check-merge-policy-schema.py",
                    source / "scripts" / "check-merge-policy-schema.py")
    # As the real repository does. Without it, merely running the source copy would leave
    # bytecode that the cleanliness gate then reports as an untracked change to the evaluator.
    (source / ".gitignore").write_text("__pycache__/\n", encoding="utf-8")
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

    # The tip can move between resolving it and fetching. A checkout at the OLD commit is stale
    # the moment the fetch lands, and must not install looking clean.
    moved = iter([{"branch": "main", "oid": head}, {"branch": "main", "oid": "e" * 40}])
    install.github_default_branch = lambda owner, name: next(moved)
    message = refusal(source, tmp / "prefix-moved")
    check("a default tip that advances between resolving and fetching is refused",
          "stale" in message and "moved mid-install" in message, message[:200])
    check("nothing was installed after a mid-install move", not (tmp / "prefix-moved").exists())

    # A RENAME between the two reads, where the new name already has a tracking ref at the same
    # commit: every OID comparison would agree while the checkout sat on the old branch.
    git(source, "branch", "-f", "trunk", "main")
    git(source, "push", "-q", "origin", "trunk")
    renamed = iter([{"branch": "main", "oid": head}, {"branch": "trunk", "oid": head}])
    install.github_default_branch = lambda owner, name: next(renamed)
    message = refusal(source, tmp / "prefix-renamed")
    check("a default branch RENAMED between the two reads is refused",
          "changed from 'main' to 'trunk'" in message and "reconciling by assumption" in message,
          message[:200])
    check("nothing was installed after a mid-install rename", not (tmp / "prefix-renamed").exists())

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

    print("the installer refuses to run out of a checkout it cannot vouch for")
    fixture_spec = importlib.util.spec_from_file_location(
        "fixture_install", source / "tools" / "icn-merge-pr" / "install.py")
    fixture = importlib.util.module_from_spec(fixture_spec)
    fixture_spec.loader.exec_module(fixture)
    fixture.repository_identity = lambda src: ("example", "icn")
    fixture.github_default_branch = lambda owner, name: {"branch": "main", "oid": head}

    git(source, "checkout", "-q", "-b", "feat/self-check")
    try:
        fixture.refuse_if_run_from_a_candidate_checkout()
        check("an installer run out of a feature-branch checkout refuses", False, "it proceeded")
    except fixture.InstallRefused as exc:
        check("an installer run out of a feature-branch checkout refuses",
              "cannot vouch for anything" in str(exc) and "feat/self-check" in str(exc),
              str(exc)[:200])
        check("the refusal tells the operator to take the installer from the ref",
              "show origin/main:tools/icn-merge-pr/install.py" in str(exc), str(exc)[:200])
    git(source, "checkout", "-q", "main")
    try:
        fixture.refuse_if_run_from_a_candidate_checkout()
        check("an installer run out of the default branch proceeds", True)
    except fixture.InstallRefused as exc:
        check("an installer run out of the default branch proceeds", False, str(exc)[:160])

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

    # Only an installed copy may mutate. `gh` is kept off PATH so this stops at the transport
    # rather than reaching GitHub; what matters is WHICH refusal it is.
    no_gh = {"PATH": str(tmp / "no-such-bin"), "HOME": str(tmp)}
    proc = subprocess.run([str(binary), "merge", "1", "--authorize", "--repo", "example/icn"],
                          capture_output=True, text=True, env=no_gh)
    check("the installed program passes the mutation trust gate",
          "REFUSED_NOT_INSTALLED" not in proc.stdout
          and "REFUSED_UNAVAILABLE_EVIDENCE" in proc.stdout, proc.stdout[:200])
    proc = subprocess.run([str(binary), "merge", "1", "--authorize", "--repo", "someone/fork"],
                          capture_output=True, text=True, env=no_gh)
    check("an installed program refuses to merge a repository it was not installed from",
          "REFUSED_UNTRUSTED_TARGET" in proc.stdout, proc.stdout[:200])
    proc = subprocess.run([str(binary), "check", "1", "--repo", "someone/fork"],
                          capture_output=True, text=True, env=no_gh)
    check("evaluating another repository is still allowed — it mutates nothing",
          "REFUSED_UNTRUSTED_TARGET" not in proc.stdout, proc.stdout[:200])

    # An install directory outlives the install, so an edited tree is the realistic tampering.
    evaluator_installed = lib / "icn_merge_pr" / "evaluate.py"
    kept = evaluator_installed.read_text(encoding="utf-8")
    evaluator_installed.write_text(kept + "\n# edited after installation\n", encoding="utf-8")
    proc = subprocess.run([str(binary), "merge", "1", "--authorize", "--repo", "example/icn"],
                          capture_output=True, text=True, env=no_gh)
    check("an installed file edited after installation refuses to mutate",
          "REFUSED_NOT_INSTALLED" in proc.stdout and "evaluate.py" in proc.stdout,
          proc.stdout[:220])
    evaluator_installed.write_text(kept, encoding="utf-8")
    proc = subprocess.run([str(binary), "merge", "1", "--authorize", "--repo", "example/icn"],
                          capture_output=True, text=True, env=no_gh)
    check("restoring the file restores the install",
          "REFUSED_NOT_INSTALLED" not in proc.stdout, proc.stdout[:200])

    source_copy = source / "tools" / "icn-merge-pr" / "icn_merge_pr" / "__main__.py"
    proc = subprocess.run([sys.executable, str(source_copy), "merge", "1", "--authorize",
                           "--repo", "example/icn"], capture_output=True, text=True, env=no_gh)
    check("the same code run straight out of the checkout refuses to mutate",
          "REFUSED_NOT_INSTALLED" in proc.stdout, proc.stdout[:200])

    # The record is a claim, and a pull request can commit a file. A checkout that writes itself
    # a provenance record must not thereby unlock the mutation it was written to gate.
    forged = source / "tools" / "icn-merge-pr" / "provenance.json"
    shutil.copyfile(lib / "provenance.json", forged)
    proc = subprocess.run([sys.executable, str(source_copy), "merge", "1", "--authorize",
                           "--repo", "example/icn"], capture_output=True, text=True, env=no_gh)
    check("a provenance record committed into the source layout does not unlock mutation",
          "REFUSED_NOT_INSTALLED" in proc.stdout, proc.stdout[:200])
    renamed = source / "tools" / "vendored-helper"
    shutil.copytree(lib, renamed, ignore=shutil.ignore_patterns("__pycache__"))
    forged_record = json.loads((renamed / "provenance.json").read_text(encoding="utf-8"))
    forged_record["lib"] = str(renamed)
    (renamed / "provenance.json").write_text(json.dumps(forged_record), encoding="utf-8")
    proc = subprocess.run([sys.executable, str(renamed / "icn_merge_pr" / "__main__.py"),
                           "merge", "1", "--authorize", "--repo", "example/icn"],
                          capture_output=True, text=True, env=no_gh)
    check("a renamed copy inside the source checkout, with a matching record, still does not "
          "mutate", "REFUSED_NOT_INSTALLED" in proc.stdout, proc.stdout[:220])
    shutil.rmtree(renamed)

    relocated = tmp / "relocated" / "merge-pr"
    shutil.copytree(lib, relocated, ignore=shutil.ignore_patterns("__pycache__"))
    proc = subprocess.run([sys.executable, str(relocated / "icn_merge_pr" / "__main__.py"),
                           "merge", "1", "--authorize", "--repo", "example/icn"],
                          capture_output=True, text=True, env=no_gh)
    check("an installed tree moved away from the location its record names does not mutate",
          "REFUSED_NOT_INSTALLED" in proc.stdout, proc.stdout[:200])
    forged.unlink()

    print("the install tree is CLOSED before anything is imported from it")
    # An unrecorded top-level module is the whole defect: `cli` imports `json`, the install root
    # is what goes on the import path, and every recorded digest stays intact.
    marker = tmp / "HOSTILE-EXECUTED"
    hostile_module = lib / "json.py"
    hostile_module.write_text(
        f"open({str(marker)!r}, 'w').write('executed')\nraise SystemExit(0)\n", encoding="utf-8")
    proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True, env=no_gh)
    check("an unrecorded top-level module does NOT execute",
          not marker.exists(), "the hostile module ran")
    check("an unrecorded top-level module refuses the invocation",
          proc.returncode != 0 and "REFUSED_NOT_INSTALLED" in proc.stdout, proc.stdout[:160])
    check("the refusal names the file the record does not describe",
          "json.py" in proc.stdout, proc.stdout[:200])
    hostile_module.unlink()

    smuggled_module = lib / "icn_merge_pr" / "sneaky.py"
    smuggled_module.write_text("raise SystemExit(0)\n", encoding="utf-8")
    proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True, env=no_gh)
    check("an unrecorded module inside the package refuses before the package is imported",
          proc.returncode != 0 and "REFUSED_NOT_INSTALLED" in proc.stdout
          and "sneaky.py" in proc.stdout, proc.stdout[:200])
    smuggled_module.unlink()

    recorded_file = lib / "icn_merge_pr" / "evaluate.py"
    kept_bytes = recorded_file.read_bytes()
    recorded_file.unlink()
    recorded_file.symlink_to(tmp / "elsewhere.py")
    (tmp / "elsewhere.py").write_bytes(kept_bytes)
    proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True, env=no_gh)
    check("a recorded file replaced by a symlink refuses even with identical content",
          proc.returncode != 0 and "symlink" in proc.stdout, proc.stdout[:200])
    recorded_file.unlink()
    recorded_file.write_bytes(kept_bytes)

    proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True, env=no_gh)
    check("a clean closed installation still works", proc.returncode == 0 and head in proc.stdout,
          proc.stdout[:160])
    for _ in range(3):
        subprocess.run([str(binary), "--help"], capture_output=True, text=True, env=no_gh)
    check("repeated runs leave no bytecode behind, so the tree stays closed",
          not (lib / "icn_merge_pr" / "__pycache__").exists() and not (lib / "__pycache__").exists())
    proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True, env=no_gh)
    check("and the installation is still usable after those runs", proc.returncode == 0,
          proc.stdout[:160])

    print("the provenance record is typed evidence, not whatever JSON happened to parse")
    record_file = lib / "provenance.json"
    good_record = json.loads(record_file.read_text(encoding="utf-8"))
    check("the installer records a full Git object id",
          isinstance(good_record["source_commit"], str)
          and len(good_record["source_commit"]) == 40
          and all(c in "0123456789abcdef" for c in good_record["source_commit"]),
          repr(good_record.get("source_commit")))

    for label, value in (("an integer", 7), ("a boolean", True), ("null", None),
                         ("a list", []), ("an object", {}), ("an empty string", ""),
                         ("a short sha", good_record["source_commit"][:7]),
                         ("40 non-hex characters", "z" * 40),
                         ("a sha with trailing whitespace", good_record["source_commit"] + " ")):
        broken = dict(good_record, source_commit=value)
        record_file.write_text(json.dumps(broken), encoding="utf-8")
        proc = subprocess.run([str(binary), "merge", "1", "--authorize", "--repo", "example/icn"],
                              capture_output=True, text=True, env=no_gh)
        structured = "REFUSED_NOT_INSTALLED" in proc.stdout and "Traceback" not in proc.stderr
        check(f"a source commit that is {label} refuses structurally",
              proc.returncode != 0 and structured,
              (proc.stdout[:100] + proc.stderr[:100]))
    for label, field, value in (("repository", "repository", 7),
                                ("repository", "repository", "no-slash"),
                                ("default branch", "default_branch", None),
                                ("a digest", "files", {"icn_merge_pr/cli.py": "nothex"}),
                                ("a file path", "files", {"../escape.py": "a" * 64})):
        broken = dict(good_record); broken[field] = value
        record_file.write_text(json.dumps(broken), encoding="utf-8")
        proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True,
                              env=no_gh)
        check(f"a malformed {label} refuses structurally",
              proc.returncode != 0 and "REFUSED_NOT_INSTALLED" in proc.stdout
              and "Traceback" not in proc.stderr, (proc.stdout[:100] + proc.stderr[:100]))

    record_file.write_text(json.dumps(good_record, indent=2, sort_keys=True) + "\n",
                           encoding="utf-8")
    proc = subprocess.run([str(binary), "provenance"], capture_output=True, text=True, env=no_gh)
    check("the installer's own record is accepted", proc.returncode == 0 and head in proc.stdout,
          proc.stdout[:160])

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
