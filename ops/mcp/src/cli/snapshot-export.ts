#!/usr/bin/env node
// Export a consistent offline snapshot of an EXISTING icn-ops session registry, or verify one.
//
//   snapshot-export --profile <full|observation> --source <registry.db> --dest <snapshot.db>
//                   [--busy-timeout-ms N] [--max-source-bytes N]
//   snapshot-export --verify <snapshot.db> [--expect-profile <full|observation>]
//
// --profile, --source and --dest are all required. There is no default source (this tool must
// not guess which registry to read, and never creates or migrates one) and no default profile
// (full = recovery inside the ICN trust boundary; observation = least-privilege, for an
// external reader — a disclosure decision the caller must make). See
// docs/architecture/AGENT_RUNTIME.md, "Registry snapshot export".
//
// Exit codes: 0 exported / verified · 1 refused or NOT verified (one JSON line on stderr for an
// export refusal; the verdict on stdout for --verify) · 2 usage.
//
// A consumer of an external observation should always pass `--expect-profile observation`:
// without it, a valid `full` pair (mailbox bodies, transcript paths) verifies as ok.

import { exportRegistrySnapshot, SnapshotError, verifySnapshotPair } from "../state/snapshot-export.js";

const USAGE =
  "usage: snapshot-export --profile <full|observation> --source <registry.db> --dest <snapshot.db> " +
  "[--busy-timeout-ms N] [--max-source-bytes N]\n" +
  "       snapshot-export --verify <snapshot.db> [--expect-profile <full|observation>]\n";

function usage(msg?: string): never {
  if (msg) process.stderr.write(`snapshot-export: ${msg}\n`);
  process.stderr.write(USAGE);
  process.exit(2);
}

const FLAGS = ["--profile", "--source", "--dest", "--busy-timeout-ms", "--max-source-bytes", "--verify", "--expect-profile"];

function parse(argv: string[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--help" || a === "-h") usage();
    if (!FLAGS.includes(a)) usage(`unknown argument ${a}`);
    const v = argv[++i];
    if (v === undefined || v.startsWith("--")) usage(`${a} needs a value`);
    out[a] = v;
  }
  return out;
}

function intFlag(args: Record<string, string>, flag: string): number | undefined {
  const raw = args[flag];
  if (raw === undefined) return undefined;
  if (!/^\d{1,15}$/.test(raw)) usage(`${flag} must be a non-negative integer`);
  return Number(raw);
}

const args = parse(process.argv.slice(2));

if (args["--verify"] !== undefined) {
  const extra = Object.keys(args).filter((k) => k !== "--verify" && k !== "--expect-profile");
  if (extra.length) usage("--verify takes no option other than --expect-profile");
  const expect = args["--expect-profile"];
  if (expect !== undefined && expect !== "full" && expect !== "observation") usage("--expect-profile must be 'full' or 'observation'");
  const v = verifySnapshotPair(args["--verify"], expect ? { expectProfile: expect } : {});
  process.stdout.write(JSON.stringify(v) + "\n");
  process.exit(v.ok ? 0 : 1);
}

if (args["--expect-profile"] !== undefined) usage("--expect-profile applies only to --verify");
const profile = args["--profile"];
if (profile !== "full" && profile !== "observation") usage("--profile must be 'full' or 'observation'; there is no default");
if (!args["--source"]) usage("--source is required");
if (!args["--dest"]) usage("--dest is required");

try {
  const r = exportRegistrySnapshot({
    profile,
    source: args["--source"],
    dest: args["--dest"],
    busyTimeoutMs: intFlag(args, "--busy-timeout-ms"),
    maxSourceBytes: intFlag(args, "--max-source-bytes"),
  });
  process.stdout.write(
    JSON.stringify({
      ok: true,
      profile: r.meta.profile,
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
