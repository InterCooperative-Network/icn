# ICN Project Rules (supplements CLAUDE.md)

## Reasoning Foundation

Agent reasoning derives from `docs/ai/ICN_CONSTITUTIONAL_CORE.md`.
Session grounding follows `docs/ai/ICN_SESSION_FRAME_TEMPLATE.md`.
See `docs/ai/WORKFLOW_ARCHITECTURE.md` for the full four-doc architecture.

> Meaning firewall, kernel boundary, error handling, PR conventions, and testing patterns
> are covered in CLAUDE.md and `.claude/rules/{kernel-boundary,rust-core,gateway,deploy}.md`.
> This file contains ONLY rules not covered elsewhere.

## Reducer Purity

- Reducers receive immutable `StateSnapshot`
- Reducers return state delta, not mutated state
- Reducers have NO access to async runtime, network, or time

## Bootstrap Security

- Genesis capabilities expire after 60 seconds
- Running phase denies requests for unregistered domains
- Never allow permanent backdoors
- `AllowAllOracle` active only during genesis bootstrap

## Code Review Quick Checklist

- [ ] No domain imports in kernel crates (enforced by firewall-guard hook)
- [ ] No `unwrap()`/`expect()` in non-test code (enforced by panic-guard hook)
- [ ] Error paths use `ErrCode`, not ad-hoc strings
- [ ] Tests cover error paths, not just happy path
- [ ] TTL/cache values have documented security trade-offs
- [ ] Canon edits labeled: `[sync edit]` vs `[governance edit proposal]`
