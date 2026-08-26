#!/usr/bin/env python3
"""Merge-policy invariants (icn#2651, icn#2656).

`ops/state/truth/policy.json` is the registered owner of merge requirements. Two classes of
drift got past every existing gate:

  1. The owner named a field that cannot hold the value it was compared against.
     `admin_bypass.condition` read `mergeStateStatus=MERGEABLE`. `MERGEABLE` is a member of
     GitHub's `MergeableState` enum (the `mergeable` field); `mergeStateStatus` is
     `MergeStateStatus` and has no such member. The documented admin exception was therefore
     unsatisfiable, and nothing noticed because prose is not type-checked.

  2. A consumer restated a value the owner owns. `.agents/skills/merge-pr/SKILL.md` merged with
     `--merge` while `default_strategy` was `squash`, and `auto_merge.command` baked `--squash`
     into a string inside the owner itself — so the owner duplicated its own field and could
     contradict it.

Every MUST-FAIL case below is a reconstruction of real pre-fix state; the controls prove the
checks are not simply failing on everything.

Run: python3 scripts/tests/test_merge_policy_invariants.py
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
POLICY = ROOT / "ops" / "state" / "truth" / "policy.json"
SKILLS = ROOT / "ops" / "state" / "truth" / "skills.json"

# GitHub GraphQL enums, pinned so this runs offline. Re-verify with:
#   gh api graphql -f query='{m:__type(name:"MergeableState"){enumValues{name}}
#                             s:__type(name:"MergeStateStatus"){enumValues{name}}}'
# Verified 2026-08-26 against the live API.
MERGEABLE_STATE = {"MERGEABLE", "CONFLICTING", "UNKNOWN"}
MERGE_STATE_STATUS = {
    "DIRTY", "UNKNOWN", "BLOCKED", "BEHIND", "UNSTABLE", "HAS_HOOKS", "CLEAN",
}
# `gh pr merge` accepts exactly these strategies.
GH_MERGE_STRATEGIES = {"merge", "squash", "rebase"}
# gh api graphql -f query='{__type(name:"PullRequestReviewDecision"){enumValues{name}}}'
REVIEW_DECISIONS = {"CHANGES_REQUESTED", "APPROVED", "REVIEW_REQUIRED"}
STRATEGY_FLAG_RE = re.compile(r"--(merge|squash|rebase)\b")

failures = []


def check(desc, cond):
    if cond:
        print(f"  ok   {desc}")
    else:
        print(f"  FAIL {desc}")
        failures.append(desc)


merge = json.loads(POLICY.read_text(encoding="utf-8"))["merge"]

# --- (1) impossible field/value combinations in the canonical policy ---------
print("field/value coherence")

strategy = merge.get("default_strategy")
check(f"default_strategy {strategy!r} is a real gh pr merge strategy",
      strategy in GH_MERGE_STRATEGIES)

bypass = merge.get("admin_bypass", {})
req = bypass.get("requires")
check("admin_bypass declares machine-checkable `requires`, not only prose",
      isinstance(req, dict) and req != {})
check("admin_bypass still declares never_for", bool(bypass.get("never_for")))
check("admin_bypass declares fail_closed behaviour", bool(bypass.get("fail_closed")))

if isinstance(req, dict):
    check(f"requires.mergeable {req.get('mergeable')!r} is a MergeableState member",
          req.get("mergeable") in MERGEABLE_STATE)
    mss = req.get("merge_state_status_in", [])
    check("requires.merge_state_status_in is a non-empty list",
          isinstance(mss, list) and len(mss) > 0)
    check(f"every merge_state_status_in value is a MergeStateStatus member: {mss}",
          isinstance(mss, list) and all(v in MERGE_STATE_STATUS for v in mss))
    # THE REGRESSION. The pre-fix owner compared mergeStateStatus against MERGEABLE.
    check("no MergeableState value has leaked into a mergeStateStatus field",
          isinstance(mss, list)
          and not (set(mss) & (MERGEABLE_STATE - MERGE_STATE_STATUS)))
    # CLEAN needs no bypass; permitting it would make the exception a general bypass.
    check("admin bypass is not permitted on a CLEAN merge state",
          isinstance(mss, list) and "CLEAN" not in mss)

# The WHOLE FILE must not associate either field with a value the other's enum owns. Review on
# #2656 caught the first version of this check: it rejected the one literal
# `mergeStateStatus=MERGEABLE` and sailed past `readiness_definition`'s
# "mergeStateStatus is MERGEABLE or UNSTABLE" — the identical truth conflict, spelled with a
# word instead of an `=`, in a top-level key the check never looked at. A guard that only knows
# one spelling of a defect is a guard against that spelling, not against the defect.
print("no field is associated with the other enum's values, anywhere in the file")

# Values each enum owns EXCLUSIVELY. UNKNOWN is in both, so it proves nothing and is omitted.
MERGEABLE_ONLY = MERGEABLE_STATE - MERGE_STATE_STATUS       # MERGEABLE, CONFLICTING
STATUS_ONLY = MERGE_STATE_STATUS - MERGEABLE_STATE          # DIRTY, BLOCKED, BEHIND, ...

# EXACTLY ONE key is exempt: `field_note` exists to state "mergeStateStatus ... never takes the
# value MERGEABLE", which no negation-aware scanner should have to special-case twice. Review on
# #2656 was right that the earlier set was too broad — `note` and `description` are generic
# names, and `auto_merge.note` carries operative instructions, so a conflict planted there would
# have left this "whole-file" test green. Everything else is scanned; the window rules
# (stop at the next field, at a sentence break, at a negation) are what let genuine prose pass.
DOC_KEYS = {"field_note"}


def walk_strings(node, key=None, path="$"):
    """Yield (path, key, string) for every string value in the document."""
    if isinstance(node, dict):
        for k, v in node.items():
            yield from walk_strings(v, k, f"{path}.{k}")
    elif isinstance(node, list):
        for n, v in enumerate(node):
            yield from walk_strings(v, key, f"{path}[{n}]")
    elif isinstance(node, str):
        yield path, key, node


# An association holds only inside ONE clause about ONE field. The window therefore ends at the
# next mention of EITHER field — otherwise "mergeable is MERGEABLE, and mergeStateStatus is
# CLEAN" reads as `mergeable … CLEAN` and the checker flags the sentence that states the rule
# correctly. It also ends at a sentence break or a negation, so prose that says a field NEVER
# takes a value is not read as saying it does.
# CASE-SENSITIVE on purpose: the field names are camelCase and the enum values are UPPERCASE,
# so an IGNORECASE pattern made `mergeable` match the VALUE `MERGEABLE` and truncated the
# window to nothing — which silently disarmed the whole check. Caught by its own controls.
FIELD_RE = re.compile(r"\bmergeStateStatus\b|\bmergeable\b")


def associations(text: str, field: str, values: set[str]) -> list[str]:
    hits = []
    for m in re.finditer(rf"\b{field}\b", text):
        rest = text[m.end():]
        nxt = FIELD_RE.search(rest)
        window = rest[: nxt.start()] if nxt else rest[:80]
        window = re.split(r"[.;]|\bnever\b|\bnot\b|\bdifferent\b", window, maxsplit=1)[0]
        for v in values:
            if re.search(rf"\b{v}\b", window):
                hits.append(f"{field} … {v}")
    return hits


whole = json.loads(POLICY.read_text(encoding="utf-8"))
conflicts = []
for path, key, text in walk_strings(whole):
    if key in DOC_KEYS:
        continue
    conflicts += [f"{path}: {h}" for h in associations(text, "mergeStateStatus", MERGEABLE_ONLY)]
    conflicts += [f"{path}: {h}" for h in associations(text, "mergeable", STATUS_ONLY)]
check(f"no cross-enum field/value association in any string: {conflicts}", not conflicts)

# CONTROLS: the scanner must catch BOTH spellings of the defect, in ANY key.
_c1 = associations("Required checks green AND mergeStateStatus=MERGEABLE but stalled",
                   "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: catches the `mergeStateStatus=MERGEABLE` spelling", bool(_c1))
_c2 = associations("mergeStateStatus is MERGEABLE or UNSTABLE", "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: catches the `mergeStateStatus is MERGEABLE` spelling", bool(_c2))
_c3 = associations("mergeable is CLEAN", "mergeable", STATUS_ONLY)
check("CONTROL: catches the reverse confusion (`mergeable is CLEAN`)", bool(_c3))
# ...and must NOT fire on the note that documents the distinction.
_c4 = associations("`mergeStateStatus` is a DIFFERENT enum and never takes the value MERGEABLE",
                   "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: does not fire on prose documenting the distinction", not _c4)
_c5 = associations("mergeable is MERGEABLE, and mergeStateStatus is CLEAN or UNSTABLE",
                   "mergeable", STATUS_ONLY)
check("CONTROL: a correct two-field sentence is not a conflict", not _c5)
_c6 = associations("mergeable is MERGEABLE, and mergeStateStatus is CLEAN or UNSTABLE",
                   "mergeStateStatus", MERGEABLE_ONLY)
check("CONTROL: ...and its second clause is clean too", not _c6)

# --- (1b) the bypass gate must be fail-closed and revocable ------------------
# Review on #2656: a DENYLIST of bad conclusions silently permits any state nobody enumerated.
# GitHub's check-run conclusions include `stale` and `startup_failure`; legacy commit statuses
# add `error`. Only an allowlist is fail-closed against states GitHub has not invented yet.
print("admin bypass is fail-closed and revocable")

check("admin_bypass exposes an `allowed` switch the owner can flip",
      isinstance(bypass.get("allowed"), bool))
check("admin_bypass uses no conclusion DENYLIST",
      isinstance(req, dict) and "no_required_check_concluded" not in req)
if isinstance(req, dict):
    allow_done = req.get("required_check_conclusion_allowlist")
    allow_pend = req.get("required_check_pending_allowlist")
    check("required_check_conclusion_allowlist is a non-empty list",
          isinstance(allow_done, list) and len(allow_done) > 0)
    check("required_check_pending_allowlist is a non-empty list",
          isinstance(allow_pend, list) and len(allow_pend) > 0)
    # A bypass that tolerates a failure is not a bypass exception, it is a bypass.
    FORBIDDEN = {"FAILURE", "TIMED_OUT", "CANCELLED", "ACTION_REQUIRED",
                 "STALE", "STARTUP_FAILURE", "ERROR"}
    leaked = FORBIDDEN & set(allow_done or []) | FORBIDDEN & set(allow_pend or [])
    check(f"no failing/aborted state is allowlisted: {sorted(leaked)}", not leaked)
    check("the two allowlists are disjoint",
          not (set(allow_done or []) & set(allow_pend or [])))

# The skill must honour the off switch, and must consult BOTH allowlists.
# (asserted against the skill body in section 4)

# --- (1c) the bypass must not skip gates readiness independently requires ----
# `--admin` bypasses EVERY branch protection, not only the check gate. Review on #2656: with a
# stalled runner AND a CHANGES_REQUESTED review, every check-shaped requirement still held.
print("admin bypass reproduces the readiness review gates")

readiness = " ".join(whole.get("readiness_definition", []))
if isinstance(req, dict):
    check("bypass refuses a draft PR", req.get("is_draft") is False)
    # An ALLOWLIST, like every other state requirement here. A `*_not_in` denylist would let a
    # fourth PullRequestReviewDecision value pass by omission — the same fail-open the
    # conclusion denylist had (icn#2656 review).
    check("review decision uses an allowlist, not a denylist",
          "review_decision_not_in" not in req
          and isinstance(req.get("review_decision_allowlist"), list))
    rd_allow = req.get("review_decision_allowlist") or []
    check("bypass refuses CHANGES_REQUESTED", "CHANGES_REQUESTED" not in rd_allow)
    check("bypass refuses REVIEW_REQUIRED", "REVIEW_REQUIRED" not in rd_allow)
    check("the live null decision is admitted EXPLICITLY, not by omission", None in rd_allow)
    check("every non-null allowlisted decision is a real PullRequestReviewDecision",
          all(v in REVIEW_DECISIONS for v in rd_allow if v is not None))
    check("bypass refuses unresolved review threads",
          req.get("unresolved_review_threads") == 0)
    # Tie the two documents together so neither can drift alone.
    if "CHANGES_REQUESTED" in readiness:
        check("readiness requires no CHANGES_REQUESTED, and so does the bypass",
              "CHANGES_REQUESTED" not in rd_allow)
    if "threads resolved" in readiness:
        check("readiness requires resolved threads, and so does the bypass",
              req.get("unresolved_review_threads") == 0)
check("admin_bypass states why it must mirror readiness",
      bool(bypass.get("scope_note")))

# --- (2) required/non-required sets cannot overlap ---------------------------
required = set(merge.get("required_checks", []))
non_blocking = set(merge.get("non_blocking_checks", []))
check("required_checks is non-empty", len(required) > 0)
check(f"required_checks and non_blocking_checks are disjoint: {sorted(required & non_blocking)}",
      not (required & non_blocking))

# --- (3) the owner must not duplicate its own strategy -----------------------
print("no duplicated strategy inside the owner")
auto = merge.get("auto_merge", {})
check("auto_merge carries no executable command string",
      not any(k in auto for k in ("command", "cmd", "run")))
check("auto_merge points at default_strategy rather than naming a strategy",
      auto.get("strategy_from") == "default_strategy")
auto_blob = json.dumps({k: v for k, v in auto.items() if k != "note"})
check(f"auto_merge hardcodes no --merge/--squash/--rebase flag: {auto_blob[:80]}",
      STRATEGY_FLAG_RE.search(auto_blob) is None)
check("auto_merge still declares its flags structurally",
      isinstance(auto.get("gh_flags"), list) and auto["gh_flags"])

# CONTROL: the checks above would be vacuous if they passed on the pre-fix shape.
_pre_fix_auto = {"command": "gh pr merge <N> --auto --squash", "use_when": "..."}
check("CONTROL: the pre-#2656 auto_merge shape would FAIL these checks",
      "command" in _pre_fix_auto and STRATEGY_FLAG_RE.search(json.dumps(_pre_fix_auto)))
_pre_fix_bypass = {"condition": "Required checks are green AND mergeStateStatus=MERGEABLE ..."}
check("CONTROL: the pre-#2656 admin_bypass shape would FAIL these checks",
      "requires" not in _pre_fix_bypass
      and "mergeStateStatus=MERGEABLE" in json.dumps(_pre_fix_bypass))

# Commands live in fenced blocks or in `- \`...\`` bullets. Prose about a command is not a
# command: an earlier revision of this checker flagged the sentence explaining why a bare
# `gh pr view` is wrong, which is the checker failing on its own documentation.
EVAL_RE = re.compile(r"\beval\s+[\"'`$]")


def extract_commands(markdown: str) -> list[str]:
    out, in_fence = [], False
    for line in markdown.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            if line.strip():
                out.append(line.strip())
            continue
        # An inline command BULLET only: `- \`gh pr checks <N>\`, ...`. A command named
        # mid-paragraph is prose about a command, not an instruction to run one.
        if not line.lstrip().startswith("- "):
            continue
        for m in re.finditer(r"`([^`]+)`", line):
            tok = m.group(1).strip()
            if tok.startswith(("gh ", "git ", "jq ", "eval ")):
                out.append(tok)
    return out


# --- (4) consumers must not restate what they claim to load ------------------
print("consumers do not duplicate policy values")

registry = json.loads(SKILLS.read_text(encoding="utf-8"))
skill_paths = []
for entry in registry.get("skills", {}).get("icn_level", []):
    if entry.get("name") in ("merge-pr", "merge-prs", "integrate-pr-stack"):
        skill_paths.append((entry["name"], ROOT / entry["canonical_path"],
                            [ROOT / m["path"] for m in entry.get("provider_mirrors", [])]))
check("the merge skills are resolvable from the canonical registry", len(skill_paths) >= 1)

# `merge-pr` is the thin skill that declares policy.json canonical AND lists the strategy under
# never_hardcode. It is held to its own contract: no literal strategy flag in a merge command.
MERGE_CMD_RE = re.compile(r"gh pr merge[^\n`]*--(merge|squash|rebase)\b")
for name, canonical, mirrors in skill_paths:
    body = canonical.read_text(encoding="utf-8")
    if name != "merge-pr":
        continue
    hits = MERGE_CMD_RE.findall(body)
    check(f"{name}: no `gh pr merge --<strategy>` literal; the strategy is substituted: {hits}",
          not hits)
    check(f"{name}: substitutes the strategy from the policy", "${STRATEGY}" in body)
    # Only real commands are inspected, never prose. A sentence explaining why a bare
    # `gh pr view` is wrong must not be mistaken for one.
    commands = extract_commands(body)
    check(f"{name}: does not shell-eval policy values",
          not any(EVAL_RE.search(c) for c in commands))
    bare = [c for c in commands
            if re.search(r"gh pr (view|checks|merge)\b", c)
            and not re.search(r"gh pr (view|checks|merge) +(<N>|\$)", c)]
    check(f"{name}: every gh pr view/checks/merge command is explicitly addressed: {bare}",
          not bare)
    # Review on #2656: `allowed: false` must actually revoke the bypass. A skill that only
    # checks `.requires` treats a present `false` as neither absent nor ambiguous.
    check(f"{name}: gates the admin path on .merge.admin_bypass.allowed",
          "admin_bypass.allowed" in body)
    check(f"{name}: consults both required-check allowlists",
          "required_check_conclusion_allowlist" in body
          and "required_check_pending_allowlist" in body)
    check(f"{name}: names the states an allowlist must exclude",
          "STARTUP_FAILURE" in body and "STALE" in body)
    # Review round 3: a merge gate is only as good as the commit and branch it was computed
    # against, and as complete as the thread page it read.
    merges = [c for c in commands if re.search(r"gh pr merge\b", c)]
    check(f"{name}: every merge invocation pins the inspected head: {len(merges)} found",
          bool(merges) and all("--match-head-commit" in c for c in merges))
    check(f"{name}: captures headRefOid to pin against", "headRefOid" in body)
    check(f"{name}: reads protection for the PR's actual base, not a hardcoded main",
          "${BASE}/protection" in body
          and "branches/main/protection" not in body)
    # Inspect the COMMAND, not the body: an earlier revision of this check read the prose
    # mention of `--paginate` and stayed green when the flag was removed from the query itself.
    # Same prose/command confusion this file already had to learn once.
    # The GraphQL call spans several fenced lines, so the machinery is asserted across the
    # extracted COMMAND lines collectively — never against the body, where a prose mention of
    # `--paginate` kept this green after the flag was removed from the query itself.
    cmd_text = " ".join(commands)
    check(f"{name}: reads review threads at all", "reviewThreads" in cmd_text)
    check(f"{name}: and paginates them rather than reading one page",
          all(tok in cmd_text for tok in ("--paginate", "hasNextPage", "endCursor")))

    # Review round 4: `--auto` waits for BRANCH PROTECTION's requirements, not for
    # policy.json's. A policy-required check that is not a live protection context would not
    # hold the merge. And `--auto` returns without merging, so the procedure must not fall
    # through to post-merge steps or report a merge it has not confirmed.
    # Review round 5: filtering on state=="PENDING" alone missed QUEUED/IN_PROGRESS/WAITING/
    # REQUESTED/EXPECTED — the same incomplete enumeration the pending ALLOWLIST exists to
    # avoid. `gh pr checks --json bucket` normalises all of them to one value.
    check(f"{name}: proves pending policy-required checks are live protection contexts",
          "required_status_checks.contexts" in cmd_text)
    check(f"{name}: detects pending checks by normalised bucket, not one state spelling",
          'bucket=="pending"' in cmd_text.replace(" ", "")
          and 'state=="PENDING"' not in cmd_text.replace(" ", ""))
    # A permitted exception the procedure never invokes is one the skill cannot perform.
    admin_cmds = [c for c in commands if "--admin" in c]
    check(f"{name}: actually invokes the authorized admin merge: {len(admin_cmds)}",
          bool(admin_cmds) and all("--match-head-commit" in c for c in admin_cmds))
    check(f"{name}: stops after arming auto-merge instead of reporting a merge",
          "Then STOP" in body and "does not merge" in body)
    check(f"{name}: confirms merged state before the post-merge steps",
          "state,mergedAt,mergeCommit" in cmd_text)
    check(f"{name}: pulls the actual base branch, not a hardcoded main",
          "git checkout main" not in body)
    check(f"{name}: its output contract admits a not-merged outcome",
          "Auto-merge armed" in body and "has **not** merged" in body)

    # The bypass overrides every protection, so the skill must check the review gates too.
    check(f"{name}: checks the gates --admin would also bypass",
          all(t in body for t in ("is_draft", "review_decision_allowlist",
                                  "unresolved_review_threads")))

    for mirror in mirrors:
        check(f"{name}: provider mirror {mirror.relative_to(ROOT)} is byte-identical",
              mirror.read_text(encoding="utf-8") == body)

# CONTROL: the regex must actually catch the pre-fix skill text.
check("CONTROL: the pre-#2651 skill body would FAIL the strategy-literal check",
      bool(MERGE_CMD_RE.search("3. If all checks are green, merge:\n   - `gh pr merge --merge`")))
check("CONTROL: a bare `gh pr view --json state` would FAIL the addressing check",
      not re.search(r"gh pr (view|checks|merge) +(<N>|\$)", "gh pr view --json state"))

# --- (5) a disabled bypass must be expressible and must be honoured ----------
print("a disabled bypass is expressible")

_disabled = dict(bypass)
_disabled["allowed"] = False
check("CONTROL: `allowed: false` is a valid shape the owner can write",
      _disabled["allowed"] is False and "requires" in _disabled)
# The skill's own text must reach `allowed` BEFORE the detailed requirements, otherwise a
# revoked bypass is still performed.
for name, canonical, _m in skill_paths:
    if name != "merge-pr":
        continue
    body = canonical.read_text(encoding="utf-8")
    i_allowed = body.find("admin_bypass.allowed")
    i_requires = body.find(".requires.mergeable")
    check("merge-pr: `allowed` is checked before the detailed requirements",
          i_allowed != -1 and i_requires != -1 and i_allowed < i_requires)


# --- (6) control-flow reachability, not just command existence ---------------
# Five rounds of static string checks passed while the procedure contained an unreachable
# security-sensitive branch (icn#2656 round 6): the `--admin` command EXISTED, and every state
# that could qualify for it was consumed by the pending/auto handler two steps earlier. Strings
# prove a command exists; these prove it is reachable in the state it handles, and unreachable
# without escalation-specific authorization.
#
# Deliberately a lexer over numbered steps, not a parser: the properties are orderings and
# containments over step indices, the smallest model that can express "step 3 consumes the
# state step 5 needs".
print("admin branch reachability and authorization")

ADMIN_CMD = re.compile(r"gh pr merge\b[^\n]*--admin")
AUTO_CMD = re.compile(r"auto_merge\.gh_flags|--auto\b")
STOPS = re.compile(r"\*\*Then STOP\.\*\*|\bSTOP the procedure\b")
AUTH_ARG = re.compile(r"\$ARGUMENTS[^\n]*--admin|includes\s+`--admin`")
AUTH_FRESH = re.compile(r"fresh(?:ly)?\s+(?:explicit\s+)?confirmation"
                        r"|confirmation[^\n]{0,80}administrator privileges", re.I)
AUTH_SCOPE = re.compile(r"this PR|that PR number", re.I)


def numbered_steps(md: str):
    """[(n, text)] for the top-level numbered steps of the procedure."""
    body = md.split("## Steps", 1)[-1].split("## Output", 1)[0]
    parts = re.split(r"^(\d+)\.\s", body, flags=re.M)
    return [(int(parts[i]), parts[i + 1]) for i in range(1, len(parts) - 1, 2)]


def step_commands(text: str):
    out, inf = [], False
    for ln in text.splitlines():
        if ln.lstrip().startswith("```"):
            inf = not inf
            continue
        if inf and ln.strip():
            out.append(ln.strip())
    return out


for name, canonical, _m in skill_paths:
    if name != "merge-pr":
        continue
    md = canonical.read_text(encoding="utf-8")
    # Prose assertions match against this. Markdown wraps phrases across lines, and a literal
    # match has silently failed on the very text it was written to find five times in this
    # file's history. Commands are still matched against `md`/`commands`, never `flat`.
    flat = " ".join(md.split())
    stepping = numbered_steps(md)
    admin_i = auto_i = None
    for idx, (n, t) in enumerate(stepping):
        cmds = step_commands(t)
        if any(ADMIN_CMD.search(c) for c in cmds):
            admin_i = idx
        if any(AUTO_CMD.search(c) for c in cmds):
            auto_i = idx
    check("the procedure parses into numbered steps", len(stepping) >= 5)
    check("an --admin merge command exists", admin_i is not None)
    check("an auto-merge command exists", auto_i is not None)

    if admin_i is not None and auto_i is not None:
        an, at = stepping[admin_i]
        # THE ROUND-6 DEFECT. The admin exception requires a PENDING required check; if the
        # pending handler runs first and terminates, no qualifying state ever reaches it.
        check(f"admin branch precedes the pending/auto handler (admin=step {an}, "
              f"auto=step {stepping[auto_i][0]})", admin_i < auto_i)
        stopping = [n for n, t in stepping[:admin_i] if STOPS.search(t)]
        check(f"no step before the admin branch terminates the procedure: {stopping}",
              not stopping)

        # Authorization must gate EXECUTION, not merely be mentioned somewhere.
        cmd_pos = min((m.start() for m in ADMIN_CMD.finditer(at)), default=len(at))
        pre = at[:cmd_pos]
        check("the admin step demands the --admin argument or a fresh confirmation",
              bool(AUTH_ARG.search(at) or AUTH_FRESH.search(at)))
        check("that demand appears BEFORE the --admin command in the same step",
              bool(AUTH_ARG.search(pre) or AUTH_FRESH.search(pre)))
        check("authorization is scoped to this PR / this invocation",
              bool(AUTH_SCOPE.search(pre)))
        check("a generic prior yes is explicitly insufficient",
              bool(re.search(r"generic|retroactiv", at, re.I)))
        check("a failed admin gate refuses rather than falling back",
              bool(re.search(r"refuse the escalation|not a downgrade|do not fall back", at, re.I)))
        check("human authorization is stated not to lift eligibility",
              "never_for" in at and bool(re.search(r"not sufficient|does not lift", at, re.I)))

        # No --admin anywhere else, and specifically not on the ordinary path.
        others = [n for j, (n, t) in enumerate(stepping)
                  if j != admin_i and any(ADMIN_CMD.search(c) for c in step_commands(t))]
        check(f"no --admin command outside the authorized branch: {others}", not others)
        # Whitespace-normalised: the phrase wraps across lines in the rendered markdown, and
        # a literal match silently failed on the very text it was written to find.
        ordinary = " ".join(stepping[auto_i][1].split())  # normalised: see `flat` above
        check("the ordinary path forbids escalation in prose too",
              bool(re.search(r"never to escalate|no admin escalation", ordinary, re.I)))
        # Bounded audit, round 6: 5a/5b/5c covered green and both pending shapes and nothing
        # else, so a FAILING required check fell through step 5 into the merged-state check and
        # was refused only implicitly. An unhandled state on a merge path is a state nobody
        # decided about.
        check("the ordinary path handles a FAILING required check explicitly",
              bool(re.search(r"A required check has failed", ordinary))
              and bool(re.search(r"exhaustive", ordinary, re.I)))
        check("and refuses rather than offering escalation",
              bool(re.search(r"do not offer, suggest or escalate", ordinary, re.I)))
        # Final-head review of 10de4f3d. Route 2 of the authorization gate (fresh confirmation
        # after a block) was unreachable: step 4 runs before step 5 and nothing routed back —
        # the same unreachable-branch class as the round-6 finding, reintroduced one step over.
        check("a blocked stalled state can return to the admin gate",
              bool(re.search(r"return to step 4", ordinary, re.I)))
        check("...but only from the stalled states, never from the failing one",
              bool(re.search(r"5d must never offer one|do not offer, suggest or escalate",
                             flat, re.I)))
        # `--match-head-commit` pins the head and nothing pins the base; a retarget leaves the
        # head unchanged while the admin merge lands on protection never inspected.
        admin_txt = stepping[admin_i][1]
        admin_flat = " ".join(admin_txt.split())
        check("the admin path revalidates head AND base immediately before the bypass",
              "headRefOid,baseRefName" in admin_txt
              and bool(re.search(r"nothing pins the base", admin_flat, re.I)))
        check("a moved head or base refuses rather than proceeding",
              bool(re.search(r"refuse and start over", admin_flat, re.I)))
        # An unsuccessful protection load is missing evidence, not "no requirements".
        check("an unavailable protection load is treated as missing evidence",
              bool(re.search(r"unsuccessful load is missing evidence", flat, re.I))
              and "LIVE=UNAVAILABLE" in md)

# CONTROLS: the model must reject the round-5 shape it was written against.
_r5 = ("## Steps\n\n3. pending\n\n   ```bash\n   gh pr merge <N> --auto --squash\n   ```\n\n"
       "   **Then STOP.**\n\n5. bypass\n\n   ```bash\n   gh pr merge <N> --admin --squash\n   ```\n\n## Output\n")
_st = numbered_steps(_r5)
_ai = next(i for i, (n, t) in enumerate(_st) if any(ADMIN_CMD.search(c) for c in step_commands(t)))
_ui = next(i for i, (n, t) in enumerate(_st) if any(AUTO_CMD.search(c) for c in step_commands(t)))
check("CONTROL: the round-5 ordering (admin after auto) is rejected", not (_ai < _ui))
check("CONTROL: the round-5 STOP before the admin branch is detected",
      bool([n for n, t in _st[:_ai] if STOPS.search(t)]))
check("CONTROL: a step with no authorization demand is rejected",
      not (AUTH_ARG.search(_st[_ai][1]) or AUTH_FRESH.search(_st[_ai][1])))

print()
if failures:
    print(f"check-merge-policy: {len(failures)} failure(s)")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("check-merge-policy: clean")
