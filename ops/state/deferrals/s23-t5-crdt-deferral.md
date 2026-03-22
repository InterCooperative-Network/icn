# s23-t5 Deferral — #1095 CRDT OrSet + LwwRegister

**Date**: 2026-03-22
**Sprint**: 23 (Baseline Lock)
**Deferred to**: Sprint 24 (Commons Compute Hardening)

## Rationale

`CrdtType::OrSet` and `LwwRegister` are enum variants defined in
`icn-kernel-api/src/coord.rs` with zero concrete implementations.
Implementing CRDT semantics in a stabilization sprint introduces new
unreviewed surface area before the baseline is locked.

This is P2 feature work. Sprint 23's governing rule is convergence, not
coverage. The CRDT implementation belongs in Sprint 24 where Commons
Compute primitives are the primary spine.

## Demo Impact

None. Flows A/B/C do not require OrSet or LwwRegister.

## GitHub

Issue #1095 updated with explicit deferral comment.
Comment: https://github.com/InterCooperative-Network/icn/issues/1095#issuecomment-4106633040
