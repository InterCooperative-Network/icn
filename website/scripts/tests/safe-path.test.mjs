// safe-path.test.mjs — request paths resolve inside the root, or not at all.
//
//   node --test scripts/tests/
//
// Covers CodeQL alerts 107/108 (js/path-injection) on audit-layout.mjs's
// static server: the containment decision must be reachable without touching
// the filesystem, and must be separator-aware.

import { strict as assert } from "node:assert";
import path from "node:path";
import { test, describe } from "node:test";

import { isWithinRoot, resolveWithinRoot } from "../lib/safe-path.mjs";

const ROOT = "/srv/site/dist";

describe("isWithinRoot", () => {
  test("the root itself is inside the root", () => {
    assert.equal(isWithinRoot(ROOT, ROOT), true);
  });

  test("a child is inside", () => {
    assert.equal(isWithinRoot(ROOT, `${ROOT}/index.html`), true);
    assert.equal(isWithinRoot(ROOT, `${ROOT}/a/b/c.css`), true);
  });

  test("a sibling sharing the root's name prefix is NOT inside", () => {
    // The bug this pins: `"/srv/site/dist-secrets".startsWith("/srv/site/dist")`
    // is true, so the previous plain startsWith check accepted a sibling
    // directory as a child.
    assert.equal(isWithinRoot(ROOT, "/srv/site/dist-secrets/key.pem"), false);
    assert.equal(isWithinRoot(ROOT, "/srv/site/distant"), false);
  });

  test("a parent is not inside", () => {
    assert.equal(isWithinRoot(ROOT, "/srv/site"), false);
    assert.equal(isWithinRoot(ROOT, "/etc/passwd"), false);
  });
});

describe("resolveWithinRoot — ordinary paths", () => {
  const ok = (url, expected) =>
    test(`serves ${JSON.stringify(url)}`, () =>
      assert.equal(resolveWithinRoot(ROOT, url), path.join(ROOT, expected)));

  ok("/", "");
  ok("/index.html", "index.html");
  ok("/docs/archive/README", "docs/archive/README");
  ok("/assets/app.css", "assets/app.css");
  ok("/a%20b/c.png", "a b/c.png");
  ok("//double//slash//", "double/slash");
  ok("/./index.html", "index.html");
  ok("/docs/../index.html", "index.html");
});

describe("resolveWithinRoot — traversal cannot escape", () => {
  // Note the shape of the guarantee: `..` is *clamped*, not rejected. A
  // traversal resolves to a path inside the root that will simply not exist,
  // which is the same answer a browser gets for any other missing asset. The
  // property under test is containment, never "the string looks scary".
  const contained = (url) =>
    test(`cannot escape via ${JSON.stringify(url)}`, () => {
      const got = resolveWithinRoot(ROOT, url);
      assert.ok(
        got === null || isWithinRoot(ROOT, got),
        `expected null or a path inside ${ROOT}, got ${got}`,
      );
      assert.ok(
        got === null || !got.startsWith("/etc/"),
        `reached a real system path: ${got}`,
      );
    });

  contained("/../etc/passwd");
  contained("/../../../../etc/passwd");
  contained("/docs/../../etc/passwd");
  contained("/%2e%2e/%2e%2e/etc/passwd");
  contained("/%2E%2E%2Fetc%2Fpasswd");
  contained("/..%2f..%2fetc%2fpasswd");
  contained("/....//....//etc/passwd");
  contained("\\..\\..\\etc\\passwd");
  contained("/..\\../etc/passwd");
  contained("/../dist-secrets/key.pem");
});

describe("resolveWithinRoot — clamping, not escaping", () => {
  test("a traversal above the root lands inside the root", () => {
    const got = resolveWithinRoot(ROOT, "/../../etc/passwd");
    assert.equal(got, path.join(ROOT, "etc/passwd"));
    assert.equal(isWithinRoot(ROOT, got), true);
  });

  test("a double-encoded traversal stays one literal segment", () => {
    // A server decodes once. `%252e` -> the text `%2e`, which is a filename,
    // not a dot segment — so this must not become `..`.
    const got = resolveWithinRoot(ROOT, "/%252e%252e/x");
    assert.equal(got, path.join(ROOT, "%2e%2e/x"));
  });
});

describe("resolveWithinRoot — malformed input is refused outright", () => {
  test("malformed percent-encoding returns null", () => {
    assert.equal(resolveWithinRoot(ROOT, "/%"), null);
    assert.equal(resolveWithinRoot(ROOT, "/%zz"), null);
    assert.equal(resolveWithinRoot(ROOT, "/%e0%a4%a"), null);
  });

  test("an embedded NUL returns null", () => {
    assert.equal(resolveWithinRoot(ROOT, "/index.html%00.png"), null);
    assert.equal(resolveWithinRoot(ROOT, "/a\0b"), null);
  });

  test("the return value is always absolute or null", () => {
    for (const u of ["/", "/a", "/../..", "/%2e%2e", "//", "/./"]) {
      const got = resolveWithinRoot(ROOT, u);
      assert.ok(got === null || path.isAbsolute(got), `${u} -> ${got}`);
    }
  });
});
