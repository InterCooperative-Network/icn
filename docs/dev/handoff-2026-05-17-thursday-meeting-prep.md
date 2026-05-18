---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-17
---

# Handoff — Thursday meeting prep truth packet — 2026-05-17

**Topic:** Produce a docs-only meeting-prep packet that grounds the upcoming Thursday cooperative / Launch / ICN conversation in current repo truth.
**Branch:** `docs/thursday-meeting-truth-packet-2026-05-17`
**Base:** `main` at session start (HEAD `f9a98a2f4` — `test(devnet): add RedundancyProof Slice B fixture`).
**Closes:** none.
**Refs:** none filed by this session.

## Summary

This PR lands one new strategy doc and one new dev handoff. The strategy doc is the meeting brief Matt walks into Thursday's conversation with; the handoff records what was inspected, what was preserved, and what is deliberately *not* part of this session.

This is **docs-only / control-plane-only**. No runtime change, no schema mint, no new ADR, no new RFC, no new contract URN, no production-readiness claim, no live-federation claim, no formal-NYCN-pilot claim, no Phase 2 completion claim, no NYCN private partner data, no K3s / DNS / Forgejo / deploy / GitHub mutation.

## Files changed

- [`docs/strategy/ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md`](../strategy/ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md) — new. The meeting brief. One-sentence frame, ninety-second explanation, a current-truth table separating runtime / fixture / spec / not-ready, a "fixture-backed" plain-language section, a "design / spec only" section, an explicit non-claims list, the institutional spine, a meeting demo spine, a question list for cooperative developers, a list of concrete collaboration asks, and a closing posture statement.
- [`docs/dev/handoff-2026-05-17-thursday-meeting-prep.md`](handoff-2026-05-17-thursday-meeting-prep.md) — new. This file.
- `docs/INDEX.md` — one line under *Strategy Documents (`strategy/`)* pointing at the new brief.

## Repo truth used

Each fact in the brief was sourced from the documents below, not from memory or sprint vocabulary.

| Source doc | Key facts extracted |
|---|---|
| [`docs/STATE.md`](../STATE.md) | Phase 2 status ⏳; partner-bound; NYCN is the *intended* first partner, not a formally committed pilot; the abuse-case strategy doc landed 2026-05-16; cross-sprint disciplines (no payment / wallet / currency / balance / token / crypto / blockchain / timebank framing; LocalDomain not Coop; member-shell-cockpit shared degraded-state rule; privacy-posture-is-not-private-content). |
| [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) | Phase 2 deliverables list extended through #1755–#1764 + the architecture-spec sprint (#1814–#1840). Open `[ ]` items name the work that is *not* done. `idea-0019` (#1748) acceptance gates remain partial. |
| [`docs/OVERVIEW.md`](../OVERVIEW.md) | One-paragraph definition (constraint engine, eight primitives, four entity types); §9 *Current State* split (strong / partial / not ready); regulatory-safe vocabulary table. |
| [`docs/DESIGN_PRINCIPLES.md`](../DESIGN_PRINCIPLES.md) | Three-tier hierarchy: 5 operational invariants, 6 firewall-contract invariants (currently advisory through Waves 2–6), 10 frozen-core invariants. The meta-principles (§5) and the audit findings (§7) frame the meeting's honesty bar. |
| [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) | Ten one-line doctrine rules (§2). Ten code-anchored abuse stories with file:line anchors against `main @ d57ff1d6e` (§3). Receipts prove events, not legitimacy; authority shortcuts must label themselves; unresolved standing is not standing; accepted is not applied. Strategy only — no implementation. |
| [`docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../architecture/DEBIAN_APPLIANCE_MODEL.md) | Stage: **bootable dev image**, not production. No signed release, no immutable rootfs, no A-B updates, no claim flow, no partner-federation activation. `smoke-local.sh --real` one-VM smoke verifies `/v1/health` on port 8080. |
| [`docs/spec/network-anti-entropy-proof-loops.md`](../spec/network-anti-entropy-proof-loops.md) | The entire spec-named proof-artifact identifier set is wire-stable as of #1862. Live emission, classification, repair, and federation rollout are explicitly *out of scope*. Forward work: Slice B / Slice C and runtime emission. |
| [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) | v0 rendering contract. Not a native app. Not a runtime change. Mobile-first, offline-tolerant, accessibility-first, plain-language-first. Consumes ADR-0020 standing, ADR-0027 ActionCard, ADR-0026 receipts. |
| [`docs/spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) | v0 contract for the operator-facing surface. Not a dashboard implementation. Twelve cockpit surfaces, fourteen operator action-card scenarios. Steward required-action card contract is open work (#1837). |
| [`docs/INDEX.md`](../INDEX.md) and [`docs/registry.toml`](../registry.toml) | Doc-control conventions. `docs/strategy/` and `docs/dev/` both have prefix defaults in `[[doc_path_defaults]]`; new docs do not require explicit `[docs."path"]` rows unless they appear under `explicit_registry_warn_prefixes` (currently `docs/plans/`, `docs/planning/`, `docs/onboarding/`). |

Key state facts extracted and used in the brief:

- Phase 2 remains active and partner-bound.
- Proof rail identifiers are wire-stable; runtime emission / live repair / live federation proof loops remain forward work.
- Member shell and steward cockpit are specs + fixtures, not live apps.
- Debian appliance is bootable dev image / local smoke level only.
- Abuse-case hardening is explicit doctrine, but most hardening tracks remain open implementation work.
- Regulatory-safe vocabulary is mandatory: settlement, obligation, allocation, unit, position, receipt, provenance, evidence.

## Preserved boundaries

- **Docs-only.** No Rust, TypeScript, schema, contract, runtime, gateway, kernel, actor, handler, dispatch, or CI behavior changed.
- **No ADR, RFC, or contract URN.** The brief is a reading-frame for one meeting, not an architectural decision.
- **No production-readiness claim.**
- **No live-federation claim.**
- **No formal partner pilot claim.** NYCN is referenced only as the active partnership track / intended first partner per [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) §Phase 2.
- **No NYCN private / partner data.** All examples in the brief are generic ("cooperative developers," "cooperative development organizations").
- **No K3s / DNS / Forgejo / deploy mutation.**
- **No regulatory vocabulary drift.** The brief never uses *payment*, *currency*, *balance*, *wallet*, *token*, *crypto*, *blockchain*, or *timebank* for ICN-native primitives. They appear only inside the explicit "do not claim / avoid this framing" section as negated framings.
- **No Phase 2 completion claim.** Phase 2 is described as ⏳ and partner-bound.
- **No appliance production claim.** The appliance is described as bootable dev image only.
- **No claim of fixture = live.** "Fixture-backed" is explicitly defined as deterministic-in-tests, *not* live network, peers, or institution.

## Validation run

Recorded commands and results:

```text
$ python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
<see PR description for exact output>

$ python3 .github/scripts/compliance_linter.py
<see PR description for exact output>

$ git diff --check
<see PR description — should report no whitespace errors>
```

`lint-arch` was not run because the new docs are reading-frames, not architecture specs; the script targets architecture specs that consume crate names. If reviewers ask, it can be run against the brief but is not expected to surface anything load-bearing.

## Known limitations

- This is a meeting-prep packet, not a full proof-level taxonomy or capability status matrix.
- Does not implement any hardening track from [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md).
- Does not create a demo, a fixture, or a runtime slice.
- Does not update live website copy (`web/` / public site).
- Does not create partner-specific material. NYCN, Launch.coop, or any other named partner is referenced only as context already present in the repo's existing state.
- Does not update [`docs/STATE.md`](../STATE.md) — this is a meeting-prep artifact, not a truth-sync. If a sync-edit is wanted after Thursday, it should be a separate doc-control PR.
- Will drift after Thursday. The brief's snapshot points (Phase 2 status, fixture inventory, wire-stable record set) will move; treat the brief as a 2026-05-17 reading, not a living index.

## Recommended next move

After Thursday's meeting, the suggested repo move for Monday is:

> **Create a proof-level taxonomy and capability status matrix under `docs/reference/project-index/`.** The brief's *What exists today* table is row-shaped but not formally typed; a structured capability matrix that classifies every named primitive by runtime / fixture / spec / not-ready would either advance or close [#1796](https://github.com/InterCooperative-Network/icn/issues/1796) (or whichever issue tracks the project-index taxonomy follow-up). This is the natural next docs-only artifact: it preserves the meeting brief's honesty bar and turns it into something a reviewer can grep.

Other candidate next moves (descriptive, not selected here):

- Update [`docs/STATE.md`](../STATE.md) with a post-meeting sync-edit if the meeting produced concrete framing changes worth recording.
- File one or more abuse-case hardening P0 issues from [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) §14 if the meeting surfaces a partner-level demand for one of them.
- Begin a Slice C fixture (`QuorumSyncCheck` federation placement) if the meeting confirms federation proof-loops as a near-term legibility surface.

None of these are part of this PR.
