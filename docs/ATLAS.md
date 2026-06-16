---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-16
---

# ICN Ecosystem Atlas

> **Top-level front door to the whole ICN ecosystem — for humans and for agent/LLM sessions.**
>
> This file is an **index, not a source of truth.** It *composes* the orientation system that
> already exists (see [§2](#2-the-maps-already-exist--route-here-first)) and adds only the
> cross-repo + boundary layer that nothing else owns. When this file disagrees with a canonical
> owner, **the owner wins** ([`ops/state/truth/sources.json`](../ops/state/truth/sources.json) is
> the spine). Never restate volatile facts (status, phases, PR/sprint numbers, IPs) here — link them.

If you read nothing else: ICN is the generic civic/institutional **substrate**. NYCN is the
**first institution package** built on it. The two are deliberately kept apart by the *meaning
firewall* ([§5](#5-boundaries--guardrails)). Everything below routes you to where the real truth lives.

---

## 1. What is ICN / What is NYCN

**ICN (InterCooperative Network)** is institutional infrastructure for democratic organizations
— a substrate daemon (the "constraint engine": apps translate meaning into constraints; the
kernel enforces them without understanding meaning) so cooperatives, communities, and
federations can prove decisions, operate on infrastructure they control, and coordinate across
boundaries without a platform landlord. Canonical: [`VISION.md`](../VISION.md), [`README.md`](../README.md),
[`docs/ARCHITECTURE.md`](ARCHITECTURE.md).

**NYCN (New York Cooperative Network)** is the **first institution ecosystem package / reference
federation** built beside ICN — federation + organizing cooperative + community + committees + a
Summit 2026 activity, with accessibility commitments and support-institution patterns. **NYCN is
not a fork of ICN and contains no ICN primitives.** Its substantive package truth and all private
operating detail live in the **private `nycn` repo** — not in this public repo. ICN core only
knows *that NYCN exists*, *what kind of package it is*, *which ICN surfaces it depends on*, and
*which primitives it pressures upstream*.

**The civic-runtime loop (ICN's core arc):**

```
identity → standing → authority → action card → authorized action
  → CCL/runtime evaluation → storage/compute/governance/economic transition
  → receipt → sync/federation/replication → member shell + steward cockpit
  → challenge/repair/next action
```

---

## 2. The maps already exist — route here first

ICN has a **show-ready orientation layer** at [`docs/reference/project-index/`](reference/project-index/README.md)
plus a machine-readable **truth spine** at [`ops/state/truth/`](../ops/state/truth/). This atlas does
**not** duplicate them — start there:

| You want… | Go to (canonical owner) |
|-----------|--------------------------|
| "What is real right now?" | [`current-truth-map.md`](reference/project-index/current-truth-map.md) → defers to [`docs/STATE.md`](STATE.md) + [`docs/PHASE_PROGRESS.md`](PHASE_PROGRESS.md) |
| Which source outranks which on conflict | [`source-of-truth-map.md`](reference/project-index/source-of-truth-map.md) (human hierarchy) + [`ops/state/truth/sources.json`](../ops/state/truth/sources.json) (machine spine) |
| What may be shown / red lines (no laundering) | [`show-readiness-map.md`](reference/project-index/show-readiness-map.md) |
| Subsystem readiness scorecard | [`docs/status.toml`](status.toml) + [`project-coverage-matrix.md`](reference/project-index/project-coverage-matrix.md) |
| Proof-level claim vocabulary (L0–L8) | [`proof-level-taxonomy-capability-matrix.md`](reference/project-index/proof-level-taxonomy-capability-matrix.md) |
| Org-repo map (icn/nycn/icn-learn/bridge) | [`repository-map.md`](reference/project-index/repository-map.md) — **this atlas extends it** with the private ops/provider layer ([§3](#3-repositories--full-cross-repo-view)) |
| Repo internals (tree / Rust workspace) | [`source-tree-map.md`](reference/project-index/source-tree-map.md), [`rust-workspace-map.md`](reference/project-index/rust-workspace-map.md), [`repo-atlas.md`](reference/project-index/repo-atlas.md) |
| Runtime surfaces a member touches today | [`runtime-surface-map.md`](reference/project-index/runtime-surface-map.md) |
| Decisions / design | [`docs/adr/`](adr/) (index [`ADR-0018`](adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md)) · [`docs/rfcs/`](rfcs/) ([`README`](rfcs/README.md)) |
| "Start here by role" hub | [`docs/reference/project-index/README.md`](reference/project-index/README.md) (has an Agent/Claude row) · [`docs/INDEX.md`](INDEX.md) |

**What this atlas adds that the family does not:** the **cross-repo view including the private
`icn-infra` / `network-ops` ops & provider layers** ([§3](#3-repositories--full-cross-repo-view)),
a consolidated public-safe **boundary + claims guardrail** ([§5](#5-boundaries--guardrails)), the
**NYCN-home reconciliation flag** ([§4](#4-nycn--icn-the-package-boundary)), a top-level **agent
preflight** ([§6](#6-agent--llm-session-preflight--read-in-order)), and the machine-readable
companion [`ops/state/ecosystem.json`](../ops/state/ecosystem.json).

---

## 3. Repositories — full cross-repo view

Extends [`repository-map.md`](reference/project-index/repository-map.md) (which covers
`icn`/`nycn`/`icn-learn`/`icn-community-bridge`) with the **private ops & provider layers**. This
public atlas **names** private repos and their *role only*; it does not reproduce their content.

| Repo | Visibility | Responsible for (canonical-for) | Must NOT contain |
|------|-----------|----------------------------------|------------------|
| **`icn`** (this repo) | public | Generic primitives & runtime, public substrate docs, architecture, ADRs/RFCs, `docs/STATE.md`, `docs/status.toml`, truth spine (`ops/state/truth/`). Public truth surface. | Institution-local meaning; private operating detail; secrets; real personal/partner data |
| **`nycn`** | private | NYCN institution package: Summit 2026 structure, committees, accessibility commitments, support-institution patterns, sponsor lifecycle examples, package manifest, **private-overlay templates** | ICN primitives; product/substrate truth; real PII/sponsor/attendee data in git |
| **`icn-infra`** | private | Org **operating contracts**, runbooks, the cross-repo **boundary contract** (`docs/boundary.md`), **public-claims gates** (`docs/PUBLIC-CLAIMS.md`), **source classification** (`docs/SOURCE-CLASSIFICATION.md`), host roles, service contracts | Secrets (referenced by name only); product/substrate truth |
| **`network-ops`** | private | Concrete **provider reality**: personal/homelab topology, raw configs, **secrets**. Most-private layer. | Anything that should be product or org truth |
| **`icn-learn`** | private | ICN Academy — role-based learning/onboarding. **Teaches from canonical; is not canonical.** | Canonical definitions of ICN |
| **`icn-community-bridge`** | private | Discord→Matrix onboarding bridge — scaffold/docs-first, **not deployed**. Outreach/routing. | Governance authority; canonical truth |
| **`org-github` / `.github`** | public | Org profile + shared community-health files | Product/architecture truth |
| **`homelab-inventory`** | private (personal) | Infra the homelab runs on; observed **read-only** from `ops/`. | ICN code/product truth |

**Public vs private:** public = `icn`, `.github`. Private = `nycn`, `icn-infra`, `icn-learn`,
`icn-community-bridge`. Provider/personal (most private) = `network-ops`, `homelab-inventory`.
Private repos' docs are access-gated; read them in-repo if you have access.

The three-layer **product / org-ops / provider** split (`icn` / `icn-infra` / `network-ops`) is
defined canonically in `icn-infra/docs/boundary.md` *(private)*. The institution-package boundary
(ICN ↔ NYCN) is canonical in
[`docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](architecture/INSTITUTION_PACKAGE_BOUNDARY.md).

---

## 4. NYCN ↔ ICN: the package boundary

- **NYCN package truth** (structure, templates, fixtures, mappings, private overlays) lives in the
  **private `nycn` repo**. Real names/emails/phones/addresses/sponsor amounts/real DIDs **never
  enter git** — overlays are git-ignored; only `*.private.example.yaml` templates ship.
- **NYCN→ICN dependency contract** is owned by `nycn/docs/sync/ICN_DEPENDENCY_STATUS.md` *(private)*.
- **Primitives NYCN pressures upstream on ICN** (track as ICN work, not NYCN work):
  - **Obligation / allocation / settlement primitives** — [`RFC-0001`](rfcs/RFC-0001-obligation-allocation-settlement-primitives.md) (draft). Blocks NYCN's sponsor obligation lifecycle.
  - Organizer/operating-body model (RFC-0012, candidate); external settlement bridge (RFC-0013, candidate); support-institution pattern (RFC-0014, candidate); public maturity-claim convention (ADR-0033, proposed); public-surface/learning-repo arch ([`RFC-0015`](rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md)).
- **Activation model (3 layers):** public package export → private overlay (git-ignored, binds real
  DIDs/contacts) → live ICN-node state. Real holder-label → DID activation requires consent +
  role-named authority + steward/operator + privacy-reviewer signoff.

> **Open reconciliation question (flag, do not resolve here):** there are currently **two NYCN
> homes** with *divergent self-descriptions*. The in-core tree [`institutions/nycn/`](../institutions/nycn/)
> (created earlier) calls itself *"the operational home for institution-owned artifacts going
> forward"* with conditions for a later split; the separate `nycn` repo calls the in-core tree a
> *"canonical test fixture … not produced by, or owned by, this NYCN repo."* The in-core tree
> carries placeholder data only (no PII observed). **Canonical ownership needs an explicit
> decision** — reconcile against
> [`docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](architecture/INSTITUTION_PACKAGE_BOUNDARY.md)
> and [`docs/strategy/ICN_NYCN_INSTITUTION_PACKAGE_BOUNDARY.md`](strategy/ICN_NYCN_INSTITUTION_PACKAGE_BOUNDARY.md).
> Until then, treat the **private `nycn` repo** as the substantive package home and
> `institutions/nycn/` as the in-core, public-safe fixture/shape.

---

## 5. Boundaries & guardrails

**The meaning firewall (load-bearing):** ICN owns generic primitives and runtime; institution
packages own local meaning; the kernel/substrate stays meaning-blind. Local meanings must not be
welded into substrate primitive types. Canonical: [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) and the
5 invariants in [`AGENTS.md`](../AGENTS.md).

**Readiness vocabulary — no laundering.** Never restate a lower tier as a higher one: *planned ≠
done · rehearsed ≠ live · private ≠ public · fixture ≠ real.* The maturity tiers — `shipped/runtime`,
`local/demo/rehearsal`, `fixture-only`, `package-declared planned`, `blocked on ICN primitives`,
`future/speculative`, `explicitly not true` — align with [`show-readiness-map.md`](reference/project-index/show-readiness-map.md),
the [`status.toml`](status.toml) enum (`exceeds-spec`→`foundation-only`), and the proof-level
taxonomy (L0–L8). CI enforces parts of this: *Readiness Overclaim Check*, *Regulatory Compliance
Linter*, *Meaning Firewall Check*, *Firewall Contract Enforcement*, *Secrets Placeholder Check*,
*Pilot Provenance Invariant*.

**Never claim publicly without gates** (owned by `icn-infra/docs/PUBLIC-CLAIMS.md`, private):
production readiness, live federation, a formal pilot/partner, a running Forge (Forgejo is a
*component*, not the Forge — icn-infra ADR-0004), or a live Matrix/community bridge.

**Never copy across these boundaries:**
- NYCN private operating detail (Summit fixtures, private overlays, real PII/sponsor/attendee data) → **never** into public `icn`. Reference at boundary level only.
- `icn-infra` / `network-ops` operational values, host addresses, secrets → never into public `icn` (extraction requires the icn-infra scrub gate, ADR-0005, private).
- ICN primitives → never copied *into* `nycn` (NYCN depends, doesn't fork).
- `icn-learn` / `icn-community-bridge` → never become canonical; they consume/route.

---

## 6. Agent / LLM session preflight — read in order

1. **This file** (`docs/ATLAS.md`) — ecosystem orientation + where truth lives.
2. [`AGENTS.md`](../AGENTS.md) (the 5 invariants) + [`CLAUDE.md`](../CLAUDE.md) (Claude-specific rules).
3. [`docs/reference/project-index/current-truth-map.md`](reference/project-index/current-truth-map.md) → [`docs/STATE.md`](STATE.md) + [`docs/PHASE_PROGRESS.md`](PHASE_PROGRESS.md) — what's real now.
4. [`docs/status.toml`](status.toml) — before claiming any subsystem works.
5. [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) — system truth + meaning firewall.
6. [`ops/state/truth/sources.json`](../ops/state/truth/sources.json) — the spine; resolve any conflict here.
7. **Live state:** `git branch --show-current`, `gh pr list`, `gh run list` — never hardcode volatile facts.
8. Boundary/claims/private-ops work → the access-gated docs named in [§3](#3-repositories--full-cross-repo-view)/[§5](#5-boundaries--guardrails) (in `icn-infra` / `nycn`).

---

## 7. Keeping this atlas current

Manually maintained and intentionally thin (an index). Review when:

- [ ] A repo is added/removed/changes visibility → update [§3](#3-repositories--full-cross-repo-view) + `ops/state/ecosystem.json` (and `docs/reference/project-index/repository-map.md` for the org-repo subset).
- [ ] A canonical map/owner moves or is created → update the [§2](#2-the-maps-already-exist--route-here-first) routing + `ecosystem.json`.
- [ ] The readiness **enum** changes (not its values) → update [§5](#5-boundaries--guardrails).
- [ ] A NYCN→ICN dependency/RFC pressure appears or lands → update [§4](#4-nycn--icn-the-package-boundary).
- [ ] The NYCN-home reconciliation is decided → resolve the open question in [§4](#4-nycn--icn-the-package-boundary).
- [ ] **Do not** record status/phase/PR/sprint values here — link their owners. If you're copying a value in, link instead.

> **Do-not-leak reminder:** this file is in the **public** `icn` repo. Never add NYCN private
> detail, `icn-infra`/`network-ops` operational values, host addresses, secrets, or real
> personal/partner data. Reference private repos by name and role only.
