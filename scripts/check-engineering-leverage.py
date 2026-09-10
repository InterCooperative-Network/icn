#!/usr/bin/env python3
"""Drift gate for the engineering-leverage control plane.

Proves that the canonical owner of ICN's systemic-leverage behaviour parses, is
registered in the truth spine, is projected to exactly the surfaces the registry
declares, and has not quietly grown a second copy of the delivery lifecycle.

Run from anywhere; resolves the repository root itself.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

OWNER = "ops/state/truth/engineering-leverage.json"
SKILL = "systemic-leverage"
DISPOSITIONS = ["NOW", "FOLLOW_UP", "ARCHITECTURAL", "NONE"]
# Distinguishing these is the whole point; a catalogue that drops one is not the policy.
REQUIRED_STAGES = [
    "symptom",
    "reproduction",
    "immediate_cause",
    "enabling_mechanism",
    "local_remediation",
    "regression_witness",
    "recurring_class",
    "structural_options",
    "disposition",
]
# Vocabulary that belongs to delivery.json. If it appears as a STATE here, this
# file has started to become a second lifecycle owner.
DELIVERY_STATES = {
    "IMPLEMENTING", "REVIEWING", "FIXING", "VERIFYING", "FROZEN", "MERGING", "DONE",
}


class Check:
    def __init__(self) -> None:
        self.n = 0
        self.errors: list[str] = []

    def ok(self, cond: bool, msg: str) -> bool:
        self.n += 1
        if not cond:
            self.errors.append(msg)
        return cond

    def fail(self, msg: str) -> None:
        self.n += 1
        self.errors.append(msg)


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    )


def main() -> int:
    root = repo_root()
    c = Check()

    # ---- 1. the canonical owner exists and parses -------------------------
    owner_path = root / OWNER
    if not owner_path.is_file():
        print(f"check-engineering-leverage: FAIL\n  - canonical owner missing: {OWNER}")
        return 1
    try:
        doc = json.loads(owner_path.read_text())
    except json.JSONDecodeError as exc:
        print(f"check-engineering-leverage: FAIL\n  - {OWNER} does not parse: {exc}")
        return 1
    c.ok(True, "")

    # ---- 2. the loop keeps its distinct stages ----------------------------
    stages = [s.get("id") for s in doc.get("loop", {}).get("stages", [])]
    for want in REQUIRED_STAGES:
        c.ok(want in stages, f"loop.stages is missing '{want}' — the loop's stages are the policy")
    c.ok(
        stages.index("immediate_cause") < stages.index("enabling_mechanism")
        if "immediate_cause" in stages and "enabling_mechanism" in stages else False,
        "enabling_mechanism must come after immediate_cause; the ordering is the reasoning",
    )
    c.ok(
        bool(doc.get("loop", {}).get("claim_separation", {}).get("rule")),
        "loop.claim_separation.rule missing — 'instance fixed' vs 'class prevented' must stay distinct",
    )

    # ---- 3. dispositions are exactly the closed set -----------------------
    got = list(doc.get("dispositions", {}).keys())
    c.ok(got == DISPOSITIONS, f"dispositions must be exactly {DISPOSITIONS}, got {got}")
    for d in DISPOSITIONS:
        spec = doc.get("dispositions", {}).get(d, {})
        c.ok(bool(spec.get("meaning")), f"disposition {d} has no meaning")
        c.ok(bool(spec.get("test")), f"disposition {d} has no test — it cannot be applied consistently")
    none = doc.get("dispositions", {}).get("NONE", {})
    c.ok(
        "first-class" in json.dumps(none).lower() or "expected" in json.dumps(none).lower(),
        "NONE must be stated as a legitimate answer, or agents will manufacture architecture work",
    )

    # ---- 4. every pattern is evidence-backed ------------------------------
    patterns = doc.get("patterns", [])
    c.ok(len(patterns) >= 5, f"pattern catalogue is too thin to be useful: {len(patterns)}")
    seen: set[str] = set()
    ref = re.compile(r"(#\d+|/|\.md|\.json)")
    for p in patterns:
        pid = p.get("id", "<unnamed>")
        c.ok(pid not in seen, f"duplicate pattern id: {pid}")
        seen.add(pid)
        for field in ("signal", "question", "preferred_response"):
            c.ok(bool(p.get(field)), f"pattern {pid} missing {field}")
        ex = p.get("examples", [])
        c.ok(bool(ex), f"pattern {pid} has no examples — speculative patterns are not allowed here")
        for e in ex:
            c.ok(
                bool(ref.search(str(e.get("reference", "")))),
                f"pattern {pid} example has no resolvable reference (issue/PR/path): {e.get('reference')!r}",
            )
            c.ok(bool(e.get("fact")), f"pattern {pid} example has no fact")

    # ---- 5. delivery.json stays the lifecycle owner -----------------------
    ob = doc.get("owner_boundary", {})
    c.ok(bool(ob.get("conflict_rule")), "owner_boundary.conflict_rule missing")
    c.ok(
        "delivery.json" in json.dumps(ob),
        "owner_boundary must name ops/state/truth/delivery.json as the lifecycle owner",
    )
    c.ok(
        "delivery.json wins" in ob.get("conflict_rule", ""),
        "conflict_rule must state that delivery.json wins",
    )
    # This file must not redefine lifecycle states.
    top_level_states = set(doc.get("lifecycle", {}).get("states", []) or [])
    c.ok(
        not (top_level_states & DELIVERY_STATES),
        f"this file declares delivery lifecycle states {sorted(top_level_states & DELIVERY_STATES)} "
        "— that is a second lifecycle owner",
    )
    c.ok(
        bool(doc.get("anti_patterns")),
        "anti_patterns missing — the framework must name its own failure modes",
    )

    # ---- 6. registered in the truth spine --------------------------------
    spine = json.loads((root / "ops/state/truth/sources.json").read_text())
    dom = spine.get("domains", {}).get("engineering_leverage")
    if c.ok(bool(dom), "engineering_leverage is not a registered truth domain in sources.json"):
        c.ok(dom.get("owner") == OWNER, f"sources.json points engineering_leverage at {dom.get('owner')!r}")
        c.ok(
            dom.get("checker") == "scripts/check-engineering-leverage.py",
            "sources.json must name this checker so the domain is mechanically enforced",
        )
        c.ok(
            "delivery.json" in dom.get("not_owned_here", ""),
            "sources.json entry must disclaim lifecycle ownership",
        )

    # ---- 7. the skill projection exists and mirrors exactly ---------------
    reg = json.loads((root / "ops/state/truth/skills.json").read_text())
    entry = next(
        (s for s in reg["skills"]["icn_level"] if s.get("name") == SKILL), None
    )
    if c.ok(bool(entry), f"skill {SKILL!r} is not registered in skills.json"):
        canon = root / entry["canonical_path"]
        c.ok(canon.is_file(), f"canonical skill missing: {entry['canonical_path']}")
        if canon.is_file():
            body = canon.read_text()
            # The projection must point at the owner, not restate it.
            c.ok(OWNER in body, f"{entry['canonical_path']} must cite {OWNER} as its canonical source")
            c.ok(
                "delivery.json" in body,
                "the skill must state the delivery boundary it may not cross",
            )
            for d in DISPOSITIONS:
                c.ok(d in body, f"the skill must name disposition {d}")
        for m in entry.get("provider_mirrors", []):
            mp = root / m["path"]
            c.ok(mp.is_file(), f"provider mirror missing: {m['path']}")
            if mp.is_file() and canon.is_file() and m.get("policy") == "exact_mirror":
                c.ok(
                    mp.read_bytes() == canon.read_bytes(),
                    f"provider mirror drifted from canonical: {m['path']}",
                )

    # ---- 8. control-plane consumers are wired ----------------------------
    # A policy nobody is routed to is prose. These surfaces must reference the
    # skill or the owner by name.
    consumers = {
        "AGENTS.md": root / "AGENTS.md",
        "docs/ai/ICN_CONSTITUTIONAL_CORE.md": root / "docs/ai/ICN_CONSTITUTIONAL_CORE.md",
    }
    for label, path in consumers.items():
        if c.ok(path.is_file(), f"expected consumer missing: {label}"):
            text = path.read_text()
            c.ok(
                SKILL in text or OWNER in text,
                f"{label} does not reference the systemic-leverage policy — agents will not encounter it",
            )

    # ---- 9. no second canonical owner ------------------------------------
    # Naming the vocabulary is fine and expected — sources.json must describe
    # the domain it registers. What must not exist is a second file that
    # DECLARES the vocabulary structurally, i.e. carries its own dispositions
    # or loop-stage definitions. That is the difference between a pointer and
    # an owner, and only the structural form can drift into a rival authority.
    for other in (root / "ops/state/truth").glob("*.json"):
        if other.name == Path(OWNER).name:
            continue
        try:
            od = json.loads(other.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        if not isinstance(od, dict):
            continue
        rival = od.get("dispositions")
        if isinstance(rival, dict) and set(rival) >= set(DISPOSITIONS):
            c.fail(
                f"{other.relative_to(root)} declares its own 'dispositions' object covering "
                f"{DISPOSITIONS} — there must be exactly one canonical owner"
            )
        rival_loop = od.get("loop", {})
        if isinstance(rival_loop, dict) and rival_loop.get("stages"):
            c.fail(
                f"{other.relative_to(root)} declares its own 'loop.stages' — "
                "the systemic-leverage loop has one owner"
            )

    if c.errors:
        print("check-engineering-leverage: FAIL")
        for e in c.errors:
            print(f"  - {e}")
        return 1
    print(f"check-engineering-leverage: clean ({c.n} assertions passed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
