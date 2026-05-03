import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export type ExecSafeResult = {
  ok: boolean;
  code: number | null;
  stdout: string;
  stderr: string;
};

/**
 * Run a subprocess with argv only (no shell). Never throws; missing binaries
 * become ok: false with stderr populated.
 */
export async function execFileNoThrow(
  file: string,
  args: readonly string[],
  options: { cwd?: string; timeoutMs?: number; maxBuffer?: number } = {}
): Promise<ExecSafeResult> {
  try {
    const { stdout, stderr } = await execFileAsync(file, [...args], {
      cwd: options.cwd,
      timeout: options.timeoutMs ?? 10_000,
      maxBuffer: options.maxBuffer ?? 10 * 1024 * 1024,
      encoding: "utf-8",
    });
    return {
      ok: true,
      code: 0,
      stdout: String(stdout ?? "").trim(),
      stderr: String(stderr ?? "").trim(),
    };
  } catch (err: unknown) {
    const e = err as {
      code?: string | number;
      stdout?: string | Buffer;
      stderr?: string | Buffer;
      message?: string;
    };
    const stdout = e.stdout != null ? String(e.stdout).trim() : "";
    const stderrRaw = e.stderr != null ? String(e.stderr).trim() : "";
    const code = typeof e.code === "number" ? e.code : null;
    return {
      ok: false,
      code,
      stdout,
      stderr: stderrRaw || (e.message ?? "exec failed"),
    };
  }
}
