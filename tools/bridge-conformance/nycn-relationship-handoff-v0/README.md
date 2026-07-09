# NYCN relationship handoff conformance fixture — nycn-relationship-handoff-v0 (fake)

This is a **fake** conformance fixture. It imports **no real data**, reaches no
network, writes to no ICN node, and models **conformance only**. Nothing was
imported, handed off, or synced. It is not a bridge implementation, not a
connector, not a runtime import, and not pilot-readiness.

It expresses NYCN's **fake relationship-pipeline airlock rehearsal** — the
package-local source rehearsal in the NYCN repo covering public recognition,
commitments/obligations, permissions, declined relationships, and held external
references — in ICN's generic governed-bridge conformance contract, proving the
package-local flow can be represented **without ICN core learning any package
vocabulary**. The source rehearsal's own package-local nouns and receipt names
stay in the NYCN repo; the field-by-field translation is documented in
`docs/spec/governed-bridge-nycn-handoff-map.md`.

## What it exercises

The minimum governed-bridge chain, end to end, on invented data:

```
binding  ->  dry-run  ->  steward review  ->  expected receipts
```

Three fake source records map to twenty-three actions across six custody kinds:

| Generic field_path (`relationship.*`) | privacy class | ICN custody kind | steward verb |
|---|---|---|---|
| `public_listing_name` | public_after_approval | `artifact_registry` | approve (001); block, automatic (002) |
| `public_listing_category` | public_after_approval | `artifact_registry` | approve (001) |
| `public_recognition_preference` | internal_only | `artifact_registry` | approve (001) |
| `public_recognition_permission` | internal_only | `policy_block` | block, automatic (002) |
| `commitment_level_label` | internal_only | `governed_object` (commitment-record) | approve (001); **hold** (002) |
| `fulfillment_request`, `fulfillment_preference` | internal_only | `governed_object` (commitment-record) | approve (001) |
| `contact_role`, `contact_channel` | relationship_restricted | `scoped_vault` (relationship-restricted) | approve (001, 002) |
| `external_invoice_reference`, `external_insurance_reference` | finance_restricted | `scoped_vault` (finance-restricted) | approve (001) |
| `external_status_observed` | external_reference | `external_reference` | approve (001) |
| `follow_up_consent` | follow_up_only | `governed_object` (follow-up-record) | approve (001, 003); block, automatic (002) |
| `closure_reason` | internal_only | `scoped_vault` (relationship-internal) | approve (003) |
| `free_text_note` | discard | `discard` | discard (all) |

## The two governed-object classes

This fixture is the first to use the **generic** `GovernedObjectCreationReceipt`
(landed with the governed-object receipt family): the commitment record is a
governed object whose `object_class: commitment-record` is **institution-declared
opaque data** — ICN core carries the string and never interprets it. The
consent-gated follow-up record keeps `FollowUpObjectCreationReceipt`, the
follow-up class instance. The binding's per-field `required_receipts` pins which
receipt each field expects; a commitment record is an institutional fact record,
never a legal instrument or a claim about external enforceability.

## Deliberately partial coverage and limits

- **`policy_gate` is not exercised** — the flow's one gate (recognition
  permission) is a publication *block* where denied and a consumed approval
  basis where granted; no field was invented to reach the remaining kind.
- **Record-state custody limit**: a field path has one custody target in the
  binding. Record 003 (declined/closed relationship) therefore does **not**
  propose its public name — the binding routes `public_listing_name` to the
  publication registry, and a declined record's name must stay internal. The
  declined record is carried by `closure_reason` into an internal vault scope,
  never a commitment object, and no renamed field was invented to bypass the
  limit (documented in the handoff map as a candidate future contract feature).
- **Held references vs observed status**: the external invoice/insurance
  references are **vault-held references** (reference/status only; the documents
  stay with their external custodian); only `external_status_observed` is a true
  observe-only external reference. Nothing external is ever processed.
- **Derived surfaces**: the source rehearsal's fulfillment/follow-up "card"
  candidates have **no write actions here** — cards are derived read views over
  the commitment and follow-up objects, which are already receipted. The
  deprecated card-write receipt name is discussed only in
  `docs/spec/governed-bridge-receipts.md`; it never appears in this directory.

## Invocation

```
python3 tools/validate-governed-bridge-conformance.py
python3 tools/validate-governed-bridge-conformance.py tools/bridge-conformance/nycn-relationship-handoff-v0
```

Exit code 0 on success, nonzero on any failure.

## Non-claims

- fake data only; nothing was imported, handed off, synced, or written to a node;
- ICN core does not know the source package's vocabulary; all field paths,
  privacy classes, scopes, and `object_class` values here are opaque
  institution-declared strings;
- no runtime implementation; no bridge connector; no real import; no live sync;
- no real people, organizations, contacts, references, or identifiers;
- no production, pilot-readiness, or live-federation claim;
- no claim that NYCN operations are ICN-native today;
- no payment-processing / wallet / token / cryptocurrency / settlement framing
  (external references are observed or held, never processed);
- action cards are derived read surfaces, never write targets;
- a receipt records an institutional fact and grants zero authority.
