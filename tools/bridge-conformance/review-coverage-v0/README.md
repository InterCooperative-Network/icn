# Governed bridge conformance — review-coverage-v0 (fake fixture)

This is a **fake** conformance fixture. It imports **no real data**, reaches no
network, writes to no ICN node, and models **conformance only**. It is not a
bridge implementation, not a connector, not a runtime import, and not
pilot-readiness.

## What it exercises

The minimum governed-bridge chain, end to end, on invented data:

```
binding  ->  dry-run  ->  steward review  ->  expected receipts
```

Four files:

- `binding.example.yaml` — the per-run binding (allowed sources, per-field
  custody map, promotion gates, required receipts, external-reference policy).
- `dry-run.example.yaml` — the proposed actions over a source *shape* (writes
  nothing; every action is decision-required; opaque source-record refs only).
- `steward-review.example.yaml` — one steward decision per proposed action,
  keyed by `(source_record_ref, field_path)`, bound to a verifiable reviewer
  authority reference.
- `expected-receipts.example.yaml` — the receipt set the run must emit.

## What the validator checks

Run `tools/validate-governed-bridge-conformance.py` (from the repo root). It
proves the minimum invariants from the six governed-bridge docs:

- every proposed field is classified and routed to an **allowed custody target**
  (`scoped_vault` / `artifact_registry` / `governed_object` / `external_reference`
  / `policy_gate` / `policy_block` / `discard`);
- coverage is **per `(record, field)`** — never a bare field name, no silent
  gaps, no duplicate coverage;
- the dry-run **writes nothing** and its `plan_hash` matches the reviewed hash;
- a role label is **not** authority proof — a `reviewer_authority_ref` is
  required;
- approved writes require `BridgeImportReceipt` **plus** the target receipt;
- **the binding is the per-run receipt contract** — an approved write must also
  expect every receipt its field's `field_custody_map` entry declares;
- **every binding custody rule must be exercised** — a `field_custody_map`
  field that no dry-run action proposes is an error;
- a verified-transfer `ArtifactReceipt` does **not** satisfy
  `ArtifactRegistrationReceipt`;
- **action cards are derived read views, not write targets** — no action-card
  write receipt may appear;
- **external references are observe-only** — no settlement / payment processing;
- a conservative privacy scan (reserved email domains only, no phones, no dollar
  amounts, no raw URLs, no credential/payment-processing keys).

## Derived from

- `docs/architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md`
- `docs/spec/governed-bridge-receipts.md`
- `docs/spec/governed-bridge-toolmanifest-modes.md`
- `docs/spec/governed-bridge-service-binding.md`
- `docs/spec/governed-bridge-external-references.md`
- `docs/spec/governed-bridge-steward-review.md`

## Invocation

```
python3 tools/validate-governed-bridge-conformance.py
python3 tools/validate-governed-bridge-conformance.py tools/bridge-conformance/review-coverage-v0
```

Exit code 0 on success, nonzero on any failure. There is no global fixture
wrapper in this repo; this validator is invoked directly (and can be wired into
CI in a follow-up).

## Non-claims

- no runtime implementation; no bridge connector; no real import; no live sync;
- no production, pilot-readiness, or live-federation claim; no deployed bridge
  behavior;
- no private data; no raw Drive import;
- no payment-processing / wallet / token / cryptocurrency framing (external
  settlement is *observed*, never processed);
- no claim that current NYCN operations are ICN-native.
