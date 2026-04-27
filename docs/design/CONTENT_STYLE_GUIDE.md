---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-04-26
Last Updated: 2026-04-26
Purpose: Voice, vocabulary, and copy patterns for ICN member-facing surfaces. Pairs with the design language brief and accessibility baseline.
---

# ICN Content Style Guide

## Voice

- **Plain.** Grade-9 reading level on first pass. Jargon only when it earns its keep, and always linked to a glossary entry.
- **Direct.** Short sentences. Active voice. No corporate-tone hedging ("we strive to ensure...").
- **Calm.** No urgency theater. No exclamation marks for routine state.
- **Honest about uncertainty.** Say "this is proposed", "this has not been verified", "you can challenge this" when those things are true.
- **Receipts-aware.** When something is final, say so. When something is provisional, say so.

## Regulatory-safe vocabulary

ICN-native primitives use this vocabulary. Do not substitute money/wallet/balance/currency vocabulary into ICN-native UI.

| Use | Avoid for ICN primitives | Notes |
|-----|--------------------------|-------|
| obligation | debt, IOU, balance owed | Authorized commitment to deliver |
| allocation | budget line, payment plan | Authorized assignment of capacity |
| settlement | payment, transfer, payout | Closing an obligation per its terms |
| unit | currency, token, coin | Generic accounting unit |
| position | balance, wallet, account | A holder's standing in a unit |
| settlement asset | money, fiat, stablecoin | The asset used to settle externally |
| external settlement instruction | bank transfer, payout request | Instruction to a bridge partner |
| bridge receipt | confirmation of payment | Returned record from external partner |
| mandate | permission, authorization, role | Authorized scope to act |
| receipt | confirmation, transaction | Provenance-bearing record of an act |
| standing | member status, account level | A member's recognized authority and history |

External-system language (bank transfer, ACH, currency) is allowed when describing the external system itself. The boundary is: ICN-native primitives use ICN-native vocabulary; external systems can be named for what they are.

## Concept words to use carefully

- **"verified"** — only when there is code/test/runtime evidence linked. See [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md).
- **"accepted"** — for ADRs and RFCs that have reached accepted status. Not interchangeable with "implemented".
- **"governance"** — refers to authority and process, not to "admin tools".
- **"federation"** — a relation between sovereign coops, never a centralized hub.
- **"institution"** — a governed entity. Not a metaphor.

## Dangerous-action copy

Any action that creates an obligation, casts a vote, dispatches a mandate, settles a position, or rotates a key needs:

1. A short summary of what is about to happen, in plain language.
2. The mandate or authority that backs it (named, linked).
3. A reversibility statement: reversible / appealable / final.
4. The receipt that will be produced, named in advance.
5. A confirm step that requires explicit user input (not a default-yes button).

Example pattern:

> **You are about to:** record the obligation **"Buy 200 print flyers from co-op press"** under mandate **"Summit print budget Q2"**.
>
> **What this does:** creates an `Obligation` record visible to your federation council.
> **Reversibility:** can be challenged through the dispute path within 14 days.
> **Receipt:** an `ArtifactReceipt` will be issued and added to your standing view.
>
> [ Confirm ]   [ Cancel ]

## Formal record vs summary

A surface that shows ICN data in two layers should label them.

- **Summary** — plain-language, member-readable. Not legally formal.
- **Formal record** — the canonical record (receipt, ledger entry, governance proof). Always reachable from the summary.

Never show a summary alone for a contested or appealable action — the formal record link must be present.

## Numbers and units

- Always show the unit. Never a bare number for a quantity.
- Currency-style numbers use locale-appropriate formatting; never strip the unit symbol or code.
- Time is shown in the member's locale; the formal record carries UTC alongside.

## Translations and language justice

- Plain-language English first pass.
- Spanish second pass for any surface used by NYC-based or community-org members.
- The member chooses; never auto-detect from IP.
- Never machine-translate a formal record. Translate the summary; link to the canonical original.

## Errors

- Tell the member what they can do next. "Try again", "contact support", "wait and retry" are all worse than a specific recoverable action.
- Never blame the member.
- Never expose stack traces, internal IDs without context, or unredacted error data on member-facing surfaces.

## What this guide is not

- Not a marketing voice doc.
- Not a vocabulary lock for institution packages — packages bring their own copy and brand voice on top of the ICN substrate.
- Not legal review — legal-binding copy goes through a separate review path.

## See also

- [docs/design-language/brief-v0.md](../design-language/brief-v0.md)
- [docs/design/ACCESSIBILITY_BASELINE.md](ACCESSIBILITY_BASELINE.md)
- [docs/rfcs/RFC-0001-obligation-allocation-settlement-primitives.md](../rfcs/RFC-0001-obligation-allocation-settlement-primitives.md)
- [docs/glossary.md](../glossary.md)
