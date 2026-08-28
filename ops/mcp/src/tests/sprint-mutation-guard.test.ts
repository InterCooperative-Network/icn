// The sprint board must refuse mutation while no sprint is active (icn#2419).
//
// WHY THIS FILE EXISTS, AND WHY IT CALLS THE TOOLS
//   `ops/state/sprint/current.json` is the registered `sprint_state` truth owner. Every
//   mutating task tool loaded it, edited it and saved it with no cadence check, so any agent
//   calling `create_task` would resurrect "current work" inside a dormant board.
//
//   `tasks.test.ts` covers this file's shape — but via `fs` on a fixture and the pure
//   `computeNextSprint`. Neither touches a tool handler, which is the layer the guards live
//   in. That is the same gap that shipped `claim_files`/`session_info` broken with 163 tests
//   green (see `session-tools.test.ts`). So every refusal below is driven THROUGH THE
//   PROTOCOL against a redirected state file, and the file is re-read afterwards.
//
// NOTE ON IMPORTS (review finding, PR #2657)
//   Nothing is imported from `../tools/tasks.js` at module scope. `tasks.ts` resolves
//   SPRINT_FILE once at load, so a module-scope import would bind it to the real repo path
//   before `ICN_ROOT` is set. Every symbol — pure predicates included — is pulled off the
//   same dynamically imported instance the tools are registered from, so there is exactly one
//   module instance per test and no split-brain between the two.

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import {
  mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, readdirSync, copyFileSync,
} from "fs";
import { execFileSync } from "child_process";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

type TasksModule = typeof import("../tools/tasks.js");

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..");

/** A dormant v2-shaped record, minus the prose. Deliberately has no `started`/`epics`. */
const DORMANT_V2 = {
  schema: "icn-sprint-state/v2",
  cadence: "dormant",
  active_sprint: null,
  sprint: 26,
  status: "closed",
  tasks: [{ id: "t1", title: "archived", status: "pending", pr: null, assignee: null, epic: null }],
  next_sprint_number: null,
};

/**
 * A genuinely running board. This is the reviewer's exact counter-example, and it is the
 * positive control the whole suite rests on: the previous `status === "active"` predicate
 * REFUSED this record, and the registered resolver reports it as active.
 */
const ACTIVE_V1 = {
  cadence: "active",
  active_sprint: 31,
  sprint: 31,
  status: "in-progress",
  name: "Sprint 31",
  started: "2026-08-01",
  goals: [],
  epics: {},
  tasks: [{ id: "t1", title: "live", status: "pending", pr: null, assignee: null, epic: null }],
};

const MUTATORS: Array<[string, Record<string, unknown>]> = [
  ["create_task", { id: "resurrected", title: "should never be written" }],
  ["claim_task", { task_id: "t1", session_id: "s1", agent_name: "tester" }],
  ["update_task", { task_id: "t1", status: "done" }],
  ["delete_task", { task_id: "t1" }],
  ["close_sprint", { next_name: "should never be opened" }],
];

let root: string;
let sprintFile: string;
let client: Client;
let tasks: TasksModule;
let priorIcnRoot: string | undefined;

async function boot(record: unknown) {
  writeFileSync(sprintFile, JSON.stringify(record, null, 2) + "\n");
  vi.resetModules();
  tasks = await import("../tools/tasks.js");
  const server = new McpServer({ name: "icn-ops-test", version: "0.0.0" });
  tasks.registerTaskTools(server, null as never);
  const [ct, st] = InMemoryTransport.createLinkedPair();
  client = new Client({ name: "test-client", version: "0.0.0" });
  await Promise.all([server.connect(st), client.connect(ct)]);
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "icn-sprint-guard-"));
  mkdirSync(join(root, "ops", "state", "sprint"), { recursive: true });
  sprintFile = join(root, "ops", "state", "sprint", "current.json");
  priorIcnRoot = process.env["ICN_ROOT"];
  process.env["ICN_ROOT"] = root;
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

// ---------------------------------------------------------------------------
// The vocabulary is not ours. Prove it still matches the owner.
// ---------------------------------------------------------------------------

describe("cadence vocabulary tracks scripts/check-truth-spine.py", () => {
  /** Parse a `NAME = {"a", "b"}` set literal out of the Python owner. */
  function pySet(src: string, name: string): Set<string> {
    const m = new RegExp(`^${name}\\s*=\\s*\\{([^}]*)\\}`, "m").exec(src);
    if (!m) throw new Error(`${name} not found in check-truth-spine.py`);
    return new Set([...m[1]!.matchAll(/"([^"]+)"/g)].map((x) => x[1]!));
  }

  it("has identical DORMANT and ACTIVE value sets", async () => {
    await boot(DORMANT_V2);
    const py = readFileSync(join(REPO_ROOT, "scripts", "check-truth-spine.py"), "utf-8");

    // Control first: if the parser silently returned nothing, every comparison below would
    // be vacuously true against an empty TS set.
    const pyDormant = pySet(py, "DORMANT_VALUES");
    const pyActive = pySet(py, "ACTIVE_VALUES");
    expect(pyDormant.size).toBeGreaterThan(0);
    expect(pyActive.size).toBeGreaterThan(0);

    expect([...tasks.DORMANT_CADENCES].sort()).toEqual([...pyDormant].sort());
    expect([...tasks.ACTIVE_CADENCES].sort()).toEqual([...pyActive].sort());

    const keys = /^DORMANCY_KEYS\s*=\s*\(([^)]*)\)/m.exec(py);
    expect(keys).not.toBeNull();
    const pyKeys = [...keys![1]!.matchAll(/"([^"]+)"/g)].map((x) => x[1]!);
    expect([...tasks.DORMANCY_KEYS]).toEqual(pyKeys);
  });
});

// ---------------------------------------------------------------------------
// Differential: the TS resolver must agree with the shell resolver it mirrors.
// ---------------------------------------------------------------------------
//
// The vocabulary test above catches drift in the WORD LISTS. This catches drift in the
// ALGORITHM, which is the other half of "one canonical source": `resolveSprintCadence` is a
// hand-mirror of `.claude/hooks/session-orient.sh`, and nothing but this stops the two
// diverging. Both are driven over the same records and their verdicts compared.

describe("resolveSprintCadence agrees with .claude/hooks/session-orient.sh", () => {
  const CASES: Array<[string, unknown]> = [
    ["dormant declared", { cadence: "dormant", active_sprint: null, status: "closed" }],
    ["active with an in-progress label", { cadence: "active", active_sprint: 31, status: "in-progress" }],
    ["running with an open label", { cadence: "running", active_sprint: 7, status: "open" }],
    ["absent active_sprint is silence", { sprint: 26, status: "closed", tasks: [] }],
    ["active + null active_sprint", { cadence: "active", active_sprint: null }],
    ["dormant + a sprint number", { cadence: "dormant", active_sprint: 31 }],
    ["unrecognised cadence", { cadence: "weekly", active_sprint: 31 }],
    ["active_sprint null, no cadence", { active_sprint: null, status: "closed" }],
    ["a status label alone decides nothing", { status: "active" }],
    ["case and whitespace tolerant", { cadence: "  Dormant " }],
  ];

  /** Run the real hook against `record` in a throwaway repo skeleton. */
  function hookVerdict(record: unknown): "active" | "dormant" | "unresolved" {
    const box = mkdtempSync(join(tmpdir(), "icn-hook-"));
    mkdirSync(join(box, "ops", "state", "sprint"), { recursive: true });
    mkdirSync(join(box, "ops", "state", "truth"), { recursive: true });
    mkdirSync(join(box, ".claude", "hooks"), { recursive: true });
    copyFileSync(
      join(REPO_ROOT, ".claude", "hooks", "session-orient.sh"),
      join(box, ".claude", "hooks", "session-orient.sh")
    );
    copyFileSync(
      join(REPO_ROOT, "ops", "state", "truth", "sources.json"),
      join(box, "ops", "state", "truth", "sources.json")
    );
    writeFileSync(join(box, "ops", "state", "sprint", "current.json"), JSON.stringify(record));
    const out = execFileSync("bash", [join(box, ".claude", "hooks", "session-orient.sh")], {
      env: { ...process.env, CLAUDE_PROJECT_DIR: box, ICN_ROOT: box },
      encoding: "utf-8",
    });
    rmSync(box, { recursive: true, force: true });
    const line = out.split("\n").find((l) => l.includes("sprint: ")) ?? "";
    const verdict = line.split("sprint: ")[1] ?? "";
    if (verdict.startsWith("none active")) return "dormant";
    if (verdict.startsWith("unresolved")) return "unresolved";
    return "active";
  }

  it.each(CASES)("%s", async (_name, record) => {
    await boot(DORMANT_V2);
    expect(tasks.resolveSprintCadence(record).kind).toBe(hookVerdict(record));
  });

  // CONTROL: the comparison would be vacuous if the hook answered the same thing every time.
  it("the hook itself produces all three verdicts across these cases", () => {
    const seen = new Set(CASES.map(([, r]) => hookVerdict(r)));
    expect([...seen].sort()).toEqual(["active", "dormant", "unresolved"]);
  });
});

// ---------------------------------------------------------------------------
// Cadence resolution — three-valued, mirroring .claude/hooks/session-orient.sh
// ---------------------------------------------------------------------------

describe("resolveSprintCadence", () => {
  beforeEach(async () => {
    await boot(DORMANT_V2);
  });

  it("resolves a declared active board", () => {
    expect(tasks.resolveSprintCadence({ cadence: "active", active_sprint: 31 })).toMatchObject({
      kind: "active",
      sprint: 31,
    });
    expect(tasks.resolveSprintCadence({ cadence: "running", active_sprint: 7 }).kind).toBe("active");
  });

  it("resolves a declared dormant board", () => {
    for (const c of ["dormant", "inactive", "none", "paused"]) {
      expect(tasks.resolveSprintCadence({ cadence: c }).kind).toBe("dormant");
    }
    expect(tasks.resolveSprintCadence({ active_sprint: null }).kind).toBe("dormant");
  });

  it("is case- and whitespace-tolerant, like the owner", () => {
    expect(tasks.resolveSprintCadence({ cadence: "  Dormant " }).kind).toBe("dormant");
    expect(tasks.resolveSprintCadence({ cadence: " ACTIVE ", active_sprint: 3 }).kind).toBe("active");
  });

  // The four cases the registered resolver reports as `unresolved`. None may be mutable, and
  // none may be reported as dormant — silence is not a declaration.
  it("refuses to resolve silence or self-contradiction", () => {
    const cases: Array<[string, unknown]> = [
      ["absent active_sprint key is silence, not null", { sprint: 26, status: "closed" }],
      ["active cadence + null active_sprint", { cadence: "active", active_sprint: null }],
      ["dormant cadence + a sprint number", { cadence: "dormant", active_sprint: 31 }],
      ["unrecognised cadence", { cadence: "weekly", active_sprint: 31 }],
      ["not an object", ["not", "an", "object"]],
    ];
    for (const [why, rec] of cases) {
      expect(tasks.resolveSprintCadence(rec).kind, why).toBe("unresolved");
    }
  });
});

describe("assertSprintMutable", () => {
  beforeEach(async () => {
    await boot(DORMANT_V2);
  });

  // POSITIVE CONTROLS. Without these the suite could be green because everything is refused.
  it("permits a genuinely active board", () => {
    expect(tasks.assertSprintMutable(ACTIVE_V1).ok).toBe(true);
    expect(tasks.assertSprintMutable({ cadence: "running", active_sprint: 7 }).ok).toBe(true);
    expect(tasks.assertSprintMutable({ active_sprint: 12 }).ok).toBe(true);
  });

  // The old predicate's two failures, pinned in both directions.
  it("no longer refuses an active board whose status label is not the word active", () => {
    expect(ACTIVE_V1.status).not.toBe("active");
    expect(tasks.assertSprintMutable(ACTIVE_V1).ok).toBe(true);
  });

  it("no longer permits a dormant board whose status label happens to be active", () => {
    const r = tasks.assertSprintMutable({ cadence: "dormant", active_sprint: null, status: "active" });
    expect(r.ok).toBe(false);
  });

  it("fails closed on dormant, unresolved, silent and malformed records", () => {
    for (const rec of [
      DORMANT_V2,
      {},
      { sprint: 26 },
      { cadence: "weekly", active_sprint: 31 },
      { cadence: "active", active_sprint: null },
      { status: "active" },
      null,
      ["x"],
    ]) {
      expect(tasks.assertSprintMutable(rec).ok, JSON.stringify(rec)).toBe(false);
    }
  });

  // `declares_dormancy` is fail-OPEN by design (silence is not dormancy), so a guard written
  // as `!declaresDormancy(x)` would make silence mutable. Pin the polarity.
  it("treats a lifecycle dormancy declaration as refusal even with a live active_sprint", () => {
    expect(tasks.assertSprintMutable({ lifecycle: "inactive", active_sprint: 31 }).ok).toBe(false);
  });

  // Review round 2, reproduced before fixing: `declares_dormancy` returns on the FIRST key it
  // recognises, so mirroring it let `cadence: "active"` short-circuit past `lifecycle:
  // "inactive"`. Every dormancy key is scanned now.
  it("refuses when one cadence key says active and another declares dormancy", () => {
    expect(
      tasks.assertSprintMutable({ cadence: "active", lifecycle: "inactive", active_sprint: 31 }).ok
    ).toBe(false);
    expect(
      tasks.assertSprintMutable({ lifecycle: "dormant", cadence: "running", active_sprint: 7 }).ok
    ).toBe(false);
    // Control: agreeing keys are still mutable, so the check is not refusing on key count.
    expect(
      tasks.assertSprintMutable({ cadence: "active", lifecycle: "running", active_sprint: 31 }).ok
    ).toBe(true);
  });

  // Review round 2: the state file is parsed with an unchecked assertion, so a non-null but
  // unusable `active_sprint` read as an active board.
  it("refuses an active_sprint that cannot name a sprint", () => {
    for (const bad of [false, true, {}, [], "", "   ", NaN, Infinity]) {
      expect(
        tasks.assertSprintMutable({ cadence: "active", active_sprint: bad }).ok,
        `active_sprint: ${JSON.stringify(bad)}`
      ).toBe(false);
    }
    // Controls: real identifiers stay mutable, including 0 and a string id.
    for (const good of [31, 0, "31", "2026-Q3"]) {
      expect(
        tasks.assertSprintMutable({ cadence: "active", active_sprint: good }).ok,
        `active_sprint: ${JSON.stringify(good)}`
      ).toBe(true);
    }
  });

  // Review round 2: the refusal used to hardcode today's lineage dispute, so it would keep
  // citing icn#2637 long after the number was settled.
  it("mentions the numbering decision only while the successor is undetermined", () => {
    const undetermined = tasks.assertSprintMutable(DORMANT_V2);
    if (undetermined.ok) throw new Error("expected a refusal");
    expect(undetermined.message).toContain("icn#2637");

    const settled = tasks.assertSprintMutable({ ...DORMANT_V2, next_sprint_number: 44 });
    if (settled.ok) throw new Error("expected a refusal");
    expect(settled.message).not.toContain("icn#2637");
    expect(settled.message).not.toMatch(/numbering planes/);
    // Still refuses, and still says why — only the stale recovery advice is gone.
    expect(settled.message).toContain("no sprint is active");
  });

  // Review round 3: the refusal asserted "no sprint is active" for records whose cadence the
  // resolver expressly refused to resolve — a cadence fact this surface must not emit.
  it("reports unresolved cadence as unresolved, not as dormancy", () => {
    const dormant = tasks.assertSprintMutable({ cadence: "dormant", active_sprint: null });
    if (dormant.ok) throw new Error("expected a refusal");
    expect(dormant.message).toContain("no sprint is active");

    for (const rec of [{ sprint: 26 }, { cadence: "weekly", active_sprint: 3 }, { cadence: "active", active_sprint: null }]) {
      const r = tasks.assertSprintMutable(rec);
      if (r.ok) throw new Error(`expected a refusal for ${JSON.stringify(rec)}`);
      expect(r.message, JSON.stringify(rec)).toContain("cadence cannot be resolved");
      expect(r.message, JSON.stringify(rec)).not.toContain("no sprint is active");
    }
  });

  it("routes the caller to the numbering decision, not to a guess", () => {
    const r = tasks.assertSprintMutable(DORMANT_V2);
    if (r.ok) throw new Error("expected a refusal");
    expect(r.message).toContain("icn#2637");
    expect(r.message).toContain("gh issue list");
  });
});

// ---------------------------------------------------------------------------
// The tools, over the protocol
// ---------------------------------------------------------------------------

describe("mutating task tools against a dormant board", () => {
  beforeEach(async () => {
    await boot(DORMANT_V2);
  });

  it.each(MUTATORS)("%s is refused", async (name, args) => {
    const res = await callRaw(name, args);
    expect(res.isError).toBe(true);
    expect(res.content[0]?.text).toMatch(/Refusing to (mutate the sprint board|close the sprint)/);
  });

  it("leaves the state file byte-identical after every refused call", async () => {
    const before = readFileSync(sprintFile, "utf-8");
    for (const [name, args] of MUTATORS) await callRaw(name, args);
    expect(readFileSync(sprintFile, "utf-8")).toBe(before);
  });

  it("get_tasks still works — reads are unaffected", async () => {
    const res = await callRaw("get_tasks", {});
    expect(res.isError).toBeFalsy();
    expect(res.content[0]?.text).toContain("t1");
  });
});

describe("mutating task tools against an active board", () => {
  beforeEach(async () => {
    await boot(ACTIVE_V1);
  });

  it("create_task succeeds", async () => {
    const res = await callRaw("create_task", { id: "real", title: "a real task" });
    expect(res.isError).toBeFalsy();
    const after = JSON.parse(readFileSync(sprintFile, "utf-8"));
    expect(after.tasks.map((t: { id: string }) => t.id)).toContain("real");
  });

  it("update_task and claim_task succeed", async () => {
    expect((await callRaw("claim_task", { task_id: "t1", session_id: "s", agent_name: "a" })).isError).toBeFalsy();
    expect((await callRaw("update_task", { task_id: "t1", status: "done" })).isError).toBeFalsy();
  });
});

// ---------------------------------------------------------------------------
// close_sprint — the two structural gates
// ---------------------------------------------------------------------------

describe("close_sprint cannot fabricate a successor", () => {
  // ACTIVE so the cadence guard is out of the way: this proves the successor gate stands on
  // its own rather than being masked by dormancy.
  it("refuses when next_sprint_number is absent, even on an active board", async () => {
    await boot(ACTIVE_V1);
    const res = await callRaw("close_sprint", { next_name: "fabricated" });
    expect(res.isError).toBe(true);
    expect(res.content[0]?.text).toContain("UNDETERMINED");
    expect(res.content[0]?.text).toContain("icn#2637");
    // 26+1 and 31+1 must appear nowhere: the tool must not even suggest a number.
    expect(res.content[0]?.text).not.toMatch(/\b32\b/);
  });

  it("refuses when next_sprint_number is explicitly null", async () => {
    await boot({ ...ACTIVE_V1, next_sprint_number: null });
    const res = await callRaw("close_sprint", { next_name: "fabricated" });
    expect(res.isError).toBe(true);
    expect(res.content[0]?.text).toContain("UNDETERMINED");
  });

  it("writes nothing and archives nothing when it refuses", async () => {
    await boot(ACTIVE_V1);
    const before = readFileSync(sprintFile, "utf-8");
    await callRaw("close_sprint", { next_name: "fabricated" });
    expect(readFileSync(sprintFile, "utf-8")).toBe(before);
    let archived: string[] = [];
    try {
      archived = readdirSync(join(root, "ops", "state", "sprint", "history"));
    } catch {
      archived = [];
    }
    expect(archived).toEqual([]);
  });

  // DISCRIMINATOR, and the reason the identity gate runs first. With an explicit successor
  // the close still refuses — but on SHAPE, not identity. The refusal reason changing is what
  // proves the successor gate actually passed rather than the tool blanket-refusing.
  //
  // It also records the honest consequence of this PR: `close_sprint` cannot currently succeed
  // on ANY record the cadence guard admits, because being active requires `active_sprint` and
  // the v1 write model cannot emit that key. That is the bounded outcome — v2 closure refuses
  // until a v2 transition contract exists — not an accident.
  it("with an explicit successor it refuses on SHAPE, not on identity", async () => {
    await boot({ ...ACTIVE_V1, next_sprint_number: 44 });
    const res = await callRaw("close_sprint", { next_name: "Sprint 44" });
    expect(res.isError).toBe(true);
    expect(res.content[0]?.text).toContain("would DELETE");
    expect(res.content[0]?.text).not.toContain("UNDETERMINED");
  });

  // Review round 3: the generic refusal was de-hardcoded but close_sprint's own kept citing
  // this board's 26/27/28 lineage, which would be false guidance on any other board.
  it("its refusal states no particular board's lineage", async () => {
    await boot({ ...ACTIVE_V1, sprint: 77, active_sprint: 77 });
    const res = await callRaw("close_sprint", { next_name: "x" });
    const text = res.content[0]!.text;
    expect(res.isError).toBe(true);
    // Word-bounded: `icn#2637` legitimately contains "26", and the point is that no bare
    // sprint NUMBER from this board's history is stated.
    for (const stale of ["26", "27", "28"]) {
      expect(text, `must not hardcode sprint ${stale}`).not.toMatch(new RegExp(`\\b${stale}\\b`));
    }
    for (const stale of ["numbering planes", "narrated cadence"]) {
      expect(text, `must not hardcode "${stale}"`).not.toContain(stale);
    }
    // Still actionable: names the field to set and where the decision lives.
    expect(text).toContain("next_sprint_number");
    expect(text).toContain("icn#2637");
  });

  // Review round 3: the MCP description advertised archive-and-start, which cannot occur —
  // passing the identity gate requires `next_sprint_number`, itself a non-v1 key the shape
  // gate then rejects.
  it("does not advertise an operation it can never perform", async () => {
    await boot(DORMANT_V2);
    const { tools } = await client.listTools();
    const desc = tools.find((t) => t.name === "close_sprint")!.description!;
    expect(desc).toMatch(/REFUSES/);
    expect(desc).not.toMatch(/start a new sprint/i);
  });

  it("resolveSuccessorSprint never derives a number", async () => {
    await boot(DORMANT_V2);
    expect(tasks.resolveSuccessorSprint({ sprint: 26 }).kind).toBe("undetermined");
    expect(tasks.resolveSuccessorSprint({ sprint: 26, next_sprint_number: null }).kind).toBe("undetermined");
    expect(tasks.resolveSuccessorSprint({ sprint: 26, next_sprint_number: "" }).kind).toBe("undetermined");
    expect(tasks.resolveSuccessorSprint({ sprint: 26, next_sprint_number: 29 })).toMatchObject({
      kind: "explicit",
      sprint: 29,
    });
  });
});

describe("close_sprint cannot downgrade a v2 record through the v1 write model", () => {
  /** An ACTIVE v2 record with an explicit successor: both other gates are satisfied. */
  const ACTIVE_V2 = {
    schema: "icn-sprint-state/v2",
    domain: "sprint_state",
    cadence: "active",
    active_sprint: 31,
    sprint: 31,
    status: "in-progress",
    name: "Sprint 31",
    started: "2026-08-01",
    goals: [],
    epics: {},
    tasks: [],
    next_sprint_number: 44,
    board_lineage: { last_recorded_sprint: 30 },
    current_work_pointer: { note: "live query" },
    notes: "must survive",
  };

  it("refuses, naming every field the v1 write would delete", async () => {
    await boot(ACTIVE_V2);
    const res = await callRaw("close_sprint", { next_name: "Sprint 44" });
    expect(res.isError).toBe(true);
    const text = res.content[0]!.text;
    for (const f of [
      "schema",
      "domain",
      "cadence",
      "active_sprint",
      "status",
      "next_sprint_number",
      "board_lineage",
      "current_work_pointer",
      "notes",
    ]) {
      expect(text, `expected the refusal to name ${f}`).toContain(f);
    }
  });

  // THE STRUCTURAL REGRESSION. Not "is the board dormant today" — "can a v2 record ever be
  // rewritten as v1". Every gate before this one is satisfied, so only the shape check stands.
  it("no v2 field is lost: the record is byte-identical after the refused close", async () => {
    await boot(ACTIVE_V2);
    const before = readFileSync(sprintFile, "utf-8");
    await callRaw("close_sprint", { next_name: "Sprint 44" });
    const after = readFileSync(sprintFile, "utf-8");
    expect(after).toBe(before);
    const parsed = JSON.parse(after);
    expect(parsed.schema).toBe("icn-sprint-state/v2");
    expect(parsed.board_lineage).toEqual({ last_recorded_sprint: 30 });
    expect(parsed.notes).toBe("must survive");
  });

  it("fieldsLostByV1Write reports exactly the non-v1 keys, and nothing for a v1 record", async () => {
    await boot(DORMANT_V2);
    expect(tasks.fieldsLostByV1Write(ACTIVE_V2)).toEqual([
      "active_sprint",
      "board_lineage",
      "cadence",
      "current_work_pointer",
      "domain",
      "next_sprint_number",
      "notes",
      "schema",
      "status",
    ]);
    // Control: a pure v1 record loses nothing, so the gate is not simply always-on.
    expect(
      tasks.fieldsLostByV1Write({
        sprint: 1,
        name: "n",
        started: "d",
        goals: [],
        tasks: [],
        epics: {},
      })
    ).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// The live repository record must be refused by the shipped guard.
// ---------------------------------------------------------------------------

describe("the real ops/state/sprint/current.json", () => {
  it("is dormant, and every mutator refuses it", async () => {
    const live = JSON.parse(
      readFileSync(join(REPO_ROOT, "ops", "state", "sprint", "current.json"), "utf-8")
    );
    await boot(live);
    expect(tasks.resolveSprintCadence(live).kind).toBe("dormant");
    for (const [name, args] of MUTATORS) {
      expect((await callRaw(name, args)).isError, name).toBe(true);
    }
  });
});
