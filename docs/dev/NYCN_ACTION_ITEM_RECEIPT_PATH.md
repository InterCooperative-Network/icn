Status: discovery runbook
Authority: dev runbook
Audience: ICN developers + NYCN package operators
Cluster: local gateway (`start-gateway-test.sh`); K3s mutation deferred to explicit operator decision

# NYCN action-item receipt proof path

This runbook closes the gap left open by
[`NYCN_K3S_PROOF_PATH.md`](./NYCN_K3S_PROOF_PATH.md): driving one
action item from creation to completion against a real ICN gateway,
with the smoke fixture's governance domain as the host, and the
HTTP retrieval of the resulting `ActionItemCompletionReceipt`. The
retrieval endpoint
(`GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`)
landed in [ICN #1675](https://github.com/InterCooperative-Network/icn/pull/1675)
(`91a63eec` on `main`). The runbook is therefore self-checking
end-to-end over HTTP, with no on-disk Sled inspection.

It is a **discovery runbook**: every transcript below is verbatim
output from the steps as executed (transcripts captured against
`icnd 2f732176` for steps 1–7; transcripts for the new step 8
captured against `icnd 91a63eec`). It does not document what the
system is supposed to do; it documents what the system did.

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

As originally written this runbook left the receipt-retrieval
half open and recommended a single GET endpoint. That endpoint
shipped in [ICN #1675](https://github.com/InterCooperative-Network/icn/pull/1675)
(merged into `main` at `91a63eec`) and is now part of this runbook
as Step 8 below. The local HTTP proof loop is therefore complete
end-to-end without on-disk inspection.

K3s mutation remains an explicit operator decision. This runbook
does not target K3s end-to-end; once an operator chooses to
exercise the loop on K3s, the same Step 8 GET applies unchanged
against `10.8.30.40:30080`.

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

In the standalone gateway/HTTP path shown by that log, the
governance manager is backed by the **same gateway Sled DB**
(`crates/icn-gateway/src/server.rs:773` —
`GovernanceManager::new_with_sled(sled_db_arc)` over the same
`sled_db_arc` that the gateway just initialized as temporary).
Because that DB is temporary here, both the HTTP-created action
items and the `ActionItemCompletionReceipt` records live only in
the temporary gateway store and are lost when the daemon stops.

The separate governance-actor action-item store opened by
`apps/governance/src/init.rs:108` at
`<data_dir>/governance_action_items` applies only to the
**actor-backed** governance path (`Governance manager connected to
daemon with persistent action items` in the log), not to this
standalone run.

If the gateway is started with a persistent gateway store instead
of the temporary one, the same standalone HTTP path keeps both
action items and receipts in that persistent gateway DB. In that
configuration, receipts are stored under the
`receipt:action_item_completion:rec:` and
`receipt:action_item_completion:by_item:` Sled prefixes (see
`crates/icn-gateway/src/receipt_store.rs:38–46`), retrievable by
the `get_action_item_completion_by_item` /
`list_action_item_completions_by_item` backend methods.

The presence of either the persistent or temporary gateway store
is enough to satisfy the manager's `put_action_item_completion`
invocation; the runtime contract is the same.

### 8. Retrieve the completion receipt over HTTP

Closed by [ICN #1675](https://github.com/InterCooperative-Network/icn/pull/1675),
landed on `main` at `91a63eec`. The default recommendation from
the original "Three narrow next steps" section below — option
(a), a single read endpoint — shipped. The operator command is
now part of the runbook proper:

```sh
curl -sS \
  http://127.0.0.1:8085/v1/gov/domains/nycn-icnctl-smoke-federation-gov/action-items/$ITEM_ID/completion-receipt \
  -H "Authorization: Bearer $TOKEN"
```

Verbatim against `icnd 91a63eec` (single line wrapped for
display; `record_hash` is the full 32-byte
`Hash`-as-`Vec<u8>` array — `serde_bytes` is not in play, so the
field serializes as a JSON array of integers):

```json
{
  "item_id": "6ef7a6b8-1752-42cc-8a1a-1d9cc0f27d7f",
  "domain_id": "nycn-icnctl-smoke-federation-gov",
  "actor_did": "did:icn:zFLjfYPgF2BEg7NMFcxsM498Zd4VPUTvKh7K3XQrD93Tk",
  "transition": "completed",
  "completed_at": 1777420018,
  "record_hash": [250,211,103,51,11,119,225,7,91,248,200,27,79,110,213,227,12,194,218,168,5,8,186,84,115,121,120,123,18,216,151,48]
}
```

Authorization: `governance:read` plus the same domain-membership
check the rest of the action-item read surface uses.

Negative paths (each verified live against the same daemon):

- Missing/invalid token → `HTTP 401`.
- Malformed UUID in path → `HTTP 400` (parse error from
  `parse_action_item_id`, before any DB lookup).
- Caller has a valid token but is not a member of the requested
  domain → `HTTP 403` (`Only domain members can perform this
  action ...`). Surfaced by `check_domain_membership` before any
  receipt-store lookup.
- Caller asks about a domain that does not exist at all → `HTTP
  404` (`Domain not found: ...`). Same precondition path as 403,
  different branch.
- No receipt persisted for the item, caller is a member → `HTTP 404`.
- Caller is a member, item id is canonical, but the receipt's
  stored `domain_id` does not match the path's `domain_id` →
  `HTTP 404` (does not leak existence across governance domains).
- Non-canonical UUID variants in the path (uppercase hex, URN form
  `urn:uuid:...`) — the handler canonicalizes via
  `parse_action_item_id(&item_id)?.to_string()` before the
  receipt-store lookup, so all variants resolve to the same
  persisted record. Pinned by
  `apps/governance/tests/me_action_item_receipt_chain.rs::completion_receipt_endpoint_canonicalizes_non_canonical_uuids`.

This step closes the **local HTTP proof loop**:

> NYCN smoke fixture → bootstrap apply → standing → action card →
> action_item complete → `ActionItemCompletionReceipt` over HTTP.

K3s mutation remains an explicit operator decision. This runbook
does not prove K3s end-to-end and does not claim Phase 2 is
complete.

## Future follow-ons (not blocking this runbook)

The other two narrow next steps from the original "Three narrow
next steps" section remain optional, post-#1675 follow-ons:

- **Surface `completion_receipt_id` on the `ActionItemResponse`
  JSON** when status is `completed`. Saves one round-trip for
  callers that already know they just completed an item.
- **Add a tiny `icnctl gov action-item completion-receipt
  --domain <id> --item <id>`** subcommand wrapping the new
  endpoint. Useful for operator runbooks but the raw `curl` form
  documented above is sufficient.

Neither is required for the local HTTP proof loop. Pick them up
only if a concrete operator workflow demands them.

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
