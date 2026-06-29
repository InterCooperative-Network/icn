---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-29
---

# CodeQL alert triage — 2026-06-29

This is a point-in-time, static triage record for GitHub CodeQL alerts [#100](https://github.com/InterCooperative-Network/icn/security/code-scanning/100) and [#101](https://github.com/InterCooperative-Network/icn/security/code-scanning/101). It records evidence and follow-up without changing code, alert state, CodeQL configuration, or repository settings.

## Inspection basis

- Repository commit inspected: `76f5fcb6a3c0d04016610f0b72d0e3d162ddde19`.
- CodeQL rule for both alerts: `rust/hard-coded-cryptographic-value` (`critical` scanner severity).
- Most recent alert instances inspected: `refs/heads/main` at `74d677e57cbc0676dc6af2934bc679427a461b98`.
- The affected source paths did not change between the instance commit and the inspected commit.
- Repository policy basis: [`SECURITY.md`](../../SECURITY.md) places the Rust workspace in scope.
- Assessment method: static source, call-site, dependency, history, and nearby-test inspection only. No runtime validation or CodeQL query-engine inspection was performed.

## Detailed triage

### Alert #100 — VUI commitment salt

| Field | Assessment |
|---|---|
| Reported location | `icn/crates/icn-steward/src/enrollment.rs:97`, `EnrollmentRequest::new` |
| CodeQL report | A hard-coded value is used as a salt. |
| Preliminary classification | False-positive candidate |
| Triage verdict | `not_actionable` with high static confidence |
| Product surface | In-scope Rust library API; no supported trust boundary is crossed by the reported literal |

The reported `[0u8; 16]` is a destination-buffer initialization. On the next statement, `rand_core::OsRng.fill_bytes` overwrites the full buffer before `compute_commitment` receives it. The locked `rand_core` implementation identifies `OsRng` as an operating-system randomness source implementing `CryptoRng`. Nearby enrollment integration coverage also expects equal inputs to produce different commitments because the salts differ.

No dismissal occurred because alert disposition is outside this triage slice and requires maintainer review. No remediation occurred because the static path does not carry the zero initialization into cryptographic use; changing runtime code solely to silence the scanner is not supported by the evidence.

Recommended next action: a maintainer may review the scanner result and its query precision in a separately authorized alert-management pass. Preserve the current RNG overwrite unless new evidence identifies a distinct defect.

### Alert #101 — proof replay nonce

| Field | Assessment |
|---|---|
| Reported location | `icn/crates/icn-zkp/src/types.rs:126`, `ProofContext::new` |
| CodeQL report | A hard-coded value is used as a nonce. |
| Preliminary classification | False-positive candidate |
| Triage verdict | `not_actionable` with high static confidence |
| Product surface | In-scope Rust library API; no supported trust boundary is crossed by the reported literal |

The reported `[0u8; 16]` is a destination-buffer initialization. On the next statement, `rand::rng().fill_bytes` overwrites the full buffer before `ProofContext` is constructed. The locked `rand` 0.9.4 implementation uses a thread-local ChaCha12 generator seeded and periodically reseeded from `OsRng`. The generated nonce is then bound into proof public inputs and checked by `ZkVerifier`'s replay guard; nearby coverage expects reuse of one generated context nonce to fail.

No dismissal occurred because alert disposition is outside this triage slice and requires maintainer review. No remediation occurred because the static path does not carry the zero initialization into the constructed proof context; changing runtime code solely to silence the scanner is not supported by the evidence.

Recommended next action: a maintainer may review the scanner result and its query precision in a separately authorized alert-management pass. If a deployed process forks, separately verify that its RNG reseeding behavior satisfies the deployment threat model; the imported alert does not claim or establish that path.

## Other open alerts inventoried

The live audit returned eight additional open instances of the same rule. They were inventoried but not assessed in detail here.

| Alert | Reported use | Location | Detailed classification |
|---|---|---|---|
| `#29` | Password | `icn/crates/icn-gateway/src/email_client.rs:853` | Not assessed in this slice |
| `#30` | Nonce | `icn/crates/icn-gossip/src/handlers/blob_nonce_guard.rs:212` | Not assessed in this slice |
| `#31` | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:767` | Not assessed in this slice |
| `#32` | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:792` | Not assessed in this slice |
| `#33` | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:860` | Not assessed in this slice |
| `#34` | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:889` | Not assessed in this slice |
| `#35` | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:893` | Not assessed in this slice |
| `#37` | Nonce | `icn/crates/icn-steward/src/token.rs:325` | Not assessed in this slice |

Recommended next action: triage these alerts individually against their product surfaces and nearby controls. Do not infer their disposition from alerts #100 and #101 merely because the rule ID matches.

## Nonclaims

- This does not dismiss any CodeQL/code-scanning alert.
- This does not remediate any vulnerability.
- This does not complete security hardening.
- This does not establish production readiness.
- This does not make ICN pilot-ready, organizer-ready, member-ready, or live-federated.
- This does not complete Phase 2.
- This does not complete #2041.
- This does not claim #2082 or #2113 completion.
- This does not change runtime behavior, CodeQL setup, branch protection, or repository security settings.
