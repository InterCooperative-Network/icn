// serve-dist.test.mjs — the audit's static server, driven end to end.
//
//   node --test scripts/tests/
//
// safe-path.test.mjs pins the containment function. This file pins the thing
// that actually regressed: the *order* in which the server consults it, and
// the fact that the ephemeral-server contract (loopback, ephemeral port,
// index.html for directories) still holds after the fix.

import { strict as assert } from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, before, describe, test } from "node:test";

import { serveDist } from "../lib/serve-dist.mjs";

let root, parent, base, server;
const SENTINEL = "sibling-file-contents-that-must-never-be-served";

before(async () => {
  parent = fs.mkdtempSync(path.join(os.tmpdir(), "serve-dist-"));
  root = path.join(parent, "dist");
  fs.mkdirSync(path.join(root, "sub"), { recursive: true });
  fs.writeFileSync(path.join(root, "index.html"), "<h1>home</h1>");
  fs.writeFileSync(path.join(root, "sub", "index.html"), "<h1>sub</h1>");
  fs.writeFileSync(path.join(root, "app.css"), "body{}");

  // A sibling directory whose name starts with the root's name. This is the
  // shape that defeated the old `startsWith` containment check.
  fs.mkdirSync(path.join(parent, "dist-secrets"), { recursive: true });
  fs.writeFileSync(path.join(parent, "dist-secrets", "key.pem"), SENTINEL);
  fs.writeFileSync(path.join(parent, "secret.txt"), SENTINEL);

  const started = await serveDist(root);
  server = started.server;
  base = `http://127.0.0.1:${started.port}`;
});

after(() => server?.close());

describe("serves the build output", () => {
  test("a file", async () => {
    const res = await fetch(`${base}/index.html`);
    assert.equal(res.status, 200);
    assert.equal(res.headers.get("content-type"), "text/html; charset=utf-8");
    assert.equal(await res.text(), "<h1>home</h1>");
  });

  test("a directory route resolves to index.html", async () => {
    for (const url of ["/", "/sub", "/sub/"]) {
      const res = await fetch(`${base}${url}`);
      assert.equal(res.status, 200, `${url} should serve`);
      assert.match(await res.text(), /<h1>(home|sub)<\/h1>/);
    }
  });

  test("content types are mapped by extension", async () => {
    const res = await fetch(`${base}/app.css`);
    assert.equal(res.headers.get("content-type"), "text/css; charset=utf-8");
  });

  test("a missing file is 404, not a crash", async () => {
    assert.equal((await fetch(`${base}/nope.html`)).status, 404);
  });

  test("the server is loopback-only on an ephemeral port", () => {
    const addr = server.address();
    assert.equal(addr.address, "127.0.0.1");
    assert.ok(addr.port > 0);
  });
});

describe("never serves outside the root", () => {
  const cases = [
    "/../secret.txt",
    "/../../secret.txt",
    "/sub/../../secret.txt",
    "/%2e%2e/secret.txt",
    "/..%2fsecret.txt",
    "/../dist-secrets/key.pem",
    "/..%2fdist-secrets%2fkey.pem",
  ];

  for (const url of cases) {
    test(`refuses ${url}`, async () => {
      const res = await fetch(`${base}${url}`);
      const body = await res.text();
      assert.notEqual(res.status, 200, `${url} was served`);
      assert.ok(!body.includes(SENTINEL), `${url} leaked the secret`);
    });
  }

  test("control — the old containment check would have served the sibling", () => {
    // Reproduces the composition that shipped on b3b7657c: join first, then
    // test containment with a bare startsWith.
    const oldPath = path.join(root, "/../dist-secrets/key.pem");
    assert.equal(oldPath, path.join(parent, "dist-secrets", "key.pem"));
    assert.equal(
      path.resolve(oldPath).startsWith(path.resolve(root)),
      true,
      "the sibling-prefix bug is the premise of this regression test",
    );
    // …and it holds a real secret, so that check was not academic.
    assert.equal(fs.readFileSync(oldPath, "utf-8"), SENTINEL);
  });
});

describe("the filesystem is not touched before containment is decided", () => {
  /**
   * Run `fn` with the fs calls this server makes recorded.
   *
   * Patching the properties of the shared `node:fs` default export works
   * because serve-dist.mjs holds the same object, and it is the only way to
   * observe *whether* a call happened rather than only what it returned.
   */
  async function recordFsPaths(fn) {
    const seen = [];
    const originals = {
      statSync: fs.statSync,
      readFileSync: fs.readFileSync,
      existsSync: fs.existsSync,
    };
    for (const name of Object.keys(originals)) {
      fs[name] = (p, ...rest) => {
        seen.push(String(p));
        return originals[name](p, ...rest);
      };
    }
    try {
      await fn();
    } finally {
      Object.assign(fs, originals);
    }
    return seen;
  }

  test("a traversal reaches no path outside the root", async () => {
    const seen = await recordFsPaths(async () => {
      const res = await fetch(`${base}/../../../../etc/passwd`);
      await res.text();
    });

    assert.ok(seen.length > 0, "expected the server to consult the filesystem");
    for (const p of seen) {
      const abs = path.resolve(p);
      assert.ok(
        abs === path.resolve(root) ||
          abs.startsWith(path.resolve(root) + path.sep),
        `fs was called on a path outside the root: ${p}`,
      );
    }

    // Control: the old code built the path with path.join, which normalises a
    // traversal all the way out of the root, and handed *that* straight to
    // fs.existsSync before any containment check ran. Withholding the response
    // afterwards did not undo the access.
    assert.equal(path.join(root, "/../../../../etc/passwd"), "/etc/passwd");
  });

  test("a refused request touches the filesystem not at all", async () => {
    let status;
    const seen = await recordFsPaths(async () => {
      const res = await fetch(`${base}/%00`);
      status = res.status;
      await res.text();
    });
    assert.equal(status, 403);
    assert.deepEqual(
      seen,
      [],
      "a refused request must not reach the filesystem",
    );
  });
});
