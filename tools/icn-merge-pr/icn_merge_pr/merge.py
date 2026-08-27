"""The single mutation. One head-pinned ordinary merge, then proof.

WHAT MAKES IT SAFE
- The expected head SHA is sent with the request, so GitHub itself refuses if the branch moved
  after the refreshed snapshot was taken.
- Exactly one merge request is issued. A refusal is FINAL: there is no retry, no weaker flag, no
  privileged second attempt. That path does not exist in this file to be reached.
- Success is not what the merge call returned. It is what a fresh read of the PR says afterwards.

WHAT MAKES IT HONEST
Once a request has been dispatched, "it failed" is no longer a free thing to say. A transport
that dies after the PUT leaves the world in a state this process cannot see, and reporting
REFUSED there would state a complete outcome — nothing happened — on evidence nobody has. So
every post-dispatch failure resolves through a fresh read, and when that read cannot settle the
question the outcome is MERGE_UNCONFIRMED: not success, and not a claim that nothing happened.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from . import codes
from .errors import GitHubRefused, MergeToolError, TransportIndeterminate
from .snapshot import Snapshot
from .strategy import api_merge_method

# The merge commit's identity, as GitHub reports it. Same expectation the provenance record is
# held to: a full Git object id, not merely something truthy.
_MERGE_OID = re.compile(r"\A[0-9a-fA-F]{40}\Z")


def _usable_sha(*candidates) -> str | None:
    """The first candidate that is actually a commit id, or None. Never a truthy stand-in."""
    for candidate in candidates:
        if isinstance(candidate, str) and _MERGE_OID.match(candidate):
            return candidate
    return None


@dataclass(frozen=True)
class MergeOutcome:
    outcome: str
    detail: str
    merge_commit_sha: str | None
    attempted: bool


def _read_back(client, snap: Snapshot) -> dict | None:
    """A fresh read of the PR, or None when even that could not be obtained."""
    try:
        return client.pull_request_merge_state(snap.owner, snap.name, snap.number)
    except MergeToolError:
        return None


def perform_merge(client, snap: Snapshot, strategy: str) -> MergeOutcome:
    """Merge `snap` at exactly the head it recorded, then re-read GitHub to prove what happened."""
    method = api_merge_method(strategy)          # closed-set lookup; never a constructed flag
    # Anything raised from here on is resolved against a fresh read, never reported as a refusal
    # on the strength of the failure alone.
    try:
        response = client.merge_pull_request(snap.owner, snap.name, snap.number,
                                             sha=snap.head_oid, merge_method=method)
    except TransportIndeterminate as exc:
        # GitHub never answered. A read taken now is a point-in-time observation, not proof the
        # dispatched request is finished — the server may still be processing it — so a negative
        # read here may NOT be reported as a refusal. The uncertainty is the finding.
        after = _read_back(client, snap)
        if after is not None and after.get("merged") is True:
            return MergeOutcome(
                codes.MERGE_UNCONFIRMED,
                f"the merge request got no answer ({exc.detail}) and a fresh read reports PR "
                f"#{snap.number} MERGED at {after.get('merge_commit_sha')}. This run may or may "
                f"not have caused that, and it does not claim to have: a human must establish "
                f"which before anything else acts on it.",
                after.get("merge_commit_sha"), attempted=True)
        seen = ("the PR could not be re-read" if after is None
                else f"a read taken immediately afterwards reports merged={after.get('merged')!r}")
        return MergeOutcome(
            codes.MERGE_UNCONFIRMED,
            f"the merge request got no answer from GitHub ({exc.detail}) and {seen}, which cannot "
            f"prove a dispatched request is finished. Whether PR #{snap.number} merges is UNKNOWN: "
            f"a human must establish the state, and nothing may re-issue the merge in the "
            f"meantime.",
            None, attempted=True)
    except GitHubRefused as exc:
        # GitHub ANSWERED, with a status. That is a decision, not a lost message.
        after = _read_back(client, snap)
        if after is None:
            return MergeOutcome(
                codes.MERGE_UNCONFIRMED,
                f"GitHub refused the merge ({exc.detail}) but the PR could not be re-read, so its "
                f"state is UNKNOWN. A human must establish it before anything else acts on "
                f"PR #{snap.number}.",
                None, attempted=True)
        if after.get("merged") is True:
            return MergeOutcome(
                codes.MERGE_UNCONFIRMED,
                f"GitHub refused the merge ({exc.detail}) but a fresh read reports PR "
                f"#{snap.number} MERGED at {after.get('merge_commit_sha')}. This run does not "
                f"claim to have caused that: a human must establish what did.",
                after.get("merge_commit_sha"), attempted=True)
        # Refusal CONFIRMED by evidence, not merely by the call having failed.
        return MergeOutcome(
            codes.REFUSED_GITHUB,
            f"GitHub refused the merge and a fresh read confirms PR #{snap.number} is not merged "
            f"(state={after.get('state')!r}): {exc.detail}. This is final — the program does not "
            f"know a weaker way to ask.",
            None, attempted=True)

    # The transport normalises this too, but the guarantee that matters — nothing raises after a
    # mutation has been dispatched — belongs where the value is USED. A crash here would lose the
    # one fact worth reporting: that a merge request went out and its result is unknown.
    if not isinstance(response, dict):
        response = {}

    # POST-READ. The merge response is a claim; this is the evidence.
    after = _read_back(client, snap)
    if after is None:
        return MergeOutcome(
            codes.MERGE_UNCONFIRMED,
            f"the merge request was accepted but the PR could not be re-read, so whether PR "
            f"#{snap.number} merged is UNKNOWN. A human must establish the state before anything "
            f"else acts on it.",
            response.get("sha"), attempted=True)
    if after.get("merged") is not True:
        return MergeOutcome(
            codes.MERGE_UNCONFIRMED,
            f"the merge request was accepted but a fresh read reports merged="
            f"{after.get('merged')!r} (state={after.get('state')!r}). This is NOT success: a "
            f"human must establish what happened to PR #{snap.number} before anything else acts "
            f"on it.",
            after.get("merge_commit_sha") or response.get("sha"), attempted=True)

    # MERGED means BOTH things: a fresh read proves merged, AND the merge commit can be named.
    # Source order is unchanged — the post-read first, then the merge response, which the merge
    # API documents as the resulting merge commit — but whichever supplies it must be a real
    # commit id. Returning success with `merge_commit_sha: None` broke the result contract the
    # skill states and threw away the audit identity callers rely on.
    sha = _usable_sha(after.get("merge_commit_sha"), response.get("sha"))
    if sha is None:
        return MergeOutcome(
            codes.MERGE_UNCONFIRMED,
            f"GitHub reports PR #{snap.number} merged, but the merge commit identity could not be "
            f"established (post-read {after.get('merge_commit_sha')!r}, merge response "
            f"{response.get('sha')!r}), so this invocation will not claim a confirmed MERGED. The "
            f"merge did happen; what is missing is the commit it produced, and a human should "
            f"record that before anything else acts on it.",
            None, attempted=True)
    return MergeOutcome(
        codes.MERGED,
        f"PR #{snap.number} merged into {snap.default_branch} with the {strategy} strategy at "
        f"head {snap.head_oid[:12]}; merge commit {sha}",
        sha,
        attempted=True)
