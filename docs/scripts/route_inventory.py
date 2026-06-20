#!/usr/bin/env python3
"""Generate (or --check) a mechanical route inventory for the ICN gateway.

Scans the gateway Rust source for Actix routing **attribute macros**
(`#[get("...")]`, `#[post(...)]`, `#[put(...)]`, `#[delete(...)]`, `#[patch(...)]`,
`#[head(...)]`, and `#[route("...", method = "...")]`), compares the discovered
route declarations against the documented OpenAPI paths, and writes a
claim-disciplined markdown inventory.

This is deliberately humble (issue #2112):
- It proves route **declarations** exist in source. It does NOT prove route
  correctness, authentication, test coverage, production readiness, or public
  safety. "OpenAPI documented" does not mean complete or correct.
- Full mounted-path resolution is **best-effort** (a module->scope heuristic
  read from server.rs), not a real Rust parser. The relative macro path and the
  source file are authoritative; the "group" column is approximate.
- It scans the gateway crate's attribute macros for the route table, and
  additionally FLAGS macro-less `web::resource("…")`/`.route(…)` registration
  *candidates* by file:line (not parsed into routes — see `resource_candidates`).
  Out-of-crate HTTP handlers (e.g. `icn-governance-actor`) are still not captured.

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
# Anchored at an identifier boundary so it does NOT match `api::<seg>::` *inside*
# longer crate paths like `icn_kernel_api::services::` or `icn_api::LedgerService::`.
MODREF_RE = re.compile(r'(?<![A-Za-z0-9_])api::([A-Za-z_]\w*)::')
OAPI_PATH_RE = re.compile(r'^  (/\S*):\s*$')
OAPI_METHOD_RE = re.compile(r'^    (get|post|put|delete|patch|head):\s*$')
# Macro-less route-registration *candidates* (issue #2112, PR3). FLAGGED, not
# parsed into routes: a quoted `web::resource("…")` literal, and `.route(…)`
# method bindings. RESOURCE_RE requires a quoted argument, so `web::resource().to()`
# in a doc comment does not match.
RESOURCE_RE = re.compile(r'web::resource\("([^"]*)"\)')
ROUTE_CALL_RE = re.compile(r'\.route\(')


def discover_routes() -> list[dict]:
    routes: list[dict] = []
    if not GATEWAY_SRC.is_dir():
        return routes
    api_root = GATEWAY_SRC / "api"
    for rs in sorted(GATEWAY_SRC.rglob("*.rs")):
        lines = rs.read_text(encoding="utf-8", errors="replace").splitlines()
        rel = rs.relative_to(ROOT).as_posix()
        # Key by the TOP-LEVEL `src/api/<group>` segment (what server.rs registers as
        # `api::<group>::...`), not the leaf filename — otherwise nested files such as
        # `api/sdis/simple_enrollment.rs` would key on `simple_enrollment` and never
        # resolve a scope. Files directly under `api/` use their own stem.
        try:
            parts = rs.relative_to(api_root).parts
            module = parts[0][:-3] if len(parts) == 1 else parts[0]
        except ValueError:
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
    """Best-effort: associate each `web::scope("/seg")` with the `api::<module>::` refs
    that appear in `.service(...)`/`.configure(...)` *registration* calls beneath it
    (until the next scope). Only registration sites count — bare type references
    (e.g. `crate::api::sdis::SdisState::new()`) are skipped so they cannot win the
    `setdefault` and mislead the mapping. Approximate, not a real parser."""
    out: dict[str, str] = {}
    if not SERVER_RS.is_file():
        return out
    current = ""
    for line in SERVER_RS.read_text(encoding="utf-8", errors="replace").splitlines():
        sm = SCOPE_RE.search(line)
        if sm:
            current = sm.group(1) if sm.group(1) != "/v1" else ""
        if ".service(" not in line and ".configure(" not in line:
            continue
        for mod in MODREF_RE.findall(line):
            out.setdefault(mod, current)
    return out


def openapi_paths() -> list[dict]:
    """Parse the generated OpenAPI `paths:` block into [{path, methods}].
    Methods are the per-path operation keys (get/post/…), upper-cased."""
    if not OPENAPI.is_file():
        return []
    out: list[dict] = []
    in_paths, cur = False, None
    for line in OPENAPI.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.rstrip() == "paths:":
            in_paths = True
            continue
        if in_paths and line and not line[0].isspace():
            break
        m = OAPI_PATH_RE.match(line)
        if m:
            cur = {"path": m.group(1), "methods": []}
            out.append(cur)
            continue
        mm = OAPI_METHOD_RE.match(line)
        if mm and cur is not None:
            cur["methods"].append(mm.group(1).upper())
    return out


def resource_candidates() -> list[dict]:
    """Mechanically FLAG macro-less Actix route-registration candidates in the
    gateway crate: `web::resource("…")` resource literals and `.route(…)` method
    bindings. These are deliberately NOT counted as routes and NOT path-resolved:
    a regex scan cannot reliably distinguish a production registration from a
    test fixture (e.g. inside `#[cfg(test)]` / `test::init_service`), nor resolve
    the mounted path without a real parser. They are surfaced by file:line for
    human review and stay `unknown / needs local verification`."""
    out: list[dict] = []
    if not GATEWAY_SRC.is_dir():
        return out
    for rs in sorted(GATEWAY_SRC.rglob("*.rs")):
        rel = rs.relative_to(ROOT).as_posix()
        for i, line in enumerate(rs.read_text(encoding="utf-8", errors="replace").splitlines()):
            # Independent checks (not elif), and finditer over resources, so a
            # one-line `web::resource("…").route(…)` — or several resource literals
            # on one line — are all recorded, not just the first match.
            for rm in RESOURCE_RE.finditer(line):
                out.append({"kind": "web::resource", "literal": rm.group(1), "file": rel, "line": i + 1})
            if ROUTE_CALL_RE.search(line):
                out.append({"kind": ".route", "literal": "", "file": rel, "line": i + 1})
    return out


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


def render(routes: list[dict], scopes: dict[str, str], oapi: list[dict], reg_candidates: list[dict], commit: str) -> str:
    # Match by (METHOD, normalized-path) pairs, not path alone — otherwise a path
    # documented for one verb but routed under another would falsely count as
    # matched and be hidden from the unmatched section (issue #2112, PR4 review).
    oapi_ops = {(m, norm(e["path"])) for e in oapi for m in (e["methods"] or ["?"])}
    matched_ops: set = set()
    by_method: dict[str, int] = {}
    documented = 0
    rows = []
    for r in sorted(routes, key=lambda x: (x["module"], x["path"], x["method"])):
        by_method[r["method"]] = by_method.get(r["method"], 0) + 1
        scope = scopes.get(r["module"], "")
        fp = full_path(scope, r["path"])
        candidates = {norm(fp), norm(scope + (r["path"] if r["path"].startswith("/") else "/" + r["path"])), norm(r["path"])}
        hit = {(r["method"], c) for c in candidates} & oapi_ops
        documented_here = bool(hit)
        if documented_here:
            documented += 1
            matched_ops |= hit
        rows.append((r, fp, "yes" if documented_here else "no"))

    # OpenAPI operations (method + path) that no discovered gateway route matched.
    unmatched_ops = [
        {"method": m, "path": e["path"]}
        for e in oapi
        for m in (e["methods"] or ["?"])
        if (m, norm(e["path"])) not in matched_ops
    ]

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
    out.append(f"- Matched as documented (best-effort method+path match): {documented}")
    out.append(f"- Not matched to OpenAPI (best-effort): {undoc} (~{undoc / total * 100:.0f}% of discovered)" if total else "- Not matched to OpenAPI: 0")
    out.append(f"- Documented share of discovered routes: ~{pct_doc:.1f}%")
    out.append(f"- **OpenAPI operations (method + path) not matched to a discovered gateway route: {len(unmatched_ops)}** (see section below)")
    out.append("")
    out.append("> The gap is structural: only handlers hand-annotated for utoipa reach the OpenAPI spec. "
               "Of the OpenAPI paths, several belong to `icn-governance-actor` HTTP handlers that live outside "
               "the gateway crate and are not captured by this macro scan — so the documented/undocumented "
               "counts here are a best-effort comparison, while the two headline counts (discovered macros, "
               "OpenAPI paths) are the robust measured facts.")
    out.append("")
    out.append("## OpenAPI paths not matched to discovered gateway routes")
    out.append("")
    out.append("These OpenAPI-documented paths did **not** mechanically match any discovered gateway route macro. "
               "That is expected when a handler is documented via `#[utoipa::path]` but **registered outside the "
               "scanned `icn/crates/icn-gateway/src/**` tree** — e.g. the governance app (`icn-governance-actor` = "
               "`icn/apps/governance`) documents its `/gov/*` handlers with utoipa and mounts them via "
               "`web::resource(\"…\").route(…)` in `apps/governance/src/http/configure.rs` (no attribute macros). "
               "OpenAPI presence does **not** prove a runtime route exists or is mounted; these stay "
               "`unknown / needs local verification`.")
    out.append("")
    if unmatched_ops:
        out.append("| Method | OpenAPI path | Matched gateway route | Status | Claim safety |")
        out.append("|---|---|---|---|---|")
        for op in sorted(unmatched_ops, key=lambda x: (x["path"], x["method"])):
            out.append(f"| {op['method']} | `{md_table_escape(op['path'])}` | no | "
                       "unknown / needs local verification | needs review |")
        out.append("")
    else:
        out.append("- None: every OpenAPI operation matched a discovered gateway route.")
        out.append("")
    out.append("## Limitations")
    out.append("")
    out.append("- **Full mounted-path resolution is best-effort.** The `Group` column is `/v1` + a `web::scope(\"…\")` "
               "segment associated from `server.rs` `.service(...)`/`.configure(...)` registration sites (matched at "
               "identifier boundaries, keyed by the handler's top-level `icn/crates/icn-gateway/src/api/<group>` "
               "module) + the relative macro path. It is **not** a real Rust parser and may be wrong for "
               "deeply-nested or conditional scopes. The relative macro `Path` and the `Source` file are authoritative.")
    out.append("- The route table is **attribute macros only**. Macro-less `web::resource(\"…\")`/`.route(…)` "
               "registrations are now **flagged** below (see *Unparsed route-registration candidates*) but **not** "
               "parsed into routes; out-of-crate handlers (e.g. `icn-governance-actor::http`) are still not captured.")
    out.append("- This is **generated-map work, not API documentation completion** (issue #2112). It does not add or "
               "change any OpenAPI content or runtime behavior.")
    out.append("")
    out.append("## Routes")
    out.append("")
    out.append("Status vocabulary is the existing set from `source-of-truth-map.md`; no new labels are introduced. "
               "A static macro scan proves only that a routing macro is **declared** in source — not that the handler "
               "is mounted and served (registration happens elsewhere via `.service(...)` / `.configure(...)`, which "
               "this pass does not resolve). This scan therefore **cannot** assert `gateway-backed`: every discovered route's "
               "status defaults to `unknown / needs local verification` and claim-safety to `needs review`, pending "
               "human or runtime confirmation.")
    out.append("")
    out.append("| Method | Path (macro) | Group (best-effort) | Source | Handler | OpenAPI | Status | Claim safety |")
    out.append("|---|---|---|---|---|---|---|---|")
    for r, fp, doc in rows:
        out.append(
            f"| {r['method']} | `{md_table_escape(r['path'] or '(empty)')}` | `{md_table_escape(fp)}` | "
            f"`{r['file']}`:{r['line']} | `{md_table_escape(r['handler'])}` | {doc} | unknown / needs local verification | needs review |"
        )
    out.append("")

    # Macro-less route-registration candidates (issue #2112, PR3) — flagged, not parsed.
    res_n = sum(1 for c in reg_candidates if c["kind"] == "web::resource")
    route_n = sum(1 for c in reg_candidates if c["kind"] == ".route")
    out.append("## Unparsed route-registration candidates")
    out.append("")
    out.append("Beyond attribute macros, Actix can also register routes via "
               "`web::resource(\"…\").route(web::<method>().to(handler))`. This scan **flags** such sites "
               "mechanically but does **not** parse them into routes: a regex cannot reliably tell a production "
               "registration from a **test fixture** (e.g. inside `#[cfg(test)]` / `test::init_service`), and it does "
               "not resolve the mounted path without a real parser. They are **not** counted in the route total "
               "above and remain `unknown / needs local verification` — treat them as leads for human review.")
    out.append("")
    out.append(f"- **Candidates flagged: {len(reg_candidates)}** "
               f"(`web::resource(\"…\")`: {res_n} · `.route(…)`: {route_n})")
    out.append("")
    if reg_candidates:
        out.append("| Kind | Resource literal | Source |")
        out.append("|---|---|---|")
        for c in sorted(reg_candidates, key=lambda x: (x["file"], x["line"])):
            lit = f"`{md_table_escape(c['literal'])}`" if c["literal"] else "—"
            out.append(f"| `{c['kind']}` | {lit} | `{c['file']}`:{c['line']} |")
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
    reg_candidates = resource_candidates()
    if not routes:
        print(f"ERROR: no routes discovered under {GATEWAY_SRC} — wrong repo root?", file=sys.stderr)
        return 2
    content = render(routes, scopes, oapi, reg_candidates, commit)

    if args.write:
        ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
        ARTIFACT.write_text(content + "\n", encoding="utf-8")
        print(f"wrote {ARTIFACT.relative_to(ROOT)}: {len(routes)} routes, {len(oapi)} OpenAPI paths, "
              f"{len(reg_candidates)} non-macro candidates")
        return 0

    # --check
    if not ARTIFACT.is_file():
        print(f"STALE: {ARTIFACT.relative_to(ROOT)} does not exist — run --write", file=sys.stderr)
        return 1
    committed = ARTIFACT.read_text(encoding="utf-8")
    if strip_volatile(committed).strip() == strip_volatile(content).strip():
        print(f"OK: route inventory up to date ({len(routes)} routes, {len(oapi)} OpenAPI paths, "
              f"{len(reg_candidates)} non-macro candidates)")
        return 0
    print(f"STALE: {ARTIFACT.relative_to(ROOT)} differs from a fresh scan — run --write and commit", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
