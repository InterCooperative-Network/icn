// Generates the golden OBSERVATION fixture with the REAL committed exporter.
//
//   cd ops/mcp
//   node --experimental-strip-types --no-warnings \
//        --import ./src/tests/fixtures/ts-resolve-register.mjs \
//        src/tests/fixtures/observation-golden/generate.ts <empty-output-dir>
//
// (Node >= 22.6. No build step, and nothing is written under ops/mcp/dist.)
//
// Writes into <output-dir>: observation.db, observation.db.meta.json, contract.json,
// test-vectors.json. The source registry is built in a private temp directory and deleted.
//
// WHAT IS AND IS NOT REPRODUCIBLE
//   contract.json and test-vectors.json are byte-stable for a given exporter source.
//   observation.db and its sidecar are NOT byte-reproducible: every export draws a fresh random
//   salt for withheld claim-path tokens (deliberately — see claimToken), and the sidecar carries
//   generation-time timestamps and environment versions. Regenerating therefore changes both
//   files and the sha256. The pair is still internally consistent (the sidecar's sha256 matches)
//   and its LOGICAL content — everything except token values — is identical; the test suite
//   proves that by regenerating and comparing.
//
// Everything is fictional. The only "real" strings in the output are tool versions.

import { mkdirSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from "fs";
import { syncBuiltinESMExports } from "module";
import os from "node:os";
import { tmpdir } from "os";
import { join, resolve } from "path";

// The sidecar records `host.claimed.hostname` from os.hostname(). A committed fixture must not
// carry the generating machine's real name, so substitute a fictional one BEFORE the exporter
// reads it. Live-binding sync makes `import { hostname } from "os"` see the replacement; the
// exporter itself is untouched, so this is still the genuine production code path.
const SYNTHETIC_HOST = "golden-fixture-host";
(os as unknown as { hostname: () => string }).hostname = () => SYNTHETIC_HOST;
syncBuiltinESMExports();

const { default: Database } = await import("better-sqlite3");
const { initDb } = await import("../../../state/db.js");
const { exportRegistrySnapshot, verifySnapshotPair } = await import("../../../state/snapshot-export.js");
const { SYNTHETIC, deriveContract, deriveVectors } = await import("./contract.js");

const outDir = resolve(process.argv[2] ?? "");
if (!process.argv[2]) throw new Error("usage: generate.ts <empty-output-dir>");
mkdirSync(outDir, { recursive: true });
if (readdirSync(outDir).length) throw new Error(`${outDir} is not empty; refusing to overwrite`);

const work = mkdtempSync(join(tmpdir(), "icn-golden-src-"));
let pin: InstanceType<typeof Database> | null = null;
let writer: InstanceType<typeof Database> | null = null;
try {
  const sourcePath = join(work, "synthetic-registry.db");
  writer = initDb(sourcePath);
  // Fixed, deterministic schema_version stamps (the ladder would stamp datetime('now')).
  writer.exec("UPDATE schema_version SET applied_at = '2026-01-01 00:00:00'");

  // Keep the committed rows in the WAL: no automatic checkpoint, and a reader pins the log so a
  // checkpoint cannot absorb it. The export below therefore must read committed-but-
  // uncheckpointed data — that is the property this fixture documents.
  writer.pragma("wal_autocheckpoint = 0");
  pin = new Database(sourcePath, { readonly: true });
  pin.exec("BEGIN");
  pin.prepare("SELECT COUNT(*) FROM sessions").get();

  const S = SYNTHETIC;
  const ins = writer.prepare(
    `INSERT INTO sessions
       (id, repo, repo_id, worktree, worktree_id, worktree_path, worktree_name, worktree_generation,
        branch_at_registration, head_at_registration, task_description, task_ref, pr_ref,
        parent_session_id, provider, agent_pid, host, provider_session_id, transcript_path,
        current_activity, started_at, last_heartbeat, last_progress, progress_count)
     VALUES (@id, 'icn-golden', @repo_id, 'GOLDEN-CANARY-free-worktree-label', @worktree_id,
        '${S.root}/GOLDEN-CANARY-worktree-path', @name, @gen,
        'GOLDEN-CANARY-branch-name', 'GOLDEN-CANARY-0000000000000000000000000000000000',
        'GOLDEN-CANARY-task-description', 'GOLDEN-CANARY-task-ref', 'GOLDEN-CANARY-pr-ref',
        @parent, 'claude', @pid, @host, @psid, '${S.root}/GOLDEN-CANARY-transcript.jsonl',
        'GOLDEN-CANARY-current-activity', @t0, @t1, @t2, @n)`
  );
  const base = { repo_id: S.repoId, host: "golden-host-a", parent: null as string | null, pid: 41001 as number | null };
  writer.transaction(() => {
    // Lane alpha: three activations (one conversation, one with NO conversation id, one with NO
    // generation). Lane beta: one activation that a lane-alpha lookup must never select.
    ins.run({ ...base, id: "sess-alpha-1", worktree_id: S.laneAlpha, name: "lane-alpha", gen: S.generationAlpha, psid: S.conversationAlpha1, t0: "2026-01-01 10:00:00", t1: "2026-01-01 10:20:00", t2: "2026-01-01 10:15:00", n: 7 });
    ins.run({ ...base, id: "sess-alpha-2", worktree_id: S.laneAlpha, name: "lane-alpha", gen: S.generationAlpha, psid: null, parent: "sess-alpha-1", pid: 41002, t0: "2026-01-01 10:05:00", t1: "2026-01-01 10:21:00", t2: null, n: 0 });
    ins.run({ ...base, id: "sess-alpha-3", worktree_id: S.laneAlpha, name: "lane-alpha", gen: null, psid: S.conversationAlpha3, pid: null, t0: "2026-01-01 10:10:00", t1: "2026-01-01 10:22:00", t2: "2026-01-01 10:18:00", n: 3 });
    ins.run({ ...base, id: "sess-beta-1", worktree_id: S.laneBeta, name: "lane-beta", gen: S.generationBeta, psid: S.conversationBeta1, host: "golden-host-b", pid: 41009, t0: "2026-01-01 11:00:00", t1: "2026-01-01 11:30:00", t2: "2026-01-01 11:25:00", n: 12 });
    // A pre-identity row: repo_id/worktree_id/generation/conversation all NULL.
    ins.run({ ...base, id: "sess-legacy-1", repo_id: null, worktree_id: null, name: "lane-legacy", gen: null, psid: null, pid: null, t0: "2025-12-31 09:00:00", t1: "2025-12-31 09:30:00", t2: null, n: 0 });

    const claim = writer!.prepare("INSERT INTO file_claims (file_path, session_id, claimed_at) VALUES (?, ?, ?)");
    // A deliberate ownership conflict on a disclosable path, ACROSS lanes.
    claim.run("icn/crates/golden-core/src/lib.rs", "sess-alpha-1", "2026-01-01 10:01:00");
    claim.run("icn/crates/golden-core/src/lib.rs", "sess-beta-1", "2026-01-01 11:01:00");
    claim.run("docs/golden/notes.md", "sess-alpha-2", "2026-01-01 10:06:00");
    // A conflict on a path that must be withheld: same token for both rows, path never shown.
    claim.run(`${S.root}/GOLDEN-CANARY-secret.key`, "sess-alpha-1", "2026-01-01 10:02:00");
    claim.run(`${S.root}/GOLDEN-CANARY-secret.key`, "sess-beta-1", "2026-01-01 11:02:00");
    claim.run("~/GOLDEN-CANARY-tilde-claim", "sess-alpha-2", "2026-01-01 10:07:00");
    claim.run("docs/golden/with space.md", "sess-alpha-3", "2026-01-01 10:11:00");

    // Everything the restricted profile must not carry at all.
    writer!.prepare("INSERT INTO mailbox (to_session, from_session, kind, payload, created_at) VALUES ('sess-alpha-1','sess-alpha-2','GOLDEN-CANARY-kind',?,1)").run(JSON.stringify({ body: "GOLDEN-CANARY-mailbox-body" }));
    writer!.prepare("INSERT INTO events (scope, type, payload, created_at) VALUES ('GOLDEN-CANARY-scope','x',?,1)").run(JSON.stringify({ n: "GOLDEN-CANARY-event-payload" }));
    writer!.prepare("INSERT INTO watchers_process (session_id, pid, label, created_at) VALUES ('sess-alpha-1', 9, 'GOLDEN-CANARY-watcher-label', 1)").run();
    writer!.prepare("INSERT INTO health_cache (key, value) VALUES ('svc', ?)").run(JSON.stringify({ u: "GOLDEN-CANARY-health" }));
    writer!.prepare("INSERT INTO decision_index (id, title, tags, file_path) VALUES ('d1','GOLDEN-CANARY-decision','t','docs/adr/none.md')").run();
  })();

  const walBytes = statSync(`${sourcePath}-wal`).size;
  if (walBytes === 0) throw new Error("fixture invariant: the committed rows must still be in the WAL");

  const dest = join(outDir, "observation.db");
  const r = exportRegistrySnapshot({ profile: "observation", source: sourcePath, dest });
  const v = verifySnapshotPair(dest);
  if (!v.ok) throw new Error(`generated pair does not verify: ${v.problems.join("; ")}`);

  writeFileSync(join(outDir, "contract.json"), JSON.stringify(deriveContract(dest), null, 2) + "\n");
  writeFileSync(join(outDir, "test-vectors.json"), JSON.stringify(deriveVectors(dest), null, 2) + "\n");
  process.stdout.write(
    JSON.stringify({ ok: true, outDir, sha256: r.meta.snapshot.sha256, bytes: r.meta.snapshot.bytes, source_wal_bytes: walBytes }) + "\n"
  );
} finally {
  try { pin?.exec("COMMIT"); pin?.close(); } catch { /* best effort */ }
  try { writer?.close(); } catch { /* best effort */ }
  rmSync(work, { recursive: true, force: true });
}
