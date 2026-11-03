// Child-process runner for verifySnapshotPair, so a verifier that HANGS (a FIFO where a file
// should be) is bounded by the parent's spawn timeout instead of freezing the test run.
//
//   node --import ./ts-resolve-register.mjs verify-child.ts <dest> [expectProfile]
//
// Prints the verdict as one JSON line. Exits non-zero (uncaught) if verifySnapshotPair throws.

import { verifySnapshotPair } from "../../state/snapshot-export.js";

const [dest, expectProfile] = process.argv.slice(2);
const verdict = expectProfile
  ? verifySnapshotPair(dest!, { expectProfile: expectProfile as "full" | "observation" })
  : verifySnapshotPair(dest!);
process.stdout.write(JSON.stringify(verdict) + "\n");
