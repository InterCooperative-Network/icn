# Docs Reality Audit - 2026-02-11

## Scope

Initial code-first documentation alignment pass, focused on core orientation docs and skill bootstrap for repeatable drift cleanup.

## Canonical truth sources used

- `AGENTS.md`
- `icn/Cargo.toml`
- `.github/workflows/*.yml`
- `icn/bins/*/Cargo.toml`
- `docs/INDEX.md`

## Changes applied (Batch A1)

1. `docs/README.md`
- Replaced non-existent `dev-journal/` reference with `docs/development/sessions/`.
- Replaced non-existent `decisions/` reference with `docs/adr/`.

2. `docs/STATE.md`
- Removed dead `docs/dev-journal/ROADMAP.md` references.
- Repointed roadmap mention to `docs/development/sessions/undated/ROADMAP.md` and architecture direction to `docs/architecture/KERNEL_APP_SEPARATION.md`.

3. `docs/ARCHITECTURE.md`
- Clarified canonical app topology (`icn/apps/*`) with explicit note that top-level `apps/*` is transitional.
- Fixed broken production hardening link to `docs/security/production-hardening.md`.

4. Skill created: `.codex/skills/icn-docs-reality-sync/`
- Added `SKILL.md` with recursive self-correction loop.
- Added references:
  - `references/reality-sources.md`
  - `references/mismatch-rubric.md`
- Added scanner:
  - `scripts/doc_reality_scan.sh`

## Mismatch table (open items after Batch A1)

| Severity | Doc | Mismatch | Truth source | Planned batch |
|---|---|---|---|---|
| blocker | `docs/PHASE_HISTORY.md` | broken local links to design docs (`scheduler-evolution-plan.md`, `multi-device-identity-design.md`, `capability-based-features.md`) | files exist under `docs/design/` | A2 |
| high | `docs/architecture/KERNEL_APP_SEPARATION.md` | examples and rules still imply top-level `apps/*` as canonical | `AGENTS.md`, `docs/adr/ADR-0010-app-topology.md` | B1 |
| high | `docs/ci/CI_CURRENT_STATUS.md` and status docs | stale "all green / ready now" claims from 2026-01-20 and older point-in-time docs | workflow files + dated status policy | D1 |
| high | multiple active docs under `docs/guides/`, `docs/reference/`, `docs/operations/` | broken relative links from prior reorganizations | repo path map + scanner output | A2/A3 |

## Verification commands and outcomes

```bash
python3 /home/ubuntu/.codex/skills/.system/skill-creator/scripts/quick_validate.py .codex/skills/icn-docs-reality-sync
```

Result: `Skill is valid!`

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: scanner runs successfully and reports remaining mismatches.

## Recursive self-correction score (Batch A1)

- Accuracy: 4/5
- Completeness: 3/5
- Consistency: 4/5
- Verifiability: 4/5

Trigger status: `Completeness < 4` -> continue with Batch A2.

## Changes applied (Batch A2)

1. `docs/architecture/ARCHITECTURE_INDEX.md`
- Fixed multiple broken references introduced by docs reorganization:
  - `./docs/*` paths -> correct `../*` and `../guides/*` targets
  - security/design/ops links pointed to current locations
  - root repo links corrected to `../../CONTRIBUTING.md` and `../../CHANGELOG.md`
  - roadmap link changed to historical session roadmap path

2. `docs/architecture/ARCHITECTURE_QUICK_REF.md`
- Fixed `./docs/ARCHITECTURE.md` and `./docs/GETTING_STARTED.md` links to valid `../` targets.

3. Deployment guides:
- `docs/deployment/DEPLOYMENT_GUIDE.md`
- `docs/deployment/DEPLOYMENT_COMPLETE.md`
- `docs/deployment/DEPLOYMENT_READY.md`
- `docs/deployment/QUICK_DEPLOY.md`
- Repointed broken references (`docs/ARCHITECTURE.md`, `docs/api/`, `docs/production-hardening.md`, `MOBILE_APP_STATUS.md`) to current docs paths.

## Changes applied (Batch A3)

1. `docs/PHASE_HISTORY.md`
- Fixed broken links:
  - `KERNEL_APP_SEPARATION.md` -> `architecture/KERNEL_APP_SEPARATION.md`
  - `scheduler-evolution-plan.md` -> `design/scheduler-evolution-plan.md`
  - `multi-device-identity-design.md` -> `design/multi-device-identity-design.md`
  - `capability-based-features.md` -> `design/capability-based-features.md`
  - `production-hardening.md` -> `security/production-hardening.md`
- Applied CRLF-safe, in-place substitutions to avoid noisy full-file rewrite.

## Mismatch table (open items after Batch A3)

| Severity | Doc | Mismatch | Truth source | Planned batch |
|---|---|---|---|---|
| high | session/status historical docs under `docs/development/sessions/` | stale "all green / ready for production" phrasing without point-in-time labeling | dated status policy + workflow files | D2 |
| high | multiple active docs under `docs/guides/`, `docs/reference/`, `docs/operations/` | broken relative links from prior reorganizations | repo path map + scanner output | A3/A4 |

## Verification updates (Batch A2)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/(architecture/ARCHITECTURE_INDEX.md|architecture/ARCHITECTURE_QUICK_REF.md|deployment/DEPLOYMENT_GUIDE.md|deployment/DEPLOYMENT_COMPLETE.md|deployment/DEPLOYMENT_READY.md|deployment/QUICK_DEPLOY.md)\\|"
```

Result: no matches (all targeted links resolved).

## Recursive self-correction score (Batch A2)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 4/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch A2; proceed to A3 for remaining blockers/highs.

## Verification updates (Batch A3)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/PHASE_HISTORY.md\\|"
```

Result: no matches (PHASE_HISTORY blocker closed).

## Recursive self-correction score (Batch A3)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 4/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch A3; continue with B1 and D1.

## Changes applied (Batch B1)

1. `docs/architecture/KERNEL_APP_SEPARATION.md`
- Updated topology language to match canonical app root rule:
  - Canonical runtime app location: `icn/apps/*`
  - Top-level `apps/*` documented as legacy/transitional during migration
- Updated example and appendix wording to avoid presenting `apps/*` as canonical.

## Changes applied (Batch D1)

1. `docs/ci/CI_CURRENT_STATUS.md`
- Replaced stale "all green/deploy now" narrative with a dated operational snapshot format.
- Added explicit authoritative sources and re-verification commands.
- Added update protocol requiring date, branch/commit, commands, outcomes, and blockers.

## Verification updates (Batch B1 + D1)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/(PHASE_HISTORY.md|architecture/KERNEL_APP_SEPARATION.md|ci/CI_CURRENT_STATUS.md|README.md|STATE.md|ARCHITECTURE.md|architecture/ARCHITECTURE_INDEX.md|architecture/ARCHITECTURE_QUICK_REF.md|deployment/DEPLOYMENT_GUIDE.md|deployment/DEPLOYMENT_COMPLETE.md|deployment/DEPLOYMENT_READY.md|deployment/QUICK_DEPLOY.md)\\|"
```

Result: no matches for targeted core docs.

```bash
rg -n "ALL GREEN|READY FOR PRODUCTION DEPLOYMENT|Recommendation: \\*\\*DEPLOY NOW\\*\\*" docs/ci/CI_CURRENT_STATUS.md docs/status/*.md docs/development/sessions/**/*.md
```

Result: no stale claims in `docs/ci/CI_CURRENT_STATUS.md`; remaining hits are in historical session docs.

## Recursive self-correction score (Batch B1 + D1)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 4/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for this batch; continue with A4/D2 backlog.

## Changes applied (Batch A4 - slice 1)

1. `docs/guides/developer/DEV_ENVIRONMENT.md`
- Fixed related-doc links to current paths:
  - `HOMELAB_DEPLOYMENT.md` -> `docs/operations/deployment/HOMELAB_DEPLOYMENT.md`
  - `GETTING_STARTED.md` -> `docs/GETTING_STARTED.md`
  - `ARCHITECTURE.md` -> `docs/ARCHITECTURE.md`

## Verification updates (Batch A4 - slice 1)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/guides/developer/DEV_ENVIRONMENT.md\\|"
```

Result: no matches.

## Changes applied (Batch A4 - slice 2)

1. `docs/guides/operations/operations-guide.md`
- Repointed deployment/incident/changelog/roadmap links to current locations.

2. `docs/guides/operations/backup-and-recovery.md`
- Repointed multi-device, deployment, and architecture links to current paths.

3. `docs/guides/operations/replication-operations.md`
- Repointed architecture/roadmap/changelog links.
- Replaced obsolete `docs/dev-journal/` reference with `docs/development/sessions/`.

4. `docs/guides/operations/troubleshooting.md`
- Repointed incident response and production hardening links.

5. `docs/reference/api/api-versioning.md`
- Repointed architecture/capability/deployment/changelog references.

6. `docs/reference/api/topic-subscriptions-api.md`
- Repointed architecture and CLAUDE references to valid paths.

## Verification updates (Batch A4 - slice 2)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/(guides/operations/operations-guide.md|guides/operations/backup-and-recovery.md|guides/operations/replication-operations.md|guides/operations/troubleshooting.md|reference/api/api-versioning.md|reference/api/topic-subscriptions-api.md)\\|"
```

Result: no matches.

## Changes applied (Batch A4 - slice 3)

1. `docs/reference/config/identity-backend-configuration.md`
- Fixed architecture reference path.

2. `docs/reference/config/trust-threshold-configuration.md`
- Fixed architecture, production-hardening, and deployment guide references.

3. `docs/operations/deployment/deployment-guide.md`
- Fixed config/docker links to top-level directories.
- Fixed architecture/security/API references to current docs paths.

4. `docs/operations/deployment/distributed-tracing.md`
- Fixed production hardening, Kubernetes deployment, and architecture references.

5. `docs/operations/deployment/HOMELAB_DEPLOYMENT.md`
- Fixed `deploy/k8s/README.md` and developer environment link paths.

## Verification updates (Batch A4 - slice 3)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/(reference/config/identity-backend-configuration.md|reference/config/trust-threshold-configuration.md|operations/deployment/deployment-guide.md|operations/deployment/distributed-tracing.md|operations/deployment/HOMELAB_DEPLOYMENT.md)\\|"
```

Result: no matches.

## Mismatch table (open after A4 slices 1-3)

| Severity | Doc set | Mismatch | Truth source | Planned batch |
|---|---|---|---|---|
| high | additional active docs under `docs/guides/*`, `docs/reference/*`, `docs/operations/*` not yet touched | residual broken links from historical reorg | scanner output + path map | A4 (next slices) |
| high | historical session/status docs under `docs/development/sessions/` | stale “all green / ready now” language without explicit historical framing | dated status policy + workflow files | D2 |

## Recursive self-correction score (A4 slices 2-3)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 4/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue with remaining A4 slices.

## Changes applied (Batch A4 - slice 4)

1. `docs/guides/FAQ.md`
- Repointed broken links for design/internal/core docs:
  - NAT traversal, multi-device identity, legal considerations
  - economic safety, governance primitives
  - roadmap/contributing/getting started/architecture/operations guide

2. `docs/guides/user/cooperative-setup-guide.md`
- Repointed broken links for onboarding and references:
  - getting started, backup and recovery, architecture observability anchor
  - governance primitives, economic safety, trust threshold config
  - API reference, glossary

## Verification updates (Batch A4 - slice 4)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/(guides/FAQ.md|guides/user/cooperative-setup-guide.md)\\|"
```

Result: no matches.

## Consolidated verification (A4 slices 2-4)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg -n "MISSING_LINK\\|docs/(guides/FAQ.md|guides/user/cooperative-setup-guide.md|guides/operations/operations-guide.md|guides/operations/backup-and-recovery.md|guides/operations/replication-operations.md|guides/operations/troubleshooting.md|reference/api/api-versioning.md|reference/api/topic-subscriptions-api.md|reference/config/identity-backend-configuration.md|reference/config/trust-threshold-configuration.md|operations/deployment/deployment-guide.md|operations/deployment/distributed-tracing.md|operations/deployment/HOMELAB_DEPLOYMENT.md)\\|"
```

Result: no matches.

## Mismatch table (open after A4 slices 1-4)

| Severity | Doc set | Mismatch | Truth source | Planned batch |
|---|---|---|---|---|
| high | remaining active docs outside already-patched sets | residual broken links from historical reorg | scanner output + path map | A4 (next slices) |
| high | historical session/status docs under `docs/development/sessions/` | stale “all green / ready now” language without explicit historical framing | dated status policy + workflow files | D2 |

## Recursive self-correction score (A4 slice 4)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 4/5
- Verifiability: 5/5

Trigger status: all scores >= 4; proceed to next A4 slices and D2.

## Changes applied (Batch A4 - slice 5: design/economics links)

1. `docs/design/COMMONS_EVOLUTION.md`
- Repointed architecture/governance/security links to current locations:
  - `../ARCHITECTURE.md`
  - `governance/governance-primitives.md`
  - `../security/threat-model.md`

2. `docs/design/compute-substrate-design.md`
- Fixed missing references for gap analysis, economics safety, governance, and repo root guidance:
  - `../architecture/IMPLEMENTATION_GAP_ANALYSIS.md`
  - `economics/economic-safety.md`
  - `governance/governance.md`
  - `../../CLAUDE.md`

3. `docs/design/economics/ECONOMIC_VISION.md`
- Corrected roadmap and architecture links to canonical docs paths.

4. `docs/design/economics/contribution-credits-design.md`
- Corrected roadmap and federation roadmap links to current docs/development paths.

5. `docs/design/economics/econ-modeling.md`
- Corrected simulation references to `sims/mutual-credit/` from the economics subdirectory depth.

6. `docs/design/economics/economic-safety.md`
- Corrected roadmap, operations, and implementation source links:
  - `../../development/sessions/undated/ROADMAP.md`
  - `../../operations/deployment/incident-response.md`
  - `../../guides/operations/operations-guide.md`
  - `../../../icn/crates/icn-ledger/src/credit_policy.rs`
  - `../../../icn/crates/icn-ledger/src/dispute.rs`

## Verification updates (Batch A4 - slice 5)

```bash
for f in docs/design/compute-substrate-design.md docs/design/economics/ECONOMIC_VISION.md docs/design/economics/contribution-credits-design.md docs/design/economics/econ-modeling.md docs/design/economics/economic-safety.md; do
  ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg "^MISSING_LINK\|${f//\//\/}\|" || true
done
```

Result: no missing-link matches for the targeted design/economics docs.

## Audit ledger updates (slice 5)

- `docs/design/COMMONS_EVOLUTION.md | docs/ARCHITECTURE.md, docs/design/governance/governance-primitives.md, docs/security/threat-model.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/compute-substrate-design.md | docs/architecture/IMPLEMENTATION_GAP_ANALYSIS.md, docs/design/economics/economic-safety.md, docs/design/governance/governance.md, CLAUDE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/economics/ECONOMIC_VISION.md | docs/development/sessions/undated/ROADMAP.md, docs/ARCHITECTURE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/economics/contribution-credits-design.md | docs/development/sessions/undated/ROADMAP.md, docs/development/federation-roadmap-implementation.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/economics/econ-modeling.md | sims/mutual-credit/RESULTS_SUMMARY.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/economics/economic-safety.md | docs/development/sessions/undated/ROADMAP.md, docs/operations/deployment/incident-response.md, docs/guides/operations/operations-guide.md, icn/crates/icn-ledger/src/credit_policy.rs, icn/crates/icn-ledger/src/dispute.rs | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 5)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue to next drift slice.

## Changes applied (Batch A4 - slice 6: governance design links)

1. `docs/design/governance/PROJECT_GOVERNANCE.md`
- Fixed broken root-level governance links:
  - `../../../CONTRIBUTING.md`
  - `../../../CODE_OF_CONDUCT.md`
  - `../../development/sessions/undated/ROADMAP.md`

2. `docs/design/governance/governance.md`
- Fixed architecture, social recovery, and roadmap links:
  - `../../ARCHITECTURE.md`
  - `../sdis/social-recovery.md`
  - `../../development/sessions/undated/ROADMAP.md`

3. `docs/design/governance/witness-trust-validation.md`
- Fixed trust/governance/config/security references:
  - `../../development/trust-multi-graph-migration.md`
  - `../../development/sessions/2026-01/2025-01-17-governance-ledger-integration.md`
  - `../../reference/config/trust-threshold-configuration.md`
  - `../../security/production-hardening.md`

## Verification updates (Batch A4 - slice 6)

```bash
for f in docs/design/governance/PROJECT_GOVERNANCE.md docs/design/governance/governance.md docs/design/governance/witness-trust-validation.md; do
  ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg "^MISSING_LINK\|${f//\//\/}\|" || true
done
```

Result: no missing-link matches for targeted governance design docs.

## Audit ledger updates (slice 6)

- `docs/design/governance/PROJECT_GOVERNANCE.md | CONTRIBUTING.md, CODE_OF_CONDUCT.md, docs/development/sessions/undated/ROADMAP.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/governance/governance.md | docs/ARCHITECTURE.md, docs/design/sdis/social-recovery.md, docs/development/sessions/undated/ROADMAP.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/governance/witness-trust-validation.md | docs/development/trust-multi-graph-migration.md, docs/development/sessions/2026-01/2025-01-17-governance-ledger-integration.md, docs/reference/config/trust-threshold-configuration.md, docs/security/production-hardening.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 6)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue to next drift slice.

## Changes applied (Batch A4 - slice 7: design platform links)

1. `docs/design/capability-based-features.md`
- Fixed crate source links from `docs/design/` depth to canonical `icn/` paths.
- Updated moved module links:
  - `icn-net/src/actor/mod.rs`
  - `icn-obs/src/metrics/mod.rs`

2. `docs/design/scheduler-evolution-plan.md`
- Corrected compute module links to `../../icn/crates/icn-compute/...`.
- Corrected roadmap and project guidance links:
  - `../development/sessions/undated/ROADMAP.md`
  - `../../CLAUDE.md`
- Repointed strategic gap analysis to dated session doc path.

3. `docs/design/razeto-integration-design.md`
- Corrected related document links:
  - `economics/contribution-credits-design.md`
  - `../ARCHITECTURE.md`
  - `../PHASE_HISTORY.md`

## Verification updates (Batch A4 - slice 7)

```bash
for f in docs/design/capability-based-features.md docs/design/scheduler-evolution-plan.md docs/design/razeto-integration-design.md; do
  ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg "^MISSING_LINK\|${f//\//\/}\|" || true
done
```

Result: no missing-link matches for targeted design docs.

## Audit ledger updates (slice 7)

- `docs/design/capability-based-features.md | icn/crates/icn-net/src/{version.rs,protocol.rs,actor/mod.rs}, icn/crates/icn-obs/src/metrics/mod.rs | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/scheduler-evolution-plan.md | icn/crates/icn-compute/src/{actor/mod.rs,scheduler.rs}, docs/development/sessions/undated/ROADMAP.md, CLAUDE.md, docs/development/sessions/2026-01/2025-01-15-strategic-gap-analysis.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/design/razeto-integration-design.md | docs/design/economics/contribution-credits-design.md, docs/ARCHITECTURE.md, docs/PHASE_HISTORY.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 7)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue to next drift slice.

## Changes applied (Batch A4 - slice 8: api/deploy/development links)

1. `docs/api/README.md`
- Repointed pilot coordinator guide link to `docs/internal/pilots/pilot-coordinator-guide.md`.

2. `docs/demo/DEMO_README.md`
- Repointed script link to repo script path: `../../scripts/quick-start-test.sh`.

3. `docs/deployment/DEPLOY_TEST_NETWORK.md`
- Fixed testing plan/quickstart links to `docs/development/testing/*`.
- Fixed `CLAUDE.md` link to repo root `../../CLAUDE.md`.

4. `docs/development/RELEASE_PROCESS.md`
- Replaced dead `UPGRADING.md` links with canonical migration guide:
  - `../migration-guides/version-upgrades.md`

5. `docs/development/code-quality-improvements.md`
- Fixed deny config source link to `../../icn/deny.toml`.

6. `docs/development/testing/TESTING_QUICKSTART.md`
- Fixed deploy guide link to `../../deployment/DEPLOY_TEST_NETWORK.md`.

7. `docs/examples/policies/README.md`
- Fixed governance primitives link to `../../design/governance/governance-primitives.md`.

## Verification updates (Batch A4 - slice 8)

```bash
for f in docs/api/README.md docs/demo/DEMO_README.md docs/deployment/DEPLOY_TEST_NETWORK.md docs/development/RELEASE_PROCESS.md docs/development/code-quality-improvements.md docs/development/testing/TESTING_QUICKSTART.md docs/examples/policies/README.md; do
  ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg "^MISSING_LINK\|${f//\//\/}\|" || true
done
```

Result: no missing-link matches for targeted files.

## Audit ledger updates (slice 8)

- `docs/api/README.md | docs/internal/pilots/pilot-coordinator-guide.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/demo/DEMO_README.md | scripts/quick-start-test.sh | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/deployment/DEPLOY_TEST_NETWORK.md | docs/development/testing/{INTERNAL_TESTING_PLAN.md,TESTING_QUICKSTART.md}, CLAUDE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/development/RELEASE_PROCESS.md | docs/migration-guides/version-upgrades.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/development/code-quality-improvements.md | icn/deny.toml | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/development/testing/TESTING_QUICKSTART.md | docs/deployment/DEPLOY_TEST_NETWORK.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/examples/policies/README.md | docs/design/governance/governance-primitives.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 8)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue to next drift slice.

## Changes applied (Batch A4 - slice 9: sdis/features/glossary/migration links)

1. `docs/design/sdis/social-recovery.md`
- Fixed related design and source links:
  - `../multi-device-identity-design.md`
  - `../../../icn/crates/icn-identity/src/recovery.rs`
  - `../../../icn/crates/icn-core/tests/recovery_integration.rs`

2. `docs/features/witness-signature-best-practices.md`
- Fixed cross-doc references to current architecture/security/development/design locations.

3. `docs/glossary.md`
- Fixed design references for contribution credits, governance primitives, and commons evolution.

4. `docs/migration-guides/keystore-versions.md`
- Fixed multi-device identity and operations guide references to current docs paths.

5. `docs/migration-guides/version-upgrades.md`
- Fixed backup/operations references to `docs/guides/operations/*`.

## Verification updates (Batch A4 - slice 9)

```bash
for f in docs/design/sdis/social-recovery.md docs/features/witness-signature-best-practices.md docs/glossary.md docs/migration-guides/keystore-versions.md docs/migration-guides/version-upgrades.md; do
  ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg "^MISSING_LINK\|${f//\//\/}\|" || true
done
```

Result: no missing-link matches for targeted files.

## Audit ledger updates (slice 9)

- `docs/design/sdis/social-recovery.md | docs/design/multi-device-identity-design.md, icn/crates/icn-identity/src/recovery.rs, icn/crates/icn-core/tests/recovery_integration.rs | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/features/witness-signature-best-practices.md | docs/design/economics/ECONOMIC_ARCHITECTURE.md, docs/security/production-hardening.md, docs/development/trust-multi-graph-migration.md, docs/design/governance/governance-primitives.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/glossary.md | docs/design/economics/contribution-credits-design.md, docs/design/governance/governance-primitives.md, docs/design/COMMONS_EVOLUTION.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/migration-guides/keystore-versions.md | docs/design/multi-device-identity-design.md, docs/guides/operations/{backup-and-recovery.md,operations-guide.md} | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/migration-guides/version-upgrades.md | docs/guides/operations/{backup-and-recovery.md,operations-guide.md} | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 9)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue to next drift slice.

## Changes applied (Batch A4 - slice 10: security/observability/onboarding)

1. `docs/security/production-hardening.md`
- Fixed crate source paths to `../../icn/...` from `docs/security/` depth.
- Updated moved module references:
  - `icn-core/src/supervisor/mod.rs`
  - `icn-net/src/actor/mod.rs`
- Fixed architecture/API references:
  - `../ARCHITECTURE.md`
  - `../reference/api/topic-subscriptions-api.md`
- Context normalization: converted test-count claim to historical snapshot wording with explicit date.

2. `docs/security/SECRET_MANAGEMENT.md`
- Fixed cross-doc references:
  - `production-hardening.md`
  - `../operations/deployment/incident-response.md`
  - `../archive/2025/KUBERNETES_DEPLOYMENT.md`
- Context normalization: labeled Kubernetes deployment link as archived (2025).

3. `docs/security/threat-model.md`
- Fixed incident response and architecture links:
  - `../operations/deployment/incident-response.md`
  - `../ARCHITECTURE.md`

4. `docs/observability/storage-metrics.md`
- Fixed distributed tracing and security hardening references:
  - `../operations/deployment/distributed-tracing.md`
  - `../security/production-hardening.md`

5. `docs/observability/tail-based-sampling.md`
- Fixed security hardening reference to `../security/production-hardening.md`.

6. Onboarding workshops:
- `docs/onboarding/workshops/workshop-03-runtime.md`: `../../../CLAUDE.md`
- `docs/onboarding/workshops/workshop-05-network-gossip.md`: `../../../CLAUDE.md`
- `docs/onboarding/workshops/workshop-07-gateway-sdk.md`: `../../../sdk/typescript/README.md`
- `docs/onboarding/workshops/workshop-09-ops.md`:
  - `../../operations/deployment/HOMELAB_DEPLOYMENT.md`
  - `../../security/production-hardening.md`

## Verification updates (Batch A4 - slice 10)

```bash
for f in docs/security/SECRET_MANAGEMENT.md docs/security/production-hardening.md docs/security/threat-model.md docs/observability/storage-metrics.md docs/observability/tail-based-sampling.md docs/onboarding/workshops/workshop-03-runtime.md docs/onboarding/workshops/workshop-05-network-gossip.md docs/onboarding/workshops/workshop-07-gateway-sdk.md docs/onboarding/workshops/workshop-09-ops.md; do
  ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg "^MISSING_LINK\|${f//\//\/}\|" || true
done
```

Result: no missing-link matches for targeted files.

## Audit ledger updates (slice 10)

- `docs/security/production-hardening.md | icn/crates/{icn-net,icn-core,icn-ledger,icn-gossip}, docs/ARCHITECTURE.md, docs/reference/api/topic-subscriptions-api.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/security/SECRET_MANAGEMENT.md | docs/security/production-hardening.md, docs/operations/deployment/incident-response.md, docs/archive/2025/KUBERNETES_DEPLOYMENT.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/security/threat-model.md | docs/operations/deployment/incident-response.md, docs/ARCHITECTURE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/observability/storage-metrics.md | docs/operations/deployment/distributed-tracing.md, docs/security/production-hardening.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/observability/tail-based-sampling.md | docs/security/production-hardening.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/onboarding/workshops/workshop-03-runtime.md | CLAUDE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/onboarding/workshops/workshop-05-network-gossip.md | CLAUDE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/onboarding/workshops/workshop-07-gateway-sdk.md | sdk/typescript/README.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/onboarding/workshops/workshop-09-ops.md | docs/operations/deployment/HOMELAB_DEPLOYMENT.md, docs/security/production-hardening.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 10)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue to next drift slice.

## Changes applied (Batch A4 - slice 11: developer/internal/sdis/security-roadmap/vision)

1. `docs/guides/developer/i18n-guide.md`
- Fixed root and workspace path references for contribution guide and localization directories.

2. `docs/guides/developer/DOCUMENTATION_STYLE.md`
- Replaced placeholder broken link example with non-link file-location literal.

3. `docs/internal/pilots/pilot-playbook.md`
- Fixed incident response link to canonical ops runbook path.

4. `docs/internal/pilots/pilot-readiness-gaps.md`
- Replaced dead completion-doc link with existing governance-ledger session record.

5. `docs/internal/status/GAP_ANALYSIS.md`
- Fixed references to repo root `CLAUDE.md`, roadmap, and internal testing plan.

6. `docs/sdis/SDIS_SYSTEM.md`
- Repointed missing SDIS references to canonical design/security docs.

7. `docs/security/TOFU_SECURITY_MODEL.md`
- Replaced dead trust-graph link with current trust migration doc.
- Converted plain-text source references into verified clickable links.

8. `docs/security/security-roadmap.md`
- Fixed architecture and root guidance links.

9. `docs/vision/FUTURE_WORK.md`
- Fixed gap/roadmap/architecture references to canonical docs locations.

## Verification updates (Batch A4 - slice 11)

```bash
for f in docs/guides/developer/i18n-guide.md docs/guides/developer/DOCUMENTATION_STYLE.md docs/internal/pilots/pilot-playbook.md docs/internal/pilots/pilot-readiness-gaps.md docs/internal/status/GAP_ANALYSIS.md docs/sdis/SDIS_SYSTEM.md docs/security/TOFU_SECURITY_MODEL.md docs/security/security-roadmap.md docs/vision/FUTURE_WORK.md; do
  ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg "^MISSING_LINK\|${f//\//\/}\|" || true
done
```

Result: no missing-link matches for targeted files.

Global missing-link count after this slice: `29`.

## Audit ledger updates (slice 11)

- `docs/guides/developer/i18n-guide.md | CONTRIBUTING.md, icn/bins/icnctl/locales, icn/crates/icn-gateway/locales, web/pilot-ui/locales, sdk/react-native/examples/CoopWallet/src/i18n | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/guides/developer/DOCUMENTATION_STYLE.md | style guidance example normalization | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/internal/pilots/pilot-playbook.md | docs/operations/deployment/incident-response.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/internal/pilots/pilot-readiness-gaps.md | docs/development/sessions/2026-01/2025-01-17-governance-ledger-integration.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/internal/status/GAP_ANALYSIS.md | CLAUDE.md, docs/development/sessions/undated/ROADMAP.md, docs/development/testing/INTERNAL_TESTING_PLAN.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/sdis/SDIS_SYSTEM.md | docs/design/multi-device-identity-design.md, docs/security/threat-model.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/security/TOFU_SECURITY_MODEL.md | docs/development/trust-multi-graph-migration.md, docs/ARCHITECTURE.md, docs/security/production-hardening.md, icn/crates/icn-net/src/{tls.rs,actor/mod.rs} | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/security/security-roadmap.md | docs/ARCHITECTURE.md, CLAUDE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/vision/FUTURE_WORK.md | docs/internal/status/GAP_ANALYSIS.md, docs/development/sessions/undated/ROADMAP.md, docs/ARCHITECTURE.md | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 11)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue.

## Changes applied (Batch A4 - slice 12: final residual scan cleanup)

1. `docs/DOCUMENTATION_MAINTENANCE.md`
- Fixed root-relative path examples to docs-relative examples.
- Reworked markdown-link regex examples to avoid scanner false positives from literal markdown-link syntax in code examples.
- Updated cross-reference examples to current existing docs paths.

2. `docs/architecture/ARCHITECTURE_COMPLETION_STATUS_OLD.md`
- Repointed historical references to current canonical locations:
  - `../ARCHITECTURE.md`
  - `../development/sessions/undated/ROADMAP.md`
  - `../GETTING_STARTED.md`
  - `../security/production-hardening.md`
  - `../design/governance/governance-primitives.md`

3. `docs/architecture/IMPLEMENTATION_GAP_ANALYSIS.md`
- Repointed `../dev-journal/` to `../development/sessions/`.

4. `docs/pilots/decision_registry_treasury_vote.md`
- Replaced non-repo wildcard link with explicit local-artifact note for `.copilot/session-state/*/plan.md`.

5. `docs/security/phase-10c-security-analysis.md`
- Corrected source-code links to repo root relative paths:
  - `../../icn/crates/icn-ccl/src/ast.rs`
  - `../../icn/crates/icn-ccl/src/messages.rs`
  - `../../icn/bins/icnctl/src/main.rs`

## Verification updates (Batch A4 - slice 12)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg '^MISSING_LINK\|'
```

Result: zero missing-link findings.

## Audit ledger updates (slice 12)

- `docs/DOCUMENTATION_MAINTENANCE.md | docs/{INDEX.md,architecture/ARCHITECTURE_MAP.md,architecture/README.md,reference/api/api-versioning.md} | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/architecture/ARCHITECTURE_COMPLETION_STATUS_OLD.md | docs/{ARCHITECTURE.md,GETTING_STARTED.md,security/production-hardening.md,design/governance/governance-primitives.md,development/sessions/undated/ROADMAP.md} | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/architecture/IMPLEMENTATION_GAP_ANALYSIS.md | docs/development/sessions/ | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/pilots/decision_registry_treasury_vote.md | local artifact note + docs policy | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`
- `docs/security/phase-10c-security-analysis.md | icn/{crates/icn-ccl/src/{ast.rs,messages.rs},bins/icnctl/src/main.rs} | ./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (A4 slice 12)

- Accuracy: 5/5
- Completeness: 5/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; link-drift backlog cleared for active scanner scope.

## Changes applied (Batch D2 - slice 1: context normalization)

1. `docs/architecture/ARCHITECTURE_COMPLETION_STATUS_OLD.md`
- Converted absolute readiness language to explicit historical framing (2025-12-17 snapshot).
- Reworded test and production-readiness statements to point-in-time conclusions.

2. `docs/internal/status/GAP_ANALYSIS.md`
- Recast status and progress statements as historical snapshot language (2025-12-04).
- Preserved underlying findings while removing present-tense implication.

3. `docs/security/production-hardening.md`
- Recast Phase 7 completion claim as historical cycle claim.
- Softened “production-ready rules” wording to avoid present-tense readiness assertion.

4. `docs/vision/FUTURE_WORK.md`
- Marked document as historical planning snapshot (2025-12-04).
- Recast gap-closure and pilot-blocker statements as date-bounded planning context.

## Verification updates (Batch D2 - slice 1)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg '^MISSING_LINK\|'
rg -n "Historical note|historical|snapshot|pilot-ready|production-ready|all critical and high-priority" docs/architecture/ARCHITECTURE_COMPLETION_STATUS_OLD.md docs/internal/status/GAP_ANALYSIS.md docs/security/production-hardening.md docs/vision/FUTURE_WORK.md
```

Result:
- Missing-link findings remain zero.
- Targeted stale-claim hotspots now include explicit historical/snapshot framing.

## Audit ledger updates (D2 slice 1)

- `docs/architecture/ARCHITECTURE_COMPLETION_STATUS_OLD.md | docs/ARCHITECTURE.md + CI/workflow reality context | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/internal/status/GAP_ANALYSIS.md | dated snapshot normalization policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/security/production-hardening.md | phase-bound security claim normalization | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/vision/FUTURE_WORK.md | roadmap/status historical framing policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (D2 slice 1)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue to next context slice.

## Changes applied (Batch D2 - slice 2: active-doc readiness language)

1. `docs/guides/FAQ.md`
- Kept FAQ question unchanged, but normalized answer to explicit 2025-11-21 status snapshot language.
- Added direction to verify current readiness against latest status/CI artifacts.

2. `docs/internal/pilots/pilot-limitations.md`
- Reframed overview and "What's NOT Limited" section from present-tense readiness claims to dated snapshot assessment wording.

3. `docs/security/security-roadmap.md`
- Reframed intro from implied current production-readiness to roadmap guidance.
- Marked phase status as snapshot wording.

## Verification updates (Batch D2 - slice 2)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg '^MISSING_LINK\|'
rg -n "Status snapshot|Status \(snapshot\)|assessed as production-ready at that time|roadmap guidance" docs/guides/FAQ.md docs/internal/pilots/pilot-limitations.md docs/security/security-roadmap.md
```

Result:
- Missing-link findings remain zero.
- Targeted active-doc readiness language now uses snapshot/qualified wording.

## Audit ledger updates (D2 slice 2)

- `docs/guides/FAQ.md | snapshot policy for readiness claims | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/internal/pilots/pilot-limitations.md | pilot snapshot framing policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/security/security-roadmap.md | roadmap-vs-readiness framing policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (D2 slice 2)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue.

## Changes applied (Batch D2 - slice 3: architecture/deploy/config wording)

1. `docs/architecture/ARCHITECTURAL_GAPS_AND_FIXES.md`
- Marked document as historical review snapshot (2025-12-17).
- Reframed pilot-readiness conclusion statements to point-in-time assessment language.

2. `docs/deployment/DEPLOYMENT_GUIDE.md`
- Replaced absolute "Ready to Deploy" footer wording with snapshot guidance phrasing.
- Reframed backend readiness language as revision-time, pilot-scoped guidance.

3. `docs/reference/config/identity-backend-configuration.md`
- Replaced absolute "production-ready" phrasing for Age backend with maturity-based wording.

## Verification updates (Batch D2 - slice 3)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg '^MISSING_LINK\|'
rg -n "pilot-ready|production-ready|historical review snapshot|production-capable|mature backend" docs/architecture/ARCHITECTURAL_GAPS_AND_FIXES.md docs/deployment/DEPLOYMENT_GUIDE.md docs/reference/config/identity-backend-configuration.md
```

Result:
- Missing-link findings remain zero.
- Targeted wording in active docs now uses qualified/snapshot phrasing.

## Audit ledger updates (D2 slice 3)

- `docs/architecture/ARCHITECTURAL_GAPS_AND_FIXES.md | historical framing policy for readiness claims | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/deployment/DEPLOYMENT_GUIDE.md | deployment guidance phrasing normalization | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/reference/config/identity-backend-configuration.md | backend maturity wording normalization | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (D2 slice 3)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue.

## Changes applied (Batch C2 - slice 1: code-aligned API/backend docs)

1. `docs/reference/api/topic-subscriptions-api.md`
- Updated incoming subscription handler example to match current runtime pattern in `icn-core/src/supervisor/init_network.rs`:
  - non-blocking callback
  - `tokio::spawn` for async work
  - `gossip_handle.write().await` and `subscribe(...).await`
- Removed outdated `blocking_write()` example semantics.

2. `icn/bins/icnd/src/main.rs`
- Corrected hardware-backend error guidance path to the canonical docs location:
  - `docs/reference/config/identity-backend-configuration.md`

## Verification updates (Batch C2 - slice 1)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg '^MISSING_LINK\|'
rg -n "blocking_write\(|reference/config/identity-backend-configuration\.md" docs/reference/api/topic-subscriptions-api.md icn/bins/icnd/src/main.rs
```

Result:
- Missing-link findings remain zero.
- API example and daemon error guidance align with current code paths and async behavior.

## Audit ledger updates (C2 slice 1)

- `docs/reference/api/topic-subscriptions-api.md | icn/crates/icn-core/src/supervisor/init_network.rs | doc_reality_scan + semantic grep | pass | reviewed_on(2026-02-11)`
- `icn/bins/icnd/src/main.rs | docs/reference/config/identity-backend-configuration.md | path verification + grep | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (C2 slice 1)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue.

## Changes applied (Batch D2 - slice 4: internal status snapshot framing)

1. `docs/internal/status/gap-analysis.md`
- Marked document as historical analysis snapshot (2025-11-18).
- Reframed substrate readiness claim to explicit point-in-time assessment language.

2. `docs/internal/status/strategic-gap-analysis.md`
- Marked status as historical strategic analysis snapshot (2025-01-15).
- Reframed completion/test-count/readiness statements to date-bounded wording.

3. `docs/internal/status/multi-device-status.md`
- Reframed summary and issue status claims as snapshot assessments.
- Reworded production-ready language to historical status notation.

4. `docs/architecture/ARCHITECTURE_REVIEW_SUMMARY.md`
- Marked report as historical review snapshot (2025-12-17).
- Reframed conclusion/recommendation and status labels to explicit review-time context.

## Verification updates (Batch D2 - slice 4)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg '^MISSING_LINK\|'
rg -n "Historical|snapshot|Status at review time|production-ready|pilot-ready" docs/internal/status/gap-analysis.md docs/internal/status/strategic-gap-analysis.md docs/internal/status/multi-device-status.md docs/architecture/ARCHITECTURE_REVIEW_SUMMARY.md
```

Result:
- Missing-link findings remain zero.
- Targeted internal status and architecture review docs now use explicit snapshot framing.

## Audit ledger updates (D2 slice 4)

- `docs/internal/status/gap-analysis.md | status snapshot framing policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/internal/status/strategic-gap-analysis.md | status snapshot framing policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/internal/status/multi-device-status.md | status snapshot framing policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`
- `docs/architecture/ARCHITECTURE_REVIEW_SUMMARY.md | architecture review snapshot framing policy | doc_reality_scan + stale-claim grep | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (D2 slice 4)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue.

## Changes applied (Batch C2 - slice 2: config/runtime docs vs icnd behavior)

1. `docs/reference/config/CONFIGURATION.md`
- Corrected config loading behavior to match `icnd`:
  - no multi-path auto-search order
  - `--config` explicit path or runtime defaults (`Config::default()`)
- Corrected example config reference path to `icn/config/icn.toml.example`.
- Reframed validation guidance to prefer `icnd --validate-config` (runtime-native validation).
- Reworked `validate-config.py` usage examples to require explicit `--schema` path (optional/legacy path).
- Updated troubleshooting/migration snippets from hardcoded `icn.toml` assumptions to generic `config.toml` paths.
- Clarified IDE schema section as optional/local schema workflow.

2. `docs/reference/config/identity-backend-configuration.md`
- Replaced hardcoded `icn.toml` assumption with generic `--config` file wording.
- Corrected default keystore location semantics to `{data_dir}/identity.age` and typical Linux default `~/.local/share/icn/identity.age`.
- Updated runtime selection and `--validate-config` examples to match current daemon behavior.

## Verification updates (Batch C2 - slice 2)

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh . | rg '^MISSING_LINK\|'
rg -n "Config::default\(\)|--validate-config|identity\.age|icn\.toml|searches in this order" docs/reference/config/CONFIGURATION.md docs/reference/config/identity-backend-configuration.md icn/bins/icnd/src/main.rs
```

Result:
- Missing-link findings remain zero.
- Config/runtime documentation now aligns with current `icnd` load and validation behavior.

## Audit ledger updates (C2 slice 2)

- `docs/reference/config/CONFIGURATION.md | icn/bins/icnd/src/main.rs + icn/config/icn.toml.example + scripts/validate-config.py | doc_reality_scan + semantic grep | pass | reviewed_on(2026-02-11)`
- `docs/reference/config/identity-backend-configuration.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/identity.rs | doc_reality_scan + semantic grep | pass | reviewed_on(2026-02-11)`

## Recursive self-correction score (C2 slice 2)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4; continue.
## Changes applied (Batch C2 - Quickstart/runtime docs)

1. `icn/QUICKSTART.md`
- Aligned passphrase guidance with `icnd` runtime precedence:
  - `ICN_KEYSTORE_PASSPHRASE` preferred
  - `ICN_PASSPHRASE` legacy fallback
- Replaced legacy `~/.icn/identity.age` assumptions with default Linux path from `Config::default()` (`~/.local/share/icn/identity.age`).
- Reworked "without identity" flow to use `--data-dir /tmp/icn-demo` instead of renaming files in-place.
- Corrected static asset layout to current files under `crates/icn-gateway/static/` (`index.html`, `style.css`, `app.js`, `README.md`).
- Kept health endpoint guidance on `/v1/health`.

2. `icn/start-gateway.sh`
- Updated operator hint to prefer `ICN_KEYSTORE_PASSPHRASE` (was legacy-only `ICN_PASSPHRASE`).

## Verification updates (Batch C2)

```bash
rg -n "ICN_KEYSTORE_PASSPHRASE|ICN_PASSPHRASE|~/.local/share/icn/identity.age|~/.icn/identity.age|crates/icn-gateway/static/js|crates/icn-gateway/static/css" icn/QUICKSTART.md icn/start-gateway.sh
```

Result: quickstart and startup script now use preferred passphrase variable, retain legacy fallback note, and no longer reference obsolete static subdirectories.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no `MISSING_LINK` findings.

## Audit ledger additions (Batch C2)

- `icn/QUICKSTART.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs + icn/crates/icn-gateway/static/* | rg -n "ICN_KEYSTORE_PASSPHRASE|ICN_PASSPHRASE|data_dir|identity.age|health" + find icn/crates/icn-gateway/static -maxdepth 2 -type f | aligned | reviewed_on(2026-02-11)`
- `icn/start-gateway.sh | icn/bins/icnd/src/main.rs (passphrase precedence comments) | rg -n "ICN_KEYSTORE_PASSPHRASE|ICN_PASSPHRASE" icn/start-gateway.sh icn/bins/icnd/src/main.rs | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C2)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C2; continue next A4 slices and D2 historical normalization.
## Changes applied (Batch C3 - onboarding/ops runtime semantics)

1. `docs/GETTING_STARTED.md`
- Added explicit `--data-dir ~/.icn` usage for `icnctl` and `icnd` in startup flow so identity and daemon state align.
- Updated keystore terminology/path to `{data_dir}/identity.age`.

2. `docs/operations/deployment/deployment-guide.md`
- Corrected daemon defaults and examples:
  - QUIC default port `7777` (not `4433`)
  - Prometheus metrics default `9100` (not `9090`)
  - data/identity path examples to `/var/lib/icn/identity.age`
- Removed unsupported environment-variable assumptions and clarified that path/network settings are controlled via `--data-dir`, `--config`, and config file fields.
- Corrected systemd and Docker examples to pass explicit `--data-dir`/`--config`.
- Fixed backup/recovery examples to reference `identity.age` instead of `keystore.age`.

3. `docs/ops/runbooks/README.md`
- Updated environment variable table to reflect current runtime behavior (`ICN_KEYSTORE_PASSPHRASE`, `ICN_PASSPHRASE`, `ICN_GATEWAY_JWT_SECRET`).
- Added explicit CLI-path guidance for `icnd` and `icnctl`.

## Verification updates (Batch C3)

```bash
rg -n "~/.icn/keystore.age|0.0.0.0:4433|:9090/metrics|ICN_DATA_DIR=|ICN_OBSERVABILITY_LOG_LEVEL|/etc/icn/icn.toml|targets: \['icnd:9090'\]" docs/GETTING_STARTED.md docs/operations/deployment/deployment-guide.md docs/ops/runbooks/README.md
```

Result: no matches.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; only expected historical/stale-marker reports remain.

## Audit ledger additions (Batch C3)

- `docs/GETTING_STARTED.md | icn/bins/icnctl/src/main.rs:get_data_dir + icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "get_data_dir|identity.age|data_dir|listen_addr|metrics_port" | aligned | reviewed_on(2026-02-11)`
- `docs/operations/deployment/deployment-guide.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "data_dir|listen_addr|metrics_port|ICN_KEYSTORE_PASSPHRASE|ICN_GATEWAY_JWT_SECRET" | aligned | reviewed_on(2026-02-11)`
- `docs/ops/runbooks/README.md | icn/bins/icnd/src/main.rs + icn/bins/icnctl/src/main.rs | rg -n "ICN_KEYSTORE_PASSPHRASE|ICN_PASSPHRASE|--data-dir|--config" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C3)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C3; proceed with next active-doc slices.
## Changes applied (Batch C4 - active FAQ/architecture defaults)

1. `docs/guides/FAQ.md`
- Corrected default P2P port references from `5600` to `7777` and updated firewall examples.
- Replaced hard-coded keystore path (`~/.icn/keystore.age`) with runtime-correct `{data_dir}/identity.age` wording.
- Reworded persistence and troubleshooting sections to use configured data-dir semantics.
- Kept reset example explicit with `--data-dir ~/.icn` to avoid ambiguity.

2. `docs/architecture/KERNEL_APP_SEPARATION.md`
- Corrected daemon default store-path example to `~/.local/share/icn/store/` (Linux default via `Config::default().data_dir`).

## Verification updates (Batch C4)

```bash
rg -n "5600|keystore.age|~/.icn/store/|~/.icn/keystore.age" docs/guides/FAQ.md docs/architecture/KERNEL_APP_SEPARATION.md
```

Result: no matches.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; only expected historical marker reports remain.

## Audit ledger additions (Batch C4)

- `docs/guides/FAQ.md | icn/crates/icn-core/src/config/mod.rs + icn/bins/icnd/src/main.rs | rg -n "listen_addr|metrics_port|keystore_path|data_dir" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/architecture/KERNEL_APP_SEPARATION.md | icn/crates/icn-core/src/config/mod.rs | rg -n "store_path|data_dir|dirs::data_dir" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C4)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C4; continue active-doc drift reduction.
## Changes applied (Batch C5 - active operations/API defaults)

1. `docs/guides/operations/troubleshooting.md`
- Updated QUIC troubleshooting examples from stale `5600` to default `7777/udp`.
- Adjusted in-pod socket checks to UDP (`ss -ulnp`) and UDP probe example (`nc -zvu`).

2. `docs/guides/operations/replication-operations.md`
- Updated metrics endpoint examples from `:9090/metrics` to `:9100/metrics`.

3. `docs/guides/operations/operations-guide.md`
- Updated backup content examples to current keystore naming (`identity.age`) and `{data_dir}` semantics.
- Updated port conflict example from `4433` to `7777`.
- Removed hard-coded config-path assumption in remediation text.

4. `docs/reference/api/topic-subscriptions-api.md`
- Updated metrics query endpoint from `:9090/metrics` to `:9100/metrics`.

5. `docs/operations/deployment/incident-response.md`
- Updated QUIC firewall verification from `5600` to `7777`.

## Verification updates (Batch C5)

```bash
rg -n "\b5600\b|\b4433\b|:9090/metrics|keystore.age" docs/guides/operations/troubleshooting.md docs/guides/operations/replication-operations.md docs/guides/operations/operations-guide.md docs/reference/api/topic-subscriptions-api.md docs/operations/deployment/incident-response.md
```

Result: no matches.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C5)

- `docs/guides/operations/troubleshooting.md | icn/crates/icn-core/src/config/mod.rs (listen_addr default) | rg -n "listen_addr|7777" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/guides/operations/replication-operations.md | icn/crates/icn-core/src/config/mod.rs (observability.metrics_port default) | rg -n "metrics_port|9100" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/guides/operations/operations-guide.md | icn/bins/icnctl/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "identity.age|store|listen_addr" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/reference/api/topic-subscriptions-api.md | icn/crates/icn-core/src/config/mod.rs (metrics_port default) | rg -n "metrics_port|9100" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/operations/deployment/incident-response.md | icn/crates/icn-core/src/config/mod.rs (listen_addr default) | rg -n "listen_addr|7777" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C5)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C5; continue through remaining active doc drift.
## Changes applied (Batch C6 - deployment/config/perf active docs)

1. `docs/deployment/DEPLOYMENT_GUIDE.md`
- Updated stale metrics default in environment example from `9090` to `9100`.
- Updated Prometheus endpoint reference to `http://localhost:9100/metrics`.
- Added explicit note that `icnd` does not directly consume `ICN_DATA_DIR`/`ICN_QUIC_PORT`/`ICN_METRICS_PORT`; authoritative settings are CLI flags + config fields.

2. `docs/reference/config/CONFIGURATION.md`
- Replaced incorrect blanket env override table with currently supported runtime envs:
  - `ICN_GATEWAY_JWT_SECRET`
  - `OTEL_EXPORTER_OTLP_ENDPOINT`
  - `OTEL_SERVICE_NAME`
  - `ICN_KEYSTORE_PASSPHRASE` / legacy `ICN_PASSPHRASE`
- Corrected precedence wording to avoid implying all config fields are env-overridable.
- Updated troubleshooting debug example from `ICN_LOG_LEVEL=debug icnd` to `icnd --log-level debug`.

3. `docs/examples/policies/README.md`
- Updated metrics endpoint reference from `:9090` to `:9100`.

4. `docs/performance/PROFILING.md`
- Updated iftop port filter from `5600` to default QUIC port `7777`.

5. `docs/guides/operations/operations-guide.md`
- Corrected backup verification command to use `icnctl --data-dir ...` instead of unsupported `ICN_DATA_DIR=... icnctl ...`.

## Verification updates (Batch C6)

```bash
rg -n "ICN_DATA_DIR|ICN_NETWORK_LISTEN_ADDR|ICN_LOG_LEVEL|:9090/metrics|port 5600|ICN_METRICS_PORT=9090" docs/deployment/DEPLOYMENT_GUIDE.md docs/reference/config/CONFIGURATION.md docs/examples/policies/README.md docs/performance/PROFILING.md docs/guides/operations/operations-guide.md
```

Result: remaining matches are explanatory notes about unsupported env usage; stale operational claims removed.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C6)

- `docs/deployment/DEPLOYMENT_GUIDE.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "metrics_port|listen_addr|ICN_GATEWAY_JWT_SECRET" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/reference/config/CONFIGURATION.md | icn/bins/icnd/src/main.rs | rg -n "ICN_GATEWAY_JWT_SECRET|OTEL_EXPORTER_OTLP_ENDPOINT|OTEL_SERVICE_NAME|ICN_KEYSTORE_PASSPHRASE|ICN_PASSPHRASE|--log-level" | aligned | reviewed_on(2026-02-11)`
- `docs/examples/policies/README.md | icn/crates/icn-core/src/config/mod.rs | rg -n "metrics_port" | aligned | reviewed_on(2026-02-11)`
- `docs/performance/PROFILING.md | icn/crates/icn-core/src/config/mod.rs | rg -n "listen_addr|7777" | aligned | reviewed_on(2026-02-11)`
- `docs/guides/operations/operations-guide.md | icn/bins/icnctl/src/main.rs:get_data_dir | rg -n "get_data_dir|--data-dir" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C6)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C6; continue with next active-doc drift slice.
## Changes applied (Batch C7 - architecture/onboarding identity and network defaults)

1. `docs/ARCHITECTURE.md`
- Replaced stale keystore path examples (`.../identity/keypair.age`) with current `{data_dir}/identity.age` semantics.
- Updated mDNS/peer port example from `4433` to default `7777`.
- Updated config guidance from `$ICN_DATA_DIR/config.toml` + outdated `[node]/bind_addr` example to current `data_dir`, `[network].listen_addr`, and explicit `--config` usage.
- Replaced unsupported env override claims (`ICN_DATA_DIR`, `ICN_LOG_LEVEL`) with runtime-correct CLI override guidance.

2. `docs/architecture/ARCHITECTURE_INDEX.md`
- Updated identity-save flow path to `{data_dir}/identity.age`.

3. `docs/onboarding/reference/module-04-identity-trust.md`
- Replaced `~/.icn/keystore.age` references with `{data_dir}/identity.age` in architecture diagram, lifecycle notes, and identity flow.

4. `docs/design/multi-device-identity-design.md`
- Updated keystore save path reference to `{data_dir}/identity.age`.

## Verification updates (Batch C7)

```bash
rg -n "keypair.age|keystore.age|port=4433|bind_addr = \"0.0.0.0:4433\"|ICN_DATA_DIR|ICN_LOG_LEVEL" docs/ARCHITECTURE.md docs/architecture/ARCHITECTURE_INDEX.md docs/onboarding/reference/module-04-identity-trust.md docs/design/multi-device-identity-design.md
```

Result: stale runtime/default references removed from the edited docs.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C7)

- `docs/ARCHITECTURE.md | icn/crates/icn-core/src/config/mod.rs + icn/bins/icnd/src/main.rs | rg -n "data_dir|keystore_path|listen_addr|--config|--log-level" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/architecture/ARCHITECTURE_INDEX.md | icn/crates/icn-core/src/config/mod.rs | rg -n "identity.age|keystore_path" | aligned | reviewed_on(2026-02-11)`
- `docs/onboarding/reference/module-04-identity-trust.md | icn/crates/icn-core/src/config/mod.rs + icn/bins/icnctl/src/main.rs:get_keystore_path | rg -n "identity.age|get_keystore_path" | aligned | reviewed_on(2026-02-11)`
- `docs/design/multi-device-identity-design.md | icn/crates/icn-core/src/config/mod.rs + icn/bins/icnctl/src/main.rs:get_keystore_path | rg -n "identity.age|get_keystore_path" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C7)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C7; continue active-doc drift cleanup.
## Changes applied (Batch C8 - testing and migration guide command drift)

1. `docs/development/testing/INTERNAL_TESTING_PLAN.md`
- Replaced unsupported local-process examples (`ICN_DATA_DIR=...` and `--bind`) with valid per-node config-file startup (`--config /tmp/icn-nodeN.toml`) using `network.listen_addr` and per-node observability ports.

2. `docs/development/testing/testing-rpc.md`
- Updated daemon/runtime examples from stale defaults (`0.0.0.0:4433`) to current `0.0.0.0:7777`.
- Updated sample data-dir output to Linux default path `~/.local/share/icn`.
- Updated peer dial example port to `7777`.

3. `docs/migration-guides/version-upgrades.md`
- Updated metrics endpoint examples from `:9090/metrics` to `:9100/metrics`.

## Verification updates (Batch C8)

```bash
rg -n "ICN_DATA_DIR=/tmp/icn-node|--bind 127.0.0.1|0.0.0.0:4433|:9090/metrics" docs/development/testing/INTERNAL_TESTING_PLAN.md docs/development/testing/testing-rpc.md docs/migration-guides/version-upgrades.md
```

Result: no matches.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C8)

- `docs/development/testing/INTERNAL_TESTING_PLAN.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "--config|listen_addr|metrics_port|health_port" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/development/testing/testing-rpc.md | icn/crates/icn-core/src/config/mod.rs | rg -n "listen_addr|data_dir" | aligned | reviewed_on(2026-02-11)`
- `docs/migration-guides/version-upgrades.md | icn/crates/icn-core/src/config/mod.rs | rg -n "metrics_port|9100" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C8)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C8; continue active-doc drift cleanup.
## Changes applied (Batch D3 - deployment status snapshot framing)

1. `docs/deployment/DEPLOYMENT_ACTIVE.md`
2. `docs/deployment/DEPLOYMENT_STATUS_2025-12-12.md`
3. `docs/deployment/DEPLOYMENT_READY.md`

Updates:
- Added explicit historical snapshot banners (with concrete date context) to prevent these docs from being interpreted as current operational truth.
- Added pointer to `docs/ci/CI_CURRENT_STATUS.md` for current/live status verification.

## Verification updates (Batch D3)

```bash
rg -n "Historical snapshot|not current|CI_CURRENT_STATUS" docs/deployment/DEPLOYMENT_ACTIVE.md docs/deployment/DEPLOYMENT_STATUS_2025-12-12.md docs/deployment/DEPLOYMENT_READY.md
```

Result: all three docs now explicitly marked as historical snapshots.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch D3)

- `docs/deployment/DEPLOYMENT_ACTIVE.md | docs/ci/CI_CURRENT_STATUS.md + document date metadata | rg -n "Date|Status|Historical snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/deployment/DEPLOYMENT_STATUS_2025-12-12.md | docs/ci/CI_CURRENT_STATUS.md + document date metadata | rg -n "December 12, 2025|Historical snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/deployment/DEPLOYMENT_READY.md | docs/ci/CI_CURRENT_STATUS.md + document date metadata | rg -n "December 12, 2025|Historical readiness snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch D3)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch D3; continue with remaining active docs.
## Changes applied (Batch C9 - architecture map + user manual runtime alignment)

1. `docs/manual/ICN_USER_MANUAL.md`
- Replaced unsupported daemon commands/flags:
  - `icnd --validate` -> `icnd --validate-config`
  - removed `icnd --dry-run` usage
  - removed `icnd --generate-config`
- Updated config workflow to use `icn/config/icn.toml.example` copied to `~/.icn/config.toml`.
- Updated startup examples to explicit `icnd --config ~/.icn/config.toml --data-dir ~/.icn`.
- Corrected keystore naming to `identity.age`.
- Updated config examples from obsolete sections/keys (`[node]`, `[metrics]`, gateway `listen_addr`) to current structure (`data_dir`, `[observability]`, gateway `bind_addr`).
- Updated environment variable table to supported vars (`ICN_KEYSTORE_PASSPHRASE`, `ICN_PASSPHRASE`, `ICN_GATEWAY_JWT_SECRET`, OTEL vars).
- Updated bootstrap grep path from `~/.icn/icn.toml` to `~/.icn/config.toml`.

2. `docs/architecture/ARCHITECTURE_MAP.md`
- Updated metrics default in config examples from `9090` to `9100`.
- Replaced unsupported env override claim (`ICN_DATA_DIR`) with CLI `--data-dir` guidance.
- Updated docker env block to supported secrets/logging vars and explicit runtime flag note (`icnd --data-dir /data --gateway-enable`).

## Verification updates (Batch C9)

```bash
rg -n "--validate\b|--dry-run|--generate-config|keystore.age|\[node\]|\[metrics\]|listen_addr = \"127.0.0.1:8080\"|ICN_CONFIG|ICN_DATA_DIR|ICN_LOG_LEVEL" docs/manual/ICN_USER_MANUAL.md docs/architecture/ARCHITECTURE_MAP.md
```

Result: stale/unsupported runtime claims removed from edited sections.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C9)

- `docs/manual/ICN_USER_MANUAL.md | icn/bins/icnd/src/main.rs + icn/bins/icnctl/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "--validate-config|--data-dir|--config|identity.age|bind_addr|observability" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/architecture/ARCHITECTURE_MAP.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "metrics_port|ICN_GATEWAY_JWT_SECRET|--data-dir" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C9)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C9; continue with remaining active-doc drift.
## Changes applied (Batch C10 - keystore migration guide vs identity runtime)

1. `docs/migration-guides/keystore-versions.md`
- Updated format status to reflect current migration target (`v3` current, `v2.1` legacy).
- Corrected migration semantics to match code path in `icn-identity`:
  - legacy formats migrate to v3 on unlock
  - keystore is re-encrypted and saved in-place at `{data_dir}/identity.age`
  - removed unsupported claim that migration auto-creates `backup-v1` files
- Updated file/path references from `keystore.age` to `identity.age` in backup and verification examples.
- Updated multi-keystore guidance from unsupported `ICN_DATA_DIR=... icnctl ...` to `icnctl --data-dir ...`.
- Corrected downgrade statement to v3 format.

## Verification updates (Batch C10)

```bash
rg -n "backup-v1|ICN_DATA_DIR=|keystore.age|v2.1 keystores cannot" docs/migration-guides/keystore-versions.md
```

Result: obsolete backup/env/path claims removed; version/downgrade wording aligned.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C10)

- `docs/migration-guides/keystore-versions.md | icn/crates/icn-identity/src/keystore.rs + icn/bins/icnctl/src/main.rs | rg -n "migrat|v3|identity.age|--data-dir" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C10)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C10; continue active-doc drift cleanup.
## Changes applied (Batch C11 - config/onboarding validation command cleanup)

1. `docs/reference/config/CONFIGURATION.md`
- Removed stale `--dry-run` guidance from upgrade checklist (flag not available in current `icnd`).

2. `docs/onboarding/reference/module-09-ops-deploy.md`
- Updated validation path example from `/etc/icn/icn.toml` to `/etc/icn/config.toml`.
- Updated sample validation output keys from stale fields to current config naming (`network.listen_addr`, `observability.metrics_port`).

## Verification updates (Batch C11)

```bash
rg -n "--dry-run|/etc/icn/icn.toml|network\.port|metrics\.enabled" docs/reference/config/CONFIGURATION.md docs/onboarding/reference/module-09-ops-deploy.md
```

Result: no matches.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C11)

- `docs/reference/config/CONFIGURATION.md | icn/bins/icnd/src/main.rs | rg -n "validate_config|--validate-config" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/onboarding/reference/module-09-ops-deploy.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "--validate-config|listen_addr|metrics_port" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C11)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C11; continue active-doc drift cleanup.
## Changes applied (Batch C12 - module-09 deployment config schema correction)

1. `docs/onboarding/reference/module-09-ops-deploy.md`
- Replaced outdated configuration model (`network.port`, `bind_address`, `[rpc]`, `[metrics]`) with current schema (`network.listen_addr`, gateway `bind_addr`, `[observability]`).
- Replaced unsupported environment override examples (`ICN_NETWORK_PORT`) with supported secret/logging vars.
- Updated systemd/Kubernetes examples to use `/etc/icn/config.toml` and explicit `--data-dir /var/lib/icn`.
- Updated CLI examples to current config path conventions.

## Verification updates (Batch C12)

```bash
rg -n "ICN_NETWORK_PORT|network.port|\[rpc\]|\[metrics\]|bind_address|/etc/icn/icn.toml" docs/onboarding/reference/module-09-ops-deploy.md
```

Result: no matches.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C12)

- `docs/onboarding/reference/module-09-ops-deploy.md | icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/mod.rs | rg -n "listen_addr|bind_addr|observability|--config|--data-dir" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C12)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C12; continue active-doc drift cleanup.
## Changes applied (Batch C13 - PR review feedback fixes)

1. `scripts/pilot_receipt_chain_demo.sh`
- Set gateway connectivity check path to `/v1/healthz` (gateway liveness under `/v1` scope).

2. `docs/guides/developer/DEV_ENVIRONMENT.md`
- Removed exposed code-server password and replaced with secure-channel retrieval guidance.

3. `docs/internal/pilots/pilot-limitations.md`
- Added explicit snapshot date (`2026-02-11`) to make deployability framing unambiguous.

## Verification updates (Batch C13)

```bash
rg -n "/v1/healthz|/healthz|Password:|Snapshot Date" scripts/pilot_receipt_chain_demo.sh docs/guides/developer/DEV_ENVIRONMENT.md docs/internal/pilots/pilot-limitations.md
```

Result: liveness path corrected, credential removed, snapshot date present.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C13)

- `scripts/pilot_receipt_chain_demo.sh | icn/crates/icn-gateway/src/api/health.rs + server route wiring | rg -n "healthz|/healthz|/v1/health" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/guides/developer/DEV_ENVIRONMENT.md | security hygiene policy (no embedded secrets) | manual review | aligned | reviewed_on(2026-02-11)`
- `docs/internal/pilots/pilot-limitations.md | docs snapshot-date policy + document context | rg -n "Snapshot Date|Overview" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C13)

- Accuracy: 5/5
- Completeness: 5/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C13.
## Changes applied (Batch C14 - workshop ops config schema alignment)

1. `docs/onboarding/workshops/workshop-09-ops.md`
- Updated workshop config example from obsolete sections/keys to current `icnd` schema:
  - added top-level `data_dir`
  - `[gateway].bind_addr` instead of `listen_addr`
  - replaced `[metrics]` + `[logging]` with `[observability]` (`metrics_port`, `health_port`, `log_level`)
  - removed unsupported `cors_origins` field
- Updated Docker run example to expose health port and use preferred daemon passphrase env var (`ICN_KEYSTORE_PASSPHRASE`).

## Verification updates (Batch C14)

```bash
rg -n "\[metrics\]|\[logging\]|listen_addr = \"0.0.0.0:8080\"|cors_origins|ICN_PASSPHRASE=workshop" docs/onboarding/workshops/workshop-09-ops.md
```

Result: no matches.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C14)

- `docs/onboarding/workshops/workshop-09-ops.md | icn/crates/icn-core/src/config/mod.rs + icn/bins/icnd/src/main.rs | rg -n "data_dir|gateway.bind_addr|observability|ICN_KEYSTORE_PASSPHRASE" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C14)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C14.

## Changes applied (Batch C15 - status/demo snapshot normalization)

1. `docs/demo/DEMO_STATUS_SUMMARY.md`
2. `docs/demo/DEMO_CURRENT_STATUS.md`
3. `docs/demo/DEMO_FINAL_STATUS.md`
4. `docs/demo/DEMO_SUCCESS_FINAL.md`
5. `docs/demo/DEMO_README.md`
6. `docs/mobile/MOBILE_APP_STATUS.md`
7. `docs/development/testing/DR_TEST_RESULTS.md`
8. `docs/status/DEPLOYMENT_VERIFICATION.md`
9. `docs/status/CURRENT_SYSTEM_STATUS.md`

- Added explicit historical snapshot banners with concrete dates.
- Added pointers to current verification sources (`docs/ci/CI_CURRENT_STATUS.md` and, where relevant, `docs/status/CURRENT_SYSTEM_STATUS.md`).
- Preserved underlying historical content while removing ambiguity that these reports are evergreen status truths.

## Verification updates (Batch C15)

```bash
rg -n "Historical snapshot|For current project/CI status|For current status|For current readiness|Historical test snapshot" docs/demo/DEMO_STATUS_SUMMARY.md docs/demo/DEMO_CURRENT_STATUS.md docs/demo/DEMO_FINAL_STATUS.md docs/demo/DEMO_SUCCESS_FINAL.md docs/demo/DEMO_README.md docs/mobile/MOBILE_APP_STATUS.md docs/development/testing/DR_TEST_RESULTS.md docs/status/DEPLOYMENT_VERIFICATION.md docs/status/CURRENT_SYSTEM_STATUS.md
```

Result: all touched status/demo docs now contain explicit snapshot framing and current-status pointers.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain (archive/internal marker references and topology policy mentions).

## Audit ledger additions (Batch C15)

- `docs/demo/DEMO_STATUS_SUMMARY.md | document timestamp + docs/ci/CI_CURRENT_STATUS.md | rg -n "Date:|Historical snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/demo/DEMO_CURRENT_STATUS.md | document timestamp + docs/ci/CI_CURRENT_STATUS.md | rg -n "Date:|Historical snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/demo/DEMO_FINAL_STATUS.md | document timestamp + docs/ci/CI_CURRENT_STATUS.md | rg -n "Date:|Historical snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/demo/DEMO_SUCCESS_FINAL.md | document timestamp + docs/ci/CI_CURRENT_STATUS.md | rg -n "Date:|Historical snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/demo/DEMO_README.md | document created date + status pointer policy | rg -n "Created:|snapshot|CI_CURRENT_STATUS|CURRENT_SYSTEM_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/mobile/MOBILE_APP_STATUS.md | explicit legacy snapshot date + CI status pointer | rg -n "2024-12-12|Historical snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/development/testing/DR_TEST_RESULTS.md | test date + revalidation requirement | rg -n "Test Date|Historical test snapshot|Re-run DR" | aligned | reviewed_on(2026-02-11)`
- `docs/status/DEPLOYMENT_VERIFICATION.md | timestamp + CI status pointer | rg -n "Date:|Historical deployment snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`
- `docs/status/CURRENT_SYSTEM_STATUS.md | timestamp + CI status pointer | rg -n "Date:|Historical demo-environment snapshot|CI_CURRENT_STATUS" | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C15)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C15; continue active-doc content drift cleanup.

## Changes applied (Batch C16 - demo/status path portability normalization)

Touched files:

- `docs/status/CURRENT_SYSTEM_STATUS.md`
- `docs/status/DEPLOYMENT_VERIFICATION.md`
- `docs/status/FINAL_DEMO_SUCCESS.md`
- Demo snapshot set under `docs/demo/`:
  - `DEMO_AUDIT.md`
  - `DEMO_BUG_FOUND.md`
  - `DEMO_CURRENT_STATUS.md`
  - `DEMO_FINAL_SESSION_SUMMARY.md`
  - `DEMO_INFRASTRUCTURE_COMPLETE.md`
  - `DEMO_LOGIN_INFO.md`
  - `DEMO_NEXT_IMMEDIATE_STEPS.md`
  - `DEMO_NEXT_STEPS.md`
  - `DEMO_PROGRESS_UPDATE.md`
  - `DEMO_QUICK_START.md`
  - `DEMO_SESSION_1_SUMMARY.md`
  - `DEMO_SESSION_COMPLETE.md`
  - `DEMO_STATUS_SUMMARY.md`
  - `DEMO_SUCCESS_FINAL.md`
  - `DEMO_WIRING_STATUS.md`

Changes:

- Replaced machine-specific paths with portable placeholders:
  - `/home/matt/projects/icn` -> `<repo-root>`
  - `/home/matt/icn-demo-test` -> `<demo-data-dir>`
- Preserved command semantics while making examples reusable across environments.

## Verification updates (Batch C16)

```bash
rg -n "/home/matt/projects/icn|/home/matt/icn-demo-test|<repo-root>|<demo-data-dir>" docs/demo docs/status/CURRENT_SYSTEM_STATUS.md docs/status/DEPLOYMENT_VERIFICATION.md docs/status/FINAL_DEMO_SUCCESS.md
```

Result: no `/home/matt/...` paths remain in the touched demo/status files; placeholder paths present in all converted locations.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain (archive/internal marker references and topology policy mentions).

## Audit ledger additions (Batch C16)

- `docs/demo/* + docs/status/{CURRENT_SYSTEM_STATUS,DEPLOYMENT_VERIFICATION,FINAL_DEMO_SUCCESS}.md | portability policy (no machine-specific absolute paths in reusable docs) | rg -n "/home/matt/projects/icn|/home/matt/icn-demo-test|<repo-root>|<demo-data-dir>" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C16)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C16; continue active-doc content drift cleanup.

## Changes applied (Batch C17 - SDIS API prefix and reality caveats)

1. `docs/sdis/SDIS_QUICK_START.md`
2. `docs/sdis/SDIS_SYSTEM.md`
3. `docs/sdis/SDIS_IMPLEMENTATION_COMPLETE.md`

Changes:

- Replaced stale `/api/v1/sdis/*` references with `/v1/sdis/*`.
- Added explicit caveats that these SDIS docs include design/snapshot material and must be validated against currently wired gateway routes.
- Fixed stale resource references in quick-start (`docs/SDIS_SYSTEM.md` -> `docs/sdis/SDIS_SYSTEM.md`, `docs/security-model.md` -> `docs/security/SDIS_THREAT_MODEL.md`).

## Verification updates (Batch C17)

```bash
rg -n "/api/v1/sdis|/v1/sdis" docs/sdis/SDIS_QUICK_START.md docs/sdis/SDIS_SYSTEM.md docs/sdis/SDIS_IMPLEMENTATION_COMPLETE.md
```

Result: no `/api/v1/sdis` occurrences remain in touched SDIS docs; `/v1/sdis` paths now used.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C17)

- `docs/sdis/SDIS_QUICK_START.md | icn/crates/icn-gateway/src/api/sdis/{mod.rs,simple_enrollment.rs} | rg -n "/api/v1/sdis|/v1/sdis|enrollment/start|verify/level1|verify/level2" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/sdis/SDIS_SYSTEM.md | icn/crates/icn-gateway/src/api/sdis/mod.rs | rg -n "/api/v1/sdis|/v1/sdis" + source review | partially-aligned-with-caveat | reviewed_on(2026-02-11)`
- `docs/sdis/SDIS_IMPLEMENTATION_COMPLETE.md | snapshot-date policy + gateway route prefix convention | rg -n "/api/v1/sdis|/v1/sdis|Historical implementation snapshot" + source review | aligned-with-snapshot-caveat | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C17)

- Accuracy: 4/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C17; continue active-doc content drift cleanup.

## Changes applied (Batch C18 - SDIS status route-wiring correction)

1. `docs/sdis/SDIS_STATUS.md`

Changes:

- Added explicit historical snapshot banner with truth-source pointers.
- Replaced overbroad "all SDIS endpoints are live" framing with currently wired endpoint groups from `sdis::configure`.
- Corrected endpoint-shape drift for anchor/recovery routes:
  - current wired paths use `/v1/sdis/anchor/*` and `/v1/sdis/recovery/start`
  - older docs may still reference `/v1/sdis/anchors/*` or `/v1/sdis/recovery/initiate`
- Updated backend implementation section language to reflect wired routes with mixed feature maturity.

## Verification updates (Batch C18)

```bash
rg -n "Historical snapshot|Known naming drift|/v1/sdis/anchor|/v1/sdis/recovery/start|WIRED WITH MIXED MATURITY" docs/sdis/SDIS_STATUS.md
```

Result: snapshot framing and corrected route-shape caveats present.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C18)

- `docs/sdis/SDIS_STATUS.md | icn/crates/icn-gateway/src/{server.rs,api/sdis/anchor.rs,api/sdis/recovery.rs,api/sdis/simple_enrollment.rs} | rg -n "scope\\(\"/sdis\"\\)|configure\\(api::sdis::recovery::configure\\)|configure\\(api::sdis::anchor::configure\\)|scope\\(\"/anchor\"\\)|scope\\(\"/recovery\"\\)" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C18)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C18; continue active-doc content drift cleanup.

## Changes applied (Batch C19 - deployment full-stack endpoint drift correction)

1. `docs/deployment/FULL_STACK_DEPLOY.md`

Changes:

- Added historical snapshot banner with current truth sources.
- Replaced stale gateway checks:
  - `/health` -> `/v1/health`
  - removed nonexistent `/api/v1/whoami`; replaced with `/v1/auth/challenge` example
  - `/ws` -> `/v1/ws/{coop_id}`
- Replaced stale `/api/v1/*` examples with current v1 paths:
  - ledger transaction example -> `POST /v1/ledger/{coop_id}/payment`
  - governance examples -> `POST /v1/gov/proposals`, `POST /v1/gov/proposals/{id}/vote`
- Updated request body shapes to match current models:
  - proposal payload uses tagged `{"type":"text","body":"..."}`
  - vote uses `choice` + optional `comment`.
- Replaced trust-example hardcoding with OpenAPI-discovery check (`/api-docs/openapi.json`) to avoid asserting non-authoritative trust paths.

## Verification updates (Batch C19)

```bash
rg -n "/api/v1/|http://localhost:8080/health\\b|ws://localhost:8080/ws\\b|/api-docs/openapi.json|/v1/gov/proposals|/v1/ledger/.+/payment|/v1/auth/challenge|/v1/ws/" docs/deployment/FULL_STACK_DEPLOY.md
```

Result: stale `/api/v1/*`, `/health`, and `/ws` forms removed from this file; current v1 forms present.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C19)

- `docs/deployment/FULL_STACK_DEPLOY.md | icn/crates/icn-gateway/src/server.rs + icn/crates/icn-gateway/src/models.rs + docs/api/openapi.yaml | rg -n "scope\\(\"/v1\"\\)|scope\\(\"/ledger\"\\)|scope\\(\"/gov\"\\)|/v1/ws/\\{coop_id\\}|CreateProposalRequest|CastVoteRequest" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C19)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C19; continue active-doc content drift cleanup.

## Changes applied (Batch C20 - getting-started gateway endpoint correction)

1. `docs/GETTING_STARTED.md`

Changes:

- Replaced legacy gateway health path `/health` with `/v1/health`.
- Replaced legacy WebSocket path `/ws/{coop_id}` with `/v1/ws/{coop_id}`.
- Clarified monitoring wording from "Web Dashboard" to explicit "Gateway Health Endpoint" to avoid implying a UI at gateway root.

## Verification updates (Batch C20)

```bash
rg -n "/health\\b|/v1/health|ws://localhost:8080/ws|/v1/ws/" docs/GETTING_STARTED.md
```

Result: only `/v1/health` and `/v1/ws/{coop_id}` variants remain.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C20)

- `docs/GETTING_STARTED.md | docs/api/openapi.yaml + icn/crates/icn-gateway/src/server.rs | rg -n "/v1/health|/v1/ws/\\{coop_id\\}|scope\\(\"/v1\"\\)" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C20)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C20; continue active-doc content drift cleanup.

## Changes applied (Batch C21 - SDIS quick-start endpoint/body alignment)

1. `docs/sdis/SDIS_QUICK_START.md`

Changes:

- Replaced legacy enrollment examples (`/v1/sdis/enroll*`) with currently wired flow:
  - `POST /v1/sdis/enrollment/start`
  - `POST /v1/sdis/vouch/{id}`
  - `GET /v1/sdis/status/{id}`
- Updated request/response examples to match current simple-enrollment request models (`identity_name`, `coop_id`, `vouch_statement`) and status shape.
- Replaced legacy anchor examples (`/v1/sdis/anchors*`) with current anchor routes:
  - `POST /v1/sdis/anchor/devices/add`
  - `GET /v1/sdis/anchor/{id}/devices`
- Updated advanced endpoint table from legacy `enroll/anchors/recover` names to current `anchor/*` and `recovery/*` route shapes.
- Added explicit caveat that JavaScript SDK snippet is conceptual and should be verified against current `sdk/typescript` exports.

## Verification updates (Batch C21)

```bash
rg -n "/v1/sdis/enroll|/v1/sdis/anchors|/v1/sdis/recover/initiate|/v1/sdis/enrollment/start|/v1/sdis/anchor/|/v1/sdis/recovery/" docs/sdis/SDIS_QUICK_START.md
```

Result: legacy `enroll/anchors/recover/initiate` forms removed; current `enrollment/anchor/recovery` forms present.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C21)

- `docs/sdis/SDIS_QUICK_START.md | icn/crates/icn-gateway/src/api/sdis/{simple_enrollment.rs,anchor.rs,recovery.rs} | rg -n "enrollment/start|vouch/\\{enrollment_id\\}|status/\\{enrollment_id\\}|scope\\(\"/anchor\"\\)|scope\\(\"/recovery\"\\)" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C21)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C21; continue active-doc content drift cleanup.

## Changes applied (Batch C22 - active guide/manual v1 route normalization)

Touched files:

1. `docs/guides/operations/operations-guide.md`
2. `docs/onboarding/workshops/workshop-07-gateway-sdk.md`
3. `docs/onboarding/workshops/workshop-09-ops.md`
4. `docs/manual/ICN_USER_MANUAL.md`
5. `docs/ops/runbooks/05-troubleshooting.md`
6. `docs/development/testing/DR_TEST_RESULTS.md`
7. `docs/reference/api/API_REFERENCE.md`

Changes:

- Replaced legacy health/websocket/auth path usage in active docs:
  - `/health` -> `/v1/health`
  - `/health/detailed` -> `/v1/health/detailed`
  - `ws://.../ws/...` -> `ws://.../v1/ws/...`
  - `/v1/auth/token` -> `/v1/auth/verify`
- Normalized workshop text/diagrams and key-takeaway tables to v1 route names.
- Updated manual appendix endpoint table to current route shapes for auth, identity resolution, ledger, and governance path prefixes.
- Updated operations guide wording from implied gateway-root “dashboard” to explicit health + metrics checks.
- Added explicit snapshot caveat to `docs/reference/api/API_REFERENCE.md` that authoritative routes are in OpenAPI docs.

## Verification updates (Batch C22)

```bash
rg -n "localhost:8080/health\\b|localhost:8080/health/detailed\\b|ws://localhost:8080/ws\\b|/v1/auth/token\\b|/v1/v1/" docs/guides/operations/operations-guide.md docs/onboarding/workshops/workshop-07-gateway-sdk.md docs/onboarding/workshops/workshop-09-ops.md docs/manual/ICN_USER_MANUAL.md docs/ops/runbooks/05-troubleshooting.md docs/development/testing/DR_TEST_RESULTS.md docs/reference/api/API_REFERENCE.md
```

Result: no matches in touched files.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C22)

- `docs/guides/operations/operations-guide.md | icn/crates/icn-gateway/src/server.rs + docs/api/openapi.yaml | rg -n "scope\\(\"/v1\"\\)|/v1/health" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/onboarding/workshops/workshop-07-gateway-sdk.md | icn/crates/icn-gateway/src/server.rs | rg -n "/v1/auth/challenge|/v1/auth/verify|/v1/ws/\\{coop_id\\}|/v1/health" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/onboarding/workshops/workshop-09-ops.md | icn/crates/icn-gateway/src/api/health.rs | rg -n "/health|/health/detailed" + source review (scoped under /v1) | aligned | reviewed_on(2026-02-11)`
- `docs/manual/ICN_USER_MANUAL.md | icn/crates/icn-gateway/src/{server.rs,models.rs} + docs/api/openapi.yaml | rg -n "/v1/auth/verify|/v1/ledger/\\{coop_id\\}/balance/\\{did\\}|/v1/gov/proposals|/v1/health" + source review | aligned | reviewed_on(2026-02-11)`
- `docs/reference/api/API_REFERENCE.md | docs/api/openapi.yaml + docs/api/OPENAPI.md | manual review + snapshot caveat insertion | partially-aligned-with-authoritative-pointer | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C22)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C22; continue active-doc content drift cleanup.

## Changes applied (Batch C23 - SDIS API guide route-shape correction)

1. `docs/sdis/SDIS_API_GUIDE.md`

Changes:

- Updated anchor management section from legacy `/v1/sdis/anchors*` routes to current wired anchor routes:
  - `GET /v1/sdis/anchor/{anchor_id}/devices`
  - `POST /v1/sdis/anchor/devices/add`
  - `GET /v1/sdis/anchor/{anchor_id}`
  - `POST /v1/sdis/anchor/rotate-keys`
- Updated recovery section from `/v1/sdis/recovery/initiate` and top-level `/v1/sdis/recovery/complete` to current wired paths:
  - `POST /v1/sdis/recovery/start`
  - `GET /v1/sdis/recovery/{recovery_id}`
  - `POST /v1/sdis/recovery/{recovery_id}/complete`
- Realigned request/response examples to the current request structs (`anchor_id`, `device_pubkey`, `verification_data`, `new_keybundle`, optional `recovery_share`).
- Updated integration flow shorthand to match `anchor/*` and `recovery/start` naming.

## Verification updates (Batch C23)

```bash
rg -n "/v1/sdis/anchors\\b|/v1/sdis/anchors/|/v1/sdis/recovery/initiate\\b|/v1/sdis/recovery/complete\\b|/v1/sdis/recover" docs/sdis/SDIS_API_GUIDE.md
```

Result: no legacy anchors/recover/initiate path forms remain.

```bash
rg -n "/v1/sdis/anchor/|/v1/sdis/recovery/start|/v1/sdis/recovery/\\{recovery_id\\}/complete" docs/sdis/SDIS_API_GUIDE.md
```

Result: current anchor and recovery route shapes present.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C23)

- `docs/sdis/SDIS_API_GUIDE.md | icn/crates/icn-gateway/src/api/sdis/{anchor.rs,recovery.rs} + icn/crates/icn-gateway/src/server.rs | rg -n "scope\\(\"/anchor\"\\)|scope\\(\"/recovery\"\\)|configure\\(api::sdis::anchor::configure\\)|configure\\(api::sdis::recovery::configure\\)" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C23)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C23; continue active-doc content drift cleanup.

## Changes applied (Batch C24 - SDIS design/roadmap route taxonomy refresh)

Touched files:

1. `docs/sdis/SDIS_SYSTEM.md`
2. `docs/sdis/SDIS_STEWARD_ROADMAP.md`
3. `docs/sdis/SDIS_IMPLEMENTATION_COMPLETE.md`

Changes:

- `SDIS_SYSTEM.md`:
  - Added explicit route-note that some design examples retain legacy naming.
  - Updated Gateway API integration summary to current route taxonomy:
    - enrollment workflow (`/v1/sdis/enrollment/*`, `/status/*`, `/vouch/*`, `/reject/*`)
    - anchor (`/v1/sdis/anchor/*`)
    - recovery (`/v1/sdis/recovery/*`)
    - verification/ephemeral (`/v1/sdis/verify/*`, `/v1/sdis/ephemeral/*`)
- `SDIS_STEWARD_ROADMAP.md`:
  - Updated status sentence from “not exposed” to “exposed but needs productization hardening.”
  - Replaced legacy endpoint block with currently wired route shapes from `server.rs` + SDIS module configs.
- `SDIS_IMPLEMENTATION_COMPLETE.md`:
  - Added explicit note that legacy endpoint naming may appear and to prefer `SDIS_API_GUIDE.md` for current route shapes.

## Verification updates (Batch C24)

```bash
rg -n "legacy naming|/v1/sdis/enrollment/\\*|/v1/sdis/anchor/\\*|/v1/sdis/recovery/\\*|aren't exposed|/v1/sdis/enrollment/start|/v1/sdis/recovery/start" docs/sdis/SDIS_SYSTEM.md docs/sdis/SDIS_IMPLEMENTATION_COMPLETE.md docs/sdis/SDIS_STEWARD_ROADMAP.md
```

Result: route taxonomy updates and explicit caveats present; stale “not exposed” claim removed.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C24)

- `docs/sdis/SDIS_SYSTEM.md + docs/sdis/SDIS_STEWARD_ROADMAP.md + docs/sdis/SDIS_IMPLEMENTATION_COMPLETE.md | icn/crates/icn-gateway/src/server.rs + icn/crates/icn-gateway/src/api/sdis/{simple_enrollment.rs,anchor.rs,recovery.rs} | rg -n "scope\\(\"/sdis\"\\)|configure\\(api::sdis::simple_enrollment::configure\\)|configure\\(api::sdis::anchor::configure\\)|configure\\(api::sdis::recovery::configure\\)" + source review | aligned-with-design-caveats | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C24)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C24; continue active-doc content drift cleanup.

## Changes applied (Batch C25 - incident-response gateway health path correction)

1. `docs/operations/deployment/incident-response.md`

Changes:

- Replaced remaining gateway health checks from `/health` to `/v1/health` in incident response workflows.

## Verification updates (Batch C25)

```bash
rg -n "localhost:8080/health\\b|localhost:8080/v1/health\\b" docs/operations/deployment/incident-response.md
```

Result: all touched checks now use `/v1/health`.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C25)

- `docs/operations/deployment/incident-response.md | icn/crates/icn-gateway/src/server.rs + docs/api/openapi.yaml | rg -n "/v1/health|scope\\(\"/v1\"\\)" + source review | aligned | reviewed_on(2026-02-11)`

## Recursive self-correction score (Batch C25)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C25; continue active-doc content drift cleanup.

## Changes applied (Batch C26 - deployment/demo snapshot framing normalization)

Touched files:

1. `docs/deployment/DEPLOYMENT_COMPLETE.md`
2. `docs/deployment/DEPLOYMENT_GUIDE.md`
3. `docs/deployment/QUICK_DEPLOY.md`
4. `docs/status/CURRENT_SYSTEM_STATUS.md`
5. `docs/demo/DEMO_QUICK_START.md`

Changes:

- Converted undated "production-ready/current" framing to explicit historical or reference framing.
- Added explicit guidance to validate runtime truth via live checks and `docs/ci/CI_CURRENT_STATUS.md`.
- Preserved command examples while clarifying they are point-in-time references, not current guarantees.

## Verification updates (Batch C26)

```bash
rg -n "Historical snapshot|Historical quick-reference snapshot|validate against current runtime|current deployment truth|2025-12-18 snapshot" docs/deployment/DEPLOYMENT_COMPLETE.md docs/deployment/DEPLOYMENT_GUIDE.md docs/deployment/QUICK_DEPLOY.md docs/status/CURRENT_SYSTEM_STATUS.md docs/demo/DEMO_QUICK_START.md
```

Result: snapshot/reference framing present in all touched docs.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C26)

- `docs/deployment/DEPLOYMENT_COMPLETE.md + docs/deployment/DEPLOYMENT_GUIDE.md + docs/deployment/QUICK_DEPLOY.md + docs/status/CURRENT_SYSTEM_STATUS.md + docs/demo/DEMO_QUICK_START.md | icn/crates/icn-core/src/config/gateway.rs + icn/bins/icnd/src/main.rs + docs/ci/CI_CURRENT_STATUS.md | rg -n "default_gateway_bind_addr|init_gateway_port|gateway_port|Status:|Historical" + doc_reality_scan.sh | aligned-with-explicit-snapshot-context | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C26)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C26; continue active-doc content drift cleanup.

## Changes applied (Batch C27 - active status/readiness language normalization)

Touched files:

1. `docs/deployment/DEPLOYMENT_READY.md`
2. `docs/deployment/DEPLOYMENT_INVITE_SYSTEM.md`
3. `docs/status/DEPLOYMENT_VERIFICATION.md`
4. `docs/operations/README.md`
5. `docs/security/README.md`

Changes:

- Reframed remaining present-tense readiness claims in active operational docs to explicit snapshot wording.
- Added guidance to consult `docs/ci/CI_CURRENT_STATUS.md` for current posture where missing.
- Preserved historical context while removing implied "current" assertions.

## Verification updates (Batch C27)

```bash
rg -n "Status at Snapshot Date|Historical deployment snapshot|Historical assessment snapshot|Status Snapshot \\(2025-12-04\\)|current invite-system readiness" docs/deployment/DEPLOYMENT_READY.md docs/deployment/DEPLOYMENT_INVITE_SYSTEM.md docs/status/DEPLOYMENT_VERIFICATION.md docs/operations/README.md docs/security/README.md
```

Result: normalized status framing present in all touched docs.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C27)

- `docs/deployment/DEPLOYMENT_READY.md + docs/deployment/DEPLOYMENT_INVITE_SYSTEM.md + docs/status/DEPLOYMENT_VERIFICATION.md + docs/operations/README.md + docs/security/README.md | docs/ci/CI_CURRENT_STATUS.md + icn/bins/icnd/src/main.rs + icn/crates/icn-core/src/config/gateway.rs | rg -n "Status|Historical|snapshot|CI_CURRENT_STATUS" + doc_reality_scan.sh | aligned-with-snapshot-policy | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C27)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C27; continue active-doc content drift cleanup.

## Changes applied (Batch C28 - demo/mobile/testing status language normalization)

Touched files:

1. `docs/demo/DEMO_SUCCESS_FINAL.md`
2. `docs/demo/DEMO_INFRASTRUCTURE_COMPLETE.md`
3. `docs/demo/DEMO_CURRENT_STATUS.md`
4. `docs/mobile/MOBILE_WALLET_INTEGRATION.md`
5. `docs/mobile/MOBILE_APP_STATUS.md`
6. `docs/testing/INTEGRATION_TEST_STATUS.md`
7. `docs/status/DEPLOYMENT_VERIFICATION.md`

Changes:

- Replaced remaining present-tense "fully operational/production-ready" claims with explicit snapshot framing.
- Preserved historical outcomes while removing implied current-readiness guarantees.
- Kept existing historical date context and current-status pointers intact.

## Verification updates (Batch C28)

```bash
rg -n "Historical demo success snapshot|Operational in Snapshot Environment|production-capable for the documented scope|Status at Snapshot Date|Confidence \\(snapshot\\)|Historical integration snapshot" docs/demo/DEMO_SUCCESS_FINAL.md docs/demo/DEMO_INFRASTRUCTURE_COMPLETE.md docs/demo/DEMO_CURRENT_STATUS.md docs/mobile/MOBILE_WALLET_INTEGRATION.md docs/mobile/MOBILE_APP_STATUS.md docs/testing/INTEGRATION_TEST_STATUS.md docs/status/DEPLOYMENT_VERIFICATION.md
```

Result: snapshot-oriented status language present in all touched docs.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C28)

- `docs/demo/DEMO_SUCCESS_FINAL.md + docs/demo/DEMO_INFRASTRUCTURE_COMPLETE.md + docs/demo/DEMO_CURRENT_STATUS.md + docs/mobile/MOBILE_WALLET_INTEGRATION.md + docs/mobile/MOBILE_APP_STATUS.md + docs/testing/INTEGRATION_TEST_STATUS.md + docs/status/DEPLOYMENT_VERIFICATION.md | docs/ci/CI_CURRENT_STATUS.md + snapshot date headers in touched docs | rg -n "Status|Historical|snapshot|production-ready|fully operational" + doc_reality_scan.sh | aligned-with-snapshot-policy | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C28)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C28; continue active-doc content drift cleanup.

## Changes applied (Batch C29 - remaining active wording normalization)

Touched files:

1. `docs/demo/DEMO_FINAL_STATUS.md`
2. `docs/demo/DEMO_INFRASTRUCTURE_COMPLETE.md`
3. `docs/demo/DEMO_PROGRESS_UPDATE.md`
4. `docs/demo/DEMO_SESSION_COMPLETE.md`
5. `docs/mobile/MOBILE_APP_STATUS.md`
6. `docs/operations/deployment/incident-response.md`
7. `docs/status/FINAL_SESSION_STATUS.md`

Changes:

- Replaced remaining "production-ready/current status" phrases in active-facing docs with snapshot-aware wording.
- Updated section labels from "Current Status" to "Snapshot Status" where content is historical.
- Kept operational meaning while avoiding present-tense readiness assertions.

## Verification updates (Batch C29)

```bash
rg -n "Historical demo completion snapshot|validated for the snapshot demo flow|operational for the snapshot demo flow|production-capable for pilot scope|Snapshot Status|Status at this update" docs/demo/DEMO_FINAL_STATUS.md docs/demo/DEMO_INFRASTRUCTURE_COMPLETE.md docs/demo/DEMO_PROGRESS_UPDATE.md docs/demo/DEMO_SESSION_COMPLETE.md docs/mobile/MOBILE_APP_STATUS.md docs/operations/deployment/incident-response.md docs/status/FINAL_SESSION_STATUS.md
```

Result: snapshot-normalized wording present across all touched docs.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C29)

- `docs/demo/DEMO_FINAL_STATUS.md + docs/demo/DEMO_INFRASTRUCTURE_COMPLETE.md + docs/demo/DEMO_PROGRESS_UPDATE.md + docs/demo/DEMO_SESSION_COMPLETE.md + docs/mobile/MOBILE_APP_STATUS.md + docs/operations/deployment/incident-response.md + docs/status/FINAL_SESSION_STATUS.md | docs/ci/CI_CURRENT_STATUS.md + snapshot date headers in touched docs | rg -n "Status|snapshot|production-ready|current status" + doc_reality_scan.sh | aligned-with-snapshot-policy | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C29)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C29; proceed with branch sync against main.

## Changes applied (Batch C30 - post-merge active wording cleanup)

Touched files:

1. `docs/deployment/DEPLOYMENT_READY.md`
2. `docs/security/README.md`
3. `docs/security/SECURITY_FOLLOWUP.md`
4. `docs/mobile/MOBILE_TODO.md`

Changes:

- Normalized residual "current status/deployment ready" phrasing to explicit snapshot framing.
- Preserved historical context and technical content; removed implied present-tense readiness assertions.

## Verification updates (Batch C30)

```bash
rg -n "Current Status|READY FOR DEPLOYMENT|Status Snapshot \\(2025-12-18\\)|Status Snapshot|Deployment-ready assessment" docs/deployment/DEPLOYMENT_READY.md docs/security/README.md docs/security/SECURITY_FOLLOWUP.md docs/mobile/MOBILE_TODO.md
```

Result: status framing normalized in touched docs.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C30)

- `docs/deployment/DEPLOYMENT_READY.md + docs/security/README.md + docs/security/SECURITY_FOLLOWUP.md + docs/mobile/MOBILE_TODO.md | docs/ci/CI_CURRENT_STATUS.md + local snapshot date headers | rg -n "Status|snapshot|ready" + doc_reality_scan.sh | aligned-with-snapshot-policy | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C30)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C30; continue active-doc content drift cleanup.

## Changes applied (Batch C31 - post-merge audit correction for liveness path)

Touched files:

1. `docs/internal/status/DOCS_REALITY_AUDIT_2026-02-11.md`

Changes:

- Corrected historical Batch C13 note to reflect actual gateway liveness route wiring (`/v1/healthz`), matching current `scripts/pilot_receipt_chain_demo.sh` and gateway server scope.

## Verification updates (Batch C31)

```bash
rg -n "/v1/healthz|/healthz" scripts/pilot_receipt_chain_demo.sh scripts/demo-single-node.sh icn/crates/icn-gateway/src/server.rs icn/crates/icn-gateway/src/api/health.rs docs/internal/status/DOCS_REALITY_AUDIT_2026-02-11.md
```

Result: liveness endpoint references align on `/v1/healthz` for gateway-scoped checks; no contradictory statement remains in the audit note.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C31)

- `docs/internal/status/DOCS_REALITY_AUDIT_2026-02-11.md | scripts/pilot_receipt_chain_demo.sh + icn/crates/icn-gateway/src/{server.rs,api/health.rs} | rg -n "/v1/healthz|scope\\(\"/v1\"\\)|\\[get\\(\"/healthz\"\\)\\]" + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C31)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C31; continue active-doc content drift cleanup.

## Changes applied (Batch C32 - active docs path/status cleanup)

Touched files:

1. `docs/testing/TESTING_SUMMARY.md`
2. `docs/pilots/hosted-approach.md`
3. `docs/security/SECURITY_TESTING_GUIDE.md`
4. `docs/security/GATEWAY_CSP.md`
5. `docs/onboarding/reference/module-09-ops-deploy.md`
6. `docs/onboarding/reference/module-10-contributor-workflow.md`
7. `docs/onboarding/reference/module-12-observability.md`
8. `docs/onboarding/reference/module-13-security-privacy.md`
9. `docs/onboarding/reading-map.md`
10. `docs/onboarding/workshops/workshop-04-identity-trust.md`
11. `docs/performance/trust-service-performance.md`

Changes:

- Replaced stale references to `docs/production-hardening.md` with canonical `docs/security/production-hardening.md` in active docs.
- Reworded one hard readiness conclusion and one pilot strategy statement to explicit snapshot context.

## Verification updates (Batch C32)

```bash
rg -n "docs/production-hardening\\.md|production-ready substrate|ready for deployment\\.|Snapshot conclusion \\(2025-12-18\\)" docs/testing/TESTING_SUMMARY.md docs/pilots/hosted-approach.md docs/security/SECURITY_TESTING_GUIDE.md docs/security/GATEWAY_CSP.md docs/onboarding/reference/module-09-ops-deploy.md docs/onboarding/reference/module-10-contributor-workflow.md docs/onboarding/reference/module-12-observability.md docs/onboarding/reference/module-13-security-privacy.md docs/onboarding/reading-map.md docs/onboarding/workshops/workshop-04-identity-trust.md docs/performance/trust-service-performance.md
```

Result: stale `docs/production-hardening.md` references removed in touched active docs; snapshot wording applied in targeted status text.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C32)

- `docs/testing/TESTING_SUMMARY.md + docs/pilots/hosted-approach.md + docs/security/SECURITY_TESTING_GUIDE.md + docs/security/GATEWAY_CSP.md + docs/onboarding/reference/module-09-ops-deploy.md + docs/onboarding/reference/module-10-contributor-workflow.md + docs/onboarding/reference/module-12-observability.md + docs/onboarding/reference/module-13-security-privacy.md + docs/onboarding/reading-map.md + docs/onboarding/workshops/workshop-04-identity-trust.md + docs/performance/trust-service-performance.md | docs/security/production-hardening.md + docs/ci/CI_CURRENT_STATUS.md + targeted grep checks | rg -n "production-hardening\\.md|production-ready|ready for deployment|snapshot" + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C32)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C32; continue active-doc content drift cleanup.

## Changes applied (Batch C33 - archive labeling hardening)

Touched files:

1. `docs/archive/README.md` (new)
2. `docs/archive/2026/README.md` (new)
3. `docs/archive/2026/demo-sessions/*.md` (20 files)

Changes:

- Added top-level archive policy documentation to clearly mark archived material as non-authoritative.
- Added 2026 archive README with scope and current-truth pointers.
- Added a standardized "Archived Document Notice (2026-02-12)" banner to every archived demo-session markdown file.

## Verification updates (Batch C33)

```bash
rg -n "Archived Document Notice" docs/archive/2026/demo-sessions/*.md | wc -l
```

Result: `20` archived demo-session files contain explicit archive warnings.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C33)

- `docs/archive/README.md + docs/archive/2026/README.md + docs/archive/2026/demo-sessions/*.md | archive policy + current truth map (docs/INDEX.md, docs/STATE.md, docs/ci/CI_CURRENT_STATUS.md) | rg -n "Archived Document Notice|historical|authoritative" + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C33)

- Accuracy: 5/5
- Completeness: 5/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C33; continue targeted stale-claim cleanup.

## Changes applied (Batch C34 - final active-area stale-claim trim)

Touched files:

1. `docs/deployment/DEPLOYMENT_ACTIVE.md`
2. `docs/guides/FAQ.md`

Changes:

- Reworded one remaining active deployment claim from absolute present-tense to snapshot-context wording.
- Softened FAQ substrate phrasing from "production-ready" to "production-capable" while retaining explicit snapshot context.

## Verification updates (Batch C34)

```bash
rg -n "\\bproduction-ready\\b|\\bfully operational\\b|\\bready for deployment\\b" docs/testing docs/pilots docs/deployment docs/status docs/security docs/mobile docs/operations docs/sdis docs/guides --glob '*.md'
```

Result: only the FAQ section title phrase remains (`Is ICN production-ready?`), with body content now snapshot-qualified.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C34)

- `docs/deployment/DEPLOYMENT_ACTIVE.md + docs/guides/FAQ.md | snapshot language policy + current-status docs | rg stale-claim scan + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C34)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C34; continue active-doc sweep as needed.

## Changes applied (Batch C35 - archive 2025 warning coverage completion)

Touched files:

1. `docs/archive/2025/KUBERNETES_DEPLOYMENT.md`
2. `docs/archive/2025/PRODUCTION_DEPLOYMENT_GUIDE.md`
3. `docs/archive/2025/TRANSITION_PLAN.md`
4. `docs/archive/2025/hsm-tpm-follow-up-issues.md`
5. `docs/archive/2025/hsm-tpm-roadmap.md`
6. `docs/archive/2025/phase-16c-plan.md`
7. `docs/archive/2025/tpm-implementation-plan.md`
8. `docs/archive/2025/tpm-setup.md`

Changes:

- Added standardized `Archived Document Notice (2026-02-12)` banner to every previously unmarked 2025 archive markdown file.
- Ensured archive corpus communicates non-authoritative status consistently across years.

## Verification updates (Batch C35)

```bash
for f in docs/archive/2025/*.md; do
  [ \"$(basename \"$f\")\" = \"README.md\" ] && continue
  rg -qi \"ARCHIVED|historical snapshot|Archived Document Notice\" \"$f\" || echo \"$f\"
done
```

Result: no unmarked 2025 archive markdown files remain.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C35)

- `docs/archive/2025/{KUBERNETES_DEPLOYMENT.md,PRODUCTION_DEPLOYMENT_GUIDE.md,TRANSITION_PLAN.md,hsm-tpm-follow-up-issues.md,hsm-tpm-roadmap.md,phase-16c-plan.md,tpm-implementation-plan.md,tpm-setup.md} | archive labeling policy + docs/archive/README.md | archive-marker scan + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C35)

- Accuracy: 5/5
- Completeness: 5/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C35; continue targeted stale-claim cleanup.

## Changes applied (Batch C36 - sprint/testing readiness wording normalization)

Touched files:

1. `docs/development/testing/DR_TEST_RESULTS.md`
2. `docs/development/testing/INTERNAL_TESTING_PLAN.md`
3. `docs/development/testing/BETA_TESTING_GUIDE.md`
4. `docs/development/federation-roadmap-implementation.md`
5. `docs/onboarding/reference/module-09-ops-deploy.md`
6. `docs/sprints/SPRINT2_PROGRESS.md`
7. `docs/sprints/SPRINT2_COMPLETE.md`
8. `docs/sprints/SPRINT3_COMPLETE.md`
9. `docs/sprints/SPRINT3_STATUS.md`

Changes:

- Replaced absolute present-tense "production-ready / ready to deploy" phrasing with snapshot-qualified or production-capable wording.
- Kept historical meaning and quantitative evidence while removing implied current-state guarantees.
- Renamed one lingering "Current Status" section header to "Status Snapshot" in federation roadmap documentation.

## Verification updates (Batch C36)

```bash
rg -n "production-ready|fully operational|READY TO DEPLOY|ready for deployment|Current Status|current status" docs/development/testing/DR_TEST_RESULTS.md docs/development/testing/INTERNAL_TESTING_PLAN.md docs/development/testing/BETA_TESTING_GUIDE.md docs/development/federation-roadmap-implementation.md docs/onboarding/reference/module-09-ops-deploy.md docs/sprints/SPRINT2_PROGRESS.md docs/sprints/SPRINT2_COMPLETE.md docs/sprints/SPRINT3_COMPLETE.md docs/sprints/SPRINT3_STATUS.md
```

Result: stale-claim matches cleared from these targeted files.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C36)

- `docs/development/testing/{DR_TEST_RESULTS.md,INTERNAL_TESTING_PLAN.md,BETA_TESTING_GUIDE.md} + docs/development/federation-roadmap-implementation.md + docs/onboarding/reference/module-09-ops-deploy.md + docs/sprints/{SPRINT2_PROGRESS.md,SPRINT2_COMPLETE.md,SPRINT3_COMPLETE.md,SPRINT3_STATUS.md} | snapshot language policy + docs/ci/CI_CURRENT_STATUS.md | stale-claim grep + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C36)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C36; continue targeted stale-claim cleanup.

## Changes applied (Batch C37 - architecture historical-context banners)

Touched files:

1. `docs/architecture/COMPREHENSIVE_ARCHITECTURE_REVIEW_2025-12-17.md`
2. `docs/architecture/ARCHITECTURE_REVIEW_SUMMARY.md`
3. `docs/architecture/ARCHITECTURE_UPDATE_2025-12-17.md`
4. `docs/architecture/SESSION_SUMMARY_2025-12-17_ARCHITECTURE_AUDIT.md`
5. `docs/architecture/ARCHITECTURE_INDEX.md`
6. `docs/architecture/ARCHITECTURE_QUICK_REF.md`
7. `docs/architecture/ARCHITECTURE_MAP.md`

Changes:

- Added explicit historical/snapshot banners at file tops for dated 2025 architecture review outputs.
- Clarified that these documents are point-in-time analysis and pointed readers to current canonical docs.
- Softened one status metadata line in `ARCHITECTURE_MAP.md` from pilot-readiness label to snapshot context wording.

## Verification updates (Batch C37)

```bash
rg -n "Historical|snapshot|current architecture|current status" docs/architecture/COMPREHENSIVE_ARCHITECTURE_REVIEW_2025-12-17.md docs/architecture/ARCHITECTURE_REVIEW_SUMMARY.md docs/architecture/ARCHITECTURE_UPDATE_2025-12-17.md docs/architecture/SESSION_SUMMARY_2025-12-17_ARCHITECTURE_AUDIT.md docs/architecture/ARCHITECTURE_INDEX.md docs/architecture/ARCHITECTURE_QUICK_REF.md docs/architecture/ARCHITECTURE_MAP.md
```

Result: all touched architecture review docs now include explicit historical-context framing.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C37)

- `docs/architecture/{COMPREHENSIVE_ARCHITECTURE_REVIEW_2025-12-17.md,ARCHITECTURE_REVIEW_SUMMARY.md,ARCHITECTURE_UPDATE_2025-12-17.md,SESSION_SUMMARY_2025-12-17_ARCHITECTURE_AUDIT.md,ARCHITECTURE_INDEX.md,ARCHITECTURE_QUICK_REF.md,ARCHITECTURE_MAP.md} | snapshot language policy + current truth pointers (docs/ARCHITECTURE.md, docs/STATE.md, docs/ci/CI_CURRENT_STATUS.md) | historical-banner grep + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C37)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C37; continue targeted stale-claim cleanup.

## Changes applied (Batch C38 - SDIS/sprint/deployment snapshot wording pass)

Touched files:

1. `docs/deployment/DEPLOYMENT_STATUS_2025-12-12.md`
2. `docs/sprints/SPRINT2_COMPLETE.md`
3. `docs/sprints/SPRINT3_STATUS.md`
4. `docs/sprints/SPRINT3_DAY1_COMPLETE.md`
5. `docs/sprints/SPRINT_COMPLETE_2025-12-16.md`
6. `docs/sdis/SDIS_SYSTEM.md`
7. `docs/sdis/SDIS_STEWARD_ROADMAP.md`
8. `docs/sdis/SDIS_SESSION_COMPLETE_20251212.md`

Changes:

- Reframed residual pilot-readiness language in dated deployment/sprint docs to explicit snapshot assessments.
- Renamed remaining "Current Status" headings to "Status Snapshot" in targeted SDIS docs.
- Added historical snapshot banner and wording adjustments to `SDIS_SESSION_COMPLETE_20251212.md` to avoid present-tense readiness interpretation.

## Verification updates (Batch C38)

```bash
rg -n "production-ready|fully operational|PILOT-READY|Current Status|current status" docs/deployment/DEPLOYMENT_STATUS_2025-12-12.md docs/sprints/SPRINT2_COMPLETE.md docs/sprints/SPRINT3_STATUS.md docs/sprints/SPRINT3_DAY1_COMPLETE.md docs/sprints/SPRINT_COMPLETE_2025-12-16.md docs/sdis/SDIS_SYSTEM.md docs/sdis/SDIS_STEWARD_ROADMAP.md docs/sdis/SDIS_SESSION_COMPLETE_20251212.md
```

Result: target files now use snapshot framing; remaining matches are contextual references (for example, "For current status..." pointers and roadmap action text), not unqualified current-state assertions.

```bash
./.codex/skills/icn-docs-reality-sync/scripts/doc_reality_scan.sh .
```

Result: no broken relative links; expected marker-only findings remain.

## Audit ledger additions (Batch C38)

- `docs/deployment/DEPLOYMENT_STATUS_2025-12-12.md + docs/sprints/{SPRINT2_COMPLETE.md,SPRINT3_STATUS.md,SPRINT3_DAY1_COMPLETE.md,SPRINT_COMPLETE_2025-12-16.md} + docs/sdis/{SDIS_SYSTEM.md,SDIS_STEWARD_ROADMAP.md,SDIS_SESSION_COMPLETE_20251212.md} | snapshot language policy + current-status pointer policy | targeted stale-claim grep + doc_reality_scan.sh | aligned | reviewed_on(2026-02-12)`

## Recursive self-correction score (Batch C38)

- Accuracy: 5/5
- Completeness: 4/5
- Consistency: 5/5
- Verifiability: 5/5

Trigger status: all scores >= 4 for Batch C38; continue targeted stale-claim cleanup.
