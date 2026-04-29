---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-04-29
---

# Cooperative-developer discovery brief

> **Audience**: Matt and anyone else preparing for a discovery
> conversation with a cooperative developer or
> formation-infrastructure collective.
> **Status**: internal prep doc. Not a sales pitch. Not an external
> brief. Not for forwarding.
> **First named conversation in scope**: McKenzie Jones
> (launch.coop / comp.coop, USFWC peer-advising connection,
> Worker Place / NY Co-op Summit world).

## What this document is for

Discovery + language calibration. Listening, mostly. Calibrating
how ICN lands when described in cooperative language to someone
who already builds cooperative infrastructure for a living.

It is **not**:

- a sales pitch;
- a technical demo;
- an attempt to commit comp.coop (or any other cooperative tech
  collective) as the long-term ICN builder;
- a request for funding or partnership in a first call.

If the conversation goes well, the next step is *another
conversation*, not a deal.

## One-paragraph ICN explanation, in cooperative language

> ICN is public infrastructure for democratic organizations. It
> helps groups define membership, authority, decisions,
> responsibilities, and receipts so people can see what was
> decided, who authorized it, what work came out of it, and what
> actually happened. It is not a platform anyone owns. It runs
> on hardware the cooperative controls. It produces records
> another cooperative can verify without trusting a vendor.

That is the whole pitch. Anything more belongs in the second
conversation.

## Lifecycle distinction (the load-bearing frame)

Where launch.coop sits and where ICN sits are **different
moments in the lifecycle of a cooperative**, not competing
products.

- **launch.coop** helps people *form* worker cooperatives.
  Guided startup — incorporation, bylaws, role definition,
  initial governance, early-stage operations infrastructure.
  This is the part where a group of people becomes an
  institutional body.
- **ICN** helps existing groups and federations *operate and
  coordinate*. Governance after formation: tracking decisions,
  authorities, responsibilities, follow-through, receipts;
  cross-organization coordination; shared resources; federation
  evidence other co-ops can verify. This is the part where
  institutional bodies do work over time.

The handoff to test is whether launch.coop's graduates ever hit
problems that look like "we need governance to scale, not just
to start." If they do, ICN may eventually become a layer they
can coordinate or federate through. If they don't, that is also
a useful finding.

There is no overlap in purpose. There may be eventual handoff.
That is the whole strategic relationship to test.

## Discovery questions

Run these in roughly this order. Listen more than answer.

1. **Where is launch.coop right now** — beta, scaling, funding,
   active users? What does the user volume look like and how is
   the work funded?
2. **Does launch.coop stop at formation, or does it extend into
   operations / governance / federation?** If it extends, how
   far?
3. **What is comp.coop's role in this picture** — contractor,
   co-builder, long-term technical steward, something else?
   Is comp.coop's work fungible (any contractor could do it)
   or is it a distinct cooperative product?
4. **What recurring needs do new co-ops show up with after
   formation?** When the formation work is done, what are the
   first three things they email back about?
5. **Do co-op founders ever ask for** governance tracking, task
   ownership, accountability, shared resources, or
   inter-cooperative coordination? Where are they currently
   solving those problems — Loomio? Slack? memory? off-the-shelf
   SaaS? not solving them?
6. **Does "InterCooperative Network" land clearly as cooperative
   infrastructure**, or does the name trigger crypto / web3 /
   platform / fintech associations? If it triggers the wrong
   thing, what name *would* land?
7. **Would a movement-owned-infrastructure presentation at the
   2026 Co-op Summit be useful** — and what capacity signals
   should we respect when offering one? (Don't offer this in
   the first call unless it comes up naturally.)

A good first conversation produces answers to most of 1–6 and
defers 7. A great first conversation produces a clear "yes,
this would be useful, here is the next person you should talk
to."

## Cautions (read these before the call)

- **Do not commit comp.coop as the ICN builder.** Even if the
  conversation goes well. Even if it feels right. The first
  call is for understanding the relationship, not assigning a
  role. If it comes up, the answer is "let's talk about that
  in a follow-up; I want to be careful here."
- **Do not pitch ICN as crypto, web3, tokens, currency,
  wallets, or fintech.** ICN is infrastructure for democratic
  organizations. It is not a token economy. It is not a
  currency. It is not a wallet. The fact that some pieces are
  cryptographically anchored is a *property* of trustworthy
  records, not a marketing wedge. If McKenzie reads any of
  that vocabulary into the conversation, name the
  misunderstanding directly and reframe.
- **Do not lead with civilization-scale theory.** ICN's
  long-arc thinking is real, but it is not a first-call topic.
  Lead with concrete cooperative coordination problems
  (governance after formation, decisions surviving turnover,
  federation evidence other co-ops can verify) and let the
  arc emerge over time.
- **Do not over-claim the runtime.** Phase 2 is not complete.
  NYCN is the intended first cooperative partner but not a
  formally committed pilot. The action-card / completion-
  receipt loop is verified locally and on K3s but has not run
  on a real institutional decision yet. Be specific about
  what is shipped vs. what is intended.
- **Treat this as listening and language calibration first.**
  The most valuable output of a first call is the language
  McKenzie uses to describe the problems she sees co-ops have
  after formation. Capture that vocabulary; it is the input
  to every subsequent conversation.

## Useful framing lines

A small handful of sentences that have worked for this kind of
audience:

- "Democracy needs infrastructure: records, rules,
  responsibilities, receipts, and repairable processes."
- "ICN is not trying to replace cooperative developers; it is
  trying to give democratic organizations better shared
  machinery."
- "launch.coop helps a co-op form. ICN may become a layer that
  co-op can coordinate or federate through later. Different
  moments in the lifecycle."
- "We are looking for the second-call problems — the things
  co-ops bring back after formation that nobody owns yet."
- "NYCN is an internal reference and pilot package
  demonstrating how organizer material can become structured
  action items, reviewable decisions, assignments, local proof
  loops, federation-surface evidence, and human operator
  procedures. It is a worked example, not a public product
  claim."

Avoid: "platform," "ecosystem play," "stack," "TVL,"
"governance protocol" without immediate translation, anything
that sounds like a Web3 deck.

## What we have ready (for context, not for showing in a first
call)

If asked what concrete artifacts exist, the honest answer is:

- A working ICN gateway with action items, action cards,
  meeting attendance, and completion receipts; a local
  HTTP proof loop documented end-to-end; a K3s smoke proof
  closure recorded against a deployed image.
- A NYCN-side procedural ladder (parser → review → decisions
  → publish dry-run → assignee binding → local publisher →
  local proof runner → federation-surface evidence) that runs
  off committed fixtures, reviewable on disk, with operator
  gates on every mutation step.
- An organizer-facing briefing + simple summit demo + a
  one-command local preflight runner so a steward can walk
  the chain without an agent in the loop.
- A whole-NYCN operating-surfaces model that names what lives
  on Drive, Sheets, Google Groups, the website, etc., with
  per-surface privacy boundaries; a repo-safe
  communication-groups fixture that never commits real
  members or message archives; a steward-facing directory
  tool that validates and renders that fixture.

Show none of this in the first call. Let questions reveal
which piece, if any, is interesting. Most of it will not be
relevant to launch.coop's actual problems, and that is fine.

## What "good" looks like coming out of this conversation

- A clear understanding of where launch.coop sits in the
  lifecycle and what comp.coop's role is.
- A short list of post-formation problems co-ops actually
  bring back to launch.coop.
- A read on whether "InterCooperative Network" lands clearly
  or triggers the wrong association — and if the latter,
  what alternate framing might work.
- An agreement (or non-agreement) about whether a follow-up
  conversation makes sense.
- Notes that another cooperative-developer conversation can
  reuse: the questions don't have to be re-derived next time.

## Where to put what comes back

Whatever McKenzie says about post-formation co-op needs goes
into one of the existing strategy docs (`ICN-Roadmap-Live.md`,
`ICN-Gap-Analysis-March-2026.md`, or
`institutional-ecosystem-arc.md`) — whichever the content fits.
Whatever she says about *language* (what works, what triggers
the wrong association) goes into `ICN-Pitch.md`. Don't update
either until after the conversation.

If a second call happens and the relationship deepens, this
doc gets a successor — not an edit — that names the specific
relationship being built and what each side is committing to.
This doc stays generic so it can be reused for the next
cooperative-developer conversation.
