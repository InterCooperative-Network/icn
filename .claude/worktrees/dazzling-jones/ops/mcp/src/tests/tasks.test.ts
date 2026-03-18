import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { readFileSync, writeFileSync, copyFileSync, mkdirSync, rmSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

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
