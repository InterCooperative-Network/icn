# Dogfood Slice — design a NYCN-real proof slice

A dogfood slice is a NYCN-real path that exercises a generic ICN
primitive end-to-end: bootstrap → standing → action card → action →
receipt → audit/export. It is the artifact that promotes generic ICN
substrate from "named" to "proven."

> **A dogfood slice is the cheapest honest test of a generic ICN
> primitive.** It runs against real-shaped (but fictional or redacted)
> NYCN package data and produces real receipts, not slideware.

## When to write one

- An idea names a future ICN object and the project needs to know
  whether the existing substrate already supports it.
- An institution-side need (NYCN) has matured enough to test a
  generic ICN primitive, but the primitive should **not** absorb
  institution-specific meaning to satisfy the test.
- A public claim is being considered and the proof path needs to be
  named explicitly.

## Outline

```markdown
# {slice name} — dogfood slice

**Idea card(s):** ops/ideas/ideas.yaml#idea-NNNN
**Owner / session:** ...
**Date:** YYYY-MM-DD

## What this slice proves

One sentence. The generic ICN primitive being exercised.

Examples:
- "The action-card proof loop works end-to-end for a NYCN
  Content/RFP workflow without any ICN core change."
- "Bridge imports from a Drive folder produce a `BridgeImportReceipt`
  that NYCN typed records can refer to."

## What this slice does NOT prove

Be explicit. A dogfood slice is bounded.

- Not a full Summit run.
- Not a full institution package activation.
- Not a federation-scale test.

## Slice steps

Each step is concrete and named. Each step's authority comes from a
charter / role / standing source. Each step's evidence is a receipt
or a recorded view.

1. Bootstrap: NYCN package files used (paths in the NYCN repo).
2. Standing: which holders the slice runs against (`/me/standing`).
3. Action cards: which `(source, action)` pairs the slice exercises.
4. Authorized actions: what runs against the gateway / governance app.
5. Receipts: which receipt classes are emitted; expected counts.
6. Audit / export: how the slice's receipts are surfaced
   (`icnctl audit`, `/me/receipts`, etc.).

## Boundary check

- Generic ICN substrate is not modified to fit the slice.
- NYCN-specific meaning stays in NYCN package files.
- No private data committed (real partner names → fictional; real
  contacts → private overlay).
- No public website change unless the slice is promoting a
  `public_claim` and an ADR `implementation_status` allows it.

## Validation commands

Exact commands the slice needs to run, in order. Future-state-only
commands are listed but marked `(planned)`.

## Acceptance criteria

- Each receipt class is emitted with the expected `(source, action)`
  pair and resolves through `/me/receipts`.
- All actions trace back to a charter / role / standing source.
- Audit export contains every emitted receipt and excludes private
  vault scopes.

## What promotes after this slice succeeds

A successful dogfood slice can promote:

- `promoted_issue` for any small generic ICN gap surfaced.
- `promoted_package_task` for institution-side follow-up in NYCN.
- `promoted_website_claim` only if the slice's evidence matches the
  claim's maturity band (per ADR-0033).
- `promoted_rfc_candidate` only if the slice surfaced an unresolved
  generic design space — most do not.
```

## Discipline

- Dogfood slices are bounded. A slice that grows past 6 steps should
  decompose into multiple slices.
- A dogfood slice that requires an ICN core change is a **failure
  signal**, not a deliverable. The framing brief should be re-opened.
- A dogfood slice that requires private data is a **boundary
  violation**. Use fictional or redacted values.
- Receipts are the unit of evidence. A slice without receipts proves
  nothing reusable.
