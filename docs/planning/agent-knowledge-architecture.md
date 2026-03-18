# Agent Knowledge Architecture Spec

**Status:** Draft
**Date:** 2026-03-18
**Author:** Matt Faherty
**Purpose:** Design the retrieval and indexing architecture that lets any agent — Claude Code on icn-dev, Cowork on Zentith, or future surfaces — find and use the right knowledge on demand.

---

## 1. Problem Statement

Two knowledge systems exist. Neither knows about the other. Both depend on agents reading the right files in the right order. There is no search, no structured lookup, and no cross-system bridging. The result: agents working in one system are blind to knowledge in the other, and even within a single system, relevant docs are only found if the agent happens to browse the right directory.

### Current Systems

**System A: ICN Repo** (`~/projects/icn`)
- Agent: Claude Code on icn-dev (or any Claude Code session)
- Entry: `CLAUDE.md` (33KB, auto-loaded) → `.claude/` (rules, agents, skills, hooks) → `docs/` (200+ files, 40+ dirs)
- Deep context: `docs/GOLDEN_PROMPT.md` (27KB)
- Strength: Rich operational context for ICN development
- Weakness: No path to personal context, cooperative movement knowledge, or cross-project state

**System B: Launchpad** (`.claude_launchpad/`)
- Agent: Cowork sessions, coordination-os plugin
- Entry: `CLAUDE.md` (19KB, via system prompt) → `BOOT.md` → `TASKS.md` → `SYSTEM_MAP.md`
- Knowledge: `memory/` (80+ files), `operations/`, `fabric/`, `projects/`
- Strength: Rich personal context, cross-project state, identity, relationships
- Weakness: No path to ICN codebase context, specialized agents, or repo-level docs

### Failure Modes

1. **Blind spots:** Cowork session designing ICN mobile UX cannot access the 11 specialized ICN agents, the GOLDEN_PROMPT, or the repo's 200+ docs. Claude Code session on icn-dev cannot access funding strategy, cooperative contacts, or personal health/disability context that affects scheduling.

2. **Stale indexes:** INDEX.md, SYSTEM_MAP.md, and typed sub-indexes are static files that go stale. No mechanism forces updates when docs change. The knowledge base grows but the indexes don't keep pace.

3. **No retrieval:** Agent wants "what do I know about federation settlement?" — must manually browse directories, read INDEX.md, guess which files, then read them. No structured lookup, no search, no tagging.

4. **Redundant context in every entry point:** CLAUDE.md (Launchpad) carries a 19KB personal profile including terms, projects, people, and infrastructure — loaded into context on every session whether needed or not. Most sessions use <20% of it.

5. **No freshness signal:** An agent reading `memory/knowledge/icn/architecture.md` has no way to know if it's current or stale without reading it and comparing against live state. SYSTEM_MAP.md addresses this for some files but is itself a static document that gets stale.

---

## 2. Design Principles

1. **Repo is always the baseline for code-adjacent knowledge.** The ICN repo's docs/ directory is the source of truth for anything about ICN's architecture, codebase, APIs, deployment, and development process. Launchpad can reference it but should not duplicate it.

2. **Launchpad is the baseline for personal and cross-project knowledge.** Identity, relationships, funding, cooperative movement context, and cross-project orchestration live in Launchpad. The repo should not duplicate these.

3. **Agents should pull context on demand, not load everything upfront.** Instead of cramming 19KB of personal context into every session, provide a lightweight orientation + a retrieval mechanism that pulls relevant knowledge when the task requires it.

4. **Every doc must be findable in ≤2 hops from an entry point.** If a new agent session can't reach a document within: entry point → index → target, the document is effectively invisible.

5. **Indexes must be structured data, not prose.** Static prose indexes (like the current INDEX.md) are human-readable but agent-hostile. Structured metadata (YAML, JSON, or markdown tables with consistent fields) enables programmatic lookup.

6. **Freshness must be tracked.** Every indexed document should have a `last_verified` date so agents can assess whether to trust it or re-verify against live state.

---

## 3. Architecture

### 3.1 Three-Layer Model

```
Layer 1: Entry Points (auto-loaded, minimal)
  ├── ICN CLAUDE.md (repo) — Claude Code sessions
  ├── Launchpad CLAUDE.md — Cowork sessions
  └── Both point to Layer 2

Layer 2: Indexes (structured, queryable)
  ├── KNOWLEDGE_INDEX.yaml — unified structured index
  ├── Per-system navigation (docs/INDEX.md, memory/knowledge/INDEX.md)
  └── Cross-references between systems

Layer 3: Knowledge (the actual docs)
  ├── ICN repo docs/ (200+ files)
  ├── Launchpad memory/ (80+ files)
  ├── Launchpad operations/ + fabric/
  └── Projects (ICN, summit, homelab, etc.)
```

### 3.2 Layer 1: Entry Points (redesigned)

**Goal:** Minimal context that orients the agent and teaches it how to retrieve more.

#### ICN Repo CLAUDE.md (for Claude Code)

Keep the current operational content (build commands, architecture, git workflow, agent rules). These are needed every session.

**Add:** A "Knowledge Retrieval" section that teaches the agent how to find deeper context:

```markdown
## Knowledge Retrieval

When you need context beyond this file:

1. **ICN architecture/design:** Read `docs/GOLDEN_PROMPT.md` (27KB master context)
2. **Current project state:** Read `docs/STATE.md`
3. **Planning/strategy:** Browse `docs/planning/` and `docs/strategy/`
4. **Mobile UX:** Read `docs/mobile/icn-mobile-ux-spec-v1.md`
5. **Personal context** (scheduling, health, funding, contacts): Available via Launchpad
   at `.claude_launchpad/CLAUDE.md` (if accessible) or ask the user
6. **Cooperative movement context:** Available via Launchpad `memory/knowledge/cooperative/`
7. **Structured lookup:** See `docs/KNOWLEDGE_INDEX.yaml` for metadata-tagged doc catalog
```

**Remove from CLAUDE.md:** Nothing. It's already well-scoped for its purpose.

#### Launchpad CLAUDE.md (for Cowork)

**Problem:** Currently 19KB, loaded on every session. Most of it is reference material (education history, career arc, psychological profile, infrastructure details) that's rarely needed in full.

**Proposed change:** Split into:

- **CLAUDE.md** (slimmed to ~8KB) — identity essentials, active projects, current priorities, terms hot-cache, and retrieval instructions
- **memory/context/identity-full.md** — the complete personal profile (education, career, psych profile, reddit, writing voice)
- **memory/context/infrastructure.md** — the detailed homelab/network architecture (already partially exists)

The slim CLAUDE.md tells agents: "I am Matt, here's what's active, here's how to find more." The full profile is one hop away when needed.

**Add to slim CLAUDE.md:** A "Knowledge Retrieval" section parallel to the repo version:

```markdown
## Knowledge Retrieval

When you need context beyond this file:

1. **Full personal profile:** `memory/context/identity-full.md`
2. **ICN project state:** `projects/icn/` (crate reference, ecosystem map, mobile spec, status)
3. **ICN codebase context:** ICN repo `CLAUDE.md` + `docs/GOLDEN_PROMPT.md` (on icn-dev)
4. **Cooperative movement:** `memory/knowledge/cooperative/` (79 docs indexed)
5. **Writing projects:** `memory/knowledge/writing/` (79 docs indexed)
6. **People/contacts:** `memory/people/` (40+ contacts)
7. **Funding strategy:** `memory/projects/funding-strategy.md`
8. **Infrastructure:** `memory/knowledge/homelab/` + `fabric/topology.yaml`
9. **Structured lookup:** `KNOWLEDGE_INDEX.yaml` for metadata-tagged catalog
10. **Task state:** `TASKS.md` (current), `ACTIVITY_LOG.md` (history)
```

### 3.3 Layer 2: Unified Knowledge Index

**This is the core new artifact.**

A single structured index file (`KNOWLEDGE_INDEX.yaml`) that catalogs every significant document across both systems with consistent metadata. This enables:

- "What docs exist about X?" → grep/filter the index
- "Is this doc current?" → check `last_verified`
- "What system is this in?" → check `system` field
- "What should I read for task Y?" → filter by `domain` + `type`

#### Schema

```yaml
# KNOWLEDGE_INDEX.yaml
version: 1
last_updated: "2026-03-18"

documents:
  - id: icn-golden-prompt
    title: "ICN Golden Development Prompt"
    path: "docs/GOLDEN_PROMPT.md"
    system: icn-repo
    domain: icn
    type: agent-context
    size_kb: 27
    last_verified: "2026-03-18"
    summary: "Master context for AI agents working on ICN. Architecture, current state, coding conventions."
    depends_on: [icn-claude-md]
    tags: [entry-point, architecture, development, agent-context]

  - id: icn-mobile-ux-spec
    title: "ICN Mobile Member UX Spec v1"
    path: "docs/mobile/icn-mobile-ux-spec-v1.md"
    system: icn-repo  # migrated from launchpad
    domain: icn
    type: spec
    size_kb: 36
    last_verified: "2026-03-18"
    summary: "Build-facing mobile UX spec. Navigation, object model, vocabulary, provenance, QR, demo flows."
    depends_on: [icn-crate-reference, icn-ecosystem-map, icn-gap-analysis]
    tags: [mobile, ux, spec, gateway-api, react-native]

  - id: icn-gap-analysis
    title: "ICN System-by-System Gap Analysis"
    path: "docs/strategy/ICN-Gap-Analysis-March-2026.md"
    system: icn-repo
    domain: icn
    type: analysis
    size_kb: 27
    last_verified: "2026-03-17"
    summary: "Every subsystem traced from spec to code. Only governance proven. Federation settlement undefined. Mana killed."
    tags: [gap-analysis, reality-check, status]

  # ... (every significant doc gets an entry)

  - id: lp-funding-strategy
    title: "Funding Strategy"
    path: "memory/projects/funding-strategy.md"
    system: launchpad
    domain: funding
    type: strategy
    last_verified: "2026-03-12"
    summary: "Anti-extractive funding pipeline. ACCES-VR, STF, CFNE, Working World, KIVA, PASS."
    tags: [funding, grants, cooperative, disability]

  - id: lp-ny-coop-summit
    title: "NY Cooperative Summit 2026"
    path: "memory/projects/ny-coop-summit.md"
    system: launchpad
    domain: cooperative
    type: project
    last_verified: "2026-03-12"
    summary: "October 2026 summit. Budget $24K. Location TBD Albany/Schenectady. Matt = core organizer."
    tags: [summit, cooperative, event, icn-demo]
```

#### Fields

| Field | Type | Purpose |
|-------|------|---------|
| `id` | string | Unique identifier, grep-friendly |
| `title` | string | Human-readable name |
| `path` | string | Relative path within its system |
| `system` | enum | `icn-repo` or `launchpad` |
| `domain` | enum | `icn`, `cooperative`, `homelab`, `writing`, `funding`, `personal`, `infrastructure` |
| `type` | enum | `agent-context`, `spec`, `analysis`, `strategy`, `reference`, `project`, `knowledge`, `operational` |
| `size_kb` | int | Rough size for context-budget planning |
| `last_verified` | date | When someone confirmed this doc is still accurate |
| `summary` | string | 1-2 sentence description |
| `depends_on` | list | Other doc IDs this doc references or requires |
| `tags` | list | Freeform tags for search/filter |

#### Location

The index lives in **both** systems:
- `~/projects/icn/docs/KNOWLEDGE_INDEX.yaml` (repo copy)
- `.claude_launchpad/KNOWLEDGE_INDEX.yaml` (launchpad copy)

One is the source of truth (Launchpad, since it spans both systems). The repo copy is synced manually or by a future automation.

### 3.4 Layer 3: Knowledge Organization (minimal changes)

The actual doc trees stay where they are. The index makes them findable. Only two structural changes needed:

1. **Launchpad CLAUDE.md slimming** — move reference-heavy sections to dedicated files in `memory/context/`
2. **Cross-system pointers** — each system's entry point includes a "how to access the other system" section

---

## 4. Cross-System Bridging

### 4.1 ICN Repo → Launchpad

An agent on icn-dev needs Launchpad context when:
- Scheduling work (health/disability constraints)
- Writing for an audience (cooperative organizer context)
- Funding applications (grant strategy, contacts)
- Summit planning (people, budget, timeline)

**Bridge mechanism:** The repo's CLAUDE.md tells agents that personal/cross-project context lives in Launchpad. If the agent is on icn-dev (WSL2 or SSH), Launchpad is accessible at `/mnt/c/Users/mattf/.claude_launchpad/` or via OneDrive sync. If not accessible, the agent asks the user.

### 4.2 Launchpad → ICN Repo

A Cowork session needs repo context when:
- Designing ICN features (crate reference, gateway API, architecture)
- Evaluating ICN state (gap analysis, sprint plan, demo status)
- Writing about ICN (whitepaper, pitch, scenarios)

**Bridge mechanism:** Launchpad's CLAUDE.md tells agents that ICN codebase context lives in the repo. From Cowork, the repo is accessible via WSL2 (`wsl.exe` method documented in CLAUDE.md). The `projects/icn/` directory in Launchpad holds planning docs that have been migrated to the repo — these are convenience copies, the repo is authoritative.

### 4.3 Specialized Agents (future)

The 11 ICN agents in `.claude/agents/` are currently only available to Claude Code sessions on the repo. Cowork sessions can't invoke them. A future enhancement would be:

- An MCP tool that proxies queries to the repo's specialized agents
- Or: extract agent knowledge into standalone docs that Cowork can read

This is not needed now but should be designed for.

---

## 5. Freshness Protocol

### Problem

Static docs go stale. An agent reading `memory/projects/icn.md` doesn't know if it reflects yesterday's state or last month's.

### Solution

Every entry in KNOWLEDGE_INDEX.yaml has a `last_verified` date. The protocol:

1. **On document update:** Author updates `last_verified` in the index entry
2. **On session start:** Agent can scan the index for stale entries (>30 days unverified)
3. **On use:** If an agent reads a doc and finds it's stale, it updates the `last_verified` date after confirming accuracy — or flags it as needing review

### Staleness Tiers

| Age | Status | Action |
|-----|--------|--------|
| <7 days | Fresh | Trust it |
| 7-30 days | Aging | Trust it but note the date |
| 30-90 days | Stale | Verify key claims before acting on them |
| >90 days | Historical | Treat as background context, not operational truth |

---

## 6. Retrieval Patterns

### Pattern 1: "What do I know about X?"

```
Agent receives task mentioning "federation settlement"
  → Scans KNOWLEDGE_INDEX.yaml for tags containing "federation" or "settlement"
  → Finds: icn-gap-analysis (tags: [gap-analysis, reality-check])
           icn-ecosystem-map (tags: [federation, surfaces])
           icn-forward-plan (tags: [federation, roadmap])
  → Reads the most relevant (gap analysis, since it's tagged as reality-check)
  → Now has grounded context for the task
```

### Pattern 2: "Who should I contact about X?"

```
Agent needs to discuss summit logistics
  → Scans KNOWLEDGE_INDEX.yaml for domain: cooperative, type: project
  → Finds: lp-ny-coop-summit
  → Reads it → finds Joe Marraffino as key contact
  → Scans memory/people/ for joe-marraffino.md
  → Has full contact context
```

### Pattern 3: "Is this doc still accurate?"

```
Agent reads memory/projects/icn.md for ICN status
  → Checks KNOWLEDGE_INDEX.yaml entry: last_verified: 2026-03-12
  → 6 days old → Fresh tier → trusts it
  → If >30 days: would verify against live repo state before acting on claims
```

### Pattern 4: "What should I read for this task?"

```
Agent starting a mobile UX implementation session
  → Scans KNOWLEDGE_INDEX.yaml for tags containing "mobile" + domain: icn
  → Finds: icn-mobile-ux-spec (spec, 36KB)
           icn-crate-reference (reference, 25KB)
           icn-gap-analysis (analysis, 27KB)
  → Reads in priority order: spec first, then crate reference for implementation details
```

---

## 7. Implementation Phases

### Phase 1: Index Creation (1-2 sessions)

Build KNOWLEDGE_INDEX.yaml from scratch by auditing both systems:
- Catalog every significant doc in Launchpad memory/ and projects/
- Catalog every significant doc in ICN repo docs/
- Assign IDs, domains, types, tags, freshness dates
- Place copies in both systems

### Phase 2: Entry Point Optimization (1 session)

- Slim Launchpad CLAUDE.md (move reference sections → memory/context/)
- Add "Knowledge Retrieval" sections to both CLAUDE.md files
- Add cross-system bridge instructions
- Update BOOT.md to reference the index

### Phase 3: Freshness Audit (1 session)

- Walk through every indexed doc
- Mark last_verified dates
- Identify and archive docs that are >90 days stale
- Flag docs that need updating

### Phase 4: Agent Integration (future)

- Build a Claude Code skill (`/knowledge-lookup`) that queries the index
- Build a Cowork skill that can read ICN repo docs via WSL2
- Consider: MCP tool that serves the index as structured data

---

## 8. What This Does NOT Include

- **No database.** The index is a YAML file, not a SQLite DB or vector store. Agents can grep it.
- **No auto-sync.** The index is manually maintained. Automation is Phase 4+.
- **No LLM embeddings.** Semantic search is interesting but premature. Tags + grep is sufficient.
- **No restructuring of existing docs.** The docs stay where they are. The index makes them findable.
- **No merging of the two systems.** Launchpad and ICN repo serve different purposes. They should stay separate but connected.

---

## 9. Success Criteria

1. A new agent session (Claude Code or Cowork) can find any significant document within 2 hops from its entry point
2. An agent can answer "what do I know about X?" by scanning the index, without browsing directories
3. Every document in the index has a freshness date that's <30 days old
4. The ICN repo's CLAUDE.md explicitly teaches agents how to access Launchpad context
5. The Launchpad's CLAUDE.md explicitly teaches agents how to access repo context
6. No agent session is blind to knowledge that exists in the other system

---

*Spec generated 2026-03-18. Implement in order: Phase 1 (index) → Phase 2 (entry points) → Phase 3 (freshness) → Phase 4 (tooling).*
