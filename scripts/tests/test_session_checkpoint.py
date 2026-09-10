#!/usr/bin/env python3
"""Controls for ops/scripts/icn-session-checkpoint.

The point of the checkpoint format is that state survives WITHOUT the harness
that produced it, so these tests run the tool against throwaway git repositories
and read only the files it wrote. Nothing here imports a Claude API, reads a
provider directory, or assumes a transcript exists.

The properties under test are the ones that make a checkpoint trustworthy:

* it works with no provider transcript at all (portability);
* a transcript, when supplied, is copied and hashed rather than referenced or
  parsed (the vendor artefact is evidence, not a dependency);
* content hashes actually detect tampering (`verify` must fail, not warn);
* the manifest keeps observed / captured / generated apart, because a consumer
  that cannot tell a re-derivable fact from a stale one will act on the stale
  one;
* the tool never invents narrative it did not receive.

Run: python3 scripts/tests/test_session_checkpoint.py
"""

import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
TOOL = ROOT / "ops" / "scripts" / "icn-session-checkpoint"

failures = []


def check(desc, cond):
    print(f"  {'ok  ' if cond else 'FAIL'} {desc}")
    if not cond:
        failures.append(desc)


def git(repo, *args):
    subprocess.run(
        ["git", *args], cwd=repo, check=True, capture_output=True, text=True
    )


def a_repo(tmp: pathlib.Path) -> pathlib.Path:
    """A minimal git worktree with one commit and one uncommitted change."""
    repo = tmp / "repo"
    repo.mkdir()
    git(repo, "init", "-q", "-b", "main")
    git(repo, "config", "user.email", "test@example.invalid")
    git(repo, "config", "user.name", "test")
    (repo / "tracked.txt").write_text("one\n", encoding="utf-8")
    git(repo, "add", "tracked.txt")
    git(repo, "commit", "-q", "-m", "first")
    # An unstaged modification: `git status --porcelain` puts a space in column
    # one for these, and a parser that trims it truncates every path.
    (repo / "tracked.txt").write_text("two\n", encoding="utf-8")
    return repo


def create(out, repo, *extra):
    return subprocess.run(
        [sys.executable, str(TOOL), "create", "--out", str(out), "--cwd", str(repo), *extra],
        capture_output=True,
        text=True,
    )


def verify(out):
    return subprocess.run(
        [sys.executable, str(TOOL), "verify", str(out)], capture_output=True, text=True
    )


def main() -> int:
    if not TOOL.is_file():
        print(f"missing tool: {TOOL}")
        return 1

    with tempfile.TemporaryDirectory() as td:
        tmp = pathlib.Path(td)
        repo = a_repo(tmp)

        # --- portability: no transcript, no harness, still a checkpoint ------
        print("a checkpoint without any provider artefact")
        out = tmp / "ckpt-bare"
        run = create(out, repo)
        check("create succeeds with no transcript and no handoff", run.returncode == 0)
        manifest_path = out / "manifest.json"
        check("manifest.json is written", manifest_path.is_file())
        if not manifest_path.is_file():
            print(run.stderr)
            return 1
        m = json.loads(manifest_path.read_text(encoding="utf-8"))
        check("schema is declared", m.get("schema") == "icn-session-checkpoint/v1")
        check("verify passes on a bare checkpoint", verify(out).returncode == 0)

        # --- the three truth classes stay apart ------------------------------
        print("truth classes")
        check("observed / captured / generated are separate blocks",
              all(k in m for k in ("observed", "captured", "generated")))
        check("each class explains what it is",
              set(m.get("truth_classes", {})) == {"observed", "captured", "generated"})
        check("live-state owners are named rather than duplicated",
              m.get("authority", {}).get("live_state_owners")
              == "ops/state/truth/sources.json")

        # --- observed state is real, and column-correct ----------------------
        print("observed git state")
        g = m["observed"]["git"]
        check("branch is captured", g.get("branch") == "main")
        check("HEAD is a full sha", isinstance(g.get("head"), str) and len(g["head"]) == 40)
        check("dirty is reported", g.get("dirty") is True)
        check("the changed path is not truncated",
              g.get("changed_files") == ["tracked.txt"])

        # --- unresolvable facts are reported, not guessed --------------------
        print("facts this repository cannot supply")
        check("a missing PR is reported with a reason, not invented",
              m["captured"]["pull_request"]["resolved"] is None
              and bool(m["captured"]["pull_request"].get("reason")))
        check("checks are unresolved when there is no PR",
              m["captured"]["checks"]["resolved"] is None)
        check("no narrative is invented", m["generated"]["notes"] == []
              and m["generated"]["handoff"] is None)

        # --- a supplied transcript is opaque evidence ------------------------
        print("a checkpoint carrying a provider transcript")
        transcript = tmp / "session.jsonl"
        transcript.write_text('{"vendor":"whatever","n":1}\n', encoding="utf-8")
        handoff = tmp / "handoff.md"
        handoff.write_text("# Handoff\n\nWhat happened.\n", encoding="utf-8")

        out2 = tmp / "ckpt-full"
        run = create(
            out2, repo,
            "--transcript", str(transcript),
            "--handoff", str(handoff),
            "--provider", "some-harness",
            "--ref", "icn#1",
            "--note", "a note the tool did not write",
        )
        check("create succeeds with artefacts", run.returncode == 0)
        m2 = json.loads((out2 / "manifest.json").read_text(encoding="utf-8"))

        roles = {a["role"]: a for a in m2["artifacts"]}
        check("both artefacts are recorded", set(roles) == {"handoff", "provider-transcript"})
        check("the transcript is marked opaque",
              roles.get("provider-transcript", {}).get("opaque") is True)
        check("the transcript is COPIED, not referenced",
              (out2 / roles["provider-transcript"]["path"]).is_file())
        check("the copy is byte-identical",
              (out2 / roles["provider-transcript"]["path"]).read_bytes()
              == transcript.read_bytes())
        check("the agent's note is carried through unchanged",
              m2["generated"]["notes"] == ["a note the tool did not write"])
        check("a supplied reference is recorded as unqueried",
              m2["captured"]["references"][0]["ref"] == "icn#1")

        # The whole point: delete the source and the checkpoint still stands.
        transcript.unlink()
        handoff.unlink()
        check("verify passes after the originals are gone", verify(out2).returncode == 0)

        # --- hashes are load-bearing ----------------------------------------
        print("integrity")
        tampered = out2 / roles["handoff"]["path"]
        tampered.write_text("# Handoff\n\nSomething else entirely.\n", encoding="utf-8")
        result = verify(out2)
        check("verify FAILS on a tampered artefact", result.returncode != 0)
        check("verify names the tampered file",
              "handoff.md" in (result.stdout + result.stderr))

        # --- review findings from PR #2702, pinned so they cannot return -----
        print("credentials and machine-local paths stay out of the manifest")
        check("no absolute source path is recorded for an artefact",
              all("source_path" not in a for a in m2["artifacts"]))

        # The portable artifact must carry repository IDENTITY, never the remote
        # string. Each shape below is exported and then the whole checkpoint
        # tree is searched, byte-wise, for the sentinel secret.
        SENTINEL = "s3cr3t-sentinel-not-a-real-token-9eec9e"
        shapes = [
            ("plain https", f"https://github.com/org/repo.git",
             {"form": "url", "host": "github.com", "owner": "org", "name": "repo"}),
            ("https with token userinfo",
             f"https://x-access-token:{SENTINEL}@github.com/org/repo.git",
             {"form": "url", "host": "github.com", "owner": "org", "name": "repo"}),
            ("https with user:password",
             f"https://alice:{SENTINEL}@example.com/team/sub/repo.git",
             {"form": "url", "host": "example.com", "owner": "team/sub", "name": "repo"}),
            ("ssh:// url", "ssh://git@github.com:22/org/repo.git",
             {"form": "url", "host": "github.com", "owner": "org", "name": "repo"}),
            ("scp-like", "git@github.com:org/repo.git",
             {"form": "scp", "host": "github.com", "owner": "org", "name": "repo"}),
            ("query and fragment",
             f"https://github.com/org/repo.git?access_token={SENTINEL}#frag",
             {"form": "url", "host": "github.com", "owner": "org", "name": "repo"}),
            ("local path", "/srv/git/repo.git",
             {"form": "path", "host": None, "owner": None, "name": "repo"}),
            ("malformed", "not a url at all",
             {"form": "unknown", "host": None, "owner": None, "name": None}),
        ]

        for label, remote_url, expected in shapes:
            git(repo, "remote", "add", "origin", remote_url)
            out_r = tmp / f"ckpt-remote-{abs(hash(label))}"
            run = create(out_r, repo)
            git(repo, "remote", "remove", "origin")

            check(f"[{label}] create succeeds", run.returncode == 0)
            if run.returncode != 0:
                continue
            mr = json.loads((out_r / "manifest.json").read_text(encoding="utf-8"))
            origin = mr["observed"]["repository"]["origin"]
            check(f"[{label}] identity parsed as expected",
                  all(origin.get(k) == v for k, v in expected.items()))
            check(f"[{label}] no raw remote URL field is emitted",
                  "remote_origin" not in mr["observed"]["repository"])

            # The real assertion: the sentinel appears NOWHERE in the artifact.
            polluted = [
                str(f.relative_to(out_r))
                for f in out_r.rglob("*") if f.is_file()
                and SENTINEL.encode() in f.read_bytes()
            ]
            check(f"[{label}] sentinel absent from the whole checkpoint tree",
                  polluted == [])

        # And the tripwire behind the allowlist must actually fire.
        print("the credential tripwire fires")
        import subprocess as _sp
        probe = _sp.run(
            [sys.executable, "-c",
             "import importlib.util,sys;"
             f"spec=importlib.util.spec_from_loader('t',importlib.machinery.SourceFileLoader('t',{str(TOOL)!r}));"
             "m=importlib.util.module_from_spec(spec);"
             "import importlib.machinery;"
             "spec.loader.exec_module(m);"
             "m.assert_no_credentials({'x':'https://u:p@h/'})"],
            capture_output=True, text=True)
        check("assert_no_credentials rejects a userinfo URL", probe.returncode != 0)
        check("the tripwire does not print the offending value",
              "u:p@h" not in (probe.stdout + probe.stderr))

        print("artefacts cannot collide")
        # Two artefacts sharing a basename must not overwrite one another.
        collide_dir = tmp / "collide"
        collide_dir.mkdir()
        same_a = collide_dir / "a"
        same_a.mkdir()
        same_b = collide_dir / "b"
        same_b.mkdir()
        (same_a / "notes.md").write_text("handoff text\n", encoding="utf-8")
        (same_b / "notes.md").write_text("transcript text\n", encoding="utf-8")
        out_col = tmp / "ckpt-collide"
        run = create(out_col, repo,
                     "--handoff", str(same_a / "notes.md"),
                     "--transcript", str(same_b / "notes.md"))
        check("create succeeds with two same-named artefacts", run.returncode == 0)
        mcol = json.loads((out_col / "manifest.json").read_text(encoding="utf-8"))
        paths = {a["path"] for a in mcol["artifacts"]}
        check("the two artefacts get distinct paths", len(paths) == 2)
        bodies = {(out_col / p).read_text(encoding="utf-8") for p in paths}
        check("neither artefact overwrote the other",
              bodies == {"handoff text\n", "transcript text\n"})
        check("verify passes on the collided-basename checkpoint",
              verify(out_col).returncode == 0)

        print("verify survives a malformed manifest")
        for label, artifacts in (
            ("artifact record is not a dict", ["just a string"]),
            ("artifact record has no path", [{"sha256": "0" * 64}]),
            ("artifact record has no sha256",
             [{"path": "artifacts/handoff/notes.md"}]),
            ("artifacts is not a list", {"nope": True}),
            ("path escapes the checkpoint",
             [{"path": "../../etc/passwd", "sha256": "0" * 64}]),
            ("path is absolute",
             [{"path": "/etc/passwd", "sha256": "0" * 64}]),
            ("path is outside artifacts/",
             [{"path": "manifest.json", "sha256": "0" * 64}]),
        ):
            out_bad = tmp / f"ckpt-bad-{abs(hash(label))}"
            out_bad.mkdir()
            # A real file at the referenced path, so a record missing `sha256`
            # reaches the hash comparison instead of stopping at "MISSING".
            real = out_bad / "artifacts" / "handoff"
            real.mkdir(parents=True)
            (real / "notes.md").write_text("x\n", encoding="utf-8")
            bad = dict(m)
            bad["artifacts"] = artifacts
            (out_bad / "manifest.json").write_text(json.dumps(bad), encoding="utf-8")
            result = verify(out_bad)
            check(f"verify fails cleanly when {label}",
                  result.returncode != 0 and "Traceback" not in result.stderr)

        print("a reused output directory cannot smuggle stale artefacts")
        reuse = tmp / "ckpt-reuse"
        t2 = tmp / "second.jsonl"
        t2.write_text('{"n":2}\n', encoding="utf-8")
        h2 = tmp / "second.md"
        h2.write_text("# second\n", encoding="utf-8")
        run = create(reuse, repo, "--transcript", str(t2), "--handoff", str(h2))
        check("first create into a fresh directory succeeds", run.returncode == 0)
        first_files = {str(f.relative_to(reuse)) for f in reuse.rglob("*") if f.is_file()}
        check("the first checkpoint has its artefacts", len(first_files) == 3)

        run = create(reuse, repo)
        check("a bare re-create into a non-empty directory is refused without --replace",
              run.returncode != 0)
        check("the refusal explains why", "not empty" in (run.stdout + run.stderr))

        run = create(reuse, repo, "--replace")
        check("--replace succeeds", run.returncode == 0)
        mre = json.loads((reuse / "manifest.json").read_text(encoding="utf-8"))
        check("the replacing manifest declares no artefacts", mre["artifacts"] == [])
        left = {str(f.relative_to(reuse)) for f in reuse.rglob("*") if f.is_file()}
        check("no stale artefact survived the replacement", left == {"manifest.json"})
        check("verify passes on the replaced checkpoint", verify(reuse).returncode == 0)

        print("verify catches undeclared and index-stripped manifests")
        # Undeclared file planted beside a valid manifest.
        planted = out2 / "artifacts" / "handoff" / "extra.md"
        planted.parent.mkdir(parents=True, exist_ok=True)
        planted.write_text("not in the manifest\n", encoding="utf-8")
        result = verify(out2)
        check("verify FAILS on an undeclared artefact", result.returncode != 0)
        check("verify names the undeclared file", "extra.md" in (result.stdout + result.stderr))
        planted.unlink()

        for label, mutate in (
            ("the artifacts index is removed", lambda d: d.pop("artifacts", None)),
            ("the artifacts index is null", lambda d: d.update(artifacts=None)),
        ):
            out_ix = tmp / f"ckpt-ix-{abs(hash(label))}"
            out_ix.mkdir()
            (out_ix / "artifacts").mkdir()
            (out_ix / "artifacts" / "orphan.bin").write_text("x", encoding="utf-8")
            stripped = dict(m)
            mutate(stripped)
            (out_ix / "manifest.json").write_text(json.dumps(stripped), encoding="utf-8")
            check(f"verify FAILS when {label}", verify(out_ix).returncode != 0)

        # --- an unknown schema is refused, not half-understood ---------------
        print("schema discipline")
        out3 = tmp / "ckpt-future"
        out3.mkdir()
        future = dict(m)
        future["schema"] = "icn-session-checkpoint/v99"
        (out3 / "manifest.json").write_text(json.dumps(future), encoding="utf-8")
        check("verify refuses a schema it does not implement",
              verify(out3).returncode != 0)

        # --- the tool is discoverable as a capability ------------------------
        print("capability registration")
        header = TOOL.read_text(encoding="utf-8").split("\n", 40)[:6]
        check("declares a #: capability: header for the generated index",
              any(line.startswith("#: capability:") for line in header))
        check("is executable, or the capability generator skips it",
              TOOL.stat().st_mode & 0o111 != 0)

    print()
    if failures:
        print(f"icn-session-checkpoint tests: {len(failures)} failure(s)")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("icn-session-checkpoint tests: clean")
    return 0


if __name__ == "__main__":
    if shutil.which("git") is None:
        print("git unavailable; skipping")
        sys.exit(0)
    sys.exit(main())
