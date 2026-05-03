import { safeJsonParse } from "../diagnostics/safe-json.js";

function readyFromConditions(conditions: unknown): string {
  if (!Array.isArray(conditions)) return "Unknown";
  const ready = conditions.find(
    (c: unknown) => (c as { type?: string }).type === "Ready"
  ) as { status?: string } | undefined;
  return typeof ready?.status === "string" ? ready.status : "Unknown";
}

/**
 * Map `kubectl get pods … -o json` stdout to the compact list shape previously
 * produced by jq (no shell / jq dependency).
 */
export function summarizePodsFromKubectlJsonString(stdout: string): unknown {
  const p = safeJsonParse(stdout, "kubectl get pods -o json");
  if (!p.ok) {
    return { error: p.error, preview: p.preview };
  }
  const doc = p.value;
  if (!doc || typeof doc !== "object") {
    return { error: "kubectl JSON was not an object" };
  }
  const items = (doc as { items?: unknown }).items;
  if (!Array.isArray(items)) {
    return {
      error: "kubectl JSON missing items array",
      keys: Object.keys(doc as object).slice(0, 20),
    };
  }
  return items.map((item: unknown) => {
    const meta = (item as { metadata?: { name?: string; namespace?: string } })
      .metadata;
    const status = (item as {
      status?: { phase?: string; conditions?: unknown[] };
    }).status;
    return {
      name: meta?.name ?? null,
      namespace: meta?.namespace ?? null,
      phase: status?.phase ?? null,
      ready: readyFromConditions(status?.conditions),
    };
  });
}
