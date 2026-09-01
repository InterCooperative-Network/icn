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
import importlib.util
import json
import os
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


def agent_file(text_name, extra=""):
    return "---\nname: %s\ndescription: fixture\n%s---\n\nBody for %s.\n" % (
        text_name, extra, text_name)


def copilot_file(text_name, extra="infer: false\n"):
    """A Copilot definition. `extra` is raw front matter, so a test can express any
    combination of the retired `infer` and its replacement `disable-model-invocation`."""
    return agent_file(text_name, extra)


def base_registry():
    """A minimal, well-formed icn-agents/v2 registry over two surfaces."""
    return {
        "schema": "icn-agents/v2",
        # The surface KEY is an arbitrary label; provider_type is what semantics bind to.
        "provider_surfaces": {
            "claude": {"tree": ".claude/agents", "provider_type": "claude-code"},
            "copilot": {"tree": ".github/agents", "provider_type": "github-copilot"},
        },
        # Must equal the checker's implemented vocabulary exactly, in both directions.
        "relationship_model": {
            "single_surface": "one",
            "exact_mirror": "identical",
            "provider_variant": "deliberate",
            "divergent_unreviewed": "unadjudicated",
        },
        "declared_scope": {"out_of_scope": [], "completeness_claim": "x"},
        "agents": [
            {
                "name": "solo",
                "relationship": "single_surface",
                "routing_triggers": ["solo work"],
                "not_for": [],
                "surfaces": {"claude": {"path": ".claude/agents/solo.md"}},
            },
            {
                "name": "twin",
                "relationship": "exact_mirror",
                "routing_triggers": ["twin work"],
                "not_for": [],
                "surfaces": {
                    "claude": {"path": ".claude/agents/twin.md"},
                    "copilot": {"path": ".github/agents/twin.md",
                                "automatic_invocation": False},
                },
            },
        ],
    }


def base_skills():
    # `enforcement.scan_scope` and both tree lists are REQUIRED: the uncovered-directory
    # claim is proven against the scan scope, so an absent scope would certify the boundary
    # on no evidence (icn#2632 review round 18).
    return {
        "enforcement": {"scan_scope": {"canonical_trees": [], "provider_trees": []}},
        "declared_scope": {
            "cross_registry": {
                "agent_surfaces_tracked_by_agents_json": [".claude/agents", ".github/agents"],
                # REQUIRED as of round 27: a deleted claim must not read as "no known gaps".
                # The fixture declares the empty list because the fixture genuinely has none.
                "known_uncovered_directories": [],
            }
        }
    }


def build(tmp, registry, skills, files):
    root = pathlib.Path(tmp)
    for rel in (".claude/agents", ".github/agents", "ops/state/truth", "scripts/tests"):
        (root / rel).mkdir(parents=True, exist_ok=True)
    (root / "scripts").mkdir(parents=True, exist_ok=True)
    shutil.copy(str(CHECKER), str(root / "scripts/check-agent-registry.py"))
    (root / "not-a-caller.md").write_text("no invocation here\n", encoding="utf-8")
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
    # Provider-specific front matter differs on purpose: `infer` is Copilot-native and has
    # no Claude counterpart. Bodies are what an exact_mirror compares.
    ".github/agents/twin.md": copilot_file("twin"),
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
f[".github/agents/twin.md"] = copilot_file("twin").replace("Body for", "Different body for")
rc, out = run(r, base_skills(), f)
check("an adjudicated provider_variant with differing bodies passes", rc == 0)

# The registry must not demand cross-provider front-matter equivalence: `infer` is
# Copilot-only, `color`/`model` are Claude-only, and an exact_mirror compares BODIES.
r = base_registry()
f = dict(BASE_FILES)
f[".claude/agents/twin.md"] = agent_file("twin", "color: purple\nmodel: opus\ntools: all\n")
f[".github/agents/twin.md"] = copilot_file("twin")
rc, out = run(r, base_skills(), f)
check("exact_mirror holds when only provider-specific front matter differs", rc == 0)

# NOTE: an earlier revision asserted here that a Copilot file declaring no `infer` was
# "unknown", and recorded null. GitHub's reference documentation disproves that -- `infer`
# is retired with default true and `disable-model-invocation` defaults to false, so an
# agent declaring neither IS automatically invocable. That control was removed rather than
# adjusted: it was not a weak assertion, it was a false one. The replacement is the
# `neither key -> provider default` case in the semantics matrix above.


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
     lambda r: r["provider_surfaces"].__setitem__(
         "ghost", {"tree": ".ghost/agents", "provider_type": "claude-code"}),
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
     expect="no record names that exact path")

case("a record points at a file with a different stem",
     mutate_reg=setpath(0, "claude", ".claude/agents/twin.md"),
     expect="two agents wearing one name")

case("front matter declares a different name than the record",
     mutate_files=lambda f: f.__setitem__(".claude/agents/solo.md", agent_file("impostor")),
     expect="front matter declares")

case("two records share one logical name",
     lambda r: r["agents"].append(copy.deepcopy(r["agents"][0])), expect="duplicate record")

case("single_surface declared but two surfaces named",
     lambda r: r["agents"][1].__setitem__("relationship", "single_surface"),
     expect="single_surface but 2")

case("exact_mirror declared but the bodies differ",
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", copilot_file("twin").replace("Body", "Drifted body")),
     expect="bodies differ")

case("a declared mirror_pair whose bodies differ",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "divergent_unreviewed"),
                           r["agents"][1].__setitem__("divergence", {"owning_issue": "x"}),
                           r["agents"][1].__setitem__("mirror_pairs", [["claude", "copilot"]])),
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", copilot_file("twin").replace("Body", "Drifted body")),
     expect="mirror pair")

case("mirror_pairs names a surface the record does not expose",
     lambda r: r["agents"][0].__setitem__("mirror_pairs", [["claude", "copilot"]]),
     expect="does not expose")

case("divergent_unreviewed with no owning issue",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "divergent_unreviewed"),
                           r["agents"][1].__setitem__("divergence", {})),
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", copilot_file("twin").replace("Body", "Other body")),
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

case("known_uncovered_directories names a tracked provider tree",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"].__setitem__(
         "known_uncovered_directories", [".github/agents"]),
     expect="agents.json declares .github/agents as a surface")


# --- provider semantics: effective behaviour, not raw syntax --------------------
# GitHub retired `infer` and replaced it with `disable-model-invocation`
# (docs.github.com/en/copilot/reference/custom-agents-configuration, verified 2026-08-30):
#   infer                     retired, default TRUE
#   disable-model-invocation  default FALSE, takes precedence when both are present
# So these test effective automatic invocation, not one obsolete key.
print()
print("--- provider semantics (effective automatic invocation) ---")

def semantics(label, front, expect_auto):
    """MUST-PASS: the registry claim matching the provider's effective behaviour."""
    r = base_registry()
    r["agents"][1]["surfaces"]["copilot"]["automatic_invocation"] = expect_auto
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = copilot_file("twin", front)
    rc, out = run(r, base_skills(), f)
    check("%s -> automatic_invocation=%s" % (label, expect_auto), rc == 0)

semantics("legacy `infer: true`", "infer: true\n", True)
semantics("legacy `infer: false`", "infer: false\n", False)
semantics("neither key -> provider default", "", True)
semantics("`disable-model-invocation: true`", "disable-model-invocation: true\n", False)
semantics("`disable-model-invocation: false`", "disable-model-invocation: false\n", True)
semantics("PRECEDENCE: dmi:false wins over infer:false",
          "infer: false\ndisable-model-invocation: false\n", True)
semantics("PRECEDENCE: dmi:true wins over infer:true",
          "infer: true\ndisable-model-invocation: true\n", False)

# The registry concept must survive the provider renaming its own key. Both spellings
# below mean "not automatically invocable", so the same registry claim holds for each --
# which is the whole point of projecting semantics instead of mirroring syntax.
for label, front in (("retired spelling", "infer: false\n"),
                     ("current spelling", "disable-model-invocation: true\n")):
    r = base_registry()
    r["agents"][1]["surfaces"]["copilot"]["automatic_invocation"] = False
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = copilot_file("twin", front)
    rc, out = run(r, base_skills(), f)
    check("the registry claim is unchanged by the provider's %s" % label, rc == 0)

print()
case("REAL P2: the registry claim disagrees with effective behaviour",
     mutate_files=lambda f: f.__setitem__(".github/agents/twin.md",
                                          copilot_file("twin", "infer: true\n")),
     expect="registry says automatic_invocation")

case("REAL P2: an unsupported scalar (`infer: flase`) is not false",
     mutate_files=lambda f: f.__setitem__(".github/agents/twin.md",
                                          copilot_file("twin", "infer: flase\n")),
     expect="not a boolean the provider defines")

case("an unsupported scalar in the replacement key either",
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", copilot_file("twin", "disable-model-invocation: yes\n")),
     expect="not a boolean the provider defines")

case("omitted key recorded as manual, when the provider default is automatic",
     mutate_files=lambda f: f.__setitem__(".github/agents/twin.md", copilot_file("twin", "")),
     expect="registry says automatic_invocation")

case("a copilot record that states no automatic_invocation at all",
     lambda r: r["agents"][1]["surfaces"]["copilot"].pop("automatic_invocation"),
     expect="silently unclassified")

case("automatic_invocation recorded on a Claude surface, which projects no semantics",
     lambda r: r["agents"][0]["surfaces"]["claude"].__setitem__(
         "automatic_invocation", True),
     expect="not a semantic provider type")

for k in ("description", "color", "model", "tools", "target",
          "infer", "disable-model-invocation", "user-invocable"):
    case("provider-native %r copied back into the registry" % k,
         (lambda key: (lambda r: r["agents"][1]["surfaces"]["copilot"].__setitem__(key, "x")))(k),
         expect="owned by the provider definition")


# --- round 12: ambiguity, asynchrony, and claims narrowed to what is provable ----
print()
print("--- a semantic cannot come from an ambiguous declaration ---")

# One helper reads every front-matter key this checker consumes, so the uniqueness rule is
# one rule rather than a per-key patch. Repetition is invalid whether the values conflict
# or agree: the provider is free to resolve it differently than we do.
for key, first, second, label in (
        ("infer", "false", "true", "conflicting"),
        ("infer", "false", "false", "identical"),
        ("disable-model-invocation", "true", "false", "conflicting"),
        ("name", "twin", "other", "identity")):
    r = base_registry()
    r["agents"][1]["surfaces"]["copilot"]["automatic_invocation"] = False
    f = dict(BASE_FILES)
    if key == "name":
        fm = "name: %s\nname: %s\ndescription: fixture\ninfer: false\n" % (first, second)
    else:
        fm = ("name: twin\ndescription: fixture\n%s: %s\n%s: %s\n"
              % (key, first, key, second))
    f[".github/agents/twin.md"] = "---\n%s---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    check("REAL P2: duplicate %s (%s) is rejected" % (key, label),
          rc != 0 and ("appears 2 times" in out or "appears 3 times" in out))

r = base_registry()
f = dict(BASE_FILES)
f[".github/agents/twin.md"] = copilot_file("twin", "infer: false\n")
rc, out = run(r, base_skills(), f)
check("MUST-PASS a single occurrence of each key still resolves", rc == 0)


print()

# `2>&1` and `&>` are redirections, not control operators. Every real caller uses the first.
print()
print("--- the gap list claims only what enforcement proves ---")

case("known_uncovered_directories names an escaping path",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"].__setitem__(
         "known_uncovered_directories", ["../outside"]),
     expect="escapes the repository root")

case("the retired provider_surfaces_no_registry_covers name is rejected",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"].__setitem__(
         "provider_surfaces_no_registry_covers", [".claude/commands"]),
     expect="claims a provider classification no owner supplies")

# --- round 11: the executable position, the exit status, and the extension ------
print()


print()

# The checker IS Python's selected program in each of these, so round 10's rule accepts
# them; the exit status never reaches the caller, so the advertised gate cannot fail.
# `2>&1` is a redirection, not a status operator: every real caller uses it.
print()
print("--- a record must point at a file the provider loads ---")

def wrong_extension_case():
    """REAL P2: renaming a definition off .md and updating its record.

    Every per-record check passed -- inside the tree, direct child, exists, stem matches --
    while `definitions_on_disk` inventories only *.md, so the agent left the surface and the
    reverse scan had nothing to miss.
    """
    tmp = tempfile.mkdtemp()
    try:
        root = pathlib.Path(tmp)
        (root / ".claude/agents").mkdir(parents=True)
        (root / "ops/state/truth").mkdir(parents=True)
        (root / "scripts/tests").mkdir(parents=True)
        shutil.copy(str(CHECKER), str(root / "scripts/check-agent-registry.py"))
        (root / "scripts/tests/t.py").write_text(
            'import subprocess\nsubprocess.run(["python3", "scripts/check-agent-registry.py"])\n',
            encoding="utf-8")
        (root / ".claude/agents/solo.txt").write_text(agent_file("solo"), encoding="utf-8")
        reg = {"schema": "icn-agents/v2",
               "provider_surfaces": {"claude": {"tree": ".claude/agents",
                                                "provider_type": "claude-code"}},
               "relationship_model": {"single_surface": "x", "exact_mirror": "x",
                                      "provider_variant": "x", "divergent_unreviewed": "x"},
               "declared_scope": {},
               "agents": [{"name": "solo", "relationship": "single_surface",
                           "routing_triggers": [], "not_for": [],
                           "surfaces": {"claude": {"path": ".claude/agents/solo.txt"}}}]}
        sk = {"declared_scope": {"cross_registry": {
            "agent_surfaces_tracked_by_agents_json": [".claude/agents"]}}}
        (root / "ops/state/truth/agents.json").write_text(json.dumps(reg), encoding="utf-8")
        (root / "ops/state/truth/skills.json").write_text(json.dumps(sk), encoding="utf-8")
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        check("REAL P2: a record path that is not .md is rejected",
              p.returncode != 0 and "does not end in .md" in (p.stdout + p.stderr))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

wrong_extension_case()


# --- round 10: a role is proven by the operation, not by role-shaped tokens ------
print()


print()

# Python's synopsis is `[-c cmd | -m mod | file | -]`. After -c or -m, a later token naming
# the checker is an ARGUMENT to the selected program: the checker never runs.
# Every shape the three real callers actually use.
# --- round 9: mentions, bytes, and metadata that outlives its relationship -------
print()

# MUST-PASS: the shapes the three real callers actually use.
print()
print("--- a mirror claim is a claim about bytes ---")

# `.strip()` erased leading/trailing whitespace before comparison, so a four-space indent --
# which turns the first line into a Markdown code block -- read as byte-identical.
for label, body in (("leading indent", "    Body for twin."),
                    ("trailing whitespace", "Body for twin.   "),
                    ("extra blank line", "Body for twin.\n")):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: fixture\ninfer: false\n---\n\n%s\n" % body)
    rc, out = run(r, base_skills(), f)
    check("REAL P2: exact_mirror differing only by %s is rejected" % label,
          rc != 0 and "bodies differ" in out)


print()
print("--- divergence metadata must not outlive its relationship ---")

case("REAL P2: exact_mirror retaining divergence metadata",
     lambda r: r["agents"][1].__setitem__(
         "divergence", {"adjudicated": True, "why": "different"}),
     expect="divergence metadata is retained")

case("single_surface retaining divergence metadata",
     lambda r: r["agents"][0].__setitem__("divergence", {"owning_issue": "icn#2632"}),
     expect="divergence metadata is retained")


# --- round 8: the provider scalar's type, and claims of role ---------------------
print()
print("--- provider scalar types ---")

# The full matrix the provider actually defines. Quoted forms are YAML STRINGS; accepting
# them would be the checker inventing a type the provider never declared.
for front, expect_auto, label in (
        ("infer: true\n", True, "unquoted true"),
        ("infer: false\n", False, "unquoted false"),
        ("disable-model-invocation: true\n", False, "unquoted dmi true"),
        ("disable-model-invocation: false\n", True, "unquoted dmi false"),
        ("", True, "neither key (provider default)"),
        ("infer: true\ndisable-model-invocation: true\n", False, "precedence: dmi wins"),
        ("infer: false\ndisable-model-invocation: false\n", True, "precedence: dmi wins"),
):
    r = base_registry()
    r["agents"][1]["surfaces"]["copilot"]["automatic_invocation"] = expect_auto
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = copilot_file("twin", front)
    rc, out = run(r, base_skills(), f)
    check("MUST-PASS %s -> automatic_invocation=%s" % (label, expect_auto), rc == 0)

# Both truth values, both quote styles, both keys: a quoted scalar must never be coerced,
# and must fail whichever value the registry happens to claim.
for key in ("infer", "disable-model-invocation"):
    for quoted in ('"true"', "'true'", '"false"', "'false'"):
        for claim in (True, False):
            r = base_registry()
            r["agents"][1]["surfaces"]["copilot"]["automatic_invocation"] = claim
            f = dict(BASE_FILES)
            f[".github/agents/twin.md"] = copilot_file("twin", "%s: %s\n" % (key, quoted))
            rc, out = run(r, base_skills(), f)
            check("REAL P2: %s: %s is a string, not a boolean (registry claims %s)"
                  % (key, quoted, claim),
                  rc != 0 and "quoted string, not a YAML boolean" in out)

for junk in ("yes", "0", "1", "flase", "TRUE", "True"):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = copilot_file("twin", "infer: %s\n" % junk)
    rc, out = run(r, base_skills(), f)
    check("unsupported scalar infer: %s is rejected" % junk,
          rc != 0 and "not a boolean the provider defines" in out)


print()
print("--- one physical tree, one surface ---")

def alias_case():
    """REAL P2: two surface ids over one tree.

    Every file is inventoried twice and exposed under both ids, an exact_mirror compares a
    body with itself, and the cross-registry set collapses the duplicate -- so the registry
    claims a provider surface that does not independently exist.
    """
    r = base_registry()
    r["provider_surfaces"]["claude_alias"] = {"tree": ".claude/agents",
                                              "provider_type": "claude-code"}
    for rec in r["agents"]:
        if "claude" in rec["surfaces"]:
            rec["surfaces"]["claude_alias"] = dict(rec["surfaces"]["claude"])
            if rec["relationship"] == "single_surface":
                rec["relationship"] = "exact_mirror"
    s = base_skills()
    s["declared_scope"]["cross_registry"][
        "agent_surfaces_tracked_by_agents_json"] = [".claude/agents", ".github/agents"]
    rc, out = run(r, s, dict(BASE_FILES))
    check("REAL P2: two surface ids declaring the same tree",
          rc != 0 and "One physical tree is one surface" in out)

alias_case()

case("a trailing-slash spelling of an already-declared tree",
     lambda r: r["provider_surfaces"].__setitem__(
         "dupe", {"tree": ".claude/agents/", "provider_type": "claude-code"}),
     expect="resolves to the same directory")


print()

# --- round 7: claims that pass for the wrong reason -----------------------------
print()
print("--- weak-check hardening ---")

case("REAL P2: divergent_unreviewed that is also adjudicated",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "divergent_unreviewed"),
                           r["agents"][1].__setitem__("divergence",
                               {"owning_issue": "icn#2632", "adjudicated": True})),
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", copilot_file("twin").replace("Body", "Other body")),
     expect="both unreviewed and adjudicated")

case("REAL P2: a mirror pair naming one surface twice",
     lambda r: r["agents"][1].__setitem__("mirror_pairs", [["claude", "claude"]]),
     expect="names one surface twice")

for k in ("routing_triggers", "not_for"):
    case("REAL P2: %s deleted from a record" % k,
         (lambda key: (lambda r: r["agents"][0].pop(key)))(k),
         expect="must be an array of strings")
    case("REAL P2: %s replaced with a scalar" % k,
         (lambda key: (lambda r: r["agents"][0].__setitem__(key, "a string")))(k),
         expect="must be an array of strings")

# MUST-PASS: a real caller still passes.
r = base_registry()
rc, out = run(r, base_skills(), dict(BASE_FILES))
check("a caller that actually runs the checker still passes", rc == 0)


print()

# --- provider identity vs surface label -----------------------------------------
print()
print("--- provider type binds semantics, surface id does not ---")

def relabel(r, old, new):
    """Rename a surface key everywhere, changing nothing else."""
    r["provider_surfaces"][new] = r["provider_surfaces"].pop(old)
    for rec in r["agents"]:
        if old in rec.get("surfaces", {}):
            rec["surfaces"][new] = rec["surfaces"].pop(old)
        if rec.get("mirror_pairs"):
            rec["mirror_pairs"] = [[new if x == old else x for x in p]
                                   for p in rec["mirror_pairs"]]

# MUST-PASS: a harmless relabel must not change what is enforced.
r = base_registry()
relabel(r, "copilot", "github")
rc, out = run(r, base_skills(), dict(BASE_FILES))
check("a surface id renamed copilot->github still passes with semantics intact", rc == 0)

# MUST-FAIL: the same rename must NOT let the semantic disappear.
r = base_registry()
relabel(r, "copilot", "github")
r["agents"][1]["surfaces"]["github"].pop("automatic_invocation")
rc, out = run(r, base_skills(), dict(BASE_FILES))
check("REAL P2: renaming the surface id cannot drop behavioural enforcement",
      rc != 0 and "silently unclassified" in out)

# MUST-PASS: two surfaces may share one provider technology.
r = base_registry()
r["provider_surfaces"]["claude"]["provider_type"] = "claude-code"
r["provider_surfaces"]["second_claude"] = {"tree": ".claude/agents",
                                           "provider_type": "claude-code"}
rc, out = run(r, base_skills(), dict(BASE_FILES))
check("two surfaces may declare the same provider_type (tree reuse aside)",
      "no adapter" not in out)

case("an unknown provider_type",
     lambda r: r["provider_surfaces"]["copilot"].__setitem__("provider_type", "acme-ai"),
     expect="has no adapter")

case("a surface with no provider_type at all",
     lambda r: r["provider_surfaces"]["copilot"].pop("provider_type"),
     expect="provider_type")

case("a semantic recorded for a provider_type that projects none",
     lambda r: r["agents"][0]["surfaces"]["claude"].__setitem__(
         "automatic_invocation", True),
     expect="not a semantic provider type")


# --- structural validation: valid VALUE before true CLAIM -----------------------
print()
print("--- structural validation ---")

for bad, label in ((0, "0"), (1, "1"), ("false", '"false"'), (None, "null"),
                   ([], "[]"), ({}, "{}")):
    case("REAL P2: automatic_invocation is %s, not a boolean" % label,
         (lambda v: (lambda r: r["agents"][1]["surfaces"]["copilot"].__setitem__(
             "automatic_invocation", v)))(bad),
         expect="must be a real JSON boolean")

case("agents is not an array",
     lambda r: r.__setitem__("agents", {"nope": 1}), expect="must be an array")

case("a surface entry is not an object",
     lambda r: r["agents"][0]["surfaces"].__setitem__("claude", "a string"),
     expect="must be an object")

case("a record path is not a string",
     lambda r: r["agents"][0]["surfaces"]["claude"].__setitem__("path", 42),
     expect="path: must be a non-empty string")

case("relationship is not a string",
     lambda r: r["agents"][0].__setitem__("relationship", ["single_surface"]),
     expect="relationship must be a non-empty string")

case("divergence.adjudicated is 1 rather than true",
     lambda r: (r["agents"][1].__setitem__("relationship", "provider_variant"),
                r["agents"][1].__setitem__("divergence", {"adjudicated": 1, "why": "x"})),
     expect="must be a real boolean")

case("a mirror_pairs entry is not two strings",
     lambda r: r["agents"][1].__setitem__("mirror_pairs", [["claude"]]),
     expect="two surface-id strings")

case("cross_registry list contains a non-string",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"].__setitem__(
         "agent_surfaces_tracked_by_agents_json", [".claude/agents", 7]),
     expect="cross_registry")

# A malformed document must produce findings, never a traceback.
r = base_registry()
r["provider_surfaces"] = "not an object"
rc, out = run(r, base_skills(), dict(BASE_FILES))
check("a malformed registry reports findings rather than raising",
      rc != 0 and "Traceback" not in out and "must be an object" in out)


# --- identity is the path, not a derived name ----------------------------------
print()
print("--- path identity ---")

def nested_decoy_case():
    """REAL P2: a record points at a nested file sharing a stem with the real one.

    Every per-record check passes (inside the tree, exists, stem matches, front matter
    matches), and a stem-keyed reverse scan then counts the top-level file as registered --
    so the body the provider actually loads escapes relationship and semantic validation.
    """
    tmp = tempfile.mkdtemp()
    try:
        root = pathlib.Path(tmp)
        (root / ".claude/agents/subdir").mkdir(parents=True)
        (root / "ops/state/truth").mkdir(parents=True)
        (root / ".claude/agents/subdir/solo.md").write_text(agent_file("solo"), encoding="utf-8")
        (root / ".claude/agents/solo.md").write_text(
            agent_file("solo", "color: red\n"), encoding="utf-8")
        reg = {"schema": "icn-agents/v2",
               "provider_surfaces": {"claude": {"tree": ".claude/agents",
                                               "provider_type": "claude-code"}},
               "relationship_model": {"single_surface": "x", "exact_mirror": "x",
                                      "provider_variant": "x", "divergent_unreviewed": "x"},
               "declared_scope": {},
               "agents": [{"name": "solo", "relationship": "single_surface",
                           "routing_triggers": [], "not_for": [],
                           "surfaces": {"claude": {"path": ".claude/agents/subdir/solo.md"}}}]}
        sk = {"declared_scope": {"cross_registry": {
            "agent_surfaces_tracked_by_agents_json": [".claude/agents"]}}}
        (root / "ops/state/truth/agents.json").write_text(json.dumps(reg), encoding="utf-8")
        (root / "ops/state/truth/skills.json").write_text(json.dumps(sk), encoding="utf-8")
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        out = p.stdout + p.stderr
        check("REAL P2: a nested decoy sharing a stem is rejected",
              p.returncode != 0 and "nested below the declared tree" in out)
        check("...and the unregistered top-level file is reported by exact path",
              "no record names that exact path" in out)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

nested_decoy_case()


# --- derived data must not be stored in a truth owner ---------------------------
print()
print("--- stored derived data ---")

case("REAL P2: divergent_unreviewed stores a body_similarity score",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "divergent_unreviewed"),
                           r["agents"][1].__setitem__("divergence",
                               {"owning_issue": "icn#2632", "body_similarity": {"a/b": 0.5}})),
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", copilot_file("twin").replace("Body", "Other body")),
     expect="stored derived data")

case("provider_variant stores a body_similarity score",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "provider_variant"),
                           r["agents"][1].__setitem__("divergence",
                               {"adjudicated": True, "why": "x", "body_similarity": 0.4})),
     mutate_files=lambda f: f.__setitem__(
         ".github/agents/twin.md", copilot_file("twin").replace("Body", "Other body")),
     expect="stored derived data")

case("REAL P2: known_uncovered_directories names a path that does not exist",
     mutate_skills=lambda s: s["declared_scope"]["cross_registry"].__setitem__(
         "known_uncovered_directories", ["tools/does-not-exist/skills"]),
     expect="not an existing directory")


# --- relationship vocabulary must equal enforcement, both directions -------------
print()
print("--- relationship vocabulary vs enforcement ---")

case("REAL P2: provider_variant whose bodies have all become identical",
     mutate_reg=lambda r: (r["agents"][1].__setitem__("relationship", "provider_variant"),
                           r["agents"][1].__setitem__(
                               "divergence", {"adjudicated": True, "why": "claimed"})),
     expect="every body is identical")

case("single_surface record that names two surfaces",
     lambda r: r["agents"][1].__setitem__("relationship", "single_surface"),
     expect="single_surface but 2")


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
# --- the third and last level of the same allowlist ----------------------------
print()
print("--- the document that forbids an unpinned copy must not carry one ---")

# Records were closed, then surface declarations, and the ROOT was still open: a top-level
# `infer: false` in agents.json exited 0. Each level was opened by the previous one being
# closed, which is this checker's recurring shape.
for key, value, label in (
        ("infer", False, "a provider-owned infer at the registry root"),
        ("tools", ["Read"], "provider-owned tools at the registry root"),
        ("agent", [], "a misspelled agents key")):
    def mutate(r, k=key, v=value):
        r[k] = v
    case(label, mutate_reg=mutate, expect="registry root")

# --- a symlink cycle is a finding, not a RuntimeError -------------------------
print()
print("--- a self-referential tree is reported ---")

# `Path.resolve()` raises RuntimeError on a symlink cycle under CPython 3.11 -- the version
# agent-drift-check.yml selects -- and RuntimeError is neither ValueError nor OSError, so a
# committed self-referential tree ended the run with a traceback. Fourth instance of the
# crash-instead-of-report class in this PR; every repository-controlled resolve() is guarded.
def symlink_cycle_case():
    tmp = tempfile.mkdtemp()
    try:
        r = base_registry()
        r["provider_surfaces"]["loop"] = {"tree": "cycle", "provider_type": "claude-code"}
        sk = base_skills()
        sk["declared_scope"]["cross_registry"]["agent_surfaces_tracked_by_agents_json"] = [
            ".claude/agents", ".github/agents", "cycle"]
        root = build(tmp, r, sk, dict(BASE_FILES))
        os.symlink(str(root / "cycle"), str(root / "cycle"))
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        out = p.stdout + p.stderr
        check("REAL P2: a self-referential provider tree is reported, not raised",
              p.returncode != 0 and "Traceback" not in out)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


symlink_cycle_case()

# --- a canonical truth file that is not a file --------------------------------
print()
print("--- an unreadable truth owner is a finding, not a traceback ---")

# Third instance of this class in this PR, after a definition directory and a non-UTF-8
# definition: `read_text()` on a path replaced by a DIRECTORY raises IsADirectoryError, an
# OSError, which the ValueError handler did not catch -- so the run ended before report().
# Both truth owners are covered here rather than only the one that was reported.
def unreadable_truth_owner_case():
    for victim in ("agents.json", "skills.json"):
        tmp = tempfile.mkdtemp()
        try:
            root = build(tmp, base_registry(), base_skills(), dict(BASE_FILES))
            target = root / "ops/state/truth" / victim
            target.unlink()
            target.mkdir()
            p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                               capture_output=True, text=True)
            out = p.stdout + p.stderr
            check("REAL P2: %s replaced by a directory is reported, not raised" % victim,
                  p.returncode != 0 and "Traceback" not in out
                  and ("unparseable" in out.lower() or "UNPARSEABLE" in out))
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


unreadable_truth_owner_case()

# --- the logical name is provider-specific ------------------------------------
print()
print("--- a compound provider suffix is not part of the agent name ---")

# GitHub's current template creates `<name>.agent.md`, and its configuration reference takes
# the config file name "minus `.md` or `.agent.md`" -- so both are loaded. `Path.stem` on
# `x.agent.md` is `x.agent`, so adopting that convention for even one file would have made
# this checker reject a definition the provider loads. The name extraction is per provider now.
def agent_md_suffix_case():
    r = base_registry()
    r["agents"][1]["surfaces"]["copilot"]["path"] = ".github/agents/twin.agent.md"
    f = dict(BASE_FILES)
    f.pop(".github/agents/twin.md")
    f[".github/agents/twin.agent.md"] = copilot_file("twin")
    rc, out = run(r, base_skills(), f)
    check("MUST-PASS a github-copilot definition named <name>.agent.md", rc == 0)

    # ...and the suffix is NOT a universal convention. Claude Code has no `.agent.md` form,
    # so the same filename there yields the logical name `twin.agent` and must be refused.
    r2 = base_registry()
    r2["agents"][1]["surfaces"]["claude"]["path"] = ".claude/agents/twin.agent.md"
    f2 = dict(BASE_FILES)
    f2.pop(".claude/agents/twin.md")
    f2[".claude/agents/twin.agent.md"] = agent_file("twin")
    rc2, out2 = run(r2, base_skills(), f2)
    check("REAL P2: .agent.md is not a claude-code convention",
          rc2 != 0 and "twin.agent" in out2)

    # A genuine name mismatch must still be caught for both providers.
    r3 = base_registry()
    r3["agents"][1]["surfaces"]["copilot"]["path"] = ".github/agents/other.agent.md"
    f3 = dict(BASE_FILES)
    f3.pop(".github/agents/twin.md")
    f3[".github/agents/other.agent.md"] = copilot_file("twin")
    rc3, out3 = run(r3, base_skills(), f3)
    check("REAL P2: a real name mismatch is still caught under the compound suffix", rc3 != 0)


agent_md_suffix_case()

# --- the allowlist one level up, and a file that is not text -------------------
print()
print("--- a surface declaration says WHERE definitions live, never what they say ---")

# The record-level allowlist left `provider_surfaces` open, so a provider-native field could
# sit on the surface DECLARATION instead -- the same unpinned copy PROVIDER_OWNED_KEYS
# forbids, one level up from where the rule was written.
for key, value, label in (
        ("infer", False, "a provider-owned infer on a surface declaration"),
        ("description", "copied", "a provider-owned description on a surface declaration"),
        ("tools", ["Read"], "provider-owned tools on a surface declaration"),
        ("treee", ".github/agents", "a misspelled tree")):
    def mutate(r, k=key, v=value):
        r["provider_surfaces"]["copilot"][k] = v
    case(label, mutate_reg=mutate, expect="provider_surfaces.copilot")


print()
print("--- a file that is not text is a finding, not a traceback ---")

# `read_text()` raised UnicodeDecodeError outside the InvalidDefinition handler, so a
# malformed provider input stopped the gate from reporting anything -- including findings it
# had already collected. Same shape as the directory-named-`twin.md` case.
def non_utf8_case():
    tmp = tempfile.mkdtemp()
    try:
        root = build(tmp, base_registry(), base_skills(), dict(BASE_FILES))
        target = root / ".claude/agents/solo.md"
        target.write_bytes(b"---\nname: solo\ndescription: fixture\n---\n\n\xff\xfe not text\n")
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        out = p.stdout + p.stderr
        check("REAL P2: a non-UTF-8 definition is reported, not raised",
              p.returncode != 0 and "Traceback" not in out
              and "cannot be read as UTF-8" in out)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


non_utf8_case()

# --- a comment on a provider boolean is not part of the value -----------------
print()
print("--- an explanatory comment must not break a valid definition ---")

# The strict boolean comparison read the RAW string, so `infer: false # keep manual` was
# refused -- the required gate red on a valid file for carrying a comment. A false rejection,
# and one the reader/parser agreement rule cannot catch, because the parser's value here is a
# bool rather than a string.
for value, label, should_pass in (
        ("false # keep manual", "commented false", True),
        ("true  # cloud may pick this", "commented true", True),
        ("false", "plain false", True),
        ("true", "plain true", True),
        ('"false"', "quoted false is still unknown, not a boolean", False),
        ("flase # typo", "a commented typo is still unknown", False)):
    r = base_registry()
    # `infer: true` projects to automatic_invocation TRUE; `infer: false` to FALSE. The
    # record must state the projection the definition implies, or the semantic check fails
    # for an unrelated reason and the control proves nothing.
    r["agents"][1]["surfaces"]["copilot"]["automatic_invocation"] = (
        value.split("#")[0].strip() == "true")
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: fixture\ninfer: %s\n---\n\nBody for twin.\n" % value)
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)

# --- parser boundary: YAML validity has an owner now (maintainer decision) ------
print()
print("--- every accumulated malformed case is rejected by the parse-validity owner ---")

# Rounds 20-29 found ten variants of one defect: a hand-written reader accepting or rejecting
# syntax differently from a real parser. The corpus below is those cases. The assertion is no
# longer "our reader has a rule for this example" but "PyYAML rejects it, therefore so do we".
import yaml as _yparse

_MALFORMED = [
    ("tools: value: bad", "round 29: a second mapping separator"),
    ("tools: [Read,, Write]", "an empty flow element"),
    ('tools: ["a" "b"]', "adjacent quoted flow elements"),
    ("tools: [Read", "an unterminated flow sequence"),
    ('tools: "Read', "an unterminated quoted scalar"),
    ('tools: "a" trailing', "trailing content after a closing quote"),
    ("description: |\n  two spaces\n one space", "a block body that dedents"),
    ("tools:\n  - Read\n  orphan: value", "a sequence becoming a mapping"),
    ("tools:\n  - https://example.com\n    child: bad", "a child under a URL scalar item"),
    ("tools:\n  key: value\n    child: bad", "a child under a scalar-valued key"),
    ("metadata:\n  owner: x\n team: y", "a mapping that dedents mid-mapping"),
]
for extra, label in _MALFORMED:
    fm = "name: twin\ndescription: fixture\n%s" % extra
    # The corpus is only meaningful if PyYAML really rejects these. Assert that first, or the
    # control below proves nothing.
    try:
        _yparse.safe_load(fm)
        really_bad = False
    except Exception:
        really_bad = True
    check("corpus case is genuinely invalid YAML: %s" % label, really_bad)
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s\ninfer: false\n---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    check("REAL P2: %s is rejected" % label, rc != 0 and "Traceback" not in out)


print()
print("--- the parser check is load-bearing, not decorative ---")

# MUTATION CONTROL. If YAML validation is removed or neutered, the corpus above must stop
# being rejected. Without this, a future edit could delete the parse step and every test here
# would still pass on the ICN rules alone -- the check would have skipped itself.
def parser_bypass_control():
    import importlib.util as _il
    spec = _il.spec_from_file_location("bypassed", str(CHECKER))
    mod = _il.module_from_spec(spec)
    spec.loader.exec_module(mod)

    class _NeuteredYaml:
        YAMLError = mod.yaml.YAMLError

        @staticmethod
        def safe_load(_text):
            return {"name": "twin", "description": "fixture"}   # never raises

    mod.yaml = _NeuteredYaml
    survived = []
    for extra, label in _MALFORMED:
        text = "---\nname: twin\ndescription: fixture\n%s\ninfer: false\n---\n\nBody.\n" % extra
        try:
            mod.parse_registered_agent_front_matter(text, "github-copilot")
            survived.append(label)          # accepted with the parser neutered
        except mod.InvalidDefinition:
            # Still refused with the parser neutered, which means an ICN rule caught it --
            # the agreement rule and the duplicate-key rule both still run. That is a pass
            # for this control: what it measures is the cases that ONLY the parser catches,
            # counted in `survived` above.
            pass
    # Some cases are ALSO caught by ICN rules (the reader/parser agreement rule still runs
    # against the neutered loader). The control is that neutering the parser changes the
    # outcome for at least one case -- i.e. the parser is doing work nothing else does.
    check("neutering the YAML parser lets malformed front matter through (%d/%d cases)"
          % (len(survived), len(_MALFORMED)), len(survived) > 0)


parser_bypass_control()

# --- round 28: a nonempty element is not a valid one; a colon is not a separator -
print()
print("--- flow elements and mapping separators ---")

try:
    import yaml as _y28
except ImportError:
    _y28 = None

_B28 = "name: twin\ndescription: fixture\n"
for fm, label, should_pass in (
        # Nonempty by the comma test, and a loader still raises: adjacent quoted scalars need
        # a comma. The element goes through the SAME quoted-scalar reader used everywhere else.
        ('fixture: ["a" "b"]', "adjacent quoted flow elements", False),
        ('fixture: ["a"x]', "trailing junk after a quoted element", False),
        ('fixture: ["a", "b"]', "a well-formed quoted flow sequence", True),
        ("fixture: [a, b]", "a plain flow sequence", True),
        # A MAPPING SEPARATOR is `: ` or a trailing `:`. Mere colon presence marked a URL as a
        # mapping item able to own a child.
        ("tools:\n  - https://example.com\n    child: bad",
         "a child under a URL-valued scalar sequence item", False),
        ("tools:\n  - https://example.com", "a URL as a scalar sequence item", True),
        ("homepage: https://example.com", "a URL as a mapping value", True),
        ("tools:\n  - name: a\n    extra: b", "a real mapping sequence item", True),
        ("tools:\n  key:\n    child: ok", "a child under an empty-valued key", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s%s\ninfer: false\n---\n\nBody for twin.\n" % (_B28, fm)
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if _y28 is not None:
        try:
            _y28.safe_load("%s%s\ninfer: false" % (_B28, fm))
            loadable = True
        except Exception:
            loadable = False
        check("   ...and PyYAML agrees (%s)" % ("loads" if should_pass else "rejects"),
              loadable == should_pass)

# --- round 27: every inline value, and a claim that must be present ------------
print()
print("--- quoted well-formedness applies to every value, not only required ones ---")

# Optional fields were not value-checked at all, so `tools: "Read` was certified while a YAML
# loader raises ScannerError. The check that can apply to ALL values is narrower than the one
# for required string fields: quote closes, nothing follows. `infer: false` must stay legal.
try:
    import yaml as _y27
except ImportError:
    _y27 = None

for value, label, should_pass in (
        ('"Read', "an unterminated double-quoted optional value", False),
        ("'Read", "an unterminated single-quoted optional value", False),
        ('"a" trailing', "trailing content after a closing quote", False),
        ('"Read"', "a well-formed double-quoted optional value", True),
        ("'Read'", "a well-formed single-quoted optional value", True),
        ("'it''s'", "a doubled quote inside a single-quoted value", True),
        ('"a#b"', "a hash inside a quoted optional value", True),
        ('"a\\"b"', "an escaped quote inside a quoted value", True),
        # The narrow boundary: NON-STRING plain scalars stay legal in an optional field.
        # `infer: false` is real provider syntax, so the required-field type rule must not
        # leak out here.
        ("false", "a boolean optional value", True),
        ("0x10", "a hexadecimal optional value", True),
        ("[Read, Write]", "a flow optional value", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: fixture\ntools: %s\ninfer: false\n---\n\nBody for twin.\n"
        % value)
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if should_pass and _y27 is not None:
        try:
            _y27.safe_load("name: twin\ndescription: fixture\ntools: %s\ninfer: false" % value)
            loadable = True
        except Exception:
            loadable = False
        check("   ...and a real YAML parser loads it", loadable)


print()
print("--- a deleted coverage claim is not a claim of no gaps ---")

# `or []` read a DELETED known_uncovered_directories as "no known gaps" and the boundary gate
# went green, while the checked-in uncovered directories still sit there unscanned. The
# sibling claim beside it was already required for exactly this reason.
case("known_uncovered_directories deleted entirely",
     mutate_skills=lambda sk: sk["declared_scope"]["cross_registry"].pop(
         "known_uncovered_directories", None),
     expect="must not read as 'there are none'")

# --- round 26: balance is not syntax, and a second reader must be as careful ----
print()
print("--- a flow collection must have the shape a loader accepts ---")

# Balance proved only that brackets matched: `tools: [Read,, Write]` is balanced and PyYAML
# still raises. The fix is NOT to parse flow properly -- it is to support the shape the
# repository writes and refuse everything else by name.
try:
    import yaml as _y26
except ImportError:
    _y26 = None

_B26 = "name: twin\ndescription: fixture\n"
for value, label, should_pass in (
        ("[Read,, Write]", "an empty element in a flow sequence", False),
        ("{a: 1,, b: 2}", "an empty element in a flow mapping", False),
        ("[,]", "a flow sequence that is only a comma", False),
        ("[,a]", "a leading empty element", False),
        ("[a,,]", "a doubled trailing comma", False),
        # A single trailing comma IS legal YAML, and the empty collection is legal too.
        ("[Read,]", "a single trailing comma", True),
        ("[Read, ]", "a trailing comma with a space", True),
        ("[]", "the empty flow sequence", True),
        ("{}", "the empty flow mapping", True),
        ("[ ]", "an empty flow sequence with a space", True),
        # Element CONTENT is still not interpreted: a comma or a hash inside a quoted string
        # is data, and a plain scalar with a space is one element.
        ('["a,b"]', "a comma inside a quoted element", True),
        ('["a#b"]', "a hash inside a quoted element", True),
        ("[a b, c]", "a plain element containing a space", True),
        ("['it''s']", "a doubled quote inside a single-quoted element", True),
        ('["Read", "Grep", "Glob", "Bash"]', "the shape two real definitions use", True),
        # DELIBERATELY NARROWER THAN YAML, which is why this block asserts the oracle in one
        # direction only. Nested flow LOADS fine and no checked-in definition writes it;
        # supporting it means parsing flow properly, which is the whole language arriving one
        # counterexample at a time. Refused by name, and recorded as a narrowing rather than
        # a soundness claim.
        # BROADENED in the parser-boundary change: these are valid YAML, and `tools` is not
        # a key ICN reads, so there is no value for a line reader to misread. The narrowing's
        # only rationale was that the hand-written reader could not parse flow; the parser
        # owns that now, so keeping the refusal would be ceremony.
        ("[{a: [1, 2]}]", "a nested flow collection", True),
        ("[[a], b]", "a nested flow sequence", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\n%stools: %s\ninfer: false\n---\n\nBody for twin.\n" % (_B26, value))
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if _y26 is not None:
        try:
            _y26.safe_load("%stools: %s\ninfer: false" % (_B26, value))
            loadable = True
        except Exception:
            loadable = False
        # One direction only: what the reader ACCEPTS must load. The reverse does not hold --
        # nested flow is legal YAML and deliberately refused.
        if should_pass:
            check("   ...and a real YAML parser loads it", loadable)


print()
print("--- a second reader of a path must be as careful as the first ---")

# The per-surface loop reports a DIRECTORY named `twin.md` as a topology error and moves on.
# The relationship validator then called `bodies()`, `read_text` raised IsADirectoryError, and
# the checker TERMINATED before printing the findings it had already collected. The first
# reader's correct finding never reached anyone.
def mirror_directory_case():
    tmp = tempfile.mkdtemp()
    try:
        root = build(tmp, base_registry(), base_skills(), dict(BASE_FILES))
        target = root / ".github/agents/twin.md"       # a MIRROR surface, so bodies() runs
        target.unlink()
        target.mkdir()
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        out = p.stdout + p.stderr
        check("REAL P2: a mirror record pointing at a directory reports, not crashes",
              p.returncode != 0 and "Traceback" not in out)
        check("   ...and the per-surface finding actually reaches the report",
              "not a regular file" in out)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


mirror_directory_case()

# --- round 25: which items own children, and flow that must balance ------------
print()
print("--- a sequence item owns a child only if it carries a mapping ---")

try:
    import yaml as _y25
except ImportError:
    _y25 = None

_B25 = "name: twin\ndescription: fixture\n"
for fm, label, should_pass in (
        # Round 24 marked EVERY sequence item an opener. `- Read` is a scalar item.
        (_B25 + "tools:\n  - Read\n    child: bad",
         "a child under a scalar sequence item", False),
        # ...but `- name: a` then a deeper `extra: b` is one mapping with two keys, and the
        # child belongs to the ITEM, not to `a`. Refusing it would be a false rejection.
        (_B25 + "tools:\n  - name: a\n    extra: b",
         "a second key under a mapping sequence item", True),
        (_B25 + "tools:\n  -\n    name: a", "a child under an empty sequence item", True),
        (_B25 + "tools:\n  - |\n    body", "a block scalar as a sequence item", True),
        # Flow syntax is VALIDATED, not refused: two checked-in definitions really write
        # `tools: ["Read", "Grep", "Glob", "Bash"]`, so refusing it would take the gate red
        # on the repository it guards. What one line can prove is balance and termination.
        (_B25 + "tools: [Read", "an unterminated flow sequence", False),
        (_B25 + "metadata: {a: 1", "an unterminated flow mapping", False),
        (_B25 + "tools: [Read}", "a mismatched flow close", False),
        (_B25 + 'tools: ["Read]', "an unterminated string inside flow", False),
        (_B25 + 'tools: ["Read", "Grep", "Glob", "Bash"]',
         "the flow sequence two real definitions use", True),
        (_B25 + "metadata: {a: 1, b: 2}", "a flow mapping", True),
        (_B25 + 'tools: ["a#b"]', "a hash inside a flow string", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s\ninfer: false\n---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if _y25 is not None:
        try:
            _y25.safe_load(fm + "\ninfer: false")
            loadable = True
        except Exception:
            loadable = False
        # Both directions hold for THIS block: every case here is one PyYAML and the narrow
        # reader should agree on, so the oracle is asserted both ways rather than one.
        check("   ...and PyYAML agrees (%s)" % ("loads" if should_pass else "rejects"),
              loadable == should_pass)

# --- round 24: what may own a child, and what is not a file --------------------
print()
print("--- a nested entry may own a child only if its value can hold one ---")

# Round 23 checked that siblings agree on kind and opened a deeper frame for anything.
# `tools:` / `  key: value` / `    child: bad` is "mapping values are not allowed here" to a
# YAML parser, and was certified.
try:
    import yaml as _y24
except ImportError:
    _y24 = None

_B24 = "name: twin\ndescription: fixture\n"
for fm, label, should_pass in (
        (_B24 + "tools:\n  key: value\n    child: bad",
         "a mapping nested under an entry whose value is a scalar", False),
        (_B24 + "tools:\n  key:\n    child: ok",
         "a mapping nested under an empty-valued entry", True),
        (_B24 + "tools:\n  - name: a\n    extra: b",
         "a second key inside a sequence item", True),
        (_B24 + "a:\n  b:\n    c: 1", "three levels of empty-valued keys", True),
        # `in_block` only ever tracked the ROOT key, so a NESTED block scalar's body was read
        # as structure. Refusing it would have been a false rejection of ordinary YAML, and
        # the naive version of this fix did exactly that.
        (_B24 + "tools:\n  key: |\n    body", "a nested block scalar body", True),
        (_B24 + "tools:\n  key: |\n    - not a list",
         "a nested block body that looks like a sequence", True),
        (_B24 + "tools:\n  key: |\n    body\n  other: x",
         "a nested block followed by a sibling key", True),
        (_B24 + "tools:\n  key: | # why\n    body",
         "a nested block whose indicator carries a comment", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s\ninfer: false\n---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if should_pass and _y24 is not None:
        try:
            _y24.safe_load(fm + "\ninfer: false")
            loadable = True
        except Exception:
            loadable = False
        check("   ...and a real YAML parser loads it", loadable)


print()
print("--- a definition is a FILE, and a checker that crashes reports nothing ---")

# A DIRECTORY named `solo.md` matches `*.md`, so the inventory listed it as a definition and
# the read raised IsADirectoryError -- a traceback out of the canonical gate, which is the
# one outcome worse than a wrong answer.
def md_directory_case():
    tmp = tempfile.mkdtemp()
    try:
        root = build(tmp, base_registry(), base_skills(), dict(BASE_FILES))
        target = root / ".claude/agents/solo.md"
        target.unlink()
        target.mkdir()
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        out = p.stdout + p.stderr
        check("REAL P2: a directory named solo.md is a finding, not a traceback",
              p.returncode != 0 and "not a regular file" in out and "Traceback" not in out)

        # ...and it must not be inventoried as a definition either, or the reverse scan would
        # report an unregistered agent that does not exist.
        check("   ...and it is not counted as an unregistered definition",
              "unregistered" not in out.lower())
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


md_directory_case()

# --- round 23: equal indentation is not equal structure ------------------------
print()
print("--- a nested collection may not change kind at its own level ---")

# Round 22 validated nested INDENTATION and stopped there, so `tools:` then `  - Read` then
# `  orphan: value` shares one indent and was certified -- a block sequence does not become a
# mapping at its own level, and PyYAML raises. Each depth remembers which kind opened it.
try:
    import yaml as _y23
except ImportError:
    _y23 = None

_B23 = "name: twin\ndescription: fixture\n"
for fm, label, should_pass in (
        (_B23 + "tools:\n  - Read\n  orphan: value",
         "a sequence that becomes a mapping at its own level", False),
        (_B23 + "metadata:\n  owner: x\n  - Read",
         "a mapping that becomes a sequence at its own level", False),
        (_B23 + "metadata:\n  owner:\n    - a\n    b: c",
         "a kind switch one level deeper", False),
        (_B23 + "tools:\n  - Read\n  - Write", "a level sequence", True),
        (_B23 + "tools:\n  - name: a\n  - name: b", "a sequence of mappings", True),
        (_B23 + "metadata:\n  owner:\n    name: x\n  team: y",
         "a nested mapping followed by a sibling key", True),
        # A `#` line settles nothing about structure inside a nested collection -- but IS
        # content inside a block scalar. Reading it as an entry rejected a commented sequence.
        (_B23 + "tools:\n  - Read\n  # note\n  - Write", "a commented sequence", True),
        (_B23 + "metadata:\n  owner: x\n  # note\n  team: y", "a commented mapping", True),
        (_B23 + "summary: |\n  # not a comment\n  more",
         "a block scalar whose content starts with a hash", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s\ninfer: false\n---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if should_pass and _y23 is not None:
        try:
            _y23.safe_load(fm + "\ninfer: false")
            loadable = True
        except Exception:
            loadable = False
        check("   ...and a real YAML parser loads it", loadable)


print()
print("--- escape syntax is refused, not silently mis-read ---")

# A YAML loader turns `"t\x77in"` into `twin`; the decoder only removed the surrounding
# quotes, so it compared text the provider never sees and reported drift against a definition
# whose identity matches. Decoding YAML's escape table would be machinery for a spelling
# nothing uses -- not one of the 43 checked-in definitions is quoted at all.
# The MESSAGE is the assertion here, not merely the rejection. Every one of these was already
# refused before the fix -- as a DRIFT report, claiming the provider answers to a name it does
# not, which is a misleading failure on a valid definition. Asserting "rejected" would have
# passed against the old checker and proved nothing.
for spelling, label, expect in (
        # Now caught by the general reader/parser agreement rule rather than by a
        # spelling-specific refusal: the decoder does not decode escapes, so it and the
        # parser disagree about the value, which is the fact that actually matters.
        ('"t\\x77in"', "a double-quoted name with a hex escape", "read differently"),
        ('"tw\\u0069n"', "a double-quoted name with a unicode escape", "read differently"),
        ("'it''s'", "a single-quoted name with a doubled quote", "read differently"),
        ('"twin"', "a plainly double-quoted name", None),
        ("twin", "a plain name", None)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: %s\ndescription: fixture\ninfer: false\n---\n\nBody for twin.\n" % spelling)
    rc, out = run(r, base_skills(), f)
    if expect is None:
        check("MUST-PASS %s" % label, rc == 0)
    else:
        check("REAL P2: %s is refused AS unsupported syntax" % label,
              rc != 0 and expect in out and "Traceback" not in out)


print()
print("--- a gap claim must not depend on machine-local state ---")

# `is_dir()` FOLLOWS a symlink, so an uncovered directory could point outside the repository
# and every check passed -- the same defect as a registered definition symlinked out of its
# tree, in the other registry's claim.
def escaping_gap_case():
    tmp = tempfile.mkdtemp()
    outside = tempfile.mkdtemp()
    try:
        sk = base_skills()
        sk["declared_scope"]["cross_registry"]["known_uncovered_directories"] = ["external-gap"]
        root = build(tmp, base_registry(), sk, dict(BASE_FILES))
        os.symlink(outside, str(root / "external-gap"))
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        out = p.stdout + p.stderr
        check("REAL P2: an uncovered directory symlinked outside the repository is rejected",
              p.returncode != 0 and "resolves outside the repository" in out)

        # ...and one that stays inside is still a legitimate gap, so this is containment
        # rather than a ban on symlinks.
        (root / "real-gap").mkdir()
        (root / "external-gap").unlink()
        os.symlink(str(root / "real-gap"), str(root / "external-gap"))
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        check("MUST-PASS an uncovered directory symlinked WITHIN the repository",
              p.returncode == 0)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        shutil.rmtree(outside, ignore_errors=True)


escaping_gap_case()

# --- round 22: the other half of every rule already written --------------------
print()
print("--- indentation is validated for BOTH things that open it ---")

# Round 21 validated a block scalar's body indentation and left the other opener alone:
# `tools:` then `  - Read` then ` - Write` was certified while the YAML loader raised
# ParserError. One rule now, applied wherever a nested value opens.
try:
    import yaml as _y22
except ImportError:
    _y22 = None

_B22 = "name: twin\ndescription: fixture\n"
for fm, label, should_pass in (
        (_B22 + "tools:\n  - Read\n - Write", "a list that dedents mid-list", False),
        (_B22 + "metadata:\n  owner: x\n team: y", "a mapping that dedents mid-mapping", False),
        (_B22 + "tools:\n  - Read\n  - Write", "a level list", True),
        (_B22 + "metadata:\n  owner:\n    name: x", "a mapping nesting deeper", True),
        (_B22 + "metadata:\n  owner: x\n  # a real comment\n  team: y",
         "a comment inside a nested mapping", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s\ninfer: false\n---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if should_pass and _y22 is not None:
        try:
            _y22.safe_load(fm + "\ninfer: false")
            loadable = True
        except Exception:
            loadable = False
        check("   ...and a real YAML parser loads it", loadable)


print()
print("--- a coverage claim is contradicted by ANCESTRY, not only by equality ---")

# `.agents/skills` is a canonical scan tree whose direct children another registry
# inventories, so declaring `.agents/skills/foo` "covered by no registry" makes the two
# canonical registries contradict each other -- and an equality test saw two different
# strings and reported clean.
# The directory is MADE TO EXIST in every case. Without that, the old checker rejected these
# through its phantom-path branch and the control would have proved only that a message
# changed -- the finding is the contradiction, not the missing directory.
for un, tree_list, label, fragment in (
        (".github/agents/nested", None, "a directory INSIDE a declared surface",
         "declares .github/agents as a surface"),
        (".claude/agents/nested", None, "a directory inside the other surface",
         "declares .claude/agents as a surface"),
        (".claude/agents/deep/deeper", None, "a directory nested two levels inside a surface",
         "declares .claude/agents as a surface"),
        (".github/agents/nested", "canonical_trees", "a directory inside a canonical scan tree",
         "scan_scope already covers it through .github/agents"),
        (".github/agents/nested", "provider_trees", "a directory inside a provider scan tree",
         "scan_scope already covers it through .github/agents")):
    def mutate(sk, u=un, tl=tree_list):
        sk["declared_scope"]["cross_registry"]["known_uncovered_directories"] = [u]
        if tl:
            # Take it out of the SURFACE list so the scan-scope branch is the one reached.
            sk["declared_scope"]["cross_registry"][
                "agent_surfaces_tracked_by_agents_json"] = [".claude/agents"]
            sk["enforcement"]["scan_scope"][tl] = [".github/agents"]

    def add_dir(f, u=un):
        f[u + "/keep.txt"] = "not a definition\n"

    if tree_list:
        # That surface must also leave the registry, or its own record fails first.
        def drop_surface(r):
            r["provider_surfaces"].pop("copilot")
            r["agents"][1]["surfaces"].pop("copilot")
            r["agents"][1]["relationship"] = "single_surface"
        case("known_uncovered_directories names %s" % label, mutate_reg=drop_surface,
             mutate_skills=mutate, mutate_files=add_dir, expect=fragment)
    else:
        case("known_uncovered_directories names %s" % label,
             mutate_skills=mutate, mutate_files=add_dir, expect=fragment)


print()
print("--- a registered definition must RESOLVE inside its surface tree ---")

# The parent, suffix, stem, existence and front-matter checks all FOLLOW a symlink, so a
# direct child of a valid tree could be a link to a file outside the repository and every
# one of them passed. Such a definition is machine-local: it can change with no commit.
def escaping_definition_case():
    tmp = tempfile.mkdtemp()
    outside = tempfile.mkdtemp()
    try:
        root = build(tmp, base_registry(), base_skills(), dict(BASE_FILES))
        target = pathlib.Path(outside) / "solo.md"
        target.write_text(agent_file("solo"), encoding="utf-8")
        link = root / ".claude/agents/solo.md"
        link.unlink()
        os.symlink(str(target), str(link))
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        out = p.stdout + p.stderr
        check("REAL P2: a definition symlinked outside the repository is rejected",
              p.returncode != 0 and "outside the surface tree" in out)

        # ...and a link that stays INSIDE the tree is still a valid definition, so the check
        # is containment rather than a ban on symlinks.
        link.unlink()
        inside = root / ".claude/agents/real-solo.txt"
        inside.write_text(agent_file("solo"), encoding="utf-8")
        os.symlink(str(inside), str(link))
        p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                           capture_output=True, text=True)
        check("MUST-PASS a definition symlinked WITHIN its own tree", p.returncode == 0)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        shutil.rmtree(outside, ignore_errors=True)


escaping_definition_case()

# --- round 21: the block's own indentation, and the comment after a quote -------
print()
print("--- a block scalar's indentation is part of its validity ---")

# `opens_nested` accepted EVERY indented line once a block scalar opened, so a body that
# dedents mid-block was certified while the YAML loader raised ParserError -- the provider
# cannot load a definition the canonical registry called valid.
try:
    import yaml as _y21
except ImportError:
    _y21 = None

_B21 = "name: twin\ninfer: false\n"
for fm, label, should_pass in (
        (_B21 + "description: |\n  two spaces\n one space",
         "a block body that dedents mid-block", False),
        # A `#` line INSIDE a block scalar is CONTENT. Filtering it as a comment would have
        # dropped the indentation sample that makes the dedent below detectable.
        (_B21 + "description: |\n  # content, not a comment\n one space",
         "a dedent below a comment-looking block line", False),
        (_B21 + "description: |2\n   body",
         "an explicit block indentation indicator", False),
        (_B21 + "description: |\n  two spaces\n    four spaces",
         "a block body that indents further", True),
        (_B21 + "description: >-\n  folded body", "a chomping modifier", True),
        # A `#` after the CLOSING quote is a comment; one inside is data. Returning the whole
        # string made `description: "fixture" # rationale` look unterminated, so a definition
        # a YAML loader accepts was refused -- the required workflow red on valid input.
        (_B21 + 'description: "fixture" # rationale', "a comment after a quoted scalar", True),
        (_B21 + "description: 'fixture' # rationale",
         "a comment after a single-quoted scalar", True),
        (_B21 + 'description: "ticket #123"', "a hash inside a quoted scalar", True),
        (_B21 + 'description: "fixture', "a genuinely unterminated quote", False)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s\n---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    if _y21 is not None:
        try:
            _y21.safe_load(fm)
            loadable = True
        except Exception:
            loadable = False
        # Only one direction is asserted: what the registry ACCEPTS must load. The subset is
        # narrower than YAML on purpose, so a rejection proves nothing about the loader.
        if should_pass:
            check("   ...and a real YAML parser loads it", loadable)

# --- round 20: an indented line is not automatically nested content -------------
print()
print("--- indentation is supported only where YAML introduces it ---")

# Every indented line was skipped as "nested content below a validated root key", so a key
# whose value is already a scalar could be followed by an indented mapping -- which a YAML
# parser rejects outright. The provider cannot load the definition at all, and the canonical
# registry certified it. Verified against PyYAML where it is importable.
try:
    import yaml as _yaml
except ImportError:
    _yaml = None

for fm, label, should_pass in (
        ("name: twin\ndescription: fixture\ninfer: false\n  orphan: value",
         "an orphan mapping indented under a scalar", False),
        ("name: twin\ndescription: fixture\ninfer: false\n  orphan",
         "an orphan line indented under a scalar", False),
        ("name: twin\ndescription: a long\n  continued description\ninfer: false",
         "a multi-line plain scalar continuation", False),
        # The two forms YAML actually uses to introduce indentation, and the flat case.
        ("name: twin\ndescription: |\n  Real content.\ninfer: false",
         "a block scalar body", True),
        ("name: twin\ndescription: | # why\n  Real content.\ninfer: false",
         "a block scalar body under a commented indicator", True),
        ("name: twin\ndescription: fixture\nmetadata:\n  owner: x\ninfer: false",
         "a nested mapping under an empty value", True),
        ("name: twin\ndescription: fixture\ntools:\n  - Read\ninfer: false",
         "a list under an empty value", True),
        ("name: twin\ndescription: fixture\ninfer: false", "flat front matter", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = "---\n%s\n---\n\nBody for twin.\n" % fm
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)
    # A definition the registry ACCEPTS must be one the provider's parser can load. The
    # converse is deliberately not asserted: the supported subset is narrower than YAML.
    if should_pass and _yaml is not None:
        try:
            _yaml.safe_load(fm)
            loadable = True
        except Exception:
            loadable = False
        check("   ...and a real YAML parser loads it", loadable)

# --- round 19: type by allowlist, compare by value, close the record ------------
print()
print("--- a plain scalar is a string only when it provably is ---")

# The type test named the null/boolean words and matched a DECIMAL regex, so every other
# non-string YAML literal was certified as a provider-required string. Naming them one at a
# time is the losing side: each miss is silent. The rule is an allowlist now -- starts with an
# ASCII letter, and is not one of the closed words -- so these fail without being enumerated.
for value, label in (("0x10", "hexadecimal integer"),
                     ("0o17", "octal integer"),
                     ("0b1010", "binary integer"),
                     ("1_000", "underscored integer"),
                     (".inf", "positive infinity"),
                     ("-.Inf", "signed infinity"),
                     (".nan", "not-a-number"),
                     ("2026-01-01", "date"),
                     ("2026-01-01T00:00:00Z", "timestamp"),
                     ("12:30:45", "YAML 1.1 sexagesimal"),
                     ("y", "YAML 1.1 single-letter true"),
                     ("N", "YAML 1.1 single-letter false")):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: %s\ninfer: false\n---\n\nBody for twin.\n" % value)
    rc, out = run(r, base_skills(), f)
    check("REAL P2: description: %s (%s) is not a string" % (value, label),
          rc != 0 and "Traceback" not in out)

# The allowlist must still admit what the 43 checked-in definitions actually write, or the
# gate is red on the repository it guards.
for value, label in (("Reviews PRs for ICN invariants.", "ordinary prose"),
                     ('"a quoted description"', "double-quoted"),
                     ("'a quoted description'", "single-quoted"),
                     ("fixture # trailing note", "plain value with an inline comment")):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: %s\ninfer: false\n---\n\nBody for twin.\n" % value)
    rc, out = run(r, base_skills(), f)
    check("MUST-PASS description as %s" % label, rc == 0)


print()
print("--- identity is compared by VALUE, not by spelling ---")

# The required-field check accepted a quoted name and an inline comment on one; the identity
# comparison then matched the RAW text against the registered name and reported drift for a
# definition the provider resolves correctly. Two functions, two opinions, one false
# rejection. Both read the same decoder now.
for spelling, label, should_pass in (
        ('twin', "plain name", True),
        ('"twin"', "double-quoted name", True),
        ("'twin'", "single-quoted name", True),
        ('twin # canonical', "name with an inline comment", True),
        ('"other"', "quoted name that does NOT match the record", False),
        ('other # canonical', "commented name that does NOT match the record", False),
        ('| # folded\n  twin', "block-scalar name", False)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: %s\ndescription: fixture\ninfer: false\n---\n\nBody for twin.\n"
        % spelling)
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)


print()
print("--- the record vocabulary is closed ---")

# The provider-owned check read each SURFACE entry, so a provider-native field written at
# RECORD level left the registry carrying exactly the unpinned copy that check exists to
# forbid. And `mirror_pairs` is the one optional record key, so a misspelling of it drops the
# only promise the record was making while every required key would have failed as absent.
for key, value, label in (
        ("description", "a copied description", "a record-level provider-owned description"),
        ("model", "opus", "a record-level provider-owned model"),
        ("tools", ["Read"], "record-level provider-owned tools"),
        ("mirror_pair", [["claude", "copilot"]], "a misspelled mirror_pairs"),
        ("surface", {}, "a misspelled surfaces")):
    def mutate(r, k=key, v=value):
        r["agents"][1][k] = v
    case(label, mutate_reg=mutate)

# --- round 18: guards must not crash ahead of the validator that reports ------
print()
print("--- structural validation runs before any semantic access ---")

for value, label in ((1, "an integer"), ("a string", "a string"),
                     ([], "an array"), (None, "null")):
    r = base_registry()
    r["declared_scope"] = value
    rc, out = run(r, base_skills(), dict(BASE_FILES))
    check("REAL P2: declared_scope as %s -> finding, not crash" % label,
          rc != 0 and "Traceback" not in out and "must be an object" in out)


print()
print("--- the cross-registry boundary needs its parents to exist ---")

# child_obj substituted {} for an ABSENT parent, leaving scan_trees empty -- so the
# uncovered-directory claim was certified against no evidence at all.
for mutate, label in (
        (lambda s: s.pop("enforcement", None), "enforcement deleted"),
        (lambda s: s["enforcement"].pop("scan_scope", None), "scan_scope deleted"),
        (lambda s: s["enforcement"]["scan_scope"].pop("provider_trees", None),
         "provider_trees deleted"),
        (lambda s: s["enforcement"]["scan_scope"].pop("canonical_trees", None),
         "canonical_trees deleted")):
    r = base_registry()
    sk = base_skills()
    sk["enforcement"] = {"scan_scope": {"canonical_trees": [], "provider_trees": []}}
    mutate(sk)
    rc, out = run(r, sk, dict(BASE_FILES))
    check("REAL P2: %s is a finding, not silence" % label,
          rc != 0 and "Traceback" not in out)


print()
print("--- a commented block indicator must not raise ---")

# Comment stripping turned `description: | # rationale` into `|`, and the block body was
# then located by searching for a line ENDING in the stripped indicator. Nothing matched,
# so a valid definition was rejected with StopIteration.
for desc, label, should_pass in (
        ("| # rationale\n  Real content.", "literal block with an inline comment", True),
        ("> # note\n  Folded content.", "folded block with an inline comment", True),
        ("| # only a comment", "indicator plus comment, no content", False),
        ("|", "bare indicator, no content", False),
        ("|\n  plain block content", "plain block scalar", True)):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: %s\ninfer: false\n---\n\nBody for twin.\n" % desc)
    rc, out = run(r, base_skills(), f)
    ok = (rc == 0) if should_pass else (rc != 0 and "Traceback" not in out)
    check("%s %s" % ("MUST-PASS" if should_pass else "REAL P2:", label), ok)


# --- round 17: the level above the one already typed ---------------------------
print()
print("--- an inline comment is not part of the value ---")

# Round 16 compared the whole scalar against the YAML non-string literals, so a trailing
# comment defeated the membership test and the fallback accepted it.
for value, label in (("null # why", "null with a comment"),
                     ("~ # why", "~ with a comment"),
                     ("true  # why", "a boolean with a comment"),
                     ("123 # why", "a number with a comment"),
                     ("# only a comment", "nothing but a comment")):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: %s\ninfer: false\n---\n\nBody for twin.\n" % value)
    rc, out = run(r, base_skills(), f)
    check("REAL P2: description as %s is rejected" % label,
          rc != 0 and ("nonempty string" in out or "only a comment" in out))

# A `#` inside a quoted scalar is literal, and a plain value keeps its text before any
# comment -- both remain valid nonempty strings.
for value, label in ('a real description # trailing note', "plain value with a trailing comment"), \
                    ('"quoted # hash"', "a quoted value containing a hash"):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: %s\ninfer: false\n---\n\nBody for twin.\n" % value)
    rc, out = run(r, base_skills(), f)
    check("MUST-PASS %s" % label, rc == 0)


print()
print("--- malformed cross-registry PARENTS are typed too ---")

# Round 16 typed the cross-registry lists and left the objects holding them untyped, so the
# same non-totality survived one level up.
for mutate, label in (
        (lambda s: s.__setitem__("enforcement", "a string"), "enforcement as a string"),
        (lambda s: s.__setitem__("enforcement", None), "enforcement as null"),
        (lambda s: s.__setitem__("declared_scope", "a string"), "declared_scope as a string"),
        (lambda s: s["declared_scope"].__setitem__("cross_registry", [1]),
         "cross_registry as an array")):
    r = base_registry()
    sk = base_skills()
    sk.setdefault("enforcement", {})
    mutate(sk)
    rc, out = run(r, sk, dict(BASE_FILES))
    check("REAL P2: %s -> finding, not crash" % label,
          rc != 0 and "Traceback" not in out)


# --- round 16: prove types, physical identity, and totality --------------------
print()
print("--- a required string field must actually be a string ---")

# `description: null` is nonempty SOURCE TEXT, which is all a presence check proved.
for value, label in (("null", "the null literal"), ("~", "the ~ null literal"),
                     ("[]", "an empty flow sequence"), ("{}", "an empty flow mapping"),
                     ("true", "a boolean"), ("yes", "a YAML 1.1 boolean"),
                     ("123", "an integer"), ("0.5", "a float"),
                     ('""', "an empty double-quoted string"),
                     ("''", "an empty single-quoted string")):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: %s\ninfer: false\n---\n\nBody for twin.\n" % value)
    rc, out = run(r, base_skills(), f)
    check("REAL P2: required description as %s is rejected" % label,
          rc != 0 and "nonempty string" in out)

# The value forms the 43 real definitions use, plus the quoted spelling.
for value, label in (("a plain description", "plain scalar"),
                     ('"a quoted description"', "quoted scalar"),
                     (">\n  a folded\n  description", "block scalar")):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: %s\ninfer: false\n---\n\nBody for twin.\n" % value)
    rc, out = run(r, base_skills(), f)
    check("MUST-PASS description as a %s" % label, rc == 0)

r = base_registry()
f = dict(BASE_FILES)
f[".github/agents/twin.md"] = (
    "---\nname: twin\ndescription: >\ninfer: false\n---\n\nBody for twin.\n")
rc, out = run(r, base_skills(), f)
check("a block scalar with no content is rejected", rc != 0 and "no content" in out)


print()
print("--- provider trees are PHYSICALLY distinct and inside the repo ---")

def symlink_cases():
    """Lexical uniqueness is not filesystem uniqueness.

    A second tree that is a symlink to the first is lexically distinct, so both ids
    inventory the same files and a mirror comparison becomes a self-comparison. The same
    mechanism catches a `..`-free path that leaves the repo through a symlink.
    """
    tmp = tempfile.mkdtemp()
    try:
        root = pathlib.Path(tmp)
        (root / ".claude/agents").mkdir(parents=True)
        (root / "ops/state/truth").mkdir(parents=True)
        (root / ".claude/agents/solo.md").write_text(
            agent_file("solo", "description: fixture\n"), encoding="utf-8")
        os.symlink(".claude/agents", str(root / "alias-agents"))
        outside = tempfile.mkdtemp()
        os.symlink(outside, str(root / "escape-tree"))

        def run_reg(surfaces, agents, claimed):
            reg = {"schema": "icn-agents/v2", "provider_surfaces": surfaces,
                   "relationship_model": {"single_surface": "x", "exact_mirror": "x",
                                          "provider_variant": "x",
                                          "divergent_unreviewed": "x"},
                   "declared_scope": {}, "agents": agents}
            sk = {"declared_scope": {"cross_registry": {
                "agent_surfaces_tracked_by_agents_json": claimed}}}
            (root / "ops/state/truth/agents.json").write_text(json.dumps(reg), encoding="utf-8")
            (root / "ops/state/truth/skills.json").write_text(json.dumps(sk), encoding="utf-8")
            p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                               capture_output=True, text=True)
            return p.returncode, p.stdout + p.stderr

        rc, out = run_reg(
            {"claude": {"tree": ".claude/agents", "provider_type": "claude-code"},
             "alias": {"tree": "alias-agents", "provider_type": "claude-code"}},
            [{"name": "solo", "relationship": "exact_mirror",
              "routing_triggers": [], "not_for": [],
              "surfaces": {"claude": {"path": ".claude/agents/solo.md"},
                           "alias": {"path": "alias-agents/solo.md"}}}],
            [".claude/agents", "alias-agents"])
        check("REAL P2: a symlinked second surface tree is rejected",
              rc != 0 and "resolves to the same directory" in out)

        rc, out = run_reg(
            {"claude": {"tree": ".claude/agents", "provider_type": "claude-code"},
             "esc": {"tree": "escape-tree", "provider_type": "claude-code"}},
            [{"name": "solo", "relationship": "single_surface",
              "routing_triggers": [], "not_for": [],
              "surfaces": {"claude": {"path": ".claude/agents/solo.md"}}}],
            [".claude/agents", "escape-tree"])
        check("a tree leaving the repo through a symlink is rejected",
              rc != 0 and "outside the repository" in out)
        shutil.rmtree(outside, ignore_errors=True)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

symlink_cases()


print()
print("--- malformed cross-registry data produces findings, not tracebacks ---")

for value, label in (([1], "an integer element"), ([None], "a null element"),
                     ([{}], "an object element"), ([""], "an empty-string element"),
                     ("not-an-array", "a bare string")):
    r = base_registry()
    sk = base_skills()
    sk["declared_scope"]["cross_registry"]["known_uncovered_directories"] = value
    rc, out = run(r, sk, dict(BASE_FILES))
    check("REAL P2: known_uncovered_directories with %s -> finding, not crash" % label,
          rc != 0 and "Traceback" not in out and "array of nonempty strings" in out)


# --- round 15: a definition must be PROVEN valid before it is registered --------
print()
print("--- registration requires a valid definition, not just a filename ---")

def definition_case(label, front_matter_text, expect_fragment, provider="copilot"):
    """Replace one surface file's whole front matter and assert the registry refuses it."""
    r = base_registry()
    f = dict(BASE_FILES)
    path = ".github/agents/twin.md" if provider == "copilot" else ".claude/agents/twin.md"
    f[path] = front_matter_text
    rc, out = run(r, base_skills(), f)
    check("REAL P2: %s" % label, rc != 0 and expect_fragment in out)

definition_case("a registered definition with no front matter at all",
                "Body for twin.\n", "no well-formed front matter")
definition_case("an opening delimiter with no closing delimiter",
                "---\nname: twin\ndescription: fixture\ninfer: false\n\nBody for twin.\n",
                "no well-formed front matter")
definition_case("an empty front-matter block",
                "---\n\n# only a comment\n---\n\nBody for twin.\n", "empty")
definition_case("a fully indented root mapping",
                "---\n  name: twin\n  description: fixture\n  infer: false\n---\n\n"
                "Body for twin.\n", "root mapping is indented")
definition_case("a github-copilot definition missing description",
                "---\nname: twin\ninfer: false\n---\n\nBody for twin.\n",
                "requires 'description'")
definition_case("a claude-code definition missing description",
                "---\nname: twin\n---\n\nBody for twin.\n",
                "requires 'description'", provider="claude")
definition_case("a claude-code definition missing name",
                "---\ndescription: fixture\n---\n\nBody for twin.\n",
                "requires 'name'", provider="claude")

# The indented-root case is the one that changed a SEMANTIC: a YAML parser reads
# `infer: false`, the old reader saw no key and derived the default `true`.
r = base_registry()
r["agents"][1]["surfaces"]["copilot"]["automatic_invocation"] = True
f = dict(BASE_FILES)
f[".github/agents/twin.md"] = (
    "---\n  name: twin\n  description: fixture\n  infer: false\n---\n\nBody for twin.\n")
rc, out = run(r, base_skills(), f)
check("REAL P2: an indented root cannot silently flip automatic_invocation",
      rc != 0 and "root mapping is indented" in out)

# MUST-PASS: everything the 43 real definitions actually use stays legal.
r = base_registry()
f = dict(BASE_FILES)
f[".github/agents/twin.md"] = (
    "---\n"
    "name: twin\n"
    "description: >\n"
    "  a folded scalar\n"
    "  across lines\n"
    "metadata:\n"
    "  nested: value\n"
    "tools:\n"
    "  - one\n"
    "\n"
    "# a comment\n"
    "infer: false\n"
    "---\n\nBody for twin.\n")
rc, out = run(r, base_skills(), f)
check("MUST-PASS nested maps, folded scalars, lists, blanks and comments", rc == 0)


# --- round 14: the front-matter contract is a verifiable subset of YAML ---------
print()
print("--- unsupported top-level key syntax fails loudly ---")

# Every one of these is valid YAML the provider honours, and every one was read as the key
# being ABSENT -- so a definition could change name/infer/disable-model-invocation while the
# registry certified the value it thought was there.
for spelling, label in (
        ("'name': masquerade", "quoted 'name'"),
        ('"name": masquerade', 'quoted "name"'),
        ("'infer': true", "quoted 'infer'"),
        ('"infer": true', 'quoted "infer"'),
        ("'disable-model-invocation': true", "quoted 'disable-model-invocation'"),
        ('"disable-model-invocation": true', 'quoted "disable-model-invocation"'),
        ("? name", "explicit key (? name)"),
        ("name : masquerade", "space before the colon"),
        ("{name: masquerade}", "flow mapping")):
    r = base_registry()
    f = dict(BASE_FILES)
    f[".github/agents/twin.md"] = (
        "---\nname: twin\ndescription: fixture\ninfer: false\n%s\n---\n\nBody for twin.\n"
        % spelling)
    rc, out = run(r, base_skills(), f)
    # Either owner may speak first, and both establish the same fact: the key was NOT read
    # as absent. A root-level flow mapping is not valid YAML in this position, so the parser
    # rejects it before the ICN plain-key rule is reached; the quoted and explicit-key
    # spellings ARE valid YAML and reach the ICN rule.
    check("REAL P2: %s is unsupported, not absent" % label,
          rc != 0 and ("plain unquoted top-level keys" in out
                       or "not valid YAML" in out))

# The subset must not reject what the 43 real definitions actually use: indented
# continuations, block scalars and nested values all sit below a validated top-level key.
r = base_registry()
f = dict(BASE_FILES)
f[".github/agents/twin.md"] = (
    "---\n"
    "name: twin\n"
    "description: >\n"
    "  a folded scalar\n"
    "  spanning lines\n"
    "tools:\n"
    "  - one\n"
    "  - two\n"
    "# a comment\n"
    "infer: false\n"
    "---\n\nBody for twin.\n")
rc, out = run(r, base_skills(), f)
check("MUST-PASS folded scalars, lists and comments still parse", rc == 0)


# --- round 13: two deleted claims must not come back --------------------------
print()
print("--- removed self-description stays removed ---")

case("REAL P2: a re-added enforcement block",
     lambda r: r.__setitem__("enforcement", {
         "checker": "scripts/check-agent-registry.py",
         "invoked_by": ["ops/scripts/drift-check.sh"]}),
     expect="was removed in icn#2632 review round 13")

case("REAL P2: a re-added declared_scope.in_scope glob list",
     lambda r: r["declared_scope"].__setitem__("in_scope", [".claude/agents/*.md"]),
     expect="provider_surfaces is the one machine-readable owner")


print()
print("--- the checked-in registry itself ---")

p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(ROOT)],
                   capture_output=True, text=True)
check("the real ops/state/truth/agents.json passes its own checker", p.returncode == 0)

reg = json.loads((ROOT / "ops/state/truth/agents.json").read_text(encoding="utf-8"))
trees = {v["tree"] for v in reg["provider_surfaces"].values()}
on_disk = set()
# PROVIDER-SPECIFIC, exactly like the checker. `Path.stem` on `foo.agent.md` is `foo.agent`,
# so adopting the Copilot filename convention the checker supports would have left the CHECKER
# green and turned this completeness assertion -- and therefore the required workflow -- red.
# The suite and the thing it tests have to derive a name the same way.
_spec = importlib.util.spec_from_file_location("registry_under_test", str(CHECKER))
_checker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_checker)
for _sid, _sdef in reg["provider_surfaces"].items():
    _pt, _tree = _sdef["provider_type"], _sdef["tree"]
    on_disk |= {n for n in (_checker.logical_name_of(q, _pt)
                            for q in (ROOT / _tree).glob("*.md") if q.name != "README.md")
                if n is not None}
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
