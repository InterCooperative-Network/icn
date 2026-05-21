---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-18
---

> Per `AGENTS.md` L301 and `docs/dev/HANDOFF_TEMPLATE.md` L111: this handoff is for session-context continuity and is intentionally **not** committed. It lives under `docs/dev/` for Matt's Tuesday rehearsal. If you decide to commit it, that is a separate action.

# Session Handoff — 2026-05-18 — NYCN/Summit organizer meeting rehearsal script

A walking script for Tuesday's solo rehearsal of the Thursday 2026-05-21 conversation with the NYCN/Summit organizer. Walk this aloud once, end-to-end. Time §"90-second explanation." Adjust wording, not claims.

## Session Goal

Land Tuesday's rehearsal so Thursday's conversation is conversational, calibrated, and meeting-safe — without overclaim, without persuasion mode, and without the wrong workflow choice.

## Decisive Test

Can Matt walk Candidate A end-to-end aloud in under 12 minutes, with natural ask-pause-listen beats, and not slip into a sales pitch when the organizer pushes back?

---

## 60-second opener

> Two sentences first. Then a question. Then listen.

Say:
> Cooperatives have democratic values, but the digital infrastructure most of them coordinate through wasn't built to hold democratic institutions together. Loomio, Forms, Slack threads, QuickBooks, mailing lists — each piece can be useful, but when the vendor changes or a chair rotates, the institutional spine walks. There's no way to prove to a funder or a partner co-op that a decision actually happened, by whom, under what authority, without trusting our word for it.

Then ask:
> *Where in Summit work have you seen institutional memory actually get lost? Chair rotation, fiscal sponsor change, end-of-year handoff, platform migration — what gets dropped?*

Stop. Wait. Let her answer set the anchor.

---

## 90-second explanation

> Say aloud. Time it. Target under 100 seconds. Trim before you mutilate the meaning. If you run long, the second paragraph is the first to cut.

> Cooperatives have spent decades building democratic values and almost no time building the digital infrastructure those values need. We govern in Loomio, account in QuickBooks, remember in Slack threads, and federate in email. Every piece of the institutional spine — who's a member, who has authority, what was decided, what was promised, what was delivered — sits in someone else's product. When that product changes hands, when a chair rotates, when a fiscal sponsor leaves, the cooperative's memory walks. And we have no way to prove to a funder or a partner co-op that a decision actually happened, by whom, under what authority, without asking them to trust our word.
>
> ICN is an attempt to build the infrastructure layer cooperatives actually need to own: a substrate for standing, authority, decisions, obligations, receipts, evidence, and federation. Not a vote app. Not a payment app. Not a blockchain. A constraint-enforcement engine the cooperative runs, where the rules come from a charter the members ratify, and the records are signed in a way an outside party can verify later without trusting us.
>
> A real substrate exists: daemon, identity, ledger, governance primitives, gateway, and proof-bearing receipt paths. But most of the human-facing surface — the app a member would actually use, the dashboard an organizer would actually open — is still design and fixture rehearsal, not production. NYCN is the partnership track that's helping shape what that surface should look like. I'm not here to ship anything. I'm here to ask whether the spine looks real from inside Summit work, and what the smallest honest rehearsal would be. The cleanest test is the part of Summit work you already know: how a session becomes a public program, a set of obligations, a run-of-show, and something next year's organizers can inherit.

---

## Candidate A walkthrough — prompts only

> Walk this aloud. One beat, one ask, listen-pause, next beat. Skim Candidates B (sponsor) and C (accessibility) only if her earlier answers point that way.

1. **Session idea** — "A program committee member proposes a session." Ask: *How does a session idea reach the committee today? Email, doc, meeting?*
2. **Committee decides to invite** — "Committee votes, decides to invite a speaker." Ask: *What's the smallest receipt that would make this decision durable for next year's chair?*
3. **Outreach + confirmation** — "Speaker accepts; bilateral obligations form." Ask: *What would change if both sides could see the obligations they owed each other in one place?*
4. **Missing-info loop** — "Headshot late, translation missing, A/V unclear." Ask: *Today, where does this list live? What gets dropped?*
5. **Support / interpretation / room / materials** — "Each becomes a separate vendor / committee obligation." Ask: *When an interpretation vendor falls through two days before, who carries the recovery obligation?*
6. **Schedule slot assigned** — "Day 2, slot, room R." Ask: *Does this happen at the committee level or via individual chair calls today?*
7. **Public program update** — "PublicProgram entry drafted and published." Ask: *If you opened the published program today, could you trace each entry back to the obligations that justify it?*
8. **Run-of-show assignment** — "Day-of role assigned to a steward." Ask: *Today, how does this get communicated? Where does it live during the event?*
9. **Day-of confirmation** — "Steward confirms; session runs." Ask: *What would you want recorded automatically, and what would feel intrusive?*
10. **Post-event material collection** — "Recording, slides, speaker feedback, attendee feedback, incident log." Ask: *What of this actually carries into next year's planning, and what gets archived and never read again?*
11. **Feedback review (hand off)** — "Hands off to the feedback-to-obligation loop." Ask: *Where in this loop is the biggest gap — collecting, anonymizing, deciding, or honoring?*

---

## Five questions to ask

> Ask one at a time. Leave silence. Do not stack.

1. Where in Summit work have you seen institutional memory actually get lost?
2. Which Summit workflow would be worth rehearsing under this spine — and which would be a waste of it?
3. What must stay human and process-first? Where is the right anti-software boundary?
4. What would make this trustworthy to organizers — and what would make it untrustworthy?
5. What is the smallest sanitized / tabletop rehearsal that's worth a meeting?

---

## Five claims not to make

> Muscle memory. Read aloud before the call.

1. **Not production-ready.** Not externally audited, not legally / regulatorily certified.
2. **Not a formally committed cooperative pilot.** NYCN is an active partnership track.
3. **Not a live member or steward app.** Specs exist; live UIs do not.
4. **Not a signed appliance / not a live federation.** The Debian image is a dev-image; federation primitives are scaffolding.
5. **Receipts do not prove legitimacy by themselves.** Authority shortcuts must label themselves as shortcuts.

Vocabulary stays **settlement, obligation, allocation, unit, position, receipt, provenance, evidence**. Avoid the forbidden financial/crypto vocabulary listed in the Thursday brief.

---

## After-meeting capture checklist

> Fill this in within 24 hours of the meeting. Sanitized only. No private NYCN data.

- [ ] What the organizer identified as **real**.
- [ ] What she identified as **wrong / overbuilt / unclear**.
- [ ] The **smallest rehearsal** chosen.
- [ ] Who else she said should be in the next conversation.
- [ ] What needs to change in the spine doctrine, the specs, or the NYCN docs.
- [ ] Whether the post-freeze delta log in the packet needs an entry.
- [ ] Whether to schedule a follow-up conversation and on what time-scale.

---

## Topics to keep off the meeting story (community-infra firewall)

The Matrix / n8n / Cloudflare / chat.icn.zone / community-bridge / network-ops work is a **different project track** on a different agent / session. **Do not mention any of it in the Thursday meeting** unless the organizer explicitly asks about community infrastructure.

If she asks:
- Acknowledge the work exists in a separate repo (icn-community-bridge), as **design-only** for a future Discord→Matrix onboarding bridge. Phase 1 doctrine is `BRIDGE_POLICY.md` + `MODERATION_AND_CONSENT.md`.
- Do **not** describe live Matrix as a current ICN community service.
- Do **not** mention n8n automation, chat.icn.zone, Cloudflare, k3s, Forgejo, Proxmox, Vaultwarden, or any homelab/infra.
- The follow-up bridge boundary doctrine ([icn-community-bridge#1](https://github.com/InterCooperative-Network/icn-community-bridge/pull/1)) explicitly says *no implementation exists* — keep that framing.

If a community-infra topic naturally comes up, the safe move is: *"That's a separate workstream; happy to take it offline if you'd like, but it's not the Summit/ICN spine conversation."*

---

## What's Open

- [ ] Matt to decide on merge of icn#1882, icn#1883, nycn#67, icn-learn#3, icn-community-bridge#1.
- [ ] Matt to decide on icn#1881 (governance scope constants, independent).
- [ ] Tuesday rehearsal run (this script).
- [ ] Optional Wednesday visual aid — already produced as `docs/strategy/NYCN_ORGANIZER_MEETING_ONE_PAGE_AID_2026-05-21.md` on the #1882 branch.

## Unsafe Assumptions

- The Candidate A beat structure was written from NYCN docs, not from the organizer's actual workflow. If the organizer narrates a different sequence in the meeting, follow their order, not the packet's.
- The 90-second timing assumes a normal delivery pace. Read aloud at least twice on Tuesday; if it runs past 110s, cut a sentence rather than rush.
- The "five questions" set is optimized for the content/logistics seam. If she opens with a different concern (governance, finance, federation), reorder on the fly.
- The community-infra firewall above is a **rule for Thursday**, not a permanent product position. The community-bridge boundary doctrine ([icn-community-bridge#1](https://github.com/InterCooperative-Network/icn-community-bridge/pull/1)) is the documented surface; treat it as truth on Thursday only if the topic comes up.

## Next Move (Tuesday)

1. Read the packet end-to-end once: [`docs/strategy/NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md`](../strategy/NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md).
2. Say the §"90-second explanation" above aloud, twice. Time it.
3. Walk the Candidate A prompts aloud, beat by beat.
4. Read the five questions and five claims-not-to-make aloud.
5. Confirm freeze: do not touch the packet, the brief, or NYCN-side docs unless something on `main` materially changes a stated fact.
6. Print or share the one-page aid ([`docs/strategy/NYCN_ORGANIZER_MEETING_ONE_PAGE_AID_2026-05-21.md`](../strategy/NYCN_ORGANIZER_MEETING_ONE_PAGE_AID_2026-05-21.md)) if useful as a visual prop.

## Verification Commands

```bash
cd <icn-repo-root>
git fetch origin && git status --short
gh pr list -R InterCooperative-Network/icn --state open
gh pr view 1882 -R InterCooperative-Network/icn --json mergeable,statusCheckRollup
gh pr view 1883 -R InterCooperative-Network/icn --json mergeable,statusCheckRollup
```

## Truth-Plane Notes

- **Declared project truth**: `docs/PHASE_PROGRESS.md` (Phase 2 ⏳), `docs/strategy/ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md` (meeting-safe ICN facts).
- **Implementation truth**: verified from #1882 + #1883 diffs + CI rollup at session end.
- **Execution truth**: branch state, PR list, CI confirmed via `gh`.
- **Narrative truth**: pinned to the Thursday brief's "What we should not claim" section; the packet §9 mirrors it.
- **Known conflicts**: none surfaced this session. Cross-stack 404 link warnings on #1883 / icn-learn#3 / icn-community-bridge#1 are expected merge-order interdependencies, not factual disagreements.
