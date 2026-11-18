"""Session actions: join a live agent's tmux session, resume a recorded conversation, open
Claude Code in an existing worktree.

Every action is planned from evidence and re-verified immediately before it runs:
- JOIN switches *this* tmux client to an existing session. It never starts a process, never
  sends keystrokes to the agent and never detaches anyone (no `-d`).
- RESUME and OPEN delegate to the host's approved launcher, `icn-session open ... --no-create
  --no-attach`, then join the session it created. They require an exact recorded conversation id
  (never a guess), refuse when that conversation or worktree already has a live agent, and never
  create a branch or worktree.
All commands are argv lists (no shell); every value placed in them is validated first.
"""

from __future__ import annotations

import json
import os
import re
import shutil
from typing import Callable, Optional

from .procs import ancestors, same_process
from .util import Paths, run

TOKEN_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,62}$")          # icn-session's own TOKEN_RE
UUID_RE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
TMUX_NAME_RE = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_+@%-]{0,99}$")  # no ':' '.' or whitespace
READONLY_NAME_RE = re.compile(r"^(review-|review_|main-readonly|build-main-|final-main-|mcp-host|ops-mcp-live|obs-source-)")


def valid_token(v) -> bool:
    return isinstance(v, str) and bool(TOKEN_RE.match(v))


def valid_uuid(v) -> bool:
    return isinstance(v, str) and bool(UUID_RE.match(v))


def valid_tmux_name(v) -> bool:
    return isinstance(v, str) and bool(TMUX_NAME_RE.match(v))


def project_slugs(cwd: str) -> list[str]:
    """Claude Code's transcript directory name(s) for a working directory.

    Observed on this host: '/' becomes '-'. How '_' and '.' are mapped is not observable here
    (no such directory exists), so both deterministic candidates for THIS directory are accepted.
    Other directories are never searched as a fallback.
    """
    return sorted({re.sub(r"[^A-Za-z0-9-]", "-", cwd), cwd.replace("/", "-").replace(".", "-")})


def transcript_for(paths: Paths, cwd: str, uuid: str) -> Optional[str]:
    """The transcript of conversation `uuid` *for this exact directory*, or None."""
    if not (cwd and valid_uuid(uuid)):
        return None
    for slug in project_slugs(cwd):
        p = os.path.join(paths.claude_home, "projects", slug, f"{uuid}.jsonl")
        if os.path.isfile(p):
            return p
    return None


def transcript_elsewhere(paths: Paths, uuid: str) -> Optional[str]:
    if not valid_uuid(uuid):
        return None
    root = os.path.join(paths.claude_home, "projects")
    try:
        for d in os.listdir(root):
            p = os.path.join(root, d, f"{uuid}.jsonl")
            if os.path.isfile(p):
                return p
    except OSError:
        return None
    return None


def icn_session_bin() -> Optional[str]:
    """The host's approved launcher. Resolved, never assumed."""
    for c in (os.environ.get("ICN_SESSION_BIN"), shutil.which("icn-session"),
              os.path.join(os.path.expanduser("~"), "bin", "icn-session")):
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    return None


def under(path: Optional[str], root: Optional[str]) -> bool:
    if not path or not root:
        return False
    path, root = os.path.normpath(path), os.path.normpath(root)
    return path == root or path.startswith(root + os.sep)


# --------------------------------------------------------------------------- action model
def action(aid: str, label: str, enabled: bool, reason: str = "", **kw) -> dict:
    d = {"id": aid, "label": label, "enabled": enabled, "reason": reason}
    d.update(kw)
    return d


def live_conversations(snap_sessions: list[dict]) -> set:
    return {s.get("conversation") for s in snap_sessions if s.get("kind") == "agent" and s.get("conversation")}


def live_agents_in(path: Optional[str], sessions: list[dict]) -> list[dict]:
    """Live agent processes whose working directory is inside `path`. A shell's cwd is not counted."""
    return [s for s in sessions if s.get("kind") == "agent" and s.get("pid") and under(s.get("cwd"), path)]


def session_actions(view: dict, sessions: list[dict], paths: Paths, prefix: str = "the tmux prefix") -> list[dict]:
    """All actions for one session row, enabled or with the reason they are not available."""
    acts = []
    kind = view.get("kind")
    tmux = view.get("tmux")
    # ---- JOIN
    if kind == "control-center":
        acts.append(action("join", "Join session", False, "This is the control center itself."))
    elif kind == "agent" and tmux and valid_tmux_name(tmux):
        acts.append(action("join", "Join session", True,
                           f"Switches this window to tmux '{tmux}'. The agent keeps running; nothing is typed for you."
                           f" Return: {prefix}, then capital L.", target=tmux))
    elif kind == "agent":
        acts.append(action("join", "Join session", False,
                           "The agent runs in a terminal outside tmux, so it cannot be joined from here. Use that terminal."))
    elif kind == "tmux" and tmux and valid_tmux_name(tmux):
        acts.append(action("join", "Join tmux session (no agent running in it)", True,
                           f"Shell only. Return: {prefix}, then capital L.", target=tmux))
    elif kind == "service":
        acts.append(action("join", "Join session", False, "A service process (app-server/proxy), not an interactive agent."))
    else:
        acts.append(action("join", "Join session", False, "No live agent process was observed for this entry."))
    # ---- RESUME
    acts.append(resume_action(view, sessions, paths))
    # ---- INSPECT is always available
    acts.append(action("details", "Show details", True, "Read-only."))
    if view.get("handoffs") or view.get("evidence"):
        acts.append(action("open_record", "Open handoff / record", True, "Read-only reader."))
    return acts


def resume_action(view: dict, sessions: list[dict], paths: Paths) -> dict:
    """RESUME is offered only for an icn-session launcher record with an exact, verifiable conversation."""
    label = "Resume saved conversation"
    kind = view.get("kind")
    if kind in ("agent", "service", "control-center") or view.get("status") == "VERIFIED LIVE":
        return action("resume", label, False, "It is running now; join it instead of starting a second copy.")
    rec = view.get("record")
    uuid = view.get("conversation")
    if kind == "registry":
        manual = ""
        if valid_uuid(uuid) and view.get("cwd") and transcript_for(paths, view["cwd"], uuid):
            manual = (f" Manual route (check nothing else runs it first): cd {view['cwd']} && claude --resume {uuid}")
        return action("resume", label, False, "Only an icn-ops registry entry exists; icn-session can resume only conversations"
                      " it recorded itself, so nothing is started from here." + manual)
    if kind == "tmux":
        return action("resume", label, False, "A tmux session without an agent; there is no recorded conversation to resume.")
    if not rec:
        return action("resume", label, False, "No launcher record names this conversation; the exact conversation is unknown.")
    plan = plan_resume(rec, sessions, paths)
    if not plan["ok"]:
        return action("resume", label, False, plan["reason"])
    return action("resume", label, True, plan["summary"], argv=plan["argv"], target=plan["tmux_name"],
                  confirm=plan["confirm"])


def plan_resume(rec: dict, sessions: list[dict], paths: Paths) -> dict:
    """Plan `icn-session open` for an exact recorded conversation, or explain why not."""
    def no(reason):
        return {"ok": False, "reason": reason}
    name, purpose, repo = rec.get("name"), rec.get("purpose"), rec.get("repo")
    wt, uuid = rec.get("worktree"), rec.get("claude_session_id")
    if not valid_uuid(uuid):
        return no("The record has no well-formed conversation id; refusing to guess one.")
    if not (valid_token(name) and valid_token(purpose) and valid_token(repo)):
        return no("The record's name/purpose/repo are not valid icn-session tokens.")
    if not (isinstance(wt, str) and os.path.isabs(wt) and "\n" not in wt and os.path.isdir(wt)):
        return no(f"The recorded worktree is missing or invalid: {wt!r}.")
    if not under(wt, paths.worktrees):
        return no(f"The recorded worktree {wt} is outside {paths.worktrees}.")
    if not transcript_for(paths, wt, uuid):
        other = transcript_elsewhere(paths, uuid)
        return no(f"No transcript for conversation {uuid} in {wt}"
                  + (f" (it exists for a different directory: {os.path.dirname(other)}); the association does not match."
                     if other else "; it cannot be resumed reliably."))
    if uuid in live_conversations(sessions):
        return no(f"Conversation {uuid} is running right now; join it instead.")
    busy = live_agents_in(wt, sessions)
    if busy:
        return no(f"Another agent is live in this worktree ({busy[0]['name']}); a second agent there is refused.")
    if any(s.get("tmux") == name for s in sessions if s.get("kind") in ("tmux", "agent", "control-center")):
        return no(f"A tmux session named '{name}' already exists; join it instead.")
    if same_process(rec.get("fg_pid"), rec.get("fg_start"), paths.proc):
        return no("The recorded foreground process is still running.")
    exe = icn_session_bin()
    if not exe:
        return no("icn-session is not installed or not executable; nothing can be launched safely.")
    argv = [exe, "open", purpose, "--repo", repo, "--worktree", wt, "--name", name, "--no-create", "--no-attach"]
    confirm = [f"Resume saved conversation {uuid}",
               f"  worktree:   {wt}",
               f"  tmux name:  {name}  (launcher record {rec.get('file', '?')})",
               f"  command:    {' '.join(argv)}",
               "This starts Claude Code with --resume for that exact conversation through icn-session,",
               "then switches this window to it. No branch or worktree is created; no file is edited."]
    return {"ok": True, "argv": argv, "tmux_name": name, "confirm": confirm,
            "summary": f"Starts conversation {uuid[:8]} again in {os.path.basename(wt)} via icn-session (asks first)."}


def plan_open_in_worktree(wt: dict, sessions: list[dict], records: list[dict], paths: Paths,
                          check_path: Optional[Callable[[str], int]] = None) -> dict:
    """Plan 'Open Claude Code in this worktree' via icn-session, or explain why not."""
    def no(reason):
        return {"ok": False, "reason": reason}
    path, repo, name = wt.get("path"), wt.get("repo"), wt.get("name") or os.path.basename(wt.get("path") or "")
    st = wt.get("status") or {}
    if not (isinstance(path, str) and os.path.isabs(path) and "\n" not in path):
        return no("Worktree path is invalid.")
    if st.get("exists") is False or not os.path.isdir(path):
        return no("The worktree directory does not exist.")
    if not under(path, paths.worktrees):
        return no(f"Only worktrees under {paths.worktrees} are opened from here.")
    if not (valid_token(repo) and valid_token(name)):
        return no(f"'{name}' is not a valid icn-session purpose (letters, digits, '-' and '_' only).")
    if READONLY_NAME_RE.match(name) or (wt.get("branch") or "").startswith("review/"):
        return no("This looks like a review / read-only / serving checkout, not a development worktree.")
    if wt.get("detached") or not wt.get("branch"):
        return no("Detached HEAD: an inspection checkout, not a development branch.")
    if wt.get("prunable") or wt.get("locked"):
        return no("git marks this worktree locked or prunable.")
    if st.get("conflicted"):
        return no("The worktree has unresolved merge conflicts.")
    busy = live_agents_in(path, sessions)
    if busy:
        return no(f"An agent is already live here ('{busy[0]['name']}'); join it instead.")
    rc = (check_path or _check_path)(path)
    if rc == 3:
        return no("icn-session reports this as a protected path.")
    if rc not in (0,):
        return no(f"icn-session check-path could not confirm this path (exit {rc}).")
    tmux_name = f"{repo}-{name}"
    if not valid_token(tmux_name):
        return no(f"Session name '{tmux_name}' would be too long for icn-session.")
    if any(s.get("tmux") == tmux_name for s in sessions if s.get("kind") in ("tmux", "agent", "control-center")):
        return no(f"A tmux session '{tmux_name}' already exists; join it instead.")
    rec = next((r for r in records if r.get("name") == tmux_name), None)
    warn = []
    if rec and rec.get("worktree") not in (None, path):
        return no(f"An icn-session record '{tmux_name}' already points at {rec.get('worktree')}.")
    if rec and valid_uuid(rec.get("claude_session_id")):
        if rec["claude_session_id"] in live_conversations(sessions):
            return no(f"Its recorded conversation {rec['claude_session_id']} is running; join it instead.")
        warn.append(f"A launcher record exists: icn-session will RESUME its recorded conversation {rec['claude_session_id']}.")
    reg_live = [s for s in sessions if s.get("kind") == "registry" and s.get("worktree") == path and s.get("status") != "STALE"]
    if reg_live:
        warn.append(f"Note: a recent registry entry ({reg_live[0]['name']}) names this worktree but no process was found.")
    if wt.get("locks"):
        warn.append("Open file locks owned by this lane: " + ", ".join(f for lk in wt["locks"] for f in lk["files"][:3]))
    exe = icn_session_bin()
    if not exe:
        return no("icn-session is not installed or not executable; nothing can be launched safely.")
    argv = [exe, "open", name, "--repo", repo, "--worktree", path, "--name", tmux_name, "--no-create", "--no-attach"]
    confirm = ["Open Claude Code in this worktree",
               f"  worktree:   {path}",
               f"  branch:     {wt.get('branch')}   head {str(wt.get('head') or '')[:9]}",
               f"  tmux name:  {tmux_name}",
               f"  command:    {' '.join(argv)}"] + [f"  {w}" for w in warn] + [
               "This starts one Claude Code session through icn-session in the worktree above and",
               "switches this window to it. No branch or worktree is created; no file is edited."]
    return {"ok": True, "argv": argv, "tmux_name": tmux_name, "confirm": confirm, "warnings": warn}


def _check_path(path: str) -> int:
    exe = icn_session_bin()
    if not exe:
        return 127
    return run([exe, "check-path", path], timeout=20).rc


# --------------------------------------------------------------------------- tmux plumbing
def tmux(*args: str, timeout: float = 10.0):
    return run(["tmux", *args], timeout=timeout)


def verify_join(view: dict, paths: Paths, tm=tmux) -> tuple[bool, str]:
    """Re-check, immediately before switching, that the target is still the agent we showed."""
    name = view.get("tmux")
    if not valid_tmux_name(name):
        return False, "No valid tmux target."
    if tm("has-session", "-t", f"={name}").rc != 0:
        return False, f"tmux session '{name}' no longer exists (stale target). Refresh with r."
    if view.get("kind") != "agent":
        return True, ""
    pid, start = view.get("pid"), view.get("pid_start")
    if not same_process(pid, start, paths.proc):
        return False, f"The agent process {pid} has exited or its pid was reused. Refresh with r."
    r = tm("list-panes", "-s", "-t", f"={name}", "-F", "#{pane_pid}")
    pane_pids = {int(x) for x in r.out.split() if x.isdigit()}
    by_pid = {}
    for p in [pid] + _ancestry(int(pid), paths):
        by_pid[p] = True
    if not pane_pids.intersection(by_pid):
        return False, f"The agent (pid {pid}) is no longer inside tmux session '{name}'."
    conv = view.get("conversation")
    if conv:
        f = os.path.join(paths.claude_home, "sessions", f"{pid}.json")
        try:
            with open(f) as fh:
                if json.load(fh).get("sessionId") != conv:
                    return False, "The process now reports a different conversation. Refresh with r."
        except (OSError, ValueError):
            return False, "The agent's session file is gone; it may be exiting. Refresh with r."
    return True, ""


def _ancestry(pid: int, paths: Paths) -> list[int]:
    from .procs import scan_processes
    procs = {p["pid"]: p for p in scan_processes(paths.proc)}
    return ancestors(pid, procs)


def own_client(tm=tmux) -> Optional[str]:
    """The tmux client currently showing this dashboard (most recently active client of our session)."""
    pane = os.environ.get("TMUX_PANE")
    if not os.environ.get("TMUX") or not pane:
        return None
    r = tm("display-message", "-p", "-t", pane, "#{session_name}")
    sess = r.out.strip()
    if r.rc != 0 or not sess:
        return None
    r = tm("list-clients", "-t", f"={sess}", "-F", "#{client_activity} #{client_tty}")
    best = None
    for line in r.out.splitlines():
        parts = line.split(" ", 1)
        if len(parts) == 2 and parts[0].isdigit() and (best is None or int(parts[0]) > best[0]):
            best = (int(parts[0]), parts[1])
    return best[1] if best else None


def switch_argv(target: str, client: Optional[str]) -> list[str]:
    """Switch only THIS client. Other clients of either session are untouched (no -d anywhere)."""
    argv = ["tmux", "switch-client"]
    if client:
        argv += ["-c", client]
    return argv + ["-t", f"={target}"]


def attach_argv(target: str) -> list[str]:
    return ["tmux", "attach-session", "-t", f"={target}"]


# --------------------------------------------------------------------------- optional return key
RETURN_KEY = "F12"
RETURN_SESSION = "icn-control"


def return_key_state(tm=tmux) -> dict:
    """Is F12 free in tmux's root table, already ours, or taken by something else?"""
    r = tm("list-keys", "-T", "root", RETURN_KEY)
    line = r.out.strip()
    if r.rc != 0 or not line:
        return {"state": "free", "binding": None}
    if f"switch-client -t ={RETURN_SESSION}" in line:
        return {"state": "ours", "binding": line}
    return {"state": "taken", "binding": line}
