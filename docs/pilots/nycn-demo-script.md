---
Status: operational
Canonical: no
Last Reviewed: 2026-04-29
---

# NYCN Demo Script

> **Audience.** ICN-side people walking an NYCN organizer (or small group of organizers) through what ICN can demonstrate today. Not a forward-this deck.
>
> **What this is.** A step-by-step organizer-facing demo flow that focuses on action cards, the proof loop, receipts, and provenance — and is honest about the current limits.
>
> **What this is not.** A sales pitch. A technical deep-dive. A claim that any of this is finished. A presentation about ICN-the-organization. A pilot proposal.
>
> **Source of current truth.** [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). If anything in this script implies more than those say, the script is wrong.

## Goals of the demo

This demo has three goals, in priority order:

1. **Build trust by being concrete.** Show a real surface, with real receipts, against a real localhost gateway. No slides, no abstractions, no roadmap-as-product.
2. **Build trust by being honest.** Name the limits while you are showing the strengths. The limits are part of the demo.
3. **Make space for organizer questions.** The demo should leave more time for the organizer to talk than for ICN-side to talk. If that ratio inverts, stop the demo.

If a fourth goal accidentally creeps in — selling, recruiting, committing, signing — pause, name it, drop it, and resume from the last checkpoint.

## Setup before the demo

- Demo runs against a localhost ICN gateway. **No remote cluster.** No NYCN material is ever sent anywhere.
- The NYCN drive-ingest ladder is pre-staged with a fixture (or, with explicit organizer consent, a fragment of real organizer material the organizer brings).
- ICN-side person has read [`nycn-boundary-brief.md`](nycn-boundary-brief.md) within the last week.
- ICN-side person has [`nycn-organizer-asks.md`](nycn-organizer-asks.md) open in another window.
- A clean terminal, a clean browser tab pointed at the localhost gateway, and a small notepad for what the organizer says (not what the ICN-side person plans to say).

## One-paragraph framing to open with

> "What I'm going to show you is the path from a decision being recorded, to a member being asked to do something, to a tamper-evident receipt of that thing being done. That is the loop. Most of what is interesting is the receipts and where they come from. I'll also show what the ladder does to your drive content along the way, and what it does *not* do — the boundaries are deliberate."

That is the whole opening. If you find yourself adding a "and ICN is also..." sentence, stop.

## Step-by-step

Each step has three parts: **show**, **say**, and **don't say**. The "don't say" entries are not stylistic — they are scope discipline.

### Step 1 — The mutation boundary, said up front

**Show:** Nothing yet. This is verbal.

**Say:**
> "Before we run anything, the rule that's load-bearing for this whole demo: nothing I run on this machine can change anything on a remote cluster. Every step is either pure (no network) or talks only to a gateway running on this laptop. If I wanted to publish to a real cluster, I'd need two operator flags I don't have. That's the boundary."

**Don't say:** Anything aspirational about future federation, future cluster, future remote anything. The boundary is the trust mechanism. Don't undercut it.

### Step 2 — Drive content lands as a review artifact

**Show:** Run the parser against the staged fixture (or organizer-provided material). It produces a `drive-ingest-review/v1` artifact — file out, no network.

**Say:**
> "This is what the ladder sees. It's a structured view of what was in the drive content. A human — an organizer — reviews this and writes the decisions YAML by hand. That is on purpose. The ladder does not invent decisions."

**Don't say:** That this replaces organizer judgment. That this is "automation." That the ladder "understands" organizer material. It does not. It surfaces structure and waits for a human.

### Step 3 — Decisions YAML → publish dry-run

**Show:** Run the publish dry-run against the (organizer-authored or fixture) decisions YAML. It emits `drive-ingest-action-item-publish-dry-run/v1` — file out.

**Say:**
> "Before anything is published, the ladder shows the organizer exactly what would be created if they ran it for real. Nothing has happened yet. This is the second human-review boundary. If the dry-run looks wrong, the organizer rewrites the decisions YAML and runs it again."

**Don't say:** "And then the system publishes automatically." It does not. The next two steps are operator-gated.

### Step 4 — Local proof loop on a localhost gateway

**Show:** Start an ICN gateway on localhost. Walk the local proof runner: `GET /me/action-cards` to see the caller's pending action cards → `PUT .../status` to mark one complete → `GET .../completion-receipt` to retrieve the receipt the system just wrote.

**Say:**
> "These are real ICN endpoints. The action card came from a real proposal in a real governance domain on this gateway. When the action card is marked complete, the gateway writes an append-only receipt — keyed by domain and item, signed in a way another node could verify. That receipt is what makes the loop *evidence-bearing* instead of just *claim-bearing*. The retrieval endpoint is the read-side proof."

**Don't say:** "Our distributed ledger." "Tokens." "Blockchain." "Wallet." "Currency." "Account balance." None of those describe what this is. The receipt is evidentiary, not transactional.

### Step 5 — Show the receipt content honestly

**Show:** Open the receipt JSON. Walk the fields: `domain_id`, `item_id`, `record_hash`, completion metadata, signing metadata.

**Say:**
> "The receipt names what was done, in which domain, by which identity. It's the kind of record an organizer could put in a board report and have someone else check it without asking us. That is the whole point. If the receipt looked impressive but couldn't be checked, it would be useless."

**Don't say:** "And the receipt is then propagated through the federation." It is not. Federation propagation is Phase 3. In this demo it is local-only.

### Step 6 — Member standing, briefly

**Show:** `GET /v1/gov/me/standing` against the localhost gateway, as the same identity. Walk the response: identity, memberships, roles, currently selected scope.

**Say:**
> "This is the answer to 'who am I, where do I belong, what can I do' — for the same person who just completed the action item. The action card runtime and the standing read model are aimed at the same person, with the same identity, in the same domain. That alignment is the substrate under any organizer-facing UI we'd ever build."

**Don't say:** "And here's our member portal." There isn't one to show today. The endpoint exists; the polished surface above it does not.

### Step 7 — Stop, and name the limits

**Show:** Nothing. This step is verbal and is the most important one.

**Say:**
> "Three things this demo did not show, on purpose. One: live federation between two cooperatives — that is Phase 3, not built. Two: a finished member app — there's a React Native build in progress, not shippable. Three: a one-command per-cooperative deployment — that's a Phase 2 deliverable that hasn't shipped. I'm telling you this so the demo doesn't oversell. What you saw is real; the rest is honest about not being there yet."

**Don't say:** Roadmap timelines. Promised dates. "By Q3 we will have..." None of that.

### Step 8 — Hand the conversation to the organizer

**Show:** Close the terminal. Open the notepad.

**Say:**
> "That's the demo. The most useful thing now is for me to listen — does any of what you saw map to a real NYCN problem? Does any of it map to a workflow you'd want different? What did I show that doesn't fit, and what didn't I show that you wish I had?"

Then stop talking. Read [`nycn-organizer-asks.md`](nycn-organizer-asks.md) ahead of the meeting; bring the questions in case the conversation needs prompts. Otherwise, listen.

## Demo timing budget

| Section | Soft budget |
|---|---|
| Steps 1–3 (boundary + drive ingest + dry-run) | 8–10 minutes |
| Step 4–5 (proof loop + receipt) | 8–10 minutes |
| Step 6 (member standing) | 3–5 minutes |
| Step 7 (limits) | 3–5 minutes |
| Step 8 (organizer talks, ICN-side listens) | the rest of the time, ideally 50%+ |

If the demo runs long, drop step 6 first, then step 5's receipt-fields walk. Never drop step 1 or step 7.

## Hard rules during the demo

- **No promises.** Not about timelines, not about features, not about commitments.
- **No proposals.** This demo is not a pilot proposal. If the conversation moves toward "so when do we start," redirect to "let's keep it as a second conversation."
- **No NYCN data leaves the room.** Even if an organizer wants to show real material, it stays on their machine or a shared screen. Nothing is captured, copied, or sent anywhere.
- **No vocabulary slips.** Avoid: payment, currency, wallet, balance, blockchain, token, web3. Prefer: settlement, unit, identity, position, obligation, allocation, receipt, provenance.
- **If anything goes sideways**, stop the demo, name what went sideways, do not fake recovery. A demo that errors and is named honestly is more useful than a demo that succeeds and oversells.

## Pointers

| For | See |
|---|---|
| Honest scope-of-now | [`nycn-boundary-brief.md`](nycn-boundary-brief.md) |
| Organizer-facing questions | [`nycn-organizer-asks.md`](nycn-organizer-asks.md) |
| ICN-side runtime surfaces | [`docs/reference/project-index/runtime-surface-map.md`](../reference/project-index/runtime-surface-map.md) |
| Local proof-loop runbook | [`docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md`](../dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md) |
| K3s proof-loop runbook | [`docs/dev/NYCN_K3S_PROOF_PATH.md`](../dev/NYCN_K3S_PROOF_PATH.md) |
| What can / cannot be shown | [`docs/reference/project-index/show-readiness-map.md`](../reference/project-index/show-readiness-map.md) |
