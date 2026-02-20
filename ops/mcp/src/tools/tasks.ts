import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type Database from "better-sqlite3";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { dirname, join } from "path";

const ICN_ROOT = process.env["ICN_ROOT"] ?? "/home/ubuntu/projects";
const SPRINT_FILE = join(ICN_ROOT, "icn-ops/state/sprint/current.json");

interface Task {
  id: string;
  title: string;
  status: string;
  pr: number | null;
  assignee: string | null;
  epic: string | null;
}

interface SprintState {
  sprint: number;
  name: string;
  started: string;
  goals: string[];
  tasks: Task[];
  epics: Record<string, string>;
}

function loadSprint(): SprintState {
  return JSON.parse(readFileSync(SPRINT_FILE, "utf-8")) as SprintState;
}

function saveSprint(state: SprintState): void {
  writeFileSync(SPRINT_FILE, JSON.stringify(state, null, 2) + "\n");
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
      const historyDir = join(dirname(SPRINT_FILE), "history");
      if (!existsSync(historyDir)) mkdirSync(historyDir, { recursive: true });

      const historyPath = join(historyDir, `sprint-${sprint.sprint}-${sprint.started}.json`);
      writeFileSync(historyPath, JSON.stringify(sprint, null, 2) + "\n");

      const carriedOver = sprint.tasks.filter((t) => t.status !== "done");
      const next: SprintState = {
        sprint: sprint.sprint + 1,
        name: next_name,
        started: new Date().toISOString().split("T")[0]!,
        goals: next_goals ?? [],
        tasks: carriedOver.map((t) => ({ ...t, assignee: null })),
        epics: sprint.epics,
      };
      saveSprint(next);

      const doneCount = sprint.tasks.length - carriedOver.length;
      return {
        content: [
          {
            type: "text",
            text: `Sprint ${sprint.sprint} archived to ${historyPath}.\n${doneCount} tasks done, ${carriedOver.length} carried to Sprint ${next.sprint}.`,
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
