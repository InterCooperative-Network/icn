#!/usr/bin/env python3
"""Loader for the meaning-firewall taxonomy.

scripts/firewall-taxonomy.toml is the single AUTHORITATIVE DECLARATION of crate
classes and boundary exceptions. Enforcement mechanisms either load it at
runtime through this module (so they cannot drift) or hold mirrored copies that
test_firewall_taxonomy.py drift-checks in CI. See the taxonomy file header for
the exact ownership map.

CLI:
    python3 .github/scripts/firewall_taxonomy.py --bash
        Emit bash array definitions (KERNEL_CRATES, FORBIDDEN, ALLOWED_EDGES)
        for the ci.yml kernel-deps step.
    python3 .github/scripts/firewall_taxonomy.py --summary
        Human-readable dump.

Env:
    ICN_FIREWALL_TAXONOMY — override the taxonomy path (used by the
    failure-path tests to run the real checkers against doctored fixtures;
    never set in CI enforcement jobs).
"""

from __future__ import annotations

import os
import re
import sys
import tomllib
from pathlib import Path

# Repo root = two levels up from this file (.github/scripts/ -> repo root)
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
TAXONOMY_PATH = Path(
    os.environ.get(
        "ICN_FIREWALL_TAXONOMY", REPO_ROOT / "scripts" / "firewall-taxonomy.toml"
    )
)

REQUIRED_CLASSES = (
    "kernel",
    "substrate",
    "api-shell",
    "domain",
    "app",
    "facade",
    "tool",
    "bin",
)
EXCEPTION_KINDS = ("direct", "transitive-or-dev")


class TaxonomyError(RuntimeError):
    pass


def load(path: Path | None = None) -> dict:
    """Load and validate the taxonomy. Returns the parsed dict."""
    p = path or TAXONOMY_PATH
    try:
        with open(p, "rb") as f:
            data = tomllib.load(f)
    except FileNotFoundError as e:
        raise TaxonomyError(f"taxonomy file missing: {p}") from e
    except tomllib.TOMLDecodeError as e:
        raise TaxonomyError(f"taxonomy file unparseable: {p}: {e}") from e

    classes = data.get("classes", {})
    for cls in REQUIRED_CLASSES:
        if cls not in classes or not isinstance(classes[cls], list):
            raise TaxonomyError(f"taxonomy missing class list: [classes].{cls}")

    # A crate must appear in exactly one class, and names must be shell-safe:
    # the --bash emitter's output is eval'd by two consumers, so restrict the
    # charset rather than trusting quoting alone.
    name_re = re.compile(r"^[A-Za-z0-9_-]+$")
    seen: dict[str, str] = {}
    for cls in REQUIRED_CLASSES:
        for crate in classes[cls]:
            if not name_re.match(crate):
                raise TaxonomyError(f"invalid crate name (shell-unsafe): {crate!r}")
            if crate in seen:
                raise TaxonomyError(
                    f"crate '{crate}' classified twice: {seen[crate]} and {cls}"
                )
            seen[crate] = cls

    denylisted = set(classes["domain"]) | set(classes["app"])
    exception_edges: set[tuple[str, str]] = set()
    for exc in data.get("exception", []):
        for field in ("from", "to", "kind", "tracking", "expiry"):
            if field not in exc:
                raise TaxonomyError(f"exception entry missing '{field}': {exc}")
        if exc["kind"] not in EXCEPTION_KINDS:
            raise TaxonomyError(
                f"unsupported exception kind '{exc['kind']}' "
                f"(expected one of {', '.join(EXCEPTION_KINDS)}): {exc}"
            )
        if exc["expiry"] != "edge-absent":
            raise TaxonomyError(
                f"unsupported expiry '{exc['expiry']}' (only 'edge-absent'): {exc}"
            )
        if exc["from"] not in classes["kernel"]:
            raise TaxonomyError(
                f"exception 'from' must be a kernel crate (got {exc['from']})"
            )
        if exc["to"] not in denylisted:
            raise TaxonomyError(
                f"exception 'to' must be a domain/app crate (got {exc['to']})"
            )
        edge = (exc["from"], exc["to"])
        if edge in exception_edges:
            raise TaxonomyError(f"duplicate exception edge: {exc['from']} -> {exc['to']}")
        exception_edges.add(edge)
    return data


def kernel_crates(data: dict) -> list[str]:
    return list(data["classes"]["kernel"])


def enforced_crates(data: dict) -> list[str]:
    """Crates under the no-domain/app-deps prohibition: kernel + substrate."""
    return list(data["classes"]["kernel"]) + list(data["classes"]["substrate"])


def all_classified(data: dict) -> set[str]:
    out: set[str] = set()
    for cls in REQUIRED_CLASSES:
        out |= set(data["classes"][cls])
    return out


def tool_crates(data: dict) -> set[str]:
    return set(data["classes"]["tool"])


def unrestricted_crates(data: dict) -> set[str]:
    """Classes with no dependency restrictions (tool + bin)."""
    return set(data["classes"]["tool"]) | set(data["classes"]["bin"])


def denylisted_crates(data: dict) -> list[str]:
    """Crates that kernel-class crates must not depend on: domain + app."""
    return list(data["classes"]["domain"]) + list(data["classes"]["app"])


def exceptions(data: dict) -> set[tuple[str, str]]:
    return {(e["from"], e["to"]) for e in data.get("exception", [])}


def exception_entries(data: dict) -> list[dict]:
    return list(data.get("exception", []))


def shell_pins(data: dict) -> dict[str, dict[str, set[str]]]:
    out = {}
    for shell, pins in data.get("shells", {}).items():
        out[shell] = {
            "domain": set(pins.get("domain", [])),
            "app": set(pins.get("app", [])),
        }
    return out


def emit_bash(data: dict) -> str:
    """Bash lists for the ci.yml kernel-deps step (direct-edge scope).

    Every element is double-quoted: edge entries contain `->`, which bash
    would otherwise parse as a redirection.
    """

    def arr(items) -> str:
        return " ".join(f'"{i}"' for i in items)

    kernel = arr(kernel_crates(data))
    forbidden = arr(denylisted_crates(data))
    allowed = arr(f"{f}->{t}" for f, t in sorted(exceptions(data)))
    return (
        f"KERNEL_CRATES=({kernel})\n"
        f"FORBIDDEN=({forbidden})\n"
        f"ALLOWED_EDGES=({allowed})\n"
    )


def main() -> int:
    data = load()
    if "--bash" in sys.argv:
        sys.stdout.write(emit_bash(data))
        return 0
    # --summary / default
    print(f"taxonomy {data['meta']['version']} ({data['meta']['updated']})")
    for cls in REQUIRED_CLASSES:
        print(f"  {cls}: {', '.join(data['classes'][cls])}")
    print(f"  exceptions: {len(data.get('exception', []))}")
    for shell, pins in shell_pins(data).items():
        print(f"  shell {shell}: {len(pins['domain'])} domain / {len(pins['app'])} app pins")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except TaxonomyError as e:
        print(f"TAXONOMY ERROR: {e}", file=sys.stderr)
        sys.exit(2)
