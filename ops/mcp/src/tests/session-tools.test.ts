// Tool-layer tests: every session MCP tool is invoked THROUGH THE PROTOCOL.
//
// WHY THIS FILE EXISTS
//   The other suites test the runtime core and the tables underneath it. That left a gap wide
//   enough to ship a hard failure through: `claim_files` and `session_info` queried a
//   `sessions.state` column that the v3 migration deliberately does not create. `tsc` cannot
//   type-check a SQL string, no test drove the handlers, and `tools/list` still advertised both
//   — so 163 passing tests, a green capability manifest and a green CI run all vouched for two
//   tools that threw `no such column: state` on every call.
//
//   The lesson is not "add a regression test for state". It is that a tool nothing ever CALLS
//   is a tool nothing verifies. So this suite calls each one over an in-memory transport, using
//   the same client/server handshake a real agent uses, and asserts on the returned content.

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import type Database from "better-sqlite3";
import { initDb } from "../state/db.js";
import { registerSessionTools } from "../tools/sessions.js";

let db: Database.Database;
let client: Client;

/** Call a tool the way an agent does, and fail loudly if the server reported an error. */
async function call(name: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const res = (await client.callTool({ name, arguments: args })) as {
    isError?: boolean;
    content: Array<{ type: string; text: string }>;
  };
  const text = res.content?.[0]?.text ?? "";
  if (res.isError) {
    throw new Error(`tool ${name} returned isError: ${text}`);
  }
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/** Same, but returns the error text instead of throwing — for tools expected to refuse. */
async function callRaw(name: string, args: Record<string, unknown> = {}) {
  return (await client.callTool({ name, arguments: args })) as {
    isError?: boolean;
    content: Array<{ type: string; text: string }>;
  };
}

beforeEach(async () => {
  db = initDb(":memory:");
  const server = new McpServer({ name: "icn-ops-test", version: "0.0.0" });
  registerSessionTools(server, db);
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  client = new Client({ name: "test-client", version: "0.0.0" });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
});

afterEach(() => db.close());

const LANE = { cwd: process.cwd() };

describe("every registered session tool is actually callable", () => {
  it("advertises the tools it registers", async () => {
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name).sort();
    for (const expected of [
      "register_session", "list_sessions", "claim_files", "heartbeat",
      "session_progress", "session_interaction",
      "release_session", "session_lifecycle", "session_info",
      "recent_sessions",
    ]) {
      expect(names).toContain(expected);
    }
  });

  it("invokes EVERY advertised tool without a protocol-level error", async () => {
    // DERIVED FROM tools/list, never from a literal list.
    //
    // The previous version iterated a hardcoded nine-entry array, so advertising a TENTH tool
    // with a broken handler — precisely the defect this file exists to prevent — changed
    // nothing here. The hole was closed only for the tools that happened to exist the day it
    // was written. Now an advertised tool with no entry below fails the coverage assertion
    // before anything is invoked.
    const reg = (await call("register_session", { repo: "icn", ...LANE })) as {
      session_id: string;
    };
    expect(reg.session_id).toBeTruthy();
    const sid = reg.session_id;

    const ARGS: Record<string, Record<string, unknown>> = {
      register_session: { repo: "icn", ...LANE },
      list_sessions: {},
      claim_files: { session_id: sid, files: ["icn/crates/a/src/lib.rs"] },
      heartbeat: { session_id: sid },
      session_progress: { session_id: sid, kind: "file_edit" },
      session_interaction: { session_id: sid },
      session_info: { session_id: sid },
      session_lifecycle: { ...LANE },
      recent_sessions: { count: 1 },
      release_session: { session_id: sid, reason: "completed" },
    };

    const { tools } = await client.listTools();
    const advertised = tools.map((t) => t.name);

    // A tool the server VOUCHES FOR but this suite never calls is an untested tool.
    expect(advertised.filter((n) => !(n in ARGS)).sort()).toEqual([]);
    // ...and an entry here for a tool no longer advertised is stale scaffolding.
    expect(Object.keys(ARGS).filter((n) => !advertised.includes(n)).sort()).toEqual([]);

    // release_session goes last: it deletes the row every other call depends on.
    const ordered = [
      ...advertised.filter((n) => n !== "register_session" && n !== "release_session"),
      ...advertised.filter((n) => n === "release_session"),
    ];
    const failures: string[] = [];
    for (const name of ordered) {
      const res = await callRaw(name, ARGS[name]!);
      if (res.isError) failures.push(`${name}: ${res.content?.[0]?.text}`);
    }
    expect(failures).toEqual([]);
  });
});

describe("claim_files actually grants and detects conflicts", () => {
  it("grants a claim and reports it back", async () => {
    const a = (await call("register_session", { repo: "icn", ...LANE })) as { session_id: string };
    const r = (await call("claim_files", {
      session_id: a.session_id,
      files: ["icn/crates/a/src/lib.rs", "icn/crates/b/src/lib.rs"],
    })) as { claimed: string[]; conflicts: string[] };

    expect(r.claimed).toHaveLength(2);
    expect(r.conflicts).toEqual([]);
    // ...and the claim is really in the table, not just echoed back.
    expect(
      db.prepare("SELECT COUNT(*) c FROM file_claims WHERE session_id = ?").get(a.session_id)
    ).toEqual({ c: 2 });
  });

  it("detects a conflict with another live session", async () => {
    // This is the whole point of the tool. It was returning an error instead.
    const a = (await call("register_session", { repo: "icn", provider_session_id: "conv-a", ...LANE })) as { session_id: string };
    const b = (await call("register_session", { repo: "icn", provider_session_id: "conv-b", ...LANE })) as { session_id: string };

    await call("claim_files", { session_id: a.session_id, files: ["shared.rs"] });
    const r = (await call("claim_files", {
      session_id: b.session_id,
      files: ["shared.rs", "mine.rs"],
    })) as { claimed: string[]; conflicts: string[] };

    expect(r.conflicts).toEqual(["shared.rs"]);
    expect(r.claimed).toEqual(["mine.rs"]);
  });

  it("does not conflict against a session whose heartbeat has expired", async () => {
    const a = (await call("register_session", { repo: "icn", provider_session_id: "conv-a", ...LANE })) as { session_id: string };
    const b = (await call("register_session", { repo: "icn", provider_session_id: "conv-b", ...LANE })) as { session_id: string };
    await call("claim_files", { session_id: a.session_id, files: ["stale.rs"] });
    db.prepare("UPDATE sessions SET last_heartbeat = datetime('now','-999 minutes') WHERE id = ?")
      .run(a.session_id);

    const r = (await call("claim_files", { session_id: b.session_id, files: ["stale.rs"] })) as {
      claimed: string[]; conflicts: string[];
    };
    expect(r.conflicts).toEqual([]);
    expect(r.claimed).toEqual(["stale.rs"]);
  });

  it("releases claims through release_session", async () => {
    const a = (await call("register_session", { repo: "icn", ...LANE })) as { session_id: string };
    await call("claim_files", { session_id: a.session_id, files: ["x.rs"] });
    const rel = (await call("release_session", { session_id: a.session_id })) as {
      dropped: { file_claims: number };
    };
    expect(rel.dropped.file_claims).toBe(1);
  });
});

describe("session_info returns a usable record", () => {
  it("returns the session and its children", async () => {
    const parent = (await call("register_session", { repo: "icn", provider_session_id: "p", ...LANE })) as { session_id: string };
    const child = (await call("register_session", {
      repo: "icn", provider_session_id: "c", parent_session_id: parent.session_id, ...LANE,
    })) as { session_id: string };

    const info = (await call("session_info", { session_id: parent.session_id })) as {
      id: string; children: Array<{ id: string }>;
    };
    expect(info.id).toBe(parent.session_id);
    expect(info.children.map((c) => c.id)).toEqual([child.session_id]);
  });

  it("reports not_found rather than throwing for an unknown session", async () => {
    const r = (await call("session_info", { session_id: "no-such-session" })) as { error: string };
    expect(r.error).toBe("not_found");
  });
});

describe("lifecycle tools behave through the protocol", () => {
  it("register_session is idempotent on the provider session id", async () => {
    const a = (await call("register_session", { repo: "icn", provider_session_id: "conv-x", ...LANE })) as { session_id: string; created: boolean };
    const b = (await call("register_session", { repo: "icn", provider_session_id: "conv-x", ...LANE })) as { session_id: string; created: boolean };
    expect(a.created).toBe(true);
    expect(b.created).toBe(false);
    expect(b.session_id).toBe(a.session_id);
  });

  it("rejects a pid@host provider session id with a usable message", async () => {
    const res = await callRaw("register_session", {
      repo: "icn", provider_session_id: "1234@icn-dev", ...LANE,
    });
    const text = res.content?.[0]?.text ?? "";
    expect(text).toMatch(/pid@host|reusable/i);
  });

  it("session_lifecycle protects a lane with no registry row", async () => {
    const r = (await call("session_lifecycle", { worktree_id: "/no/such/lane" })) as {
      state: string;
    };
    expect(r.state).toBe("UNREGISTERED-OBSERVED");
  });

  it("session_lifecycle refuses to guess when no lane id can be resolved", async () => {
    const r = (await call("session_lifecycle", { path: "/definitely/not/a/repo" })) as {
      state: string;
    };
    // No lane resolves, so no facts can be reported — and it must say so rather than guess.
    expect(r.state).toBe("REGISTRY-UNAVAILABLE");
  });

  it("progress advances the counter but interaction does not", async () => {
    const a = (await call("register_session", { repo: "icn", ...LANE })) as { session_id: string };
    await call("session_progress", { session_id: a.session_id, kind: "command" });
    for (let i = 0; i < 5; i++) await call("session_interaction", { session_id: a.session_id });
    const info = (await call("session_info", { session_id: a.session_id })) as {
      progress_count: number;
    };
    expect(info.progress_count).toBe(1);
  });


  it("list_sessions returns the registered session with its age fields", async () => {
    await call("register_session", { repo: "icn", ...LANE });
    const r = (await call("list_sessions", {})) as {
      ttl_minutes: number; sessions: Array<Record<string, unknown>>;
    };
    expect(r.ttl_minutes).toBeGreaterThan(0);
    expect(r.sessions).toHaveLength(1);
    expect(r.sessions[0]).toHaveProperty("heartbeat_age_min");
  });
});
