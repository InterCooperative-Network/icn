#!/usr/bin/env python3
"""Fail-closed binding check: member-shell fixture paths must resolve.

The member shell (`web/member-shell/shell.js`) is the CURRENT member/organizer
rehearsal surface. Its demo mode fetches committed fixtures by RELATIVE path —
including two files it borrows from the legacy pilot-ui pack
(`web/pilot-ui/fixtures/icn-organizer-demo/`). That cross-surface binding is
load-bearing: the appliance image build (`deploy/appliance/build-image.sh`)
stages `web/member-shell` + `web/pilot-ui/fixtures` side by side precisely so
the shell's `../pilot-ui/fixtures/...` fetches keep working.

This check makes that contract mechanical: it extracts every fixture path
literal from shell.js, requires the extracted set to EQUAL the contractual
EXPECTED_BINDINGS set (a removed binding, an unreviewed new binding, and a
stale extraction pattern all fail), then resolves each path against the served
layout (page base = `web/member-shell/`, serve root = `web/`) and fails when
any referenced file is missing, escapes the serve root, or is not valid JSON.
If the fixture pack is ever moved (e.g. when pilot-ui is demoted) without
updating shell.js — or vice versa — this exits non-zero.

Offline, deterministic, <1s. Complements (does not replace)
`validate-rehearsal-shell-fixtures.py`, which validates the pack's CONTENT;
this script validates the CONSUMER BINDING.

Usage:
    python3 docs/scripts/check-member-shell-fixture-binding.py [--repo ROOT]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SHELL_JS = Path("web") / "member-shell" / "shell.js"
PAGE_BASE = Path("web") / "member-shell"
SERVE_ROOT = Path("web")

# Fixture path literals as they appear in shell.js: either member-shell-local
# ("fixtures/....json") or the cross-surface pilot-ui pack
# ("../pilot-ui/fixtures/....json").
FIXTURE_LITERAL = re.compile(
    r"""["']((?:\.\./pilot-ui/)?fixtures/[A-Za-z0-9._/-]+\.json)["']"""
)

# The CONTRACTUAL binding set. These nine paths are the shell's committed
# fixture surface (demo pack standing/cards borrowed from pilot-ui, local
# receipt + pending-publish, the community set of three, and the
# process-evidence pair). An explicit set — rather than a count floor — is
# deliberate: it fails when an expected binding disappears, when a NEW binding
# is added without review, and when the extraction pattern silently stops
# matching the surface (any of those makes the extracted set differ). The
# tradeoff: intentional fixture changes must update this set in the same PR,
# which is exactly the review moment the check exists to force.
EXPECTED_BINDINGS = frozenset(
    {
        "../pilot-ui/fixtures/icn-organizer-demo/standing.json",
        "../pilot-ui/fixtures/icn-organizer-demo/action-cards.json",
        "fixtures/demo-completion-receipt.json",
        "fixtures/pending-publish-summary.json",
        "fixtures/community-standing.json",
        "fixtures/community-action-cards.json",
        "fixtures/community-completion-receipt.json",
        "fixtures/process-evidence-receipts.json",
        "fixtures/process-evidence-export.json",
    }
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".", help="repo root (default: cwd)")
    args = parser.parse_args()
    repo = Path(args.repo).resolve()

    shell = repo / SHELL_JS
    if not shell.is_file():
        print(f"FAIL: {SHELL_JS} not found under {repo}", file=sys.stderr)
        return 1

    extracted = set(FIXTURE_LITERAL.findall(shell.read_text(encoding="utf-8")))
    missing = sorted(EXPECTED_BINDINGS - extracted)
    unexpected = sorted(extracted - EXPECTED_BINDINGS)
    if missing or unexpected:
        print(
            f"FAIL: {SHELL_JS} fixture bindings diverge from the contractual set "
            f"({len(EXPECTED_BINDINGS)} expected). If this change is intentional, "
            "update EXPECTED_BINDINGS in this script in the same PR.",
            file=sys.stderr,
        )
        for m in missing:
            print(f"  - expected binding no longer referenced: {m}", file=sys.stderr)
        for u in unexpected:
            print(f"  - unreviewed new binding: {u}", file=sys.stderr)
        return 1

    literals = sorted(extracted)

    serve_root = (repo / SERVE_ROOT).resolve()
    failures: list[str] = []
    for rel in literals:
        # Fetches resolve against the page URL (…/member-shell/), mirroring the
        # browser. resolve() collapses the "../" the same way the server does.
        target = (repo / PAGE_BASE / rel).resolve()
        if not target.is_relative_to(serve_root):
            failures.append(f"{rel} escapes the serve root ({SERVE_ROOT})")
            continue
        if not target.is_file():
            failures.append(f"{rel} -> {target.relative_to(repo)} does not exist")
            continue
        try:
            json.loads(target.read_text(encoding="utf-8"))
        except (ValueError, OSError) as exc:
            failures.append(f"{rel} -> {target.relative_to(repo)} is not valid JSON: {exc}")

    if failures:
        print("FAIL: member-shell fixture binding broken:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"OK: {len(literals)} member-shell fixture bindings resolve inside the "
        f"serve root and parse as JSON."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
