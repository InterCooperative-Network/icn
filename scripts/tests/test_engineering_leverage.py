#!/usr/bin/env python3
"""Behaviour fixtures for the systemic-leverage policy.

These do not test prose. They test that the canonical owner actually carries the
discriminating information an agent needs to reach the intended classification —
and, importantly, that it still supports answering NONE.

A policy that cannot produce NONE is an architecture-astronaut generator.
"""
from __future__ import annotations

import json
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


# Each scenario names the discriminator the policy must supply for an agent to
# classify it correctly. `pattern` is the catalogue entry that should match, or
# None when the correct answer is that no class exists.
SCENARIOS = [
    {
        "id": "A_single_typo",
        "story": "One typo in one error string, in one place.",
        "pattern": None,
        "disposition": "NONE",
        "discriminator": "NONE must be documented as legitimate, or an agent will invent a class",
    },
    {
        "id": "B_two_parsers_drifted",
        "story": (
            "Two production paths separately parse the same authority field and "
            "have already drifted."
        ),
        "pattern": "duplicated-semantic-owner",
        "disposition": "NOW",
        "discriminator": "NOW's test must key on the repair being incomplete without it",
    },
    {
        "id": "C_one_writer_forgot_the_lock",
        "story": (
            "One writer forgot to acquire the lock; five other writers each "
            "acquire it manually."
        ),
        "pattern": "remember-to-do-it-correctness",
        "disposition": "FOLLOW_UP",
        "discriminator": "FOLLOW_UP must key on the local repair being complete on its own",
    },
    {
        "id": "D_frozen_pr_redesign_suggestion",
        "story": (
            "A frozen PR receives a reviewer suggestion to redesign storage, with "
            "no current-contract violation."
        ),
        "pattern": None,
        "disposition": "ARCHITECTURAL",
        "discriminator": "the policy must forbid a systemic observation from reopening a frozen PR",
    },
    {
        "id": "E_harness_selected_zero_tests",
        "story": "A test harness selects zero tests and exits 0.",
        "pattern": "observational-falsehood",
        "disposition": "NOW",
        "discriminator": "failure-to-know must not be representable as knowing success",
    },
]


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
    patterns = {p["id"]: p for p in doc["patterns"]}
    dispositions = doc["dispositions"]
    t = T()

    for sc in SCENARIOS:
        sid = sc["id"]

        # The disposition the scenario expects must exist and be applicable.
        d = dispositions.get(sc["disposition"])
        t.ok(d is not None, f"{sid}: disposition {sc['disposition']} is not defined")
        if d:
            t.ok(
                bool(d.get("test")),
                f"{sid}: disposition {sc['disposition']} has no applicability test, so an "
                "agent cannot reach it deterministically",
            )

        # The pattern the scenario should match must exist, carry a signal an
        # agent can match against, and a preferred response.
        if sc["pattern"] is None:
            continue
        p = patterns.get(sc["pattern"])
        t.ok(p is not None, f"{sid}: expected pattern {sc['pattern']!r} is not in the catalogue")
        if p:
            t.ok(bool(p.get("signal")), f"{sid}: pattern {sc['pattern']} has no signal to match on")
            t.ok(
                bool(p.get("preferred_response")),
                f"{sid}: pattern {sc['pattern']} gives no direction, so the agent will invent one",
            )
            t.ok(
                bool(p.get("examples")),
                f"{sid}: pattern {sc['pattern']} has no precedent, so 'we have seen this before' "
                "cannot be established from the repository",
            )

    # Scenario A and D specifically: the policy must make restraint reachable.
    none = json.dumps(dispositions.get("NONE", {})).lower()
    t.ok(
        "second real occurrence" in none or "name a second" in none,
        "NONE needs a concrete test an agent can apply, not just permission to use it",
    )
    anti = json.dumps(doc.get("anti_patterns", [])).lower()
    t.ok(
        "manufactur" in anti or "every systemic observation" in anti,
        "anti_patterns must name architecture-astronaut and issue-spam failure modes",
    )

    # Scenario D specifically: freeze and scope must be protected in the policy
    # itself, not only in delivery.json, because the agent reads this file.
    boundary = json.dumps(doc.get("owner_boundary", {})) + json.dumps(doc.get("dispositions", {}))
    t.ok("delivery.json" in boundary, "the policy must defer to delivery.json explicitly")
    arch = dispositions.get("ARCHITECTURAL", {})
    t.ok(
        "smuggle" in json.dumps(arch).lower() or "current pull request" in json.dumps(arch).lower(),
        "ARCHITECTURAL must forbid folding the work into the current pull request",
    )

    if t.errors:
        print("test_engineering_leverage: FAIL")
        for e in t.errors:
            print(f"  - {e}")
        return 1
    print(
        f"test_engineering_leverage: clean ({t.n} assertions over "
        f"{len(SCENARIOS)} classification scenarios)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
