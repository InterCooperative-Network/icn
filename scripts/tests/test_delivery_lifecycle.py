#!/usr/bin/env python3
"""test_delivery_lifecycle.py — the delivery lifecycle gate rejects its own violations (icn#2661).

`ops/state/truth/delivery.json` states when comprehensive review ends and what freezes. A policy
that merely SAYS a thing is not enforcement — the question this file answers is whether a document
that says the opposite is rejected.

Every process invariant below is therefore tested the same way: take the real committed policy,
mutate it into the failure it is supposed to prevent, and assert the validator complains. A test
that only read the committed values would pass just as happily against a policy nobody enforces.

Nothing here parses English. Assertions are against structured values, registry data, and the
exit status of the real gate — a prose check has grammar to get wrong, and this repository has
already shipped one that did (icn#2651).
"""

import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import importlib.util

_spec = importlib.util.spec_from_file_location(
    "check_delivery_lifecycle", ROOT / "scripts" / "check-delivery-lifecycle.py")
gate = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gate)

DELIVERY = json.loads((ROOT / "ops" / "state" / "truth" / "delivery.json")
                      .read_text(encoding="utf-8"))
SKILLS = json.loads((ROOT / "ops" / "state" / "truth" / "skills.json").read_text(encoding="utf-8"))

failures: list[str] = []


def check(label: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {label}")
    else:
        print(f"  FAIL {label}  ({detail})")
        failures.append(label)


def mutated(**path_value) -> dict:
    """A deep copy of the real policy with `a__b__c=value` paths replaced (None deletes)."""
    doc = json.loads(json.dumps(DELIVERY))
    for dotted, value in path_value.items():
        parts = dotted.split("__")
        node = doc
        for part in parts[:-1]:
            node = node[int(part)] if part.isdigit() else node[part]
        last = parts[-1]
        key = int(last) if last.isdigit() else last
        if value is None:
            del node[key]
        else:
            node[key] = value
    return doc


def rejects(label: str, doc: dict, expect_substring: str) -> None:
    problems = gate.validate(doc)
    hit = [p for p in problems if expect_substring in p]
    check(label, bool(hit), f"no complaint containing {expect_substring!r}; got {problems[:2]}")


# ---------------------------------------------------------------------------------------------
print("the committed policy is valid, and the validator is total")

check("the committed delivery policy validates clean", gate.validate(DELIVERY) == [],
      str(gate.validate(DELIVERY)[:3]))

for hostile in (None, 7, "policy", [], [{"schema": "x"}], {"schema": 1},
                {"lifecycle": []}, {"lifecycle": {"states": "IMPLEMENTING"}},
                {"blocker_predicate": {"all_must_hold": [None, 1, "reproducible"]}},
                {"lanes": {"definitions": [{"name": "FAST", "comprehensive_generations": True}]}},
                {"provider_bindings": {"surfaces": [{"path": 1, "must_not_match": [{}]}]}}):
    try:
        out = gate.validate(hostile)
        ok = isinstance(out, list) and out != []
        detail = "returned no complaints"
    except Exception as exc:                                   # noqa: BLE001 — that IS the defect
        ok, detail = False, f"raised {type(exc).__name__}: {exc}"
    check(f"malformed input {str(hostile)[:34]!r} is reported, not raised", ok, detail)


# ---------------------------------------------------------------------------------------------
print("a push does not reset the comprehensive review generation")

check("the committed policy says a push does not reset the generation",
      DELIVERY["review_generation"]["push_resets_generation"] is False,
      str(DELIVERY["review_generation"]["push_resets_generation"]))
rejects("a policy that lets a push reset the generation is rejected",
        mutated(review_generation__push_resets_generation=True), "push_resets_generation")
rejects("a truthy stand-in for the reset switch is rejected too",
        mutated(review_generation__push_resets_generation=1), "push_resets_generation")
rejects("deleting the switch is rejected",
        mutated(review_generation__push_resets_generation=None), "push_resets_generation")
rejects("a policy with no bounded-review claim is rejected",
        mutated(review_generation__comprehensive_review_is_bounded=False),
        "comprehensive_review_is_bounded")


# ---------------------------------------------------------------------------------------------
print("DELTA does not become FULL review")

kinds = DELIVERY["review_generation"]["review_kinds"]
check("the policy defines exactly FULL and DELTA", sorted(kinds) == ["DELTA", "FULL"],
      str(sorted(kinds)))
check("after a blocker fix the next review is DELTA",
      DELIVERY["review_generation"]["after_blocker_fix"] == "DELTA",
      DELIVERY["review_generation"]["after_blocker_fix"])
rejects("a policy that re-runs FULL review after a blocker fix is rejected",
        mutated(review_generation__after_blocker_fix="FULL"), "after_blocker_fix")
rejects("a DELTA definition with no stated limit is rejected",
        mutated(review_generation__review_kinds__DELTA__may_not=None), "DELTA.may_not")
rejects("inventing a third review kind is rejected",
        mutated(review_generation__review_kinds__PARTIAL={"may_inspect": "x", "when": "y"}),
        "review_kinds")
rejects("a policy where reviews need not declare their kind is rejected",
        mutated(review_generation__every_review_declares_its_kind=False),
        "every_review_declares_its_kind")


# ---------------------------------------------------------------------------------------------
print("a frozen pull request stays frozen below the blocker threshold")

conditions = [c["id"] for c in DELIVERY["blocker_predicate"]["all_must_hold"]]
check("the predicate names every condition the code requires",
      sorted(conditions) == sorted(gate.BLOCKER_CONDITIONS), str(conditions))
for dropped in gate.BLOCKER_CONDITIONS:
    kept = [c for c in DELIVERY["blocker_predicate"]["all_must_hold"] if c["id"] != dropped]
    rejects(f"dropping the {dropped!r} condition is rejected",
            mutated(blocker_predicate__all_must_hold=kept), dropped)
rejects("a freeze with no late-finding rule is rejected",
        mutated(freeze__late_finding_rule=None), "late_finding_rule")
rejects("a freeze that names no head is rejected",
        mutated(freeze__names_an_exact_head=False), "names_an_exact_head")
rejects("dropping the no-sibling-sweep rule is rejected",
        mutated(blocker_predicate__no_sibling_sweep=None), "no_sibling_sweep")
rejects("dropping the redesign rule is rejected",
        mutated(blocker_predicate__redesign_rule=None), "redesign_rule")


# ---------------------------------------------------------------------------------------------
print("a qualifying late blocker can reopen, be fixed, and refreeze")

states = {s["name"]: s for s in DELIVERY["lifecycle"]["states"]}
check("FROZEN has a way back to FIXING", "FIXING" in states["FROZEN"]["exits_to"],
      str(states["FROZEN"]["exits_to"]))
check("that way back is conditional", bool(states["FROZEN"].get("exit_to_fixing_requires")),
      "FROZEN exits to FIXING unconditionally")
check("the refreeze path is stated", bool(DELIVERY["freeze"].get("refreeze")), "no refreeze rule")

frozen_unconditional = json.loads(json.dumps(DELIVERY))
for s in frozen_unconditional["lifecycle"]["states"]:
    if s["name"] == "FROZEN":
        del s["exit_to_fixing_requires"]
rejects("an unconditional exit from FROZEN is rejected", frozen_unconditional,
        "exit_to_fixing_requires")

# A lifecycle has to be able to finish, and every state has to be usable.
dead_end = json.loads(json.dumps(DELIVERY))
for s in dead_end["lifecycle"]["states"]:
    if s["name"] == "MERGING":
        s["exits_to"] = ["FROZEN"]
rejects("a lifecycle with no path to DONE is rejected", dead_end, "not reachable")

orphan = json.loads(json.dumps(DELIVERY))
for s in orphan["lifecycle"]["states"]:
    if s["name"] == "REVIEWING":
        s["exits_to"] = []
rejects("a lifecycle with an unreachable state is rejected", orphan, "unreachable")

talkative_terminal = json.loads(json.dumps(DELIVERY))
for s in talkative_terminal["lifecycle"]["states"]:
    if s["name"] == "DONE":
        s["exits_to"] = ["REVIEWING"]
rejects("a non-terminal DONE is rejected", talkative_terminal, "must be terminal")


# ---------------------------------------------------------------------------------------------
print("automated severity alone cannot break a freeze")

check("severity is declared advisory",
      DELIVERY["blocker_predicate"]["automated_severity_is_advisory"] is True,
      str(DELIVERY["blocker_predicate"]["automated_severity_is_advisory"]))
labels = DELIVERY["blocker_predicate"]["advisory_severity_labels"]
check("the labels that carry no authority are enumerated",
      all(x in labels for x in ("P1", "P2", "critical")), str(labels))
rejects("a policy that makes severity authoritative is rejected",
        mutated(blocker_predicate__automated_severity_is_advisory=False),
        "automated_severity_is_advisory")
rejects("dropping the advisory-label list is rejected",
        mutated(blocker_predicate__advisory_severity_labels=[]), "advisory_severity_labels")
check("automated reviewers are explicitly denied freeze-breaking authority",
      any("freeze" in x for x in DELIVERY["authority"]["automated_reviewers_may_not"]),
      str(DELIVERY["authority"]["automated_reviewers_may_not"]))
rejects("a policy that denies automated reviewers nothing is rejected",
        mutated(authority__automated_reviewers_may_not=[]), "automated_reviewers_may_not")


# ---------------------------------------------------------------------------------------------
print("a deferred observation becomes durable work rather than being dropped")

dispositions = [d["name"] for d in DELIVERY["finding_dispositions"]]
check("all four dispositions are defined",
      sorted(dispositions) == sorted(gate.DISPOSITIONS), str(dispositions))
for dropped in gate.DISPOSITIONS:
    kept = [d for d in DELIVERY["finding_dispositions"] if d["name"] != dropped]
    rejects(f"dropping the {dropped} disposition is rejected",
            mutated(finding_dispositions=kept), dropped)
check("a follow-up entry must link back to its thread",
      DELIVERY["follow_up_ledger"]["provenance_link_required"] is True,
      str(DELIVERY["follow_up_ledger"]["provenance_link_required"]))
rejects("a ledger with no provenance requirement is rejected",
        mutated(follow_up_ledger__provenance_link_required=False), "provenance_link_required")
rejects("one ledger issue per comment is rejected",
        mutated(follow_up_ledger__one_issue_per_pull_request=False),
        "one_issue_per_pull_request")


# ---------------------------------------------------------------------------------------------
print("every lane is bounded, and DEEP is the maintainer's call")

lanes = {x["name"]: x for x in DELIVERY["lanes"]["definitions"]}
check("all three lanes are defined", sorted(lanes) == sorted(gate.LANES), str(sorted(lanes)))
for name, lane in lanes.items():
    check(f"the {name} lane schedules exactly one comprehensive generation",
          type(lane["comprehensive_generations"]) is int
          and lane["comprehensive_generations"] == 1,
          repr(lane["comprehensive_generations"]))
check("DEEP requires maintainer selection", lanes["DEEP"]["requires_maintainer_selection"] is True,
      str(lanes["DEEP"].get("requires_maintainer_selection")))

loose = json.loads(json.dumps(DELIVERY))
for lane in loose["lanes"]["definitions"]:
    if lane["name"] == "STANDARD":
        lane["comprehensive_generations"] = 3
rejects("a lane that schedules repeated comprehensive review is rejected", loose,
        "comprehensive_generations")

self_serve_deep = json.loads(json.dumps(DELIVERY))
for lane in self_serve_deep["lanes"]["definitions"]:
    if lane["name"] == "DEEP":
        lane["requires_maintainer_selection"] = False
rejects("a DEEP lane an agent may select itself is rejected", self_serve_deep,
        "requires_maintainer_selection")


# ---------------------------------------------------------------------------------------------
print("ship-pr may not duplicate or bypass merge authority")

check("the orchestrator is barred from duplicating the primitive",
      DELIVERY["authority"]["mutation"]["orchestrator_may_not_duplicate_primitive"] is True,
      "not declared")
rejects("a policy that lets the orchestrator duplicate the primitive is rejected",
        mutated(authority__mutation__orchestrator_may_not_duplicate_primitive=False),
        "orchestrator_may_not_duplicate_primitive")
rejects("a policy that claims merge semantics for itself is rejected",
        mutated(authority__mutation__semantics_owner="ops/state/truth/delivery.json"),
        "semantics_owner")
rejects("a policy naming a different merge executable is rejected",
        mutated(authority__mutation__executable="scripts/merge.sh"), "executable")
rejects("a mutation block holding a spelled command is rejected",
        mutated(authority__mutation__primitive="gh pr merge --squash"), "command- or flag-shaped")

boundary_owners = [e["owner"] for e in DELIVERY["owner_boundary"]["does_not_own"]]
check("the policy disclaims merge ownership explicitly",
      any(gate.MERGE_SEMANTICS_OWNER in o for o in boundary_owners), str(boundary_owners))
rejects("a policy that stops disclaiming merge ownership is rejected",
        mutated(owner_boundary__does_not_own=[{"fact": "x", "owner": "AGENTS.md"}]),
        "does_not_own")

ship = next(e for e in SKILLS["skills"]["icn_level"] if e["name"] == "ship-pr")
patterns = [r["pattern"] for r in ship["canonical_assertions"]["must_not_match"]]
skill_text = (ROOT / ship["canonical_path"]).read_text(encoding="utf-8")

# A forbidden pattern that matches nothing is decoration. Each one is proved live against the
# line it exists to catch, and proved silent against the skill as committed.
hostile_lines = {
    r"icn-merge-pr\s+merge": "   icn-merge-pr merge \"$PR\" --authorize",
    r"gh\s+pr\s+merge": "   gh pr merge \"$PR\" --squash",
    r"--admin\b": "   ... --admin when the maintainer says so",
    r"--auto\b|--disable-auto\b": "   arm it with --auto and come back later",
    r"branches/[^/\s]+/protection": "   gh api repos/o/r/branches/main/protection",
    r"\bBuild Release\b": "   wait for Build Release, Test and Clippy",
    r"\b(all\s+)?\d+\s+required\s+(CI\s+)?checks\b": "   all 11 required checks must be green",
}
check("every forbidden ship-pr pattern has a hostile line to prove it against",
      sorted(patterns) == sorted(hostile_lines), str(sorted(set(patterns) ^ set(hostile_lines))))
for pattern in patterns:
    rx = re.compile(pattern)
    check(f"the ship-pr pattern {pattern!r} actually matches a merge path",
          bool(rx.search(hostile_lines.get(pattern, ""))), "pattern is vacuous")
    hit = [i for i, line in enumerate(skill_text.splitlines(), 1) if rx.search(line)]
    check(f"the committed ship-pr skill does not match {pattern!r}", not hit, f"line {hit[:1]}")

for ref in ship["canonical_assertions"]["must_reference"]:
    check(f"the ship-pr skill references {ref!r}", ref in skill_text, "missing")

mirror = ship["provider_mirrors"][0]
check("the ship-pr provider copy is an exact mirror", mirror["policy"] == "exact_mirror",
      mirror["policy"])
check("the ship-pr provider copy is byte-identical",
      (ROOT / mirror["path"]).read_bytes() == (ROOT / ship["canonical_path"]).read_bytes(),
      "the two copies differ")


# ---------------------------------------------------------------------------------------------
print("provider reviewer adapters cannot contradict the canonical rules")

surfaces = DELIVERY["provider_bindings"]["surfaces"]
check("all three provider surfaces are bound", len(surfaces) == 3, str(len(surfaces)))
for s in surfaces:
    check(f"{s['path']} exists", (ROOT / s["path"]).is_file(), "missing")
rejects("a policy that binds no provider surface is rejected",
        mutated(provider_bindings__surfaces=[]), "surfaces")
rejects("a policy that declares no body mirror is rejected",
        mutated(provider_bindings__body_mirrors=[]), "body_mirrors")

for m in DELIVERY["provider_bindings"]["body_mirrors"]:
    check(f"{m['mirror']} body matches {m['canonical']}",
          gate._body(ROOT / m["mirror"]) == gate._body(ROOT / m["canonical"]),
          "the two reviewer prompts have drifted apart")


# ---------------------------------------------------------------------------------------------
print("the gate itself can still fail")


def gate_root(tmp: pathlib.Path, name: str) -> pathlib.Path:
    """A minimal repository the delivery gate runs against, copied from the real one."""
    root = tmp / name
    (root / "scripts").mkdir(parents=True)
    shutil.copy2(ROOT / "scripts" / "check-delivery-lifecycle.py", root / "scripts")
    (root / "ops" / "state" / "truth").mkdir(parents=True)
    for owner in ("delivery.json", "sources.json"):
        shutil.copy2(ROOT / "ops" / "state" / "truth" / owner, root / "ops" / "state" / "truth")
    for s in surfaces:
        target = root / s["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / s["path"], target)
    mut = DELIVERY["authority"]["mutation"]
    for field in ("orchestrator", "primitive", "executable"):
        named = root / mut[field]
        named.parent.mkdir(parents=True, exist_ok=True)
        if not named.exists():
            named.write_text("", encoding="utf-8") if named.suffix else named.mkdir()
    return root


def run_gate(root: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(root / "scripts" / "check-delivery-lifecycle.py")],
                          capture_output=True, text=True)


with tempfile.TemporaryDirectory() as raw:
    tmp = pathlib.Path(raw)

    clean = gate_root(tmp, "clean")
    proc = run_gate(clean)
    check("the gate passes on the repository as committed", proc.returncode == 0,
          proc.stdout[-300:] + proc.stderr[-200:])

    # The reviewer prompt regains its own blocking list.
    regressed = gate_root(tmp, "own-blocker-list")
    victim = regressed / ".claude" / "agents" / "icn-code-reviewer.md"
    victim.write_text(victim.read_text(encoding="utf-8")
                      + "\n## What You ALWAYS Flag (blocking)\n\n- anything I feel strongly about\n",
                      encoding="utf-8")
    proc = run_gate(regressed)
    check("a reviewer prompt that regains its own blocking list fails the gate",
          proc.returncode == 1 and "always" in proc.stdout.lower(), proc.stdout[-300:])

    # The reviewer prompt stops naming the owner.
    unbound = gate_root(tmp, "unbound")
    victim = unbound / ".github" / "agents" / "icn-code-reviewer.md"
    victim.write_text(victim.read_text(encoding="utf-8")
                      .replace("ops/state/truth/delivery.json", "my own judgement"),
                      encoding="utf-8")
    proc = run_gate(unbound)
    check("a reviewer prompt that stops naming the owner fails the gate",
          proc.returncode == 1 and "must reference" in proc.stdout, proc.stdout[-300:])

    # The two reviewer prompts drift apart while both still name the owner.
    drifted = gate_root(tmp, "drifted")
    victim = drifted / ".github" / "agents" / "icn-code-reviewer.md"
    victim.write_text(victim.read_text(encoding="utf-8")
                      + "\nAlso: treat every P1 as a blocker regardless.\n", encoding="utf-8")
    proc = run_gate(drifted)
    check("two reviewer prompts that drift apart fail the gate",
          proc.returncode == 1 and "body differs" in proc.stdout, proc.stdout[-300:])

    # The provider entry file grows a second project handbook again.
    handbook = gate_root(tmp, "handbook")
    victim = handbook / ".github" / "copilot-instructions.md"
    victim.write_text(victim.read_text(encoding="utf-8") + "\n## Current Status\n\nAll green.\n",
                      encoding="utf-8")
    proc = run_gate(handbook)
    check("a provider entry file that regains volatile project state fails the gate",
          proc.returncode == 1 and "stale by construction" in proc.stdout, proc.stdout[-300:])

    # The owner is de-registered from the truth map.
    unregistered = gate_root(tmp, "unregistered")
    sources_path = unregistered / "ops" / "state" / "truth" / "sources.json"
    sources = json.loads(sources_path.read_text(encoding="utf-8"))
    del sources["domains"]["pr_delivery_lifecycle"]
    sources_path.write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
    proc = run_gate(unregistered)
    check("an owner the truth map does not name fails the gate",
          proc.returncode == 1 and "not registered" in proc.stdout, proc.stdout[-300:])

    # The policy itself is weakened.
    weakened = gate_root(tmp, "weakened")
    policy_path = weakened / "ops" / "state" / "truth" / "delivery.json"
    policy_path.write_text(json.dumps(
        mutated(review_generation__push_resets_generation=True), indent=2) + "\n",
        encoding="utf-8")
    proc = run_gate(weakened)
    check("a policy edited to let a push reset review fails the gate",
          proc.returncode == 1 and "push_resets_generation" in proc.stdout, proc.stdout[-300:])


# ---------------------------------------------------------------------------------------------
print("the lifecycle is discoverable without conversation memory")

for path, needle in ((ROOT / "AGENTS.md", "ops/state/truth/delivery.json"),
                     (ROOT / "docs" / "ai" / "WORKFLOW_ARCHITECTURE.md",
                      "ops/state/truth/delivery.json"),
                     (ROOT / ".github" / "pull_request_template.md",
                      DELIVERY["lifecycle"]["state_surface"]["begin_marker"]),
                     (ROOT / ".agents" / "skills" / "pr-create" / "SKILL.md",
                      DELIVERY["lifecycle"]["state_surface"]["begin_marker"])):
    check(f"{path.relative_to(ROOT)} names {needle!r}",
          needle in path.read_text(encoding="utf-8"), "missing")

sources = json.loads((ROOT / "ops" / "state" / "truth" / "sources.json").read_text(encoding="utf-8"))
check("the truth map registers the lifecycle owner",
      sources["domains"]["pr_delivery_lifecycle"]["owner"] == "ops/state/truth/delivery.json",
      "not registered")
check("the truth map forbids a provider surface owning review semantics",
      any("delivery.json" in rule for rule in sources["forbidden"]), "no forbidden rule")


print()
if failures:
    print(f"delivery lifecycle tests: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("delivery lifecycle tests: clean")
