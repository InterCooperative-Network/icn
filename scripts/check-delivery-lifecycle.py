#!/usr/bin/env python3
"""check-delivery-lifecycle.py — validate the PR delivery lifecycle owner (icn#2661).

`ops/state/truth/delivery.json` is the registered owner of the pull-request delivery lifecycle:
when comprehensive review ends, what freezes, and how a late finding is classified. This validates
its shape and values, and enforces that provider reviewer surfaces bind to it instead of carrying
their own blocker, severity, or freeze semantics.

CONTRACT
    validate(value) -> list[str]

TOTAL over any JSON-shaped input, for the same reason the merge-policy validator is: a validator
that raises on a malformed document reports nothing, and nothing reads as clean. Every type is
established before the value is used.

JSON TYPES MEAN JSON TYPES
`isinstance(False, int)` is True in Python, so JSON `false` would satisfy an integer check. Counts
use `type(v) is int`; the invariant switches below require real booleans, because `1` and `"yes"`
must not be able to stand in for `true` on a field whose whole job is to be un-flippable.

WHY THE PREDICATE LIVES IN CODE
`BLOCKER_CONDITIONS` is hardcoded here and never read from the document under validation, and it
pins each condition's operative SENTENCE rather than only its identifier. The blocker predicate is
the load-bearing part of the freeze: if the conditions were data, a PR could weaken its own freeze
by deleting one from a JSON file — or, more quietly, by keeping all five ids and rewriting what
they require, which reviewers would then read and apply. Either edit would look like a policy
tweak rather than what it is. The same reasoning covers the state set, the disposition vocabulary,
the review kinds and the lane names. A closed set owned by code cannot be widened, narrowed,
renamed, or redefined by data.

THE FLOORS
`RESOLVE_WHEN`, `PROVIDER_FLOOR` and `BODY_MIRROR_FLOOR` work the same way as the predicate: the
document may bind MORE than they require, never less. They exist because the binding inventory was
self-describing — three entries pointing at one prompt, `must_reference` cut to a single string and
every pattern replaced with one that cannot match, all passed, while the Copilot adapters were
detached. An enforcement list that the enforced party can shorten is not enforcement.

THE INVARIANT SWITCHES
Four fields are pinned to one value each, because each of them is a way the treadmill comes back:

    review_generation.push_resets_generation           must be false
    review_generation.after_blocker_fix                must be "DELTA"
    blocker_predicate.automated_severity_is_advisory   must be true
    authority.mutation.orchestrator_may_not_duplicate_primitive   must be true

They are not defaults. A document that sets any of them the other way is rejected, so relaxing one
requires editing this file — which is a reviewable act rather than a data tweak.

PROSE IS CHECKED ONLY BY ABSENCE
Facts that must mechanically agree are compared as values. Provider surfaces are checked for
required references (substring, no grammar to get wrong) and forbidden patterns. Nothing here
parses English.

Run: python3 scripts/check-delivery-lifecycle.py [--verbose]
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
DELIVERY = ROOT / "ops" / "state" / "truth" / "delivery.json"
SOURCES = ROOT / "ops" / "state" / "truth" / "sources.json"

# --- pinned vocabularies. Owned HERE, never read from the document under validation. -----------
STATES = ("IMPLEMENTING", "REVIEWING", "FIXING", "VERIFYING", "FROZEN", "MERGING", "DONE")
TERMINAL_STATE = "DONE"
ENTRY_STATE = "IMPLEMENTING"
DISPOSITIONS = ("BLOCKER", "FOLLOW_UP", "QUESTION", "NOT_A_FINDING")
REVIEW_KINDS = ("FULL", "DELTA")
LANES = ("FAST", "STANDARD", "DEEP")
# Not just the identifiers: the OPERATIVE SENTENCE of each condition, byte for byte. Keeping the
# five ids while rewriting what they say would leave `validate()` silent, and reviewers read the
# prose — so "the predicate is fixed by code" was bypassable by an ordinary data edit. The policy
# file still carries these sentences for a human reader; the checker requires them to be identical
# to what is written here, which puts a change of meaning back where it belongs: in a reviewed
# code diff. Only `why` stays free-form, because it is rationale rather than the rule.
BLOCKER_CONDITIONS = {
    "reproducible":
        "It is concrete and reproducible.",
    "introduced_here":
        "It is introduced by this pull request rather than merely adjacent or pre-existing.",
    "violates_stated_contract":
        "It violates an explicit acceptance condition, an established repository invariant, or a "
        "behaviour this pull request claims to provide \u2014 or it regresses behaviour this pull "
        "request actually changes, whether or not the contract mentioned it.",
    "realistic_path":
        "It occurs on a supported and realistic execution path relevant to the feature's intended "
        "operation, rather than existing only as generalised hardening speculation.",
    "materially_breaks_it":
        "Leaving it unfixed would materially make the deliverable incorrect or unusable.",
}

# When a thread may be resolved, per disposition. Pinned because the merge owner counts
# unresolved threads: an instruction to resolve a QUESTION on asking it would let an agent
# manufacture merge readiness out of a question nobody answered.
RESOLVE_WHEN = {
    "BLOCKER": "after_the_fix_is_verified",
    "FOLLOW_UP": "after_the_ledger_entry_exists",
    "NOT_A_FINDING": "after_the_reply",
    "QUESTION": "after_it_is_answered_and_reclassified",
}

# The provider-binding FLOOR. The document may bind more than this; it may not bind less.
# Without it the inventory was self-describing: three entries all pointing at one prompt, with
# `must_reference` cut to a single string and every pattern replaced by one that cannot match,
# passed every check while the Copilot adapters were detached entirely.
_REVIEWER_REFS = ("ops/state/truth/delivery.json", "BLOCKER", "FOLLOW_UP", "NOT_A_FINDING",
                  "QUESTION", "FULL", "DELTA")
_REVIEWER_PATTERNS = (r"(?i)^#+.*always\s+flag",
                      r"(?i)\b(blocking|blocker)\b\s*(issues|list)?\s*:\s*$",
                      r"(?i)\brequest_changes\b")
PROVIDER_FLOOR = {
    ".claude/agents/icn-code-reviewer.md": (_REVIEWER_REFS, _REVIEWER_PATTERNS),
    ".github/agents/icn-code-reviewer.md": (_REVIEWER_REFS, _REVIEWER_PATTERNS),
    ".github/copilot-instructions.md": (
        ("AGENTS.md", "ops/state/truth/sources.json", "ops/state/truth/delivery.json"),
        (r"(?i)^#+\s*current\s+(status|working\s+context)",
         r"(?i)^#+\s*(project\s+overview|repository\s+structure|architecture\s+patterns)",
         r"(?i)\b(STATE|PHASE_PROGRESS)\.md\b.*canonical")),
}
BODY_MIRROR_FLOOR = ((".claude/agents/icn-code-reviewer.md",
                      ".github/agents/icn-code-reviewer.md"),)

# The merge side of the boundary. Named here so the lifecycle owner cannot quietly claim it.
MERGE_SEMANTICS_OWNER = "ops/state/truth/policy.json#merge"
MERGE_EXECUTABLE = "tools/icn-merge-pr"

# A flag- or command-shaped string anywhere under `authority.mutation` would be executable
# authority as data: the one place this document hands work to a program is the one place a
# spelled command could be picked up and run.
COMMAND_SHAPED = re.compile(r"(^|\s)(gh|git|python3?)\s|(^|\s)--[a-z]")


def _str(value) -> bool:
    return isinstance(value, str) and value.strip() != ""


def _obj(value) -> bool:
    return isinstance(value, dict)


def _list(value) -> bool:
    return isinstance(value, list)


def _true(value) -> bool:
    return value is True


def _strs(value) -> bool:
    return _list(value) and value != [] and all(_str(v) for v in value)


def _check_lifecycle(doc, out) -> None:
    life = doc.get("lifecycle")
    if not _obj(life):
        out.append("lifecycle: missing or not an object")
        return
    states = life.get("states")
    if not _list(states):
        out.append("lifecycle.states: missing or not an array")
        return
    seen = []
    graph = {}
    for i, s in enumerate(states):
        if not _obj(s):
            out.append(f"lifecycle.states[{i}]: not an object")
            continue
        name = s.get("name")
        if not _str(name) or name not in STATES:
            out.append(f"lifecycle.states[{i}].name: {name!r} is not one of {list(STATES)}")
            continue
        if name in seen:
            out.append(f"lifecycle.states[{i}].name: {name!r} declared twice")
            continue
        seen.append(name)
        if not _str(s.get("means")):
            out.append(f"lifecycle.states[{i}] ({name}): means must be a non-empty string")
        exits = s.get("exits_to")
        if not _list(exits) or not all(_str(e) for e in exits):
            out.append(f"lifecycle.states[{i}] ({name}): exits_to must be an array of strings")
            continue
        unknown = [e for e in exits if e not in STATES]
        if unknown:
            out.append(f"lifecycle.states[{i}] ({name}): exits_to names unknown states {unknown}")
        graph[name] = [e for e in exits if e in STATES]

    missing = [s for s in STATES if s not in seen]
    if missing:
        out.append(f"lifecycle.states: the lifecycle must define every state; missing {missing}")
    if seen and seen != [s for s in STATES if s in seen]:
        out.append(f"lifecycle.states: declared out of lifecycle order: {seen}")

    if graph.get(TERMINAL_STATE):
        out.append(f"lifecycle.states: {TERMINAL_STATE} must be terminal, "
                   f"but exits to {graph[TERMINAL_STATE]}")

    # Reachability. A lifecycle with an unreachable state, or with no path to DONE, describes a
    # process that cannot finish — which is the exact failure this owner exists to prevent.
    if not missing:
        reached, frontier = {ENTRY_STATE}, [ENTRY_STATE]
        while frontier:
            for nxt in graph.get(frontier.pop(), []):
                if nxt not in reached:
                    reached.add(nxt)
                    frontier.append(nxt)
        unreachable = [s for s in STATES if s not in reached]
        if unreachable:
            out.append(f"lifecycle.states: unreachable from {ENTRY_STATE}: {unreachable}")
        if TERMINAL_STATE not in reached:
            out.append(f"lifecycle.states: {TERMINAL_STATE} is not reachable from {ENTRY_STATE}; "
                       f"a lifecycle with no path to completion cannot converge")
        frozen_exits = graph.get("FROZEN", [])
        if "FIXING" in frozen_exits:
            for s in states:
                if _obj(s) and s.get("name") == "FROZEN" and not _str(
                        s.get("exit_to_fixing_requires")):
                    out.append("lifecycle.states (FROZEN): exits to FIXING without stating "
                               "exit_to_fixing_requires; an unconditional way out of a freeze is "
                               "not a freeze")

    surface = life.get("state_surface")
    if not _obj(surface):
        out.append("lifecycle.state_surface: missing or not an object")
        return
    if surface.get("owner") != "github-api":
        out.append(f"lifecycle.state_surface.owner: must be 'github-api', got "
                   f"{surface.get('owner')!r} — lifecycle state is volatile per-PR state")
    for marker in ("begin_marker", "end_marker"):
        if not _str(surface.get(marker)):
            out.append(f"lifecycle.state_surface.{marker}: must be a non-empty string")
    if not _strs(surface.get("fields")):
        out.append("lifecycle.state_surface.fields: must be a non-empty array of strings")
    rendered_by = surface.get("rendered_by")
    if not _strs(rendered_by):
        out.append("lifecycle.state_surface.rendered_by: must name the surfaces that render the "
                   "block, or nothing checks that they render what this owner declares")
    not_owned = surface.get("not_owned_by")
    if not _strs(not_owned):
        out.append("lifecycle.state_surface.not_owned_by: must be a non-empty array of strings")
    elif "repository files" not in not_owned:
        out.append("lifecycle.state_surface.not_owned_by: must exclude 'repository files' — a "
                   "checked-in file naming a PR's current state is stale by construction")


def _check_review_generation(doc, out) -> None:
    rg = doc.get("review_generation")
    if not _obj(rg):
        out.append("review_generation: missing or not an object")
        return
    if not _true(rg.get("comprehensive_review_is_bounded")):
        out.append("review_generation.comprehensive_review_is_bounded: must be true")
    if rg.get("push_resets_generation") is not False:
        out.append("review_generation.push_resets_generation: must be false — if a push reopened "
                   "discovery, fixing findings would be the act that triggers the next unbounded "
                   "search, and no PR that fixes anything could converge")
    if not _str(rg.get("push_resets_generation_why")):
        out.append("review_generation.push_resets_generation_why: must state the reason")
    if not _true(rg.get("blocker_fixes_are_batched")):
        out.append("review_generation.blocker_fixes_are_batched: must be true")
    if rg.get("after_blocker_fix") != "DELTA":
        out.append(f"review_generation.after_blocker_fix: must be 'DELTA', got "
                   f"{rg.get('after_blocker_fix')!r} — re-running FULL review after a fix is the "
                   f"treadmill")
    if not _true(rg.get("every_review_declares_its_kind")):
        out.append("review_generation.every_review_declares_its_kind: must be true")

    kinds = rg.get("review_kinds")
    if not _obj(kinds):
        out.append("review_generation.review_kinds: missing or not an object")
        return
    if sorted(kinds) != sorted(REVIEW_KINDS):
        out.append(f"review_generation.review_kinds: must define exactly {list(REVIEW_KINDS)}, "
                   f"got {sorted(kinds)}")
        return
    for kind in REVIEW_KINDS:
        spec = kinds[kind]
        if not _obj(spec):
            out.append(f"review_generation.review_kinds.{kind}: not an object")
            continue
        for field in ("may_inspect", "when"):
            if not _str(spec.get(field)):
                out.append(f"review_generation.review_kinds.{kind}.{field}: "
                           f"must be a non-empty string")
    delta = kinds["DELTA"]
    if _obj(delta) and not _str(delta.get("may_not")):
        out.append("review_generation.review_kinds.DELTA.may_not: must state what a DELTA review "
                   "may not do, or DELTA is FULL review with a different label")


def _check_dispositions(doc, out) -> None:
    ds = doc.get("finding_dispositions")
    if not _list(ds):
        out.append("finding_dispositions: missing or not an array")
        return
    names = []
    for i, d in enumerate(ds):
        if not _obj(d):
            out.append(f"finding_dispositions[{i}]: not an object")
            continue
        name = d.get("name")
        if not _str(name) or name not in DISPOSITIONS:
            out.append(f"finding_dispositions[{i}].name: {name!r} is not one of "
                       f"{list(DISPOSITIONS)}")
            continue
        names.append(name)
        for field in ("means", "action"):
            if not _str(d.get(field)):
                out.append(f"finding_dispositions[{i}] ({name}).{field}: "
                           f"must be a non-empty string")
        if d.get("resolve_thread") != RESOLVE_WHEN[name]:
            out.append(f"finding_dispositions[{i}] ({name}).resolve_thread: must be "
                       f"{RESOLVE_WHEN[name]!r}, got {d.get('resolve_thread')!r}. When a thread "
                       f"may be resolved is owned by code, because merge readiness counts "
                       f"unresolved threads")
    missing = [d for d in DISPOSITIONS if d not in names]
    if missing:
        out.append(f"finding_dispositions: every disposition must be defined; missing {missing}")


def _check_blocker_predicate(doc, out) -> None:
    bp = doc.get("blocker_predicate")
    if not _obj(bp):
        out.append("blocker_predicate: missing or not an object")
        return
    conds = bp.get("all_must_hold")
    if not _list(conds):
        out.append("blocker_predicate.all_must_hold: missing or not an array")
    else:
        ids = []
        for i, c in enumerate(conds):
            if not _obj(c):
                out.append(f"blocker_predicate.all_must_hold[{i}]: not an object")
                continue
            cid = c.get("id")
            if not _str(cid) or cid not in BLOCKER_CONDITIONS:
                out.append(f"blocker_predicate.all_must_hold[{i}].id: {cid!r} is not one of "
                           f"{list(BLOCKER_CONDITIONS)}")
                continue
            if cid in ids:
                out.append(f"blocker_predicate.all_must_hold[{i}].id: {cid!r} declared twice")
                continue
            ids.append(cid)
            for field in ("condition", "why"):
                if not _str(c.get(field)):
                    out.append(f"blocker_predicate.all_must_hold[{i}] ({cid}).{field}: "
                               f"must be a non-empty string")
            if c.get("condition") != BLOCKER_CONDITIONS[cid]:
                out.append(f"blocker_predicate.all_must_hold[{i}] ({cid}).condition: does not "
                           f"match the sentence this checker pins. The predicate's MEANING is "
                           f"owned by code, not only its identifiers — otherwise a data edit "
                           f"could keep the five ids and invert what each of them requires. "
                           f"Expected: {BLOCKER_CONDITIONS[cid]!r}")
        missing = [c for c in BLOCKER_CONDITIONS if c not in ids]
        if missing:
            out.append(f"blocker_predicate.all_must_hold: the predicate is fixed by code; "
                       f"missing conditions {missing}. Dropping a condition weakens every freeze "
                       f"in the repository and must be a reviewed code change, not a data edit")

    if not _true(bp.get("automated_severity_is_advisory")):
        out.append("blocker_predicate.automated_severity_is_advisory: must be true — otherwise a "
                   "reviewer's own label becomes authority over the maintainer's freeze")
    if not _strs(bp.get("advisory_severity_labels")):
        out.append("blocker_predicate.advisory_severity_labels: must be a non-empty array of "
                   "the labels that carry no authority")
    if not _str(bp.get("severity_note")):
        out.append("blocker_predicate.severity_note: must be a non-empty string")

    sweep = bp.get("no_sibling_sweep")
    if not _obj(sweep):
        out.append("blocker_predicate.no_sibling_sweep: missing or not an object")
    else:
        for field in ("rule", "exception", "why"):
            if not _str(sweep.get(field)):
                out.append(f"blocker_predicate.no_sibling_sweep.{field}: "
                           f"must be a non-empty string")
    redesign = bp.get("redesign_rule")
    if not _obj(redesign):
        out.append("blocker_predicate.redesign_rule: missing or not an object")
    else:
        for field in ("rule", "why"):
            if not _str(redesign.get(field)):
                out.append(f"blocker_predicate.redesign_rule.{field}: must be a non-empty string")


def _check_freeze(doc, out) -> None:
    fz = doc.get("freeze")
    if not _obj(fz):
        out.append("freeze: missing or not an object")
        return
    if not _strs(fz.get("entry_conditions")):
        out.append("freeze.entry_conditions: must be a non-empty array of strings")
    if not _true(fz.get("names_an_exact_head")):
        out.append("freeze.names_an_exact_head: must be true — a freeze that does not name a head "
                   "freezes nothing")
    if not _strs(fz.get("effect")):
        out.append("freeze.effect: must be a non-empty array of strings")
    for field in ("late_finding_rule", "refreeze", "server_enforced_gates_still_apply",
                  "head_must_match_before_handoff"):
        if not _str(fz.get(field)):
            out.append(f"freeze.{field}: must be a non-empty string")


def _check_ledger(doc, out) -> None:
    fl = doc.get("follow_up_ledger")
    if not _obj(fl):
        out.append("follow_up_ledger: missing or not an object")
        return
    if not _true(fl.get("one_issue_per_pull_request")):
        out.append("follow_up_ledger.one_issue_per_pull_request: must be true — one issue per "
                   "comment converts review noise into issue noise")
    if not _true(fl.get("reuse_before_creating")):
        out.append("follow_up_ledger.reuse_before_creating: must be true")
    if not _true(fl.get("provenance_link_required")):
        out.append("follow_up_ledger.provenance_link_required: must be true — a deferred "
                   "observation without a link back to its thread loses the evidence for it")
    for field in ("entry_shape", "why_not_one_issue_per_comment", "not_a_mandate_to_re_audit"):
        if not _str(fl.get(field)):
            out.append(f"follow_up_ledger.{field}: must be a non-empty string")


def _check_lanes(doc, out) -> None:
    lanes = doc.get("lanes")
    if not _obj(lanes):
        out.append("lanes: missing or not an object")
        return
    if lanes.get("default") not in LANES:
        out.append(f"lanes.default: {lanes.get('default')!r} is not one of {list(LANES)}")
    defs = lanes.get("definitions")
    if not _list(defs):
        out.append("lanes.definitions: missing or not an array")
        return
    names = []
    for i, lane in enumerate(defs):
        if not _obj(lane):
            out.append(f"lanes.definitions[{i}]: not an object")
            continue
        name = lane.get("name")
        if not _str(name) or name not in LANES:
            out.append(f"lanes.definitions[{i}].name: {name!r} is not one of {list(LANES)}")
            continue
        names.append(name)
        for field in ("for", "shape"):
            if not _str(lane.get(field)):
                out.append(f"lanes.definitions[{i}] ({name}).{field}: must be a non-empty string")
        gens = lane.get("comprehensive_generations")
        if type(gens) is not int or gens != 1:
            out.append(f"lanes.definitions[{i}] ({name}).comprehensive_generations: must be the "
                       f"integer 1, got {gens!r} — a lane that schedules more than one "
                       f"comprehensive generation is the treadmill with a name")
        if name == "DEEP" and not _true(lane.get("requires_maintainer_selection")):
            out.append("lanes.definitions (DEEP).requires_maintainer_selection: must be true — "
                       "DEEP is the expensive lane and an agent may not select it")
    missing = [x for x in LANES if x not in names]
    if missing:
        out.append(f"lanes.definitions: every lane must be defined; missing {missing}")


def _check_authority(doc, out) -> None:
    au = doc.get("authority")
    if not _obj(au):
        out.append("authority: missing or not an object")
        return
    for field in ("maintainer_only", "agents_may_not", "automated_reviewers_may_not"):
        if not _strs(au.get(field)):
            out.append(f"authority.{field}: must be a non-empty array of strings")
    mut = au.get("mutation")
    if not _obj(mut):
        out.append("authority.mutation: missing or not an object")
        return
    if mut.get("semantics_owner") != MERGE_SEMANTICS_OWNER:
        out.append(f"authority.mutation.semantics_owner: must be {MERGE_SEMANTICS_OWNER!r}, got "
                   f"{mut.get('semantics_owner')!r} — this file does not own merge semantics")
    if mut.get("executable") != MERGE_EXECUTABLE:
        out.append(f"authority.mutation.executable: must be {MERGE_EXECUTABLE!r}, got "
                   f"{mut.get('executable')!r}")
    for field in ("orchestrator", "primitive"):
        if not _str(mut.get(field)):
            out.append(f"authority.mutation.{field}: must name a path")
    if not _true(mut.get("orchestrator_may_not_duplicate_primitive")):
        out.append("authority.mutation.orchestrator_may_not_duplicate_primitive: must be true — a "
                   "second implementation of merge semantics is a second owner of them, and the "
                   "copy is the one that rots")
    if not _str(mut.get("why")):
        out.append("authority.mutation.why: must be a non-empty string")
    for key, value in mut.items():
        if _str(value) and COMMAND_SHAPED.search(value) and key != "why":
            out.append(f"authority.mutation.{key}: holds a command- or flag-shaped string "
                       f"({value!r}). Intent is declared symbolically; only code maps it to a "
                       f"command")


def _check_boundary(doc, out) -> None:
    ob = doc.get("owner_boundary")
    if not _obj(ob):
        out.append("owner_boundary: missing or not an object")
        return
    if not _strs(ob.get("owns")):
        out.append("owner_boundary.owns: must be a non-empty array of strings")
    dno = ob.get("does_not_own")
    if not _list(dno) or dno == []:
        out.append("owner_boundary.does_not_own: must be a non-empty array")
        return
    owners = []
    for i, entry in enumerate(dno):
        if not _obj(entry):
            out.append(f"owner_boundary.does_not_own[{i}]: not an object")
            continue
        for field in ("fact", "owner"):
            if not _str(entry.get(field)):
                out.append(f"owner_boundary.does_not_own[{i}].{field}: must be a non-empty string")
        if _str(entry.get("owner")):
            owners.append(entry["owner"])
    if not any(MERGE_SEMANTICS_OWNER in o for o in owners):
        out.append(f"owner_boundary.does_not_own: must disclaim {MERGE_SEMANTICS_OWNER} "
                   f"explicitly, so this document can never grow into a second merge owner")


def validate(value) -> list[str]:
    """Every structural complaint about `value`. Total: never raises on malformed input."""
    out: list[str] = []
    if not _obj(value):
        return ["delivery.json: the document is not a JSON object"]
    if value.get("schema") != "icn.delivery-lifecycle.v1":
        out.append(f"schema: must be 'icn.delivery-lifecycle.v1', got {value.get('schema')!r}")
    for field in ("description", "principle", "why_this_exists"):
        if not _str(value.get(field)):
            out.append(f"{field}: must be a non-empty string")
    _check_boundary(value, out)
    _check_lifecycle(value, out)
    _check_review_generation(value, out)
    _check_dispositions(value, out)
    _check_blocker_predicate(value, out)
    _check_freeze(value, out)
    _check_ledger(value, out)
    _check_lanes(value, out)
    _check_authority(value, out)
    _check_provider_bindings(value, out)
    return out


def _check_provider_bindings(doc, out) -> None:
    pb = doc.get("provider_bindings")
    if not _obj(pb):
        out.append("provider_bindings: missing or not an object")
        return
    if not _str(pb.get("rule")):
        out.append("provider_bindings.rule: must be a non-empty string")
    mirrors = pb.get("body_mirrors")
    if not _list(mirrors) or mirrors == []:
        out.append("provider_bindings.body_mirrors: must be a non-empty array")
    else:
        for i, m in enumerate(mirrors):
            if not _obj(m):
                out.append(f"provider_bindings.body_mirrors[{i}]: not an object")
                continue
            for field in ("canonical", "mirror", "why"):
                if not _str(m.get(field)):
                    out.append(f"provider_bindings.body_mirrors[{i}].{field}: "
                               f"must be a non-empty string")
    declared_mirrors = [(m.get("canonical"), m.get("mirror")) for m in mirrors
                        if _obj(m)] if _list(mirrors) else []
    if sorted(declared_mirrors) != sorted(BODY_MIRROR_FLOOR):
        out.append(f"provider_bindings.body_mirrors: must pair exactly "
                   f"{[list(x) for x in BODY_MIRROR_FLOOR]}, got "
                   f"{[list(x) for x in declared_mirrors]}. The pairs are owned by code, so a "
                   f"self-referential or dropped mirror cannot silently disable the comparison")

    surfaces = pb.get("surfaces")
    if not _list(surfaces) or surfaces == []:
        out.append("provider_bindings.surfaces: must be a non-empty array")
        return
    declared_paths = [s.get("path") for s in surfaces if _obj(s)]
    if sorted(p for p in declared_paths if _str(p)) != sorted(PROVIDER_FLOOR):
        out.append(f"provider_bindings.surfaces: must bind exactly {sorted(PROVIDER_FLOOR)}, got "
                   f"{sorted(str(p) for p in declared_paths)}. The inventory is owned by code: an "
                   f"entry removed here would detach a provider adapter while this check stayed "
                   f"green")
    for i, s in enumerate(surfaces):
        if not _obj(s):
            out.append(f"provider_bindings.surfaces[{i}]: not an object")
            continue
        for field in ("path", "role"):
            if not _str(s.get(field)):
                out.append(f"provider_bindings.surfaces[{i}].{field}: must be a non-empty string")
        if not _strs(s.get("must_reference")):
            out.append(f"provider_bindings.surfaces[{i}].must_reference: must be a non-empty "
                       f"array of strings")
        floor = PROVIDER_FLOOR.get(s.get("path")) if _str(s.get("path")) else None
        if floor and _strs(s.get("must_reference")):
            missing = [r for r in floor[0] if r not in s["must_reference"]]
            if missing:
                out.append(f"provider_bindings.surfaces[{i}] ({s['path']}).must_reference: "
                           f"missing {missing}, which this checker requires. Data may add "
                           f"references; it may not drop below the floor")
        rules = s.get("must_not_match")
        if not _list(rules) or rules == []:
            out.append(f"provider_bindings.surfaces[{i}].must_not_match: must be a non-empty "
                       f"array")
            continue
        if floor:
            declared = [r.get("pattern") for r in rules if _obj(r)]
            missing = [x for x in floor[1] if x not in declared]
            if missing:
                out.append(f"provider_bindings.surfaces[{i}] ({s['path']}).must_not_match: "
                           f"missing {missing}, which this checker requires. Replacing a pattern "
                           f"with one that cannot match would defang the check silently")
        for j, rule in enumerate(rules):
            if not _obj(rule) or not _str(rule.get("pattern")) or not _str(rule.get("why")):
                out.append(f"provider_bindings.surfaces[{i}].must_not_match[{j}]: needs a "
                           f"non-empty pattern and why")
                continue
            try:
                re.compile(rule["pattern"])
            except re.error as exc:
                out.append(f"provider_bindings.surfaces[{i}].must_not_match[{j}].pattern: "
                           f"not a valid regular expression ({exc})")


# --- enforcement against the repository, not just the document --------------------------------

def enforce_surfaces(doc, root: pathlib.Path, verbose: bool) -> list[str]:
    """Every provider surface named by the owner actually binds to it."""
    out: list[str] = []
    pb = doc.get("provider_bindings")
    if not _obj(pb) or not _list(pb.get("surfaces")):
        return out
    for s in pb["surfaces"]:
        if not _obj(s) or not _str(s.get("path")):
            continue
        path = root / s["path"]
        if not path.is_file():
            out.append(f"{s['path']}: named as a provider binding but the file does not exist")
            continue
        text = path.read_text(encoding="utf-8")
        for ref in s.get("must_reference", []):
            if not _str(ref):
                continue
            if ref not in text:
                out.append(f"{s['path']}: must reference {ref!r} — a provider reviewer surface "
                           f"that does not name the canonical lifecycle has become a second "
                           f"owner of it")
            elif verbose:
                print(f"  ok   {s['path']}: references {ref!r}")
        for rule in s.get("must_not_match", []):
            if not _obj(rule) or not _str(rule.get("pattern")):
                continue
            try:
                rx = re.compile(rule["pattern"], re.M)
            except re.error:
                continue
            for lineno, line in enumerate(text.splitlines(), 1):
                if rx.search(line):
                    out.append(f"{s['path']}:{lineno}: matched forbidden pattern "
                               f"{rule['pattern']!r}. {rule.get('why', '')}\n"
                               f"            {line.strip()}")
    return out


FRONT_MATTER = re.compile(r"\A---\n.*?\n---\n", re.S)


def _body(path: pathlib.Path) -> str:
    """A provider prompt's body: everything after its YAML front matter."""
    text = path.read_text(encoding="utf-8")
    match = FRONT_MATTER.match(text)
    return text[match.end():] if match else text


def enforce_body_mirrors(doc, root: pathlib.Path, verbose: bool) -> list[str]:
    """Two provider prompts, one document. Front matter may differ; the body may not.

    This is the mechanical form of "do not maintain two independent copies of blocker semantics".
    Reference and pattern assertions catch a surface that stops naming the owner; only comparison
    catches the two of them slowly saying different things while both still name it.
    """
    out: list[str] = []
    pb = doc.get("provider_bindings")
    if not _obj(pb) or not _list(pb.get("body_mirrors")):
        return out
    for m in pb["body_mirrors"]:
        if not _obj(m) or not _str(m.get("canonical")) or not _str(m.get("mirror")):
            continue
        canonical, mirror = root / m["canonical"], root / m["mirror"]
        missing = [str(x) for x in (canonical, mirror) if not x.is_file()]
        if missing:
            out.append(f"{m['mirror']}: declared a body mirror, but {missing} does not exist")
            continue
        if _body(canonical) != _body(mirror):
            out.append(f"{m['mirror']}: body differs from its canonical source "
                       f"{m['canonical']}. {m.get('why', '')}")
        elif verbose:
            print(f"  ok   {m['mirror']}: body identical to {m['canonical']}")
    return out


BLOCK_HEADING = "ICN DELIVERY LIFECYCLE"
BLOCK_LABEL = re.compile(r"^([A-Za-z][A-Za-z -]*?):")   # labels carry hyphens: "Follow-up ledger"


def _rendered_labels(text: str) -> list[set[str]]:
    """The field labels of every lifecycle block in `text`, one set per block."""
    blocks, lines = [], text.splitlines()
    for i, line in enumerate(lines):
        if line.strip() != BLOCK_HEADING:
            continue
        labels = set()
        for following in lines[i + 1:]:
            if following.startswith("```") or not following.strip():
                break
            match = BLOCK_LABEL.match(following.strip())
            if match:
                labels.add(match.group(1).strip())
        blocks.append(labels)
    return blocks


def enforce_state_surface(doc, root: pathlib.Path, verbose: bool) -> list[str]:
    """Every surface that renders the lifecycle block renders every field the owner declares.

    The owner names the fields; three surfaces render the block. Two of them quietly rendered a
    different set, and nothing noticed — which is the same class of drift this file exists to
    stop, just inside the repository rather than in a provider prompt.
    """
    out: list[str] = []
    surface = doc.get("lifecycle", {}).get("state_surface") if _obj(doc.get("lifecycle")) else None
    if not _obj(surface) or not _strs(surface.get("fields")) or not _strs(
            surface.get("rendered_by")):
        return out
    declared = set(surface["fields"])
    for rel in surface["rendered_by"]:
        path = root / rel
        if not path.is_file():
            out.append(f"{rel}: named in lifecycle.state_surface.rendered_by but does not exist")
            continue
        blocks = _rendered_labels(path.read_text(encoding="utf-8"))
        if not blocks:
            out.append(f"{rel}: renders no {BLOCK_HEADING!r} block, but is named as a surface "
                       f"that does")
            continue
        for n, labels in enumerate(blocks, 1):
            missing = sorted(declared - labels)
            if missing:
                where = f" (block {n})" if len(blocks) > 1 else ""
                out.append(f"{rel}{where}: the lifecycle block omits {missing}, which "
                           f"lifecycle.state_surface.fields declares")
            elif verbose:
                print(f"  ok   {rel}{f' (block {n})' if len(blocks) > 1 else ''}: "
                      f"renders every declared field")
    return out


def enforce_registration(doc, root: pathlib.Path) -> list[str]:
    """The owner is registered in the truth map, and every path it names exists."""
    out: list[str] = []
    try:
        sources = json.loads((root / "ops" / "state" / "truth" / "sources.json")
                             .read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return [f"sources.json: unreadable ({exc})"]
    domains = sources.get("domains") if _obj(sources) else None
    entry = domains.get("pr_delivery_lifecycle") if _obj(domains) else None
    if not _obj(entry):
        return ["sources.json: domain 'pr_delivery_lifecycle' is not registered; an owner the "
                "truth map does not name cannot be resolved by a fresh session"]
    if entry.get("owner") != "ops/state/truth/delivery.json":
        out.append(f"sources.json#domains.pr_delivery_lifecycle.owner: must be "
                   f"'ops/state/truth/delivery.json', got {entry.get('owner')!r}")
    if entry.get("checker") != "scripts/check-delivery-lifecycle.py":
        out.append("sources.json#domains.pr_delivery_lifecycle.checker: must name this script")

    mut = doc.get("authority", {}).get("mutation") if _obj(doc.get("authority")) else None
    if _obj(mut):
        for field in ("orchestrator", "primitive", "executable"):
            named = mut.get(field)
            if _str(named) and not (root / named).exists():
                out.append(f"authority.mutation.{field}: names {named!r}, which does not exist")
    return out


def main() -> int:
    verbose = "--verbose" in sys.argv[1:]
    try:
        doc = json.loads(DELIVERY.read_text(encoding="utf-8"))
    except OSError as exc:
        print(f"check-delivery-lifecycle: FAIL\n  - {DELIVERY} is unreadable: {exc}")
        return 1
    except ValueError as exc:
        print(f"check-delivery-lifecycle: FAIL\n  - {DELIVERY} is not valid JSON: {exc}")
        return 1

    problems = validate(doc)
    problems += enforce_registration(doc, ROOT)
    problems += enforce_surfaces(doc, ROOT, verbose)
    problems += enforce_body_mirrors(doc, ROOT, verbose)
    problems += enforce_state_surface(doc, ROOT, verbose)

    if problems:
        print("check-delivery-lifecycle: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"check-delivery-lifecycle: OK — {len(STATES)} lifecycle states, "
          f"{len(BLOCKER_CONDITIONS)} blocker conditions, "
          f"{len(doc['provider_bindings']['surfaces'])} provider surfaces bound")
    return 0


if __name__ == "__main__":
    sys.exit(main())
