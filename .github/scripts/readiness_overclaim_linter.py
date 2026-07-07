#!/usr/bin/env python3
"""
Readiness Overclaim Linter - CI Script

Scans ACTIVE, claim-sensitive guidance (deployment/operations guidance and the
pilots surface — see SCAN_DIRS) for un-disclaimed, affirmative, present-tense
readiness claims that misrepresent ICN's maturity:

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

Config (optional, --config PATH): a JSON object with either or both of
`scan_dirs` / `exclude_dirs` (lists of strings). Each key present REPLACES the
corresponding default list wholesale (not merged) — e.g. icn's own
.claim-lint.json lists every current default dir plus "website" so a reader
sees the full authoritative scope in one file. With no --config, scan scope
and findings are byte-identical to the hardcoded SCAN_DIRS/EXCLUDE_DIRS below.

Historical-proof-artifact category: a dated/status-named historical doc (e.g.
DEPLOYMENT_STATUS_2025-12-12.md) that still uses raw liveness language ("live",
"running", "operational", "in production") must carry an explicit marker
before that language is exempt:

    <!-- claim-class: historical-proof ref=<sha> date=<YYYY-MM-DD> evidence=<link/issue> -->

A marker missing `ref`, or carrying an unparseable `date`, does not count as
valid. Without a valid marker, each liveness-language line in such a doc is
flagged as `unmarked-historical-liveness` — a stale/archive BANNER alone lets
a doc keep describing what it once did, but this category exists specifically
to block "exercised once" quietly reading as "still live" with no citable
evidence trail. This is separate from (and does not replace) the
archive-banner exemption used by the affirmative-overclaim patterns above.

Usage:
    python3 .github/scripts/readiness_overclaim_linter.py [--repo-root PATH] [--config PATH]

Exit codes:
    0: No un-disclaimed readiness overclaims detected
    1: Overclaim(s) detected
    2: Script error
"""

import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, field
from datetime import date
from typing import Dict, List, Optional, Sequence, Set, Tuple

# ---------------------------------------------------------------------------
# Scan scope (directories, relative to repo root). Allowlist-based and widened
# deliberately, one cleaned surface at a time (a ratchet step — see
# docs/ci/GATE_RATCHET_PLAN.md). Start: deployment/operations guidance, where
# "PRODUCTION READY" headline claims are most dangerous. Added 2026-06-26:
# docs/pilots (organizer-/partner-facing, claim-sensitive; measured low-noise —
# 10 files, 1 bounded ALLOWLIST exception).
#
# docs/reference/project-index (the claim-discipline maps themselves) was added
# 2026-06-26 once its residual was driven to zero: the nonclaim-context precision
# below (section headings, "does not claim ..." lines, FAQ questions, nothing/none
# disclaimers) handled most hits, and the project-index-specific FP classes
# (caveat-prefixed "Unsafe ...: <phrase>", quoted avoid-lists `"production-ready"`,
# risk-register cells "live federation overclaim", "claim requires ..."
# meta-statements, checklist "nonclaims" items, narrow "without requiring a live
# federation ...") were closed by targeted rules below — 18 -> 0.
#
# NOT yet added (measured after this precision pass, still > 0): docs/demo (4),
# docs/strategy (4). Their residuals need their own bounded rules/allowlists first
# — a future ratchet step (docs/ci/GATE_RATCHET_PLAN.md).
# ---------------------------------------------------------------------------
SCAN_DIRS = [
    "docs/deployment",
    "docs/operations/deployment",
    "docs/pilots",
    "docs/reference/project-index",  # added 2026-06-26; generated/ pruned via EXCLUDE_DIRS
]

# Directory basenames pruned from every scan root: generated artifacts (they
# legitimately *quote* red-line phrases in "does NOT prove ..." disclaimers) and
# archival/historical trees (dated snapshots are exempt by design). Future-proofs
# the widening so adding a root that contains these subtrees stays low-noise.
EXCLUDE_DIRS = {"generated", "archive", "dev-journal"}

# File extensions scanned. ".astro" was added alongside the pre-existing ".md"
# so a --config that widens SCAN_DIRS to "website" (pure .astro pages) is
# actually scanned — none of the default SCAN_DIRS above contain .astro files,
# so this widening does not change default-config findings.
SCAN_EXTENSIONS = (".md", ".astro")


def load_scan_config(config_path: Optional[str]) -> Tuple[Sequence[str], Set[str]]:
    """Resolve (scan_dirs, exclude_dirs) from an optional --config JSON file.

    No path -> the hardcoded defaults, unchanged. A present `scan_dirs` or
    `exclude_dirs` key REPLACES the corresponding default list wholesale (not
    merged); an absent key keeps that default. Raises ValueError on any
    problem (missing file, invalid JSON, wrong value types) so main() can
    report it and exit with the documented "script error" code rather than
    silently falling back to defaults.
    """
    if config_path is None:
        return SCAN_DIRS, EXCLUDE_DIRS
    try:
        with open(config_path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        raise ValueError("cannot read/parse config " + config_path + ": " + str(e)) from e
    if not isinstance(data, dict):
        raise ValueError("config " + config_path + " must be a JSON object")

    if "scan_dirs" in data:
        scan_dirs = data["scan_dirs"]
        if not isinstance(scan_dirs, list) or not all(isinstance(d, str) for d in scan_dirs):
            raise ValueError("config scan_dirs must be a list of strings")
    else:
        scan_dirs = SCAN_DIRS

    if "exclude_dirs" in data:
        exclude_dirs_list = data["exclude_dirs"]
        if not isinstance(exclude_dirs_list, list) or not all(
            isinstance(d, str) for d in exclude_dirs_list
        ):
            raise ValueError("config exclude_dirs must be a list of strings")
        exclude_dirs: Set[str] = set(exclude_dirs_list)
    else:
        exclude_dirs = EXCLUDE_DIRS

    return scan_dirs, exclude_dirs

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
    r"\bnot\b|n't|\bno\b|\bnone\b|\bnothing\b|\bnever\b|not yet|\bwould\b|\bif\b|\bonce\b|\bwhen\b|"
    r"\btarget\b|\bgoal\b|aspir|roadmap|\bfuture\b|do not|don't|must not|\bavoid\b|"
    r"forbidden|prerequisite|before production|in a production deployment|in production:|"
    r"\U0001F7E1"  # yellow-circle status marker used for "assessed, not production-ready"
    r")"
)
# NOTE: the bare word "without" is deliberately NOT a negation here — it is too
# broad (it would wrongly exempt a real claim like "production-ready without
# caveats"). "nothing"/"none" are safe: a genuine claim's own clause never
# carries them ("Nothing about ICN is production-ready" is a disclaimer).

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
    "docs/pilots/summit-ops-lifecycle-package-map.md:89":
        "Red-line non-claim: the line is a '**Must not claim:** ...' enumeration "
        "('that any of this is production or live federation'). The leading "
        "'Must not claim:' negates the whole bullet, but it sits in an earlier "
        "clause than the matched phrase, so the line-local negation guard misses it.",
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


# Segment delimiters for nonclaim FRAMING — same as the clause delimiters but
# WITHOUT ":" so a "framing:" prefix ("Must not claim: ...", "Unsafe ...:") stays
# attached to the list/claim it introduces. ";"/"." still split, so framing in an
# earlier sentence/clause cannot reach a separate overclaim after them.
_NONCLAIM_DELIMS = set(".;|()") | {"—"}


def _framing_segment(line, start, end):
    """Return the framing segment (delimited by _NONCLAIM_DELIMS) containing the
    [start, end) match — used to scope NONCLAIM_LINE_RE so it cannot mask a
    separate overclaim in a later clause of the same line."""
    lo = start
    while lo > 0 and line[lo - 1] not in _NONCLAIM_DELIMS:
        lo -= 1
    hi = end
    while hi < len(line) and line[hi] not in _NONCLAIM_DELIMS:
        hi += 1
    return line[lo:hi]


# --- Nonclaim CONTEXT precision (line-local + nearest-heading; no parsing) ----
# A markdown heading whose text matches this starts a block that is, by
# construction, a list of things ICN does NOT claim. Every line until the next
# heading is exempt. Narrow and explicit.
NONCLAIM_SECTION_RE = re.compile(
    r"(?i)\b("
    r"non-?claims?|non-?goals?|red[\s-]?lines?|"
    r"what (?:not to|must not be|should not be) claim(?:ed)?|"
    r"what should not be shown(?: as finished)?|"
    r"forbidden collapses?|"
    r"claims? to avoid|"                            # "Claims to avoid", "Public/demo claims to avoid"
    r"what is not\b|"                               # "What is not organizer-ready / included / working"
    r"must not (?:imply|claim|be (?:shown|presented|claimed))"  # "What the website must not imply"
    r")\b"
)

# Explicit nonclaim FRAMING: the framing tells the reader NOT to claim something,
# so an overclaim phrase in the same framed segment is being forbidden, not
# asserted (e.g. "Must not claim: ... production or live federation.";
# "Unsafe without more evidence: ICN is production-ready.").
#
# This is applied to the matched phrase's SEGMENT (delimited by ./;/|/()/em-dash —
# but NOT ":" or ",", so a "framing:" prefix stays attached to the list it
# introduces), NOT the whole line — a whole-line skip would let framing in one
# clause silently mask a separate affirmative overclaim in a LATER clause, e.g.
# "Unsafe ...: production-ready; ICN is generally available." must still flag the
# second clause (see test_nonclaim_framing_does_not_mask_later_overclaim). Direct
# readiness negations ("not production-ready") stay out of here — NEGATION_RE
# handles those clause-by-clause.
NONCLAIM_LINE_RE = re.compile(
    r"(?i)("
    r"do(?:es)? not claim|must not (?:be )?claim(?:ed)?|cannot claim|never claim|"
    r"must not be (?:presented|shown)|should not be (?:claimed|presented|shown)|"
    r"not (?:a )?formal pilot|no formal pilot|not organi[sz]er-ready|"
    # Caveat prefixes that PRESENT the phrase as a bad example / unsafe claim
    # (e.g. "Unsafe without more evidence: ICN is production-ready ...").
    r"unsafe (?:without[^:]*evidence|claim|to (?:claim|say))|do not say|don't say|avoid saying|"
    # (NB: "<phrase> claim requires ..." meta-statements are handled per-match by
    # _describes_a_claim, NOT here — a line-level skip would mask a separate
    # affirmative overclaim in a later clause of the same line.)
    # A line that is itself enumerating nonclaims (e.g. a PR-checklist item
    # "Includes nonclaims for ... live federation"). Scoped to the "nonclaims for"
    # phrasing (tolerating markdown bold, "**nonclaims** for") — a BARE mention must
    # not suppress a separate overclaim in the same colon/comma-joined segment, since
    # _framing_segment does not split on ":" or "," (e.g. "Nonclaims no longer apply:
    # ICN is production-ready." and "This page lists nonclaims, but ICN is generally
    # available." must still flag).
    r"\bnonclaims?\**\s+for\b|"
    # Narrow, phrase-specific "without ..." exemption — NOT a blanket "without"
    # negation (that would mask "production-ready without caveats"): the federation
    # phrase is explicitly being avoided (e.g. "without requiring a live federation
    # step", "without claiming live federation").
    r"without (?:requiring|claiming|needing)\b[^.\n]{0,40}\blive federation"
    r")"
)

_HEADING_RE = re.compile(r"^\s{0,3}#{1,6}\s+(.*)$")
# Markdown decoration stripped from both ends before the interrogative test, so a
# question is recognised even when wrapped in heading/bold/quote/list/quote marks.
_MD_TRIM = " \t#>*_`-\"'"


def _is_interrogative(line):
    """True if the line, stripped of markdown decoration, is a question (an FAQ
    prompt is not an assertion unless a later answer line asserts it)."""
    return line.strip().strip(_MD_TRIM).strip().endswith("?")


_QUOTE_CHARS = "\"'`"


def _phrase_is_quoted(line, start, end):
    """True if the matched phrase is wrapped in a matching quote/backtick pair
    (e.g. `"production-ready"`). This is necessary but NOT sufficient to exempt the
    phrase — the caller additionally requires the line to sit inside an explicit
    avoid/forbidden list block (see `in_avoid_list`). Quoting alone never exempts a
    claim: a scare-quoted assertion like `ICN is "production-ready".`, or a quoted
    PAIR with no avoid framing like `"live federation" and "production-ready" are
    now true.`, is still a claim and must warn."""
    before = line[start - 1] if start > 0 else ""
    after = line[end] if end < len(line) else ""
    return before in _QUOTE_CHARS and after == before


# A line that INTRODUCES a list of things ICN must not claim — e.g.
# "**Avoid** (these claim past the evidence):" or "Claims to avoid:". Such a
# lead-in puts the following bullet block into "avoid-list" context, where quoted
# red-line phrases are being forbidden, not asserted. Requires explicit
# avoid/forbidden/nonclaim framing AND a trailing ":" (a list introducer), so a
# benign sentence ending in ":" cannot open the exemption.
_AVOID_LEADIN_FRAMING = re.compile(
    r"(?i)\b("
    r"avoid|forbidden|red[\s-]?lines?|nonclaims?|non-?claims?|claims? to avoid|"
    r"claim past the evidence|do(?:es)? not claim|must not (?:claim|be|say|use)|"
    r"do not (?:claim|say|use)|don't (?:claim|say|use)|never (?:claim|say)"
    r")\b"
)


def _is_avoid_leadin(line):
    """True if the line introduces an avoid/forbidden list (carries avoid framing
    AND ends with ':' after trailing markdown emphasis is stripped)."""
    core = line.rstrip().rstrip(" *_`")
    return core.endswith(":") and bool(_AVOID_LEADIN_FRAMING.search(line))


_LIST_ITEM_RE = re.compile(r"^\s*([-*+]|\d+[.)])\s+")


def _is_list_item(line):
    """True if the line is a markdown bullet / numbered list item."""
    return bool(_LIST_ITEM_RE.match(line))


def _labels_a_risk(line, end):
    """True if the matched phrase is immediately labelled as a risk/overclaim — a
    risk-register cell ("live federation overclaim") names the danger, not a claim."""
    return "overclaim" in line[end:end + 20].lower()


_CLAIM_META_RE = re.compile(r"(?i)^\s*claims?\s+(?:require|requires|need|needs|would require)\b")


def _describes_a_claim(line, end):
    """True if the matched phrase is immediately followed by "claim requires/needs"
    — a meta-statement ABOUT making the claim (e.g. "A live federation claim
    requires evidence"), not the claim itself. Scoped to the text right after the
    match so a separate affirmative overclaim later on the line still warns."""
    return bool(_CLAIM_META_RE.match(line[end:end + 24]))


def scan_lines(rel_path, lines):
    """Flag affirmative readiness claims; honour banner, nonclaim context,
    negation, and allowlist."""
    if is_banner_exempt(lines):
        return []

    violations = []
    in_nonclaim_section = False
    in_avoid_list = False
    for line_num, line in enumerate(lines, start=1):
        heading = _HEADING_RE.match(line)
        if heading:
            # Entering/leaving a nonclaim/red-line section toggles the context.
            in_nonclaim_section = bool(NONCLAIM_SECTION_RE.search(heading.group(1)))
            in_avoid_list = False  # a heading ends any open avoid-list block
        elif _is_avoid_leadin(line):
            # An "Avoid: ..."-style lead-in opens an avoid-list block: the bullet
            # lines it introduces are red-line phrases being forbidden.
            in_avoid_list = True
        elif line.strip() and not _is_list_item(line):
            # A non-blank, non-bullet line ends the block (blank lines and bullets
            # keep it open so a lead-in can be separated from its list by a blank).
            in_avoid_list = False

        # Whole-line/context exemptions: lines under a nonclaim/red-line section and
        # interrogative/FAQ prompts are not affirmative claims. (Nonclaim FRAMING is
        # checked PER-MATCH below, scoped to the matched phrase's segment.)
        if in_nonclaim_section or _is_interrogative(line):
            continue

        key = rel_path + ":" + str(line_num)
        if key in ALLOWLIST:
            continue
        flagged = False
        for pattern, rule in OVERCLAIM_PATTERNS:
            # Iterate EVERY occurrence: a suppressed first match (e.g. a quoted
            # example) must not hide a later real assertion of the same pattern.
            for m in pattern.finditer(line):
                # Per-match precision (all scoped to THIS match so a separate
                # overclaim elsewhere on the line still warns): explicit nonclaim
                # FRAMING in the matched phrase's segment ("Must not claim: ...",
                # "Unsafe ...:", "nonclaims for ...", "without requiring ... live
                # federation"); a quoted example phrase INSIDE an avoid-list block
                # (`- "production-ready", ...` under an "Avoid:" lead-in — quoting
                # alone is NOT enough, the avoid framing must be in scope); a
                # risk-labelled phrase ("live federation overclaim"); or a
                # "<phrase> claim requires ..." meta-statement.
                if (
                    NONCLAIM_LINE_RE.search(_framing_segment(line, m.start(), m.end()))
                    or (in_avoid_list and _phrase_is_quoted(line, m.start(), m.end()))
                    or _labels_a_risk(line, m.end())
                    or _describes_a_claim(line, m.end())
                ):
                    continue
                # Only the overclaim's own clause exempts it — not an unrelated
                # negation/conditional elsewhere on the same line.
                if NEGATION_RE.search(_clause_around(line, m.start(), m.end())):
                    continue
                violations.append(
                    Violation(file=rel_path, line=line_num, text=line.rstrip()[:160], rule=rule)
                )
                flagged = True
                break  # one violation per line is enough
            if flagged:
                break
    return violations


# --- Historical-proof-artifact category ---------------------------------
#
# A dated/status-named filename is the narrow, explicit signal that a doc is a
# point-in-time snapshot (mirrors the one real example in the repo today:
# DEPLOYMENT_STATUS_2025-12-12.md). This is deliberately filename-based, NOT
# "any banner-exempt file" — the banner mechanism above already has its own
# broader, content-based exemption for the affirmative-overclaim patterns;
# widening THIS category to every banner-exempt file would sweep in docs never
# named by the design this category implements, which is scope creep this PR
# does not take on.
HISTORICAL_FILENAME_RE = re.compile(
    r"(?i)(?:status|deployment|snapshot).*\d{4}[-_]\d{2}[-_]\d{2}|"
    r"\d{4}[-_]\d{2}[-_]\d{2}.*(?:status|deployment|snapshot)"
)

# The marker this category recognises. ref and date are required for the
# marker to be VALID (an invalid marker behaves as if no marker were present);
# evidence is documented but not required, matching the acceptance criteria
# ("ref/date required for marker validity").
HISTORICAL_MARKER_RE = re.compile(
    r"<!--\s*claim-class:\s*historical-proof\b([^>]*)-->", re.IGNORECASE
)
_MARKER_ATTR_RE = re.compile(r"(\w+)=(\S+)")

LIVENESS_RE = re.compile(r"(?i)\b(live|running|operational|in production)\b")


def is_historical_doc(rel_path: str) -> bool:
    """True if the filename itself marks the doc as a dated/status-named
    historical snapshot (see HISTORICAL_FILENAME_RE for the narrow scope)."""
    return bool(HISTORICAL_FILENAME_RE.search(os.path.basename(rel_path)))


def parse_historical_marker(lines) -> Optional[Dict[str, str]]:
    """Return the marker's attributes if a VALID `claim-class: historical-proof`
    marker is present anywhere in the file (ref present, date present and
    ISO-parseable); otherwise None. A syntactically-present but malformed
    marker (missing ref/date, or an unparseable date) returns None — it does
    not exempt anything, same as no marker at all."""
    for line in lines:
        m = HISTORICAL_MARKER_RE.search(line)
        if not m:
            continue
        attrs = dict(_MARKER_ATTR_RE.findall(m.group(1)))
        ref = attrs.get("ref")
        date_str = attrs.get("date")
        if not ref or not date_str:
            continue
        try:
            date.fromisoformat(date_str)
        except ValueError:
            continue
        return attrs
    return None


def scan_historical_liveness(rel_path, lines) -> List[Violation]:
    """Flag raw liveness language in a dated/status-named historical doc that
    lacks a valid claim-class: historical-proof marker. Only applies to docs
    is_historical_doc() recognises — an ordinary current doc's liveness
    language is not this category's concern. A valid marker exempts the whole
    file, mirroring how an archive banner exempts a file from the
    affirmative-overclaim patterns above (see module docstring)."""
    if not is_historical_doc(rel_path):
        return []
    if parse_historical_marker(lines) is not None:
        return []
    violations = []
    for line_num, line in enumerate(lines, start=1):
        if _is_interrogative(line):
            continue
        for m in LIVENESS_RE.finditer(line):
            if NEGATION_RE.search(_clause_around(line, m.start(), m.end())):
                continue
            violations.append(
                Violation(
                    file=rel_path,
                    line=line_num,
                    text=line.rstrip()[:160],
                    rule="unmarked-historical-liveness",
                )
            )
            break  # one violation per line is enough, consistent with scan_lines
    return violations


def scan_file(rel_path, abs_path):
    try:
        with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
    except OSError as e:
        print("Warning: could not read " + rel_path + ": " + str(e), file=sys.stderr)
        return []
    return scan_lines(rel_path, lines)


def run_lint(repo_root, scan_dirs: Optional[Sequence[str]] = None,
             exclude_dirs: Optional[Set[str]] = None):
    scan_dirs = SCAN_DIRS if scan_dirs is None else scan_dirs
    exclude_dirs = EXCLUDE_DIRS if exclude_dirs is None else exclude_dirs
    result = LintResult()
    for scan_dir in scan_dirs:
        abs_dir = os.path.join(repo_root, scan_dir)
        if not os.path.isdir(abs_dir):
            continue
        for dirpath, dirnames, filenames in os.walk(abs_dir):
            # Prune excluded subtrees (generated artifacts, archive/dev-journal)
            # in place so os.walk does not descend into them.
            dirnames[:] = [d for d in dirnames if d not in exclude_dirs]
            for name in sorted(filenames):
                if not name.endswith(SCAN_EXTENSIONS):
                    continue
                abs_path = os.path.join(dirpath, name)
                rel_path = os.path.relpath(abs_path, repo_root).replace(os.sep, "/")
                result.files_scanned += 1
                try:
                    with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
                        lines = f.read().splitlines()
                except OSError as e:
                    # Mirror scan_file()'s per-file tolerance: an unreadable
                    # file must not fail the whole run, only be skipped.
                    print("Warning: could not read " + rel_path + ": " + str(e), file=sys.stderr)
                    continue
                # The historical-liveness category runs regardless of banner
                # exemption — that is the point of this category: a banner
                # alone must not be enough to launder liveness language.
                historical_violations = scan_historical_liveness(rel_path, lines)
                result.violations.extend(historical_violations)
                if is_banner_exempt(lines):
                    # Only count the file as "exempt" in the summary when it
                    # is genuinely clean — a file with historical_violations
                    # is not exempt, it has findings, even though the banner
                    # skips the (separate) affirmative-overclaim scan below.
                    if not historical_violations:
                        result.files_exempt.append(rel_path)
                    continue
                result.violations.extend(scan_lines(rel_path, lines))
    return result


def main():
    parser = argparse.ArgumentParser(description="ICN Readiness Overclaim Linter")
    parser.add_argument("--repo-root", default=os.getcwd(), help="Repository root (default: cwd)")
    parser.add_argument(
        "--config",
        default=None,
        help="Optional JSON config path ({\"scan_dirs\": [...], \"exclude_dirs\": [...]}); "
        "omitted keys keep the built-in default. No --config = default behavior, unchanged.",
    )
    args = parser.parse_args()
    repo_root = os.path.abspath(args.repo_root)

    try:
        scan_dirs, exclude_dirs = load_scan_config(args.config)
    except ValueError as e:
        print("ERROR: " + str(e), file=sys.stderr)
        return 2

    print("=" * 70)
    print("Readiness Overclaim Linter")
    print("Un-disclaimed production / live-federation / governance-completion claims in active guidance")
    print("=" * 70)
    print()
    print("Repo root: " + repo_root)
    print("Scope: " + ", ".join(scan_dirs))
    if args.config:
        print("Config: " + args.config)
    print("Reference: docs/dev/language-guide.md")
    print()

    try:
        result = run_lint(repo_root, scan_dirs=scan_dirs, exclude_dirs=exclude_dirs)
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
    print("  4. If it is genuine historical proof (rule: unmarked-historical-liveness): add")
    print("     <!-- claim-class: historical-proof ref=<sha> date=<YYYY-MM-DD> evidence=<link/issue> -->")
    print("     (see this script's module docstring for the marker format).")
    print("See docs/dev/language-guide.md (Readiness claims + exception policy).")
    print()
    return 1


if __name__ == "__main__":
    sys.exit(main())
