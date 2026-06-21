#!/usr/bin/env python3
"""Generate the ICN Live State Overlay v0 — bounded session-start grounding.

On-demand, read-only orientation for agents and humans starting work on the ICN
repo. It answers, at session start: what is the current repo/project state, which
facts are canonical vs generated-reference, what recently changed, what is stale,
what must NOT be claimed, which lanes own the next work, and what checks to run.

Design constraints (see docs/ai/ICN_LIVE_STATE_OVERLAY_TEMPLATE.md):

- **On-demand, not committed.** Default output is stdout. There is intentionally NO
  committed "live snapshot" file — a committed snapshot rots. Use `--output PATH` to
  write a local copy you will not commit.
- **No network required for a useful overlay.** Canonical docs, generated artifacts,
  and git are all local. GitHub (`gh`) is consulted ONLY for live PR/issue state and
  is clearly labeled live-reconfirmed; without it those fields are marked
  `NEEDS_LIVE_RECONFIRMATION`, never guessed. Pass `--no-gh` to skip it.
- **Every claim is source- or freshness-bound.** Nothing in this overlay asserts more
  than the canonical state docs support; claim boundaries are stated explicitly.

Stdlib only. Usage:

    python3 scripts/generate-live-state-overlay.py                 # markdown to stdout
    python3 scripts/generate-live-state-overlay.py --format json   # json to stdout
    python3 scripts/generate-live-state-overlay.py --output /tmp/overlay.md
    python3 scripts/generate-live-state-overlay.py --no-gh         # no GitHub calls
    python3 scripts/generate-live-state-overlay.py --check         # self-validate, exit 0/1
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

# Resolve the repo root from this script's location (scripts/ is at repo root).
ROOT = Path(__file__).resolve().parent.parent

RECONFIRM = "NEEDS_LIVE_RECONFIRMATION"

# Curated v0 set of active grounding/work lanes. Bounded knowledge: the *list* is
# curated here; each lane's live OPEN/CLOSED status is reconfirmed via gh at run
# time (or marked NEEDS_LIVE_RECONFIRMATION). Reconfirm the list itself against
# GitHub if it looks stale.
ACTIVE_LANES = [
    ("#2115", "Live State Overlay v0 — agent session-start grounding (this lane)"),
    ("#2113", "Role-based icnctl command map (live / partial / fixture)"),
    ("#2114", "Searchable invariants catalog linking each invariant to evidence"),
    ("#2112", "Route inventory / OpenAPI / public-API-claim discipline"),
    ("#2047", "Doc-freshness audit — stale ARCHITECTURE.md sections + SME review"),
    ("#2099", "CodeQL / public-surface security follow-up"),
    ("#2082", "coop_id<->EntityId mapping (auth sequence step 1)"),
    ("#2080", "Trusted positive token issuance path (auth sequence step 2; security-sensitive)"),
    ("#2081", "Treasury entity-auth enforcement cutover (auth sequence step 3)"),
]

# Claim boundaries (doctrine). These are PROHIBITIONS — the overclaim scan
# deliberately skips this section because it states what NOT to claim.
CLAIM_BOUNDARIES = [
    (
        "NYCN is a partner-track / private operating context, NOT a public formal pilot. "
        "Do not represent NYCN as a launched, signed, or committed pilot."
    ),
    (
        "ICN must NOT be claimed production-ready. Some surfaces are mature; the substrate "
        "as a whole is not production-hardened."
    ),
    "Live federation between cooperatives is NOT complete/deployed (Phase 3, not Phase 2).",
    (
        "Entity-aware authorization is NOT production-enforced. Optional entity_id/entity_type "
        "token claims are non-enforcing; the trusted-issuance source is fail-closed and unwired "
        "(see #2080)."
    ),
    (
        "The route inventory is route DISCOVERY / registration-candidate evidence — NOT proof "
        "of API correctness, auth, mounting, runtime health, or OpenAPI completeness."
    ),
    (
        "Generated artifacts (agent-context-spine, repo file-record, route inventory) are "
        "orientation / reference layers — NOT canonical truth roots. Canonical state is "
        "docs/STATE.md + docs/PHASE_PROGRESS.md."
    ),
    (
        "Private NYCN / summit / operator / partner material must NEVER be published into "
        "public ICN docs."
    ),
]
CLAIM_BOUNDARY_SOURCES = [
    "docs/reference/project-index/show-readiness-map.md",
    "docs/reference/project-index/source-of-truth-map.md",
    "docs/status.toml",
]

AGENT_START_RULES = [
    "Generate/read this Live State Overlay before planning repo work.",
    (
        "Read the relevant Agent Context Spine path brief for the files you will touch "
        "(`python3 scripts/generate-agent-context-spine.py --brief <paths>`)."
    ),
    (
        "Identify which facts are canonical (docs/STATE.md, docs/PHASE_PROGRESS.md) vs "
        "generated-reference (the grounding artifacts)."
    ),
    (
        "Identify the required verification checks for the path/subsystem you touch "
        "(see the spine path brief and AGENTS.md change-routing)."
    ),
    (
        "Identify the claim hazards in the claim_boundaries section before writing any "
        "public/docs/PR copy."
    ),
    (
        "Reconfirm anything marked NEEDS_LIVE_RECONFIRMATION against GitHub/live state "
        "before relying on it."
    ),
    "Only then plan the work. Do not merge anything without explicit per-PR authorization.",
]

# Positive overclaim phrases that must never appear in the fact-bearing sections.
OVERCLAIM_PHRASES = [
    "production ready",
    "production-ready",
    "live federation is deployed",
    "fully federated",
    "formal pilot launched",
    "entity auth is enforced",
    "entity-aware auth enforced",
    "entity-aware authorization is enforced",
    "api is fully documented",
    "openapi fully documents",
]

# Sections excluded from the overclaim scan because they negate / prohibit by design.
_NON_FACT_SECTIONS = {"claim_boundaries", "agent_start_rules"}


def _run(args: list[str], timeout: int = 10) -> str | None:
    """Run a command, return stripped stdout, or None on any failure."""
    try:
        out = subprocess.run(
            args, cwd=str(ROOT), capture_output=True, text=True, timeout=timeout
        )
    except Exception:
        return None
    if out.returncode != 0:
        return None
    return out.stdout.strip()


def _front_matter(rel: str) -> dict[str, str]:
    """Extract simple `Key: value` YAML front-matter scalars from a doc."""
    path = ROOT / rel
    fm: dict[str, str] = {}
    try:
        text = path.read_text(encoding="utf-8")
    except Exception:
        return fm
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return fm
    for line in lines[1:]:
        if line.strip() == "---":
            break
        if ":" in line:
            k, _, v = line.partition(":")
            fm[k.strip()] = v.strip()
    return fm


def _freshness(rel: str) -> str:
    """Best-effort freshness string from a doc's front-matter or a leading
    `Last Updated` / `Last Reviewed` line (markdown bold form included)."""
    fm = _front_matter(rel)
    for key in ("Last Reviewed", "Last verified", "Last Updated", "Generated"):
        if key in fm and fm[key]:
            return f"{key} {fm[key]}"
    # Fallback: scan the first few lines for a bold/plain "Last Updated/Reviewed" line.
    try:
        for line in (ROOT / rel).read_text(encoding="utf-8").splitlines()[:8]:
            stripped = line.replace("*", "").strip()
            for key in ("Last Updated:", "Last Reviewed:", "Last verified:"):
                if stripped.startswith(key):
                    return stripped[: len(key) + 16].strip()
    except Exception:
        pass
    return RECONFIRM


def _exists(rel: str) -> bool:
    return (ROOT / rel).exists()


# --- sections -----------------------------------------------------------------


def section_repo_snapshot(now: str) -> dict:
    head = _run(["git", "rev-parse", "HEAD"]) or RECONFIRM
    branch = _run(["git", "rev-parse", "--abbrev-ref", "HEAD"]) or RECONFIRM
    porcelain = _run(["git", "status", "--porcelain"])
    if porcelain is None:
        working_tree = RECONFIRM
    else:
        working_tree = "clean" if porcelain == "" else f"dirty ({len(porcelain.splitlines())} changed)"
    return {
        "source_commit": head,
        "branch": branch,
        "generated_at": now,
        "working_tree": working_tree,
        "evidence": "git rev-parse HEAD ; git rev-parse --abbrev-ref HEAD ; git status --porcelain (local)",
        "caveat": "Local working-tree snapshot at generation time. Reconfirm against origin/main "
        "(git fetch && git rev-parse origin/main) before assuming this is the merged base.",
    }


def section_canonical_state() -> list[dict]:
    phase = RECONFIRM
    pp = ROOT / "docs/PHASE_PROGRESS.md"
    try:
        for line in pp.read_text(encoding="utf-8").splitlines():
            if line.startswith("**Current Phase:**"):
                phase = line.split("**Current Phase:**", 1)[1].strip()[:160]
                break
    except Exception:
        pass
    return [
        {
            "doc": "Current project state (per-PR record)",
            "path": "docs/STATE.md",
            "canonical": True,
            "freshness": _freshness("docs/STATE.md"),
            "note": "Canonical current state. Read the newest `[sync edit]` block at the top.",
            "caveat": "Canonical; reconfirm the newest sync block reflects origin/main HEAD.",
        },
        {
            "doc": "Phase tracking",
            "path": "docs/PHASE_PROGRESS.md",
            "canonical": True,
            "freshness": _freshness("docs/PHASE_PROGRESS.md"),
            "note": f"Current phase: {phase}",
            "caveat": "Canonical; phase posture changes only via PHASE_PROGRESS/STATE sync edits.",
        },
        {
            "doc": "Reasoning foundation (constitutional core)",
            "path": "docs/ai/ICN_CONSTITUTIONAL_CORE.md",
            "canonical": False,
            "freshness": _freshness("docs/ai/ICN_CONSTITUTIONAL_CORE.md"),
            "note": "Stable agent reasoning foundation; rarely changes.",
            "caveat": "Process/reasoning doc, not a state claim.",
        },
    ]


def _latest_handoff() -> str:
    hand = sorted((ROOT / "docs/dev").glob("handoff-*.md"))
    return f"docs/dev/{hand[-1].name}" if hand else f"none found ({RECONFIRM})"


def _read_json(rel: str) -> dict | None:
    try:
        return json.loads((ROOT / rel).read_text(encoding="utf-8"))
    except Exception:
        return None


def section_grounding_artifacts() -> list[dict]:
    out: list[dict] = []
    spine = _read_json("docs/reference/project-index/generated/agent-context-spine.json")
    if spine is not None:
        c = spine.get("counts") or {}
        out.append({
            "artifact": "Agent Context Spine v0",
            "path": "docs/reference/project-index/generated/agent-context-spine.json",
            "classification": "generated-reference (canonical: false)",
            "generated": spine.get("generated", RECONFIRM),
            "source_commit": spine.get("source_commit", RECONFIRM),
            "metrics": f"{c.get('nodes', '?')} nodes / {c.get('edges', '?')} edges",
            "caveat": "Orientation map of crates/subsystems/docs/routes/invariants/claim-surfaces "
            "+ per-path code-quality briefs. NOT a truth root. Regenerate with "
            "scripts/generate-agent-context-spine.py --write.",
        })
    rec = _read_json("docs/reference/project-index/generated/icn-file-record.json")
    if rec is not None:
        out.append({
            "artifact": "Repo file-record snapshot",
            "path": "docs/reference/project-index/generated/icn-file-record.{json,md}",
            "classification": "generated-reference (mechanical inventory)",
            "generated": rec.get("generated_at", RECONFIRM),
            "source_commit": rec.get("head", RECONFIRM),
            "metrics": f"repo={rec.get('repo', '?')}; recorded at the commit above",
            "caveat": "Mechanical git ls-files + metadata inventory. Regenerate with "
            "scripts/generate_repo_record.py --repo icn=.",
        })
    ri = "docs/reference/project-index/generated/route-inventory.md"
    if _exists(ri):
        macros = RECONFIRM
        try:
            for line in (ROOT / ri).read_text(encoding="utf-8").splitlines():
                if "Discovered gateway route macros" in line:
                    macros = line.strip().lstrip("- ").strip()
                    break
        except Exception:
            pass
        out.append({
            "artifact": "Gateway route inventory",
            "path": ri,
            "classification": "generated-reference (route DISCOVERY evidence)",
            "generated": _front_matter(ri).get("Generated", RECONFIRM),
            "source_commit": "see file Snapshot section",
            "metrics": macros,
            "caveat": "Route declarations/registration candidates only — NOT auth correctness, "
            "mounting, tests, runtime health, or OpenAPI completeness. Check with "
            "python3 docs/scripts/route_inventory.py --check.",
        })
    return out


def section_recent_completed() -> dict:
    log = _run(["git", "log", "--oneline", "-12", "HEAD"])
    merges = log.splitlines() if log else []
    return {
        "source": "git log --oneline -12 HEAD (local main history)",
        "caveat": "Historical context (merged commits). NOT live truth — reconfirm any "
        "issue/PR open/closed state via GitHub. Spine #2128, truth-sync #2129, and repo "
        "file-record #2130 grounding work landed in this window.",
        "recent_merges": merges or [RECONFIRM],
    }


def section_active_lanes(use_gh: bool) -> list[dict]:
    out: list[dict] = []
    for issue, desc in ACTIVE_LANES:
        status = RECONFIRM
        source = "curated v0 lane set; reconfirm via `gh issue view %s`" % issue.lstrip("#")
        if use_gh:
            res = _run(["gh", "issue", "view", issue.lstrip("#"), "--repo",
                        "InterCooperative-Network/icn", "--json", "state"])
            if res:
                try:
                    st = json.loads(res).get("state", "")
                    if st:
                        status = f"{st} (live-reconfirmed {datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')})"
                        source = f"GitHub issue {issue}"
                except Exception:
                    pass
        out.append({"lane": issue, "description": desc, "status": status, "source": source})
    return out


def section_next_safe_targets() -> list[dict]:
    note = ("RECOMMENDED, not AUTHORIZED. These are source-bound suggestions; an agent may "
            "NOT push/merge without explicit per-PR authorization.")
    return [
        {
            "target": "Spine follow-up PR B — CI gate for plugin + spine validators",
            "rationale": "check-claude-plugin.py, check-claude-plugin-root-resolution.py, and "
            "check-agent-context-spine.py exist on main but are referenced by no workflow.",
            "source": "scripts/ on main + .github/workflows/ (grep)",
            "authorization": note,
        },
        {
            "target": "#2113 role-based icnctl command map",
            "rationale": "Small generated-reference doc lane; aligns with this grounding series.",
            "source": "GitHub issue #2113",
            "authorization": note,
        },
        {
            "target": "#2114 searchable invariants catalog",
            "rationale": "Links the five invariants to evidence; bounded docs lane.",
            "source": "GitHub issue #2114",
            "authorization": note,
        },
        {
            "target": "AVOID as a casual pick: #2080 trusted positive issuance",
            "rationale": "Security-sensitive auth lane; requires a proven authority source. Not a "
            "routine grounding slice.",
            "source": "GitHub issue #2080 + docs/rfcs/RFC-0018",
            "authorization": note,
        },
    ]


def build_overlay(use_gh: bool) -> dict:
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return {
        "schema": "icn-live-state-overlay/v0",
        "generated_at": now,
        "generator": "scripts/generate-live-state-overlay.py",
        "gh_consulted": bool(use_gh),
        "what_this_is": "Bounded, on-demand session-start grounding overlay. NOT canonical truth; "
        "canonical state is docs/STATE.md + docs/PHASE_PROGRESS.md. NOT a committed snapshot.",
        "repo_snapshot": section_repo_snapshot(now),
        "canonical_state": section_canonical_state(),
        "grounding_artifacts": section_grounding_artifacts(),
        "recent_completed_grounding_work": section_recent_completed(),
        "latest_handoff": _latest_handoff(),
        "active_work_lanes": section_active_lanes(use_gh),
        "claim_boundaries": {"sources": CLAIM_BOUNDARY_SOURCES, "boundaries": CLAIM_BOUNDARIES},
        "agent_start_rules": AGENT_START_RULES,
        "next_safe_targets": section_next_safe_targets(),
    }


REQUIRED_SECTIONS = [
    "repo_snapshot",
    "canonical_state",
    "grounding_artifacts",
    "recent_completed_grounding_work",
    "active_work_lanes",
    "claim_boundaries",
    "agent_start_rules",
    "next_safe_targets",
]


def to_markdown(o: dict) -> str:
    L: list[str] = []
    L.append("# ICN Live State Overlay v0")
    L.append("")
    L.append(f"> {o['what_this_is']}")
    L.append(f"> Generated: `{o['generated_at']}` · GitHub consulted: `{o['gh_consulted']}` · "
             "Source/freshness caveats are inline. Reconfirm `NEEDS_LIVE_RECONFIRMATION` fields.")
    L.append("")
    rs = o["repo_snapshot"]
    L.append("## 1. repo_snapshot")
    L.append(f"- source_commit: `{rs['source_commit']}`  ·  branch: `{rs['branch']}`  ·  "
             f"working_tree: `{rs['working_tree']}`  ·  generated_at: `{rs['generated_at']}`")
    L.append(f"- evidence: `{rs['evidence']}`")
    L.append(f"- caveat: {rs['caveat']}")
    L.append("")
    L.append("## 2. canonical_state")
    for e in o["canonical_state"]:
        L.append(f"- **{e['doc']}** (`{e['path']}`, canonical={e['canonical']}) — freshness: "
                 f"`{e['freshness']}`")
        L.append(f"  - {e['note']}")
        L.append(f"  - caveat: {e['caveat']}")
    L.append("")
    L.append("## 3. grounding_artifacts")
    for e in o["grounding_artifacts"]:
        L.append(f"- **{e['artifact']}** (`{e['path']}`) — {e['classification']}")
        L.append(f"  - generated: `{e['generated']}` · source_commit: `{e['source_commit']}` · "
                 f"{e['metrics']}")
        L.append(f"  - caveat: {e['caveat']}")
    L.append("")
    rc = o["recent_completed_grounding_work"]
    L.append("## 4. recent_completed_grounding_work")
    L.append(f"- source: `{rc['source']}`")
    L.append(f"- caveat: {rc['caveat']}")
    for m in rc["recent_merges"]:
        L.append(f"  - `{m}`")
    L.append(f"- latest_handoff: `{o['latest_handoff']}`")
    L.append("")
    L.append("## 5. active_work_lanes")
    for e in o["active_work_lanes"]:
        L.append(f"- {e['lane']} — {e['description']}")
        L.append(f"  - status: `{e['status']}`  ·  source: {e['source']}")
    L.append("")
    cb = o["claim_boundaries"]
    L.append("## 6. claim_boundaries")
    L.append(f"- sources: {', '.join('`%s`' % s for s in cb['sources'])}")
    for b in cb["boundaries"]:
        L.append(f"- {b}")
    L.append("")
    L.append("## 7. agent_start_rules")
    for i, r in enumerate(o["agent_start_rules"], 1):
        L.append(f"{i}. {r}")
    L.append("")
    L.append("## 8. next_safe_targets")
    for e in o["next_safe_targets"]:
        L.append(f"- **{e['target']}**")
        L.append(f"  - rationale: {e['rationale']}")
        L.append(f"  - source: {e['source']}")
        L.append(f"  - authorization: {e['authorization']}")
    L.append("")
    return "\n".join(L)


def check(o: dict) -> list[str]:
    """Return a list of problems; empty list = overlay passes."""
    problems: list[str] = []

    for sec in REQUIRED_SECTIONS:
        if sec not in o:
            problems.append(f"missing required section: {sec}")

    rs = o.get("repo_snapshot", {})
    if not rs.get("source_commit") or not rs.get("generated_at"):
        problems.append("repo_snapshot missing source_commit/generated_at")

    # Every canonical_state and grounding_artifact entry must carry a source path
    # and a freshness/caveat marker.
    for e in o.get("canonical_state", []):
        if not e.get("path") or not (e.get("freshness") or e.get("caveat")):
            problems.append(f"canonical_state entry lacks path/freshness: {e.get('doc')}")
    for e in o.get("grounding_artifacts", []):
        if not e.get("path") or not e.get("caveat"):
            problems.append(f"grounding_artifact lacks path/caveat: {e.get('artifact')}")

    # Every active lane must carry a status + source.
    for e in o.get("active_work_lanes", []):
        if not e.get("status") or not e.get("source"):
            problems.append(f"active lane lacks status/source: {e.get('lane')}")

    # claim_boundaries must cover the required hazards.
    cb_text = json.dumps(o.get("claim_boundaries", {})).lower()
    for kw in ("nycn", "production", "federation", "entity", "route inventory",
               "generated", "private"):
        if kw not in cb_text:
            problems.append(f"claim_boundaries missing required topic: {kw}")

    # No positive overclaim phrase may appear in the fact-bearing sections.
    fact = {k: v for k, v in o.items() if k not in _NON_FACT_SECTIONS}
    fact_text = json.dumps(fact).lower()
    for phrase in OVERCLAIM_PHRASES:
        if phrase in fact_text:
            problems.append(f"overclaim phrase in fact sections: {phrase!r}")

    # JSON must round-trip.
    try:
        json.loads(json.dumps(o))
    except Exception as exc:  # pragma: no cover
        problems.append(f"overlay does not serialize to JSON: {exc}")

    # Markdown must carry source/freshness caveats.
    md = to_markdown(o).lower()
    if "caveat" not in md or "freshness" not in md or "needs_live_reconfirmation" not in md.replace("-", "_"):
        problems.append("markdown output lacks source/freshness/reconfirmation caveats")

    return problems


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate the ICN Live State Overlay v0.")
    ap.add_argument("--format", choices=["markdown", "json"], default="markdown")
    ap.add_argument("--output", type=Path, help="write to this path instead of stdout")
    ap.add_argument("--no-gh", action="store_true",
                    help="do not consult GitHub; live PR/issue fields are marked NEEDS_LIVE_RECONFIRMATION")
    ap.add_argument("--check", action="store_true",
                    help="self-validate a freshly generated overlay and exit 0 (ok) / 1 (problems)")
    args = ap.parse_args()

    overlay = build_overlay(use_gh=not args.no_gh)

    if args.check:
        problems = check(overlay)
        if problems:
            print("LIVE-STATE-OVERLAY CHECK FAILED:", file=sys.stderr)
            for p in problems:
                print(f"  - {p}", file=sys.stderr)
            return 1
        print(f"OK: live state overlay valid ({len(REQUIRED_SECTIONS)} required sections, "
              "all claims source/freshness-bound, no overclaims).")
        return 0

    rendered = json.dumps(overlay, indent=2) if args.format == "json" else to_markdown(overlay)
    if args.output:
        args.output.write_text(rendered + "\n", encoding="utf-8")
        print(f"wrote {args.output}", file=sys.stderr)
    else:
        print(rendered)
    return 0


if __name__ == "__main__":
    sys.exit(main())
