/* ICN member-shell — organizer rehearsal review→confirm workflow behavioral test (#2386).
 *
 * DEV/TEST TOOL — NOT part of the member shell, NOT loaded by index.html, NOT
 * shipped. Drives the interactive organizer surface (?surface=organizer, live)
 * against the stateful in-page mock in organizer-mock.cjs, using same-origin
 * page.route interception (no real gateway, no CORS). It asserts the full
 * review→edit→assign→approve→preview→confirm→receipts→evidence loop plus the
 * security/robustness boundaries the surface must hold.
 *
 * Requires Playwright, a devDependency of web/pilot-ui. Reproduce:
 *   cd web/pilot-ui && npm ci && npx playwright install chromium
 *   ( cd web && python3 -m http.server 8099 --bind 127.0.0.1 & )   # serve the web/ root
 *   NODE_PATH=web/pilot-ui/node_modules node web/member-shell/organizer-workflow.test.cjs http://127.0.0.1:8099
 *
 * Exits 0 and prints ALL_TESTS_PASSED only if every assertion holds; otherwise
 * prints the failures and exits non-zero (fails closed).
 */
const { chromium } = require('playwright');
const { standing, DOMAIN_ONE, DOMAIN_TWO, makeWorkspace, installRoutes, submitConnect, sleep } = require('./organizer-mock.cjs');

const BASE = process.argv[2] || 'http://127.0.0.1:8099';
const ORG_URL = `${BASE}/member-shell/?mode=live&surface=organizer`;
const failures = [];
const check = (name, cond, detail) => {
  if (cond) { console.log('  PASS', name); }
  else { failures.push(name + (detail ? ' — ' + detail : '')); console.log('  FAIL', name, detail || ''); }
};

async function connect(browser, opts) {
  opts = opts || {};
  const ctx = await browser.newContext(opts.viewport ? { viewport: opts.viewport } : undefined);
  const page = await ctx.newPage();
  const seen = [];
  const { workspaces } = await installRoutes(page, { workspaces: opts.workspaces, standing: opts.standing, ppSummary: opts.ppSummary, seen });
  await page.goto(opts.url || ORG_URL, { waitUntil: 'networkidle' });
  await submitConnect(page, BASE, 'organizer-token-fixture-not-a-secret');
  return { ctx, page, seen, workspaces };
}

// review row-ai (edit → assign → approve) so it is executable + previewable.
async function driveToApproved(page) {
  await page.click('#organizer-rows-list button[data-row-id="row-ai"]');
  await page.waitForSelector('#organizer-detail-h', { timeout: 8000 });
  await page.fill('#organizer-edit-summary', 'Draft the revised sample agenda for the next cycle.');
  await page.click('[data-org-action="edit-save"]');
  await page.waitForFunction(() => /awaiting review again/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 }).catch(() => {});
  await page.selectOption('#organizer-assign-select', 'Example member');
  await page.click('[data-org-action="assign-save"]');
  await page.waitForFunction(() => /assignment updated/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 }).catch(() => {});
  await page.click('[data-review-action="approve"]');
  await page.waitForFunction(() => /review recorded/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 }).catch(() => {});
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    // ============ FULL HAPPY PATH ============
    console.log('FULL HAPPY PATH');
    {
      const { ctx, page, seen } = await connect(browser, { viewport: { width: 1280, height: 1024 } });
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      check('single eligible domain auto-opens workspace (no domain picker)',
        !(await page.isVisible('#organizer-domain-section').catch(() => true)));
      check('workspace lists the seeded rows', (await page.locator('#organizer-rows-list > li').count()) === 2);

      await driveToApproved(page);
      await page.click('[data-org-action="preview"]');
      await page.waitForSelector('#organizer-preview-h', { timeout: 8000 });
      const pv = (await page.textContent('#organizer-row-detail')) || '';
      check('preview shows the edited title', /revised sample agenda/i.test(pv));
      check('preview shows assignee label (bound)', /Example member/.test(pv) && /bound/i.test(pv));
      check('preview shows reversible = No', /Reversible/i.test(pv) && /No —/i.test(pv));
      check('preview shows creates a real action item', /Creates a real action item/i.test(pv) && /Yes/i.test(pv));

      await page.click('[data-org-action="continue"]');
      await page.waitForSelector('#organizer-confirm-h', { timeout: 8000 });
      await page.click('[data-org-action="confirm"]');
      await page.waitForSelector('#organizer-result-h', { timeout: 8000 });
      const rs = (await page.textContent('#organizer-row-detail')) || '';
      check('result confirms one action item created', /Created one action item/i.test(rs));
      check('result names the assignee', /Example member/.test(rs));

      await page.waitForSelector('#organizer-receipts-section:not([hidden])', { timeout: 8000 });
      const rc = (await page.textContent('#organizer-receipts-section')) || '';
      ['Process session opened', 'Gate result', 'Activation crossed', 'Mutation plan recorded', 'Mutation applied'].forEach((c) =>
        check('receipts render ladder class "' + c + '"', rc.indexOf(c) !== -1));
      await page.waitForSelector('#organizer-evidence-section:not([hidden])', { timeout: 8000 });
      const ev = (await page.textContent('#organizer-evidence-section')) || '';
      check('evidence shows executed outcome', /Confirmed and created/i.test(ev));
      check('member transition offered', await page.isVisible('#organizer-member-section'));

      const confirmReq = seen.filter((r) => /\/confirm$/.test(r.path) && r.method === 'POST').slice(-1)[0];
      check('confirm request sent only preview_digest', confirmReq && Object.keys(confirmReq.body || {}).length === 1 && 'preview_digest' in (confirmReq.body || {}),
        confirmReq ? JSON.stringify(Object.keys(confirmReq.body || {})) : 'no confirm request');

      check('never POSTed to bindings (setup)', !seen.some((r) => /\/rehearsal\/bindings$/.test(r.path) && r.method === 'POST'));
      check('never POSTed to reset', !seen.some((r) => /\/rehearsal\/reset$/.test(r.path)));
      check('read routes fetched with GET (list/detail/bindings/receipts/evidence)',
        seen.filter((r) => /\/(pending-publish|bindings|receipts|evidence-export)(\/[^/]+)?$/.test(r.path) && !/\/(review|assign|confirm)$/.test(r.path)).every((r) => r.method === 'GET' || r.method === 'PUT'));

      const pageText = (await page.textContent('body')) || '';
      check('no did:icn: in visible organizer DOM', !/did:icn:/i.test(pageText));
      const cred = 'organizer-token-fixture-not-a-secret';
      check('credential not in rendered HTML', !pageText.includes(cred) && !(await page.content()).includes(cred));
      check('credential not in URL', !page.url().includes(cred));
      const storage = await page.evaluate(() => ({ ls: JSON.stringify(window.localStorage), ss: JSON.stringify(window.sessionStorage), cookie: document.cookie }));
      check('no credential in localStorage/sessionStorage/cookies',
        !storage.ls.includes(cred) && !storage.ss.includes(cred) && !storage.cookie.includes(cred) && storage.cookie === '');
      await ctx.close();
    }

    // ============ WORKSPACE UNINITIALIZED (404) ============
    console.log('WORKSPACE UNINITIALIZED');
    {
      const { ctx, page, seen } = await connect(browser, { workspaces: {} });   // no workspace for dom-1
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      const txt = (await page.textContent('#organizer-workspace-section')) || '';
      check('uninitialized: explains a steward must set it up', /has not been set up|steward initializes/i.test(txt));
      check('uninitialized: standing still shown', await page.isVisible('#standing-section'));
      check('uninitialized: did NOT call reset', !seen.some((r) => /\/reset$/.test(r.path)));
      await ctx.close();
    }

    // ============ DOMAIN SELECTION (>1 eligible) ============
    console.log('DOMAIN SELECTION');
    {
      const { ctx, page } = await connect(browser, { standing: standing(DOMAIN_TWO), workspaces: { 'dom-1': makeWorkspace(), 'dom-2': makeWorkspace() } });
      await page.waitForSelector('#organizer-domain-section:not([hidden])', { timeout: 8000 });
      check('two domains → explicit picker shown', (await page.locator('#organizer-domain-choices input[type="radio"]').count()) === 2);
      check('no radio pre-selected (deliberate choice required)', (await page.locator('#organizer-domain-choices input[type="radio"]:checked').count()) === 0);
      check('workspace NOT auto-opened before choice', !(await page.isVisible('#organizer-workspace-section').catch(() => true)));
      // Pressing Open with nothing chosen must refuse and prompt, not open a domain.
      await page.click('#organizer-domain-open');
      check('Open with no selection is refused + prompts',
        !(await page.isVisible('#organizer-workspace-section').catch(() => true)) &&
        /choose a domain/i.test((await page.textContent('#organizer-domain-status')) || ''));
      // Make a deliberate choice, then open.
      await page.check('#org-domain-1');
      await page.click('#organizer-domain-open');
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      check('after an explicit choice, workspace opens', (await page.locator('#organizer-rows-list > li').count()) === 2);
      await ctx.close();
    }

    // ============ NON-EXECUTABLE ROW (decision) ============
    console.log('NON-EXECUTABLE ROW');
    {
      const { ctx, page } = await connect(browser);
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      await page.click('#organizer-rows-list button[data-row-id="row-dec"]');
      await page.waitForSelector('#organizer-detail-h', { timeout: 8000 });
      const txt = (await page.textContent('#organizer-row-detail')) || '';
      check('decision row: not-executable note shown', /not something this rehearsal slice can create/i.test(txt));
      check('decision row: no preview button', (await page.locator('[data-org-action="preview"]').count()) === 0);
      check('server markup stays inert (no injected <b>)', (await page.locator('#organizer-row-detail b').count()) === 0);
      check('server markup shown as literal text', txt.includes('<b>needs</b>'));
      await ctx.close();
    }

    // ============ UNBOUND LABEL → not confirmable ============
    console.log('UNBOUND LABEL');
    {
      const { ctx, page } = await connect(browser);
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      await page.click('#organizer-rows-list button[data-row-id="row-ai"]');
      await page.waitForSelector('#organizer-detail-h', { timeout: 8000 });
      await page.selectOption('#organizer-assign-select', 'Unbound helper');
      await page.click('[data-org-action="assign-save"]');
      await page.waitForFunction(() => /assignment updated/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 }).catch(() => {});
      await page.click('[data-review-action="approve"]');
      await page.waitForFunction(() => /review recorded/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 }).catch(() => {});
      await page.click('[data-org-action="preview"]');
      await page.waitForSelector('#organizer-preview-h', { timeout: 8000 });
      const txt = (await page.textContent('#organizer-row-detail')) || '';
      check('unbound assignee → preview shows needs steward setup', /needs steward setup/i.test(txt));
      check('unbound assignee → no Continue-to-confirm control', (await page.locator('[data-org-action="continue"]').count()) === 0);
      await ctx.close();
    }

    // ============ STALE PREVIEW (409) ============
    console.log('STALE PREVIEW 409');
    {
      const { ctx, page, workspaces } = await connect(browser);
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      await driveToApproved(page);
      await page.click('[data-org-action="preview"]');
      await page.waitForSelector('#organizer-preview-h', { timeout: 8000 });
      await page.click('[data-org-action="continue"]');
      await page.waitForSelector('#organizer-confirm-h', { timeout: 8000 });
      workspaces['dom-1'].forceStale = true;   // the row moved under us
      await page.click('[data-org-action="confirm"]');
      await page.waitForFunction(() => /changed since this preview/i.test((document.getElementById('organizer-action-status') || {}).textContent || ''), { timeout: 8000 });
      check('stale 409: shows the changed-since message', true);
      check('stale 409: no enabled Confirm remains', (await page.locator('[data-org-action="confirm"]:not([disabled])').count()) === 0);
      check('stale 409: preview cleared (no live Confirm/Continue survives)',
        (await page.locator('[data-org-action="confirm"]').count()) === 0);
      await ctx.close();
    }

    // ============ CONFIRM DOUBLE-CLICK → ONE REQUEST ============
    console.log('CONFIRM DOUBLE-CLICK');
    {
      const { ctx, page, seen } = await connect(browser);
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      await driveToApproved(page);
      await page.click('[data-org-action="preview"]');
      await page.waitForSelector('#organizer-preview-h', { timeout: 8000 });
      await page.click('[data-org-action="continue"]');
      await page.waitForSelector('#organizer-confirm-h', { timeout: 8000 });
      await page.evaluate(() => { const b = document.querySelector('[data-org-action="confirm"]'); b.click(); b.click(); b.click(); });
      await page.waitForSelector('#organizer-result-h', { timeout: 8000 });
      const confirms = seen.filter((r) => /\/confirm$/.test(r.path) && r.method === 'POST');
      check('triple-click emits exactly one confirm request', confirms.length === 1, 'count=' + confirms.length);
      await ctx.close();
    }

    // ============ ABANDONED ROW RESPONSE IGNORED ============
    console.log('ABANDONED ROW RESPONSE');
    {
      const ws = makeWorkspace();
      ws.delayDetailFor = 'row-ai'; ws.delayDetailMs = 1200;   // row-ai detail is slow
      const { ctx, page } = await connect(browser, { workspaces: { 'dom-1': ws } });
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      await page.click('#organizer-rows-list button[data-row-id="row-ai"]');  // slow
      await sleep(150);
      await page.click('#organizer-rows-list button[data-row-id="row-dec"]'); // fast; supersedes
      await page.waitForFunction(() => /decision/i.test((document.getElementById('organizer-detail-h') || {}).textContent || ''), { timeout: 8000 });
      await sleep(1400);   // let the abandoned row-ai detail arrive
      const h = (await page.textContent('#organizer-detail-h')) || '';
      check('superseded row-ai detail never overwrites row-dec', /decision/i.test(h) && !/action item/i.test(h));
      await ctx.close();
    }

    // ============ NEW CONNECTION CLEARS PRIOR ROWS ============
    console.log('RECONNECT CLEARS PRIOR ROWS');
    {
      const { ctx, page } = await connect(browser);
      await page.waitForSelector('#organizer-rows-list > li', { timeout: 8000 });
      check('first connection shows rows', (await page.locator('#organizer-rows-list > li').count()) === 2);
      await page.unroute('**/v1/gov/**');
      await page.route('**/v1/gov/**', async (route) => {
        const path = new URL(route.request().url()).pathname;
        if (path.endsWith('/v1/gov/me/standing')) { return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(standing([])) }); }
        return route.fulfill({ status: 404, contentType: 'application/json', body: '{"error":"x"}' });
      });
      await submitConnect(page, BASE, 'a-different-organizer-token');
      await page.waitForFunction(() => document.querySelectorAll('#organizer-rows-list > li').length === 0, { timeout: 8000 });
      check('reconnect cleared the prior organizer rows', (await page.locator('#organizer-rows-list > li').count()) === 0);
      check('reconnect: no-eligible-domain explained', /not a member of a domain with a rehearsal workspace/i.test((await page.textContent('#organizer-domain-section')) || ''));
      await ctx.close();
    }

    // ============ MEMBER PANEL renders rehearsal_runtime origin (plain language) ============
    console.log('MEMBER PANEL rehearsal_runtime ORIGIN');
    {
      const ctx = await browser.newContext();
      const page = await ctx.newPage();
      await page.route('**/v1/gov/me/**', async (route) => {
        const path = new URL(route.request().url()).pathname;
        const json = (o) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(o) });
        if (path.endsWith('/standing')) { return json(standing(DOMAIN_ONE)); }
        if (path.endsWith('/action-cards')) { return json({ cards: [], generated_at: 1751500000 }); }
        if (path.endsWith('/pending-publish-summary')) {
          return json({ did: 'did:icn:member', origin: 'rehearsal_runtime',
            rows: [{ id: 'r1', kind: 'action_item', plain_summary: 'A fictional rehearsal item.', status: 'approved_for_publish',
              target_scope_label: 'Example Working Group', governing_body_label: 'Example Stewards Circle',
              authority_basis: 'assigned action item', risk_level: 'low', accessibility_hint: '', source_provenance: 'committed_fixture',
              receipt_expected: { expected: true, category: 'action_item_completion_receipt' } }],
            non_claims: [], generated_at: 1751500000 });
        }
        return route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      });
      await page.goto(`${BASE}/member-shell/?mode=live`, { waitUntil: 'networkidle' });
      await submitConnect(page, BASE, 'member-token-fixture');
      await page.waitForSelector('#pending-publish-list > li', { timeout: 8000 });
      const origin = (await page.textContent('#pending-publish-origin')) || '';
      check('member panel labels rehearsal_runtime in plain language', /rehearsal node data/i.test(origin) && !/rehearsal_runtime/.test(origin));
      await ctx.close();
    }

    // ============ FOCUS RETENTION + PERSISTENT LIVE STATUS (a11y-review regression) ============
    console.log('FOCUS + LIVE STATUS');
    {
      const { ctx, page } = await connect(browser);
      await page.waitForSelector('#organizer-workspace-section:not([hidden])', { timeout: 8000 });
      await driveToApproved(page);   // ends on Approve
      const active = await page.evaluate(() => (document.activeElement && document.activeElement.id) || document.activeElement.tagName);
      check('focus stays on the item heading after Approve (not thrown to <body>)', active === 'organizer-detail-h', 'active=' + active);
      const status = (await page.textContent('#organizer-action-status')) || '';
      check('persistent action-status announces the review outcome', /review recorded/i.test(status));
      check('action-status is a single persistent polite live region',
        (await page.locator('#organizer-action-status[aria-live="polite"]').count()) === 1);
      await ctx.close();
    }

    if (failures.length) {
      throw new Error('ORGANIZER WORKFLOW TESTS FAILED (' + failures.length + '): ' + failures.join(' | '));
    }
    console.log('ALL_TESTS_PASSED');
  } finally {
    await browser.close();
  }
})().catch((e) => { console.error('ORGANIZER_TEST_ERROR', e.message); process.exit(1); });
