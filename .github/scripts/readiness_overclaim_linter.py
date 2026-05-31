#!/usr/bin/env python3
"""
Readiness Overclaim Linter - CI Script

Scans ACTIVE deployment/operations guidance for un-disclaimed, affirmative,
present-tense readiness claims that misrepresent ICN's maturity:

  - production-readiness   ("PRODUCTION READY", "ready for production", ...)
  - live-federation        ("live federation")
  - blanket operability    ("all systems operational")
  - general availability   ("generally available")

ICN is research-grade cooperative-coordination infrastructure. Dated readiness
snapshots are valuable history, so a claim is allowed when the file carries a
stale/archive BANNER, when the line is negated/conditional/aspirational, or when
it is explicitly allowlisted below. This mirrors the discipline the repo already
practices on docs/deployment/*.md (see the already-bannered siblings).

This complements compliance_linter.py (fintech vocabulary in API surfaces); it
does NOT replace it. See docs/dev/language-guide.md and docs/ci/GATE_RATCHET_PLAN.md.

Usage:
    python3 .github/scripts/readiness_overclaim_linter.py [--repo-root PATH]

Exit codes:
    0: No un-disclaimed readiness overclaims detected
    1: Overclaim(s) detected
    2: Script error
"""

import argparse
import os
import re
import sys
from dataclasses import dataclass, field
from typing import List

# ---------------------------------------------------------------------------
# Scan scope (directories, relative to repo root). Intentionally narrow for the
# first ratchet iteration: the deployment/operations guidance surface, where
# "PRODUCTION READY" headline claims are most dangerous and highest-precision.
# Widening scope is a deliberate ratchet step (docs/ci/GATE_RATCHET_PLAN.md).
# ---------------------------------------------------------------------------
SCAN_DIRS = [
    "docs/deployment",
    "docs/operations/deployment",
]

# Affirmative readiness-claim patterns (case-insensitive).
OVERCLAIM_PATTERNS = [
    (re.compile(r"\bproduction[\s-]?ready\b", re.IGNORECASE), "production-ready"),
    (re.compile(r"\bready for production\b", re.IGNORECASE), "ready for production"),
    (re.compile(r"\bapproved for production\b", re.IGNORECASE), "approved for production"),
    (re.compile(r"\bdeployment[\s-]?ready\b", re.IGNORECASE), "deployment-ready"),
    (re.compile(r"\blive federation\b", re.IGNORECASE), "live federation"),
    (re.compile(r"\ball systems operational\b", re.IGNORECASE), "all systems operational"),
    (re.compile(r"\bgeneral(ly)? availab", re.IGNORECASE), "general availability"),
    # Governance-completion overclaims. The firewall contract (docs/dev/language-guide.md
    # "Readiness claims" and docs/ci/GATE_RATCHET_PLAN.md) names this claim class, so the
    # gate must actually detect it. Kept narrow to avoid false positives.
    (re.compile(r"\bgovernance\b[^.\n;|]{0,40}\b(?:is|are)\b[^.\n;|]{0,20}\bcomplete\b", re.IGNORECASE), "governance-completion"),
    (re.compile(r"\b(?:proposal|vote|voting|member[\s-]standing)\b[^.\n;|]{0,60}\b(?:is|are)\b[^.\n;|]{0,24}\b(?:complete|fully (?:working|operational|implemented))\b", re.IGNORECASE), "governance-completion"),
]

# If any of these appear on the line, it is a non-claim (negated / conditional /
# aspirational / rule-describing) -> NOT a violation. Precision over recall: when
# in doubt we do NOT flag, because the repo deliberately uses many such non-claims.
NEGATION_RE = re.compile(
    r"(?i)("
    r"\bnot\b|n't|\bno\b|\bnever\b|not yet|\bwould\b|\bif\b|\bonce\b|\bwhen\b|"
    r"\btarget\b|\bgoal\b|aspir|roadmap|\bfuture\b|do not|don't|must not|\bavoid\b|"
    r"forbidden|prerequisite|before production|in a production deployment|in production:|"
    r"\U0001F7E1"  # yellow-circle status marker used for "assessed, not production-ready"
    r")"
)

# A recognised stale/archive banner near the top of a file exempts the whole file
# (the claim is then clearly labelled history, like the bannered deployment siblings).
# The bare word "snapshot" is NOT sufficient on its own (e.g. "Snapshot frequency is
# configurable." must not exempt a doc); require explicit archival framing, or
# "snapshot" qualified as historical/dated.
BANNER_RE = re.compile(
    r"(?i)("
    r"historical|archiv|point[\s-]in[\s-]time|not current (?:deployment|operational)|"
    r"(?:historical|archived|dated|point[\s-]in[\s-]time)\s+snapshot|"
    r"snapshot\s+(?:from|as of|dated|date:)"
    r")"
)
BANNER_SCAN_LINES = 15

# Explicit allowlist of legitimate, bounded exceptions: "relpath:line" -> reason.
# Keep this SMALL and justified. Adding an entry is the supported way to record a
# false positive WITHOUT weakening the patterns. See the exception policy in
# docs/dev/language-guide.md. (Empty at baseline: banners cover every current hit.)
ALLOWLIST = {
    # "docs/deployment/EXAMPLE.md:42": "why this affirmative line is genuinely fine",
}


@dataclass
class Violation:
    file: str
    line: int
    text: str
    rule: str


@dataclass
class LintResult:
    violations: List[Violation] = field(default_factory=list)
    files_scanned: int = 0
    files_exempt: List[str] = field(default_factory=list)


def is_banner_exempt(lines):
    """True if a stale/archive banner appears within the first BANNER_SCAN_LINES."""
    for line in lines[:BANNER_SCAN_LINES]:
        if BANNER_RE.search(line):
            return True
    return False


# Clause delimiters used to scope a negation to the same clause as the overclaim,
# so an unrelated negation in a *separate* clause cannot bypass the gate (e.g.
# "ICN is not experimental; it is PRODUCTION READY." must still flag on the second
# clause). Comma is deliberately NOT a delimiter, so a leading conditional clause
# like "Once hardened, ICN becomes production-ready." stays exempt (precision).
_CLAUSE_DELIMS = set(".;:|()") | {"—"}  # strong delimiters + em dash; not comma


def _clause_around(line, start, end):
    """Return the clause (between delimiters) containing the [start, end) match."""
    lo = start
    while lo > 0 and line[lo - 1] not in _CLAUSE_DELIMS:
        lo -= 1
    hi = end
    while hi < len(line) and line[hi] not in _CLAUSE_DELIMS:
        hi += 1
    return line[lo:hi]


def scan_lines(rel_path, lines):
    """Flag affirmative readiness claims; honour banner, negation, and allowlist."""
    if is_banner_exempt(lines):
        return []

    violations = []
    for line_num, line in enumerate(lines, start=1):
        for pattern, rule in OVERCLAIM_PATTERNS:
            m = pattern.search(line)
            if not m:
                continue
            # Only the overclaim's own clause exempts it — not an unrelated
            # negation/conditional elsewhere on the same line.
            if NEGATION_RE.search(_clause_around(line, m.start(), m.end())):
                continue
            key = rel_path + ":" + str(line_num)
            if key in ALLOWLIST:
                continue
            violations.append(
                Violation(file=rel_path, line=line_num, text=line.rstrip()[:160], rule=rule)
            )
            break  # one violation per line is enough
    return violations


def scan_file(rel_path, abs_path):
    try:
        with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
    except OSError as e:
        print("Warning: could not read " + rel_path + ": " + str(e), file=sys.stderr)
        return []
    return scan_lines(rel_path, lines)


def run_lint(repo_root):
    result = LintResult()
    for scan_dir in SCAN_DIRS:
        abs_dir = os.path.join(repo_root, scan_dir)
        if not os.path.isdir(abs_dir):
            continue
        for dirpath, _dirnames, filenames in os.walk(abs_dir):
            for name in sorted(filenames):
                if not name.endswith(".md"):
                    continue
                abs_path = os.path.join(dirpath, name)
                rel_path = os.path.relpath(abs_path, repo_root).replace(os.sep, "/")
                result.files_scanned += 1
                with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
                    lines = f.read().splitlines()
                if is_banner_exempt(lines):
                    result.files_exempt.append(rel_path)
                    continue
                result.violations.extend(scan_lines(rel_path, lines))
    return result


def main():
    parser = argparse.ArgumentParser(description="ICN Readiness Overclaim Linter")
    parser.add_argument("--repo-root", default=os.getcwd(), help="Repository root (default: cwd)")
    args = parser.parse_args()
    repo_root = os.path.abspath(args.repo_root)

    print("=" * 70)
    print("Readiness Overclaim Linter")
    print("Un-disclaimed production / live-federation / governance-completion claims in active guidance")
    print("=" * 70)
    print()
    print("Repo root: " + repo_root)
    print("Scope: " + ", ".join(SCAN_DIRS))
    print("Reference: docs/dev/language-guide.md")
    print()

    try:
        result = run_lint(repo_root)
    except Exception as e:  # documented exit code 2 for unexpected script errors
        print("ERROR: readiness linter failed: " + str(e), file=sys.stderr)
        return 2
    print("Scanned " + str(result.files_scanned) + " files; " + str(len(result.files_exempt)) +
          " exempt (stale/archive banner present)")
    print()

    if not result.violations:
        print("No un-disclaimed readiness overclaims detected.")
        print()
        print("Active deployment guidance either avoids affirmative readiness claims")
        print("or carries a stale/archive banner labelling them as historical.")
        return 0

    by_file = {}
    for v in result.violations:
        by_file.setdefault(v.file, []).append(v)

    print(str(len(result.violations)) + " READINESS OVERCLAIM(S) DETECTED:")
    print()
    for filepath in sorted(by_file.keys()):
        print("  " + filepath + ":")
        for v in by_file[filepath]:
            print("    L" + str(v.line) + ": [" + v.rule + "]")
            print("           " + v.text)
        print()
    print("To fix, choose ONE (do NOT weaken the patterns):")
    print("  1. If the doc is a dated snapshot: add a stale/archive banner near the top")
    print("     (see the bannered docs/deployment/*.md siblings; point to docs/ci/CI_CURRENT_STATUS.md).")
    print("  2. If the claim is current and true: keep it (it stays flagged until proven by CI).")
    print("  3. If it is a genuine bounded exception: add it to ALLOWLIST with a reason.")
    print("See docs/dev/language-guide.md (Readiness claims + exception policy).")
    print()
    return 1


if __name__ == "__main__":
    sys.exit(main())
