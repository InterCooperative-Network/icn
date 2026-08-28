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
print("the predicate's meaning is pinned, not just its labels")

# Keeping the five ids while rewriting what each requires left the validator silent, and the
# prose is what a reviewer actually applies. Each sentence is now owned by the checker.
for i, condition in enumerate(DELIVERY["blocker_predicate"]["all_must_hold"]):
    cid = condition["id"]
    check(f"the committed {cid!r} condition is the sentence the checker pins",
          condition["condition"] == gate.BLOCKER_CONDITIONS[cid], repr(condition["condition"]))
    rejects(f"inverting the meaning of {cid!r} while keeping its id is rejected",
            mutated(**{f"blocker_predicate__all_must_hold__{i}__condition":
                       "Anything at all qualifies, however unreachable."}),
            "owned by code")

inverted = json.loads(json.dumps(DELIVERY))
for condition in inverted["blocker_predicate"]["all_must_hold"]:
    condition["condition"] = "Any observation whatsoever is a blocker."
problems = gate.validate(inverted)
check("a wholly inverted predicate produces one complaint per condition",
      len([p for p in problems if "owned by code" in p]) == len(gate.BLOCKER_CONDITIONS),
      str(problems[:2]))

check("rationale stays free-form, so a `why` may be reworded without a code change",
      gate.validate(mutated(blocker_predicate__all_must_hold__0__why="Reworded rationale.")) == [],
      "a `why` edit was rejected")


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
print("a regression counts even when the contract is silent about it")

# A pull request does not get to break what it touched by omitting it from the contract. Without
# this, a reproducible, introduced, realistic, materially breaking regression failed the
# stated-contract condition and had to be dispositioned FOLLOW_UP — resolved without a fix.
contract_condition = next(c for c in DELIVERY["blocker_predicate"]["all_must_hold"]
                          if c["id"] == "violates_stated_contract")
check("the stated-contract condition covers regressions in behaviour the diff changes",
      "regresses behaviour this pull request actually changes" in contract_condition["condition"],
      contract_condition["condition"][:90])
check("the reviewer adapter and the owner name the same category",
      "regression in behaviour this diff actually changes"
      in (ROOT / ".claude" / "agents" / "icn-code-reviewer.md").read_text(encoding="utf-8"),
      "adapter and owner disagree")
rejects("narrowing the condition back to declared contract terms is rejected",
        mutated(**{"blocker_predicate__all_must_hold__2__condition":
                   "It violates an explicit acceptance condition."}), "owned by code")


# ---------------------------------------------------------------------------------------------
print("readiness is decided in one place")

# The skill told itself to read the merge owner and live protection to pick which gates to wait
# on, while its own Boundaries forbade exactly that. A document that contradicts itself on its
# central boundary cannot be followed consistently, and the registry pattern against a protection
# path was decorative next to prose that said to do it.
ship_wait = (ROOT / ".agents" / "skills" / "ship-pr" / "SKILL.md").read_text(encoding="utf-8")
check("ship-pr does not decide which gates matter",
      "Do not work out which gates matter" in ship_wait, "still deciding")
check("ship-pr waits only on a gate the evaluator named",
      "never wait on a check the evaluator did not name" in ship_wait, "missing")
check("ship-pr still forbids reading protection for readiness",
      "Do not read branch protection to decide readiness" in ship_wait, "boundary lost")
check("the two statements no longer contradict each other",
      "which gates actually matter from the merge owner and live branch protection"
      not in ship_wait, "contradiction remains")


# ---------------------------------------------------------------------------------------------
print("a freeze names the head that ships")

check("the owner requires the live head to match the freeze head before handoff",
      bool(DELIVERY["freeze"].get("head_must_match_before_handoff")), "no rule")
rejects("dropping the head-match rule is rejected",
        mutated(freeze__head_must_match_before_handoff=None), "head_must_match_before_handoff")
ship_hand = (ROOT / ".agents" / "skills" / "ship-pr" / "SKILL.md").read_text(encoding="utf-8")
check("ship-pr compares the freeze head before handing over",
      "head_must_match_before_handoff" in ship_hand and "freeze head" in ship_hand, "missing")
check("ship-pr refreezes rather than shipping unverified content",
      "return to FROZEN" in ship_hand, "no refreeze instruction")


# ---------------------------------------------------------------------------------------------
print("a thread is resolved when its disposition is final, not when it is written")

# Merge readiness counts unresolved threads, so "reply and resolve" applied to a QUESTION would
# convert an unanswered question into a readiness signal. When each disposition may resolve is
# therefore owned by the checker, not by the document or by a skill.
for d in DELIVERY["finding_dispositions"]:
    check(f"{d['name']} resolves {gate.RESOLVE_WHEN[d['name']]}",
          d.get("resolve_thread") == gate.RESOLVE_WHEN[d["name"]], repr(d.get("resolve_thread")))
for i, d in enumerate(DELIVERY["finding_dispositions"]):
    rejects(f"letting {d['name']} resolve at the wrong moment is rejected",
            mutated(**{f"finding_dispositions__{i}__resolve_thread": "immediately"}),
            "owned by code")
    rejects(f"dropping {d['name']}'s resolve rule is rejected",
            mutated(**{f"finding_dispositions__{i}__resolve_thread": None}), "resolve_thread")

ship_skill = (ROOT / ".agents" / "skills" / "ship-pr" / "SKILL.md").read_text(encoding="utf-8")
check("ship-pr defers to the owner's resolve rule", "resolve_thread" in ship_skill, "missing")
check("ship-pr does not resolve every thread on reply",
      "the evidence, then resolve it" not in ship_skill, "still unconditional")
check("ship-pr says a QUESTION stays unresolved",
      "QUESTION stays" in ship_skill and "unresolved" in ship_skill, "missing")


# ---------------------------------------------------------------------------------------------
print("the provider-binding inventory is a floor, not a self-description")

# The inventory used to describe itself: three entries all naming one prompt, `must_reference`
# cut to a single string and every pattern replaced with one that cannot match, passed every
# check while the Copilot adapters were detached. An enforcement list the enforced party can
# shorten is not enforcement.
check("the committed surfaces are exactly the floor",
      sorted(s["path"] for s in DELIVERY["provider_bindings"]["surfaces"])
      == sorted(gate.PROVIDER_FLOOR),
      str(sorted(s["path"] for s in DELIVERY["provider_bindings"]["surfaces"])))
for surface in DELIVERY["provider_bindings"]["surfaces"]:
    refs, patterns = gate.PROVIDER_FLOOR[surface["path"]]
    check(f"{surface['path']} declares every required reference",
          all(r in surface["must_reference"] for r in refs),
          str([r for r in refs if r not in surface["must_reference"]]))
    declared = [r["pattern"] for r in surface["must_not_match"]]
    check(f"{surface['path']} declares every required pattern",
          all(x in declared for x in patterns), str([x for x in patterns if x not in declared]))

claude_prompt = ".claude/agents/icn-code-reviewer.md"
degenerate = json.loads(json.dumps(DELIVERY))
degenerate["provider_bindings"]["surfaces"] = [
    {"path": claude_prompt, "role": f"r{i}", "must_reference": ["BLOCKER"],
     "must_not_match": [{"pattern": "a^", "why": "defanged"}]} for i in range(3)]
degenerate["provider_bindings"]["body_mirrors"] = [
    {"canonical": claude_prompt, "mirror": claude_prompt, "why": "self"}]
rejects("three entries all naming one prompt are rejected", degenerate, "must bind exactly")
rejects("a self-referential body mirror is rejected", degenerate, "must pair exactly")

dropped = json.loads(json.dumps(DELIVERY))
dropped["provider_bindings"]["surfaces"] = [
    s for s in dropped["provider_bindings"]["surfaces"]
    if s["path"] != ".github/agents/icn-code-reviewer.md"]
rejects("dropping a provider surface from the inventory is rejected", dropped, "must bind exactly")

for i, surface in enumerate(DELIVERY["provider_bindings"]["surfaces"]):
    thinned = json.loads(json.dumps(DELIVERY))
    thinned["provider_bindings"]["surfaces"][i]["must_reference"] = ["BLOCKER"]
    rejects(f"thinning must_reference on {surface['path']} is rejected", thinned,
            "may not drop below the floor")
    defanged = json.loads(json.dumps(DELIVERY))
    defanged["provider_bindings"]["surfaces"][i]["must_not_match"] = [
        {"pattern": "a^", "why": "cannot match"}]
    rejects(f"defanging the patterns on {surface['path']} is rejected", defanged,
            "defang the check silently")


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
print("every surface that renders the block renders what the owner declares")

state_surface = DELIVERY["lifecycle"]["state_surface"]
check("the owner names the surfaces that render its block",
      len(state_surface["rendered_by"]) >= 3, str(state_surface["rendered_by"]))
for rel in state_surface["rendered_by"]:
    blocks = gate._rendered_labels((ROOT / rel).read_text(encoding="utf-8"))
    check(f"{rel} renders a lifecycle block", bool(blocks), "no block found")
    for n, labels in enumerate(blocks, 1):
        check(f"{rel} block {n} renders every declared field",
              set(state_surface["fields"]) <= labels,
              str(sorted(set(state_surface["fields"]) - labels)))
rejects("a policy that names no rendering surface is rejected",
        mutated(lifecycle__state_surface__rendered_by=[]), "rendered_by")


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
    for rel in DELIVERY["lifecycle"]["state_surface"]["rendered_by"]:
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / rel, target)
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

    # A rendering surface drops a field the owner declares.
    short_block = gate_root(tmp, "short-block")
    victim = short_block / ".github" / "pull_request_template.md"
    victim.write_text("\n".join(line for line in
                                victim.read_text(encoding="utf-8").splitlines()
                                if not line.startswith("Follow-up ledger:")) + "\n",
                      encoding="utf-8")
    proc = run_gate(short_block)
    check("a surface that stops rendering a declared field fails the gate",
          proc.returncode == 1 and "Follow-up ledger" in proc.stdout, proc.stdout[-300:])

    # The predicate keeps its ids but changes what they require.
    redefined = gate_root(tmp, "redefined-predicate")
    policy_path = redefined / "ops" / "state" / "truth" / "delivery.json"
    policy_path.write_text(json.dumps(
        mutated(blocker_predicate__all_must_hold__3__condition="Anything counts."), indent=2)
        + "\n", encoding="utf-8")
    proc = run_gate(redefined)
    check("a predicate that keeps its ids but inverts their meaning fails the gate",
          proc.returncode == 1 and "owned by code" in proc.stdout, proc.stdout[-300:])

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
