# ICN Constitutional Roadmap

**Reviewed:** 2026-04-26
**Status:** non-doctrine — this document is a roadmap and candidate map, not an accepted set of decisions.
**Companion artifact:** [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml)

## Purpose and limits

This roadmap names the long arc of ICN architecture across eleven domains. It exists so that future architectural work is not designed out by accident between sessions — so that the next agent or contributor can see what is supposed to exist, what already exists, and what gap they are filling.

**It is explicitly not authoritative.**

- Accepted decisions live in [docs/adr/](../adr/). This file links ADRs but does not bind them.
- Runtime support requires code, tests, and implementation-status evidence — not entries in this document.
- "Proposed" entries are direction, not decisions; they can be modified, refused, or superseded once written as ADRs.
- The eleven domains below match the ADR families in [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml).

This roadmap uses generic "reference institution package" language. ICN core does not encode any specific institution's vocabulary.

## How to read this document

Each section answers four questions:

1. **Why it matters** — what would break, fail, or never get built without architectural attention here.
2. **ADR candidates** — the ADRs (drafted or planned) that record decisions in this domain.
3. **Existing issues** — open work that already addresses pieces of this domain.
4. **Priority horizon** — `now` (this quarter), `soon` (next quarter), `later` (beyond).

## The eleven domains

### 1. Runtime constitutional spine

The mandate-grant-receipt-effect chain that turns a governance decision into an enforceable institutional outcome with provenance.

**Why it matters.** Without a complete spine, a "decision" is a record without consequences and an "effect" is a side-effect without authority. The spine is what makes ICN an institutional substrate rather than a coordination tool.

**ADR candidates.**
- ADR-0014 (constitutional object model — accepted)
- ADR-0019 (authority grant minting seam — accepted)
- ADR-0020 (bootstrap activation + standing read model — accepted)
- ADR-0026 (receipt + provenance proof envelope — drafted)
- ADR-0025 (institutional effect record canonical schema — proposed)
- ADR-0069 (mandate-gated kernel dispatch — candidate, T4)
- ADR-0070 (governance payload execution dispatch — candidate, T4)
- ADR-0071 (proposal type taxonomy and executability — candidate, T4)
- ADR-0073 (canonical event log and replay semantics — candidate, T4)
- ADR-0074 (accepted-but-non-executable proposal semantics — candidate, T4)
- ADR-0075 (institutional atomicity and terminal-state safety — candidate, T4)

**Existing issues.** [#1607](https://github.com/InterCooperative-Network/icn/issues/1607), [#1588](https://github.com/InterCooperative-Network/icn/issues/1588), [#1606](https://github.com/InterCooperative-Network/icn/issues/1606), [#1438](https://github.com/InterCooperative-Network/icn/issues/1438), [#1436](https://github.com/InterCooperative-Network/icn/issues/1436), [#1435](https://github.com/InterCooperative-Network/icn/issues/1435).

**Missing issues.** Effect-record schema; mandate consult-on-dispatch; replay semantics test plan.

**Priority.** `now` for ADR-0026 (back-fill) and ADR-0025 (forward direction). `soon` for kernel dispatch consulting mandates (#1606 is precondition).

### 2. CCL as institutional process language

CCL today encodes thresholds and formulas; the forward direction is CCL as the language for institutional processes — workflows, deadlines, escalations, conflict resolution steps.

**Why it matters.** Institutional life is process. Hard-coding processes into Rust binds them to release cycles; expressing them in CCL gives them the same legibility, governance, and amendment path as any other charter rule.

**ADR candidates.**
- ADR-0021 (CCL determinism, fuel, capability safety — drafted, accepted)
- ADR-0022 (schema bridge and constraint compilation — drafted, accepted)
- ADR-0023 (CCL institutional process language — drafted, proposed)
- ADR-0036 (federation agreement support — candidate, T2)
- ADR-0060 (CCL-defined conflict resolution processes — candidate, T3)
- Future: CCL package hooks; CCL-defined accessibility rules; CCL-defined resource governance; schema versioning and migration.

**Existing issues.** [#863](https://github.com/InterCooperative-Network/icn/issues/863).

**Missing issues.** Process step grammar; deadline semantics; escalation rules; CCL package hook taxonomy.

**Priority.** `now` for back-fills (0021, 0022). `soon` for the institutional-process direction (0023).

### 3. Accessibility as participation floor

Accessibility is not a feature; it is the precondition for participation.

**Why it matters.** A cooperative that excludes members on the basis of bandwidth, language, ability, time, or technology is, by its own logic, not a cooperative. ICN's substrate must make accessibility constitutional — a runtime concern with a written floor.

**ADR candidates.**
- ADR-0028 (accessibility baseline — drafted, proposed)
- ADR-0054 (language/glossary model — candidate, T3)
- ADR-0055 (plain-language and glossary contract — candidate, T3)
- ADR-0056 (assistive technology semantics for action cards — candidate, T3)
- ADR-0057 (caregiving / time / wage barrier accommodation — candidate, T3)
- ADR-0058 (offline / intermittent connectivity participation — candidate, T3)
- ADR-0059 (non-command-line operator and member workflows — candidate, T3)

**Existing issues.** [#1610](https://github.com/InterCooperative-Network/icn/issues/1610), [#1611](https://github.com/InterCooperative-Network/icn/issues/1611), [#1366](https://github.com/InterCooperative-Network/icn/issues/1366).

**Missing issues.** WCAG floor checklist for ICN surfaces; language justice workflow; offline participation patterns.

**Priority.** `now` for the baseline ADR (0028). `soon` for glossary endpoint and plain-language contract.

### 4. Conflict resolution and institutional care

Conflict is the human reality layer. Without an object model and resolution paths, conflict either gets routed to whoever shouts loudest or gets buried.

**Why it matters.** A substrate that has no opinion about conflict cannot host institutions that have to live with disagreement. Conflict resolution is institutional care: not punitive, not silent, but legible.

**ADR candidates.**
- ADR-0029 (conflict resolution object model — drafted, proposed)
- ADR-0042 (federation dispute resolution model — candidate, T2)
- ADR-0060 (CCL-defined conflict resolution processes — candidate, T3)
- ADR-0061 (evidence bundle and testimony model — candidate, T3)
- ADR-0062 (remedies, sanctions, and repair agreements — candidate, T3)
- Future: appeals and finality; cross-institution conflict resolution; privacy and redaction; retaliation protection; conflict-derived institutional learning.

**Existing issues.** None directly open; compute disputes are implemented via [icn-ccl/src/disputes.rs](../../icn/crates/icn-ccl/src/disputes.rs).

**Missing issues.** Appeal lifecycle; mediation role taxonomy; remediation/repair semantics.

**Priority.** `soon` for the object model (0029). `later` for resolution processes and remedies.

### 5. Federation, delegation, and representation

Inter-cooperative coordination has structural decisions that are not the same as intra-cooperative ones.

**Why it matters.** A federation is not a coop; representation is not delegation; recognition is not membership. Conflating these produces accountability gaps and silent failure modes.

**ADR candidates.**
- ADR-0011, 0012, 0013 (canonical truth ownership, federation state origin, clearing adoption — accepted)
- ADR-0035 (federation membership lifecycle — candidate, T2)
- ADR-0036 (federation agreement support — candidate, T2)
- ADR-0037 (cross-institution mandate recognition — candidate, T2)
- ADR-0038 (entity-level voting eligibility — candidate, T2)
- ADR-0039 (RepresentativeVote / EntityAction receipts — candidate, T2)
- ADR-0040 (delegation vs representation semantics — candidate, T2)
- ADR-0042 (federation dispute resolution — candidate, T2)
- ADR-0043 (peer federation discovery and admission — candidate, T2)

**Existing issues.** [#1607](https://github.com/InterCooperative-Network/icn/issues/1607), [#1609](https://github.com/InterCooperative-Network/icn/issues/1609).

**Missing issues.** Federation membership state machine; recognition policy.

**Priority.** `soon` for membership lifecycle (0035) and voting eligibility (0038). `later` for treaties and discovery.

### 6. Economics and resource governance

Cooperative economics is not commerce; obligations and allocations are not transactions.

**Why it matters.** Treating mutual credit as a payment system, or patronage as a balance, collapses the institutional layer back into financial primitives ICN was built to avoid. Vocabulary discipline here is regulatory-safety as well as model-correctness.

**ADR candidates.**
- ADR-0004, 0005, 0007 (obligations not payments, commons credit settlement, commons resource policy — accepted)
- ADR-0031 (commons compute admission and settlement policy — drafted, accepted)
- ADR-0041 (federation clearing provenance and settlement terms — candidate, T2)
- ADR-0044 (obligation / allocation / settlement lifecycle — candidate, T2)
- ADR-0045 (mutual credit position semantics — candidate, T2)
- ADR-0046 (treasury and budget authority — candidate, T2)
- ADR-0047 (patronage and internal capital — candidate, T2)
- ADR-0048 (support-institution resource pools — candidate, T2)

**Existing issues.** [#1634](https://github.com/InterCooperative-Network/icn/issues/1634), [#1635](https://github.com/InterCooperative-Network/icn/issues/1635), [#992](https://github.com/InterCooperative-Network/icn/issues/992).

**Missing issues.** None new — issues track each line.

**Priority.** `now` for ADR-0031 (back-fill). `soon` for obligation/allocation/settlement primitives (0044).

### 7. Compute and commons

Compute in ICN is constrained execution, not autonomous decision.

**Why it matters.** Naming this boundary forecloses the failure mode of compute drifting into authority — the model where a benchmark, an LLM, or a rule-evaluator becomes the de-facto decider. Compute executes what authority asked; authority decides.

**ADR candidates.**
- ADR-0030 (compute workload manifest and authority boundary — drafted, accepted)
- ADR-0031 (commons compute admission and settlement — drafted, accepted)
- ADR-0080 (compute result receipt and verification model — candidate, T4)
- ADR-0081 (WASM module registry and governance approval — candidate, T4)
- ADR-0082 (compute privacy / data locality / selective disclosure — candidate, T4)
- Future: compute dispute resolution detail; assistive compute and meaning preservation; federated compute placement; compute operator standing.

**Existing issues.** [#992](https://github.com/InterCooperative-Network/icn/issues/992), [#959](https://github.com/InterCooperative-Network/icn/issues/959), [#937](https://github.com/InterCooperative-Network/icn/issues/937).

**Missing issues.** Governance-controlled admission policy; module approval workflow.

**Priority.** `now` for back-fills (0030, 0031). `later` for governance integration.

### 8. Member shell, mobile, and action cards

The member-facing nervous system: the part of the substrate that members actually touch.

**Why it matters.** ICN's substrate is real; the surface that members touch is the part most visibly trailing. Without architectural attention to action cards, standing UX, and mobile patterns, the substrate's strength becomes invisible to the people it serves.

**ADR candidates.**
- ADR-0027 (action card contract — drafted, proposed)
- ADR-0049 (member standing read model expansion — candidate, T3)
- ADR-0050 (active scope / session-selected scope — candidate, T3)
- ADR-0051 (mobile wallet / member shell boundary — candidate, T3)
- ADR-0052 (notification and attention model — candidate, T3)
- ADR-0053 (receipt and provenance UX — candidate, T3)

**Existing issues.** [#1608](https://github.com/InterCooperative-Network/icn/issues/1608), [#1646](https://github.com/InterCooperative-Network/icn/issues/1646), [#1605](https://github.com/InterCooperative-Network/icn/issues/1605), [#1537](https://github.com/InterCooperative-Network/icn/issues/1537), [#1366](https://github.com/InterCooperative-Network/icn/issues/1366).

**Missing issues.** None new — issue coverage is good here.

**Priority.** `now` for action card contract (0027). `soon` for standing expansion and active scope.

### 9. Institution package and deployment model

How an institution arrives on a node, lives on it, and leaves it.

**Why it matters.** Without a packaging contract, every institution that adopts ICN ends up in a custom configuration; ICN itself starts to learn institution-specific nouns; the meaning firewall erodes.

**ADR candidates.**
- ADR-0017 (monorepo consolidation — accepted)
- ADR-0020 (bootstrap activation — accepted)
- ADR-0024 (institution package manifest schema — drafted, proposed)
- ADR-0076 (public package vs private overlay boundary — candidate, T4)
- ADR-0077 (package contract pinning and compatibility — candidate, T4)
- ADR-0078 (package migration and upgrade — candidate, T4)
- ADR-0079 (backup, export, institutional continuity — candidate, T4)

**Existing issues.** [#1628](https://github.com/InterCooperative-Network/icn/issues/1628), [#1635](https://github.com/InterCooperative-Network/icn/issues/1635), [#1612](https://github.com/InterCooperative-Network/icn/issues/1612).

**Missing issues.** Package manifest schema for institution-specific extensions; reference institution graduation rule; CCL hooks in packages.

**Priority.** `now` for the manifest schema ADR (0024). `later` for migration and continuity.

### 10. Website and public truth boundary

The site is the public truth boundary. Editorial discipline is an architectural concern.

**Why it matters.** Overclaiming destroys trust faster than under-shipping. The maturity-band convention exists; recording it as an ADR locks it against drift across PRs.

**ADR candidates.**
- ADR-0032 (website truth boundary — drafted, accepted)
- ADR-0033 (public maturity claims and evidence links — drafted, proposed)
- Future: website roadmap data source; public docs / ADR / state synchronization; contributor path and community infrastructure; website accessibility baseline; narrative vs runtime claim boundary; demo system truth model.

**Existing issues.** [#1369](https://github.com/InterCooperative-Network/icn/issues/1369), [#1368](https://github.com/InterCooperative-Network/icn/issues/1368).

**Missing issues.** Maturity-claim evidence linter; demo-system truth model.

**Priority.** `now` for the truth boundary ADR (0032). `soon` for evidence linking.

### 11. Project governance and release discipline

ICN's own governance — currently informal, eventually self-hosted.

**Why it matters.** A substrate for cooperative governance that does not itself dogfood cooperative governance is hypocritical. Forward direction is to use ICN's own primitives to govern ICN, once those primitives are stable enough to bear the weight.

**ADR candidates.**
- ADR-0018 (ADR lifecycle — accepted)
- ADR-0034 (ADR candidate registry as architectural memory — drafted, accepted)
- ADR-0063 (ICN core project governance — candidate, T4)
- ADR-0064 (maintainer and steward authority — candidate, T4)
- ADR-0065 (contributor role model — candidate, T4)
- ADR-0066 (security authority and emergency patch process — candidate, T4)
- ADR-0067 (release authority and cadence — candidate, T4)
- ADR-0068 (transition path toward ICN-hosted governance — candidate, T4)

**Existing issues.** None open at this layer; this domain is the meta layer.

**Missing issues.** Release authority spec; emergency patch protocol.

**Priority.** `later` for all of these — they require the substrate to be stable enough to govern itself.

## Tranche schedule

| Tranche | Theme | This document's count | Trigger to draft |
|---|---|---|---|
| 1 | Foundational ADRs (the spine, CCL, package, member shell, accessibility, conflict, compute, website, registry) | 14 drafted | This PR |
| 2 | Federation depth and economics primitives | 14 candidates | After [#1606](https://github.com/InterCooperative-Network/icn/issues/1606) reverse-index lands; ADRs 0011/0012/0013 are anchor citations |
| 3 | Member shell detail, accessibility detail, conflict detail | 14 candidates | After action-card spec (0027) accepted and [#1608](https://github.com/InterCooperative-Network/icn/issues/1608) in flight |
| 4 | Project governance, release discipline, kernel dispatch detail, package detail, compute governance | 20 candidates | After tranches 2–3 land; policy stability is more legible |

## What this document is not

- Not an accepted set of decisions.
- Not a release schedule.
- Not a binding architectural document.
- Not authority over the ADR review process.
- Not a substitute for [docs/STATE.md](../STATE.md), [docs/PHASE_HISTORY.md](../PHASE_HISTORY.md), or [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md).

## Maintenance

When an ADR row in [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml) flips from `candidate` to `drafted`, update the corresponding section above. When an ADR is accepted, link it. When the substrate evolves enough that a domain becomes a different shape, supersede this section rather than rewriting it in place.
