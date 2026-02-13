# ICN Project Rules (supplements CLAUDE.md)

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
