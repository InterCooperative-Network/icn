/* ICN member-shell accessibility / rendered-browser audit harness.
 *
 * DEV/AUDIT TOOL — NOT part of the member shell, NOT loaded by index.html,
 * NOT shipped. It reproduces the evidence in
 * docs/demo/JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md.
 *
 * Requires Playwright + @axe-core/playwright, which are devDependencies of
 * web/pilot-ui (see #1735). Reproduce:
 *   cd web/pilot-ui && npm ci && npx playwright install chromium   # installs the lockfile-pinned Playwright + its Chromium
 *   ( cd web && python3 -m http.server 8099 --bind 127.0.0.1 & )   # serve the web/ root
 *   NODE_PATH=web/pilot-ui/node_modules node web/member-shell/a11y-walkthrough.cjs http://127.0.0.1:8099 ./out
 *
 * The exact Chromium build is whatever the installed Playwright pins (this is
 * environment-dependent; see the doc's Method section). axe/WCAG outcomes on
 * this static surface are not sensitive to recent Chromium revisions.
 *
 * Usage: node a11y-walkthrough.cjs <baseUrl> <outDir>   (default mode=demo / ?lang=en)
 *
 * Optional env var MSHELL_SET targets a demo variant by appending &set=<value>
 * to the audited URL (icn#2289). Unset (the default) audits ?mode=demo exactly
 * as before, so the documented reproduction command and its evidence are
 * unchanged. Example, to audit the process-evidence evidence surface:
 *   MSHELL_SET=process-evidence NODE_PATH=web/pilot-ui/node_modules \
 *     node web/member-shell/a11y-walkthrough.cjs http://127.0.0.1:8099 ./out
 */
const { chromium } = require('playwright');
const { AxeBuilder } = require('@axe-core/playwright');
const { mkdirSync, writeFileSync } = require('node:fs');

(async () => {
  const BASE = process.argv[2] || 'http://127.0.0.1:8099';
  const OUT = process.argv[3] || '.';
  // Optional demo variant (icn#2289). Unset → ?mode=demo, byte-for-byte the
  // original default, so existing evidence is not disturbed.
  const SET = process.env.MSHELL_SET
    ? `&set=${encodeURIComponent(process.env.MSHELL_SET)}` : '';
  const URL = `${BASE}/member-shell/?mode=demo${SET}`;
  mkdirSync(OUT, { recursive: true });

  const report = { url: URL, steps: {}, axe: {}, keyboard: {}, notes: [] };
  const log = (k, v) => { report.steps[k] = v; console.log(`  ${k}: ${typeof v === 'string' ? v : JSON.stringify(v)}`); };

  const browser = await chromium.launch({ headless: true });
  try {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 1024 } });
    const page = await ctx.newPage();
    await page.goto(URL, { waitUntil: 'networkidle' });
    // Fail closed: if the primary fixture-backed shell never renders (wrong
    // serve root, broken fixtures), do NOT write a report that could be mistaken
    // for a regenerated pass — throw so the harness exits non-zero.
    const rendered = await page.waitForSelector('#standing-section:not([hidden])', { timeout: 8000 })
      .then(() => true).catch(() => false);
    if (!rendered) {
      throw new Error('member-shell standing pane never rendered at ' + URL +
        ' — wrong serve root (serve the web/ directory) or broken fixtures. Failing closed; no report written.');
    }

    log('url_has_no_credential', !/token=|jwt=|credential=|bearer/i.test(page.url()));
    log('honesty_banner', ((await page.textContent('#honesty-banner').catch(() => '')) || '').trim());
    log('standing_visible', await page.isVisible('#standing-section').catch(() => false));
    log('memberships_count', await page.locator('#domains-list > li').count().catch(() => -1));
    log('roles_count', await page.locator('#roles-list > li').count().catch(() => -1));
    log('cards_visible', await page.isVisible('#cards-section').catch(() => false));
    log('cards_count', await page.locator('#cards-list > li').count().catch(() => -1));
    log('receipts_visible', await page.isVisible('#receipts-section').catch(() => false));
    log('receipts_count', await page.locator('#receipts-list > li').count().catch(() => -1));
    // #1726 pending-publish review-preview panel (demo mode renders it from the
    // committed fixture). axe (below) already scans it as part of the full page.
    const ppText = (await page.textContent('#pending-publish-section').catch(() => '')) || '';
    log('pending_publish_visible', await page.isVisible('#pending-publish-section').catch(() => false));
    log('pending_publish_count', await page.locator('#pending-publish-list > li').count().catch(() => -1));
    log('pending_publish_origin', ((await page.textContent('#pending-publish-origin').catch(() => '')) || '').trim());
    log('pending_publish_no_did_leak', !/did:icn:/i.test(ppText));
    log('pending_publish_boundary_present', /evidence presented for review|not authorization/i.test(ppText));
    const bodyText = (await page.textContent('body')) || '';
    log('evidence_language_present', /evidence|receipt|provenance|proves|Show evidence/i.test(bodyText));
    log('receipt_show_evidence_control', await page.locator('text=/Show evidence detail/i').count().catch(() => 0));
    log('landmarks', await page.evaluate(() => ['header','nav','main','footer'].filter((t) => document.querySelector(t)).join(',')));
    log('h1', ((await page.textContent('h1').catch(() => '')) || '').replace(/\s+/g, ' ').trim());
    log('skip_link', await page.locator('a.skip-link').count().catch(() => 0));

    await page.screenshot({ path: `${OUT}/01-desktop-demo.png`, fullPage: true });

    // Keyboard nav: tab through, record focus + visible outline
    const focusTrail = [];
    await page.keyboard.press('Tab');
    for (let i = 0; i < 16; i++) {
      const f = await page.evaluate(() => {
        const el = document.activeElement;
        if (!el || el === document.body) return null;
        const cs = getComputedStyle(el);
        return { tag: el.tagName.toLowerCase(), id: el.id || null,
          text: (el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 36),
          outlineStyle: cs.outlineStyle, outlineWidth: cs.outlineWidth };
      });
      if (f) focusTrail.push(f);
      await page.keyboard.press('Tab');
    }
    report.keyboard.trail = focusTrail;
    report.keyboard.focusable_reached = focusTrail.length;
    report.keyboard.all_have_visible_outline = focusTrail.length > 0 &&
      focusTrail.every((f) => f.outlineStyle !== 'none' && f.outlineWidth !== '0px');
    console.log('  keyboard.focusable_reached:', focusTrail.length, ' all_visible_outline:', report.keyboard.all_have_visible_outline);

    const axe = await new AxeBuilder({ page })
      .withTags(['wcag2a','wcag2aa','wcag21a','wcag21aa','wcag22aa']).analyze();
    report.axe.violations = axe.violations.map((v) => ({ id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.length }));
    report.axe.violation_count = axe.violations.length;
    report.axe.passes = axe.passes.length;
    console.log('  axe.violations:', axe.violations.length, ' axe.passes:', axe.passes.length);

    await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${OUT}/02-zoom-200pct.png`, fullPage: true });
    await page.evaluate(() => { document.documentElement.style.zoom = '1'; });

    await page.setViewportSize({ width: 360, height: 740 });
    await page.waitForTimeout(150);
    log('mobile_no_horizontal_scroll', await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2));
    await page.screenshot({ path: `${OUT}/03-mobile-360.png`, fullPage: true });

    const rmCtx = await browser.newContext({ viewport: { width: 1280, height: 1024 }, reducedMotion: 'reduce' });
    const rmPage = await rmCtx.newPage();
    await rmPage.goto(URL, { waitUntil: 'networkidle' });
    await rmPage.waitForSelector('#standing-section:not([hidden])', { timeout: 8000 }).catch(() => {});
    log('reduced_motion_standing_visible', await rmPage.isVisible('#standing-section').catch(() => false));
    await rmPage.screenshot({ path: `${OUT}/04-reduced-motion.png`, fullPage: true });

    writeFileSync(`${OUT}/walkthrough-report.json`, JSON.stringify(report, null, 2));

    // Fail closed on a regression: a regenerated run must NOT exit 0 / print
    // REPORT_WRITTEN if the accessibility checks this evidence claims are not
    // satisfied. The report JSON is written first (for diagnosis), then we throw
    // — which the outer catch turns into a non-zero exit — so the documented
    // reproduction command can never be mistaken for a fresh pass after the
    // member-shell markup regresses.
    const failures = [];
    if (report.axe.violation_count > 0) {
      failures.push(`axe: ${report.axe.violation_count} WCAG violation(s) [${report.axe.violations.map((v) => v.id).join(', ')}]`);
    }
    if (report.keyboard.all_have_visible_outline !== true) {
      failures.push('keyboard: not every focusable showed a visible focus outline');
    }
    if (report.steps.standing_visible !== true) {
      failures.push('standing pane not visible');
    }
    if (report.steps.mobile_no_horizontal_scroll !== true) {
      failures.push('mobile (360px): horizontal scroll present');
    }
    if (report.steps.reduced_motion_standing_visible !== true) {
      failures.push('reduced-motion: standing pane not visible');
    }
    // The pending-publish panel renders in the DEFAULT demo view (and the
    // community set, which still runs loadDemo). The MSHELL_SET=process-evidence
    // variant intentionally takes a different loader (loadProcessEvidenceDemo)
    // that does not fetch this panel, so only require it when the panel is
    // actually expected — otherwise this would break the process-evidence
    // accessibility/evidence regeneration path.
    const ppExpected = SET.indexOf('process-evidence') === -1;
    if (ppExpected && report.steps.pending_publish_visible !== true) {
      failures.push('pending-publish review-preview panel not visible in demo mode');
    }
    if (ppExpected && report.steps.pending_publish_no_did_leak !== true) {
      failures.push('pending-publish panel leaked a did:icn: identifier');
    }
    if (failures.length) {
      throw new Error('accessibility checks FAILED — ' + failures.join('; ') +
        '. Report written to ' + OUT + '/walkthrough-report.json for diagnosis. Failing closed (not a pass).');
    }
    console.log('REPORT_WRITTEN');
  } finally {
    await browser.close();
  }
})().catch((e) => { console.error('WALKTHROUGH_ERROR', e.message); process.exit(1); });
