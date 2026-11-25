"""v0.4 activity consolidation: grouping by evidence, classification, views, marks, retirement requests,
duplicate session records and recovery labels. Fixtures only; nothing on the host is read or changed."""

from __future__ import annotations

import json
import os
import shutil
import stat
import sys
import tempfile
import time
import unittest.mock

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, ROOT)

from icn_control import activities as A  # noqa: E402
from icn_control import picker, reconcile  # noqa: E402
from icn_control.util import AWAITING_MAINTAINER, IN_PROGRESS, LIVE, REGISTERED_NOT_RUNNING, STALE, Paths  # noqa: E402

NOW = time.time()
CATALOG = reconcile.load_catalog()
U1 = "11111111-2222-4333-8444-555555555555"


def wt(name, repo="icn", branch=None, pr=None, dirty=False, local_only=0, mtime=NOW - 40 * 86400, **kw):
    w = {"repo": repo, "path": f"/w/{repo}/{name}", "name": name, "branch": branch if branch is not None else f"feat/{name}",
         "head": "abc123", "status": {"exists": True, "dirty": dirty, "local_only_commits": local_only, "mtime": mtime,
                                      "modified": 1 if dirty else 0},
         "pr": pr, "locks": [], "row": None, "program": "icn-core", "program_rule": "test"}
    w.update(kw)
    return w


def pr(n, repo="icn", branch=None, status=IN_PROGRESS, updated=NOW - 10 * 86400, **kw):
    p = {"repo": repo, "number": n, "title": f"PR {n}", "branch": branch or f"feat/pr{n}", "draft": True,
         "updated_ts": updated, "status": status, "status_reason": f"reason {n}", "program": "icn-core",
         "required": {"verdict": "GREEN"}, "lifecycle": None, "evidence_dirs": []}
    p.update(kw)
    return p


def agent(name, cwd, worktree=None, pr_ref=None, label="IDLE AT PROMPT", tmux="t"):
    return {"kind": "agent", "name": name, "status": LIVE, "cwd": cwd, "worktree": worktree, "pid": 7, "tmux": tmux,
            "state_label": label, "last_activity": NOW - 60, "pr": pr_ref, "handoffs": [], "evidence": []}


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-a-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.paths = Paths(icn_dev=os.path.join(self.tmp, "icn-dev"), claude_home=os.path.join(self.tmp, "claude"),
                           control_state=os.path.join(self.tmp, "ctl"), proc=os.path.join(self.tmp, "proc"))
        os.makedirs(self.paths.proc)

    def build(self, prs=(), worktrees=(), sessions=(), history=None, marks=None, gh_ok=True, handoffs=(), pr_observed=None):
        views = {"prs": list(prs), "worktrees": list(worktrees), "sessions": list(sessions), "handoffs": list(handoffs)}
        if pr_observed is None:  # gh_ok: every repository in the fixture had its open-PR list observed
            pr_observed = frozenset({w["repo"] for w in worktrees} | {p["repo"] for p in prs}) if gh_ok else frozenset()
        return A.build(views, {} if history is None else history, marks or {}, self.paths, CATALOG,
                       pr_observed=pr_observed, now=NOW)

    def one(self, acts, key):
        found = [a for a in acts if a["id"] == key]
        self.assertEqual(len(found), 1, [a["id"] for a in acts])
        return found[0]


class Reconciliation(Base):
    def test_one_activity_collects_pr_worktrees_sessions_reviews_and_handoffs(self):
        p = pr(2906, branch="fix/nb042", status=AWAITING_MAINTAINER, evidence_dirs=["/s/pr2906-independent-review"],
               lifecycle={"ledger": 2907})
        author = wt("nb042", branch="fix/nb042", pr={"number": 2906, "status": AWAITING_MAINTAINER})
        review = wt("review-pr-2906", branch="review/pr-2906")
        s1 = agent("author", author["path"] + "/icn", worktree=author["path"])
        s2 = agent("reviewer", "/s/pr2906-independent-review", pr_ref={"repo": "icn", "number": 2906, "how": "inferred from directory name"})
        h = {"path": "/h/2026-11-17-pr2906-review.md", "mtime": NOW}
        acts = self.build([p], [author, review], [s1, s2], handoffs=[h])
        self.assertEqual(len(acts), 1)
        a = acts[0]
        self.assertEqual(a["id"], "pr:icn#2906")
        self.assertEqual({w["name"] for w in a["worktrees"]}, {"nb042", "review-pr-2906"})
        self.assertEqual({s["name"] for s in a["sessions"]}, {"author", "reviewer"})
        self.assertIn(h["path"], a["handoffs"])
        self.assertEqual(a["ledgers"], ["icn#2907"])
        self.assertTrue(any("review checkout review-pr-2906 names PR #2906" in ln for ln in a["links"]))
        self.assertTrue(a["attention"])

    def test_similar_names_alone_never_merge(self):
        acts = self.build([], [wt("ledger-evidence"), wt("ledger-evidence-archive"), wt("ledger-s0a")],
                          history={"icn": {"merged": {}, "closed": {}}})
        self.assertEqual(len(acts), 3)

    def test_review_number_must_name_an_open_pr_in_the_same_repo(self):
        acts = self.build([pr(5, repo="nycn")], [wt("review-pr-5", repo="icn", branch="review/pr-5")],
                          history={"icn": {}})
        self.assertEqual(len(acts), 2, "PR 5 in nycn must not absorb an icn review checkout")

    def test_review_checkouts_of_a_non_open_pr_are_not_merged_through_it(self):
        acts = self.build([], [wt("review-pr-2644", branch="review/pr-2644"), wt("review-2644-fable-final", branch="")],
                          history={"icn": {}})
        self.assertEqual(len(acts), 2, "only an OPEN PR can join review checkouts; a number alone is not evidence")
        self.assertTrue(all(a["id"].startswith("wt:") for a in acts))

    def test_services_and_dashboard_are_not_work(self):
        svc = {"kind": "service", "name": "codex service", "cwd": "/w/icn/x", "worktree": "/w/icn/x", "status": LIVE}
        cc = {"kind": "control-center", "name": "icn-control", "cwd": "/x", "status": LIVE}
        shell = {"kind": "tmux", "name": "shell", "cwd": "/tmp", "status": "IDLE"}
        self.assertEqual(self.build([], [], [svc, cc, shell]), [])

    def test_duplicate_session_records_fold_into_one_activity_and_picker_row(self):
        w = wt("lane", pr=None)
        live = agent("lane-agent", w["path"], worktree=w["path"])
        outdated = {"kind": "launcher-record", "name": "icn-lane", "status": STALE, "cwd": w["path"], "worktree": w["path"],
                    "state_label": "RECORD OUTDATED - another agent is live in this worktree", "last_activity": NOW - 9000}
        reg = {"kind": "registry", "name": "lane", "status": STALE, "cwd": w["path"], "worktree": w["path"],
               "state_label": "REGISTERED", "last_activity": NOW - 90000}
        acts = self.build([], [w], [live, outdated, reg], history={"icn": {}})
        self.assertEqual(len(acts), 1)
        self.assertEqual(len(acts[0]["sessions"]), 3)
        scoped = picker.scope_sessions([live, outdated, reg], acts)
        self.assertIn(live, scoped)
        self.assertNotIn(outdated, scoped, "the outdated record is folded into the live agent")
        self.assertEqual(len(picker.scope_sessions([live, outdated, reg], acts, include_all=True)), 3)


class Classification(Base):
    H = {"icn": {"merged": {"feat/done": [{"number": 10, "title": "done", "at": "2026-11-01"}],
                            "feat/dirtydone": [{"number": 11, "title": "x", "at": "2026-11-01"}]},
                 "closed": {"feat/dropped": [{"number": 12, "title": "dropped", "at": "2026-10-01"}]}}}

    def status(self, **kw):
        acts = self.build(history=kw.pop("history", self.H), **kw)
        self.assertEqual(len(acts), 1)
        return acts[0]

    def test_rules(self):
        cases = [
            ("live agent", dict(worktrees=[wt("a")], sessions=[agent("x", "/w/icn/a", worktree="/w/icn/a")]), A.ACTIVE),
            ("decision due", dict(prs=[pr(1, status=AWAITING_MAINTAINER)]), A.ACTIVE),
            ("PR moved today", dict(prs=[pr(2, updated=NOW - 3600)]), A.ACTIVE),
            ("old open PR", dict(prs=[pr(3, updated=NOW - 20 * 86400)]), A.PAUSED),
            ("merged", dict(worktrees=[wt("done", branch="feat/done")]), A.COMPLETED),
            ("merged but dirty", dict(worktrees=[wt("dirtydone", branch="feat/dirtydone", dirty=True)]), A.UNRESOLVED),
            ("closed unmerged", dict(worktrees=[wt("dropped", branch="feat/dropped")]), A.ARCHIVED),
            ("dirty, no PR", dict(worktrees=[wt("wip", dirty=True)]), A.PAUSED),
            ("unpublished commits, no PR", dict(worktrees=[wt("lost", local_only=2)]), A.UNRESOLVED),
            ("fresh, empty", dict(worktrees=[wt("new", mtime=NOW - 3600)]), A.PAUSED),
            ("clean, old, nothing unique", dict(worktrees=[wt("ref")]), A.ARCHIVED),
            ("no PR history", dict(worktrees=[wt("ref")], history=None), A.UNRESOLVED),
        ]
        for label, kw, want in cases:
            if label == "no PR history":
                acts = A.build({"prs": [], "worktrees": kw["worktrees"], "sessions": [], "handoffs": []}, None, {},
                               self.paths, CATALOG, pr_observed=frozenset({"icn"}), now=NOW)
                self.assertEqual(acts[0]["status"], want, label)
                continue
            a = self.status(**kw)
            self.assertEqual(a["status"], want, f"{label}: {a['why']}")
            self.assertTrue(a["why"], label)

    def test_protections_and_retirable(self):
        done = self.status(worktrees=[wt("done", branch="feat/done")])
        self.assertTrue(done["retirable"])
        locked = self.status(worktrees=[wt("done", branch="feat/done", local_only=1)])
        self.assertFalse(locked["retirable"], "commits on no remote ref protect the checkout")
        self.assertTrue(any("remote-tracking" in p for p in locked["protections"]))
        active = self.status(prs=[pr(1, status=AWAITING_MAINTAINER)])
        self.assertFalse(active["retirable"])


class ViewsAndMarks(Base):
    def test_views(self):
        acts = self.build([pr(1, status=AWAITING_MAINTAINER), pr(2, updated=NOW - 5 * 86400), pr(3, updated=NOW - 90 * 86400)],
                          [wt("done", branch="feat/done"), wt("ref")],
                          history={"icn": {"merged": {"feat/done": [{"number": 9}]}, "closed": {}}})
        by = {a["id"]: a for a in acts}
        now_ids = {a["id"] for a in acts if A.in_view(a, "now", NOW)}
        self.assertEqual(now_ids, {"pr:icn#1", "pr:icn#2"}, "dormant paused work leaves the default view")
        self.assertIn("pr:icn#3", {a["id"] for a in acts if A.in_view(a, "open", NOW)})
        arch = {a["id"] for a in acts if A.in_view(a, "archive", NOW)}
        self.assertEqual(arch, {"wt:/w/icn/done", "wt:/w/icn/ref"})
        self.assertEqual(by["wt:/w/icn/done"]["status"], A.COMPLETED)
        self.assertEqual(A.summary(acts)["attention"], 1)

    def test_marks_are_display_only_private_and_never_hide_a_live_agent(self):
        w = wt("wip", dirty=True)
        A.set_mark(self.paths, "wt:/w/icn/wip", "archived", "parked")
        st = os.stat(A.marks_path(self.paths))
        self.assertEqual(stat.S_IMODE(st.st_mode), 0o600)
        with open(A.marks_path(self.paths)) as f:
            self.assertFalse(json.load(f)["canonical"])
        acts = self.build([], [w], history={"icn": {}}, marks=A.load_marks(self.paths))
        self.assertEqual(acts[0]["status"], A.ARCHIVED)
        self.assertEqual(acts[0]["derived_status"], A.PAUSED)
        self.assertIn("uncommitted changes", " ".join(acts[0]["protections"]), "a mark never removes protection")
        live = self.build([], [w], [agent("x", w["path"], worktree=w["path"])], history={"icn": {}}, marks=A.load_marks(self.paths))
        self.assertEqual(live[0]["status"], A.ACTIVE)
        A.set_mark(self.paths, "wt:/w/icn/wip", None)
        self.assertEqual(A.load_marks(self.paths), {})
        with self.assertRaises(ValueError):
            A.set_mark(self.paths, "x", "deleted")
        with open(A.marks_path(self.paths), "w") as f:
            f.write("{broken")
        self.assertEqual(A.load_marks(self.paths), {})

    def test_retirement_request_writes_a_proposal_and_runs_nothing(self):
        acts = self.build([], [wt("done", branch="feat/done")],
                          history={"icn": {"merged": {"feat/done": [{"number": 9}]}, "closed": {}}})
        d = os.path.join(self.tmp, "req")
        with unittest.mock.patch("subprocess.run", side_effect=AssertionError("must not run anything")), \
                unittest.mock.patch("os.system", side_effect=AssertionError("must not run anything")):
            path = A.retirement_request(acts[0], d)
        with open(path) as f:
            text = f.read()
        self.assertIn("wt-clean icn --apply --only done@<sha>", text)
        self.assertIn("- [ ] worktree `/w/icn/done`", text)


class RecoveryLabels(unittest.TestCase):
    def test_labels(self):
        rs = picker.run_state
        self.assertEqual(rs({"kind": "agent", "state_label": "WORKING", "tmux": None, "tty": "/dev/pts/0"}, []),
                         "RUNNING · busy · outside tmux (/dev/pts/0)")
        self.assertEqual(rs({"kind": "agent", "state_label": "IDLE AT PROMPT", "tmux": "t"}, []), "RUNNING · waiting")
        ok = [{"id": "resume", "enabled": True, "reason": ""}]
        self.assertEqual(rs({"kind": "launcher-record", "status": REGISTERED_NOT_RUNNING}, ok), "STOPPED · resumable")
        nt = [{"id": "resume", "enabled": False, "reason": "No transcript for conversation x; it cannot be resumed reliably."}]
        self.assertEqual(rs({"kind": "launcher-record", "status": STALE, "state_label": "STOPPED"}, nt), "STOPPED · transcript unavailable")
        man = [{"id": "resume", "enabled": False, "reason": "Only an icn-ops registry entry exists ... Manual route ..."}]
        self.assertEqual(rs({"kind": "registry", "status": REGISTERED_NOT_RUNNING}, man), "STOPPED · no launcher record")
        self.assertEqual(rs({"kind": "registry", "status": STALE}, [{"id": "resume", "enabled": False, "reason": "x"}]),
                         "STALE registry entry")
        out = {"kind": "launcher-record", "status": STALE, "state_label": "RECORD OUTDATED - another agent is live"}
        self.assertEqual(rs(out, [{"id": "resume", "enabled": False, "reason": "It is running now"}]),
                         "OUTDATED record · agent live here")

    def test_tty_decoding(self):
        self.assertEqual(reconcile.tty_name((136 << 8) | 3), "/dev/pts/3")
        self.assertEqual(reconcile.tty_name((137 << 8) | 44), "/dev/pts/300")
        self.assertIsNone(reconcile.tty_name(0))
        self.assertIsNone(reconcile.tty_name((4 << 8) | 1), "a virtual console is not a pty")


class ResumeSafetyAtTaskLevel(Base):
    def test_next_action_suggests_resume_only_when_verifiably_possible(self):
        w = wt("lane")
        reg = {"kind": "registry", "name": "lane-reg", "status": REGISTERED_NOT_RUNNING, "cwd": w["path"], "worktree": w["path"],
               "conversation": U1, "state_label": "REGISTERED", "last_activity": NOW}
        acts = self.build([], [w], [reg], history={"icn": {}})
        self.assertNotIn("Resume", acts[0]["next"], "a registry-only conversation is never offered as a resume")


if __name__ == "__main__":
    unittest.main()
