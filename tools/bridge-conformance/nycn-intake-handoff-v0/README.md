# NYCN intake handoff conformance fixture — nycn-intake-handoff-v0 (fake)

This is a **fake** conformance fixture. It imports **no real data**, reaches no
network, writes to no ICN node, and models **conformance only**. Nothing was
imported, handed off, or synced. It is not a bridge implementation, not a
connector, not a runtime import, and not pilot-readiness.

It expresses NYCN's **fake intake-import airlock rehearsal**
(`docs/bridge-rehearsals/fake-intake-import-airlock/` in the NYCN repo, itself
fake) in ICN's governed-bridge conformance contract, proving the two fake sides
can meet on invented data without any change to
`tools/validate-governed-bridge-conformance.py`.

## What it exercises

The minimum governed-bridge chain, end to end, on invented data:

```
binding  ->  dry-run  ->  steward review  ->  expected receipts
```

Four fixture files plus this README:

- `binding.example.yaml` — the per-run binding: allowed source system, the
  per-field custody map derived from the NYCN intake fields, promotion gates,
  required receipts, observe-only external-reference policy.
- `dry-run.example.yaml` — the proposed actions over the intake source *shape*
  (writes nothing; every action is decision-required; opaque source-record refs
  only; no field values).
- `steward-review.example.yaml` — one steward decision per proposed action,
  keyed by `(source_record_ref, field_path)`, bound to a verifiable reviewer
  authority reference.
- `expected-receipts.example.yaml` — the receipt set the run must emit.

## NYCN intake → ICN custody, at a glance

Three fake intake records map to eight actions across five custody kinds:

| NYCN source field (`attendee.*`) | privacy class | ICN custody kind | steward verb |
|---|---|---|---|
| `role_interest` | public | `artifact_registry` | approve |
| `display_name_preference` | participant_visible | `scoped_vault` | approve |
| `interpretation_language_need` | care_sensitive | `scoped_vault` (care-restricted) | approve |
| `accessibility_need` | care_sensitive | `scoped_vault` (care-restricted) | approve |
| `follow_up_consent` (consent present) | follow_up_only | `governed_object` | approve |
| `follow_up_consent` (consent absent) | follow_up_only | `governed_object` | block (automatic) |
| `registration_reference` | external_reference | `external_reference` | approve |
| `free_text_note` | discard | `discard` | discard |

The full field-by-field derivation, vocabulary mismatches, and the doctrinal
notes live in `docs/spec/governed-bridge-nycn-handoff-map.md`.

## Custody-kind coverage is deliberately partial

This fixture proves **handoff fidelity**, not taxonomy coverage. It exercises
only the custody kinds the NYCN intake flow naturally produces —
`artifact_registry`, `scoped_vault`, `governed_object`, `external_reference`,
and `discard`. It does **not** exercise the `policy_gate` / `policy_block`
custody kinds: the intake's one gate (follow-up consent) is expressed as a
steward **block** on the underlying follow-up governed object, not as a distinct
policy-block custody target. Full custody-kind coverage is already proven by the
`review-coverage-v0` fixture; a sponsor publication-permission gate (a natural
`policy_block`) is left to a later sponsor slice. No fields were invented to
reach a coverage number.

## Follow-ups are governed objects, not card writes

NYCN's fake planning materials name a follow-up as an action-card creation. ICN
routes a consented follow-up to an underlying `governed_object`
(`FollowUpObjectCreationReceipt`); an action card is a **derived read view** of
that object, never a write target, and no card-write receipt appears anywhere in
this fixture. The deprecated card-write receipt name is discussed only in
`docs/spec/governed-bridge-nycn-handoff-map.md` (see
`docs/spec/governed-bridge-receipts.md` for why that name is deprecated); it is
never used here.

## Invocation

```
python3 tools/validate-governed-bridge-conformance.py
python3 tools/validate-governed-bridge-conformance.py tools/bridge-conformance/nycn-intake-handoff-v0
```

Exit code 0 on success, nonzero on any failure.

## Non-claims

- fake data only; nothing was imported, handed off, synced, or written to a node;
- no runtime implementation; no bridge connector; no real import; no live sync;
- no raw Drive / Sheets / SimpleTix rows; no private operational data;
- no production, pilot-readiness, or live-federation claim;
- no claim that NYCN operations are ICN-native today;
- no payment-processing / wallet / token / cryptocurrency / settlement framing;
- external references are observe-only (observed, never processed);
- action cards are derived read surfaces, never write targets;
- a receipt records an institutional fact and grants zero authority.
