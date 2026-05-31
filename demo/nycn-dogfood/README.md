# NYCN Dogfood Keystone Demo

A repeatable, self-contained demonstration of the one ICN loop that runs **live, from a clean
checkout, on a node the cooperative controls** — turning a piece of organizing work into a
**verifiable receipt**. One command; no separate clone; no live cluster.

## What this is

A small kit that boots a **local** ICN gateway, bootstraps the in-tree NYCN institution package
(`institutions/nycn/`), and walks the proven loop end-to-end:

> NYCN bootstrap → cooperative obligation (action item) → organizer action card →
> mark complete → **completion receipt with `record_hash`** → card clears.

It is the runnable form of a proof that previously lived only as a transcript.

## Core claim

**ICN can turn cooperative work into legible obligations and verifiable receipts today.**

## What this proves

- A cooperative can run its **own** coordination node — local and identity-backed, with a persistent run-local store and no central server.
- An organizer holds a **cryptographic identity** (a DID) they control.
- A piece of real work becomes a **tracked obligation** with a plain-language **action card**.
- Completing it yields a **completion receipt** carrying a `record_hash` — a 32-byte BLAKE3
  fingerprint over the receipt's bound fields, with no platform vendor in the middle.
- The receipt proves **this actor discharged this obligation at this time**.

## What this does NOT prove

This section is load-bearing. Be precise about it when showing the demo.

- **Not** the proposal / vote flow. That path is gated on **commons Member-standing provisioning**
  (enroll → voucher/anchor → join jurisdiction as Candidate → advance to Member) and is a separate,
  future slice — not demonstrated here.
- **Not** production readiness. This is research-grade infrastructure on a local test gateway.
- **Not** live-federation readiness.
- The receipt binds exactly `item_id`, `domain_id`, `actor_did`, `transition`, and `completed_at`.
  It does **not** certify the editable task title/description text — it proves the *completion
  event*, not the content of the words.
- This kit **checks** that the receipt is well-formed (a 32-byte BLAKE3 `record_hash`) and bound to
  *this* item/domain/actor/transition; it does **not** itself re-derive the BLAKE3 hash. The canonical
  re-derivation is `ActionItemCompletionReceipt::compute_record_hash` (domain tag
  `icn:gov:action_item_completion:v1`, `icn/crates/icn-governance/src/proof.rs`).

## Prerequisites

- A built `icnd` and `icnctl` (release). If missing, the script prints the exact build command;
  it will **not** build for you:
  ```
  (cd icn && cargo build --release -p icnd -p icnctl)
  ```
- `curl`, `jq`, `python3` on PATH.
- An operator keystore passphrase exported as `ICN_PASSPHRASE` (any value for a test identity).

## One-command run

From the repository root:

```bash
ICN_PASSPHRASE=demo123 ./demo/nycn-dogfood/run.sh --fresh
```

`--fresh` starts a new run-local gateway with a fresh ephemeral data directory, so repeated runs
do not collide. It only ever stops a gateway **this script** started; if the port is held by an
unknown process it stops and tells you (pass `--force-port-cleanup` to override deliberately).

## Recording a run

```bash
ICN_PASSPHRASE=demo123 ./demo/nycn-dogfood/run.sh --fresh --record
```

This tees the run to a transcript and renders a self-contained HTML replay you can open in any
browser (and screen-capture for slides). Artifacts land under
`demo/nycn-dogfood/runs/<timestamp>/` (git-ignored), or under `DOGFOOD_OUT_DIR` if you set it.
A scrubbed reference run is committed under [`sample/`](sample/).

## Expected output

Nine narrated beats ending on the receipt:

```
== 8. The node issues a verifiable completion receipt ==
{ "item_id": "...", "domain_id": "nycn-federation-gov", "actor_did": "did:icn:...",
  "transition": "completed", "completed_at": ..., "record_hash": [ ... ] }
== 9. The obligation is discharged -- the card clears, the proof remains ==
   card cleared -- obligation discharged, proof retained.
```

The script exits non-zero (with a clear message) if any critical value is missing — empty DID,
empty token, no action item, or a receipt without a `record_hash`.

## Operator checklist (record / rerun / verify / clean)

- [ ] Binaries built (`icnd`, `icnctl`); `curl`/`jq`/`python3` present; `ICN_PASSPHRASE` exported.
- [ ] Run: `ICN_PASSPHRASE=… ./demo/nycn-dogfood/run.sh --fresh --record`.
- [ ] Confirm beats 1–9 print, ending with a `record_hash` and **card cleared**.
- [ ] Rerun the same command — it must succeed again without manual cleanup (idempotent).
- [ ] Open `runs/<timestamp>/recording.html` to review the replay.
- [ ] To share a reference, **scrub** machine-local paths and confirm no token/secret/hostname,
      then copy into `sample/`.
- [ ] Clean: delete `runs/` (git-ignored) when done; the operator keystore (`~/.icn`) is reused by design.

## Troubleshooting

- **`auth token failed (empty token)`** — ensure requested scopes use the full `governance:read` /
  `governance:write` names (the short form is rejected by the gateway), and that the gateway is healthy.
- **`:8085 busy`** — another process holds the port. Use a different `GW_PORT=…`, or pass
  `--force-port-cleanup` if you are sure the holder is disposable.
- **`:7799 busy`** — the run-local **gossip** port is taken. The kit pins gossip to
  `127.0.0.1:7799` (not the default `:7777`), so an identity-backed run never collides with a
  developer's already-running default node. Use a different `GOSSIP_PORT=…`, or `--force-port-cleanup`.
- **`bootstrap apply failed`** — confirm `PKG` points at `institutions/nycn` and the binaries match
  current `main`.
- **gateway never healthy** — read `runs/<timestamp>/gateway.log`.

## Truth label

Rehearsal, not production. All identities are test / operator-provisioned DIDs — no real organizer,
member, sponsor, or attendee data. Runs on a single local `icnd` node, not a cluster, not a public
deployment. The NYCN cooperatives referenced are illustrative; the loop and its receipt are real.
