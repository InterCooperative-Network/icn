"""Regression coverage for the three release blockers of the PR #2908 FULL review (B1, B2, B3).

B1  merge readiness ignored mergeStateStatus and reviewDecision.
B2  a partial open-PR fetch was treated as a complete observation, so a worktree in the failed
    repository read as "no PR" and could become retirable.
B3  a resumable launcher conversation was not a retirement protection.

Fixtures only; nothing on the host is read or changed except the repository's own merge policy,
which is read (never copied) so that a drift between policy and display fails here.
"""

from __future__ import annotations

import itertools
import os
import shutil
import sys
import tempfile
import unittest.mock

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

from icn_control import activities as A  # noqa: E402
from icn_control import github as gh  # noqa: E402
from icn_control import reconcile, snapshot  # noqa: E402
from icn_control.util import REGISTERED_NOT_RUNNING, STALE, Obs, Paths  # noqa: E402
from test_activities import NOW, wt  # noqa: E402
from test_activities import pr as act_pr  # noqa: E402
from test_control import REQ, obs  # noqa: E402
from test_control import pr as gh_pr  # noqa: E402

CATALOG = reconcile.load_catalog()
READY_WHEN = snapshot.merge_policy()["ready_when"]
ELIGIBLE = "MERGE-ELIGIBLE"


def verdict(p, ready_when=READY_WHEN, required=REQ):
    return gh.merge_readiness(p, gh.required_check_status(p, required), ready_when)


# --------------------------------------------------------------------------- B1
class B1MergeReadiness(unittest.TestCase):
    def test_policy_owner_is_the_source_of_allowed_states(self):
        # Pin what the owner says today, so a policy change is noticed here rather than silently absorbed.
        self.assertEqual(READY_WHEN["merge_state_status_in"], ["CLEAN", "UNSTABLE"])
        self.assertEqual(READY_WHEN["review_decision_allowlist"], ["APPROVED", None])

    def test_merge_states_outside_policy_are_never_eligible(self):
        for ms in ("BEHIND", "BLOCKED", "DIRTY", "UNKNOWN", None, "HAS_HOOKS", "", "MERGEABLE", "clean"):
            r = verdict(gh_pr(merge_state=ms))
            self.assertEqual(r["verdict"], "NOT READY", ms)
            self.assertTrue(any(x.startswith(f"merge state {ms or 'unknown'} ") for x in r["reasons"]), (ms, r))

    def test_policy_admitted_merge_states_are_eligible(self):
        for ms in ("CLEAN", "UNSTABLE"):
            r = verdict(gh_pr(merge_state=ms))
            self.assertTrue(r["verdict"].startswith(ELIGIBLE), (ms, r))
            self.assertEqual(r["reasons"], [])

    def test_review_decision_allowlist(self):
        for rd in ("CHANGES_REQUESTED", "REVIEW_REQUIRED", "approved"):
            r = verdict(gh_pr(review_decision=rd))
            self.assertEqual(r["verdict"], "NOT READY", rd)
            self.assertIn(f"review decision {rd}", r["reasons"])
        for rd in ("APPROVED", None):
            self.assertTrue(verdict(gh_pr(review_decision=rd))["verdict"].startswith(ELIGIBLE), rd)

    def test_missing_or_malformed_policy_fails_closed(self):
        for rw in (None, {}, [], {"merge_state_status_in": "CLEAN", "review_decision_allowlist": [None]},
                   {"merge_state_status_in": ["CLEAN"]}):
            r = verdict(gh_pr(), ready_when=rw)
            self.assertEqual(r["verdict"], "NOT READY", rw)
            self.assertTrue(any("ready_when unavailable" in x for x in r["reasons"]), r)

    def test_eligible_only_when_every_gate_holds(self):
        # Exhaustive over the gates: one failing or unknown gate anywhere means NOT READY.
        frozen = gh_pr()["lifecycle"]
        gates = {
            "draft": [False, True],
            "required": [REQ, None],
            "unresolved_threads": [0, 2],
            "mergeable": ["MERGEABLE", "CONFLICTING", "UNKNOWN", None],
            "merge_state": ["CLEAN", "UNSTABLE", "BEHIND", "BLOCKED", "DIRTY", "UNKNOWN", "HAS_HOOKS", None],
            "review_decision": [None, "APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED"],
            "lifecycle": [frozen, None],
        }
        keys = list(gates)
        n_eligible = 0
        for combo in itertools.product(*(gates[k] for k in keys)):
            c = dict(zip(keys, combo))
            p = gh_pr(draft=c["draft"], unresolved_threads=c["unresolved_threads"], mergeable=c["mergeable"],
                      merge_state=c["merge_state"], review_decision=c["review_decision"], lifecycle=c["lifecycle"])
            good = (not c["draft"] and c["required"] is not None and c["unresolved_threads"] == 0
                    and c["mergeable"] == "MERGEABLE" and c["merge_state"] in ("CLEAN", "UNSTABLE")
                    and c["review_decision"] in (None, "APPROVED") and c["lifecycle"] is not None)
            r = verdict(p, required=c["required"])
            self.assertEqual(r["verdict"].startswith(ELIGIBLE), good, c)
            self.assertEqual(bool(r["reasons"]), not good, c)
            n_eligible += good
        self.assertEqual(n_eligible, 4, "CLEAN/UNSTABLE x APPROVED/None")

    def test_pr_views_applies_the_policy(self):
        # The wiring, not just the function: the PR view a screen renders must carry the merge-state gate.
        paths = Paths(icn_dev=tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, paths.icn_dev, ignore_errors=True)
        snap = {"github_prs": obs({"icn": [gh_pr(number=1, merge_state="BEHIND"), gh_pr(number=2, merge_state="CLEAN")]}),
                "required_checks": obs({"icn": {"checks": REQ, "source": "test"}})}
        v = {p["number"]: p for p in reconcile.pr_views(snap, paths, CATALOG)}
        self.assertEqual(v[1]["readiness"]["verdict"], "NOT READY")
        self.assertTrue(v[2]["readiness"]["verdict"].startswith(ELIGIBLE))
        with unittest.mock.patch.object(reconcile, "merge_policy", return_value=None):
            v = {p["number"]: p for p in reconcile.pr_views(snap, paths, CATALOG)}
        self.assertEqual(v[2]["readiness"]["verdict"], "NOT READY", "an unreadable policy is not a pass")


# --------------------------------------------------------------------------- B2
def _wsnap(gh_obs, wts, pr_views=()):
    return {"worktrees": obs({"worktrees": wts}), "active_worktrees": obs({"rows": {}}),
            "file_locks": obs({"locks": []}), "github_merged": obs([]), "github_prs": gh_obs,
            "_pr_views": list(pr_views)}


CLEAN = {"exists": True, "dirty": False, "local_only_commits": 0, "mtime": NOW - 40 * 86400}


class B2PartialObservation(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-b2-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.paths = Paths(icn_dev=os.path.join(self.tmp, "icn-dev"), control_state=os.path.join(self.tmp, "ctl"),
                           claude_home=os.path.join(self.tmp, "claude"), proc=os.path.join(self.tmp, "proc"))

    def test_coverage_distinguishes_empty_failed_cached_and_absent(self):
        self.assertEqual(reconcile.open_pr_observed({"github_prs": obs({"icn": [], "sdk": [gh_pr()]})}), {"icn", "sdk"})
        self.assertEqual(reconcile.open_pr_observed({"github_prs": obs({"sdk": []}, error="partial: icn: timeout")}), {"sdk"})
        self.assertEqual(reconcile.open_pr_observed({"github_prs": obs(None, ok=False, error="offline")}), frozenset())
        self.assertEqual(reconcile.open_pr_observed({"github_prs": obs({"icn": []}, from_cache=True)}), frozenset(),
                         "cached open-PR lists are not a current observation")
        self.assertEqual(reconcile.open_pr_observed({}), frozenset())

    def test_collector_partial_fetch_records_only_the_observed_repository(self):
        def fetch(owner, name):
            if name == "icn":
                raise gh.GhUnavailable("timeout")
            return []
        with unittest.mock.patch.object(snapshot, "repo_remotes", return_value={"icn": "icn", "sdk": "sdk"}), \
                unittest.mock.patch.object(gh, "fetch_open_prs", side_effect=fetch), \
                unittest.mock.patch.object(gh, "fetch_recent_merged", return_value=[]), \
                unittest.mock.patch.object(gh, "branch_required_checks", return_value=None):
            snap = snapshot.collect_github(self.paths, {"programs": []})
        o = snap["github_prs"]
        self.assertTrue(o.ok)
        self.assertIn("partial", o.error)
        self.assertEqual(reconcile.open_pr_observed(snap), {"sdk"})

    def test_failed_refresh_kept_as_stale_is_not_coverage(self):
        c = snapshot.Collector(self.paths, CATALOG, use_cache=False)
        good = obs({"icn": []}, key="GitHub open PRs")
        bad = Obs("GitHub open PRs", NOW + 5, False, None, error="offline")
        with unittest.mock.patch.object(snapshot, "collect_github", side_effect=[{"github_prs": good}, {"github_prs": bad}]), \
                unittest.mock.patch.object(snapshot, "save_cache"):
            c._run_tier("github")
            self.assertEqual(reconcile.open_pr_observed(c.snapshot()), {"icn"})
            c._run_tier("github")
        self.assertEqual(reconcile.open_pr_observed(c.snapshot()), frozenset())

    def test_truncated_open_pr_list_is_a_failure_not_a_short_list(self):
        page = {"repository": {"pullRequests": {"pageInfo": {"hasNextPage": True, "endCursor": "c"}, "nodes": []}}}
        with unittest.mock.patch.object(gh, "_gh_graphql", return_value=page):
            with self.assertRaises(gh.GhUnavailable):
                gh.fetch_open_prs("o", "icn", max_pages=2)
        last = {"repository": {"pullRequests": {"pageInfo": {"hasNextPage": False, "endCursor": None}, "nodes": []}}}
        with unittest.mock.patch.object(gh, "_gh_graphql", side_effect=[page, last]):
            self.assertEqual(gh.fetch_open_prs("o", "icn", max_pages=2), [])

    def _scenario(self, gh_obs, marks=None, pr_views=()):
        """Repository sdk observed; icn worktree `feature` whose open PR (if any) lives in icn."""
        wts = [dict(repo="icn", path="/w/icn/feature", branch="feature", status=dict(CLEAN)),
               dict(repo="sdk", path="/w/sdk/ref", branch="ref", status=dict(CLEAN))]
        snap = _wsnap(gh_obs, wts, pr_views)
        wv = {w["path"]: w for w in reconcile.worktree_views(snap, [], CATALOG)}
        views = {"prs": list(pr_views), "sessions": [], "handoffs": [],
                 "worktrees": [dict(w, locks=[], pr=w["pr"]) for w in wv.values()]}
        acts = A.build(views, {"icn": {}, "sdk": {}}, marks or {}, self.paths, CATALOG,
                       pr_observed=reconcile.open_pr_observed(snap), now=NOW)
        by_wt = {w["path"]: a for a in acts for w in a["worktrees"]}
        return wv, by_wt

    def test_failed_repository_worktree_is_protected_and_observed_one_stays_visible(self):
        partial = obs({"sdk": []}, error="partial: icn: timeout")
        wv, acts = self._scenario(partial)
        b = acts["/w/icn/feature"]
        self.assertEqual(wv["/w/icn/feature"]["verdict"], "KEEP")
        self.assertIn("open-PR state unknown for icn", wv["/w/icn/feature"]["keep_reasons"])
        self.assertEqual(b["status"], A.UNRESOLVED, b["why"])
        self.assertIn("open-PR state unknown for icn", b["protections"])
        self.assertFalse(b["retirable"])
        self.assertNotIn("retirement", b["next"])
        # The observed repository is unaffected: its clean reference checkout is classified as before.
        a = acts["/w/sdk/ref"]
        self.assertEqual(a["status"], A.ARCHIVED)
        self.assertTrue(a["retirable"])
        self.assertNotIn("open-PR state unknown for sdk", wv["/w/sdk/ref"]["keep_reasons"])

    def test_archived_mark_cannot_unprotect_an_unobserved_worktree(self):
        marks = {"wt:/w/icn/feature": {"mark": "archived", "at": "x"}}
        _, acts = self._scenario(obs({"sdk": []}, error="partial: icn: timeout"), marks=marks)
        b = acts["/w/icn/feature"]
        self.assertEqual(b["status"], A.ARCHIVED, "the mark is display-only")
        self.assertFalse(b["retirable"])
        self.assertIn("open-PR state unknown for icn", b["protections"])

    def test_total_failure_and_cache_protect_every_worktree(self):
        for o in (obs(None, ok=False, error="offline"), obs({"icn": [], "sdk": []}, from_cache=True)):
            _, acts = self._scenario(o)
            for path, a in acts.items():
                self.assertFalse(a["retirable"], (path, o.error))

    def test_recovery_after_a_fresh_observation(self):
        # icn now observed: an open PR on the branch links and protects; an empty list makes it a plain checkout.
        open_pr = act_pr(9, branch="feature", updated=NOW - 3600, cross_repo=False)
        wv, acts = self._scenario(obs({"icn": [open_pr], "sdk": []}), pr_views=[open_pr])
        b = acts["/w/icn/feature"]
        self.assertIn("open PR icn#9", b["protections"])
        self.assertNotIn("open-PR state unknown for icn", b["protections"])
        self.assertEqual(b["status"], A.ACTIVE)
        _, acts = self._scenario(obs({"icn": [], "sdk": []}))
        b = acts["/w/icn/feature"]
        self.assertEqual(b["status"], A.ARCHIVED)
        self.assertEqual(b["protections"], [])
        self.assertTrue(b["retirable"], "a successful empty list is an observation of no PR")


# --------------------------------------------------------------------------- B3
def launcher(name, path, status=REGISTERED_NOT_RUNNING):
    return {"kind": "launcher-record", "name": name, "status": status, "cwd": path, "worktree": path,
            "state_label": "STOPPED (launcher record only)", "last_activity": NOW - 3600,
            "record": {"name": name, "claude_session_id": "11111111-2222-4333-8444-555555555555"}}


class B3ResumableProtection(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-b3-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.paths = Paths(icn_dev=os.path.join(self.tmp, "icn-dev"), control_state=os.path.join(self.tmp, "ctl"),
                           claude_home=os.path.join(self.tmp, "claude"), proc=os.path.join(self.tmp, "proc"))
        os.makedirs(self.paths.proc)
        self.w = wt("feature", branch="feature")

    def act(self, sessions, marks=None, hist=None):
        acts = A.build({"prs": [], "sessions": list(sessions), "worktrees": [self.w], "handoffs": []},
                       {"icn": hist or {}}, marks or {}, self.paths, CATALOG, pr_observed=frozenset({"icn"}), now=NOW)
        return next(a for a in acts if a["worktrees"])

    CASES = {
        "no PR, no mark": ({}, {}),
        "archived mark": ({"wt:/w/icn/feature": {"mark": "archived", "at": "x"}}, {}),
        "PR merged": ({}, {"merged": {"feature": [{"number": 5}]}}),
        "PR closed": ({}, {"closed": {"feature": [{"number": 6}]}}),
    }

    def test_resumable_conversation_protects_in_every_end_state(self):
        for label, (marks, hist) in self.CASES.items():
            a = self.act([launcher("lane-conv", self.w["path"])], marks, hist)
            self.assertIn("resumable conversation lane-conv", a["protections"], label)
            self.assertFalse(a["retirable"], f"{label}: {a['status']}")
            self.assertNotIn("request retirement", a["next"], label)

    def test_without_a_conversation_the_same_checkouts_stay_retirable(self):
        # Negative control: the protection comes from the conversation, not from the end state.
        for label, (marks, hist) in self.CASES.items():
            if label == "no PR, no mark":
                continue  # an old clean checkout with no PR is Archived and retirable; covered by the next line
            a = self.act([], marks, hist)
            self.assertTrue(a["retirable"], f"{label}: {a['status']} {a['protections']}")
        self.assertTrue(self.act([])["retirable"])

    def test_every_resumable_record_is_listed(self):
        a = self.act([launcher("one", self.w["path"]), launcher("two", self.w["path"])], hist={"merged": {"feature": [{"number": 5}]}})
        self.assertIn("resumable conversation one", a["protections"])
        self.assertIn("resumable conversation two", a["protections"])

    def test_a_stale_record_alone_does_not_protect_forever(self):
        a = self.act([launcher("old", self.w["path"], status=STALE)], hist={"merged": {"feature": [{"number": 5}]}})
        self.assertNotIn("resumable conversation old", a["protections"])
        self.assertTrue(a["retirable"])

    def test_classification_and_protection_share_one_predicate(self):
        # Paused-because-resumable and the protection must agree on which records count.
        for status in (REGISTERED_NOT_RUNNING, STALE):
            a = self.act([launcher("r", self.w["path"], status=status)])
            protected = "resumable conversation r" in a["protections"]
            paused_for_it = any("saved conversation" in why for why in a["why"])
            self.assertEqual(protected, paused_for_it, status)

    def test_retirement_request_states_the_protection(self):
        a = self.act([launcher("lane-conv", self.w["path"])], hist={"merged": {"feature": [{"number": 5}]}})
        path = A.retirement_request(a, os.path.join(self.tmp, "req"))
        with open(path) as f:
            text = f.read()
        self.assertIn("resumable conversation lane-conv", text)
        self.assertNotIn("protections: none observed", text)


if __name__ == "__main__":
    unittest.main()
