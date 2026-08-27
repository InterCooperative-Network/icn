"""The single mutation. One head-pinned ordinary merge, then proof.

WHAT MAKES IT SAFE
- The expected head SHA is sent with the request, so GitHub itself refuses if the branch moved
  after the refreshed snapshot was taken.
- Exactly one request is issued. A refusal is FINAL: there is no retry, no weaker flag, no
  privileged second attempt. That path does not exist in this file to be reached.
- Success is not what the merge call returned. It is what a fresh read of the PR says afterwards.
"""

from __future__ import annotations

from dataclasses import dataclass

from . import codes
from .snapshot import Snapshot
from .strategy import api_merge_method


@dataclass(frozen=True)
class MergeOutcome:
    outcome: str
    detail: str
    merge_commit_sha: str | None
    attempted: bool


def perform_merge(client, snap: Snapshot, strategy: str) -> MergeOutcome:
    """Merge `snap` at exactly the head it recorded, then re-read GitHub to prove what happened."""
    method = api_merge_method(strategy)          # closed-set lookup; never a constructed flag
    response = client.merge_pull_request(snap.owner, snap.name, snap.number,
                                         sha=snap.head_oid, merge_method=method)

    # POST-READ. The merge response is a claim; this is the evidence.
    after = client.pull_request_merge_state(snap.owner, snap.name, snap.number)
    if after.get("merged") is not True:
        return MergeOutcome(
            codes.MERGE_UNCONFIRMED,
            f"the merge request was accepted but a fresh read reports merged="
            f"{after.get('merged')!r} (state={after.get('state')!r}). This is NOT success: a "
            f"human must establish what happened to PR #{snap.number} before anything else acts "
            f"on it.",
            after.get("merge_commit_sha") or response.get("sha"),
            attempted=True)

    sha = after.get("merge_commit_sha") or response.get("sha")
    return MergeOutcome(
        codes.MERGED,
        f"PR #{snap.number} merged into {snap.default_branch} with the {strategy} strategy at "
        f"head {snap.head_oid[:12]}; merge commit {sha}",
        sha,
        attempted=True)
