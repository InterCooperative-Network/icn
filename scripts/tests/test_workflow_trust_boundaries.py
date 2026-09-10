#!/usr/bin/env python3
"""Trust-boundary invariants for GitHub Actions workflows.

Why this exists
---------------
PR #2761 fixed a real exposure: `opencode.yml` and `claude.yml` triggered on
`issue_comment` with NO author check, handing repository secrets to any account
that could comment, and `opencode` executed a third-party action from a MUTABLE
tag. Both were fixed by hand.

Nothing prevented the next instance. `.github/workflows/**` sits behind no path
filter in any required check, so a pull request that reverts an author gate, or
repoints a credential-bearing action back to a tag, passes every check in CI.
And one instance was in fact left behind: `anthropics/claude-code-action@v1`
remained on a mutable tag in two secret-bearing workflows.

These are the two invariants that would have caught all of it, scoped
deliberately to jobs that can actually reach a secret.

Scope note
----------
Third-party actions on jobs with NO secret access are not required to be pinned
here. That would be a much larger change with a much smaller payoff, and
scope-creeping it into a security invariant is how security invariants get
reverted. `dtolnay/rust-toolchain@nightly` in fuzz.yml is the deliberate
example: a branch ref, no secrets, not flagged.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import yaml

# Events reachable by an account that is not a repository collaborator.
UNTRUSTED_TRIGGERS = {
    "issue_comment",
    "issues",
    "pull_request_review",
    "pull_request_review_comment",
    "discussion",
    "discussion_comment",
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
TRUST_GATE = "author_association"


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    )


def job_touches_secrets(job: dict) -> bool:
    return "secrets." in yaml.dump(job)


def main() -> int:
    root = repo_root()
    failures: list[str] = []
    pinned = gated = 0

    for wf in sorted((root / ".github/workflows").glob("*.yml")):
        try:
            doc = yaml.safe_load(wf.read_text())
        except yaml.YAMLError as exc:
            failures.append(f"{wf.name}: does not parse: {exc}")
            continue
        if not isinstance(doc, dict):
            continue

        on = doc.get(True) or doc.get("on") or {}
        triggers = set(on) if isinstance(on, dict) else {on} if isinstance(on, str) else set(on)
        untrusted = triggers & UNTRUSTED_TRIGGERS

        # `pull_request_target` combines an untrusted ref with a privileged
        # context. It is absent today; assert it stays that way rather than
        # discovering it later.
        if "pull_request_target" in triggers:
            failures.append(
                f"{wf.name}: uses pull_request_target, which runs untrusted PR content in a "
                "privileged context with secrets. If this is genuinely needed, it requires an "
                "explicit review, not a passing check."
            )

        for job_name, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            secret_bearing = job_touches_secrets(job)

            # (1) A secret-bearing job reachable by an untrusted trigger must
            #     gate on repository membership.
            if secret_bearing and untrusted:
                if TRUST_GATE not in str(job.get("if", "")):
                    failures.append(
                        f"{wf.name}:{job_name}: reachable by {sorted(untrusted)} and references "
                        f"secrets, but its `if:` does not check {TRUST_GATE}. Any account able to "
                        "comment could start it."
                    )
                else:
                    gated += 1

            # (2) A secret-bearing job must not run third-party code from a
            #     mutable reference.
            if not secret_bearing:
                continue
            for step in job.get("steps") or []:
                if not isinstance(step, dict):
                    continue
                uses = step.get("uses")
                if not uses or not isinstance(uses, str):
                    continue
                if uses.startswith("./"):          # local composite action
                    continue
                if uses.startswith("actions/"):    # first-party, GitHub-owned
                    continue
                ref = uses.split("@")[-1] if "@" in uses else ""
                if SHA_RE.match(ref):
                    pinned += 1
                else:
                    failures.append(
                        f"{wf.name}:{job_name}: step uses `{uses}` — a mutable ref on a job that "
                        "references secrets. Pin to a full 40-character commit SHA so third-party "
                        "code cannot change without a commit in this repository."
                    )

    if failures:
        print("test_workflow_trust_boundaries: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(
        f"test_workflow_trust_boundaries: clean "
        f"({pinned} third-party action(s) SHA-pinned on secret-bearing jobs, "
        f"{gated} untrusted-trigger job(s) author-gated)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
