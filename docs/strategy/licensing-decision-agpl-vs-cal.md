---
Status: decision input — not a ratified decision
Topic: ICN daemon/runtime layer license — AGPL-3.0 vs CAL-1.0
Last Reviewed: 2026-05-22
---

# Licensing Decision Input — AGPL-3.0 vs CAL-1.0 for the ICN Daemon Layer

> **What this is.** A decision-input note comparing two network-copyleft licenses for ICN's
> daemon/runtime layer. It is **not legal advice** and **not the ratifying decision** — per
> `LICENSING.md`, a license decision must land as a dedicated PR or RFC reviewed for the
> licensing implication. This note exists to make that decision well-informed. It addresses one
> of the open questions `LICENSING.md` already records: whether runtime/tool layers should
> adopt a network copyleft license (AGPL, CAL, or another) deliberately rather than by default.

## The question

ICN's repository currently carries a split: `LICENSE` (root) is AGPL-3.0, while the Rust
workspace declares `MIT OR Apache-2.0`. `LICENSING.md` records the relationship as unresolved.
This note does not try to resolve the whole split. It addresses one layer: **what license
should govern the network-facing runtime — the `icnd` daemon and the crates that make up the
running node — given that the reusable library crates are intended to stay permissive
(`MIT OR Apache-2.0`)?**

The realistic shape is layered: permissive libraries so others can embed ICN's primitives
freely, and a network-copyleft license on the daemon so a hosted ICN service cannot be turned
into a closed platform. The choice for that copyleft layer is **AGPL-3.0** or the
**Cryptographic Autonomy License (CAL-1.0)**.

## TL;DR recommendation

CAL-1.0 is the rare case of an uncommon license that is unusually well-matched to the project.
Its distinctive "Maintain User Autonomy" obligation is close to a verbatim legal encoding of
ICN's own thesis — institutions own their state, hold their own keys, and cannot be locked in
by an intermediary operator. AGPL-3.0 protects the *source code* but not the *data*; a hosted
ICN operator under AGPL could still hold a cooperative's records and keys hostage. CAL forbids
exactly that.

**Recommendation:** treat CAL-1.0 as the leading candidate for the daemon/runtime layer,
pending a lawyer's review of CAL specifically; keep `MIT OR Apache-2.0` on the reusable library
crates. **Do not couple this to the NLnet deadline** — both AGPL and CAL satisfy NLnet's "open
source license" requirement, so the grant can be submitted before this decision lands.

## What CAL-1.0 is

The Cryptographic Autonomy License, version 1.0, authored by Van Lindberg, was OSI-approved in
February 2020 (SPDX identifier `CAL-1.0`; the license text itself is published under CC-BY-SA
4.0). It is a network copyleft license — like AGPL, it closes the "hosted service" loophole —
but it goes further in two ways that matter for ICN:

**1. It protects user data, not just source code.** CAL's signature provision (§4.2, "Maintain
User Autonomy") says that if you operate CAL-licensed software for others, you cannot use the
license's permissions to interfere with a recipient's ability to run their own independent copy
with their own data. Concretely: you must give recipients a no-charge copy of their data in a
common electronic format (§4.2.1); you may not use cryptographic keys, technical protection
measures, or any other method to limit their access to functionality or control of their data
(§4.2.2); and you may not impose contractual restrictions achieving the same thing (§4.2.3).
CAL's definition of "Source Code" also explicitly includes the cryptographic seeds or keys
needed to use the work. It is the first OSI-approved license to do something substantive about
data.

**2. Its copyleft trigger is harder to game.** AGPL's network trigger fires only for *modified*
versions made available for remote network interaction, and can be sidestepped with proxy or
wrapper architectures. CAL triggers when the work is "distributed, communicated, made
available, or made perceptible" to a non-affiliate third party — a broader basis that catches
unmodified network use too.

CAL also includes a coordinated-security-disclosure embargo (§4.1.3 — up to 90 days to withhold
a security fix's source while a coordinated disclosure runs) and a patent-retaliation
termination clause (§5.3). For embedding CAL code as a library, CAL has a "Combined Work
Exception" (§4.5, separate SPDX id `CAL-1.0-Combined-Work-Exception`) under which the licensor
can mark specific files so they may be combined into a larger work licensed on other terms.

Two honest caveats. CAL's OSI approval was contentious — it was approved over significant
objection, and OSI co-founder Bruce Perens resigned from the organisation around that period.
And CAL is *rare*: its most visible adopter is Holochain, and it has far less interpretive and
case-law history than AGPL.

## AGPL-3.0 baseline

AGPL-3.0 is the Free Software Foundation's network copyleft license. Its §13 requires that if
you modify the program and let users interact with it remotely over a network, you must offer
those users the corresponding source. It is strong, extremely widely used, FSF-stewarded, and
well understood by open-source lawyers and corporate legal teams alike. Its limits, relative to
ICN's mission: it says nothing about user *data* or key control, and its trigger is narrower
and more game-able than CAL's.

## Side-by-side

| Dimension | AGPL-3.0 | CAL-1.0 |
|---|---|---|
| Type | Network copyleft | Network copyleft, broader |
| OSI-approved | Yes (2007) | Yes (2020) |
| Protects source code | Yes | Yes |
| Protects user data / portability | No | Yes — §4.2 |
| Bars key/DRM lock-in of users | No | Yes — §4.2.2 |
| Network trigger | Modified + remote interaction; game-able via proxies | Broader; harder to game |
| Security-disclosure embargo | No | Yes — up to 90 days (§4.1.3) |
| Familiarity / legal track record | Very high | Low — few adopters, little case law |
| Corporate-adoption chill | Moderate (many firms ban AGPL deps) | Higher (unknown to most legal teams) |
| Embedding-as-library escape hatch | (use a permissive layer instead) | "Combined Work Exception" (§4.5) |
| NLnet eligibility | Satisfies "recognised open source license" | Satisfies "recognised open source license" |

## Why CAL fits ICN specifically

ICN's entire pitch is that democratic organisations should own their institutional
infrastructure and not be captured by a platform landlord. The clearest failure mode ICN exists
to prevent is exactly this: an intermediary runs ICN-as-a-service for a set of cooperatives,
then — through pricing, contract terms, or key control — holds those cooperatives' records and
identities hostage.

AGPL does nothing about that scenario. An AGPL-hosted ICN operator must publish source but can
still withhold a cooperative's data and control its keys. CAL §4.2 forbids it directly:
recipients must be able to walk away with their data and the keys to use it. CAL is, in effect,
ICN's anti-lock-in thesis written as a license obligation. For a project whose value
proposition *is* autonomy, that alignment is unusually tight — and it is a credible,
substantive digital-sovereignty signal to a funder like NLnet.

## Cautions

- **Rarity is a real cost.** Contributors, downstream cooperatives' own IT advisors, and any
  reviewing lawyer will likely not know CAL. That is friction for both contribution and
  adoption — arguably more chilling in practice than AGPL's better-known reputation.
- **Less-tested text.** CAL's "User Data" definition is broad and novel and has little
  interpretive history. AGPL is a far more predictable quantity.
- **Operator obligations are real.** Anyone running ICN for others — including a federation
  hosting nodes for member cooperatives — takes on CAL's data-portability duties. For ICN this
  is a feature (it is the point), but it should be a conscious, communicated choice.
- **Not for the library crates.** CAL belongs on the daemon/runtime layer only. The reusable
  crates should stay `MIT OR Apache-2.0` so others can embed ICN's primitives freely; that
  permissive posture is deliberate and CAL would defeat it.
- **This needs a lawyer.** An uncommon, strong-copyleft, data-rights license should not be
  adopted on a maintainer's read alone.

## Recommended layered posture

- **Reusable library crates** (`crates/*` intended for embedding): keep `MIT OR Apache-2.0`.
- **Daemon and runtime** (`bins/icnd` and the crates that constitute the running node):
  network copyleft — **CAL-1.0 recommended over AGPL-3.0**, pending legal review, because it
  protects the specific thing ICN exists to protect.
- **Documentation**: a libre Creative Commons license (CC-BY-SA recommended).
- A CAL daemon depending on permissive library crates is unproblematic — permissive code
  flowing into a copyleft work is always fine.

## Relationship to the NLnet application

These are decoupled. AGPL-3.0 and CAL-1.0 both satisfy NLnet's requirement that software be
published under a recognised open source license. The NLnet draft already states ICN's
licensing factually (AGPL at root, MIT/Apache for libraries) and commits all grant outputs to
open release. **The licensing decision does not need to be resolved before the June 1
submission, and the grant should not wait on it.**

## Next steps

1. Maintainer review of this note; decide whether CAL-1.0 is worth pursuing for the daemon layer.
2. If yes: a lawyer reviews CAL-1.0 against ICN's deployment model — especially federation
   self-hosting and the §4.2 operator obligations.
3. Land the decision as a dedicated licensing PR/RFC per `LICENSING.md` — recording the layered
   posture, updating `LICENSING.md`'s open questions, and adding `license` fields where missing.
4. Independently and sooner: the small cleanup the NLnet checklist already names (the two
   crates with no `license` field; confirming documentation licensing) can proceed now.

---

*Sources: OSI — Cryptographic Autonomy License (full license text); SPDX `CAL-1.0`; published
commentary on CAL's OSI approval and its comparison to AGPL. This note is not legal advice.*
