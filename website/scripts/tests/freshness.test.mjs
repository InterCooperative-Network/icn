// freshness.test.mjs — the relationship between the verification date and the
// selected change log on /whats-real-now.
//
//   node --test scripts/tests/
//
// What these pin down:
//   * a stale verification date stays visible and is never advanced by newer
//     log entries;
//   * the log selection can never be reported as exhaustive, and no total is
//     produced from it;
//   * the logic is pure: it does not modify its input;
//   * the generated state files are not modified by the page or the module;
//   * no page presents a merged change as a maturity change.

import { strict as assert } from "node:assert";
import fs from "node:fs";
import path from "node:path";
import { describe, test } from "node:test";
import { fileURLToPath } from "node:url";

import { describeFreshness, entriesAfter } from "../../src/lib/freshness.ts";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const read = (rel) => fs.readFileSync(path.join(root, rel), "utf-8");

const verified = { oldest: "2026-08-14", newest: "2026-08-14" };
const base = {
  verified,
  proseReviewed: "2026-08-17",
  builtOn: "2026-11-04",
};

describe("a stale verification date stays visible", () => {
  test("newer log entries do not move the verification date", () => {
    const f = describeFreshness({
      ...base,
      entries: [
        { date: "2026-11-03", scope: "ops" },
        { date: "2026-10-27", scope: "identity" },
      ],
    });
    assert.equal(f.verifiedNewest, "2026-08-14");
    assert.equal(f.verifiedOldest, "2026-08-14");
  });

  test("the build date is carried as an absolute date, never as an age", () => {
    const f = describeFreshness({ ...base, entries: [] });
    assert.equal(f.builtOn, "2026-11-04");
    for (const key of Object.keys(f)) {
      assert.ok(!/age|days|elapsed/i.test(key), `elapsed-time field: ${key}`);
    }
    assert.ok(
      !/days/i.test(
        read("src/lib/freshness.ts").replace(/^\s*(\/\/|\*|\/\*).*$/gm, ""),
      ),
    );
  });

  test("with no entries at all, the old date is still reported", () => {
    const f = describeFreshness({ ...base, entries: [] });
    assert.equal(f.selectedAfterVerification.length, 0);
    assert.equal(f.verifiedNewest, "2026-08-14");
  });

  test("uniform vs. spread verification dates are distinguished", () => {
    assert.equal(
      describeFreshness({ ...base, entries: [] }).verificationUniform,
      true,
    );
    const spread = describeFreshness({
      ...base,
      verified: { oldest: "2026-03-17", newest: "2026-08-14" },
      entries: [],
    });
    assert.equal(spread.verificationUniform, false);
  });
});

describe("the selection is never exhaustive", () => {
  test("exhaustive is the literal false", () => {
    const f = describeFreshness({ ...base, entries: [{ date: "2026-10-01" }] });
    assert.strictEqual(f.exhaustive, false);
  });

  test("the result carries no total or count-since-review field", () => {
    const f = describeFreshness({ ...base, entries: [{ date: "2026-10-01" }] });
    for (const key of Object.keys(f)) {
      assert.ok(
        !/total|changesSince|count/i.test(key),
        `unexpected total-like field: ${key}`,
      );
    }
  });
});

describe("entriesAfter", () => {
  const entries = [
    { date: "2026-08-13" },
    { date: "2026-08-14" },
    { date: "2026-08-15" },
  ];

  test("is strictly after: a change on the verification day is not after it", () => {
    assert.deepEqual(entriesAfter(entries, "2026-08-14"), [
      { date: "2026-08-15" },
    ]);
  });

  test("does not modify its input", () => {
    const frozen = Object.freeze(entries.map((e) => Object.freeze({ ...e })));
    const before = JSON.stringify(frozen);
    entriesAfter(frozen, "2026-08-14");
    describeFreshness({ ...base, entries: frozen });
    assert.equal(JSON.stringify(frozen), before);
  });

  test("a malformed date fails loudly instead of matching nothing", () => {
    assert.throws(() => entriesAfter(entries, "14/08/2026"), /ISO date/);
    assert.throws(
      () => entriesAfter([{ date: "soon" }], "2026-08-14"),
      /ISO date/,
    );
    assert.throws(
      () =>
        describeFreshness({ ...base, proseReviewed: "August", entries: [] }),
      /ISO date/,
    );
  });
});

describe("areas touched", () => {
  test("lists distinct scopes of entries after verification only, sorted", () => {
    const f = describeFreshness({
      ...base,
      entries: [
        { date: "2026-10-27", scope: "identity" },
        { date: "2026-10-28", scope: "tools" },
        { date: "2026-10-29", scope: "identity" },
        { date: "2026-07-01", scope: "before-the-check" },
        { date: "2026-10-30" },
      ],
    });
    assert.deepEqual(f.areasTouched, ["identity", "tools"]);
  });
});

describe("the generated projections are left alone", () => {
  test("running the freshness logic does not change the generated files", () => {
    const files = [
      "src/data/project-state.generated.json",
      "src/data/build-log.generated.json",
    ].filter((f) => fs.existsSync(path.join(root, f)));
    assert.ok(files.length > 0, "run `npm run generate` first");

    const before = files.map(read);
    const log = JSON.parse(read("src/data/build-log.generated.json"));
    describeFreshness({ ...base, entries: log.entries });
    assert.deepEqual(files.map(read), before);
  });

  test("the module does not import or write canonical state or the generated files", () => {
    const src = read("src/lib/freshness.ts");
    assert.ok(!/\bimport\b[^;]*\.(json|toml)/.test(src));
    assert.ok(!/writeFile|fs\./.test(src));
    assert.ok(!/status\.toml/.test(src.replace(/^\s*\/\/.*$/gm, "")));
  });
});

describe("pages cannot turn a merged change into a maturity claim", () => {
  const wrn = read("src/pages/whats-real-now.astro");

  test("/whats-real-now still renders the verification date from the projection", () => {
    assert.match(wrn, /projectState\.newestVerified/);
    assert.match(wrn, /freshness\.verifiedNewest/);
  });

  test("/whats-real-now separates tested from operating", () => {
    assert.match(wrn, /Operating today/);
    assert.match(wrn, /no production deployment claim and no live federation/);
  });

  test("/whats-real-now labels the log as a selection, not a total", () => {
    assert.match(
      wrn,
      /a selection, not every\s+change and not a count of changes/,
    );
    assert.match(wrn, /It is a selection, not a complete list/);
  });

  test("/whats-real-now never derives a band or evidence class from the log", () => {
    // The log is only ever used for display and for describeFreshness.
    const uses = wrn.match(/buildLog\.[a-zA-Z]+/g) ?? [];
    for (const u of uses) {
      assert.ok(
        [
          "buildLog.entries",
          "buildLog.scanned",
          "buildLog.newestEntryDate",
        ].includes(u),
        `unexpected use of ${u}`,
      );
    }
    assert.ok(!/band\s*=\s*.*buildLog|buildLog.*\.band/.test(wrn));
  });

  test("the homepage does not derive a change count from the capped log", () => {
    const home = read("src/pages/index.astro");
    assert.ok(!/build-log\.generated/.test(home));
    assert.ok(!/changesSinceReview/.test(home));
  });

  test("/for-cooperatives does not derive a change count from the capped log", () => {
    const coop = read("src/pages/for-cooperatives.astro");
    assert.ok(!/build-log\.generated/.test(coop));
  });
});

// ─── One readiness account ───────────────────────────────────────────────────
//
// /whats-real-now owns the verified maturity account (PUBLIC_SITE_IA.md §3).
// The homepage and /for-cooperatives link to it; they must not keep a
// competing subsystem count or band summary. These assertions run against the
// BUILT HTML as well as the source, because a source check alone passes while
// a stale dist/ still serves the old text. The built-output tests fail, rather
// than pass, when dist/ is older than the sources it was built from.

const COMPETING = [
  /\b\d+\s+of\s+\d+\s+subsystems?\b/i,
  /\bsubsystems?\s+(were|are)\s+in\s+the\b/i,
  /strongest\s+maturity\s+band/i,
  /in\s+the\s+weakest/i,
];

function newestMtime(dir, exts) {
  let newest = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) newest = Math.max(newest, newestMtime(p, exts));
    else if (exts.some((e) => entry.name.endsWith(e)))
      newest = Math.max(newest, fs.statSync(p).mtimeMs);
  }
  return newest;
}

function builtText(rel) {
  const html = read(path.join("dist", rel));
  return html
    .replace(/<(script|style)[\s\S]*?<\/\1>/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ");
}

describe("the homepage and /for-cooperatives keep no competing readiness summary", () => {
  const home = read("src/pages/index.astro");
  const coop = read("src/pages/for-cooperatives.astro");

  test("neither page source reads subsystem counts or bands", () => {
    for (const [name, src] of [
      ["index", home],
      ["for-cooperatives", coop],
    ]) {
      assert.ok(
        !/subsystemsInBand|projectState|project-state\.generated/.test(src),
        `${name} reads project state`,
      );
      assert.ok(!/MaturityBadge/.test(src), `${name} renders a maturity badge`);
    }
  });

  test("both still send the reader to /whats-real-now", () => {
    assert.match(home, /href="\/whats-real-now"/);
    assert.match(coop, /href="\/whats-real-now"/);
  });

  test("neither page is an empty redirect: both keep substantial explanation", () => {
    assert.ok(home.length > 8000, "homepage lost its explanation");
    assert.match(coop, /What would your cooperative need to change\?/);
    assert.match(coop, /What is missing\?/);
  });

  test("dist/ is not older than the sources it must reflect", (t) => {
    if (!fs.existsSync(path.join(root, "dist/index.html"))) {
      t.skip("no dist/; run `npm run build` first");
      return;
    }
    const built = Math.min(
      fs.statSync(path.join(root, "dist/index.html")).mtimeMs,
      fs.statSync(path.join(root, "dist/for-cooperatives/index.html")).mtimeMs,
      fs.statSync(path.join(root, "dist/whats-real-now/index.html")).mtimeMs,
    );
    const sources = newestMtime(path.join(root, "src"), [
      ".astro",
      ".ts",
      ".css",
    ]);
    assert.ok(
      built >= sources,
      "dist/ is older than src/ — run `npm run build` before trusting built-output tests",
    );
  });

  for (const [name, rel] of [
    ["homepage", "index.html"],
    ["/for-cooperatives", "for-cooperatives/index.html"],
  ]) {
    test(`built ${name} carries no competing subsystem count or band summary`, (t) => {
      if (!fs.existsSync(path.join(root, "dist", rel))) {
        t.skip("no dist/; run `npm run build` first");
        return;
      }
      const text = builtText(rel);
      for (const re of COMPETING)
        assert.ok(!re.test(text), `${name} matches ${re}`);
      assert.match(text, /What's real now/);
    });
  }

  test("built /whats-real-now still carries the dated account and no elapsed-days figure", (t) => {
    if (!fs.existsSync(path.join(root, "dist/whats-real-now/index.html"))) {
      t.skip("no dist/; run `npm run build` first");
      return;
    }
    const text = builtText("whats-real-now/index.html");
    assert.match(text, /Checked, changed, and not yet re-checked/);
    assert.match(text, /2026-08-14/);
    assert.ok(
      !/\b\d+\s+days\s+before\b/.test(text),
      "elapsed-days figure is back",
    );
  });
});

// ─── Tranche 4: compaction must not delete evidence ──────────────────────────
//
// The phone layout folds gap lists and older log entries into native <details>.
// That is only acceptable if every gap and every selected change is still in
// the HTML a no-JavaScript reader receives. These run against the built page.

const decode = (t) =>
  t
    .replace(/&#x27;|&#39;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
const norm = (t) => decode(t).replace(/\s+/g, " ").trim();

describe("collapsing the phone layout deletes no evidence", () => {
  const have = fs.existsSync(path.join(root, "dist/whats-real-now/index.html"));

  test("every recorded gap is present in the built page", (t) => {
    if (!have) return t.skip("no dist/; run `npm run build` first");
    const html = norm(builtText("whats-real-now/index.html"));
    const state = JSON.parse(read("src/data/project-state.generated.json"));
    let n = 0;
    for (const s of state.subsystems)
      for (const gap of s.gaps) {
        n++;
        assert.ok(
          html.includes(norm(gap)),
          `missing gap (${s.id}): ${gap.slice(0, 60)}`,
        );
      }
    assert.ok(n > 0, "no gaps to check; the test would be vacuous");
  });

  test("every gap list sits in a native <details>, not behind script", (t) => {
    if (!have) return t.skip("no dist/; run `npm run build` first");
    const html = read("dist/whats-real-now/index.html");
    const lists = (html.match(/<ul class="subsystem-gaps/g) ?? []).length;
    const wrapped = (
      html.match(
        /<details class="subsystem-gap-details[^>]*>\s*<summary[\s\S]*?<\/summary>\s*<ul class="subsystem-gaps/g,
      ) ?? []
    ).length;
    assert.ok(lists > 0);
    assert.equal(wrapped, lists);
  });

  test("every selected change is present, including those folded away", (t) => {
    if (!have) return t.skip("no dist/; run `npm run build` first");
    const html = read("dist/whats-real-now/index.html");
    const log = JSON.parse(read("src/data/build-log.generated.json"));
    assert.equal(
      (html.match(/class="wrn-log-item"/g) ?? []).length,
      log.entries.length,
    );
    assert.ok(
      log.entries.length > 5,
      "nothing is folded; the test would be vacuous",
    );
  });

  test("every subsystem has an anchor and a link from the short version", (t) => {
    if (!have) return t.skip("no dist/; run `npm run build` first");
    const html = read("dist/whats-real-now/index.html");
    const state = JSON.parse(read("src/data/project-state.generated.json"));
    for (const s of state.subsystems) {
      assert.ok(html.includes(`id="sub-${s.id}"`), `no row anchor for ${s.id}`);
      assert.ok(
        html.includes(`href="#sub-${s.id}"`),
        `no short-version link for ${s.id}`,
      );
    }
  });

  test("the short version is derived from the projection and makes no comparative claims", () => {
    const wrn = read("src/pages/whats-real-now.astro");
    assert.match(wrn, /subsystemsInBand\(band\)/);
    assert.ok(
      !/moving fastest|trailing the substrate|load-bearing does not/i.test(
        wrn.replace(/^\s*\/\/.*$/gm, ""),
      ),
    );
  });
});

describe("the homepage action says what it is", () => {
  test("the primary action is not 'See ICN work' and the page says the walkthrough is fictional", (t) => {
    if (!fs.existsSync(path.join(root, "dist/index.html")))
      return t.skip("no dist/");
    const text = builtText("index.html");
    assert.ok(!/\bSee ICN work\b/.test(text), "old CTA label is back");
    assert.match(text, /Follow one decision/);
    assert.match(text, /A fictional walkthrough, not a live system/);
  });
});

describe("AdoptionPath readiness labels are not stronger than the evidence", () => {
  const src = read("src/components/AdoptionPath.astro");
  const stages = [
    ...src.matchAll(/n: '(\d\d)',[\s\S]*?readiness: '(\w+)'/g),
  ].map((m) => [m[1], m[2]]);

  test("six stages are parsed", () => {
    assert.equal(stages.length, 6);
  });

  test("only stage 01 is 'today'; nothing past description is 'building'", () => {
    const by = Object.fromEntries(stages);
    assert.equal(by["01"], "today");
    // 02 describes an institution using implemented entity/governance parts.
    // 03-06 need tooling or a production deployment that does not exist.
    for (const n of ["03", "04", "05", "06"])
      assert.equal(by[n], "direction", `stage ${n}`);
  });

  test("the footer does not claim a runnable production loop", () => {
    assert.ok(!/both rest on implemented\s+subsystems/.test(src));
    assert.match(src, /non-production rehearsal/);
  });

  test("/for-cooperatives says the labels are editorial, not maturity scores", () => {
    const coop = read("src/pages/for-cooperatives.astro");
    assert.match(coop, /editorial reading/);
    assert.match(coop, /not maturity scores/);
  });
});

describe("in-page links do not land under the fixed header", () => {
  test("anchored rows and headings carry a scroll offset", () => {
    const table = read("src/components/SubsystemTable.astro");
    const wrn = read("src/pages/whats-real-now.astro");
    assert.match(table, /tr\[id\]\s*\{[^}]*scroll-margin-top:\s*calc\(64px/);
    assert.match(wrn, /h2\[id\]\s*\{[^}]*scroll-margin-top:\s*calc\(64px/);
    const coop = read("src/pages/for-cooperatives.astro");
    assert.match(coop, /h2\[id\]\s*\{[^}]*scroll-margin-top:\s*calc\(64px/);
  });
});
