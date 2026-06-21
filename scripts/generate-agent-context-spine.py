#!/usr/bin/env python3
"""Generate (or --check) the ICN Agent Context Spine v0.

The spine is a small, evidence-grounded orientation artifact that bridges the
repo's existing truth systems (ops/state/truth/*.json and the generated
project-index artifacts). It lets an ICN agent ask, from repo-owned data
rather than guesswork:

    Where am I? What subsystem owns this? What docs describe it? What tests or
    scripts verify it? What routes/public surfaces could this affect? What
    invariants apply? What truth source is canonical? What skill/agent helps?

It is NOT a canonical source of truth, and it does NOT assert production / live
/ pilot readiness. Every node and edge carries an `evidence` pointer to a path
that exists on disk. v0 deliberately does not parse the Rust module graph and
does not create per-route nodes.

Usage:
    python3 scripts/generate-agent-context-spine.py --write    # regenerate
    python3 scripts/generate-agent-context-spine.py --check     # exit 1 if stale

Conventions mirror docs/scripts/route_inventory.py: deterministic output, a
generated timestamp + source commit, and a --check mode that normalizes
volatile fields before comparison. Python standard library only.
"""

from __future__ import annotations

import argparse
import json
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
        return Path(__file__).resolve().parents[1]


ROOT = repo_root()
ARTIFACT_REL = "docs/reference/project-index/generated/agent-context-spine.json"
ARTIFACT = ROOT / ARTIFACT_REL
SCRIPT_REL = "scripts/generate-agent-context-spine.py"
CHECK_REL = "scripts/check-agent-context-spine.py"
CARGO_REL = "icn/Cargo.toml"
SOURCES_REL = "ops/state/truth/sources.json"
PLUGIN_REL = "tools/claude-code/plugins/icn-agent-pack"
CLAUDE_REL = "CLAUDE.md"
AGENTS_REL = "AGENTS.md"

SCHEMA = "icn-agent-context-spine/v0"

# --- curated seed metadata (honest: hand-seeded, evidence-linked, not scanned) ---

# Core subsystems from CLAUDE.md "Core Subsystems". boundary is a coarse
# kernel/policy-oracle hint, not a per-crate dependency claim.
SUBSYSTEMS = [
    ("identity", "Decentralized identifiers (DIDs) with Ed25519 cryptography.", "foundational"),
    ("trust", "Web-of-participation trust computation feeding a Policy Oracle.", "policy-oracle"),
    ("networking", "QUIC/TLS secure sessions with mDNS discovery.", "kernel"),
    ("ledger", "Mutual credit with double-entry accounting feeding a Policy Oracle.", "policy-oracle"),
    ("contracts", "CCL (Cooperative Contract Language) execution feeding a Policy Oracle.", "policy-oracle"),
    ("gossip", "Topic-based replication with causal ordering.", "kernel"),
    ("governance", "Democratic proposals and voting feeding a Policy Oracle.", "policy-oracle"),
    ("compute", "Trust-gated distributed task execution feeding a Policy Oracle.", "policy-oracle"),
]

# crate basename -> subsystem id. Curated seed; evidence is CLAUDE.md.
CRATE_SUBSYSTEM = {
    "icn-identity": "identity",
    "icn-naming": "identity",
    "icn-trust": "trust",
    "icn-net": "networking",
    "icn-protocol": "networking",
    "icn-gossip": "gossip",
    "icn-ledger": "ledger",
    "icn-ccl": "contracts",
    "icn-governance": "governance",
    "icn-community": "governance",
    "icn-coop": "governance",
    "icn-entity": "governance",
    "icn-compute": "compute",
}

# Canonical/context docs. (route-inventory.md and icn-file-record.json are
# modeled as generated_artifact nodes instead.)
DOCS = [
    ("doc:state", "docs/STATE.md", "Current project state (canonical, truth-synced).", "canonical-state"),
    ("doc:phase-progress", "docs/PHASE_PROGRESS.md", "Phase progress tracking (canonical, truth-synced).", "canonical-state"),
    ("doc:claude", "CLAUDE.md", "Repo guidance for Claude Code: topology, subsystems, conventions.", "guidance"),
    ("doc:agents", "AGENTS.md", "Agent operating rules and the five ICN invariants.", "guidance"),
    ("doc:source-of-truth-map", "docs/reference/project-index/source-of-truth-map.md", "Precedence map: which sources outrank which when material disagrees.", "orientation"),
    ("doc:show-readiness-map", "docs/reference/project-index/show-readiness-map.md", "What can be shown now vs. what is not finished; red lines.", "orientation"),
    ("doc:proof-level-taxonomy", "docs/reference/project-index/proof-level-taxonomy-capability-matrix.md", "Proof-level taxonomy (L0-L8) and capability matrix.", "orientation"),
    ("doc:agent-mcp-tooling", "docs/guides/developer/agent-mcp-tooling.md", "The icn-ops MCP server: read-mostly diagnostics and launch doctrine.", "guide"),
    ("doc:agent-context-spine", "docs/guides/developer/agent-context-spine.md", "Agent Context Spine guide: model, regenerate/check, MCP exposure, path briefs.", "guide"),
    ("doc:claude-code-plugin", "docs/guides/developer/claude-code-plugin.md", "The icn-agent-pack Claude Code plugin (portable MCP launch, skills/agents).", "guide"),
]

# generated_artifact nodes: (id, path, description, generator script path)
GENERATED_ARTIFACTS = [
    ("generated_artifact:agent-context-spine", ARTIFACT_REL,
     "This artifact: the agent context spine (generated, non-canonical).", SCRIPT_REL),
    ("generated_artifact:route-inventory", "docs/reference/project-index/generated/route-inventory.md",
     "Mechanical gateway route inventory (route declarations at a snapshot commit).", "docs/scripts/route_inventory.py"),
    ("generated_artifact:icn-file-record", "docs/reference/project-index/generated/icn-file-record.json",
     "Mechanical file/directory record snapshot of the repo.", "scripts/generate_repo_record.py"),
]

SCRIPTS = [
    "scripts/generate-agent-context-spine.py",
    "scripts/check-agent-context-spine.py",
    "scripts/check-claude-plugin.py",
    "scripts/check-claude-plugin-root-resolution.py",
    "scripts/check-mcp-portability.py",
    "docs/scripts/route_inventory.py",
    "scripts/generate_repo_record.py",
]

# MCP diagnostic tools: (short name, diagnostics source file basename)
MCP_TOOLS = [
    ("environment_report", "environment-report.ts"),
    ("doctor", "doctor.ts"),
    ("agent_brief", "agent-brief.ts"),
    ("command_catalog", "command-catalog.ts"),
    ("state_index", "state-index.ts"),
    ("next_steps", "next-steps.ts"),
    ("verification_plan", "verification-plan.ts"),
    ("repo_map", "repo-map.ts"),
]
MCP_TOOLS_REG_REL = "ops/mcp/src/tools/agent-ops.ts"
MCP_DIAG_DIR_REL = "ops/mcp/src/diagnostics"

# The five ICN invariants from AGENTS.md "ICN invariants (non-negotiable)".
INVARIANTS = [
    ("adversarial-by-default", "Treat peers as untrusted until trust is established; no implicit trust shortcuts."),
    ("determinism", "Protocol state transitions, proofs, and derived roots must be deterministic; same inputs produce same outputs."),
    ("canonical-encodings", "Do not change wire/proof/encoding structures without explicit intent, docs, and tests."),
    ("no-panics-in-protocol-paths", "Never panic in network/protocol/actor-runtime/deserialization paths; use Result."),
    ("kernel-app-boundaries", "Keep crate layering clean; avoid dependency cycles; follow the forbidden-deps policy."),
]

# High-risk claim surfaces. Descriptions are prohibitions, never assertions of
# readiness, and deliberately avoid overclaim phrasing.
CLAIM_SURFACES = [
    ("production-readiness", "Production-readiness claim surface.",
     "A change here could be misread as asserting the system is fit for production. The spine asserts no such status; declared structure is not runtime evidence. Confirm runtime via current ops evidence and route claims to the truth-sync / docs-truth-auditor workflows.",
     ["CLAUDE.md", "docs/reference/project-index/show-readiness-map.md"]),
    ("live-federation", "Federation-liveness claim surface.",
     "A change here could be misread as asserting federation is operating across nodes. The spine maps declared structure only, not runtime liveness. Route liveness claims to the truth-sync / docs-truth-auditor workflows.",
     ["CLAUDE.md", "docs/reference/project-index/show-readiness-map.md"]),
    ("pilot-readiness", "Pilot-status claim surface.",
     "The current phase is partner-bound and not a committed pilot. Do not assert pilot status from code or docs. Route such claims to the truth-sync / docs-truth-auditor workflows.",
     ["docs/PHASE_PROGRESS.md", "CLAUDE.md"]),
    ("public-website-claims", "Public-website claim surface.",
     "Website content can outrun the daemon. Website presence is not proof of implementation. Route public claims to the docs-truth-auditor / truth-sync workflows.",
     ["docs/reference/project-index/show-readiness-map.md", "docs/reference/project-index/source-of-truth-map.md"]),
    ("route-api-docs-drift", "Route/API/docs drift surface.",
     "Gateway routes, the OpenAPI spec, and docs can drift apart. Verify with route_inventory.py --check before claiming API or documentation coverage.",
     ["docs/reference/project-index/generated/route-inventory.md", "docs/scripts/route_inventory.py"]),
]

# Path-guidance rules: the "code-quality brief" layer. Each rule maps a path
# prefix to the things an agent should care about when editing/reviewing files
# under it. Curated seed (evidence is the prefix dir + AGENTS.md verification
# matrix). All id references must resolve to real nodes — the validator enforces
# this. A changed path matches EVERY rule whose prefix it falls under, and the
# briefs are merged (most specific rules add to the general ones).
#
# Tuple: (slug, prefix, label, review_focus[], verification_commands[],
#         risk_surfaces[claim_surface id], invariants[invariant id], docs[doc id],
#         skills[skill id], agents[agent id])
PATH_GUIDANCE = [
    ("rust-workspace", "icn/", "ICN Rust workspace crate",
     ["Kernel/app boundary: no domain imports in kernel crates; no reverse meaning firewall",
      "No panics in protocol/actor/deserialization paths (no unwrap/expect on untrusted input)",
      "Determinism: no HashMap iteration-order or unseeded-random reliance",
      "Canonical encodings unchanged without explicit versioning + tests"],
     ["cd icn && cargo fmt --all --check",
      "cd icn && cargo clippy --workspace --all-targets --all-features -- -D warnings",
      "cd icn && cargo test"],
     [], ["invariant:adversarial-by-default", "invariant:determinism",
          "invariant:canonical-encodings", "invariant:no-panics-in-protocol-paths",
          "invariant:kernel-app-boundaries"],
     ["doc:agents", "doc:claude"],
     ["skill:navigator"], ["agent:icn-architect", "agent:icn-code-reviewer"],
     "AGENTS.md"),
    ("gateway", "icn/crates/icn-gateway/", "icn-gateway (gateway / route / API surface)",
     ["Route/API drift: gateway handlers vs OpenAPI vs SDK types",
      "Public-surface and auth/authz on new or changed routes",
      "Regenerate the route inventory + OpenAPI when routes change",
      "No panics on untrusted request input"],
     ["python3 docs/scripts/route_inventory.py --check",
      "cd icn && cargo clippy -p icn-gateway --all-targets -- -D warnings",
      "cd icn && cargo test -p icn-gateway"],
     ["claim_surface:route-api-docs-drift", "claim_surface:public-website-claims"],
     ["invariant:no-panics-in-protocol-paths", "invariant:adversarial-by-default"],
     ["doc:source-of-truth-map", "doc:show-readiness-map"],
     ["skill:route-impact", "skill:truth-sync"],
     ["agent:icn-code-reviewer", "agent:icn-docs-truth-auditor"],
     "AGENTS.md"),
    ("ledger", "icn/crates/icn-ledger/", "icn-ledger (mutual credit / settlement)",
     ["Mutual-credit / double-entry invariants (no value created or destroyed)",
      "Use settlement (not payment) terminology — regulatory framing",
      "Determinism of journal/state-change derivation"],
     ["cd icn && cargo test -p icn-ledger",
      "cd icn && cargo clippy -p icn-ledger --all-targets -- -D warnings"],
     [], ["invariant:determinism", "invariant:canonical-encodings"],
     ["doc:claude"],
     ["skill:navigator"], ["agent:icn-economist", "agent:icn-code-reviewer"],
     "AGENTS.md"),
    ("mcp", "ops/mcp/", "icn-ops MCP server (TypeScript)",
     ["Read-only / no mutation unless explicitly designed",
      "Clear failure modes on missing or malformed inputs",
      "ICN root & path portability (resolveMonorepoRoot; no hardcoded paths)",
      "Stable MCP tool/resource contract"],
     ["npm --prefix ./ops/mcp run build",
      "npm --prefix ./ops/mcp test",
      "python3 scripts/check-mcp-portability.py"],
     [], [],
     ["doc:agent-mcp-tooling", "doc:agent-context-spine"],
     ["skill:doctor", "skill:navigator"],
     ["agent:icn-ops", "agent:icn-code-reviewer"],
     "docs/guides/developer/agent-mcp-tooling.md"),
    ("generated", "docs/reference/project-index/generated/", "Generated project-index artifact",
     ["Do NOT hand-edit generated artifacts — regenerate via the owning script",
      "Every node/edge evidence path must resolve on disk",
      "No production/live/pilot overclaims in generated fields"],
     ["python3 scripts/generate-agent-context-spine.py --check",
      "python3 scripts/check-agent-context-spine.py",
      "python3 docs/scripts/route_inventory.py --check"],
     ["claim_surface:public-website-claims"], [],
     ["doc:source-of-truth-map"],
     ["skill:truth-sync", "skill:navigator"],
     ["agent:icn-docs-truth-auditor"],
     "docs/reference/project-index/generated"),
    ("docs", "docs/", "Documentation (truth / claim discipline)",
     ["Truth-source alignment and source precedence",
      "No production/live/pilot overclaims; bound claims by proof level",
      "Freshness and evidence for any asserted state"],
     ["python3 docs/scripts/doc_control_check.py"],
     ["claim_surface:public-website-claims", "claim_surface:production-readiness",
      "claim_surface:live-federation", "claim_surface:pilot-readiness"],
     [], ["doc:source-of-truth-map", "doc:show-readiness-map"],
     ["skill:truth-sync", "skill:navigator"],
     ["agent:icn-docs-truth-auditor"],
     "docs/reference/project-index/source-of-truth-map.md"),
    ("scripts", "scripts/", "Repo automation script (generator / validator)",
     ["Python standard library only; dependency-free",
      "Deterministic output; --check drift mode stays correct",
      "No overclaim language emitted into generated artifacts"],
     ["python3 scripts/check-agent-context-spine.py",
      "python3 scripts/generate-agent-context-spine.py --check"],
     [], [], ["doc:agent-context-spine"],
     ["skill:navigator"], ["agent:icn-code-reviewer"],
     "scripts"),
    ("docs-scripts", "docs/scripts/", "Docs automation script (generator / validator)",
     ["Python standard library only; dependency-free",
      "Deterministic output; --check drift mode stays correct"],
     ["python3 docs/scripts/route_inventory.py --check",
      "python3 docs/scripts/doc_control_check.py"],
     [], [], ["doc:source-of-truth-map"],
     ["skill:navigator"], ["agent:icn-code-reviewer", "agent:icn-docs-truth-auditor"],
     "docs/scripts"),
    ("plugin", "tools/claude-code/plugins/icn-agent-pack/", "icn-agent-pack Claude Code plugin",
     ["Plugin manifest / skill / agent schema validity and loadability",
      "Advisory hooks stay non-blocking; root resolution stays portable",
      "Do not change project-local .claude/ or root MCP config"],
     ["python3 scripts/check-claude-plugin.py",
      "python3 scripts/check-claude-plugin-root-resolution.py",
      "claude plugin validate ./tools/claude-code/plugins/icn-agent-pack"],
     [], [], ["doc:claude-code-plugin"],
     ["skill:doctor"], ["agent:icn-code-reviewer"],
     "tools/claude-code/plugins/icn-agent-pack"),
    ("truth-spine", "ops/state/truth/", "Canonical truth spine (fact ownership)",
     ["This directory OWNS fact authority — keep one source per fact",
      "Never hardcode sprint/PR/branch/cluster IPs in skills or agents",
      "Downstream skills/agents read from here; do not duplicate facts"],
     [],
     [], [], ["doc:source-of-truth-map"],
     ["skill:navigator"], ["agent:icn-architect", "agent:icn-code-reviewer"],
     "ops/state/truth"),
    ("sdk-ts", "sdk/typescript/", "TypeScript SDK (generated types drift)",
     ["Commit only regenerated types under src/generated/*",
      "API drift chain: gateway -> OpenAPI -> TS types",
      "No mixed refactor + regen commits"],
     ["cd sdk/typescript && npm ci && npm run generate-types && npm run check-types"],
     ["claim_surface:route-api-docs-drift"], [],
     ["doc:source-of-truth-map"],
     ["skill:route-impact"], ["agent:icn-code-reviewer"],
     "sdk/typescript"),
]


# --- helpers ---

def rel_exists(rel: str) -> bool:
    return (ROOT / rel).exists()


def frontmatter_field(md_path: Path, field: str) -> str | None:
    """Extract a top-level YAML frontmatter scalar (e.g. name:, description:)."""
    try:
        text = md_path.read_text(encoding="utf-8")
    except Exception:
        return None
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return None
    for line in lines[1:]:
        if line.strip() == "---":
            break
        if line.startswith(f"{field}:"):
            val = line[len(field) + 1:].strip()
            if len(val) >= 2 and val[0] in "\"'" and val[-1] == val[0]:
                val = val[1:-1]
            return val
    return None


def evidence(*rels: str, kind: str = "path") -> list[dict]:
    """Build an evidence list from repo-relative paths that exist on disk."""
    items = []
    for rel in rels:
        if rel and rel_exists(rel):
            items.append({"source": rel, "kind": kind})
    return items


def workspace_members() -> list[str]:
    """Parse [workspace].members from icn/Cargo.toml (regex-free, stdlib)."""
    text = (ROOT / CARGO_REL).read_text(encoding="utf-8")
    members: list[str] = []
    in_block = False
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("members"):
            in_block = True
            line = line.split("=", 1)[1] if "=" in line else ""
        if not in_block:
            continue
        # strip inline comments
        if "#" in line:
            line = line[: line.index("#")]
        for tok in line.replace("[", " ").replace("]", " ").split(","):
            tok = tok.strip().strip("\"'").strip()
            if tok and tok not in ("[", "]"):
                members.append(tok)
        if "]" in raw:
            break
    # de-dup preserving order
    seen, out = set(), []
    for m in members:
        if m and m not in seen:
            seen.add(m)
            out.append(m)
    return out


def crate_depends_on(member: str, dep_name: str) -> bool:
    """Cheap manifest-only check: does icn/<member>/Cargo.toml mention dep_name?

    This reads a single crate manifest, not the module graph. Used only for the
    coarse 'depends on the kernel API surface' layer.
    """
    manifest = ROOT / "icn" / member / "Cargo.toml"
    try:
        text = manifest.read_text(encoding="utf-8")
    except Exception:
        return False
    # match a dependency key line like: icn-kernel-api = ... or "icn-kernel-api"
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("#"):
            continue
        key = line.split("=", 1)[0].strip().strip("\"'")
        if key == dep_name:
            return True
    return False


# --- spine construction ---

def build_spine(commit: str, now: str) -> dict:
    nodes: list[dict] = []
    edges: list[dict] = []

    def add_node(**n):
        nodes.append(n)

    def add_edge(frm, to, etype, src):
        edges.append({"from": frm, "to": to, "type": etype, "evidence": {"source": src}})

    # subsystems
    subsystem_ids = set()
    for sid, desc, boundary in SUBSYSTEMS:
        nid = f"subsystem:{sid}"
        subsystem_ids.add(nid)
        add_node(
            id=nid, type="subsystem", name=sid, description=desc,
            boundary=boundary, source_of_truth=CLAUDE_REL, status="curated-seed",
            evidence=evidence(CLAUDE_REL, kind="core-subsystems-section"),
        )

    # crates
    crate_ids = set()
    for member in workspace_members():
        base = member.rsplit("/", 1)[-1]
        group = member.split("/", 1)[0]  # crates | apps | bins
        nid = f"crate:{base}"
        if nid in crate_ids:
            continue
        crate_ids.add(nid)
        member_rel = f"icn/{member}"
        ev = evidence(CARGO_REL, kind="workspace-member")
        if rel_exists(member_rel):
            ev.append({"source": member_rel, "kind": "crate-dir"})
        add_node(
            id=nid, type="crate", name=base, group=group,
            path=member_rel if rel_exists(member_rel) else None,
            description=f"Workspace member crate ({group}). v0 spine records membership and path only; it does not parse module internals.",
            source_of_truth=CARGO_REL, status="present", evidence=ev,
        )
        # owned_by_subsystem (curated)
        sub = CRATE_SUBSYSTEM.get(base)
        if sub:
            add_edge(nid, f"subsystem:{sub}", "owned_by_subsystem", CLAUDE_REL)
        # coarse kernel-API dependency layer (manifest-only)
        if base != "icn-kernel-api" and crate_depends_on(member, "icn-kernel-api"):
            add_edge(nid, "crate:icn-kernel-api", "depends_on", f"{member_rel}/Cargo.toml")

    # docs
    for nid, path, desc, role in DOCS:
        if not rel_exists(path):
            continue
        add_node(
            id=nid, type="doc", name=path.rsplit("/", 1)[-1], path=path,
            description=desc, doc_role=role, source_of_truth=path, status="present",
            evidence=evidence(path, kind="doc"),
        )

    # generated artifacts
    for nid, path, desc, gen in GENERATED_ARTIFACTS:
        # These are committed generated artifacts; reference the artifact path
        # unconditionally so output is identical whether or not the file is
        # present at build time (the spine artifact does not exist during its
        # own first --write, but always exists once committed).
        ev = [{"source": path, "kind": "generated-artifact"}]
        add_node(
            id=nid, type="generated_artifact", name=path.rsplit("/", 1)[-1], path=path,
            description=desc, generated_by=gen, source_of_truth=gen, status="generated",
            evidence=ev,
        )
        if rel_exists(gen):
            add_edge(f"script:{Path(gen).stem}", nid, "generates", gen)

    # route surface (single pointer; no per-route nodes in v0)
    route_md = "docs/reference/project-index/generated/route-inventory.md"
    add_node(
        id="route_surface:gateway", type="route_surface", name="gateway-route-surface",
        path=route_md,
        description="Single pointer to the mechanically generated gateway route inventory. v0 does not create per-route nodes; counts and snapshot commit live in the artifact.",
        source_of_truth="docs/scripts/route_inventory.py", status="generated-pointer",
        evidence=evidence(route_md, "docs/scripts/route_inventory.py", kind="route-source"),
    )
    if "crate:icn-gateway" in crate_ids:
        add_edge("crate:icn-gateway", "route_surface:gateway", "exposes", route_md)

    # skills (glob plugin skills)
    skills_dir = ROOT / PLUGIN_REL / "skills"
    skill_ids = set()
    for skill_md in sorted(skills_dir.glob("*/SKILL.md")):
        slug = skill_md.parent.name
        nid = f"skill:{slug}"
        skill_ids.add(nid)
        rel = str(skill_md.relative_to(ROOT))
        name = frontmatter_field(skill_md, "name") or slug
        desc = frontmatter_field(skill_md, "description") or f"Plugin skill: {slug}."
        add_node(
            id=nid, type="skill", name=name, path=rel, description=desc,
            source_of_truth=rel, status="present", evidence=evidence(rel, kind="skill"),
        )

    # agents (glob plugin agents)
    agents_dir = ROOT / PLUGIN_REL / "agents"
    for agent_md in sorted(agents_dir.glob("*.md")):
        slug = agent_md.stem
        nid = f"agent:{slug}"
        rel = str(agent_md.relative_to(ROOT))
        name = frontmatter_field(agent_md, "name") or slug
        desc = frontmatter_field(agent_md, "description") or f"Plugin agent: {slug}."
        add_node(
            id=nid, type="agent", name=name, path=rel, description=desc,
            source_of_truth=rel, status="present", evidence=evidence(rel, kind="agent"),
        )

    # mcp tools
    for short, diag_file in MCP_TOOLS:
        nid = f"mcp_tool:icn_ops_{short}"
        diag_rel = f"{MCP_DIAG_DIR_REL}/{diag_file}"
        add_node(
            id=nid, type="mcp_tool", name=f"icn_ops_{short}", short=short,
            description=f"Read-only icn-ops MCP diagnostic ({short}). Returns context as strings; never executes repo commands.",
            source_of_truth=MCP_TOOLS_REG_REL, status="present",
            evidence=evidence(MCP_TOOLS_REG_REL, diag_rel, kind="mcp-tool"),
        )

    # scripts
    for path in SCRIPTS:
        if not rel_exists(path):
            continue
        nid = f"script:{Path(path).stem}"
        add_node(
            id=nid, type="script", name=path.rsplit("/", 1)[-1], path=path,
            description=f"Repo script: {path}.", source_of_truth=path,
            status="present", evidence=evidence(path, kind="script"),
        )

    # invariants
    for slug, desc in INVARIANTS:
        nid = f"invariant:{slug}"
        add_node(
            id=nid, type="invariant", name=slug, description=desc,
            source_of_truth=AGENTS_REL, status="present",
            evidence=evidence(AGENTS_REL, kind="icn-invariants-section"),
        )
        add_edge("doc:agents", nid, "documents", AGENTS_REL)
        add_edge(nid, "truth_source:invariants", "canonical_source", SOURCES_REL)

    # claim surfaces
    for slug, name, desc, ev_paths in CLAIM_SURFACES:
        nid = f"claim_surface:{slug}"
        add_node(
            id=nid, type="claim_surface", name=name, description=desc,
            source_of_truth="docs/reference/project-index/show-readiness-map.md",
            status="advisory", asserts_readiness=False,
            evidence=evidence(*ev_paths, kind="claim-doc"),
        )

    # truth sources (from ops/state/truth/sources.json)
    truth_ids = set()
    sources = json.loads((ROOT / SOURCES_REL).read_text(encoding="utf-8"))
    for domain, meta in sorted(sources.get("domains", {}).items()):
        nid = f"truth_source:{domain}"
        truth_ids.add(nid)
        owner = meta.get("owner", "")
        ev = [{"source": SOURCES_REL, "kind": "truth-domain"}]
        # add the owner file as evidence when it is a real path
        owner_path = owner.split("#", 1)[0]
        if owner_path and rel_exists(owner_path):
            ev.append({"source": owner_path, "kind": "truth-owner"})
        add_node(
            id=nid, type="truth_source", name=domain, owner=owner,
            description=meta.get("description", ""), stability=meta.get("stability", ""),
            source_of_truth=SOURCES_REL, status="present", evidence=ev,
        )

    # documents: CLAUDE.md documents each subsystem
    for sid, _desc, _b in SUBSYSTEMS:
        add_edge("doc:claude", f"subsystem:{sid}", "documents", CLAUDE_REL)

    # verifies
    if rel_exists(CHECK_REL):
        add_edge(f"script:{Path(CHECK_REL).stem}", "generated_artifact:agent-context-spine", "verifies", CHECK_REL)
    if rel_exists("docs/scripts/route_inventory.py"):
        add_edge("script:route_inventory", "claim_surface:route-api-docs-drift", "verifies", "docs/scripts/route_inventory.py")

    # requires_skill: claim surfaces -> mitigating skills
    skill_for = {
        "claim_surface:production-readiness": "skill:truth-sync",
        "claim_surface:live-federation": "skill:truth-sync",
        "claim_surface:pilot-readiness": "skill:truth-sync",
        "claim_surface:public-website-claims": "skill:truth-sync",
        "claim_surface:route-api-docs-drift": "skill:route-impact",
    }
    for cs, sk in skill_for.items():
        if sk in skill_ids:
            target_md = ROOT / PLUGIN_REL / "skills" / sk.split(":", 1)[1] / "SKILL.md"
            add_edge(cs, sk, "requires_skill", str(target_md.relative_to(ROOT)))

    # touches_claim_surface (well-evidenced only)
    if "crate:icn-gateway" in crate_ids:
        add_edge("crate:icn-gateway", "claim_surface:route-api-docs-drift", "touches_claim_surface", route_md)
    if "crate:icn-federation" in crate_ids:
        add_edge("crate:icn-federation", "claim_surface:live-federation", "touches_claim_surface", CLAUDE_REL)

    # path_guidance: the code-quality brief layer (curated seed, id-validated).
    for (slug, prefix, label, review_focus, verify, risks, invs, docs_ids,
         skills, agents, src) in PATH_GUIDANCE:
        if not rel_exists(prefix):
            continue
        nid = f"guidance:{slug}"
        ev = evidence(prefix, kind="path-prefix")
        if rel_exists(src) and src != prefix:
            ev.append({"source": src, "kind": "authority"})
        add_node(
            id=nid, type="path_guidance", name=label, match=prefix,
            match_kind="prefix",
            description=f"Code-quality brief for changes under `{prefix}`.",
            review_focus=review_focus, verification_commands=verify,
            risk_surfaces=risks, invariants=invs, docs=docs_ids,
            recommended_skills=skills, recommended_agents=agents,
            source_of_truth=src, status="curated-seed", evidence=ev,
        )
        # graph edges so the guidance is navigable, not just a payload
        for sk in skills:
            if sk in skill_ids:
                add_edge(nid, sk, "requires_skill", prefix)
        for cs in risks:
            add_edge(nid, cs, "touches_claim_surface", prefix)

    # deterministic ordering
    nodes.sort(key=lambda n: (n["type"], n["id"]))
    edges.sort(key=lambda e: (e["type"], e["from"], e["to"]))

    # drop None-valued optional fields for cleanliness
    for n in nodes:
        for k in [k for k, v in list(n.items()) if v is None]:
            del n[k]

    return {
        "schema": SCHEMA,
        "status": "generated",
        "canonical": False,
        "generated": now,
        "source_commit": commit,
        "generator": SCRIPT_REL,
        "regenerate": f"python3 {SCRIPT_REL} --write",
        "check": f"python3 {SCRIPT_REL} --check",
        "description": (
            "ICN Agent Context Spine v0: a generated, non-canonical, evidence-grounded "
            "orientation map that bridges ops/state/truth/*.json and the generated "
            "project-index artifacts. Structure is not runtime liveness; this artifact "
            "asserts no production/live/pilot readiness. Defer to the canonical precedence "
            "in docs/reference/project-index/source-of-truth-map.md."
        ),
        "node_types": sorted({n["type"] for n in nodes}),
        "edge_types": sorted({e["type"] for e in edges}),
        "counts": {"nodes": len(nodes), "edges": len(edges)},
        "nodes": nodes,
        "edges": edges,
    }


def serialize(spine: dict) -> str:
    return json.dumps(spine, indent=2, ensure_ascii=False) + "\n"


def normalize(text: str) -> str:
    """Zero out volatile fields so --check ignores timestamp/commit churn."""
    try:
        data = json.loads(text)
    except Exception:
        return text
    data["generated"] = ""
    data["source_commit"] = ""
    return json.dumps(data, indent=2, ensure_ascii=False, sort_keys=True)


def _dedup(seq):
    """Order-preserving de-duplication."""
    seen, out = set(), []
    for x in seq:
        if x not in seen:
            seen.add(x)
            out.append(x)
    return out


def compute_brief(spine: dict, paths: list[str]) -> dict:
    """Derive a code-quality brief for the given changed paths from the spine.

    For each path: match every path_guidance rule whose prefix covers it
    (general before specific), resolve the owning crate -> subsystem(s) and
    crate -> claim surface(s) from the graph, and merge the guidance. Returns a
    compact per-path list plus a deduplicated `combined` checklist. Pure
    function over the spine dict — no filesystem or command execution.
    """
    nodes = spine.get("nodes", [])
    edges = spine.get("edges", [])
    by_id = {n["id"]: n for n in nodes}
    guidance = [n for n in nodes if n.get("type") == "path_guidance"]
    crates = [n for n in nodes if n.get("type") == "crate" and n.get("path")]

    def norm(p: str) -> str:
        return p.strip().lstrip("./") if p not in ("", ".") else p

    per_path = []
    for raw in paths:
        path = norm(raw)
        matched = [g for g in guidance
                   if path == g["match"].rstrip("/") or path.startswith(g["match"])]
        matched.sort(key=lambda g: (len(g["match"]), g["match"]))  # general -> specific

        # owning crate (longest matching crate path)
        cands = [c for c in crates
                 if path == c["path"] or path.startswith(c["path"] + "/")]
        crate = max(cands, key=lambda c: len(c["path"])) if cands else None

        subsystems, crate_claims, matched_nodes = [], [], []
        if crate:
            matched_nodes.append(crate["id"])
            for e in edges:
                if e["from"] == crate["id"] and e["type"] == "owned_by_subsystem":
                    sub = by_id.get(e["to"])
                    if sub:
                        subsystems.append(sub.get("name", e["to"]))
                if e["from"] == crate["id"] and e["type"] == "touches_claim_surface":
                    crate_claims.append(e["to"])
        matched_nodes += [g["id"] for g in matched]

        review = [x for g in matched for x in g.get("review_focus", [])]
        verify = [x for g in matched for x in g.get("verification_commands", [])]
        risks = [x for g in matched for x in g.get("risk_surfaces", [])] + crate_claims
        invs = [x for g in matched for x in g.get("invariants", [])]
        docs = [x for g in matched for x in g.get("docs", [])]
        skills = [x for g in matched for x in g.get("recommended_skills", [])]
        agents = [x for g in matched for x in g.get("recommended_agents", [])]
        areas = [g["name"] for g in matched]

        entry = {
            "path": path,
            "matched_nodes": _dedup(matched_nodes),
            "subsystems": _dedup(subsystems),
            "areas": _dedup(areas),
            "docs": _dedup(docs),
            "invariants": _dedup(invs),
            "claim_surfaces": _dedup(risks),
            "review_focus": _dedup(review),
            "verification_commands": _dedup(verify),
            "recommended_skills": _dedup(skills),
            "recommended_agents": _dedup(agents),
        }
        if not matched and not crate:
            entry["note"] = "no direct guidance match — fallback to general orientation"
            entry["recommended_skills"] = ["skill:navigator"]
            entry["recommended_agents"] = ["agent:icn-code-reviewer"]
            entry["review_focus"] = [
                "No path rule matched; orient with the navigator and run the closest "
                "verification for the affected area."
            ]
        per_path.append(entry)

    combined = {
        "review_focus": _dedup([x for e in per_path for x in e["review_focus"]]),
        "verification_commands": _dedup([x for e in per_path for x in e["verification_commands"]]),
        "risk_surfaces": _dedup([x for e in per_path for x in e["claim_surfaces"]]),
        "subsystems": _dedup([x for e in per_path for x in e["subsystems"]]),
        "areas": _dedup([x for e in per_path for x in e["areas"]]),
        "invariants": _dedup([x for e in per_path for x in e["invariants"]]),
        "docs": _dedup([x for e in per_path for x in e["docs"]]),
        "recommended_skills": _dedup([x for e in per_path for x in e["recommended_skills"]]),
        "recommended_agents": _dedup([x for e in per_path for x in e["recommended_agents"]]),
    }
    return {
        "artifact": ARTIFACT_REL,
        "note": ("Orientation brief, not a gate. Non-canonical; defer to "
                 "docs/reference/project-index/source-of-truth-map.md. Asserts no "
                 "production/live/pilot readiness."),
        "paths": per_path,
        "combined": combined,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate, check, or brief the ICN agent context spine.")
    ap.add_argument("--write", action="store_true", help="regenerate the spine artifact")
    ap.add_argument("--check", action="store_true", help="exit 1 if the committed artifact is stale")
    ap.add_argument("--brief", nargs="+", metavar="PATH",
                    help="print a code-quality brief for the given changed paths")
    args = ap.parse_args()
    if sum(1 for m in (args.write, args.check, bool(args.brief)) if m) != 1:
        ap.error("exactly one of --write / --check / --brief is required")

    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    except Exception:
        commit = "unknown"
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S+00:00")

    spine = build_spine(commit, now)
    content = serialize(spine)

    if args.brief:
        data = json.loads(ARTIFACT.read_text(encoding="utf-8")) if ARTIFACT.is_file() else spine
        print(json.dumps(compute_brief(data, args.brief), indent=2, ensure_ascii=False))
        return 0

    if args.write:
        ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
        ARTIFACT.write_text(content, encoding="utf-8")
        c = spine["counts"]
        print(f"wrote {ARTIFACT_REL}: {c['nodes']} nodes, {c['edges']} edges")
        return 0

    # --check
    if not ARTIFACT.is_file():
        print(f"STALE: {ARTIFACT_REL} does not exist - run --write", file=sys.stderr)
        return 1
    committed = ARTIFACT.read_text(encoding="utf-8")
    if normalize(committed) == normalize(content):
        c = spine["counts"]
        print(f"OK: agent context spine up to date ({c['nodes']} nodes, {c['edges']} edges)")
        return 0
    print(f"STALE: {ARTIFACT_REL} differs from a fresh generation - run --write and commit", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
