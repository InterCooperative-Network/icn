#!/usr/bin/env python3
"""Synthetic tests for scripts/check-agent-registry.py (icn#2632 stage 1).

A completeness checker that cannot fail is worse than none: it converts an unverified
claim into a verified-looking one. Every MUST-FAIL case below reconstructs a real defect
the v1 registry actually carried, and every MUST-PASS case is a control proving the rule
is not simply rejecting everything.

The two headline reconstructions:

  - `unregistered_file_on_disk` is `.github/agents/` -- 21 live GitHub Copilot definitions
    that no registry mentioned, while `agents.json` called itself the list of all agents.
  - `path_escapes_repo` is the v1 `orchestrator` record, which named
    `../../../.claude/agents/orchestrator.md`. No checker validated it, so a path that
    left the repository root sat in a registered truth owner indefinitely.

Every case runs the real checker against a temporary repo root via --repo-root.

Run: python3 scripts/tests/test_agent_registry_invariants.py
"""
import copy
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
CHECKER = ROOT / "scripts" / "check-agent-registry.py"

failures = []
checks_run = 0


def check(desc, cond):
    global checks_run
    checks_run += 1
    print(("  ok   " if cond else "  FAIL ") + desc)
    if not cond:
        failures.append(desc)


def agent_file(text_name):
    return "---\nname: %s\ndescription: fixture\n---\n\nBody for %s.\n" % (
        text_name, text_name)


def base_registry():
    """A minimal, well-formed icn-agents/v2 registry over two surfaces."""
    return {
        "schema": "icn-agents/v2",
        "provider_surfaces": {
            "claude": {"tree": ".claude/agents"},
            "copilot": {"tree": ".github/agents"},
        },
        "relationship_model": {
            "single_surface": "one",
            "exact_mirror": "identical",
            "provider_variant": "deliberate",
            "divergent_unreviewed": "unadjudicated",
        },
        "declared_scope": {"in_scope": [], "out_of_scope": [], "completeness_claim": "x"},
        "agents": [
            {
                "name": "solo",
                "relationship": "single_surface",
                "surfaces": {"claude": {"path": ".claude/agents/solo.md"}},
            },
            {
                "name": "twin",
                "relationship": "exact_mirror",
                "surfaces": {
                    "claude": {"path": ".claude/agents/twin.md"},
                    "copilot": {"path": ".github/agents/twin.md"},
                },
            },
        ],
    }


def base_skills():
    return {
        "declared_scope": {
            "cross_registry": {
                "agent_surfaces_tracked_by_agents_json": [".claude/agents", ".github/agents"],
            }
        }
    }


def build(tmp, registry, skills, files):
    root = pathlib.Path(tmp)
    for rel in (".claude/agents", ".github/agents", "ops/state/truth"):
        (root / rel).mkdir(parents=True, exist_ok=True)
    for rel, text in files.items():
        fp = root / rel
        fp.parent.mkdir(parents=True, exist_ok=True)
        fp.write_text(text, encoding="utf-8")
    (root / "ops/state/truth/agents.json").write_text(
        json.dumps(registry, indent=2), encoding="utf-8")
    sk = root / "ops/state/truth/skills.json"
    if skills == "DELETE":
        pass                                   # deliberately absent
    elif isinstance(skills, dict) and "__RAW__" in skills:
        sk.write_text(skills["__RAW__"], encoding="utf-8")
    elif skills is not None:
        sk.write_text(json.dumps(skills, indent=2), encoding="utf-8")
    return root


def run(registry, skills, files):
    tmp = tempfile.mkdtemp()
    try:
        root = build(tmp, registry, skills, files)
        p = subprocess.run(
            [sys.executable, str(CHECKER), "--repo-root", str(root)],
            capture_output=True, text=True)
        return p.returncode, p.stdout + p.stderr
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


BASE_FILES = {
    ".claude/agents/solo.md": agent_file("solo"),
    ".claude/agents/twin.md": agent_file("twin"),
    ".github/agents/twin.md": agent_file("twin"),
}


# --------------------------------------------------------------------------- controls
print("--- MUST-PASS controls (the rules are not rejecting everything) ---")

rc, out = run(base_registry(), base_skills(), dict(BASE_FILES))
check("a well-formed two-surface registry passes (rc=%d)" % rc, rc == 0)

r = base_registry()
r["agents"][1]["mirror_pairs"] = [["claude", "copilot"]]
rc, out = run(r, base_skills(), dict(BASE_FILES))
check("a mirror_pair whose bodies match passes", rc == 0)

r = base_registry()
r["agents"][1]["relationship"] = "provider_variant"
r["agents"][1]["divergence"] = {"adjudicated": True, "why": "deliberate"}
f = dict(BASE_FILES)
f[".github/agents/twin.md"] = agent_file("twin").replace("Body for", "Different body for")
rc, out = run(r, base_skills(), f)
check("an adjudicated provider_variant with differing bodies passes", rc == 0)


# --------------------------------------------------------------------------- must-fail
print()
print("--- MUST-FAIL cases (each reconstructs a real or reachable defect) ---")

cases = []


def case(label, mutate_reg=None, mutate_skills=None, mutate_files=None, expect=None):
    r, s, f = base_registry(), base_skills(), dict(BASE_FILES)
    if mutate_reg:
        mutate_reg(r)
    if mutate_skills == "DELETE":
        s = "DELETE"
    elif mutate_skills:
        mutate_skills(s)
    if mutate_files:
        mutate_files(f)
    rc, out = run(r, s, f)
    hit = (expect is None) or (expect.lower() in out.lower())
    check("%s -> rejected%s" % (label, "" if hit else " (but not for the stated reason)"),
          rc != 0 and hit)


def setpath(rec_i, surface, value):
    return lambda r: r["agents"][rec_i]["surfaces"][surface].__setitem__("path", value)


case("schema is still icn-agents/v1",
     lambda r: r.__setitem__("schema", "icn-agents/v1"), expect="icn-agents/v2")

case("provider_surfaces missing entirely",
     lambda r: r.__setitem__("provider_surfaces", {}), expect="provider_surfaces")

case("a declared surface tree does not exist",
     lambda r: r["provider_surfaces"].__setitem__("ghost", {"tree": ".ghost/agents"}),
     expect="does not exist")

case("REAL: registered path escapes the repository root (the v1 orchestrator record)",
     setpath(0, "claude", "../../../.claude/agents/orchestrator.md"),
     expect="tree")

case("registered path is missing on disk",
     setpath(0, "claude", ".claude/agents/nope.md"), expect="missing")

case("registered path sits outside its declared tree",
     setpath(0, "claude", ".github/agents/solo.md"), expect="not inside")

case("REAL: a definition on disk that no record names (.github/agents/)",
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/unregistered.md", agent_file("unregistered")),
     expect="no record names it")

case("a record points at a file with a different stem",
     mutate_reg=setpath(0, "claude", ".claude/agents/twin.md"),
     expect="two agents wearing one name")

case("front matter declares a different name than the record",
     mutate_files=lambda f: f.__setitem__(".claude/agents/solo.md", agent_file("impostor")),
     expect="front matter declares")

case("two records share one logical name",
     lambda r: r["agents"].append(copy.deepcopy(r["agents"][0])), expect="duplicate record")

case("relationship is not in relationship_model",
     lambda r: r["agents"][0].__setitem__("relationship", "vibes"),
     expect="relationship_model")

case("single_surface declared but two surfaces named",
     lambda r: r["agents"][1].__setitem__("relationship", "single_surface"),
     expect="single_surface but 2")

case("exact_mirror declared but the bodies differ",
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", agent_file("twin").replace("Body", "Drifted body")),
     expect="bodies differ")

case("a declared mirror_pair whose bodies differ",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "divergent_unreviewed"),
                           r["agents"][1].__setitem__("divergence", {"owning_issue": "x"}),
                           r["agents"][1].__setitem__("mirror_pairs", [["claude", "copilot"]])),
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", agent_file("twin").replace("Body", "Drifted body")),
     expect="mirror pair")

case("mirror_pairs names a surface the record does not expose",
     lambda r: r["agents"][0].__setitem__("mirror_pairs", [["claude", "copilot"]]),
     expect="does not expose")

case("divergent_unreviewed with no owning issue",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "divergent_unreviewed"),
                           r["agents"][1].__setitem__("divergence", {})),
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", agent_file("twin").replace("Body", "Other body")),
     expect="owning_issue")

case("provider_variant asserted without adjudication",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "provider_variant"),
                           r["agents"][1].__setitem__("divergence", {"why": "because"})),
     expect="adjudicated")

case("REAL: skills.json claims a surface agents.json does not declare",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"][
         "agent_surfaces_tracked_by_agents_json"].append("tools/claude-code/plugins"),
     expect="declares no such provider surface")

case("skills.json omits a surface agents.json does declare",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"].__setitem__(
         "agent_surfaces_tracked_by_agents_json", [".claude/agents"]),
     expect="omits it")

case("skills.json has no structured cross_registry claim at all",
     mutate_skills=lambda s: s["declared_scope"].pop("cross_registry"),
     expect="cross_registry")

case("skills.json calls a registered surface uncovered by any registry",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"].__setitem__(
         "provider_surfaces_no_registry_covers", [".github/agents"]),
     expect="covered by no registry")


# --- fail-open cases. Every one of these was green before review: a checker that skips
# --- silently is worse than absent, because it reports the skip as a pass.
case("a surface tree that escapes the repository root",
     lambda r: r["provider_surfaces"]["claude"].__setitem__("tree", "../outside/agents"),
     expect="escapes the repository root")

case("a surface tree given as an absolute path",
     lambda r: r["provider_surfaces"]["claude"].__setitem__("tree", "/etc/agents"),
     expect="absolute")

case("divergent_unreviewed whose bodies are now identical (stage 2 will cause this)",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "divergent_unreviewed"),
                           r["agents"][1].__setitem__("divergence", {"owning_issue": "x"})),
     expect="every body is now identical")

case("skills.json present but unparseable",
     mutate_skills=lambda s: s.__setitem__("__RAW__", "{not json"),
     expect="unparseable")

case("skills.json absent entirely",
     mutate_skills="DELETE", expect="missing")


# --------------------------------------------------------------- the real repository
print()
print("--- the checked-in registry itself ---")

p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(ROOT)],
                   capture_output=True, text=True)
check("the real ops/state/truth/agents.json passes its own checker", p.returncode == 0)

reg = json.loads((ROOT / "ops/state/truth/agents.json").read_text(encoding="utf-8"))
trees = {v["tree"] for v in reg["provider_surfaces"].values()}
on_disk = set()
for t in trees:
    on_disk |= {q.stem for q in (ROOT / t).glob("*.md") if q.name != "README.md"}
check("every agent definition across all %d surfaces has a record (%d names)"
      % (len(trees), len(on_disk)),
      on_disk == {a["name"] for a in reg["agents"]})


print()
if failures:
    print("test_agent_registry_invariants: %d FAILURE(S)" % len(failures))
    for f in failures:
        print("  - " + f)
    sys.exit(1)
print("test_agent_registry_invariants: all %d checks passed" % checks_run)
