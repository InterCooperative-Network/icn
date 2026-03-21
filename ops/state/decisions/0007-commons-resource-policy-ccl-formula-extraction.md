# ADR-0007: Commons Resource Policy — CCL Formula Extraction

**Date**: 2026-03-19
**Status**: accepted
**Tags**: meaning-firewall, commons-compute, ccl, compliance
**Sprint**: Sprint 15 (PR #1348, issue #1308)

## Context

The commons credit formula (how much credit a node earns per unit of contributed compute) was hardcoded in the settlement engine — a violation of the meaning firewall. The kernel/settlement layer was encoding domain policy (credit rates, formula shape) that should live in a PolicyOracle.

## Decision

Extract the commons credit formula to `CommonsResourcePolicy`, a CCL PolicyOracle in the `icn-compute` app layer. The settlement engine calls the PolicyOracle to compute credit amounts; it no longer encodes the formula itself.

This enforces the meaning firewall: the kernel (settlement engine) remains blind to what "commons credit" means. Policy can change without touching kernel code.

## Consequences

- Credit formula is configurable per-cooperative via CCL without kernel changes
- Meaning firewall integrity restored in the commons compute path
- CCL test coverage required for policy changes
- CI meaning-firewall gate blocks any future formula drift back into kernel

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Keep formula in settlement engine | Violates meaning firewall — policy logic in kernel |
| Hardcode in configmap | Not auditable, not per-coop, not CCL-governed |
