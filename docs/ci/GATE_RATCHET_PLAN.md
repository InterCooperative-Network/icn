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

## Schedule

Dates are "no later than." Advancing early is allowed if migration work is complete.

### Wave 1 (by 2026-02-23): turn on visibility and ownership

- Meaning Firewall check: OBSERVATIONAL -> WARNING
- Firewall contract enforcement: OBSERVATIONAL -> WARNING
- Forbidden deps (targeted crates): OBSERVATIONAL -> WARNING
- Coverage job: stays OBSERVATIONAL (report-only)

Definition of done:

- CI prints a clear `WARNING:` line with a remediation link for each warning.
- Owners are listed in this document.

### Wave 2 (by 2026-03-09): enforce kernel boundary on the kernel

- Forbidden deps for `icn-core` and other designated kernel crates:
  - WARNING -> BLOCKING
- Meaning Firewall check (kernel scope only):
  - WARNING -> BLOCKING for listed crates/modules
- Firewall contract enforcement:
  - stays WARNING unless false positives are eliminated

Definition of done:

- PRs that violate forbidden deps in kernel crates fail.
- Failure message includes exact dependency edge and a fix hint.

### Wave 3 (by 2026-03-23): expand enforcement outward

- Extend forbidden deps BLOCKING scope to additional kernel-adjacent crates as declared.
- Promote firewall contract enforcement to BLOCKING if stable.

Definition of done:

- Kernel boundary invariants are mechanically enforced in CI.

## Owners

- Meaning Firewall: @core-arch
- Forbidden deps: @core-arch
- Firewall contracts: @security
- Coverage: @ci

Replace with actual maintainers as needed.

## Current ratchet defaults (this repo)

- `GATE_RATCHET_PHASE_MEANING_FIREWALL=warning`
- `GATE_RATCHET_PHASE_KERNEL_DEPS=blocking`
- `GATE_RATCHET_PHASE_FIREWALL_CONTRACT=warning`
- `GATE_RATCHET_PHASE_COVERAGE=observational`
- `GATE_RATCHET_PHASE_SDK_TESTS=warning`
- `GATE_RATCHET_PHASE_A11Y=warning`
- `GATE_RATCHET_KERNEL_DEPS_SCOPE=core-only`

## Notes

Boundary Hardening prioritizes correctness over velocity. If a check stays WARNING for longer than two sprints, one of these is true:

- the migration is not real,
- the scope is wrong, or
- ownership is missing.

Entropy wins if gates stay advisory.
