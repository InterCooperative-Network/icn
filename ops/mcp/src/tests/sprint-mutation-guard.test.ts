// The sprint board must refuse mutation while no sprint is active (icn#2419).
//
// WHY THIS FILE EXISTS, AND WHY IT CALLS THE TOOLS
//   `ops/state/sprint/current.json` is the registered `sprint_state` truth owner and an
//   honestly CLOSED record. Every mutating task tool loaded it, edited it and saved it with no
//   status check, so any agent calling `create_task` would resurrect "current work" inside a
//   closed sprint.
//
//   `tasks.test.ts` already covers this file's shape — but it does it by reading and writing a
//   fixture with `fs`, and by unit-testing the pure `computeNextSprint`. Neither touches a tool
//   handler, which is precisely the layer the guard lives in. That is the same gap that shipped
//   `claim_files`/`session_info` broken with 163 tests green (see `session-tools.test.ts`). So
//   the refusals below are driven THROUGH THE PROTOCOL, against a redirected state file, and
//   the file is re-read afterwards to prove nothing was written.

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, readdirSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { assertSprintMutable } from "../tools/tasks.js";

// ---------------------------------------------------------------------------
// The pure guard. No files, no protocol — just the decision.
// ---------------------------------------------------------------------------

describe("assertSprintMutable", () => {
  it("permits an active sprint", () => {
    expect(assertSprintMutable({ sprint: 27, status: "active" }).ok).toBe(true);
  });

  it("refuses a closed sprint", () => {
    const r = assertSprintMutable({ sprint: 26, status: "closed" });
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.message).toContain("sprint 26");
    expect(r.message).toContain('status "closed"');
  });

  // The important half: absent must not mean mutable. Pre-v2 records on disk carry no
  // `status`, and defaulting those to writable would restore the exact behaviour this
  // guard removes.
  it("refuses a record with no status field at all", () => {
    const r = assertSprintMutable({ sprint: 26 });
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.message).toContain("no status field");
  });

  it("refuses any status that is not exactly active", () => {
    for (const status of ["Active", "ACTIVE", "dormant", "done", "closed", "", "paused"]) {
      expect(assertSprintMutable({ sprint: 26, status }).ok).toBe(false);
    }
  });

  // Opening the next sprint is blocked on a human decision (#2637). The refusal must say so
  // rather than inviting the caller to pick a number.
  it("routes the caller to the numbering decision, not to a guess", () => {
    const r = assertSprintMutable({ sprint: 26, status: "closed" });
    if (r.ok) throw new Error("expected a refusal");
    expect(r.message).toContain("icn#2637");
    expect(r.message).toMatch(/undetermined/i);
    expect(r.message).toContain("gh issue list");
  });
});

// ---------------------------------------------------------------------------
// The tools, over the protocol, against a real (redirected) state file.
// ---------------------------------------------------------------------------

const MUTATORS: Array<[string, Record<string, unknown>]> = [
  ["create_task", { id: "resurrected", title: "should never be written" }],
  ["claim_task", { task_id: "t1", session_id: "s1", agent_name: "tester" }],
  ["update_task", { task_id: "t1", status: "done" }],
  ["delete_task", { task_id: "t1" }],
  ["close_sprint", { next_name: "should never be opened" }],
];

/** A closed v2-shaped record, minus the prose. Deliberately has no `started`/`epics`. */
const CLOSED_V2 = {
  schema: "icn-sprint-state/v2",
  cadence: "dormant",
  sprint: 26,
  status: "closed",
  tasks: [{ id: "t1", title: "archived", status: "pending", pr: null, assignee: null, epic: null }],
  next_sprint_number: null,
};

let root: string;
let sprintFile: string;
let client: Client;
let priorIcnRoot: string | undefined;

beforeEach(async () => {
  root = mkdtempSync(join(tmpdir(), "icn-sprint-guard-"));
  mkdirSync(join(root, "ops", "state", "sprint"), { recursive: true });
  sprintFile = join(root, "ops", "state", "sprint", "current.json");
  writeFileSync(sprintFile, JSON.stringify(CLOSED_V2, null, 2) + "\n");

  // `tasks.ts` resolves SPRINT_FILE once at module load, so the env has to be set before the
  // module is imported and the module cache has to be dropped between suites.
  priorIcnRoot = process.env["ICN_ROOT"];
  process.env["ICN_ROOT"] = root;
  vi.resetModules();
  const tasks = await import("../tools/tasks.js");

  const server = new McpServer({ name: "icn-ops-test", version: "0.0.0" });
  (tasks as { registerTaskTools: (s: McpServer, d: unknown) => void }).registerTaskTools(
    server,
    null
  );
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  client = new Client({ name: "test-client", version: "0.0.0" });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
});

afterEach(() => {
  if (priorIcnRoot === undefined) delete process.env["ICN_ROOT"];
  else process.env["ICN_ROOT"] = priorIcnRoot;
  rmSync(root, { recursive: true, force: true });
});

async function callRaw(name: string, args: Record<string, unknown>) {
  return (await client.callTool({ name, arguments: args })) as {
    isError?: boolean;
    content: Array<{ type: string; text: string }>;
  };
}

describe("mutating task tools against a closed sprint", () => {
  it.each(MUTATORS)("%s is refused", async (name, args) => {
    const res = await callRaw(name, args);
    expect(res.isError).toBe(true);
    expect(res.content[0]?.text).toContain("Refusing to mutate the sprint board");
  });

  it("leaves the state file byte-identical after every refused call", async () => {
    const before = readFileSync(sprintFile, "utf-8");
    for (const [name, args] of MUTATORS) await callRaw(name, args);
    expect(readFileSync(sprintFile, "utf-8")).toBe(before);
  });

  // close_sprint is the sharpest case. Unguarded it would archive to
  // `sprint-26-undefined.json` (v2 has no `started`), then write `computeNextSprint`'s
  // `sprint: 26 + 1` over the whole record — silently answering #2637 by arithmetic and
  // destroying `next_sprint_number`, `board_lineage` and the notes with it.
  it("close_sprint neither writes an archive nor invents sprint 27", async () => {
    await callRaw("close_sprint", { next_name: "fabricated" });
    const historyDir = join(root, "ops", "state", "sprint", "history");
    let archived: string[] = [];
    try {
      archived = readdirSync(historyDir);
    } catch {
      archived = [];
    }
    expect(archived).toEqual([]);
    const after = JSON.parse(readFileSync(sprintFile, "utf-8"));
    expect(after.sprint).toBe(26);
    expect(after.status).toBe("closed");
    expect(after.next_sprint_number).toBeNull();
    expect(after.schema).toBe("icn-sprint-state/v2");
  });
});

describe("read tools are unaffected", () => {
  it("get_tasks still works on a closed sprint", async () => {
    const res = await callRaw("get_tasks", {});
    expect(res.isError).toBeFalsy();
    expect(res.content[0]?.text).toContain("t1");
  });
});

describe("an active sprint is still mutable", () => {
  it("create_task succeeds once the record says active", async () => {
    writeFileSync(
      sprintFile,
      JSON.stringify({ ...CLOSED_V2, sprint: 27, status: "active" }, null, 2) + "\n"
    );
    const res = await callRaw("create_task", { id: "real", title: "a real task" });
    expect(res.isError).toBeFalsy();
    const after = JSON.parse(readFileSync(sprintFile, "utf-8"));
    expect(after.tasks.map((t: { id: string }) => t.id)).toContain("real");
  });
});
