Status: discovery runbook
Authority: dev runbook
Audience: ICN developers + NYCN package operators
Cluster: K3s on Hyperion (`10.8.30.40`); local fallback documented

# NYCN smoke fixture → ICN gateway proof path

This runbook documents the verified path for driving the NYCN
icnctl-loadable smoke fixture (`InterCooperative-Network/nycn`,
`institution/smoke/icnctl/`, landed via [nycn#18](https://github.com/InterCooperative-Network/nycn/pull/18))
into an ICN gateway, obtaining a member-standing projection and
action-card queue, and the remaining gap to the full
package-driven action-item completion receipt path.

It is a **discovery runbook**: the steps it documents have been
executed against a real local gateway and read-only against the K3s
deployment. The K3s `apply` step is documented but deliberately left
for an explicit operator decision — this runbook does not perform
cluster mutation.

## Scope

Proves end-to-end:

- An NYCN-repo package directory loads cleanly into a real `icnd`
  gateway via `icnctl institution bootstrap apply`.
- The gateway then accepts JWT-authenticated requests for
  `/v1/gov/me/standing` and `/v1/gov/me/action-cards` and returns
  well-formed responses.
- The deployed K3s image carries the required loader, auth flow, and
  governance routes.

Does **not** prove (intentional gap, see "Remaining gap"):

- That `icnctl institution bootstrap apply` produces an action item,
  open proposal, or meeting attendance state. The current
  `BootstrapPlan` operations are entity / governance-domain /
  structure / activity / program / milestone / role-assignment only.
- That a member can complete an action item through the gateway and
  retrieve `ActionItemCompletionReceipt` from a package-driven
  bootstrap. ICN-side integration tests cover this in-process
  (`apps/governance/tests/me_action_item_receipt_chain.rs` —
  6/6 passing on `2438a362`); a package-driven seed step does not
  yet exist.

## Repo state used

- ICN: `2438a362` (main).
- NYCN: `8d70068f` (main, after #18 merge).
- NYCN smoke fixture: `institution/smoke/icnctl/` — single federation
  (`nycn-icnctl-smoke-federation`) and single governance domain
  (`nycn-icnctl-smoke-federation-gov`), `package_id:
  nycn-icnctl-smoke`. Distinct ids avoid collision with ICN's
  canonical NYCN test fixture (`institutions/nycn/`,
  `package_id: nycn`).

All `<icn-repo-root>` references below resolve to the directory
containing this file's repo (`git rev-parse --show-toplevel` from
`/home/<user>/projects/icn`). All `<nycn-repo-root>` references
resolve to the parallel NYCN checkout
(`/home/<user>/projects/nycn`).

## Prerequisites

1. **Built `icnd` and `icnctl`** in the ICN Rust workspace
   (`<icn-repo-root>/icn`):

   ```sh
   cargo build -p icnd -p icnctl
   ```

   First-build wall time on this machine: ~1m 45s for `icnd`;
   `icnctl` was already built. Output binaries land at
   `<icn-repo-root>/icn/target/debug/{icnd,icnctl}`.

2. **An icnctl identity in a temporary data directory.** Use a
   distinct passphrase and a path that is not the operator's primary
   `~/.icn` so the smoke run cannot collide with real keys:

   ```sh
   ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' \
     <icn-repo-root>/icn/target/debug/icnctl \
     --data-dir /tmp/icnctl-bootstrap-test \
     id init
   ```

   Records the DID under `/tmp/icnctl-bootstrap-test/identity.age`
   (v4 keystore, age-encrypted). The passphrase here is for a
   throwaway smoke identity only; do not reuse for real deployments.

## Path A — local gateway (verified against `icnd 2438a362`)

This path runs an ephemeral local daemon. It is the safest option
when K3s is unavailable, when validating image-side changes, or
when developing on top of unmerged ICN changes.

### A.1 Start a local gateway

`start-gateway-test.sh` at the ICN repo root is the canonical
launcher for "local NYCN bootstrap live validation" and binds
`127.0.0.1:8085` against ephemeral data dir `/tmp/icn-bootstrap-test`:

```sh
cd <icn-repo-root>
./start-gateway-test.sh
```

The script generates a fresh JWT secret on first run and execs
`icnd` in the foreground. Run it under `nohup`, in a separate
terminal, or via a background job; subsequent commands assume the
gateway is bound.

**Verify:**

```sh
curl -sS http://127.0.0.1:8085/v1/health
# {"status":"ok",...}
```

### A.2 Apply the smoke fixture against the local gateway

```sh
cd <icn-repo-root>/icn
ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' \
  target/debug/icnctl \
  --data-dir /tmp/icnctl-bootstrap-test \
  institution bootstrap apply \
  --package <nycn-repo-root>/institution/smoke/icnctl \
  --gateway http://127.0.0.1:8085 \
  --coop-id smoke-test-coop
```

**Observed output (verbatim, this session, 2026-04-28):**

```
Charter status:  valid
Completed:       3
Deferred:        0
Unsupported:     1
Failed:          no

Completed steps:
  - validate charter charter/nycn-icnctl-smoke.charter.yaml: ...
  - create Federation entity nycn-icnctl-smoke-federation: created entity as
    entity:icn:federation:nycn-icnctl-smoke-federation; governance domain
    alias nycn-icnctl-smoke-federation-gov recorded for subsequent provisioning
  - provision governance domain nycn-icnctl-smoke-federation-gov:
    provisioned governance domain as nycn-icnctl-smoke-federation-gov

Unsupported steps:
  - store charter: package-owned CCL charters validate locally today, but
    there is no generic live CCL charter registration/activation sink yet

Alias bindings:
  - nycn-icnctl-smoke-federation -> entity:icn:federation:nycn-icnctl-smoke-federation
```

The "Unsupported" step is expected and constant for every package:
`run_apply_plan` always emits it. It is not a failure.

### A.3 Auth + read endpoints (local)

```sh
TOKEN=$(ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' \
  <icn-repo-root>/icn/target/debug/icnctl \
  --data-dir /tmp/icnctl-bootstrap-test \
  auth token \
  --gateway http://127.0.0.1:8085 \
  --coop-id smoke-test-coop \
  --scopes "governance:read,entity:read" \
  | grep -E "^eyJ" | head -1)

curl -sS -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8085/v1/gov/me/standing

curl -sS -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8085/v1/gov/me/action-cards
```

Note: `gov:read` is **not** an accepted scope name. The deployed
verifier accepts the long forms only. The full set is:
`ledger:{read,write}`, `coop:{read,write,admin}`,
`governance:{read,write}`, `settlements:{read,write}`,
`federation:{read,write,admin}`, `compute:{read,write}`,
`constitutional:{read,write,admin}`, `entity:{read,write,audit}`,
`treasury:{read,write}`, `admin`.

## Path B — K3s deployment (read-only verified; apply pending)

Use this when the cluster image is current enough and the operator
has accepted that smoke entities will persist in cluster state until
manually cleared.

### B.1 Confirm cluster reachability and image freshness

```sh
curl -sS http://10.8.30.40:30080/v1/health
# {"status":"ok","version":"0.1.0",...}

ssh ubuntu@10.8.30.40 \
  "sudo kubectl -n icn get pods,svc -o wide"

ssh ubuntu@10.8.30.40 \
  "sudo kubectl -n icn get deployment icn-daemon \
   -o jsonpath='{.spec.template.spec.containers[0].image}'"
```

**Observed (this session, 2026-04-28):**

- Pod `icn-daemon-7544dc7bfc-lkrkt` Running 23h on `k3s-worker-2`.
- Service `icn-nodeport` exposes 8080 → 30080 and 7777 → 30777.
- Deployed image: `10.8.30.40:30500/icn:44fc2043409a6437ae74a35e823b74446c2789c8`.

### B.2 Verify the deployed image carries the API surface

The deployed image is built from ICN commit `44fc2043` (PR #1663,
"feat(governance): add meeting attendance receipts"). NYCN main was
8 commits ahead at the time of writing; none of those commits
touched runtime code:

```sh
git log --oneline 44fc20434..2438a362 \
  -- icn/bins/icnctl icn/apps/governance \
     icn/crates/icn-governance icn/crates/icn-gateway
# (empty)
```

All eight commits between are docs/ops/RFC. The deployed image
therefore exposes the full `icnctl institution bootstrap` surface
plus the `/v1/gov/me/{standing,action-cards}` routes used below.

### B.3 Confirm endpoints exist and accept auth (read-only)

```sh
TOKEN=$(ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' \
  <icn-repo-root>/icn/target/debug/icnctl \
  --data-dir /tmp/icnctl-bootstrap-test \
  auth token \
  --gateway http://10.8.30.40:30080 \
  --coop-id smoke-test-coop \
  --scopes "governance:read,entity:read" \
  | grep -E "^eyJ" | head -1)

curl -sS -H "Authorization: Bearer $TOKEN" \
  http://10.8.30.40:30080/v1/gov/me/standing
# {"did":"did:icn:...","domains":[],"roles":[],"authority_scopes":[],"generated_at":...}

curl -sS -H "Authorization: Bearer $TOKEN" \
  http://10.8.30.40:30080/v1/gov/me/action-cards
# {"did":"did:icn:...","cards":[],"generated_at":...}
```

Both responses are well-formed empty projections — the expected
output for a DID with no standing or assigned cards on the cluster.
This proves the routes are live and JWT-bound on K3s.

### B.4 Apply against K3s (operator decision required)

The apply command for K3s mirrors the local one with a different
gateway:

```sh
ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' \
  <icn-repo-root>/icn/target/debug/icnctl \
  --data-dir /tmp/icnctl-bootstrap-test \
  institution bootstrap apply \
  --package <nycn-repo-root>/institution/smoke/icnctl \
  --gateway http://10.8.30.40:30080 \
  --coop-id smoke-test-coop
```

Before running this command, the operator must accept the
constraints in §B.5 below. This runbook does not execute it; the
local apply at §A.2 produces equivalent code-path coverage with no
cluster persistence.

### B.5 Idempotency, cleanup, persistence

**Idempotency** — `bins/icnctl/src/institution_bootstrap.rs` carries
the test `nycn_bootstrap_apply_idempotent` plus per-operation 409
conflict-coercion unit tests: each create operation maps a
`HTTP 409 Conflict` from the gateway to `ApplyOutcome::Completed`,
so a repeated apply is safe.

**Cleanup** — there is no `icnctl institution bootstrap teardown`
or generic delete-entity command. The `icnctl institution` surface
exposes only `bootstrap {validate,plan,apply}` (verify with
`target/debug/icnctl institution bootstrap --help`); no top-level
`entity` subcommand exists in `icnctl --help`. Smoke records
therefore persist in the daemon's Sled store until that store is
cleared manually:

- For local: `rm -rf /tmp/icn-bootstrap-test` after stopping `icnd`.
- For K3s: a record is written into the persistent volume backing
  `icn-daemon`. Removing it requires either a targeted entity-delete
  endpoint (does not currently exist as part of the public surface)
  or PV reset.

**Naming hygiene** — the smoke fixture's `package_id`
(`nycn-icnctl-smoke`), entity id
(`nycn-icnctl-smoke-federation`), and governance-domain id
(`nycn-icnctl-smoke-federation-gov`) are namespaced specifically so
operators can identify smoke records when reviewing cluster state.

## Action-producing follow-on (now closed locally)

The full target loop is

> NYCN package → bootstrap → standing → action card → action_item
> complete → `ActionItemCompletionReceipt`

`icnctl institution bootstrap apply` itself only operates on the
seed kinds enumerated in `crates/icn-governance/src/bootstrap.rs`
(`EntitySeed | GovernanceDomainSeed | StructureSeed |
ActivityProgramSeed | MilestoneTemplateSeed | RoleAssignmentSeed |
PersonDirectorySeed`); none of these creates an action item, open
proposal, or meeting-attendance state, so a freshly bootstrapped
node has an empty action-card queue.

The chosen bridge — option 1 below — was the post-bootstrap HTTP
runbook, and is now landed:

1. **Post-bootstrap HTTP runbook**: see
   [`NYCN_ACTION_ITEM_RECEIPT_PATH.md`](./NYCN_ACTION_ITEM_RECEIPT_PATH.md).
   Drives `POST /v1/gov/domains/{id}/action-items` →
   `PUT .../status` → `GET .../completion-receipt` against a
   bootstrapped local gateway. The receipt-retrieval endpoint
   landed in [ICN #1675](https://github.com/InterCooperative-Network/icn/pull/1675)
   (`91a63eec` on `main`). The local HTTP proof loop is therefore
   complete end-to-end.

2. **(Not pursued.) New seed kind**: a generic `ActionItemSeed`
   that the bootstrap loader maps to `POST /domains/{id}/action-items`
   during apply. Still a viable future option if a package-driven
   bootstrap should produce action items without an explicit
   post-bootstrap HTTP step. Not a blocker for the loop today.

The ICN-side in-process integration tests
(`apps/governance/tests/me_action_item_receipt_chain.rs`) cover the
same loop — 11 tests passing on `91a63eec` (extends the original
6 with 4 endpoint tests from #1675 and one canonicalization test
added during #1675 review).

## Quick spot-check (NYCN smoke fixture loadability)

If only validating that the fixture remains shape-compatible with
the loader, no gateway is required:

```sh
cd <icn-repo-root>/icn
target/debug/icnctl institution bootstrap validate \
  --package <nycn-repo-root>/institution/smoke/icnctl
target/debug/icnctl institution bootstrap plan \
  --package <nycn-repo-root>/institution/smoke/icnctl
```

Expected plan: 3 ops (validate-charter, create-federation,
provision-domain), `Live apply supported: yes`, no warnings or
blockers.

## References

- `bins/icnctl/src/institution_bootstrap.rs` — loader, plan, apply,
  auth flow, idempotency tests.
- `crates/icn-governance/src/bootstrap.rs` —
  `InstitutionBootstrapManifest` + seed-kind schema.
- `apps/governance/src/http/configure.rs` — `/v1/gov/me/*` routes.
- `apps/governance/src/http/handlers.rs` — handler implementations.
- `apps/governance/tests/me_action_item_receipt_chain.rs` — full
  proof loop, in-process.
- `start-gateway-test.sh` — local gateway launcher.
- `deploy/k8s/WORKFLOW.md`, `deploy/k8s/Makefile` — K3s deploy
  workflow.
- NYCN repo `institution/smoke/icnctl/README.md` — fixture intent
  and limits.
- NYCN repo `docs/sync/PACKAGE_PROOF_MATRIX.md` — proof-path matrix.
