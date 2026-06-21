#!/usr/bin/env python3
"""Generate the ICN Live State Overlay v0 — bounded whole-repo orientation.

On-demand, read-only orientation for agents and humans working on the ICN repo. It
is both a bird's-eye comprehension layer for the whole project AND a session-start
grounding overlay. It answers: what is the integrated system (project_map), what
subsystems exist and where their code/docs/checks live (subsystem_overview), which
systems already exist so you do NOT reinvent them (repo_systems), how those systems
relate (system_interactions), how to change the repo safely (development_safety_map),
plus the current state, which facts are canonical vs generated-reference, what
recently changed, what must NOT be claimed, which lanes own the next work, and what
checks to run.

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

def repo_root() -> Path:
    """Repo root via `git rev-parse --show-toplevel`, falling back to this script's
    location (scripts/ is at repo root). Mirrors scripts/generate-agent-context-spine.py."""
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=Path(__file__).resolve().parent,
            capture_output=True,
            text=True,
            timeout=5,
        )
        if out.returncode == 0 and out.stdout.strip():
            return Path(out.stdout.strip())
    except Exception:
        # Not a git checkout (or git unavailable): fall through to the __file__ fallback.
        pass
    return Path(__file__).resolve().parents[1]


ROOT = repo_root()

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
        "NYCN is a partner-track / private operating context, NOT a public formal pilot. "
        "Do not represent NYCN as a launched, signed, or committed pilot."
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
_NON_FACT_SECTIONS = {"claim_boundaries", "agent_start_rules", "development_safety_map"}


# --- whole-repo / whole-project orientation (v0) --------------------------------
# These broaden the overlay from session-start status into a bird's-eye comprehension
# layer: the integrated system model, the subsystem map, the index of systems that
# already exist (so agents do not reinvent them), how those systems interact, and the
# safety path for changing the repo. ORIENTATION ONLY — none of this is a new truth
# root; canonical state remains docs/STATE.md + docs/PHASE_PROGRESS.md.

# Bird's-eye system sequence. classification is "orientation-only" for every stage:
# a navigation model sourced from CLAUDE.md "Core Subsystems" + docs/ARCHITECTURE.md,
# not a doctrine. Ordered along the substrate's core dependency flow.
PROJECT_MAP = [
    (
        "identity",
        "DIDs + Ed25519 keys answer WHO an actor is (identity, never authority on its own).",
        "icn-identity, icn-naming · CLAUDE.md Core Subsystems / Identity & Keystore",
    ),
    (
        "trust",
        "Web-of-participation trust graph, consumed by a PolicyOracle (meaning firewall).",
        "icn-trust, apps/trust · CLAUDE.md TrustPolicyOracle Flow",
    ),
    (
        "entity / membership",
        (
            "Unified entity model (individual/coop/federation) + membership — the authority "
            "layer RFC-0018 is hardening; not yet production-enforced."
        ),
        "icn-entity, icn-coop, icn-community, apps/membership · RFC-0018",
    ),
    (
        "networking / federation",
        (
            "QUIC/TLS sessions, gossip replication, and the inter-cooperative federation "
            "protocol. Cross-coop live federation is a later phase."
        ),
        "icn-net, icn-protocol, icn-gossip, icn-federation · CLAUDE.md Core Subsystems",
    ),
    (
        "state / storage",
        "Durable substrate state: Sled-backed KV, snapshots, encoding.",
        "icn-store, icn-snapshot, icn-encoding · CLAUDE.md Workspace Structure",
    ),
    (
        "governance",
        (
            "Proposals/voting and CCL Policy Oracles that translate domain meaning into "
            "generic constraints the kernel enforces blindly."
        ),
        "icn-governance, icn-ccl, apps/governance · CLAUDE.md Meaning Firewall",
    ),
    (
        "economics",
        (
            "Mutual-credit double-entry ledger + settlement engine. Regulatory framing is "
            "settlement, never payment/token."
        ),
        "icn-ledger, apps/ledger · CLAUDE.md Core Subsystems",
    ),
    (
        "applications / interfaces",
        (
            "How humans and apps reach the substrate: gateway REST/WS, SDKs, member-shell, "
            "icnctl / icn-console."
        ),
        "icn-gateway, sdk/*, web/member-shell, bins/* · CLAUDE.md Workspace Structure",
    ),
]

# Curated v0 subsystem map. Each entry points at the existing code/docs for a subsystem
# plus a verification hint. `spine_key` ties it to an agent-context-spine subsystem node
# when one exists, so section_subsystem_overview() attaches the crates the spine maps to
# that subsystem (derived from owned_by_subsystem edges, not hard-coded). Entries with
# spine_key=None are explicitly curated (the spine has no subsystem node for them yet).
SUBSYSTEM_OVERVIEW = [
    {
        "subsystem": "identity",
        "spine_key": "identity",
        "paths": "icn/crates/icn-identity, icn/crates/icn-naming",
        "docs": "CLAUDE.md Identity & Keystore",
        "status": "implemented — DID generation, Ed25519, age-encrypted keystore (v1->v4).",
        "check": "cargo test -p icn-identity",
    },
    {
        "subsystem": "trust",
        "spine_key": "trust",
        "paths": "icn/crates/icn-trust, icn/apps/trust",
        "docs": "CLAUDE.md TrustPolicyOracle Flow",
        "status": "implemented — trust graph + transitive computation feeding a PolicyOracle.",
        "check": "cargo test -p icn-trust",
    },
    {
        "subsystem": "entity / membership",
        "spine_key": None,
        "paths": "icn/crates/icn-entity, icn/crates/icn-coop, icn/crates/icn-community, "
        "icn/apps/membership",
        "docs": "RFC-0018; ADR-0035",
        "status": "partial — entity model + CoopEntityMap landed; entity-aware authorization "
        "is NOT production-enforced (non-enforcing claims; fail-closed unwired source #2080). "
        "Spine maps icn-entity/icn-coop/icn-community under subsystem:governance.",
        "check": "cargo test -p icn-entity",
    },
    {
        "subsystem": "governance",
        "spine_key": "governance",
        "paths": "icn/crates/icn-governance, icn/apps/governance",
        "docs": "CLAUDE.md Meaning Firewall / PolicyOracle Pattern",
        "status": "implemented — proposals/voting + PolicyOracle wiring.",
        "check": "cargo test -p icn-governance",
    },
    {
        "subsystem": "economics / ledger",
        "spine_key": "ledger",
        "paths": "icn/crates/icn-ledger, icn/apps/ledger",
        "docs": "CLAUDE.md Core Subsystems (regulatory framing: settlement, not payment)",
        "status": "implemented — double-entry mutual credit + settlement engine.",
        "check": "cargo test -p icn-ledger",
    },
    {
        "subsystem": "network / federation",
        "spine_key": "networking",
        "paths": "icn/crates/icn-net, icn/crates/icn-protocol, icn/crates/icn-gossip, "
        "icn/crates/icn-federation",
        "docs": "CLAUDE.md Key Protocols",
        "status": "implemented transport/gossip; cross-coop live federation is NOT deployed "
        "(later phase).",
        "check": "cargo test -p icn-net -p icn-gossip",
    },
    {
        "subsystem": "storage / state",
        "spine_key": None,
        "paths": "icn/crates/icn-store, icn/crates/icn-snapshot, icn/crates/icn-encoding",
        "docs": "CLAUDE.md Workspace Structure",
        "status": "implemented — Sled KV, snapshots, encoding. No spine subsystem node yet.",
        "check": "cargo test -p icn-store",
    },
    {
        "subsystem": "compute",
        "spine_key": "compute",
        "paths": "icn/crates/icn-compute",
        "docs": "CLAUDE.md Core Subsystems",
        "status": "present — trust-gated distributed task execution (PolicyOracle-gated).",
        "check": "cargo test -p icn-compute",
    },
    {
        "subsystem": "CCL / contracts",
        "spine_key": "contracts",
        "paths": "icn/crates/icn-ccl",
        "docs": "CLAUDE.md Cooperative Contract Language (CCL)",
        "status": "implemented — AST interpreter, fuel-metered, deterministic, not Turing-complete.",
        "check": "cargo test -p icn-ccl",
    },
    {
        "subsystem": "gateway / API",
        "spine_key": None,
        "paths": "icn/crates/icn-gateway, icn/crates/icn-api, docs/api/openapi.generated.yaml",
        "docs": "route-inventory.md; CLAUDE.md CI Failure Index (drift chain)",
        "status": "implemented REST/WS gateway; route inventory is DISCOVERY evidence, not "
        "API/auth/OpenAPI-completeness proof.",
        "check": "cargo test -p icn-gateway --features sled-storage",
    },
    {
        "subsystem": "SDK / client surfaces",
        "spine_key": None,
        "paths": "sdk/typescript, sdk/react-native, web/member-shell",
        "docs": "CLAUDE.md CI Failure Index (Check API Types Drift)",
        "status": "TS SDK + member-shell reference client exist; types are drift-checked against "
        "OpenAPI.",
        "check": "cd sdk/typescript && npm ci && npm run build && npm test",
    },
    {
        "subsystem": "docs / project-index",
        "spine_key": None,
        "paths": "docs/reference/project-index, docs/STATE.md, docs/PHASE_PROGRESS.md, "
        "docs/registry.toml",
        "docs": "docs/INDEX.md",
        "status": "canonical state (STATE/PHASE) + generated-reference index artifacts; "
        "doc-control enforced.",
        "check": "python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml",
    },
    {
        "subsystem": "website / public surface",
        "spine_key": None,
        "paths": "website/, web/member-shell",
        "docs": "claim_surface:public-website-claims (spine)",
        "status": "public-facing; bound by claim discipline (no production/pilot/federation "
        "overclaims).",
        "check": "respect claim_boundaries; see show-readiness-map.md",
    },
    {
        "subsystem": "agent tooling / Claude plugin / MCP",
        "spine_key": None,
        "paths": "tools/claude-code/plugins/icn-agent-pack, ops/mcp, "
        "scripts/generate-agent-context-spine.py",
        "docs": "doc:claude-code-plugin, doc:agent-mcp-tooling (spine)",
        "status": "present — preflight skill, agent-context-spine MCP tool, generators/checkers.",
        "check": "python3 scripts/check-claude-plugin.py; python3 scripts/check-agent-context-spine.py",
    },
]

# Index of systems that ALREADY EXIST in (or govern) this repo, so an agent does not
# reinvent them. classification in {canonical, generated-reference, advisory, operational,
# tooling}. `path` is repo-relative and existence-checked at generation time.
REPO_SYSTEMS = [
    {
        "system": "Agent Context Spine",
        "path": "docs/reference/project-index/generated/agent-context-spine.json",
        "classification": "generated-reference",
        "purpose": "Evidence-grounded orientation graph (crates/subsystems/docs/routes/invariants/"
        "claim-surfaces + per-path code-quality briefs).",
        "caveat": "Regenerate with scripts/generate-agent-context-spine.py --write. NOT a truth root.",
    },
    {
        "system": "Live State Overlay (this generator)",
        "path": "scripts/generate-live-state-overlay.py",
        "classification": "tooling",
        "purpose": "On-demand bird's-eye + session-start grounding overlay (this file).",
        "caveat": "stdout/--output only; NO committed snapshot; never cache across sessions.",
    },
    {
        "system": "Generated repo file-record",
        "path": "docs/reference/project-index/generated/icn-file-record.json",
        "classification": "generated-reference",
        "purpose": "Mechanical git ls-files + metadata inventory of the repo.",
        "caveat": "Regenerate with scripts/generate_repo_record.py --repo icn=. ; do not hand-edit.",
    },
    {
        "system": "Route inventory",
        "path": "docs/reference/project-index/generated/route-inventory.md",
        "classification": "generated-reference",
        "purpose": "Discovered gateway route macros / registration candidates.",
        "caveat": "Discovery evidence only. Check with python3 docs/scripts/route_inventory.py --check.",
    },
    {
        "system": "Doc registry / doc control",
        "path": "docs/registry.toml",
        "classification": "operational",
        "purpose": "Registry + freshness/control gate for governed docs.",
        "caveat": "Enforced by docs/scripts/doc_control_check.py; front-matter dates must match the "
        "registry.",
    },
    {
        "system": "Canonical truth / state docs",
        "path": "docs/STATE.md",
        "classification": "canonical",
        "purpose": "Current project state (with docs/PHASE_PROGRESS.md). The truth root.",
        "caveat": "Only these are canonical; everything else orients toward them.",
    },
    {
        "system": "Worktree-OS bookkeeping (worktree policy / file locks / merge queue)",
        "path": "docs/dev/AGENT_WORKTREE_POLICY.md",
        "classification": "operational",
        "purpose": "One task = one worktree = one branch = one PR; file-lock + merge-queue discipline.",
        "caveat": "The repo holds the POLICY (here + docs/dev/WORKTREES.md). The live ledgers "
        "(active-worktrees/file-locks/merge-queue) are maintained in the agent's icn-dev operating "
        "environment, NOT committed to this repo.",
    },
    {
        "system": "Claude Code plugin / ICN agent pack (+ preflight skill)",
        "path": "tools/claude-code/plugins/icn-agent-pack",
        "classification": "tooling",
        "purpose": "Portable agent pack; the preflight skill loads canonical docs + this overlay.",
        "caveat": "Validate with scripts/check-claude-plugin.py and check-claude-plugin-root-resolution.py.",
    },
    {
        "system": "MCP ops tooling",
        "path": "ops/mcp",
        "classification": "tooling",
        "purpose": "Read-only ops/diagnostics MCP surface (e.g. agent_context_spine tool).",
        "caveat": "MCP output is convenience, not canonical by itself.",
    },
    {
        "system": "CI / workflows + security/code-scanning lanes",
        "path": ".github/workflows",
        "classification": "operational",
        "purpose": "Required checks, doc-freshness, CodeQL/security scanning, drift checks.",
        "caveat": "CI owns the truth; required checks gate merge. Route-drift is warn-only.",
    },
    {
        "system": "Generated project-index docs",
        "path": "docs/reference/project-index",
        "classification": "generated-reference",
        "purpose": "Show-readiness map, source-of-truth map, proof-level taxonomy, generated/ artifacts.",
        "caveat": "Reference/orientation, not truth roots.",
    },
]

# How the major systems relate. Each is an interaction statement + its source/caveat.
SYSTEM_INTERACTIONS = [
    {
        "interaction": "docs/STATE.md + docs/PHASE_PROGRESS.md bound every canonical project claim; "
        "all other systems orient toward them and may not exceed them.",
        "source": "docs/STATE.md, docs/PHASE_PROGRESS.md (canonical)",
    },
    {
        "interaction": "Generated artifacts (spine, file-record, route inventory) orient agents but "
        "create NO canonical truth — they are reference layers.",
        "source": "claim_boundaries; show-readiness-map.md",
    },
    {
        "interaction": "The Agent Context Spine links crates -> subsystems -> docs -> claim surfaces and "
        "emits per-path verification briefs for the files you touch.",
        "source": "docs/reference/project-index/generated/agent-context-spine.json",
    },
    {
        "interaction": "The route inventory informs route/API/OpenAPI drift work but does NOT prove "
        "runtime or API correctness.",
        "source": "docs/scripts/route_inventory.py --check",
    },
    {
        "interaction": "The live-state overlay is regenerated at session start and must NOT be committed "
        "(a committed snapshot rots).",
        "source": "this generator; docs/ai/ICN_LIVE_STATE_OVERLAY_TEMPLATE.md",
    },
    {
        "interaction": "The preflight skill should generate/read this overlay AND the spine path brief "
        "before any work is planned.",
        "source": "tools/claude-code/plugins/icn-agent-pack/skills/preflight/SKILL.md",
    },
    {
        "interaction": "Merge queue + active-worktrees + file-locks protect concurrent work (one task = "
        "one worktree = one branch = one PR).",
        "source": "docs/dev/AGENT_WORKTREE_POLICY.md (policy); icn-dev operating env (live ledgers)",
    },
    {
        "interaction": "Doc-control + freshness checks enforce documentation discipline; front-matter "
        "dates must match docs/registry.toml.",
        "source": "docs/scripts/doc_control_check.py; docs/registry.toml",
    },
    {
        "interaction": "MCP can expose read-only context (e.g. the spine), but MCP output is not "
        "canonical by itself.",
        "source": "ops/mcp",
    },
]

# How to avoid the common mistakes. Guidance/prohibitions (like agent_start_rules), so the
# overclaim scan skips this section.
DEVELOPMENT_SAFETY_MAP = [
    "Identify which subsystem you are touching BEFORE editing (use project_map + subsystem_overview).",
    (
        "Check repo_systems first — confirm an existing system/tool/script does not already solve "
        "the problem; do not build a parallel one."
    ),
    (
        "Read the canonical docs (docs/STATE.md, docs/PHASE_PROGRESS.md) before trusting any "
        "generated map."
    ),
    "Use the generated maps (spine, file-record, route inventory) as navigation aids, never truth roots.",
    (
        "Identify the claim surfaces your change touches (see claim_boundaries) before writing "
        "public/docs/PR copy."
    ),
    (
        "Identify and run the required validation commands for the subsystem you touch "
        "(subsystem_overview `check` + the spine path brief)."
    ),
    (
        "Do not introduce a parallel system unless an issue explicitly authorizes replacing the "
        "existing one."
    ),
    (
        "Do not overclaim readiness (production / live federation / formal pilot / "
        "entity-auth-enforced / OpenAPI-complete)."
    ),
    "Do not merge anything without explicit per-PR authorization.",
]


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
        # Best-effort freshness read: a missing/unreadable doc falls through to RECONFIRM.
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
        # Best-effort phase extraction: an unreadable PHASE_PROGRESS leaves phase = RECONFIRM.
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
    # handoff-YYYY-MM-DD-*.md names sort chronologically by their ISO date prefix, so the
    # lexicographic max is the newest — and deterministic, unlike mtime (which a git
    # checkout flattens). Same-day handoffs tiebreak by filename.
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
            # Best-effort count parse: an unreadable route inventory leaves macros = RECONFIRM.
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
        "source": (
            "git log --oneline -12 HEAD (current checkout history — may be a branch, "
            "not main; reconfirm against origin/main)"
        ),
        "caveat": (
            "Historical context (the merged commits listed below). NOT live truth — "
            "reconfirm any issue/PR open/closed state via GitHub."
        ),
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
                    # Best-effort gh parse: bad/absent output leaves status = NEEDS_LIVE_RECONFIRMATION.
                    pass
        out.append({"lane": issue, "description": desc, "status": status, "source": source})
    return out


def section_next_safe_targets() -> list[dict]:
    note = ("RECOMMENDED, not AUTHORIZED. These are source-bound suggestions; an agent may "
            "NOT push/merge without explicit per-PR authorization.")
    return [
        {
            "target": "Spine/plugin validators — confirm whether they have a CI gate",
            "rationale": (
                "check-claude-plugin.py, check-claude-plugin-root-resolution.py, and "
                "check-agent-context-spine.py exist on main; a non-blocking CI gate is a "
                "candidate IF they are not already wired (do not assume — verify)."
            ),
            "source": "VERIFY: grep .github/workflows/ for the script names before proposing",
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


def _spine_subsystem_index() -> dict[str, list[str]]:
    """Map spine subsystem name -> sorted crate names, derived from the spine's
    owned_by_subsystem edges. Empty dict if the spine is absent/unreadable."""
    spine = _read_json("docs/reference/project-index/generated/agent-context-spine.json")
    if not spine:
        return {}
    idx: dict[str, list[str]] = {}
    for edge in spine.get("edges", []):
        if edge.get("type") != "owned_by_subsystem":
            continue
        frm = str(edge.get("from", ""))
        to = str(edge.get("to", ""))
        if not frm.startswith("crate:") or not to.startswith("subsystem:"):
            continue
        idx.setdefault(to.split(":", 1)[1], []).append(frm.split(":", 1)[1])
    return {k: sorted(v) for k, v in idx.items()}


def section_project_map() -> dict:
    return {
        "classification": "orientation-only",
        "model": "identity -> trust -> entity/membership -> networking/federation -> "
        "state/storage -> governance -> economics -> applications/interfaces",
        "source": "CLAUDE.md Core Subsystems + docs/ARCHITECTURE.md (orientation model, not a "
        "truth root)",
        "caveat": "Bird's-eye dependency-flow model for navigation only. NOT doctrine, NOT canonical.",
        "stages": [
            {"name": n, "purpose": p, "source": s, "classification": "orientation-only"}
            for (n, p, s) in PROJECT_MAP
        ],
    }


def section_subsystem_overview() -> list[dict]:
    spine_idx = _spine_subsystem_index()
    out: list[dict] = []
    for e in SUBSYSTEM_OVERVIEW:
        key = e.get("spine_key")
        spine_crates = spine_idx.get(key, []) if key else []
        out.append({
            "subsystem": e["subsystem"],
            "paths": e["paths"],
            "docs": e["docs"],
            "status": e["status"],
            "check": e["check"],
            "spine_backed": bool(key and key in spine_idx),
            "spine_subsystem": ("subsystem:%s" % key) if key else None,
            "spine_crates": spine_crates,
            "source": "curated v0 subsystem map"
            + (" + agent-context-spine owned_by_subsystem edges" if spine_crates else ""),
        })
    return out


def section_repo_systems() -> list[dict]:
    return [
        {
            "system": e["system"],
            "path": e["path"],
            "path_exists": _exists(e["path"]),
            "classification": e["classification"],
            "purpose": e["purpose"],
            "caveat": e["caveat"],
        }
        for e in REPO_SYSTEMS
    ]


def section_system_interactions() -> list[dict]:
    return [dict(x) for x in SYSTEM_INTERACTIONS]


def section_development_safety_map() -> list[str]:
    return list(DEVELOPMENT_SAFETY_MAP)


def build_overlay(use_gh: bool) -> dict:
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return {
        "schema": "icn-live-state-overlay/v0",
        "generated_at": now,
        "generator": "scripts/generate-live-state-overlay.py",
        "gh_consulted": bool(use_gh),
        "what_this_is": "Bounded, on-demand whole-repo orientation + session-start grounding overlay. "
        "It gives a bird's-eye view of the ICN repo (the integrated system model, the subsystems that "
        "exist and where their code/docs/checks live, the systems that already exist so you do not "
        "reinvent them, how those systems interact, and the development-safety path) plus current "
        "state. NOT canonical truth; canonical state is docs/STATE.md + docs/PHASE_PROGRESS.md. NOT a "
        "committed snapshot.",
        "repo_snapshot": section_repo_snapshot(now),
        "canonical_state": section_canonical_state(),
        "project_map": section_project_map(),
        "subsystem_overview": section_subsystem_overview(),
        "repo_systems": section_repo_systems(),
        "system_interactions": section_system_interactions(),
        "grounding_artifacts": section_grounding_artifacts(),
        "recent_completed_grounding_work": section_recent_completed(),
        "latest_handoff": _latest_handoff(),
        "active_work_lanes": section_active_lanes(use_gh),
        "development_safety_map": section_development_safety_map(),
        "claim_boundaries": {"sources": CLAIM_BOUNDARY_SOURCES, "boundaries": CLAIM_BOUNDARIES},
        "agent_start_rules": AGENT_START_RULES,
        "next_safe_targets": section_next_safe_targets(),
    }


REQUIRED_SECTIONS = [
    "repo_snapshot",
    "canonical_state",
    "project_map",
    "subsystem_overview",
    "repo_systems",
    "system_interactions",
    "grounding_artifacts",
    "recent_completed_grounding_work",
    "active_work_lanes",
    "development_safety_map",
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
    pm = o["project_map"]
    L.append("## 3. project_map (bird's-eye — orientation-only)")
    L.append(f"- model: `{pm['model']}`")
    L.append(f"- source: {pm['source']}  ·  caveat: {pm['caveat']}")
    for st in pm["stages"]:
        L.append(f"  - **{st['name']}** [{st['classification']}] — {st['purpose']} "
                 f"(source: {st['source']})")
    L.append("")
    L.append("## 4. subsystem_overview")
    for e in o["subsystem_overview"]:
        L.append(f"- **{e['subsystem']}** — {e['status']}")
        L.append(f"  - paths: `{e['paths']}`  ·  docs: {e['docs']}")
        if e["spine_backed"]:
            L.append(f"  - spine: `{e['spine_subsystem']}` (crates: "
                     f"{', '.join(e['spine_crates']) or '—'})")
        else:
            L.append("  - spine: curated v0 (no spine subsystem node yet)")
        L.append(f"  - check: `{e['check']}`  ·  source: {e['source']}")
    L.append("")
    L.append("## 5. repo_systems (do not reinvent these)")
    for e in o["repo_systems"]:
        L.append(f"- **{e['system']}** [{e['classification']}] (`{e['path']}`, "
                 f"exists={e['path_exists']})")
        L.append(f"  - purpose: {e['purpose']}")
        L.append(f"  - caveat: {e['caveat']}")
    L.append("")
    L.append("## 6. system_interactions")
    for e in o["system_interactions"]:
        L.append(f"- {e['interaction']}")
        L.append(f"  - source: {e['source']}")
    L.append("")
    L.append("## 7. grounding_artifacts")
    for e in o["grounding_artifacts"]:
        L.append(f"- **{e['artifact']}** (`{e['path']}`) — {e['classification']}")
        L.append(f"  - generated: `{e['generated']}` · source_commit: `{e['source_commit']}` · "
                 f"{e['metrics']}")
        L.append(f"  - caveat: {e['caveat']}")
    L.append("")
    rc = o["recent_completed_grounding_work"]
    L.append("## 8. recent_completed_grounding_work")
    L.append(f"- source: `{rc['source']}`")
    L.append(f"- caveat: {rc['caveat']}")
    for m in rc["recent_merges"]:
        L.append(f"  - `{m}`")
    L.append(f"- latest_handoff: `{o['latest_handoff']}`")
    L.append("")
    L.append("## 9. active_work_lanes")
    for e in o["active_work_lanes"]:
        L.append(f"- {e['lane']} — {e['description']}")
        L.append(f"  - status: `{e['status']}`  ·  source: {e['source']}")
    L.append("")
    L.append("## 10. development_safety_map")
    for i, r in enumerate(o["development_safety_map"], 1):
        L.append(f"{i}. {r}")
    L.append("")
    cb = o["claim_boundaries"]
    L.append("## 11. claim_boundaries")
    L.append(f"- sources: {', '.join('`%s`' % s for s in cb['sources'])}")
    for b in cb["boundaries"]:
        L.append(f"- {b}")
    L.append("")
    L.append("## 12. agent_start_rules")
    for i, r in enumerate(o["agent_start_rules"], 1):
        L.append(f"{i}. {r}")
    L.append("")
    L.append("## 13. next_safe_targets")
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

    # Whole-repo orientation sections must be populated and well-formed.
    pm = o.get("project_map", {})
    if not pm.get("model") or not pm.get("stages"):
        problems.append("project_map missing model/stages")
    for st in pm.get("stages", []):
        if not st.get("name") or not st.get("classification"):
            problems.append(f"project_map stage lacks name/classification: {st.get('name')}")

    subs = o.get("subsystem_overview", [])
    if len(subs) < 8:
        problems.append("subsystem_overview should cover the major subsystems (>=8)")
    for e in subs:
        if not (e.get("paths") or e.get("spine_crates")) or not e.get("check") or not e.get("status"):
            problems.append(f"subsystem_overview entry lacks paths/check/status: {e.get('subsystem')}")

    valid_classes = {"canonical", "generated-reference", "advisory", "operational", "tooling"}
    for e in o.get("repo_systems", []):
        if not e.get("path") or not e.get("caveat"):
            problems.append(f"repo_systems entry lacks path/caveat: {e.get('system')}")
        if e.get("classification") not in valid_classes:
            problems.append(f"repo_systems entry has invalid classification: {e.get('system')}")

    for e in o.get("system_interactions", []):
        if not e.get("interaction") or not e.get("source"):
            problems.append("system_interactions entry lacks interaction/source")
    if not o.get("development_safety_map"):
        problems.append("development_safety_map is empty")

    # NYCN demotion: present as ONE claim-boundary item, never first, never a top-level
    # section, never dominant across the overlay.
    boundaries = o.get("claim_boundaries", {}).get("boundaries", [])
    if not any("nycn" in b.lower() for b in boundaries):
        problems.append("NYCN safety boundary missing from claim_boundaries")
    if boundaries and "nycn" in boundaries[0].lower():
        problems.append("NYCN must not be the FIRST claim boundary (demote it)")
    if any("nycn" in str(k).lower() for k in o.keys()):
        problems.append("NYCN must not be a top-level overlay section")
    nycn_mentions = json.dumps(o).lower().count("nycn")
    if nycn_mentions > 5:
        problems.append(f"NYCN over-represented ({nycn_mentions} mentions) — keep it a single "
                        "boundary item, not the center of the overlay")

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
