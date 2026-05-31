#!/usr/bin/env python3
"""
API Doc Canonical-Vocabulary Guard - CI Script

Ties the hand-written API prose docs to the ACTUAL gateway route code, so they
cannot drift back to the pre-#1315 payment-rail vocabulary.

Per ADR-0006 (PR #1315) the gateway renamed its ledger surface:
    payment  -> settlement   (POST /v1/ledger/{coop}/settle, event SettlementCreated)
    balance  -> position     (GET  /v1/ledger/{coop}/position/{did})
    currency -> unit         (settlement request field)
The old routes are NOT served (they 404). The canonical names are verified
below by reading the route attributes in icn-gateway/src/api/ledger.rs.

This guard checks only the hand-written API docs (the machine spec
docs/api/openapi.generated.yaml is kept in sync with code by the api-types CI
job, and the SDK client is already canonical). It is intentionally narrow.

Checked docs:
    docs/api/README.md, docs/api/OPENAPI.md, docs/api/examples.sh

A deprecated term is allowed only inside a heading section whose title contains
"legacy" or "deprecated", or on a line explicitly marking it as such
(deprecated / legacy / renamed / not served / removed / a -> mapping arrow).
This lets the docs keep an honest old->new migration table without regressing.

Usage:
    python3 docs/api/check_api_doc_terms.py [--repo-root PATH]

Exit codes:
    0: docs use canonical primitives; no deprecated vocabulary presented as current
    1: deprecated vocabulary presented as current (violation)
    2: could not verify code truth (gateway ledger routes not found)
"""

import argparse
import os
import re
import sys

LEDGER_ROUTES_FILE = "icn/crates/icn-gateway/src/api/ledger.rs"

CHECKED_DOCS = [
    "docs/api/README.md",
    "docs/api/OPENAPI.md",
    "docs/api/examples.sh",
]

# Deprecated forms that must NOT appear as current API. Path-shaped patterns only
# (no bare words like "balance"/"payment" which legitimately appear in prose such
# as "credit limit" or internal double-entry balances).
DEPRECATED_PATTERNS = [
    (re.compile(r"ledger/[^\s`'\"]*payment"), "legacy payment route -> POST /v1/ledger/{coop}/settle"),
    (re.compile(r"ledger/[^\s`'\"]*balance"), "legacy balance route -> GET /v1/ledger/{coop}/position/{did}"),
    (re.compile(r"\bPaymentCreated\b"), "legacy event PaymentCreated -> SettlementCreated"),
    (re.compile(r"(?i)\bmake a payment\b"), '"Make a Payment" -> "Record a Settlement"'),
]

# A line is exempt if it is clearly labelled as legacy/deprecated context.
LINE_EXEMPT_RE = re.compile(r"(?i)(deprecated|legacy|renamed|not served|removed|->|→|↦)")
# A heading whose text marks a legacy/deprecated section exempts following lines.
HEADING_RE = re.compile(r"^#{1,6}\s+(.*)$")
LEGACY_HEADING_RE = re.compile(r"(?i)(legacy|deprecated)")


def canonical_routes_from_code(repo_root):
    """Read route attributes from the gateway ledger source (code truth)."""
    path = os.path.join(repo_root, LEDGER_ROUTES_FILE)
    if not os.path.isfile(path):
        return None
    segments = set()
    attr_re = re.compile(r'#\[(?:get|post|put|delete)\("([^"]+)"\)\]')
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            m = attr_re.search(line)
            if m:
                for seg in m.group(1).strip("/").split("/"):
                    if seg and not seg.startswith("{"):
                        segments.add(seg)
    return segments


def scan_doc(repo_root, rel_path):
    """Return list of (line_no, text, rule) violations for one doc."""
    abs_path = os.path.join(repo_root, rel_path)
    if not os.path.isfile(abs_path):
        return []
    violations = []
    in_legacy_section = False
    with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
        for line_num, line in enumerate(f, start=1):
            heading = HEADING_RE.match(line.strip())
            if heading:
                in_legacy_section = bool(LEGACY_HEADING_RE.search(heading.group(1)))
                continue
            if in_legacy_section or LINE_EXEMPT_RE.search(line):
                continue
            for pattern, rule in DEPRECATED_PATTERNS:
                if pattern.search(line):
                    violations.append((line_num, line.rstrip()[:140], rule))
                    break
    return violations


def main():
    parser = argparse.ArgumentParser(description="ICN API doc canonical-vocabulary guard")
    parser.add_argument("--repo-root", default=os.getcwd(), help="Repository root (default: cwd)")
    args = parser.parse_args()
    repo_root = os.path.abspath(args.repo_root)

    print("=" * 70)
    print("API Doc Canonical-Vocabulary Guard")
    print("Hand-written API docs must match the gateway's ICN-native routes")
    print("=" * 70)
    print()

    routes = canonical_routes_from_code(repo_root)
    if not routes:
        print("ERROR: could not read gateway ledger routes from " + LEDGER_ROUTES_FILE, file=sys.stderr)
        print("Cannot verify doc/code agreement; refusing to pass.", file=sys.stderr)
        return 2
    for required in ("settle", "position"):
        if required not in routes:
            print("ERROR: canonical route segment '" + required + "' not found in code.", file=sys.stderr)
            print("Routes seen: " + ", ".join(sorted(routes)), file=sys.stderr)
            print("Either the gateway changed or the guard is stale; refusing to pass.", file=sys.stderr)
            return 2
    print("Code truth (icn-gateway ledger routes): settle, position present. OK")
    print()

    all_violations = {}
    for rel in CHECKED_DOCS:
        v = scan_doc(repo_root, rel)
        if v:
            all_violations[rel] = v

    # Positive assertion: README must present the canonical surface.
    readme = os.path.join(repo_root, "docs/api/README.md")
    missing = []
    if os.path.isfile(readme):
        text = open(readme, "r", encoding="utf-8", errors="replace").read()
        for needle in ("/ledger/{coop}/settle", "/ledger/{coop}/position/{did}", "SettlementCreated"):
            if needle not in text:
                missing.append(needle)

    if not all_violations and not missing:
        print("OK: API docs use canonical ICN primitives (settle / position /")
        print("SettlementCreated); no deprecated payment/balance vocabulary is")
        print("presented as current.")
        return 0

    if missing:
        print("MISSING canonical references in docs/api/README.md:")
        for m in missing:
            print("  - expected to find: " + m)
        print()

    if all_violations:
        total = sum(len(v) for v in all_violations.values())
        print(str(total) + " DEPRECATED-AS-CURRENT VOCABULARY VIOLATION(S):")
        print()
        for rel in sorted(all_violations):
            print("  " + rel + ":")
            for line_num, text, rule in all_violations[rel]:
                print("    L" + str(line_num) + ": [" + rule + "]")
                print("           " + text)
            print()

    print("Fix: use the canonical route/event names (see ADR-0006). If you must")
    print("reference the old names, put them under a heading containing 'Legacy' or")
    print("'Deprecated', or mark the line as deprecated/renamed. Do NOT present the")
    print("old /payment, /balance, or PaymentCreated names as the current API.")
    print()
    return 1


if __name__ == "__main__":
    sys.exit(main())
