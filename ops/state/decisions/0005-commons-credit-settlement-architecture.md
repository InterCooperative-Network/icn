# ADR-0005: Commons Credit Settlement Architecture

**Date**: 2026-02-23
**Status**: accepted
**Tags**: kernel, economics, commons-compute, meaning-firewall
**Sprint**: Sprint 13 (PRs #1280, #1281)

## Context

ICN's commons compute layer needed a closed economic loop: nodes contributing compute resources to the commons should earn credits; nodes consuming commons resources should spend credits. This settlement logic must be enforced by the meaning firewall — the kernel must not encode settlement semantics directly.

## Decision

Settlement is handled by `SettlementEngine::settle_commons_receipt()` in the ledger app (PolicyOracle), invoked via `CommonsSettlementCallback` when a compute task with `scope: ScopeLevel::Commons` is submitted. Key constraints:

1. **Single authoritative gate**: metrics are emitted only from `settle_commons_receipt()` after dedup check, never from generic build helpers
2. **Scope is explicit opt-in**: `ComputeTask.scope` defaults to `Local`; `Commons` must be set explicitly
3. **Idempotency**: settlement is deduplicated by receipt hash before crediting
4. **Metric names**: `icn_compute_receipt_settlement_total{scope="commons"}`, `icn_compute_commons_credits_earned_total`, `icn_compute_commons_credits_spent_total`

Test isolation uses `metrics::with_local_recorder()` + `DebuggingRecorder` (not global install).

## Consequences

- Commons compute has a verifiable, auditable credit trail
- Settlement logic lives in the app layer (PolicyOracle), enforcing the meaning firewall
- Metric callsite is stable; moving it would require grep of production callers first
- `metrics-util` 0.19 locked to `metrics` 0.24 — `CompositeKey::name()` requires `key.key().name()`

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Emit metrics from `build_earn_entry_with_receipt` | Called by test code too — would pollute test metrics |
| Settle in kernel | Violates meaning firewall — kernel cannot encode "commons" semantics |
| Opt-out scope (Commons default) | Too aggressive — most compute tasks are local, not commons |
