---
Status: research brief
Topic: NLnet / NGI Zero Commons Fund — org, grants, requirements, GenAI policy
Last Reviewed: 2026-05-22
---

# NLnet & NGI Zero Commons Fund — Research Brief

Background research for the ICN grant application. Companion to
[`applications/nlnet-ngi-zero-commons.md`](applications/nlnet-ngi-zero-commons.md) and its
[checklist](applications/nlnet-ngi-zero-commons-checklist.md).

## Read this first — the GenAI finding

NLnet has a **formal, enforced policy on generative AI** (in force since 2025-12-08, v1.1
2026-01-26). It directly affects how ICN's proposal — which so far has been AI-drafted — must
be handled. **Do not paste an AI-written draft into the form without addressing this.** See
[the GenAI section](#nlnet-generative-ai-policy) below; it is the most consequential part of
this brief.

## NLnet — the organisation

NLnet Foundation is a Dutch public-benefit foundation, one of the oldest internet
organisations in Europe. Its roots go back to 1982 and EUnet, one of Europe's first wide-area
networks; the foundation itself was established in 1989 by NLUUG. In 1994 it created a
commercial ISP (NLnet Holding BV) and sold it to UUNET/WorldCom in 1997 — the proceeds became
an endowment that gives NLnet "an absolutely independent position." It is a recognised
public-benefit organisation (ANBI) under Dutch tax law.

NLnet is not itself the funding source for these grants. It runs **NGI Zero** — a coalition it
leads — with money from the **European Commission** (Horizon 2020 / Horizon Europe, via DG
CNECT), plus co-funding from the Swiss State Secretariat for Education, Research and Innovation
(SERI). Across five programmes, NGI Zero has channelled 50M+ euro to hundreds of independent
researchers and open-source developers. NLnet takes no economic benefit from which projects
are funded; reviews are done by salaried staff plus an independent review committee.

**Why this matters for ICN:** NLnet is a serious, mission-driven, long-horizon funder, not a
VC and not a foundation chasing trends. It funds the open internet as a commons. It is also a
repeat funder — the €500k lifetime cap implies a relationship across multiple grants — so the
*reputational* stakes of a first application are real.

## NGI Zero — the programme family

"NGI Zero" is an umbrella. The relevant fund for ICN is the **NGI0 Commons Fund**:

- A bottom-up grant programme that "directly funds the development of concrete digital
  commons." EC grant agreement 101135429; runs January 2024 – June 2027; €21.6M total.
- Siblings include **NGI0 Core** (core internet infrastructure), the theme funds **NGI TALER**
  and **NGI Fediversity**, and earlier programmes (NGI0 PET, Discovery, Entrust).
- Not to be confused with **NGI Commons** (a separate, policy-and-reports effort).

ICN is applying to the **NGI0 Commons Fund, 13th call** — open 2026-04-01, deadline
**2026-06-01, 12:00 CEST**.

## How the grant works (mechanics)

| Aspect | Detail |
|--------|--------|
| Who can apply | Anyone — individual, or formal/informal organisation of any type. No legal entity or university affiliation needed. |
| First-proposal amount | €5,000–€50,000. Lifetime cap €500,000 per applicant across the programme. |
| Payment | **Milestone-based, never up front.** You split the project into milestones, allocate a euro amount to each, and request payment as each milestone completes. |
| Grant form | A donation from NLnet (a public-benefit org). Tax treatment varies by country. |
| Timeline to decision | **3–5 months from the call deadline** — so a 2026-06-01 submission means a decision roughly Sept–Oct 2026. |
| Default project duration | 12 months (exceptions possible). Must fit within the programme's June 2027 end. |
| Overhead (F&A) costs | Generally **not** eligible; if allowed, capped at 25%. |
| Anonymity | A pseudonym is fine pre-grant; real identity needed after selection for compliance, not made public. |
| Support beyond money | Free security audits, accessibility audits, licensing advice, mentoring, packaging help. A genuine value-add. |
| Application format | Concise (~2 pages equivalent for the main proposal); English; submitted at nlnet.nl/propose. No upfront eligibility pre-check — just submit; resubmission to fix errors is routine. |

## Requirements and knock-out criteria

Two-stage review. **Stage 1** checks hard "knock-out" eligibility, then scores passing
proposals on three weighted criteria — technical excellence/feasibility (30%),
relevance/impact/strategic potential (40%), cost-effectiveness (30%) — needing a weighted
score above 5.0/7. **Stage 2** adds clarifying questions, independent fact-checking, and an
independent review committee.

Hard requirements:

- **Open source in its entirety.** All software/hardware must be released under a recognised
  free/libre/open-source licence; scientific outcomes open access. Non-negotiable. (Additional
  non-open licences *alongside* the FLOSS one are allowed.)
- **R&D as the primary objective.**
- **European dimension — a knock-out criterion.** The NGI grant is EU-tax-funded. The simplest
  way to satisfy it is EU-based people on the project. **For a non-EU project like ICN, NLnet
  explicitly allows an alternative: "A significant contribution towards the vision of the Next
  Generation Internet initiative also qualifies."** ICN's route is therefore to make the
  NGI-vision contribution (open, trustworthy, user-controlled, data-sovereign internet
  infrastructure) unmistakable. This is why the draft has a dedicated European-dimension
  paragraph — keep it strong; it is pass/fail.
- **Accessibility.** WCAG compliance is treated as "the new normal"; NLnet offers accessibility
  audits and expects software artefacts to take a11y seriously.

## NLnet generative-AI policy

NLnet has a formal **Policy on the use of Generative Artificial Intelligence** (in force
2025-12-08; v1.1 from 2026-01-26), plus a blunt FAQ entry. It governs **both** proposal-writing
and funded project work. Non-compliance "may result in rejection of the proposal or ultimately
in the termination of the running grant."

### In the application process

The FAQ is direct: *"Can I use generative AI to write parts of my proposal — the short answer
is: no. ... Please grant us the courtesy of writing the proposal yourself."* The formal policy
softens this to: applicants *may* use GenAI (drafting, translation, summarisation) **but any
such use must be disclosed**, and NLnet "encourage[s] applicants to trust their own skills and
write their own proposals."

Disclosure is not a one-line note. If GenAI is used, the applicant must maintain and submit a
**prompt provenance log** listing: the model used, dates/times of prompts, the prompts
themselves, and the unedited output. The FAQ adds that undisclosed GenAI use is "likely to
result in the proposal being rejected, and tarnishing your reputation."

### In funded project work

If ICN were funded, the policy continues to apply to the work:

- All submitted work must be legally publishable under a FLOSS licence — GenAI outputs must be
  checked so they don't reproduce copyrighted/incompatible material (check code-assistant
  terms).
- **Purely AI-generated outputs are not eligible for payment** (and under EU law are not
  copyrightable — they fall into the public domain).
- AI-generated content must not be presented as human-authored. The human grantee remains
  accountable for correctness and must be able to explain design and code decisions.
- **Substantive GenAI use must be publicly disclosed and logged**: a README description of how
  GenAI is used in the project, and per-commit provenance for AI-assisted code (model +
  version + prompts in the commit message). For non-code uses (tests, docs) a README
  description suffices.

NLnet is explicit that it is **not anti-AI** ("we are not against GenAI") and deterministic
code generation, ordinary machine learning, and fuzzing are out of scope of this policy — it
targets GenAI/LLMs specifically.

### What this means for ICN — concretely

This is the action-bearing part of the brief.

1. **The proposal must be Matt's own writing.** The current draft in
   `applications/nlnet-ngi-zero-commons.md` was assembled with AI assistance. It is sound as a
   *scaffold* — structure, fact-base, the slice decision, the European-dimension framing — but
   it should **not** be pasted into the form as-is. The strongly recommended path: Matt writes
   the final form answers himself, in his own voice, using the draft as an outline and source
   of verified facts. NLnet wants the applicant's own proposal and evaluates a short document
   carefully; an applicant's own writing reads better than disclosed LLM text.
2. **If any AI assistance is used in the final text, it must be disclosed** with the prompt
   provenance log (model, timestamps, prompts, unedited output) per policy — silently
   submitting AI-written text risks rejection and reputational harm with a repeat funder.
3. **ICN's "AI-augmented development" identity becomes a compliance obligation under a grant.**
   If funded, the funded milestones need: a GenAI-use description in the repo README, per-commit
   provenance marking for AI-assisted code, and FLOSS-cleanliness checks. Worth getting ahead
   of — a short GenAI section in the ICN README is cheap to add now and signals good faith.
4. Drop the loose "AI-augmented development" phrasing from grant materials unless it is paired
   with this disclosure discipline; under NLnet's lens it is a claim that invites scrutiny, not
   a selling point.

## What this changes for the application draft

- **Budget → milestones.** NLnet pays per milestone. The draft's budget table and six-month
  plan should be merged into milestone form: milestone A/B/C/D, each with a deliverable and a
  euro amount. (The current €3k admin line is ~6% — safely under the 25% F&A cap.)
- **European dimension** — confirmed knock-out; the NGI-vision framing in the draft is the
  correct route for a non-EU applicant. Keep it unmistakable.
- **Tone** — concise, technical, frugal, honest. NLnet rewards a precise bounded R&D slice and
  is unimpressed by vision sprawl; the draft's bounded "proof-loop runtime" framing fits.
- **Timeline expectation** — decision ~Sept–Oct 2026; the project's 6-month plan fits inside
  the programme's June 2027 end with room to spare.

## Sources

- [NLnet — NGI Zero Commons Fund](https://nlnet.nl/commonsfund/) and [FAQ](https://nlnet.nl/commonsfund/faq/)
- [NLnet — Guide for Applicants](https://nlnet.nl/commonsfund/guideforapplicants/) and [Eligibility](https://nlnet.nl/commonsfund/eligibility/)
- [NLnet — Policy on the use of Generative AI for NLnet-funded projects](https://nlnet.nl/foundation/policies/generativeAI/)
- [NLnet — About NGI Zero](https://nlnet.nl/NGI0/)
- [NLnet Foundation — background](https://donors.fundsforngos.org/information-technology/nlnet-foundation-supporting-open-secure-and-decentralised-internet-technologies/)
