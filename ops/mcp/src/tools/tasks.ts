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
   * Cadence status. Optional because this interface predates `icn-sprint-state/v2`
   * (#2636) and pre-v2 archives on disk do not carry it. Absent is NOT "active":
   * see `assertSprintMutable`.
   */
  status?: string;
  /**
   * v2 only, and deliberately `null` today: two numbering planes disagree and a human
   * must reconcile them (#2637). Nothing here may infer it.
   */
  next_sprint_number?: number | null;
}

/**
 * The only cadence status under which the sprint board may be mutated.
 *
 * `ops/state/sprint/current.json` is the registered `sprint_state` truth owner and is an
 * honestly CLOSED record — Sprint 26 was closed retroactively by the 2026-07-13 truth refresh
 * (#2413) and re-declared dormant by #2636. Every mutating tool below used to load it, edit it
 * and save it with no status check at all, so any agent calling `create_task` would silently
 * resurrect "current work" inside a closed sprint: exactly the stale-as-current failure the
 * refresh removed.
 */
const MUTABLE_STATUS = "active";

export interface SprintMutationRefusal {
  ok: false;
  message: string;
}

/**
 * Fail CLOSED on anything that is not explicitly `active`.
 *
 * A MISSING status is refused too, and that is the important half. Treating absent as
 * mutable would restore the exact pre-#2413 behaviour for any record that predates v2, which
 * is the failure mode this guard exists to remove — and "the field I use to decide is not
 * there" is never evidence that mutation is safe.
 *
 * Pure, and exported, so the refusal can be tested without redirecting the real state file.
 */
export function assertSprintMutable(
  sprint: Pick<SprintState, "sprint" | "status">
): { ok: true } | SprintMutationRefusal {
  if (sprint.status === MUTABLE_STATUS) return { ok: true };
  const observed = sprint.status === undefined ? "no status field" : `status "${sprint.status}"`;
  return {
    ok: false,
    message:
      `Refusing to mutate the sprint board: sprint ${sprint.sprint} has ${observed}, ` +
      `not "${MUTABLE_STATUS}". ops/state/sprint/current.json is the sprint_state truth owner ` +
      "and is dormant by design, not stale — see its `notes` field.\n\n" +
      "Opening the next sprint is NOT available here: the next sprint number is undetermined " +
      "because two numbering planes disagree, and picking one would fabricate board lineage. " +
      "That decision is tracked at icn#2637 and must be made by a human first.\n\n" +
      "Current work is a live query, not a board row: " +
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

function loadSprint(): SprintState {
  return JSON.parse(readFileSync(SPRINT_FILE, "utf-8")) as SprintState;
}

function saveSprint(state: SprintState): void {
  writeFileSync(SPRINT_FILE, JSON.stringify(state, null, 2) + "\n");
}

/**
 * Compute the next sprint from the current one: carry over every task that
 * isn't done (clearing its assignee), increment the sprint number, and apply
 * the new name/goals. Pure — the caller supplies the start date and handles
 * archiving/persistence — so the carry-over invariant is unit-testable.
 */
export function computeNextSprint(
  current: SprintState,
  nextName: string,
  nextGoals: string[],
  startedDate: string
): SprintState {
  const carriedOver = current.tasks.filter((t) => t.status !== "done");
  // Copy goals/epics so the returned state shares no mutable references with
  // the input — mutating the result must never leak back into `current`.
  return {
    sprint: current.sprint + 1,
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
      const historyDir = join(dirname(SPRINT_FILE), "history");
      if (!existsSync(historyDir)) mkdirSync(historyDir, { recursive: true });

      const historyPath = join(historyDir, `sprint-${sprint.sprint}-${sprint.started}.json`);
      writeFileSync(historyPath, JSON.stringify(sprint, null, 2) + "\n");

      const startedDate = new Date().toISOString().split("T")[0]!;
      const next = computeNextSprint(
        sprint,
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
