/* ICN member-shell — pending-publish review-preview panel behavioral test (#1726).
 *
 * DEV/TEST TOOL — NOT part of the member shell, NOT loaded by index.html, NOT
 * shipped. Targeted test for the panel that consumes
 * GET /v1/gov/me/pending-publish-summary (icn#1728). Complements the whole-page
 * accessibility audit in a11y-walkthrough.cjs (which axe-scans the panel as part
 * of the full page); this file asserts the panel's behavior in demo AND live
 * mode. Live mode is exercised with same-origin page.route interception, so no
 * real gateway is needed and there is no CORS.
 *
 * Requires Playwright, a devDependency of web/pilot-ui (see #1735). Reproduce:
 *   cd web/pilot-ui && npm ci && npx playwright install chromium
 *   ( cd web && python3 -m http.server 8099 --bind 127.0.0.1 & )   # serve the web/ root
 *   NODE_PATH=web/pilot-ui/node_modules node web/member-shell/pending-publish.test.cjs http://127.0.0.1:8099
 *
 * Exits 0 and prints ALL_TESTS_PASSED only if every assertion holds; otherwise
 * prints the failures and exits non-zero (fails closed).
 */
const { chromium } = require('playwright');

const BASE = process.argv[2] || 'http://127.0.0.1:8099';
const failures = [];
const check = (name, cond, detail) => {
  if (cond) { console.log('  PASS', name); }
  else { failures.push(name + (detail ? ' — ' + detail : '')); console.log('  FAIL', name, detail || ''); }
};

// Minimal valid upstream responses so loadLive() reaches the pending-publish
// fetch. renderStanding/renderCards tolerate empty arrays.
const STANDING = { did: 'did:icn:live-fixture-subject', domains: [], roles: [], generated_at: 1751500000 };
const CARDS = { cards: [], generated_at: 1751500000 };

const LIVE_ROWS = {
  did: 'did:icn:live-fixture-subject',
  origin: 'live_runtime',
  rows: [{
    id: 'live-row-1', kind: 'decision',
    plain_summary: 'A live-node decision awaiting review.',
    status: 'pending_review', target_scope_label: 'A circle',
    governing_body_label: 'A circle', assignee_label: null,
    authority_basis: 'governing_body_agenda', risk_level: 'normal',
    accessibility_hint: 'Read-only review context.',
    source_provenance: 'governance_record',
    receipt_expected: { expected: true, category: 'governance_receipt' }
  }],
  non_claims: ['read-only'], generated_at: 1751500000
};
const LIVE_EMPTY = { did: 'did:icn:live-fixture-subject', origin: 'live_runtime', rows: [], non_claims: ['read-only'], generated_at: 1751500000 };

async function openLive(browser, ppHandler) {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  const seen = [];
  // Same-origin interception (gateway URL is the page's own origin): no CORS.
  await page.route('**/v1/gov/me/**', async (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname;
    seen.push({ path: path, method: req.method() });
    if (path.endsWith('/v1/gov/me/standing')) {
      return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(STANDING) });
    }
    if (path.endsWith('/v1/gov/me/action-cards')) {
      return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(CARDS) });
    }
    if (path.endsWith('/v1/gov/me/pending-publish-summary')) {
      return ppHandler(route);
    }
    return route.fulfill({ status: 404, contentType: 'application/json', body: '{"error":"not found"}' });
  });
  await page.goto(`${BASE}/member-shell/?mode=live`, { waitUntil: 'networkidle' });
  await page.fill('#gateway-url', BASE);
  await page.fill('#credential', 'test-token-fixture-not-a-secret');
  await page.evaluate(() => document.getElementById('connect-form').requestSubmit());
  return { ctx, page, seen };
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    // ============ DEMO MODE (committed fixture) ============
    console.log('DEMO MODE');
    {
      const ctx = await browser.newContext({ viewport: { width: 1280, height: 1024 } });
      const page = await ctx.newPage();
      await page.goto(`${BASE}/member-shell/?mode=demo`, { waitUntil: 'networkidle' });
      await page.waitForSelector('#pending-publish-section:not([hidden])', { timeout: 8000 }).catch(() => {});
      const sec = '#pending-publish-section';
      const text = (await page.textContent(sec).catch(() => '')) || '';

      check('demo: panel visible', await page.isVisible(sec).catch(() => false));
      const count = await page.locator('#pending-publish-list > li').count().catch(() => -1);
      check('demo: renders all 5 fixture rows', count === 5, 'count=' + count);
      check('demo: origin chip labels committed fixture', /fictional rehearsal data|committed fixture/i.test(
        (await page.textContent('#pending-publish-origin').catch(() => '')) || ''));

      // Every supported row status is rendered distinctly.
      const statuses = ['Awaiting review', 'Approved to stage the next step', 'Set aside', 'Needs an edit', 'Needs more information'];
      statuses.forEach((s) => check('demo: status label present "' + s + '"', text.indexOf(s) !== -1));

      // Boundary / non-claim copy present.
      check('demo: boundary copy present', /evidence presented for review/i.test(text) && /not authorization/i.test(text));
      // Privacy: no DID in the panel.
      check('demo: no did:icn: leak in panel', !/did:icn:/i.test(text));
      // Vocabulary firewalls.
      check('demo: no package nouns', !/nycn|summit|sponsor|registration|simpletix/i.test(text));
      check('demo: no money vocabulary', !/\bpayment\b|\bwallet\b|\bbalance\b|\bcurrency\b|\btransaction\b/i.test(text));
      // No decision/approval controls: the panel records nothing.
      const btns = await page.locator('#pending-publish-list button, #pending-publish-list input, #pending-publish-list [role="button"]').count().catch(() => -1);
      check('demo: no decision controls in panel', btns === 0, 'controls=' + btns);
      await ctx.close();
    }

    // ============ LIVE MODE (route-intercepted; no real gateway) ============
    console.log('LIVE MODE — rows');
    {
      const { ctx, page, seen } = await openLive(browser, (route) =>
        route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(LIVE_ROWS) }));
      await page.waitForSelector('#pending-publish-list > li', { timeout: 8000 }).catch(() => {});
      const ppReqs = seen.filter((r) => r.path.endsWith('/v1/gov/me/pending-publish-summary'));
      check('live: panel called the endpoint path', ppReqs.length >= 1, JSON.stringify(seen.map((s) => s.method + ' ' + s.path)));
      check('live: endpoint fetched with GET only (no mutation)', ppReqs.length > 0 && ppReqs.every((r) => r.method === 'GET'));
      check('live: no POST/PUT/PATCH/DELETE to pending-publish', !seen.some((r) =>
        r.path.endsWith('/v1/gov/me/pending-publish-summary') && r.method !== 'GET'));
      check('live: renders live rows', (await page.locator('#pending-publish-list > li').count().catch(() => -1)) === 1);
      check('live: origin chip labels live node data', /live node data/i.test(
        (await page.textContent('#pending-publish-origin').catch(() => '')) || ''));
      await ctx.close();
    }

    console.log('LIVE MODE — empty state');
    {
      const { ctx, page } = await openLive(browser, (route) =>
        route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(LIVE_EMPTY) }));
      await page.waitForSelector('#pending-publish-section:not([hidden])', { timeout: 8000 }).catch(() => {});
      const text = (await page.textContent('#pending-publish-section').catch(() => '')) || '';
      check('live-empty: panel visible with honest empty state', /no items awaiting review/i.test(text));
      check('live-empty: origin still labeled live', /live node data/i.test(
        (await page.textContent('#pending-publish-origin').catch(() => '')) || ''));
      await ctx.close();
    }

    console.log('LIVE MODE — endpoint error is resilient');
    {
      const { ctx, page } = await openLive(browser, (route) =>
        route.fulfill({ status: 500, contentType: 'application/json', body: '{"error":"boom"}' }));
      // The core standing render must still succeed even though the panel fetch failed.
      const standingOk = await page.waitForSelector('#standing-section:not([hidden])', { timeout: 8000 }).then(() => true).catch(() => false);
      check('live-error: core view still renders when panel endpoint 500s', standingOk);
      check('live-error: panel stays hidden (not a broken half-render)',
        !(await page.isVisible('#pending-publish-section').catch(() => false)));
      await ctx.close();
    }

    if (failures.length) {
      throw new Error('PANEL TESTS FAILED (' + failures.length + '): ' + failures.join(' | '));
    }
    console.log('ALL_TESTS_PASSED');
  } finally {
    await browser.close();
  }
})().catch((e) => { console.error('PANEL_TEST_ERROR', e.message); process.exit(1); });
