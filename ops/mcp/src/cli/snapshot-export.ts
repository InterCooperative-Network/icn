#!/usr/bin/env node
// Export a consistent offline snapshot of an EXISTING icn-ops session registry.
//
//   node dist/cli/snapshot-export.js --source <registry.db> --dest <snapshot.db> [--busy-timeout-ms N]
//
// Both paths are required. There is no default source: this tool must not guess which registry
// to read, and it never creates or migrates one. See docs/architecture/AGENT_RUNTIME.md,
// "Registry snapshot export".
//
// Exit codes: 0 exported · 1 refused (stdout empty, stderr = one JSON error line) · 2 usage.

import { exportRegistrySnapshot, SnapshotError } from "../state/snapshot-export.js";

function usage(msg?: string): never {
  if (msg) process.stderr.write(`snapshot-export: ${msg}\n`);
  process.stderr.write(
    "usage: snapshot-export --source <registry.db> --dest <snapshot.db> [--busy-timeout-ms N]\n"
  );
  process.exit(2);
}

function parseArgs(argv: string[]): { source: string; dest: string; busyTimeoutMs?: number } {
  const out: Record<string, string> = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--help" || a === "-h") usage();
    if (!["--source", "--dest", "--busy-timeout-ms"].includes(a)) usage(`unknown argument ${a}`);
    const v = argv[++i];
    if (v === undefined || v.startsWith("--")) usage(`${a} needs a value`);
    out[a] = v;
  }
  if (!out["--source"]) usage("--source is required");
  if (!out["--dest"]) usage("--dest is required");
  let busyTimeoutMs: number | undefined;
  if (out["--busy-timeout-ms"] !== undefined) {
    if (!/^\d+$/.test(out["--busy-timeout-ms"])) usage("--busy-timeout-ms must be a non-negative integer");
    busyTimeoutMs = Number(out["--busy-timeout-ms"]);
  }
  return { source: out["--source"], dest: out["--dest"], busyTimeoutMs };
}

const args = parseArgs(process.argv.slice(2));
try {
  const r = exportRegistrySnapshot(args);
  process.stdout.write(
    JSON.stringify({
      ok: true,
      dest: r.dest,
      meta_path: r.meta_path,
      sha256: r.meta.snapshot.sha256,
      bytes: r.meta.snapshot.bytes,
      schema_version: r.meta.snapshot.schema_version_max,
    }) + "\n"
  );
} catch (e) {
  const code = e instanceof SnapshotError ? e.code : "INTERNAL";
  process.stderr.write(JSON.stringify({ ok: false, code, message: (e as Error).message }) + "\n");
  process.exit(1);
}
