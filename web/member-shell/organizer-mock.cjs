/* ICN member-shell — shared in-page mock of the Rehearsal-mode runtime routes
 * (#2386), used by the organizer behavioral test and the organizer accessibility
 * audit. Same-origin Playwright route interception; no real gateway, no CORS.
 *
 * DEV/TEST support module — NOT part of the member shell, NOT shipped.
 *
 * The mock is stateful: review/edit/assign bump a per-row version, approve gates
 * preview, preview mints a version-bound digest, and confirm rejects a stale
 * digest — so tests exercise the same invariants the real runtime enforces.
 */
'use strict';

// Deterministic 64-hex "digest" bound to a row version — the mock's stand-in for
// the runtime's preview digest. Any version change makes a prior digest stale.
function hex64(seed) {
  let s = '';
  for (let i = 0; i < 64; i++) { s += ((((seed * 7 + i * 13) % 16) + 16) % 16).toString(16); }
  return s;
}

function standing(domains) {
  return { did: 'did:icn:organizer-fixture-not-a-real-identity',
    domains: domains, roles: [], authority_scopes: [], generated_at: 1751500000 };
}
const DOMAIN_ONE = [{ domain_id: 'dom-1', domain_name: 'Example Working Group', membership_source: 'static_list', status: 'member' }];
const DOMAIN_TWO = DOMAIN_ONE.concat([{ domain_id: 'dom-2', domain_name: 'Example Stewards Circle', membership_source: 'static_list', status: 'member' }]);

function makeWorkspace() {
  return {
    generation: 1,
    ver: { 'row-ai': 1, 'row-dec': 1 },
    rows: {
      'row-ai': { id: 'row-ai', kind: 'action_item',
        plain_summary: 'Draft a sample agenda for the next organizing cycle.',
        status: 'pending_review', target_scope_label: 'Example Working Group',
        governing_body_label: 'Example Stewards Circle', assignee_label: null,
        authority_basis: 'assigned action item', risk_level: 'low',
        accessibility_hint: 'Plain-language review context.', source_provenance: 'committed_fixture',
        receipt_expected: { expected: true, category: 'action_item_completion_receipt' },
        note: null, executed: false },
      'row-dec': { id: 'row-dec', kind: 'decision',
        plain_summary: 'A fictional decision <b>needs</b> more information.',
        status: 'needs_more_info', target_scope_label: 'Example Working Group',
        governing_body_label: 'Example Stewards Circle', assignee_label: null,
        authority_basis: 'governing body agenda', risk_level: 'normal',
        accessibility_hint: '', source_provenance: 'governance_record',
        receipt_expected: { expected: true, category: 'governance_receipt' },
        note: null, executed: false }
    },
    order: ['row-ai', 'row-dec'],
    bindings: [{ label: 'Example member', bound: true }, { label: 'Unbound helper', bound: false }],
    lastPreview: null, forceStale: false, delayDetailFor: null, delayDetailMs: 0
  };
}
function boundOf(ws, label) {
  if (!label) { return null; }
  const b = ws.bindings.filter((x) => x.label === label)[0];
  return b ? b.bound : false;
}
function baseRow(r) {
  const o = { id: r.id, kind: r.kind, plain_summary: r.plain_summary, status: r.status,
    target_scope_label: r.target_scope_label, governing_body_label: r.governing_body_label,
    authority_basis: r.authority_basis, risk_level: r.risk_level, accessibility_hint: r.accessibility_hint,
    source_provenance: r.source_provenance, receipt_expected: r.receipt_expected };
  if (r.assignee_label) { o.assignee_label = r.assignee_label; }
  return o;
}
function listingRow(ws, r) {
  return Object.assign(baseRow(r), { version: ws.ver[r.id], executed: r.executed, assignee_bound: boundOf(ws, r.assignee_label) });
}
function detailBody(ws, r) {
  const row = Object.assign(baseRow(r), { version: ws.ver[r.id], note: r.note, executed: r.executed });
  if (r.executed) { row.execution = { action_item_id: 'ai-' + r.id, plan_record_hash: hex64(90), application_record_hash: hex64(91) }; }
  return { row: row, assignee_bound: boundOf(ws, r.assignee_label) };
}
function confirmBody(ws, rowId, r, idempotent) {
  return {
    row_id: rowId, action_item_id: 'ai-' + rowId, session_id: 'sess-fixture',
    decision_id: 'dec-' + rowId + '-' + ws.ver[rowId], decision_record_hash: hex64(50 + ws.ver[rowId]),
    gate_record_hash: hex64(60), activation_id: 'act-' + rowId, activation_record_hash: hex64(61),
    plan_id: 'plan-' + rowId, plan_record_hash: hex64(62), application_id: 'app-' + rowId,
    application_record_hash: hex64(63), result_hash: hex64(64), preview_digest: hex64(ws.ver[rowId]),
    idempotent: idempotent, non_claims: ['Receipts record process facts and grant no authority.']
  };
}
const STATUS_AFTER = { approve: 'approved_for_publish', reject: 'rejected', needs_edit: 'needs_edit', needs_more_info: 'needs_more_info' };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function handleRehearsal(json, method, domain, suffix, req, workspaces) {
  const ws = workspaces[domain];
  if (!ws) { return json(404, { error: 'No rehearsal workspace is initialized for this domain.' }); }
  const jbody = (() => { try { return req.postDataJSON(); } catch (e) { return {}; } })();

  if (suffix === '' || suffix === '/pending-publish') {
    return json(200, { generation: ws.generation, rows: ws.order.map((id) => listingRow(ws, ws.rows[id])) });
  }
  if (suffix === '/bindings') {
    if (method === 'POST') { return json(403, { error: 'setup credential required' }); }  // organizer must never reach this
    return json(200, { bindings: ws.bindings });
  }
  if (suffix === '/reset') { return json(403, { error: 'setup credential required' }); }   // organizer must never reach this
  if (suffix === '/receipts') {
    return json(200, { generation: ws.generation, receipts: ws.receipts || [], non_claims: ['Receipts record process facts and grant no authority.'] });
  }
  if (suffix === '/evidence-export') {
    return json(200, {
      contract: 'urn:icn:contract:rehearsal-workflow-evidence:v1', origin: 'rehearsal_runtime',
      domain_id: domain, run_id: 'run-fixture-0001', generation: ws.generation, session_id: 'sess-fixture',
      session_record_hash: hex64(7),
      rows: ws.order.map((id) => { const r = ws.rows[id]; return { id: r.id, kind: r.kind, outcome: r.executed ? 'executed' : (r.status === 'approved_for_publish' ? 'approved-not-executed' : 'deferred'), version: ws.ver[id], source_provenance: r.source_provenance, assignee_label: r.assignee_label, mutation_executed: r.executed }; }),
      decisions: ws.decisionLog || [], bindings: ws.bindings,
      privacy_review: { dids_exported: false, credentials_exported: false, private_overlay_values_exported: false, identity_exposure: 'labels-and-bound-flags-only' },
      non_claims: ['Not a pilot; fictional rehearsal data.'],
      privacy_review_result: 'clean', hash_domain_tag: 'icn:gov:rehearsal_workflow_evidence:v1',
      generated_at: 1751500000, packet_hash: hex64(11), packet_hash_sha256: hex64(12)
    });
  }
  const m = suffix.match(/^\/pending-publish\/([^/]+)(\/[a-z]+)?$/);
  if (m) {
    const rowId = decodeURIComponent(m[1]);
    const action = m[2] || '';
    const r = ws.rows[rowId];
    if (!r) { return json(404, { error: 'No such proposed-work row in this domain\'s rehearsal workspace' }); }

    if (action === '' && method === 'GET') {
      if (ws.delayDetailFor === rowId && ws.delayDetailMs) { await sleep(ws.delayDetailMs); }
      return json(200, detailBody(ws, r));
    }
    if (action === '' && method === 'PUT') {   // edit
      if (r.executed) { return json(409, { error: 'already executed' }); }
      r.plain_summary = String(jbody.plain_summary || '').trim();
      r.status = 'pending_review'; ws.ver[rowId] += 1; ws.lastPreview = null;
      return json(200, { row: baseRow(r), version: ws.ver[rowId] });
    }
    if (action === '/review' && method === 'POST') {
      if (r.executed) { return json(409, { error: 'already executed' }); }
      const decision = jbody.decision;
      if (!STATUS_AFTER[decision]) { return json(400, { error: 'Unknown review decision' }); }
      r.status = STATUS_AFTER[decision]; ws.ver[rowId] += 1; r.note = jbody.note || null; ws.lastPreview = null;
      ws.decisionLog = (ws.decisionLog || []).concat([{ seq: (ws.decisionLog ? ws.decisionLog.length : 0) + 1, row_id: rowId, row_version: ws.ver[rowId], decision: decision, decision_id: 'dec-' + rowId + '-' + ws.ver[rowId], record_hash: hex64(50 + ws.ver[rowId]), note_present: !!jbody.note }]);
      ws.receipts = (ws.receipts || []).concat([{ class: 'decision_recorded', id: 'dec-' + rowId + '-' + ws.ver[rowId], record_hash: hex64(50 + ws.ver[rowId]), row_id: rowId, decision: decision }]);
      return json(200, { row: baseRow(r), version: ws.ver[rowId], decision_receipt: { decision_id: 'dec-' + rowId + '-' + ws.ver[rowId], record_hash: hex64(50 + ws.ver[rowId]) } });
    }
    if (action === '/assign' && method === 'POST') {
      if (r.executed) { return json(409, { error: 'already executed' }); }
      const label = (jbody.assignee_label === undefined || jbody.assignee_label === null) ? null : String(jbody.assignee_label).trim();
      if (label && !ws.bindings.some((b) => b.label === label)) { return json(422, { error: 'Unknown assignee label', label: label }); }
      r.assignee_label = label || null; r.status = 'pending_review'; ws.ver[rowId] += 1; ws.lastPreview = null;
      return json(200, { row: baseRow(r), version: ws.ver[rowId], assignee_bound: boundOf(ws, r.assignee_label) });
    }
    if (action === '/preview' && method === 'GET') {
      if (r.kind !== 'action_item') { return json(422, { error: 'This kind of proposed work is reviewable but not executable in this rehearsal slice.', row_id: rowId }); }
      if (r.status !== 'approved_for_publish') { return json(409, { error: 'Preview requires an approved review decision on the current version of this row.' }); }
      const digest = hex64(ws.ver[rowId]);
      ws.lastPreview = { rowId: rowId, version: ws.ver[rowId], digest: digest };
      const bound = boundOf(ws, r.assignee_label);
      return json(200, {
        row_id: rowId, version: ws.ver[rowId], generation: ws.generation, action: 'create_action_item',
        domain_id: domain, title: r.plain_summary, description: 'Rehearsal action item for: ' + r.plain_summary,
        assignee_label: r.assignee_label, assignee_bound: bound, authority_basis: r.authority_basis,
        risk_level: r.risk_level, receipt_expected: r.receipt_expected, reversible: false,
        permanence_note: 'Confirming creates a real action item and permanent process receipts on this Rehearsal Node.',
        privacy_note: 'The preview and all receipts are value-withheld: member identities appear only as labels.',
        confirmable: !(r.assignee_label && bound === false),
        preview_digest: digest, plan_id: 'rehearsal-plan-run-' + rowId + '-v' + ws.ver[rowId],
        expected_receipts: ['process_gate_result', 'activation_crossed', 'mutation_plan_recorded', 'mutation_applied']
      });
    }
    if (action === '/confirm' && method === 'POST') {
      if (r.kind !== 'action_item') { return json(422, { error: 'This kind of proposed work is not executable in this rehearsal slice.' }); }
      if (r.status !== 'approved_for_publish') { return json(409, { error: 'Confirm requires an approved review decision on the current version of this row.' }); }
      if (r.assignee_label && boundOf(ws, r.assignee_label) === false) { return json(409, { error: 'Assignee label is not bound to an identity; bind it or clear it.' }); }
      const digest = jbody.preview_digest;
      if (ws.forceStale || digest !== hex64(ws.ver[rowId])) { return json(409, { error: 'The preview is stale: the proposed work changed since it was made.' }); }
      if (r.executed) { return json(200, confirmBody(ws, rowId, r, true)); }
      r.executed = true;
      ws.receipts = (ws.receipts || []).concat([
        { class: 'process_session_opened', id: 'sess-fixture', record_hash: hex64(7) },
        { class: 'process_gate_result', id: 'scope-confirmation:plan-' + rowId, record_hash: hex64(60) },
        { class: 'activation_crossed', id: 'act-' + rowId, record_hash: hex64(61), references_decision: 'dec-' + rowId + '-' + ws.ver[rowId] },
        { class: 'mutation_plan_recorded', id: 'plan-' + rowId, record_hash: hex64(62), body_hash: digest, references_activation: 'act-' + rowId },
        { class: 'mutation_applied', id: 'app-' + rowId, record_hash: hex64(63), references_plan: 'plan-' + rowId, result_hash: hex64(64), action_item_id: 'ai-' + rowId }
      ]);
      return json(201, confirmBody(ws, rowId, r, false));
    }
  }
  return json(404, { error: 'not found' });
}

// Install the mock on a page. `opts.seen` (optional array) records every request.
async function installRoutes(page, opts) {
  opts = opts || {};
  const workspaces = opts.workspaces || { 'dom-1': makeWorkspace() };
  const st = opts.standing || standing(DOMAIN_ONE);
  const seen = opts.seen || [];
  await page.route('**/v1/gov/**', async (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname;
    const method = req.method();
    let body = null; try { body = req.postDataJSON(); } catch (e) { body = null; }
    seen.push({ method, path, body });
    const json = (status, obj) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(obj) });
    if (path.endsWith('/v1/gov/me/standing')) { return json(200, st); }
    if (path.endsWith('/v1/gov/me/pending-publish-summary')) { return (opts.ppSummary ? opts.ppSummary(json) : json(200, { did: st.did, origin: 'committed_fixture', rows: [], non_claims: [], generated_at: 1751500000 })); }
    const rm = path.match(/\/v1\/gov\/domains\/([^/]+)\/rehearsal(\/.*)?$/);
    if (rm) { return handleRehearsal(json, method, decodeURIComponent(rm[1]), rm[2] || '', req, workspaces); }
    return json(404, { error: 'not found' });
  });
  return { workspaces, seen };
}

// Fill the connect form and submit (the surface's single credential path).
async function submitConnect(page, base, credential) {
  await page.fill('#gateway-url', base);
  await page.fill('#credential', credential || 'organizer-token-fixture-not-a-secret');
  await page.evaluate(() => document.getElementById('connect-form').requestSubmit());
}

module.exports = {
  hex64, standing, DOMAIN_ONE, DOMAIN_TWO, makeWorkspace, installRoutes, submitConnect, sleep
};
