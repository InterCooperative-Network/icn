// serve-dist.mjs — the ephemeral static server that audit-layout.mjs audits.
//
// Extracted from audit-layout.mjs so the containment behaviour can be tested:
// that script is top-level and calls process.exit, so importing it runs a full
// audit. scripts/tests/serve-dist.test.mjs drives this server directly and
// asserts the property that actually failed before — that a request escaping
// the root reaches no filesystem call at all, not merely no response body.

import fs from "node:fs";
import http from "node:http";
import path from "node:path";

import { resolveWithinRoot } from "./safe-path.mjs";

/**
 * Minimal static server for the built site. Enough for an audit: index.html
 * resolution for directory routes, correct content types for the handful of
 * asset kinds Astro emits, and nothing else.
 */
export function serveDist(root) {
  const TYPES = {
    ".html": "text/html; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".svg": "image/svg+xml",
    ".json": "application/json",
    ".xml": "application/xml",
    ".woff2": "font/woff2",
    ".png": "image/png",
    ".ico": "image/x-icon",
  };
  const server = http.createServer((req, res) => {
    const requestPath = (req.url ?? "/").split("?")[0].split("#")[0];

    // Validate and normalise BEFORE any filesystem access. The previous
    // version called fs.existsSync/fs.statSync first and checked containment
    // afterwards, which is the whole defect: refusing to *send* the bytes does
    // not undo having *looked*. resolveWithinRoot touches no filesystem, so it
    // can run first, and it compares on separator boundaries so a sibling
    // directory like `dist-secrets` cannot pass as a child of `dist`.
    let filePath = resolveWithinRoot(root, requestPath);
    if (filePath === null) {
      res.writeHead(403, { "content-type": "text/plain" }).end("forbidden");
      return;
    }

    try {
      // statSync throws for a missing path; the catch below turns that into a
      // 404, so no separate existsSync probe is needed.
      if (fs.statSync(filePath).isDirectory()) {
        // "index.html" contains no traversal, so the result stays inside root.
        filePath = path.join(filePath, "index.html");
      }
      const body = fs.readFileSync(filePath);
      res.writeHead(200, {
        "content-type":
          TYPES[path.extname(filePath)] ?? "application/octet-stream",
      });
      res.end(body);
    } catch {
      res.writeHead(404, { "content-type": "text/plain" }).end("not found");
    }
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () =>
      resolve({ server, port: server.address().port }),
    );
  });
}
