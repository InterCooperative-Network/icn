"""v0.3 session management: join, return, resume, open-in-worktree, quick switcher.

Unit tests use fixtures only. The end-to-end class drives the real curses UI inside an ISOLATED
tmux server (TMUX_TMPDIR) with fake `claude` processes and a fake `icn-session`, through a real
attached tmux client on a pseudo-terminal. Nothing on the host's own tmux server is touched.
"""

from __future__ import annotations

import json
import os
import pty
import select
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
import uuid as uuidlib
import unittest.mock

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, ROOT)

from icn_control import picker, procs, sessionctl  # noqa: E402
from icn_control.util import LIVE, REGISTERED_NOT_RUNNING, CmdResult, Paths  # noqa: E402

U1, U2 = "11111111-2222-4333-8444-555555555555", "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-s-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.paths = Paths(icn_dev=os.path.join(self.tmp, "icn-dev"), claude_home=os.path.join(self.tmp, "claude"),
                           icn_session_state=os.path.join(self.tmp, "s"), control_state=os.path.join(self.tmp, "c"),
                           proc=os.path.join(self.tmp, "proc"))
        os.makedirs(self.paths.proc)
        self.fake_bin = os.path.join(self.tmp, "icn-session")
        with open(self.fake_bin, "w") as f:
            f.write("#!/bin/sh\nexit 0\n")
        os.chmod(self.fake_bin, 0o755)
        p = unittest.mock.patch.dict(os.environ, {"ICN_SESSION_BIN": self.fake_bin})
        p.start()
        self.addCleanup(p.stop)

    def wt(self, name, repo="icn"):
        p = os.path.join(self.paths.worktrees, repo, name)
        os.makedirs(p, exist_ok=True)
        return p

    def transcript(self, cwd, uid):
        d = os.path.join(self.paths.claude_home, "projects", sessionctl.project_slugs(cwd)[0])
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, f"{uid}.jsonl"), "w").close()

    def fake_proc(self, pid, start):
        d = os.path.join(self.paths.proc, str(pid))
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "stat"), "w") as f:
            f.write(f"{pid} (claude) S 1 " + " ".join(["0"] * 17) + f" {start} " + " ".join(["0"] * 10))

    def record(self, wt, uid=U1, name="icn-lane", **kw):
        r = {"name": name, "purpose": "lane", "repo": "icn", "worktree": wt, "claude_session_id": uid,
             "launch_mode": "tmux", "fg_pid": "", "fg_start": "", "file": "/x/icn-lane.json"}
        r.update(kw)
        return r


# --------------------------------------------------------------------------- actions & plans
class Actions(Base):
    def test_join_only_for_live_agent_in_tmux(self):
        live = {"kind": "agent", "tmux": "icn-a", "pid": 5, "status": LIVE, "name": "a"}
        acts = {a["id"]: a for a in sessionctl.session_actions(live, [live], self.paths, "Ctrl-a")}
        self.assertTrue(acts["join"]["enabled"])
        self.assertEqual(acts["join"]["target"], "icn-a")
        self.assertIn("Ctrl-a, then capital L", acts["join"]["reason"])
        self.assertFalse(acts["resume"]["enabled"], "never a second copy of a running conversation")
        bare = dict(live, tmux=None)
        j = {a["id"]: a for a in sessionctl.session_actions(bare, [bare], self.paths)}["join"]
        self.assertFalse(j["enabled"])
        self.assertIn("outside tmux", j["reason"])
        svc = {"kind": "service", "tmux": None, "status": LIVE, "name": "s"}
        self.assertFalse(sessionctl.session_actions(svc, [], self.paths)[0]["enabled"])
        weird = dict(live, tmux="bad;name")
        self.assertFalse(sessionctl.session_actions(weird, [], self.paths)[0]["enabled"], "invalid tmux target")

    def test_resume_planned_only_from_exact_recorded_conversation(self):
        w = self.wt("lane")
        self.transcript(w, U1)
        view = {"kind": "launcher-record", "status": REGISTERED_NOT_RUNNING, "record": self.record(w), "conversation": U1}
        a = {x["id"]: x for x in sessionctl.session_actions(view, [view], self.paths)}["resume"]
        self.assertTrue(a["enabled"], a["reason"])
        self.assertEqual(a["argv"], [self.fake_bin, "open", "lane", "--repo", "icn", "--worktree", w, "--name", "icn-lane",
                                     "--no-create", "--no-attach"])
        self.assertTrue(any(U1 in line for line in a["confirm"]))

    def test_resume_refusals(self):
        w = self.wt("lane")
        self.transcript(w, U1)
        rec = self.record(w)
        cases = {
            "no transcript": (self.record(w, uid=U2), [], "cannot be resumed reliably"),
            "guess": (self.record(w, uid="latest"), [], "refusing to guess"),
            "running": (rec, [{"kind": "agent", "conversation": U1, "pid": 1, "cwd": "/elsewhere", "name": "x"}], "running right now"),
            "agent in worktree": (rec, [{"kind": "agent", "conversation": U2, "pid": 1, "cwd": w + "/sub", "name": "y"}], "Another agent"),
            "tmux exists": (rec, [{"kind": "tmux", "tmux": "icn-lane", "name": "icn-lane"}], "already exists"),
            "bad token": (self.record(w, name="x;rm -rf ~"), [], "not valid icn-session tokens"),
            "newline path": (self.record(w + "\nevil"), [], "missing or invalid"),
            "outside root": (self.record(self.tmp), [], "outside"),
        }
        for label, (r, sessions, expect) in cases.items():
            plan = sessionctl.plan_resume(r, sessions, self.paths)
            self.assertFalse(plan["ok"], label)
            self.assertIn(expect, plan["reason"], label)

    def test_resume_refuses_mismatched_worktree_association(self):
        w, other = self.wt("lane"), self.wt("other")
        self.transcript(other, U1)  # the conversation belongs to a DIFFERENT directory
        plan = sessionctl.plan_resume(self.record(w), [], self.paths)
        self.assertFalse(plan["ok"])
        self.assertIn("does not match", plan["reason"])

    def test_resume_refuses_live_foreground_record_but_not_reused_pid(self):
        w = self.wt("lane")
        self.transcript(w, U1)
        self.fake_proc(777, 42)
        self.assertFalse(sessionctl.plan_resume(self.record(w, fg_pid="777", fg_start="42"), [], self.paths)["ok"])
        self.assertTrue(sessionctl.plan_resume(self.record(w, fg_pid="777", fg_start="41"), [], self.paths)["ok"],
                        "same pid with another start time is a different (reused) process")

    def test_registry_only_conversation_is_never_started(self):
        w = self.wt("lane")
        self.transcript(w, U1)
        view = {"kind": "registry", "status": REGISTERED_NOT_RUNNING, "conversation": U1, "cwd": w, "name": "r"}
        a = {x["id"]: x for x in sessionctl.session_actions(view, [view], self.paths)}["resume"]
        self.assertFalse(a["enabled"])
        self.assertIn(f"claude --resume {U1}", a["reason"], "a manual route is offered instead")
        self.assertNotIn("argv", a)

    def test_manual_route_is_quoted_and_only_for_validated_paths(self):
        evil = self.wt("x; touch PWNED")
        self.transcript(evil, U1)
        view = {"kind": "registry", "status": REGISTERED_NOT_RUNNING, "conversation": U1, "cwd": evil, "name": "r"}
        reason = {x["id"]: x for x in sessionctl.session_actions(view, [view], self.paths)}["resume"]["reason"]
        self.assertIn("cd '" + evil + "' && claude --resume", reason, "the path is shell-quoted")
        outside = os.path.join(self.tmp, "outside")
        os.makedirs(outside)
        self.transcript(outside, U1)
        view2 = dict(view, cwd=outside)
        reason2 = {x["id"]: x for x in sessionctl.session_actions(view2, [view2], self.paths)}["resume"]["reason"]
        self.assertNotIn("claude --resume", reason2, "no command is suggested for a path outside the worktrees root")

    def test_missing_launcher_binary(self):
        w = self.wt("lane")
        self.transcript(w, U1)
        with unittest.mock.patch.object(sessionctl, "icn_session_bin", return_value=None):
            self.assertIn("not installed", sessionctl.plan_resume(self.record(w), [], self.paths)["reason"])


class OpenInWorktree(Base):
    def wtview(self, name, **kw):
        p = self.wt(name)
        v = {"repo": "icn", "path": p, "name": name, "branch": f"feat/{name}", "head": "abc", "status": {"exists": True}}
        v.update(kw)
        return v

    def plan(self, v, sessions=(), records=(), rc=0):
        return sessionctl.plan_open_in_worktree(v, list(sessions), list(records), self.paths, check_path=lambda p: rc)

    def test_ok_and_exact_argv(self):
        v = self.wtview("lane-x")
        pl = self.plan(v)
        self.assertTrue(pl["ok"], pl.get("reason"))
        self.assertEqual(pl["argv"], [self.fake_bin, "open", "lane-x", "--repo", "icn", "--worktree", v["path"],
                                      "--name", "icn-lane-x", "--no-create", "--no-attach"])

    def test_refusals(self):
        live = lambda p: [{"kind": "agent", "pid": 9, "cwd": p, "name": "busy"}]  # noqa: E731
        v = self.wtview("lane-y")
        cases = [
            (self.wtview("review-pr-1"), [], 0, "read-only"),
            (self.wtview("main-readonly"), [], 0, "read-only"),
            (self.wtview("lane-d", detached=True, branch=None), [], 0, "Detached"),
            (self.wtview("lane-p", prunable=True), [], 0, "prunable"),
            (v, live(v["path"]), 0, "already live"),
            (self.wtview("lane-z"), [], 3, "protected"),
            (self.wtview("lane-q"), [], 1, "could not confirm"),
            (self.wtview("bad.name"), [], 0, "not a valid icn-session purpose"),
            (dict(self.wtview("lane-m"), status={"exists": False}), [], 0, "does not exist"),
        ]
        for wt, sessions, rc, expect in cases:
            pl = self.plan(wt, sessions, rc=rc)
            self.assertFalse(pl["ok"], wt["name"])
            self.assertIn(expect, pl["reason"], wt["name"])
        outside = dict(v, path=self.tmp)
        self.assertIn("Only worktrees under", self.plan(outside)["reason"])

    def test_shell_cwd_is_not_an_owning_agent(self):
        v = self.wtview("lane-s")
        shell = [{"kind": "tmux", "tmux": "someshell", "cwd": v["path"], "name": "someshell"}]
        self.assertTrue(self.plan(v, shell)["ok"])

    def test_existing_record_means_resume_and_is_shown(self):
        v = self.wtview("lane-r")
        rec = {"name": "icn-lane-r", "worktree": v["path"], "claude_session_id": U1}
        pl = self.plan(v, records=[rec])
        self.assertTrue(pl["ok"])
        self.assertTrue(any("RESUME" in w for w in pl["warnings"]))
        self.assertFalse(self.plan(v, sessions=[{"kind": "agent", "conversation": U1, "pid": 1, "cwd": "/x", "name": "n"}],
                                   records=[rec])["ok"])
        self.assertIn("points at", self.plan(v, records=[dict(rec, worktree="/elsewhere")])["reason"])
        self.assertIn("already exists", self.plan(v, sessions=[{"kind": "tmux", "tmux": "icn-lane-r", "name": "t"}])["reason"])


# --------------------------------------------------------------------------- join verification & tmux argv
class FakeTmux:
    def __init__(self, sessions, panes):
        self.sessions, self.panes, self.calls = set(sessions), panes, []

    def __call__(self, *args, timeout=10.0):
        self.calls.append(args)
        if args[0] == "has-session":
            return CmdResult(0 if args[2].lstrip("=") in self.sessions else 1, "", "")
        if args[0] == "list-panes":
            return CmdResult(0, "\n".join(str(p) for p in self.panes.get(args[3].lstrip("="), [])), "")
        if args[0] == "display-message":
            return CmdResult(0, "icn-control\n", "")
        if args[0] == "list-clients":
            return CmdResult(0, "100 /dev/pts/7\n300 /dev/pts/9\n200 /dev/pts/8\n", "")
        if args[0] == "list-keys":
            return CmdResult(1, "", "")
        return CmdResult(0, "", "")


class JoinVerification(Base):
    def setUp(self):
        super().setUp()
        self.fake_proc(500, 77)
        os.makedirs(os.path.join(self.paths.claude_home, "sessions"))
        with open(os.path.join(self.paths.claude_home, "sessions", "500.json"), "w") as f:
            json.dump({"pid": 500, "sessionId": U1}, f)
        self.view = {"kind": "agent", "tmux": "icn-a", "pid": 500, "pid_start": "77", "conversation": U1}

    def test_ok(self):
        ok, why = sessionctl.verify_join(self.view, self.paths, FakeTmux({"icn-a"}, {"icn-a": [500]}))
        self.assertTrue(ok, why)

    def test_stale_target(self):
        ok, why = sessionctl.verify_join(self.view, self.paths, FakeTmux({"icn-b"}, {}))
        self.assertFalse(ok)
        self.assertIn("stale", why)

    def test_pid_reused(self):
        ok, why = sessionctl.verify_join(dict(self.view, pid_start="76"), self.paths, FakeTmux({"icn-a"}, {"icn-a": [500]}))
        self.assertFalse(ok)
        self.assertIn("reused", why)

    def test_agent_moved_out_of_target(self):
        ok, why = sessionctl.verify_join(self.view, self.paths, FakeTmux({"icn-a"}, {"icn-a": [999]}))
        self.assertFalse(ok)
        self.assertIn("no longer inside", why)

    def test_conversation_changed(self):
        with open(os.path.join(self.paths.claude_home, "sessions", "500.json"), "w") as f:
            json.dump({"pid": 500, "sessionId": U2}, f)
        ok, why = sessionctl.verify_join(self.view, self.paths, FakeTmux({"icn-a"}, {"icn-a": [500]}))
        self.assertFalse(ok)
        self.assertIn("different conversation", why)

    def test_switch_moves_only_our_client_and_never_detaches(self):
        with unittest.mock.patch.dict(os.environ, {"TMUX": "/x,1,0", "TMUX_PANE": "%3"}):
            client = sessionctl.own_client(FakeTmux(set(), {}))
        self.assertEqual(client, "/dev/pts/9", "the most recently active client of the dashboard's session")
        argv = sessionctl.switch_argv("icn-a", client)
        self.assertEqual(argv, ["tmux", "switch-client", "-c", "/dev/pts/9", "-t", "=icn-a"])
        self.assertNotIn("-d", argv)
        self.assertNotIn("-d", sessionctl.attach_argv("icn-a"))
        self.assertEqual(sessionctl.attach_argv("icn-a")[-1], "=icn-a", "exact-match target, no prefix matching")
        with unittest.mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(sessionctl.own_client(FakeTmux(set(), {})))

    def test_return_key_state(self):
        self.assertEqual(sessionctl.return_key_state(FakeTmux(set(), {}))["state"], "free")
        taken = lambda *a, **k: CmdResult(0, "bind-key -T root F12 send-prefix", "")  # noqa: E731
        self.assertEqual(sessionctl.return_key_state(taken)["state"], "taken")
        ours = lambda *a, **k: CmdResult(0, "bind-key -T root F12 switch-client -t =icn-control", "")  # noqa: E731
        self.assertEqual(sessionctl.return_key_state(ours)["state"], "ours")


# --------------------------------------------------------------------------- picker model
class Picker(unittest.TestCase):
    def test_sort_filter_state_and_labels(self):
        now = time.time()
        sessions = [
            {"kind": "launcher-record", "name": "old-lane", "status": REGISTERED_NOT_RUNNING, "last_activity": now - 50,
             "program": "nycn", "repo": "nycn", "worktree": "/w/nycn/old-lane", "state_label": "STOPPED"},
            {"kind": "agent", "name": "pr2906-review", "status": LIVE, "last_activity": now - 3000, "program": "icn-core",
             "repo": "icn", "cwd": "/s/pr2906", "state_label": "IDLE AT PROMPT", "pid": 4242, "tmux": "icn-r",
             "pr": {"repo": "icn", "number": 2906, "how": "inferred from directory name", "title": "clearing"}},
            {"kind": "agent", "name": "compute", "status": LIVE, "last_activity": now - 10, "program": "icn-core",
             "repo": "icn", "cwd": "/s/compute", "state_label": "WORKING", "pid": 4343, "tmux": "icn-c"},
            {"kind": "tmux", "name": "shell", "status": "IDLE"},
            {"kind": "control-center", "name": "icn-control", "status": LIVE},
        ]
        acts = lambda s: [{"id": "join", "enabled": s["kind"] == "agent"},  # noqa: E731
                          {"id": "resume", "enabled": s["kind"] == "launcher-record"}]
        rows = picker.build_rows(sessions, {"icn-core": "ICN Core", "nycn": "NYCN"}, acts)
        self.assertEqual([r["name"] for r in rows], ["compute", "pr2906-review", "old-lane"], "running first, then recent")
        self.assertEqual([r["state"] for r in rows], ["RUNNING · busy", "RUNNING · waiting", "STOPPED · resumable"])
        self.assertEqual([r["can"] for r in rows], ["join", "join", "resume"])
        self.assertEqual(rows[1]["program"], "ICN Core")
        self.assertTrue(all("4242" not in r["name"] and "4343" not in r["name"] for r in rows), "no pid as the label")
        self.assertEqual([r["name"] for r in picker.filter_rows(rows, "2906")], ["pr2906-review"])
        self.assertEqual([r["name"] for r in picker.filter_rows(rows, "nycn resumable")], ["old-lane"])
        self.assertEqual(picker.filter_rows(rows, "  "), rows)
        self.assertEqual(picker.default_action(rows[2]), "resume")


# --------------------------------------------------------------------------- end to end in a pty
@unittest.skipIf(shutil.which("tmux") is None, "tmux not installed")
class EndToEnd(unittest.TestCase):
    """Real curses UI, real tmux client on a pty, isolated tmux server, fake agents and launcher."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="icnctl-e2e-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.home = os.path.join(self.tmp, "home")
        self.claude_home = os.path.join(self.home, ".claude")
        self.dev = os.path.join(self.tmp, "icn-dev")
        self.state = os.path.join(self.tmp, "icn-session")
        for d in (self.home, os.path.join(self.claude_home, "sessions"), os.path.join(self.dev, "repos"), self.state):
            os.makedirs(d, exist_ok=True)
        self.bin = os.path.join(self.tmp, "bin")
        os.makedirs(self.bin)
        shutil.copy("/bin/sleep", os.path.join(self.bin, "claude"))   # a process whose comm is 'claude'
        self.calls = os.path.join(self.tmp, "launcher-calls.txt")
        launcher = os.path.join(self.bin, "icn-session")
        with open(launcher, "w") as f:   # fake host launcher: records argv, creates the session it was asked for
            f.write("#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$CALLS\"\n"
                    "if [ \"$1\" = check-path ]; then exit 0; fi\n"
                    "name=''; while [ $# -gt 0 ]; do [ \"$1\" = --name ] && name=\"$2\"; shift; done\n"
                    "tmux new-session -d -s \"$name\" \"$FAKEBIN/claude 3600\"\n")
        os.chmod(launcher, 0o755)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("TMUX")}
        self.env.update(TMUX_TMPDIR=self.tmp, HOME=self.home, ICN_CONTROL_CLAUDE_HOME=self.claude_home,
                        ICN_DEV_ROOT=self.dev, ICN_SESSION_STATE_DIR=self.state, ICN_SESSION_BIN=launcher,
                        ICN_CONTROL_STATE_DIR=os.path.join(self.tmp, "ctl"), CALLS=self.calls, FAKEBIN=self.bin,
                        TERM="xterm-256color", PATH=self.bin + os.pathsep + os.environ.get("PATH", ""))
        self.addCleanup(lambda: subprocess.run(["tmux", "kill-server"], env=self.env, capture_output=True))
        self.pids = {}
        for name in ("alpha", "beta"):
            self.start_agent(name)
        # A stopped conversation with an exact record + transcript, in a worktree under the fake root.
        self.wt = os.path.join(self.dev, "worktrees", "icn", "gamma-lane")
        os.makedirs(self.wt)
        tdir = os.path.join(self.claude_home, "projects", self.wt.replace("/", "-"))
        os.makedirs(tdir)
        self.uid = str(uuidlib.uuid4())
        open(os.path.join(tdir, f"{self.uid}.jsonl"), "w").close()
        with open(os.path.join(self.state, "icn-gamma-lane.json"), "w") as f:
            json.dump({"name": "icn-gamma-lane", "purpose": "gamma-lane", "repo": "icn", "worktree": self.wt,
                       "claude_session_id": self.uid, "launch_mode": "tmux", "updated_at": "2026-11-18T00:00:00Z"}, f)
        # The dashboard runs in its own tmux session, with a real client attached on a pty.
        cmd = f"cd {ROOT} && exec python3 -I -c 'import sys; sys.path.insert(0, \"{ROOT}\"); from icn_control.__main__ import main; sys.exit(main([]))'"
        self.tm("new-session", "-d", "-s", "icn-control", "-x", "180", "-y", "50", cmd)
        self.tm("set-option", "-t", "=icn-control:", "@icn_control", "1")
        self.client_pid, self.client_fd = pty.fork()
        if self.client_pid == 0:
            os.execvpe("tmux", ["tmux", "attach-session", "-t", "=icn-control"], self.env)
        self.addCleanup(self._close_client)
        self.wait(lambda: "ICN DEVELOPMENT CONTROL CENTER" in self.screen(), "dashboard up")

    def _close_client(self):
        try:
            os.kill(self.client_pid, 9)
            os.waitpid(self.client_pid, 0)
        except OSError:
            # the test client may already have exited with its tmux server; nothing to clean up
            pass

    def start_agent(self, name):
        cwd = os.path.join(self.tmp, f"{name}-dir")   # separate directories: separate pieces of work
        os.makedirs(cwd, exist_ok=True)
        self.tm("new-session", "-d", "-s", f"agent-{name}", "-c", cwd, "-x", "120", "-y", "30", f"{self.bin}/claude 3600")
        pid = int(self.tm("list-panes", "-t", f"=agent-{name}", "-F", "#{pane_pid}").stdout.split()[0])
        start = procs.proc_start_ticks(pid)
        with open(os.path.join(self.claude_home, "sessions", f"{pid}.json"), "w") as f:
            json.dump({"pid": pid, "procStart": start, "sessionId": str(uuidlib.uuid4()), "cwd": cwd,
                       "name": f"{name}-task-session", "status": "idle", "statusUpdatedAt": time.time() * 1000,
                       "tmux": f"agent-{name}:@1.%1"}, f)
        self.pids[name] = (pid, start)

    def tm(self, *args):
        return subprocess.run(["tmux", *args], env=self.env, capture_output=True, text=True, timeout=20)

    def pump(self):
        while select.select([self.client_fd], [], [], 0)[0]:
            try:
                if not os.read(self.client_fd, 65536):
                    break
            except OSError:
                break

    def wait(self, cond, what, timeout=40.0):
        end = time.time() + timeout
        while time.time() < end:
            self.pump()
            if cond():
                return
            time.sleep(0.3)
        self.fail(f"timed out waiting for {what}; screen was:\n{self.screen()}")

    def screen(self, target="icn-control"):
        return self.tm("capture-pane", "-p", "-t", f"={target}:").stdout

    def client_session(self):
        out = self.tm("list-clients", "-F", "#{session_name}").stdout.split()
        return out[0] if out else None

    def keys(self, *k):
        self.tm("send-keys", "-t", "=icn-control:", *k)
        time.sleep(0.5)
        self.pump()

    def read_calls(self):
        if not os.path.exists(self.calls):
            return ""
        with open(self.calls) as f:
            return f.read()

    def alive(self, name):
        return procs.same_process(*self.pids[name])

    def test_switch_return_switch_and_resume(self):
        # 1. picker from anywhere, filter by a recognisable name, join
        self.wait(lambda: "WORK" in self.screen(), "Work is the default screen")
        self.keys("5")   # start from another screen: g must work everywhere
        self.keys("g")
        self.wait(lambda: "SWITCH TO A CLAUDE SESSION" in self.screen(), "picker")
        self.assertIn("alpha-task-session", self.screen())
        self.assertIn("gamma-lane", self.screen(), "the stopped conversation is listed")
        self.keys("-l", "beta")
        self.wait(lambda: "beta-task-session" in self.screen() and "alpha-task-session" not in self.screen(), "filter")
        self.keys("Enter")
        self.wait(lambda: "Join session" in self.screen(), "action menu")
        self.keys("Enter")
        self.wait(lambda: self.client_session() == "agent-beta", "client moved to agent-beta")
        self.assertTrue(self.alive("beta"))
        self.assertEqual(self.tm("list-clients", "-t", "=icn-control").stdout.strip(), "", "only our client moved")
        # 2. return with prefix + L (default prefix in the isolated server is C-b); the agent keeps running
        os.write(self.client_fd, b"\x02L")
        self.wait(lambda: self.client_session() == "icn-control", "back on the dashboard")
        self.assertTrue(self.alive("beta"), "returning must not stop the agent")
        self.wait(lambda: "Last joined: beta-task-session" in self.screen(), "dashboard remembers the join")
        # 3. pick a different session and enter it
        self.keys("g")
        self.wait(lambda: "SWITCH TO A CLAUDE SESSION" in self.screen(), "picker again")
        self.keys("-l", "alpha")
        self.wait(lambda: "beta-task-session" not in self.screen(), "filter alpha")
        self.keys("Enter")
        self.wait(lambda: "Join session" in self.screen(), "action menu alpha")
        self.keys("Enter")
        self.wait(lambda: self.client_session() == "agent-alpha", "client moved to agent-alpha")
        os.write(self.client_fd, b"\x02L")
        self.wait(lambda: self.client_session() == "icn-control", "back again")
        self.assertTrue(self.alive("alpha") and self.alive("beta"))
        # 4. resume the stopped conversation: cancel first (nothing may run), then confirm
        self.keys("g")
        self.wait(lambda: "SWITCH TO A CLAUDE SESSION" in self.screen(), "picker for resume")
        self.keys("-l", "gamma")
        self.wait(lambda: "STOPPED" in self.screen(), "stopped row")
        self.keys("Enter")
        self.wait(lambda: "Resume saved conversation" in self.screen(), "resume offered")
        self.keys("Enter")
        self.wait(lambda: self.uid in self.screen() and "Press y" in self.screen(), "confirmation shows exact id")
        self.keys("n")
        self.wait(lambda: "Nothing was started" in self.screen() or "SWITCH TO" in self.screen(), "cancelled")
        launched = self.read_calls()
        self.assertNotIn(" open ", f" {launched} ", "cancel must not start anything")
        self.assertEqual(self.tm("has-session", "-t", "=icn-gamma-lane").returncode, 1)
        self.keys("Escape")
        self.keys("g")
        self.wait(lambda: "SWITCH TO A CLAUDE SESSION" in self.screen(), "picker for resume 2")
        self.keys("-l", "gamma")
        self.wait(lambda: "STOPPED" in self.screen(), "stopped row 2")
        self.keys("Enter")
        self.wait(lambda: "Resume saved conversation" in self.screen(), "resume offered 2")
        self.keys("Enter")
        self.wait(lambda: "Press y" in self.screen(), "confirm 2")
        self.keys("y")
        self.wait(lambda: self.client_session() == "icn-gamma-lane", "joined the resumed session")
        calls = [line for line in self.read_calls().splitlines() if line.startswith("open ")]
        self.assertEqual(calls, [f"open gamma-lane --repo icn --worktree {self.wt} --name icn-gamma-lane --no-create --no-attach"])
        os.write(self.client_fd, b"\x02L")
        self.wait(lambda: self.client_session() == "icn-control", "back after resume")
        # 5. the now-running conversation cannot be resumed a second time
        self.keys("g")
        self.wait(lambda: "SWITCH TO A CLAUDE SESSION" in self.screen(), "picker after resume")
        self.keys("-l", "gamma")
        self.wait(lambda: "RUNNING" in self.screen() and "It is running now; join it" in self.screen(),
                  "the resumed conversation shows as running, and a second resume is refused")
        self.keys("Escape")

    def test_stale_target_is_refused_and_resize_survives(self):
        self.keys("4")
        self.wait(lambda: "alpha-task-session" in self.screen(), "sessions screen")
        self.tm("kill-session", "-t", "=agent-alpha")   # the target disappears after it was displayed
        self.keys("/")
        self.keys("-l", "alpha-task")
        self.keys("Enter")
        self.keys("a")
        # Either the stale row is refused at join time, or a refresh already removed it. Never a switch.
        self.wait(lambda: "no longer" in self.screen() or "Nothing matches" in self.screen(), "stale target refused")
        self.assertEqual(self.client_session(), "icn-control")
        self.tm("resize-window", "-t", "=icn-control:", "-x", "50", "-y", "10")
        self.wait(lambda: "Terminal too small" in self.screen(), "small-terminal message")
        self.tm("resize-window", "-t", "=icn-control:", "-x", "170", "-y", "45")
        self.wait(lambda: "ICN DEVELOPMENT CONTROL CENTER" in self.screen(), "redraw after resize")
        self.keys("q")
        self.wait(lambda: "Quit the control center?" in self.screen(), "quit asks inside tmux")
        self.keys("n")
        self.wait(lambda: "ICN DEVELOPMENT CONTROL CENTER" in self.screen(), "still running after declining quit")


if __name__ == "__main__":
    unittest.main()
