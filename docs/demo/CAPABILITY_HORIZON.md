---
Status: operational
Canonical: no
Last Reviewed: 2026-07-13
---

# From This Rehearsal to the Horizon: What ICN Does Today, and What It Is For

**Who this is for:** an organizer or member about to see (or having just seen) the
Rehearsal Node demonstration, and the facilitator presenting it.

**Truth discipline:** every forward-looking statement here carries one of four labels.
Nothing unlabeled is a claim.

| Label | Meaning |
|---|---|
| **[Demonstrated]** | You can do this in the current rehearsal, today, on the demo appliance. |
| **[Primitive exists]** | The underlying mechanism is built and tested in the substrate, but no one has assembled a human-facing workflow around it yet. |
| **[Specified]** | Designed and written down as a contract or architecture decision; not yet implemented. |
| **[Horizon]** | The longer-term capability the architecture is intended to make possible. Not built. Not promised on a date. |

The current demonstration is **not the product**. It is the first understandable
process built on a more general foundation — one bounded loop, chosen because an
organizer can judge it in fifteen minutes.

---

## Layer 1 — What you are doing in the rehearsal **[Demonstrated]**

In plain terms, you will:

1. **Review** a piece of proposed work your institution might act on.
2. **See who has the authority** to change it — and notice what you *cannot* do from
   your seat (a member cannot approve; an organizer cannot complete another
   member's work; nobody silently gains a second role).
3. **Revise and assign** it — change the text, pick the responsible person.
4. **Confirm exactly what you saw.** The system shows you a final preview and will
   only accept a confirmation of *that exact version*. If anything changed underneath
   you, the confirmation fails and tells you so.
5. **Complete the work** as the member it was assigned to.
6. **Keep the receipts.** Every important step left a record. You can export the
   evidence and hand it to someone else, who can run an independent check on it —
   and it carries no sign-in secrets and no private personal details (technical
   proof pointers like hashes may appear; the rehearsal export withholds
   identities entirely).

Everything runs on one small local machine. No cloud account, no vendor, no outside
network connection. The fictional institution in the demo owns its process and its
records outright.

You do not need to know how any of it works internally to judge whether the *process*
is one your institution would trust.

---

## Layer 2 — What ICN was doing underneath **[Demonstrated]**

The same walk-through, seen from the machinery's side. Each mechanism below is
general-purpose — the action-item workflow is just the first process wired to it.

| What happened | What ICN actually did |
|---|---|
| You opened your seat without creating an account or pasting a credential | Issued you a short-lived session credential scoped to your role — the browser never held broader authority than your seat required |
| The member couldn't approve; the organizer couldn't complete | Enforced role capabilities on every request, on the server, not in the page — the buttons weren't just hidden, the authority genuinely doesn't exist in the session |
| The preview you confirmed was the version that took effect | Bound your confirmation to a digest (a fingerprint) of the exact previewed content; a stale or tampered version fails closed |
| Steps produced receipts | Recorded each authorized transition — who was authorized, what changed, in what order — as durable receipts that survive restart |
| The evidence exported cleanly | Produced a portable evidence packet with credentials and private personal details withheld by construction (the rehearsal export withholds identities too); a steward-side check validates the packet independently — today that check confirms structure and leak-absence for the rehearsal packet, and for the pending-publish packet type it additionally rejects tampered copies |
| It all worked offline | The institution's process ran entirely on infrastructure it controls — there is no platform landlord in the loop |

This is why ICN is not a task-management app with extra steps: the *app* is
replaceable, but the identity, authority, versions, receipts, and evidence live in a
substrate the institution keeps.

---

## Layer 3 — What else the same foundation can carry

The rehearsal exercised one process. The mechanisms in Layer 2 are not specific to
action items. Here is what the same substrate does or could carry — each example
labeled honestly:

| Institutional process | Status today |
|---|---|
| Review → revise → assign → digest-bound confirm → complete → receipt (this rehearsal) | **[Demonstrated]** |
| Proposals and votes that produce decision receipts | **[Primitive exists]** — the governance engine, vote gates, and decision receipts are built and tested over HTTP; the rehearsal surface doesn't expose them yet |
| Scheduling a meeting and recording attendance and decisions | **[Primitive exists]** — meeting and attendance receipts are runtime capabilities without an assembled interface |
| Admitting a member (invitation, enrollment, standing) | **[Primitive exists]** — invitation and enrollment ceremonies and the member-standing read model exist; the admission *workflow* is not assembled |
| Approving a budget or spending mandate | **[Primitive exists]** — double-entry institutional accounts and budget objects exist in the substrate; no human-facing approval flow yet |
| Allocating a mutual-aid or shared fund | **[Primitive exists]** — mutual-credit and allocation records are core ledger capabilities; the allocation workflow is not assembled |
| Issuing a credential or standing attestation | **[Primitive exists]** — standing is served live today; attestation and privacy-preserving proof primitives exist without an issuance interface |
| Authorizing a shared service between groups | **[Primitive exists]** — an authenticated service registry (announce/discover/withdraw with ownership checks) is built and test-pinned; no institutional authorization workflow around it |
| Delegating a committee mandate, with limits and recall | **[Specified]** — delegation and mandate objects are designed (authority that can be granted without being surrendered); early gate tests exist; not an assembled capability |
| Recording a dispute and its resolution | **[Specified]** — dispute records have early tooling; the resolution *process* is design work |
| Coordinating work between independent cooperatives | **[Specified]** — federation protocol code and cross-ledger settlement primitives exist, but no two real institutions have ever operated them together; treat as unvalidated design until a two-node rehearsal happens |
| Creating a formal agreement between institutions (a federation treaty) | **[Specified]** — treaty and agreement structures are part of the contract-language design |

The honest pattern to notice: **the mechanisms generalize; the interfaces do not exist
yet.** For the **[Primitive exists]** rows, building the workflow is
interface-and-validation work on mechanisms that already run. The **[Specified]** rows
are further out: their mechanisms still need to be implemented and validated before any
interface question arises.

---

## Layer 4 — What a fully developed ICN is for **[Horizon]**

Everything in this section is the capability horizon: the destination the architecture
is aimed at, stated so you can judge whether it is worth aiming at. None of it is
built, scheduled, or promised.

Democratic institutions currently exist digitally *inside other people's platforms*.
Their member lists, decisions, records, and money-adjacent processes live in tools
governed by someone else's incentives, and leaving a platform usually means losing
part of the institution.

A fully developed ICN would be shared institutional infrastructure through which
cooperatives, communities, and federations could:

- **Run their own digital operations** — membership, governance, coordination,
  record-keeping — on infrastructure they operate or choose. **[Horizon]**
- **Carry their rules and records between applications**: the institution is the
  durable thing; interfaces become replaceable. **[Horizon]**
- **Replace a tool without replacing the institution** — the receipts, standing, and
  authority survive the swap. **[Horizon]** *(the separation that makes this possible —
  substrate vs. surface — is the architecture you just used **[Demonstrated]**)*
- **Coordinate across organizational boundaries** — shared projects, procurement,
  mutual aid, training, care work — through agreements between peers rather than
  through a platform that owns them both. **[Horizon]**
- **Share services and infrastructure without creating a central owner**, governing
  the shared layer together. **[Horizon]**
- **Track obligations and contributions** without converting every relationship into
  a speculative market — records of what institutions owe and contribute to each
  other, governed by their own rules. **[Horizon]**
- **Preserve democratic authority as the network grows** — growth adds participants
  to governance rather than concentrating control. **[Horizon]**
- **Accumulate a cooperative digital commons**: infrastructure owned and governed by
  the institutions that use it, built up piece by piece, where components are
  replaced over time but the institutional foundation persists. **[Horizon]**

ICN is not "software for cooperatives." It is an attempt to build the missing
infrastructure through which democratic institutions can exist digitally,
interoperate, and accumulate shared capacity — without being reorganized around the
interests of a platform owner.

What the current rehearsal contributes to that horizon is small and real: it shows
one institution running one governed process, with real authority boundaries, honest
receipts, portable evidence, and no landlord. **[Demonstrated]**

---

## For facilitators: the presentation progression

1. **Do the loop** (Layer 1). Let the organizer drive; assist with navigation only —
   do not perform the judgment steps for them.
2. **Reveal the machinery** (Layer 2) — one pass through the table, in their words.
3. **Open the map** (Layer 3) — pick two or three rows the room actually cares about;
   read the labels out loud. Do not skip the labels.
4. **Name the horizon** (Layer 4) — briefly. The point is informed imagination, not a
   pitch.
5. **Ask which real process they would rehearse next** — and listen for whether their
   answer is on the [Primitive exists] rows (near-term buildable) or further out.

Questions worth asking beyond "did you like it":

- Did the authority boundaries make sense? Did anything feel like it granted or
  blocked the *wrong* authority?
- Did the receipts make the process more trustworthy — or just noisier?
- What felt like unnecessary machinery?
- What organizational process would you actually use this for first?
- What would stop you from using it?
- What would you need to control yourselves for this to be acceptable?
- What information must never leave your custody?
- Which platform dependency would you most want to replace?
- Of the future capabilities, which sounds useful — and which sounds like
  architecture looking for a problem?

The demonstration exists to test ICN's theory of value, not its ability to render a
page. "This is not useful to us" is a valid, recordable outcome.

---

**What this document must never be used to claim:** production readiness, pilot
readiness, organizer approval, accessibility completion (#2041 is an open human
gate), live federation, or that any [Specified]/[Horizon] item exists. When this
document and reality disagree, reality wins — update the document.
