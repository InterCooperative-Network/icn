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


# Each scenario carries structured FACTS. The classifier evaluates the policy's
# own per-disposition predicates against those facts and derives a disposition.
#
# The expected label is NOT an input to the classifier — it is compared only
# afterwards. That is the difference between proving the policy classifies the
# story and checking that familiar words are still present somewhere.
SCENARIOS = [
    {
        "id": "A_single_typo",
        "story": "one typo in one error string in one place",
        "facts": {
            "recurring_class_established": False,
            "changes_subsystem_boundary": False,
            "structural_change_required_or_in_contract": False,
        },
        "pattern": None,
        "expect": "NONE",
    },
    {
        "id": "B_two_parsers_drifted",
        "story": (
            "the same authority fact is parsed independently in two production "
            "locations and they have drifted"
        ),
        "facts": {
            # Factoring the already-existing canonical parser IS the smallest
            # correct repair, so the structural correction is not optional.
            "recurring_class_established": True,
            "changes_subsystem_boundary": False,
            "structural_change_required_or_in_contract": True,
        },
        "pattern": "duplicated-semantic-owner",
        "expect": "NOW",
    },
    {
        "id": "C_one_writer_forgot_the_lock",
        "story": (
            "correctness depends on every caller remembering to acquire a lock "
            "before it mutates"
        ),
        "facts": {
            # Joining the lock domain is complete and provable on its own; the
            # guarded-mutation API is separate work.
            "recurring_class_established": True,
            "changes_subsystem_boundary": False,
            "structural_change_required_or_in_contract": False,
        },
        "pattern": "remember-to-do-it-correctness",
        "expect": "FOLLOW_UP",
    },
    {
        "id": "D_frozen_pr_redesign_suggestion",
        "story": (
            "a reviewer proposes changing the storage model and the subsystem "
            "boundary on a frozen pull request"
        ),
        "facts": {
            "recurring_class_established": True,
            "changes_subsystem_boundary": True,
            "structural_change_required_or_in_contract": False,
        },
        "pattern": None,
        "expect": "ARCHITECTURAL",
    },
    {
        "id": "E_harness_selected_zero_tests",
        "story": (
            "a gate reports success because the mechanism failed to observe the "
            "thing it was supposed to check"
        ),
        "facts": {
            "recurring_class_established": True,
            "changes_subsystem_boundary": False,
            "structural_change_required_or_in_contract": True,
        },
        "pattern": "observational-falsehood",
        "expect": "NOW",
    },
]

_STOP = {
    "the", "a", "an", "and", "or", "it", "its", "of", "to", "in", "on", "is",
    "are", "was", "were", "be", "been", "that", "this", "they", "them", "their",
    "for", "by", "with", "as", "at", "from", "has", "have", "had", "not", "no",
    "one", "two", "more", "than", "each", "every", "any", "some", "which",
}


class Ambiguous(Exception):
    """Zero or several dispositions matched. Never resolved by preference."""


def classify(facts: dict, dispositions: dict) -> str:
    """Derive a disposition from FACTS alone.

    Takes no expected value and no scenario identity: there is nothing here it
    could bias toward. Ambiguity raises rather than choosing.
    """
    matched = []
    for name, spec in dispositions.items():
        pred = (spec.get("predicate") or {}).get("all_of")
        if not pred:
            raise Ambiguous(f"disposition {name} carries no predicate to evaluate")
        if all(facts.get(k) == v for k, v in pred.items()):
            matched.append(name)
    if len(matched) != 1:
        raise Ambiguous(
            f"{len(matched)} dispositions matched {sorted(matched)} — a classification "
            "must be uniquely justified, so this fails closed"
        )
    return matched[0]


def _tokens(text: str) -> set:
    words = re.findall(r"[a-z]+", text.lower())
    out = set()
    for w in words:
        if w in _STOP or len(w) < 4:
            continue
        for suf in ("ing", "ed", "es", "s"):
            if w.endswith(suf) and len(w) - len(suf) >= 4:
                w = w[: -len(suf)]
                break
        out.add(w)
    return out


def classify_pattern(story: str, patterns: list) -> tuple:
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
        self.errors: list = []

    def ok(self, cond: bool, msg: str) -> None:
        self.n += 1
        if not cond:
            self.errors.append(msg)


def main() -> int:
    root = repo_root()
    doc = json.loads((root / OWNER).read_text())
    patterns = doc["patterns"]
    dispositions = doc["dispositions"]
    declared_facts = set(doc.get("classification", {}).get("facts", {}))
    t = T()

    t.ok(bool(declared_facts), "classification.facts is missing; nothing defines the fact space")

    # ---- scenarios are DERIVED, never asserted -------------------------------
    for sc in SCENARIOS:
        sid = sc["id"]
        t.ok(
            set(sc["facts"]) == declared_facts,
            f"{sid}: scenario facts {sorted(sc['facts'])} do not match the declared fact space "
            f"{sorted(declared_facts)} — a scenario cannot assert facts the policy does not define",
        )
        try:
            got = classify(sc["facts"], dispositions)
        except Ambiguous as exc:
            t.ok(False, f"{sid}: classification failed closed: {exc}")
            continue
        t.ok(
            got == sc["expect"],
            f"{sid}: policy derives {got!r} from these facts, scenario expects "
            f"{sc['expect']!r} — the predicates no longer classify this story as specified",
        )

        best, score, unique = classify_pattern(sc["story"], patterns)
        if sc["pattern"] is None:
            t.ok(score <= 2, f"{sid}: expected no clear pattern but {best!r} matched (score {score})")
            continue
        t.ok(best == sc["pattern"],
             f"{sid}: story classifies as {best!r}, expected {sc['pattern']!r}")
        t.ok(unique, f"{sid}: classification tied; signals do not discriminate")

    # ---- adversarial witnesses ----------------------------------------------
    # (1) Magic wording retained, predicate meaning changed -> must NOT pass.
    import copy
    drifted = copy.deepcopy(dispositions)
    drifted["NOW"]["meaning"] = "unrelated text that still says partly repaired"
    drifted["NOW"]["test"] = "unrelated text that still says partly repaired"
    drifted["NOW"]["predicate"]["all_of"]["structural_change_required_or_in_contract"] = False
    b = next(s for s in SCENARIOS if s["id"] == "B_two_parsers_drifted")
    try:
        got = classify(b["facts"], drifted)
        t.ok(
            got != b["expect"],
            "keyword-preservation witness: NOW kept the phrase 'partly repaired' while its "
            "predicate changed, and the suite still derived NOW — the classifier is reading "
            "prose, not predicates",
        )
    except Ambiguous:
        t.ok(True, "")  # failing closed is also a correct detection

    # (2) Changing scenario facts changes the derived disposition.
    flipped = dict(b["facts"], structural_change_required_or_in_contract=False)
    try:
        moved = classify(flipped, dispositions)
    except Ambiguous as exc:
        moved = f"<ambiguous: {exc}>"
    t.ok(
        moved == "FOLLOW_UP",
        "fact-sensitivity witness: flipping structural_change_required_or_in_contract on "
        f"scenario B derived {moved!r} rather than FOLLOW_UP, so the facts are inert",
    )

    # (3) Ambiguity fails closed rather than preferring an expected answer.
    collided = copy.deepcopy(dispositions)
    collided["FOLLOW_UP"]["predicate"]["all_of"] = dict(
        collided["NOW"]["predicate"]["all_of"]
    )
    try:
        classify(b["facts"], collided)
        t.ok(False, "ambiguity witness: two identical predicates did not fail closed")
    except Ambiguous:
        t.ok(True, "")
    unmatchable = {k: None for k in declared_facts}
    try:
        classify(unmatchable, dispositions)
        t.ok(False, "ambiguity witness: unmatchable facts did not fail closed")
    except Ambiguous:
        t.ok(True, "")

    # (4) The expected label is not reachable by the classifier.
    import inspect
    sig = set(inspect.signature(classify).parameters)
    t.ok(
        sig == {"facts", "dispositions"},
        f"classify() takes {sorted(sig)}; it must see only facts and the policy, never the "
        "expected label or the scenario identity",
    )

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
        f"test_engineering_leverage: clean ({t.n} assertions; {len(SCENARIOS)} scenarios "
        "derived from policy predicates; 5 adversarial witnesses)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
