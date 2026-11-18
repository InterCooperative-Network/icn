"""GitHub observation via the authenticated `gh` CLI. Read-only.

Failure is explicit: if `gh` is missing, unauthenticated or offline, the result is an
error observation, never "zero pull requests".
"""

from __future__ import annotations

import json
import re
import time
from typing import Optional

from .util import parse_iso, run

OPEN_PRS_QUERY = """
query($owner:String!,$name:String!,$after:String){repository(owner:$owner,name:$name){
 pullRequests(states:OPEN,first:20,after:$after,orderBy:{field:UPDATED_AT,direction:DESC}){
  pageInfo{hasNextPage endCursor}
  nodes{number title url isDraft headRefName headRefOid baseRefName updatedAt createdAt
   mergeable mergeStateStatus reviewDecision authorAssociation author{login __typename} body
   reviews(last:10){nodes{body state authorAssociation author{login __typename} submittedAt commit{oid}}}
   reviewThreads(first:100){nodes{isResolved}}
   commits(last:1){nodes{commit{statusCheckRollup{state contexts(first:100){nodes{__typename
     ... on CheckRun{name conclusion status} ... on StatusContext{context state}}}}}}}}}}}
"""

MERGED_PRS_QUERY = """
query($q:String!){search(query:$q,type:ISSUE,first:30){nodes{... on PullRequest{
 number title url headRefName mergedAt author{login} repository{name}}}}}
"""

ISSUE_QUERY = """
query($owner:String!,$name:String!,$n:Int!){repository(owner:$owner,name:$name){
 issueOrPullRequest(number:$n){... on Issue{number title state url updatedAt}
  ... on PullRequest{number title state url updatedAt}}}}
"""

# Only these GitHub author associations may assert delivery state or approval. The icn repository is
# public: anyone can open a PR whose body carries a lifecycle block, or post a review with a findings line.
TRUSTED_ASSOCIATIONS = {"OWNER", "MEMBER", "COLLABORATOR"}

BOT_LOGINS = {"copilot-pull-request-reviewer", "chatgpt-codex-connector", "github-actions",
              "dependabot", "coderabbitai", "claude", "copilot"}


class GhUnavailable(Exception):
    pass


def _gh_graphql(query: str, variables: dict, timeout: float = 45.0) -> dict:
    argv = ["gh", "api", "graphql", "-f", f"query={query}"]
    for k, v in variables.items():
        if v is None:
            continue
        argv += ["-F" if isinstance(v, int) else "-f", f"{k}={v}"]
    r = run(argv, timeout=timeout)
    if r.rc == 127:
        raise GhUnavailable("gh CLI not installed")
    if r.rc != 0:
        msg = (r.err or r.out).strip().splitlines()
        raise GhUnavailable("gh api failed: " + (msg[-1][:200] if msg else f"rc={r.rc}"))
    d = json.loads(r.out)
    if d.get("errors"):
        raise GhUnavailable("GraphQL error: " + str(d["errors"][0].get("message"))[:200])
    return d["data"]


def fetch_open_prs(owner: str, name: str, max_pages: int = 8, retries: int = 1) -> list[dict]:
    out, after = [], None
    for _ in range(max_pages):
        for attempt in range(retries + 1):
            try:
                data = _gh_graphql(OPEN_PRS_QUERY, {"owner": owner, "name": name, "after": after})
                break
            except GhUnavailable:
                if attempt >= retries:
                    raise
                time.sleep(2)
        repo = data.get("repository")
        if repo is None:
            raise GhUnavailable(f"repository {owner}/{name} not visible to this gh login")
        prs = repo["pullRequests"]
        out.extend(prs["nodes"])
        if not prs["pageInfo"]["hasNextPage"]:
            break
        after = prs["pageInfo"]["endCursor"]
    return [normalize_pr(p, name) for p in out]


def fetch_recent_merged(owner: str, repos: list[str], days: int = 7) -> list[dict]:
    since = time.strftime("%Y-%m-%d", time.gmtime(time.time() - days * 86400))
    q = " ".join(f"repo:{owner}/{r}" for r in repos) + f" is:pr is:merged merged:>={since} sort:updated-desc"
    data = _gh_graphql(MERGED_PRS_QUERY, {"q": q})
    out = []
    for n in data["search"]["nodes"]:
        if not n:
            continue
        out.append({"repo": n["repository"]["name"], "number": n["number"], "title": n["title"],
                    "url": n["url"], "branch": n["headRefName"], "merged_at": n["mergedAt"],
                    "merged_ts": parse_iso(n["mergedAt"]), "author": (n.get("author") or {}).get("login")})
    out.sort(key=lambda x: -(x["merged_ts"] or 0))
    return out


def fetch_issue(owner: str, name: str, number: int) -> dict:
    data = _gh_graphql(ISSUE_QUERY, {"owner": owner, "name": name, "n": int(number)})
    return (data.get("repository") or {}).get("issueOrPullRequest") or {}


def branch_required_checks(owner: str, name: str, branch: str = "main") -> Optional[list[str]]:
    """Live required status checks from branch protection; None if not readable."""
    r = run(["gh", "api", f"repos/{owner}/{name}/branches/{branch}/protection/required_status_checks"], timeout=20)
    if r.rc != 0:
        return None
    try:
        d = json.loads(r.out)
    except ValueError:
        return None
    names = [c.get("context") for c in d.get("checks") or [] if c.get("context")]
    return sorted(set(names or d.get("contexts") or []))


# --------------------------------------------------------------------------- normalization
LIFECYCLE_BLOCK = re.compile(r"<!--\s*ICN-DELIVERY-LIFECYCLE:BEGIN\s*-->(.*?)<!--\s*ICN-DELIVERY-LIFECYCLE:END\s*-->", re.S)
ISSUE_REF = re.compile(r"#(\d{2,6})")


def parse_lifecycle(body: Optional[str]) -> Optional[dict]:
    """Parse the canonical ICN DELIVERY LIFECYCLE block from a PR body (owner: delivery.json)."""
    if not body:
        return None
    m = LIFECYCLE_BLOCK.search(body)
    if not m:
        return None
    fields = {}
    for line in m.group(1).splitlines():
        mm = re.match(r"^\s*([A-Za-z][A-Za-z \-]+?):\s+(.*)$", line)
        if mm:
            fields[mm.group(1).strip().lower()] = mm.group(2).strip()
    state = (fields.get("state") or "").split()[0].upper() if fields.get("state") else None
    freeze = fields.get("freeze head") or ""
    fm = re.match(r"([0-9a-f]{7,40})", freeze)
    ledger = fields.get("follow-up ledger") or ""
    lm = ISSUE_REF.search(ledger)
    return {"state": state, "lane": fields.get("lane"), "freeze_head": fm.group(1) if fm else None,
            "review_generation": fields.get("review generation"), "known_blockers": fields.get("known blockers"),
            "ledger": int(lm.group(1)) if lm else None, "raw": fields}


FINDINGS_LINE = re.compile(r"(\d+)\s*BLOCKER\D+(\d+)\s*FOLLOW_UP\D+(\d+)\s*QUESTION", re.I)


def classify_review(rv: dict, pr_author: Optional[str]) -> dict:
    """Classify one review. Automated review is evidence, never human approval."""
    a = rv.get("author") or {}
    login = a.get("login") or "?"
    body = rv.get("body") or ""
    is_bot = a.get("__typename") == "Bot" or login.endswith("[bot]") or login.lower() in BOT_LOGINS
    ai_marked = bool(re.search(r"(?i)AI-generated|generated (?:by|with) (?:claude|codex|copilot)", body[:600]))
    trusted = rv.get("authorAssociation") in TRUSTED_ASSOCIATIONS
    if is_bot:
        kind = "automated (bot)"
    elif not trusted:
        kind = f"account without write association ({rv.get('authorAssociation') or 'unknown'})"
    elif ai_marked:
        kind = "AI-generated review posted from a human account"
    elif pr_author and login == pr_author:
        kind = "PR author's own account"
    else:
        kind = "human"
    # Findings and generation are only read from reviews by trusted, non-bot accounts.
    fm = FINDINGS_LINE.search(body) if (trusted and not is_bot) else None
    gen = re.search(r"\b(FULL|DELTA)\b", body[:2000]) if (trusted and not is_bot) else None
    return {"author": login, "state": rv.get("state"), "submitted_at": rv.get("submittedAt"),
            "commit": ((rv.get("commit") or {}).get("oid")), "kind": kind,
            "counts_as_human_approval": rv.get("state") == "APPROVED" and kind == "human",
            "findings": ({"blocker": int(fm.group(1)), "follow_up": int(fm.group(2)), "question": int(fm.group(3))}
                         if fm else None),
            "generation": gen.group(1) if gen else None}


def normalize_pr(p: dict, repo: str) -> dict:
    author = (p.get("author") or {}).get("login")
    checks = []
    rollup_state = None
    nodes = (p.get("commits") or {}).get("nodes") or []
    if nodes:
        roll = (nodes[0].get("commit") or {}).get("statusCheckRollup") or {}
        rollup_state = roll.get("state")
        for c in ((roll.get("contexts") or {}).get("nodes") or []):
            if c.get("__typename") == "CheckRun":
                checks.append({"name": c.get("name"), "status": c.get("status"), "conclusion": c.get("conclusion")})
            else:
                checks.append({"name": c.get("context"), "status": "COMPLETED", "conclusion": c.get("state")})
    threads = (p.get("reviewThreads") or {}).get("nodes") or []
    reviews = [classify_review(r, author) for r in ((p.get("reviews") or {}).get("nodes") or [])]
    assoc = p.get("authorAssociation")
    lifecycle = parse_lifecycle(p.get("body"))
    untrusted_lifecycle = lifecycle is not None and assoc not in TRUSTED_ASSOCIATIONS
    if untrusted_lifecycle:
        lifecycle = None
    return {
        "repo": repo, "number": p["number"], "title": p.get("title") or "", "url": p.get("url"),
        "draft": bool(p.get("isDraft")), "branch": p.get("headRefName"), "head": p.get("headRefOid"),
        "base": p.get("baseRefName"), "updated_at": p.get("updatedAt"), "updated_ts": parse_iso(p.get("updatedAt")),
        "author": author, "mergeable": p.get("mergeable"), "merge_state": p.get("mergeStateStatus"),
        "review_decision": p.get("reviewDecision"), "checks": checks, "rollup": rollup_state,
        "unresolved_threads": sum(1 for t in threads if not t.get("isResolved")),
        "reviews": reviews, "lifecycle": lifecycle, "author_association": assoc,
        "lifecycle_untrusted": untrusted_lifecycle,
    }


def required_check_status(pr: dict, required: Optional[list[str]]) -> dict:
    """Required-check verdict on the PR's current head. Unknown when the set is unknown."""
    if required is None:
        return {"verdict": "UNKNOWN", "detail": "required-check set unknown", "missing": [], "failing": [], "pending": []}
    if pr.get("base") not in (None, "main"):
        return {"verdict": "NOT RUN", "detail": f"stacked on {pr.get('base')} (required CI runs only on PRs into main)",
                "missing": list(required), "failing": [], "pending": []}
    latest: dict = {}
    for c in pr.get("checks") or []:
        latest.setdefault(c["name"], []).append(c)
    missing, failing, pending, ok = [], [], [], []
    for name in required:
        runs = latest.get(name)
        if not runs:
            missing.append(name)
            continue
        concl = [(r.get("conclusion") or "").upper() for r in runs]
        status = [(r.get("status") or "").upper() for r in runs]
        if any(s and s != "COMPLETED" for s in status) or any(c in ("", "PENDING", "EXPECTED") for c in concl):
            pending.append(name)
        elif all(c in ("SUCCESS", "SKIPPED", "NEUTRAL") for c in concl):
            ok.append(name)
        else:
            failing.append(name)
    if failing:
        verdict = "FAILING"
    elif pending:
        verdict = "PENDING"
    elif missing:
        verdict = "INCOMPLETE"
    else:
        verdict = "GREEN"
    return {"verdict": verdict, "detail": f"{len(ok)}/{len(required)} required green",
            "missing": missing, "failing": failing, "pending": pending}


def freeze_matches_head(pr: dict) -> Optional[bool]:
    """True/False when both the freeze head and the live head are known; None otherwise."""
    lc = pr.get("lifecycle") or {}
    fh, head = lc.get("freeze_head"), pr.get("head")
    if not fh or not head:
        return None
    return head.lower().startswith(fh.lower())


def merge_readiness(pr: dict, req: dict) -> dict:
    """A conservative, explained readiness verdict. Never says READY when anything is unknown.

    This is a display aid. The merge primitive and live branch protection remain the authority.
    """
    reasons = []
    lc = pr.get("lifecycle")
    if pr.get("draft"):
        reasons.append("draft")
    if req["verdict"] != "GREEN":
        reasons.append(f"required CI {req['verdict'].lower()} ({req['detail']})")
    if pr.get("unresolved_threads"):
        reasons.append(f"{pr['unresolved_threads']} unresolved review thread(s)")
    if pr.get("mergeable") not in ("MERGEABLE",):
        reasons.append(f"mergeable={pr.get('mergeable') or 'unknown'}")
    if not lc:
        reasons.append("no delivery lifecycle block")
    elif lc.get("state") != "FROZEN":
        reasons.append(f"lifecycle {lc.get('state') or 'unknown'} (not FROZEN)")
    elif freeze_matches_head(pr) is False:
        reasons.append("head moved after freeze (lifecycle block is stale)")
    elif freeze_matches_head(pr) is None:
        reasons.append("freeze head not recorded")
    if not reasons:
        return {"verdict": "MERGE-ELIGIBLE (needs explicit maintainer authorization)", "reasons": []}
    return {"verdict": "NOT READY", "reasons": reasons}
