// html-scripts.test.mjs — inline <script> extraction must not fail open.
//
//   node --test scripts/tests/
//
// The regexp these tests replaced (`/<script\b[^>]*>([\s\S]*?)<\/script>/gi`)
// missed several spellings of a script block that browsers accept. Each miss
// meant check-fixture-safety.mjs never saw that block's contents and reported
// PASS on a page it had not read — CodeQL alert 106, js/bad-tag-filter.

import { strict as assert } from "node:assert";
import { test, describe } from "node:test";

import { extractInlineScripts } from "../lib/html-scripts.mjs";

/**
 * The exact pattern that shipped on b3b7657c, kept as a control.
 *
 * Without it these tests would pass on the old implementation too for the
 * cases it happened to handle, and would not demonstrate that the variants
 * below are the ones that actually broke it.
 */
function legacyExtract(html) {
  return [...html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)].map(
    (m) => m[1],
  );
}

const MARKER = "fetch('/api/live')";

describe("extractInlineScripts — closing-tag variants", () => {
  // Each of these is a valid script element per the HTML5 tree construction
  // rules. The `legacyMisses` flag records which ones the old regexp dropped.
  const cases = [
    { name: "plain", html: `<script>${MARKER}</script>`, legacyMisses: false },
    {
      name: "space before > in end tag",
      html: `<script>${MARKER}</script >`,
      legacyMisses: true,
    },
    {
      name: "newline before > in end tag",
      html: `<script>${MARKER}</script\n>`,
      legacyMisses: true,
    },
    {
      name: "tab before > in end tag",
      html: `<script>${MARKER}</script\t>`,
      legacyMisses: true,
    },
    {
      name: "uppercase end tag with space",
      html: `<SCRIPT>${MARKER}</SCRIPT >`,
      legacyMisses: true,
    },
    {
      name: "mixed case with attribute",
      html: `<ScRiPt type="module">${MARKER}</ScRiPt >`,
      legacyMisses: true,
    },
    {
      // The legacy pattern does not lose this block, it mangles it: `[^>]*`
      // stops at the `>` inside the attribute value, so the captured "body"
      // begins with the attribute leftovers `b">`. Recorded as found, and
      // pinned exactly by the body-fidelity test below.
      name: "attribute value containing >",
      html: `<script data-x="a>b">${MARKER}</script>`,
      legacyMisses: false,
    },
    {
      name: "unterminated block at end of document",
      html: `<html><body><script>${MARKER}`,
      legacyMisses: true,
    },
    {
      name: "whitespace inside the start tag",
      html: `<script\n  type="module"\n>${MARKER}</script>`,
      legacyMisses: false,
    },
  ];

  for (const { name, html, legacyMisses } of cases) {
    test(`finds the body: ${name}`, () => {
      const found = extractInlineScripts(html);
      assert.ok(
        found.some((s) => s.includes(MARKER)),
        `expected the script body to be extracted from: ${JSON.stringify(html)}`,
      );
    });

    test(`control — legacy regexp ${legacyMisses ? "missed" : "found"}: ${name}`, () => {
      const legacyFound = legacyExtract(html).some((s) => s.includes(MARKER));
      // Asserting the control's *actual* behaviour both ways keeps this test
      // honest: if a future edit made the legacy pattern correct, or made one
      // of these variants stop being a miss, this fails loudly instead of
      // quietly turning into a tautology.
      assert.equal(
        legacyFound,
        !legacyMisses,
        `the legacy control no longer behaves as recorded for: ${name}`,
      );
    });
  }
});

describe("extractInlineScripts — body fidelity", () => {
  test("an attribute containing > does not bleed into the body", () => {
    const html = `<script data-x="a>b">${MARKER}</script>`;
    assert.deepEqual(extractInlineScripts(html), [MARKER]);

    // Control: the legacy pattern captured the attribute leftovers as though
    // they were script source. It found the marker, so this was not a
    // fail-open, but any check reading the "body" was reading markup.
    assert.deepEqual(legacyExtract(html), [`b">${MARKER}`]);
  });
});

describe("extractInlineScripts — what must NOT be returned", () => {
  test("external scripts have no body and are skipped", () => {
    const found = extractInlineScripts(
      `<script src="/theme.js"></script><script>${MARKER}</script>`,
    );
    assert.deepEqual(found, [MARKER]);
  });

  test("SRC in any casing is still treated as external", () => {
    assert.deepEqual(extractInlineScripts(`<script SRC="/a.js"></script>`), []);
  });

  test("a script inside a <template> is still inspected", () => {
    const found = extractInlineScripts(
      `<template><script>${MARKER}</script></template>`,
    );
    assert.ok(found.some((s) => s.includes(MARKER)));
  });

  test("text that merely looks like a script tag is not a script", () => {
    const found = extractInlineScripts(
      `<p>write &lt;script&gt;${MARKER}&lt;/script&gt; to embed</p>`,
    );
    assert.deepEqual(found, []);
  });

  test("a commented-out script is not executable and is not returned", () => {
    assert.deepEqual(
      extractInlineScripts(`<!-- <script>${MARKER}</script> -->`),
      [],
    );
  });
});

describe("extractInlineScripts — document order and shape", () => {
  test("returns every inline body, in order", () => {
    assert.deepEqual(
      extractInlineScripts(`<script>a</script><script >b</script >`),
      ["a", "b"],
    );
  });

  test("an empty inline script yields an empty string, not a skip", () => {
    assert.deepEqual(extractInlineScripts(`<script></script>`), [""]);
  });

  test("a page with no scripts yields an empty list", () => {
    assert.deepEqual(extractInlineScripts(`<p>hello</p>`), []);
  });

  test("throws on a non-string so a caller cannot scan 'undefined'", () => {
    assert.throws(() => extractInlineScripts(undefined), TypeError);
  });
});
