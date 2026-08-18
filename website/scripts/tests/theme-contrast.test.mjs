// theme-contrast.test.mjs — the shared colour tokens meet WCAG AA in both themes.
//
//   node --test scripts/tests/
//
// Why this exists, and why it is a token test rather than a page test.
//
// The rendered audit (audit-layout.mjs) measures overflow, heading outline,
// landmarks, text size and labelling. It does not measure contrast, and it
// runs in one theme. So a light-theme palette could — and did — sit below the
// AA floor across every page while `just website-verify` stayed green: teal
// links at 3.74:1 on white and 3.04:1 on inline-code, the primary CTA at
// 3.81:1, all from three shared tokens.
//
// The defect surface was the token table, not any one page, so that is what
// this pins. Each pair below was observed on a rendered page; the ratios are
// computed from the values actually declared in global.css, so the test fails
// when someone edits a token, not when someone edits prose.
//
// Deliberately NOT a general contrast scanner: it says nothing about
// component-local colours or composited ancestor stacks. Those need a rendered
// pass in both themes — see "follow-up: rendered WCAG contrast audit coverage".

import { strict as assert } from "node:assert";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test, describe } from "node:test";

const here = path.dirname(fileURLToPath(import.meta.url));
const cssPath = path.join(here, "..", "..", "src", "styles", "global.css");
const css = fs.readFileSync(cssPath, "utf-8");

/** Pull one declaration block out of global.css by its selector. */
function block(selector) {
  const start = css.indexOf(`${selector} {`);
  assert.notEqual(start, -1, `global.css no longer contains "${selector} {"`);
  const end = css.indexOf("\n}", start);
  assert.notEqual(end, -1, `unterminated "${selector}" block`);
  return css.slice(start, end);
}

/** Custom properties declared directly in a block. */
function tokens(selector) {
  const out = {};
  for (const m of block(selector).matchAll(/^\s*(--[\w-]+):\s*([^;]+);/gm)) {
    out[m[1]] = m[2].trim();
  }
  return out;
}

const DARK = tokens(":root");
const LIGHT = { ...DARK, ...tokens('[data-theme="light"]') };

/** Resolve var() indirection within a theme, e.g. --code-fg: var(--accent-teal). */
function resolve(theme, value, depth = 0) {
  assert.ok(depth < 8, `var() cycle resolving "${value}"`);
  const m = /^var\((--[\w-]+)\)$/.exec(value.trim());
  if (!m) return value.trim();
  const next = theme[m[1]];
  assert.ok(next !== undefined, `token ${m[1]} is not declared`);
  return resolve(theme, next, depth + 1);
}

/** #rgb / #rrggbb / rgb() / rgba() → {rgb:[r,g,b], a}. */
function parseColor(value) {
  const v = value.trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v);
  if (hex) {
    let h = hex[1];
    if (h.length === 3) h = [...h].map((c) => c + c).join("");
    return {
      rgb: [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)),
      a: 1,
    };
  }
  const fn = /^rgba?\(([^)]+)\)$/i.exec(v);
  assert.ok(fn, `cannot parse colour "${value}"`);
  const parts = fn[1].split(",").map((p) => parseFloat(p));
  return { rgb: parts.slice(0, 3), a: parts.length > 3 ? parts[3] : 1 };
}

/** Composite a possibly-translucent colour over an opaque one. */
function over(fg, bg) {
  return fg.a >= 1
    ? fg.rgb
    : fg.rgb.map((c, i) => Math.round(c * fg.a + bg[i] * (1 - fg.a)));
}

/** WCAG 2.x relative luminance. */
function luminance([r, g, b]) {
  const lin = [r, g, b].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
}

function contrast(theme, fgToken, bgToken, baseToken) {
  const base = parseColor(resolve(theme, theme[baseToken])).rgb;
  const bg = over(parseColor(resolve(theme, theme[bgToken])), base);
  const fg = over(parseColor(resolve(theme, theme[fgToken])), bg);
  const [l1, l2] = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
  return (l1 + 0.05) / (l2 + 0.05);
}

// Foreground token · background token · base it composites over · floor.
// AA is 4.5:1 for body text; 3.0:1 only for >=24px (or >=18.66px bold), and
// none of these pairs are large text — the accent tokens carry 12–16px labels,
// links and code, so every row is held to 4.5.
const PAIRS = [
  ["--accent-teal", "--bg-primary", "--bg-primary", 4.5, "links on the page"],
  ["--accent-teal", "--bg-surface", "--bg-primary", 4.5, "callout labels"],
  ["--accent-teal", "--accent-teal-glow", "--bg-primary", 4.5, "teal badge"],
  ["--code-fg", "--bg-code", "--bg-primary", 4.5, "inline code"],
  ["--badge-amber-fg", "--accent-amber-glow", "--bg-primary", 4.5, "amber badge"],
  ["--text-secondary", "--bg-primary", "--bg-primary", 4.5, "body text"],
  ["--text-muted", "--bg-surface", "--bg-primary", 4.5, "explanatory notes"],
  ["--text-muted", "--bg-card", "--bg-primary", 4.5, "muted text on cards"],
  ["--btn-primary-fg", "--accent-amber", "--bg-primary", 4.5, "primary CTA"],
  [
    "--btn-primary-fg",
    "--accent-amber-bright",
    "--bg-primary",
    4.5,
    "primary CTA hover",
  ],
];

for (const [themeName, theme] of [
  ["dark", DARK],
  ["light", LIGHT],
]) {
  describe(`${themeName} theme meets WCAG AA`, () => {
    for (const [fg, bg, base, floor, what] of PAIRS) {
      test(`${what}: ${fg} on ${bg}`, () => {
        const ratio = contrast(theme, fg, bg, base);
        assert.ok(
          ratio >= floor,
          `${themeName}: ${fg} on ${bg} is ${ratio.toFixed(2)}:1, below the ${floor}:1 floor`,
        );
      });
    }
  });
}

describe("the token table itself", () => {
  test("every token a pair names is declared in both themes", () => {
    for (const [fg, bg, base] of PAIRS) {
      for (const t of [fg, bg, base]) {
        assert.ok(DARK[t] !== undefined, `${t} missing from :root`);
        assert.ok(LIGHT[t] !== undefined, `${t} missing from the light theme`);
      }
    }
  });

  test("the contrast maths agrees with the WCAG reference points", () => {
    // Black on white is exactly 21:1; a colour against itself is exactly 1:1.
    const ref = { "--w": "#fff", "--b": "#000" };
    assert.equal(Math.round(contrast(ref, "--b", "--w", "--w")), 21);
    assert.equal(Math.round(contrast(ref, "--w", "--w", "--w")), 1);
  });
});
