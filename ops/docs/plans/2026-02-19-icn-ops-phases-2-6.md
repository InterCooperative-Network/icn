# ICN-Ops Phases 2–6 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete the icn-ops orchestration plane by wiring the Phase 1 MCP server into Claude Code, adding tests, building root-level skills, giving icn-website first-class Claude support, and adding CI.

**Architecture:** Phase 1 (repo scaffold + MCP server TypeScript code) is complete and pushed. These phases activate it: register the MCP server in Claude Code config, write tests to verify correctness, create skills that compose MCP tools, and extend Claude support to icn-website.

**Tech Stack:** TypeScript 5.4, Node.js v22, `@modelcontextprotocol/sdk` 1.26.0, `better-sqlite3` 9.6.0, Zod, Vitest (add in Task 2), Claude Code `.claude/` config (SKILL.md, agents/, settings.json, rules/)

**Prerequisite:** All commands run from `/home/ubuntu/projects/` unless noted. `icn-ops/` repo exists and phase 1 is committed.

---

## Phase 2: Activate and Test the MCP Server

### Task 1: Register MCP server in root `.claude/settings.json`

**Files:**
- Create: `.claude/settings.json`

This registers the MCP server so every Claude Code session starting from `/home/ubuntu/projects/` gets the `icn-ops` tools.

**Step 1: Create `.claude/settings.json`**

```json
{
  "mcpServers": {
    "icn-ops": {
      "command": "node",
      "args": ["/home/ubuntu/projects/icn-ops/mcp/dist/index.js"],
      "env": {
        "ICN_OPS_DB": "/home/ubuntu/projects/icn-ops/mcp/data/icn-ops.db",
        "ICN_ROOT": "/home/ubuntu/projects"
      }
    }
  },
  "permissions": {
    "allow": [
      "Bash(cargo:*)",
      "Bash(just:*)",
      "Bash(git:*)",
      "Bash(gh:*)",
      "Bash(docker:*)",
      "Bash(kubectl:*)",
      "Bash(node:*)",
      "Bash(npm:*)",
      "Bash(npx:*)",
      "Bash(sccache:*)",
      "Bash(curl:*)",
      "Bash(find:*)",
      "Bash(ls:*)",
      "Bash(cat:*)",
      "Bash(wc:*)",
      "Bash(head:*)",
      "Bash(jq:*)",
      "Bash(mkdir:*)"
    ],
    "deny": [
      "Bash(rm -rf:*)",
      "Bash(sudo:*)"
    ]
  }
}
```

**Step 2: Verify settings.json is valid JSON**

```bash
node -e "JSON.parse(require('fs').readFileSync('.claude/settings.json','utf8')); console.log('valid')"
```
Expected: `valid`

**Step 3: Rebuild the MCP server to ensure dist/ is current**

```bash
cd icn-ops/mcp && npm run build
```
Expected: exits 0, `dist/index.js` updated

**Step 4: Smoke-test the MCP server starts**

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' | node /home/ubuntu/projects/icn-ops/mcp/dist/index.js
```
Expected: JSON response with `"result"` containing `"serverInfo":{"name":"icn-ops",...}`

**Step 5: Commit**

```bash
cd /home/ubuntu/projects
git add .claude/settings.json
git commit -m "feat(claude): register icn-ops MCP server at root"
```

---

### Task 2: Add Vitest and write unit tests for session management

**Files:**
- Modify: `icn-ops/mcp/package.json`
- Create: `icn-ops/mcp/vitest.config.ts`
- Create: `icn-ops/mcp/src/tests/sessions.test.ts`

The current `package.json` has `jest` in the test script but we haven't installed it. Replace with Vitest — better ESM support, no `--experimental-vm-modules` needed.

**Step 1: Install Vitest**

```bash
cd /home/ubuntu/projects/icn-ops/mcp
npm install --save-dev vitest @vitest/coverage-v8
```
Expected: adds vitest to devDependencies

**Step 2: Update `package.json` test script**

Replace the `"test"` script in `mcp/package.json`:
```json
"test": "vitest run",
"test:watch": "vitest",
"test:coverage": "vitest run --coverage"
```

**Step 3: Create `vitest.config.ts`**

```typescript
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/tests/**/*.test.ts"],
  },
});
```

**Step 4: Write failing tests for session management**

Create `mcp/src/tests/sessions.test.ts`:

```typescript
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { initDb } from "../state/db.js";
import type Database from "better-sqlite3";
import { randomUUID } from "crypto";

// Use in-memory DB for tests
let db: Database.Database;

beforeEach(() => {
  db = initDb(":memory:");
});

afterEach(() => {
  db.close();
});

describe("sessions", () => {
  it("inserts a session and retrieves it", () => {
    const id = randomUUID();
    db.prepare(
      "INSERT INTO sessions (id, repo, worktree, task_description) VALUES (?, ?, ?, ?)"
    ).run(id, "icn", null, "testing");

    const row = db
      .prepare("SELECT * FROM sessions WHERE id = ?")
      .get(id) as { id: string; repo: string };
    expect(row.id).toBe(id);
    expect(row.repo).toBe("icn");
  });

  it("deletes session and cascades file_claims", () => {
    const id = randomUUID();
    db.prepare(
      "INSERT INTO sessions (id, repo) VALUES (?, ?)"
    ).run(id, "icn");
    db.prepare(
      "INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)"
    ).run("crates/foo/src/lib.rs", id);

    db.prepare("DELETE FROM sessions WHERE id = ?").run(id);

    const claims = db
      .prepare("SELECT * FROM file_claims WHERE session_id = ?")
      .all(id);
    expect(claims).toHaveLength(0);
  });

  it("advisory file claim prevents duplicate from another session", () => {
    const id1 = randomUUID();
    const id2 = randomUUID();
    db.prepare("INSERT INTO sessions (id, repo) VALUES (?, ?)").run(id1, "icn");
    db.prepare("INSERT INTO sessions (id, repo) VALUES (?, ?)").run(id2, "icn");

    db.prepare(
      "INSERT INTO file_claims (file_path, session_id) VALUES (?, ?)"
    ).run("crates/foo/src/lib.rs", id1);

    // Simulate the check in sessions.ts
    const existing = db
      .prepare(
        `SELECT session_id FROM file_claims
         WHERE file_path = ? AND session_id != ?`
      )
      .get("crates/foo/src/lib.rs", id2);

    expect(existing).toBeTruthy();
  });
});
```

**Step 5: Run tests — expect PASS (DB logic is correct)**

```bash
cd /home/ubuntu/projects/icn-ops/mcp && npm test
```
Expected: `3 passed`

**Step 6: Commit**

```bash
cd /home/ubuntu/projects/icn-ops
git add mcp/package.json mcp/vitest.config.ts mcp/src/tests/sessions.test.ts
git commit -m "test(mcp): add Vitest and session management unit tests"
```

---

### Task 3: Write unit tests for task board and decision tools

**Files:**
- Create: `icn-ops/mcp/src/tests/tasks.test.ts`
- Create: `icn-ops/mcp/src/tests/decisions.test.ts`
- Create: `icn-ops/mcp/src/tests/fixtures/sprint.json` (test fixture)

**Step 1: Create sprint fixture**

Create `mcp/src/tests/fixtures/sprint.json`:

```json
{
  "sprint": 99,
  "name": "Test Sprint",
  "started": "2026-02-19",
  "goals": ["test goal"],
  "tasks": [
    {
      "id": "test-task-1",
      "title": "A test task",
      "status": "pending",
      "pr": null,
      "assignee": null,
      "epic": null
    }
  ],
  "epics": {}
}
```

**Step 2: Write task board tests**

Create `mcp/src/tests/tasks.test.ts`:

```typescript
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { readFileSync, writeFileSync, copyFileSync, mkdirSync, rmSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(__dirname, "fixtures/sprint.json");
const TEST_DIR = join(__dirname, "tmp");
const TEST_SPRINT = join(TEST_DIR, "current.json");

// Point the tasks module at our test fixture via env
process.env["ICN_ROOT"] = join(__dirname, "..");

beforeEach(() => {
  mkdirSync(TEST_DIR, { recursive: true });
  copyFileSync(FIXTURE, TEST_SPRINT);
});

afterEach(() => {
  rmSync(TEST_DIR, { recursive: true, force: true });
});

describe("sprint state (file operations)", () => {
  it("reads sprint fixture", () => {
    const data = JSON.parse(readFileSync(TEST_SPRINT, "utf-8"));
    expect(data.sprint).toBe(99);
    expect(data.tasks).toHaveLength(1);
  });

  it("adds a new task and persists it", () => {
    const data = JSON.parse(readFileSync(TEST_SPRINT, "utf-8"));
    data.tasks.push({
      id: "new-task",
      title: "New task",
      status: "pending",
      pr: null,
      assignee: null,
      epic: null,
    });
    writeFileSync(TEST_SPRINT, JSON.stringify(data, null, 2));

    const reread = JSON.parse(readFileSync(TEST_SPRINT, "utf-8"));
    expect(reread.tasks).toHaveLength(2);
    expect(reread.tasks[1].id).toBe("new-task");
  });

  it("task status transition pending → in-progress", () => {
    const data = JSON.parse(readFileSync(TEST_SPRINT, "utf-8"));
    const task = data.tasks.find((t: { id: string }) => t.id === "test-task-1");
    expect(task).toBeDefined();
    task.status = "in-progress";
    task.assignee = "test-agent";
    writeFileSync(TEST_SPRINT, JSON.stringify(data, null, 2));

    const reread = JSON.parse(readFileSync(TEST_SPRINT, "utf-8"));
    const updated = reread.tasks.find((t: { id: string }) => t.id === "test-task-1");
    expect(updated.status).toBe("in-progress");
    expect(updated.assignee).toBe("test-agent");
  });
});
```

**Step 3: Write decision index tests**

Create `mcp/src/tests/decisions.test.ts`:

```typescript
import { describe, it, expect, beforeEach } from "vitest";
import { initDb } from "../state/db.js";
import type Database from "better-sqlite3";

let db: Database.Database;

beforeEach(() => {
  db = initDb(":memory:");
});

describe("decision_index", () => {
  it("inserts and searches decisions", () => {
    db.prepare(
      "INSERT INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run("0001", "Orchestration Plane Architecture", "orchestration,mcp", "state/decisions/0001-test.md", "2026-02-19");

    const results = db
      .prepare("SELECT * FROM decision_index WHERE title LIKE ?")
      .all("%Orchestration%") as Array<{ id: string; title: string }>;

    expect(results).toHaveLength(1);
    expect(results[0].id).toBe("0001");
  });

  it("tag search returns correct results", () => {
    db.prepare(
      "INSERT INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run("0001", "ADR One", "networking,kernel", "state/decisions/0001.md", "2026-02-19");

    db.prepare(
      "INSERT INTO decision_index (id, title, tags, file_path, created_at) VALUES (?, ?, ?, ?, ?)"
    ).run("0002", "ADR Two", "deployment", "state/decisions/0002.md", "2026-02-19");

    const kernelResults = db
      .prepare("SELECT * FROM decision_index WHERE tags LIKE ?")
      .all("%kernel%") as Array<{ id: string }>;

    expect(kernelResults).toHaveLength(1);
    expect(kernelResults[0].id).toBe("0001");
  });
});
```

**Step 4: Run all tests**

```bash
cd /home/ubuntu/projects/icn-ops/mcp && npm test
```
Expected: `7 passed` (3 session tests + 3 task tests + 2 decision tests = ~8)

Fix any failures before proceeding. Common issue: `import.meta.url` in tests — Vitest handles this natively so it should work.

**Step 5: Commit**

```bash
cd /home/ubuntu/projects/icn-ops
git add mcp/src/tests/
git commit -m "test(mcp): add task board and decision index unit tests"
```

---

## Phase 3: Root-Level Skills

### Task 4: Create `/status` skill

**Files:**
- Create: `icn-ops/automation/skills/status/SKILL.md`
- Create: `.claude/skills/status/SKILL.md` (symlink or copy — copy for simplicity)

The `/status` skill gives a dashboard view: sprint progress, active sessions, worktree freshness, CI state.

**Step 1: Create the skill in icn-ops**

Create `icn-ops/automation/skills/status/SKILL.md`:

```markdown
---
name: status
description: Show full project status dashboard — active sessions, sprint tasks, worktree freshness, CI, and cluster health
---

You are the ICN development status dashboard. Show the current state of the entire development environment.

Use the icn-ops MCP tools in this order:
1. `list_sessions` — show active agent sessions (skip if empty)
2. `get_tasks` with status="all" — show sprint task board grouped by status
3. `worktree_status` — show worktrees sorted by staleness (stale = >10 commits behind main)
4. `ci_status` with repo="icn" — show CI for current branch
5. `cluster_health` — show K3s pod status and service reachability
6. `build_cache_status` — show sccache stats (one line summary)

Format output as a concise dashboard. Use ✅ / ⏳ / ❌ / ⚠️ symbols for status. Group by section with bold headers. Total output should be scannable in 30 seconds.

If any MCP tool fails (server not running), show the tool name and error, continue with remaining tools.
```

**Step 2: Copy to root `.claude/skills/`**

```bash
mkdir -p /home/ubuntu/projects/.claude/skills/status
cp /home/ubuntu/projects/icn-ops/automation/skills/status/SKILL.md \
   /home/ubuntu/projects/.claude/skills/status/SKILL.md
```

**Step 3: Commit**

```bash
cd /home/ubuntu/projects/icn-ops
git add automation/skills/status/
git commit -m "feat(skills): add /status dashboard skill"
```

---

### Task 5: Create `/sync-and-build` skill

**Files:**
- Create: `icn-ops/automation/skills/sync-and-build/SKILL.md`
- Create: `.claude/skills/sync-and-build/SKILL.md`

**Step 1: Create the skill**

Create `icn-ops/automation/skills/sync-and-build/SKILL.md`:

```markdown
---
name: sync-and-build
description: Sync ICN documentation to the website and verify the build succeeds
disable-model-invocation: true
---

Run the cross-repo content pipeline:

1. Check for doc changes:
   ```bash
   git -C /home/ubuntu/projects/icn diff --name-only HEAD~1 HEAD -- docs/ | head -20
   ```
   Report which docs changed (or "no recent changes").

2. Run the sync script:
   ```bash
   cd /home/ubuntu/projects/icn-website && bash scripts/sync-from-icn.sh
   ```
   Report: files synced count, any errors.

3. Build the website:
   ```bash
   cd /home/ubuntu/projects/icn-website && npm run build 2>&1
   ```
   Report: success or failure with error excerpt.

4. If build succeeded, check for broken internal links:
   ```bash
   grep -r 'href="/' /home/ubuntu/projects/icn-website/src --include="*.astro" -l | head -10
   ```
   Report: pages with absolute hrefs that might be broken (informational only).

5. Summary: "Sync complete — N files synced, build ✅/❌"
```

**Step 2: Copy to root skills**

```bash
mkdir -p /home/ubuntu/projects/.claude/skills/sync-and-build
cp /home/ubuntu/projects/icn-ops/automation/skills/sync-and-build/SKILL.md \
   /home/ubuntu/projects/.claude/skills/sync-and-build/SKILL.md
```

**Step 3: Commit**

```bash
cd /home/ubuntu/projects/icn-ops
git add automation/skills/sync-and-build/
git commit -m "feat(skills): add /sync-and-build cross-repo pipeline skill"
```

---

### Task 6: Create `/worktree` skill

**Files:**
- Create: `icn-ops/automation/skills/worktree/SKILL.md`
- Create: `.claude/skills/worktree/SKILL.md`

**Step 1: Create the skill**

Create `icn-ops/automation/skills/worktree/SKILL.md`:

```markdown
---
name: worktree
description: Manage ICN git worktrees — create, list status, cleanup stale, rebase on main
disable-model-invocation: true
---

Manage worktrees in /home/ubuntu/projects/icn-wt/. The worktree script is at /home/ubuntu/projects/icn/scripts/worktrees.sh.

Parse the argument to determine the subcommand:

**`/worktree status`** (default if no args):
Use the `worktree_status` MCP tool to get structured status. Format as a table:
| Name | Branch | Behind Main | Claimed By |
And flag stale worktrees (>10 commits behind) with ⚠️.

**`/worktree create <name>`**:
```bash
cd /home/ubuntu/projects/icn && bash scripts/worktrees.sh create <name>
```
Report the new worktree path and branch name.

**`/worktree cleanup`**:
1. Call `worktree_status` MCP tool
2. List worktrees where `stale=true` and `claimedBy=null`
3. Ask for confirmation before removing each stale unclaimed worktree:
   ```bash
   cd /home/ubuntu/projects/icn && bash scripts/worktrees.sh remove <name>
   ```

**`/worktree rebase <name>`**:
```bash
cd /home/ubuntu/projects/icn-wt/<name>/icn && git fetch origin && git rebase origin/main
```
Report: rebased N commits, or conflicts found.

If no subcommand recognized, show: "Usage: /worktree [status|create <name>|cleanup|rebase <name>]"
```

**Step 2: Copy to root skills**

```bash
mkdir -p /home/ubuntu/projects/.claude/skills/worktree
cp /home/ubuntu/projects/icn-ops/automation/skills/worktree/SKILL.md \
   /home/ubuntu/projects/.claude/skills/worktree/SKILL.md
```

**Step 3: Commit**

```bash
cd /home/ubuntu/projects/icn-ops
git add automation/skills/worktree/
git commit -m "feat(skills): add /worktree lifecycle management skill"
```

---

### Task 7: Create root orchestrator agent

**Files:**
- Create: `.claude/agents/orchestrator.md`

**Step 1: Create the agent**

Create `.claude/agents/orchestrator.md`:

```markdown
---
name: orchestrator
description: Cross-repo ICN development coordinator. Knows all repos, their relationships, current sprint state, and can delegate to the right place. Use when tasks span multiple repos or you need a birds-eye view.
---

You are the ICN development orchestrator. You understand the full development environment:

## Repos You Coordinate

| Repo | Path | Purpose |
|------|------|---------|
| `icn` | `/home/ubuntu/projects/icn` | Main ICN substrate daemon (Rust, 39 crates). Cargo workspace at `icn/icn/`. |
| `icn-website` | `/home/ubuntu/projects/icn-website` | Astro 5 public website. `src/content/docs/` is gitignored — synced from `icn/docs/`. |
| `icn-ops` | `/home/ubuntu/projects/icn-ops` | This orchestration plane. MCP server, state, CI. |
| `icn-wt` | `/home/ubuntu/projects/icn-wt` | Git worktrees for parallel feature dev. Each is a full `icn` checkout. |

## Key Relationships

- ICN docs sync one-way: `icn/docs/` → `icn-website/src/content/docs/` (via `sync-from-icn.sh`)
- Worktrees are branches of `icn/` — build commands work the same (`cd icn-wt/<name>/icn && cargo build`)
- Cluster at `10.8.10.40` runs whatever is in `icn/deploy/k8s/`
- `homelab-inventory` (not on this VM) manages the infrastructure — read-only from here

## Your Capabilities

1. **Triage tasks to the right repo**: "This API change also affects the TypeScript SDK and the website docs"
2. **Detect cross-repo impacts**: ICN wire format change → TypeScript SDK needs update → website API docs need update
3. **Check for conflicts**: Before claiming work, call `list_sessions` to see if another agent is touching the same files
4. **Sprint awareness**: Call `get_tasks` to see what's in progress before taking on new work
5. **Environment health**: Call `cluster_health` and `repo_status` to understand current state

## When to Delegate

- Rust/Cargo work → use the icn agents in `icn/.claude/agents/` (icn-rust-core, icn-gateway-api, etc.)
- Website Astro work → work in `icn-website/` context with its own Claude config
- Infrastructure mutations → do NOT do this from here; homelab mutations belong in `homelab-inventory`

## ICN Invariants (Never Violate)

1. Adversarial-by-default: peers untrusted until trust established
2. Determinism: same inputs → same outputs for all state transitions
3. Canonical encodings: no wire format changes without explicit intent + docs + tests
4. No panics in protocol paths: always `Result<T, E>`
5. Kernel/app boundaries: enforced by forbidden-deps CI gate (no domain crates in kernel crates)
```

**Step 2: Commit**

```bash
cd /home/ubuntu/projects
git add .claude/agents/orchestrator.md
git commit -m "feat(claude): add cross-repo orchestrator agent"
```

---

## Phase 4: Website DX

### Task 8: Create `icn-website/CLAUDE.md`

**Files:**
- Create: `icn-website/CLAUDE.md`

**Step 1: Write CLAUDE.md**

```markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this repository.

## What This Repo Is

The ICN public website at `intercooperative.network`. Built with Astro 5 + TypeScript. Static site deployed via GitHub Pages.

**This repo is content + presentation only.** The substrate docs live in `icn/docs/` — they sync here at build time.

## Commands

```bash
npm ci                  # Install dependencies (use ci, not install, for reproducibility)
npm run dev             # Dev server at http://localhost:4321
npm run build           # Production build (auto-syncs icn/docs/ first)
npm run lint            # Astro TypeScript check
npm run format          # Prettier formatting
npm run deploy          # Build + deploy to GitHub Pages
```

## Content Sync — CRITICAL

`src/content/docs/` is **gitignored**. It is synced from `icn/docs/` at build time via:
```bash
bash scripts/sync-from-icn.sh
```

**DO NOT edit files in `src/content/docs/`** — edits will be overwritten on the next build. Edit the source files in `/home/ubuntu/projects/icn/docs/` instead, then run the sync.

To manually sync + preview:
```bash
bash scripts/sync-from-icn.sh && npm run dev
```

## Architecture

- `src/pages/` — File-based routing. Each `.astro` file = one URL route.
- `src/components/` — Reusable components. NetworkGraph uses D3.js. Search uses Fuse.js.
- `src/content/` — Content collections with Zod schema validation (docs synced, blog manual)
- `src/lib/markdown.ts` — Markdown rendering with link rewriting for synced docs
- `src/styles/global.css` — CSS custom properties (design tokens). Always use these vars, never hardcode colors.

## Design System

CSS custom properties in `src/styles/global.css`:
- `--color-primary: #00D4AA` (teal)
- `--color-secondary: #3B82F6` (blue)
- `--color-bg: #0A0E1A` (dark navy)
- Fonts: Inter (body), Outfit (headings), JetBrains Mono (code)

Dark theme is default. Do not add `prefers-color-scheme` media queries — the theme is always dark.

## Adding Content

- **New page**: Create `src/pages/<route>.astro`. Use existing pages as templates.
- **Blog post**: Create `src/pages/blog/<slug>.astro` with frontmatter.
- **ICN docs**: Edit source in `icn/docs/`, never here.
```

**Step 2: Commit to icn-website**

```bash
cd /home/ubuntu/projects/icn-website
git add CLAUDE.md
git commit -m "feat(claude): add CLAUDE.md for website development guidance"
```

---

### Task 9: Create `icn-website/.claude/settings.json` with hooks

**Files:**
- Create: `icn-website/.claude/settings.json`

Two hooks:
1. **Prettier auto-format** — runs after every Edit/Write on `.astro` and `.ts` files
2. **Block synced content guard** — PreToolUse: blocks edits to `src/content/docs/`

**Step 1: Create `.claude/` directory and `settings.json`**

```bash
mkdir -p /home/ubuntu/projects/icn-website/.claude
```

Create `icn-website/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "bash -c 'case \"$CLAUDE_TOOL_INPUT_FILE_PATH\" in */src/content/docs/*) echo \"{\\\"decision\\\":\\\"block\\\",\\\"reason\\\":\\\"src/content/docs/ is synced from icn/docs/ — edit the source at /home/ubuntu/projects/icn/docs/ instead, then run: bash scripts/sync-from-icn.sh\\\"}\" && exit 2 ;; esac'",
            "timeout": 3000
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "bash -c 'case \"$CLAUDE_TOOL_INPUT_FILE_PATH\" in *.astro|*.ts|*.tsx|*.js|*.mjs|*.css|*.json) cd /home/ubuntu/projects/icn-website && npx prettier --write \"$CLAUDE_TOOL_INPUT_FILE_PATH\" 2>/dev/null || true ;; esac'",
            "timeout": 15000
          }
        ]
      }
    ]
  }
}
```

**Step 2: Verify JSON is valid**

```bash
node -e "JSON.parse(require('fs').readFileSync('/home/ubuntu/projects/icn-website/.claude/settings.json','utf8')); console.log('valid')"
```
Expected: `valid`

**Step 3: Commit**

```bash
cd /home/ubuntu/projects/icn-website
git add .claude/settings.json
git commit -m "feat(claude): add hooks — Prettier auto-format + synced-content guard"
```

---

### Task 10: Create `icn-website/.claude/rules/astro-conventions.md`

**Files:**
- Create: `icn-website/.claude/rules/astro-conventions.md`

**Step 1: Create the rule**

```bash
mkdir -p /home/ubuntu/projects/icn-website/.claude/rules
```

Create `icn-website/.claude/rules/astro-conventions.md`:

```markdown
---
description: Astro and website conventions for icn-website. Applied to all .astro, .ts, .css files.
globs: ["src/**/*.astro", "src/**/*.ts", "src/**/*.css"]
---

## Astro Conventions

**File-based routing only.** Pages go in `src/pages/`. No custom router.

**Content collections for data.** If data has more than 3 items, put it in `src/content/` with a Zod schema, not hardcoded in `.astro` files.

**No inline styles.** Use CSS custom properties from `src/styles/global.css` or scoped `<style>` blocks in `.astro` files. Never `style="..."` attributes.

**Script placement.** Put JavaScript in `<script>` tags at the bottom of `.astro` files, or in `src/scripts/` as modules. No `onclick=` attributes.

**Dark theme is permanent.** Do not add `prefers-color-scheme` media queries. The theme is always dark (`--color-bg: #0A0E1A`).

**Component props are typed.** All Astro component props must have TypeScript types defined in `interface Props {}`.

**Images.** Use Astro's `<Image>` component from `astro:assets` for optimized images, not raw `<img>` tags.

**Imports.** Use `@/` path aliases for `src/` imports where configured. Check `astro.config.mjs` for alias config.

## What NOT to Do

- Do not edit `src/content/docs/` — it is synced from `icn/docs/` and gitignored
- Do not install new npm packages without checking `package.json` first for existing equivalents
- Do not use Tailwind utility classes for new styles — the site uses CSS custom properties
- Do not remove `export const prerender = true` from pages (static site)
```

**Step 2: Commit**

```bash
cd /home/ubuntu/projects/icn-website
git add .claude/rules/astro-conventions.md
git commit -m "feat(claude): add Astro conventions rule for website"
```

---

## Phase 5: CI for icn-ops

### Task 11: Add GitHub Actions CI for icn-ops

**Files:**
- Create: `icn-ops/.github/workflows/ci.yml`

**Step 1: Create the workflow**

```bash
mkdir -p /home/ubuntu/projects/icn-ops/.github/workflows
```

Create `icn-ops/.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  mcp-server:
    name: MCP Server (TypeScript)
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "22"
          cache: "npm"
          cache-dependency-path: mcp/package-lock.json

      - name: Install dependencies
        run: cd mcp && npm ci

      - name: Type check
        run: cd mcp && npx tsc --noEmit

      - name: Run tests
        run: cd mcp && npm test

      - name: Build
        run: cd mcp && npm run build

  validate-state:
    name: Validate State Files
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "22"

      - name: Validate JSON state files
        run: |
          for f in state/config/*.json state/sprint/current.json; do
            echo "Validating $f..."
            node -e "JSON.parse(require('fs').readFileSync('$f','utf8')); console.log('  OK')"
          done
```

**Step 2: Commit and push**

```bash
cd /home/ubuntu/projects/icn-ops
git add .github/workflows/ci.yml
git commit -m "ci: add GitHub Actions for MCP server lint, test, build + state validation"
git push
```

**Step 3: Verify CI passes on GitHub**

```bash
gh run list --repo InterCooperative-Network/icn-ops --limit 3
```
Expected: one run in progress or completed for the `ci` workflow.

---

## Phase 6: Update Root CLAUDE.md

### Task 12: Update root CLAUDE.md to document MCP tools and skills

**Files:**
- Modify: `CLAUDE.md`

The root CLAUDE.md now needs to document the MCP tools, skills, and orchestrator agent that were created in phases 3–4. The icn-ops section added in Phase 1 should be expanded.

**Step 1: Add a "Cross-Repo Skills" section to root CLAUDE.md**

Add after the `## Orchestration Plane (icn-ops/)` section:

```markdown
## Cross-Repo Skills (root `.claude/skills/`)

These skills work from any repo in this workspace:

| Skill | Invocation | Purpose |
|-------|-----------|---------|
| `/status` | User or Claude | Full dashboard: sessions, sprint, worktrees, CI, cluster |
| `/sync-and-build` | User | Sync icn/docs/ → icn-website, then build |
| `/worktree` | User | Manage worktrees: status, create, cleanup, rebase |

## Agents (root `.claude/agents/`)

| Agent | When to use |
|-------|------------|
| `orchestrator` | Tasks spanning multiple repos, cross-repo impact analysis, sprint triage |

For ICN-specific agents (rust-core, gateway-api, security, etc.) see `icn/.claude/agents/`.
```

**Step 2: Commit**

```bash
cd /home/ubuntu/projects
git add CLAUDE.md
git commit -m "docs(claude): document MCP tools, skills, and orchestrator agent"
```

---

## Verification Checklist

After all phases are complete, verify:

```bash
# MCP server starts cleanly
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' | node /home/ubuntu/projects/icn-ops/mcp/dist/index.js

# All MCP tests pass
cd /home/ubuntu/projects/icn-ops/mcp && npm test

# icn-ops CI passing on GitHub
gh run list --repo InterCooperative-Network/icn-ops --limit 3

# Root settings.json is valid
node -e "JSON.parse(require('fs').readFileSync('/home/ubuntu/projects/.claude/settings.json','utf8')); console.log('valid')"

# Skills exist at root level
ls /home/ubuntu/projects/.claude/skills/

# Website CLAUDE.md exists
ls /home/ubuntu/projects/icn-website/CLAUDE.md

# Website hooks settings valid
node -e "JSON.parse(require('fs').readFileSync('/home/ubuntu/projects/icn-website/.claude/settings.json','utf8')); console.log('valid')"
```

All should succeed.
