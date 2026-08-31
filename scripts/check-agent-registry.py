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
import ast
import difflib
import json
import pathlib
import re
import shlex
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


class AmbiguousFrontMatter(Exception):
    """A key this checker consumes appears more than once at the top level."""


def front_matter_value(block, key):
    """The single top-level value for `key`, None if absent.

    Raises when the key repeats. Taking the first match let a definition declare
    `infer: false` and `infer: true` and be certified as one of them -- a registry semantic
    cannot be derived from an ambiguous provider declaration, and that holds whether the
    repeated values contradict each other or agree. Every key this checker consumes goes
    through here, so the rule is one rule rather than a per-key patch.
    """
    if not block:
        return None
    matches = re.findall(r"^%s:[ \t]*(.*)$" % re.escape(key), block, re.M)
    if len(matches) > 1:
        raise AmbiguousFrontMatter(
            "%s appears %d times in the front matter. One occurrence or none -- a repeated "
            "key leaves the provider free to resolve it differently than this checker does"
            % (key, len(matches)))
    return matches[0].strip() if matches else None


def strict_bool(block, key):
    """True / False / None-when-absent. Any other present value is an error.

    Coercing an unrecognised scalar would let the registry certify provider behaviour the
    provider does not define: `infer: flase` is not false, it is unknown.
    """
    raw = front_matter_value(block, key)
    if raw is None:
        return None
    v = raw.strip()
    # No quote stripping. `infer: "false"` is a YAML *string*, and turning it into a boolean
    # manufactures a type the provider never declared -- the registry would then certify
    # behaviour on the strength of the checker's own coercion. Only the two unquoted YAML
    # boolean literals are accepted; everything else, quoted or otherwise, is unknown.
    #
    # Deliberately not a YAML parser: the repository has no established YAML dependency for
    # this path (only tools/validate-governed-bridge-conformance.py imports it, inside a
    # function, and no CI step installs it), and two scalar keys do not justify one. This
    # preserves exactly the distinction that matters and pretends to nothing more.
    if v == "true":
        return True
    if v == "false":
        return False
    if len(v) >= 2 and v[0] == v[-1] and v[0] in ("\"", "'"):
        raise InvalidProviderBoolean(
            "%s: %s is a quoted string, not a YAML boolean. The provider reads a string "
            "here, so treating it as %s would be the checker inventing a type."
            % (key, raw, v.strip("\"'")))
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

            for k in ("routing_triggers", "not_for"):
                req(as_str_list(rec.get(k)) [0] if isinstance(rec.get(k), list) else False,
                    "%s: %s must be an array of strings. This file is the canonical owner of "
                    "agent routing, so a deleted or scalar routing field is a malformed routing "
                    "table, not a missing nicety." % (label, k))

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


# An invocation, not a mention. Comment lines are dropped (`#` opens a comment in both shell
# and YAML), then a line must actually run the checker through an interpreter. A bare path in
# a workflow `paths:` filter, an `if [[ -f ... ]]` guard or a `fail "... is missing"` string
# is a reference, not a gate.
_INTERPRETERS = ("python", "python3")

# Python's own synopsis is `[-c cmd | -m mod | file | -]`: the executable alternatives are
# mutually exclusive, and after `-c` or `-m` every later token is an ARGUMENT to the selected
# program, not a script. `python3 -c 'pass' scripts/check-agent-registry.py` runs nothing.
#
# Deliberately no option modelling. All three real callers are `python3 <path> [args]` with no
# interpreter options at all, so the first token after argv0 must BE the script. A future
# caller that legitimately adds `-u` will turn this gate red until the shape is added on
# purpose -- which is the safer failure than a verifier speculating broadly enough to accept a
# no-op.
_VAR_PREFIX = re.compile(r"^\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/")


def _candidate_commands(line):
    """Command strings a line could actually execute.

    The line itself with a leading YAML `run:` or shell keyword removed, plus the contents of
    any `$(...)`. The three real callers wrap the invocation as `if out=$(python3 ... ); then`,
    so the substitution has to be looked inside.
    """
    stripped = line.strip()
    stripped = re.sub(r"^-?\s*(run|command)\s*:\s*", "", stripped)
    stripped = re.sub(r"^(if|elif|while|until|then|do|else)\s+", "", stripped)
    yield re.sub(r"^[A-Za-z_][A-Za-z0-9_]*=", "", stripped)
    for inner in re.findall(r"\$\(([^()]*)\)", line):
        yield re.sub(r"^[A-Za-z_][A-Za-z0-9_]*=", "", inner.strip())


# Operators that stop a non-zero exit from reaching the caller, or hand the status to
# something else. `2>&1` is a redirection and contains none of these, which is why the real
# callers still pass. Anything else fails closed: a novel wrapper should make this gate red
# until its status semantics are deliberately established.
# `&` as a control operator backgrounds the command, so its eventual status never reaches
# the caller. `2>&1` and `&>` are redirections and must stay legal -- every real caller uses
# the first. The negative lookbehind/ahead below distinguish them: a redirecting `&` is
# preceded by a digit or `>`, or followed by `>`.
_STATUS_BREAKING = re.compile(
    r"\|\||&&|;|(?<![0-9>&])\|(?!\|)|(?<![0-9>&])&(?![>&])")


def _selected_script(tokens):
    """The file Python would execute, or None if it selects something else."""
    if len(tokens) < 2:
        return None
    argv0 = tokens[0].strip("\"'").rsplit("/", 1)[-1]
    if argv0 not in _INTERPRETERS:
        return None
    candidate = tokens[1].strip("\"'")
    if candidate.startswith("-"):
        # -c, -m, - and every other option: the program is not this token.
        return None
    return _VAR_PREFIX.sub("", candidate)


def invokes_checker(text, target="scripts/check-agent-registry.py"):
    """True only when some line RUNS the checker as Python's selected program.

    A role is proven by the operation performed, not by the presence of role-shaped tokens.
    Mentions that must NOT count: `echo "python3 ..."`, an `[[ -f ... ]]` guard, a
    `fail "... is missing"` string, a workflow `paths:` entry, a comment, and any invocation
    where the path is an argument to a different selected program.
    """
    for line in text.splitlines():
        if line.strip().startswith("#"):
            continue
        for cand in _candidate_commands(line):
            if target.rsplit("/", 1)[-1] not in cand:
                continue
            try:
                tokens = shlex.split(cand, comments=True)
            except ValueError:
                continue
            if _STATUS_BREAKING.search(cand):
                # `python3 ... || true` runs the checker and throws its verdict away, so the
                # advertised standalone gate can no longer fail.
                continue
            script = _selected_script(tokens)
            if script and (script == target or script.endswith("/" + target)
                           or script.endswith(target)):
                return True
    return False


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
        """Raw post-front-matter bodies. NOT stripped.

        `.strip()` erased leading and trailing whitespace before every comparison, so
        indenting a prompt's first line by four spaces -- which turns it into a Markdown code
        block, changing what the provider renders -- still reported the bodies byte-identical.
        A mirror claim is a claim about bytes; normalising them first makes it a claim about
        something else.
        """
        out = {}
        for sid, entry in (rec.get("surfaces") or {}).items():
            fp = self.root / (entry or {}).get("path", "")
            if fp.exists():
                _, body = split_front_matter(fp.read_text(encoding="utf-8"))
                out[sid] = body
        return out

    # ---- relationship validators. One per declared type; see SEMANTICS above. -------

    def _reject_divergence(self, rec, rel):
        if rec.get("divergence"):
            self.fail("%s: relationship %s but divergence metadata is retained. Stage 2 "
                      "converges records by changing the relationship, and leftover "
                      "adjudication or owning-issue fields would keep asserting a difference "
                      "the relationship says no longer exists." % (rec["name"], rel))

    def _v_single_surface(self, rec):
        n = len(rec.get("surfaces") or {})
        if n != 1:
            self.fail("%s: relationship single_surface but %d surfaces are named"
                      % (rec["name"], n))
        self._reject_divergence(rec, "single_surface")

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
        self._reject_divergence(rec, "exact_mirror")

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
        if div.get("adjudicated") is True:
            self.fail("%s: divergent_unreviewed with divergence.adjudicated = true claims the "
                      "divergence is both unreviewed and adjudicated. Promote it to "
                      "provider_variant, which is what an adjudicated divergence is."
                      % rec["name"])
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
        # One physical tree is one registered surface. Two ids over the same directory made
        # every file appear as two exposures, let an exact_mirror compare a body with itself,
        # and collapsed back to one entry in the cross-registry set -- so the registry claimed
        # a provider surface that does not independently exist. There is no alias model and no
        # demonstrated need for one; if one is ever added it must carry its own semantics
        # rather than reusing this field.
        seen_trees = {}
        for sid, sdef in sorted(surfaces.items()):
            t = (sdef or {}).get("tree")
            if not isinstance(t, str):
                continue
            norm = pathlib.PurePosixPath(t).as_posix().rstrip("/")
            if norm in seen_trees:
                self.fail("provider_surfaces.%s declares tree %s, already declared by %s. One "
                          "physical tree is one surface: two ids over one directory inventory "
                          "it twice and let a mirror compare a file with itself."
                          % (sid, t, seen_trees[norm]))
            else:
                seen_trees[norm] = sid

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
                if not path.endswith(".md"):
                    self.fail("%s.%s: path %s does not end in .md. The inventory reads only "
                              "*.md, and so does the provider, so a record pointing at any "
                              "other extension describes a file nothing loads -- renaming a "
                              "definition and updating its record would otherwise remove the "
                              "agent from the surface silently." % (name, sid, path))
                    continue
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
                try:
                    declared = front_matter_value(fm, "name")
                except AmbiguousFrontMatter as exc:
                    self.fail("%s.%s: %s" % (name, sid, exc))
                    continue
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
                if pair[0] == pair[1]:
                    self.fail("%s: mirror_pairs entry %r names one surface twice. Both "
                              "membership checks pass and the body is compared with itself, so "
                              "the pair reports verified while the surface it was meant to pin "
                              "drifts freely." % (name, pair))
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
            self.fail("enforcement: must be an object naming the checker and its callers.")
            return
        # Existence is not role verification. `checker` must identify THIS script, and
        # `tests` must actually exercise it -- otherwise the registry can point readers at
        # any file that happens to exist, which is how AGENTS.md passed as the enforcement
        # implementation.
        # The running script's own repository-relative path. Compared as a normalized
        # relative path, not an absolute one, so this holds under --repo-root too: a fixture
        # root is a different directory but the claim is about which file plays the role.
        me = pathlib.Path(__file__).resolve()
        my_rel = pathlib.PurePosixPath(me.parent.name, me.name).as_posix()
        val = enf.get("checker")
        if not isinstance(val, str) or not val:
            self.fail("enforcement.checker: must be a non-empty path string")
        else:
            declared_rel = pathlib.PurePosixPath(val).as_posix()
            if declared_rel != my_rel:
                self.fail("enforcement.checker names %s, but the running checker is %s. "
                          "Existence is not role verification: the registry must identify its "
                          "actual enforcement implementation, not any file that exists."
                          % (val, my_rel))
            elif not (self.root / val).is_file():
                self.fail("enforcement.checker names %s, which does not exist under this "
                          "repository root." % val)
            else:
                self.ok("enforcement.checker identifies the running checker (%s)" % my_rel)

        if "tests" in enf:
            self.fail("enforcement.tests was removed in icn#2632 review round 12 and must not "
                      "return. No consumer read it, and four review rounds were spent building "
                      "AST machinery to prove a copy nothing needed -- each round accepting "
                      "something execution-shaped that did not execute. Re-adding it "
                      "reintroduces an unpinned claim; the suite is gated directly in CI.")

        # `enforcement.tests` was REMOVED (icn#2632 review round 12). It named this
        # checker's invariant suite, and nothing outside this file ever read it:
        # drift-check.sh, what-matters-now.sh and generate-live-state-overlay.py all
        # consume agents.json without touching `enforcement`, and CI runs the suite
        # directly from .github/workflows/agent-drift-check.yml rather than resolving a
        # path through the registry. Four review rounds were spent building AST machinery
        # to prove a copy nothing consumed -- each round accepting something
        # execution-shaped that did not execute. Deleting the claim removes the class.
        # The suite itself is untouched and still gated in CI.
        callers = enf.get("invoked_by")
        if not isinstance(callers, list):
            self.fail("enforcement.invoked_by: must be an array of caller paths")
            return
        for c in callers:
            if not isinstance(c, str) or not c:
                self.fail("enforcement.invoked_by: %r is not a path string" % (c,))
            elif not (self.root / c).is_file():
                self.fail("enforcement.invoked_by names %s, which does not exist." % c)
            elif not invokes_checker((self.root / c).read_text(
                    encoding="utf-8", errors="replace")):
                self.fail("enforcement.invoked_by names %s, but no executable line there runs "
                          "the checker. A textual mention is not an invocation: all three "
                          "current callers name the path in comments, error strings or a "
                          "workflow paths filter, so a substring test would keep passing after "
                          "the real `run:` was deleted." % c)
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
            except AmbiguousFrontMatter as exc:
                self.fail("%s.%s: %s (%s). A registry semantic cannot be derived from an "
                          "ambiguous provider declaration." % (name, sid, exc, path))
                continue
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
        scope = (sj.get("enforcement", {}) or {}).get("scan_scope") or {}
        scan_trees = set(scope.get("canonical_trees") or []) | set(scope.get("provider_trees") or [])
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
        # Only what enforcement can establish. The field used to be named
        # `provider_surfaces_no_registry_covers`, which asserted a classification no owner
        # supplies -- so the check could test existence and nothing more, and AGENTS.md
        # passed as a provider surface. Narrowed to directories, verified as such.
        legacy = cross.get("provider_surfaces_no_registry_covers")
        if legacy is not None:
            self.fail("skills.json still declares provider_surfaces_no_registry_covers. That "
                      "name claims a provider classification no owner supplies; it is "
                      "known_uncovered_directories now, which is what the checker can prove.")
        for un in cross.get("known_uncovered_directories") or []:
            path = pathlib.PurePosixPath(un)
            if un.startswith("/") or ".." in path.parts:
                self.fail("skills.json known_uncovered_directories: %s is absolute or escapes "
                          "the repository root." % un)
            elif un in trees:
                self.fail("skills.json lists %s as covered by no registry, but agents.json "
                          "declares it as a surface." % un)
            elif un in scan_trees:
                self.fail("skills.json lists %s as covered by no registry, but its own "
                          "enforcement.scan_scope already covers it." % un)
            elif not (self.root / un).is_dir():
                self.fail("skills.json known_uncovered_directories: %s is not an existing "
                          "directory. A gap list naming a file or a phantom path is as untrue "
                          "as one omitting a real gap." % un)
            else:
                self.ok("uncovered directory %s exists and is covered by no registry" % un)

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
