#!/usr/bin/env python3
"""Validate the icn-agent-pack Claude Code plugin skeleton.

This guard keeps the plugin at `tools/claude-code/plugins/icn-agent-pack/`
structurally valid and portable so it can be loaded with
`claude --plugin-dir ./tools/claude-code/plugins/icn-agent-pack` from the repo
root, a subdirectory, or a higher-level directory (the icn-ops MCP server now
launches through a portable wrapper, not a launch-dir-relative path).

It is intentionally narrow, read-only, and dependency-free (standard library
only — no PyYAML). It does NOT build or run anything.

Companion checks (run separately):
    python3 scripts/check-claude-plugin-root-resolution.py   # resolver branch tests
    claude plugin validate ./tools/claude-code/plugins/icn-agent-pack  # Anthropic's validator

Usage:
    python3 scripts/check-claude-plugin.py            # default plugin path
    python3 scripts/check-claude-plugin.py <plugin>   # explicit plugin dir
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterator

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PLUGIN = ROOT / "tools" / "claude-code" / "plugins" / "icn-agent-pack"

# Plugin MCP launch must go through the portable wrapper, never a launch-dir
# relative path or a direct dist entrypoint.
WRAPPER_SUFFIX = "/bin/icn-ops-mcp"
FORBIDDEN_MCP_SUBSTRINGS = ["./ops/mcp", "ops/mcp/dist/index.js", "node dist/index.js"]

REQUIRED_DIRS = ["skills", "agents", "hooks", "bin"]
REQUIRED_BIN = ["icn-find-root", "icn-ops-mcp"]

REQUIRED_SKILLS = ["preflight", "truth-sync", "authority-spine", "route-impact", "navigator", "doctor"]
# Heavy procedural workflows that must disable model auto-invocation.
HEAVY_SKILLS = {"preflight", "truth-sync", "authority-spine", "route-impact", "navigator"}
REQUIRED_AGENTS = [
    "icn-architect",
    "icn-economist",
    "icn-code-reviewer",
    "icn-ops",
    "icn-docs-truth-auditor",
    "icn-navigator",
]

# Docs guide lives at repo root (not inside the plugin).
DOCS_GUIDE_REL = "docs/guides/developer/claude-code-plugin.md"

# ${CLAUDE_PLUGIN_ROOT} or $CLAUDE_PLUGIN_ROOT appearing anywhere in a hook command.
PLUGIN_ROOT_VAR = re.compile(r"\$\{?CLAUDE_PLUGIN_ROOT\}?")
# The script path after the (optionally quoted) plugin-root reference.
PLUGIN_ROOT_SCRIPT = re.compile(r"\$\{?CLAUDE_PLUGIN_ROOT\}?\"?/([^\s\"']+)")
# Accepted, space-safe quoted forms of the plugin-root reference (for shell hooks).
QUOTED_FORMS = ('"${CLAUDE_PLUGIN_ROOT}"', '"$CLAUDE_PLUGIN_ROOT"')


class Checker:
    def __init__(self, plugin: Path) -> None:
        self.plugin = plugin
        self.errors: list[str] = []

    def err(self, message: str) -> None:
        self.errors.append(message)

    def rel(self, path: Path) -> str:
        try:
            return str(path.relative_to(ROOT))
        except ValueError:
            return str(path)

    def load_json(self, path: Path) -> Any:
        if not path.exists():
            self.err(f"missing required file: {self.rel(path)}")
            return None
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            self.err(f"invalid JSON in {self.rel(path)}: {exc}")
            return None

    def parse_frontmatter(self, path: Path) -> dict[str, str] | None:
        """Minimal YAML-frontmatter reader for flat top-level scalar keys.

        Dependency-free: handles the simple `key: value` lines our skills/agents
        use. Returns a dict of top-level keys (quotes stripped from values), or
        None if no `--- ... ---` frontmatter block is present.
        """
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:  # pragma: no cover - unexpected
            self.err(f"cannot read {self.rel(path)}: {exc}")
            return None
        if not text.startswith("---"):
            return None
        lines = text.splitlines()
        end = next((i for i in range(1, len(lines)) if lines[i].strip() == "---"), None)
        if end is None:
            return None
        fm: dict[str, str] = {}
        for line in lines[1:end]:
            m = re.match(r"^([A-Za-z0-9_-]+):\s*(.*)$", line)
            if not m:
                continue
            key, val = m.group(1), m.group(2).strip()
            if len(val) >= 2 and val[0] == val[-1] and val[0] in "\"'":
                val = val[1:-1]
            fm[key] = val
        return fm

    # --- individual checks -------------------------------------------------

    def check_manifest(self) -> None:
        manifest = self.plugin / ".claude-plugin" / "plugin.json"
        data = self.load_json(manifest)
        if data is None:
            return
        if data.get("name") != "icn-agent-pack":
            self.err(
                f"{self.rel(manifest)}: 'name' must be 'icn-agent-pack' "
                f"(found {data.get('name')!r})"
            )
        if not data.get("version"):
            self.err(f"{self.rel(manifest)}: missing 'version'")
        if not data.get("description"):
            self.err(f"{self.rel(manifest)}: missing 'description'")

    def check_claude_plugin_dir(self) -> None:
        """.claude-plugin/ must contain ONLY plugin.json."""
        cp = self.plugin / ".claude-plugin"
        if not cp.is_dir():
            self.err(f"missing required directory: {self.rel(cp)}/")
            return
        entries = sorted(p.name for p in cp.iterdir())
        if entries != ["plugin.json"]:
            extra = [e for e in entries if e != "plugin.json"]
            self.err(
                f"{self.rel(cp)}/ must contain only plugin.json "
                f"(unexpected entries: {extra}; components belong at the plugin root)"
            )

    def check_required_dirs(self) -> None:
        for name in REQUIRED_DIRS:
            d = self.plugin / name
            if not d.is_dir():
                self.err(f"missing required directory: {self.rel(d)}/")

    def check_bin(self) -> None:
        for name in REQUIRED_BIN:
            f = self.plugin / "bin" / name
            if not f.is_file():
                self.err(f"missing required helper: {self.rel(f)}")
            elif not os.access(f, os.X_OK):
                self.err(f"helper not executable: {self.rel(f)} (chmod +x it)")

    def check_bin_syntax(self) -> None:
        """Syntax-check every FILE in bin/ by its shebang.

        Iterates files only, so directories (e.g. a stray __pycache__) are
        ignored. Python files are checked with the builtin compile() — which
        writes no .pyc cache — and shell files with `sh -n`.
        """
        bin_dir = self.plugin / "bin"
        if not bin_dir.is_dir():
            return
        for f in sorted(bin_dir.iterdir()):
            if not f.is_file():
                continue  # skip __pycache__ and any other directory
            try:
                text = f.read_text(encoding="utf-8", errors="replace")
            except OSError as exc:  # pragma: no cover - unexpected
                self.err(f"cannot read {self.rel(f)}: {exc}")
                continue
            first_line = text.splitlines()[0] if text else ""
            if "python" in first_line:
                try:
                    compile(text, str(f), "exec")  # in-memory only; writes no cache
                except SyntaxError as exc:
                    self.err(f"python syntax error in {self.rel(f)}: {exc}")
            else:
                try:
                    proc = subprocess.run(
                        ["sh", "-n", str(f)],
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        text=True,
                    )
                except OSError:
                    continue  # no `sh` available to check with; skip rather than fail
                if proc.returncode != 0:
                    self.err(f"shell syntax error in {self.rel(f)}: {proc.stderr.strip()}")

    def check_skills(self) -> None:
        for skill in REQUIRED_SKILLS:
            skill_md = self.plugin / "skills" / skill / "SKILL.md"
            if not skill_md.is_file():
                self.err(f"missing required skill file: {self.rel(skill_md)}")
                continue
            fm = self.parse_frontmatter(skill_md)
            if fm is None:
                self.err(f"{self.rel(skill_md)}: missing or malformed YAML frontmatter")
                continue
            if not fm.get("name"):
                self.err(f"{self.rel(skill_md)}: frontmatter missing 'name'")
            if not fm.get("description"):
                self.err(f"{self.rel(skill_md)}: frontmatter missing 'description'")
            # Heavy procedural workflows (and any user-invocable skill, e.g. doctor)
            # must disable model auto-invocation.
            needs_disable = skill in HEAVY_SKILLS or fm.get("user-invocable") == "true"
            if needs_disable and fm.get("disable-model-invocation") != "true":
                self.err(f"{self.rel(skill_md)}: must set 'disable-model-invocation: true'")

    def check_agents(self) -> None:
        for agent in REQUIRED_AGENTS:
            agent_md = self.plugin / "agents" / f"{agent}.md"
            if not agent_md.is_file():
                self.err(f"missing required agent: {self.rel(agent_md)}")
                continue
            fm = self.parse_frontmatter(agent_md)
            if fm is None:
                self.err(f"{self.rel(agent_md)}: missing or malformed YAML frontmatter")
                continue
            if not fm.get("name"):
                self.err(f"{self.rel(agent_md)}: frontmatter missing 'name'")
            if not fm.get("description"):
                self.err(f"{self.rel(agent_md)}: frontmatter missing 'description'")

    def check_readme(self) -> None:
        if not (self.plugin / "README.md").is_file():
            self.err(f"missing required file: {self.rel(self.plugin / 'README.md')}")

    def check_docs_guide(self) -> None:
        guide = ROOT / DOCS_GUIDE_REL
        if not guide.is_file():
            self.err(f"missing docs guide: {DOCS_GUIDE_REL}")

    def check_mcp(self) -> None:
        mcp = self.plugin / ".mcp.json"
        data = self.load_json(mcp)
        if data is None:
            return
        raw = mcp.read_text(encoding="utf-8")
        for bad in FORBIDDEN_MCP_SUBSTRINGS:
            if bad in raw:
                self.err(
                    f"{self.rel(mcp)}: must not contain {bad!r} "
                    "(launch via the portable ${CLAUDE_PLUGIN_ROOT}/bin/icn-ops-mcp wrapper)"
                )
        servers = data.get("mcpServers")
        if not isinstance(servers, dict) or "icn-ops" not in servers:
            self.err(f"{self.rel(mcp)} must define mcpServers.icn-ops")
            return
        server = servers["icn-ops"]
        cmd = server.get("command")
        if (
            not isinstance(cmd, str)
            or "CLAUDE_PLUGIN_ROOT" not in cmd
            or not cmd.rstrip().endswith(WRAPPER_SUFFIX)
        ):
            self.err(
                f"{self.rel(mcp)}: icn-ops must launch via "
                "'${CLAUDE_PLUGIN_ROOT}/bin/icn-ops-mcp' "
                f"(found command={cmd!r})"
            )

    def check_lsp(self) -> None:
        lsp = self.plugin / ".lsp.json"
        if not lsp.exists():
            return  # .lsp.json is optional
        data = self.load_json(lsp)
        if data is None:
            return
        if not isinstance(data, dict):
            self.err(f"{self.rel(lsp)}: must be a JSON object ('{{}}' placeholder or LSP server map)")
            return
        if data == {}:
            return  # documented empty placeholder is valid
        # Standalone .lsp.json is a direct {name: config} map; tolerate a
        # {"lspServers": {...}} wrapper (the inline-in-plugin.json shape).
        servers = data["lspServers"] if isinstance(data.get("lspServers"), dict) else data
        if not isinstance(servers, dict) or not servers:
            self.err(f"{self.rel(lsp)}: expected '{{}}' or a non-empty map of LSP server configs")
            return
        for name, cfg in servers.items():
            if not isinstance(cfg, dict):
                self.err(f"{self.rel(lsp)}: server {name!r} must be an object")
                continue
            if not isinstance(cfg.get("command"), str) or not cfg.get("command"):
                self.err(f"{self.rel(lsp)}: server {name!r} missing required string 'command'")
            if not isinstance(cfg.get("extensionToLanguage"), dict) or not cfg.get("extensionToLanguage"):
                self.err(
                    f"{self.rel(lsp)}: server {name!r} missing required object 'extensionToLanguage'"
                )

    def check_hooks(self) -> Any:
        hooks_json = self.plugin / "hooks" / "hooks.json"
        data = self.load_json(hooks_json)  # errors if missing
        if data is None:
            return None
        if not isinstance(data.get("hooks"), dict):
            self.err(f"{self.rel(hooks_json)}: must have a top-level 'hooks' object")
        if "description" in data:
            self.err(
                f"{self.rel(hooks_json)}: drop the top-level 'description' — it is not a documented "
                "plugin hooks field (move notes to README to keep the config schema-clean)"
            )
        return data

    @staticmethod
    def _iter_hook_commands(hooks_data: Any) -> Iterator[str]:
        """Yield every command string from a command-type hook in hooks.json."""
        if not isinstance(hooks_data, dict):
            return
        events = hooks_data.get("hooks")
        if not isinstance(events, dict):
            return
        for groups in events.values():
            if not isinstance(groups, list):
                continue
            for group in groups:
                if not isinstance(group, dict):
                    continue
                for hook in group.get("hooks", []) or []:
                    if isinstance(hook, dict) and hook.get("type") == "command":
                        cmd = hook.get("command")
                        if isinstance(cmd, str):
                            yield cmd

    def check_hook_commands(self, hooks_data: Any) -> None:
        """Hook commands referencing the plugin root must quote it and point at real scripts."""
        hooks_json = self.plugin / "hooks" / "hooks.json"
        for cmd in self._iter_hook_commands(hooks_data):
            if not PLUGIN_ROOT_VAR.search(cmd):
                continue
            if not any(q in cmd for q in QUOTED_FORMS):
                self.err(
                    f"{self.rel(hooks_json)}: unquoted CLAUDE_PLUGIN_ROOT in command {cmd!r} "
                    '(quote it, e.g. "\\"${CLAUDE_PLUGIN_ROOT}\\"/bin/..." to be space-safe)'
                )
            m = PLUGIN_ROOT_SCRIPT.search(cmd)
            if not m:
                continue
            script = self.plugin / m.group(1)
            if not script.exists():
                self.err(f"hook references missing script: {self.rel(script)}")
            elif not os.access(script, os.X_OK):
                self.err(f"hook script is not executable: {self.rel(script)} (chmod +x it)")

    # --- driver ------------------------------------------------------------

    def run(self) -> int:
        if not self.plugin.is_dir():
            self.err(f"plugin directory not found: {self.rel(self.plugin)}")
            return self.report()
        self.check_manifest()
        self.check_claude_plugin_dir()
        self.check_required_dirs()
        self.check_bin()
        self.check_bin_syntax()
        self.check_skills()
        self.check_agents()
        self.check_readme()
        self.check_docs_guide()
        self.check_mcp()
        self.check_lsp()
        hooks_data = self.check_hooks()
        self.check_hook_commands(hooks_data)
        return self.report()

    def report(self) -> int:
        if self.errors:
            print("Claude plugin check FAILED:", file=sys.stderr)
            for e in self.errors:
                print(f"  - {e}", file=sys.stderr)
            return 1
        print(f"Claude plugin check passed: {self.rel(self.plugin)}")
        print("  also run: python3 scripts/check-claude-plugin-root-resolution.py")
        print("  also run: claude plugin validate ./tools/claude-code/plugins/icn-agent-pack")
        return 0


def main() -> None:
    plugin = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else DEFAULT_PLUGIN
    raise SystemExit(Checker(plugin).run())


if __name__ == "__main__":
    main()
