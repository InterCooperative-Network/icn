/** Parse JSON from external command output without throwing. */

export type SafeJsonResult =
  | { ok: true; value: unknown }
  | { ok: false; error: string; preview?: string };

export function safeJsonParse(text: string, context: string): SafeJsonResult {
  const trimmed = text.trim();
  if (!trimmed) {
    return { ok: false, error: `${context}: empty output` };
  }
  try {
    return { ok: true, value: JSON.parse(trimmed) as unknown };
  } catch (e) {
    return {
      ok: false,
      error: `${context}: invalid JSON (${e instanceof Error ? e.message : String(e)})`,
      preview: trimmed.slice(0, 400),
    };
  }
}
