#!/usr/bin/env python3
"""Generate (or --check) a mechanical route inventory for the ICN gateway.

Scans the gateway Rust source for Actix routing **attribute macros**
(`#[get("...")]`, `#[post(...)]`, `#[put(...)]`, `#[delete(...)]`, `#[patch(...)]`,
`#[head(...)]`, and `#[route("...", method = "...")]`), compares the discovered
route declarations against the documented OpenAPI paths, and writes a
claim-disciplined markdown inventory.

This is deliberately humble (issue #2112, PR1):
- It proves route **declarations** exist in source. It does NOT prove route
  correctness, authentication, test coverage, production readiness, or public
  safety. "OpenAPI documented" does not mean complete or correct.
- Full mounted-path resolution is **best-effort** (a module->scope heuristic
  read from server.rs), not a real Rust parser. The relative macro path and the
  source file are authoritative; the "group" column is approximate.
- It scans the gateway crate's attribute macros only. `web::resource`/`.route`
  registrations and out-of-crate HTTP handlers (e.g. `icn-governance-actor`) are
  not fully captured.

Usage:
    python3 docs/scripts/route_inventory.py --write    # regenerate the artifact
    python3 docs/scripts/route_inventory.py --check     # exit 1 if the artifact is stale
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
GATEWAY_SRC = ROOT / "icn" / "crates" / "icn-gateway" / "src"
SERVER_RS = GATEWAY_SRC / "server.rs"
OPENAPI = ROOT / "docs" / "api" / "openapi.generated.yaml"
ARTIFACT = ROOT / "docs" / "reference" / "project-index" / "generated" / "route-inventory.md"
SCRIPT_REL = "docs/scripts/route_inventory.py"

ATTR_RE = re.compile(r'#\[(get|post|put|delete|patch|head)\("([^"]*)"\)\]')
ROUTE_RE = re.compile(r'#\[route\("([^"]*)"\s*,\s*method\s*=\s*"([A-Za-z]+)"')
FN_RE = re.compile(r'\b(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)')
SCOPE_RE = re.compile(r'web::scope\("(/[^"]*)"\)')
MODREF_RE = re.compile(r'api::([A-Za-z_]\w*)::')
OAPI_PATH_RE = re.compile(r'^  (/\S*):\s*$')


def discover_routes() -> list[dict]:
    routes: list[dict] = []
    if not GATEWAY_SRC.is_dir():
        return routes
    for rs in sorted(GATEWAY_SRC.rglob("*.rs")):
        lines = rs.read_text(encoding="utf-8", errors="replace").splitlines()
        rel = rs.relative_to(ROOT).as_posix()
        module = rs.stem if rs.stem != "mod" else rs.parent.name
        for i, line in enumerate(lines):
            method = path = None
            m = ATTR_RE.search(line)
            if m:
                method, path = m.group(1).upper(), m.group(2)
            else:
                rm = ROUTE_RE.search(line)
                if rm:
                    path, method = rm.group(1), rm.group(2).upper()
            if not method:
                continue
            handler = "?"
            for j in range(i + 1, min(i + 8, len(lines))):
                fm = FN_RE.search(lines[j])
                if fm:
                    handler = fm.group(1)
                    break
            routes.append(
                {
                    "method": method,
                    "path": path,
                    "file": rel,
                    "line": i + 1,
                    "handler": handler,
                    "module": module,
                }
            )
    return routes


def module_scope_map() -> dict[str, str]:
    """Best-effort: associate each `web::scope("/seg")` with the api::<module>:: refs
    that textually follow it (until the next scope). Approximate, not a real parser."""
    out: dict[str, str] = {}
    if not SERVER_RS.is_file():
        return out
    current = ""
    for line in SERVER_RS.read_text(encoding="utf-8", errors="replace").splitlines():
        sm = SCOPE_RE.search(line)
        if sm:
            current = sm.group(1) if sm.group(1) != "/v1" else ""
        for mod in MODREF_RE.findall(line):
            out.setdefault(mod, current)
    return out


def openapi_paths() -> list[str]:
    if not OPENAPI.is_file():
        return []
    paths, in_paths = [], False
    for line in OPENAPI.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.rstrip() == "paths:":
            in_paths = True
            continue
        if in_paths and line and not line[0].isspace():
            break
        m = OAPI_PATH_RE.match(line)
        if m:
            paths.append(m.group(1))
    return paths


def norm(p: str) -> str:
    p = re.sub(r"\{[^}]+\}", "{}", p)
    p = re.sub(r"/+", "/", p)
    return p.rstrip("/") or "/"


def full_path(scope: str, rel: str) -> str:
    rel = rel if rel.startswith("/") or rel == "" else "/" + rel
    fp = "/v1" + scope + rel
    fp = re.sub(r"/+", "/", fp)
    return fp.rstrip("/") or "/v1"


def md_table_escape(s: str) -> str:
    return s.replace("|", "\\|")


def render(routes: list[dict], scopes: dict[str, str], oapi: list[str], commit: str) -> str:
    oapi_norm = {norm(p) for p in oapi}
    by_method: dict[str, int] = {}
    documented = 0
    rows = []
    for r in sorted(routes, key=lambda x: (x["module"], x["path"], x["method"])):
        by_method[r["method"]] = by_method.get(r["method"], 0) + 1
        scope = scopes.get(r["module"], "")
        fp = full_path(scope, r["path"])
        candidates = {norm(fp), norm(scope + (r["path"] if r["path"].startswith("/") else "/" + r["path"])), norm(r["path"])}
        documented_here = bool(candidates & oapi_norm)
        if documented_here:
            documented += 1
        rows.append((r, fp, "yes" if documented_here else "no"))

    total = len(routes)
    undoc = total - documented
    pct_doc = (documented / total * 100) if total else 0.0
    now = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    method_line = " · ".join(f"{m} {by_method[m]}" for m in sorted(by_method))

    out: list[str] = []
    out.append("---")
    out.append("Status: generated")
    out.append("Canonical: no")
    out.append(f"Generated: {now}")
    out.append("---")
    out.append("")
    out.append("# Gateway Route Inventory (generated)")
    out.append("")
    out.append(f"> Generated mechanically by [`{SCRIPT_REL}`](../../../scripts/route_inventory.py). "
               "**Do not hand-edit** — rerun the script.")
    out.append(f"> Regenerate: `python3 {SCRIPT_REL} --write`  ·  Check drift: `python3 {SCRIPT_REL} --check`")
    out.append("")
    out.append("## What this proves / does not prove")
    out.append("")
    out.append("- **Proves:** these route declarations (Actix routing attribute macros) exist in the gateway source at the snapshot commit.")
    out.append("- **Does NOT prove:** route correctness, authentication/authorization, test coverage, runtime health, production readiness, or public-claim safety. `OpenAPI documented = yes` does **not** mean the contract is complete or correct.")
    out.append("- Defer to canonical truth/precedence: [`source-of-truth-map.md`](../source-of-truth-map.md) and proof levels in [`proof-level-taxonomy-capability-matrix.md`](../proof-level-taxonomy-capability-matrix.md). This is an orientation artifact (`Canonical: no`).")
    out.append("")
    out.append("## Snapshot")
    out.append("")
    out.append(f"- Source commit: `{commit}`")
    out.append("- Gateway source scanned: `icn/crates/icn-gateway/src/**`")
    out.append("- OpenAPI spec: `docs/api/openapi.generated.yaml`")
    out.append("")
    out.append("## Coverage summary")
    out.append("")
    out.append(f"- **Discovered gateway route macros: {total}** ({method_line})")
    out.append(f"- **OpenAPI documented paths: {len(oapi)}**")
    out.append(f"- Matched as documented (best-effort path match): {documented}")
    out.append(f"- Not matched to OpenAPI (best-effort): {undoc} (~{undoc / total * 100:.0f}% of discovered)" if total else "- Not matched to OpenAPI: 0")
    out.append(f"- Documented share of discovered routes: ~{pct_doc:.1f}%")
    out.append("")
    out.append("> The gap is structural: only handlers hand-annotated for utoipa reach the OpenAPI spec. "
               "Of the OpenAPI paths, several belong to `icn-governance-actor` HTTP handlers that live outside "
               "the gateway crate and are not captured by this macro scan — so the documented/undocumented "
               "counts here are a best-effort comparison, while the two headline counts (discovered macros, "
               "OpenAPI paths) are the robust measured facts.")
    out.append("")
    out.append("## Limitations (PR1)")
    out.append("")
    out.append("- **Full mounted-path resolution is best-effort.** The `Group` column is inferred from a simple "
               "`web::scope(\"…\")` ↔ `api::<module>::` association read from `server.rs`; it is **not** a real Rust "
               "parser and may be wrong for deeply-nested or conditional scopes. The relative macro `Path` and the "
               "`Source` file are authoritative.")
    out.append("- Scans **attribute macros only** in `icn-gateway/src`. `web::resource(\"…\")`/`.route(\"…\")` "
               "registrations and out-of-crate handlers (e.g. `icn-governance-actor::http`) are not fully captured.")
    out.append("- This is **generated-map work, not API documentation completion** (issue #2112). It does not add or "
               "change any OpenAPI content or runtime behavior.")
    out.append("")
    out.append("## Routes")
    out.append("")
    out.append("Status vocabulary is the existing set from `source-of-truth-map.md`. PR1 defaults every discovered "
               "route to `gateway-backed` (it is served by the gateway) and claim-safety to `needs review` "
               "(per-route proof level is `unknown / needs local verification` pending human review).")
    out.append("")
    out.append("| Method | Path (macro) | Group (best-effort) | Source | Handler | OpenAPI | Status | Claim safety |")
    out.append("|---|---|---|---|---|---|---|---|")
    for r, fp, doc in rows:
        out.append(
            f"| {r['method']} | `{md_table_escape(r['path'] or '(empty)')}` | `{md_table_escape(fp)}` | "
            f"`{r['file']}`:{r['line']} | `{md_table_escape(r['handler'])}` | {doc} | gateway-backed | needs review |"
        )
    out.append("")
    return "\n".join(out)


def strip_volatile(text: str) -> str:
    """Remove non-deterministic lines so --check ignores timestamp/commit churn."""
    keep = []
    for line in text.splitlines():
        if line.startswith("Generated:") or line.startswith("- Source commit:"):
            continue
        keep.append(line)
    return "\n".join(keep)


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate or check the gateway route inventory.")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--write", action="store_true", help="regenerate the inventory artifact")
    g.add_argument("--check", action="store_true", help="exit 1 if the committed artifact is stale")
    args = ap.parse_args()

    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    except Exception:
        commit = "unknown"

    routes = discover_routes()
    scopes = module_scope_map()
    oapi = openapi_paths()
    if not routes:
        print(f"ERROR: no routes discovered under {GATEWAY_SRC} — wrong repo root?", file=sys.stderr)
        return 2
    content = render(routes, scopes, oapi, commit)

    if args.write:
        ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
        ARTIFACT.write_text(content + "\n", encoding="utf-8")
        print(f"wrote {ARTIFACT.relative_to(ROOT)}: {len(routes)} routes, {len(oapi)} OpenAPI paths")
        return 0

    # --check
    if not ARTIFACT.is_file():
        print(f"STALE: {ARTIFACT.relative_to(ROOT)} does not exist — run --write", file=sys.stderr)
        return 1
    committed = ARTIFACT.read_text(encoding="utf-8")
    if strip_volatile(committed).strip() == strip_volatile(content).strip():
        print(f"OK: route inventory up to date ({len(routes)} routes, {len(oapi)} OpenAPI paths)")
        return 0
    print(f"STALE: {ARTIFACT.relative_to(ROOT)} differs from a fresh scan — run --write and commit", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
