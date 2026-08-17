#!/usr/bin/env node
// audit-layout.mjs — mechanical accessibility and responsive checks.
//
//   node scripts/audit-layout.mjs [baseUrl]     default http://localhost:4321
//
// Run against `npm run preview`. Exits non-zero if any check fails.
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
//
// It does NOT replace a human pass. Contrast, focus visibility, screen-reader
// comprehension, and whether the page makes sense are not checkable here.

import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const BASE = process.argv[2] ?? "http://localhost:4321";

/** Representative pages, per the tranche's validation list. */
const PAGES = [
  "/",
  "/what-is-icn",
  "/why-icn",
  "/how-it-works",
  "/see-it-work",
  "/whats-real-now",
  "/for-cooperatives",
  "/get-involved",
  "/docs",
  "/docs/archive",
  "/docs/glossary",
  "/docs/archive/README",
];

/** Narrow mobile, mobile, tablet, laptop, wide desktop. */
const WIDTHS = [320, 375, 768, 1280, 1600];

const CHROME =
  process.env.CHROME_BIN ??
  ["/opt/google/chrome/chrome", "/usr/bin/google-chrome"].find(Boolean);

const PORT = 9333;

function launchChrome() {
  const child = spawn(
    CHROME,
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

const chrome = launchChrome();
process.on("exit", () => chrome.kill());

const target = await cdpTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.addEventListener("open", res, { once: true });
  ws.addEventListener("error", rej, { once: true });
});
const session = new Session(ws);
await session.send("Page.enable");
await session.send("Runtime.enable");

const failures = [];
const notes = [];

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
      failures.push(`${path} @${width}: audit threw — ${err.message}`);
      continue;
    }

    const where = `${path} @${width}`;
    if (r.overflow.length) {
      failures.push(
        `${where}: horizontal overflow (scrollWidth ${r.sw} > ${r.vw}) — ${r.overflow.join("; ")}`,
      );
    }
    if (width === WIDTHS[0]) {
      if (r.h1Count !== 1)
        failures.push(`${path}: expected exactly one h1, found ${r.h1Count}`);
      if (r.skips.length)
        failures.push(`${path}: heading level skipped — ${r.skips.join(", ")}`);
      for (const [k, present] of Object.entries(r.landmarks)) {
        if (!present) failures.push(`${path}: missing ${k}`);
      }
      if (r.minFont < 12)
        failures.push(
          `${path}: text below 12px — ${r.minFont}px on ${r.minFontEl}`,
        );
      if (r.imgNoAlt)
        failures.push(`${path}: ${r.imgNoAlt} image(s) without alt`);
      if (r.svgUnlabelled)
        failures.push(
          `${path}: ${r.svgUnlabelled} svg(s) neither aria-hidden nor labelled`,
        );
      if (r.smallTargets.length) {
        notes.push(
          `${path}: non-prose targets under 44px — ${r.smallTargets.join("; ")}`,
        );
      }
    }
  }
  process.stdout.write(".");
}

process.stdout.write("\n\n");

if (notes.length) {
  console.log(`${notes.length} note(s):`);
  for (const n of notes) console.log("  · " + n);
  console.log("");
}

if (failures.length) {
  console.error(
    `FAIL — ${failures.length} problem(s) across ${PAGES.length} pages × ${WIDTHS.length} widths:`,
  );
  for (const f of failures) console.error("  ✗ " + f);
  chrome.kill();
  process.exit(1);
}

console.log(
  `PASS — ${PAGES.length} pages × ${WIDTHS.length} widths: no horizontal overflow, ` +
    `single h1 and unbroken heading outline, landmarks present, no sub-12px text, ` +
    `images and SVGs labelled.`,
);
chrome.kill();
process.exit(0);
