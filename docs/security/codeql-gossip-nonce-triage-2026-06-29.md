---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-29
---

# CodeQL gossip nonce triage — 2026-06-29

This is a point-in-time, static triage record for GitHub CodeQL alerts [#30](https://github.com/InterCooperative-Network/icn/security/code-scanning/30) through [#35](https://github.com/InterCooperative-Network/icn/security/code-scanning/35). It records evidence and follow-up without changing code, alert state, CodeQL configuration, repository settings, or branch protection.

## Inspection basis

- Repository commit inspected: `e4af12c5556064b2e9fed058de3a6d778833d37f`.
- CodeQL rule for all six alerts: `rust/hard-coded-cryptographic-value` (`critical` scanner severity), reporting that a hard-coded value is used as a nonce.
- Most recent alert instances inspected: `refs/heads/main` at `76f5fcb6a3c0d04016610f0b72d0e3d162ddde19`.
- The affected source paths did not change between the instance commit and the inspected commit.
- Repository policy basis: [`SECURITY.md`](../../SECURITY.md) places the Rust workspace in scope.
- Assessment method: static source, compilation-boundary, call-site, history, and nearby-test inspection only. No runtime validation or CodeQL query-engine inspection was performed.

## Shared product boundary

Every reported literal is below an explicit `#[cfg(test)]` module boundary: `blob_nonce_guard.rs:189` for alert #30 and `blob_transfer.rs:511` for alerts #31–#35. These fixtures are compiled only for tests and are not production nonce-generation paths.

In shipped code, `GossipMessage::BlobRequest` carries `request_id` as part of the signed message, dispatch passes that field to `handle_blob_request`, and `BlobNonceGuard::check_and_record` tracks the caller-supplied identifier per sender. Blob chunks derive their replay key from `blake3(request_id || chunk_index)`. The flagged literals do not feed those paths outside test compilation.

## Detailed triage

| Alert | Reported location | Test purpose and static evidence | Classification | Verdict |
|---|---|---|---|---|
| `#30` | `icn/crates/icn-gossip/src/handlers/blob_nonce_guard.rs:212` | `duplicate_nonce_rejected` deliberately submits the same `[1u8; 32]` value twice and asserts replay rejection. | Test-only finding | `not_actionable` — high confidence |
| `#31` | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:767` | `blob_request_handler_rejects_expired` uses the fixed request ID as an incidental fixture while `expires_at = 1` selects the expired-request path. | Test-only finding | `not_actionable` — high confidence |
| `#32` | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:792` | `blob_request_handler_rejects_mismatched_sender` uses the fixed request ID as an incidental fixture; a separately generated requester mismatch selects the behavior under test. | Test-only finding | `not_actionable` — high confidence |
| `#33` | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:860` | `blob_request_handler_rejects_duplicate_request` deliberately reuses `[0x11; 32]` for two calls and asserts that only one nonce is tracked. | Test-only finding | `not_actionable` — high confidence |
| `#34` | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:889` | `blob_request_handler_allows_different_requests` uses `[0x11; 32]` as the first of two visibly distinct sentinels. | Test-only finding | `not_actionable` — high confidence |
| `#35` | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:893` | The same test uses `[0x22; 32]` as the second distinct sentinel and asserts that two nonces are tracked. | Test-only finding | `not_actionable` — high confidence |

The scanner's data flow is meaningful within test execution: each fixture reaches replay-checking code. The security disposition follows from the product boundary and intent, not from denying that test-only flow. Randomizing these fixtures would not improve shipped-code security and would make the duplicate-versus-distinct test intent less explicit.

No alert was dismissed. Alert disposition remains a separate maintainer action under repository CodeQL policy. No remediation was made because the evidence does not identify a production defect or support a runtime change.

Recommended next action: retain these fixtures as clear replay-test sentinels. In a separately authorized alert-management pass, maintainers may review whether CodeQL's test-file classification supports a policy-consistent disposition; this record does not prescribe or perform one.

## Explicitly out of scope

Alerts [#29](https://github.com/InterCooperative-Network/icn/security/code-scanning/29) and [#37](https://github.com/InterCooperative-Network/icn/security/code-scanning/37) were not assessed. Their presence under the same CodeQL rule does not imply the disposition recorded for #30–#35.

## Proof gaps

- No CodeQL query-engine trace was inspected beyond the live alert and instance metadata.
- No unit, integration, or runtime tests were executed during static triage.
- This assessment does not evaluate unrelated request-ID design questions or other hard-coded-value alerts.

## Nonclaims

- This does not dismiss any CodeQL/code-scanning alert.
- This does not remediate any vulnerability.
- This does not complete security hardening.
- This does not establish production readiness.
- This does not make ICN pilot-ready, organizer-ready, member-ready, or live-federated.
- This does not complete Phase 2 or close #2041.
- This does not claim completion of #1726, #1727, #1746, #2082, or #2113.
- This does not change runtime behavior, CodeQL setup, workflows, branch protection, repository settings, or repository security settings.
