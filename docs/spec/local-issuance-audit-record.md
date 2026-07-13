---
Status: normative (design decision; implementation not yet landed)
Canonical: no
Last Reviewed: 2026-07-13
---

# Local issuance audit record (`icnctl --local-mint`)

**Resolves the design question in issue #2399.** This spec decides whether
each trusted-local issuance should leave a durable, secret-free record, what
that record contains, and why it does NOT reuse the daemon-side evidence
surfaces at issuance time. It is scoped to the **rehearsal/appliance
boundary** — the only caller class of `--local-mint` today. It is NOT the
production trusted-issuance architecture (#2080) and does not change it.

## Decision

**Yes — each `--local-mint` issuance SHOULD leave a minimal, secret-free,
append-only local record, written by `icnctl` itself, explicitly labeled as
operational provenance (NOT cryptographic evidence).** A new record shape is
justified; the existing daemon-side surfaces cannot represent the event
correctly at issuance time (see "Why not reuse", below) — but the record is
designed so the daemon MAY later bind it into the existing evidence
architecture without a schema change.

## Why not reuse an existing contract at issuance time

The issue's stated preference is to reuse an existing surface. Both
candidates were traced and fail for the same structural reason: **the mint is
an offline signing operation in a separate process.**

- `mint_local_trusted_token` (`icn/bins/icnctl/src/main.rs`) reads
  `ICN_GATEWAY_JWT_SECRET` from the environment and signs in-process. No
  daemon connection exists or is required — that is the point of the
  bootstrap path (#2396: the network self-asserted `/auth/verify` flow is
  fail-closed on routable binds per #2075).
- The **effect-dispatch evidence WAL** (#1990) and the **ADR-0026
  receipt ladder** are daemon-side subsystems reached through the running
  `icnd`. Binding at issuance time would require either a running daemon
  (defeats the bootstrap purpose — the seed mints before and during daemon
  bring-up) or a new local RPC/write path into the daemon's stores from a
  second process (a new write surface into evidence stores — strictly worse
  for the trust boundary than a separate operator log).

Reuse therefore happens at the **schema and ingestion** level, not the
transport level: the record is shaped so a later, optional daemon-side
ingestion (startup scan → evidence WAL entry per record) needs no redesign.
That ingestion is explicitly deferred and NOT required for the rehearsal
boundary (single disposable VM, operator-held secret).

## Record shape (v0)

One JSON object per line, append-only:
`<data-dir>/issuance-log/local-mint.jsonl` (default
`/var/lib/icn/issuance-log/local-mint.jsonl` on the appliance; `0640 icn:icn`,
directory created `0750` on first write).

```json
{
  "record_class": "urn:icn:record:local-issuance:v0",
  "cryptographic_evidence": false,
  "mode": "trusted-local",
  "issued_at": 1789300000,
  "expires_at": 1789303600,
  "subject_did": "did:icn:…",
  "coop_id": "nycn",
  "scopes": ["governance:read", "governance:pending-publish:review"],
  "scope_set_b3": "<64-hex BLAKE3 of the sorted, comma-joined scope list>",
  "issuer_instance": "<64-hex BLAKE3 of the node's public instance identity: the operator DID string>",
  "minted_by": "icnctl <version> (<subcommand: auth-token | institution-bootstrap>)"
}
```

Field rules:

- **MUST NOT** contain the JWT, the signing secret, any value derived from
  the secret, a passphrase, or any private overlay data. `issuer_instance`
  is derived from the node's PUBLIC operator DID only.
- `cryptographic_evidence: false` is a REQUIRED literal. The record is an
  unsigned operator log; the label is what prevents it being mistaken for
  signed evidence (the #2399 MUST-NOT), the same honesty-by-labeling pattern
  as the appliance manifest's `non_production`/`signed` fields.
- The record is **not a bearer credential**: it contains no token material
  and cannot authenticate anything.
- `subject_did` is acceptable at the rehearsal boundary because the only
  subjects are the appliance's own operator identity and fictional rehearsal
  identities. If `--local-mint` ever graduates beyond that boundary, this
  field MUST be re-reviewed (fingerprint-only is the likely production
  shape) — that review belongs to #2080's lane, not here.
- Failures to write the record MUST fail the mint (fail-closed): an
  operator who asked for auditability does not get a silent gap. A node
  with an unwritable data dir is already unfit to seed.

## What this record is NOT

- NOT democratic authorization, governance approval, or a receipt of
  institutional consent — `mode: "trusted-local"` records exactly what it
  is: the node's operator invoked local bootstrap issuance.
- NOT revocation infrastructure: expiry (`expires_at`) is the only
  invalidation, same as the token itself.
- NOT the #2080 production trusted-issuance path, and NOT a weakening of
  #2075 (the fail-closed self-asserted `/auth/verify` posture is untouched).
- NOT wired into gossip, federation, or any network surface.

## Deferred (explicitly out of scope here)

- Daemon-side ingestion of the log into the effect-dispatch evidence WAL
  (optional future binding; schema above is stable enough to ingest).
- A signed variant (would require deciding which key signs an issuance
  record before the keystore is necessarily unlocked — a real design
  question that the rehearsal boundary does not need answered).
- Operator-facing `icnctl issuance list` tooling.

## Acceptance criteria for the implementation PR (follow-up, `type:impl`)

1. Both `--local-mint` call sites (`icnctl auth token`,
   `icnctl institution bootstrap apply`) append a record per mint.
2. A unit test asserts the record contains no `eyJ` shape and no secret
   env value even when the token/secret are known to the test.
3. A test asserts mint fails when the log directory is unwritable.
4. `icn-demo-seed`/`icn-demo-verify` behavior unchanged (records are
   additive); the seed-redaction smoke stays green.
5. The appliance runbook's evidence-path section mentions the log.
