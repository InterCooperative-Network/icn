#!/usr/bin/env python3
"""Canonical-state lag check for docs/STATE.md.

Catches the specific drift this guard exists to prevent: a truth-sync block in
docs/STATE.md asserting that a PR/issue is *open / not merged / not on `main`*
when the repository's own history shows it merged. (This is exactly what
happened to #2128 — the #2129 sync block was written moments before #2128
merged and said it was "not present on `main`"; nothing corrected it until the
next refresh.)

STATE.md is APPEND-ONLY: older `[sync edit]` blocks legitimately record claims
that were true when written, so flagging them forever would be noise. By
default this check therefore scans only the **newest** `[sync edit]` block —
the current view a reader/agent grounds on. Pass `--all` to audit every block
(useful to prove the detector works against historical drift).

For each PR/issue number asserted as not-merged/not-on-main, the check looks
for a merge of that number in `git log <ref>` (full commit message, strict
`#N` boundary). Confirmation is POSITIVE-evidence-only: if a merge cannot be
confirmed from git history, the check stays silent rather than cry wolf.

Exit codes (mirrors generate_repo_record.py / the generated-truth gate):
    0  no lag found (or nothing to check)
    1  lag found -> advisory; CI maps this to ::warning::, does not block
    2  checker error (STATE.md unreadable, non-git checkout, or unresolvable --ref) -> real breakage

Python standard library only.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    try:
        top = subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=Path(__file__).resolve().parent,
            text=True,
        ).strip()
        return Path(top)
    except Exception:
        return Path(__file__).resolve().parents[1]


# Phrases that assert a PR/issue is NOT yet merged / NOT on main. Kept tight and
# high-signal so legitimate discussion of genuinely-open issues (e.g. "#2080
# remains OPEN") is not matched — only not-*merged* / not-*on-main* assertions.
_NOT_MERGED_PATTERNS = [
    r"not\s+present\s+on\s+`?main`?",
    r"not\s+(?:yet\s+)?on\s+`?main`?",
    r"not\s+(?:yet\s+)?merged",
    r"is\s+OPEN,\s+not\s+merged",
    r"OPEN,\s+not\s+merged",
    r"live\s+only\s+on\s+branch",
]
_NOT_MERGED_RE = re.compile("|".join(_NOT_MERGED_PATTERNS), re.IGNORECASE)
_REF_RE = re.compile(r"#(\d{3,6})")
# A clause is a sentence-ish span; we tie a not-merged phrase to refs in the
# same clause so we don't bind "#2080 is open" to a "not merged" phrase three
# sentences away.
_CLAUSE_SPLIT_RE = re.compile(r"(?<=[.;\n])")


def newest_block(text: str) -> str:
    """Return the newest `<!-- [sync edit] ... -->` block, or '' if none."""
    m = re.search(r"<!--\s*\[sync edit\].*?-->", text, re.DOTALL)
    return m.group(0) if m else ""


def all_blocks(text: str) -> str:
    return "\n".join(re.findall(r"<!--\s*\[sync edit\].*?-->", text, re.DOTALL))


def find_asserted_not_merged(scope: str) -> set[str]:
    """Refs (#N) asserted not-merged/not-on-main within the same clause."""
    refs: set[str] = set()
    for clause in _CLAUSE_SPLIT_RE.split(scope):
        if _NOT_MERGED_RE.search(clause):
            refs.update(_REF_RE.findall(clause))
    return refs


def git_merge_evidence(ref_num: str, repo: Path, gitref: str) -> str | None:
    """Return a one-line merge-commit description if `#ref_num` appears merged
    in git history, else None. Strict `#N` boundary to avoid #21 matching #210.

    The git environment (work-tree + ref) is validated up front in main(), so a
    `CalledProcessError` here is not a broken checkout — `git log --grep` exits 0
    with empty output when there is simply no match. We therefore treat an error
    as "no positive evidence" (return None) by design: positive-evidence-only,
    never cry wolf. The real-breakage cases (non-git checkout, unresolvable ref)
    are caught earlier and exit 2."""
    try:
        out = subprocess.check_output(
            ["git", "-C", str(repo), "log", gitref, "--no-merges",
             f"--grep=#{ref_num}", "--format=%h\t%s\t%b", "-n", "50"],
            text=True, stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError:
        return None
    boundary = re.compile(rf"#{ref_num}(?!\d)")
    for line in out.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t", 2)
        sha = parts[0]
        msg = "\t".join(parts[1:])
        if boundary.search(msg):
            subj = parts[1] if len(parts) > 1 else ""
            return f"{sha} {subj}".strip()
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description="Canonical-state lag check for docs/STATE.md")
    ap.add_argument("--repo", default=str(repo_root()), help="repo root (default: git toplevel)")
    ap.add_argument("--state", default=None, help="path to STATE.md (default: <repo>/docs/STATE.md)")
    ap.add_argument("--ref", default="HEAD", help="git ref to treat as merged history (default: HEAD)")
    ap.add_argument("--all", action="store_true",
                    help="scan ALL sync blocks, not just the newest (audit mode)")
    args = ap.parse_args()

    repo = Path(args.repo).resolve()
    state = Path(args.state) if args.state else repo / "docs" / "STATE.md"
    if not state.is_file():
        print(f"::error::STATE.md not found at {state}", file=sys.stderr)
        return 2

    # Validate the git environment up front so a non-git checkout or an
    # unresolvable --ref is a checker error (exit 2), not a silent false "OK".
    try:
        subprocess.check_output(
            ["git", "-C", str(repo), "rev-parse", "--is-inside-work-tree"],
            text=True, stderr=subprocess.DEVNULL)
        subprocess.check_output(
            ["git", "-C", str(repo), "rev-parse", "--verify", "--quiet", f"{args.ref}^{{commit}}"],
            text=True, stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError:
        print(f"::error::not a git work tree at {repo}, or unresolvable --ref {args.ref!r}",
              file=sys.stderr)
        return 2

    text = state.read_text(encoding="utf-8")
    scope = all_blocks(text) if args.all else newest_block(text)
    if not scope:
        print("OK: no [sync edit] block found to check.")
        return 0

    refs = find_asserted_not_merged(scope)
    if not refs:
        print(f"OK: {'all blocks' if args.all else 'newest sync block'} asserts nothing as unmerged/not-on-main.")
        return 0

    lags: list[str] = []
    for ref_num in sorted(refs, key=int):
        ev = git_merge_evidence(ref_num, repo, args.ref)
        if ev:
            lags.append(f"#{ref_num}: STATE.md says not-merged/not-on-main, but git shows it merged -> {ev}")

    if lags:
        for line in lags:
            print(f"::warning::canonical-state lag: {line}")
        print(f"\nLAG: {len(lags)} stale not-merged claim(s) in "
              f"{'STATE.md (all blocks)' if args.all else 'the newest STATE.md sync block'}. "
              "Refresh the sync block to record the merge.")
        return 1

    print(f"OK: {len(refs)} not-merged claim(s) checked; none contradicted by git history.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
