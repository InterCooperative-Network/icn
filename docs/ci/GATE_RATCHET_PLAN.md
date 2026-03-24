# CI Gate Ratchet Plan (Q1 2026)

## Purpose

ICN is in Boundary Hardening mode. This plan graduates currently observational checks into enforceable gates without blocking ongoing migration work prematurely.

Invariant: kernel stays domain-agnostic; policy semantics stay in apps; safety checks are enforceable, not advisory.

## Ratchet states

Each check MUST be in exactly one state:

- OBSERVATIONAL: Runs on CI, does not fail PRs.
- WARNING: Runs on CI and posts PR annotation or log warning. Does not fail PRs.
- BLOCKING: Fails CI on violation.

## Graduation rules

A check can graduate only if:

1. Owners are listed (who fixes failures).
2. The failure message includes a remediation pointer (doc, script, or command).
3. Scope is explicit (what it checks, what it ignores).

## Branch protection enforcement

`GATE_RATCHET_PHASE_* = blocking` means the CI job exits 1 on violation. To be mechanically
unbypassable, the job name must also appear in GitHub branch protection required status checks —
a `--admin` merge can otherwise land a violation even when the job fails.

Required checks (branch protection as of 2026-03-24):
`Build Release`, `Test`, `Clippy`, `Format Check`,
`Meaning Firewall Check`, `Kernel Forbidden Dependencies`, `Firewall Contract Enforcement`

When a gate graduates to BLOCKING, add its job name to branch protection via the API:

```bash
gh api repos/InterCooperative-Network/icn/branches/main/protection \
  --method PUT \
  --field 'required_status_checks[contexts][]=<Job Name>' \
  ... # preserve all other fields
```

This step is tracked as a distinct action for each gate in the Wave schedule above.

## Schedule

Dates are "no later than." Advancing early is allowed if migration work is complete.

### Wave 1 (by 2026-02-23): turn on visibility and ownership ✅ COMPLETE

- Meaning Firewall check: OBSERVATIONAL → WARNING ✅
- Firewall contract enforcement: OBSERVATIONAL → WARNING ✅
- Forbidden deps (targeted crates): OBSERVATIONAL → WARNING ✅
- Coverage job: stays OBSERVATIONAL (report-only)

Definition of done:

- CI prints a clear `WARNING:` line with a remediation link for each warning.
- Owners are listed in this document.

### Wave 2 (by 2026-03-09): enforce kernel boundary on the kernel ✅ COMPLETE

- Forbidden deps for `icn-core` and other designated kernel crates:
  - WARNING → BLOCKING ✅ (2026-03-09)
- Meaning Firewall check (kernel scope only):
  - WARNING → BLOCKING ✅ (advanced 2026-03-24; 15 days after deadline, all violations resolved)
- Firewall contract enforcement:
  - stays WARNING unless false positives are eliminated (graduated in Wave 3)

Definition of done:

- PRs that violate forbidden deps in kernel crates fail.
- Failure message includes exact dependency edge and a fix hint.

**Wave 2 fully complete** (2026-03-24): `Meaning Firewall Check` added to branch protection required checks.

### Wave 3 (by 2026-03-23): expand enforcement outward ✅ COMPLETE

- Extend forbidden deps BLOCKING scope to additional kernel-adjacent crates as declared.
- Promote firewall contract enforcement to BLOCKING if stable.
  - Advanced 2026-03-24 (1 day after deadline; passing consistently on main).

Definition of done:

- Kernel boundary invariants are mechanically enforced in CI.

**Wave 3 fully complete** (2026-03-24): `Firewall Contract Enforcement` added to branch protection required checks.

### Wave 4 (target: Q2 2026): graduate remaining advisory signals

#### Regulatory Compliance Linter (`GATE_RATCHET_PHASE_COMPLIANCE`)
Current phase: WARNING

Graduation condition:
- Zero compliance violations on main for 2+ consecutive sprints.
- Script covers all new public-facing API surfaces added since linter was introduced.

Graduation path: WARNING → BLOCKING → add to branch protection required checks.

Owner: @core-arch

#### TypeScript SDK Tests (`GATE_RATCHET_PHASE_SDK_TESTS`) ✅ GRADUATED (2026-03-24)
Phase advanced to BLOCKING. `continue-on-error` removed.

Investigation confirmed: the "tests may need gateway running" comment was incorrect. All 157 tests
use injected `mockFetch` and have no real network dependency. Tests run in 6 seconds, fully
isolated. Build + tests pass cleanly on Node 22.

Next step: add `TypeScript SDK` to branch protection required checks.

Owner: @sdk

#### Accessibility Tests (`GATE_RATCHET_PHASE_A11Y`)
Current phase: WARNING (with `continue-on-error: true`)

Graduation condition:
- Zero accessibility violations in a full Playwright chromium run against a production build.
- Passing consistently for 2+ sprints (no flakes).
- Remove `continue-on-error` from the accessibility job.

Graduation path: WARNING → BLOCKING → add to branch protection required checks.

Owner: @frontend

### Coverage (permanently observational)
`GATE_RATCHET_PHASE_COVERAGE=observational`

Coverage is a reporting signal, not a quality gate. Thresholds change as the codebase grows;
enforcing a fixed threshold would create perverse incentives. Coverage stays OBSERVATIONAL
permanently. Reports upload to Codecov for trend analysis only.

Note: runs on a self-hosted `zentith` runner (`nohup` process in WSL2) that must be restarted
manually after Windows reboots. If coverage goes dark, restart the runner. See MEMORY.md for
restart command.

## Owners

- Meaning Firewall: @core-arch
- Forbidden deps: @core-arch
- Firewall contracts: @security
- Compliance linter: @core-arch
- SDK tests: @sdk
- Accessibility: @frontend
- Coverage: @ci

## Current ratchet defaults (as of 2026-03-24)

| Variable | Value | Job Name |
|----------|-------|----------|
| `GATE_RATCHET_PHASE_MEANING_FIREWALL` | `blocking` | Meaning Firewall Check |
| `GATE_RATCHET_PHASE_KERNEL_DEPS` | `blocking` | Kernel Forbidden Dependencies |
| `GATE_RATCHET_PHASE_FIREWALL_CONTRACT` | `blocking` | Firewall Contract Enforcement |
| `GATE_RATCHET_PHASE_COMPLIANCE` | `warning` | Regulatory Compliance Linter |
| `GATE_RATCHET_PHASE_SDK_TESTS` | `blocking` | TypeScript SDK |
| `GATE_RATCHET_PHASE_A11Y` | `warning` | Accessibility Tests |
| `GATE_RATCHET_PHASE_COVERAGE` | `observational` | Test Coverage |

Branch protection required checks (must be updated separately when gates graduate):
`Build Release`, `Test`, `Clippy`, `Format Check`

Added to required checks (gap closed 2026-03-24):
`Meaning Firewall Check`, `Kernel Forbidden Dependencies`, `Firewall Contract Enforcement`

Blocking but not yet in required checks (next action):
`TypeScript SDK`

## Notes

Boundary Hardening prioritizes correctness over velocity. If a check stays WARNING for longer than
two sprints, one of these is true:

- the migration is not real,
- the scope is wrong, or
- ownership is missing.

Entropy wins if gates stay advisory.
