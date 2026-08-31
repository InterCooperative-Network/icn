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

# The complete record vocabulary, checked as an ALLOWLIST -- see the record loop.
RECORD_KEYS = ("name", "relationship", "surfaces", "routing_triggers", "not_for",
               "divergence", "mirror_pairs")

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


# ICN's registered-agent front-matter contract is deliberately NARROWER than YAML. Providers
# may accept the whole language; this checker requires a subset it can verify, because the
# alternative is pretending a regex understands YAML. Every top-level key must be plain and
# unquoted:
#
#     name: value          supported
#     'name': value        UNSUPPORTED -- the old reader saw no `name` at all
#     "name": value        UNSUPPORTED
#     ? name               UNSUPPORTED (explicit key)
#     name : value         UNSUPPORTED (space before the colon)
#     {name: value}        UNSUPPORTED (flow mapping)
#
# Each of those is valid YAML the provider would honour, and each was silently read as
# ABSENT -- so a definition could change `name`, `infer` or `disable-model-invocation` while
# the registry certified the value it thought was there. Unsupported syntax now fails loudly
# instead of being reinterpreted as absence. All 43 checked-in definitions already comply.
_PLAIN_TOP_LEVEL_KEY = re.compile(r"^[A-Za-z_][A-Za-z0-9_-]*:(?:[ \t]|$)")


class InvalidDefinition(Exception):
    """The file is not a valid registered agent definition in the supported subset."""


# What each provider requires before a file IS a definition -- distinct from what the
# registry OWNS. Requiring `description` to exist does not make agents.json its owner; the
# value stays provider-side and is never copied here.
#
#   claude-code     name + description. Evidenced by this repository's own validator,
#                   scripts/check-claude-plugin.py:231-234, which errors on either missing.
#   github-copilot  description is required per
#                   docs.github.com/en/copilot/reference/custom-agents-configuration
#                   (verified 2026-08-30); `name` is optional there, the filename standing in.
PROVIDER_REQUIRED_FIELDS = {
    "claude-code": ("name", "description"),
    "github-copilot": ("description",),
}

# Plain scalars YAML reads as something other than a string, RESTRICTED to those that begin
# with an ASCII letter. A required string field whose value is one of these is not "present"
# in any useful sense: `description: null` is nonempty SOURCE TEXT, which is all the original
# presence check proved.
#
# Every other non-string plain scalar begins with a digit, a sign, `.`, `~`, `<` or `=`, and
# is excluded by the allowlist in `decode_inline_scalar` rather than by being named here.
# The YAML 1.1 single letters y/n are in the set because 1.1 resolvers are still in the field
# and a one-letter `name:` would otherwise load as a boolean.
_NON_STRING_WORDS = frozenset(
    w for base in ("null", "true", "false", "yes", "no", "on", "off", "y", "n")
    for w in (base, base.capitalize(), base.upper())
)
# Bare `|`/`>` with an optional CHOMPING modifier. The explicit indentation indicator
# (`|2`) is deliberately not supported: it moves the indent the body must use, which is the
# one thing the block check below establishes from the body itself. All 20 block scalars in
# the checked-in definitions are a bare `>`.
_BLOCK_SCALAR = re.compile(r"^[|>][+-]?$")


def strip_inline_comment(raw):
    """The scalar text with any inline comment removed, stripped.

    In YAML `#` opens a comment only when it starts the scalar or follows whitespace, and only
    outside quotes -- so a quoted value keeps its `#` verbatim. Shared, because the block
    indicator, the type test and the identity comparison must agree on where the value ends:
    when only one of them stripped, `description: | # rationale` stopped looking like a block
    scalar and a VALID definition was rejected.
    """
    v = raw.strip()
    if v[:1] in ("\"", "'"):
        # A `#` INSIDE the quotes is data; one AFTER the closing quote is a comment. Returning
        # the whole string treated `description: "fixture" # rationale` as a quoted scalar
        # whose last character is no longer a quote, so it was refused as UNTERMINATED -- the
        # required workflow red on a definition a YAML loader accepts.
        end = v.find(v[0], 1)
        if end < 0:
            return v                      # unterminated; the decoder reports it as such
        return (v[:end + 1] + re.split(r"(?:^|\s)#", v[end + 1:], maxsplit=1)[0]).strip()
    return re.split(r"(?:^|\s)#", v, maxsplit=1)[0].strip()


def decode_inline_scalar(raw):
    """(text, problem) for a value written on the key's own line -- exactly one is None.

    THE ONE READER. The required-field check and the front-matter/registry identity
    comparison both call this, because they disagreed: the field check accepted
    `name: "icn-ci-reliability"` and `name: icn-ci-reliability # canonical` as valid strings,
    and the comparison then matched the RAW text against the registered name and reported
    drift for a definition the provider loads correctly. A false rejection, from two
    functions holding different opinions about what a definition says.
    """
    if not raw.strip():
        return None, "is empty"
    v = strip_inline_comment(raw)
    if not v:
        return None, "is only a comment"

    if v[0] in "\"'":
        if len(v) < 2 or v[-1] != v[0]:
            return None, "is an unterminated quoted scalar"
        body = v[1:-1]
        return (None, "is an empty quoted string") if not body.strip() else (body, None)

    # PLAIN SCALARS ARE ACCEPTED BY ALLOWLIST, NOT BY EXCLUSION LIST.
    #
    # The exclusion list named the null/boolean words and a decimal-number regex, so every
    # OTHER non-string YAML literal was certified as a string: `0x10`, `0o17`, `0b1010`,
    # `1_000`, `.inf`, `.nan`, `2026-01-01`, `2026-01-01T00:00:00Z`, `12:30:45` (1.1
    # sexagesimal) and the 1.1 booleans `y`/`n`. Naming them one at a time is the losing side
    # of this: every miss is silent, and the resolver set is not ours to freeze.
    #
    # Every non-string plain scalar in YAML 1.1 and 1.2 either begins with a character that is
    # not an ASCII letter -- a digit, sign, `.`, `~`, `<`, `=` -- or is one of a CLOSED set of
    # words. So a plain scalar is PROVABLY a string when it starts with an ASCII letter and is
    # not one of those words, and nothing else is accepted.
    #
    # All 43 checked-in definitions are inside that subset: 43 plain names and 23 plain
    # descriptions all start with a letter, 20 descriptions are block scalars, none is quoted
    # and none is digit-leading. `description: 5 reviewers approve` IS a string to YAML and
    # fails here anyway, explicitly, saying to quote it -- the round-14 trade, a narrow
    # grammar that fails loudly over a broad one that guesses.
    if v in _NON_STRING_WORDS or v == "~":
        return None, "is the YAML %s literal, not a string" % v
    if v[0] in "[{":
        return None, "is a flow %s, not a string" % ("sequence" if v[0] == "[" else "mapping")
    if not re.match(r"^[A-Za-z]", v):
        return None, ("begins with %r, so YAML may resolve it as a number, date, timestamp or "
                      "null rather than a string; quote it to make it one" % v[0])
    return v, None


def required_string_problem(block, key):
    """Why `key` is not a nonempty string in the supported subset, or None.

    The supported value forms are exactly the ones the 43 checked-in definitions use plus
    the quoted spelling: a plain scalar, a quoted scalar, or a block scalar with content.
    Anything whose type this cannot establish fails rather than being counted as present.
    """
    raw = front_matter_value(block, key)
    if raw is None:
        return "is absent"
    if not raw.strip():
        return "is empty"

    if _BLOCK_SCALAR.match(strip_inline_comment(raw)):
        # `description: >` -- the value is the indented block beneath it. Located by KEY:
        # an earlier revision searched for the line ending in the stripped indicator, so
        # `description: | # rationale` matched nothing and raised StopIteration, rejecting a
        # VALID definition with a traceback. Comment stripping must not make the key
        # unfindable.
        lines = block.split("\n")
        idx = next((i for i, ln in enumerate(lines)
                    if re.match(r"^%s:" % re.escape(key), ln)), None)
        if idx is None:
            return "declares a block scalar whose key line cannot be located"
        body = []
        for ln in lines[idx + 1:]:
            if ln.strip() and not ln[0].isspace():
                break
            body.append(ln)
        if not any(ln.strip() for ln in body):
            return "is a block scalar with no content"
        return None

    return decode_inline_scalar(raw)[1]


def parse_registered_agent_front_matter(text, provider_type):
    """Validate a file AS a definition and return its front-matter block.

    Raises InvalidDefinition. This is the single gate a file must pass before it counts as a
    registered provider definition, so that afterwards `front_matter_value(block, key) is
    None` means exactly one thing: the definition is structurally valid and this optional key
    is genuinely absent. It no longer also means "the file had no front matter", "the YAML was
    spelled differently", or "the root mapping was indented".

    Deliberately not a YAML parser. It proves the file lies inside the supported subset and
    refuses everything else, which is a claim it can actually keep.
    """
    m = re.match(r"^---\n(.*?)\n---\n", text, re.S)
    if not m:
        raise InvalidDefinition(
            "no well-formed front matter: a registered definition must open and close with a "
            "`---` line. An earlier revision read a missing block as an empty one, so a file "
            "the provider rejects as malformed could still be registered")
    block = m.group(1)

    substantive = [(i, ln) for i, ln in enumerate(block.split("\n"), 1)
                   if ln.strip() and not ln.lstrip().startswith("#")]
    if not substantive:
        raise InvalidDefinition("front matter block is empty")

    first_no, first = substantive[0]
    if first[0].isspace():
        raise InvalidDefinition(
            "line %d, %r: the root mapping is indented. A YAML parser still reads it as the "
            "root, but this reader treats indented lines as nested content, so every semantic "
            "key would vanish and its default would be derived instead. The root mapping of a "
            "registered definition begins at column 0" % (first_no, first[:60]))

    # AN INDENTED LINE IS NOT AUTOMATICALLY NESTED CONTENT. Every indented line used to be
    # skipped, so `infer: false` followed by `  orphan: value` was certified -- and a YAML
    # parser rejects that outright ("mapping values are not allowed here"), meaning the
    # provider cannot load the definition at all while the canonical registry called it valid.
    #
    # YAML introduces indented content in exactly two ways: a block-scalar indicator, or an
    # empty value opening a nested mapping or list. After a key whose value is already a
    # scalar, indentation is either an error or a multi-line plain scalar -- and the latter is
    # excluded on purpose, the round-14 trade again: quote it or use a block scalar. All 43
    # checked-in definitions are inside this subset; every one of their 42 indented lines sits
    # beneath a block scalar, and none opens a nested mapping or continues a plain scalar.
    # Walked over ALL lines rather than `substantive`, because a `#` line INSIDE a block
    # scalar is content, not a comment -- dropping it would lose the indentation sample that
    # decides whether the block is well formed.
    opens_nested = False        # may an indented line follow the key just read?
    in_block = False            # ...and is that indentation a block scalar's body?
    block_indent = None         # the indent its first body line established
    for i, line in enumerate(block.split("\n"), 1):
        if not line.strip():
            continue
        if line[0].isspace():
            if not opens_nested:
                raise InvalidDefinition(
                    "line %d, %r: indented content follows a key whose value is already a "
                    "scalar. A YAML parser rejects this, so the provider cannot load the "
                    "definition -- indentation is supported only beneath a block scalar or an "
                    "empty value. Quote the value or use a block scalar." % (i, line[:60]))
            if in_block:
                # A BLOCK SCALAR'S INDENTATION IS PART OF ITS VALIDITY. Accepting every
                # indented line meant a body that dedents mid-block -- two spaces, then one --
                # was certified while the YAML loader raised ParserError, so the provider could
                # not load a definition the canonical registry called valid.
                indent = len(line) - len(line.lstrip())
                if block_indent is None:
                    block_indent = indent
                elif indent < block_indent:
                    raise InvalidDefinition(
                        "line %d, %r: indented %d columns, inside a block scalar whose body "
                        "established %d. A line that dedents out of the block without reaching "
                        "column 0 is not a root key either, and a YAML parser raises rather "
                        "than reading it as content."
                        % (i, line[:60], indent, block_indent))
            continue
        if line.lstrip().startswith("#"):
            continue
        if not _PLAIN_TOP_LEVEL_KEY.match(line):
            raise InvalidDefinition(
                "line %d, %r: registered ICN agent front matter requires plain unquoted "
                "top-level keys. Quoted, explicit (`? key`), flow-mapping and "
                "space-before-colon spellings are valid YAML the provider honours, and this "
                "reader would treat them as the key being ABSENT -- so a semantic field could "
                "change while the registry certified the old value." % (i, line[:60]))
        value = strip_inline_comment(line.split(":", 1)[1])
        if re.match(r"^[|>][+-]?\d", value):
            raise InvalidDefinition(
                "line %d, %r: an explicit block indentation indicator is not supported. It "
                "moves the indent the body must use, which is the one thing this reader "
                "establishes from the body itself -- so supporting it would mean guessing. "
                "Use a bare `|` or `>`." % (i, line[:60]))
        in_block = bool(_BLOCK_SCALAR.match(value))
        block_indent = None
        opens_nested = in_block or not value

    for field in PROVIDER_REQUIRED_FIELDS.get(provider_type, ()):
        try:
            problem = required_string_problem(block, field)
        except AmbiguousFrontMatter as exc:
            raise InvalidDefinition(str(exc))
        if problem:
            raise InvalidDefinition(
                "provider_type %r requires %r to be a nonempty string for the file to be a "
                "valid agent definition, and it %s. The registry does not own that field -- "
                "it only refuses to certify a file the provider would reject as malformed"
                % (provider_type, field, problem))
    return block


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
    if "declared_scope" in reg:
        req(as_obj(reg["declared_scope"])[0],
            "declared_scope: must be an object, found %s"
            % type(reg["declared_scope"]).__name__)

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

        # These run AFTER validate_structure, which has now proven declared_scope is an
        # object. An earlier revision placed them first, so `declared_scope: 1` reached a
        # membership test and raised TypeError -- a structural guard crashing ahead of the
        # structural validator that exists to report it.
        if "enforcement" in reg:
            self.fail("enforcement was removed in icn#2632 review round 13 and must not "
                      "return. Nothing outside this checker ever dereferenced it, and each "
                      "field spawned machinery to prove a self-description no consumer "
                      "needed. The gates are real and unchanged; they do not need their "
                      "identities copied here.")
        if "in_scope" in reg.get("declared_scope", {}):
            self.fail("declared_scope.in_scope was removed in icn#2632 review round 13. "
                      "provider_surfaces is the one machine-readable owner of provider "
                      "topology; a second glob list changed no behaviour and could only "
                      "become false.")

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
        # PHYSICAL identity, not lexical. A lexically distinct second tree can be a symlink
        # to the first: both ids then inventory the same files, mirror comparisons become
        # self-comparisons, and the registry claims a surface that does not independently
        # exist. Round 8 closed the string-identity case; this closes the filesystem one.
        #
        # Resolution also proves containment, which a `..`-free path does not: a symlink can
        # leave the repository without any lexical evidence.
        try:
            repo_root = self.root.resolve()
        except (ValueError, OSError) as exc:
            self.fail("cannot resolve the repository root (%s)" % exc)
            repo_root = None
        seen_trees = {}
        for sid, sdef in sorted(surfaces.items()):
            t = (sdef or {}).get("tree")
            if not isinstance(t, str) or repo_root is None:
                continue
            try:
                real = (self.root / t).resolve()
            except (ValueError, OSError) as exc:
                self.fail("provider_surfaces.%s: tree %s cannot be resolved (%s)"
                          % (sid, t, exc))
                continue
            if not real.exists():
                continue                      # reported by the tree-existence check below
            if not real.is_relative_to(repo_root):
                self.fail("provider_surfaces.%s: tree %s resolves to %s, outside the "
                          "repository. A path with no `..` can still leave the repo through a "
                          "symlink, so containment is proven by resolution, not by spelling."
                          % (sid, t, real))
                continue
            if real in seen_trees:
                self.fail("provider_surfaces.%s declares tree %s, which resolves to the same "
                          "directory as %s. One physical tree is one surface: two ids over one "
                          "directory inventory it twice and let a mirror compare a file with "
                          "itself -- and a symlink makes them lexically distinct."
                          % (sid, t, seen_trees[real]))
            else:
                seen_trees[real] = sid

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

            # RECORD KEYS ARE AN ALLOWLIST. The provider-owned check below reads each
            # SURFACE entry, so a provider-native field written at RECORD level -- a
            # top-level `description` on the agent -- left the registry carrying exactly the
            # unpinned copy `PROVIDER_OWNED_KEYS` exists to forbid, and the gate stayed
            # green. An unknown key is refused for the same reason a misspelled one is:
            # `mirror_pairs` is optional, so `mirror_pair` silently drops the only promise
            # the record was making. Every other record key is required and would fail as
            # absent; this one would not.
            for k in sorted(set(rec) - set(RECORD_KEYS)):
                if k in PROVIDER_OWNED_KEYS:
                    self.fail("%s: %r is provider-native syntax owned by the provider "
                              "definition, not the registry, and a record-level copy is "
                              "unpinned by any surface -- the registry records derived "
                              "SEMANTICS, never mirrored syntax." % (name, k))
                else:
                    self.fail("%s: %r is not a record key (%s). An unrecognised key is most "
                              "often a misspelled optional one, which drops its promise "
                              "silently." % (name, k, ", ".join(RECORD_KEYS)))

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

                try:
                    fm = parse_registered_agent_front_matter(
                        fp.read_text(encoding="utf-8"), surfaces[sid]["provider_type"])
                except InvalidDefinition as exc:
                    self.fail("%s.%s: %s (%s)" % (name, sid, exc, path))
                    continue
                try:
                    declared = front_matter_value(fm, "name")
                except AmbiguousFrontMatter as exc:
                    self.fail("%s.%s: %s" % (name, sid, exc))
                    continue
                if declared is not None:
                    # DECODED, not raw. The provider's loader sees the VALUE; this saw the
                    # spelling, so a quoted name or an inline comment on it was reported as
                    # drift against a definition the provider resolves to exactly the
                    # registered name -- a false rejection. A block-scalar name is refused
                    # rather than guessed at: folding is not something this checker can do,
                    # and no checked-in definition writes one.
                    if _BLOCK_SCALAR.match(strip_inline_comment(declared)):
                        self.fail("%s.%s: front matter writes name as a block scalar. The "
                                  "registry compares identities and cannot fold one; write "
                                  "the name inline." % (name, sid))
                    else:
                        text, problem = decode_inline_scalar(declared)
                        if problem is not None:
                            self.fail("%s.%s: front-matter name %s" % (name, sid, problem))
                        elif text != name:
                            self.fail("%s.%s: front matter declares name: %s. The provider "
                                      "loads the front-matter name, so the registry would "
                                      "route to a name the provider does not answer to."
                                      % (name, sid, text))

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

        self.check_cross_registry(surfaces)
        return self.report()

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

        # Type every PARENT before chaining. Round 16 typed the cross-registry LISTS and left
        # the objects holding them untyped, so `skills.json.enforcement: "a string"` raised
        # AttributeError rather than reporting -- the same non-totality one level up. This
        # checker runs as its own standalone gate and cannot assume the skill checker ran.
        def child_obj(parent, key, label, required=False):
            if key not in parent:
                if required:
                    self.fail("skills.json %s is missing. Without it this checker cannot "
                              "prove known_uncovered_directories are actually uncovered, and "
                              "silently substituting an empty scope would certify the "
                              "boundary on no evidence." % label)
                return {}
            ok, value = as_obj(parent[key])
            if not ok:
                self.fail("skills.json %s must be an object, found %s. Malformed canonical "
                          "data must produce a finding here, not a traceback."
                          % (label, type(parent[key]).__name__))
                return {}
            return value

        ok, sj_obj = as_obj(sj)
        if not ok:
            self.fail("skills.json must be a JSON object, found %s" % type(sj).__name__)
            return
        enforcement_present = "enforcement" in sj_obj
        enforcement = child_obj(sj_obj, "enforcement", "enforcement", required=True)
        # Keyed on PRESENCE, not truthiness: popping scan_scope leaves enforcement as an
        # empty dict, and `bool({})` would have read that as "enforcement absent" and skipped
        # the requirement -- silently restoring the very gap this check closes.
        scope = child_obj(enforcement, "scan_scope", "enforcement.scan_scope",
                          required=enforcement_present)
        scope_present = "scan_scope" in enforcement
        scan_trees = set()
        for key in ("canonical_trees", "provider_trees"):
            if key not in scope:
                if scope_present:
                    self.fail("skills.json enforcement.scan_scope.%s is missing. The "
                              "uncovered-directory claim is proven against the scan scope, so "
                              "an absent list would let it be certified on no evidence." % key)
                continue
            ok, lst = as_str_list(scope[key])
            if not ok:
                self.fail("skills.json enforcement.scan_scope.%s must be an array of "
                          "nonempty strings, found %r" % (key, scope[key]))
            else:
                scan_trees |= set(lst)
        declared = child_obj(sj_obj, "declared_scope", "declared_scope")
        cross = child_obj(declared, "cross_registry", "declared_scope.cross_registry")
        for field in ("agent_surfaces_tracked_by_agents_json",
                      "known_uncovered_directories",
                      "provider_surfaces_no_registry_covers"):
            if field in cross and not as_str_list(cross[field])[0]:
                self.fail("skills.json declared_scope.cross_registry.%s must be an array of "
                          "nonempty strings, found %r. Malformed canonical data must produce a "
                          "finding here, not a traceback: this checker consumes the field "
                          "directly as its own standalone gate and cannot assume another "
                          "checker ran first." % (field, cross[field]))
                cross = {k: v for k, v in cross.items() if k != field}

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
