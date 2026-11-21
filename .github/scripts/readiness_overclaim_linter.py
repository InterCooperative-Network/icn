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

Config (optional, --config PATH): a JSON object with any of `scan_dirs` /
`exclude_dirs` (lists of strings) and `scan_manifest` (a repo-relative path to a
{"files": [{"path", "bannered"}]} manifest of the files that actually reach a
public surface — see load_manifest). `scan_manifest` exists because the public
docs surface is not a directory: `docs/` is mostly withheld from the site, so
"scan all of docs/" would gate private material while "scan none of it" leaves
every republished page ungated. A missing or malformed manifest is a hard error
(exit 2), never a quiet empty scan. Each key present REPLACES the
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


def load_scan_config(
    config_path: Optional[str],
) -> Tuple[Sequence[str], Set[str], Optional[str]]:
    """Resolve (scan_dirs, exclude_dirs, scan_manifest) from an optional --config
    JSON file.

    No path -> the hardcoded defaults, unchanged. A present `scan_dirs` or
    `exclude_dirs` key REPLACES the corresponding default list wholesale (not
    merged); an absent key keeps that default. `scan_manifest` is a repo-relative
    path to a published-file manifest (see load_manifest) and defaults to None,
    i.e. directory scanning only. Raises ValueError on any problem (missing file,
    invalid JSON, wrong value types) so main() can report it and exit with the
    documented "script error" code rather than silently falling back to defaults.
    """
    if config_path is None:
        return SCAN_DIRS, EXCLUDE_DIRS, None
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

    if "scan_manifest" in data:
        scan_manifest = data["scan_manifest"]
        if not isinstance(scan_manifest, str) or not scan_manifest:
            raise ValueError("config scan_manifest must be a non-empty string")
    else:
        scan_manifest = None

    return scan_dirs, exclude_dirs, scan_manifest


def load_manifest(repo_root: str, manifest_rel: str) -> List[Tuple[str, bool]]:
    """Read a published-file manifest and return sorted [(rel_path, bannered)].

    The manifest names the files that actually reach a public surface, which a
    directory walk cannot express: `docs/` is mostly WITHHELD from the site, so
    scanning all of it would gate private material, and scanning none of it (the
    previous state) left every republished page ungated. `bannered` means the
    published RENDERING of that file carries a stale/archive banner, and is
    honoured exactly as a banner found in the source text.

    Shape:
        {"files": [{"path": "docs/X.md", "bannered": false}, ...]}

    FAILS CLOSED. Every problem raises ValueError so main() exits 2. The manifest
    is a build artifact, so "absent" means "generation did not run" — a gate that
    then quietly scanned nothing and passed would be worse than no gate at all,
    because it would report success.
    """
    abs_path = os.path.join(repo_root, manifest_rel)
    try:
        with open(abs_path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        raise ValueError(
            "cannot read/parse scan_manifest " + manifest_rel + ": " + str(e)
        ) from e
    if not isinstance(data, dict) or not isinstance(data.get("files"), list):
        raise ValueError(
            "scan_manifest " + manifest_rel + " must be a JSON object with a 'files' list"
        )

    entries: Dict[str, bool] = {}
    for i, item in enumerate(data["files"]):
        where = manifest_rel + " files[" + str(i) + "]"
        if not isinstance(item, dict):
            raise ValueError(where + " must be an object")
        rel = item.get("path")
        bannered = item.get("bannered", False)
        if not isinstance(rel, str) or not rel:
            raise ValueError(where + " needs a non-empty string 'path'")
        if not isinstance(bannered, bool):
            raise ValueError(where + " 'bannered' must be a boolean")
        slashed = rel.replace(os.sep, "/")
        norm = os.path.normpath(rel).replace(os.sep, "/")
        # Reject absolute paths, parent escapes, and un-normalised spellings
        # ("docs/./x.md") rather than normalising them: a manifest that does not
        # say plainly which file it means is a generator bug.
        if os.path.isabs(rel) or norm != slashed or norm.startswith(".."):
            raise ValueError(
                where + " path must be a normalised repo-relative path: " + rel
            )
        if not os.path.isfile(os.path.join(repo_root, norm)):
            raise ValueError(where + " path does not exist in the repo: " + rel)
        # Deduplicate. Conflicting metadata for one path is a generator bug, not
        # something to silently resolve in favour of the laxer value.
        if norm in entries and entries[norm] != bannered:
            raise ValueError(
                manifest_rel + " lists " + norm + " twice with conflicting 'bannered'"
            )
        entries[norm] = bannered
    if not entries:
        raise ValueError("scan_manifest " + manifest_rel + " lists no files")
    return sorted(entries.items())

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
    r"\bneither\b|"  # "Neither proof claims production reachability or live federation"
    r"forbidden|prerequisite|before production|in a production deployment|in production:|"
    r"\U0001F7E1|"  # yellow-circle status marker used for "assessed, not production-ready"
    # Same precedent, other markers the corpus actually uses: "❌ **General
    # Availability**: not yet in this snapshot" and "⏳" for pending.
    r"\u274C|\u23F3"
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

    # ── Policy/meta documents that must name the phrases they govern ─────────
    # These four files DEFINE the claim discipline. They cannot describe a
    # forbidden phrase without writing it, and no parser rule should try to tell
    # "quoting the rule" from "breaking the rule" in prose.
    "docs/dev/language-guide.md:222":
        "The language guide itself: the line enumerates what a reader must not be "
        "led to believe ('... that ICN is production-ready, that a live federation "
        "is operating ...'). Quoting the forbidden claim IS this document's job.",
    "docs/dev/language-guide.md:223":
        "Continuation of the same enumeration in docs/dev/language-guide.md:222 "
        "('... proposal/vote/member-standing governance is complete').",
    "docs/ci/GATE_RATCHET_PLAN.md:121":
        "Specification of this very linter's precision rules; the line lists the "
        "shapes it must tolerate, including the question form 'Is this ready for "
        "production?'. Naming the pattern is not asserting it.",
    "docs/ci/GATE_RATCHET_PLAN.md:123":
        "Same specification: the line enumerates caveat prefixes and quoted "
        "avoid-lists that the linter must not flag, one of which is the literal "
        "string \"production-ready\".",
    "docs/guides/developer/agent-context-spine.md:68":
        "Describes a validation step that greps for overclaim language, quoting the "
        "terms it greps for ('production ready', 'live federation').",

    # ── Nonclaim framing the line-local guards cannot reach ─────────────────
    # Each is a red line being drawn, not a claim being made. They are listed
    # individually rather than as a parser rule because each sits in a DIFFERENT
    # syntactic position, and widening the guards to cover them would weaken the
    # clause scoping that keeps a genuine claim catchable on the same line.
    "docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md:110":
        "The negation is an em-dash appositive: 'Generated UI kits depict signed "
        "actions, live federation, ... - none of which the repo has shipped "
        "end-to-end'. The em dash is a clause delimiter, so the negation lands in "
        "the next clause and the line-local guard cannot see it.",
    "docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md:145":
        "The phrase is an 'e.g.' example inside parentheses ('(e.g. live "
        "federation between two coops)') whose host sentence labels such surfaces "
        "'future-state / roadmap'. Parentheses are clause delimiters, so the "
        "labelling is out of the matched clause.",
    "docs/design/assets/briefs/VE-002-scope-model.md:14":
        "Same shape as docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md:145: '(e.g. a live "
        "federation between two real cooperatives)' introduced as an example and "
        "labelled 'future-state / roadmap' outside the parenthetical.",
    "docs/design/assets/ASSET_REGISTER.md:43":
        "Register table cell reading 'future-state / roadmap (live federation)' - "
        "the qualifier immediately precedes the parenthetical it qualifies, and the "
        "row's own status column says 'planned'.",
    "docs/design/evidence-packet-produced-receipt-decision-rung.md:173":
        "Final bullet of a red-line list opened by \"':v1' 'produced' **explicitly "
        "excludes** (must be stated as non-claims ...):\". The avoid-list block is "
        "tracked, but it only exempts phrases that are QUOTED, and this list writes "
        "them bare.",
    "docs/demo/GOVERNANCE_PROPOSAL_FIXTURE_HANDOFF.md:31":
        "Scope exclusion: '... without adding backend demo mode, real signing, real "
        "vote submission, live federation, ... or production claims.' The narrow "
        "'without ...' exemption in NONCLAIM_LINE_RE is scoped to 'without "
        "requiring', deliberately not a blanket 'without'.",
    "docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md:258":
        "Acceptance criterion describing the term list an overclaim grep must "
        "cover, and asserting those terms may appear 'only in explicit non-claim / "
        "red-line / out-of-scope contexts'.",
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


# --- Rendered-text normalisation -------------------------------------------
#
# An HTML comment is not a public claim: the docs site renders markdown, and
# `<!-- ... -->` never reaches the page. docs/STATE.md carries machine-readable
# sync notes in comments that enumerate what a change does NOT claim; scanning
# the raw bytes reported those as 23 affirmative overclaims on a surface no
# reader can see.
#
# Comment spans are blanked with SPACES rather than deleted, so line numbers AND
# column offsets survive — every column-based guard below (_clause_around,
# _framing_segment, _phrase_is_quoted) keeps working unchanged.
#
# Two things deliberately keep reading the RAW lines, not this view:
#   - is_banner_exempt(), so stripping cannot silently revoke an exemption;
#   - parse_historical_marker(), because the claim-class marker IS a comment.
#
# Inside a fenced code block a comment is displayed literally, so it IS rendered
# and is left intact.
_FENCE_RE = re.compile(r"^\s{0,3}(`{3,}|~{3,})")


def fenced_line_numbers(lines):
    """1-based line numbers of fence markers and the lines they enclose.

    A fenced block is a code sample, not prose: "backend = \"age\"  # Software
    keystore (production-ready)" is configuration being shown, and
    "/etc/letsencrypt/live/api.example.org/..." is a path, not a live endpoint.
    Skipping these lines BEFORE the heading/avoid-list state machine also fixes a
    latent bug: a "# comment" inside a shell fence otherwise parses as a markdown
    heading and can reset nonclaim-section state mid-document.
    """
    fenced = set()
    fence = None
    for i, line in enumerate(lines, start=1):
        m = _FENCE_RE.match(line)
        if m:
            tok = m.group(1)[0] * 3
            if fence is None:
                fence = tok
            elif fence == tok:
                fence = None
            fenced.add(i)
            continue
        if fence is not None:
            fenced.add(i)
    return fenced


def strip_html_comments(lines):
    """Return `lines` with HTML-comment spans replaced by spaces.

    Line count and column offsets are preserved exactly. Fenced code blocks are
    left untouched (a comment shown as code is rendered text)."""
    out = []
    in_comment = False
    fence = None
    for line in lines:
        if not in_comment:
            m = _FENCE_RE.match(line)
            if m:
                tok = m.group(1)[0] * 3
                if fence is None:
                    fence = tok
                elif fence == tok:
                    fence = None
                out.append(line)
                continue
        if fence is not None:
            out.append(line)
            continue
        if not in_comment and "<!--" not in line:
            out.append(line)
            continue
        chars = list(line)
        i, n = 0, len(line)
        while i < n:
            if in_comment:
                j = line.find("-->", i)
                end = n if j < 0 else j + 3
                for k in range(i, end):
                    chars[k] = " "
                if j < 0:
                    i = n
                else:
                    in_comment = False
                    i = end
            else:
                j = line.find("<!--", i)
                if j < 0:
                    break
                in_comment = True
                for k in range(j, min(j + 4, n)):
                    chars[k] = " "
                i = j + 4
        out.append("".join(chars))
    return out


# --- Inline negating lead-in ------------------------------------------------
#
# `_is_avoid_leadin` only recognises a lead-in that ENDS a line ("Must not
# claim:" followed by bullets). A lead-in can also govern an enumeration on its
# OWN line:
#
#   This sync explicitly does NOT claim: a session lifecycle; ...; live
#   federation; Phase 2 completion.
#
# `_clause_around` splits on ";" and hands the guard the bare fragment
# " live federation", so the negation two clauses earlier never reaches it and a
# red-line list reads as an affirmative claim. The scope of such a lead-in runs
# from its ":" to the end of the sentence it opens — a following sentence is
# NOT covered, so "... does not claim X. ICN is production-ready." still flags.

# The ":" must follow the framing phrase IMMEDIATELY — only whitespace and
# closing markdown emphasis may intervene. Words between the two can invert the
# meaning: "Nonclaims no longer apply: ICN is production-ready." carries avoid
# framing and a colon, but it REVOKES the nonclaims and then makes a real claim.
# Requiring adjacency keeps "does NOT claim:" in and "no longer apply:" out.
# One notion of "the sentence ended", shared by the inline and cross-line
# negation scopes.
_SENTENCE_END_ANY_RE = re.compile(r"[.!?](?=\s|$)")

# (No "^" anchor: Pattern.match(line, pos) already anchors at pos, whereas "^"
# would only ever match at offset 0.)
_LEADIN_COLON_GAP_RE = re.compile(r"[\s*_`)\]]*:")


def _inline_negation_scope(line, start):
    """True if an inline negating lead-in earlier on `line` governs the match at
    `start` — i.e. a "does not claim"-style framing is immediately followed by
    ":" before the match, with no sentence boundary in between."""
    colon = None
    for m in _AVOID_LEADIN_FRAMING.finditer(line):
        gap = _LEADIN_COLON_GAP_RE.match(line, m.end())
        if gap is None:
            continue
        c = gap.end() - 1
        if c < start and (colon is None or c > colon):
            colon = c
    if colon is None:
        return False
    term = _SENTENCE_END_ANY_RE.search(line, colon)
    return not (term and term.start() < start)


# --- negated sentences that wrap across lines --------------------------------
#
# Every guard above is line-local, so a negated sentence that soft-wraps puts its
# framing out of reach. The corpus hard-wraps prose at ~80 columns, so this is
# structural, not incidental:
#
#   "... It does not"                            <- framing ends the line
#   "adopt itself, authorize a production deployment, or certify any profile as"
#   "production-ready."                          <- flagged, with no negation in sight
#
#   "**It may not claim:** production-ready, pilot-ready, organizer-approved,"
#   "accessibility-complete, live federation, real institutional deployment, formal"
#
# The scope is deliberately bounded by the SENTENCE, not the paragraph: on a
# continuation line only matches BEFORE the first sentence terminator are
# excused, so "...does not claim: a, b," / "c. ICN is production-ready." still
# flags the second sentence. A blank line, a heading, or a fence also ends it.
# Deliberately the "not claiming" family only. The copulas (is/are/was/were not)
# are ordinary prose negation: including them would open a continuation scope on
# any wrapped sentence containing "is not", which is far more of the corpus than
# this rule needs. `does not` is the only form the corpus actually exercises; the
# red-line modals are kept beside it because they carry the same intent.
_DANGLING_NEGATOR_RE = re.compile(
    r"(?i)(?:\b(?:does|do|did|must|may|can|could|will|would|shall)\s+not"
    r"|\b(?:cannot|never))\s*[:,]?\s*$"
)


def _opens_negated_continuation(line):
    """True if `line` leaves a negating sentence unfinished."""
    if _DANGLING_NEGATOR_RE.search(line):
        return True
    # A nonclaim lead-in whose enumeration STARTS on this line and has not ended.
    # The enumeration must actually begin here: a line ending at its own colon
    # ("We do not claim:") hands off to the next line, where the existing
    # avoid-list state machine governs — and that machine deliberately requires
    # BULLETS, so a plain prose sentence after such a lead-in still flags.
    for m in _AVOID_LEADIN_FRAMING.finditer(line):
        gap = _LEADIN_COLON_GAP_RE.match(line, m.end())
        if gap is None:
            continue
        rest = line[gap.end():]
        if not rest.strip(" \t*_`"):
            continue
        if not _SENTENCE_END_ANY_RE.search(rest):
            return True
    return False


# --- label: value pairs -----------------------------------------------------
#
# ":" is a _CLAUSE_DELIMS member, which is right for splitting independent
# assertions ("ICN is not experimental; it is PRODUCTION READY.") but wrong for a
# label and the value that qualifies it. The qualifier can sit on either side:
#
#   "**Target:** Production-ready Q1 2026"        qualifier in the LABEL
#   "❌ **General Availability**: not yet ..."     qualifier in the VALUE
#
# Both are ONE assertion, and _clause_around hands the guard only half of it.
# This crosses exactly ONE ":" boundary and never a "." or ";", so a genuinely
# separate assertion later on the line is still out of reach — "Status: ICN is
# production-ready." has no qualifier on either side and still flags.


# A label position only qualifies when the label IS essentially the qualifier —
# a field name, not a clause. "Nonclaims no longer apply: ICN is production-ready."
# carries a negation word in its label but ASSERTS something, and must still flag;
# requiring the label to reduce to a bare qualifier keeps it out.
_QUALIFIER_LABEL_RE = re.compile(
    r"(?i)^[\s*_`#>\d.)\-]*"
    r"(target|goal|objective|milestone|aspiration|roadmap|planned|eta|future)"
    r"[\s*_`]*$"
)


def _label_value_negation(line, start):
    """True if the match is the VALUE of a `qualifier-label:` pair.

    Only this direction needs handling. The mirror case ("❌ **General
    Availability**: not yet in this snapshot", where the qualifier sits in the
    value) is covered by the ordinary clause mechanism, because the ❌ status
    marker is itself in NEGATION_RE — no extra machinery required.
    """
    lo = start
    while lo > 0 and line[lo - 1] not in _CLAUSE_DELIMS:
        lo -= 1

    # The label qualifies the match only if it reduces to a bare forward-looking
    # field name ("**Target:**"). Exactly one ":" boundary is crossed, and never a
    # "." or ";", so a separate assertion later on the line stays in reach.
    if lo > 0 and line[lo - 1] == ":":
        lo2 = lo - 1
        while lo2 > 0 and line[lo2 - 1] not in _CLAUSE_DELIMS:
            lo2 -= 1
        if _QUALIFIER_LABEL_RE.match(line[lo2:lo - 1]):
            return True

    return False


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
    r"must not (?:imply|claim|be (?:shown|presented|claimed))|"  # "What the website must not imply"
    # Scope-boundary headings. A section that names itself as work NOT done by
    # this document enumerates red lines exactly like a "Non-goals" section does;
    # the receipt-contract family writes it as "N. Deferred work (explicitly out
    # of scope of this contract ...)". Kept to explicit scope-exclusion wording —
    # "Future work" and "Roadmap" are deliberately NOT here, because those
    # sections do make forward-looking assertions of their own.
    r"deferred work|out of scope|not in scope|"
    r"not[\s-]yet[\s-]done|"                            # "What stays explicitly not-yet-done"
    r"claims?\b[^.]{0,40}does not make"                 # "Claims this doctrine does not make"
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
    # "**It may not claim:** production-ready, ..." — same act of not-claiming,
    # different modal. cannot/can not included; NONCLAIM_LINE_RE already knows
    # "cannot claim" for its own (line-level) purpose.
    r"(?:may|can|could|shall|will|would) ?not,? (?:claim|be claimed)|cannot claim|"
    r"never (?:be used to )?claim|"        # "must never be used to claim: ..."
    r"not (?:built|implemented|shipped)|"  # "Future lanes ... **not built**: ..."
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

    # Banner detection above reads the RAW lines; claim scanning below reads the
    # rendered view, because an HTML comment is not a public claim.
    lines = strip_html_comments(lines)
    fenced = fenced_line_numbers(lines)

    violations = []
    in_nonclaim_section = False
    in_avoid_list = False
    negated_cont = False
    for line_num, line in enumerate(lines, start=1):
        # Fenced content is a code sample, never a prose claim. Checked before the
        # heading/avoid-list bookkeeping so fence content cannot drive that state.
        if line_num in fenced:
            negated_cont = False
            continue
        if not line.strip():
            negated_cont = False
        heading = _HEADING_RE.match(line)
        if heading:
            # Entering/leaving a nonclaim/red-line section toggles the context.
            in_nonclaim_section = bool(NONCLAIM_SECTION_RE.search(heading.group(1)))
            in_avoid_list = False  # a heading ends any open avoid-list block
            negated_cont = False   # ...and any open negated sentence
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

        # How far into this line an unfinished negated sentence still reaches: up
        # to the first sentence terminator OR the first ":" — a colon on a
        # continuation line introduces a fresh assertion ("... is not" /
        # "a drill: ICN is production-ready."), and no wrapped red-line list in
        # the corpus needs to cross one.
        cont_cutoff = -1
        if negated_cont:
            stops = [m.start() for m in (_SENTENCE_END_ANY_RE.search(line),) if m]
            colon = line.find(":")
            if colon >= 0:
                stops.append(colon)
            cont_cutoff = min(stops) if stops else len(line)
        if negated_cont and _SENTENCE_END_ANY_RE.search(line):
            negated_cont = False
        if _opens_negated_continuation(line):
            negated_cont = True

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
                    or _inline_negation_scope(line, m.start())
                    or _label_value_negation(line, m.start())
                    or m.start() < cont_cutoff
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
    # The marker lookup above needs the RAW lines (the marker is itself an HTML
    # comment). Liveness language is then matched against the rendered view.
    lines = strip_html_comments(lines)
    fenced = fenced_line_numbers(lines)
    violations = []
    for line_num, line in enumerate(lines, start=1):
        if line_num in fenced or _is_interrogative(line):
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


def _walk_targets(repo_root, scan_dirs, exclude_dirs):
    """Yield repo-relative paths of scannable files under each scan_dir."""
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
                yield os.path.relpath(abs_path, repo_root).replace(os.sep, "/")


def run_lint(repo_root, scan_dirs: Optional[Sequence[str]] = None,
             exclude_dirs: Optional[Set[str]] = None,
             manifest_entries: Optional[Sequence[Tuple[str, bool]]] = None):
    scan_dirs = SCAN_DIRS if scan_dirs is None else scan_dirs
    exclude_dirs = EXCLUDE_DIRS if exclude_dirs is None else exclude_dirs

    # rel_path -> bannered, so a file reachable BOTH by directory walk and by the
    # manifest is read and reported exactly once. A walked file carries no banner
    # metadata of its own (False); where the manifest says `bannered`, that wins,
    # because it carries the rendered-site invariant a source walk cannot see.
    targets: Dict[str, bool] = {}
    for rel_path in _walk_targets(repo_root, scan_dirs, exclude_dirs):
        targets.setdefault(rel_path, False)
    for rel_path, bannered in manifest_entries or ():
        targets[rel_path] = targets.get(rel_path, False) or bannered

    result = LintResult()
    for rel_path in sorted(targets):
        bannered = targets[rel_path]
        abs_path = os.path.join(repo_root, rel_path)
        result.files_scanned += 1
        try:
            with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
                lines = f.read().splitlines()
        except OSError as e:
            # Mirror scan_file()'s per-file tolerance: an unreadable file must
            # not fail the whole run, only be skipped.
            print("Warning: could not read " + rel_path + ": " + str(e), file=sys.stderr)
            continue
        # The historical-liveness category runs regardless of banner exemption —
        # that is the point of this category: a banner alone must not be enough
        # to launder liveness language. This holds for a manifest `bannered`
        # file exactly as it holds for a source banner.
        historical_violations = scan_historical_liveness(rel_path, lines)
        result.violations.extend(historical_violations)
        if bannered or is_banner_exempt(lines):
            # Only count the file as "exempt" in the summary when it is genuinely
            # clean — a file with historical_violations is not exempt, it has
            # findings, even though the banner skips the (separate)
            # affirmative-overclaim scan below.
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
        scan_dirs, exclude_dirs, scan_manifest = load_scan_config(args.config)
        manifest_entries = (
            load_manifest(repo_root, scan_manifest) if scan_manifest else None
        )
    except ValueError as e:
        print("ERROR: " + str(e), file=sys.stderr)
        return 2

    print("=" * 70)
    print("Readiness Overclaim Linter")
    print("Un-disclaimed production / live-federation / governance-completion claims in active guidance")
    print("=" * 70)
    print()
    print("Repo root: " + repo_root)
    scope_parts = list(scan_dirs)
    if scan_manifest:
        scope_parts.append(
            scan_manifest + " (" + str(len(manifest_entries)) + " published files)"
        )
    print("Scope: " + (", ".join(scope_parts) if scope_parts else "(empty)"))
    if args.config:
        print("Config: " + args.config)
    print("Reference: docs/dev/language-guide.md")
    print()

    try:
        result = run_lint(repo_root, scan_dirs=scan_dirs, exclude_dirs=exclude_dirs,
                          manifest_entries=manifest_entries)
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
