import { describe, it, expect } from "vitest";
import {
  runCommand,
  runCommandJson,
  commandAvailable,
  runCommandQuick,
} from "../utils/commands.js";
import { summarizePodsFromKubectlJsonString } from "../utils/kubectl-pods.js";

describe("runCommand", () => {
  it("captures successful stdout", async () => {
    const r = await runCommand("node", ["-e", "console.log('ok')"], {
      timeoutMs: 5000,
      maxStdoutBytes: 1024,
    });
    expect(r.ok).toBe(true);
    expect(r.exitCode).toBe(0);
    expect(r.stdout).toBe("ok");
    expect(r.timedOut).toBe(false);
  });

  it("returns structured failure for missing binary", async () => {
    const r = await runCommand("this-command-should-not-exist-icn-xyz", ["--help"], {
      timeoutMs: 3000,
      maxStdoutBytes: 256,
      maxStderrBytes: 256,
    });
    expect(r.ok).toBe(false);
    expect(r.exitCode).toBeNull();
    expect(r.stderr.length).toBeGreaterThan(0);
  });

  it("returns ok false for non-zero exit", async () => {
    const r = await runCommand("node", ["-e", "process.exit(2)"], { timeoutMs: 5000 });
    expect(r.ok).toBe(false);
    expect(r.exitCode).toBe(2);
  });

  it("times out long-running child", async () => {
    const r = await runCommand(
      "node",
      ["-e", "setTimeout(() => {}, 8000)"],
      { timeoutMs: 250, maxStderrBytes: 4096 }
    );
    expect(r.ok).toBe(false);
    expect(r.timedOut).toBe(true);
  });
});

describe("runCommandJson", () => {
  it("parses JSON stdout on success", async () => {
    const r = await runCommandJson("node", ["-e", "console.log(JSON.stringify({a:1}))"], {
      timeoutMs: 5000,
      parseContext: "test-json",
    });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ a: 1 });
  });

  it("returns parse failure for invalid JSON", async () => {
    const r = await runCommandJson("node", ["-e", "console.log('not-json')"], {
      timeoutMs: 5000,
      parseContext: "bad",
    });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.parse.ok).toBe(false);
  });
});

describe("commandAvailable", () => {
  it("is true for node", async () => {
    expect(await commandAvailable("node")).toBe(true);
  });
});

describe("runCommandQuick", () => {
  it("runs with short default budget", async () => {
    const r = await runCommandQuick("node", ["-e", "console.log(1)"]);
    expect(r.ok).toBe(true);
  });
});

describe("summarizePodsFromKubectlJsonString", () => {
  it("returns diagnostic for malformed JSON", () => {
    const r = summarizePodsFromKubectlJsonString("{");
    expect(r).toEqual(expect.objectContaining({ error: expect.any(String) }));
  });

  it("maps pod items", () => {
    const doc = {
      items: [
        {
          metadata: { name: "pod-a", namespace: "ns1" },
          status: {
            phase: "Running",
            conditions: [{ type: "Ready", status: "True" }],
          },
        },
      ],
    };
    const r = summarizePodsFromKubectlJsonString(JSON.stringify(doc));
    expect(Array.isArray(r)).toBe(true);
    expect((r as { name: string }[])[0]).toMatchObject({
      name: "pod-a",
      namespace: "ns1",
      phase: "Running",
      ready: "True",
    });
  });
});
