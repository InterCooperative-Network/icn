---
Status: descriptive
Canonical: no
Last Reviewed: 2026-04-29
---

# NYCN Boundary Brief

> **Audience.** NYCN organizers and stewards considering whether ICN is worth a closer look as the substrate underneath their drive-ingest operator ladder. Also any ICN-side person preparing for that conversation.
>
> **What this is.** An honest scope-of-now brief: what ICN can demonstrate today, what it cannot honestly claim yet, what NYCN organizers would need to validate before a pilot rehearsal, and what is **not** being asked of NYCN at this stage.
>
> **What this is not.** A pitch. A partnership ask. A request for commitment. A claim of capabilities that have not been built. A request to put NYCN data into a remote system.
>
> **Source of current truth.** [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). If anything in this brief disagrees with those, treat those as authoritative.

## What ICN can demonstrate now

These surfaces exist in the codebase, are exercised in tests, and have been walked end-to-end at least once:

- **A decision-to-action-to-receipt loop.** A governance domain accepts a proposal, the accepted proposal materializes an action item, a member completes the action, and the system writes an append-only `ActionItemCompletionReceipt` that can be retrieved over HTTP at `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`.
- **The same loop for two adjacent paths.** Proposal/vote produces a `GovernanceDecisionReceipt`. Meeting attendance (`Present` or `Remote`) produces a `MeetingAttendanceReceipt`. Both follow the same provenance discipline.
- **A member-facing read model.** `GET /v1/gov/me/standing` returns the caller's identity, memberships, roles, and capabilities. `GET /v1/gov/me/action-cards` returns the things waiting on them.
- **Charter activation against a running gateway.** A charter (CCL YAML) can be ratified through governance and become live constraint that the kernel enforces — without touching the kernel's source or restarting the daemon.
- **The local proof loop runs against a localhost gateway.** Documented in [`docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md`](../dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md). No remote cluster is involved.
- **The same loop has been exercised once on K3s** under operator authorization, against deployed image `91a63eec`. Documented in [`docs/dev/NYCN_K3S_PROOF_PATH.md`](../dev/NYCN_K3S_PROOF_PATH.md).
- **The NYCN drive-ingest operator ladder, in the separate `fahertym/nycn` repo,** walks organizer drive content through parser → review → decisions → publish dry-run → assignee binding → local publisher → local proof runner → federation surface bridge → operator pilot runbook + ladder checker, and adjacent operator-facing tools. Per [`docs/STATE.md`](../STATE.md), NYCN #21–#32 are merged; NYCN #33 was open at last sync. The ladder is pure file-in / file-out or localhost-only operator-gated.
- **A documented mutation boundary.** No NYCN-side tool ever mutates a remote ICN cluster. K3s mutation is only ever performed ICN-side, under operator authorization, and is recorded in the proof-path runbooks above.

## What ICN cannot honestly claim yet

These are in the repo as code or design surfaces, but they are not finished products. Anyone presenting ICN to organizers should not represent them as such:

- **A live, multi-cooperative production network.** ICN runs on a homelab K3s cluster. There is no live multi-cooperative production deployment. No member currently logs in to a hosted cooperative on ICN.
- **A formal NYCN pilot.** NYCN is the *intended* first cooperative partner; the partnership is active and in good faith but not formalized. There is no signed agreement, no committed launch date, no pilot contract.
- **Live federation between two cooperatives.** No two cooperatives currently federate over ICN in production. The federation primitives exist as code; cross-org coordination is Phase 3 in [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md), not Phase 2.
- **A finished mobile app.** The React Native SDK and mobile member-app work exist. Treat the mobile UX spec at [`docs/mobile/icn-mobile-ux-spec-v1.md`](../mobile/icn-mobile-ux-spec-v1.md) as a build-facing spec, not a shipped product.
- **A complete action-card runtime.** Three of the five planned source paths are proof-bearing today. Two (`signal_rule`, `obligation_lifecycle`) are RFC-gated under [#1646](https://github.com/InterCooperative-Network/icn/issues/1646).
- **Defined K3s teardown semantics.** Smoke artifacts persist on the cluster; namespaced teardown is not yet specified ([#1679](https://github.com/InterCooperative-Network/icn/issues/1679)).
- **A one-command, non-technical deployment per cooperative.** That is a Phase 2 deliverable that has not shipped.
- **A polished pilot onboarding guide for non-technical members.** Same — not yet shipped.

## What NYCN organizers would need to validate

If a pilot rehearsal eventually happens, these are the things only NYCN organizers can answer. ICN cannot pre-answer them:

- **Workflow integrity.** Does the path from "drive content shows up" → "review" → "decisions" → "action items in ICN" actually match how NYCN organizers do this work today? Is the parser seeing the right shape of organizer artifacts? Are the review boundaries the right human-review boundaries?
- **Source-of-truth alignment.** When the ladder produces a `drive-ingest-review/v1` artifact, does it line up with what an organizer would have written by hand? Where are the silent disagreements? Are there organizer practices the ladder is silently flattening?
- **Authority alignment.** When an `action_item` action card lands in front of a member, is the right person on the hook? Is the assignee binding capturing real responsibility, or is it inventing it?
- **Receipt usefulness.** Once the proof loop closes for an action item, is the receipt content something an NYCN organizer would actually want — for board reports, for member legibility, for recordkeeping?
- **Stewardship fit.** Is the operator-gated, two-flag-required local publisher acceptable as the *only* path that ever produces remote effects? Or do organizers need a different boundary?

These are questions that can only be answered by NYCN people running the ladder against real (or fixture-equivalent) NYCN material, on their own machines, on their own time, on their own terms.

## What this conversation is **not** asking

These are explicit non-asks. If they come up in conversation, the answer is "we are not asking for that."

- **No partnership commitment.** Not on the first conversation, not on the second, not on the third unless NYCN organizers explicitly initiate it.
- **No data transfer.** No NYCN-controlled material moves into an ICN-hosted system. The mutation boundary is the load-bearing safety property.
- **No infrastructure handover.** ICN does not propose to host or run anything for NYCN. If anything ever runs in production on NYCN's behalf, it runs on hardware NYCN controls.
- **No re-platforming.** The drive-ingest ladder runs against the existing NYCN drive workflow; it does not propose moving NYCN off Google Drive, off Google Groups, off existing operator tooling.
- **No agreement to be a public reference.** NYCN being the "intended first cooperative partner" is internal-language for "we are talking to NYCN organizers in good faith." It is not a marketing relationship.
- **No request to validate ICN as an organization.** ICN is being built whether or not NYCN engages further. The conversation is about whether the substrate is useful for NYCN, not whether NYCN endorses ICN.

## What an honest "next step" looks like

If the conversation goes well, the next step is **another conversation**, not a deal. A reasonable second-conversation outcome looks like:

- An NYCN organizer tries the local preflight runner on their own machine, against their own (or fixture-equivalent) drive content, and tells us what they see.
- Or an organizer reads `START_HERE.md` / `ORGANIZER_QUICKSTART.md` from `fahertym/nycn` and tells us where the language lands and where it doesn't.
- Or an organizer points at one specific NYCN workflow and asks "would the ladder make sense here?" and we work through that one workflow honestly.
- Or an organizer says "this is not a fit for NYCN right now, but here is what would have to be true," which is also useful.

A great second conversation answers one or two specific NYCN workflow questions. It does not produce a partnership announcement.

## Pointers

| For | See |
|---|---|
| Current ICN truth | [`docs/STATE.md`](../STATE.md), [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) |
| ICN runtime surfaces | [`docs/reference/project-index/runtime-surface-map.md`](../reference/project-index/runtime-surface-map.md) |
| ICN/NYCN boundary in detail | [`docs/reference/project-index/pilot-and-nycn-map.md`](../reference/project-index/pilot-and-nycn-map.md) |
| What ICN is for (doctrine) | [`docs/architecture/THE_COMMONS.md`](../architecture/THE_COMMONS.md) |
| ICN-side proof loops | [`docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md`](../dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md), [`docs/dev/NYCN_K3S_PROOF_PATH.md`](../dev/NYCN_K3S_PROOF_PATH.md) |
| Companion demo script | [`nycn-demo-script.md`](nycn-demo-script.md) |
| Companion organizer asks | [`nycn-organizer-asks.md`](nycn-organizer-asks.md) |
