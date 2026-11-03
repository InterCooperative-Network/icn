// Test-only module-resolution hook: lets Node's built-in type stripping run this package's
// TypeScript sources, which import their siblings as "./x.js" (the NodeNext convention).
// Used by snapshot-child.ts so a REAL child process can be SIGKILLed mid-export.
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

export async function resolve(specifier, context, next) {
  if (specifier.startsWith(".") && specifier.endsWith(".js") && context.parentURL?.startsWith("file:")) {
    const ts = new URL(specifier.replace(/\.js$/, ".ts"), context.parentURL);
    if (existsSync(fileURLToPath(ts))) return next(ts.href, context);
  }
  return next(specifier, context);
}
