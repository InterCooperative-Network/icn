"""Curses user interface for the ICN Development Control Center.

Design rules:
- Every screen says where its data came from and how old it is.
- Colors follow status; UNKNOWN is never green.
- The only action that touches anything outside this program is an explicit, confirmed
  tmux attach/switch to an EXISTING session. Nothing is started, stopped or deleted.
"""

from __future__ import annotations

import curses
import json
import os
import subprocess
import textwrap
import time
from typing import Optional

from . import __version__
from . import snapshot as snapmod
from .picker import build_rows, default_action, filter_rows
from .reconcile import build_views, latest_review_generation, session_views
from .sessionctl import (attach_argv, own_client, plan_open_in_worktree, plan_resume, return_key_state,
                         session_actions, switch_argv, verify_join)
from .records import summarize_handoff
from .snapshot import Collector
from .util import (AWAITING_MAINTAINER, BLOCKED, COMPLETED, IDLE, IN_PROGRESS, LIVE, REGISTERED_NOT_RUNNING,
                   STALE, UNKNOWN, WARNING, Paths, ago, clean, fmt_ts, redact, run, short)

SCREENS = ["Overview", "Lanes", "Agents & Sessions", "Worktrees", "Pull Requests", "Handoffs", "System Health"]
VIEWABLE_EXT = (".md", ".txt", ".log", ".json", ".yaml", ".yml", ".patch", ".diff", ".tsv", ".jsonl")
MAX_VIEW_BYTES = 2_000_000

HELP = """\
ICN DEVELOPMENT CONTROL CENTER — HELP

Moving around
  1-7 or Tab / Shift-Tab     switch screen (Overview, Lanes, Agents, Worktrees, PRs, Handoffs, Health)
  Left / Right               switch screen
  Up / Down, PgUp / PgDn     move the selection (or scroll a page)
  Home / End                 first / last item
  Enter                      open full details for the selected item
  Esc or q (in a viewer)     go back

Switching between Claude sessions
  g                          SESSION SWITCHER (from any screen): type to filter, Up/Down, Enter to choose, Esc to close
  Enter (screen 3)           actions for the selected session: Join / Resume saved conversation / Details
  a  (screen 3)              join the selected session's EXISTING tmux session right away (re-verified first)
  c  (screens 2 and 4)       open Claude Code in the selected existing worktree (shows the plan, asks first)
  Join never starts a process, never types anything for you and never disconnects anyone else.
  Resume and Open run the host launcher (icn-session) only after you confirm, and refuse when the
  conversation or worktree already has a live agent, or when the exact conversation is unknown.

Other actions
  r                          refresh everything now (otherwise: local every 20s, git and GitHub every 5 min)
  o                          open the selected item's handoff / record in the built-in reader
  /                          filter the current list (empty filter clears)
  k                          Worktrees: toggle 'only worktrees that must be kept'
  s                          show where every piece of data came from and when it was observed
  ?                          this help
  q                          quit (asks first inside tmux; PREFIX then d leaves it running instead)

Coming back from a Claude session (PREFIX is read from your tmux config)
  PREFIX then L (capital)    back to this control center; Claude keeps running (the usual way)
  PREFIX then s              tmux's own session list, if you lose your way
  PREFIX then d              (only when this dashboard runs with --here) detach; Claude keeps running

What the status words mean
  VERIFIED LIVE              a process was observed running just now (pid and start time checked)
  IN PROGRESS                work is open and moving (draft PR, lifecycle IMPLEMENTING/REVIEWING/...)
  IDLE                       observed, but not active (agent waiting at its prompt, empty tmux shell)
  AWAITING MAINTAINER        a decision only you can make (frozen PR, review findings to disposition)
  BLOCKED                    something specific is failing (named in the detail)
  REGISTERED, NOT RUNNING    a record says a session exists, but no process was found
  STALE                      the data is old, or a record contradicts what is running now
  WARNING                    host health past a threshold
  UNKNOWN                    the source could not be read. Unknown is never shown as OK.

Nothing here starts, stops, deletes or merges anything. 'Cleanup candidate' is a hint to verify,
never an instruction. Records shown here are read from their owners; this program is not one.
"""


def may_open(path: str, icn_dev: str) -> tuple[bool, str]:
    """Only text records inside ~/icn-dev (after resolving symlinks) and never key material."""
    real = os.path.realpath(path)
    root = os.path.realpath(icn_dev)
    if not real.startswith(root + os.sep):
        return False, f"Refusing to open {real}: outside {root}"
    if not real.endswith(VIEWABLE_EXT):
        return False, "Only text records (.md, .txt, .log, .json, ...) are opened here"
    if "/private/" in real[len(root):] and not real.endswith(".md"):
        return False, "Only Markdown notes are opened from the private area"
    return True, ""


def tmux_prefix() -> str:
    """The configured tmux prefix as a human label (e.g. 'Ctrl-a'). Never assume Ctrl-b."""
    try:
        r = subprocess.run(["tmux", "show-options", "-gv", "prefix"], capture_output=True, text=True, timeout=3)
        key = r.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        key = ""
    if not key:
        return "the tmux prefix key"
    return "Ctrl-" + key[2:] if key.startswith("C-") else key


class UI:
    def __init__(self, stdscr, paths: Paths, catalog: dict):
        self.scr = stdscr
        self.paths, self.catalog = paths, catalog
        self.collector = Collector(paths, catalog)
        self.views: dict = {}
        self.snap: dict = {}
        self.seen_version = -1
        self.screen = 0
        self.sel = [0] * len(SCREENS)
        self.top = [0] * len(SCREENS)
        self.scroll = [0] * len(SCREENS)
        self.filters = [""] * len(SCREENS)
        self.sel_key = [None] * len(SCREENS)
        self.in_tmux = bool(os.environ.get("TMUX"))
        self.last_join = None
        self.keep_only = False
        self.message = ""
        self.message_ts = 0.0
        self.started = time.time()
        self.prefix = tmux_prefix()
        self.back_hint = f"{self.prefix} then L"
        try:
            if return_key_state()["state"] == "ours":
                self.back_hint = f"F12 (or {self.prefix} then L)"
        except Exception:
            # best-effort: the hint falls back to the prefix key, which always works
            pass
        self._load_prefs()

    # ------------------------------------------------------------------ prefs (non-authoritative UI state)
    def _prefs_path(self) -> str:
        return os.path.join(self.paths.control_state, "prefs.json")

    def _load_prefs(self) -> None:
        try:
            with open(self._prefs_path()) as f:
                p = json.load(f)
            self.screen = int(p.get("screen", 0)) % len(SCREENS)
            self.keep_only = bool(p.get("keep_only", False))
        except (OSError, ValueError, TypeError):
            # best-effort: UI preferences and terminal capabilities are optional
            pass

    def _save_prefs(self) -> None:
        try:
            os.makedirs(self.paths.control_state, mode=0o700, exist_ok=True)
            tmp = self._prefs_path() + ".tmp"
            with open(tmp, "w") as f:
                json.dump({"screen": self.screen, "keep_only": self.keep_only}, f)
            os.chmod(tmp, 0o600)
            os.replace(tmp, self._prefs_path())
        except OSError:
            # best-effort: UI preferences and terminal capabilities are optional
            pass

    # ------------------------------------------------------------------ colors
    def init_colors(self) -> None:
        self.color = {}
        if not curses.has_colors():
            return
        curses.start_color()
        try:
            curses.use_default_colors()
            bg = -1
        except curses.error:
            bg = curses.COLOR_BLACK
        pairs = {
            LIVE: curses.COLOR_GREEN, IN_PROGRESS: curses.COLOR_CYAN, IDLE: curses.COLOR_BLUE,
            AWAITING_MAINTAINER: curses.COLOR_MAGENTA, BLOCKED: curses.COLOR_RED, WARNING: curses.COLOR_YELLOW,
            STALE: curses.COLOR_YELLOW, REGISTERED_NOT_RUNNING: curses.COLOR_YELLOW, COMPLETED: curses.COLOR_BLUE,
            UNKNOWN: curses.COLOR_WHITE, "title": curses.COLOR_CYAN, "dim": curses.COLOR_WHITE,
        }
        for i, (k, c) in enumerate(pairs.items(), start=1):
            curses.init_pair(i, c, bg)
            self.color[k] = curses.color_pair(i)
        curses.init_pair(40, curses.COLOR_BLACK, curses.COLOR_CYAN)
        self.color["bar"] = curses.color_pair(40)
        curses.init_pair(41, curses.COLOR_BLACK, curses.COLOR_WHITE)
        self.color["sel"] = curses.color_pair(41)

    def attr(self, status: str) -> int:
        a = self.color.get(status, 0)
        if status in (BLOCKED, AWAITING_MAINTAINER):
            a |= curses.A_BOLD
        if status in (STALE, UNKNOWN):
            a |= curses.A_DIM if status == STALE else 0
        return a

    # ------------------------------------------------------------------ drawing primitives
    def put(self, y: int, x: int, text: str, attr: int = 0, width: Optional[int] = None) -> None:
        h, w = self.scr.getmaxyx()
        if y < 0 or y >= h or x >= w:
            return
        maxw = (w - x) if width is None else min(width, w - x)
        if maxw <= 0:
            return
        s = clean(text).replace("\t", " ")
        try:
            self.scr.addnstr(y, x, s, maxw - (1 if y == h - 1 and x + maxw >= w else 0), attr)
        except curses.error:
            # best-effort: UI preferences and terminal capabilities are optional
            pass

    def flash(self, msg: str) -> None:
        self.message, self.message_ts = msg, time.time()

    # ------------------------------------------------------------------ data
    def refresh_views(self, force: bool = False) -> None:
        v = self.collector.version
        if v == self.seen_version and not force:
            return
        self.seen_version = v
        self.snap = self.collector.snapshot()
        self.views = build_views(self.snap, self.paths, self.catalog)
        self.restore_selection()

    @staticmethod
    def row_key(screen: int, r: dict):
        if screen == 2:
            return (r.get("kind"), r.get("name"), r.get("pid"), r.get("tmux"), r.get("cwd"))
        if screen == 4:
            return (r.get("repo"), r.get("number"))
        return r.get("path") or r.get("id")

    def restore_selection(self) -> None:
        """Keep the same item selected when a refresh re-sorts a list (and across tmux switches)."""
        for s in (1, 2, 3, 4, 5):
            key = self.sel_key[s]
            if key is None:
                continue
            for i, r in enumerate(self.items(s)):
                if self.row_key(s, r) == key:
                    self.sel[s] = i
                    break

    def tier_status(self) -> str:
        parts = []
        names = {"fast": "local", "records": "records", "git": "git", "github": "GitHub"}
        keys = {"fast": "processes", "records": "file_locks", "git": "worktrees", "github": "github_prs"}
        for t in ("fast", "git", "github"):
            o = self.snap.get(keys[t])
            if self.collector.running.get(t) and o is not None and o.from_cache:
                parts.append(f"{names[t]}: CACHED {ago(o.observed_at)}, checking…")
            elif self.collector.running.get(t) and o is None:
                parts.append(f"{names[t]}: checking…")
            elif o is None:
                parts.append(f"{names[t]}: not yet")
            elif not o.ok:
                parts.append(f"{names[t]}: UNAVAILABLE")
            elif o.from_cache:
                parts.append(f"{names[t]}: CACHED {ago(o.observed_at)}")
            else:
                parts.append(f"{names[t]}: {ago(o.observed_at)}" + (" (partial)" if o.error else ""))
        return "  ·  ".join(parts)

    # ------------------------------------------------------------------ lists per screen
    def items(self, screen: Optional[int] = None) -> list:
        v = self.views
        screen = self.screen if screen is None else screen
        f = self.filters[screen].lower()
        if screen == 1:
            rows = v.get("programs", [])
        elif screen == 2:
            rows = v.get("sessions", [])
        elif screen == 3:
            rows = v.get("worktrees", [])
            if self.keep_only:
                rows = [w for w in rows if w["verdict"] == "KEEP"]
        elif screen == 4:
            rows = v.get("prs", [])
        elif screen == 5:
            rows = v.get("handoffs", [])
        else:
            return []
        if f:
            rows = [r for r in rows if f in json.dumps(r, default=str).lower()]
        return rows

    # ------------------------------------------------------------------ main draw
    def draw(self) -> None:
        self.scr.erase()
        h, w = self.scr.getmaxyx()
        title = f" ICN DEVELOPMENT CONTROL CENTER  v{__version__} "
        self.put(0, 0, " " * w, self.color.get("bar", curses.A_REVERSE))
        self.put(0, 0, title, self.color.get("bar", curses.A_REVERSE) | curses.A_BOLD)
        self.put(0, len(title) + 1, self.tier_status(), self.color.get("bar", curses.A_REVERSE))
        x = 0
        for i, name in enumerate(SCREENS):
            label = f" {i + 1} {name} "
            a = (self.color.get("sel", curses.A_REVERSE) | curses.A_BOLD) if i == self.screen else curses.A_DIM
            self.put(1, x, label, a)
            x += len(label) + 1
        body_top, body_bottom = 3, h - 2
        if h < 12 or w < 60:
            self.put(3, 0, "Terminal too small. Please enlarge the window (at least 60x12).", self.attr(WARNING))
        else:
            [self.draw_overview, self.draw_programs, self.draw_sessions, self.draw_worktrees,
             self.draw_prs, self.draw_handoffs, self.draw_health][self.screen](body_top, body_bottom, w)
        foot = " g:SWITCH SESSION  ?:help  1-7:screens  ↑↓:select  Enter:actions  a:join  c:open Claude  o:record  r:refresh  /:filter  q:quit "
        if self.filters[self.screen]:
            foot = f" filter: '{self.filters[self.screen]}' (/ to change) |" + foot
        self.put(h - 1, 0, foot.ljust(w), self.color.get("bar", curses.A_REVERSE))
        if self.message and time.time() - self.message_ts < 6:
            self.put(h - 2, 0, " " + self.message, self.attr(AWAITING_MAINTAINER))
        elif self.last_join:
            self.put(h - 2, 0, f" Last joined: {self.last_join[0]} ({ago(self.last_join[1])}).  "
                     f"From any session, {self.back_hint} comes back here.", curses.A_DIM)
        self.scr.noutrefresh()
        curses.doupdate()

    # generic list + detail layout
    def draw_list(self, top: int, bottom: int, w: int, header: str, rows: list, fmt, detail) -> None:
        n = len(rows)
        s = self.screen
        self.sel[s] = max(0, min(self.sel[s], n - 1)) if n else 0
        list_h = max(4, min((bottom - top) * 55 // 100, n + 2))
        self.put(top, 0, header[:w], curses.A_BOLD | curses.A_UNDERLINE)
        vis = list_h - 1
        if self.sel[s] < self.top[s]:
            self.top[s] = self.sel[s]
        if self.sel[s] >= self.top[s] + vis:
            self.top[s] = self.sel[s] - vis + 1
        if n:
            self.sel_key[s] = self.row_key(s, rows[self.sel[s]])
        for i in range(vis):
            idx = self.top[s] + i
            if idx >= n:
                break
            text, status = fmt(rows[idx])
            a = self.attr(status)
            if idx == self.sel[s]:
                a = self.color.get("sel", curses.A_REVERSE) | curses.A_BOLD
            self.put(top + 1 + i, 0, ("▶ " if idx == self.sel[s] else "  ") + text, a, w)
        if n == 0:
            self.put(top + 1, 2, self.empty_reason(), self.attr(UNKNOWN))
        if n > vis:
            self.put(top, w - 18, f" {self.sel[s] + 1}/{n} ", curses.A_DIM)
        dy = top + list_h
        self.put(dy, 0, "─" * (w - 1), curses.A_DIM)
        if n:
            lines = detail(rows[self.sel[s]])
            self.draw_lines(dy + 1, bottom, w, lines, 0)

    def empty_reason(self) -> str:
        key = {1: None, 2: "processes", 3: "worktrees", 4: "github_prs", 5: "handoffs"}.get(self.screen)
        o = self.snap.get(key) if key else None
        if key and o is None:
            return "Not observed yet — collecting…"
        if o is not None and not o.ok:
            return f"UNKNOWN — source unavailable: {o.error}"
        if self.filters[self.screen]:
            return "Nothing matches the filter."
        return "Observed: none."

    def draw_lines(self, top: int, bottom: int, w: int, lines: list, offset: int) -> int:
        """Render (text, attr) lines with wrapping; returns total wrapped line count."""
        wrapped = []
        for item in lines:
            text, a = item if isinstance(item, tuple) else (item, 0)
            if not text:
                wrapped.append(("", a))
                continue
            indent = len(text) - len(text.lstrip(" "))
            for j, part in enumerate(textwrap.wrap(text, max(20, w - 1), subsequent_indent=" " * (indent + 2),
                                                   drop_whitespace=False) or [""]):
                wrapped.append((part, a))
        for i, (t, a) in enumerate(wrapped[offset:offset + (bottom - top)]):
            self.put(top + i, 0, t, a, w)
        return len(wrapped)

    def H(self, t: str):
        return (t, self.color.get("title", 0) | curses.A_BOLD)

    # ------------------------------------------------------------------ screens
    def overview_lines(self) -> list:
        v = self.views
        L = []
        sess = v.get("sessions", [])
        agents = [s for s in sess if s["kind"] == "agent"]
        L.append(self.H("ACTIVE DEVELOPMENT SESSIONS  (process table + tmux + Claude session files)"))
        if self.snap.get("processes") is None:
            L.append(("  collecting…", self.attr(UNKNOWN)))
        for s in agents:
            where = s.get("pr") and f"{s['pr']['repo']}#{s['pr']['number']}" or s.get("branch") or os.path.basename(s.get("cwd") or "?")
            L.append((f"  ● {s['name'][:40]:40} {s['state_label'][:46]:46} {where}", self.attr(s["status"])))
        if self.snap.get("processes") is not None and not agents:
            L.append(("  no agent processes observed", self.attr(UNKNOWN)))
        other = [s for s in sess if s["kind"] != "agent"]
        if other:
            L.append((f"  + {len(other)} other session record(s)/tmux/service rows — see screen 3", curses.A_DIM))
        L.append("")
        L.append(self.H("NEEDS YOUR ATTENTION"))
        for a in v.get("attention", [])[:14]:
            L.append((f"  [{a['status']}] {a['title']}", self.attr(a["status"])))
            if a.get("detail"):
                L.append((f"      {a['detail']}   — {a['source']}", curses.A_DIM))
        if not v.get("attention"):
            L.append("  nothing flagged by the reconciliation rules")
        L.append("")
        L.append(self.H("WHAT SHOULD HAPPEN NEXT  (rule-based; each line names its reason)"))
        for st in v.get("next", []):
            L.append(f"  → {st}")
        L.append("")
        prs = v.get("prs", [])
        gh = self.snap.get("github_prs")
        L.append(self.H("OPEN PULL REQUESTS BY STATE" + (f"  (GitHub {ago(gh.observed_at)}{', CACHED' if gh and gh.from_cache else ''})" if gh else "")))
        if gh is None or not gh.ok:
            L.append((f"  GitHub: UNKNOWN — {gh.error if gh else 'not yet observed'}", self.attr(UNKNOWN)))
        else:
            counts: dict = {}
            for p in prs:
                counts[p["status"]] = counts.get(p["status"], 0) + 1
            L.append("  " + "   ".join(f"{k}: {n}" for k, n in sorted(counts.items(), key=lambda x: -x[1])))
            for p in [p for p in prs if p["status"] in (AWAITING_MAINTAINER, BLOCKED)][:8]:
                L.append((f"  {p['repo']}#{p['number']:<5} {p['status']:20} {p['title'][:70]}", self.attr(p["status"])))
        L.append("")
        L.append(self.H("BLOCKED WORK"))
        blocked = [p for p in prs if p["status"] == BLOCKED]
        for p in blocked[:6]:
            L.append((f"  {p['repo']}#{p['number']} — {p['status_reason']}", self.attr(BLOCKED)))
        if not blocked:
            L.append(("  none observed" if gh and gh.ok else "  UNKNOWN (GitHub not observed)", self.attr(UNKNOWN)))
        L.append("")
        L.append(self.H("RECENTLY MERGED, LAST 7 DAYS  (merged ≠ independently verified capability)"))
        for m in v.get("merged", [])[:8]:
            L.append(f"  ✓ {m['repo']}#{m['number']} merged {ago(m.get('merged_ts'))}: {m['title'][:80]}")
        if not v.get("merged"):
            o = self.snap.get("github_merged")
            L.append(("  " + ("none" if o and o.ok else "UNKNOWN — " + (o.error if o else "not yet observed")), self.attr(UNKNOWN)))
        L.append("")
        L.append(self.H("CRITICAL INFRASTRUCTURE"))
        hw = [a for a in v.get("attention", []) if a["source"] in ("statvfs", "/proc/meminfo") or "disk" in a["title"].lower() or "MCP" in a["title"]]
        for a in hw:
            L.append((f"  [{a['status']}] {a['title']} — {a['detail']}", self.attr(a["status"])))
        if not hw:
            L.append("  no host-health thresholds crossed")
        return L

    def draw_overview(self, top, bottom, w):
        lines = self.overview_lines()
        s = self.screen
        total = self.draw_lines(top, bottom, w, lines, self.scroll[s])
        self.scroll[s] = max(0, min(self.scroll[s], max(0, total - (bottom - top))))

    def draw_programs(self, top, bottom, w):
        def fmt(p):
            st = AWAITING_MAINTAINER if p["waiting"] else BLOCKED if p["blocked"] else LIVE if p["live_agents"] \
                else IN_PROGRESS if p["prs"] else UNKNOWN
            return (f"{p['name'][:30]:30} PRs {len(p['prs']):>3}  agents {len(p['live_agents']):>2}  "
                    f"worktrees {len(p['worktrees']):>3}  awaiting you {len(p['waiting']):>2}  blocked {len(p['blocked']):>2}", st)

        def detail(p):
            L = [self.H(p["name"]), f"  {p['summary']}  (curated description, programs.json — not a status source)",
                 f"  Owner: {p['owner']}", ("  NEXT: " + p["next"], curses.A_BOLD)]
            for t in p["tracking"]:
                live = t.get("live") or {}
                L.append(f"  Tracking: {t['ref']} {live.get('state', 'UNKNOWN')} — {live.get('title') or live.get('error') or ''} ({t['why']})")
            L.append(self.H(f"  Open PRs ({len(p['prs'])})"))
            for x in p["prs"][:12]:
                L.append((f"    {x['repo']}#{x['number']} [{x['status']}] {x['title'][:70]}  ({x['program_rule']})", self.attr(x["status"])))
            L.append(self.H(f"  Sessions ({len(p['sessions'])})"))
            for x in p["sessions"][:8]:
                L.append((f"    {x['name']} — {x['state_label']}", self.attr(x["status"])))
            L.append(self.H(f"  Worktrees ({len(p['worktrees'])}); kept: {sum(1 for x in p['worktrees'] if x['verdict'] == 'KEEP')}"))
            for x in [x for x in p["worktrees"] if x["live_sessions"] or (x["status"] or {}).get("dirty")][:8]:
                L.append(f"    {x['repo']}/{x['name']} [{x.get('branch') or 'detached'}] {'; '.join(x['keep_reasons'][:2])}")
            L.append(self.H("  Related handoffs (by file name keyword)"))
            for x in p["handoffs"][:6]:
                L.append(f"    {ago(x['mtime']):>10}  {x['path']}")
            L.append(("  Program classification is rule-based (programs.json); the rule is shown per item.", curses.A_DIM))
            return L

        self.draw_list(top, bottom, w, "PROGRAM (long-lived)            counts are live; nothing here is inferred as 'inactive'",
                       self.items(), fmt, detail)

    def actions_for(self, view: dict) -> list:
        return session_actions(view, self.views.get("sessions", []), self.paths, self.prefix)

    def session_detail(self, s) -> list:
        acts = self.actions_for(s)
        live = s["kind"] == "agent"
        L = [self.H(f"{s['name']}  —  {s['state_label']}"), ""]
        L.append(self.H("What is it working on?"))
        L.append(f"  Task: {s.get('task') or 'unknown'}" + (f"   [{s['task_source']}]" if s.get("task_source") else "   [no source]"))
        if s.get("pr"):
            L.append(f"  PR: {s['pr']['repo']}#{s['pr']['number']} [{s['pr'].get('status')}] {s['pr']['title'][:80]}"
                     f"   [{s['pr']['how']}]")
        else:
            L.append("  PR: none associated")
        L.append(f"  Program: {s.get('program') or '?'}   [{s.get('program_rule') or 'rule unknown'}]")
        L.append("")
        L.append(self.H("Is Claude actually running?   [live process evidence]"))
        if live:
            L.append((f"  YES - {s.get('agent')} process {s.get('pid')} observed now (pid and start time checked)"
                      + (f", started {ago(s.get('started'))}" if s.get("started") else ""), self.attr(LIVE)))
            src = "Claude Code's own session status file" if s.get("conversation") else "no status file; from transcript writes"
            L.append(f"  Busy or waiting: {s['state_label']}   [{src}]")
            L.append(f"  tmux: {s.get('tmux') or 'NOT in tmux (a plain terminal)'}"
                     + ("   (another client is attached too)" if s.get("tmux_attached") else ""))
        elif s["kind"] == "service":
            L.append(("  A service process (app-server/proxy) is running; it is not an interactive agent.", self.attr(IDLE)))
        elif s["kind"] == "control-center":
            L.append(("  This is the control center's own tmux session.", self.attr(LIVE)))
        else:
            L.append(("  NO - no agent process was found for this entry when the process table was last read.",
                      self.attr(s["status"])))
        L.append("")
        L.append(self.H("Where   [git]"))
        L.append(f"  Worktree: {s.get('worktree') or '(not a git worktree)'}")
        L.append(f"  Branch: {s.get('branch') or '-'}   head: {short(s.get('head'))}   directory: {s.get('cwd') or '?'}")
        L.append("")
        L.append(self.H("Last observable progress"))
        L.append(f"  {ago(s.get('last_activity'))} ({fmt_ts(s.get('last_activity'))})   sources: {', '.join(s['sources'])}")
        L.append("")
        L.append(self.H("Recorded metadata   [records; not proof that anything is running]"))
        rec = s.get("record") or {}
        if rec:
            L.append(f"  icn-session record '{rec.get('name')}': conversation {rec.get('claude_session_id') or '-'},"
                     f" worktree {rec.get('worktree')}, launched {rec.get('launch_mode')}, updated {rec.get('updated_at')}")
        elif s.get("icn_session_record"):
            L.append(f"  icn-session record: {s['icn_session_record']}")
        if s.get("conversation"):
            L.append(f"  Conversation id: {s['conversation']}")
        if s.get("registry_id"):
            L.append(f"  icn-ops registry: {s['registry_id'][:8]}, heartbeat {ago(s.get('registry_heartbeat'))},"
                     f" activity {s.get('registry_activity') or '-'}")
        L.append(f"  Claimed files: {', '.join(s['claims'][:8]) if s.get('claims') else 'none recorded'}")
        L.append("")
        L.append(self.H("Handoff / design packet"))
        for hpath in s.get("handoffs") or []:
            L.append(f"  Handoff: {hpath}")
        for e in s.get("evidence") or []:
            L.append(f"  Record:  {e}")
        if not (s.get("handoffs") or s.get("evidence")):
            L.append("  none found")
        L.append("")
        L.append(self.H("What can I do?   (Enter on this screen, or g from anywhere)"))
        for a in acts:
            if a["enabled"]:
                L.append((f"  [available]    {a['label']} - {a['reason']}", self.attr(LIVE)))
            else:
                L.append((f"  [unavailable]  {a['label']} - {a['reason']}", curses.A_DIM))
        return L

    def draw_sessions(self, top, bottom, w):
        def fmt(s):
            where = (s.get("branch") or os.path.basename((s.get("cwd") or "?").rstrip("/")))[:34]
            return (f"{s['status'][:22]:22} {s['name'][:36]:36} {s['state_label'][:34]:34} {ago(s.get('last_activity')):>10}  {where}", s["status"])

        self.draw_list(top, bottom, w, "STATUS                 SESSION                              STATE                              ACTIVITY  WHERE"
                       "      (Enter: join / resume / details)",
                       self.items(), fmt, self.session_detail)

    def worktree_detail(self, x) -> list:
        st = x["status"] or {}
        a = self.attr(LIVE if x["live_sessions"] else (AWAITING_MAINTAINER if x["verdict"] == "KEEP" else UNKNOWN))
        L = [self.H(f"{x['repo']}/{x['name']}"),
             (f"  {x['verdict']}", a | curses.A_BOLD),
             f"  Path: {x['path']}",
             f"  Branch: {x.get('branch') or '(detached)'}   head: {short(x.get('head'))}   upstream: {st.get('upstream') or 'none'}"
             f"   ahead/behind: {st.get('ahead', '?')}/{st.get('behind', '?')}",
             f"  Uncommitted: {st.get('modified', '?')} modified, {st.get('untracked', '?')} untracked, {st.get('conflicted', 0)} conflicted"
             f"   commits on no remote-tracking ref: {st.get('local_only_commits', '?')}",
             f"  Remote refs last fetched: {ago((self.snap.get('worktrees').data or {}).get('remote_refs_updated', {}).get(x['repo'])) if self.snap.get('worktrees') and self.snap['worktrees'].ok else '?'} (no fetch is ever done here)",
             "  Keep because: " + ("; ".join(x["keep_reasons"]) or "-"),
             "  Cleanup signals: " + ("; ".join(x["candidate_reasons"]) or "-"),
             f"  Live sessions: {', '.join(x['live_sessions']) or 'none'}",
             f"  Open PR: #{x['pr']['number']} [{x['pr']['status']}]" if x.get("pr") else "  Open PR: none for this branch",
             f"  Program: {x['program']} ({x['program_rule']})"]
        for lk in x.get("locks") or []:
            L.append(f"  Lock ({lk['status']}): {', '.join(lk['files'][:6])}")
        row = x.get("row")
        if row:
            L.append(self.H("  active-worktrees.yaml (historical claim, may be outdated)"))
            L.append(f"    task: {row.get('task')}")
            L.append(f"    status: {row.get('status')}")
        return L

    def draw_worktrees(self, top, bottom, w):
        def fmt(x):
            st = x["status"] or {}
            flags = []
            if x["live_sessions"]:
                flags.append("LIVE")
            if st.get("dirty"):
                flags.append("DIRTY")
            if st.get("local_only_commits"):
                flags.append(f"{st['local_only_commits']} LOCAL-ONLY")
            if x.get("pr"):
                flags.append(f"PR#{x['pr']['number']}")
            if x.get("locks"):
                flags.append("LOCKED-FILES")
            if st.get("exists") is False:
                flags.append("MISSING")
            status = LIVE if x["live_sessions"] else (AWAITING_MAINTAINER if (st.get("dirty") or st.get("local_only_commits")) else
                                                     (IN_PROGRESS if x["verdict"] == "KEEP" else UNKNOWN))
            verdict = "KEEP" if x["verdict"] == "KEEP" else ("CANDIDATE" if x["verdict"].startswith("CLEANUP") else "no-signal")
            return (f"{verdict:9} {x['repo'][:12]:12} {x['name'][:40]:40} {(x.get('branch') or 'detached')[:36]:36} {' '.join(flags)}", status)

        rows = self.items()
        hdr = f"VERDICT   REPO         WORKTREE                                 BRANCH                               FLAGS   ({'only KEEP' if self.keep_only else 'all'}; k toggles)"
        self.draw_list(top, bottom, w, hdr, rows, fmt, self.worktree_detail)

    def pr_detail(self, p) -> list:
        lc = p.get("lifecycle") or {}
        req = p["required"]
        L = [self.H(f"{p['repo']}#{p['number']}  {p['title']}"),
             (f"  {p['status']}: {p['status_reason']}", self.attr(p["status"])),
             f"  {p.get('url')}",
             f"  Branch: {p['branch']} → {p['base']}   head {short(p['head'])}   author {p['author']}   {'DRAFT' if p['draft'] else 'ready for review'}",
             f"  Updated: {ago(p.get('updated_ts'))}   mergeable: {p.get('mergeable')}   merge state: {p.get('merge_state')}",
             (f"  Required CI: {req['verdict']} — {req['detail']}", self.attr(LIVE if req["verdict"] == "GREEN" else BLOCKED if req["verdict"] == "FAILING" else UNKNOWN))]
        if req["failing"]:
            L.append(f"    failing: {', '.join(req['failing'])}")
        if req["pending"]:
            L.append(f"    pending: {', '.join(req['pending'])}")
        if req["missing"]:
            L.append(f"    not reported: {', '.join(req['missing'][:8])}")
        L.append(f"  Unresolved review threads: {p['unresolved_threads']}")
        L.append(self.H("  Delivery lifecycle (PR body block; owner: ops/state/truth/delivery.json)"))
        if lc:
            L.append(f"    state {lc.get('state')}   lane {lc.get('lane')}   freeze head {lc.get('freeze_head') or '-'}"
                     f"   follow-up ledger {('#' + str(lc['ledger'])) if lc.get('ledger') else '-'}")
            if lc.get("review_generation"):
                L.append(f"    review generation: {lc['review_generation'][:220]}")
        else:
            L.append("    no lifecycle block in the PR body")
        L.append(self.H("  Reviews (automated / AI reviews are evidence, never human approval)"))
        for r in (p.get("reviews") or [])[-6:]:
            f = r.get("findings")
            L.append(f"    {r['state']:17} by {r['author']} [{r['kind']}] on {short(r.get('commit'))}"
                     + (f"  {r['generation'] or ''} {f['blocker']} BLOCKER / {f['follow_up']} FOLLOW_UP / {f['question']} QUESTION" if f else ""))
        if not p.get("reviews"):
            L.append("    none")
        rv = latest_review_generation(p)
        if rv and p.get("head") and rv.get("commit") and rv["commit"] != p["head"]:
            L.append(("    NOTE: the latest review generation is on an older head than the PR's current head.", self.attr(STALE)))
        human = any(r["counts_as_human_approval"] for r in p.get("reviews") or [])
        L.append(f"  Human approval present: {'yes' if human else 'no'}   (GitHub reviewDecision: {p.get('review_decision') or 'none'})")
        L.append((f"  Merge readiness: {p['readiness']['verdict']}", curses.A_BOLD))
        for r in p["readiness"]["reasons"]:
            L.append(f"    - {r}")
        for d in p.get("evidence_dirs") or []:
            L.append(f"  Local review evidence: {d}   (press o)")
        L.append(f"  Program: {p['program']} ({p['program_rule']})")
        return L

    def draw_prs(self, top, bottom, w):
        def fmt(p):
            lc = (p.get("lifecycle") or {}).get("state") or "-"
            return (f"{p['repo'][:6]:6} #{p['number']:<5} {p['status'][:20]:20} {'draft' if p['draft'] else 'ready':5} "
                    f"{lc[:10]:10} CI:{p['required']['verdict'][:7]:7} {p['title'][:80]}", p["status"])

        self.draw_list(top, bottom, w, "REPO   PR     STATUS               DRAFT LIFECYCLE  REQ CI     TITLE",
                       self.items(), fmt, self.pr_detail)

    def handoff_detail(self, hd) -> list:
        try:
            sm = summarize_handoff(hd["path"])
        except Exception as exc:
            return [(f"  cannot read: {exc}", self.attr(UNKNOWN))]
        L = [self.H(sm["title"]), f"  {hd['path']}", f"  modified {ago(hd['mtime'])} ({fmt_ts(hd['mtime'])}); press Enter or o to read it in full",
             ("  Handoffs are memory, not truth: re-verify any status they state against the live screens.", curses.A_DIM)]
        names = {"done": "Completed / results", "open": "Unfinished / open / follow-ups", "decision": "Decisions needed",
                 "next": "Next step"}
        for k, title in names.items():
            blocks = sm["sections"].get(k) or []
            if not blocks:
                continue
            L.append(self.H(f"  {title}"))
            for b in blocks[:2]:
                L.append(f"    ## {b['heading']}")
                for line in b["lines"][:5]:
                    L.append("      " + redact(line))
        return L

    def draw_handoffs(self, top, bottom, w):
        def fmt(hd):
            return (f"{ago(hd['mtime']):>10}  {hd['title'][:70]:70}  {os.path.relpath(hd['path'], self.paths.icn_dev)}",
                    IN_PROGRESS if time.time() - hd["mtime"] < 86400 else UNKNOWN)

        self.draw_list(top, bottom, w, "  MODIFIED  TITLE                                                                   FILE",
                       self.items(), fmt, self.handoff_detail)

    def health_lines(self) -> list:
        L = [self.H("FILESYSTEMS  (statvfs, live)")]
        for fs in (self.snap.get("fs").data if self.snap.get("fs") and self.snap["fs"].ok else []):
            if fs.get("error"):
                L.append((f"  {fs['mount']}: UNKNOWN {fs['error']}", self.attr(UNKNOWN)))
                continue
            st = BLOCKED if fs["used_pct"] >= 90 else WARNING if fs["used_pct"] >= 80 else LIVE
            bar = "█" * int(fs["used_pct"] // 4) + "░" * (25 - int(fs["used_pct"] // 4))
            L.append((f"  {fs['mount']:12} {bar} {fs['used_pct']:5}% used  {fs['free_gib']:>7} GiB free of {fs['size_gib']} GiB"
                      + ("" if fs.get("is_mountpoint") else "  (NOT a mount point)"), self.attr(st)))
        mem, ld = self.snap.get("mem"), self.snap.get("load")
        if mem and mem.ok:
            L.append(f"  Memory: {mem.data['available_gib']} GiB available of {mem.data['total_gib']} GiB; swap used {mem.data['swap_used_gib']} GiB")
        if ld and ld.ok:
            L.append(f"  Load: {ld.data['load1']} / {ld.data['load5']} / {ld.data['load15']} on {ld.data['cpus']} CPUs")
        L.append("")
        dg = self.snap.get("disk_guard")
        L.append(self.H("DISK GUARD  (last report written by icn-disk-guard; not re-run here)"))
        if dg and dg.ok:
            d = dg.data["data"]
            L.append((f"  level {d.get('level')} at {d.get('ts')} ({ago(dg.data['mtime'])})", self.attr(WARNING if d.get("level") != "OK" else LIVE)))
            b = d.get("build") or {}
            L.append(f"  build volume {b.get('build_volume_gib')} GiB · in-tree {b.get('intree_gib')} GiB · sccache {b.get('sccache_gib')} GiB"
                     f" · evictable {b.get('evictable_gib')} GiB · undiscovered {b.get('undiscovered_gib')} GiB · orphan {b.get('orphan_gib')} GiB")
            hr = d.get("headroom") or {}
            if hr:
                L.append("  headroom: " + ", ".join(f"{k}={v}" for k, v in hr.items()))
            L.append(("  Stale-artifact candidates come from the guard's own 'evictable' figure. Nothing is deleted from here;"
                      " use icn-disk-guard --report and verify ownership first.", curses.A_DIM))
        else:
            L.append((f"  UNKNOWN — {dg.error if dg else 'not observed'}", self.attr(UNKNOWN)))
        L.append("")
        L.append(self.H("BUILD ADMISSION  (~/icn-dev/state/admit)"))
        rs = self.snap.get("reservations")
        if rs and rs.ok:
            L.append(f"  mode: {rs.data.get('mode') or 'unknown'}   live reservations: {len(rs.data['reservations'])}")
            for r in rs.data["reservations"]:
                exp = r.get("expires", "0")
                L.append(f"    {r['id']} class={r.get('cls')} budget={r.get('budget')}GiB lane={r.get('lane')}"
                         f" expires {fmt_ts(float(exp)) if exp.isdigit() else '?'}")
            for line in rs.data.get("recent_log", [])[-4:]:
                L.append(("    " + line[:200], curses.A_DIM))
        else:
            L.append((f"  UNKNOWN — {rs.error if rs else 'not observed'}", self.attr(UNKNOWN)))
        L.append("")
        L.append(self.H("RUNNING BUILDS  (cargo/rustc processes, live)"))
        bd = self.snap.get("builds")
        if bd and bd.ok:
            for b in bd.data:
                L.append(f"  pid {b['pid']} {b['comm']} in {b.get('cwd')}")
            if not bd.data:
                L.append("  none running")
        else:
            L.append(("  UNKNOWN", self.attr(UNKNOWN)))
        L.append("")
        ml = self.snap.get("mcp_live")
        L.append(self.H("icn-ops MCP LIVE UPDATER"))
        if ml and ml.ok:
            d = ml.data["data"]
            st = LIVE if d.get("state") in ("ok", "current", "live", "success") else BLOCKED
            L.append((f"  state {d.get('state')} · live {short(d.get('live_commit'))} · origin/main {short(d.get('origin_main'))} · checked {d.get('checked_utc')}", self.attr(st)))
            if d.get("detail"):
                L.append(f"  {d['detail'][:200]}")
        else:
            L.append((f"  UNKNOWN — {ml.error if ml else 'not observed'}", self.attr(UNKNOWN)))
        L.append("")
        L.append(self.H("WARNINGS"))
        from .reconcile import health_warnings
        for a in health_warnings(self.snap, time.time()):
            L.append((f"  [{a['status']}] {a['title']} — {a['detail']} ({a['source']})", self.attr(a["status"])))
        return L

    def draw_health(self, top, bottom, w):
        s = self.screen
        total = self.draw_lines(top, bottom, w, self.health_lines(), self.scroll[s])
        self.scroll[s] = max(0, min(self.scroll[s], max(0, total - (bottom - top))))

    def sources_lines(self) -> list:
        L = [self.H("WHERE EVERY PIECE OF DATA CAME FROM"), ""]
        for k in sorted(self.snap):
            if k.startswith("_"):
                continue
            o = self.snap[k]
            st = LIVE if o.ok and not o.from_cache else (STALE if o.ok else UNKNOWN)
            label = "live" if st == LIVE else ("CACHED/STALE" if st == STALE else "UNAVAILABLE")
            L.append((f"  {o.source:34} {label:13} observed {fmt_ts(o.observed_at)} ({ago(o.observed_at)}) in {o.duration_s:.1f}s", self.attr(st)))
            if o.error:
                L.append((f"      {o.error}", curses.A_DIM))
        L.append("")
        L.append(f"  Cache (non-authoritative, for fast startup only): {os.path.join(self.paths.control_state, 'last-snapshot.json')}")
        L.append("  Program catalog (curated, non-authoritative): ops/dev-control/programs.json")
        return L

    # ------------------------------------------------------------------ full-screen viewers
    def viewer(self, lines: list, title: str) -> None:
        off = 0
        while True:
            self.scr.erase()
            h, w = self.scr.getmaxyx()
            self.put(0, 0, (" " + title).ljust(w), self.color.get("bar", curses.A_REVERSE))
            total = self.draw_lines(1, h - 1, w, lines, off)
            self.put(h - 1, 0, f" ↑↓ PgUp PgDn Home End: scroll   q/Esc: back   line {off + 1}/{max(total, 1)} ".ljust(w),
                     self.color.get("bar", curses.A_REVERSE))
            self.scr.refresh()
            k = self.scr.getch()
            page = max(1, h - 3)
            if k in (ord("q"), 27, curses.KEY_LEFT, ord("h")):
                return
            if k in (curses.KEY_DOWN, ord("j")):
                off += 1
            elif k in (curses.KEY_UP, ord("k")):
                off -= 1
            elif k in (curses.KEY_NPAGE, ord(" ")):
                off += page
            elif k == curses.KEY_PPAGE:
                off -= page
            elif k == curses.KEY_HOME:
                off = 0
            elif k == curses.KEY_END:
                off = total - page
            elif k == curses.KEY_RESIZE:
                pass
            off = max(0, min(off, max(0, total - page)))

    def open_file(self, path: str) -> None:
        ok, why = may_open(path, self.paths.icn_dev)
        if not ok:
            self.flash(why)
            return
        path = os.path.realpath(path)
        try:
            with open(path, "rb") as f:
                raw = f.read(MAX_VIEW_BYTES)
        except OSError as exc:
            self.flash(f"Cannot open: {exc}")
            return
        text = redact(raw.decode(errors="replace"))
        lines = []
        for line in text.splitlines():
            a = 0
            if line.startswith("#"):
                a = self.color.get("title", 0) | curses.A_BOLD
            lines.append((line, a))
        if len(raw) >= MAX_VIEW_BYTES:
            lines.append(("[truncated at 2 MB]", self.attr(WARNING)))
        self.viewer(lines, f"{path}   (read-only; memory, not truth — re-verify live state)")

    def open_selected_record(self) -> None:
        rows = self.items()
        if not rows:
            return
        x = rows[self.sel[self.screen]]
        cands = []
        if self.screen == 2:
            cands = (x.get("handoffs") or []) + (x.get("evidence") or [])
        elif self.screen == 5:
            cands = [x["path"]]
        elif self.screen == 4:
            for d in x.get("evidence_dirs") or []:
                for name in ("review-body.md", "HANDOFF.md"):
                    if os.path.exists(os.path.join(d, name)):
                        cands.append(os.path.join(d, name))
        elif self.screen == 1:
            cands = [h["path"] for h in x.get("handoffs") or []]
        elif self.screen == 3:
            cands = [os.path.expanduser(r) for r in ((x.get("row") or {}).get("refs") or [])]
        cands = [c for c in cands if os.path.isfile(c)]
        if not cands:
            self.flash("No record file is associated with this item.")
            return
        if len(cands) == 1:
            self.open_file(cands[0])
            return
        choice = self.choose("Open which record?", cands)
        if choice is not None:
            self.open_file(cands[choice])

    def choose(self, title: str, options: list[str]) -> Optional[int]:
        sel = 0
        while True:
            self.scr.erase()
            h, w = self.scr.getmaxyx()
            self.put(0, 0, (" " + title + "   Enter: open   Esc: cancel").ljust(w), self.color.get("bar", curses.A_REVERSE))
            for i, o in enumerate(options[:h - 2]):
                self.put(1 + i, 0, ("▶ " if i == sel else "  ") + o, (self.color.get("sel", curses.A_REVERSE) if i == sel else 0))
            self.scr.refresh()
            k = self.scr.getch()
            if k in (27, ord("q")):
                return None
            if k in (10, 13, curses.KEY_ENTER):
                return sel
            if k == curses.KEY_DOWN:
                sel = min(sel + 1, len(options) - 1)
            if k == curses.KEY_UP:
                sel = max(sel - 1, 0)

    def confirm(self, lines: list[str]) -> bool:
        """y continues; any other KEY cancels. A terminal resize is not a key: redraw and keep asking."""
        self.scr.timeout(-1)
        try:
            while True:
                self.scr.erase()
                for i, line in enumerate(lines):
                    self.put(2 + i, 2, line, curses.A_BOLD if i == 0 else 0)
                self.put(3 + len(lines), 2, "Press y to continue, any other key to cancel.", self.attr(AWAITING_MAINTAINER))
                self.scr.refresh()
                k = self.scr.getch()
                if k in (curses.KEY_RESIZE, -1):
                    continue
                return k in (ord("y"), ord("Y"))
        finally:
            self.scr.timeout(1000)

    # ------------------------------------------------------------------ session switching
    def program_names(self) -> dict:
        return {p["id"]: p["name"] for p in self.catalog.get("programs", [])}

    def session_picker(self) -> None:
        """The quick switcher: every agent conversation, running first, filter as you type."""
        text, sel, top = "", 0, 0
        sel_key = None
        while True:
            self.refresh_views()
            rows = filter_rows(build_rows(self.views.get("sessions", []), self.program_names(), self.actions_for), text)
            if sel_key is not None:
                for i, r in enumerate(rows):
                    if self.row_key(2, r["view"]) == sel_key:
                        sel = i
                        break
            sel = max(0, min(sel, len(rows) - 1))
            self.scr.erase()
            h, w = self.scr.getmaxyx()
            bar = self.color.get("bar", curses.A_REVERSE)
            self.put(0, 0, " SWITCH TO A CLAUDE SESSION   type to filter · ↑↓ select · Enter choose · Esc close ".ljust(w), bar | curses.A_BOLD)
            self.put(1, 0, f" filter: {text}_", curses.A_BOLD)
            self.put(1, max(30, w - 60), f"{len(rows)} shown · running first, then most recent", curses.A_DIM)
            hdr = f"  {'STATE':21} {'SESSION / TASK':34} {'PROGRAM':20} {'REPO / WORKTREE':34} {'PR':11} {'ACTIVITY':>10}  CAN"
            self.put(2, 0, hdr, curses.A_BOLD | curses.A_UNDERLINE)
            list_h = max(3, h - 10)
            if sel < top:
                top = sel
            if sel >= top + list_h:
                top = sel - list_h + 1
            for i in range(list_h):
                idx = top + i
                if idx >= len(rows):
                    break
                r = rows[idx]
                line = (f"{r['state'][:21]:21} {r['name'][:34]:34} {r['program'][:20]:20} "
                        f"{(r['repo'] + '/' + r['where'])[:34]:34} {r['pr'][:11]:11} {r['activity']:>10}  {r['can']}")
                st = LIVE if r["live"] else (REGISTERED_NOT_RUNNING if r["can"] == "resume" else UNKNOWN)
                a = (self.color.get("sel", curses.A_REVERSE) | curses.A_BOLD) if idx == sel else self.attr(st)
                self.put(3 + i, 0, ("▶ " if idx == sel else "  ") + line, a, w)
            if not rows:
                self.put(3, 2, "No sessions match." if text else "No agent sessions observed yet (press r on the main screen to refresh).",
                         self.attr(UNKNOWN))
            dy = 3 + list_h
            self.put(dy, 0, "─" * (w - 1), curses.A_DIM)
            if rows:
                r = rows[sel]
                sel_key = self.row_key(2, r["view"])
                self.put(dy + 1, 0, f" {r['name']}: {r['task'][:w - 10]}", curses.A_BOLD)
                if r["pr"]:
                    self.put(dy + 2, 0, f"   PR {r['pr']} [{r['pr_how']}]   program: {r['program']}", curses.A_DIM)
                y = dy + 3
                for act in r["actions"][:3]:
                    tag = "available  " if act["enabled"] else "unavailable"
                    self.put(y, 0, f"   [{tag}] {act['label']}: {act['reason']}",
                             self.attr(LIVE) if act["enabled"] else curses.A_DIM, w)
                    y += 1
            self.put(h - 1, 0, f" Return here from any session: {self.back_hint} ".ljust(w), bar)
            self.scr.refresh()
            k = self.scr.getch()
            if k == -1 or k == curses.KEY_RESIZE:
                continue
            if k == 27:
                return
            if k in (10, 13, curses.KEY_ENTER):
                if rows and self.action_menu(rows[sel]["view"], preselect=default_action(rows[sel])):
                    return
            elif k == curses.KEY_DOWN:
                sel += 1
            elif k == curses.KEY_UP:
                sel = max(0, sel - 1)
            elif k == curses.KEY_NPAGE:
                sel += list_h
            elif k == curses.KEY_PPAGE:
                sel = max(0, sel - list_h)
            elif k == curses.KEY_HOME:
                sel = 0
            elif k == curses.KEY_END:
                sel = len(rows) - 1
            elif k in (curses.KEY_BACKSPACE, 127, 8):
                text = text[:-1]
                sel_key = None
            elif 32 <= k < 127 and len(text) < 40:
                text += chr(k)
                sel_key, sel = None, 0
            sel = max(0, sel)

    def action_menu(self, view: dict, preselect: Optional[str] = None) -> bool:
        """Show every action for one session (unavailable ones say why). True if we switched away."""
        acts = self.actions_for(view)
        enabled = [i for i, a in enumerate(acts) if a["enabled"]]
        sel = next((i for i, a in enumerate(acts) if a["id"] == preselect and a["enabled"]), enabled[0] if enabled else 0)
        while True:
            self.scr.erase()
            h, w = self.scr.getmaxyx()
            bar = self.color.get("bar", curses.A_REVERSE)
            self.put(0, 0, f" {view['name']} — {view['state_label']} ".ljust(w), bar | curses.A_BOLD)
            self.put(1, 0, f"  {view.get('task') or ''}"[:w], curses.A_DIM)
            for i, a in enumerate(acts):
                y = 3 + i * 2
                mark = "▶ " if i == sel else "  "
                attr = (self.color.get("sel", curses.A_REVERSE) | curses.A_BOLD) if i == sel else (0 if a["enabled"] else curses.A_DIM)
                self.put(y, 0, f"{mark}{a['label']}{'' if a['enabled'] else '   (unavailable)'}", attr, w)
                self.put(y + 1, 4, a["reason"], curses.A_DIM if not a["enabled"] else 0, w - 4)
            self.put(h - 1, 0, " ↑↓ choose · Enter do it · Esc back ".ljust(w), bar)
            self.scr.refresh()
            k = self.scr.getch()
            if k == -1 or k == curses.KEY_RESIZE:
                continue
            if k in (27, curses.KEY_LEFT):
                return False
            if k == curses.KEY_DOWN:
                sel = min(sel + 1, len(acts) - 1)
            elif k == curses.KEY_UP:
                sel = max(sel - 1, 0)
            elif k in (10, 13, curses.KEY_ENTER):
                a = acts[sel]
                if not a["enabled"]:
                    self.flash(a["reason"])
                    continue
                if a["id"] == "details":
                    self.viewer(self.session_detail(view), view["name"])
                    continue
                if a["id"] == "open_record":
                    self.open_records(view.get("handoffs", []) + view.get("evidence", []))
                    continue
                if a["id"] == "join":
                    return self.join_view(view)
                if a["id"] == "resume":
                    return self.launch("resume", view)
                return False

    def join_selected(self) -> None:
        if self.screen != 2:
            self.flash("Press g to pick a session, or go to screen 3.")
            return
        rows = self.items()
        if rows:
            self.join_view(rows[self.sel[self.screen]])

    def join_view(self, view: dict) -> bool:
        acts = {a["id"]: a for a in self.actions_for(view)}
        j = acts.get("join")
        if not j or not j["enabled"]:
            self.flash((j or {}).get("reason") or "This session cannot be joined.")
            return False
        ok, why = verify_join(view, self.paths)
        if not ok:
            self.flash(why)
            self.collector.refresh_all()
            return False
        return self.switch_to(view["tmux"], view["name"])

    def switch_to(self, target: str, label: str) -> bool:
        """Move only THIS terminal to `target`. The agent and every other client are untouched."""
        if self.in_tmux:
            r = run(switch_argv(target, own_client()), timeout=10)
            if r.rc != 0:
                self.flash(f"Could not switch: {r.err.strip()[:150]}")
                return False
            self.last_join = (label, time.time())
            return True
        # Outside tmux (--here): attach in this terminal until the user detaches.
        curses.def_prog_mode()
        curses.endwin()
        print(f"\nJoining '{label}'. Leave it running and come back: {self.prefix} then d.\n", flush=True)
        subprocess.call(attach_argv(target))
        curses.reset_prog_mode()
        self.scr.refresh()
        self.last_join = (label, time.time())
        self.flash(f"Back from '{label}'. It is still running.")
        return True

    def fresh_sessions(self) -> tuple[list, list]:
        """Re-read processes, tmux and records right now (not the cached view) for a launch decision."""
        snap = dict(self.snap)
        snap.update(snapmod.collect_fast(self.paths))
        snap["_pr_views"] = self.views.get("prs", [])
        recs = snap["icn_session"].data if snap.get("icn_session") is not None and snap["icn_session"].ok else []
        return session_views(snap, self.paths), recs

    def launch(self, kind: str, subject: dict) -> bool:
        """Resume a recorded conversation or open Claude in a worktree, via icn-session. Asks first."""
        sessions, recs = self.fresh_sessions()
        plan = plan_resume(subject["record"], sessions, self.paths) if kind == "resume" else \
            plan_open_in_worktree(subject, sessions, recs, self.paths)
        if not plan["ok"]:
            self.flash(plan["reason"])
            return False
        if not self.confirm(plan["confirm"] + ["", f"Afterwards: {self.prefix} then L brings you back here."]):
            self.flash("Cancelled. Nothing was started.")
            return False
        # Re-plan once more after the human pause: state may have changed while the prompt was open.
        sessions, recs = self.fresh_sessions()
        again = plan_resume(subject["record"], sessions, self.paths) if kind == "resume" else \
            plan_open_in_worktree(subject, sessions, recs, self.paths)
        if not again["ok"] or again["argv"] != plan["argv"]:
            self.flash("State changed while confirming; nothing was started: " + again.get("reason", "plan changed"))
            return False
        self.scr.erase()
        self.put(1, 2, "Starting through icn-session…", curses.A_BOLD)
        self.scr.refresh()
        r = run(again["argv"], timeout=120)
        if r.rc != 0:
            self.viewer([(f"icn-session exited {r.rc}; nothing else was done.", self.attr(BLOCKED)), ""]
                        + [(line, 0) for line in redact(r.out + r.err).splitlines()], "icn-session output")
            return False
        if run(["tmux", "has-session", "-t", f"={again['tmux_name']}"], timeout=10).rc != 0:
            self.flash(f"icn-session reported success but tmux session '{again['tmux_name']}' is missing.")
            return False
        self.collector.refresh_all()
        return self.switch_to(again["tmux_name"], again["tmux_name"])

    def open_claude_here(self) -> None:
        """Open Claude Code in the selected worktree (screen 4) or one of the selected program's (screen 2)."""
        if self.screen == 3:
            rows = self.items()
            if rows:
                self.launch("open", rows[self.sel[self.screen]])
            return
        if self.screen == 1:
            rows = self.items()
            if not rows:
                return
            prog = rows[self.sel[self.screen]]
            sessions = self.views.get("sessions", [])
            recs = self.snap["icn_session"].data if self.snap.get("icn_session") is not None and self.snap["icn_session"].ok else []
            cands = [w for w in prog["worktrees"] if (w.get("status") or {}).get("exists")]
            if not cands:
                self.flash("This program has no existing worktrees.")
                return
            labels = []
            for w in cands:
                plan = plan_open_in_worktree(w, sessions, recs, self.paths, check_path=lambda p: 0)
                labels.append(f"{w['repo']}/{w['name']}  [{w.get('branch') or 'detached'}]  "
                              + ("can open" if plan["ok"] else "unavailable: " + plan["reason"][:70]))
            choice = self.choose(f"Open Claude Code in a {prog['name']} worktree (the full check runs next)", labels)
            if choice is not None:
                self.launch("open", cands[choice])
            return
        self.flash("Open Claude works on the Worktrees screen (4) or a program on Lanes (2).")

    def open_records(self, paths_: list) -> None:
        cands = [c for c in paths_ if os.path.isfile(c)]
        if not cands:
            self.flash("No record file is associated with this item.")
        elif len(cands) == 1:
            self.open_file(cands[0])
        else:
            choice = self.choose("Open which record?", cands)
            if choice is not None:
                self.open_file(cands[choice])

    def prompt(self, label: str, initial: str = "") -> str:
        h, w = self.scr.getmaxyx()
        self.put(h - 2, 0, " " * w)
        self.put(h - 2, 0, label, curses.A_BOLD)
        curses.echo()
        curses.curs_set(1)
        self.scr.timeout(-1)
        try:
            s = self.scr.getstr(h - 2, len(label), 60).decode(errors="replace")
        except curses.error:
            s = initial
        finally:
            curses.noecho()
            try:
                curses.curs_set(0)
            except curses.error:
                # best-effort: UI preferences and terminal capabilities are optional
                pass
            self.scr.timeout(1000)
        return s.strip()

    def enter_details(self) -> None:
        rows = self.items()
        if self.screen in (0, 6) or not rows:
            return
        x = rows[self.sel[self.screen]]
        if self.screen == 5:
            self.open_file(x["path"])
            return
        fn = {1: None, 2: self.session_detail, 3: self.worktree_detail, 4: self.pr_detail}.get(self.screen)
        if self.screen == 1:
            # reuse the programs detail by drawing it into a viewer
            self.viewer(self._program_detail_lines(x), x["name"])
            return
        self.viewer(fn(x), SCREENS[self.screen])

    def _program_detail_lines(self, p) -> list:
        L = [self.H(p["name"]), f"  {p['summary']}", f"  Owner: {p['owner']}", ("  NEXT: " + p["next"], curses.A_BOLD), ""]
        for title, rows, f in (
                ("Open PRs", p["prs"], lambda x: (f"  {x['repo']}#{x['number']} [{x['status']}] {x['title']} — {x['status_reason']}", self.attr(x["status"]))),
                ("Sessions", p["sessions"], lambda x: (f"  {x['name']} — {x['state_label']} — {x.get('cwd')}", self.attr(x["status"]))),
                ("Worktrees", p["worktrees"], lambda x: f"  {x['verdict'][:9]:9} {x['repo']}/{x['name']} [{x.get('branch')}] {'; '.join(x['keep_reasons'][:3])}"),
                ("Related handoffs", p["handoffs"], lambda x: f"  {ago(x['mtime']):>10}  {x['path']}")):
            L.append(self.H(f"{title} ({len(rows)})"))
            L.extend(f(x) for x in rows)
            L.append("")
        return L

    # ------------------------------------------------------------------ loop
    def handle(self, k: int) -> bool:
        s = self.screen
        rows_n = len(self.items())
        h, _ = self.scr.getmaxyx()
        page = max(1, (h - 5) // 2)
        scrolling = s in (0, 6)
        if k == ord("q"):
            if self.in_tmux and not self.confirm([
                    "Quit the control center?",
                    "Quitting stops it. To leave it running and come back later instead,",
                    f"press {self.prefix} then d (detach). Claude sessions are not affected either way."]):
                return True
            return False
        if k == ord("g"):
            self.session_picker()
        elif ord("1") <= k <= ord("7"):
            self.screen = k - ord("1")
        elif k in (9, curses.KEY_RIGHT):
            self.screen = (s + 1) % len(SCREENS)
        elif k in (curses.KEY_BTAB, curses.KEY_LEFT):
            self.screen = (s - 1) % len(SCREENS)
        elif k in (curses.KEY_DOWN, ord("j")):
            if scrolling:
                self.scroll[s] += 1
            else:
                self.sel[s] = min(self.sel[s] + 1, max(0, rows_n - 1))
        elif k in (curses.KEY_UP, ord("k")) and not (k == ord("k") and s == 3):
            if scrolling:
                self.scroll[s] = max(0, self.scroll[s] - 1)
            else:
                self.sel[s] = max(0, self.sel[s] - 1)
        elif k == curses.KEY_NPAGE:
            if scrolling:
                self.scroll[s] += page
            else:
                self.sel[s] = min(self.sel[s] + page, max(0, rows_n - 1))
        elif k == curses.KEY_PPAGE:
            if scrolling:
                self.scroll[s] = max(0, self.scroll[s] - page)
            else:
                self.sel[s] = max(0, self.sel[s] - page)
        elif k == curses.KEY_HOME:
            self.sel[s], self.scroll[s] = 0, 0
        elif k == curses.KEY_END:
            self.sel[s] = max(0, rows_n - 1)
        elif k in (10, 13, curses.KEY_ENTER):
            if s == 2 and rows_n:
                self.action_menu(self.items()[self.sel[s]])
            else:
                self.enter_details()
        elif k == ord("r"):
            self.collector.refresh_all()
            self.flash("Refreshing all sources (git and GitHub take ~30s)…")
        elif k == ord("o"):
            self.open_selected_record()
        elif k == ord("a"):
            self.join_selected()
        elif k == ord("c"):
            self.open_claude_here()
        elif k == ord("k") and s == 3:
            self.keep_only = not self.keep_only
            self.sel[s] = 0
        elif k == ord("/"):
            if s in (0, 6):
                self.flash("Filtering works on list screens (2-6).")
            else:
                self.filters[s] = self.prompt("Filter (empty clears): ")
                self.sel[s] = 0
        elif k == ord("s"):
            self.viewer(self.sources_lines(), "Data sources")
        elif k == ord("?"):
            self.viewer([(line.replace("PREFIX", self.prefix), 0) for line in HELP.splitlines()], "Help")
        elif k == curses.KEY_RESIZE:
            pass
        return True

    def run(self) -> int:
        try:
            curses.curs_set(0)
        except curses.error:
            # best-effort: UI preferences and terminal capabilities are optional
            pass
        self.init_colors()
        self.scr.keypad(True)
        self.scr.timeout(1000)
        self.collector.start()
        self.refresh_views(force=True)
        while True:
            self.refresh_views()
            self.draw()
            k = self.scr.getch()
            if k == -1:
                continue
            if not self.handle(k):
                break
        self.collector.stop.set()
        self._save_prefs()
        return 0


def run_tui(paths: Paths, catalog: dict) -> int:
    os.environ.setdefault("ESCDELAY", "50")
    return curses.wrapper(lambda scr: UI(scr, paths, catalog).run())
