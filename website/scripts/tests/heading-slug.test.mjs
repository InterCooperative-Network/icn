// heading-slug.test.mjs — GitHub-compatible anchors, and the safety invariant.
//
//   node --test scripts/tests/
//
// Two jobs, and the first is the reason the file exists:
//
//   1. Pin the slugs that 701 in-page anchors across the docs corpus depend on
//      (#1369). The algorithm has non-obvious rules — one hyphen per space,
//      not per run — and "tidying" them silently breaks every affected TOC.
//   2. Demonstrate that the CodeQL findings on this code (alerts 104 and 105,
//      js/double-escaping and js/incomplete-multi-character-sanitization) do
//      not describe a reachable injection, by asserting the invariant that
//      makes them unreachable rather than asserting it in prose.

import { strict as assert } from "node:assert";
import { describe, test } from "node:test";

import { marked } from "marked";

import {
  createHeadingSlugger,
  slugifyHeading,
} from "../../src/lib/headingSlug.ts";

/** What a heading actually arrives as: marked's inline rendering. */
const rendered = (markdown) => marked.parseInline(markdown);

describe("the id can only ever be [A-Za-z0-9_-]", () => {
  // This is the whole security argument. Whatever the tag strip and the entity
  // pass do or fail to do, the final allowlist removes every `<`, `>`, `"`,
  // `'` and `&`, so the value interpolated into id="..." can neither close the
  // attribute nor open an element.
  const attempts = [
    "<script>alert(1)</script>",
    "<script src=x",
    "<scr<x>ipt>alert(1)",
    "</script >",
    '" onload="alert(1)',
    '" onmouseover=alert(1) x="',
    "'><img src=x onerror=alert(1)>",
    "&lt;script&gt;alert(1)&lt;/script&gt;",
    "&amp;lt;script&amp;gt;",
    "&amp;amp;lt;img src=x&amp;amp;gt;",
    "&#39;&quot;&amp;&lt;&gt;",
    "&#x27;onload&#x27;",
    "<<script>>",
    '<img src=x onerror="alert(1)">',
    "javascript:alert(1)",
    'a" id="b',
    " <script>",
    "<!--<script>-->",
    "<svg/onload=alert(1)>",
  ];

  for (const raw of attempts) {
    test(`safe for ${JSON.stringify(raw)}`, () => {
      for (const input of [raw, rendered(raw)]) {
        const slug = slugifyHeading(input);
        assert.match(
          slug,
          /^[\w-]*$/,
          `slug escaped the allowlist: ${JSON.stringify(slug)}`,
        );
        for (const ch of ["<", ">", '"', "'", "&"]) {
          assert.ok(!slug.includes(ch), `slug contains ${ch}: ${slug}`);
        }
      }
    });
  }

  test("the invariant holds for the whole rendered heading path", () => {
    for (const raw of attempts) {
      const html = `<h2 id="${slugifyHeading(rendered(raw))}">x</h2>`;
      // Exactly the two `<` and the two `"` this template writes itself:
      // nothing was injected through the slug.
      assert.equal((html.match(/</g) ?? []).length, 2);
      assert.equal((html.match(/"/g) ?? []).length, 2);
    }
  });
});

describe("ampersands and entities — the #1369 anchor class", () => {
  const cases = [
    // The heading that named the rule: removing "&" leaves two adjacent
    // spaces, and each space becomes its own hyphen.
    ["5. Storage & Replication", "5-storage--replication"],
    ["Storage & Replication", "storage--replication"],
    ["Q&A", "qa"],
    ["x & y & z", "x--y--z"],
    ["Design & Build & Ship", "design--build--ship"],
    // Entities must not leave their letters behind: "amp", "lt", "quot", "39".
    ["Storage &amp; Replication", "storage--replication"],
    ["&lt;tag&gt; handling", "tag-handling"],
    ["&quot;quoted&quot; thing", "quoted-thing"],
    ["It&#39;s here", "its-here"],
  ];

  for (const [input, expected] of cases) {
    test(`${JSON.stringify(input)} -> ${expected}`, () => {
      assert.equal(slugifyHeading(input), expected);
    });
  }

  test("no entity leaks its own letters into a slug", () => {
    for (const [entity, letters] of [
      ["&amp;", "amp"],
      ["&lt;", "lt"],
      ["&gt;", "gt"],
      ["&quot;", "quot"],
      ["&#39;", "39"],
    ]) {
      assert.ok(
        !slugifyHeading(`A ${entity} B`).includes(letters),
        `${entity} leaked "${letters}"`,
      );
    }
  });

  test("a doubly escaped entity is decoded once, not twice", () => {
    // The js/double-escaping finding (alert 104). The old five sequential
    // passes turned `&amp;lt;` into `&lt;` and then into `<`, which the
    // allowlist then deleted, yielding an empty slug. One pass leaves the
    // literal text `lt;`, and `lt` is what GitHub produces for a heading whose
    // rendered text content is the four characters `&lt;`.
    assert.equal(slugifyHeading("&amp;lt;"), "lt");
    assert.equal(slugifyHeading("&amp;amp;"), "amp");
    assert.equal(slugifyHeading("a &amp;lt; b"), "a-lt-b");
  });
});

describe("GitHub-compatible shape", () => {
  const cases = [
    ["Overview", "overview"],
    ["Getting Started", "getting-started"],
    ["What's Real Now", "whats-real-now"],
    ["1. Introduction", "1-introduction"],
    ["A/B Testing", "ab-testing"],
    ["100% Done", "100-done"],
    ["snake_case_name", "snake_case_name"],
    ["already-hyphenated", "already-hyphenated"],
    ["  padded  ", "padded"],
    ["Multi   Space", "multi---space"],
    ["ADR-0026: Receipts", "adr-0026-receipts"],
  ];
  for (const [input, expected] of cases) {
    test(`${JSON.stringify(input)} -> ${expected}`, () => {
      assert.equal(slugifyHeading(input), expected);
    });
  }

  test("a tab is whitespace and becomes one hyphen", () => {
    assert.equal(slugifyHeading("Tab\tSeparated"), "tab-separated");
  });

  test("each space becomes one hyphen, runs are NOT collapsed", () => {
    // Collapsing runs is the single most tempting "cleanup" here, and it
    // breaks every corpus anchor containing a stripped character.
    assert.equal(slugifyHeading("a  b"), "a--b");
    assert.equal(slugifyHeading("a   b"), "a---b");
  });

  test("inline markup contributes its text, not its tags", () => {
    assert.equal(
      slugifyHeading(rendered("The `config` file")),
      "the-config-file",
    );
    assert.equal(slugifyHeading(rendered("**Bold** heading")), "bold-heading");
    assert.equal(slugifyHeading(rendered("A [link](/x) here")), "a-link-here");
  });

  test("tag stripping runs to a fixed point", () => {
    // A single pass over this reassembles a tag that was not in the input,
    // which is what js/incomplete-multi-character-sanitization describes.
    assert.equal(slugifyHeading("<scr<x>ipt>ok"), "iptok");
  });
});

describe("duplicate headings", () => {
  test("get GitHub's numeric suffixes", () => {
    const slug = createHeadingSlugger();
    assert.equal(slug("Overview"), "overview");
    assert.equal(slug("Overview"), "overview-1");
    assert.equal(slug("Overview"), "overview-2");
    assert.equal(slug("Other"), "other");
    assert.equal(slug("Other"), "other-1");
  });

  test("counters are per-page, not global", () => {
    const a = createHeadingSlugger();
    const b = createHeadingSlugger();
    assert.equal(a("Overview"), "overview");
    assert.equal(b("Overview"), "overview");
  });
});

describe("characterisation — pre-existing behaviour, unchanged by this pass", () => {
  test("non-ASCII letters are dropped (JS \\w is ASCII-only)", () => {
    // GitHub keeps these; this implementation never has. Recorded so the
    // difference is a known, findable decision rather than a surprise, and
    // deliberately left alone — changing it would move live anchors.
    assert.equal(slugifyHeading("CJK 中文 heading"), "cjk--heading");
    assert.equal(slugifyHeading("café"), "caf");
  });
});
