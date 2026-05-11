---
Status: operational
Canonical: no
Last Reviewed: 2026-05-11
---

# Governance Proposal/Vote Fixture Handoff

> For current project truth, defer to [`docs/STATE.md`](../STATE.md), [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md), [`docs/reference/project-index/source-of-truth-map.md`](../reference/project-index/source-of-truth-map.md), and [`ICN_SYSTEM_DEMO_READINESS_MAP.md`](ICN_SYSTEM_DEMO_READINESS_MAP.md). Runtime-side receipt/envelope discipline for the baseline loop lives in PR [#1787](https://github.com/InterCooperative-Network/icn/pull/1787) (`icn-baseline-lock`); this handoff is still **pilot-ui / demo-surface** scope for [#1777](https://github.com/InterCooperative-Network/icn/issues/1777) and does not implement the slice.

## Objective

Add the next narrow fixture-backed step to the guided organizer/member demo:

```text
Standing -> Action Cards -> Governance proposal/vote fixture
```

Current local demo path:

```text
Standing -> Action Cards
```

Later follow-up path:

```text
Standing -> Action Cards -> Governance proposal/vote fixture -> Receipt/provenance fixture
```

This is intentionally a frontend/demo-fixture slice. It should make the Governance tab feel coherent in `?mode=demo` without adding backend demo mode, real signing, real vote submission, live federation, NYCN-specific content, or production claims.

## Current known state

- `?mode=demo` exists as the guided pilot-ui demo entry.
- Member standing is fixture-backed in demo mode.
- Action cards are fixture-backed in demo mode.
- Governance remains gateway-backed / not fully fixture-backed.
- #1727 remains open because full backend/fixture-backed demo mode is not complete.
- #1777 tracks this next narrow governance fixture slice.

## Source files to inspect first

Before editing, re-read the actual source shapes. Do not invent fixture fields from memory.

```text
web/pilot-ui/app.js
web/pilot-ui/index.html
web/pilot-ui/style.css
web/pilot-ui/fixtures/icn-organizer-demo/README.md
web/pilot-ui/fixtures/icn-organizer-demo/standing.json
web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json
web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js
web/pilot-ui/tests/e2e/standing-action-cards.spec.js
docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md
docs/reference/project-index/pilot-ui-current-state-map.md
docs/reference/project-index/runtime-surface-map.md
docs/contracts/institution-package/action-card.schema.json
icn/apps/governance/src/http/
icn/crates/icn-governance/
```

Useful discovery commands when local tooling is available:

```bash
grep -RIn "load.*Proposal\|proposal\|vote\|governance" web/pilot-ui icn/apps/governance/src/http icn/crates/icn-governance | head -200
grep -RIn "me/action-cards\|me/standing\|completion-receipt" web/pilot-ui icn/apps/governance/src/http docs | head -100
grep -RIn "GovernanceDecisionReceipt\|ActionItemCompletionReceipt\|MeetingAttendanceReceipt" icn docs web | head -100
```

## Recommended implementation shape

### 1. Add a governance fixture file

Likely path:

```text
web/pilot-ui/fixtures/icn-organizer-demo/governance-proposals.json
```

The fixture should contain one deterministic fictional proposal/vote scenario.

Minimum content should be derived from actual gateway/UI shapes after re-reading source. Conceptually it should include:

- proposal id;
- proposal title;
- plain-language summary;
- governing body or scope label;
- domain id / relevant scope id if the existing UI expects it;
- authority basis;
- voting status;
- deadline or review window;
- demo holder DID;
- possible vote choices if the UI already renders them safely;
- expected receipt class or receipt expectation if already modeled.

Use fictional identifiers such as:

```text
did:icn:example-organizer-demo-not-live
demo.coop.organizing
demo.proposal.governance-fixture-001
```

Do not include real names, real DIDs, real email addresses, real organizer material, NYCN-specific content, Drive paths, sponsor details, or attendee data.

### 2. Load fixture only in demo mode

Pattern to preserve:

```text
if DEMO_MODE === 'demo'
  load committed fixture JSON
else
  preserve existing gateway behavior
```

Do not change live/gateway behavior for non-demo mode.

### 3. Render the governance fixture in the Governance tab

The goal is read/review rendering first.

If the existing UI supports safe local vote button rendering, actions may be visibly disabled or labeled as fixture-only. Do not simulate a real signed vote unless the existing UI already has a clearly bounded local-only pattern for it.

Recommended copy:

```text
Fixture-backed demo proposal. This is fictional local demo data. No vote is submitted.
```

### 4. Update demo guide/readiness docs

Update:

```text
docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md
docs/reference/project-index/pilot-ui-current-state-map.md
```

Only upgrade the exact slice:

```text
Governance proposal/vote fixture: fixture-backed
```

Do not imply:

```text
all governance surfaces are fixture-backed
receipts are fixture-backed
ledger is fixture-backed
federation is fixture-backed
backend demo mode exists
```

### 5. Add validation/tests

At minimum:

- fixture JSON is reachable;
- fixture JSON has expected required fields;
- demo mode renders the proposal without gateway dependency;
- fixture contains no real-contact/private-data obvious strings;
- non-demo gateway path remains untouched;
- demo copy clearly labels fixture state.

Likely test location:

```text
web/pilot-ui/tests/e2e/
```

The existing demo fixture preload test is a good pattern.

## Acceptance checklist

- [ ] `?mode=demo` Governance tab renders one deterministic fictional proposal/vote fixture.
- [ ] The fixture path is local and committed under `web/pilot-ui/fixtures/icn-organizer-demo/`.
- [ ] No outbound gateway call is required for the fixture-backed proposal/vote display.
- [ ] Existing non-demo governance behavior remains gateway-backed and intact.
- [ ] The surface clearly labels the proposal as fixture-backed fictional demo data.
- [ ] Demo docs are updated only for the specific governance proposal/vote slice.
- [ ] Receipts/provenance fixture coverage remains a follow-up, not silently claimed.
- [ ] No NYCN/private/partner-specific data appears in fixtures or copy.
- [ ] No production, formal NYCN pilot, live federation, private-overlay activation, service-hosting, Tool Commons, or backend demo-mode claim is introduced.
- [ ] Tests or fixture validation cover shape and no-private-data constraints.

## Non-goals

Do not do these in the #1777 implementation PR:

- backend `--demo-mode`;
- schema changes;
- new contract URNs;
- real vote signing/submission;
- real gateway mutation;
- private-overlay activation;
- NYCN-specific governance semantics in ICN core;
- receipt/provenance fixture slice;
- ledger/federation/compute/service-hosting/mobile implementation;
- closure of #1727.

## Likely PR title

```text
demo(pilot-ui): add governance proposal/vote fixture slice
```

## Suggested PR body skeleton

```markdown
## Summary

Adds the next narrow fixture-backed guided pilot-ui demo slice: Governance proposal/vote.

Current local demo path becomes:

Standing -> Action Cards -> Governance proposal/vote fixture

## What changed

- Added fictional governance proposal/vote fixture data.
- Loaded the fixture only when `?mode=demo` is active.
- Preserved gateway-backed governance behavior outside demo mode.
- Updated demo readiness docs to mark only this proposal/vote slice as fixture-backed.
- Added fixture/demo rendering validation.

## Non-goals

- No backend `--demo-mode`.
- No real vote submission.
- No receipt/provenance fixture yet.
- No production, formal NYCN pilot, live federation, private-overlay, service-hosting, or Tool Commons claim.
- Does not close #1727.

## Verification

```bash
cd web/pilot-ui
npm ci
npm run test
npm run test:e2e
npm run test:a11y
```
```

## Follow-up after #1777

Next narrow slice:

```text
Standing -> Action Cards -> Governance proposal/vote fixture -> Receipt/provenance fixture
```

That follow-up should add a fictional receipt/provenance fixture connected to the governance proposal/vote fixture and render a plain-language receipt summary before formal proof/hash details.
