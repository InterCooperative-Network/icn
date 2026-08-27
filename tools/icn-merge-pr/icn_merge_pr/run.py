"""Orchestration: evaluate, refresh, re-evaluate, then merge once — or refuse.

EVALUATION IS NOT PERMISSION
A passing evaluation describes a moment that has already gone by. Immediately before mutating,
this module calls the SAME snapshot loader again, compares the pinned identities, and runs EVERY
gate against the new evidence. Anything that moved refuses and demands a fresh evaluation; nothing
is carried forward from the first pass except the identities being compared.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from . import codes
from .errors import MergeToolError
from .evaluate import Decision, Reason, evaluate
from .merge import perform_merge
from .snapshot import Snapshot, load_snapshot
from .strategy import select_strategy


@dataclass
class Result:
    command: str
    owner: str
    name: str
    number: int
    outcome: str
    reasons: list[Reason] = field(default_factory=list)
    evidence: dict | None = None
    strategy: dict | None = None
    merge: dict | None = None

    def as_dict(self) -> dict:
        return {
            "tool": "icn-merge-pr",
            "command": self.command,
            "repository": f"{self.owner}/{self.name}",
            "pr": self.number,
            "outcome": self.outcome,
            "reasons": [{"code": r.code, "detail": r.detail} for r in self.reasons],
            "evidence": self.evidence,
            "strategy": self.strategy,
            "merge": self.merge,
        }


def _evidence(snap: Snapshot) -> dict:
    return {
        "state": snap.state,
        "is_draft": snap.is_draft,
        "head_oid": snap.head_oid,
        "base_ref_name": snap.base_ref_name,
        "base_ref_oid": snap.base_ref_oid,
        "default_branch": snap.default_branch,
        "default_branch_oid": snap.default_branch_oid,
        "mergeable": snap.mergeable,
        "merge_state_status": snap.merge_state_status,
        "review_decision": snap.review_decision,
        "review_threads_total": snap.review_threads_total,
        "unresolved_review_threads": snap.unresolved_threads,
        "merge_queue_present": snap.merge_queue_present,
        "is_in_merge_queue": snap.is_in_merge_queue,
        "auto_merge_armed": snap.auto_merge_armed,
        "required_check_outcomes": {
            name: [{"outcome": o.outcome, "app_id": o.app_id}
                   for o in snap.checks.get(name, ())]
            for name in sorted(snap.policy.required_checks | snap.protection.required_contexts)
        },
        "protection": {
            "branch": snap.default_branch,
            "required_contexts": sorted(snap.protection.required_contexts),
            "required_producers": {k: v for k, v in
                                   sorted(snap.protection.required_bindings.items())},
            "required_approving_review_count": snap.protection.required_approving_review_count,
            "strict": snap.protection.strict,
            "enforce_admins": snap.protection.enforce_admins,
        },
        "bypass": {
            "enforce_admins": snap.bypass.enforce_admins,
            "classic_allowances": list(snap.bypass.classic_allowances),
            "rulesets": [{"id": r.id, "name": r.name, "enforcement": r.enforcement,
                          "enforcing": r.enforcing, "bypass_actors": list(r.bypass_actors)}
                         for r in snap.bypass.rulesets],
            "open_paths": list(snap.bypass.open_paths),
        },
        "policy": {
            "loaded_from_oid": snap.policy_oid,
            "sha256": snap.policy_sha256,
            "default_strategy": snap.policy.default_strategy,
            "live_source": snap.policy.live_source,
        },
    }


# Identity that must not move between evaluation and mutation, with the code each change reports.
_PINNED = (
    ("head_oid", codes.REFUSED_HEAD_CHANGED, "the PR head"),
    ("base_ref_name", codes.REFUSED_BASE_CHANGED, "the PR base branch"),
    ("base_ref_oid", codes.REFUSED_BASE_CHANGED, "the PR base commit"),
    ("default_branch", codes.REFUSED_DEFAULT_BRANCH_CHANGED, "the default branch"),
    ("default_branch_oid", codes.REFUSED_DEFAULT_BRANCH_CHANGED, "the default-branch commit"),
)


def detect_race(before: Snapshot, after: Snapshot) -> Reason | None:
    """The first pinned identity that moved, or None. Policy is compared by content, not by name."""
    for attribute, code, label in _PINNED:
        was, now = getattr(before, attribute), getattr(after, attribute)
        if was != now:
            return Reason(code, f"{label} changed between evaluation and merge: {was!r} -> {now!r}"
                                " — evaluate again against the new state")
    if before.policy_oid != after.policy_oid or before.policy_sha256 != after.policy_sha256:
        return Reason(codes.REFUSED_POLICY_DRIFT,
                      f"merge policy changed between evaluation and merge: "
                      f"{before.policy_oid[:12]}/{before.policy_sha256[:12]} -> "
                      f"{after.policy_oid[:12]}/{after.policy_sha256[:12]}")
    return None


# The evaluator's own source. If either of these changed on the default branch since the copy now
# running was installed, that copy is out of date about how to decide a merge.
EVALUATOR_PATHS = ("tools/icn-merge-pr", "scripts/check-merge-policy-schema.py")


def stale_evaluator(client, owner: str, name: str, installed: str, live: str) -> str | None:
    """The first evaluator path that differs between the installed commit and the live tip.

    Deliberately NOT "the installed commit must be the live tip". Merging advances the default
    branch, so that rule would refuse every merge after the first one — a gate nobody can use is
    a gate that gets bypassed. The question that actually matters is narrower: has THIS PROGRAM,
    or the policy validator it vendored at install time, changed since it was installed. An
    unrelated commit landing on the default branch does not make an evaluator wrong; a fix to the
    evaluator does.

    Fails closed: a path that cannot be resolved at either commit counts as changed.
    """
    if installed == live:
        return None
    for path in EVALUATOR_PATHS:
        was = client.object_oid(owner, name, installed, path)
        now = client.object_oid(owner, name, live, path)
        if was is None or now is None or was != now:
            return path
    return None


def run(client, owner: str, name: str, number: int, *, authorize: bool,
        requested_strategy: str | None = None, exception_reason: str | None = None,
        installed_commit: str | None = None) -> Result:
    """`check` when `authorize` is false; the full evaluate-refresh-merge path when it is true."""
    command = "merge" if authorize else "check"
    result = Result(command=command, owner=owner, name=name, number=number,
                    outcome=codes.READY)
    try:
        snap = load_snapshot(client, owner, name, number)
        strategy, reason = select_strategy(snap.policy, requested_strategy, exception_reason)
    except MergeToolError as exc:
        result.outcome = exc.outcome
        result.reasons = [Reason(exc.outcome, exc.detail),
                          *(Reason(exc.outcome, d) for d in exc.details)]
        return result

    result.evidence = _evidence(snap)
    result.strategy = {
        "selected": strategy,
        "source": "policy_default" if strategy == snap.policy.default_strategy
                  else "operator_exception",
        "reason": reason,
    }
    decision: Decision = evaluate(snap, strategy)
    result.outcome = decision.outcome
    result.reasons = list(decision.reasons)
    if not decision.ready or not authorize:
        return result

    # --- authorised, and ready as of a moment that has now passed. Prove it again. --------------
    try:
        fresh = load_snapshot(client, owner, name, number)
    except MergeToolError as exc:
        result.outcome = exc.outcome
        result.reasons = [Reason(exc.outcome, f"refreshing evidence before merge: {exc.detail}")]
        return result

    result.evidence = _evidence(fresh)
    race = detect_race(snap, fresh)
    if race is not None:
        result.outcome = race.code
        result.reasons = [race]
        return result

    if installed_commit:
        try:
            drifted = stale_evaluator(client, owner, name, installed_commit,
                                      fresh.default_branch_oid)
        except MergeToolError as exc:
            result.outcome = exc.outcome
            result.reasons = [Reason(exc.outcome,
                                     f"checking whether this evaluator is current: {exc.detail}")]
            return result
        if drifted is not None:
            result.outcome = codes.REFUSED_EVALUATOR_STALE
            result.reasons = [Reason(
                codes.REFUSED_EVALUATOR_STALE,
                f"{drifted} changed on {fresh.default_branch} since this copy was installed from "
                f"{installed_commit[:12]} (now {fresh.default_branch_oid[:12]}). The program "
                f"deciding this merge is not the program the default branch now describes — "
                f"reinstall with `python3 tools/icn-merge-pr/install.py` and evaluate again.")]
            return result

    recheck = evaluate(fresh, strategy)
    if not recheck.ready:
        result.outcome = recheck.outcome
        result.reasons = [Reason(r.code, f"on refresh: {r.detail}") for r in recheck.reasons]
        return result

    try:
        merged = perform_merge(client, fresh, strategy)
    except MergeToolError as exc:
        # Reached only BEFORE a request is dispatched — an invalid strategy is the one way in.
        # Everything after dispatch is resolved inside perform_merge against a fresh read, so no
        # path here can report "not merged" on evidence nobody has.
        result.outcome = exc.outcome
        result.reasons = [Reason(exc.outcome, exc.detail)]
        result.merge = {"attempted": False, "confirmed_merged": False, "merge_commit_sha": None}
        return result

    result.outcome = merged.outcome
    result.reasons = [Reason(merged.outcome, merged.detail)]
    result.merge = {
        "attempted": merged.attempted,
        "confirmed_merged": merged.outcome == codes.MERGED,
        "merge_commit_sha": merged.merge_commit_sha,
    }
    return result
