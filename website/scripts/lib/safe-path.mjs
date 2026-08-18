// safe-path.mjs — resolve a request path inside a root directory, or refuse.
//
// ─── The defect this replaces ────────────────────────────────────────────────
//
// audit-layout.mjs's static server did this:
//
//     let filePath = path.join(root, url);
//     if (fs.existsSync(filePath) && fs.statSync(filePath).isDirectory()) { ... }
//     if (!path.resolve(filePath).startsWith(path.resolve(root))) { 403 }
//
// Two problems, and the ordering one is the more interesting:
//
//  1. **The filesystem was touched before the containment check.** CodeQL
//     flagged both `fs.existsSync` and `fs.statSync` as path-injection sinks
//     (alerts 107 and 108) and the ordering is why. Withholding the *response*
//     after the fact does not undo the *access*: a caller still learns whether
//     an arbitrary absolute path exists and whether it is a directory, from
//     response timing and from the 403-vs-404 split. A guard placed after the
//     operation it is meant to guard is not a guard.
//
//  2. **`startsWith` is not containment.** With root `/build/dist`, the path
//     `/build/dist-secrets/key.pem` passes `startsWith("/build/dist")` — it is
//     a sibling directory, not a child. Containment has to be checked on
//     separator boundaries.
//
// This module does the validation with no filesystem access at all, so it can
// run *before* any `fs.*` call rather than after one.

import path from "node:path";

/**
 * True if `target` is `root` itself or lies beneath it.
 *
 * Compares on a separator boundary, so `/a/dist-evil` is not inside `/a/dist`.
 */
export function isWithinRoot(root, target) {
  const r = path.resolve(root);
  const t = path.resolve(target);
  return t === r || t.startsWith(r + path.sep);
}

/**
 * Map a URL path onto an absolute filesystem path inside `root`.
 *
 * Returns `null` for anything that cannot be served, and the caller must treat
 * `null` as a refusal without touching the filesystem.
 *
 * Rejected: malformed percent-encoding, NUL bytes (which truncate paths in
 * some syscall layers), and any result that escapes the root.
 *
 * Traversal is neutralised in URL space before a filesystem path is built.
 * `..` segments are collapsed by `path.posix.normalize` against a leading `/`,
 * which clamps at the root: `/../../etc/passwd` normalises to `/etc/passwd`
 * and is then resolved *inside* `root`. Backslashes are treated as separators
 * too, so a `..\..\` style path cannot smuggle a segment past the collapse.
 *
 * Percent-encoding is decoded exactly once, which is what an HTTP server does.
 * A double-encoded `%252e%252e%252f` therefore decodes to the literal text
 * `%2e%2e%2f`, stays a single ordinary filename segment, and resolves to a
 * file that does not exist rather than to a parent directory.
 *
 * Scope: containment here is lexical. A symlink inside `root` pointing outside
 * it would still resolve, which is the right depth for a server whose root is
 * Astro's own build output on a local or CI machine and whose lifetime is one
 * audit run. It is not a general-purpose static file server.
 *
 * @param {string} root
 * @param {string} urlPath  the path portion of a request URL, still encoded
 * @returns {string|null}
 */
export function resolveWithinRoot(root, urlPath) {
  const rootResolved = path.resolve(root);

  let decoded;
  try {
    decoded = decodeURIComponent(String(urlPath));
  } catch {
    return null; // malformed percent-encoding, e.g. "/%"
  }
  if (decoded.includes("\0")) return null;

  const normalized = path.posix.normalize("/" + decoded.replace(/\\/g, "/"));
  const resolved = path.resolve(rootResolved, "." + normalized);

  return isWithinRoot(rootResolved, resolved) ? resolved : null;
}
