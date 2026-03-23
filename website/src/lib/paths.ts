// Centralized path resolution for the website.
// Handles both `cd website && npm run build` and `npm --prefix website run build` from repo root.

import fs from "node:fs";
import path from "node:path";

export function resolveRepoDocsRoot(): string {
  const cwd = process.cwd();
  const candidates = [
    path.resolve(cwd, "docs"), // repo root runner (npm --prefix website ...)
    path.resolve(cwd, "..", "docs"), // website/ runner (cd website && npm run build)
  ];

  const init = process.env.INIT_CWD;
  if (init) candidates.push(path.resolve(init, "docs"));

  for (const p of candidates) {
    if (fs.existsSync(p)) return p;
  }

  throw new Error(
    `Cannot locate docs/. Tried:\n${candidates.map((c) => `- ${c}`).join("\n")}`,
  );
}

export function resolveRepoDoc(...parts: string[]): string {
  return path.join(resolveRepoDocsRoot(), ...parts);
}
