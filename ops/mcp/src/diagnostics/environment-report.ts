import { existsSync } from "node:fs";
import { join } from "node:path";
import { createRequire } from "node:module";
import { inspectMcpConfigs } from "./mcp-config.js";
import { execFileNoThrow } from "./exec-safe.js";

export type EnvironmentWarning = { code: string; message: string };

export type EnvironmentReport = {
  repoRoot: string;
  warnings: EnvironmentWarning[];
  git: {
    branch: string | null;
    commit: string | null;
    dirtySummary: string;
    porcelainLines: number | null;
  };
  node: { version: string; abi: string };
  npm: { version: string | null; warning?: string };
  rust: { version: string | null; warning?: string };
  python: { version: string | null; warning?: string };
  gh: {
    available: boolean;
    authSummary: string | null;
    warning?: string;
  };
  kubectl: {
    available: boolean;
    context: string | null;
    warning?: string;
  };
  paths: {
    nodeModules: boolean;
    distIndex: boolean;
  };
  betterSqlite3: { loadable: boolean; detail?: string };
  mcpConfigs: ReturnType<typeof inspectMcpConfigs>;
};

async function gitLine(
  repoRoot: string,
  args: string[]
): Promise<{ ok: boolean; out: string }> {
  const r = await execFileNoThrow("git", args, { cwd: repoRoot, timeoutMs: 10_000 });
  return { ok: r.ok, out: r.stdout };
}

export async function buildEnvironmentReport(repoRoot: string): Promise<EnvironmentReport> {
  const warnings: EnvironmentWarning[] = [];

  const branchR = await gitLine(repoRoot, ["rev-parse", "--abbrev-ref", "HEAD"]);
  const commitR = await gitLine(repoRoot, ["rev-parse", "HEAD"]);
  const porR = await gitLine(repoRoot, ["status", "--porcelain"]);

  let porcelainLines: number | null = null;
  let dirtySummary = "unknown";
  if (porR.ok) {
    const lines = porR.out ? porR.out.split("\n").filter(Boolean) : [];
    porcelainLines = lines.length;
    dirtySummary = lines.length === 0 ? "clean" : `dirty (${lines.length} paths)`;
  } else {
    dirtySummary = "git unavailable or not a repo";
    warnings.push({ code: "git_status", message: "Could not read git status in repo root." });
  }

  const npmR = await execFileNoThrow("npm", ["-v"], { cwd: repoRoot, timeoutMs: 5000 });
  const npmVersion = npmR.ok ? npmR.stdout : null;
  if (!npmR.ok) {
    warnings.push({ code: "npm_missing", message: npmR.stderr || "npm not found" });
  }

  const rustcR = await execFileNoThrow("rustc", ["-V"], { cwd: repoRoot, timeoutMs: 5000 });
  const rustVersion = rustcR.ok ? rustcR.stdout : null;
  if (!rustcR.ok) {
    warnings.push({ code: "rustc_missing", message: "rustc not on PATH or failed." });
  }

  const pyR = await execFileNoThrow("python3", ["-V"], { cwd: repoRoot, timeoutMs: 5000 });
  const pythonVersion = pyR.ok ? pyR.stdout : null;
  if (!pyR.ok) {
    warnings.push({ code: "python3_missing", message: "python3 not on PATH or failed." });
  }

  let ghAvailable = false;
  let ghAuthSummary: string | null = null;
  const ghVer = await execFileNoThrow("gh", ["--version"], { cwd: repoRoot, timeoutMs: 5000 });
  if (ghVer.ok) {
    ghAvailable = true;
    const ghR = await execFileNoThrow("gh", ["auth", "status"], { cwd: repoRoot, timeoutMs: 8000 });
    const combined = [ghR.stdout, ghR.stderr].filter(Boolean).join("\n").slice(0, 500);
    ghAuthSummary = combined || (ghR.ok ? "ok" : "unknown");
    if (!ghR.ok) {
      warnings.push({
        code: "gh_auth",
        message: "gh is installed but auth status did not succeed (may be unauthenticated).",
      });
    }
  } else {
    warnings.push({ code: "gh_missing", message: "gh CLI not available." });
  }

  let kubectlAvailable = false;
  let kubectlContext: string | null = null;
  const kc = await execFileNoThrow(
    "kubectl",
    ["config", "current-context"],
    { cwd: repoRoot, timeoutMs: 5000 }
  );
  if (kc.ok && kc.stdout) {
    kubectlAvailable = true;
    kubectlContext = kc.stdout;
  } else {
    const whichK = await execFileNoThrow("kubectl", ["version", "--client=true"], {
      cwd: repoRoot,
      timeoutMs: 5000,
    });
    kubectlAvailable = whichK.ok;
    if (!kubectlAvailable) {
      warnings.push({ code: "kubectl_missing", message: "kubectl not available or not configured." });
    } else {
      warnings.push({
        code: "kubectl_no_context",
        message: "kubectl present but no current context (or context command failed).",
      });
    }
  }

  const nm = join(repoRoot, "ops", "mcp", "node_modules");
  const dist = join(repoRoot, "ops", "mcp", "dist", "index.js");

  let betterLoadable = false;
  let betterDetail: string | undefined;
  const pkgJson = join(repoRoot, "ops", "mcp", "package.json");
  if (existsSync(pkgJson)) {
    try {
      const req = createRequire(pkgJson);
      req("better-sqlite3");
      betterLoadable = true;
    } catch (e) {
      betterLoadable = false;
      betterDetail = e instanceof Error ? e.message : String(e);
      warnings.push({
        code: "better_sqlite3",
        message: "better-sqlite3 could not be loaded from ops/mcp (ABI or install issue).",
      });
    }
  } else {
    warnings.push({ code: "ops_mcp_package", message: "ops/mcp/package.json not found." });
  }

  return {
    repoRoot,
    warnings,
    git: {
      branch: branchR.ok ? branchR.out : null,
      commit: commitR.ok ? commitR.out : null,
      dirtySummary,
      porcelainLines,
    },
    node: {
      version: process.version,
      abi: process.versions.modules ?? "unknown",
    },
    npm: { version: npmVersion },
    rust: { version: rustVersion },
    python: { version: pythonVersion },
    gh: {
      available: ghAvailable,
      authSummary: ghAuthSummary,
    },
    kubectl: {
      available: kubectlAvailable,
      context: kubectlContext,
    },
    paths: {
      nodeModules: existsSync(nm),
      distIndex: existsSync(dist),
    },
    betterSqlite3: { loadable: betterLoadable, detail: betterDetail },
    mcpConfigs: inspectMcpConfigs(repoRoot),
  };
}

/** Sync variant for tests: minimal report without spawning many binaries. */
export function buildEnvironmentReportSync(repoRoot: string): Pick<
  EnvironmentReport,
  "repoRoot" | "paths" | "mcpConfigs" | "node" | "betterSqlite3" | "warnings"
> {
  const warnings: EnvironmentWarning[] = [];
  const nm = join(repoRoot, "ops", "mcp", "node_modules");
  const dist = join(repoRoot, "ops", "mcp", "dist", "index.js");
  let betterLoadable = false;
  let betterDetail: string | undefined;
  const pkgJson = join(repoRoot, "ops", "mcp", "package.json");
  if (existsSync(pkgJson)) {
    try {
      const req = createRequire(pkgJson);
      req("better-sqlite3");
      betterLoadable = true;
    } catch (e) {
      betterLoadable = false;
      betterDetail = e instanceof Error ? e.message : String(e);
      warnings.push({
        code: "better_sqlite3",
        message: "better-sqlite3 could not be loaded from ops/mcp (ABI or install issue).",
      });
    }
  }
  if (!existsSync(nm)) {
    warnings.push({ code: "missing_node_modules", message: "ops/mcp/node_modules missing." });
  }
  if (!existsSync(dist)) {
    warnings.push({ code: "missing_dist", message: "ops/mcp/dist/index.js missing." });
  }
  return {
    repoRoot,
    warnings,
    node: {
      version: process.version,
      abi: process.versions.modules ?? "unknown",
    },
    paths: {
      nodeModules: existsSync(nm),
      distIndex: existsSync(dist),
    },
    betterSqlite3: { loadable: betterLoadable, detail: betterDetail },
    mcpConfigs: inspectMcpConfigs(repoRoot),
  };
}
