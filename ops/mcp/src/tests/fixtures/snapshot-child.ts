// Child-process runner for the hard-kill tests. Run by node (type stripping), not by vitest:
//
//   node --import ./ts-resolve-register.mjs snapshot-child.ts <profile> <source> <dest> [killAt]
//
// `killAt` names an export stage at which the process SIGKILLs ITSELF — no handlers, no finally
// blocks, no cleanup: exactly what an OOM kill or a pulled plug leaves behind. The umask is
// zeroed first so the test also proves file modes do not depend on the ambient umask.

import { exportRegistrySnapshot } from "../../state/snapshot-export.js";

process.umask(0);
const [profile, source, dest, killAt] = process.argv.slice(2);
const die = () => process.kill(process.pid, "SIGKILL");

const r = exportRegistrySnapshot({
  profile: profile as "full" | "observation",
  source: source!,
  dest: dest!,
  hooks: {
    afterCopy: killAt === "afterCopy" ? die : undefined,
    afterVerify: killAt === "afterVerify" ? die : undefined,
    afterPublishDb: killAt === "afterPublishDb" ? die : undefined,
  },
});
process.stdout.write(JSON.stringify({ ok: true, dest: r.dest }) + "\n");
