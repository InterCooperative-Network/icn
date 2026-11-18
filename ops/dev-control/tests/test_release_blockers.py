"""Regression coverage for the three release blockers of the PR #2908 FULL review (B1, B2, B3).

B1  merge readiness ignored mergeStateStatus and reviewDecision.
B2  a partial open-PR fetch was treated as a complete observation, so a worktree in the failed
    repository read as "no PR" and could become retirable.
B3  a resumable launcher conversation was not a retirement protection.
F1  (late finding after freeze) cached or stale GitHub observations still produced MERGE-ELIGIBLE.
N2  (late finding) the session switcher restored the previous selection on every redraw, so Up/Down/
    Home/End/Page keys were undone and Enter opened the first session.
N3  (late finding) only the first 100 review threads were read, so an unresolved later thread was
    invisible and readiness could read MERGE-ELIGIBLE.

Fixtures only; nothing on the host is read or changed except the repository's own merge policy,
which is read (never copied) so that a drift between policy and display fails here.
"""

from __future__ import annotations

import itertools
import os
import shutil
import sys
import tempfile
import time
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


# --------------------------------------------------------------------------- F1
def _ready_snap(now, pr_at=None, req_at=None, pr_cache=False, req_cache=False, prs=None, pr_ok=True, req_ok=True,
                pr_error=None, drop=()):
    snap = {"github_prs": Obs("GitHub open PRs", now if pr_at is None else pr_at, pr_ok,
                              ({"icn": prs if prs is not None else [gh_pr(number=1)]} if pr_ok else None),
                              error=pr_error, from_cache=pr_cache),
            "required_checks": Obs("required checks", now if req_at is None else req_at, req_ok,
                                   ({"icn": {"checks": REQ, "source": "test"}} if req_ok else None), from_cache=req_cache)}
    for k in drop:
        snap.pop(k)
    return snap


class F1ReadinessFreshness(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-f1-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.paths = Paths(icn_dev=os.path.join(self.tmp, "icn-dev"), control_state=os.path.join(self.tmp, "ctl"),
                           claude_home=os.path.join(self.tmp, "claude"), proc=os.path.join(self.tmp, "proc"))
        self.now = time.time()

    def readiness(self, snap, number=1):
        return next(p for p in reconcile.pr_views(snap, self.paths, CATALOG) if p["number"] == number)

    def assert_gap(self, snap, fragment):
        v = self.readiness(snap)
        self.assertEqual(v["readiness"]["verdict"], "NOT READY", fragment)
        gaps = [r for r in v["readiness"]["reasons"] if r.startswith("current GitHub evidence unavailable: ")]
        self.assertEqual(len(gaps), 1, v["readiness"]["reasons"])
        self.assertIn(fragment, gaps[0])
        return v

    def test_fresh_evidence_is_still_eligible(self):
        v = self.readiness(_ready_snap(self.now))
        self.assertTrue(v["readiness"]["verdict"].startswith(ELIGIBLE), v["readiness"])
        self.assertEqual(v["readiness"]["reasons"], [])
        self.assertIsNone(reconcile.readiness_evidence_gap(_ready_snap(self.now), self.now))

    def test_cached_pr_data_is_not_ready_but_details_stay_visible(self):
        v = self.assert_gap(_ready_snap(self.now, pr_at=self.now - 6 * 3600, pr_cache=True), "PR data is cached from")
        # Historical details remain on display; only the verdict refuses to vouch for them.
        self.assertEqual((v["number"], v["merge_state"], v["required"]["verdict"]), (1, "CLEAN", "GREEN"))

    def test_cached_required_checks_alone_are_not_ready(self):
        self.assert_gap(_ready_snap(self.now, req_at=self.now - 6 * 3600, req_cache=True), "required-check data is cached")

    def test_live_but_old_observation_is_not_ready(self):
        old = self.now - reconcile.READINESS_MAX_AGE_S - 5
        self.assert_gap(_ready_snap(self.now, pr_at=old), "PR data last observed")
        self.assert_gap(_ready_snap(self.now, req_at=old), "required-check data last observed")
        edge = self.now - reconcile.READINESS_MAX_AGE_S + 30
        self.assertTrue(self.readiness(_ready_snap(self.now, pr_at=edge, req_at=edge))["readiness"]["verdict"].startswith(ELIGIBLE))

    def test_missing_or_failed_required_checks_are_not_ready(self):
        self.assert_gap(_ready_snap(self.now, drop=("required_checks",)), "required-check data not observed")
        self.assert_gap(_ready_snap(self.now, req_ok=False), "required-check data unavailable")
        snap = _ready_snap(self.now, pr_ok=False)
        self.assertEqual(reconcile.pr_views(snap, self.paths, CATALOG), [], "no PR rows without PR data")
        self.assertIn("PR data unavailable", reconcile.readiness_evidence_gap(snap, self.now))
        self.assertIn("PR data not observed", reconcile.readiness_evidence_gap({}, self.now))

    def test_malformed_or_missing_observation_time_is_not_ready(self):
        for bad in (None, "yesterday", float("nan"), 0, -1, True, self.now + 3600):
            snap = _ready_snap(self.now)
            snap["github_prs"].observed_at = bad
            self.assert_gap(snap, "PR data has no usable observation time")

    def test_partial_fresh_observation_keeps_observed_repositories_judged(self):
        # A partial fetch is coverage-limited (B2), not stale: the repositories it did observe are current.
        v = self.readiness(_ready_snap(self.now, pr_error="partial: sdk: timeout"))
        self.assertTrue(v["readiness"]["verdict"].startswith(ELIGIBLE), v["readiness"])

    def test_failed_refresh_then_recovery(self):
        c = snapshot.Collector(self.paths, CATALOG, use_cache=False)
        good = _ready_snap(time.time())
        bad = {"github_prs": Obs("GitHub open PRs", time.time(), False, None, error="offline"),
               "required_checks": Obs("required checks", time.time(), False, None, error="offline")}
        with unittest.mock.patch.object(snapshot, "collect_github", side_effect=[good, bad, _ready_snap(time.time())]), \
                unittest.mock.patch.object(snapshot, "save_cache"):
            c._run_tier("github")
            self.assertTrue(self.readiness(c.snapshot())["readiness"]["verdict"].startswith(ELIGIBLE))
            c._run_tier("github")  # the collector keeps the last good data, marked cached
            v = self.readiness(c.snapshot())
            self.assertEqual(v["readiness"]["verdict"], "NOT READY")
            self.assertTrue(any("PR data is cached" in r for r in v["readiness"]["reasons"]), v["readiness"])
            c._run_tier("github")  # recovery
            self.assertTrue(self.readiness(c.snapshot())["readiness"]["verdict"].startswith(ELIGIBLE))

    def test_restart_from_disk_cache_is_not_ready(self):
        snapshot.save_cache(self.paths, _ready_snap(time.time()))
        loaded = snapshot.load_cache(self.paths)
        self.assertTrue(loaded["github_prs"].from_cache and loaded["required_checks"].from_cache)
        self.assert_gap(loaded, "is cached from")

    def test_gap_is_never_overridden_by_other_gates(self):
        p = gh_pr()
        self.assertTrue(verdict(p)["verdict"].startswith(ELIGIBLE))
        r = gh.merge_readiness(p, gh.required_check_status(p, REQ), READY_WHEN, evidence_gap="PR data is cached")
        self.assertEqual(r["verdict"], "NOT READY")
        self.assertEqual(r["reasons"], ["current GitHub evidence unavailable: PR data is cached"])


# --------------------------------------------------------------------------- N2
import curses  # noqa: E402  (key constants only; no terminal is initialised)

from icn_control import tui  # noqa: E402
from icn_control.util import LIVE  # noqa: E402


class _ScriptedScreen:
    """Stands in for the curses window: fixed size, drawing is ignored, keys come from a script."""

    def __init__(self, keys):
        self.keys = list(keys)

    def getmaxyx(self):
        return 50, 180

    def erase(self):
        pass

    def refresh(self):
        pass

    def addnstr(self, *a):
        pass

    def getch(self):
        if not self.keys:
            raise AssertionError("key script exhausted without the switcher choosing a session")
        k = self.keys.pop(0)
        return ord(k) if isinstance(k, str) else k


def _agent(name, age):
    return {"kind": "agent", "name": name, "status": LIVE, "last_activity": time.time() - age, "program": "icn-core",
            "repo": "icn", "cwd": f"/s/{name}", "state_label": "IDLE AT PROMPT", "pid": 1000 + int(name[1:]), "tmux": f"t-{name}"}


class N2SwitcherNavigation(unittest.TestCase):
    """Drives the real UI.session_picker loop; only the screen and the data source are scripted."""

    def run_picker(self, keys, sessions_per_refresh):
        ui = tui.UI.__new__(tui.UI)
        ui.scr, ui.color, ui.picker_all, ui.back_hint = _ScriptedScreen(keys), {}, False, ""
        ui.catalog, ui.paths = {"programs": []}, Paths(icn_dev=tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, ui.paths.icn_dev, ignore_errors=True)
        feed = list(sessions_per_refresh)
        state = {"sessions": feed[0]}

        def refresh_views(force=False):
            # Redraw n observes feed[n] (the last entry repeats), as the collector delivers new data.
            state["sessions"] = feed.pop(0) if len(feed) > 1 else feed[0]
            state["draws"] = state.get("draws", 0) + 1
        chosen = []
        ui.refresh_views = refresh_views
        ui.picker_sessions = lambda: state["sessions"]
        ui.session_activity_titles = lambda: {}
        ui.actions_for = lambda v: [{"id": "join", "enabled": True, "label": "Join session", "reason": "test"}]
        ui.action_menu = lambda view, preselect=None: chosen.append(view["name"]) or True
        ui.session_picker()
        self.assertEqual(len(chosen), 1)
        return chosen[0]

    FOUR = [_agent("s1", 10), _agent("s2", 20), _agent("s3", 30), _agent("s4", 40)]  # displayed s1..s4

    def test_navigation_keys_choose_the_highlighted_row(self):
        D, U, E = curses.KEY_DOWN, curses.KEY_UP, 10
        cases = [([E], "s1"), ([D, E], "s2"), ([D, D, E], "s3"), ([curses.KEY_END, E], "s4"),
                 ([curses.KEY_END, U, E], "s3"), ([curses.KEY_END, curses.KEY_HOME, E], "s1"),
                 ([curses.KEY_NPAGE, E], "s4"), ([curses.KEY_NPAGE, curses.KEY_PPAGE, E], "s1"),
                 ([D] * 9 + [E], "s4"), ([U, U, E], "s1")]
        for keys, want in cases:
            self.assertEqual(self.run_picker(keys, [self.FOUR]), want, keys)

    def test_filtering_then_navigating(self):
        self.assertEqual(self.run_picker(["s", "3", 10], [self.FOUR]), "s3")
        self.assertEqual(self.run_picker(["s", curses.KEY_DOWN, curses.KEY_DOWN, 10], [self.FOUR]), "s3")
        self.assertEqual(self.run_picker([curses.KEY_DOWN, "s", "4", 127, 127, curses.KEY_DOWN, 10], [self.FOUR]), "s2",
                         "a filter edit restarts from the top; navigation after it still holds")

    def test_selection_follows_the_session_when_a_refresh_reorders_rows(self):
        reordered = [_agent("s2", 5), _agent("s1", 10), _agent("s3", 30), _agent("s4", 40)]  # s2 now first
        # Navigated to s2, then the data refreshed and s2 moved to the top: Enter must still choose s2.
        self.assertEqual(self.run_picker([curses.KEY_DOWN, 10], [self.FOUR, reordered]), "s2")
        # No navigation, a redraw (-1 = no key) with reordered data: the highlighted session stays s1.
        self.assertEqual(self.run_picker([-1, 10], [self.FOUR, reordered]), "s1")

    def test_selection_survives_its_row_disappearing(self):
        gone = [_agent("s1", 10), _agent("s3", 30), _agent("s4", 40)]
        # On s2 when s2 vanishes: the index is clamped to a still-visible row and Enter names that row.
        self.assertIn(self.run_picker([curses.KEY_DOWN, 10], [self.FOUR, gone]), {"s1", "s3"})


# --------------------------------------------------------------------------- N3
import re  # noqa: E402

from test_control import FROZEN_BODY  # noqa: E402

FROZEN_HEAD = gh.parse_lifecycle(FROZEN_BODY)["freeze_head"]


class _FakeGraphQL:
    """Answers OPEN_PRS_QUERY like GitHub would: it honours the query's own reviewThreads(first:N) and returns
    totalCount / pageInfo only when the query asks for them. `threads` is the PR's full thread list."""

    def __init__(self, threads, drop=()):
        self.threads, self.drop, self.queries = threads, set(drop), []

    def __call__(self, query, variables, timeout=45.0):
        self.queries.append(query)
        m = re.search(r"reviewThreads\(first:(\d+)\)\{(.*?)nodes\{isResolved\}", query, re.S)
        first, fields = int(m.group(1)), m.group(2)
        conn = {"nodes": [{"isResolved": r} for r in self.threads[:first]]}
        if "totalCount" in fields and "totalCount" not in self.drop:
            conn["totalCount"] = len(self.threads)
        if "hasNextPage" in fields and "pageInfo" not in self.drop:
            conn["pageInfo"] = {"hasNextPage": len(self.threads) > first}
        node = {"number": 7, "title": "t", "url": "u", "isDraft": False, "headRefName": "feat/t", "headRefOid": FROZEN_HEAD,
                "baseRefName": "main", "updatedAt": "2026-11-18T00:00:00Z", "createdAt": "2026-11-17T00:00:00Z",
                "isCrossRepository": False, "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN", "reviewDecision": None,
                "authorAssociation": "OWNER", "author": {"login": "me", "__typename": "User"}, "body": FROZEN_BODY,
                "reviews": {"nodes": []}, "reviewThreads": conn,
                "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": "SUCCESS", "contexts": {"nodes": [
                    {"__typename": "CheckRun", "name": n, "status": "COMPLETED", "conclusion": "SUCCESS"} for n in REQ]}}}}]}}
        return {"repository": {"pullRequests": {"pageInfo": {"hasNextPage": False, "endCursor": None}, "nodes": [node]}}}


class N3ReviewThreadCompleteness(unittest.TestCase):
    """End to end: fetch_open_prs -> normalize_pr -> pr_views, against a GraphQL fake that paginates like GitHub."""

    def view(self, threads, drop=()):
        fake = _FakeGraphQL(threads, drop)
        with unittest.mock.patch.object(gh, "_gh_graphql", fake):
            prs = gh.fetch_open_prs("o", "icn")
        snap = {"github_prs": obs({"icn": prs}), "required_checks": obs({"icn": {"checks": REQ, "source": "test"}})}
        paths = Paths(icn_dev=tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, paths.icn_dev, ignore_errors=True)
        return reconcile.pr_views(snap, paths, CATALOG)[0]

    def assert_ready(self, v):
        self.assertTrue(v["readiness"]["verdict"].startswith(ELIGIBLE), v["readiness"])

    def assert_incomplete(self, v):
        self.assertEqual(v["readiness"]["verdict"], "NOT READY")
        self.assertTrue(any(r.startswith("review threads incomplete") for r in v["readiness"]["reasons"]), v["readiness"])

    def test_query_asks_for_completeness_metadata(self):
        self.assertRegex(gh.OPEN_PRS_QUERY, r"reviewThreads\(first:100\)\{totalCount pageInfo\{hasNextPage\}")

    def test_complete_observations_are_judged_normally(self):
        for n in (0, 99, 100):
            v = self.view([True] * n)
            self.assertTrue(v["threads_complete"], n)
            self.assert_ready(v)
            self.assertEqual(gh.thread_summary(v), "0 unresolved")
        v = self.view([True] * 99 + [False])  # an unresolved thread inside the first page is still counted
        self.assertEqual(v["readiness"]["verdict"], "NOT READY")
        self.assertIn("1 unresolved review thread(s)", v["readiness"]["reasons"])
        self.assertTrue(v["threads_complete"])

    def test_unresolved_thread_beyond_the_first_page_is_never_eligible(self):
        v = self.view([True] * 100 + [False])  # the 101st thread is unresolved
        self.assertFalse(v["threads_complete"])
        self.assert_incomplete(v)
        self.assertEqual(gh.thread_summary(v), "at least 0 unresolved; 100 of 101 threads observed")
        live_like = self.view([True] * 100 + [False] * 4)  # the shape of the live witness: 104 threads, 4 unresolved on page 2
        self.assert_incomplete(live_like)
        self.assertIn("100 of 104 threads observed", gh.thread_summary(live_like))

    def test_more_than_a_page_even_if_all_resolved_is_not_vouched_for(self):
        self.assert_incomplete(self.view([True] * 150))

    def test_missing_pagination_metadata_is_not_completeness(self):
        for drop in (("totalCount",), ("pageInfo",), ("totalCount", "pageInfo")):
            for n in (0, 5):
                v = self.view([True] * n, drop=drop)
                self.assertFalse(v["threads_complete"], (drop, n))
                self.assert_incomplete(v)
        self.assertIn("an unknown number of", gh.thread_summary(self.view([True] * 3, drop=("totalCount",))))

    def test_inconsistent_metadata_is_not_completeness(self):
        node = {"number": 1, "reviewThreads": {"totalCount": 104, "pageInfo": {"hasNextPage": False},
                                               "nodes": [{"isResolved": True}] * 100}}
        self.assertFalse(gh.normalize_pr(node, "icn")["threads_complete"], "count disagrees with nodes returned")
        node["reviewThreads"] = {"totalCount": True, "pageInfo": {"hasNextPage": False}, "nodes": [{"isResolved": True}]}
        self.assertFalse(gh.normalize_pr(node, "icn")["threads_complete"], "a boolean is not a count")
        self.assertFalse(gh.normalize_pr({"number": 1}, "icn")["threads_complete"], "no connection at all")


if __name__ == "__main__":
    unittest.main()
