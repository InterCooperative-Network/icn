"""Small shared helpers: status vocabulary, observations, bounded subprocess calls."""

from __future__ import annotations

import os
import re
import subprocess
import time
from dataclasses import dataclass, field
from typing import Any, Optional

# ---------------------------------------------------------------------------
# Status vocabulary. Every status shown to the maintainer is one of these.
# UNKNOWN is the default: missing information must never render as success.
# ---------------------------------------------------------------------------
LIVE = "VERIFIED LIVE"
REGISTERED_NOT_RUNNING = "REGISTERED, NOT RUNNING"
COMPLETED = "COMPLETED"
BLOCKED = "BLOCKED"
AWAITING_MAINTAINER = "AWAITING MAINTAINER"
STALE = "STALE"
UNKNOWN = "UNKNOWN"
IN_PROGRESS = "IN PROGRESS"
IDLE = "IDLE"
WARNING = "WARNING"  # host-health thresholds only; never used for work items

STATUSES = (LIVE, IN_PROGRESS, IDLE, WARNING, AWAITING_MAINTAINER, BLOCKED, REGISTERED_NOT_RUNNING,
            STALE, COMPLETED, UNKNOWN)


@dataclass
class Obs:
    """The result of reading one source: data plus where/when it came from."""

    source: str
    observed_at: float
    ok: bool
    data: Any = None
    error: Optional[str] = None
    from_cache: bool = False
    duration_s: float = 0.0

    def age_s(self, now: Optional[float] = None) -> float:
        return (now or time.time()) - self.observed_at

    def to_json(self) -> dict:
        return {"source": self.source, "observed_at": self.observed_at, "ok": self.ok,
                "data": self.data, "error": self.error, "duration_s": round(self.duration_s, 3)}

    @classmethod
    def from_json(cls, d: dict) -> "Obs":
        return cls(source=d.get("source", "?"), observed_at=float(d.get("observed_at", 0)),
                   ok=bool(d.get("ok")), data=d.get("data"), error=d.get("error"),
                   from_cache=True, duration_s=float(d.get("duration_s", 0)))


def observe(source: str, fn, *args, **kwargs) -> Obs:
    """Run one adapter; an exception becomes an explicit failed observation."""
    t0 = time.time()
    try:
        data = fn(*args, **kwargs)
        return Obs(source=source, observed_at=t0, ok=True, data=data, duration_s=time.time() - t0)
    except Exception as exc:  # adapters must never take the dashboard down
        return Obs(source=source, observed_at=t0, ok=False, error=redact(f"{type(exc).__name__}: {exc}"),
                   duration_s=time.time() - t0)


@dataclass
class CmdResult:
    rc: int
    out: str
    err: str
    timed_out: bool = False


def run(argv: list[str], timeout: float = 20.0, cwd: Optional[str] = None,
        env: Optional[dict] = None) -> CmdResult:
    """Run a command without a shell, bounded by a timeout. Never raises for rc != 0."""
    full_env = dict(os.environ)
    full_env.setdefault("GIT_OPTIONAL_LOCKS", "0")  # never take index.lock in someone's worktree
    full_env["GIT_TERMINAL_PROMPT"] = "0"
    full_env["GH_PROMPT_DISABLED"] = "1"
    if env:
        full_env.update(env)
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, cwd=cwd,
                           env=full_env, stdin=subprocess.DEVNULL)
        return CmdResult(p.returncode, p.stdout, p.stderr)
    except subprocess.TimeoutExpired as exc:
        out = exc.stdout.decode() if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        return CmdResult(124, out, f"timed out after {timeout}s", timed_out=True)
    except FileNotFoundError:
        return CmdResult(127, "", f"{argv[0]}: not found")


_SECRETISH = re.compile(
    r"(gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}"
    r"|(?i:(?:token|password|secret|api[_-]?key)\s*[=:]\s*)\S+)")


_CONTROL = re.compile(r"[\x00-\x08\x0b-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]")


def clean(text) -> str:
    """Neutralize terminal control sequences and bidi overrides in externally sourced text.

    PR titles, branch names, review bodies and handoff files are not trusted to be inert.
    Tab and newline are kept; every other C0/C1 control and bidi override becomes '?'.
    """
    if text is None:
        return ""
    return _CONTROL.sub("?", str(text))


def redact(text: str) -> str:
    """Belt-and-braces scrub of token-shaped strings from anything we display."""
    if not text:
        return text
    return _SECRETISH.sub("<REDACTED>", text)


def ago(ts: Optional[float], now: Optional[float] = None) -> str:
    """Human 'how long ago' for an epoch timestamp; '?' when unknown."""
    if ts is None or ts <= 0:
        return "?"
    d = int((now or time.time()) - ts)
    if d < 0:
        return "just now"
    if d < 60:
        return f"{d}s ago"
    if d < 3600:
        return f"{d // 60}m ago"
    if d < 86400:
        return f"{d // 3600}h {d % 3600 // 60}m ago"
    return f"{d // 86400}d ago"


def fmt_ts(ts: Optional[float]) -> str:
    if not ts:
        return "?"
    return time.strftime("%Y-%m-%d %H:%M:%SZ", time.gmtime(ts))


def parse_iso(s: Optional[str]) -> Optional[float]:
    """Parse ISO-8601 / sqlite 'YYYY-MM-DD HH:MM:SS' (UTC) into epoch seconds."""
    if not s:
        return None
    s = s.strip().replace("T", " ").rstrip("Z")
    s = re.sub(r"[+-]00:?00$", "", s)
    s = s.split(".")[0]
    try:
        import calendar
        return float(calendar.timegm(time.strptime(s, "%Y-%m-%d %H:%M:%S")))
    except ValueError:
        try:
            import calendar
            return float(calendar.timegm(time.strptime(s, "%Y-%m-%d")))
        except ValueError:
            return None


def short(sha: Optional[str], n: int = 9) -> str:
    return (sha or "")[:n] or "-"


def home() -> str:
    return os.path.expanduser("~")


@dataclass
class Paths:
    """Host locations. Everything is overridable so tests can point at fixtures."""

    icn_dev: str = field(default_factory=lambda: os.environ.get("ICN_DEV_ROOT", os.path.join(home(), "icn-dev")))
    claude_home: str = field(default_factory=lambda: os.environ.get("ICN_CONTROL_CLAUDE_HOME", os.path.join(home(), ".claude")))
    icn_session_state: str = field(default_factory=lambda: os.environ.get(
        "ICN_SESSION_STATE_DIR", os.path.join(os.environ.get("XDG_STATE_HOME", os.path.join(home(), ".local", "state")), "icn-session")))
    control_state: str = field(default_factory=lambda: os.environ.get(
        "ICN_CONTROL_STATE_DIR", os.path.join(os.environ.get("XDG_STATE_HOME", os.path.join(home(), ".local", "state")), "icn-control")))
    proc: str = "/proc"

    @property
    def repos(self) -> str:
        return os.path.join(self.icn_dev, "repos")

    @property
    def state(self) -> str:
        return os.path.join(self.icn_dev, "state")

    @property
    def handoffs(self) -> str:
        return os.path.join(self.icn_dev, "handoffs")

    @property
    def worktrees(self) -> str:
        return os.path.join(self.icn_dev, "worktrees")

    def registry_db(self, repo_key: str = "icn") -> str:
        return os.environ.get("ICN_OPS_DB", os.path.join(self.repos, f"{repo_key}.git", "icn-ops.db"))
