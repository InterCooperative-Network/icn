// Reports a failed weekly Security Audit run on ONE open tracking issue.
//
// Called from .github/workflows/security-audit.yml through actions/github-script.
// Tested by test_security_audit_issue.js (node --test).
//
// Why this is a module and not an inline script: the inline version looked up an
// existing issue by the label `security-audit` but created issues without it (and
// the label never existed), so the lookup could not match and every failing run
// opened a duplicate (#2737, #2784, #2804, #2805, #2815). The lookup key and the
// creation labels now come from one constant, and the tests pin that.
'use strict';

const TITLE = 'Weekly Security Audit Failed';
const DEDUP_LABEL = 'security-audit';
const CREATE_LABELS = ['priority:critical', 'type:impl', DEDUP_LABEL];

// The tracker is an issue this workflow itself opened. On a public repository anyone
// can open an issue with this title (and the title is all the legacy fallback has),
// so trusting the title alone would let an outside account receive the audit's
// failure reports -- and then close or edit them away. Issues created with the
// workflow's GITHUB_TOKEN are authored by the reserved app login below.
const WORKFLOW_AUTHOR = 'github-actions[bot]';

const isTracker = (issue) =>
  issue.title === TITLE &&
  issue.state === 'open' &&
  !issue.pull_request &&
  issue.user != null &&
  issue.user.login === WORKFLOW_AUTHOR &&
  issue.user.type === 'Bot';

// Deterministic choice among several candidates: the most recently opened.
const newest = (issues) => issues.reduce((a, b) => (b.number > a.number ? b : a));

async function ensureDedupLabel(github, repo) {
  try {
    await github.rest.issues.getLabel({ ...repo, name: DEDUP_LABEL });
  } catch (error) {
    if (error.status !== 404) throw error;
    await github.rest.issues.createLabel({
      ...repo,
      name: DEDUP_LABEL,
      color: 'b60205',
      description: 'Tracking issue for the weekly Security Audit workflow (its deduplication key)',
    });
  }
}

async function findTracker(github, repo) {
  const labelled = await github.paginate(github.rest.issues.listForRepo, {
    ...repo,
    state: 'open',
    labels: DEDUP_LABEL,
    per_page: 100,
  });
  const byLabel = labelled.filter(isTracker);
  if (byLabel.length > 0) return { issue: newest(byLabel), adopted: false };

  // Trackers opened before the label was applied at creation carry no dedup label.
  // Adopt the newest such issue instead of opening yet another duplicate.
  const open = await github.paginate(github.rest.issues.listForRepo, {
    ...repo,
    state: 'open',
    per_page: 100,
  });
  const legacy = open.filter(isTracker);
  if (legacy.length > 0) return { issue: newest(legacy), adopted: true };

  return null;
}

async function reportAuditFailure({ github, context }) {
  const repo = { owner: context.repo.owner, repo: context.repo.repo };
  const runUrl = `${context.serverUrl}/${repo.owner}/${repo.repo}/actions/runs/${context.runId}`;

  const found = await findTracker(github, repo);
  if (found) {
    if (found.adopted) {
      await ensureDedupLabel(github, repo);
      await github.rest.issues.addLabels({ ...repo, issue_number: found.issue.number, labels: [DEDUP_LABEL] });
    }
    await github.rest.issues.createComment({
      ...repo,
      issue_number: found.issue.number,
      body: `Another security audit failure detected.\n\n**Workflow Run:** ${runUrl}`,
    });
    return { action: 'commented', number: found.issue.number, adopted: found.adopted };
  }

  await ensureDedupLabel(github, repo);
  const { data } = await github.rest.issues.create({
    ...repo,
    title: TITLE,
    body: [
      'The weekly security audit has detected issues that need attention.',
      '',
      `**Workflow Run:** ${runUrl}`,
      '',
      'Please review the workflow logs and address any security advisories.',
      '',
      '---',
      '_This issue was automatically created by the weekly security audit workflow._',
    ].join('\n'),
    labels: CREATE_LABELS,
  });
  return { action: 'created', number: data.number };
}

module.exports = reportAuditFailure;
Object.assign(module.exports, { reportAuditFailure, TITLE, DEDUP_LABEL, CREATE_LABELS, WORKFLOW_AUTHOR });
