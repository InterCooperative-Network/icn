#!/usr/bin/env python3
"""Check repo MCP configs for contributor-portable wiring.

This guard is intentionally narrow. It protects checked-in MCP config from
accidentally depending on one developer's workstation, VM, WSL path, homelab
host, or environment variable override.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]

CONFIGS = [
    ROOT / ".mcp.json",
    ROOT / ".cursor" / "mcp.json",
]

# Every MCP-capable client must start icn-ops the same way so `tsc` + `node`
# run under one Node, and postinstall keeps better-sqlite3 aligned with that Node.
ICN_OPS_COMMAND = "npm"
ICN_OPS_ARGS = ["--prefix", "./ops/mcp", "run", "start:stdio"]

FORBIDDEN_PATTERNS = [
    re.compile(r"/home/[^\s\"']+"),
    re.compile(r"/Users/[^\s\"']+"),
    re.compile(r"[A-Za-z]:\\\\Users\\\\"),
    re.compile(r"\\\\wsl\$", re.IGNORECASE),
    re.compile(r"10\.8\.30\."),
    re.compile(r"100\.106\.184\.21"),
]


def fail(message: str) -> None:
    print(f"MCP portability check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        fail(f"missing required config: {path.relative_to(ROOT)}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON in {path.relative_to(ROOT)}: {exc}")


def assert_no_forbidden_machine_paths(path: Path) -> None:
    text = path.read_text(encoding="utf-8")
    for pattern in FORBIDDEN_PATTERNS:
        match = pattern.search(text)
        if match:
            fail(
                f"{path.relative_to(ROOT)} contains machine-specific value {match.group(0)!r}"
            )


def assert_mcp_shape(path: Path, data: dict[str, Any]) -> dict[str, Any]:
    servers = data.get("mcpServers")
    if not isinstance(servers, dict):
        fail(f"{path.relative_to(ROOT)} must contain object key 'mcpServers'")
    server = servers.get("icn-ops")
    if not isinstance(server, dict):
        fail(f"{path.relative_to(ROOT)} must define mcpServers.icn-ops")
    if "env" in server and isinstance(server["env"], dict) and "ICN_ROOT" in server["env"]:
        fail(f"{path.relative_to(ROOT)} must not hard-code ICN_ROOT")
    return server


def main() -> None:
    for path in CONFIGS:
        assert_no_forbidden_machine_paths(path)
        data = load_json(path)
        server = assert_mcp_shape(path, data)

        if server.get("command") != ICN_OPS_COMMAND:
            fail(
                f"{path.relative_to(ROOT)} must launch icn-ops with command {ICN_OPS_COMMAND!r}"
            )
        if server.get("args") != ICN_OPS_ARGS:
            fail(
                f"{path.relative_to(ROOT)} icn-ops args must be {ICN_OPS_ARGS!r} "
                "(keep .mcp.json and .cursor/mcp.json identical for all MCP clients)"
            )

    print("MCP portability check passed")


if __name__ == "__main__":
    main()
