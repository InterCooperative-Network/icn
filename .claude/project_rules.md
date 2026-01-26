# ICN Development Rules

## Architectural Rules

### Rule 1: Meaning Firewall
- Kernel crates MUST NOT import `icn-trust`, `icn-governance`, `icn-ccl` or any domain-specific crate
- Apps import domain crates and expose generic `ConstraintSet` to kernel
- Run firewall check: `grep -r 'use icn_trust::' crates/icn-{net,gateway,gossip,ledger}/src && exit 1`

### Rule 2: PolicyOracle is Synchronous
- `PolicyOracle::evaluate()` is sync by design
- Use `parking_lot::RwLock` (not `tokio::sync::RwLock`) for app state accessed in evaluate()
- Tech debt tracked in #874 for async migration

### Rule 3: Reducer Purity
- Reducers receive immutable `StateSnapshot`
- Reducers return state delta, not mutated state
- Reducers have NO access to async runtime, network, or time

### Rule 4: Bootstrap Security
- Genesis capabilities expire after 60 seconds
- Running phase denies requests for unregistered domains
- Never allow permanent backdoors

## Code Review Checklist

- [ ] No domain imports in kernel crates
- [ ] PolicyOracle returns only generic constraints (no trust_score in custom)
- [ ] Reducers are pure (no async, no side effects)
- [ ] Error handling: log before fallback, never silent failures
- [ ] Tests cover error paths, not just happy path
- [ ] TTL/cache values have documented security trade-offs

## Issue Labels

- `kernel-api`: Changes to kernel primitive traits
- `core`: Runtime, supervisor, dispatcher
- `trust`: Trust graph and PolicyOracle
- `ccl`: Cooperative Contract Language
- `meaning-firewall`: Violations of kernel/app separation

## PR Conventions

- Title: `feat|fix|refactor(scope): description`
- Scope: `kernel-api`, `core`, `trust-app`, `ccl`, `net`, `gateway`, `gossip`, `ledger`
- Co-author: Include `Co-Authored-By: claude` for AI-assisted commits
- Squash merge for feature PRs
