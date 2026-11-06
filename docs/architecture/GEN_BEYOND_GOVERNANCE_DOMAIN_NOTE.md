---
Status: proposed
Canonical: no
Last Reviewed: 2026-11-06
---

# Design note — generalizing GEN beyond `GovernanceDomainV1`

**Non-normative, non-mutating.** This note changes no code, tag, encoding or contract and creates no
issue. It records what the rung-4 actor-model firewall
([`GOVERNANCE_HUMAN_ACTOR_MODEL.md`](GOVERNANCE_HUMAN_ACTOR_MODEL.md)) assumes and which questions
the GEN owner (#2602) must settle before human contexts other than an institution can be modelled.
Context kinds named here (`PersonalContextV1`, `HouseholdContextV1`) are **proposals**, not
implemented kinds; GEN-A's existing kind tags are untouched.

## 1. The assumption rung 4 makes, and why it is bounded

GEN-A and GEN-B bind a Subject to a *governance-domain id*; the firewall protects a
`GovernanceDomain`. Everything in rung 4 is therefore **institution-shaped**: a domain, a
membership source, an electorate, a proposal. That is correct for cooperatives and federations and
is the only slice the Technical Alpha needs. It must not be read as "every human relationship is an
institution". Specifically, three things in the current code would be *wrong* if reused as a
universal foundation:

1. **`GovernanceDomainId` as the context key.** A person's private context or a household has no
   charter, registrar or steward; forcing one in puts an institution into human genesis.
2. **`MembershipSource` as the model of "who is in".** Both variants are Principal lists or
   trust-graph thresholds. A bilateral recognition between two people is neither.
3. **Recognition as a domain-issued fact.** GEN-B's recognition is issued *about a Subject by a
   domain*. Mutual recognition between peers has no issuing domain.

## 2. Distinctions the architecture must keep apart

| Keep distinct | Why it matters |
|---|---|
| Context identity vs institutional authority | A context gives a Subject a scoped identity; it grants no standing. Authority needs a legitimate, context-specific policy. |
| Bilateral recognition vs institutional membership | Two people can recognise each other, cooperate and commit without creating an institution; consent stays scoped to each participant. |
| Personal / household contexts vs cooperative governance | A household is not a miniature cooperative; a personal context needs no registrar. |
| Informal agreements vs formally governed decisions | Groups may adopt procedures without becoming chartered. A decision rule is not a charter. |
| Device delegation vs institutional mandate | N1 device grants (what a device may do for a Subject) are not ADR-0014 authority grants (what an office may do in a domain). |
| Shared logical computer vs physical machine ownership | Resource authority, device ownership and logical membership are three relations. |
| Federation participation vs centralised control | A federation must not inherit control over members' people, identities or computing. |

## 3. Open questions for #2602 (GEN)

1. Context kinds: do personal and household contexts become new GEN-A kinds, and what is the id
   grammar for a kind that has no registrar-assigned name? (The kind byte feeds the N1 KDF; adding
   one is permanent.)
2. Is there a recognition shape for *mutual* recognition (household, bilateral) distinct from the
   domain-issued GEN-B fact? If so, who verifies it, and what stops it being read as membership?
3. How is an informal group's decision rule represented without a charter, and what is the weakest
   evidence a relying party may require?
4. Which cross-context unlinkability guarantees hold when one human participates in a household,
   a cooperative and a federation, and what correlation does a shared device create?
5. What replaces `MembershipSource` for Subject-keyed domains (roster, admission decision,
   revocation), and who may issue the admission decision?
6. How do resource-commons contributions (physical device, logical computer, institutional policy)
   compose without any one of them implying the others?

## 4. What this note does not claim

No personal, household, bilateral or federative context genesis exists. No production recognition
issuer exists. Nothing here is a deployment or readiness statement.
