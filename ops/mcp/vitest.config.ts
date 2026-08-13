import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/tests/**/*.test.ts"],
    // Several diagnostics tests (buildDoctorReport, buildNextStepsReport) spawn
    // real subprocesses — git, python3, and probes for kubectl/gh — sequentially
    // against a bare temp dir. On this VM that costs ~0.7–1.3s; on a GitHub
    // runner, where those binaries are cold or absent, the same tests exceed
    // vitest's 5s default and time out. The tests are correct; the default is
    // just too tight for a suite that shells out. Raised so CI measures the
    // behaviour rather than the runner's process-spawn latency.
    testTimeout: 30_000,
  },
});
