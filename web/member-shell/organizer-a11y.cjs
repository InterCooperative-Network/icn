/* ICN member-shell — organizer rehearsal surface accessibility / rendered-browser
 * audit harness (#2386).
 *
 * DEV/AUDIT TOOL — NOT part of the member shell, NOT loaded by index.html, NOT
 * shipped. Companion to a11y-walkthrough.cjs (which audits the demo member view).
 * This audits the interactive organizer surface (?surface=organizer, live) driven
 * against the stateful mock in organizer-mock.cjs, at its richest states, plus the
 * pseudo-locale and RTL variants.
 *
 * IMPORTANT: an automated axe/Playwright pass does NOT close the human
 * assistive-technology gate (#2041). It is one input to the 12-category
 * ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md checklist; the screen-reader / switch /
 * human-zoom categories still require a real AT pass.
 *
 * Requires Playwright + @axe-core/playwright (devDependencies of web/pilot-ui):
 *   cd web/pilot-ui && npm ci && npx playwright install chromium
 *   ( cd web && python3 -m http.server 8099 --bind 127.0.0.1 & )   # serve the web/ root
 *   NODE_PATH=web/pilot-ui/node_modules node web/member-shell/organizer-a11y.cjs http://127.0.0.1:8099 ./out
 *
 * Fails closed: writes the report then exits non-zero if any WCAG violation, any
 * focusable without a visible outline, a horizontal scroll at 375px, or a missing
 * pane is found. Prints REPORT_WRITTEN only on a full pass.
 */
const { chromium } = require('playwright');
const { AxeBuilder } = require('@axe-core/playwright');
const { mkdirSync, writeFileSync } = require('node:fs');
const { installRoutes, submitConnect, makeWorkspace } = require('./organizer-mock.cjs');

const BASE = process.argv[2] || 'http://127.0.0.1:8099';
const OUT = process.argv[3] || '.';
const orgUrl = (lang) => `${BASE}/member-shell/?mode=live&surface=organizer${lang ? '&lang=' + encodeURIComponent(lang) : ''}`;
const WCAG = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

async function open(browser, opts) {
  opts = opts || {};
  const ctx = await browser.newContext(Object.assign({ viewport: opts.viewport || { width: 1280, height: 1024 } }, opts.ctx || {}));
  const page = await ctx.newPage();
  await installRoutes(page, { workspaces: { 'dom-1': makeWorkspace() } });
  await page.goto(opts.url || orgUrl(), { waitUntil: 'networkidle' });
  await submitConnect(page, BASE, 'organizer-token-fixture-not-a-secret');
  await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
  return { ctx, page };
}

// Drive to the review-controls state (all form controls present) then approve so
// the preview panel is reachable.
async function toReviewState(page) {
  await page.click('#organizer-rows-list button[data-row-id="row-ai"]');
  await page.waitForSelector('#organizer-detail-h', { timeout: 8000 });
}
async function toPreviewState(page) {
  await page.selectOption('#organizer-assign-select', 'Example member');
  await page.click('[data-org-action="assign-save"]');
  await page.waitForFunction(() => /assignment updated/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 }).catch(() => {});
  await page.click('[data-review-action="approve"]');
  await page.waitForFunction(() => /review recorded/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 }).catch(() => {});
  await page.click('[data-org-action="preview"]');
  await page.waitForSelector('#organizer-preview-h', { timeout: 8000 });
}

(async () => {
  mkdirSync(OUT, { recursive: true });
  const report = { states: {}, axe: {}, keyboard: {}, notes: [
    'Automated axe/Playwright pass. Does NOT satisfy the human assistive-technology gate (#2041): screen-reader (3.2/3.9), switch (3.5), and human 200%-zoom + contrast (3.3) categories still require a real AT pass.'
  ] };
  const violations = [];
  const browser = await chromium.launch({ headless: true });

  async function axeState(page, name) {
    const res = await new AxeBuilder({ page }).withTags(WCAG).analyze();
    report.axe[name] = { violations: res.violations.map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.length })), violation_count: res.violations.length, passes: res.passes.length };
    res.violations.forEach((v) => violations.push(name + ':' + v.id));
    console.log(`  axe[${name}]: violations=${res.violations.length} passes=${res.passes.length}`);
  }

  try {
    // ---- Desktop: audit list, review-controls, preview, and confirmed states ----
    const { ctx, page } = await open(browser);
    report.states.workspace_visible = await page.isVisible('#organizer-workspace-section');
    await axeState(page, 'workspace-list');

    await toReviewState(page);
    report.states.review_controls = (await page.locator('[data-review-action]').count()) >= 4 &&
      (await page.locator('#organizer-edit-summary').count()) === 1 &&
      (await page.locator('#organizer-assign-select').count()) === 1;
    await axeState(page, 'review-controls');

    // Keyboard: tab through the review state; every focusable must show an outline.
    const trail = [];
    await page.evaluate(() => document.querySelector('a.skip-link').focus());
    for (let i = 0; i < 24; i++) {
      await page.keyboard.press('Tab');
      const f = await page.evaluate(() => {
        const el = document.activeElement;
        if (!el || el === document.body) { return null; }
        const cs = getComputedStyle(el);
        return { tag: el.tagName.toLowerCase(), id: el.id || null, outlineStyle: cs.outlineStyle, outlineWidth: cs.outlineWidth };
      });
      if (f) { trail.push(f); }
    }
    report.keyboard.reached = trail.length;
    report.keyboard.all_visible_outline = trail.length > 0 && trail.every((f) => f.outlineStyle !== 'none' && f.outlineWidth !== '0px');
    console.log('  keyboard: reached=' + trail.length + ' all_visible_outline=' + report.keyboard.all_visible_outline);

    await toPreviewState(page);
    await axeState(page, 'preview');
    await page.click('[data-org-action="continue"]');
    await page.waitForSelector('#organizer-confirm-h', { timeout: 8000 });
    await axeState(page, 'confirm');
    await page.click('[data-org-action="confirm"]');
    await page.waitForSelector('#organizer-result-h', { timeout: 8000 });
    await page.waitForSelector('#organizer-receipts-section:not([hidden])', { timeout: 8000 });
    await axeState(page, 'result-receipts-evidence');
    await page.screenshot({ path: `${OUT}/org-01-desktop.png`, fullPage: true });

    // 200% zoom (reflow, no assertion beyond axe already run + screenshot)
    await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${OUT}/org-02-zoom-200pct.png`, fullPage: true });
    await page.evaluate(() => { document.documentElement.style.zoom = '1'; });

    // 375px mobile: no horizontal scroll
    await page.setViewportSize({ width: 375, height: 812 });
    await page.waitForTimeout(150);
    report.states.mobile_no_horizontal_scroll = await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2);
    await page.screenshot({ path: `${OUT}/org-03-mobile-375.png`, fullPage: true });
    console.log('  mobile_no_horizontal_scroll:', report.states.mobile_no_horizontal_scroll);
    await ctx.close();

    // ---- Pseudo-locale (i18n coverage) ----
    const ploc = await open(browser, { url: orgUrl('qps-ploc') });
    await axeState(ploc.page, 'pseudo-locale');
    report.states.pseudo_locale_workspace = await ploc.page.isVisible('#organizer-workspace-section');
    await ploc.page.screenshot({ path: `${OUT}/org-04-pseudo.png`, fullPage: true });
    await ploc.ctx.close();

    // ---- RTL ----
    const rtl = await open(browser, { url: orgUrl('ar') });
    report.states.rtl_dir = await rtl.page.evaluate(() => document.documentElement.getAttribute('dir'));
    await axeState(rtl.page, 'rtl');
    await rtl.page.screenshot({ path: `${OUT}/org-05-rtl.png`, fullPage: true });
    await rtl.ctx.close();

    // ---- Reduced motion ----
    const rm = await open(browser, { ctx: { reducedMotion: 'reduce' } });
    report.states.reduced_motion_workspace = await rm.page.isVisible('#organizer-workspace-section');
    await rm.page.screenshot({ path: `${OUT}/org-06-reduced-motion.png`, fullPage: true });
    await rm.ctx.close();

    report.axe.total_violations = violations.length;
    writeFileSync(`${OUT}/organizer-a11y-report.json`, JSON.stringify(report, null, 2));

    const failures = [];
    if (violations.length) { failures.push('axe WCAG violations: ' + violations.join(', ')); }
    if (report.keyboard.all_visible_outline !== true) { failures.push('keyboard: a focusable lacked a visible outline'); }
    if (report.states.mobile_no_horizontal_scroll !== true) { failures.push('375px: horizontal scroll present'); }
    if (report.states.review_controls !== true) { failures.push('review controls did not all render'); }
    if (report.states.rtl_dir !== 'rtl') { failures.push('RTL: document dir not rtl'); }
    if (report.states.reduced_motion_workspace !== true) { failures.push('reduced-motion: workspace not visible'); }
    if (report.states.pseudo_locale_workspace !== true) { failures.push('pseudo-locale: workspace not visible'); }
    if (failures.length) {
      throw new Error('organizer accessibility checks FAILED — ' + failures.join('; ') +
        '. Report at ' + OUT + '/organizer-a11y-report.json. Failing closed (not a pass).');
    }
    console.log('REPORT_WRITTEN');
  } finally {
    await browser.close();
  }
})().catch((e) => { console.error('ORGANIZER_A11Y_ERROR', e.message); process.exit(1); });
