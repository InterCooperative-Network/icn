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

import {
  daysBetween,
  describeFreshness,
  entriesAfter,
} from "../../src/lib/freshness.ts";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const read = (rel) => fs.readFileSync(path.join(root, rel), "utf-8");

const verified = { oldest: "2026-08-14", newest: "2026-08-14" };
const base = {
  verified,
  proseReviewed: "2026-08-17",
  today: "2026-11-04",
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

  test("the age is measured from the verification date to the build date", () => {
    const f = describeFreshness({ ...base, entries: [] });
    assert.equal(f.verificationAgeDays, 82);
    assert.equal(daysBetween("2026-08-14", "2026-08-14"), 0);
  });

  test("with no entries at all, the old date and its age are still reported", () => {
    const f = describeFreshness({ ...base, entries: [] });
    assert.equal(f.selectedAfterVerification.length, 0);
    assert.equal(f.verificationAgeDays, 82);
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
