/**
 * Safe subprocess helpers — execFile argv only (no shell) by default.
 * Structured results for diagnostics and pollers; never throws to callers.
 */

import { execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  safeJsonParse,
  type SafeJsonResult,
} from "../diagnostics/safe-json.js";

const execFileAsync = promisify(execFile);

const DEFAULT_TIMEOUT_MS = 10_000;
const DEFAULT_MAX_STDOUT = 512 * 1024;
const DEFAULT_MAX_STDERR = 64 * 1024;

export type RunCommandResult = {
  ok: boolean;
  exitCode: number | null;
  signal: string | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
};

function truncate(s: string, maxBytes: number): string {
  if (s.length <= maxBytes) return s;
  return `${s.slice(0, maxBytes)}\n… [truncated ${s.length - maxBytes} chars]`;
}

export type RunCommandOptions = {
  cwd?: string;
  timeoutMs?: number;
  maxStdoutBytes?: number;
  maxStderrBytes?: number;
  env?: NodeJS.ProcessEnv;
};

/**
 * Run `command` with argv (no shell). Returns structured output; does not throw
 * for missing binaries, non-zero exit, or timeouts.
 */
export async function runCommand(
  command: string,
  args: readonly string[],
  options: RunCommandOptions = {}
): Promise<RunCommandResult> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const maxOut = options.maxStdoutBytes ?? DEFAULT_MAX_STDOUT;
  const maxErr = options.maxStderrBytes ?? DEFAULT_MAX_STDERR;
  const maxBuffer = Math.min(50 * 1024 * 1024, maxOut + maxErr + 64 * 1024);

  try {
    const { stdout, stderr } = await execFileAsync(command, [...args], {
      cwd: options.cwd,
      timeout: timeoutMs,
      maxBuffer,
      encoding: "utf-8",
      env: options.env ?? process.env,
    });
    return {
      ok: true,
      exitCode: 0,
      signal: null,
      stdout: truncate(String(stdout ?? "").trim(), maxOut),
      stderr: truncate(String(stderr ?? "").trim(), maxErr),
      timedOut: false,
    };
  } catch (err: unknown) {
    const e = err as NodeJS.ErrnoException & {
      status?: number;
      stdout?: string | Buffer;
      stderr?: string | Buffer;
      signal?: string;
      killed?: boolean;
    };
    const timedOut = e.code === "ETIMEDOUT" || e.killed === true;
    const exitCode =
      typeof e.status === "number"
        ? e.status
        : typeof e.code === "number"
          ? e.code
          : null;
    const stdout = truncate(String(e.stdout ?? "").trim(), maxOut);
    const stderrRaw = truncate(String(e.stderr ?? "").trim(), maxErr);
    const stderr =
      stderrRaw || (e.message ?? "").slice(0, maxErr) || "execFile failed";
    return {
      ok: false,
      exitCode,
      signal: e.signal ?? null,
      stdout,
      stderr,
      timedOut,
    };
  }
}

/** Shorter default timeout for lightweight probes. */
export async function runCommandQuick(
  command: string,
  args: readonly string[],
  options: Omit<RunCommandOptions, "timeoutMs"> & { timeoutMs?: number } = {}
): Promise<RunCommandResult> {
  return runCommand(command, args, { ...options, timeoutMs: options.timeoutMs ?? 4000 });
}

export type RunCommandJsonResult =
  | { ok: true; run: RunCommandResult; value: unknown }
  | { ok: false; run: RunCommandResult; parse: SafeJsonResult };

/**
 * Run command and parse stdout as JSON (guarded). Command must succeed (exit 0)
 * for parse to run; otherwise returns ok:false with run populated.
 */
export async function runCommandJson(
  command: string,
  args: readonly string[],
  options: RunCommandOptions & { parseContext?: string } = {}
): Promise<RunCommandJsonResult> {
  const run = await runCommand(command, args, options);
  if (!run.ok) {
    return {
      ok: false,
      run,
      parse: { ok: false, error: "command did not exit successfully", preview: run.stderr },
    };
  }
  const ctx = options.parseContext ?? `${command} ${args[0] ?? ""}`;
  const parse = safeJsonParse(run.stdout, ctx);
  if (!parse.ok) {
    return { ok: false, run, parse };
  }
  return { ok: true, run, value: parse.value };
}

/** True if `command` appears executable (default probe: `--version`). */
export async function commandAvailable(
  command: string,
  versionArgs: readonly string[] = ["--version"]
): Promise<boolean> {
  const r = await runCommandQuick(command, [...versionArgs], {
    maxStdoutBytes: 512,
    maxStderrBytes: 512,
  });
  return r.ok;
}

/** Optional probe: same as runCommand but intended for best-effort checks. */
export async function maybeCommand(
  command: string,
  args: readonly string[],
  options: RunCommandOptions = {}
): Promise<RunCommandResult> {
  return runCommand(command, args, options);
}
