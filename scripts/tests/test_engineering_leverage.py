#!/usr/bin/env python3
"""Behaviour fixtures for the systemic-leverage policy.

These do not test prose. They test that the canonical owner actually carries the
discriminating information an agent needs to reach the intended classification —
and, importantly, that it still supports answering NONE.

A policy that cannot produce NONE is an architecture-astronaut generator.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

OWNER = "ops/state/truth/engineering-leverage.json"


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    )


# Each scenario is classified BY THE POLICY DATA, not by assertion of field
# existence. `story` is matched against `patterns[].signal`; `discriminator` is a
# phrase that must appear in the expected disposition's own `test`. Both
# participate, so mutating a signal or a disposition test breaks the derivation.
SCENARIOS = [
    {
        "id": "A_single_typo",
        "story": "one typo in one error string in one place",
        "pattern": None,
        "disposition": "NONE",
        "discriminator": "second real occurrence",
    },
    {
        "id": "B_two_parsers_drifted",
        "story": (
            "the same authority fact is parsed independently in two production "
            "locations and they have drifted"
        ),
        "pattern": "duplicated-semantic-owner",
        "disposition": "NOW",
        "discriminator": "partly repaired",
    },
    {
        "id": "C_one_writer_forgot_the_lock",
        "story": (
            "correctness depends on every caller remembering to acquire a lock "
            "before it mutates"
        ),
        "pattern": "remember-to-do-it-correctness",
        "disposition": "FOLLOW_UP",
        "discriminator": "complete and provable on its own",
    },
    {
        "id": "D_frozen_pr_redesign_suggestion",
        "story": (
            "a reviewer proposes changing the storage model and the subsystem "
            "boundary on a frozen pull request"
        ),
        "pattern": None,
        "disposition": "ARCHITECTURAL",
        "discriminator": "smuggle",
    },
    {
        "id": "E_harness_selected_zero_tests",
        "story": (
            "a gate reports success because the mechanism failed to observe the "
            "thing it was supposed to check"
        ),
        "pattern": "observational-falsehood",
        "disposition": "NOW",
        "discriminator": "partly repaired",
    },
]

_STOP = {
    "the", "a", "an", "and", "or", "it", "its", "of", "to", "in", "on", "is",
    "are", "was", "were", "be", "been", "that", "this", "they", "them", "their",
    "for", "by", "with", "as", "at", "from", "has", "have", "had", "not", "no",
    "one", "two", "more", "than", "each", "every", "any", "some", "which",
}


def _tokens(text: str) -> set[str]:
    words = re.findall(r"[a-z]+", text.lower())
    out = set()
    for w in words:
        if w in _STOP or len(w) < 4:
            continue
        # crude stem so "parsed"/"parses"/"parse" and "remembering"/"remember"
        # meet; enough to make signal text load-bearing without pulling in a
        # stemming dependency for five fixtures.
        for suf in ("ing", "ed", "es", "s"):
            if w.endswith(suf) and len(w) - len(suf) >= 4:
                w = w[: -len(suf)]
                break
        out.add(w)
    return out


def classify_pattern(story: str, patterns: list) -> tuple:
    """Return (best_id, score, unique) derived from the catalogue's own signals."""
    st = _tokens(story)
    scored = sorted(
        ((len(st & _tokens(p["signal"])), p["id"]) for p in patterns), reverse=True
    )
    best_score, best_id = scored[0]
    unique = len(scored) == 1 or scored[0][0] > scored[1][0]
    return best_id, best_score, unique


class T:
    def __init__(self) -> None:
        self.n = 0
        self.errors: list[str] = []

    def ok(self, cond: bool, msg: str) -> None:
        self.n += 1
        if not cond:
            self.errors.append(msg)


def main() -> int:
    root = repo_root()
    doc = json.loads((root / OWNER).read_text())
    patterns = doc["patterns"]
    dispositions = doc["dispositions"]
    t = T()

    for sc in SCENARIOS:
        sid = sc["id"]

        # --- the disposition must be derivable from its own documented test ---
        spec = dispositions.get(sc["disposition"])
        t.ok(spec is not None, f"{sid}: disposition {sc['disposition']} is not defined")
        if spec:
            probe = f"{spec.get('meaning','')} {spec.get('test','')} {spec.get('constraint','')} {spec.get('note','')}".lower()
            t.ok(
                sc["discriminator"].lower() in probe,
                f"{sid}: {sc['disposition']} no longer carries the discriminator "
                f"{sc['discriminator']!r} that makes this scenario classifiable — the policy "
                "changed and this scenario can no longer be decided from it",
            )

        # --- the pattern must be derivable from the catalogue's own signals ---
        best, score, unique = classify_pattern(sc["story"], patterns)
        if sc["pattern"] is None:
            # A scenario with no class must not match strongly. This is what
            # keeps NONE reachable rather than nominal.
            t.ok(
                score <= 2,
                f"{sid}: expected no clear pattern but {best!r} matched with score {score}; "
                "a catalogue that matches everything cannot produce NONE",
            )
            continue
        t.ok(
            best == sc["pattern"],
            f"{sid}: story classifies as {best!r} (score {score}), expected {sc['pattern']!r} — "
            "the signal text no longer discriminates this scenario",
        )
        t.ok(unique, f"{sid}: classification tied between patterns; signals do not discriminate")

    # NONE and ARCHITECTURAL must remain reachable and constrained.
    t.ok("delivery.json" in json.dumps(doc.get("owner_boundary", {})),
         "the policy must defer to delivery.json explicitly")
    t.ok(bool(doc.get("anti_patterns")),
         "anti_patterns must name this framework's own failure modes")

    if t.errors:
        print("test_engineering_leverage: FAIL")
        for e in t.errors:
            print(f"  - {e}")
        return 1
    print(
        f"test_engineering_leverage: clean ({t.n} assertions; "
        f"{len(SCENARIOS)} scenarios classified from the catalogue)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
