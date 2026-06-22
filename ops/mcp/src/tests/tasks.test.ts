import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { readFileSync, writeFileSync, copyFileSync, mkdirSync, rmSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { computeNextSprint, type SprintState } from "../tools/tasks.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(__dirname, "fixtures/sprint.json");
const TEST_DIR = join(__dirname, "tmp");
const TEST_SPRINT = join(TEST_DIR, "current.json");

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
    const updated = reread.tasks.find(
      (t: { id: string }) => t.id === "test-task-1"
    );
    expect(updated.status).toBe("in-progress");
    expect(updated.assignee).toBe("test-agent");
  });
});

describe("computeNextSprint", () => {
  const base: SprintState = {
    sprint: 26,
    name: "Sprint 26",
    started: "2026-06-01",
    goals: ["ship demo"],
    epics: { devops: "DevOps" },
    tasks: [
      { id: "t1", title: "done", status: "done", pr: 1, assignee: "matt", epic: "devops" },
      { id: "t2", title: "wip", status: "in-progress", pr: null, assignee: "matt", epic: "devops" },
      { id: "t3", title: "todo", status: "pending", pr: null, assignee: null, epic: null },
    ],
  };

  it("increments the sprint number", () => {
    expect(computeNextSprint(base, "Sprint 27", [], "2026-07-01").sprint).toBe(27);
  });

  it("carries over only non-done tasks and clears their assignee", () => {
    const next = computeNextSprint(base, "Sprint 27", [], "2026-07-01");
    expect(next.tasks.map((t) => t.id)).toEqual(["t2", "t3"]);
    expect(next.tasks.every((t) => t.assignee === null)).toBe(true);
  });

  it("applies the new name, goals, and start date; preserves epics", () => {
    const next = computeNextSprint(base, "Sprint 27", ["land pilot"], "2026-07-01");
    expect(next.name).toBe("Sprint 27");
    expect(next.goals).toEqual(["land pilot"]);
    expect(next.started).toBe("2026-07-01");
    expect(next.epics).toEqual({ devops: "DevOps" });
  });

  it("does not mutate the input sprint", () => {
    const snapshot = JSON.parse(JSON.stringify(base));
    computeNextSprint(base, "Sprint 27", [], "2026-07-01");
    expect(base).toEqual(snapshot);
  });

  it("returns state that shares no mutable references with the input", () => {
    const goals = ["ship"];
    const next = computeNextSprint(base, "Sprint 27", goals, "2026-07-01");
    expect(next.epics).not.toBe(base.epics);
    expect(next.goals).not.toBe(goals);
    // Mutating the result must not leak back into the inputs via aliasing.
    next.epics.devops = "MUTATED";
    next.goals.push("leaked");
    expect(base.epics).toEqual({ devops: "DevOps" });
    expect(goals).toEqual(["ship"]);
  });
});
