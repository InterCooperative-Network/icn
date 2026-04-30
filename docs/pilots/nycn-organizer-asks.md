---
Status: descriptive
Canonical: no
Last Reviewed: 2026-04-29
---

# NYCN Organizer Asks

> **Audience.** ICN-side people preparing for a conversation with NYCN organizers, and the organizers themselves if they want to see what we are actually trying to learn.
>
> **What this is.** A bounded set of questions to ask NYCN organizers — workflow validation, source/data validation, pilot-readiness — without assuming any commitment.
>
> **What this is not.** A partnership ask. A vendor questionnaire. A list of features to pitch. A way to qualify NYCN as a customer. A request that could only be answered by NYCN saying yes to a pilot.
>
> **Frame.** Every question here is something we want to **learn** from NYCN organizers, not something we want them to **decide**. If a question reads as a sales question, it is wrong and should be rewritten.
>
> **Source of current truth.** [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). Use these as the authoritative reference for any specific claim about ICN's current state.

## Why we are asking these

NYCN is the intended first cooperative partner for ICN's Phase 2 — active partnership track, not a formal pilot. The drive-ingest operator ladder in `InterCooperative-Network/nycn` is a procedural spine for walking organizer material into ICN action-item proofs. Before that spine is exercised against real organizer work, organizers need to validate that the spine matches the work.

Only NYCN organizers can answer most of these questions. ICN-side cannot pre-answer them.

## Workflow validation

The point of these is to find out whether the ladder reflects how NYCN organizers actually do this work today — or whether it accidentally invented a workflow nobody asked for.

1. **Where does drive content actually come from in your week?** Is it organizer-authored, member-submitted, summit-collected, or something else? Does the source of the content change what you do with it next?
2. **What is the *first* human action** taken on drive content in your current workflow — review, triage, summarize, file, share, ignore-until-someone-flags-it, something else? Where does that first action happen — in the drive itself, in Google Groups, in a meeting, in someone's head?
3. **Who has authority to turn drive content into a decision?** Is it a single steward, a committee, a quorum, an asynchronous consent process, an organizer's call followed by post-hoc ratification? Where is that authority recorded today, if anywhere?
4. **What's the difference between a *decision* and an *action item* in NYCN practice?** Is the boundary clean (decision → action item) or fuzzy (some decisions imply work without ever generating an action item; some action items exist without a recorded decision)?
5. **Who, by name or role, is on the hook for an action item once it is created?** How is that assignment communicated, and what makes someone *aware* they have been assigned something?
6. **What does "complete" mean for an action item in your practice?** Is it "the work was done," "someone confirmed the work was done," "the next meeting noted the work was done," or "nobody complained for two weeks"?
7. **What's the failure mode you most want a system to prevent?** Lost decisions, lost responsibilities, lost paper trail, double-counted work, decisions that quietly never happened, something else?

## Data and source validation

These are about the *shape* of what the ladder sees vs the shape of what NYCN organizers actually have. The risk we are trying to surface is silent flattening: where the parser produces a clean structure and the organizer thinks "that's not actually how we work, but it looks plausible."

8. **When the parser produces a `drive-ingest-review/v1` artifact, does it line up with what you would have written by hand from the same source?** Where is the ladder simplifying? Where is it inventing?
9. **Are there organizer practices encoded in your drive content that the ladder cannot see?** (Examples: tacit conventions about file naming, meeting-notes formatting, decision shorthand, who is allowed to edit what.) If so, is each invisible practice load-bearing or incidental?
10. **Is Google Drive the only place this content lives, or does it also live in Google Groups, in meeting minutes, in a Slack/Discord, in a wiki, in someone's head?** When the ladder takes drive content as the source, what is it missing?
11. **When the ladder binds an action item to an assignee** (the `drive-ingest-action-item-publish-dry-run-bound/v1` step), does the assignee match who you would have named yourself? Where would the binding go wrong?
12. **What kind of receipt would actually be useful to you for board reporting, member legibility, or grant reporting?** What fields, what format, what level of detail? What would be useless overkill?

## Pilot-readiness

These are about what would have to be true for an operator pilot rehearsal to even make sense — not about whether NYCN wants one.

13. **What organizer time is currently *not* available, and at what cost?** If a pilot rehearsal needed two hours of organizer attention in a given week, what would it cost NYCN — in displaced work, in volunteer attention, in trust capital?
14. **What infrastructure is NYCN willing to run on its own machines?** Is there an organizer comfortable with a localhost gateway? With a long-running process? With reading a terminal? Or is the bar "everything must be in a browser tab on a phone"?
15. **What infrastructure is NYCN explicitly *not* willing to run, ever?** What would crossing that line cost NYCN's relationship with members?
16. **What would have to be true for a first rehearsal to feel safe?** (Examples: fixture-equivalent material, not real organizer content; no other cooperative aware of the rehearsal; the rehearsal recorded only by NYCN; the rehearsal stopped on first surprise.)
17. **What are the conditions under which NYCN would *not* want to be the intended first cooperative partner?** (We need to be able to hear this without it being awkward. The answer "it would be awkward to say so" is itself useful.)
18. **Who else needs to be in the room before any pilot rehearsal happens?** Stewards, members, board, advisors, legal? Whose absence would later turn into "we should have asked them first"?
19. **What is the smallest thing the ladder could do, on real NYCN material, that would tell you whether the spine is correct?** That is the actual first rehearsal — smaller than a pilot, smaller than a partnership, just one honest test.

## Things we are explicitly not asking

These are the questions ICN-side people should *not* be asking NYCN organizers in this phase. Listed here so the asks above stay clean by contrast.

- **"Will NYCN commit to a pilot?"** No. Commitment is downstream of validation, not a prerequisite for it.
- **"Can ICN announce NYCN as a partner?"** No. NYCN being the intended first cooperative partner is internal language, not a marketing claim.
- **"Will NYCN host an ICN node?"** No. The mutation boundary stays in place: NYCN-side tooling never mutates a remote cluster, and ICN does not propose to host anything for NYCN.
- **"Can ICN have your member list, your governance content, or your historical drive content?"** No. None of that material moves into ICN-side systems as part of validation.
- **"Will NYCN endorse ICN publicly?"** No. ICN is being built whether or not NYCN endorses it.
- **"Will NYCN sign a memorandum?"** No. There is no memorandum.

If any of those questions creep into a real conversation with organizers, the ICN-side person should pause, name the slip, and return to the validation questions above.

## How to use this in conversation

- **Read 1–7 before the conversation; pick three or four.** Asking all of them would feel like a survey. Pick the three or four most relevant to what NYCN actually does, and let the conversation pull others out.
- **Read 8–12 to know what to listen for during the demo.** These are not interview questions; they are awareness primers. If an organizer's reaction during the demo lands on one of these, follow it.
- **Save 13–19 for after the demo.** Pilot-readiness questions only make sense once the organizer has seen what the demo shows and not shows.
- **Take notes on what the organizer says, not on what we plan to say next.** The output of the conversation is "what NYCN organizers actually told us," not "how the conversation went."

## After the conversation

- **Update [`nycn-boundary-brief.md`](nycn-boundary-brief.md) if the conversation revealed a "what ICN cannot honestly claim" item we missed.**
- **Update [`docs/STATE.md`](../STATE.md) only with what is now demonstrably true.** A conversation alone does not change Phase 2 status. Phase 2 status changes when partnership formalizes and a rehearsal happens.
- **Do not write a press post, summary blog, or external announcement about the conversation.** A conversation is not a milestone.

## Pointers

| For | See |
|---|---|
| Honest scope-of-now | [`nycn-boundary-brief.md`](nycn-boundary-brief.md) |
| Demo flow with limits called out | [`nycn-demo-script.md`](nycn-demo-script.md) |
| ICN/NYCN boundary in detail | [`docs/reference/project-index/pilot-and-nycn-map.md`](../reference/project-index/pilot-and-nycn-map.md) |
| ICN doctrine | [`docs/architecture/THE_COMMONS.md`](../architecture/THE_COMMONS.md) |
| Cooperative-developer discovery brief (analog tone) | [`docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md`](../strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md) |
