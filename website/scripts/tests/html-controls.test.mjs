// html-controls.test.mjs — the walkthrough stays read-only, structurally.
//
//   node --test "scripts/tests/*.test.mjs"
//
// check-fixture-safety.mjs check 7 asserts that /see-it-work ships no mutation
// affordance. It used to assert that with three hand-written tag regexps, which
// miss controls that are structurally identical to ones they catch — a
// single-quoted attribute, an unquoted one, a label nested inside a <span>.
// Each miss is a working control on a page the checker then calls read-only.
//
// Every "should count" case below is checked against the *old* patterns too, so
// the suite records which ones were genuine misses rather than asserting only
// that the new code works.

import { strict as assert } from "node:assert";
import { describe, test } from "node:test";

import { findMutationControls } from "../lib/html-controls.mjs";

/** The three patterns that shipped before this pass, kept as controls. */
const LEGACY = [
  /<form\b/i,
  /<button[^>]*>\s*(confirm|submit|approve|vote|send)\b/i,
  /<input[^>]*type="submit"/i,
];
const legacyFinds = (html) => LEGACY.some((re) => re.test(html));

const found = (html) => findMutationControls(html).length > 0;

describe("controls that ARE mutation capability", () => {
  // legacyMissed records the old behaviour. Asserting it both ways keeps this
  // from decaying into a tautology if the patterns are ever revived.
  const cases = [
    ["form, plain", "<form><input></form>", false],
    ["form, uppercase", "<FORM><input></FORM>", false],
    [
      "form, attributes and whitespace",
      '<form\n  method="post"\n>x</form>',
      false,
    ],

    ["input submit, double-quoted", '<input type="submit">', false],
    ["input submit, single-quoted", "<input type='submit'>", true],
    ["input submit, unquoted", "<input type=submit>", true],
    ["input submit, spaces around =", '<input type = "submit">', true],
    ["input submit, uppercase value", '<INPUT TYPE="SUBMIT">', false],
    [
      "input submit, attribute order",
      '<input value="Go" type="submit">',
      false,
    ],
    [
      "input image (submits a form)",
      '<input type="image" src="/go.png">',
      true,
    ],
    [
      "input submit labelled by value",
      '<input type="submit" value="Confirm">',
      false,
    ],

    [
      "button type=submit, neutral label",
      '<button type="submit">Go</button>',
      true,
    ],
    [
      "button type=submit, single-quoted",
      "<button type='submit'>Go</button>",
      true,
    ],
    ["button, plain action label", "<button>Confirm</button>", false],
    [
      "button, label nested in a span",
      "<button><span>Confirm</span></button>",
      true,
    ],
    [
      "button, label two levels down",
      "<button><span><b>Approve</b></span></button>",
      true,
    ],
    [
      "button, icon with aria-label",
      '<button aria-label="Approve"><svg></svg></button>',
      true,
    ],
    [
      "button, icon with title",
      '<button title="Send payment"><svg></svg></button>',
      true,
    ],
    ["button, uppercase tag and label", "<BUTTON>APPROVE</BUTTON>", false],
    ["button, space before >", "<button >Vote</button>", false],
    ["button, newline around label", "<button>\n  Submit\n</button>", false],
    [
      "button, inside a template",
      "<template><button>Confirm</button></template>",
      false,
    ],
  ];

  for (const [name, html, legacyMissed] of cases) {
    test(`detects: ${name}`, () => {
      assert.ok(found(html), `not detected: ${html}`);
    });

    test(`control — legacy ${legacyMissed ? "MISSED" : "caught"}: ${name}`, () => {
      assert.equal(
        legacyFinds(html),
        !legacyMissed,
        `the legacy control no longer behaves as recorded for: ${name}`,
      );
    });
  }
});

describe("things that are NOT mutation capability", () => {
  const cases = [
    ["navigation button", "<button>Next</button>"],
    ["disclosure button", "<button>Show details</button>"],
    [
      "the real page's theme toggle",
      '<button id="theme-toggle" aria-label="Toggle theme"><svg></svg></button>',
    ],
    [
      "the real page's nav toggle",
      '<button class="nav-toggle" aria-label="Open menu"><svg></svg></button>',
    ],
    ["text input", '<input type="text" name="q">'],
    ["checkbox", '<input type="checkbox">'],
    ["radio", '<input type="radio">'],
    ["reset input", '<input type="reset">'],
    ["input with no type", "<input>"],
    [
      "prose containing the word submit",
      "<p>Submit the form on the member shell.</p>",
    ],
    ["a heading containing the word approve", "<h2>How approvals work</h2>"],
    [
      "escaped markup in a code sample",
      "<code>&lt;input type=&quot;submit&quot;&gt;</code>",
    ],
    ["a link, however it is worded", '<a href="/contact">Send us a note</a>'],
    ["a div that merely says confirm", "<div>Confirm</div>"],
    ["an empty document", "<html><body></body></html>"],
  ];

  for (const [name, html] of cases) {
    test(`ignores: ${name}`, () => {
      const hits = findMutationControls(html);
      assert.deepEqual(
        hits,
        [],
        `false positive on ${name}: ${JSON.stringify(hits)}`,
      );
    });
  }
});

describe("findings describe what was found", () => {
  test("names the kind and quotes the element", () => {
    const [hit] = findMutationControls('<input type="submit" value="Confirm">');
    assert.equal(hit.why, "submit input");
    assert.match(hit.detail, /input/);
    assert.match(hit.detail, /submit/);
  });

  test("a form and a submit button are reported separately", () => {
    const hits = findMutationControls(
      '<form><button type="submit">Go</button></form>',
    );
    const whys = hits.map((h) => h.why).sort();
    assert.deepEqual(whys, ["<form> element", "submit button"]);
  });

  test("every finding on a page is returned, in document order", () => {
    const hits = findMutationControls(
      "<button>Confirm</button><form></form><button>Approve</button>",
    );
    assert.equal(hits.length, 3);
    assert.equal(hits[1].why, "<form> element");
  });
});

describe("fails closed", () => {
  // The failure mode that matters is not a crash, it is a checker that reports
  // "no mutation controls" because it was handed something it could not read.
  for (const bad of [undefined, null, 42, [], { nope: true }]) {
    test(`refuses ${JSON.stringify(bad) ?? String(bad)} rather than returning []`, () => {
      assert.throws(() => findMutationControls(bad), TypeError);
    });
  }

  test("an already-parsed document is accepted", async () => {
    const { parseHtml } = await import("../lib/html-tree.mjs");
    const doc = parseHtml("<button>Confirm</button>");
    assert.equal(findMutationControls(doc).length, 1);
  });
});

describe("characterisation — word boundaries, deliberately unchanged", () => {
  test("'Resend' does not match 'send'", () => {
    // Preserved from the original pattern list rather than widened here.
    // Widening the vocabulary is a separate decision about what the check
    // asserts, not part of making it read structure instead of text.
    assert.deepEqual(findMutationControls("<button>Resend later</button>"), []);
    assert.deepEqual(
      findMutationControls("<button>Subsequent step</button>"),
      [],
    );
  });
});
