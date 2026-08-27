"""The gate. Pure: a Snapshot in, a Decision out. No I/O, no clock, no network.

That purity is what makes the behaviour testable without merging anything, which is the whole
reason merge semantics moved out of Markdown. Every gate below is the structured `ready_when`
contract in `ops/state/truth/policy.json`, consumed as data — this file does not restate what it
declares, and it does not carry a second copy of the required-check list or its count.
"""

from __future__ import annotations

from dataclasses import dataclass

from . import codes
from .snapshot import PENDING, Snapshot


@dataclass(frozen=True)
class Reason:
    code: str
    detail: str


@dataclass(frozen=True)
class Decision:
    outcome: str
    reasons: tuple[Reason, ...]

    @property
    def ready(self) -> bool:
        return self.outcome == codes.READY


def _check_verdicts(snap: Snapshot) -> tuple[list[str], list[str], list[str]]:
    """(failed, pending, missing) over EVERY canonical required check.

    The canonical set is the union of the pinned policy list and the live protection contexts. It
    is only ever a union in principle — the drift gate refuses when they differ — but accounting
    over the union means a check named by either owner can never go unaccounted for.

    A required check GitHub never reported is MISSING, not absent-so-fine. A green non-required
    check is not consulted at all, so it can never stand in for a missing required one.

    THE NAME IS NOT THE CHECK. Branch protection can pin a required check to one GitHub App, and
    this repository does exactly that. A check matching only by name would let a green result from
    some other producer — another App, or a plain commit status posted by any token holding
    `repo:status` — satisfy a gate the configured producer never passed. Where protection names a
    producer, only that producer's runs are consulted; everything else is as good as absent.
    """
    canonical = sorted(snap.policy.required_checks | snap.protection.required_contexts)
    allow = snap.policy.check_conclusion_allowlist
    failed: list[str] = []
    pending: list[str] = []
    missing: list[str] = []
    for name in canonical:
        reported = snap.checks.get(name, ())
        want = snap.protection.required_bindings.get(name)
        occurrences = ([o for o in reported if o.app_id == want] if want is not None
                       else list(reported))
        if not occurrences:
            missing.append(f"{name} (no run from the required producer, app {want})"
                           if want is not None and reported else name)
            continue
        # Worst wins. A re-run that went green does not erase a red occurrence for this gate.
        outcomes = [o.outcome for o in occurrences]
        if any(o != PENDING and o not in allow for o in outcomes):
            failed.append(f"{name} ({', '.join(outcomes)})")
        elif any(o == PENDING for o in outcomes):
            pending.append(name)
    return failed, pending, missing


def evaluate(snap: Snapshot, strategy: str) -> Decision:
    """Every gate, in a fixed order. The outcome is the FIRST refusal; all reasons are reported."""
    reasons: list[Reason] = []
    policy = snap.policy

    def refuse(code: str, detail: str) -> None:
        reasons.append(Reason(code, detail))

    # --- configuration soundness ---------------------------------------------------------------
    # Fail closed on drift between the pinned policy and live branch protection. Either direction
    # is a real disagreement about what gates a merge, and neither owner may be quietly preferred.
    declared = snap.policy.required_checks
    live = snap.protection.required_contexts
    if declared != live:
        only_policy = sorted(declared - live)
        only_live = sorted(live - declared)
        refuse(codes.REFUSED_POLICY_DRIFT,
               f"pinned policy at {policy.oid[:12]} and live protection on "
               f"{snap.default_branch!r} disagree about the required-check set; "
               f"policy-only={only_policy} protection-only={only_live}")

    # --- the PR itself --------------------------------------------------------------------------
    if snap.state != "OPEN":
        refuse(codes.REFUSED_STATE, f"PR #{snap.number} is {snap.state}, not OPEN")
    if snap.is_draft != policy.ready_is_draft:
        refuse(codes.REFUSED_DRAFT,
               f"PR #{snap.number} is a draft; policy admits is_draft={policy.ready_is_draft}")
    if snap.mergeable != policy.ready_mergeable:
        refuse(codes.REFUSED_NOT_MERGEABLE,
               f"mergeable is {snap.mergeable}, and policy admits only {policy.ready_mergeable}")
    if snap.merge_state_status not in policy.ready_merge_states:
        refuse(codes.REFUSED_MERGE_STATE,
               f"mergeStateStatus is {snap.merge_state_status}; policy admits "
               f"{sorted(policy.ready_merge_states)}")

    # --- review ---------------------------------------------------------------------------------
    if snap.review_decision not in policy.review_decision_allowlist:
        refuse(codes.REFUSED_REVIEW,
               f"reviewDecision {snap.review_decision!r} is not in the policy allowlist "
               f"{list(policy.review_decision_allowlist)}")
    if "CHANGES_REQUESTED" in snap.opinionated_review_states:
        refuse(codes.REFUSED_REVIEW,
               "a reviewer's latest opinionated review requests changes")
    needed = snap.protection.required_approving_review_count
    if needed > 0 and snap.review_decision != "APPROVED":
        refuse(codes.REFUSED_REVIEW,
               f"live protection requires {needed} approving review(s) and reviewDecision is "
               f"{snap.review_decision!r}")

    # --- threads ---------------------------------------------------------------------------------
    if snap.unresolved_threads > policy.max_unresolved_threads:
        refuse(codes.REFUSED_THREADS,
               f"{snap.unresolved_threads} unresolved review thread(s) of "
               f"{snap.review_threads_total} total; policy admits "
               f"{policy.max_unresolved_threads}")

    # --- required checks --------------------------------------------------------------------------
    failed, pending, missing = _check_verdicts(snap)
    if failed:
        refuse(codes.REFUSED_REQUIRED_CHECK_FAILED, f"required check(s) not accepted: {failed}")
    if pending:
        refuse(codes.REFUSED_REQUIRED_CHECK_PENDING,
               f"required check(s) still running: {pending} — pending is not ready")
    if missing:
        refuse(codes.REFUSED_REQUIRED_CHECK_MISSING,
               f"required check(s) GitHub never reported: {missing} — a check this program cannot "
               "account for is not ready")

    # --- nothing may be deferred -------------------------------------------------------------------
    if policy.require_queue_absent and snap.merge_queue_present:
        refuse(codes.REFUSED_MERGE_QUEUE,
               f"branch {snap.default_branch!r} has a merge queue; an ordinary merge there would "
               "enqueue rather than merge, which is a deferred outcome this program may not "
               "produce")
    if policy.require_not_in_queue and snap.is_in_merge_queue:
        refuse(codes.REFUSED_ALREADY_QUEUED, f"PR #{snap.number} is already in a merge queue")
    if policy.require_auto_merge_absent and snap.auto_merge_armed:
        refuse(codes.REFUSED_ALREADY_AUTO_ARMED,
               f"PR #{snap.number} already has an auto-merge request armed by another actor; it "
               "is reported, never disarmed")

    # --- strategy -----------------------------------------------------------------------------------
    if strategy not in snap.allowed_merge_methods:
        refuse(codes.REFUSED_STRATEGY_UNAVAILABLE,
               f"the repository does not allow the {strategy!r} merge method "
               f"(allowed: {sorted(snap.allowed_merge_methods)})")

    if reasons:
        return Decision(reasons[0].code, tuple(reasons))
    return Decision(codes.READY, ())
