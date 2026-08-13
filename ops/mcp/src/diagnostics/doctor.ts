import { join } from "node:path";
import { buildEnvironmentReport } from "./environment-report.js";
import { buildStateIndex } from "./state-index.js";
import { runCommand } from "../utils/commands.js";

export type DoctorCheck = {
  id: string;
  severity: "ok" | "warn" | "error";
  message: string;
  detail?: string;
};

export type DoctorReport = {
  severity: "ok" | "warn" | "error";
  summary: string;
  checks: DoctorCheck[];
  suggested_repairs: string[];
};

/**
 * A checkout on `main` that is behind its own `origin/main` is serving stale
 * truth: every canonical doc it exposes (STATE.md, ARCHITECTURE.md, AGENTS.md,
 * ops/state/truth/*) is that many commits out of date. Escalate to `error` past
 * this many commits — at that point the tree is not "slightly behind", it is a
 * different repository state than the one it claims to represent.
 */
export const MAIN_STALE_ERROR_COMMITS = 25;

/**
 * A feature branch is *expected* to sit behind main while work is in progress,
 * so behind-ness is only worth mentioning once the branch base is old enough
 * that a rebase is genuinely overdue.
 */
export const FEATURE_BASE_STALE_WARN_COMMITS = 50;

/**
 * Classify how stale a checkout is relative to origin/main.
 *
 * `behindCount` of NaN means origin/main could not be resolved (no remote ref,
 * not a repo) — that is "unknown", never "fine".
 */
export function classifyTreeFreshness(
  branch: string | null,
  behindCount: number
): { severity: DoctorCheck["severity"]; message: string; detail?: string; repair?: string } {
  if (!Number.isFinite(behindCount)) {
    // `warn`, not `ok`. The top-level report folds only `severity` (see
    // maxSeverity), so returning "ok" here with an explanatory message would
    // still render the whole environment as "healthy" — reintroducing, one
    // level down, the exact defect this check exists to remove: a confident
    // health verdict over truth nobody verified. Unverifiable is not fine.
    return {
      severity: "warn",
      message:
        "origin/main not resolvable here; checkout freshness is UNVERIFIED (not confirmed current).",
      repair: "git fetch origin  # let doctor resolve origin/main and verify freshness",
    };
  }
  if (behindCount === 0) {
    return { severity: "ok", message: "Checkout is level with origin/main." };
  }
  if (branch === "main") {
    const stale = behindCount >= MAIN_STALE_ERROR_COMMITS;
    return {
      severity: stale ? "error" : "warn",
      message: `Checkout is on main but ${behindCount} commit(s) behind origin/main — canonical docs and ops/state served from here are stale.`,
      ...(stale
        ? {
            detail: `At or past ${MAIN_STALE_ERROR_COMMITS} commits this tree no longer represents main; treat anything read from it as unverified.`,
          }
        : {}),
      repair: "git pull --ff-only  # re-sync the tree serving canonical docs",
    };
  }
  if (behindCount >= FEATURE_BASE_STALE_WARN_COMMITS) {
    return {
      severity: "warn",
      message: `Branch base is ${behindCount} commit(s) behind origin/main; a rebase is overdue.`,
      repair: "git fetch origin && git rebase origin/main",
    };
  }
  return {
    severity: "ok",
    message: `Feature branch ${behindCount} commit(s) behind origin/main (expected during active work).`,
  };
}

function maxSeverity(
  a: DoctorReport["severity"],
  b: DoctorCheck["severity"]
): DoctorReport["severity"] {
  const rank = { ok: 0, warn: 1, error: 2 };
  return rank[a] >= rank[b] ? a : b;
}

export async function buildDoctorReport(repoRoot: string): Promise<DoctorReport> {
  const checks: DoctorCheck[] = [];
  const suggested: string[] = [];
  let top: DoctorReport["severity"] = "ok";

  const env = await buildEnvironmentReport(repoRoot);

  if (!env.paths.nodeModules) {
    checks.push({
      id: "node_modules",
      severity: "error",
      message: "ops/mcp/node_modules is missing.",
    });
    suggested.push("cd ops/mcp && npm ci");
    top = maxSeverity(top, "error");
  } else {
    checks.push({ id: "node_modules", severity: "ok", message: "ops/mcp/node_modules present." });
  }

  if (!env.paths.distIndex) {
    checks.push({
      id: "dist",
      severity: "warn",
      message: "ops/mcp/dist/index.js missing (MCP host may still build via start:stdio).",
    });
    suggested.push("cd ops/mcp && npm run build");
    top = maxSeverity(top, "warn");
  } else {
    checks.push({ id: "dist", severity: "ok", message: "ops/mcp/dist/index.js present." });
  }

  if (!env.betterSqlite3.loadable) {
    checks.push({
      id: "better_sqlite3",
      severity: "error",
      message: "better-sqlite3 failed to load (often Node ABI mismatch).",
      detail: env.betterSqlite3.detail,
    });
    suggested.push("cd ops/mcp && npm rebuild better-sqlite3");
    suggested.push("Ensure the MCP host uses the same Node major as used for npm ci.");
    top = maxSeverity(top, "error");
  } else {
    checks.push({
      id: "better_sqlite3",
      severity: "ok",
      message: "better-sqlite3 loads under this Node process.",
    });
  }

  for (const f of env.mcpConfigs.files) {
    if (!f.exists) {
      checks.push({
        id: `mcp_${f.path}`,
        severity: "warn",
        message: `MCP config missing: ${f.path}`,
      });
      top = maxSeverity(top, "warn");
      continue;
    }
    if (f.parseError) {
      checks.push({
        id: `mcp_${f.path}_parse`,
        severity: "error",
        message: `Could not parse ${f.path}: ${f.parseError}`,
      });
      top = maxSeverity(top, "error");
      continue;
    }
    if (f.invokesDistDirectly) {
      checks.push({
        id: `mcp_${f.path}_dist`,
        severity: "error",
        message: `${f.path} invokes dist/index.js directly; use npm start:stdio instead.`,
      });
      suggested.push("Set icn-ops to: npm --prefix ./ops/mcp run start:stdio");
      top = maxSeverity(top, "error");
    }
    if (f.usesCanonicalLaunch === false) {
      checks.push({
        id: `mcp_${f.path}_canonical`,
        severity: "error",
        message: `${f.path} does not use the canonical icn-ops launch command.`,
      });
      suggested.push("Match scripts/check-mcp-portability.py enforced stanza.");
      top = maxSeverity(top, "error");
    } else if (f.usesCanonicalLaunch === true) {
      checks.push({
        id: `mcp_${f.path}_canonical`,
        severity: "ok",
        message: `${f.path} uses canonical launch.`,
      });
    }
  }

  const py = await runCommand(
    "python3",
    [join(repoRoot, "scripts", "check-mcp-portability.py")],
    {
      cwd: repoRoot,
      timeoutMs: 15_000,
      maxStdoutBytes: 32 * 1024,
      maxStderrBytes: 32 * 1024,
    }
  );
  if (py.ok) {
    checks.push({ id: "portability_script", severity: "ok", message: "MCP portability script passed." });
  } else {
    checks.push({
      id: "portability_script",
      severity: "error",
      message: "MCP portability script failed.",
      detail: [py.stderr, py.stdout].filter(Boolean).join("\n").slice(0, 800),
    });
    suggested.push("Fix .mcp.json / .cursor/mcp.json per script output.");
    top = maxSeverity(top, "error");
  }

  if (!env.kubectl.available) {
    checks.push({
      id: "kubectl",
      severity: "warn",
      message: "kubectl not available or not configured (cluster tools will be limited).",
    });
    top = maxSeverity(top, "warn");
  } else {
    checks.push({
      id: "kubectl",
      severity: "ok",
      message: `kubectl available${env.kubectl.context ? ` (context: ${env.kubectl.context})` : ""}.`,
    });
  }

  if (!env.gh.available) {
    checks.push({
      id: "gh",
      severity: "warn",
      message: "gh CLI not available (PR automation helpers unavailable).",
    });
    top = maxSeverity(top, "warn");
  } else {
    checks.push({ id: "gh", severity: "ok", message: "gh CLI available." });
  }

  if (env.git.porcelainLines === null) {
    checks.push({
      id: "dirty_tree",
      severity: "ok",
      message: "Git porcelain unavailable; dirty-tree check skipped.",
    });
  } else if (env.git.porcelainLines > 0) {
    checks.push({
      id: "dirty_tree",
      severity: "warn",
      message: `Dirty worktree: ${env.git.dirtySummary}`,
    });
    suggested.push("git status — commit or stash before switching tasks if required by workflow.");
    top = maxSeverity(top, "warn");
  } else {
    checks.push({
      id: "dirty_tree",
      severity: "ok",
      message: "Worktree clean.",
    });
  }

  // Tree freshness. `dirty_tree` above answers "are there uncommitted edits?",
  // which is trivially "no" for a checkout nobody has touched in weeks — so a
  // clean tree is, on its own, an inverted signal for the staleness we care
  // about. This check asks the question that actually matters for a host that
  // serves canonical docs: how far behind origin/main is this checkout?
  //
  // No fetch is performed (doctor is read-only diagnosis), so `origin/main` is
  // whatever the local object store last saw. That makes this a LOWER BOUND on
  // staleness, never an overestimate.
  const behindProbe = await runCommand(
    "git",
    ["rev-list", "--count", "HEAD..origin/main"],
    { cwd: repoRoot, timeoutMs: 5_000, maxStdoutBytes: 256 }
  );
  const behindCount = behindProbe.ok
    ? Number.parseInt(behindProbe.stdout.trim(), 10)
    : Number.NaN;

  const freshness = classifyTreeFreshness(env.git.branch, behindCount);
  checks.push({
    id: "tree_freshness",
    severity: freshness.severity,
    message: freshness.message,
    ...(freshness.detail ? { detail: freshness.detail } : {}),
  });
  if (freshness.repair) suggested.push(freshness.repair);
  top = maxSeverity(top, freshness.severity);

  const runnerProbes: [string, Awaited<ReturnType<typeof runCommand>>][] = [
    ["git", await runCommand("git", ["--version"], { cwd: repoRoot, timeoutMs: 4000, maxStdoutBytes: 256 })],
    ["npm", await runCommand("npm", ["-v"], { cwd: repoRoot, timeoutMs: 4000, maxStdoutBytes: 256 })],
    [
      "python3",
      await runCommand("python3", ["-V"], { cwd: repoRoot, timeoutMs: 4000, maxStdoutBytes: 256 }),
    ],
  ];
  for (const [name, pr] of runnerProbes) {
    if (!pr.ok) {
      checks.push({
        id: `runner_${name}`,
        severity: "warn",
        message: `${name} CLI probe failed (command runner).`,
        detail: [pr.stderr, pr.timedOut ? "timed out" : ""].filter(Boolean).join(" ").slice(0, 400),
      });
      top = maxSeverity(top, "warn");
    } else {
      checks.push({
        id: `runner_${name}`,
        severity: "ok",
        message: `${name} CLI responds.`,
      });
    }
  }

  const state = buildStateIndex(repoRoot);
  const requiredState = ["ops/state/sprint/current.json", "ops/state/config/repo-map.json"];
  for (const id of requiredState) {
    const row = state.entries.find((e) => e.id === id);
    if (!row?.present) {
      checks.push({
        id: `state_${id.replace(/\//g, "_")}`,
        severity: "warn",
        message: `Expected ops state file missing: ${id}`,
      });
      top = maxSeverity(top, "warn");
    } else {
      checks.push({
        id: `state_${id.replace(/\//g, "_")}`,
        severity: "ok",
        message: `Present: ${id}`,
      });
    }
  }

  const summary =
    top === "ok"
      ? "Environment looks healthy for icn-ops MCP and local agent workflows."
      : top === "warn"
        ? "Environment usable with warnings; review checks before high-risk changes."
        : "Blocking issues detected; fix errors before relying on MCP or native tooling.";

  return {
    severity: top,
    summary,
    checks,
    suggested_repairs: [...new Set(suggested)],
  };
}
