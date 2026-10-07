// Regression tests for the weekly Security Audit's failure-issue reporter.
//
// The defect these pin (icn#2815 and its duplicates #2737, #2784, #2804, #2805):
// the workflow looked for an existing open issue by a label it never applied
// when creating one, so every failing run opened a fresh duplicate.
//
// Run: node --test .github/scripts/test_security_audit_issue.js
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const mod = require('./security_audit_issue.js');

const run = typeof mod === 'function' ? mod : mod.reportAuditFailure;
const TITLE = 'Weekly Security Audit Failed';

// A fake of the slice of the GitHub REST API the reporter uses. It is stricter
// than GitHub on purpose: applying a label that does not exist throws, so the
// reporter cannot depend on label auto-creation behaviour.
function fakeGitHub({ issues = [], labels = ['priority:critical', 'type:impl'] } = {}) {
  const state = {
    issues: issues.map((i) => ({ state: 'open', labels: [], ...i })),
    labels: new Set(labels),
    comments: [],
    nextNumber: Math.max(1000, ...issues.map((i) => i.number)) + 1,
  };
  const requireLabels = (names) => {
    for (const n of names) {
      if (!state.labels.has(n)) throw Object.assign(new Error(`label ${n} does not exist`), { status: 422 });
    }
  };
  const view = (i) => ({
    number: i.number,
    title: i.title,
    state: i.state,
    labels: i.labels.map((name) => ({ name })),
    ...(i.pull_request ? { pull_request: {} } : {}),
  });
  const listForRepo = async ({ state: wanted = 'open', labels: filter } = {}) => {
    const required = filter ? filter.split(',').map((s) => s.trim()) : [];
    return {
      data: state.issues
        .filter((i) => wanted === 'all' || i.state === wanted)
        .filter((i) => required.every((l) => i.labels.includes(l)))
        .map(view),
    };
  };
  const github = {
    paginate: async (method, params) => (await method(params)).data,
    rest: {
      issues: {
        listForRepo,
        create: async ({ title, body, labels: ls = [] }) => {
          requireLabels(ls);
          const issue = { number: state.nextNumber++, title, body, state: 'open', labels: [...ls] };
          state.issues.push(issue);
          return { data: view(issue) };
        },
        createComment: async ({ issue_number, body }) => {
          state.comments.push({ issue_number, body });
          return { data: {} };
        },
        addLabels: async ({ issue_number, labels: ls }) => {
          requireLabels(ls);
          const issue = state.issues.find((i) => i.number === issue_number);
          for (const l of ls) if (!issue.labels.includes(l)) issue.labels.push(l);
          return { data: {} };
        },
        getLabel: async ({ name }) => {
          if (!state.labels.has(name)) throw Object.assign(new Error('Not Found'), { status: 404 });
          return { data: { name } };
        },
        createLabel: async ({ name }) => {
          state.labels.add(name);
          return { data: { name } };
        },
      },
    },
  };
  return { github, state };
}

const context = (runId) => ({
  serverUrl: 'https://github.com',
  runId,
  repo: { owner: 'o', repo: 'r' },
});

const openTrackers = (state) =>
  state.issues.filter((i) => i.title === TITLE && i.state === 'open' && !i.pull_request);

test('a first failure opens exactly one tracking issue, and it carries the dedup label', async () => {
  const { github, state } = fakeGitHub();
  await run({ github, context: context(1) });
  const trackers = openTrackers(state);
  assert.equal(trackers.length, 1);
  assert.ok(trackers[0].labels.includes(mod.DEDUP_LABEL), 'the created issue must be findable next run');
  assert.ok(trackers[0].body.includes('/actions/runs/1'));
});

test('a second failure comments on the open tracker instead of opening a duplicate', async () => {
  const { github, state } = fakeGitHub();
  await run({ github, context: context(1) });
  await run({ github, context: context(2) });
  assert.equal(openTrackers(state).length, 1, 'the defect: each failing run opened a fresh issue');
  assert.equal(state.comments.length, 1);
  assert.equal(state.comments[0].issue_number, openTrackers(state)[0].number);
  assert.ok(state.comments[0].body.includes('/actions/runs/2'));
});

test('unlabeled legacy duplicates are adopted: the newest gets the label and the comment', async () => {
  // The shape of what is on the repository today.
  const legacy = [2737, 2784, 2804, 2805, 2815].map((number) => ({
    number,
    title: TITLE,
    labels: ['type:impl', 'priority:critical'],
  }));
  const { github, state } = fakeGitHub({ issues: legacy });
  await run({ github, context: context(3) });
  assert.equal(openTrackers(state).length, 5, 'no sixth duplicate');
  assert.deepEqual(state.comments.map((c) => c.issue_number), [2815]);
  const adopted = state.issues.find((i) => i.number === 2815);
  assert.ok(adopted.labels.includes(mod.DEDUP_LABEL), 'adoption makes the next run find it by label');

  // And the run after that finds it by label, still without a new issue.
  await run({ github, context: context(4) });
  assert.equal(openTrackers(state).length, 5);
  assert.deepEqual(state.comments.map((c) => c.issue_number), [2815, 2815]);
});

test('pull requests and closed issues with the same title are never treated as the tracker', async () => {
  const { github, state } = fakeGitHub({
    issues: [
      { number: 10, title: TITLE, pull_request: true, labels: [] },
      { number: 11, title: TITLE, state: 'closed', labels: [] },
    ],
  });
  await run({ github, context: context(5) });
  const trackers = openTrackers(state);
  assert.equal(trackers.length, 1);
  assert.ok(trackers[0].number > 11, 'a new tracker, not the PR or the closed issue');
  assert.equal(state.comments.length, 0);
});

test('the label the lookup filters on is one of the labels a created issue carries', () => {
  // The structural form of the defect: lookup key and creation labels drifted apart.
  assert.equal(typeof mod.DEDUP_LABEL, 'string');
  assert.ok(Array.isArray(mod.CREATE_LABELS));
  assert.ok(mod.CREATE_LABELS.includes(mod.DEDUP_LABEL));
});
