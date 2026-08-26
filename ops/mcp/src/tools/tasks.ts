import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { dirname, join } from "path";
import { resolveOpsStatePath } from "../paths.js";

const SPRINT_FILE = resolveOpsStatePath("sprint", "current.json");

export interface Task {
  id: string;
  title: string;
  status: string;
  pr: number | null;
  assignee: string | null;
  epic: string | null;
}

export interface SprintState {
  sprint: number;
  name: string;
  started: string;
  goals: string[];
  tasks: Task[];
  epics: Record<string, string>;
  /**
   * Cadence — the field the registered resolver actually decides activity on.
   * Optional because this interface predates `icn-sprint-state/v2` (#2636).
   */
  cadence?: string;
  /** Second dormancy-bearing key recognised by `declares_dormancy()`. */
  lifecycle?: string;
  /** v2. `null` is an explicit dormancy declaration; ABSENT is silence, not null. */
  active_sprint?: number | string | null;
  /** Display label only — NOT the activity determinant. See `resolveSprintCadence`. */
  status?: string;
  /** v2, and deliberately `null` today: two numbering planes disagree (#2637). */
  next_sprint_number?: number | string | null;
}

function loadSprint(): SprintState {
  return JSON.parse(readFileSync(SPRINT_FILE, "utf-8")) as SprintState;
}

function saveSprint(state: SprintState): void {
  writeFileSync(SPRINT_FILE, JSON.stringify(state, null, 2) + "\n");
}

// ---------------------------------------------------------------------------
// Sprint cadence — mirrored from the registered owners, not invented here.
// ---------------------------------------------------------------------------
//
// The activity of the sprint board is NOT `status`. `status` is a display label:
// `.claude/hooks/session-orient.sh` prints it as `"{active_sprint} ({status})"` and
// `scripts/tests/test_sprint_state_invariants.py` drives that resolver with
// `status: "in-progress"` and `status: "open"` on records it expects to read as ACTIVE.
// Activity is decided by `cadence`/`lifecycle` plus `active_sprint`.
//
// An earlier revision of this guard used `status === "active"` as the sole authority. That
// was wrong in BOTH directions, which is why it is replaced rather than widened: it refused
// `{cadence: "active", active_sprint: 31, status: "in-progress"}` — a genuinely running
// sprint — and it permitted `{cadence: "dormant", status: "active"}`, a self-contradictory
// record the registered resolver reports as `unresolved`.
//
// These three constants are the vocabulary of `declares_dormancy()` in
// `scripts/check-truth-spine.py`, duplicated here only because that owner is Python and this
// is TypeScript. `sprint-mutation-guard.test.ts` parses the Python source and asserts the two
// sets are identical, so drift is a test failure rather than a comment nobody re-reads.
export const DORMANT_CADENCES: ReadonlySet<string> = new Set([
  "dormant",
  "inactive",
  "none",
  "paused",
]);
export const ACTIVE_CADENCES: ReadonlySet<string> = new Set(["active", "running"]);
export const DORMANCY_KEYS = ["cadence", "lifecycle"] as const;

export type SprintCadence =
  | { kind: "active"; sprint: number | string }
  | { kind: "dormant"; reason: string }
  | { kind: "unresolved"; reason: string };

/**
 * Resolve the board's cadence exactly as `.claude/hooks/session-orient.sh` does.
 *
 * Three-valued on purpose. `unresolved` is not a synonym for `dormant`: silence and
 * self-contradiction are states in which the owner has declared nothing, and collapsing them
 * into either answer states a fact the record does not carry. Both are un-mutable here, but
 * they are un-mutable for different reasons and the refusal says which.
 */
export function resolveSprintCadence(state: unknown): SprintCadence {
  if (typeof state !== "object" || state === null || Array.isArray(state)) {
    return { kind: "unresolved", reason: "the sprint record is not a JSON object" };
  }
  const rec = state as Record<string, unknown>;
  const hasActiveKey = Object.prototype.hasOwnProperty.call(rec, "active_sprint");
  const active = rec["active_sprint"];

  if (Object.prototype.hasOwnProperty.call(rec, "cadence")) {
    const cadence = String(rec["cadence"]).trim().toLowerCase();
    if (DORMANT_CADENCES.has(cadence)) {
      if (hasActiveKey && active !== null && active !== undefined) {
        return {
          kind: "unresolved",
          reason: `the record contradicts itself: cadence="${cadence}" but active_sprint=${JSON.stringify(active)}`,
        };
      }
      return { kind: "dormant", reason: `cadence is "${cadence}"` };
    }
    if (ACTIVE_CADENCES.has(cadence)) {
      if (!hasActiveKey || active === null || active === undefined) {
        return {
          kind: "unresolved",
          reason: `the record contradicts itself: cadence="${cadence}" but no active_sprint`,
        };
      }
      return { kind: "active", sprint: active as number | string };
    }
    return { kind: "unresolved", reason: `unrecognised cadence "${cadence}"` };
  }

  // No cadence key. `active_sprint: null` is the other accepted spelling of dormancy, but the
  // key must be PRESENT — an absent key is silence, and reading silence as dormancy is the
  // fail-open the registered resolver exists to avoid.
  if (!hasActiveKey) {
    return { kind: "unresolved", reason: "the record declares neither cadence nor active_sprint" };
  }
  if (active === null) return { kind: "dormant", reason: "active_sprint is null" };
  return { kind: "active", sprint: active as number | string };
}

/**
 * Does ANY supported cadence key declare dormancy?
 *
 * Deliberately **not** a faithful port of `declares_dormancy()` in
 * `scripts/check-truth-spine.py`. That function returns on the first key it recognises, so
 * `{cadence: "active", lifecycle: "inactive"}` short-circuits on `cadence` and never inspects
 * `lifecycle` — which let a record explicitly declaring dormancy on its second key be treated
 * as mutable (review, #2657). Every key is scanned here, and any dormancy declaration wins.
 *
 * The divergence is only ever in the refusing direction, and the polarity matters: this is a
 * write gate, not the dormancy oracle. `declares_dormancy` answers "has the owner declared
 * nothing is running", where an early return is harmless; this answers "is it safe to write",
 * where it is not.
 */
function anyKeyDeclaresDormancy(rec: Record<string, unknown>): boolean {
  for (const key of DORMANCY_KEYS) {
    if (!Object.prototype.hasOwnProperty.call(rec, key)) continue;
    if (DORMANT_CADENCES.has(String(rec[key]).trim().toLowerCase())) return true;
  }
  return Object.keys(rec).some((k) => k.startsWith("active_") && rec[k] === null);
}

/**
 * Is `active_sprint` a value that can actually name a sprint?
 *
 * `resolveSprintCadence` only asks whether the value is non-null, because that is what
 * `session-orient.sh` asks. But the state file is parsed with an unchecked type assertion, so
 * `active_sprint: false`, `{}`, `[]` or `""` all reached the resolver as "not null" and were
 * read as an active board (review, #2657). A malformed record must fail closed, not open.
 *
 * Checked here rather than in the resolver so the resolver stays a faithful mirror of the
 * hook. The hook shares this leniency; that is an upstream observation, not something this
 * guard can fix on its behalf.
 */
function isUsableSprintIdentifier(value: unknown): boolean {
  if (typeof value === "number") return Number.isFinite(value);
  if (typeof value === "string") return value.trim() !== "";
  return false;
}

export interface SprintMutationRefusal {
  ok: false;
  message: string;
}

/**
 * May the sprint board be mutated right now?
 *
 * Fails CLOSED on everything that is not a positively resolved ACTIVE cadence — dormant,
 * unresolved, silent, contradictory, malformed. Note the polarity: this is a positive
 * activity test, NOT `!declaresDormancy(...)`. `declares_dormancy` is deliberately fail-open
 * (silence is not dormancy), so negating it would make silence *mutable*, which is precisely
 * the pre-#2413 behaviour this guard removes.
 */
export function assertSprintMutable(state: unknown): { ok: true } | SprintMutationRefusal {
  const resolved = resolveSprintCadence(state);
  const rec =
    typeof state === "object" && state !== null && !Array.isArray(state)
      ? (state as Record<string, unknown>)
      : {};

  let because: string | null = null;
  if (resolved.kind !== "active") {
    because = resolved.reason;
  } else if (anyKeyDeclaresDormancy(rec)) {
    because = "another cadence key declares dormancy, contradicting the active one";
  } else if (!isUsableSprintIdentifier(resolved.sprint)) {
    because = `active_sprint is ${JSON.stringify(resolved.sprint)}, which cannot name a sprint`;
  }
  if (because === null) return { ok: true };

  // Recovery guidance is generated from the record in hand, never copied from today's owner
  // state (review, #2657). Once the lineage dispute is settled and `next_sprint_number` is
  // set, repeating the icn#2637 paragraph would be false guidance.
  const successor = resolveSuccessorSprint(state);
  const numbering =
    successor.kind === "explicit"
      ? ""
      : "\n\nNote that the successor sprint number is also undetermined " +
        `(${successor.reason}); that decision is tracked at icn#2637 and belongs to a human.`;

  return {
    ok: false,
    message:
      `Refusing to mutate the sprint board: no sprint is active (${resolved.kind} — ${because}). ` +
      "ops/state/sprint/current.json is the sprint_state truth owner; activity is decided by " +
      "cadence/active_sprint, not by the status label." +
      numbering +
      "\n\nCurrent work is a live query, not a board row: " +
      "gh issue list --repo InterCooperative-Network/icn --state open",
  };
}

/** Uniform MCP error result for a refused mutation. */
function refuse(refusal: SprintMutationRefusal) {
  return {
    content: [{ type: "text" as const, text: refusal.message }],
    isError: true as const,
  };
}

// ---------------------------------------------------------------------------
// Successor identity, and the v1 write model
// ---------------------------------------------------------------------------

/**
 * The exact set of top-level keys `saveSprint` can write, because `computeNextSprint` builds
 * a `SprintState` literal with these and no others.
 *
 * Any key present in a loaded record and absent from this set is a field the v1 write model
 * would silently DROP. That is checked structurally rather than by listing v2's fields, so a
 * future schema is protected by the same guard without anyone remembering to update it.
 */
const V1_WRITABLE_KEYS: ReadonlySet<string> = new Set([
  "sprint",
  "name",
  "started",
  "goals",
  "tasks",
  "epics",
]);

/** Top-level keys a `saveSprint(computeNextSprint(...))` round-trip would destroy. */
export function fieldsLostByV1Write(state: unknown): string[] {
  if (typeof state !== "object" || state === null || Array.isArray(state)) return [];
  return Object.keys(state as Record<string, unknown>)
    .filter((k) => !V1_WRITABLE_KEYS.has(k))
    .sort();
}

export type SuccessorResolution =
  | { kind: "explicit"; sprint: number | string }
  | { kind: "undetermined"; reason: string };

/**
 * The successor sprint number, read — never derived.
 *
 * `current.sprint + 1` used to supply this. On the live record that is `26 + 1 = 27`, and 27
 * is exactly the number `next_sprint_number_note` exists to forbid: the board never advanced
 * past 26 while the narrated cadence already spent 27 and 28, so either choice fabricates
 * lineage the repository cannot support (#2637). Arithmetic is not a source of identity.
 */
export function resolveSuccessorSprint(state: unknown): SuccessorResolution {
  const rec =
    typeof state === "object" && state !== null && !Array.isArray(state)
      ? (state as Record<string, unknown>)
      : {};
  const next = rec["next_sprint_number"];
  if (typeof next === "number" || (typeof next === "string" && next.trim() !== "")) {
    return { kind: "explicit", sprint: next };
  }
  return {
    kind: "undetermined",
    reason:
      next === null
        ? "the record sets next_sprint_number: null, which is a deliberate declaration that the successor is UNDETERMINED"
        : "the record carries no next_sprint_number",
  };
}

/**
 * Compute the next sprint from the current one: carry over every task that isn't done
 * (clearing its assignee), apply the RESOLVED successor number, and apply the new
 * name/goals. Pure — the caller supplies the start date and handles archiving/persistence —
 * so the carry-over invariant is unit-testable.
 *
 * `nextSprint` is a PARAMETER, not `current.sprint + 1`. It used to be the latter, which made
 * this function a source of sprint identity by arithmetic; on the live record that yields 27,
 * the one number `next_sprint_number_note` exists to forbid (#2637). Taking it as an argument
 * means a fabricated successor cannot originate here — the caller must have read one.
 */
export function computeNextSprint(
  current: SprintState,
  nextSprint: number | string,
  nextName: string,
  nextGoals: string[],
  startedDate: string
): SprintState {
  const carriedOver = current.tasks.filter((t) => t.status !== "done");
  // Copy goals/epics so the returned state shares no mutable references with
  // the input — mutating the result must never leak back into `current`.
  return {
    sprint: nextSprint as number,
    name: nextName,
    started: startedDate,
    goals: [...nextGoals],
    tasks: carriedOver.map((t) => ({ ...t, assignee: null })),
    epics: { ...current.epics },
  };
}

export function registerTaskTools(
  server: McpServer,
  _db: Database.Database
): void {
  server.tool(
    "get_tasks",
    "List all sprint tasks with their current status, assignees, and blockers.",
    {
      status: z
        .enum(["pending", "in-progress", "in-review", "done", "blocked", "all"])
        .optional()
        .default("all")
        .describe("Filter by status"),
    },
    async ({ status }) => {
      const sprint = loadSprint();
      const tasks =
        status === "all"
          ? sprint.tasks
          : sprint.tasks.filter((t) => t.status === status);
      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(
              { sprint: sprint.sprint, name: sprint.name, tasks },
              null,
              2
            ),
          },
        ],
      };
    }
  );

  server.tool(
    "claim_task",
    "Atomically assign a task to this session's agent. Fails if already assigned.",
    {
      task_id: z.string().describe("Task ID from get_tasks"),
      session_id: z.string().describe("Your session ID from register_session"),
      agent_name: z.string().describe("Human-readable name for this agent"),
    },
    async ({ task_id, agent_name }) => {
      const sprint = loadSprint();
      const mutable = assertSprintMutable(sprint);
      if (!mutable.ok) return refuse(mutable);
      const task = sprint.tasks.find((t) => t.id === task_id);
      if (!task) {
        return {
          content: [{ type: "text", text: `Error: task ${task_id} not found` }],
          isError: true,
        };
      }
      if (task.assignee && task.assignee !== agent_name) {
        return {
          content: [
            {
              type: "text",
              text: `Error: task already assigned to ${task.assignee}`,
            },
          ],
          isError: true,
        };
      }
      task.assignee = agent_name;
      task.status = "in-progress";
      saveSprint(sprint);
      return {
        content: [{ type: "text", text: `Task ${task_id} claimed by ${agent_name}` }],
      };
    }
  );

  server.tool(
    "update_task",
    "Update task status, notes, or PR number.",
    {
      task_id: z.string(),
      status: z
        .enum(["pending", "in-progress", "in-review", "done", "blocked"])
        .optional(),
      pr: z.number().optional().describe("GitHub PR number"),
      assignee: z.string().nullable().optional(),
    },
    async ({ task_id, status, pr, assignee }) => {
      const sprint = loadSprint();
      const mutable = assertSprintMutable(sprint);
      if (!mutable.ok) return refuse(mutable);
      const task = sprint.tasks.find((t) => t.id === task_id);
      if (!task) {
        return {
          content: [{ type: "text", text: `Error: task ${task_id} not found` }],
          isError: true,
        };
      }
      if (status !== undefined) task.status = status;
      if (pr !== undefined) task.pr = pr;
      if (assignee !== undefined) task.assignee = assignee;
      saveSprint(sprint);
      return {
        content: [{ type: "text", text: JSON.stringify(task, null, 2) }],
      };
    }
  );

  server.tool(
    "delete_task",
    "Remove a task from the current sprint.",
    {
      task_id: z.string().describe("Task ID to remove"),
    },
    async ({ task_id }) => {
      const sprint = loadSprint();
      const mutable = assertSprintMutable(sprint);
      if (!mutable.ok) return refuse(mutable);
      const idx = sprint.tasks.findIndex((t) => t.id === task_id);
      if (idx === -1) {
        return {
          content: [{ type: "text", text: `Error: task ${task_id} not found` }],
          isError: true,
        };
      }
      const removed = sprint.tasks.splice(idx, 1)[0];
      saveSprint(sprint);
      return {
        content: [{ type: "text", text: `Removed: ${removed.title}` }],
      };
    }
  );

  server.tool(
    "close_sprint",
    "Archive current sprint to history/ and start a new sprint, carrying over unfinished tasks.",
    {
      next_name: z.string().describe("Name for the next sprint"),
      next_goals: z
        .array(z.string())
        .optional()
        .describe("Goals for the next sprint"),
    },
    async ({ next_name, next_goals }) => {
      const sprint = loadSprint();
      const mutable = assertSprintMutable(sprint);
      if (!mutable.ok) return refuse(mutable);

      // GATE 1 — identity. Ordered FIRST so each gate is independently observable through the
      // tool: a record with no successor refuses here, and one WITH a successor gets past this
      // point and refuses on shape below. If shape came first it would mask this gate entirely,
      // because any record active enough to reach close_sprint necessarily carries active_sprint,
      // which the v1 write model cannot emit. The successor number is READ, never derived.
      const successor = resolveSuccessorSprint(sprint);
      if (successor.kind !== "explicit") {
        return refuse({
          ok: false,
          message:
            "Refusing to close the sprint: the successor sprint number is UNDETERMINED " +
            `(${successor.reason}).\n\n` +
            "This tool will not infer one. Two numbering planes disagree \u2014 the board never " +
            "advanced past 26, while the narrated cadence already spent 27 and 28 \u2014 so both " +
            "27 and 29 are lineage claims the repository cannot support. The decision is " +
            "tracked at icn#2637; set next_sprint_number in the record once it is made.",
        });
      }

      // GATE 2 — structural. `saveSprint` writes only the v1 key set, so closing a record
      // that carries anything else destroys those fields. Checked by comparing the loaded
      // record's keys against what the write model can emit, so a v3 record is protected by
      // the same code, and it does NOT depend on the board being dormant today: a future v2
      // record marked active reaches here and must still be refused.
      const lost = fieldsLostByV1Write(sprint);
      if (lost.length > 0) {
        return refuse({
          ok: false,
          message:
            "Refusing to close the sprint: this MCP write path is v1-shaped and would DELETE " +
            `${lost.length} field(s) from the record \u2014 ${lost.join(", ")}.\n\n` +
            "Closing an `icn-sprint-state/v2` record needs v2 transition semantics that do not " +
            "exist yet: what the successor's cadence, active_sprint and board_lineage should be " +
            "is a governance decision, not a serialization detail. Until that contract exists " +
            "this tool refuses rather than round-tripping v2 state through the v1 model.",
        });
      }

      const historyDir = join(dirname(SPRINT_FILE), "history");
      if (!existsSync(historyDir)) mkdirSync(historyDir, { recursive: true });

      const historyPath = join(historyDir, `sprint-${sprint.sprint}-${sprint.started}.json`);
      writeFileSync(historyPath, JSON.stringify(sprint, null, 2) + "\n");

      const startedDate = new Date().toISOString().split("T")[0]!;
      const next = computeNextSprint(
        sprint,
        successor.sprint,
        next_name,
        next_goals ?? [],
        startedDate
      );
      saveSprint(next);

      const doneCount = sprint.tasks.length - next.tasks.length;
      return {
        content: [
          {
            type: "text",
            text: `Sprint ${sprint.sprint} archived to ${historyPath}.\n${doneCount} tasks done, ${next.tasks.length} carried to Sprint ${next.sprint}.`,
          },
        ],
      };
    }
  );

  server.tool(
    "create_task",
    "Add a new task to the current sprint.",
    {
      id: z.string().describe("Unique task ID (use GitHub issue number or short kebab-case slug)"),
      title: z.string(),
      epic: z.string().nullable().optional(),
    },
    async ({ id, title, epic }) => {
      const sprint = loadSprint();
      const mutable = assertSprintMutable(sprint);
      if (!mutable.ok) return refuse(mutable);
      if (sprint.tasks.find((t) => t.id === id)) {
        return {
          content: [{ type: "text", text: `Error: task ${id} already exists` }],
          isError: true,
        };
      }
      const task: Task = {
        id,
        title,
        status: "pending",
        pr: null,
        assignee: null,
        epic: epic ?? null,
      };
      sprint.tasks.push(task);
      saveSprint(sprint);
      return {
        content: [{ type: "text", text: JSON.stringify(task, null, 2) }],
      };
    }
  );
}
