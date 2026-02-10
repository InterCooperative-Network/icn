---
name: icn-kernel-app-separation-extraction
description: "Reduce kernel app coupling with safe seam refactors while preserving behavior. Use for meaning firewall and boundary extraction work in ICN."
---

# ICN Kernel/App Separation Extraction

Use this skill to reduce kernel-app coupling while preserving behavior.

## Inputs

- PR diff or branch state.
- Current ratchet budget and failing test names.
- Target crates (usually `icn-core` plus app crates).

## Workflow

1. Diagnose coupling leak points.
- Locate direct app-layer imports in kernel-adjacent crates.
- Locate kernel code consuming domain/business-rule types.
- Locate convenience helpers pulling heavy app dependencies into core paths.

2. Choose seam pattern.
- Pattern A: boundary adapter module in app crate implementing kernel-facing trait.
- Pattern B: thin re-export shim in a boundary-safe crate.
- Pattern C: move serialization/DTO conversion to the edge.
- Pattern D: trait plus newtype boundary to prevent dependency creep.

3. Plan small, reviewable steps.
- Keep each step compiling.
- Keep behavior and trust gates unchanged.
- Keep protocol determinism and canonical encoding unchanged.

4. Apply minimal diff.
- Rewrite imports through seam.
- Keep runtime execution path identical unless explicitly required.
- Preserve fail-closed behavior.

5. Verify.
```bash
cd icn
cargo test -p icn-core strict_core_governance_reference_ratchet --lib
cargo test -p icn-core --lib
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
- Add target-specific tests if touched modules require them.

## Anti-Patterns

- Do not alias imports only to satisfy lexical counters without creating a real seam.
- Do not move policy semantics into kernel crates.
- Do not relax auth/trust/validation checks to pass tests.
- Do not change wire/proof/encoding semantics without explicit intent and docs.

## Output Contract

Return:
1. coupling diagnosis (file-level),
2. refactor plan (ordered, small steps),
3. diff shape (files/modules/import rewrites),
4. verification commands and outcomes,
5. residual risks or follow-up work.
