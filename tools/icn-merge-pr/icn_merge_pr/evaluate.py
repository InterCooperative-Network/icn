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

    # --- is this merge actually an ORDINARY one? -------------------------------------------------
    # First, because it is a precondition for everything below rather than one gate among them.
    # The merge request carries a head SHA and a strategy; it does not carry the readiness this
    # program just proved. What makes the request ordinary is that the SERVER re-applies branch
    # protection to it. If protection does not apply to the credential doing the merging — a
    # repository administrator's, say — then GitHub may accept the merge even though a required
    # check, a review gate or the base tip moved after the final refresh, and no `--admin` need
    # appear anywhere for that to happen. Review proved an earlier judgement of mine wrong here:
    # this evaluator DOES rely on enforce_admins, because its whole claim to be "ordinary" rests
    # on the server, not on the shape of the request.
    if not snap.protection.enforce_admins:
        refuse(codes.REFUSED_PROTECTION_BYPASSABLE,
               f"branch protection on {snap.default_branch!r} does not apply to administrators "
               f"and other bypass-capable roles, so the server would not re-enforce it against "
               f"the credential performing this merge. An ordinary merge is ordinary only when "
               f"the server enforces protection against the caller; this program has no "
               f"privileged path and will not stand in for one.")
    # `enforce_admins` alone does not close every bypass path: classic pull-request allowances and
    # bypass actors on an actively enforced ruleset each let some caller merge past the gates while
    # that flag is true. This program does NOT ask whether the current caller matches one of them —
    # that would be an authorization engine, and identity resolution is exactly the authority this
    # primitive should not hold. The EXISTENCE of an active bypass path is what makes the merge
    # non-ordinary, so any open path refuses, whoever it belongs to.
    open_paths = snap.bypass.open_paths
    if open_paths:
        refuse(codes.REFUSED_PROTECTION_BYPASSABLE,
               f"branch protection on {snap.default_branch!r} has {len(open_paths)} configured "
               f"bypass path(s): {list(open_paths)}. The ordinary merger mutates only when no "
               f"server-side bypass path exists at all; if this repository needs bypass actors "
               f"that is a privileged authority design and does not belong inside ordinary merge.")

    # --- configuration soundness ---------------------------------------------------------------
    # Fail closed on drift between the pinned policy and live branch protection. Either direction
    # is a real disagreement about what gates a merge, and neither owner may be quietly preferred.
    #
    # WHAT THE GATE COVERS, and why it is not "every branch-protection setting". A control belongs
    # here when the pinned policy makes a checkable declaration about it AND this evaluator relies
    # on it to decide readiness: the required-check set and its producers, whether a branch must be
    # current, and how many approvals are required. Live protection may not quietly erase any of
    # those, in either direction.
    #
    # `enforce_admins` was excluded here on the reasoning that this evaluator has no privileged
    # path whose availability could depend on it. Review showed that reasoning to be wrong: the
    # evaluator relies on the setting not to gain a privilege but to be denied one, because a
    # bypass-capable credential turns an ordinary request into a privileged merge server-side.
    # It is a gated control now, in both directions.
    # DERIVED, not a second policy switch. Policy already states the requirement — zero unresolved
    # review threads — and the only question here is whether the SERVER will hold that requirement
    # at the moment of the merge. Opening or resolving a thread does not move the head SHA, so the
    # head pin cannot bind thread state to the mutation and the client-side gate can only describe
    # a moment that has already passed; GitHub's own conversation-resolution protection is what
    # closes that window. Deriving it keeps ONE owner for the requirement: if policy ever permitted
    # unresolved threads, this stops asking for an enforcement it no longer needs.
    if policy.max_unresolved_threads == 0 and not snap.protection.conversation_resolution:
        refuse(codes.REFUSED_POLICY_DRIFT,
               f"pinned policy at {policy.oid[:12]} requires "
               f"{policy.max_unresolved_threads} unresolved review thread(s), but live protection "
               f"on {snap.default_branch!r} does not require conversation resolution. Resolving "
               f"or opening a thread does not change the head SHA, so nothing this program pins "
               f"can stop a thread appearing between its final check and the merge; the server "
               f"has to be the one enforcing it.")
    want_admins = snap.policy.require_enforce_admins
    if want_admins is not None and snap.protection.enforce_admins != want_admins:
        refuse(codes.REFUSED_POLICY_DRIFT,
               f"pinned policy at {policy.oid[:12]} declares enforce_admins={want_admins} but "
               f"live protection on {snap.default_branch!r} reports "
               f"{snap.protection.enforce_admins}; whether protection applies to the caller is "
               f"not something live configuration may quietly change")
    want_approvals = snap.policy.require_approvals
    if want_approvals is not None and \
            snap.protection.required_approving_review_count != want_approvals:
        refuse(codes.REFUSED_POLICY_DRIFT,
               f"pinned policy at {policy.oid[:12]} declares required_approvals={want_approvals} "
               f"but live protection on {snap.default_branch!r} requires "
               f"{snap.protection.required_approving_review_count}; live protection may not erase "
               f"an approval requirement the canonical policy states")
    want_strict = snap.policy.require_strict_status_checks
    if want_strict is not None and snap.protection.strict != want_strict:
        refuse(codes.REFUSED_POLICY_DRIFT,
               f"pinned policy at {policy.oid[:12]} declares strict_up_to_date={want_strict} but "
               f"live protection on {snap.default_branch!r} reports strict="
               f"{snap.protection.strict}; the up-to-date requirement policy relies on is not the "
               f"one in force")
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
