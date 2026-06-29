---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-29
---

# CodeQL triage closeout — 2026-06-29

This is a point-in-time, static closeout record for the CodeQL alerts that were open when inspected. It consolidates the two earlier triage records, completes detailed triage for alerts #29 and #37, and gives maintainers a disposition checklist. It does not change alert state or runtime behavior.

## Inspection basis

- Repository commit inspected: `ae70ddb6bae868e3ecd4a37e02262a0e61282bc6`.
- Most recent alert instances were on `refs/heads/main` at `cf898d5714b59bbf1279a731fb7e548feb33eace`; the affected source files did not change between that commit and the inspected commit.
- Open-alert inventory: GitHub REST `GET /repos/InterCooperative-Network/icn/code-scanning/alerts`, paginated and filtered to `state == "open"`; details and all instances were fetched for every returned alert.
- GitHub CodeQL default setup was configured to analyze Actions, JavaScript/TypeScript, Python, and Rust. No repository-owned advanced CodeQL workflow was present or added.
- Repository policy basis: [`SECURITY.md`](../../SECURITY.md) places the Rust workspace, including the gateway and steward crates, in scope.
- Assessment method: static alert metadata, source, compilation boundaries, call sites, history, manifests, existing merged triage, and nearby controls. No runtime validation or CodeQL query-engine inspection was performed.
- No alert was dismissed or otherwise modified. No repository setting, workflow, branch-protection rule, or security setting changed.

## Open-alert inventory

All ten alerts were open and undismissed at inspection time. All use CodeQL rule `rust/hard-coded-cryptographic-value` with `critical` scanner severity.

| Alert | Reported use | Location | State | Existing detailed record | Current disposition bucket |
|---|---|---|---|---|---|
| [#101](https://github.com/InterCooperative-Network/icn/security/code-scanning/101) | Nonce | `icn/crates/icn-zkp/src/types.rs:126` | Open, undismissed | [Alerts #100/#101 triage](codeql-alert-triage-2026-06-29.md) | False-positive candidate; `not_actionable`, high confidence |
| [#100](https://github.com/InterCooperative-Network/icn/security/code-scanning/100) | Salt | `icn/crates/icn-steward/src/enrollment.rs:97` | Open, undismissed | [Alerts #100/#101 triage](codeql-alert-triage-2026-06-29.md) | False-positive candidate; `not_actionable`, high confidence |
| [#37](https://github.com/InterCooperative-Network/icn/security/code-scanning/37) | Nonce | `icn/crates/icn-steward/src/token.rs:325` | Open, undismissed | This document | Test-only finding; `not_actionable`, high confidence |
| [#35](https://github.com/InterCooperative-Network/icn/security/code-scanning/35) | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:893` | Open, undismissed | [Gossip nonce triage](codeql-gossip-nonce-triage-2026-06-29.md) | Test-only finding; `not_actionable`, high confidence |
| [#34](https://github.com/InterCooperative-Network/icn/security/code-scanning/34) | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:889` | Open, undismissed | [Gossip nonce triage](codeql-gossip-nonce-triage-2026-06-29.md) | Test-only finding; `not_actionable`, high confidence |
| [#33](https://github.com/InterCooperative-Network/icn/security/code-scanning/33) | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:860` | Open, undismissed | [Gossip nonce triage](codeql-gossip-nonce-triage-2026-06-29.md) | Test-only finding; `not_actionable`, high confidence |
| [#32](https://github.com/InterCooperative-Network/icn/security/code-scanning/32) | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:792` | Open, undismissed | [Gossip nonce triage](codeql-gossip-nonce-triage-2026-06-29.md) | Test-only finding; `not_actionable`, high confidence |
| [#31](https://github.com/InterCooperative-Network/icn/security/code-scanning/31) | Nonce | `icn/crates/icn-gossip/src/handlers/blob_transfer.rs:767` | Open, undismissed | [Gossip nonce triage](codeql-gossip-nonce-triage-2026-06-29.md) | Test-only finding; `not_actionable`, high confidence |
| [#30](https://github.com/InterCooperative-Network/icn/security/code-scanning/30) | Nonce | `icn/crates/icn-gossip/src/handlers/blob_nonce_guard.rs:212` | Open, undismissed | [Gossip nonce triage](codeql-gossip-nonce-triage-2026-06-29.md) | Test-only finding; `not_actionable`, high confidence |
| [#29](https://github.com/InterCooperative-Network/icn/security/code-scanning/29) | Password | `icn/crates/icn-gateway/src/email_client.rs:853` | Open, undismissed | This document | Test-only finding; `not_actionable`, high confidence |

## Detailed triage: alert #29 — fake SMTP credential fixture

| Field | Assessment |
|---|---|
| CodeQL report | A hard-coded value is used as a password. |
| Reported source | `SmtpConfig::sendgrid("api-key", ...)` at `email_client.rs:853` |
| Sink | `SmtpConfig::sendgrid` passes its `api_key` parameter into the SMTP password field used by `lettre::Credentials` |
| Classification | Test-only finding |
| Verdict | `not_actionable` with high static confidence |
| Product boundary | The reported literal is below `#[cfg(test)]` at `email_client.rs:847` and is not compiled into the shipped gateway |

The literal is an obvious fake credential in `test_smtp_config_sendgrid`. The test checks SendGrid provider defaults—host, port, and username—and does not perform SMTP delivery. Repository-wide call-site inspection found `SmtpConfig::sendgrid` only in this test. The shipped notification processor instead reads the SMTP password from the operator-provided `ICN_SMTP_PASSWORD` environment variable and passes it to `SmtpConfig::new`.

The production credential path is real and in scope, but it does not originate at the reported hard-coded string. Replacing this transparent test fixture with runtime secret loading or randomized text would not improve shipped behavior.

Recommended next action: maintainers may review #29 for a policy-consistent test/false-positive disposition. No runtime remediation is indicated by this alert's reported source.

## Detailed triage: alert #37 — response nonce fixture

| Field | Assessment |
|---|---|
| CodeQL report | A hard-coded value is used as a nonce. |
| Reported source | `let nonce = [9u8; 16]` at `token.rs:325` |
| Sink | `TokenResponse::new(..., nonce)` at `token.rs:327` |
| Classification | Test-only fixture/sentinel finding |
| Verdict | `not_actionable` with high static confidence |
| Product boundary | The reported literal is below `#[cfg(test)]` at `token.rs:243` and is not compiled into the shipped steward crate |

The fixed value is local to `test_token_response`, which asserts that the response preserves the supplied correlation nonce. Repository-wide call-site inspection found the fixed `TokenResponse::new` call only in this test. In shipped code, `TokenRequest::new` allocates a 16-byte nonce and fills the entire buffer with `rand_core::OsRng`; `TokenResponse` accepts the original request nonce for correlation rather than generating a new nonce.

The public response type and correlation field are shipped, but the reported hard-coded source is not. Randomizing the equality fixture would add no product security property and would make the field-preservation test less explicit.

Recommended next action: maintainers may review #37 for a policy-consistent test/false-positive disposition. No runtime remediation is indicated by this alert's reported source.

## Previously triaged alerts

- Alerts #100 and #101 remain covered by [CodeQL alert triage — 2026-06-29](codeql-alert-triage-2026-06-29.md). In both cases a zero-initialized destination buffer is fully overwritten by an RNG before cryptographic use; both remain `not_actionable` with high static confidence.
- Alerts #30–#35 remain covered by [CodeQL gossip nonce triage — 2026-06-29](codeql-gossip-nonce-triage-2026-06-29.md). Every reported literal is below `#[cfg(test)]` and intentionally selects duplicate, distinct, expiry, or sender-mismatch behavior; all remain `not_actionable` with high static confidence.
- The affected source for these eight alerts did not change after their merged triage records. Their live alert states remained open and undismissed.

## Maintainer disposition checklist

This checklist is advisory. It does not perform or require an alert-state change.

- [ ] Review #100 and #101 as likely false-positive dispositions: the zero initializers are overwritten before use.
- [ ] Review #29, #30–#35, and #37 as likely test/false-positive dispositions: the reported literals are excluded from shipped builds by `#[cfg(test)]`.
- [ ] If changing alert state, verify the current default-branch instance and select a reason consistent with repository security policy and the evidence linked above.
- [ ] Record any administrative disposition separately from code remediation; this pass found no alert needing a runtime patch.
- [ ] Re-run the live inventory after any later CodeQL analysis because this record is point-in-time evidence, not a durable assertion that the alert set remains unchanged.

At inspection time:

- Likely eligible for maintainer false-positive/test review: #29–#35, #37, #100, and #101.
- Needs remediation based on this static evidence: none.
- Needs runtime validation to decide the imported claim: none.
- Changed naturally outside this documentation pass: none.

## Proof gaps

- No CodeQL query-engine trace was inspected beyond live alert and instance metadata.
- No runtime, SMTP, token-flow, unit, integration, or human AT validation was performed.
- No alert was dismissed, so the checklist does not verify GitHub administrative reason selection or post-dismissal behavior.
- The inventory may change after later CodeQL analyses; the exact date, commit, and instance commit above bound this record.

## Recommended next actions

1. Have a maintainer independently review the ten evidence-backed disposition candidates before any alert administration.
2. Keep alert administration separate from runtime remediation and record the chosen reason per alert.
3. Continue to treat future CodeQL findings as new inputs requiring their own source, sink, boundary, and reachability assessment.
4. Keep human accessibility testing under #2041 and broader acceptance work under #1727 and #1746 separate from this security-documentation record.

## Nonclaims

- This does not dismiss any CodeQL/code-scanning alert.
- This does not remediate any vulnerability.
- This does not complete security hardening.
- This does not establish production readiness.
- This does not make ICN pilot-ready, organizer-ready, member-ready, or live-federated.
- This does not complete Phase 2 or #2041.
- This does not claim #2082 or #2113 completion.
- This does not change CodeQL setup, workflows, branch protection, repository settings, repository security settings, or runtime behavior.
- Human AT remains open under #2041.
- Maintainer alert disposition remains a separate administrative and security action.
