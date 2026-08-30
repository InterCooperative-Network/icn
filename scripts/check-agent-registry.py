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
import difflib
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


# --- one reusable type mechanism ------------------------------------------------------
# Deliberately the same shape as scripts/check-merge-policy-schema.py, which learned this in
# icn#2651/#2658: `isinstance(False, int)` is True, so a JSON number satisfied a boolean check
# and `0 == False` compared equal. This checker repeated that defect -- a stored
# `automatic_invocation: 0` compared equal to a derived `False` and reported clean.
def as_obj(v):
    return (True, v) if isinstance(v, dict) else (False, None)


def as_str(v):
    return (True, v) if isinstance(v, str) and v else (False, None)


def as_exact_bool(v):
    return (True, v) if type(v) is bool else (False, None)


def as_str_list(v):
    if not isinstance(v, list):
        return (False, None)
    return (True, v) if all(isinstance(x, str) and x for x in v) else (False, None)


ALL_SEMANTIC_FIELDS = frozenset()   # rebound below, once the adapters are defined.


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


# Semantics belong to the PROVIDER TECHNOLOGY, not to the registry's label for a surface.
# Dispatching on the surface ID meant renaming `copilot` to `github` -- a harmless relabel --
# silently dropped every behavioural check, because an unknown ID projected nothing.
#
# A provider type that genuinely projects no organizational behaviour maps to an explicit
# empty dict. That is deliberate representation, not absence from a table: adding a new
# provider type without deciding what it projects fails rather than defaulting to silence.
#
# Several surfaces may share one provider type -- `.claude/agents` and the icn-agent-pack
# plugin tree are both loaded by Claude Code -- so this is keyed by technology, not by tree.
PROVIDER_ADAPTERS = {
    "github-copilot": {"automatic_invocation": copilot_automatic_invocation},
    "claude-code": {},
}


def validate_structure(reg):
    """Is this a valid registry VALUE? Returns messages; empty means structurally sound.

    TOTAL over any JSON-shaped input -- it must never raise merely because the document is
    malformed, because a validator that crashes reports nothing and nothing reads as clean.
    This checker demonstrated that directly: a surface missing `provider_type` produced a
    KeyError instead of a finding.

    Structural validation answers "is this a valid registry value". Semantic validation, in
    Checker, answers "is that valid value TRUE about the repository and provider state".
    The two questions are kept apart: semantic code below may assume these types hold.
    """
    bad = []

    def req(cond, msg):
        if not cond:
            bad.append(msg)
        return bool(cond)

    ok, reg = as_obj(reg)
    if not req(ok, "registry: not a JSON object"):
        return bad

    req(as_str(reg.get("schema"))[0], "schema: must be a non-empty string")

    ok, surfaces = as_obj(reg.get("provider_surfaces"))
    if req(ok, "provider_surfaces: must be an object"):
        for sid, sdef in sorted(surfaces.items()):
            ok, sdef = as_obj(sdef)
            if not req(ok, "provider_surfaces.%s: must be an object" % sid):
                continue
            req(as_str(sdef.get("tree"))[0],
                "provider_surfaces.%s.tree: must be a non-empty string" % sid)
            req(as_str(sdef.get("provider_type"))[0],
                "provider_surfaces.%s.provider_type: must be a non-empty string -- semantics "
                "dispatch on it, so it cannot be absent or of another type" % sid)

    req(as_obj(reg.get("relationship_model"))[0], "relationship_model: must be an object")

    agents = reg.get("agents")
    if req(isinstance(agents, list), "agents: must be an array"):
        for i, rec in enumerate(agents):
            ok, rec = as_obj(rec)
            if not req(ok, "agents[%d]: must be an object" % i):
                continue
            label = rec.get("name") if isinstance(rec.get("name"), str) else "agents[%d]" % i
            req(as_str(rec.get("name"))[0], "%s: name must be a non-empty string" % label)
            req(as_str(rec.get("relationship"))[0],
                "%s: relationship must be a non-empty string" % label)

            ok, surfs = as_obj(rec.get("surfaces"))
            if req(ok, "%s: surfaces must be an object" % label):
                for sid, entry in sorted(surfs.items()):
                    ok, entry = as_obj(entry)
                    if not req(ok, "%s.%s: must be an object" % (label, sid)):
                        continue
                    req(as_str(entry.get("path"))[0],
                        "%s.%s.path: must be a non-empty string" % (label, sid))
                    for field, value in sorted(entry.items()):
                        # Only KNOWN semantic projections are type-checked here. An unknown
                        # key is a semantic question -- is it provider-owned, or not projected
                        # by this provider type -- and answering it structurally would pre-empt
                        # the better message with a type complaint.
                        if field not in ALL_SEMANTIC_FIELDS:
                            continue
                        req(as_exact_bool(value)[0],
                            "%s.%s.%s: must be a real JSON boolean, not %s(%r). bool is a "
                            "subclass of int in Python, so 0/1 would compare equal to the "
                            "derived value and pass."
                            % (label, sid, field, type(value).__name__, value))

            mp = rec.get("mirror_pairs")
            if mp is not None and req(isinstance(mp, list),
                                      "%s: mirror_pairs must be an array" % label):
                for pair in mp:
                    req(as_str_list(pair)[0] and len(pair) == 2,
                        "%s: each mirror_pairs entry must be two surface-id strings, got %r"
                        % (label, pair))

            div = rec.get("divergence")
            if div is not None and req(as_obj(div)[0],
                                       "%s: divergence must be an object" % label):
                if "adjudicated" in div:
                    req(as_exact_bool(div["adjudicated"])[0],
                        "%s: divergence.adjudicated must be a real boolean, not %s(%r)"
                        % (label, type(div["adjudicated"]).__name__, div["adjudicated"]))
                for k in ("why", "owning_issue"):
                    if k in div:
                        req(as_str(div[k])[0],
                            "%s: divergence.%s must be a non-empty string" % (label, k))

    ok, scope = as_obj(reg.get("declared_scope"))
    if ok:
        ok, cross = as_obj(scope.get("cross_registry"))
        if ok:
            for k in ("agent_surfaces_tracked_by_agents_json",
                      "provider_surfaces_no_registry_covers"):
                if k in cross:
                    req(as_str_list(cross[k])[0],
                        "declared_scope.cross_registry.%s: must be an array of non-empty "
                        "strings" % k)
    return bad


ALL_SEMANTIC_FIELDS = frozenset(f for spec in PROVIDER_ADAPTERS.values() for f in spec)


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
        """Direct children only, keyed by repo-relative PATH.

        Keyed by path, not stem: a record pointing at `<tree>/subdir/foo.md` and a provider
        file at `<tree>/foo.md` share a stem, so a stem-keyed reverse scan counted the
        top-level file as registered while the provider loaded a body no record described.
        Identity is the path.
        """
        d = self.root / tree
        if not d.is_dir():
            return None
        return {"%s/%s" % (tree, p.name): p
                for p in sorted(d.glob("*.md")) if p.name != "README.md"}

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
        else:
            self._report_similarity(rec)
        if "body_similarity" in div:
            self.fail("%s: divergence.body_similarity is stored derived data and is not "
                      "recomputed. Remove it -- --verbose prints fresh values." % rec["name"])

    def _report_similarity(self, rec):
        """Compute pairwise body similarity fresh and print it. Never stored.

        A stored score goes stale the moment a prompt is partially edited, while the record
        still reads green -- derived data rotting inside a truth owner is the defect this
        registry exists to end, so the number is produced on demand instead.
        """
        b = self.bodies(rec)
        ids = sorted(b)
        for i, x in enumerate(ids):
            for y in ids[i + 1:]:
                self.ok("%s: body similarity %s/%s = %.2f (recomputed, not stored)"
                        % (rec["name"], x, y,
                           difflib.SequenceMatcher(None, b[x], b[y]).ratio()))

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
        else:
            self._report_similarity(rec)
        if "body_similarity" in div:
            self.fail("%s: divergence.body_similarity is stored derived data. A partial prompt "
                      "edit makes it stale while this record still reads green. Remove it -- "
                      "--verbose recomputes similarity on every run." % rec["name"])

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

        structural = validate_structure(reg)
        if structural:
            for m in structural:
                self.fail("STRUCTURE %s" % m)
            # Semantic checks below assume these types. Running them on a malformed value is
            # how a checker crashes instead of reporting, so stop here.
            return self.report()

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
        usable = {}          # surface ids whose declaration validated; records may use these
        for sid, sdef in sorted(surfaces.items()):
            ptype = (sdef or {}).get("provider_type")
            if ptype not in PROVIDER_ADAPTERS:
                self.fail("provider_surfaces.%s: provider_type %r has no adapter. Semantics "
                          "bind to the provider technology, not to this surface's label, and a "
                          "type with no adapter must fail rather than silently project nothing. "
                          "Known types: %s." % (sid, ptype, ", ".join(sorted(PROVIDER_ADAPTERS))))
                continue
            usable[sid] = ptype
            self.ok("surface %s is provider_type %s (%d semantics)"
                    % (sid, ptype, len(PROVIDER_ADAPTERS[ptype])))
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
                if sid not in usable:
                    # Its declaration already failed above. Continuing would index an adapter
                    # that does not exist -- a checker that raises reports nothing.
                    self.fail("%s.%s: surface declaration is invalid, so this record cannot be "
                              "validated against it." % (name, sid))
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
                # Providers load the tree's direct children. A nested path is loaded by
                # nobody, and it lets a record satisfy every per-record check while the file
                # the provider actually reads goes unregistered.
                if pathlib.PurePosixPath(path).parent != pathlib.PurePosixPath(tree):
                    self.fail("%s.%s: path %s is nested below the declared tree %s. Only "
                              "direct children are loaded, so a nested file is a record "
                              "describing something no provider reads." % (name, sid, path, tree))
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

                self.check_semantics(name, sid, entry, fm, path,
                                     surfaces[sid]["provider_type"])
                registered.setdefault(sid, set()).add(path)
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
            for relpath in sorted(found):
                if relpath not in registered.get(sid, set()):
                    self.fail("%s exists but no record names that exact path on surface %r. An "
                              "unregistered provider agent must not be able to appear silently."
                              % (relpath, sid))

        self.check_enforcement_claims(reg)
        self.check_cross_registry(surfaces)
        return self.report()

    def check_enforcement_claims(self, reg):
        """The registry names its own checker, tests and callers. Those are claims too.

        Nothing verified them, so moving or renaming any of these files would leave the truth
        owner pointing at something that does not exist -- the same unpinned-copy failure this
        file keeps finding, applied to its own enforcement block.
        """
        enf = reg.get("enforcement")
        if not isinstance(enf, dict):
            self.fail("enforcement: must be an object naming the checker, its tests and its "
                      "callers.")
            return
        for key in ("checker", "tests"):
            val = enf.get(key)
            if not isinstance(val, str) or not val:
                self.fail("enforcement.%s: must be a non-empty path string" % key)
            elif not (self.root / val).is_file():
                self.fail("enforcement.%s names %s, which does not exist." % (key, val))
            else:
                self.ok("enforcement.%s -> %s exists" % (key, val))
        callers = enf.get("invoked_by")
        if not isinstance(callers, list):
            self.fail("enforcement.invoked_by: must be an array of caller paths")
            return
        for c in callers:
            if not isinstance(c, str) or not c:
                self.fail("enforcement.invoked_by: %r is not a path string" % (c,))
            elif not (self.root / c).is_file():
                self.fail("enforcement.invoked_by names %s, which does not exist." % c)
            elif "check-agent-registry" not in (self.root / c).read_text(
                    encoding="utf-8", errors="replace"):
                self.fail("enforcement.invoked_by names %s, but that file does not invoke this "
                          "checker. A caller list nothing verifies is how a gate silently "
                          "stops running." % c)
            else:
                self.ok("enforcement.invoked_by -> %s invokes this checker" % c)

    def check_semantics(self, name, sid, entry, fm, path, provider_type):
        """Registry semantic projections must equal what the provider file actually means."""
        spec = PROVIDER_ADAPTERS[provider_type]
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
                self.fail("%s.%s: %r is not a semantic provider type %r projects. Adapters own "
                          "which semantics exist; recording one here asserts behaviour that "
                          "provider does not have." % (name, sid, field, provider_type))

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
            elif not (self.root / un).exists():
                self.fail("skills.json names %s as an uncovered provider surface, but no such "
                          "tree exists. A gap list that names phantom trees is as untrue as "
                          "one that omits real ones." % un)
            else:
                self.ok("uncovered surface %s exists and is named as a gap" % un)

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
