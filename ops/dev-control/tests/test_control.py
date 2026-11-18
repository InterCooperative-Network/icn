"""Tests for the ICN Development Control Center (stdlib unittest; no network, no host state).

Run from ops/dev-control:  python3 -m unittest discover -s tests -v
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, ROOT)

from icn_control import github as gh  # noqa: E402
from icn_control import gitwt, procs, records, reconcile, snapshot  # noqa: E402
from icn_control.util import (AWAITING_MAINTAINER, BLOCKED, IDLE, IN_PROGRESS, LIVE, REGISTERED_NOT_RUNNING,  # noqa: E402
                              STALE, UNKNOWN, CmdResult, Obs, Paths, redact)

CATALOG = reconcile.load_catalog()
NOW = time.time()


def obs(data, ok=True, key="x", from_cache=False, error=None):
    return Obs(source=key, observed_at=NOW, ok=ok, data=data, error=error, from_cache=from_cache)


class Fixture(unittest.TestCase):
    """A disposable ~/icn-dev, ~/.claude and /proc lookalike."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-test-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.paths = Paths(icn_dev=os.path.join(self.tmp, "icn-dev"), claude_home=os.path.join(self.tmp, "claude"),
                           icn_session_state=os.path.join(self.tmp, "icn-session"),
                           control_state=os.path.join(self.tmp, "control"), proc=os.path.join(self.tmp, "proc"))
        for d in (self.paths.state, self.paths.handoffs, self.paths.repos, self.paths.worktrees, self.paths.proc):
            os.makedirs(d, exist_ok=True)

    def fake_proc(self, pid, start, comm="claude"):
        d = os.path.join(self.paths.proc, str(pid))
        os.makedirs(d, exist_ok=True)
        fields = ["S", "1", str(pid), str(pid), "0", "-1", "0"] + ["0"] * 12 + [str(start)] + ["0"] * 10
        with open(os.path.join(d, "stat"), "w") as f:
            f.write(f"{pid} ({comm}) " + " ".join(fields))

    def wt_dir(self, name):
        p = os.path.join(self.paths.worktrees, "icn", name)
        os.makedirs(p, exist_ok=True)
        return p


# --------------------------------------------------------------------------- process matching
class ProcessMatching(Fixture):
    def test_pid_alone_is_not_identity(self):
        self.fake_proc(4242, 1000)
        self.assertTrue(procs.same_process(4242, "1000", self.paths.proc))
        self.assertFalse(procs.same_process(4242, "999", self.paths.proc), "reused pid must not match")
        self.assertFalse(procs.same_process(4242, None, self.paths.proc), "no recorded start time proves nothing")
        self.assertFalse(procs.same_process(5555, "1000", self.paths.proc), "dead pid")
        self.assertFalse(procs.same_process("not-a-pid", "1", self.paths.proc))

    def test_comm_with_spaces_and_parens(self):
        d = os.path.join(self.paths.proc, "77")
        os.makedirs(d)
        fields = ["S", "1"] + ["0"] * 17 + ["31337"] + ["0"] * 5
        with open(os.path.join(d, "stat"), "w") as f:
            f.write("77 (we ird) (x) " + " ".join(fields))
        self.assertEqual(procs.proc_start_ticks(77, self.paths.proc), "31337")

    def test_real_own_process(self):
        start = procs.proc_start_ticks(os.getpid())
        self.assertTrue(procs.same_process(os.getpid(), start))

    def test_agent_role_and_name_from_fixed_tokens_only(self):
        self.assertEqual(procs.agent_role(["/home/u/.codex/packages/app-server-daemon/releases/x/codex"]), "service")
        self.assertEqual(procs.agent_role(["codex", "app-server", "proxy"]), "service")
        self.assertEqual(procs.agent_role(["claude", "--model", "opus"]), "interactive")
        self.assertEqual(procs.agent_name(["claude", "--name", "review-2906"]), "review-2906")
        self.assertIsNone(procs.agent_name(["claude", "-p", "a prompt with --name inside"]))

    def test_claude_session_files_validated_and_key_files_never_read(self):
        sd = os.path.join(self.paths.claude_home, "sessions")
        os.makedirs(sd)
        self.fake_proc(100, 500)
        for pid, start, sid in ((100, "500", "s-live"), (101, "600", "s-dead")):
            with open(os.path.join(sd, f"{pid}.json"), "w") as f:
                json.dump({"pid": pid, "procStart": start, "sessionId": sid, "cwd": "/x"}, f)
        key = os.path.join(sd, "100.secret.key")
        with open(key, "w") as f:
            f.write("SECRET")
        os.chmod(key, 0)  # any attempt to read it would raise
        rows = {r["sessionId"]: r for r in procs.claude_session_files(self.paths)}
        self.assertTrue(rows["s-live"]["process_alive"])
        self.assertFalse(rows["s-dead"]["process_alive"])


# --------------------------------------------------------------------------- registry / records
class RegistryAndRecords(Fixture):
    def make_db(self, rows):
        db = os.path.join(self.paths.repos, "icn.git", "icn-ops.db")
        os.makedirs(os.path.dirname(db), exist_ok=True)
        con = sqlite3.connect(db)
        con.execute("CREATE TABLE sessions (id TEXT PRIMARY KEY, repo TEXT, worktree TEXT, task_description TEXT,"
                    " started_at TEXT, last_heartbeat TEXT, worktree_path TEXT, worktree_name TEXT,"
                    " provider_session_id TEXT, agent_pid INTEGER)")
        con.execute("CREATE TABLE file_claims (file_path TEXT, session_id TEXT, claimed_at TEXT)")
        for r in rows:
            con.execute("INSERT INTO sessions (id, repo, worktree_path, worktree_name, provider_session_id, last_heartbeat,"
                        " task_description) VALUES (?,?,?,?,?,?,?)", r)
        con.execute("INSERT INTO file_claims VALUES ('icn/src/a.rs', 'r1', '2026-11-17 00:00:00')")
        con.commit()
        con.close()
        return db

    def test_registry_read_only_and_unchanged(self):
        db = self.make_db([("r1", "icn", "/w/a", "a", "sess-1", "2026-11-17 10:00:00", "task A")])
        with open(db, "rb") as f:
            before = hashlib.sha256(f.read()).hexdigest()
        d = records.registry_sessions(db)
        self.assertEqual(d["sessions"][0]["claims"], ["icn/src/a.rs"])
        with open(db, "rb") as f:
            self.assertEqual(hashlib.sha256(f.read()).hexdigest(), before)
        con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
        with self.assertRaises(sqlite3.OperationalError):
            con.execute("INSERT INTO sessions (id, repo) VALUES ('x','y')")
        con.close()

    def test_missing_registry_is_unavailable_not_empty(self):
        with self.assertRaises(records.SourceUnavailable):
            records.registry_sessions(os.path.join(self.tmp, "nope.db"))
        self.assertFalse(os.path.exists(os.path.join(self.tmp, "nope.db")), "must not create a database")

    def test_yaml_missing_file_and_missing_pyyaml(self):
        with self.assertRaises(records.SourceUnavailable):
            records.active_worktrees(self.paths)
        with open(os.path.join(self.paths.state, "active-worktrees.yaml"), "w") as f:
            f.write("worktrees: []\n")
        with mock.patch.object(records, "yaml", None):
            with self.assertRaises(records.SourceUnavailable):
                records.active_worktrees(self.paths)

    def test_active_worktree_refs_and_lock_conflicts(self):
        if records.yaml is None:
            self.skipTest("PyYAML not installed")
        with open(os.path.join(self.paths.state, "active-worktrees.yaml"), "w") as f:
            f.write("worktrees:\n  - repo: icn\n    path: worktrees/icn/a\n    branch: b\n"
                    "    task: \"see ~/icn-dev/handoffs/x.md\"\n    status: open\n")
        rows = records.active_worktrees(self.paths)["rows"]
        row = rows[os.path.join(self.paths.icn_dev, "worktrees", "icn", "a")]
        self.assertEqual(row["refs"], ["~/icn-dev/handoffs/x.md"])
        locks = [{"repo": "icn", "owner": "a", "status": "open", "files": ["Cargo.lock"]},
                 {"repo": "icn", "owner": "b", "status": "open", "files": ["Cargo.lock"]},
                 {"repo": "icn", "owner": "c", "status": "released", "files": ["Cargo.lock"]}]
        self.assertEqual(records.open_lock_conflicts(locks), [{"repo": "icn", "file": "Cargo.lock", "owners": ["a", "b"]}])

    def test_handoff_summary_sections(self):
        p = os.path.join(self.paths.handoffs, "h.md")
        with open(p, "w") as f:
            f.write("# Title\n## Results\n- done thing\n## Open follow-ups\n- left\n## Decision needed\n- pick\n## Next\n- step\n")
        sm = records.summarize_handoff(p)
        self.assertEqual(sm["title"], "Title")
        self.assertEqual(sm["sections"]["done"][0]["lines"], ["- done thing"])
        self.assertEqual(sm["sections"]["decision"][0]["lines"], ["- pick"])
        self.assertEqual(sm["sections"]["next"][0]["lines"], ["- step"])


# --------------------------------------------------------------------------- git parsing
class GitParsing(unittest.TestCase):
    def test_worktree_porcelain(self):
        text = ("worktree /r/icn.git\nbare\n\nworktree /w/a\nHEAD abc\nbranch refs/heads/feat/x\n\n"
                "worktree /w/b\nHEAD def\ndetached\nprunable gitdir file points to non-existent location\n")
        rows = gitwt.parse_worktree_porcelain(text)
        self.assertEqual(rows[1]["branch"], "feat/x")
        self.assertTrue(rows[2]["detached"])
        self.assertIn("prunable", rows[2])

    def test_status_v2(self):
        st = gitwt.parse_status_v2("# branch.oid abc\n# branch.upstream origin/x\n# branch.ab +2 -1\n"
                                   "1 .M N... 100644 100644 100644 a b f.rs\n? new.txt\nu UU N... x\n")
        self.assertEqual((st["ahead"], st["behind"], st["modified"], st["untracked"], st["conflicted"]), (2, 1, 1, 1, 1))
        self.assertTrue(st["dirty"])
        self.assertFalse(gitwt.parse_status_v2("# branch.oid abc\n")["dirty"])
        self.assertIsNone(gitwt.parse_status_v2("# branch.oid abc\n")["ahead"], "no upstream = unknown, not zero")

    def test_toplevel_longest_prefix_and_no_false_prefix(self):
        wts = [{"path": "/w/icn/a"}, {"path": "/w/icn/a-b"}, {"path": "/w/icn/a/nested"}]
        self.assertEqual(gitwt.toplevel_for("/w/icn/a-b/src", wts)["path"], "/w/icn/a-b")
        self.assertEqual(gitwt.toplevel_for("/w/icn/a/nested/x", wts)["path"], "/w/icn/a/nested")
        self.assertIsNone(gitwt.toplevel_for("/w/icn/abc", wts))

    def test_real_git_dirty_and_local_only_commits(self):
        if shutil.which("git") is None:
            self.skipTest("git not installed")
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.invalid",
                   GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@example.invalid")
        run = lambda *a: subprocess.run(["git", "-C", tmp, *a], check=True, capture_output=True, env=env)  # noqa: E731
        run("init", "-q")
        with open(os.path.join(tmp, "f"), "w") as f:
            f.write("x")
        run("add", "f")
        run("commit", "-qm", "c")
        with open(os.path.join(tmp, "g"), "w") as f:
            f.write("y")
        st = gitwt.worktree_status({"path": tmp})
        self.assertTrue(st["dirty"])
        self.assertEqual(st["local_only_commits"], 1, "a commit on no remote ref must be counted")
        self.assertFalse(os.path.exists(os.path.join(tmp, ".git", "index.lock")))
        self.assertEqual(gitwt.worktree_status({"path": os.path.join(tmp, "gone")}), {"exists": False})


# --------------------------------------------------------------------------- GitHub reconciliation
FROZEN_BODY = """intro
<!-- ICN-DELIVERY-LIFECYCLE:BEGIN -->
```
ICN DELIVERY LIFECYCLE
State:                FROZEN
Lane:                 STANDARD
Review generation:    1 (FULL) at 029d0511; CLOSED
Freeze head:          029d0511056bce919aea5618da97e19bf055b41f (= reviewed head)
Known blockers:       0
Follow-up ledger:     #2907
```
<!-- ICN-DELIVERY-LIFECYCLE:END -->
"""
REQ = ["Build Release", "Test"]


def pr(**kw):
    base = {"repo": "icn", "number": 1, "title": "t", "draft": False, "branch": "b", "head": "029d0511056bce919aea5618da97e19bf055b41f",
            "base": "main", "updated_ts": NOW, "author": "me", "mergeable": "MERGEABLE", "merge_state": "CLEAN",
            "checks": [{"name": "Build Release", "status": "COMPLETED", "conclusion": "SUCCESS"},
                       {"name": "Test", "status": "COMPLETED", "conclusion": "SUCCESS"}],
            "unresolved_threads": 0, "reviews": [], "lifecycle": gh.parse_lifecycle(FROZEN_BODY), "url": "u"}
    base.update(kw)
    return base


class GitHubReconciliation(unittest.TestCase):
    def test_lifecycle_parse(self):
        lc = gh.parse_lifecycle(FROZEN_BODY)
        self.assertEqual((lc["state"], lc["ledger"], lc["freeze_head"][:8]), ("FROZEN", 2907, "029d0511"))
        self.assertIsNone(gh.parse_lifecycle("no block here"))
        self.assertIsNone(gh.parse_lifecycle(None))

    def test_reviews_never_count_automation_as_approval(self):
        bot = gh.classify_review({"state": "APPROVED", "author": {"login": "copilot-pull-request-reviewer", "__typename": "Bot"}}, "me")
        ai = gh.classify_review({"state": "APPROVED", "author": {"login": "me", "__typename": "User"}, "authorAssociation": "OWNER",
                                 "body": "> AI-generated independent review\nFindings:    0 BLOCKER · 4 FOLLOW_UP · 0 QUESTION · FULL"}, "me")
        own = gh.classify_review({"state": "APPROVED", "author": {"login": "me", "__typename": "User"}, "body": "lgtm",
                                  "authorAssociation": "OWNER"}, "me")
        human = gh.classify_review({"state": "APPROVED", "author": {"login": "other", "__typename": "User"}, "body": "ok",
                                    "authorAssociation": "MEMBER"}, "me")
        outsider = gh.classify_review({"state": "APPROVED", "author": {"login": "drive-by", "__typename": "User"},
                                       "authorAssociation": "NONE",
                                       "body": "FULL\nFindings: 0 BLOCKER · 0 FOLLOW_UP · 0 QUESTION"}, "me")
        self.assertFalse(outsider["counts_as_human_approval"], "approval needs a write association")
        self.assertIsNone(outsider["findings"], "an outsider cannot assert a review generation")
        self.assertFalse(bot["counts_as_human_approval"])
        unlisted = gh.classify_review({"state": "APPROVED", "author": {"login": "brand-new-reviewer", "__typename": "Bot"}}, "me")
        suffixed = gh.classify_review({"state": "APPROVED", "author": {"login": "x[bot]", "__typename": "User"}}, "me")
        self.assertEqual(unlisted["kind"], "automated (bot)", "GraphQL Bot type alone must classify as automation")
        self.assertFalse(suffixed["counts_as_human_approval"])
        self.assertFalse(ai["counts_as_human_approval"])
        self.assertEqual(ai["findings"], {"blocker": 0, "follow_up": 4, "question": 0})
        self.assertFalse(own["counts_as_human_approval"])
        self.assertTrue(human["counts_as_human_approval"])

    def test_required_checks_unknown_missing_failing_pending_stacked(self):
        self.assertEqual(gh.required_check_status(pr(), None)["verdict"], "UNKNOWN")
        self.assertEqual(gh.required_check_status(pr(), REQ)["verdict"], "GREEN")
        self.assertEqual(gh.required_check_status(pr(checks=[]), REQ)["verdict"], "INCOMPLETE")
        failing = pr(checks=[{"name": "Test", "status": "COMPLETED", "conclusion": "FAILURE"},
                             {"name": "Build Release", "status": "COMPLETED", "conclusion": "SUCCESS"}])
        self.assertEqual(gh.required_check_status(failing, REQ)["verdict"], "FAILING")
        pending = pr(checks=[{"name": "Test", "status": "IN_PROGRESS", "conclusion": None},
                             {"name": "Build Release", "status": "COMPLETED", "conclusion": "SUCCESS"}])
        self.assertEqual(gh.required_check_status(pending, REQ)["verdict"], "PENDING")
        self.assertEqual(gh.required_check_status(pr(base="feat/x"), REQ)["verdict"], "NOT RUN")

    def test_readiness_never_ready_when_anything_unknown(self):
        p = pr()
        self.assertTrue(gh.merge_readiness(p, gh.required_check_status(p, REQ))["verdict"].startswith("MERGE-ELIGIBLE"))
        self.assertEqual(gh.merge_readiness(p, gh.required_check_status(p, None))["verdict"], "NOT READY")
        self.assertEqual(gh.merge_readiness(pr(lifecycle=None), gh.required_check_status(p, REQ))["verdict"], "NOT READY")
        self.assertEqual(gh.merge_readiness(pr(mergeable=None), gh.required_check_status(p, REQ))["verdict"], "NOT READY")
        moved = pr(head="ffff" * 10)
        r = gh.merge_readiness(moved, gh.required_check_status(moved, REQ))
        self.assertIn("head moved after freeze (lifecycle block is stale)", r["reasons"])

    def test_pr_status_frozen_moved_and_review_pending(self):
        paths = Paths(icn_dev=tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, paths.icn_dev, ignore_errors=True)
        snap = {"github_prs": obs({"icn": [pr(number=2905), pr(number=7, head="ffff" * 10),
                                           pr(number=2906, lifecycle=gh.parse_lifecycle(FROZEN_BODY.replace("FROZEN", "IMPLEMENTING")),
                                              reviews=[gh.classify_review({"state": "COMMENTED", "author": {"login": "me"}, "authorAssociation": "OWNER",
                                                                           "commit": {"oid": "029d0511056bce919aea5618da97e19bf055b41f"},
                                                                           "body": "AI-generated. FULL\nFindings: 0 BLOCKER · 4 FOLLOW_UP · 0 QUESTION"}, "me")])]}),
                "required_checks": obs({"icn": {"checks": REQ}})}
        views = {p["number"]: p for p in reconcile.pr_views(snap, paths, CATALOG)}
        self.assertEqual(views[2905]["status"], AWAITING_MAINTAINER)
        self.assertIn("#2907", views[2905]["status_reason"])
        self.assertEqual(views[7]["status"], STALE)
        self.assertIn("author disposition pending", views[2906]["status_reason"])
        self.assertIn("4 FOLLOW_UP", views[2906]["status_reason"])

    def test_spoofed_lifecycle_from_outsider_is_ignored(self):
        raw = {"number": 9, "title": "t\x1b]0;pwned\x07", "body": FROZEN_BODY, "authorAssociation": "NONE",
               "author": {"login": "drive-by"}, "headRefOid": "029d0511056bce919aea5618da97e19bf055b41f"}
        p = gh.normalize_pr(raw, "icn")
        self.assertIsNone(p["lifecycle"])
        self.assertTrue(p["lifecycle_untrusted"])
        p.update(required=gh.required_check_status(p, REQ))
        status, why = reconcile.pr_status(p)
        self.assertEqual(status, UNKNOWN)
        self.assertIn("not a repository member", why)
        member = gh.normalize_pr(dict(raw, authorAssociation="MEMBER"), "icn")
        self.assertEqual(member["lifecycle"]["state"], "FROZEN")

    def test_terminal_escapes_are_neutralized(self):
        from icn_control.util import clean
        self.assertEqual(clean("a\x1b[2Jb\x07c\u202ed\te\nf\x9b"), "a?[2Jb?c?d\te\nf?")
        self.assertEqual(clean(None), "")

    def test_gh_missing_is_unavailable_not_zero(self):
        with mock.patch.object(gh, "run", return_value=CmdResult(127, "", "gh: not found")):
            with self.assertRaises(gh.GhUnavailable):
                gh.fetch_open_prs("o", "r")
        with mock.patch.object(gh, "run", return_value=CmdResult(1, "", "HTTP 504")):
            with self.assertRaises(gh.GhUnavailable):
                gh.fetch_open_prs("o", "r", retries=0)
        snap = {"github_prs": obs(None, ok=False, key="GitHub open PRs", error="gh api failed: offline")}
        self.assertEqual(reconcile.pr_views(snap, Paths(), CATALOG), [])
        att = reconcile.attention(snap, [], [], [])
        self.assertTrue(any(a["status"] == UNKNOWN and "unavailable" in a["title"] for a in att))


# --------------------------------------------------------------------------- session reconciliation
class SessionReconciliation(Fixture):
    def base_snap(self, **over):
        snap = {"processes": obs([]), "tmux": obs({"available": True, "sessions": [], "panes": []}),
                "claude_sessions": obs([]), "registry": obs({"sessions": []}), "icn_session": obs([]),
                "worktrees": obs({"worktrees": []}), "active_worktrees": obs({"rows": {}}),
                "file_locks": obs({"locks": []}), "handoffs": obs([]), "_pr_views": []}
        snap.update(over)
        return snap

    def test_worktree_on_disk_is_never_an_active_agent(self):
        p = self.wt_dir("old-lane")
        snap = self.base_snap(worktrees=obs({"worktrees": [{"repo": "icn", "path": p, "branch": "x", "status": {"exists": True}}]}))
        self.assertEqual(reconcile.session_views(snap, self.paths), [])

    def test_live_agent_vs_registry_only(self):
        p = self.wt_dir("lane")
        self.fake_proc(900, 42)
        procs_ = [{"pid": 900, "ppid": 1, "comm": "claude", "cwd": p, "start": "42", "agent_role": "interactive"}]
        csf = [{"pid": 900, "sessionId": "S1", "cwd": p, "status": "busy", "statusUpdatedAt": NOW * 1000, "process_alive": True, "name": "lane-agent"}]
        reg = {"sessions": [
            {"id": "r-live", "provider_session_id": "S1", "worktree_path": p, "worktree_name": "lane", "last_heartbeat_ts": NOW, "claims": ["a.rs"]},
            {"id": "r-ghost", "provider_session_id": "S9", "worktree_path": p, "worktree_name": "ghost", "last_heartbeat_ts": NOW - 60, "claims": []},
            {"id": "r-old", "provider_session_id": "S8", "worktree_path": p, "worktree_name": "old", "last_heartbeat_ts": NOW - 86400, "claims": []}]}
        snap = self.base_snap(processes=obs(procs_), claude_sessions=obs(csf), registry=obs(reg),
                              worktrees=obs({"worktrees": [{"repo": "icn", "path": p, "branch": "x"}]}))
        v = {s["name"]: s for s in reconcile.session_views(snap, self.paths)}
        self.assertEqual(v["lane-agent"]["status"], LIVE)
        self.assertEqual(v["lane-agent"]["state_label"], "WORKING")
        self.assertIn("a.rs (registry claim)", v["lane-agent"]["claims"])
        self.assertEqual(v["ghost"]["status"], REGISTERED_NOT_RUNNING)
        self.assertEqual(v["old"]["status"], STALE)

    def test_registered_pr_ref_beats_inference(self):
        p = self.wt_dir("pr2905-review")  # directory name would infer #2905
        reg = {"sessions": [{"id": "r", "provider_session_id": "S", "worktree_path": p, "worktree_name": "x",
                             "last_heartbeat_ts": NOW, "claims": [], "pr_ref": "icn#2906"}]}
        prs = [{"repo": "icn", "number": 2905, "branch": "a", "title": "a"}, {"repo": "icn", "number": 2906, "branch": "b", "title": "b"}]
        v = reconcile.session_views(self.base_snap(registry=obs(reg), _pr_views=prs), self.paths)
        self.assertEqual((v[0]["pr"]["number"], v[0]["pr"]["how"]), (2906, "registered pr_ref/task_ref"))

    def test_dead_claude_session_file_is_not_live(self):
        csf = [{"pid": 901, "sessionId": "S2", "cwd": "/x", "status": "busy", "process_alive": False}]
        snap = self.base_snap(claude_sessions=obs(csf))
        self.assertEqual(reconcile.session_views(snap, self.paths), [])

    def test_launcher_record_stopped_vs_outdated(self):
        a, b = self.wt_dir("a"), self.wt_dir("b")
        self.fake_proc(910, 7)
        recs = [{"name": "icn-a", "purpose": "a", "worktree": a, "launch_mode": "foreground", "fg_pid": "910", "fg_start": "6",
                 "claude_session_id": "C1", "file": "fa"},
                {"name": "icn-b", "purpose": "b", "worktree": b, "launch_mode": "foreground", "fg_pid": "999", "fg_start": "1",
                 "claude_session_id": "C2", "file": "fb"}]
        procs_ = [{"pid": 910, "ppid": 1, "comm": "claude", "cwd": b, "start": "7", "agent_role": "interactive"}]
        snap = self.base_snap(icn_session=obs(recs), processes=obs(procs_),
                              worktrees=obs({"worktrees": [{"repo": "icn", "path": a}, {"repo": "icn", "path": b}]}))
        v = {s["name"]: s for s in reconcile.session_views(snap, self.paths)}
        self.assertEqual(v["icn-a"]["status"], REGISTERED_NOT_RUNNING, "pid 910 is alive but with another start time")
        self.assertIn("Safe to resume", v["icn-a"]["resume"])
        self.assertEqual(v["icn-b"]["status"], STALE)
        self.assertIn("Do not resume", v["icn-b"]["resume"])

    def test_tmux_without_agent_and_agent_in_tmux(self):
        p = self.wt_dir("t")
        procs_ = [{"pid": 50, "ppid": 1, "comm": "zsh", "cwd": p, "start": "1"},
                  {"pid": 51, "ppid": 50, "comm": "claude", "cwd": p, "start": "2", "agent_role": "interactive"},
                  {"pid": 60, "ppid": 1, "comm": "zsh", "cwd": p, "start": "3"}]
        tm = {"available": True, "sessions": [{"name": "with-agent", "attached": 0, "activity": NOW},
                                              {"name": "shell-only", "attached": 0, "activity": NOW},
                                              {"name": "icn-control", "attached": 1, "activity": NOW, "is_control_center": True}],
              "panes": [{"session": "with-agent", "pane_pid": 50, "path": p}, {"session": "shell-only", "pane_pid": 60, "path": p},
                        {"session": "icn-control", "pane_pid": 70, "path": p}]}
        snap = self.base_snap(processes=obs(procs_), tmux=obs(tm))
        v = {s["name"]: s for s in reconcile.session_views(snap, self.paths)}
        agent = next(s for s in v.values() if s["kind"] == "agent")
        self.assertEqual(agent["tmux"], "with-agent")
        self.assertNotIn("with-agent", v)
        self.assertEqual(v["shell-only"]["state_label"], "TMUX OPEN, NO AGENT PROCESS")
        self.assertEqual(v["shell-only"]["status"], IDLE)
        self.assertEqual(v["icn-control"]["kind"], "control-center")

    def test_service_process_is_not_an_occupying_agent(self):
        procs_ = [{"pid": 70, "ppid": 1, "comm": "codex", "cwd": "/somewhere", "start": "1", "agent_role": "service"}]
        v = reconcile.session_views(self.base_snap(processes=obs(procs_)), self.paths)
        self.assertEqual(v[0]["kind"], "service")
        self.assertNotIn("OCCUPIED", v[0]["resume"])


# --------------------------------------------------------------------------- ownership / worktrees
class Ownership(Fixture):
    def wsnap(self, wts, prs=(), merged=(), gh_ok=True, locks=()):
        return {"worktrees": obs({"worktrees": wts}), "active_worktrees": obs({"rows": {}}),
                "file_locks": obs({"locks": list(locks)}), "github_merged": obs(list(merged)),
                "github_prs": obs({}) if gh_ok else obs(None, ok=False, error="offline"), "_pr_views": list(prs)}

    def test_keep_reasons_and_candidates(self):
        clean = {"exists": True, "dirty": False, "local_only_commits": 0}
        wts = [{"repo": "icn", "path": "/w/dirty", "branch": "d", "status": dict(clean, dirty=True, modified=2)},
               {"repo": "icn", "path": "/w/unpushed", "branch": "u", "status": dict(clean, local_only_commits=3)},
               {"repo": "icn", "path": "/w/merged", "branch": "m", "status": clean},
               {"repo": "icn", "path": "/w/locked", "branch": "l", "status": clean},
               {"repo": "icn", "path": "/w/unknown", "branch": "k", "status": dict(clean, local_only_commits=None)}]
        merged = [{"repo": "icn", "branch": "m", "number": 5, "merged_ts": NOW, "cross_repo": False},
                  {"repo": "icn", "branch": "l", "number": 6, "merged_ts": NOW, "cross_repo": False}]
        locks = [{"repo": "icn", "owner": "locked", "status": "open", "files": ["x"]}]
        v = {w["name"]: w for w in reconcile.worktree_views(self.wsnap(wts, merged=merged, locks=locks), [], CATALOG)}
        self.assertEqual(v["dirty"]["verdict"], "KEEP")
        self.assertEqual(v["unpushed"]["verdict"], "KEEP")
        self.assertTrue(v["merged"]["verdict"].startswith("CLEANUP CANDIDATE"))
        self.assertEqual(v["locked"]["verdict"], "KEEP", "an open file lock outranks a merged PR")
        self.assertIn("unpushed-commit count unknown", v["unknown"]["keep_reasons"])

    def test_fork_pr_with_same_branch_name_never_matches(self):
        clean = {"exists": True, "dirty": False, "local_only_commits": 0}
        wts = [{"repo": "icn", "path": "/w/x", "branch": "fix/x", "status": clean}]
        fork_open = [{"repo": "icn", "branch": "fix/x", "number": 66, "status": IN_PROGRESS, "cross_repo": True}]
        fork_merged = [{"repo": "icn", "branch": "fix/x", "number": 67, "merged_ts": NOW, "cross_repo": True}]
        unknown = [{"repo": "icn", "branch": "fix/x", "number": 68, "merged_ts": NOW}]  # field absent = not trusted
        v = reconcile.worktree_views(self.wsnap(wts, prs=fork_open, merged=fork_merged + unknown), [], CATALOG)[0]
        self.assertIsNone(v["pr"])
        self.assertIsNone(v["merged_pr"])
        self.assertFalse(v["verdict"].startswith("CLEANUP"), "a fork's merge must not suggest cleaning our worktree")
        self.assertTrue(gh.normalize_pr({"number": 1, "isCrossRepository": None}, "icn")["cross_repo"])
        self.assertFalse(gh.normalize_pr({"number": 1, "isCrossRepository": False}, "icn")["cross_repo"])

    def test_github_unknown_keeps_everything(self):
        wts = [{"repo": "icn", "path": "/w/m", "branch": "m", "status": {"exists": True, "dirty": False, "local_only_commits": 0}}]
        v = reconcile.worktree_views(self.wsnap(wts, gh_ok=False), [], CATALOG)
        self.assertEqual(v[0]["verdict"], "KEEP")
        self.assertIn("GitHub state unknown", v[0]["keep_reasons"])

    def test_same_name_worktrees_in_two_repos_stay_distinct(self):
        st = {"exists": True, "dirty": False, "local_only_commits": 0}
        wts = [{"repo": "icn", "path": "/w/icn/main", "branch": "main", "status": st},
               {"repo": "nycn", "path": "/w/nycn/main", "branch": "main", "status": dict(st, dirty=True)}]
        prs = [{"repo": "nycn", "branch": "main", "number": 9, "status": IN_PROGRESS, "cross_repo": False}]
        v = {w["path"]: w for w in reconcile.worktree_views(self.wsnap(wts, prs=prs), [], CATALOG)}
        self.assertIsNone(v["/w/icn/main"]["pr"])
        self.assertEqual(v["/w/nycn/main"]["pr"]["number"], 9)
        self.assertEqual(v["/w/nycn/main"]["program"], "nycn")


# --------------------------------------------------------------------------- stale state & cache
class StaleState(Fixture):
    def test_cache_round_trip_is_marked_and_private(self):
        snap = {"github_prs": obs({"icn": []}), "processes": obs([{"pid": 1}])}
        snapshot.save_cache(self.paths, snap)
        path = snapshot.cache_path(self.paths)
        self.assertEqual(stat.S_IMODE(os.stat(path).st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(os.stat(self.paths.control_state).st_mode), 0o700)
        loaded = snapshot.load_cache(self.paths)
        self.assertTrue(loaded["github_prs"].from_cache)
        self.assertNotIn("processes", loaded, "live process state is never cached")
        att = reconcile.attention(loaded, [], [], [])
        self.assertTrue(any(a["status"] == STALE and "cached" in a["title"].lower() for a in att))

    def test_failed_refresh_keeps_last_good_data_as_stale(self):
        c = snapshot.Collector(self.paths, CATALOG, use_cache=False)
        good = obs({"icn": []}, key="GitHub open PRs")
        bad = Obs("GitHub open PRs", NOW + 5, False, None, error="offline")
        with mock.patch.object(snapshot, "collect_github", side_effect=[{"github_prs": good}, {"github_prs": bad}]), \
                mock.patch.object(snapshot, "save_cache"):
            c._run_tier("github")
            c._run_tier("github")
        o = c.snapshot()["github_prs"]
        self.assertTrue(o.ok and o.from_cache)
        self.assertIn("refresh failed: offline", o.error)

    def test_corrupt_cache_is_ignored(self):
        os.makedirs(self.paths.control_state)
        with open(snapshot.cache_path(self.paths), "w") as f:
            f.write("{not json")
        self.assertEqual(snapshot.load_cache(self.paths), {})

    def test_old_disk_guard_and_overdue_reservation_warn(self):
        snap = {"disk_guard": obs({"file": "dg", "mtime": NOW - 7 * 3600, "data": {"level": "OK"}}),
                "reservations": obs({"reservations": [{"id": "r1", "lane": "x", "expires": str(int(NOW - 10))}]}),
                "fs": obs([{"mount": "/mnt/build", "used_pct": 95.0, "free_gib": 5, "is_mountpoint": True}])}
        titles = [a["title"] for a in reconcile.health_warnings(snap, NOW)]
        self.assertIn("Disk guard report is old", titles)
        self.assertTrue(any("past TTL" in t for t in titles))
        self.assertTrue(any(a["status"] == BLOCKED for a in reconcile.health_warnings(snap, NOW)))


# --------------------------------------------------------------------------- programs, safety helpers
class ProgramsAndSafety(unittest.TestCase):
    def test_all_seven_programs_and_deterministic_classification(self):
        self.assertEqual([p["name"] for p in CATALOG["programs"]],
                         ["ICN Core", "NYCN", "Mutualware", "Public & Website", "Community & Learning",
                          "Infrastructure & Integration", "Research Incubator"])
        self.assertFalse(CATALOG["canonical"])
        self.assertEqual(reconcile.classify("nycn", "fix website", CATALOG)[0], "nycn", "repo rule wins")
        self.assertEqual(reconcile.classify("icn", "docs(website): x", CATALOG)[0], "public-website")
        self.assertEqual(reconcile.classify("icn", "fix(federation): book once", CATALOG)[0], "icn-core")
        self.assertEqual(reconcile.classify("icn", "Mutualware research notebook", CATALOG)[0], "mutualware")
        self.assertEqual(reconcile.classify("icn", "cipher", CATALOG)[0], "icn-core", "keyword 'ci' must not match inside words")

    def test_program_with_no_work_is_not_called_inactive(self):
        progs = reconcile.program_views(CATALOG, [], [], [], [], {})
        mw = next(p for p in progs if p["id"] == "mutualware")
        self.assertIn("No source-backed next action", mw["next"])
        self.assertIn("unassigned", mw["owner"])

    def test_redaction(self):
        self.assertNotIn("ghp_", redact("token ghp_" + "a" * 36))
        self.assertIn("<REDACTED>", redact("password: hunter2"))
        self.assertEqual(redact("plain text"), "plain text")

    def test_may_open(self):
        from icn_control.tui import may_open
        root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, root, ignore_errors=True)
        os.makedirs(os.path.join(root, "handoffs"))
        os.makedirs(os.path.join(root, "private"))
        ok = os.path.join(root, "handoffs", "h.md")
        open(ok, "w").close()
        link = os.path.join(root, "handoffs", "escape.md")
        os.symlink("/etc/passwd", link)
        self.assertTrue(may_open(ok, root)[0])
        self.assertFalse(may_open(link, root)[0], "symlink out of the root is refused")
        self.assertFalse(may_open(os.path.join(root, "handoffs", "id.key"), root)[0])
        self.assertFalse(may_open(os.path.join(root, "private", "x.json"), root)[0])
        self.assertTrue(may_open(os.path.join(root, "private", "OPERATIONS-HANDOFF.md"), root)[0])


# --------------------------------------------------------------------------- launcher behaviour
@unittest.skipIf(shutil.which("tmux") is None, "tmux not installed")
class LauncherSafety(unittest.TestCase):
    """Runs bin/icn-control against an ISOLATED tmux server (TMUX_TMPDIR), never the host's."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-launch-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.env = {k: v for k, v in os.environ.items() if k != "TMUX"}
        self.env.update(TMUX_TMPDIR=self.tmp, ICN_CONTROL_STATE_DIR=os.path.join(self.tmp, "state"),
                        ICN_DEV_ROOT=os.path.join(self.tmp, "icn-dev"), ICN_SESSION_STATE_DIR=os.path.join(self.tmp, "s"),
                        ICN_CONTROL_CLAUDE_HOME=os.path.join(self.tmp, "claude"), HOME=self.tmp)
        os.makedirs(os.path.join(self.tmp, "icn-dev", "repos"))
        self.addCleanup(lambda: subprocess.run(["tmux", "kill-server"], env=self.env, capture_output=True))
        self.launcher = os.path.join(ROOT, "bin", "icn-control")

    def run_l(self, *args):
        return subprocess.run([self.launcher, *args], env=self.env, capture_output=True, text=True, timeout=120,
                              stdin=subprocess.DEVNULL)

    def test_status_when_not_running(self):
        r = self.run_l("--status")
        self.assertEqual(r.returncode, 0)
        self.assertIn("not running", r.stdout)

    def test_open_without_terminal_refuses_cleanly(self):
        r = self.run_l()
        self.assertEqual(r.returncode, 2)
        self.assertIn("--report", r.stderr)

    def test_never_touches_a_foreign_session_with_our_name(self):
        subprocess.run(["tmux", "new-session", "-d", "-s", "icn-control", "sleep 300"], env=self.env, check=True)
        r = self.run_l("--restart")
        self.assertEqual(r.returncode, 4)
        self.assertIn("not ours", r.stderr)
        self.assertEqual(subprocess.run(["tmux", "has-session", "-t", "=icn-control"], env=self.env).returncode, 0)

    def test_report_with_empty_host_shows_unknowns_not_success(self):
        r = self.run_l("--report", "--no-github")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Source not yet observed: github_prs", r.stdout)
        self.assertIn("UNKNOWN", r.stdout)
        self.assertIn("[FAIL  ] git worktrees", r.stdout)

    def test_caller_directory_and_pythonpath_cannot_inject_code(self):
        planted = os.path.join(self.tmp, "planted")
        os.makedirs(planted)
        for mod in ("json.py", "icn_control.py", "argparse.py"):
            with open(os.path.join(planted, mod), "w") as f:
                f.write("raise SystemExit('PLANTED MODULE EXECUTED')\n")
        env = dict(self.env, PYTHONPATH=planted)
        r = subprocess.run([self.launcher, "--version"], env=env, cwd=planted, capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("PLANTED", r.stdout + r.stderr)

    def test_unknown_option(self):
        self.assertEqual(self.run_l("--bogus").returncode, 2)


if __name__ == "__main__":
    unittest.main()
