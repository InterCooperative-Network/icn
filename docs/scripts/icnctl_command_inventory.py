#!/usr/bin/env python3
"""Generate (or --check) a role-based, claim-disciplined inventory of `icnctl` commands.

Mechanically parses the `clap` derive command tree under `icn/bins/icnctl/src/**`
(`#[derive(Subcommand)] enum …`, plus the top-level `#[derive(Parser)]` -> `Commands`
enum) and emits the full command-path list with source file:line. Issue #2113.

What is mechanical (drift-checked) vs curated:
- **Mechanical:** the set of commands, their subcommand paths, and source file:line —
  derived from the clap enums. Counts never hand-maintained.
- **Curated (needs review):** the `role` column is a small, explicit top-level-group ->
  role map (a navigation heuristic, NOT derived from clap and asserting no usability or
  safety). The `status` column is uniformly `unknown / needs local verification`: a
  static scan proves a command is **declared**, not that it is implemented / wired to a
  live gateway / fixture-only / production-ready. Distinguishing
  live/partial/fixture/planned per command is a human/runtime follow-up.
- **Proof level** is capped at **L1** (a command declaration exists in source) per
  `proof-level-taxonomy-capability-matrix.md`. A static scan cannot assert higher.

Usage:
    python3 docs/scripts/icnctl_command_inventory.py --write    # regenerate artifact
    python3 docs/scripts/icnctl_command_inventory.py --check     # exit 1 if stale
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def repo_root() -> Path:
    try:
        top = subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=Path(__file__).resolve().parent,
            text=True,
        ).strip()
        return Path(top)
    except Exception:
        return Path(__file__).resolve().parents[2]


ROOT = repo_root()
ICNCTL_SRC = ROOT / "icn" / "bins" / "icnctl" / "src"
ARTIFACT = ROOT / "docs" / "reference" / "project-index" / "generated" / "icnctl-command-inventory.md"
SCRIPT_REL = "docs/scripts/icnctl_command_inventory.py"

# The top-level clap command enum (the `#[command(subcommand)] command: Commands` field
# of the `#[derive(Parser)]` struct).
TOP_ENUM = "Commands"

# Curated, top-level-group -> role navigation heuristic (issue #2113). This is NOT
# mechanically derived and asserts nothing about usability, safety, or readiness — it is
# a "where would I look first" aid, and every entry is needs-review. Uncurated groups
# fall back to `unknown`. Roles use the #2113 vocabulary: organizer / operator /
# developer / agent / maintainer / unknown.
ROLE_BY_GROUP: dict[str, str] = {
    "status": "operator",
    "id": "developer",
    "device": "developer",
    "recovery": "developer",
    "trust": "operator",
    "ledger": "developer",
    "dispute": "organizer",
    "contract": "developer",
    "network": "operator",
    "federation": "operator",
    "gov": "organizer",
    "institution": "organizer",
    "backup": "operator",
    "restore": "operator",
    "verify-backup": "operator",
    "snapshot": "operator",
    "init-coop": "organizer",
    "coop": "maintainer",
    "auth": "developer",
    "compute": "developer",
    "policy": "operator",
    "quota": "operator",
    "steward": "operator",
    "commons": "organizer",
    "charter": "organizer",
    "amendment": "organizer",
    "appeal": "organizer",
    "api": "developer",
    "receipts": "developer",
    "audit": "operator",
    "registry": "developer",
    "preflight": "operator",
    "completions": "developer",
}

UNIFORM_STATUS = "unknown / needs local verification"
PROOF_LEVEL = "L1"

CFG_RE = re.compile(r"^\s*#\[cfg\((.+)\)\]\s*$")
ENUM_RE = re.compile(r"#\[derive\([^)]*\bSubcommand\b[^)]*\)\]")
ENUM_DECL_RE = re.compile(r"^\s*(?:pub\s+)?enum\s+([A-Za-z_]\w*)\s*\{")
# A variant at enum top level: `Name`, `Name,`, `Name {`, or `Name(Type)`. Group 3 is the
# tuple-delegation type (if any); group 4 distinguishes a struct variant (`{`).
VARIANT_RE = re.compile(r"^    ([A-Z][A-Za-z0-9]*)\s*(\(([^)]*)\))?\s*(\{|,|$)")
SUBCMD_ATTR = "#[command(subcommand)]"
FIELD_TYPE_RE = re.compile(r"^\s*(?:pub\s+)?\w+\s*:\s*([A-Za-z_][\w:]*)")


def camel_to_kebab(name: str) -> str:
    s = re.sub(r"(?<!^)(?=[A-Z])", "-", name)
    return s.lower()


def struct_variant_delegation(lines: list[str], start: int) -> str | None:
    """A struct-style variant `Name { … }` delegates to a sub-enum when its body has a
    `#[command(subcommand)]` attribute on a field, e.g.
    `Bootstrap { #[command(subcommand)] command: InstitutionBootstrapCommands }`.
    Returns the field's type (final `::` segment), or None. `start` is the variant line
    (which carries the opening `{`)."""
    depth = 0
    want = False
    n = len(lines)
    k = start
    while k < n:
        line = lines[k]
        if SUBCMD_ATTR in line:
            want = True
        elif want and line.strip():
            fm = FIELD_TYPE_RE.match(line)
            if fm:
                return fm.group(1).split("::")[-1]
            if not line.strip().startswith("#"):
                want = False  # not a subcommand field after all
        depth += line.count("{") - line.count("}")
        if depth <= 0 and k > start:
            return None
        k += 1
    return None


def parse_enums(text: str, rel: str) -> dict[str, list[dict]]:
    """Parse every `#[derive(...Subcommand...)] enum X { … }`. Returns
    {enum_name: [{variant, delegates_to|None, line, file}]}. Brace-depth tracked so nested
    braces in variant bodies don't end the enum early; struct variants are inspected for a
    nested `#[command(subcommand)]` field delegation."""
    out: dict[str, list[dict]] = {}
    lines = text.splitlines()
    i = 0
    n = len(lines)
    while i < n:
        if ENUM_RE.search(lines[i]):
            # Find the `enum X {` within the next few lines (skip other attrs).
            j = i + 1
            enum_name = None
            while j < min(i + 6, n):
                dm = ENUM_DECL_RE.match(lines[j])
                if dm:
                    enum_name = dm.group(1)
                    break
                j += 1
            if enum_name is None:
                i += 1
                continue
            variants: list[dict] = []
            depth = 0
            started = False
            pending_cfg: str | None = None  # a `#[cfg(...)]` seen since the last variant
            k = j
            while k < n:
                line = lines[k]
                if not started:
                    depth += line.count("{")
                    started = depth > 0
                    depth -= line.count("}")
                    k += 1
                    continue
                # Only treat a line as a variant when at the enum's own brace depth (1).
                if depth == 1:
                    cm = CFG_RE.match(line)
                    if cm:
                        # `#[cfg(...)]` on a variant (e.g. `#[cfg(feature = "post-quantum")]`)
                        # means the command only exists when that cfg is active. Attach it to
                        # the next variant. (Field-level cfgs live at depth 2 and are ignored.)
                        pending_cfg = cm.group(1).strip()
                    else:
                        vm = VARIANT_RE.match(line)
                        if vm:
                            delegates = None
                            if vm.group(3):
                                # `Name(Type)` — strip module path, take the final segment.
                                delegates = vm.group(3).strip().split("::")[-1]
                            elif vm.group(4) == "{":
                                # Struct variant — may delegate via a nested subcommand field.
                                delegates = struct_variant_delegation(lines, k)
                            variants.append(
                                {"variant": vm.group(1), "delegates_to": delegates,
                                 "line": k + 1, "file": rel, "cfg": pending_cfg}
                            )
                            pending_cfg = None
                depth += line.count("{") - line.count("}")
                if depth <= 0:
                    break
                k += 1
            out[enum_name] = variants
            i = k + 1
            continue
        i += 1
    return out


def build_tree() -> tuple[list[dict], list[str]]:
    """Return (leaf_commands, unparsed_notes). Leaf = a variant that does not delegate
    to a parsed Subcommand enum. Path = kebab(ancestors) joined by space."""
    enums: dict[str, list[dict]] = {}
    if not ICNCTL_SRC.is_dir():
        return [], [f"icnctl src not found at {ICNCTL_SRC}"]
    for rs in sorted(ICNCTL_SRC.rglob("*.rs")):
        rel = rs.relative_to(ROOT).as_posix()
        parsed = parse_enums(rs.read_text(encoding="utf-8", errors="replace"), rel)
        for name, variants in parsed.items():
            if name == "__files__":
                continue
            enums[name] = variants

    leaves: list[dict] = []
    unparsed: list[str] = []
    if TOP_ENUM not in enums:
        return [], [f"top-level `{TOP_ENUM}` enum not found among parsed enums"]

    seen_enums: set[str] = set()

    def walk(enum_name: str, prefix: list[str], cfg: str | None) -> None:
        if enum_name in seen_enums:  # cycle guard (none expected)
            return
        seen_enums.add(enum_name)
        for v in enums[enum_name]:
            kebab = camel_to_kebab(v["variant"])
            path = prefix + [kebab]
            # A command is feature-gated if it OR any ancestor group is cfg-gated.
            vcfg = v.get("cfg")
            combined = " && ".join(c for c in (cfg, vcfg) if c) or None
            dele = v["delegates_to"]
            if dele and dele in enums:
                walk(dele, path, combined)
            else:
                if dele and dele not in enums:
                    # Tuple variant whose type is not a parsed Subcommand enum —
                    # treat as a leaf command but note the unresolved delegation.
                    unparsed.append(
                        f"`{' '.join(path)}` -> unresolved subcommand type `{dele}` "
                        f"(`{v['file']}`:{v['line']})"
                    )
                leaves.append(
                    {
                        "path": " ".join(path),
                        "group": path[0],
                        "file": v["file"],
                        "line": v["line"],
                        "cfg": combined,
                    }
                )
        seen_enums.discard(enum_name)

    walk(TOP_ENUM, [], None)
    leaves.sort(key=lambda x: x["path"])
    return leaves, unparsed


def md_escape(s: str) -> str:
    return s.replace("|", "\\|")


def render(leaves: list[dict], unparsed: list[str], commit: str) -> str:
    # A command guarded by `#[cfg(feature = …)]` is NOT present in the default build
    # (`icnctl` Cargo.toml has `default = []`). Count those separately so the default-build
    # surface and totals are not inflated (issue #2113 review).
    default_cmds = [c for c in leaves if not c.get("cfg")]
    gated_cmds = [c for c in leaves if c.get("cfg")]

    by_role: dict[str, list[dict]] = {}
    role_counts: dict[str, int] = {}
    group_set = set()
    for c in default_cmds:
        role = ROLE_BY_GROUP.get(c["group"], "unknown")
        c["role"] = role
        by_role.setdefault(role, []).append(c)
        role_counts[role] = role_counts.get(role, 0) + 1
        group_set.add(c["group"])
    for c in gated_cmds:
        c["role"] = ROLE_BY_GROUP.get(c["group"], "unknown")
        group_set.add(c["group"])

    now = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    total = len(default_cmds)
    role_order = ["organizer", "operator", "developer", "agent", "maintainer", "unknown"]

    o: list[str] = []
    o.append("---")
    o.append("Status: generated")
    o.append("Canonical: no")
    o.append(f"Generated: {now}")
    o.append("---")
    o.append("")
    o.append("# `icnctl` Command Inventory (generated)")
    o.append("")
    o.append(f"> Generated mechanically by [`{SCRIPT_REL}`](../../../scripts/icnctl_command_inventory.py). "
             "**Do not hand-edit** — rerun the script.")
    o.append(f"> Regenerate: `python3 {SCRIPT_REL} --write`  ·  Check drift: `python3 {SCRIPT_REL} --check`  ·  Issue [#2113](https://github.com/InterCooperative-Network/icn/issues/2113)")
    o.append("")
    o.append("## What this proves / does not prove")
    o.append("")
    o.append("- **Proves:** these `icnctl` command declarations exist in the clap command tree "
             "under `icn/bins/icnctl/src/**` at the snapshot commit (proof level **L1**).")
    o.append("- **Does NOT prove:** that a command works, is safe for organizers, is "
             "production-ready, is wired to a live gateway, has correct auth/permissions, is "
             "part of a supported pilot flow, or is appropriate for non-technical users.")
    o.append("- **`role` is a curated navigation heuristic** (top-level command group -> role), "
             "**not** mechanically derived from clap and **needs review**. It says \"where might I "
             "look first\", never \"this user may safely run this\".")
    o.append("- **`status` is uniformly `" + UNIFORM_STATUS + "`** by construction: a static clap "
             "scan proves a command is *declared*, not whether it is `implemented` / "
             "`implemented but partial` / `fixture-backed` / `gateway-backed` / "
             "`docs-only / design-direction` / `planned`. Assigning those per command is a "
             "human/runtime follow-up — so nothing here is presented as live.")
    o.append("- Defer to canonical truth/precedence: [`source-of-truth-map.md`](../source-of-truth-map.md) "
             "and proof levels in [`proof-level-taxonomy-capability-matrix.md`](../proof-level-taxonomy-capability-matrix.md). "
             "Orientation artifact (`Canonical: no`); companion to [`generated/route-inventory.md`](route-inventory.md).")
    o.append("")
    o.append("## Snapshot")
    o.append("")
    o.append(f"- Source commit: `{commit}`")
    o.append("- Source scanned: `icn/bins/icnctl/src/**` (clap `#[derive(Subcommand)]` / `#[derive(Parser)]` tree)")
    o.append("")
    o.append("## Summary")
    o.append("")
    o.append(f"- **Total leaf commands (default build): {total}**")
    o.append(f"- Top-level command groups: {len(group_set)}")
    o.append("- By role (curated, needs review): "
             + " · ".join(f"{r} {role_counts.get(r, 0)}" for r in role_order if role_counts.get(r, 0)))
    o.append(f"- By status: every default-build command is `{UNIFORM_STATUS}` ({total}) — see note above.")
    o.append(f"- Proof level: every command is `{PROOF_LEVEL}` (declaration exists in source).")
    o.append(f"- **Feature-gated commands (NOT in the default build, excluded from the counts "
             f"above): {len(gated_cmds)}** (see section below).")
    o.append(f"- Unparsed / unresolved candidates: {len(unparsed)} (see section below).")
    o.append("")
    o.append("## Commands by role")
    o.append("")
    o.append("Role is the **curated** top-level-group heuristic (needs review). `status` and "
             "`proof` are uniform by construction (see the note above).")
    o.append("")
    for role in role_order:
        cmds = sorted(by_role.get(role, []), key=lambda x: x["path"])
        if not cmds:
            continue
        o.append(f"### {role} ({len(cmds)})")
        o.append("")
        o.append("| Command | Status | Proof | Source |")
        o.append("|---|---|---|---|")
        for c in cmds:
            o.append(f"| `icnctl {md_escape(c['path'])}` | {UNIFORM_STATUS} | {PROOF_LEVEL} | "
                     f"`{c['file']}`:{c['line']} |")
        o.append("")

    o.append("## Feature-gated commands (not in the default build)")
    o.append("")
    o.append("These commands are guarded by a Cargo `#[cfg(feature = …)]` and are **absent from "
             "the default `icnctl` build** (`icn/bins/icnctl/Cargo.toml` has `default = []`). They "
             "are **excluded** from the counts and role tables above and only exist when the named "
             "feature is enabled at build time.")
    o.append("")
    if gated_cmds:
        o.append("| Command | Required cfg | Role (curated) | Proof | Source |")
        o.append("|---|---|---|---|---|")
        for c in sorted(gated_cmds, key=lambda x: x["path"]):
            o.append(f"| `icnctl {md_escape(c['path'])}` | `cfg({md_escape(c['cfg'])})` | "
                     f"{c['role']} | {PROOF_LEVEL} | `{c['file']}`:{c['line']} |")
        o.append("")
    else:
        o.append("- None: every discovered command is present in the default build.")
        o.append("")

    o.append("## Commands by status")
    o.append("")
    o.append(f"All {total} default-build commands carry the conservative status "
             f"`{UNIFORM_STATUS}`. The static clap scan cannot mechanically distinguish "
             "`implemented` / `implemented but partial` / `fixture-backed` / `gateway-backed` / "
             "`docs-only / design-direction` / `planned`; that per-command classification is a "
             "human/runtime verification follow-up (so demo/dev-gated commands are never "
             "presented here as live).")
    o.append("")

    o.append("## Unparsed / unknown candidates")
    o.append("")
    if unparsed:
        o.append("Variants whose subcommand type could not be resolved to a parsed clap enum "
                 "(listed as leads, recorded as leaf commands above with a caveat):")
        o.append("")
        for u in sorted(unparsed):
            o.append(f"- {u}")
        o.append("")
    else:
        o.append("- None: every top-level variant resolved to either a leaf command or a parsed "
                 "`#[derive(Subcommand)]` enum.")
        o.append("")

    o.append("## Safe vs unsafe claims (examples)")
    o.append("")
    o.append("- ✅ Safe: \"`icnctl api export-openapi` is declared in the CLI at this commit (L1).\"")
    o.append(f"- ✅ Safe: \"`icnctl` declares {total} commands across these groups; role grouping is a "
             "curated navigation aid pending review.\"")
    o.append("- ❌ Unsafe: \"`icnctl gov …` is organizer-ready\" / \"this command is live in "
             "production\" / \"this command is safe for non-technical organizers\" — none of "
             "that is established by a declaration scan.")
    o.append("")
    return "\n".join(o)


def strip_volatile(text: str) -> str:
    keep = [ln for ln in text.splitlines()
            if not ln.startswith("Generated:") and not ln.startswith("- Source commit:")]
    return "\n".join(keep)


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate or check the icnctl command inventory.")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--write", action="store_true", help="regenerate the inventory artifact")
    g.add_argument("--check", action="store_true", help="exit 1 if the committed artifact is stale")
    args = ap.parse_args()

    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    except Exception:
        commit = "unknown"

    leaves, unparsed = build_tree()
    if not leaves:
        # Surface the diagnostic build_tree() produced (missing src dir / missing top-level
        # enum) instead of a generic guess, so failures point at the real cause.
        detail = "; ".join(unparsed) if unparsed else f"no clap commands found under {ICNCTL_SRC}"
        print(f"ERROR: icnctl command inventory could not be built: {detail}", file=sys.stderr)
        return 2
    content = render(leaves, unparsed, commit)
    gated = sum(1 for c in leaves if c.get("cfg"))
    counts = f"{len(leaves) - gated} default-build + {gated} feature-gated, {len(unparsed)} unparsed"

    if args.write:
        ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
        ARTIFACT.write_text(content + "\n", encoding="utf-8")
        print(f"wrote {ARTIFACT.relative_to(ROOT)}: {counts}")
        return 0

    if not ARTIFACT.is_file():
        print(f"STALE: {ARTIFACT.relative_to(ROOT)} does not exist — run --write", file=sys.stderr)
        return 1
    committed = ARTIFACT.read_text(encoding="utf-8")
    if strip_volatile(committed).strip() == strip_volatile(content).strip():
        print(f"OK: icnctl command inventory up to date ({counts})")
        return 0
    print(f"STALE: {ARTIFACT.relative_to(ROOT)} differs from a fresh scan — run --write and commit",
          file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
