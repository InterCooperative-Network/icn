---
description: Trace a feature or concept across crates, routes, tests, and docs — produce a full cross-cutting map
allowed-tools: Read, Grep, Glob, Bash(cargo metadata:*, cargo tree:*, find:*, rg:*)
---

Trace the feature or concept described by the user across the entire ICN workspace. Produce a complete cross-cutting map.

**Input:** The user has named a feature, concept, crate, or behavior to trace (e.g. "mutual credit settlement", "DID resolution", "governance proposal flow", "CCL execution").

**Step 1: Identify entry points**
Search for the concept in:
- Cargo workspace members (`cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys; d=json.load(sys.stdin); [print(p['name'],p['manifest_path']) for p in d['packages']]"`)
- Source files: `rg -l "<concept>" crates/ bins/`
- Test files: `rg -l "<concept>" crates/**/tests/ crates/**/*_test.rs`
- Docs: `rg -l "<concept>" docs/`

**Step 2: Map crate ownership**
For each entry point, identify which crate owns it, what layer it's in (kernel/app), and what other crates it imports from.

**Step 3: Trace the data flow**
Follow the feature through:
- Where it originates (user call / network message / CCL contract / governance event)
- Which actors handle it
- What state mutations occur and which stores are touched
- What events or messages are emitted downstream
- Where it terminates (response / ledger entry / gossip message)

**Step 4: Find the API surface**
- REST routes in `icn-gateway` that expose this feature
- JSON-RPC methods in `icn-rpc`
- `icnctl` subcommands in `bins/icnctl`
- TypeScript SDK bindings in `sdk/`

**Step 5: Find the tests**
- Unit tests in the owning crate
- Integration tests in `crates/icn-testkit/`
- End-to-end tests in `demo/` or `scripts/`
- Missing test coverage (flag explicitly)

**Step 6: Check docs**
- Which docs/ files describe this feature
- Is the docs state current with the code (check dates / git blame)
- Any ADRs related to this feature

**Output format:**
```
## Feature Trace: <concept>

### Crate Map
| Crate | Layer | Role | Key Types/Traits |
|-------|-------|------|-----------------|
...

### Data Flow
1. Entry: ...
2. ...
N. Exit: ...

### API Surface
- REST: POST /api/v1/...
- RPC: ...
- CLI: icnctl ...

### Tests
- [x] Unit: crates/icn-xxx/tests/...
- [ ] Integration: MISSING (note gap)
- [x] E2E: demo/...

### Docs
- docs/feature-name.md (up to date / stale)
- ADR: docs/architecture/adr-NNN.md (or: no ADR)

### Gaps & Risks
- ...
```
