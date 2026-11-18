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
from .reconcile import build_views, latest_review_generation
from .records import summarize_handoff
from .snapshot import Collector
from .util import (AWAITING_MAINTAINER, BLOCKED, COMPLETED, IDLE, IN_PROGRESS, LIVE, REGISTERED_NOT_RUNNING,
                   STALE, UNKNOWN, WARNING, Paths, ago, clean, fmt_ts, redact, short)

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

Actions
  r                          refresh everything now (otherwise: local every 20s, git and GitHub every 5 min)
  o                          open the selected item's handoff / record in the built-in reader
  a                          attach to the selected session's EXISTING tmux session (asks first)
  /                          filter the current list (empty filter clears)
  k                          Worktrees: toggle 'only worktrees that must be kept'
  s                          show where every piece of data came from and when it was observed
  ?                          this help
  q                          quit (from the top level)

After attaching to an agent's tmux session (PREFIX is read from your tmux config)
  PREFIX then d              detach and leave the agent running
  PREFIX then L (capital)    jump back to this control center (when it runs inside tmux)

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
        self.keep_only = False
        self.message = ""
        self.message_ts = 0.0
        self.started = time.time()
        self.prefix = tmux_prefix()
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
    def items(self) -> list:
        v = self.views
        f = self.filters[self.screen].lower()
        if self.screen == 1:
            rows = v.get("programs", [])
        elif self.screen == 2:
            rows = v.get("sessions", [])
        elif self.screen == 3:
            rows = v.get("worktrees", [])
            if self.keep_only:
                rows = [w for w in rows if w["verdict"] == "KEEP"]
        elif self.screen == 4:
            rows = v.get("prs", [])
        elif self.screen == 5:
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
        foot = " ?:help  1-7/Tab:screens  ↑↓:select  Enter:details  o:open record  a:attach  r:refresh  /:filter  s:sources  q:quit "
        if self.filters[self.screen]:
            foot = f" filter: '{self.filters[self.screen]}' (/ to change) |" + foot
        self.put(h - 1, 0, foot.ljust(w), self.color.get("bar", curses.A_REVERSE))
        if self.message and time.time() - self.message_ts < 6:
            self.put(h - 2, 0, " " + self.message, self.attr(AWAITING_MAINTAINER))
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

    def session_detail(self, s) -> list:
        L = [self.H(f"{s['name']}  —  {s['state_label']}"),
             (f"  Status: {s['status']}    sources: {', '.join(s['sources'])}", self.attr(s["status"])),
             f"  Kind: {s['kind']}   pid: {s.get('pid') or '-'}   tmux: {s.get('tmux') or '-'}"
             f"{' (someone is attached)' if s.get('tmux_attached') else ''}",
             f"  Directory: {s.get('cwd') or '?'}",
             f"  Worktree: {s.get('worktree') or '(not a git worktree)'}   branch: {s.get('branch') or '-'}   head: {short(s.get('head'))}",
             f"  Last activity: {ago(s.get('last_activity'))} ({fmt_ts(s.get('last_activity'))})"
             + (f"   process started {ago(s.get('started'))}" if s.get("started") else ""),
             f"  Task: {s.get('task') or 'unknown'}" + (f"   [{s['task_source']}]" if s.get("task_source") else "")]
        if s.get("pr"):
            L.append(f"  PR: {s['pr']['repo']}#{s['pr']['number']} [{s['pr'].get('status')}] {s['pr']['title'][:80]} ({s['pr']['how']})")
        else:
            L.append("  PR: none associated")
        L.append(f"  Claimed files: {', '.join(s['claims'][:8]) if s.get('claims') else 'none recorded'}")
        if s.get("registry_id"):
            L.append(f"  Registry: {s['registry_id'][:8]} heartbeat {ago(s.get('registry_heartbeat'))}, activity {s.get('registry_activity') or '-'}")
        for hpath in s.get("handoffs") or []:
            L.append(f"  Handoff: {hpath}   (press o)")
        for e in s.get("evidence") or []:
            L.append(f"  Record: {e}")
        if not s.get("handoffs"):
            L.append("  Handoff: none found")
        L.append(("  RESUME: " + s["resume"], curses.A_BOLD))
        if s.get("tmux") and s["kind"] in ("agent", "tmux", "service"):
            L.append(("  Press a to attach to tmux session '" + s["tmux"] + "' (you will be asked to confirm).", curses.A_DIM))
        return L

    def draw_sessions(self, top, bottom, w):
        def fmt(s):
            where = (s.get("branch") or os.path.basename((s.get("cwd") or "?").rstrip("/")))[:34]
            return (f"{s['status'][:22]:22} {s['name'][:36]:36} {s['state_label'][:34]:34} {ago(s.get('last_activity')):>10}  {where}", s["status"])

        self.draw_list(top, bottom, w, "STATUS                 SESSION                              STATE                              ACTIVITY  WHERE",
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
        self.scr.erase()
        h, w = self.scr.getmaxyx()
        for i, line in enumerate(lines):
            self.put(2 + i, 2, line, curses.A_BOLD if i == 0 else 0)
        self.put(3 + len(lines), 2, "Press y to continue, any other key to cancel.", self.attr(AWAITING_MAINTAINER))
        self.scr.refresh()
        self.scr.timeout(-1)
        try:
            return self.scr.getch() in (ord("y"), ord("Y"))
        finally:
            self.scr.timeout(1000)

    def attach_selected(self) -> None:
        if self.screen != 2:
            self.flash("Attach works on the Agents & Sessions screen (3).")
            return
        rows = self.items()
        if not rows:
            return
        s = rows[self.sel[self.screen]]
        name = s.get("tmux")
        if not name:
            self.flash("This session has no tmux session to attach to (see its RESUME line).")
            return
        if s["kind"] == "control-center":
            self.flash("That is this control center.")
            return
        # Re-verify right now: the session must still exist.
        if subprocess.run(["tmux", "has-session", "-t", f"={name}"], capture_output=True).returncode != 0:
            self.flash(f"tmux session '{name}' no longer exists. Press r to refresh.")
            return
        inside = bool(os.environ.get("TMUX"))
        lines = [f"Attach to tmux session '{name}'?",
                 f"It contains: {s['state_label']}",
                 "Anything you type there goes to that session (and to the agent, if one is running)."]
        if s.get("tmux_attached"):
            lines.append("Someone else is already attached to it; you will share the screen.")
        lines.append(f"To leave it running and come back here: {self.prefix} then L (capital L)." if inside
                     else f"To leave it running: {self.prefix} then d. You will return to this screen.")
        if not self.confirm(lines):
            self.flash("Attach cancelled.")
            return
        if inside:
            r = subprocess.run(["tmux", "switch-client", "-t", f"={name}"], capture_output=True, text=True)
            self.flash(f"Switched. {self.prefix} then L returns here." if r.returncode == 0 else f"switch failed: {r.stderr.strip()[:120]}")
        else:
            curses.def_prog_mode()
            curses.endwin()
            subprocess.call(["tmux", "attach-session", "-t", f"={name}"])
            curses.reset_prog_mode()
            self.scr.refresh()
            self.flash(f"Detached from '{name}'.")

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
            return False
        if ord("1") <= k <= ord("7"):
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
            self.enter_details()
        elif k == ord("r"):
            self.collector.refresh_all()
            self.flash("Refreshing all sources (git and GitHub take ~30s)…")
        elif k == ord("o"):
            self.open_selected_record()
        elif k == ord("a"):
            self.attach_selected()
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
