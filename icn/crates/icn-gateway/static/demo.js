// ── Config ────────────────────────────────────────────────────────────────────
document.getElementById('gwUrl').value = window.location.origin;
function getGateway() { return document.getElementById('gwUrl').value.replace(/\/+$/, ''); }
function getApi() { return getGateway() + '/v1'; }

const COOP_ID = 'finger-lakes-food';
const COOP_NAME = 'Finger Lakes Food Co-op';
const DOMAIN_ID = 'coop:' + COOP_ID;

// ── State ─────────────────────────────────────────────────────────────────────
let ids = {}; // alice, bob, carol
let charterId = null;
let proposalId = null;

// ── Utilities ─────────────────────────────────────────────────────────────────
function $(id) { return document.getElementById(id); }

function setStatus(text, state) {
  $('statusText').textContent = text;
  const dot = $('statusDot');
  dot.className = 'status-dot' + (state ? ' ' + state : '');
}

function addStep(bodyId, text, status) {
  const icons = { pending: '•', active: '◦', done: '✓', fail: '✗' };
  const body = $(bodyId);
  const div = document.createElement('div');
  div.className = 'step ' + status;
  const icon = document.createElement('div');
  icon.className = 'step-icon';
  icon.textContent = icons[status] || '•';
  const content = document.createElement('div');
  const label = document.createElement('span');
  label.textContent = text;
  content.appendChild(label);
  div.appendChild(icon);
  div.appendChild(content);
  body.appendChild(div);
  return { div, content };
}

function updateStep(s, status, detail) {
  const icons = { pending: '•', active: '◦', done: '✓', fail: '✗' };
  s.div.className = 'step ' + status;
  s.div.querySelector('.step-icon').textContent = icons[status] || '•';
  if (detail) {
    let d = s.div.querySelector('.step-detail');
    if (!d) { d = document.createElement('div'); d.className = 'step-detail'; s.content.appendChild(d); }
    d.textContent = detail;
  }
}

async function api(method, path, body, token) {
  const opts = { method, headers: { 'Content-Type': 'application/json' } };
  if (token) opts.headers['Authorization'] = 'Bearer ' + token;
  if (body) opts.body = JSON.stringify(body);
  const r = await fetch(getApi() + path, opts);
  const text = await r.text();
  let json = null;
  try { json = JSON.parse(text); } catch(e) {}
  if (r.status >= 400) throw new Error((json && json.error) || text || ('HTTP ' + r.status));
  return { status: r.status, data: json };
}

// ── Crypto ────────────────────────────────────────────────────────────────────
const B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
function b58enc(bytes) {
  let n = 0n;
  for (const b of bytes) n = n * 256n + BigInt(b);
  let r = '';
  while (n > 0n) { r = B58[Number(n % 58n)] + r; n = n / 58n; }
  for (const b of bytes) { if (b === 0) r = '1' + r; else break; }
  return r;
}
function hexEnc(bytes) { return Array.from(bytes).map(b => b.toString(16).padStart(2,'0')).join(''); }
function hexDec(hex) {
  const b = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) b[i/2] = parseInt(hex.substr(i,2), 16);
  return b;
}

async function makeIdentity(name) {
  const kp = await crypto.subtle.generateKey({ name: 'Ed25519' }, true, ['sign','verify']);
  const pub = new Uint8Array(await crypto.subtle.exportKey('raw', kp.publicKey));
  return { name, did: 'did:icn:z' + b58enc(pub), keyPair: kp, token: null };
}

async function authenticate(id, scopes) {
  const cr = await api('POST', '/auth/challenge', { did: id.did });
  const sig = new Uint8Array(await crypto.subtle.sign('Ed25519', id.keyPair.privateKey, hexDec(cr.data.nonce)));
  const vr = await api('POST', '/auth/verify', { did: id.did, signature: hexEnc(sig), coop_id: COOP_ID, scopes });
  id.token = vr.data.token;
}

function memberCard(name, role, did, cssClass) {
  const card = document.createElement('div');
  card.className = 'member';
  const av = document.createElement('div');
  av.className = 'member-avatar ' + cssClass;
  av.textContent = name[0];
  const info = document.createElement('div');
  const nameEl = document.createElement('div');
  nameEl.className = 'member-name';
  nameEl.textContent = name;
  const roleEl = document.createElement('div');
  roleEl.className = 'member-role' + (role.includes('Coordinator') ? ' coordinator' : '');
  roleEl.textContent = role + ' • ' + did.substring(0, 22) + '...';
  info.appendChild(nameEl);
  info.appendChild(roleEl);
  card.appendChild(av);
  card.appendChild(info);
  return card;
}

// ── Phase 1: Founding Assembly ────────────────────────────────────────────────
async function runPhase1() {
  const btn = $('btn1');
  btn.disabled = true;
  btn.textContent = 'Running...';
  btn.className = 'phase-btn running';
  $('body1').replaceChildren();
  setStatus('Founding assembly...', 'running');

  const coordScopes = ['coop:read','coop:write','coop:admin','governance:read','governance:write','treasury:read','treasury:write'];

  try {
    let s = addStep('body1', 'Generating equal founding member identities...', 'active');
    ids.alice = await makeIdentity('Alice');
    ids.bob   = await makeIdentity('Bob');
    ids.carol = await makeIdentity('Carol');
    updateStep(s, 'done', 'Three Ed25519 keypairs — self-sovereign digital identities');

    const membersDiv = document.createElement('div');
    membersDiv.className = 'members';
    membersDiv.appendChild(memberCard('Alice', 'Coordinator', ids.alice.did, 'alice'));
    membersDiv.appendChild(memberCard('Bob', 'Member', ids.bob.did, 'bob'));
    membersDiv.appendChild(memberCard('Carol', 'Member', ids.carol.did, 'carol'));
    $('body1').appendChild(membersDiv);

    s = addStep('body1', 'Alice authenticates as coordinator...', 'active');
    await authenticate(ids.alice, coordScopes);
    updateStep(s, 'done', 'DID challenge-response complete, JWT issued');

    s = addStep('body1', 'Creating cooperative: ' + COOP_NAME + '...', 'active');
    await api('POST', '/coops', { id: COOP_ID, name: COOP_NAME }, ids.alice.token);
    updateStep(s, 'done');

    s = addStep('body1', 'Creating governance domain...', 'active');
    await api('POST', '/gov/domains', {
      id: DOMAIN_ID, name: COOP_NAME + ' Governance', profile: 'cooperative_default',
      quorum_percent: 50, approval_percent: 51, voting_period_days: 7,
      members: [ids.alice.did]
    }, ids.alice.token);
    updateStep(s, 'done', '50% quorum, simple majority, 7-day default period');

    s = addStep('body1', 'Adding Bob as equal founding member...', 'active');
    await api('POST', '/coops/' + COOP_ID + '/members', { did: ids.bob.did, role: 'participant', display_name: 'Bob' }, ids.alice.token);
    await api('POST', '/gov/domains/' + DOMAIN_ID + '/members', { did: ids.bob.did, weight: 1.0 }, ids.alice.token);
    updateStep(s, 'done', 'Equal voting weight: 1.0');

    s = addStep('body1', 'Adding Carol as equal founding member...', 'active');
    await api('POST', '/coops/' + COOP_ID + '/members', { did: ids.carol.did, role: 'participant', display_name: 'Carol' }, ids.alice.token);
    await api('POST', '/gov/domains/' + DOMAIN_ID + '/members', { did: ids.carol.did, weight: 1.0 }, ids.alice.token);
    updateStep(s, 'done', 'Equal voting weight: 1.0');

    $('phase1').className = 'phase done open';
    btn.textContent = '✓ Founding Assembly';
    btn.className = 'phase-btn';
    $('phase2').className = 'phase open';
    $('btn2').disabled = false;
    setStatus('Cooperative founded with 3 equal members — ready to ratify charter', 'ok');
  } catch(e) {
    btn.textContent = 'Retry';
    btn.className = 'phase-btn';
    btn.disabled = false;
    const err = document.createElement('div');
    err.className = 'error-box';
    err.textContent = 'Phase 1 failed: ' + e.message;
    $('body1').appendChild(err);
    setStatus('Error: ' + e.message, 'error');
  }
}

// ── Phase 2: Charter Ratification ────────────────────────────────────────────
async function runPhase2() {
  const btn = $('btn2');
  btn.disabled = true;
  btn.textContent = 'Running...';
  btn.className = 'phase-btn running';
  $('body2').replaceChildren();
  setStatus('Charter ratification vote...', 'running');

  const govScopes = ['governance:read','governance:write'];

  try {
    let s = addStep('body2', 'Alice submits founding charter for ratification...', 'active');
    const cr = await api('POST', '/gov/proposals', {
      domain_id: DOMAIN_ID,
      title: 'Ratify the Finger Lakes Food Co-op Founding Charter',
      description: 'Adopt the cooperative\'s founding charter, establishing member rights, governance structure, and operating principles.',
      payload: {
        type: 'text',
        body: 'COOPERATIVE CHARTER — Finger Lakes Food Co-op\n\n1. MEMBERSHIP: Any person who supports our mission may join. All members have equal voting rights.\n2. GOVERNANCE: Decisions are made democratically. No member has more authority than any other.\n3. PURPOSE: To provide affordable, healthy food to our community and build economic resilience through cooperation.\n4. COORDINATION: Coordinators are elected for specific purposes and terms. All coordination roles rotate.\n5. AMENDMENT: This charter may be amended by a two-thirds vote of members.'
      }
    }, ids.alice.token);
    charterId = cr.data.id || cr.data.proposal_id;
    updateStep(s, 'done', 'Charter proposal: ' + charterId);

    const charterCard = document.createElement('div');
    charterCard.className = 'proposal-card';
    const ct = document.createElement('div');
    ct.className = 'proposal-title';
    ct.textContent = 'Ratify the Finger Lakes Food Co-op Founding Charter';
    const cd = document.createElement('div');
    cd.className = 'proposal-desc';
    cd.textContent = 'Establishes member rights, governance structure, and operating principles. No member has more authority than any other. All coordination roles rotate.';
    charterCard.appendChild(ct);
    charterCard.appendChild(cd);
    $('body2').appendChild(charterCard);

    s = addStep('body2', 'Opening charter for ratification vote...', 'active');
    await api('POST', '/gov/proposals/' + charterId + '/open', { voting_period_seconds: 3600 }, ids.alice.token);
    updateStep(s, 'done');

    s = addStep('body2', 'Alice votes to ratify...', 'active');
    await api('POST', '/gov/proposals/' + charterId + '/vote', { choice: 'for', comment: 'This charter reflects our values. I vote to ratify.' }, ids.alice.token);
    updateStep(s, 'done');

    s = addStep('body2', 'Bob authenticates and votes to ratify...', 'active');
    await authenticate(ids.bob, govScopes);
    await api('POST', '/gov/proposals/' + charterId + '/vote', { choice: 'for', comment: 'Agreed. These rules give every member a voice.' }, ids.bob.token);
    updateStep(s, 'done');

    s = addStep('body2', 'Carol authenticates and votes to ratify...', 'active');
    await authenticate(ids.carol, govScopes);
    await api('POST', '/gov/proposals/' + charterId + '/vote', { choice: 'for', comment: 'Unanimously supported. Let\'s build something together.' }, ids.carol.token);
    updateStep(s, 'done');

    const votesDiv = document.createElement('div');
    votesDiv.className = 'votes-container';
    [{who:'Alice'},{who:'Bob'},{who:'Carol'}].forEach(v => {
      const row = document.createElement('div');
      row.className = 'vote-row';
      const badge = document.createElement('span');
      badge.className = 'vote-badge for';
      badge.textContent = 'for';
      const name = document.createElement('span');
      name.textContent = v.who + ' ratified the charter';
      row.appendChild(badge);
      row.appendChild(name);
      votesDiv.appendChild(row);
    });
    $('body2').appendChild(votesDiv);

    s = addStep('body2', 'Alice closes the ratification...', 'active');
    await api('POST', '/gov/proposals/' + charterId + '/close', {}, ids.alice.token);
    updateStep(s, 'done', 'Charter ratified 3/3 — cooperative is constituted');

    $('phase2').className = 'phase done open';
    btn.textContent = '✓ Charter Ratified';
    btn.className = 'phase-btn';
    $('phase3').className = 'phase open';
    $('btn3').disabled = false;
    setStatus('Charter ratified 3-0 — ready for first proposal', 'ok');
  } catch(e) {
    btn.textContent = 'Retry';
    btn.className = 'phase-btn';
    btn.disabled = false;
    const err = document.createElement('div');
    err.className = 'error-box';
    err.textContent = 'Phase 2 failed: ' + e.message;
    $('body2').appendChild(err);
    setStatus('Error: ' + e.message, 'error');
  }
}

// ── Phase 3: Democratic Decision ──────────────────────────────────────────────
async function runPhase3() {
  const btn = $('btn3');
  btn.disabled = true;
  btn.textContent = 'Running...';
  btn.className = 'phase-btn running';
  $('body3').replaceChildren();
  setStatus('Running democratic vote...', 'running');

  const govScopes = ['governance:read','governance:write'];

  try {
    let s = addStep('body3', 'Bob submits a budget proposal...', 'active');
    const pr = await api('POST', '/gov/proposals', {
      domain_id: DOMAIN_ID,
      title: 'Approve $12,000 for community kitchen equipment',
      description: 'Purchase commercial-grade equipment for the shared community kitchen: convection oven ($4,000), industrial mixer ($3,000), prep tables and storage ($2,500), safety equipment and small tools ($2,500). This serves all 47 member households.',
      payload: { type: 'budget', amount: 12000, recipient: ids.alice.did, currency: 'USD', purpose: 'Community Kitchen Equipment' }
    }, ids.bob.token);
    proposalId = pr.data.id || pr.data.proposal_id;
    updateStep(s, 'done', 'Proposal: ' + proposalId);

    const card = document.createElement('div');
    card.className = 'proposal-card';
    const title = document.createElement('div');
    title.className = 'proposal-title';
    title.textContent = 'Approve $12,000 for community kitchen equipment';
    const desc = document.createElement('div');
    desc.className = 'proposal-desc';
    desc.textContent = 'Convection oven ($4,000) · Industrial mixer ($3,000) · Prep tables and storage ($2,500) · Safety and small tools ($2,500) · Serves all 47 member households.';
    const meta = document.createElement('div');
    meta.className = 'proposal-meta';
    const m1 = document.createElement('span'); m1.textContent = '💰 $12,000 USD';
    const m2 = document.createElement('span'); m2.textContent = '👤 Proposed by Bob';
    const m3 = document.createElement('span'); m3.textContent = '🏛️ ' + COOP_NAME;
    meta.appendChild(m1); meta.appendChild(m2); meta.appendChild(m3);
    card.appendChild(title); card.appendChild(desc); card.appendChild(meta);
    $('body3').appendChild(card);

    s = addStep('body3', 'Bob opens the proposal for voting...', 'active');
    await api('POST', '/gov/proposals/' + proposalId + '/open', { voting_period_seconds: 3600 }, ids.bob.token);
    updateStep(s, 'done', 'Voting period: 1 hour');

    s = addStep('body3', 'Alice votes FOR...', 'active');
    await api('POST', '/gov/proposals/' + proposalId + '/vote', { choice: 'for', comment: 'This serves every member household. Fully support.' }, ids.alice.token);
    updateStep(s, 'done');

    s = addStep('body3', 'Bob votes FOR his own proposal...', 'active');
    await api('POST', '/gov/proposals/' + proposalId + '/vote', { choice: 'for', comment: 'I believe in this investment.' }, ids.bob.token);
    updateStep(s, 'done');

    s = addStep('body3', 'Carol votes FOR...', 'active');
    await api('POST', '/gov/proposals/' + proposalId + '/vote', { choice: 'for', comment: 'The kitchen benefits everyone.' }, ids.carol.token);
    updateStep(s, 'done');

    const votesDiv = document.createElement('div');
    votesDiv.className = 'votes-container';
    [{label:'Alice voted'},{label:'Bob voted (proposer)'},{label:'Carol voted'}].forEach(v => {
      const row = document.createElement('div');
      row.className = 'vote-row';
      const badge = document.createElement('span');
      badge.className = 'vote-badge for';
      badge.textContent = 'for';
      const name = document.createElement('span');
      name.textContent = v.label;
      row.appendChild(badge);
      row.appendChild(name);
      votesDiv.appendChild(row);
    });
    $('body3').appendChild(votesDiv);

    $('phase3').className = 'phase done open';
    btn.textContent = '✓ Votes Cast';
    btn.className = 'phase-btn';
    $('phase4').className = 'phase open';
    $('btn4').disabled = false;
    setStatus('3 votes cast — Carol will close and verify', 'ok');
  } catch(e) {
    btn.textContent = 'Retry';
    btn.className = 'phase-btn';
    btn.disabled = false;
    const err = document.createElement('div');
    err.className = 'error-box';
    err.textContent = 'Phase 3 failed: ' + e.message;
    $('body3').appendChild(err);
    setStatus('Error: ' + e.message, 'error');
  }
}

// ── Phase 4: Verification ─────────────────────────────────────────────────────
async function runPhase4() {
  const btn = $('btn4');
  btn.disabled = true;
  btn.textContent = 'Running...';
  btn.className = 'phase-btn running';
  $('body4').replaceChildren();
  setStatus('Carol closing vote and generating proof...', 'running');

  try {
    let s = addStep('body4', 'Carol closes the vote...', 'active');
    await api('POST', '/gov/proposals/' + proposalId + '/close', {}, ids.carol.token);
    updateStep(s, 'done', 'Closed by Carol — rotating authority demonstrated');

    s = addStep('body4', 'Retrieving proposal outcome...', 'active');
    const pr = await api('GET', '/gov/proposals/' + proposalId, null, ids.carol.token);
    const stateKey = typeof pr.data.state === 'object' ? Object.keys(pr.data.state)[0] : String(pr.data.state);
    updateStep(s, 'done', 'Outcome: ' + stateKey);

    s = addStep('body4', 'Computing vote tally...', 'active');
    const tr = await api('GET', '/gov/proposals/' + proposalId + '/tally', null, ids.carol.token);
    const tally = tr.data;
    updateStep(s, 'done', tally.for_votes + ' for · ' + tally.against_votes + ' against · ' + tally.abstain_votes + ' abstain');

    const tallyDiv = document.createElement('div');
    tallyDiv.className = 'tally';
    const total = tally.for_votes + tally.against_votes + tally.abstain_votes;
    const forPct = total > 0 ? Math.round(tally.for_votes / total * 100) : 0;
    const againstPct = total > 0 ? Math.round(tally.against_votes / total * 100) : 0;
    const bar = document.createElement('div');
    bar.className = 'tally-bar';
    const forBar = document.createElement('div');
    forBar.className = 'tally-for';
    forBar.style.width = forPct + '%';
    const againstBar = document.createElement('div');
    againstBar.className = 'tally-against';
    againstBar.style.width = againstPct + '%';
    bar.appendChild(forBar);
    bar.appendChild(againstBar);
    const labels = document.createElement('div');
    labels.className = 'tally-labels';
    const fl = document.createElement('span');
    fl.textContent = '\u2705 For: ' + tally.for_votes + ' (' + forPct + '%)';
    const al = document.createElement('span');
    al.textContent = '\u274c Against: ' + tally.against_votes;
    labels.appendChild(fl);
    labels.appendChild(al);
    const result = document.createElement('div');
    result.className = 'tally-result ' + (stateKey === 'Accepted' ? 'accepted' : 'rejected');
    result.textContent = stateKey === 'Accepted' ? '\u2705 PROPOSAL ACCEPTED' : '\u274c PROPOSAL ' + stateKey.toUpperCase();
    tallyDiv.appendChild(bar);
    tallyDiv.appendChild(labels);
    tallyDiv.appendChild(result);
    $('body4').appendChild(tallyDiv);

    s = addStep('body4', 'Generating cryptographic proof...', 'active');
    const pfr = await api('GET', '/gov/proposals/' + proposalId + '/proof', null, ids.carol.token);
    updateStep(s, 'done', 'Tamper-proof receipt generated');

    if (pfr.data) {
      const receipt = document.createElement('div');
      receipt.className = 'receipt';
      const h3 = document.createElement('h3');
      h3.textContent = '\uD83D\uDD10 Governance Receipt';
      receipt.appendChild(h3);

      const receiptData = pfr.data.receipt || pfr.data;
      const toHex = v => v ? (Array.isArray(v) ? hexEnc(new Uint8Array(v)) : String(v)) : 'N/A';
      const voteHash = toHex(receiptData.vote_hash);
      const decisionHash = toHex(receiptData.decision_hash);

      [
        ['Proposal', proposalId],
        ['Domain', DOMAIN_ID],
        ['Outcome', receiptData.outcome || stateKey],
        ['Tally', tally.for_votes + ' for / ' + tally.against_votes + ' against / ' + tally.abstain_votes + ' abstain'],
        ['Closed by', 'Carol'],
        ['Vote Hash', voteHash.substring(0,16) + '...' + voteHash.slice(-16)],
        ['Decision Hash', decisionHash.substring(0,16) + '...' + decisionHash.slice(-16)],
      ].forEach(function(r) {
        const row = document.createElement('div');
        row.className = 'receipt-row';
        const label = document.createElement('span');
        label.className = 'receipt-label';
        label.textContent = r[0];
        const value = document.createElement('span');
        value.className = 'receipt-value';
        value.textContent = r[1];
        row.appendChild(label);
        row.appendChild(value);
        receipt.appendChild(row);
      });

      const note = document.createElement('div');
      note.style.cssText = 'margin-top:.65rem;font-size:.78rem;color:var(--dim);line-height:1.5';
      note.textContent = 'Any modification to the votes, outcome, or proposal details changes the hash and breaks verification. This receipt can be checked independently against the raw ballot records.';
      receipt.appendChild(note);
      $('body4').appendChild(receipt);
    }

    $('phase4').className = 'phase done open';
    btn.textContent = '\u2713 Verified';
    btn.className = 'phase-btn';
    setStatus('Demo complete — decision recorded with cryptographic proof', 'ok');
  } catch(e) {
    btn.textContent = 'Retry';
    btn.className = 'phase-btn';
    btn.disabled = false;
    const err = document.createElement('div');
    err.className = 'error-box';
    err.textContent = 'Phase 4 failed: ' + e.message;
    $('body4').appendChild(err);
    setStatus('Error: ' + e.message, 'error');
  }
}

// ── Wire up buttons ───────────────────────────────────────────────────────────
document.getElementById('btn1').addEventListener('click', runPhase1);
document.getElementById('btn2').addEventListener('click', runPhase2);
document.getElementById('btn3').addEventListener('click', runPhase3);
document.getElementById('btn4').addEventListener('click', runPhase4);
