---
Status: draft
Canonical: no
Last Reviewed: 2026-05-01
---

# Licensing strategy matrix (autonomy review)

> **Status: planning artifact only.** Not legal advice. Not a relicensing decision. No license metadata changes are proposed by this document. No obligations are defined for downstream consumers. This is a maintainer-facing scaffold for a future maintainer/legal review and any follow-up RFC or ADR. Where this prose and `LICENSE` / per-crate `Cargo.toml` files disagree, the metadata governs.

> **Companion docs**:
> [`../../LICENSING.md`](../../LICENSING.md) records the current repository license metadata and the open questions this document begins to scope;
> [`../internal/legal-considerations.md`](../internal/legal-considerations.md) records community-deployment regulatory questions (also "not legal advice");
> [`../architecture/KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md) and [`../architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md) describe the structural boundaries this matrix is organized along.

> **Tracking issue**: [#1692](https://github.com/InterCooperative-Network/icn/issues/1692).

## Purpose

Begin a docs-only licensing strategy pass after PR #1686 (`244b8bb0`) documented the current inconsistent metadata. Produce a component-by-component licensing/autonomy matrix and a recommendation outline that future maintainer/legal review can refine into an RFC/ADR candidate.

This document does not select a license, does not modify metadata, and does not give legal advice.

## Current metadata summary

Carried forward verbatim from [`LICENSING.md`](../../LICENSING.md):

- The root [`LICENSE`](../../LICENSE) file contains the GNU Affero General Public License, Version 3 (AGPL-3.0).
- [`icn/Cargo.toml`](../../icn/Cargo.toml) declares `[workspace.package] license = "MIT OR Apache-2.0"`.
- Of 49 canonical `Cargo.toml` files (excluding `target/` and `.claude/worktrees/`): 13 declare `MIT OR Apache-2.0` explicitly, 34 inherit via `license.workspace = true`, 2 declare no `license` field at all (`examples/wasm-compute/Cargo.toml`, `icn/crates/icn-ccl/fuzz/Cargo.toml`).
- No `Cargo.toml` in the canonical set declares AGPL-3.0.
- The Cargo `license` field is intent metadata, not a license document.

The relationship between the root `LICENSE` (AGPL-3.0) and the workspace metadata (`MIT OR Apache-2.0`) is an [open question](#decision-questions-for-maintainerlegal-review).

## Why source-code availability is not enough for ICN

ICN's anti-enclosure promise is institutional autonomy, not just open source code. A source-code-only license — even AGPL-3.0 — does not by itself guarantee that a hosted ICN user retains:

- **Identity continuity.** Cooperative DIDs and signing keys remain controlled by the cooperative, not the host.
- **Record portability.** Members can verify receipts; institutions can export full governance / ledger / receipt history at any time, in a portable form, regardless of who hosted them.
- **Federation portability.** An institution can change federation membership or hosting providers without losing state, without re-signing receipts, and without re-issuing identity.
- **Provenance integrity.** Receipts are signed by the institution's keys, not the host's; provenance graphs remain intact across migration.
- **Hosted-provider exit.** A hosted provider cannot become a landlord. Tenant export is a baseline requirement, not a paid feature.
- **Source access for modified network services.** When a host runs a modified ICN, that host's modifications are available to the network of users.

Some of these guarantees are within scope of a source-code copyleft (AGPL or similar). Others — particularly identity continuity, record portability, federation portability, and hosted-provider exit — may be more naturally enforced by **policy layers** (certification, trademark, hosting agreements, federation admission) rather than by the source license itself.

This document organizes the question along those two axes.

## Component-by-component matrix

The matrix is descriptive of present state and candidate posture only. **Posture is "candidate to evaluate," not "decision."** Final scope, license selection, and policy obligations are deferred to a future maintainer/legal review.

| Component | Current metadata signal | Path evidence | Adoption pressure | Enclosure risk | Autonomy risk | Candidate posture | Open questions |
|---|---|---|---|---|---|---|---|
| Root repository / source distribution | `LICENSE` contains AGPL-3.0 | [`LICENSE`](../../LICENSE) | Medium (project as whole) | Medium-high (source-only copyleft) | High (no key/data/receipt portability guarantee) | Network copyleft + policy layer | Whether AGPL-3.0 is intended to govern the project as a whole; relationship with the Rust workspace metadata |
| Kernel / substrate crates (`icn-core`, `icn-kernel-api`, `icn-net`, `icn-gossip`, `icn-store`, `icn-encoding`, `icn-protocol`, `icn-services`, `icn-commons`) | `license.workspace = true` → `MIT OR Apache-2.0` | `icn/crates/icn-{core,kernel-api,net,gossip,store,encoding,protocol,services,commons}/Cargo.toml` | High (reusable substrate; permissive aids ecosystem reuse) | Lower (kernel is small surface; firewall keeps it semantically blind per [KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md)) | Lower (kernel doesn't hold institutional state) | Permissive (preserve `MIT OR Apache-2.0`) | Whether kernel's permissive posture should be made explicit per-crate vs. inherited; SPDX header policy |
| Identity / crypto crates (`icn-identity`, `icn-crypto`, `icn-crypto-pq`, `icn-zkp`, `icn-authz`, `icn-naming`, `icn-steward`) | Mixed: `icn-zkp` and `icn-crypto-pq` declare `MIT OR Apache-2.0` explicitly; the rest inherit via workspace | `icn/crates/icn-{identity,crypto,crypto-pq,zkp,authz,naming,steward}/Cargo.toml` | High (broad reuse value for any DID / Ed25519 / PQ ecosystem) | Lower (crypto primitives are commodity once standardized) | Medium (key custody patterns matter institutionally; library APIs themselves do not capture data) | Permissive for libraries; pair with **policy** layer for key-custody requirements | Whether key-custody / rotation / export policies live as runtime contracts, certification policy, or hosted-provider obligations |
| Daemon / runtime binaries (`icnd`, `icnctl`, `icn-console`) | `license.workspace = true` → `MIT OR Apache-2.0` | `icn/bins/{icnd,icnctl,icn-console}/Cargo.toml` | Lower (the binaries themselves are not typically library deps) | **High** (a hosted modified daemon could withhold member-facing source / data without obligation) | **High** (the daemon holds keys, receipts, governance state during operation) | Network copyleft (AGPL or similar) **plus** policy layer for tenant export / key custody | The contradiction the LICENSING.md flagged: workspace says `MIT OR Apache-2.0`; root `LICENSE` says AGPL. Future decision must resolve which signal applies to the daemon, and whether AGPL alone is sufficient |
| Gateway / API services (`icn-gateway`, `icn-rpc`, `icn-api`, `icn-http-kit`) | `license.workspace = true` → `MIT OR Apache-2.0` | `icn/crates/icn-{gateway,rpc,api,http-kit}/Cargo.toml` | High (API surface attracts ecosystem) | Medium (a hosted gateway is the most likely landlord vector) | High (the gateway is what members and apps actually talk to; observability / identity binding lives here) | Mixed: permissive for API surface + libraries; **network copyleft + autonomy policy for hosted instances** | Whether AGPL on hosted gateways suffices; whether "hosted ICN" requires a separate certification policy distinct from source license |
| SDKs (`sdk/typescript/`, `sdk/react-native/`) | `MIT OR Apache-2.0` (per `package.json` / repo conventions) | `sdk/typescript/package.json`, `sdk/react-native/package.json`, `sdk/typescript/README.md` | High (SDK adoption depends on permissive posture) | Lower (SDKs are libraries; they do not host institutional state) | Lower (SDKs are clients, not custodians) | Permissive (preserve current) | None novel; SPDX header policy applies |
| Apps / tools (`apps/governance`, `apps/trust`, `apps/ledger`, `apps/echo`, `icn/apps/{governance,membership,charter,ledger}`) | All declare `license = "MIT OR Apache-2.0"` explicitly | the eight `apps/*` and `icn/apps/*` `Cargo.toml`s | Medium (Policy Oracles + apps are reusable but more domain-shaped than substrate) | Medium-high (apps interpret meaning; a hosted modified app could absorb governance state without source obligation) | Medium-high (apps are where institutional decisions and receipts originate) | Network copyleft for app deployments **or** policy-layer requirement that hosted apps publish modifications | Whether apps should follow the daemon's posture or stay permissive with separate policy guarantees |
| Docs (text under `docs/`) | No per-doc license declaration; effectively covered by root `LICENSE` (AGPL-3.0) for the source distribution | `docs/**/*.md`, `LICENSING.md`, `README.md` | Medium (docs are reused as orientation by adopters) | Lower (text reuse is rarely an enclosure vector) | Lower | Same posture as the source distribution; consider explicit documentation license (e.g., CC-BY or CC-BY-SA) only if a decision is made to distinguish docs from code | Whether to declare a separate documentation license; CC-BY vs CC-BY-SA fit |
| Generated artifacts (`docs/reference/project-index/generated/`, dist artifacts) | No per-artifact license declaration; covered by source distribution license | `docs/reference/project-index/generated/icn-file-record.{json,md}` (PR #1691) and similar | Lower | Lower | Lower (artifacts describe state, not state itself) | Same as source distribution | Whether generated artifacts should carry an explicit license header for downstream redistribution |
| Deployment manifests (`deploy/`, `docker/`, `monitoring/`, `ops/`) | Deploy README references `MIT OR Apache-2.0`; manifests themselves carry no per-file license declaration | `deploy/`, `deploy/README.md`, `docker/`, `monitoring/`, `ops/` | Medium (operators copy manifests as starting points) | Medium (manifest defaults can lock in landlord-shaped configurations) | Medium (default config shapes how hosted providers run ICN) | Permissive for manifests + policy guidance for hosted-provider autonomy defaults | Whether hosted-provider autonomy policy should ship with default manifests (e.g., default tenant-export config, default key-custody defaults) |
| Institution packages (per [`INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md)) | Per-package; ICN does not own institution-specific licensing decisions | NYCN's package lives at `InterCooperative-Network/nycn`; this document does not modify NYCN | N/A (per-institution) | N/A (per-institution) | N/A (per-institution) | Per-institution decision; ICN-side recommendation is to publish guidance, not to dictate | Whether ICN provides a recommended package-licensing template (mirroring the recommended boundary doc) |
| Website / public narrative (`website/`) | Uses `MIT OR Apache-2.0` per repo conventions | `website/`, `web/api-docs/`, `web/pilot-ui/`, `web/dashboard/` | Medium | Lower | Lower | Permissive; consider documentation license for narrative content if a docs-license decision is made | Whether website narrative content (vs. code) should declare a separate license |
| **External reference: NYCN package** (not modified by this document) | Per-package metadata at `InterCooperative-Network/nycn`; not in scope here | external repo | N/A | N/A | N/A | per-institution; out of scope | None for this document |
| **External reference: icn-learn** (not modified by this document) | Per-package metadata at `InterCooperative-Network/icn-learn`; not in scope here | external repo | N/A | N/A | N/A | per-package; out of scope | None for this document |

The "Candidate posture" column is **what to evaluate**, not what to adopt. None of these postures is selected by this document.

### Must-not-overclaim notes

- The matrix describes **observed metadata** plus **structural risk shape**; it does not assert that any candidate posture is the right answer for any component.
- Where the matrix lists "Network copyleft" or "CAL/CAL-like" as a candidate, that means *as a candidate to evaluate*, not as a settled choice.
- Where the matrix lists "policy layer," that includes potential mechanisms that do not yet exist in the project (certification policy, trademark policy, hosted-provider autonomy policy, federation admission criteria). Those mechanisms would have to be authored separately if the maintainer/legal review chooses to use them.
- The Cargo `license` field is intent metadata; the matrix's "Current metadata signal" column reflects what `Cargo.toml` declares, not what governs reuse in any binding sense.

## License / policy strategy options

Five non-exclusive option families for the maintainer/legal review to consider. Each is described as a candidate; the recommendation in this document is **not to choose one** but to scope a future decision.

### Option A — Keep current metadata; clean inconsistencies only

- Resolve the two `Cargo.toml` files with no `license` field (`examples/wasm-compute`, `icn/crates/icn-ccl/fuzz`) by either adding `license.workspace = true` or an explicit license.
- Decide whether the root `LICENSE` (AGPL-3.0) is intended or whether the workspace `MIT OR Apache-2.0` is canonical, and align the metadata accordingly.
- Do not change any other license posture.
- **Cost**: low. **Strength against enclosure**: weak (no new guarantees beyond current code license). **Compatible with**: every other option as a step zero.

### Option B — Permissive libraries + AGPL runtime / services

- Keep `MIT OR Apache-2.0` for substrate / kernel crates / SDKs / API client libraries.
- Move daemon / runtime / hosted-service crates explicitly to AGPL-3.0.
- Document the boundary in `LICENSING.md` and update Cargo metadata to match.
- **Cost**: medium (per-crate license-field updates; ecosystem-compatibility review). **Strength against enclosure**: medium (network copyleft for hosted services). **Gap**: does not by itself guarantee key/data/receipt portability for a hosted user.

### Option C — Permissive libraries + CAL / CAL-like runtime / autonomy layer

- Keep `MIT OR Apache-2.0` for libraries.
- Adopt CAL (Cryptographic Autonomy License) — or CAL-like obligations — for components where user control over keys and personal data is core (daemon, gateway, custody-bearing apps).
- Pair with policy layers for non-license obligations.
- **Cost**: high (legal review of CAL fit; ecosystem-compatibility review; CAL is less broadly adopted than AGPL/MIT/Apache). **Strength against enclosure**: high (data/key control is licensed, not just policy-bound). **Risk**: CAL is novel relative to common licenses; ecosystem and contributor reactions need evaluation. CAL is **not** chosen by this document; its appropriateness for any specific ICN component is an open question.

### Option D — Code license stays conventional; autonomy moves into policy layers

- Keep current license metadata or move to Option B.
- Author dedicated policy layers separately:
  - **Hosted-provider autonomy policy** (tenant export, key custody, rotation, receipt portability obligations for any hosted ICN deployment).
  - **Certification / trademark policy** (use of "ICN" name and certification mark requires meeting autonomy criteria).
  - **Federation admission baseline** (federations admitting an institution require autonomy guarantees the institution can verify).
- **Cost**: medium-high (policy authoring + governance). **Strength against enclosure**: medium-high (depending on enforcement). **Risk**: policy layers without legal teeth can be ignored by hostile hosts. A trademark policy is the most enforceable of the three.

### Option E — Hybrid model

- Combine Options A, B, C, and D selectively across the matrix:
  - permissive substrate (Option A baseline + Option B's per-component split for libraries vs runtime);
  - network copyleft for hosted services (Option B);
  - CAL or CAL-like obligations only on components where data/key autonomy is the central guarantee (Option C, narrow scope);
  - non-license autonomy policy layered on top (Option D).
- **Cost**: highest (every component decided explicitly; multiple policy artifacts authored). **Strength against enclosure**: highest, if the hybrid is coherent. **Risk**: complexity for adopters and contributors; review burden.

The maintainer/legal review will likely converge on Option A as a step zero (resolve inconsistencies) and then choose between Options B–E for the architectural posture. **No option is recommended by this document**; the goal is to scope the decision, not to make it.

## ICN autonomy guarantees to preserve

Independent of which license/policy combination future review selects, the autonomy guarantees the strategy should preserve are:

1. **Institutional export.** Every institution can export the full governance / ledger / receipt / membership history at any time, in a documented format, signed by the institution's keys.
2. **Key custody and rotation.** Identity keys remain under the institution's control; key rotation is supported without losing record continuity; a hosted provider does not hold primary custody of an institution's signing keys.
3. **Receipt / provenance portability.** Receipts are signed by the institution's keys, not the host's; the receipt envelope (per [`ADR-0026`](../adr/) and [`KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md)) is portable across hosts.
4. **Federation portability.** Federation membership is changeable without losing state; an institution can leave a federation, change federations, or be hosted independently.
5. **Hosted-provider exit.** Tenant export is a baseline requirement of any hosted ICN deployment, not a paid or opt-in feature. Exit must be technically possible, format-documented, and signature-preserving.
6. **Source access for modified network services.** When a host runs a modified ICN, the modifications are available to the network of users (the AGPL-style guarantee, but written into both license and policy where overlap is helpful).
7. **Institution package portability.** An institution's package (e.g., NYCN's) remains portable across ICN deployments without dependency on a specific host's modifications.
8. **No platform-landlord capture.** No structural mechanism by which a hosted provider can become a landlord over the cooperative network — neither through license loophole, certification monopoly, nor data-exit friction.

These guarantees are the substantive output the licensing strategy should defend. The license + policy combination is the means; these guarantees are the ends.

## Decision questions for maintainer/legal review

Carried forward from `LICENSING.md` and #1692, refined where this matrix sharpens them:

1. Which parts of ICN should remain permissively licensed for adoption (kernel, identity / crypto libraries, SDKs)?
2. Which parts need network copyleft (daemon, gateway, hosted apps)?
3. Which parts need data/autonomy protections beyond source-code copyleft (custody-bearing components)?
4. Is CAL appropriate for any ICN component? Which components, and under what scope?
5. If CAL is not appropriate, which CAL-like obligations need to live in policy, certification, hosting agreements, or governance requirements?
6. Should runtime / tool layers differ from low-level libraries in license posture (the Option B / E split)?
7. Should institution packages have a different posture than generic ICN substrate? Should ICN publish a recommended package-licensing template?
8. What export / key / provenance rights must every hosted ICN user retain, in concrete terms (formats, frequency, signature requirements)?
9. What is the relationship between license, trademark, certification, and federation admission? Is one of these mechanisms load-bearing over the others?
10. What specific metadata changes would be needed if the project adopts a new posture (per-crate `license` fields, `workspace.package.license`, root `LICENSE`, `NOTICE` files, SPDX headers)? In what order should they land?
11. What relationship is intended between the root `LICENSE` (AGPL-3.0) and the workspace `MIT OR Apache-2.0`? Is one signal stale, or is a hybrid posture intended?
12. What licensing applies to the two `Cargo.toml` files with no `license` field, and should they declare `license.workspace = true` or an explicit license?

Resolving any of these is a maintainer/legal decision and should land as a dedicated, explicit PR or RFC.

## Recommended next artifacts

If the maintainer/legal review proceeds, the following artifacts are likely needed. **None of them is started by this document**; this is a scoping list for the next planning step:

1. **Licensing strategy RFC candidate.** `docs/rfcs/RFC-NNNN-licensing-strategy.md` proposing one of Options A–E (or a refined hybrid) with explicit per-component decisions and a metadata-change sequence.
2. **SPDX / header audit.** Per-file inventory of where SPDX identifiers exist, where they would be added, and the policy decision on whether to add them.
3. **Component metadata audit.** Per-crate inventory of `license` field values vs. desired values, including the two unset crates; the existing PR #1691 generated repo-record snapshot can be a starting point.
4. **Hosted-provider autonomy policy.** A separate document defining tenant-export obligations, key-custody constraints, receipt-portability requirements, and federation-portability requirements that any hosted ICN deployment is expected to meet.
5. **Certification / trademark policy.** Use of the "ICN" name and any certification mark requires meeting the autonomy criteria above. This is a non-license enforcement mechanism that complements (or stands in for) license-based copyleft.
6. **Federation admission autonomy baseline.** Federations admitting an institution require the institution to retain the autonomy guarantees above. A baseline checklist for federation admission.
7. **Updated `LICENSING.md`.** After the strategy decision lands, the conservative metadata + open-questions document is updated to record the decision (in a separate PR clearly titled and reviewed for the licensing implication).

## Recommended framing on CAL

CAL is a **candidate to evaluate** for components where user control over keys and personal data is core (daemon, gateway, custody-bearing apps). Specifically:

- CAL-like data and key custody obligations are a closer fit for ICN's anti-enclosure goals than AGPL alone, because AGPL primarily protects source code availability while CAL extends to the user's data and keys.
- CAL is less broadly adopted than AGPL / MIT / Apache. Ecosystem and contributor reactions need evaluation; some downstream adopters may be unwilling or legally unable to adopt CAL-licensed components.
- CAL adoption requires legal review. CAL is not chosen by this document. Whether CAL applies to any specific ICN component is one of the [decision questions](#decision-questions-for-maintainerlegal-review).

A safe summary recommendation, for the future maintainer/legal review to refine or replace:

> Evaluate CAL or CAL-like obligations for runtime / hosted / autonomy-sensitive layers, while preserving permissive licensing for low-level reusable primitives where adoption matters.

This recommendation is **not a decision**. It is an articulation of the trade-off the review will need to weigh.

## Non-goals

This document does **not**:

- modify `LICENSE`, any `Cargo.toml` license field, or any other license-bearing artifact;
- relicense any code;
- add SPDX headers;
- give legal advice;
- define obligations for downstream consumers;
- select a license or a license combination for any component;
- make any change to NYCN, icn-learn, or any non-ICN-repo source;
- start runtime implementation, create crates, or modify code;
- start RFC-0016, RFC-0017, or any PR B/C/D/E work;
- start a hosted-provider autonomy policy, a certification / trademark policy, or a federation admission baseline (those are listed as **next-step artifacts**, not deliverables of this document);
- assert that the present license metadata signals are intentional, harmonized, or final.

When the future maintainer/legal review converges on a posture, the resulting decision lands as a dedicated, explicit PR or RFC clearly titled and reviewed for the licensing implication. This document supports that future review by scoping the question; it does not pre-empt it.
