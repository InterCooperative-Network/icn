# Spring Cleaning Inventory — 2026-02-23

> One change-class per PR. No semantic changes unless forced by CI or a documented bug.
> Labels: ✅ safe micro-PR | ⚠️ needs decision | ❌ not worth it

---

## Phase 1: CI + Automation Rakes

### A) Coverage flake — `continue-on-error` / `outcome` mismatch ✅

**Root cause**: In `.github/workflows/ci.yml`, the "Generate coverage" step uses:
```yaml
continue-on-error: ${{ env.GATE_RATCHET_PHASE_COVERAGE != 'blocking' }}
```
When tarpaulin crashes (known spurious failures), `continue-on-error` sets the step
`conclusion` to `success` but the real `outcome` is `failure`. The downstream "Upload
coverage to Codecov" step likely checks `steps.generate_coverage.outcome` — which sees
`failure` — and skips. The job then marks overall failure because the intended steps
didn't run, even though the gate was supposed to be non-blocking.

**Fix**: Promote `continue-on-error` from the step level to the job level, or make the
upload step conditional on the file existing rather than step outcome:
```yaml
- name: Upload coverage to Codecov
  if: hashFiles('./icn/coverage/cobertura.xml') != ''
```

**PR**: `ci: make coverage upload resilient to tarpaulin spurious failures`

---

### B) Benchmark "Compare Against Base" — not paths-filtered ⚠️

**Location**: `.github/workflows/benchmark.yml`

The benchmark compare job runs on PRs that only add a new crate with zero performance
surface (e.g., icn-authz, pure docs PRs). It then flakes because there's nothing to
compare against.

**Decision needed**: Add paths-filter so benchmark compare only runs when:
- `icn/crates/**` modified (non-new-crate changes)
- Not `icn/crates/icn-authz/**` (pure addition, no existing perf regression)

**PR**: `ci: skip benchmark compare on new-crate-only or docs-only PRs`

---

## Phase 2: Repository Topology / Root Confusion

### C) No `.gitattributes` → CRLF churn accumulates ✅

**Evidence**: Stashes `@{1}`, `@{2}`, `@{3}` were entirely CRLF normalization noise
(175 add / 175 del same file, 2000+ line diffs). Zero signal, maximum noise.

**Fix**: Add `.gitattributes` at repo root enforcing LF for all text files:
```
* text=auto eol=lf
*.rs text eol=lf
*.toml text eol=lf
*.md text eol=lf
*.yml text eol=lf
*.yaml text eol=lf
*.ts text eol=lf
*.json text eol=lf
*.sh text eol=lf
```

**Watch**: Run `git diff --stat` after adding `.gitattributes`. If the normalization
commit is >500 lines, stage it separately. If it explodes (thousands of files), apply
only to high-churn file types first.

**PR**: `chore: add .gitattributes to enforce LF line endings`

---

### D) SDK path inconsistency in old docs ⚠️

**Evidence**: Several archived session docs and the sprint docs reference:
- `cd icn/sdk/typescript` (wrong — SDK is at monorepo root)
- `cd ../sdk/typescript` (relative, fragile)
- `/home/matt/projects/icn/sdk/...` (hardcoded to a different user's path)

**Affected files** (examples):
- `docs/development/sprints/sprint-2026-02-20.md`: `cd icn/sdk/typescript`
- `docs/development/sprints/sprint-2026-02-19.md`: `cd ../sdk/typescript`
- `docs/testing/MOBILE_APP_TESTING_GUIDE.md`: `/home/matt/projects/icn/sdk/react-native/...`

**Decision**: These are historical session notes — not active runbooks. Archived docs are
expected to be wrong. Active docs (CLAUDE.md, development guides) are already correct.

**Verdict**: ❌ Not worth touching archived session notes. Fix only if a doc is actively
used as a runbook.

---

### E) `whereami.sh` helper script ✅

**Purpose**: Executable sanity check referenced in CLAUDE.md. Prevents root-confusion
class of errors permanently.

```bash
#!/usr/bin/env bash
# scripts/whereami.sh — prints which repo root you're in
echo "git top-level: $(git rev-parse --show-toplevel)"
test -f Cargo.toml && echo "✓ Rust workspace root (Cargo.toml found)" || echo "✗ Not Rust root"
test -f sdk/typescript/package.json && echo "✓ Monorepo root (sdk/typescript/package.json found)" || echo ""
```

**PR**: `scripts: add whereami.sh root-sanity helper`

---

## Phase 3: Production Code Rakes

### F) `unimplemented!()` in production paths ⚠️

**Locations**:
- `icn/crates/icn-core/src/resource_enforcer_actor.rs:511,582,671` — 3 arms
- `icn/crates/icn-gateway/src/api/names.rs:199-235` — 8 arms (names API stubs)

**Context**: The names API stubs are expected placeholders for `icn-naming` integration
(active work). The resource_enforcer stubs may be reachable in tests.

**Decision**: `unimplemented!()` in request-handling paths is a panic vector. Options:
1. Replace with `GatewayError::NotImplemented` returns (correct)
2. Annotate with `#[allow(unreachable_code)]` + explicit comment if unreachable
3. Leave if guarded by feature flag or test-only

**PR**: `fix(gateway): replace unimplemented!() stubs with proper NotImplemented errors`

---

### G) 84 TODO/FIXME comments — categorized ✅ / ⚠️

**Volume**: 84 markers. Most are valid deferred-work notes, not actionable now.

**High-signal cluster** (should become GitHub issues):
- `icn-ledger/src/commons_credits.rs:26,56` — governance configurability (concrete, scoped)
- `icn-rpc/src/server.rs:204` + handler/*.rs — TODO(#769) coop enforcement (8 handlers, linked issue)
- `icn-core/src/apps/dispatcher.rs:185` — TODO(#873) copy-on-write StateSnapshot
- `icn-core/src/replication/adjuster.rs:233` — TODO(#924) federation gossip integration

**Low-signal / safe to leave**:
- Phase 2.3 stubs (scheduled future work)
- Hardware key TODOs (TPM, PQ — long-horizon)
- Trust optimization TODOs (premature optimization guard, already noted)

**Verdict**: File issues for the high-signal cluster. Leave the rest.

**PR**: None — file GitHub issues directly, no code change.

---

## Phase 4: Documentation Hygiene

### H) `docs/REORGANIZATION_2026.md` references "no longer actively referenced" docs ⚠️

The file itself calls out documents that are stale. Worth a quick pass to either delete
the referenced stale docs or move them to `docs/archive/`.

**Verdict**: ⚠️ Needs doc audit, not a code change. Low priority.

---

### I) `QUICKSTART.md` stale (was in CRLF stash) ❌

The stash showed a 175-add/175-del CRLF normalization on `QUICKSTART.md`. If the
`.gitattributes` PR lands first, this resolves as a side effect.

**Verdict**: ❌ Blocked on `.gitattributes` PR. Handle as normalization commit.

---

## Execution Order (safe sequencing)

| # | PR | Risk | Blocked by |
|---|-----|------|-----------|
| 1 | `ci: coverage upload resilient to tarpaulin flakes` | Low | Nothing |
| 2 | `chore: .gitattributes LF enforcement` | Low-Medium | Nothing (watch diff size) |
| 3 | `scripts: add whereami.sh` | Low | Nothing |
| 4 | `fix(gateway): replace unimplemented!() stubs` | Medium | PR #2 (branch hygiene) |
| 5 | `ci: benchmark compare paths-filter` | Low | Decision on policy |

---

## Out of Scope (for this sprint)

- Refactoring gateway module layout
- Upgrading any dependencies
- Fixing all clippy warnings (7886 unwrap() calls — mostly test code)
- Touching archived session docs
- "While I'm in here" changes of any kind

---

*Inventory produced 2026-02-23 via Phase 1 sweep: rg TODO/FIXME, .gitattributes check,
CI workflow analysis, production unimplemented!() grep.*
