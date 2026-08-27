"""Stable outcome codes for the ordinary-merge evaluator (icn#2651 stage B).

A consumer reads `outcome`, never prose. Prose accompanies a result; it never carries it.

TWO MUTATION OUTCOMES ONLY
`MERGED` and every `REFUSED_*` code. There is no queued, armed, deferred or partially-applied
outcome, because the primitive has none: it merges now or it refuses. `MERGE_UNCONFIRMED` is not a
third mutation outcome — it is the honest report of a mutation whose result GitHub would not
confirm, and it is never success.

THERE IS NO ADMIN CODE. The ordinary evaluator has no privileged path to report.
"""

from __future__ import annotations

# --- success --------------------------------------------------------------------------------
READY = "READY"                        # evaluation only: every gate passed, nothing was mutated
MERGED = "MERGED"                      # GitHub confirmed `merged == true` on a post-mutation read

# --- the mutation attempt could not be confirmed ----------------------------------------------
# Not success. A merge request was accepted by the API but the post-read did not prove the PR
# merged, so neither "merged" nor "nothing happened" is a true statement. A human must look.
MERGE_UNCONFIRMED = "MERGE_UNCONFIRMED"

# --- invocation -------------------------------------------------------------------------------
REFUSED_USAGE = "REFUSED_USAGE"                          # unknown option, bad shape, missing arg
REFUSED_FORBIDDEN_OPTION = "REFUSED_FORBIDDEN_OPTION"    # a privileged/deferred alias was passed

# --- trust sequence ---------------------------------------------------------------------------
REFUSED_NOT_DEFAULT_BASE = "REFUSED_NOT_DEFAULT_BASE"    # base != externally resolved default
REFUSED_POLICY_INVALID = "REFUSED_POLICY_INVALID"        # pinned policy failed its own schema
REFUSED_POLICY_DRIFT = "REFUSED_POLICY_DRIFT"            # policy vs live protection disagree

# --- readiness --------------------------------------------------------------------------------
REFUSED_STATE = "REFUSED_STATE"                          # PR is not OPEN
REFUSED_DRAFT = "REFUSED_DRAFT"
REFUSED_NOT_MERGEABLE = "REFUSED_NOT_MERGEABLE"          # MergeableState is not MERGEABLE
REFUSED_MERGE_STATE = "REFUSED_MERGE_STATE"              # mergeStateStatus outside the ready set
REFUSED_REVIEW = "REFUSED_REVIEW"
REFUSED_THREADS = "REFUSED_THREADS"
REFUSED_REQUIRED_CHECK_PENDING = "REFUSED_REQUIRED_CHECK_PENDING"
REFUSED_REQUIRED_CHECK_FAILED = "REFUSED_REQUIRED_CHECK_FAILED"
REFUSED_REQUIRED_CHECK_MISSING = "REFUSED_REQUIRED_CHECK_MISSING"
REFUSED_MERGE_QUEUE = "REFUSED_MERGE_QUEUE"              # a queue on the base would enqueue us
REFUSED_ALREADY_QUEUED = "REFUSED_ALREADY_QUEUED"        # this PR is already in a queue
REFUSED_ALREADY_AUTO_ARMED = "REFUSED_ALREADY_AUTO_ARMED"  # another actor armed auto-merge

# --- strategy ---------------------------------------------------------------------------------
REFUSED_STRATEGY_INVALID = "REFUSED_STRATEGY_INVALID"        # outside the closed enum owned by code
REFUSED_STRATEGY_UNAVAILABLE = "REFUSED_STRATEGY_UNAVAILABLE"  # repository does not allow it

# --- races ------------------------------------------------------------------------------------
REFUSED_HEAD_CHANGED = "REFUSED_HEAD_CHANGED"
REFUSED_BASE_CHANGED = "REFUSED_BASE_CHANGED"
REFUSED_DEFAULT_BRANCH_CHANGED = "REFUSED_DEFAULT_BRANCH_CHANGED"

# --- environment ------------------------------------------------------------------------------
REFUSED_GITHUB = "REFUSED_GITHUB"                        # GitHub refused the merge. Final.
REFUSED_UNAVAILABLE_EVIDENCE = "REFUSED_UNAVAILABLE_EVIDENCE"  # evidence missing/unreadable
REFUSED_NOT_INSTALLED = "REFUSED_NOT_INSTALLED"          # provenance record missing/unreadable
REFUSED_UNTRUSTED_TARGET = "REFUSED_UNTRUSTED_TARGET"    # target is not the installed repository
REFUSED_EVALUATOR_STALE = "REFUSED_EVALUATOR_STALE"      # this program changed on the default branch

ALL_CODES = frozenset(
    v for k, v in list(globals().items())
    if k.isupper() and not k.startswith("_") and isinstance(v, str) and k != "ALL_CODES"
)

# Anything that is not READY or MERGED is a refusal or an unconfirmed mutation; both exit non-zero.
EXIT_OK = 0
EXIT_REFUSED = 1
EXIT_USAGE = 2


def exit_code(outcome: str) -> int:
    if outcome in (READY, MERGED):
        return EXIT_OK
    if outcome in (REFUSED_USAGE, REFUSED_FORBIDDEN_OPTION):
        return EXIT_USAGE
    return EXIT_REFUSED
