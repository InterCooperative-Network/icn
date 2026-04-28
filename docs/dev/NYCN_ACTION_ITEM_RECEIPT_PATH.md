Status: discovery runbook
Authority: dev runbook
Audience: ICN developers + NYCN package operators
Cluster: local gateway (`start-gateway-test.sh`); K3s deferred until receipt-retrieval gap is closed

# NYCN action-item receipt proof path

This runbook closes the gap left open by
[`NYCN_K3S_PROOF_PATH.md`](./NYCN_K3S_PROOF_PATH.md): driving one
action item from creation to completion against a real ICN gateway,
with the smoke fixture's governance domain as the host, and showing
that the gateway emits an `ActionItemCompletionReceipt`. It also
documents the remaining narrow gap: there is no HTTP endpoint that
exposes the persisted receipt back to a caller.

It is a **discovery runbook**: every transcript below is verbatim
output from the steps as executed against `icnd 2f732176` (post
#1673 merge). It does not document what the system is supposed to
do; it documents what the system did.

## Relationship to `NYCN_K3S_PROOF_PATH.md`

`NYCN_K3S_PROOF_PATH.md` proved the package-loading half of the
loop:

> NYCN smoke fixture → `icnctl institution bootstrap apply` → empty
> `/me/standing` projection → empty `/me/action-cards` projection.

This runbook takes that as given, then proves the action-producing
half:

> bootstrap-already-applied →
> `POST /v1/gov/domains/{id}/action-items` →
> non-empty `/me/action-cards` (one `action_item / complete` card) →
> `PUT /v1/gov/domains/{id}/action-items/{id}/status` to `completed` →
> card removed → `ActionItemCompletionReceipt` emitted server-side.

It deliberately does **not** target K3s in this iteration — the
receipt-retrieval HTTP gap (see "Remaining gap") is the next thing
to close. Once a retrieval endpoint exists, the same loop can be
exercised against K3s and verified end-to-end without on-disk
inspection.

## Prerequisites

Same as `NYCN_K3S_PROOF_PATH.md` Path A:

- Built `icnd` and `icnctl` (`cargo build -p icnd -p icnctl` from
  `<icn-repo-root>/icn`).
- Smoke icnctl identity at `/tmp/icnctl-bootstrap-test`
  (`ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' icnctl ... id
  init`).
- Local gateway via `<icn-repo-root>/start-gateway-test.sh`
  (binds `127.0.0.1:8085`, ephemeral data dir
  `/tmp/icn-bootstrap-test`).
- NYCN smoke fixture at
  `<nycn-repo-root>/institution/smoke/icnctl/`.

## Verified loop

All commands run from `<icn-repo-root>/icn` unless noted. Bearer
tokens are abbreviated `${TOKEN}` in the transcript; the actual
token is fetched via `icnctl auth token` once per shell session.

### 1. Apply the smoke fixture

```sh
ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' \
  target/debug/icnctl \
  --data-dir /tmp/icnctl-bootstrap-test \
  institution bootstrap apply \
  --package <nycn-repo-root>/institution/smoke/icnctl \
  --gateway http://127.0.0.1:8085 \
  --coop-id smoke-test-coop
```

Verbatim:

```
Completed:       3
Deferred:        0
Unsupported:     1
Failed:          no

Completed steps:
  - validate charter ...
  - create Federation entity nycn-icnctl-smoke-federation: created
    entity as entity:icn:federation:nycn-icnctl-smoke-federation;
    governance domain alias nycn-icnctl-smoke-federation-gov
    recorded for subsequent provisioning
  - provision governance domain nycn-icnctl-smoke-federation-gov:
    provisioned governance domain as
    nycn-icnctl-smoke-federation-gov

Unsupported steps:
  - store charter: ...

Alias bindings:
  - nycn-icnctl-smoke-federation ->
    entity:icn:federation:nycn-icnctl-smoke-federation
```

### 2. Get a JWT and read standing

The bootstrap subject DID is now a `static_list` member of the
governance domain seeded by the fixture:

```sh
TOKEN=$(ICN_KEYSTORE_PASSPHRASE='smoke-test-passphrase' \
  target/debug/icnctl \
  --data-dir /tmp/icnctl-bootstrap-test \
  auth token \
  --gateway http://127.0.0.1:8085 \
  --coop-id smoke-test-coop \
  --scopes "governance:read,governance:write,entity:read" \
  | grep -E "^eyJ" | head -1)

curl -sS http://127.0.0.1:8085/v1/gov/me/standing \
  -H "Authorization: Bearer $TOKEN"
```

Verbatim:

```json
{
  "did": "did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk",
  "domains": [
    {
      "domain_id": "nycn-icnctl-smoke-federation-gov",
      "domain_name": "NYCN icnctl Smoke Federation Governance",
      "membership_source": "static_list",
      "status": "member"
    }
  ],
  "roles": [],
  "authority_scopes": [],
  "generated_at": 1777417387
}
```

### 3. Action-card queue is initially empty

```sh
curl -sS http://127.0.0.1:8085/v1/gov/me/action-cards \
  -H "Authorization: Bearer $TOKEN"
```

Verbatim:

```json
{
  "did": "did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk",
  "cards": [],
  "generated_at": 1777417387
}
```

### 4. Create one action item assigned to the bootstrap subject

```sh
DID="did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk"

curl -sS -X POST \
  http://127.0.0.1:8085/v1/gov/domains/nycn-icnctl-smoke-federation-gov/action-items \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d "{\"title\":\"Smoke fixture proof item\",\
      \"description\":\"Repo-safe placeholder action item to drive the proof loop.\",\
      \"assignee\":\"$DID\",\
      \"priority\":\"medium\"}"
```

Note: `priority` accepts `low | medium | high | critical`. `normal`
is rejected. The response carries the new `id`:

```json
{
  "id": "005782cf-d8ad-4d64-b848-ec12d6a6afa1",
  "domain_id": "nycn-icnctl-smoke-federation-gov",
  "title": "Smoke fixture proof item",
  "description": "Repo-safe placeholder action item to drive the proof loop.",
  "assignee": "did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk",
  "status": "pending",
  "priority": "medium",
  "created_by": "did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk",
  "created_at": 1777417409,
  "updated_at": 1777417409,
  "tags": [],
  "notes": [],
  "is_overdue": false
}
```

The handler enforces `governance:write` scope and domain
membership; both are satisfied because the bootstrap apply seeded
the caller as a `$bootstrap_subject_did` member of the smoke
domain (see step 2 standing).

### 5. Action card derives from the new item

```sh
curl -sS http://127.0.0.1:8085/v1/gov/me/action-cards \
  -H "Authorization: Bearer $TOKEN"
```

Verbatim:

```json
{
  "did": "did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk",
  "cards": [
    {
      "id": "card-action_item-005782cf-d8ad-4d64-b848-ec12d6a6afa1-complete",
      "source_kind": "action_item",
      "action_kind": "complete",
      "scope": "individual",
      "title": "Complete: Smoke fixture proof item",
      "summary": "Repo-safe placeholder action item to drive the proof loop.",
      "authority_basis": "assigned_action_item",
      "required_authority_scope": [
        "complete:005782cf-d8ad-4d64-b848-ec12d6a6afa1"
      ],
      "risk_level": "normal",
      "accessibility_hint": "Action items may be reassigned or accommodated; ask the owning structure.",
      "receipt_expected": true,
      "source_id": "005782cf-d8ad-4d64-b848-ec12d6a6afa1",
      "domain_id": "nycn-icnctl-smoke-federation-gov"
    }
  ],
  "generated_at": 1777417409
}
```

`receipt_expected: true` and `authority_basis:
assigned_action_item` confirm the card derives from the action-item
assignment.

### 6. Complete the action item

The status update path requires `governance:write` and creator-or-
assignee identity:

```sh
ITEM_ID="005782cf-d8ad-4d64-b848-ec12d6a6afa1"

curl -sS -X PUT \
  "http://127.0.0.1:8085/v1/gov/domains/nycn-icnctl-smoke-federation-gov/action-items/$ITEM_ID/status" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"status":"completed"}'
```

Verbatim:

```json
{
  "id": "005782cf-d8ad-4d64-b848-ec12d6a6afa1",
  "domain_id": "nycn-icnctl-smoke-federation-gov",
  "title": "Smoke fixture proof item",
  ...
  "status": "completed",
  ...
  "updated_at": 1777417420
}
```

The handler's manager call is the receipt-emission seam:
`apps/governance/src/manager.rs:4870–4890` — when the transition
crosses into `ActionItemStatus::Completed` from a non-completed
prior state and a receipt store is wired,
`store.put_action_item_completion(&receipt)` is invoked with a
freshly-constructed `ActionItemCompletionReceipt`. The save fails
the entire transition (rather than logging-and-continuing) so the
receipt and state-change are atomic.

### 7. Action card disappears after completion

```sh
curl -sS http://127.0.0.1:8085/v1/gov/me/action-cards \
  -H "Authorization: Bearer $TOKEN"
```

Verbatim:

```json
{
  "did": "did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk",
  "cards": [],
  "generated_at": 1777417420
}
```

The card derivation drops cards whose underlying action items are
no longer pending, as expected.

## Idempotency confirmation

Re-running `icnctl institution bootstrap apply` against the same
gateway / data dir reports the per-operation 409 → `Completed`
coercion implemented in
`bins/icnctl/src/institution_bootstrap.rs`:

```
Completed:       3
Deferred:        0
Unsupported:     1
Failed:          no

Completed steps:
  - validate charter ...
  - create Federation entity nycn-icnctl-smoke-federation:
    entity entity:icn:federation:nycn-icnctl-smoke-federation
    already exists
  - provision governance domain nycn-icnctl-smoke-federation-gov:
    governance domain nycn-icnctl-smoke-federation-gov already
    exists
```

Same completed count, same alias bindings, no failures. This
matches the unit-test assertion in
`nycn_bootstrap_apply_idempotent` and the per-create-op 409 stub
tests in the same file.

## Where the receipt actually lives in this mode

`start-gateway-test.sh` runs `icnd` with `--data-dir
/tmp/icn-bootstrap-test`, but the gateway's primary Sled DB falls
back to in-memory storage in this mode. From the daemon log:

```
icn_gateway::server: Using temporary in-memory storage for gateway
icn_gateway::server: Receipt store initialized
icn_gateway::server: Governance manager running standalone with
  persistent action items
```

So in this mode, action items are persisted (separate
`SledActionItemStore` opened by `apps/governance/src/init.rs:108`
at `<data_dir>/governance_action_items`) but the
`ActionItemCompletionReceipt` lives in the in-memory gateway DB and
is lost when the daemon stops. A daemon configured with a
persistent gateway store puts the receipt under the
`receipt:action_item_completion:rec:` and
`receipt:action_item_completion:by_item:` Sled prefixes (see
`crates/icn-gateway/src/receipt_store.rs:38–46`), retrievable by
the `get_action_item_completion_by_item` /
`list_action_item_completions_by_item` backend methods.

The presence of either the persistent or in-memory store is enough
to satisfy the manager's `put_action_item_completion` invocation;
the runtime contract is the same.

## Remaining gap: HTTP retrieval of `ActionItemCompletionReceipt`

This is the only gate between the runbook and a fully self-checking
HTTP loop:

> the gateway has no HTTP endpoint that exposes
> `get_action_item_completion_by_item` (or its `list_*` sibling).

Confirmed:

```
$ grep -RIn "get_action_item_completion\|list_action_item_completion" \
    apps/governance/src/http
# (empty — handlers do not expose these)
```

`icnctl receipts` covers economic-chain receipts (allocations,
intents, decision hashes — see `bins/icnctl/src/...` and
`crates/icn-gateway/src/api/receipts.rs`). It does not cover
action-item completion receipts.

Until that gap is closed, an operator running this runbook can
prove steps 1–7 from HTTP responses but must rely on either:

- `cargo test -p icn-governance-actor --test
  me_action_item_receipt_chain`
  (in-process; passes 6/6 on `2f732176`), or
- a daemon configured with a persistent gateway DB, stopped after
  the loop, with the on-disk Sled prefix inspected by a small Rust
  helper that opens the DB read-only and lists the
  `receipt:action_item_completion:*` prefix.

Neither is a clean HTTP-only operator workflow.

## Three narrow next steps

Pick one. All keep this runbook valid; (a) is the smallest:

(a) **Expose a single HTTP endpoint** —
`GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`
— that wraps `get_action_item_completion_by_item` from the receipt
backend, requires `governance:read`, and 404s if no receipt exists.
~50 lines + a targeted handler test. No new primitives. Closes the
loop and does not change the bootstrap surface.

(b) **Add `completion_receipt_id` to the `ActionItemResponse`
JSON** when the item's status is `completed`. The receipt id is
already available on the receipt the manager just persisted; surface
it on the existing
`PUT .../status` response and on `GET .../action-items/{id}`. This
makes the receipt id visible without adding a new endpoint, but
still requires (a) to fetch the receipt body itself.

(c) **Add a tiny `icnctl gov action-item completion-receipt --domain
<id> --item <id>`** subcommand wrapping (a). Useful for the
operator runbook but a no-op without (a).

Default recommendation: do (a). It is the smallest unit of work
that makes the proof loop self-checking over HTTP and will allow a
later runbook to verify the receipt against the deployed K3s
gateway without on-disk inspection. (b) and (c) are nice but
strictly follow-ons.

## Cleanup

After the loop:

```sh
# Stop the daemon (SIGINT, the script execs icnd in foreground)
pkill -INT -f "target/debug/icnd --data-dir /tmp/icn-bootstrap-test"

# Wipe ephemeral state
rm -rf /tmp/icn-bootstrap-test
# Identity keystore can stay for re-runs:
# rm -rf /tmp/icnctl-bootstrap-test
```

## References

- [`NYCN_K3S_PROOF_PATH.md`](./NYCN_K3S_PROOF_PATH.md) — package-
  loading half of the loop and K3s read-only verification.
- `bins/icnctl/src/institution_bootstrap.rs` — apply, idempotency
  tests.
- `apps/governance/src/http/configure.rs` — action-item routes.
- `apps/governance/src/http/handlers.rs` — `create_action_item`
  (l. 2550), `update_action_item_status` (l. 2767),
  `get_my_action_cards` (~l. 5078).
- `apps/governance/src/manager.rs` — receipt emission seam
  (l. 4870–4890).
- `crates/icn-gateway/src/receipt_store.rs` — Sled prefixes for
  action-item completion receipts (l. 38–46) and the
  `get/list_action_item_completion_by_item` methods (l. 1622–1660).
- `apps/governance/src/receipt_backend.rs` — backend trait surface
  for the same methods.
- `apps/governance/tests/me_action_item_receipt_chain.rs` — full
  in-process proof loop with a recording receipt backend.
- NYCN repo `institution/smoke/icnctl/README.md` — fixture intent
  and limits.
