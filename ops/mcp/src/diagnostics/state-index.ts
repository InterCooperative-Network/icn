import { existsSync, statSync } from "node:fs";
import { join } from "node:path";
import type { StateIndexEntry } from "./schema.js";

export type { StateIndexEntry } from "./schema.js";

const ENTRIES: Omit<StateIndexEntry, "absolutePath" | "present" | "kind">[] = [
  { id: "docs/STATE.md", description: "Current project state summary." },
  { id: "docs/PHASE_PROGRESS.md", description: "Phase progress tracking." },
  { id: "docs/ARCHITECTURE.md", description: "System architecture (large canonical doc)." },
  { id: "docs/OVERVIEW.md", description: "High-level overview." },
  { id: "docs/genesis.md", description: "Genesis / origin narrative (if present)." },
  { id: "docs/GOLDEN_PROMPT.md", description: "Agent context bundle (if present)." },
  { id: "docs/adr", description: "Architecture Decision Records directory." },
  { id: "docs/rfcs/README.md", description: "RFC index / process entrypoint." },
  { id: "ops/state/sprint/current.json", description: "Active sprint JSON (orchestration plane)." },
  { id: "ops/state/config/repo-map.json", description: "Repo topology and infra hints for ops tooling." },
  { id: "ops/state/truth/sources.json", description: "Canonical truth spine: source-of-truth map." },
  { id: "AGENTS.md", description: "Agent operating rules for this repository." },
];

export function buildStateIndex(repoRoot: string): { entries: StateIndexEntry[] } {
  const entries: StateIndexEntry[] = [];
  for (const e of ENTRIES) {
    const absolutePath = join(repoRoot, e.id);
    let present = false;
    let kind: "file" | "directory" = "file";
    try {
      if (!existsSync(absolutePath)) {
        present = false;
      } else {
        const st = statSync(absolutePath);
        kind = st.isDirectory() ? "directory" : "file";
        present = true;
      }
    } catch {
      present = false;
    }
    entries.push({
      ...e,
      absolutePath,
      present,
      kind,
    });
  }
  return { entries };
}
