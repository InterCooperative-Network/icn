#!/usr/bin/env node
// audit-layout.mjs — mechanical accessibility and responsive checks.
//
//   node scripts/audit-layout.mjs [--profile pr|full] [--json <path>] [--base <url>]
//
// Serves `dist/` itself on an ephemeral port and audits that. Pass --base to
// audit an already-running server instead. Exits non-zero if any check fails.
//
// Serving in-process rather than orchestrating `astro preview` in the shell is
// deliberate: a backgrounded preview needs a readiness poll, a trap to clean
// up, and a fixed port that conflicts with whatever the developer already has
// running. One process that owns its own server has none of those problems and
// behaves identically on a laptop and on a CI runner.
//
//   --profile pr     7 pages x 3 widths — the PR gate. ~15s.
//   --profile full   12 pages x 5 widths — the deep sweep. ~40s. (default)
//   --json <path>    also write a machine-readable report, for CI artifacts
//                    and for agents that would rather parse than read.
//
// ─── Why this exists ─────────────────────────────────────────────────────────
//
// No CI job builds, type-checks, or tests `website/` on a pull request — the
// deploy workflow is push-to-main only. That makes local checks the entire
// safety net, and the properties below are exactly the ones that are tedious to
// verify by hand across five viewports and twelve pages, and therefore the ones
// that quietly regress.
//
// ─── Why not Playwright ──────────────────────────────────────────────────────
//
// Playwright lives in web/pilot-ui and is not a website/ dependency. Adding it
// here would mean a browser download and a large dependency tree for a handful
// of assertions. This drives the Chrome already on the machine over the
// DevTools Protocol using Node's built-in WebSocket — no new dependencies at
// all, which is what website/.claude/rules/astro-conventions.md asks for.
//
// ─── What it checks ──────────────────────────────────────────────────────────
//
//   1. No horizontal overflow at any tested width (#1740). The single most
//      common responsive regression, and invisible until someone opens the
//      site on a phone.
//   2. Exactly one <h1>, and no skipped heading levels — the outline a screen
//      reader navigates by.
//   3. Landmarks present: banner/nav, main, contentinfo.
//   4. Interactive targets meet a minimum height (44px per
//      docs/design-language/accessibility.md §3.7; inline links inside prose
//      are exempt, since padding a link inside a sentence breaks the line box).
//   5. No text below 12px (§3.9).
//   6. Every image has alt text; every meaningful SVG is labelled or hidden.
//   7. Under prefers-reduced-motion, nothing is left animating and no content
//      is stranded invisible.
//
// Check 7 is here rather than in the manual pass because the failure it
// catches is not a comfort preference — it is content loss. The reveal system
// hides elements at opacity 0 and reveals them from script; if reduced-motion
// handling regresses, a reader who has asked for less motion gets a page with
// missing sections and no indication anything is wrong. Computed styles under
// an emulated media feature are deterministic, so the check is stable.
//
// It does NOT replace a human pass. Contrast, focus visibility, screen-reader
// comprehension, and whether the page makes sense are not checkable here.

import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";

import { serveDist } from "./lib/serve-dist.mjs";

const websiteRootDir = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

const argv = process.argv.slice(2);
function flag(name, fallback) {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? argv[i + 1] : fallback;
}

const EXPLICIT_BASE = flag("base", null);
const PROFILE = flag("profile", "full");
const JSON_OUT = flag("json", null);
const SERVE_DIR = path.resolve(websiteRootDir, flag("serve", "dist"));

/**
 * Two matrices.
 *
 * `pr` is the subset that covers every distinct layout archetype on the site —
 * a marketing page, a long-form narrative, the walkthrough, the generated
 * state table, the docs landing, an archive page, and a rendered markdown
 * document — at the three widths where the layout actually changes. Adding the
 * remaining pages mostly re-tests the same CSS.
 *
 * `full` is the complete sweep, for the scheduled run.
 */
const PAGE_SETS = {
  pr: [
    "/",
    "/what-is-icn",
    "/how-it-works",
    "/see-it-work",
    "/whats-real-now",
    "/docs",
    "/docs/archive",
  ],
  full: [
    "/",
    "/what-is-icn",
    "/why-icn",
    "/how-it-works",
    "/see-it-work",
    "/whats-real-now",
    "/for-cooperatives",
    "/get-involved",
    "/glossary",
    "/docs",
    "/docs/archive",
    "/docs/archive/README",
  ],
};

const WIDTH_SETS = {
  // 320 catches min-width failures, 768 catches the tablet breakpoint edge
  // (where several grids change), 1280 catches the desktop layout.
  pr: [320, 768, 1280],
  full: [320, 375, 768, 1280, 1600],
};

if (!PAGE_SETS[PROFILE]) {
  console.error(`[audit] unknown profile "${PROFILE}". Use pr or full.`);
  process.exit(2);
}

const PAGES = PAGE_SETS[PROFILE];
const WIDTHS = WIDTH_SETS[PROFILE];

/**
 * Find a Chrome or Chromium binary.
 *
 * The previous implementation was `[pathA, pathB].find(Boolean)`, which
 * returns the first truthy *string* rather than the first path that exists —
 * so it always resolved to one hardcoded location whether or not anything was
 * there, and failed opaquely everywhere else.
 *
 * Order: an explicit CHROME_BIN, then names on PATH (which is how a developer
 * machine and most CI images actually expose it), then the usual install
 * locations. GitHub's ubuntu runners ship Chrome, so nothing is downloaded.
 */
function findChrome() {
  if (process.env.CHROME_BIN) {
    if (fs.existsSync(process.env.CHROME_BIN)) return process.env.CHROME_BIN;
    throw new Error(
      `CHROME_BIN is set to "${process.env.CHROME_BIN}" but no file exists there.`,
    );
  }

  const names = [
    "google-chrome-stable",
    "google-chrome",
    "chromium-browser",
    "chromium",
    "chrome",
  ];
  const dirs = (process.env.PATH ?? "").split(path.delimiter).filter(Boolean);
  for (const name of names) {
    for (const dir of dirs) {
      const candidate = path.join(dir, name);
      try {
        fs.accessSync(candidate, fs.constants.X_OK);
        return candidate;
      } catch {
        /* keep looking */
      }
    }
  }

  const wellKnown = [
    "/opt/google/chrome/chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/snap/bin/chromium",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  ];
  for (const candidate of wellKnown) {
    if (fs.existsSync(candidate)) return candidate;
  }

  throw new Error(
    "No Chrome or Chromium binary found.\n" +
      `  Tried on PATH: ${names.join(", ")}\n` +
      `  Tried directly: ${wellKnown.join(", ")}\n` +
      "  Set CHROME_BIN to an explicit path, or install Chrome. This audit\n" +
      "  deliberately does not download a browser.",
  );
}

const PORT = 9333;

function launchChrome() {
  const child = spawn(
    findChrome(),
    [
      "--headless=new",
      `--remote-debugging-port=${PORT}`,
      "--no-sandbox",
      "--disable-gpu",
      "--disable-dev-shm-usage",
      // The site links Google Fonts. Blocking outbound font requests keeps the
      // audit hermetic and fast; it does not affect layout width, which is what
      // we measure, because the fallback stack is metric-similar enough for the
      // overflow question. Text-size checks read computed styles, not glyphs.
      "--host-resolver-rules=MAP fonts.googleapis.com 127.0.0.1,MAP fonts.gstatic.com 127.0.0.1",
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  return child;
}

async function cdpTarget() {
  for (let i = 0; i < 50; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json/new?about:blank`, {
        method: "PUT",
      });
      if (res.ok) return await res.json();
    } catch {
      /* not up yet */
    }
    await sleep(200);
  }
  throw new Error("Chrome did not expose a DevTools endpoint");
}

/** Minimal CDP client over the built-in WebSocket. */
class Session {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      const resolver = this.pending.get(msg.id);
      if (resolver) {
        this.pending.delete(msg.id);
        msg.error
          ? resolver.reject(new Error(msg.error.message))
          : resolver.resolve(msg.result);
      }
    });
  }

  send(method, params = {}) {
    const id = ++this.id;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const { result, exceptionDetails } = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (exceptionDetails) {
      throw new Error(
        exceptionDetails.text +
          " " +
          (exceptionDetails.exception?.description ?? ""),
      );
    }
    return result.value;
  }
}

/**
 * The audit, as a string evaluated in the page. Kept as one expression so the
 * whole result crosses the protocol boundary in a single round trip — the
 * per-call overhead is what makes a chatty version take minutes.
 */
const AUDIT_EXPR = `(() => {
  const d = document.documentElement;
  const vw = d.clientWidth;
  const sw = d.scrollWidth;

  // An element that sticks out inside an overflow:auto ancestor is correctly
  // contained — it scrolls its own container, not the page. Reporting those
  // buries the actual offender, so walk up and skip them.
  const inScrollContainer = (el) => {
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      const ov = getComputedStyle(p).overflowX;
      if (ov === 'auto' || ov === 'scroll' || ov === 'hidden') return true;
    }
    return false;
  };

  const overflow = [];
  if (sw > vw + 1) {
    for (const el of document.body.querySelectorAll('*')) {
      const r = el.getBoundingClientRect();
      if (r.right > vw + 1 && r.width > 0 && !inScrollContainer(el)) {
        overflow.push(el.tagName.toLowerCase() + (el.className ? '.' + String(el.className).trim().split(/\\s+/)[0] : '') + ' right=' + Math.round(r.right) + ' w=' + Math.round(r.width));
        if (overflow.length >= 3) break;
      }
    }
  }

  // Heading outline.
  const headings = [...document.querySelectorAll('h1,h2,h3,h4,h5,h6')]
    .filter(h => h.offsetParent !== null || h.getClientRects().length)
    .map(h => Number(h.tagName[1]));
  const h1Count = headings.filter(n => n === 1).length;
  const skips = [];
  for (let i = 1; i < headings.length; i++) {
    if (headings[i] - headings[i-1] > 1) skips.push('h' + headings[i-1] + ' -> h' + headings[i]);
  }

  // Landmarks.
  const landmarks = {
    nav: !!document.querySelector('nav, [role=navigation]'),
    main: !!document.querySelector('main, [role=main]'),
    footer: !!document.querySelector('footer, [role=contentinfo]'),
    skipLink: !!document.querySelector('.skip-link'),
  };

  // Interactive target heights. Inline links inside a paragraph or list item
  // are exempt: giving them a 44px box would break the surrounding line box,
  // and WCAG 2.2 exempts targets in a sentence for exactly that reason.
  const smallTargets = [];
  for (const el of document.querySelectorAll('a[href], button, summary, input, select')) {
    const r = el.getBoundingClientRect();
    if (!r.width && !r.height) continue;
    const inProse = el.closest('p, li, .prose, dd, td, figcaption, .frag-item, .term, .wt-field-note');
    if (inProse) continue;
    if (r.height < 44) {
      smallTargets.push(el.tagName.toLowerCase() + (el.className ? '.' + String(el.className).trim().split(/\\s+/)[0] : '') + ' ' + Math.round(r.width) + 'x' + Math.round(r.height));
    }
  }

  // Smallest rendered text.
  let minFont = 999, minFontEl = '';
  for (const el of document.body.querySelectorAll('*')) {
    if (el.children.length || !el.textContent.trim()) continue;
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) continue;
    const fs = parseFloat(getComputedStyle(el).fontSize);
    if (fs && fs < minFont) { minFont = fs; minFontEl = el.tagName.toLowerCase() + (el.className ? '.' + String(el.className).trim().split(/\\s+/)[0] : ''); }
  }

  // Images and SVGs.
  const imgNoAlt = [...document.images].filter(i => !i.hasAttribute('alt')).length;
  const svgUnlabelled = [...document.querySelectorAll('svg')].filter(s =>
    s.getAttribute('aria-hidden') !== 'true' && !s.getAttribute('aria-label') && !s.querySelector('title')
  ).length;

  return { vw, sw, overflow, h1Count, headingCount: headings.length, skips,
           landmarks, smallTargets: [...new Set(smallTargets)].slice(0, 6),
           minFont: Math.round(minFont * 100) / 100, minFontEl, imgNoAlt, svgUnlabelled };
})()`;

// ─── Run ─────────────────────────────────────────────────────────────────────

let staticServer = null;
let BASE = EXPLICIT_BASE;
if (!BASE) {
  if (!fs.existsSync(SERVE_DIR)) {
    console.error(
      `[audit] FAIL: ${path.relative(websiteRootDir, SERVE_DIR)} not found. Run \`npm run build\` first.`,
    );
    process.exit(1);
  }
  const served = await serveDist(SERVE_DIR);
  staticServer = served.server;
  BASE = `http://127.0.0.1:${served.port}`;
}

const chrome = launchChrome();
function cleanup() {
  chrome.kill();
  staticServer?.close();
}
process.on("exit", cleanup);

const target = await cdpTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.addEventListener("open", res, { once: true });
  ws.addEventListener("error", rej, { once: true });
});
const session = new Session(ws);
await session.send("Page.enable");
await session.send("Runtime.enable");

/** Structured findings, for the JSON report and for concise output. */
const findings = [];
const notes = [];

/**
 * Remediation hints keyed by finding kind. CI output that says what to do is
 * the difference between a check people fix and a check people re-run.
 */
const HINTS = {
  overflow:
    "an element is wider than the viewport. Usual causes: white-space:nowrap on a long label, a transform that moves a box outside its layout box, a <pre> or table inside a flex/grid item without min-width:0, or a long unbroken string needing overflow-wrap:anywhere.",
  h1: "each page needs exactly one <h1>.",
  "heading-skip":
    "heading levels must not skip. Change the element level; visual weight is a separate concern set by CSS.",
  landmark: "the page is missing a required landmark or the skip link.",
  "text-size":
    "text below 12px. Use var(--text-xs) as the floor; for relative sizes use max(<em value>, var(--text-xs)).",
  "img-alt":
    "every <img> needs an alt attribute (empty alt for decorative images).",
  "svg-label":
    'decorative SVGs need aria-hidden="true" focusable="false"; meaningful ones need aria-label or a <title>.',
  "reduced-motion":
    "with prefers-reduced-motion, content must be fully visible and nothing may still be animating. Usual cause: a new component animates opacity without inheriting the global reduced-motion rule in global.css, so a reader who asked for less motion loses the content entirely.",
};

function record(kind, page, width, detail) {
  findings.push({ kind, page, width, detail });
}

for (const path of PAGES) {
  for (const width of WIDTHS) {
    await session.send("Emulation.setDeviceMetricsOverride", {
      width,
      height: 900,
      deviceScaleFactor: 1,
      mobile: width < 700,
    });
    await session.send("Page.navigate", { url: BASE + path });
    // Poll for readiness rather than waiting on a load event, so a blocked
    // third-party request cannot stall the whole audit.
    for (let i = 0; i < 40; i++) {
      const state = await session.evaluate("document.readyState");
      if (state === "complete" || state === "interactive") break;
      await sleep(100);
    }
    await sleep(120);

    let r;
    try {
      r = await session.evaluate(AUDIT_EXPR);
    } catch (err) {
      record("error", path, width, err.message);
      continue;
    }

    if (r.overflow.length) {
      record(
        "overflow",
        path,
        width,
        `+${r.sw - r.vw}px \u2014 ${r.overflow[0]}`,
      );
    }
    if (width === WIDTHS[0]) {
      if (r.h1Count !== 1) record("h1", path, null, `found ${r.h1Count}`);
      if (r.skips.length)
        record("heading-skip", path, null, r.skips.join(", "));
      for (const [k, present] of Object.entries(r.landmarks)) {
        if (!present) record("landmark", path, null, `missing ${k}`);
      }
      if (r.minFont < 12)
        record("text-size", path, null, `${r.minFont}px on ${r.minFontEl}`);
      if (r.imgNoAlt) record("img-alt", path, null, `${r.imgNoAlt} image(s)`);
      if (r.svgUnlabelled)
        record("svg-label", path, null, `${r.svgUnlabelled} svg(s)`);
      if (r.smallTargets.length) {
        notes.push(
          `${path}: non-prose targets under 44px \u2014 ${r.smallTargets.join("; ")}`,
        );
      }
    }
  }
  process.stdout.write(".");
}

// ─── Reduced motion ──────────────────────────────────────────────────────────
//
// One extra pass at a single width. The site's global rule collapses animation
// and transition durations to 0.01ms under prefers-reduced-motion, so anything
// still above 50ms has bypassed it (an !important, an inline style, or a new
// component that never inherited the rule).

await session.send("Emulation.setEmulatedMedia", {
  features: [{ name: "prefers-reduced-motion", value: "reduce" }],
});
await session.send("Emulation.setDeviceMetricsOverride", {
  width: WIDTHS[WIDTHS.length - 1],
  height: 900,
  deviceScaleFactor: 1,
  mobile: false,
});

const REDUCED_MOTION_EXPR = `(() => {
  let stranded = 0, animating = 0, transitioning = 0, firstStranded = '';
  for (const el of document.body.querySelectorAll('*')) {
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) continue;
    const cs = getComputedStyle(el);
    if (parseFloat(cs.opacity) === 0) {
      stranded++;
      if (!firstStranded) firstStranded = el.tagName.toLowerCase() + (el.className ? '.' + String(el.className).trim().split(/\\s+/)[0] : '');
    }
    if (cs.animationName !== 'none' && parseFloat(cs.animationDuration) > 0.05) animating++;
    if (parseFloat(cs.transitionDuration) > 0.05) transitioning++;
  }
  return { stranded, animating, transitioning, firstStranded,
           unrevealed: document.querySelectorAll('.reveal:not(.revealed)').length };
})()`;

for (const path of PAGES) {
  await session.send("Page.navigate", { url: BASE + path });
  for (let i = 0; i < 40; i++) {
    const state = await session.evaluate("document.readyState");
    if (state === "complete" || state === "interactive") break;
    await sleep(100);
  }
  await sleep(150);
  try {
    const r = await session.evaluate(REDUCED_MOTION_EXPR);
    if (r.stranded > 0) {
      record(
        "reduced-motion",
        path,
        null,
        `${r.stranded} element(s) left at opacity 0 (first: ${r.firstStranded})`,
      );
    }
    if (r.unrevealed > 0) {
      record(
        "reduced-motion",
        path,
        null,
        `${r.unrevealed} .reveal element(s) never revealed`,
      );
    }
    if (r.animating > 0) {
      record(
        "reduced-motion",
        path,
        null,
        `${r.animating} element(s) still animating`,
      );
    }
    if (r.transitioning > 0) {
      record(
        "reduced-motion",
        path,
        null,
        `${r.transitioning} element(s) with transitions over 50ms`,
      );
    }
  } catch (err) {
    record("error", path, null, `reduced-motion pass: ${err.message}`);
  }
  process.stdout.write("~");
}

process.stdout.write("\n\n");

if (JSON_OUT) {
  fs.writeFileSync(
    JSON_OUT,
    JSON.stringify(
      {
        schema: "icn-website-layout-audit/v1",
        profile: PROFILE,
        base: BASE,
        pages: PAGES,
        widths: WIDTHS,
        pass: findings.length === 0,
        findings,
        notes,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(`[audit] report \u2192 ${JSON_OUT}`);
}

if (notes.length) {
  console.log(`[audit] ${notes.length} note(s), not failures:`);
  for (const n of notes.slice(0, 3)) console.log("        \u00b7 " + n);
  if (notes.length > 3)
    console.log(
      `        \u00b7 \u2026and ${notes.length - 3} more (see the --json report)`,
    );
  console.log("");
}

if (findings.length) {
  console.error(
    `[audit] FAIL \u2014 ${findings.length} problem(s) across ${PAGES.length} pages \u00d7 ${WIDTHS.length} widths (profile: ${PROFILE})\n`,
  );
  // Grouped by kind so each remediation hint prints once rather than after
  // every occurrence.
  const byKind = new Map();
  for (const f of findings) {
    if (!byKind.has(f.kind)) byKind.set(f.kind, []);
    byKind.get(f.kind).push(f);
  }
  for (const [kind, list] of byKind) {
    for (const f of list) {
      const at = f.width ? ` @ ${f.width}px` : "";
      console.error(`  FAIL ${f.page}${at}\n       ${kind}: ${f.detail}`);
    }
    if (HINTS[kind]) console.error(`       \u2192 ${HINTS[kind]}\n`);
  }
  cleanup();
  process.exit(1);
}

console.log(
  `[audit] PASS \u2014 ${PAGES.length} pages \u00d7 ${WIDTHS.length} widths (profile: ${PROFILE}): ` +
    `no horizontal overflow, single h1 and unbroken heading outline, landmarks present, ` +
    `no sub-12px text, images and SVGs labelled, reduced-motion leaves nothing hidden.`,
);
cleanup();
process.exit(0);
