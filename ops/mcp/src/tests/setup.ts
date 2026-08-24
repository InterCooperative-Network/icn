// Test-suite isolation.
//
// `resolveMonorepoRoot()` honours an ambient `ICN_ROOT`, which on icn-dev the shell profile
// pins to the mcp-host worktree. `releaseSession` appends a record to
// `<root>/ops/state/session-log.jsonl`, so running the suite APPENDED FABRICATED RELEASE
// RECORDS TO THE LIVE ICN OPERATIONAL LOG — measured at +12,654 bytes across two runs, with
// fixture rows (`repo:"repoA"`, `provider_session_id:"conv-a"`) landing in the real audit
// trail. The tests passed either way, so CI stayed green while real state was corrupted.
//
// Pinning ICN_ROOT to a per-run temp directory makes the suite hermetic without any test
// needing to know about it. Nothing here changes what is under test: the same code runs, it
// simply resolves its state root somewhere disposable.
import { mkdtempSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { afterAll } from "vitest";

const root = mkdtempSync(join(tmpdir(), "icn-vitest-root-"));
process.env["ICN_ROOT"] = root;

afterAll(() => {
  rmSync(root, { recursive: true, force: true });
});
