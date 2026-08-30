#!/usr/bin/env python3
"""Make ops/state/truth/agents.json mechanically true about every provider surface.

The v1 registry recorded 17 paths, all under `.claude/agents/`, and claimed to be "all
available agents". `.github/agents/` held 21 GitHub Copilot definitions and the
icn-agent-pack plugin held 6 more, none of them mentioned. Nothing failed, because the
only checks that existed walked a single tree in a single direction.

Two rules keep the registry honest, and both are enforced here rather than asserted:

  OWNERSHIP.   The registry owns logical identity, surface topology, relationships, and
               semantic projections that are organizationally meaningful. A provider
               definition owns its own native syntax. A value lives in both places only
               when the checker pins the registry's claim to the provider file -- an
               enforced projection is not a second owner; an unenforced copy is.

  SEMANTICS.   No schema concept exists without executable enforcement. Every relationship
               type must have a validator in RELATIONSHIP_VALIDATORS, and the registry's
               declared vocabulary must equal the implemented one in BOTH directions, so a
               truth-owner edit cannot invent an unchecked relationship class.

Provider syntax is translated, never mirrored:

    provider syntax  ->  provider adapter  ->  stable registry claim

`automatic_invocation` is the projection that matters organizationally: can this surface
cause the agent to be selected WITHOUT an explicit invocation? For GitHub Copilot that is
derived from live provider rules (see COPILOT_AUTOMATIC_INVOCATION), not from any single
key -- `infer` is retired and `disable-model-invocation` replaced it, so a registry that
stored raw `infer` would have been pinned to obsolete syntax.

Scope: inventory, provider topology and behaviour projections. Prompt CONTENT is icn#2632
stage 2 and is not checked here.

Run: python3 scripts/check-agent-registry.py [--verbose]
"""
import argparse
import json
import pathlib
import re
import sys

REGISTRY = "ops/state/truth/agents.json"
SKILLS = "ops/state/truth/skills.json"

# Provider-native values the ownership model assigns to the provider definition. The
# registry must not carry a second, unpinned copy of any of them.
PROVIDER_OWNED_KEYS = ("description", "color", "model", "tools", "target",
                       "mcp-servers", "metadata", "infer",
                       "disable-model-invocation", "user-invocable")

# Verified against https://docs.github.com/en/copilot/reference/custom-agents-configuration
# on 2026-08-30:
#   infer                      RETIRED. "Enables Copilot cloud agent to automatically use
#                              this custom agent based on task context." Default: true.
#   disable-model-invocation   Replaces it. "Disables Copilot cloud agent from
#                              automatically using this custom agent based on task
#                              context." Default: false. Takes precedence over `infer`.
# So an agent that declares neither key IS automatically invocable. Recording the absence
# as "unknown" would be truthful about syntax and false about behaviour.
COPILOT_AUTOMATIC_INVOCATION = (
    "disable-model-invocation when present, else retired `infer`, else true "
    "(docs.github.com/en/copilot/reference/custom-agents-configuration, verified 2026-08-30)")


class InvalidProviderBoolean(Exception):
    """A provider key is present with a value the provider does not define."""


def split_front_matter(text):
    m = re.match(r"^---\n(.*?)\n---\n(.*)$", text, re.S)
    return (m.group(1), m.group(2)) if m else (None, text)


def front_matter_value(block, key):
    if not block:
        return None
    m = re.search(r"^%s:\s*(.*)$" % re.escape(key), block, re.M)
    return m.group(1).strip() if m else None


def strict_bool(block, key):
    """True / False / None-when-absent. Any other present value is an error.

    Coercing an unrecognised scalar would let the registry certify provider behaviour the
    provider does not define: `infer: flase` is not false, it is unknown.
    """
    raw = front_matter_value(block, key)
    if raw is None:
        return None
    v = raw.strip().strip('"').strip("'")
    if v == "true":
        return True
    if v == "false":
        return False
    raise InvalidProviderBoolean("%s: %r is not a boolean the provider defines" % (key, raw))


def copilot_automatic_invocation(block):
    """Effective automatic-invocation for a Copilot agent, per current provider rules."""
    dmi = strict_bool(block, "disable-model-invocation")
    if dmi is not None:
        return not dmi
    legacy = strict_bool(block, "infer")
    if legacy is not None:
        return legacy
    return True


# Which surfaces project which semantics. A surface absent from this map makes no
# behavioural claim, and the registry must not record one for it.
SURFACE_SEMANTICS = {
    "copilot": {"automatic_invocation": copilot_automatic_invocation},
}


class Checker:
    def __init__(self, root, verbose=False):
        self.root = pathlib.Path(root)
        self.verbose = verbose
        self.errors = []
        self.notes = []

    def fail(self, msg):
        self.errors.append(msg)

    def ok(self, msg):
        if self.verbose:
            self.notes.append(msg)

    def definitions_on_disk(self, tree):
        d = self.root / tree
        if not d.is_dir():
            return None
        return {p.stem: p for p in sorted(d.glob("*.md")) if p.name != "README.md"}

    def bodies(self, rec):
        out = {}
        for sid, entry in (rec.get("surfaces") or {}).items():
            fp = self.root / (entry or {}).get("path", "")
            if fp.exists():
                _, body = split_front_matter(fp.read_text(encoding="utf-8"))
                out[sid] = body.strip()
        return out

    # ---- relationship validators. One per declared type; see SEMANTICS above. -------

    def _v_single_surface(self, rec):
        n = len(rec.get("surfaces") or {})
        if n != 1:
            self.fail("%s: relationship single_surface but %d surfaces are named"
                      % (rec["name"], n))

    def _v_exact_mirror(self, rec):
        b = self.bodies(rec)
        if len(rec.get("surfaces") or {}) < 2:
            self.fail("%s: exact_mirror claims a cross-surface relationship but names one "
                      "surface" % rec["name"])
            return
        if len(set(b.values())) > 1:
            self.fail("%s: declared exact_mirror but the bodies differ (%s). A mirror that "
                      "drifts must fail, not silently become a variant."
                      % (rec["name"], " vs ".join(sorted(b))))
        else:
            self.ok("%s: exact_mirror verified byte-identical" % rec["name"])

    def _v_provider_variant(self, rec):
        div = rec.get("divergence") or {}
        if len(rec.get("surfaces") or {}) < 2:
            self.fail("%s: provider_variant claims a cross-surface relationship but names "
                      "one surface" % rec["name"])
            return
        if not div.get("adjudicated"):
            self.fail("%s: provider_variant asserts the divergence is deliberate and requires "
                      "divergence.adjudicated = true." % rec["name"])
        if not div.get("why"):
            self.fail("%s: provider_variant requires divergence.why." % rec["name"])
        b = self.bodies(rec)
        if len(b) > 1 and len(set(b.values())) == 1:
            self.fail("%s: declared provider_variant but every body is identical. An "
                      "adjudicated intent does not keep a claim true -- if stage 2 converged "
                      "these, the record is now exact_mirror." % rec["name"])

    def _v_divergent_unreviewed(self, rec):
        div = rec.get("divergence") or {}
        if len(rec.get("surfaces") or {}) < 2:
            self.fail("%s: divergent_unreviewed claims a cross-surface relationship but names "
                      "one surface" % rec["name"])
            return
        if not div.get("owning_issue"):
            self.fail("%s: divergent_unreviewed requires divergence.owning_issue so the debt "
                      "has an owner." % rec["name"])
        b = self.bodies(rec)
        if len(b) > 1 and len(set(b.values())) == 1:
            self.fail("%s: declared divergent_unreviewed but every body is now identical. "
                      "Promote it to exact_mirror -- a resolved divergence must not keep "
                      "claiming to be one." % rec["name"])

    @property
    def RELATIONSHIP_VALIDATORS(self):
        return {
            "single_surface": self._v_single_surface,
            "exact_mirror": self._v_exact_mirror,
            "provider_variant": self._v_provider_variant,
            "divergent_unreviewed": self._v_divergent_unreviewed,
        }

    # -------------------------------------------------------------------------------

    def run(self):
        reg_path = self.root / REGISTRY
        if not reg_path.exists():
            print("MISSING: %s" % REGISTRY)
            return 1
        try:
            reg = json.loads(reg_path.read_text(encoding="utf-8"))
        except ValueError as exc:
            print("UNPARSEABLE: %s (%s)" % (REGISTRY, exc))
            return 1

        if reg.get("schema") != "icn-agents/v2":
            self.fail("schema: expected icn-agents/v2, found %r. This checker enforces the "
                      "provider-surface model; a v1 file cannot express it."
                      % (reg.get("schema"),))
            return self.report()

        surfaces = reg.get("provider_surfaces") or {}
        if not surfaces:
            self.fail("provider_surfaces: missing or empty -- nothing declares which trees "
                      "hold agent definitions, so completeness cannot mean anything.")
            return self.report()

        validators = self.RELATIONSHIP_VALIDATORS
        declared_rels = set(reg.get("relationship_model") or {})
        implemented = set(validators)
        for extra in sorted(declared_rels - implemented):
            self.fail("relationship_model declares %r but no validator implements it. A "
                      "relationship type exists only if executable semantics exist for it, "
                      "or a truth-owner edit could create an unchecked class." % extra)
        for missing in sorted(implemented - declared_rels):
            self.fail("the checker implements relationship %r but relationship_model does not "
                      "declare it. The declared and enforced vocabularies must agree."
                      % missing)
        if declared_rels == implemented:
            self.ok("relationship vocabulary agrees with enforcement (%d types)"
                    % len(implemented))

        # ---- surface trees --------------------------------------------------
        disk = {}
        for sid, sdef in sorted(surfaces.items()):
            tree = (sdef or {}).get("tree")
            if not tree:
                self.fail("provider_surfaces.%s: no tree declared" % sid)
                continue
            if tree.startswith("/") or ".." in pathlib.PurePosixPath(tree).parts:
                self.fail("provider_surfaces.%s: tree %s is absolute or escapes the repository "
                          "root. A surface must be inside the repo or the checker validates "
                          "something this repo does not own." % (sid, tree))
                continue
            found = self.definitions_on_disk(tree)
            if found is None:
                self.fail("provider_surfaces.%s: declared tree %s does not exist" % (sid, tree))
                continue
            disk[sid] = found
            self.ok("surface %s -> %s (%d definitions)" % (sid, tree, len(found)))

        # ---- records --------------------------------------------------------
        seen = set()
        registered = {sid: set() for sid in disk}
        for rec in reg.get("agents") or []:
            name = rec.get("name")
            if not name:
                self.fail("a record has no name: %r" % (rec,))
                continue
            if name in seen:
                self.fail("%s: duplicate record. One logical agent, one record -- two records "
                          "sharing a name is exactly the masquerade this registry prevents."
                          % name)
                continue
            seen.add(name)

            rec_surfaces = rec.get("surfaces") or {}
            if not rec_surfaces:
                self.fail("%s: no surfaces. A record that names no surface describes nothing."
                          % name)
                continue

            for sid, entry in sorted(rec_surfaces.items()):
                if sid not in surfaces:
                    self.fail("%s: surface %r is not a declared provider_surface" % (name, sid))
                    continue
                path = (entry or {}).get("path")
                if not path:
                    self.fail("%s.%s: no path" % (name, sid))
                    continue
                tree = surfaces[sid]["tree"]
                if not path.startswith(tree + "/"):
                    self.fail("%s.%s: path %s is not inside the declared tree %s"
                              % (name, sid, path, tree))
                    continue
                if ".." in pathlib.PurePosixPath(path).parts:
                    self.fail("%s.%s: path %s escapes the repository root" % (name, sid, path))
                    continue
                fp = self.root / path
                if not fp.exists():
                    self.fail("%s.%s -> %s (missing)" % (name, sid, path))
                    continue
                if fp.stem != name:
                    self.fail("%s.%s: file is %s.md. A record must not point at a file with a "
                              "different name -- that is two agents wearing one name."
                              % (name, sid, fp.stem))
                    continue

                fm, _ = split_front_matter(fp.read_text(encoding="utf-8"))
                declared = front_matter_value(fm, "name")
                if declared is not None and declared != name:
                    self.fail("%s.%s: front matter declares name: %s. The provider loads the "
                              "front-matter name, so the registry would route to a name the "
                              "provider does not answer to." % (name, sid, declared))

                for k in PROVIDER_OWNED_KEYS:
                    if k in (entry or {}):
                        self.fail("%s.%s: %r is provider-native syntax owned by the provider "
                                  "definition, not the registry. The registry records derived "
                                  "SEMANTICS, never mirrored syntax -- read it from %s."
                                  % (name, sid, k, path))

                self.check_semantics(name, sid, entry, fm, path)
                registered.setdefault(sid, set()).add(name)
                self.ok("%s.%s -> %s" % (name, sid, path))

            # mirror pairs: an enforced promise between two surfaces, independent of the
            # record-level relationship.
            for pair in rec.get("mirror_pairs") or []:
                if not (isinstance(pair, list) and len(pair) == 2):
                    self.fail("%s: mirror_pairs entries must name exactly two surfaces, found "
                              "%r" % (name, pair))
                    continue
                missing = [x for x in pair if x not in rec_surfaces]
                if missing:
                    self.fail("%s: mirror_pairs names surface(s) %s that this record does not "
                              "expose" % (name, ", ".join(missing)))
                    continue
                b = self.bodies(rec)
                a, c = pair
                if a in b and c in b and b[a] != b[c]:
                    self.fail("%s: %s and %s are declared a mirror pair but their bodies "
                              "differ. The claim is enforced, so drift fails here rather than "
                              "quietly becoming a variant." % (name, a, c))
                elif a in b and c in b:
                    self.ok("%s: mirror pair %s/%s verified byte-identical" % (name, a, c))

            rel = rec.get("relationship")
            if rel not in validators:
                self.fail("%s: relationship %r has no enforcement semantics. Valid types are "
                          "%s." % (name, rel, ", ".join(sorted(validators))))
                continue
            validators[rel](rec)

        # ---- completeness, the other direction ------------------------------
        for sid, found in sorted(disk.items()):
            for stem in sorted(found):
                if stem not in registered.get(sid, set()):
                    self.fail("%s/%s.md exists but no record names it on surface %r. An "
                              "unregistered provider agent must not be able to appear silently."
                              % (surfaces[sid]["tree"], stem, sid))

        self.check_cross_registry(surfaces)
        return self.report()

    def check_semantics(self, name, sid, entry, fm, path):
        """Registry semantic projections must equal what the provider file actually means."""
        spec = SURFACE_SEMANTICS.get(sid, {})
        for field, derive in sorted(spec.items()):
            if field not in (entry or {}):
                self.fail("%s.%s: the registry owns %r for this surface and this record does "
                          "not state it. An agent the provider may select unbidden must not be "
                          "silently unclassified." % (name, sid, field))
                continue
            try:
                actual = derive(fm)
            except InvalidProviderBoolean as exc:
                self.fail("%s.%s: %s (%s). An unsupported scalar is unknown, not false -- the "
                          "registry must not certify provider behaviour the provider does not "
                          "define." % (name, sid, exc, path))
                continue
            if entry[field] != actual:
                self.fail("%s.%s: registry says %s=%r, %s means %r. Rule: %s"
                          % (name, sid, field, entry[field], path, actual,
                             COPILOT_AUTOMATIC_INVOCATION if field == "automatic_invocation"
                             else "provider adapter"))
            else:
                self.ok("%s.%s: %s=%r matches the provider file" % (name, sid, field, actual))
        for field in (entry or {}):
            if field == "path":
                continue
            if field not in spec:
                self.fail("%s.%s: %r is not a semantic this surface projects. Surfaces declare "
                          "their semantics in SURFACE_SEMANTICS; recording one here asserts "
                          "behaviour that provider does not have." % (name, sid, field))

    def check_cross_registry(self, surfaces):
        sk = self.root / SKILLS
        if not sk.exists():
            self.fail("%s is missing. The cross-registry boundary between the two registries "
                      "cannot be verified without it." % SKILLS)
            return
        try:
            sj = json.loads(sk.read_text(encoding="utf-8"))
        except ValueError as exc:
            self.fail("skills.json is unparseable (%s). It is a registered truth owner and "
                      "holds the structured cross-registry contract, so skipping the boundary "
                      "check on a parse error would be fail-open exactly where this checker is "
                      "supposed to fail closed." % exc)
            return
        trees = {v["tree"] for v in surfaces.values() if v.get("tree")}
        cross = (sj.get("declared_scope", {}) or {}).get("cross_registry") or {}
        claimed = cross.get("agent_surfaces_tracked_by_agents_json")
        if claimed is None:
            self.fail("skills.json declared_scope.cross_registry."
                      "agent_surfaces_tracked_by_agents_json is missing. The cross-registry "
                      "claim must be structured data: a checker cannot tell 'X is tracked' "
                      "from 'X is NOT tracked' by reading prose, which is how the two "
                      "registries contradicted each other unnoticed.")
            return
        claimed = set(claimed)
        for extra in sorted(claimed - trees):
            self.fail("skills.json says agents.json tracks %s, but agents.json declares no "
                      "such provider surface." % extra)
        for missing in sorted(trees - claimed):
            self.fail("agents.json declares provider surface %s, but skills.json's "
                      "cross_registry list omits it. Both registries must agree on the "
                      "boundary between them." % missing)
        if claimed == trees:
            self.ok("skills.json and agents.json agree on all %d agent surfaces" % len(trees))
        for un in cross.get("provider_surfaces_no_registry_covers") or []:
            if un in trees:
                self.fail("skills.json lists %s as covered by no registry, but agents.json "
                          "declares it as a surface." % un)

    def report(self):
        for n in self.notes:
            print("  ok   %s" % n)
        if self.errors:
            print("check-agent-registry: %d PROBLEM(S)" % len(self.errors))
            for e in self.errors:
                print("  - %s" % e)
            return 1
        print("check-agent-registry: clean")
        return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo-root", default=None,
                    help="repository root to check (default: the root containing this script)")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()
    root = (pathlib.Path(args.repo_root).resolve() if args.repo_root
            else pathlib.Path(__file__).resolve().parents[1])
    return Checker(root, args.verbose).run()


if __name__ == "__main__":
    sys.exit(main())
