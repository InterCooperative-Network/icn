"""ONE typed evidence boundary. Everything the decision rests on is gathered here, or not at all.

TRUST SEQUENCE (icn#2658 `consumer_contract_note`), in this exact order:

  1. resolve the repository's default branch from EXTERNAL GitHub metadata;
  2. read the target PR's `baseRefName`;
  3. if they differ, REFUSE before loading any policy — a non-default base must never supply the
     document that defines readiness;
  4. only then pin the trusted revision, load policy from it, and validate.

No field inside policy.json establishes policy authority. `branch.primary` in particular is
repository configuration for other consumers and is NOT read here.

WHICH REVISION IS PINNED
`repository.defaultBranchRef.target.oid`, the externally resolved tip of the trusted default
branch. `pullRequest.baseRefOid` is deliberately NOT the pin: GitHub reports it as the base commit
associated with the PR, which lags the branch tip (observed on this repository: a PR whose
`baseRefOid` was the previous default-branch commit while the branch had already moved on).
Pinning the lagging field would either refuse every PR that is not freshly rebased, or — worse —
pin policy to an older revision than the one being merged into. `baseRefOid` is still captured,
because a change in it between evaluation and mutation is a race.
"""

from __future__ import annotations

from dataclasses import dataclass

from .errors import EvidenceUnavailable, NotDefaultBase
from .policy import MergePolicy, load_policy

# Pinned vocabularies, owned by this code. A value GitHub reports that is not in one of these is
# a state this program does not understand, and an unknown state is never a ready state.
PR_STATES = frozenset({"OPEN", "CLOSED", "MERGED"})
MERGEABLE_STATE = frozenset({"MERGEABLE", "CONFLICTING", "UNKNOWN"})
MERGE_STATE_STATUS = frozenset({"DIRTY", "UNKNOWN", "BLOCKED", "BEHIND", "UNSTABLE",
                                "HAS_HOOKS", "CLEAN"})
REVIEW_DECISIONS = frozenset({"CHANGES_REQUESTED", "APPROVED", "REVIEW_REQUIRED"})
# PullRequestReviewState. `latestOpinionatedReviews` should only ever yield the first two,
# but the gate blocks one literal value, so a state outside this set is a review whose
# meaning this program does not know — and it may be an objection.
REVIEW_STATES = frozenset({"APPROVED", "CHANGES_REQUESTED", "COMMENTED", "DISMISSED",
                           "PENDING"})

# Non-terminal check states. Anything here, and anything unrecognised, normalises to PENDING —
# never to a conclusion, because "I do not know what this is" may not read as green.
_RUNNING = frozenset({"QUEUED", "IN_PROGRESS", "WAITING", "PENDING", "REQUESTED"})
_STATUS_CONTEXT = {"SUCCESS": "SUCCESS", "FAILURE": "FAILURE", "ERROR": "FAILURE",
                   "PENDING": "PENDING", "EXPECTED": "PENDING"}
PENDING = "PENDING"

_MAX_PAGES = 100          # 100 pages x 100 nodes. A cursor that never advances is a defect.


@dataclass(frozen=True)
class CheckOccurrence:
    """One reported run of a check: what it concluded, and which GitHub App produced it."""

    outcome: str
    app_id: int | None


@dataclass(frozen=True)
class Protection:
    """Live branch-protection configuration for the branch actually being merged into."""

    required_contexts: frozenset[str]
    required_bindings: dict[str, int | None]
    required_approving_review_count: int
    strict: bool


@dataclass(frozen=True)
class Snapshot:
    """Every piece of evidence the decision uses, read at one moment."""

    owner: str
    name: str
    number: int
    default_branch: str
    default_branch_oid: str
    state: str
    is_draft: bool
    head_oid: str
    base_ref_name: str
    base_ref_oid: str
    mergeable: str
    merge_state_status: str
    review_decision: str | None
    opinionated_review_states: tuple[str, ...]
    review_threads_total: int
    unresolved_threads: int
    merge_queue_present: bool
    is_in_merge_queue: bool
    auto_merge_armed: bool
    checks: dict[str, tuple[CheckOccurrence, ...]]
    protection: Protection
    allowed_merge_methods: frozenset[str]
    policy: MergePolicy

    @property
    def policy_oid(self) -> str:
        return self.policy.oid

    @property
    def policy_sha256(self) -> str:
        return self.policy.text_sha256


def _enum(value, allowed, label):
    if value not in allowed:
        raise EvidenceUnavailable(f"{label} is {value!r}, which is not a value this program knows")
    return value


def _app_id(node) -> int | None:
    app = ((node.get("checkSuite") or {}).get("app") or {}).get("databaseId")
    return app if type(app) is int else None


def _normalise_check(node) -> tuple[str, CheckOccurrence] | None:
    """(name, occurrence) for one rollup context, or None when the node is not one we can read."""
    if not isinstance(node, dict):
        return None
    kind = node.get("__typename")
    if kind == "CheckRun":
        name = node.get("name")
        if not isinstance(name, str):
            return None
        status = node.get("status")
        if status in _RUNNING or status != "COMPLETED":
            return (name, CheckOccurrence(PENDING, _app_id(node)))
        conclusion = node.get("conclusion")
        return (name, CheckOccurrence(conclusion if isinstance(conclusion, str) else PENDING,
                                      _app_id(node)))
    if kind == "StatusContext":
        name = node.get("context")
        if not isinstance(name, str):
            return None
        # A commit status has no App behind it, so it can never satisfy a producer-bound check.
        return (name, CheckOccurrence(_STATUS_CONTEXT.get(node.get("state"), PENDING), None))
    return None


def _collect_threads(client, owner, name, number) -> tuple[int, int]:
    """(total, unresolved) across EVERY page.

    Pagination is the whole point. A single `first: 100` page cannot prove a clean thread state
    on a PR with more threads than that, and "the first page looked clean" is exactly the shape
    of a fail-open that nobody notices until it merges over an unresolved objection.

    The count is RECONCILED against the nodes actually read. Paging to the last page is not the
    same as having seen every thread: a response can carry `totalCount: 12` and hand back fewer
    nodes — a partial GraphQL error, or filtering — and then "no unresolved thread was found"
    would be a statement about threads nobody looked at. Seeing fewer than the count is missing
    evidence, and missing evidence is not ready.
    """
    cursor, total, unresolved, seen, pages = None, None, 0, 0, 0
    while True:
        page = client.review_threads_page(owner, name, number, cursor)
        if total is None:
            total = page.get("totalCount")
            if type(total) is not int:
                raise EvidenceUnavailable("review thread totalCount is unreadable")
        nodes = page.get("nodes")
        if not isinstance(nodes, list):
            raise EvidenceUnavailable("review thread page had no nodes")
        for node in nodes:
            if not isinstance(node, dict) or not isinstance(node.get("isResolved"), bool):
                raise EvidenceUnavailable("a review thread did not report a resolution state")
            seen += 1
            if not node["isResolved"]:
                unresolved += 1
        info = page.get("pageInfo") or {}
        pages += 1
        if not info.get("hasNextPage"):
            if seen != total:
                raise EvidenceUnavailable(
                    f"GitHub reports {total} review thread(s) but only {seen} could be read; the "
                    "unread ones cannot be shown to be resolved")
            return total, unresolved
        cursor = info.get("endCursor")
        if not cursor or pages >= _MAX_PAGES:
            raise EvidenceUnavailable("review thread pagination did not terminate")


def _collect_reviews(client, owner, name, number) -> tuple[str, ...]:
    """Every reviewer's latest opinionated review, across EVERY page.

    Same shape of fail-open as the review threads, and the same answer. `reviewDecision` alone is
    not enough here — policy explicitly admits a null decision, which is what this repository
    reports with no required approvals — so an unread CHANGES_REQUESTED sitting on page two would
    be an objection the gate never saw.
    """
    cursor, total, pages = None, None, 0
    states: list[str] = []
    while True:
        page = client.opinionated_reviews_page(owner, name, number, cursor)
        if total is None:
            total = page.get("totalCount")
            if type(total) is not int:
                raise EvidenceUnavailable("opinionated review totalCount is unreadable")
        nodes = page.get("nodes")
        if not isinstance(nodes, list):
            raise EvidenceUnavailable("opinionated review page had no nodes")
        for node in nodes:
            if not isinstance(node, dict) or not isinstance(node.get("state"), str):
                raise EvidenceUnavailable("a review did not report a state")
            states.append(_enum(node["state"], REVIEW_STATES, "an opinionated review state"))
        info = page.get("pageInfo") or {}
        pages += 1
        if not info.get("hasNextPage"):
            if len(states) != total:
                raise EvidenceUnavailable(
                    f"GitHub reports {total} opinionated review(s) but only {len(states)} could "
                    "be read; the unread ones cannot be shown not to object")
            return tuple(states)
        cursor = info.get("endCursor")
        if not cursor or pages >= _MAX_PAGES:
            raise EvidenceUnavailable("opinionated review pagination did not terminate")


def _collect_checks(client, owner, name, number,
                    head_oid) -> dict[str, tuple[CheckOccurrence, ...]]:
    """name -> every outcome reported for it, across EVERY page of the rollup.

    A name can appear more than once when a check is re-run; all occurrences are kept so the gate
    can take the worst rather than whichever the API happened to list last.

    That is exactly why the count is RECONCILED here as well, and why "a missing check refuses
    anyway" is not the whole story. If a name has a red occurrence and a green one and the red is
    the node that goes missing, the name is still present and every occurrence the gate can see is
    green — so it would report ready on the strength of evidence it never received.
    """
    cursor, total, seen, pages = None, None, 0, 0
    found: dict[str, list[CheckOccurrence]] = {}
    while True:
        page = client.check_contexts_page(owner, name, number, cursor)
        if page.get("head_oid") not in (None, head_oid):
            # The rollup belongs to a different commit than the PR reported. That is not stale
            # data to work around; it is inconsistent evidence, and the run must not decide on it.
            raise EvidenceUnavailable(
                f"check rollup is for {page.get('head_oid')}, but the PR head is {head_oid}")
        if total is None:
            total = page.get("totalCount")
            if type(total) is not int:
                raise EvidenceUnavailable("status check rollup totalCount is unreadable")
        nodes = page.get("nodes")
        if not isinstance(nodes, list):
            raise EvidenceUnavailable("status check rollup page had no nodes")
        for node in nodes:
            entry = _normalise_check(node)
            if entry is None:
                # A node in the rollup this program cannot read is still a node it must account
                # for; leaving it out silently would be the same hole as dropping it.
                raise EvidenceUnavailable("a status check rollup entry was unreadable")
            found.setdefault(entry[0], []).append(entry[1])
            seen += 1
        info = page.get("pageInfo") or {}
        pages += 1
        if not info.get("hasNextPage"):
            if seen != total:
                raise EvidenceUnavailable(
                    f"GitHub reports {total} status check result(s) but only {seen} could be "
                    "read; an occurrence that went unread cannot be shown to be green")
            return {k: tuple(v) for k, v in found.items()}
        cursor = info.get("endCursor")
        if not cursor or pages >= _MAX_PAGES:
            raise EvidenceUnavailable("status check pagination did not terminate")


def load_snapshot(client, owner: str, name: str, number: int) -> Snapshot:
    """Gather all decision evidence. Raises rather than returning a half-known state."""
    # (1) EXTERNAL trust root. Nothing in the repository's own content participates in this.
    meta = client.repository_metadata(owner, name)
    default_branch = meta["default_branch"]
    default_oid = meta["default_branch_oid"]
    if not isinstance(default_branch, str) or not isinstance(default_oid, str) or not default_oid:
        raise EvidenceUnavailable("GitHub did not report a usable default branch")

    # (2) the target's declared base.
    pr = client.pull_request_core(owner, name, number)
    base_ref_name = pr.get("baseRefName")
    if not isinstance(base_ref_name, str) or not base_ref_name:
        # Not "some other branch" — no branch at all. Reporting this as a stacked base would send
        # the operator to the stack flow for a PR whose base GitHub never told us.
        raise EvidenceUnavailable("PR did not report a base branch")

    # (3) refuse a non-default base BEFORE any policy is read from it.
    if base_ref_name != default_branch:
        raise NotDefaultBase(
            f"PR #{number} targets {base_ref_name!r}, but the repository's default branch is "
            f"{default_branch!r}. A stacked or non-default base belongs to the separate stack "
            f"flow; this primitive merges into the default branch only.")

    head_oid = pr.get("headRefOid")
    if not isinstance(head_oid, str) or not head_oid:
        raise EvidenceUnavailable("PR did not report a head OID")
    base_ref_oid = pr.get("baseRefOid")
    if not isinstance(base_ref_oid, str) or not base_ref_oid:
        raise EvidenceUnavailable("PR did not report a base OID")
    is_draft = pr.get("isDraft")
    if not isinstance(is_draft, bool):
        # `bool(None)` is False, and False is the value the draft gate accepts. A cast here would
        # turn evidence GitHub did not send into evidence that the PR is ready.
        raise EvidenceUnavailable("PR did not report a draft state")

    # (4) pin the trusted revision and load policy FROM IT.
    policy = load_policy(client, owner, name, default_oid)

    protection_raw = client.branch_protection(owner, name, default_branch)
    # Type-checked HERE as well as in the transport. The transport is where a malformed response
    # is detected; this is where the value is used, and a gate that compares `None > 0` does not
    # refuse — it raises, mid-decision, which reports nothing anyone can act on.
    contexts = protection_raw.get("required_contexts")
    approvals = protection_raw.get("required_approving_review_count")
    if not isinstance(contexts, list) or not all(isinstance(c, str) for c in contexts):
        raise EvidenceUnavailable("branch protection did not report a readable required-check set")
    strict = protection_raw.get("strict")
    if type(strict) is not bool:
        raise EvidenceUnavailable(
            f"branch protection did not report a readable strict setting ({strict!r}); an "
            "unreadable up-to-date requirement is not a satisfied one")
    if type(approvals) is not int:
        raise EvidenceUnavailable(
            f"branch protection did not report a readable approving-review count ({approvals!r}); "
            "an unreadable requirement is not no requirement")
    bindings = protection_raw.get("required_bindings") or {}
    if not isinstance(bindings, dict) or not all(
            value is None or type(value) is int for value in bindings.values()):
        raise EvidenceUnavailable(
            "branch protection did not report readable required-check producers")
    protection = Protection(
        required_contexts=frozenset(contexts),
        required_bindings=dict(bindings),
        required_approving_review_count=approvals,
        strict=strict,
    )

    review_decision = pr.get("reviewDecision")
    if review_decision is not None:
        _enum(review_decision, REVIEW_DECISIONS, "reviewDecision")
    review_states = _collect_reviews(client, owner, name, number)

    total_threads, unresolved = _collect_threads(client, owner, name, number)
    checks = _collect_checks(client, owner, name, number, head_oid)

    auto_merge = pr.get("autoMergeRequest")
    in_queue = pr.get("isInMergeQueue")
    if not isinstance(in_queue, bool):
        raise EvidenceUnavailable("PR did not report merge-queue membership")

    allowed = {method for method, flag in (("merge", meta["merge_allowed"]),
                                           ("squash", meta["squash_allowed"]),
                                           ("rebase", meta["rebase_allowed"])) if flag}

    return Snapshot(
        owner=owner,
        name=name,
        number=number,
        default_branch=default_branch,
        default_branch_oid=default_oid,
        state=_enum(pr.get("state"), PR_STATES, "PR state"),
        is_draft=is_draft,
        head_oid=head_oid,
        base_ref_name=base_ref_name,
        base_ref_oid=base_ref_oid,
        mergeable=_enum(pr.get("mergeable"), MERGEABLE_STATE, "mergeable"),
        merge_state_status=_enum(pr.get("mergeStateStatus"), MERGE_STATE_STATUS,
                                 "mergeStateStatus"),
        review_decision=review_decision,
        opinionated_review_states=review_states,
        review_threads_total=total_threads,
        unresolved_threads=unresolved,
        merge_queue_present=client.merge_queue_present(owner, name, default_branch),
        is_in_merge_queue=in_queue or (pr.get("mergeQueueEntry") is not None),
        auto_merge_armed=auto_merge is not None,
        checks=checks,
        protection=protection,
        allowed_merge_methods=frozenset(allowed),
        policy=policy,
    )
