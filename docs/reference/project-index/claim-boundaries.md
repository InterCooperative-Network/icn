---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Claim-Boundary Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), current source/tests, and accepted ADRs/RFCs. This map is an **operating manual**, not a truth root.

## 1. Purpose

This guide collects ICN's already-existing firewall, proof-level, source-precedence, and show-readiness rules into one **operational claim-boundary** reference: the rule that *a claim must not outrun its evidence*, applied to inventories, PR bodies, generated artifacts, organizer-rehearsal docs, and public language.

It is **not** a canonical truth source and adds **no** new status vocabulary. It routes to the documents that already own each rule. When this map and a canonical source disagree, the canonical source wins (see §3). There is deliberately no `docs/truth/` tree — a parallel truth root is exactly what the meaning-firewall and source-precedence discipline exist to prevent (see [`README.md`](README.md) §Truth layer).

## 2. Two firewalls — do not conflate

ICN uses "Meaning Firewall" for an **architectural** boundary. The **claim-boundary** firewall is a separate, mostly-conventional discipline. Conflating them is itself a claim-discipline error: the architectural CI checks do **not** police claims, and the claim conventions do **not** police kernel imports.

| Firewall | What it asserts | Where it is defined | Enforcement status |
|---|---|---|---|
| **Architectural Meaning Firewall** | Apps translate meaning into constraints; the kernel enforces constraints **without understanding meaning**. Kernel crates must not import/depend on domain/app crates. | [`KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md), [`meaning-firewall-audit.md`](../../architecture/meaning-firewall-audit.md), onboarding [`05-the-meaning-firewall.md`](../../onboarding/path/phase-2-architecture/05-the-meaning-firewall.md) | **Tool-enforced (partial).** `scripts/check-meaning-firewall.sh` (kernel `use icn_trust::`, with known migrations #865/#866/#867) and `.github/scripts/firewall_denylist.py` (kernel→domain crate deps, #1007). Migration not complete; some couplings tracked, not yet zero. |
| **Claim-boundary firewall** | Docs, PRs, inventories, demos, and public pages must not claim more than the evidence proves. | This map; [`proof-level-taxonomy-capability-matrix.md`](proof-level-taxonomy-capability-matrix.md); [`show-readiness-map.md`](show-readiness-map.md); [`source-of-truth-map.md`](source-of-truth-map.md). | **Mostly convention.** Narrow tool coverage only: `compliance_linter.py` (fintech vocabulary on API surfaces), `readiness_overclaim_linter.py` (scans `docs/deployment` + `docs/operations/deployment` + `docs/pilots`; ~9 patterns across production-readiness / live-federation / blanket-operability / general-availability / governance-completion — a narrow phrase-level guardrail, not whole-firewall enforcement), `doc_control_check.py` (doc truth-class front-matter), the route-inventory `--check` gate. The forbidden collapses in §5 are **not** mechanically enforced. |

## 3. Source-of-truth hierarchy

Full rules and conflict resolution: [`source-of-truth-map.md`](source-of-truth-map.md). Summary order (highest first) — **this map does not replace it**:

1. Current source code and tests (test-only fixtures prove test behavior, not production readiness)
2. `docs/STATE.md` and `docs/PHASE_PROGRESS.md`
3. Accepted ADRs/RFCs (proposed/draft = design direction, not shipped)
4. Recent merged PR bodies / issue-closure notes / review dispositions
5. Runtime contracts, schemas, OpenAPI surfaces, contract docs
6. Project-index maps (navigation; route to truth, do not override it)
7. Architecture / strategy / design-direction docs
8. Generated records (prove files existed at generation time, not current truth)
9. Historical / archive material (archaeology only)

## 4. Status vs proof level

Two **orthogonal** axes; carry both. Detail: [`proof-level-taxonomy-capability-matrix.md`](proof-level-taxonomy-capability-matrix.md).

- **Status** answers *"is the code written, and how completely?"* (`live`/`partial`/`planned`/`fixture-demo`/`unknown`, or the `source-of-truth-map.md` implementation labels).
- **Proof level (L0–L8)** answers *"how much verification evidence stands behind the claim?"* (`L0` named → `L1` schema/contract → `L2` unit → `L3` integration → `L4` local proof loop → `L5` live daemon/gateway → `L6` multi-node/devnet → `L7` partner/organizer rehearsal → `L8` production hardening).

Load-bearing consequences:

- A `live` or `partial` status is **not** a production-readiness claim.
- A static generated inventory (clap-tree scan, route scan) is **L1** — it proves a surface is *declared*, not that its handler runs, unless stronger evidence (a test, a live run) is explicitly cited.

## 5. Forbidden collapses

Each row pairs a tempting overclaim with the minimum evidence that would license the stronger claim. "Enforced today?" states honestly whether tooling catches the collapse or it rests on review convention.

| Unsafe collapse (do not assert) | Minimum evidence to license it | Where to verify | Enforced today? |
|---|---|---|---|
| declaration exists ⇒ implementation works | a test, or handler-body read showing real work | source; tests | No (convention) |
| OpenAPI documents a path ⇒ it is mounted/complete/correct | the route is mounted in the gateway router and behaves | gateway router + handler; `generated/route-inventory.md` | No (convention) |
| route registered ⇒ auth/correctness/show-safe | auth gate + correctness test; accessibility/privacy review for show | handler + tests; §9 red lines | No (convention) |
| client call exists ⇒ server/daemon method is wired | the cited RPC method is registered in daemon dispatch (or the route exists server-side) | [`icn/crates/icn-rpc/src/server.rs`](../../../icn/crates/icn-rpc/src/server.rs); `generated/route-inventory.md` | No (convention) |
| `partial` ⇒ `live` | local-only operation, or binary-spawning e2e proof | handler; `icn/bins/icnctl/tests/**` | No (convention) |
| `live` ⇒ production-ready | hardening: security/privacy/recovery/observability (L8) | `show-readiness-map.md`; `STATE.md` | Narrow: `readiness_overclaim_linter.py` (deployment/ops + pilots docs) |
| fixture/demo data ⇒ live participant state | a live daemon/gateway run, not a committed fixture | `proof-level-taxonomy` rows 7/11/12 | No (convention) |
| fixture-backed shell ⇒ organizer-ready rehearsal | L7 accessibility/privacy checklist **and** #1746 acceptance | `proof-level-taxonomy` §accessibility gate; #1746 | No (convention) |
| NYCN intended partner ⇒ formal pilot | a signed agreement / committed launch | `show-readiness-map.md`; `STATE.md` | No (convention) |
| federation commands/primitives ⇒ live federation | two cooperatives federating in production (Phase 3) | `PHASE_PROGRESS.md`; `show-readiness-map.md` | Narrow: overclaim linter catches the "live federation" phrase, but only in the deployment/ops + pilots docs it scans |
| local proof loop ⇒ multi-node/devnet/production | a recorded L6 devnet/K3s run | `proof-level-taxonomy` (L5 vs L6) | No (convention) |
| website maturity band ⇒ evidence | an ADR/issue/code-path/phase link that resolves | ADR-0032 bands; ADR-0033 (**proposed**) | Editorial only; ADR-0033 linter **not built** |

## 6. Worked examples (the completed #2113 icnctl-status lane)

The icnctl command-status inventory ([`generated/icnctl-command-inventory.md`](generated/icnctl-command-inventory.md), #2113) is a concrete reference implementation of §5:

- **Declared ≠ implemented.** The clap-tree scan is **L1** — it proves a command is *declared*. Status is a *separate*, curated signal verified from handler source, never inferred from the declaration.
- **#2222 (gov delegation) — client call ≠ server wiring.** `gov vote delegate/delegations/revoke` call `governance.delegation.*`, but those methods are **not registered** in [`icn-rpc/src/server.rs`](../../../icn/crates/icn-rpc/src/server.rs) and a repo-wide search finds them only in the CLI. Against a running daemon they return method-not-found, so they were **not** marked `partial` — left `unknown`.
- **#2223 (federation) — `partial` only after a server-side check.** All 20 `partial` federation commands were classified `partial` *because* each cited `network.*`/`federation.*` RPC method was confirmed registered in `server.rs`, or the gateway route was confirmed in `route-inventory.md`. "Does not claim live federation" is on the nonclaims for exactly this reason.
- **#2224 (final unknowns) — honest non-collapse.** `policy.*` became `partial` because the methods are registered in daemon dispatch; `init-coop` was left **`unknown`** because static review cannot honestly collapse its hybrid behavior (always-local keystore/config/bootstrap **plus** a best-effort gateway `POST /v1/gov/domains` gated on daemon-up *and* a token) into a single status.

## 7. Generated artifact rules

Per [`docs/ci/GENERATED_TRUTH_DRIFT.md`](../../ci/GENERATED_TRUTH_DRIFT.md):

- A generated artifact must state **what it proves and does not prove** (generated records prove files existed at generation time, not current truth — hierarchy rank 8).
- **Do not hand-edit generated files.** Edit the generator and regenerate (`docs/scripts/icnctl_command_inventory.py --write`, `docs/scripts/route_inventory.py --write`).
- Every committed generated artifact must either **have a drift check** (wired into `generated-truth.yml` / `docs-freshness.yml`) **or be explicitly declared on-demand with no committed snapshot**. A committed generated artifact with neither silently rots.
- Both committed project-index inventories are now drift-gated in `docs-freshness.yml` (warn-on-drift, advisory): `generated/route-inventory.md` via `route_inventory.py --check` (#2112) and `generated/icnctl-command-inventory.md` via `icnctl_command_inventory.py --check` (#2113 — wired as the immediate follow-up to this map). Both are warn-only (a stale artifact emits a CI `::warning::`, not a merge block) and are listed as promotion candidates in [`docs/ci/GENERATED_TRUTH_DRIFT.md`](../../ci/GENERATED_TRUTH_DRIFT.md).
- Generated records are orientation aids, not truth roots.

## 8. PR body / review checklist (inventory & status PRs)

Reusable checklist — copy into the PR body or run it as a reviewer:

- [ ] Distinguishes **mechanical discovery** (clap/route scan) from **curated status**.
- [ ] Every `partial` cites **server-side wiring** (RPC method registered in `server.rs`) or **route registration** where relevant.
- [ ] Every `live` cites **local-only** behavior or **binary-spawning e2e** evidence.
- [ ] Every `planned` cites an explicit **placeholder / `bail!` / "not yet supported"** marker.
- [ ] **Unknowns left unknown** where evidence is ambiguous (not force-fit to a positive status).
- [ ] Uses `Refs #N` (not a closing keyword) when the issue's acceptance is not met.
- [ ] Includes **nonclaims** for production readiness, formal pilot readiness, live federation, Phase 2 completion, and (when relevant) #2082 and #2099.

## 9. Public / show-readiness language

Full list and the suggested demo narrative: [`show-readiness-map.md`](show-readiness-map.md); website convention: [ADR-0032](../../adr/ADR-0032-website-truth-boundary.md) (accepted; maturity bands `strong`/`advancing`/`maturing`/`behind`/`not-yet`). Red lines to restate briefly:

- No crypto / web3 / token framing (no token, no native currency, receipts are evidentiary, not tradable).
- No financial-product framing (use settlement / unit / identity / position / obligation / allocation; the `compliance_linter.py` enforces the vocabulary on API surfaces).
- No NYCN commitment claim (intended first partner, not a signed/launched pilot).
- No live-federation claim (Phase 3, not Phase 2).
- No blanket production-readiness claim across the substrate.

[ADR-0033](../../adr/ADR-0033-public-maturity-claims-and-evidence-links.md) (evidence-link requirement) is **proposed, not implemented** — do not claim evidence-link enforcement exists yet.

## 10. Non-goals

This map is **not**: a new canonical truth source; a replacement for `STATE.md` / `PHASE_PROGRESS.md`; a new status vocabulary; a new CI gate; a runtime behavior change; and **not** a claim that the claim-boundary rules are fully enforced (most are convention — §2, §5).

## Where to read deeper

| You want… | Read |
|---|---|
| Which source wins on conflict | [`source-of-truth-map.md`](source-of-truth-map.md) |
| How strong the proof is (L0–L8) | [`proof-level-taxonomy-capability-matrix.md`](proof-level-taxonomy-capability-matrix.md) |
| What is safe to show / red lines | [`show-readiness-map.md`](show-readiness-map.md) |
| Which routes / `icnctl` commands back the rehearsal | [`organizer-rehearsal-operability-map.md`](organizer-rehearsal-operability-map.md) |
| The architectural Meaning Firewall | [`KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md) |
| Generated-artifact drift discipline | [`docs/ci/GENERATED_TRUTH_DRIFT.md`](../../ci/GENERATED_TRUTH_DRIFT.md) |
