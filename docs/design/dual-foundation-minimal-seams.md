# Dual-Foundation Minimal-Seams Slice

Status: implemented as a narrow boundary hardening slice.

## What This Slice Changes

This tranche closes two structural gaps without broad refactoring:

1. Economics seam:
   - `icn-compute` no longer has to own credit-requirement meaning directly.
   - A deterministic policy contract callback can provide required credits.
   - Compute keeps mechanical enforcement at submit/admission and reservation seams.
   - Fallback to local formula remains for compatibility when no policy callback is wired.

2. Governance seam:
   - Proposal payloads are now explicitly classified as:
     - `Declarative`
     - `ExecutionRequired`
   - `ExecutionRequired` proposals cannot finalize as accepted without a traceable closure backend.
   - In manager/actor close paths, accepted execution-required proposals fail when closure artifacts cannot be persisted.

## Boundary Definitions

- Policy meaning vs compute enforcement:
  - Policy/app layer decides required credits (deterministic output).
  - Compute actor enforces mechanically (`balance >= required`, reserve/reject).

- Governance acceptance vs execution closure:
  - Acceptance is not sufficient for operational payloads.
  - Execution-required decisions must be closable into durable provenance artifacts (receipt-store backed governance receipt and mandate path, plus allocation artifacts for economic payloads).

## Intentionally Left Out

- No full reservation/settlement redesign.
- No broad CCL/PolicyOracle migration across all compute pricing semantics.
- No new parallel governance receipt systems.
- No payload-by-payload executor redesign; this slice only hardens closure invariants at acceptance seams.
