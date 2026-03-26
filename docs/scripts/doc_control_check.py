#!/usr/bin/env python3
"""
Documentation control plane validation for ICN.

Validates:
  - docs/**/*.md against docs/registry.toml
  - merged per-path records from explicit [docs."path"] entries + longest-prefix defaults
  - control.allowlisted_docs_subdirs for structural drift
  - required metadata headers on control-plane canonical docs (control.canonical_doc_paths only)
  - registry integrity: orphan [docs."…"] rows, broken superseded_by targets; YAML vs merged drift (warn)

Usage:
  python3 docs/scripts/doc_control_check.py [--repo ROOT] [--registry PATH] [--strict] [--write-document-registry PATH]

Exit codes: 0 ok, 1 validation failed
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from datetime import date, datetime
from pathlib import Path
from typing import Any

try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib  # type: ignore[no-redef]
    except ImportError:
        print("ERROR: Python 3.11+ or tomli required", file=sys.stderr)
        sys.exit(1)

HEADER_BLOCK_RE = re.compile(
    r"^(?:---\s*\n(?P<yaml>[\s\S]*?)\n---\s*\n|<!--\s*\n(?P<html>[\s\S]*?)\n-->\s*\n?)",
    re.MULTILINE,
)
YAML_LINE_RE = re.compile(r"^(\w[\w\s]*?):\s*(.+)$")
TRUTH_CLASSES = frozenset(
    {"normative", "descriptive", "operational", "historical", "draft"}
)
ROLES = frozenset(
    {
        "architecture",
        "design",
        "spec",
        "planning",
        "strategy",
        "guide",
        "status",
        "archive",
        "internal",
        "template",
        "reference",
        "development_session",
    }
)
CANONICAL_VALUES = frozenset({"yes", "no"})
STATUS_VALUES = frozenset(
    {"active", "living", "superseded", "archived", "draft", "missing", "canonical"}
)

H1_RE = re.compile(r"^#\s+(.+?)\s*$", re.MULTILINE)


def load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as f:
        return tomllib.load(f)


def parse_header_metadata(text: str, max_lines: int = 120) -> dict[str, str]:
    """Extract Status / Canonical / Last Reviewed from YAML frontmatter or HTML comment."""
    head = "\n".join(text.splitlines()[:max_lines])
    m = HEADER_BLOCK_RE.match(head)
    if not m:
        return {}
    block = (m.group("yaml") or m.group("html") or "").strip()
    out: dict[str, str] = {}
    for line in block.splitlines():
        lm = YAML_LINE_RE.match(line.strip())
        if not lm:
            continue
        key = lm.group(1).strip().lower().replace(" ", "_")
        val = lm.group(2).strip()
        if key == "status":
            out["status"] = val.split("|")[0].strip().lower()
        elif key == "canonical":
            out["canonical"] = val.split("|")[0].strip().lower()
        elif key in ("last_reviewed", "last-reviewed"):
            out["last_reviewed"] = val.strip()
    return out


def validate_date(s: str) -> bool:
    try:
        datetime.strptime(s, "%Y-%m-%d")
        return True
    except ValueError:
        return False


def merge_defaults(
    rel_posix: str,
    explicit: dict[str, Any] | None,
    defaults: list[dict[str, Any]],
) -> dict[str, Any]:
    """Longest matching prefix wins; explicit entry overlays."""
    matched: dict[str, Any] = {}
    best_len = -1
    for rule in defaults:
        prefix = rule.get("prefix", "")
        if prefix and (
            rel_posix == prefix.rstrip("/")
            or rel_posix.startswith(prefix.rstrip("/") + "/")
        ):
            plen = len(prefix)
            if plen > best_len:
                best_len = plen
                matched = {k: v for k, v in rule.items() if k != "prefix"}
    rec = dict(matched)
    if explicit:
        for k, v in explicit.items():
            if v is not None and k not in ("depends_on", "sections"):
                rec[k] = v
    return rec


def legacy_infer_fields(rec: dict[str, Any]) -> dict[str, Any]:
    """Fill control fields from legacy category/status when new keys absent."""
    out = dict(rec)
    st = str(out.get("status", "")).lower()
    cat = str(out.get("category", "")).lower()

    if "truth_class" not in out or out["truth_class"] in ("", "inherit"):
        if st in ("archived", "superseded"):
            out["truth_class"] = "historical"
        elif st == "draft":
            out["truth_class"] = "draft"
        elif cat == "guide" or "operations" in cat:
            out["truth_class"] = "operational"
        elif cat == "architecture":
            out["truth_class"] = "descriptive"
        else:
            out["truth_class"] = out.get("truth_class", "descriptive")

    if "role" not in out or out["role"] in ("", "inherit"):
        role_map = {
            "architecture": "architecture",
            "design": "design",
            "strategy": "strategy",
            "guide": "guide",
            "reference": "reference",
            "operations": "guide",
            "security": "reference",
            "status": "status",
        }
        out["role"] = role_map.get(cat, "reference")

    if "canonical" not in out or out["canonical"] in ("", "inherit"):
        out["canonical"] = "yes" if st == "canonical" else "no"

    if "freshness" not in out or out["freshness"] in ("", "inherit"):
        out["freshness"] = "current" if st in ("canonical", "living") else "unknown"

    if "recommended_action" not in out or not out["recommended_action"]:
        out["recommended_action"] = "keep"

    if "domain_tags" not in out or not out["domain_tags"]:
        out["domain_tags"] = [cat] if cat else ["general"]

    if "last_reviewed" not in out or not out["last_reviewed"]:
        lu = out.get("last_updated")
        if isinstance(lu, str) and validate_date(lu):
            out["last_reviewed"] = lu
        else:
            out["last_reviewed"] = "unknown"

    return out


def extract_first_h1(text: str) -> str | None:
    """First markdown H1 after optional YAML frontmatter."""
    if text.startswith("---"):
        end = text.find("\n---\n", 3)
        if end != -1:
            text = text[end + 5 :]
    m = H1_RE.search(text)
    return m.group(1).strip() if m else None


def collect_markdown_paths(repo: Path) -> list[Path]:
    docs = repo / "docs"
    paths = sorted(docs.rglob("*.md"))
    return paths


def render_document_registry_md(
    repo: Path,
    merged_by_path: dict[str, dict[str, Any]],
    control: dict[str, Any],
    explicit_docs_under_docs: int,
) -> str:
    lines: list[str] = []
    lines.append("---")
    lines.append("Status: descriptive")
    lines.append("Canonical: no")
    lines.append("Last Reviewed: " + date.today().isoformat())
    lines.append("---")
    lines.append("")
    lines.append("# ICN Document Registry (human summary)")
    lines.append("")
    lines.append(
        "This file is a **companion** to the machine-readable [`registry.toml`](registry.toml). "
        "Regenerate after registry or corpus changes:"
    )
    lines.append("")
    lines.append(
        "```bash\npython3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml \\\n"
        "  --write-document-registry docs/DOCUMENT_REGISTRY.md\n```"
    )
    lines.append("")
    lines.append("## Truth classes (merged onto every doc)")
    lines.append("")
    lines.append("| Value | Meaning |")
    lines.append("|-------|---------|")
    lines.append("| `normative` | How the system is supposed to work (specs, binding ADRs, control policy). |")
    lines.append("| `descriptive` | What exists or what the repo claims today (inventories, architecture narrative). |")
    lines.append("| `operational` | How to run, verify, or recover (runbooks, checklists). |")
    lines.append("| `historical` | Point-in-time or superseded; not current truth. Prefer `docs/archive/YYYY/`. |")
    lines.append("| `draft` | Proposal / WIP; do not treat as frozen. |")
    lines.append("")
    lines.append("## Canonical vs non-canonical")
    lines.append("")
    lines.append(
        "- **Control-plane canonical:** exactly the paths in `[control].canonical_doc_paths` in `registry.toml`. "
        "Those files must carry `Status` / `Canonical` / `Last Reviewed` in YAML (or HTML comment) matching the merged registry row."
    )
    lines.append(
        "- **Elsewhere:** `canonical = \"yes\"` in a `[docs.\"…\"]` row does **not** mean control-plane canon; prefer `canonical = \"no\"` for domain hubs unless you are intentionally promoting a doc (and then add it to `canonical_doc_paths`)."
    )
    lines.append(
        "- **Non-canonical:** still useful, but not in the four-path control set, or historical/draft."
    )
    lines.append(
        "- **History:** paths under `docs/archive/` are historical by convention; active paths with `Status: historical` or registry `truth_class = historical` must not override canon without explicit review."
    )
    lines.append("")
    lines.append("## Canonical documentation (control plane)")
    lines.append("")
    lines.append("| Path | Role | Truth class |")
    lines.append("|------|------|-------------|")
    canon = control.get("canonical_doc_paths", [])
    for p in canon:
        m = merged_by_path.get(p, {})
        lines.append(
            f"| `{p}` | {m.get('role', '')} | {m.get('truth_class', '')} |"
        )
    lines.append("")
    lines.append("## Corpus coverage")
    lines.append("")
    lines.append(f"- **Markdown files under docs/**: {len(merged_by_path)}")
    by_tc: dict[str, int] = {}
    for m in merged_by_path.values():
        tc = str(m.get("truth_class", "unknown"))
        by_tc[tc] = by_tc.get(tc, 0) + 1
    lines.append("- **By truth_class (merged):**")
    for k in sorted(by_tc.keys()):
        lines.append(f"  - `{k}`: {by_tc[k]}")
    lines.append("")
    lines.append("## Schema and policy")
    lines.append("")
    lines.append(
        "- Full field definitions: [`internal/documentation-control-system/02_DOCUMENT_REGISTRY_SPEC.md`](internal/documentation-control-system/02_DOCUMENT_REGISTRY_SPEC.md)"
    )
    lines.append(
        "- Placement rules: [`internal/documentation-control-system/05_DOC_PLACEMENT_AND_TRUTH_CLASS_RULES.md`](internal/documentation-control-system/05_DOC_PLACEMENT_AND_TRUTH_CLASS_RULES.md)"
    )
    lines.append(
        "- Top-level control spec: [`DOCUMENTATION_CONTROL_SYSTEM.md`](DOCUMENTATION_CONTROL_SYSTEM.md)"
    )
    lines.append("")
    lines.append("## Defaults vs explicit `[docs.\"…\"]` rows")
    lines.append("")
    lines.append(
        "1. **Prefix defaults:** `[[doc_path_defaults]]` apply by longest matching `prefix` (e.g. `docs/plans/`). Every file under `docs/**/*.md` gets a merged record."
    )
    lines.append(
        "2. **Explicit row:** add `[docs.\"docs/relative/path.md\"]` only when you need to override defaults or attach `title`, `superseded_by`, audiences, etc."
    )
    lines.append(
        "3. **Do not** fork a second registry format; extend `registry.toml` only."
    )
    lines.append("")
    lines.append("## How to update the registry (checklist)")
    lines.append("")
    lines.append("1. Pick the correct directory (see maintenance guide + placement rules doc).")
    lines.append(
        "2. Add or adjust `[[doc_path_defaults]]` if a **whole subtree** should change class/role."
    )
    lines.append(
        "3. Otherwise add `[docs.\"docs/…\"]` with `truth_class`, `role`, `canonical`, `last_reviewed` / `last_updated`, `recommended_action`, `domain_tags` as needed."
    )
    lines.append("4. For new top-level folders under `docs/`, extend `[control].allowlisted_docs_subdirs` or CI will fail.")
    lines.append(
        "5. Run `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml` (and `--strict` before merge if you touched contested areas)."
    )
    lines.append(
        "6. Refresh this file with `--write-document-registry` when you want counts and the canon table up to date."
    )
    lines.append("")
    lines.append("## CI vs local enforcement")
    lines.append("")
    lines.append(
        "What fails in CI, what warns, what `--strict` adds, and when to promote strict to CI: "
        "[`DOCUMENTATION_MAINTENANCE.md`](DOCUMENTATION_MAINTENANCE.md#documentation-control-plane-start-here). "
        "Policy toggles live in `[control.enforcement]` in `registry.toml`."
    )
    lines.append("")
    lines.append("## Registry coverage (this snapshot)")
    lines.append("")
    lines.append(
        f"- **Files scanned:** {len(merged_by_path)} Markdown files under `docs/`"
    )
    lines.append(
        f"- **Explicit `[docs.\"…\"]` rows under `docs/`:** {explicit_docs_under_docs} (remaining files rely on `[[doc_path_defaults]]` only)"
    )
    lines.append("")
    return "\n".join(lines)


def run_enforcement_checks(
    repo: Path,
    merged_by_path: dict[str, dict[str, Any]],
    file_texts: dict[str, str],
    explicit_hit: dict[str, bool],
    control: dict[str, Any],
    canon_paths: list[str],
    strict: bool,
    docs_table: dict[str, Any],
) -> tuple[list[str], list[str]]:
    """Return (errors, warnings)."""
    errors: list[str] = []
    warnings: list[str] = []
    enf = control.get("enforcement", {})
    days = int(enf.get("stale_canonical_warning_days", 90))
    canon_set = frozenset(canon_paths)

    def bump(
        msg: str,
        fail_key: str,
        warn_key: str,
        *,
        strict_ok: bool = True,
    ) -> None:
        fail = bool(enf.get(fail_key, False))
        warn = bool(enf.get(warn_key, True))
        if fail or (strict and strict_ok):
            errors.append(msg)
        elif warn:
            warnings.append(msg)

    # Explicit registry rows pointing at missing files
    or_shown = 0
    or_cap = 40
    for reg_key in sorted(docs_table.keys()):
        if not (reg_key.startswith("docs/") and reg_key.endswith(".md")):
            continue
        if (repo / reg_key).is_file():
            continue
        if or_shown >= or_cap:
            break
        or_shown += 1
        bump(
            f"[orphan-registry] explicit [docs.\"…\"] row but file missing: {reg_key}",
            "fail_orphan_registry_entry",
            "warn_orphan_registry_entry",
        )

    # Broken superseded_by targets (explicit rows)
    sb_shown = 0
    sb_cap = 40
    for rel, row in docs_table.items():
        if not isinstance(row, dict):
            continue
        tgt = row.get("superseded_by")
        if not tgt or not isinstance(tgt, str):
            continue
        t = tgt.strip()
        if not t.endswith(".md"):
            continue
        resolved = t if t.startswith("docs/") else f"docs/{t.lstrip('./')}"
        if (repo / resolved).is_file():
            continue
        if sb_shown >= sb_cap:
            break
        sb_shown += 1
        bump(
            f"[superseded-by] {rel}: superseded_by target missing: {tgt!r} (expected `{resolved}`)",
            "fail_broken_superseded_by",
            "warn_broken_superseded_by",
        )

    # Duplicate H1 across different directories
    if enf.get("warn_duplicate_h1_across_dirs", True) or enf.get(
        "fail_duplicate_h1_across_dirs", False
    ):
        title_to_paths: dict[str, list[str]] = {}
        for rel, txt in file_texts.items():
            h1 = extract_first_h1(txt)
            if not h1:
                continue
            key = h1.strip().casefold()
            title_to_paths.setdefault(key, []).append(rel)
        dup_shown = 0
        dup_cap = 40
        for title, paths in sorted(title_to_paths.items()):
            if len(paths) < 2:
                continue
            parents = {str(Path(p).parent) for p in paths}
            if len(parents) < 2:
                continue
            if all(Path(p).name.lower() == "readme.md" for p in paths):
                continue
            if dup_shown >= dup_cap:
                break
            dup_shown += 1
            sample = ", ".join(paths[:8])
            more = f" (+{len(paths) - 8} more)" if len(paths) > 8 else ""
            bump(
                f"[duplicate-h1] duplicate H1 title across directories ({title[:80]}): {sample}{more}",
                "fail_duplicate_h1_across_dirs",
                "warn_duplicate_h1_across_dirs",
            )

    # Stale canonical Last Reviewed
    for cpath in canon_paths:
        f = repo / cpath
        if not f.is_file():
            continue
        text = file_texts.get(cpath) or f.read_text(encoding="utf-8", errors="replace")
        meta = parse_header_metadata(text)
        lr = meta.get("last_reviewed", "")
        if not validate_date(lr):
            continue
        reviewed = datetime.strptime(lr, "%Y-%m-%d").date()
        age = (date.today() - reviewed).days
        if age > days:
            bump(
                f"[stale-canonical] {cpath}: Last Reviewed is {age} days old (threshold {days})",
                "fail_stale_canonical",
                "warn_stale_canonical",
            )

    # YAML frontmatter vs merged registry (non–control-plane docs only)
    ym_shown = 0
    ym_cap = 120
    for rel, merged in sorted(merged_by_path.items()):
        if rel in canon_set:
            continue
        meta = parse_header_metadata(file_texts.get(rel, ""))
        if not meta:
            continue
        tc = merged.get("truth_class")
        # Only compare when YAML uses a control-plane truth-class label (not legacy session labels).
        if (
            "status" in meta
            and meta["status"] in TRUTH_CLASSES
            and meta["status"] != tc
            and ym_shown < ym_cap
        ):
            ym_shown += 1
            bump(
                f"[yaml-mismatch] {rel}: YAML Status {meta['status']!r} != registry truth_class {tc!r}",
                "fail_yaml_registry_mismatch",
                "warn_yaml_registry_mismatch",
                strict_ok=False,
            )
        can = str(merged.get("canonical", "")).lower()
        if "canonical" in meta and meta["canonical"] != can and ym_shown < ym_cap:
            ym_shown += 1
            bump(
                f"[yaml-mismatch] {rel}: YAML Canonical {meta['canonical']!r} != registry canonical={can!r}",
                "fail_yaml_registry_mismatch",
                "warn_yaml_registry_mismatch",
                strict_ok=False,
            )
        mlr = str(merged.get("last_reviewed", ""))
        if (
            "last_reviewed" in meta
            and validate_date(mlr)
            and validate_date(meta["last_reviewed"])
            and meta["last_reviewed"] != mlr
            and ym_shown < ym_cap
        ):
            ym_shown += 1
            bump(
                f"[yaml-mismatch] {rel}: YAML Last Reviewed {meta['last_reviewed']!r} != registry {mlr!r}",
                "fail_yaml_registry_mismatch",
                "warn_yaml_registry_mismatch",
                strict_ok=False,
            )

    # Superseded but not under archive
    ss_shown = 0
    ss_cap = 40
    for rel in sorted(merged_by_path.keys()):
        merged = merged_by_path[rel]
        st = str(merged.get("status", "")).lower()
        if st != "superseded" or rel.startswith("docs/archive/"):
            continue
        if ss_shown >= ss_cap:
            break
        ss_shown += 1
        msg = (
            f"[superseded-location] {rel}: status superseded but not under docs/archive/ "
            f"({'missing superseded_by' if not merged.get('superseded_by') else 'has superseded_by'})"
        )
        bump(
            msg,
            "fail_superseded_outside_archive",
            "warn_superseded_outside_archive",
        )

    # High-churn dirs: prefer explicit registry row
    prefixes = tuple(enf.get("explicit_registry_warn_prefixes", []))
    if prefixes and (
        enf.get("warn_missing_explicit_registry", True)
        or enf.get("fail_missing_explicit_registry", False)
    ):
        ex_shown = 0
        ex_cap = 80
        for rel in sorted(merged_by_path.keys()):
            if explicit_hit.get(rel):
                continue
            if not any(rel.startswith(p) for p in prefixes):
                continue
            if ex_shown >= ex_cap:
                break
            ex_shown += 1
            bump(
                f"[explicit-registry] {rel}: classification debt — no explicit [docs.\"…\"] row "
                f"(prefix defaults only; non-blocking in CI; backfill when the doc stabilizes)",
                "fail_missing_explicit_registry",
                "warn_missing_explicit_registry",
                strict_ok=False,
            )

    return errors, warnings


EXPLICIT_REGISTRY_WARN_RE = re.compile(r"^\[explicit-registry\]\s+([^:]+):")


def warning_summary_lines(
    warnings: list[str],
    explicit_registry_prefixes: tuple[str, ...] = (),
) -> list[str]:
    """Grouped counts for stderr (tags are `[category]` prefixes)."""
    counts: Counter[str] = Counter()
    for w in warnings:
        if w.startswith("[") and "]" in w:
            tag = w[1 : w.index("]")]
            counts[tag] += 1
        else:
            counts["(untagged)"] += 1
    lines = ["---", "Warning summary (by tag):"]
    for k in sorted(counts.keys()):
        lines.append(f"  {k}: {counts[k]}")
    lines.append("---")
    lines.append(
        "Missing explicit row by prefix (non-blocking debt; see DOCUMENTATION_MAINTENANCE.md):"
    )
    if explicit_registry_prefixes:
        prefix_counts: Counter[str] = Counter()
        sorted_prefixes = sorted(explicit_registry_prefixes, key=len, reverse=True)
        for w in warnings:
            m = EXPLICIT_REGISTRY_WARN_RE.match(w)
            if not m:
                continue
            rel = m.group(1)
            for p in sorted_prefixes:
                if rel.startswith(p):
                    prefix_counts[p] += 1
                    break
        for p in sorted(explicit_registry_prefixes):
            lines.append(f"  {p}: {prefix_counts.get(p, 0)}")
    else:
        lines.append("  (no explicit_registry_warn_prefixes configured)")
    lines.append("---")
    return lines


def main() -> int:
    ap = argparse.ArgumentParser(description="ICN documentation control checks")
    ap.add_argument("--repo", type=Path, default=Path("."), help="Repository root")
    ap.add_argument("--registry", type=Path, default=None, help="Path to registry.toml")
    ap.add_argument(
        "--strict",
        action="store_true",
        help="Treat enforcement warnings as errors (see [control.enforcement])",
    )
    ap.add_argument(
        "--write-document-registry",
        type=Path,
        default=None,
        help="Write DOCUMENT_REGISTRY.md summary to this path",
    )
    args = ap.parse_args()
    repo = args.repo.resolve()
    reg_path = args.registry or (repo / "docs" / "registry.toml")
    if not reg_path.is_file():
        print(f"ERROR: registry not found: {reg_path}", file=sys.stderr)
        return 1

    data = load_toml(reg_path)
    control = data.get("control", {})
    docs_table: dict[str, Any] = data.get("docs", {})
    defaults: list[dict[str, Any]] = data.get("doc_path_defaults", [])

    allow = frozenset(control.get("allowlisted_docs_subdirs", []))
    if not allow:
        print("ERROR: [control].allowlisted_docs_subdirs is required", file=sys.stderr)
        return 1

    errors: list[str] = []
    merged_by_path: dict[str, dict[str, Any]] = {}
    file_texts: dict[str, str] = {}
    explicit_hit: dict[str, bool] = {}

    for path in collect_markdown_paths(repo):
        rel = path.relative_to(repo).as_posix()
        text = path.read_text(encoding="utf-8", errors="replace")
        file_texts[rel] = text
        parts = Path(rel).parts
        if len(parts) >= 2 and parts[0] == "docs":
            if len(parts) == 2 and parts[1].endswith(".md"):
                pass
            else:
                top = parts[1]
                if top not in allow:
                    errors.append(
                        f"Directory not allowlisted: {rel} (top-level `{top}`)"
                    )

        explicit = docs_table.get(rel)
        explicit_hit[rel] = explicit is not None
        merged = merge_defaults(rel, explicit, defaults)
        merged = legacy_infer_fields(merged)

        if merged.get("truth_class") not in TRUTH_CLASSES:
            errors.append(f"{rel}: invalid or missing truth_class after merge")
        if merged.get("role") not in ROLES:
            errors.append(f"{rel}: invalid or missing role after merge")
        c = str(merged.get("canonical", "")).lower()
        if c not in CANONICAL_VALUES:
            errors.append(f"{rel}: canonical must be yes/no, got {merged.get('canonical')!r}")

        merged_by_path[rel] = merged

    # Canonical doc headers
    canon_paths = control.get("canonical_doc_paths", [])
    required_keys = control.get(
        "required_header_keys", ["Status", "Canonical", "Last Reviewed"]
    )
    for cpath in canon_paths:
        f = repo / cpath
        if not f.is_file():
            errors.append(f"Canonical path missing on disk: {cpath}")
            continue
        text = file_texts.get(cpath) or f.read_text(encoding="utf-8", errors="replace")
        meta = parse_header_metadata(text)
        norm_keys = {k.lower().replace(" ", "_") for k in required_keys}
        if "status" not in meta:
            errors.append(f"{cpath}: missing Status in YAML/HTML header block")
        elif meta["status"] not in TRUTH_CLASSES:
            errors.append(f"{cpath}: Status not a valid truth class label: {meta['status']!r}")
        if "canonical" not in meta or meta["canonical"] not in ("yes", "no"):
            errors.append(f"{cpath}: Canonical: yes|no missing or invalid in header")
        if "last_reviewed" not in meta or not validate_date(meta["last_reviewed"]):
            errors.append(
                f"{cpath}: Last Reviewed missing or not YYYY-MM-DD (got {meta.get('last_reviewed')!r})"
            )

        merged = merged_by_path.get(cpath, {})
        if str(merged.get("canonical", "")).lower() != "yes":
            errors.append(
                f"{cpath}: registry merged record must have canonical=yes for control-plane canon set"
            )
        if meta.get("status") != merged.get("truth_class"):
            errors.append(
                f"{cpath}: YAML Status {meta.get('status')!r} does not match merged registry truth_class {merged.get('truth_class')!r}"
            )
        mlr_m = merged.get("last_reviewed", "")
        if validate_date(str(mlr_m)) and meta.get("last_reviewed") != mlr_m:
            errors.append(
                f"{cpath}: YAML Last Reviewed {meta.get('last_reviewed')!r} != merged registry last_reviewed {mlr_m!r}"
            )
        mcan = str(merged.get("canonical", "")).lower()
        if meta.get("canonical") != mcan:
            errors.append(
                f"{cpath}: YAML Canonical {meta.get('canonical')!r} != merged registry canonical={mcan!r}"
            )

    enf_errors, enf_warnings = run_enforcement_checks(
        repo,
        merged_by_path,
        file_texts,
        explicit_hit,
        control,
        canon_paths,
        args.strict,
        docs_table,
    )
    errors.extend(enf_errors)

    explicit_under_docs = sum(1 for k in docs_table if k.startswith("docs/"))

    if args.write_document_registry:
        out = render_document_registry_md(
            repo, merged_by_path, control, explicit_under_docs
        )
        args.write_document_registry.parent.mkdir(parents=True, exist_ok=True)
        args.write_document_registry.write_text(out, encoding="utf-8")
        print(f"Wrote {args.write_document_registry}")

    if errors:
        print("Documentation control validation FAILED:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    warn_cap = 120
    if enf_warnings:
        ex_prefixes = tuple(
            control.get("enforcement", {}).get("explicit_registry_warn_prefixes", [])
        )
        for line in warning_summary_lines(enf_warnings, ex_prefixes):
            print(line, file=sys.stderr)

        def _warn_sort_key(w: str) -> tuple[int, str, str]:
            if w.startswith("[") and "]" in w:
                return (0, w[1 : w.index("]")], w)
            return (1, "", w)

        for w in sorted(enf_warnings, key=_warn_sort_key)[:warn_cap]:
            print(f"WARNING: {w}", file=sys.stderr)
        if len(enf_warnings) > warn_cap:
            print(
                f"WARNING: ... and {len(enf_warnings) - warn_cap} more warnings",
                file=sys.stderr,
            )

    print(
        f"OK: doc control check passed ({len(merged_by_path)} docs markdown files under docs/)"
        + (f"; {len(enf_warnings)} enforcement warnings" if enf_warnings else "")
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
