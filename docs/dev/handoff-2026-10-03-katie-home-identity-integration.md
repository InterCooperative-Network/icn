---
Status: descriptive
Authority: session memory
Canonical: no
Last verified: 2026-10-03
---

# Session Handoff — 2026-10-03 — Katie / Home identity integration (N4-A)

> **Memory status:** historical session evidence only. Reverify every branch, PR, CI, issue, and blocker claim before acting on it.

## Session scope

**Goal:** reconcile how ICN currently represents human continuity, contextual Subjects, device
Principals, bounded authority, revocation, recovery and contexts against the Katie forcing use
case (durable person, replaceable devices: phone, Pi, desktop, VM 802), then land the smallest
upstream slice that moves the fixture scenario *S with devices A and B; A revoked; A refused, B
accepted, S unchanged* from documentation to a consumable library primitive.

**Boundary:** no production identity for Katie or Matt; no `ContinuityRoot` generated or stored
anywhere; no change to the network-ops Pi thin-client deployment, which proceeds conventionally;
no Mutualware naming change (public brand stays ICN per #2801/#2802); no new household/personal
context kind; no gateway route, container, persistence, replication, recovery or production wiring;
no merge.

## Observed checkout at handoff time

- repo root: `~/icn-dev/worktrees/icn/katie-home-identity-integration`
- branch: `task/katie-home-identity-integration`
- HEAD: `473cb0372` (feat(identity): evaluate delegated device authority at a relying-party position (N4-A))
- base / observed `origin/main`: `7ef8a670a`
- working tree: clean except `.claude/hooks/hook-health.sh` mode bit (pre-existing bootstrap
  workaround for #2691, deliberately not staged; PR #2692 owns the fix)
- icn-ops session: registered, lane uncontended
- observed at: 2026-10-03

These values are provenance for this handoff, not instructions for the next session.

## Durable truth consulted

| Domain/question | Owner consulted | Relevant conclusion |
|---|---|---|
| identity semantics | `docs/architecture/IDENTITY_SEMANTICS.md` (canonical) | seven contracts; Device Principal = ordinary `Did` + grant; I5 "verification returns a grant"; §2.2 historical-authorization limit; §5 context-scoping is client discipline, GEN owns binding |
| GEN-A byte contract | `docs/architecture/GEN_SUBJECT_CONTEXT_GENESIS.md` | only context kind is `GovernanceDomainV1`; context id is any non-empty UTF-8; verified genesis is historical, not current authority |
| convergence ladder | icn#2694 §7 | rung 5 "N4-A relying-party prefix/evaluation verifier `derive_at(E)`" — implemented here; depends on N1 only |
| N4 lane | icn#2599 | invariants 4 (non-retroactive), 6 (position not signer-chosen), 8 (forks halt), 9 (authorship ≠ authorization) all pinned by tests |
| Mutualware | icn#2801, PR #2802 claim ledger | public-brand-first, protocol-stable; no cutover; row C-ID-01 refuses "Human Subject = Principal" |
| operating contract / merge | `AGENTS.md`, `ops/state/truth/policy.json`, `delivery.json` | one bounded change per PR; no merge without per-PR authorization |
| live state | `gh` | open: #2800 (N1-D, rung 2, worktree `task-2694-n1d`), #2802 (truth reconciliation, draft); no N4-A branch/PR/issue existed |

## Evidence produced or inspected

- `cargo test -p icn-identity --test device_authority` → RED (two unresolved imports) before implementation; 25 passed after.
- `cargo test -p icn-identity` → all suites green, including the 216-test N1 suite.
- `cargo fmt --all --check`; `cargo clippy -p icn-identity --all-targets -- -D warnings`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
- `python3 icn/crates/icn-identity/tests/reference/device_act_reference.py` → 4/4 vectors agree with the Rust literals.
- `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --write-document-registry docs/DOCUMENT_REGISTRY.md` → OK (66 pre-existing warnings).
- Three bounded read-only archaeology passes (N1 code; legacy/runtime wiring; client/context docs). Key findings below.

## Work completed

1. `authority_log::derive_prefix(subject, store, through)` — the unchanged N1 fold over bodies at positions ≤ `through`. Sound because candidates at `p` come only from bodies at `p`. `derive_prefix(MAX_POSITION) == derive`.
2. `icn_identity::device_authority` — `DeviceActV1` canonical bytes under `icn.n4.device-act`, `sign_device_act`, `evaluate_device_authority`, `verify_device_act`, refusal enums, `DeviceAuthorityEvidence`.
3. Three `pub(crate)` widenings (`PrincipalKey::{encode,decode}`, `DeviceCapability::from_tag`) and one new `Reader::lp` so the act reuses N1 framing instead of re-deriving it.
4. `tests/device_authority.rs` — 25 tests: the acceptance scenario, non-retroactive revocation, relying-party position rule, fork/gap/unknown fail-closed, capability and span refusals, establishment-authority-as-device refusal, rotation inside the prefix, 4! ingest-order invariance, strict codec, domain distinctness, no-clock guard, cross-implementation vector.
5. `docs/architecture/N4A_DEVICE_AUTHORITY_EVALUATION.md` (normative, non-canonical) registered in `docs/registry.toml`, `docs/INDEX.md`, regenerated `docs/DOCUMENT_REGISTRY.md`. §9 is the external-consumer boundary and the downstream contract.

## Work not completed

- Push and PR were not opened in the first session segment; see the PR for this branch once opened.
- The strict deterministic **container** for (N1 facts, act) — the one missing boundary (doc §9.2 item 2). Not started.
- `icnctl` verb consuming that container. Not started; waits on the container and on #2777's exclusion domain.
- Routing comments on #2694 (rung 5 landed ahead of rungs 3–4) and #2599 (first tranche). Not posted; maintainer's call.
- ADR-0083 drift: its front matter says `not-started` while `docs/spec/institutional-domain.md` and code say landed. Observed, not fixed (out of scope).

## Decisions and rationale

- **N4-A chosen as the slice, ahead of ladder rungs 3–4.** Rungs 3 (GEN-B recognition) and 4 (domain rail) are institutional and do not gate device evaluation; the Katie scenario needs exactly #2694 §7. No contract was changed.
- **Prefix evaluation by filtering the body set, not by modifying the fold loop.** Keeps `resolve` the single decision point and makes the equivalence `derive_prefix(≥frontier−1) == derive` trivially true and tested. Rejected: adding a `revoked_at` to `DeviceGrant` (an N1 semantic change) and a "derive-as-of" state snapshot (a second selector).
- **The act binds the evaluation position but the relying party must match it.** Satisfies N4 invariant 6 while preventing a relying party from re-evaluating an act at a position the signer never saw. Rejected: unbound position (replayable across contexts) and signer-authoritative position (invariant 6 violation).
- **Relying party supplies the Subject.** An act naming another Subject is refused before any signature work, so authority is evaluated in the relying party's context, never the signer's.
- **Establishment authority refused as a device.** Mirrors GEN-A's bootstrap separation; stated as an Alpha-profile rule, not an N1 claim.
- **Payload opaque, `act_id` witness-independent.** Replay slots belong to action families (#2694 §9); N4-A never parses app bytes.
- **Module placed outside `authority_log/` with its own clock guard**, mirroring GEN-A's "outer protocol" placement rather than widening the N1 guard.
- **GEN stance:** personal/household contexts are a new GEN context kind, not a `GovernanceDomainV1` string; a household needs no charter and no founders; nothing to build until #2602 defines the kind.
- **Custody stance:** the continuity root is client-held by contract (HIA §12.2). A Home process may act only as Katie's own client, which makes that process a custody surface equal to her phone; infrastructure operated for others never qualifies. Not solved by storing anything in network-ops.
- **External boundary:** a stateless verifier over retained canonical facts; the gateway is at most a hosted relying party, never the owner of the truth.

## Unsafe assumptions

- `cryptography` 41 in the VM Python is a correct Ed25519 (used only for the audit reference).
- The 2026-08-28 agent-context spine is stale for the new module (it predates GEN-A too); regeneration is a separate chore.
- No other session started N4 work between the live query and the commit.

## Durable promotion check

| Discovery | Durable surface | Promoted? |
|---|---|---|
| "authorized at E" unanswerable after a revoke from the frontier view | `derive_prefix` + tests + doc §5.1 | yes |
| act-bytes contract and vectors | doc §6/§8, Rust literals, Python reference | yes |
| relying-party position rule and act-class mapping | doc §7.1–7.2 | yes |
| downstream MAY/MUST NOT contract | doc §9.3 | yes |
| legacy survey (node key signs recovery attestation; `/v1/devices` unreachable; RN SDK re-implements signing; `Handshake` path ungated) | already in HIA findings / PRINCIPAL_MODEL §2 except the `Handshake` gate note, which has no issue | partly — **context-loss risk:** the `handlers/handshake.rs` ungated neighbour-set write deserves an issue |
| personal/household context kind needed | none (belongs in #2602) | no — context-loss risk until commented on #2602 |
| ADR-0083 status drift | none | no |

## Suggested resume point

This is a **recommendation, not current truth**.

1. Reverify checkout and `origin/main`; `cargo test -p icn-identity --test device_authority`.
2. Requery the PR for this branch, its reviews and required checks.
3. Re-resolve `IDENTITY_SEMANTICS.md`, the N4-A doc, #2694 and #2599.
4. If the premise holds: specify the (facts, act) container as the next bounded slice, consuming the #2800 fact-record layout once it lands; then the `icnctl` verb.

## Reverification targets

- PR state, required checks and review threads for this branch.
- #2800 (N1-D) merge state — the container should align with its record layout.
- #2777 (exclusion domain) — gates any `icnd`/`icnctl` wiring.
- #2602 for any new context kind; #2599 for lifecycle tranches.

## Files and surfaces touched

- `icn/crates/icn-identity/src/device_authority.rs` — new N4-A module
- `icn/crates/icn-identity/src/authority_log/derive.rs` — `derive_prefix`
- `icn/crates/icn-identity/src/authority_log/{mod,body,encoding}.rs` — export and `pub(crate)` framing reuse
- `icn/crates/icn-identity/src/lib.rs` — module registration
- `icn/crates/icn-identity/tests/device_authority.rs`, `tests/reference/device_act_reference.py` — tests and audit reference
- `docs/architecture/N4A_DEVICE_AUTHORITY_EVALUATION.md`, `docs/registry.toml`, `docs/INDEX.md`, `docs/DOCUMENT_REGISTRY.md` — contract and registration
- this handoff

## Closing classification

- Work level: test/library-only, plus normative documentation
- Semantic contract changed: no (N1 wire format, bodies, digests and selection untouched; N4-A is additive)
- Merge performed: no
- Deploy/release/migration performed: no
