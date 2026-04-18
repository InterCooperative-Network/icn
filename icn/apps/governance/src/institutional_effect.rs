//! Durable artifact of an accepted proposal's translated institutional effect.
//!
//! `InstitutionalEffectRecord` is the governance-app answer to the question
//! "what artifact did this decision actually create?" It is NOT a dispatch
//! receipt (that lives downstream — e.g. commons steward registration, ledger
//! freeze status) and it is NOT a cryptographic proof (see
//! `GovernanceDecisionReceipt` / `GovernanceProof`). It is the app-layer
//! record that translation from [`crate::http::configure::GovernanceEffect`]
//! occurred at acceptance time, what its targets and reason were, and when.
//!
//! Economic effects (Budget, Treasury, Allocation, SurplusAllocation) are
//! already recorded as `AllocationReceipt`s and are NOT duplicated here.
//! This record type covers the non-economic translation targets:
//!
//! - `freeze_member`, `unfreeze_member`
//! - `deploy_charter`
//! - `appoint_steward`, `revoke_steward`
//!
//! `Unhandled` payloads are never persisted.

use icn_kernel_api::receipts::Hash;
use serde::{Deserialize, Serialize};

/// Persisted artifact of an accepted proposal's translated institutional effect.
///
/// One record per accepted proposal that translates to a structured
/// `GovernanceEffect`. Keyed by `record_id` (uuid). Indexed by `proposal_id`
/// for read-model retrieval.
///
/// `decision_hash` is `Some` when the governance decision receipt was
/// available at record-persistence time, which binds this record into the
/// INV-5 provenance chain. It is `None` only in edge cases where the
/// governance receipt lookup fails at record time — the proposal_id and
/// recorded_at still locate the record unambiguously.
///
/// `payload` carries the full translation details so audit consumers don't
/// need a case-split on `effect_kind` to recover context. The typed fields
/// (`target_did`, `target_ref`, `reason`) are duplicated out of `payload`
/// for cheap read-model queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstitutionalEffectRecord {
    /// Stable identifier (uuidv4) for this record.
    pub record_id: String,
    /// Proposal whose acceptance emitted this record.
    pub proposal_id: String,
    /// Governance domain the proposal ran in.
    pub domain_id: String,
    /// Hash of the governance decision receipt, if available at record time.
    /// Links this record into the INV-5 provenance chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<Hash>,
    /// Lowercase snake_case effect label, matching
    /// `crate::http::configure::GovernanceEffect` variants:
    /// `"freeze_member"`, `"unfreeze_member"`, `"deploy_charter"`,
    /// `"appoint_steward"`, `"revoke_steward"`.
    pub effect_kind: String,
    /// DID affected by the effect (frozen/unfrozen member, appointed/revoked
    /// steward). `None` for payloads where no member is targeted (e.g. charter
    /// deployment).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_did: Option<String>,
    /// Secondary targeting reference — charter_id for deploy_charter, region
    /// for appoint_steward, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    /// Textual reason carried by the proposal payload, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Unix seconds when the record was written.
    pub recorded_at: u64,
    /// Full translation details as structured JSON. Consumers that need
    /// specifics beyond the typed fields (e.g. bond_amount, term_length,
    /// duration_seconds) read from here.
    pub payload: serde_json::Value,
}

impl InstitutionalEffectRecord {
    /// Construct a new record with a freshly generated `record_id`.
    pub fn new(
        proposal_id: impl Into<String>,
        domain_id: impl Into<String>,
        decision_hash: Option<Hash>,
        effect_kind: impl Into<String>,
        target_did: Option<String>,
        target_ref: Option<String>,
        reason: Option<String>,
        recorded_at: u64,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            record_id: uuid::Uuid::new_v4().to_string(),
            proposal_id: proposal_id.into(),
            domain_id: domain_id.into(),
            decision_hash,
            effect_kind: effect_kind.into(),
            target_did,
            target_ref,
            reason,
            recorded_at,
            payload,
        }
    }
}

/// Translate an accepted proposal's payload into the record to persist.
///
/// Returns `None` for payload types that do not produce a structured
/// institutional effect record:
///
/// - `Text`, `Membership`, `ConfigChange`, `SchedulingPolicy`,
///   `VetoProposal`, `ForceCloseProposal`, `RollbackLedger`,
///   `DisputeResolution`, `ProtocolUpgrade`: not translated to a
///   `GovernanceEffect` variant today — `Unhandled`, no record.
/// - `Budget`, `Treasury`, `Allocation`, `SurplusAllocation`: already
///   persisted as `AllocationReceipt`; not duplicated here.
/// - `Sdis` variants other than `AppointSteward` / `RemoveSteward`:
///   not yet wired.
///
/// Kept in lockstep with the match in `http::handlers::close_proposal` and
/// with `payload_effect_kind` in `manager.rs`. If that mapping changes,
/// this must change with it.
pub fn record_from_accepted_payload(
    proposal_id: &str,
    domain_id: &str,
    decision_hash: Option<Hash>,
    payload: &icn_governance::ProposalPayload,
    recorded_at: u64,
) -> Option<InstitutionalEffectRecord> {
    use icn_governance::ProposalPayload;

    match payload {
        ProposalPayload::FreezeMember {
            member,
            reason,
            duration_seconds,
        } => {
            let payload_json = serde_json::json!({
                "member": member.to_string(),
                "reason": reason,
                "duration_seconds": duration_seconds,
            });
            Some(InstitutionalEffectRecord::new(
                proposal_id,
                domain_id,
                decision_hash,
                "freeze_member",
                Some(member.to_string()),
                None,
                Some(reason.clone()),
                recorded_at,
                payload_json,
            ))
        }
        ProposalPayload::UnfreezeMember { member, reason } => {
            let payload_json = serde_json::json!({
                "member": member.to_string(),
                "reason": reason,
            });
            Some(InstitutionalEffectRecord::new(
                proposal_id,
                domain_id,
                decision_hash,
                "unfreeze_member",
                Some(member.to_string()),
                None,
                Some(reason.clone()),
                recorded_at,
                payload_json,
            ))
        }
        ProposalPayload::Charter {
            charter_id,
            charter_yaml,
        } => {
            // charter_yaml can be large; store a hash reference, not the body.
            let yaml_len = charter_yaml.len();
            let payload_json = serde_json::json!({
                "charter_id": charter_id,
                "charter_yaml_bytes": yaml_len,
            });
            Some(InstitutionalEffectRecord::new(
                proposal_id,
                domain_id,
                decision_hash,
                "deploy_charter",
                None,
                Some(charter_id.clone()),
                None,
                recorded_at,
                payload_json,
            ))
        }
        ProposalPayload::Sdis {
            proposal: sdis_prop,
        } => match sdis_prop {
            icn_governance::sdis::SdisProposal::AppointSteward {
                candidate,
                bond_amount,
                term_length,
                region,
                ..
            } => {
                let payload_json = serde_json::json!({
                    "candidate": candidate.to_string(),
                    "region": region,
                    "bond_amount": bond_amount,
                    "term_length_seconds": term_length,
                });
                Some(InstitutionalEffectRecord::new(
                    proposal_id,
                    domain_id,
                    decision_hash,
                    "appoint_steward",
                    Some(candidate.to_string()),
                    Some(region.clone()),
                    None,
                    recorded_at,
                    payload_json,
                ))
            }
            icn_governance::sdis::SdisProposal::RemoveSteward {
                steward, reason, ..
            } => {
                let payload_json = serde_json::json!({
                    "steward": steward.to_string(),
                    "reason": reason,
                });
                Some(InstitutionalEffectRecord::new(
                    proposal_id,
                    domain_id,
                    decision_hash,
                    "revoke_steward",
                    Some(steward.to_string()),
                    None,
                    Some(reason.clone()),
                    recorded_at,
                    payload_json,
                ))
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use icn_governance::ProposalPayload;
    use icn_identity::Did;

    fn did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    #[test]
    fn text_payload_produces_no_record() {
        let r = record_from_accepted_payload(
            "p1",
            "d",
            None,
            &ProposalPayload::Text { body: "hi".into() },
            123,
        );
        assert!(r.is_none(), "Text is Unhandled → no record");
    }

    #[test]
    fn budget_payload_produces_no_record() {
        let r = record_from_accepted_payload(
            "p1",
            "d",
            None,
            &ProposalPayload::Budget {
                amount: 1,
                currency: "HOURS".into(),
                recipient: did(1),
                purpose: "x".into(),
            },
            123,
        );
        assert!(
            r.is_none(),
            "Budget already has AllocationReceipt — no duplicate institutional record",
        );
    }

    #[test]
    fn freeze_member_populates_typed_fields_and_payload() {
        let target = did(7);
        let r = record_from_accepted_payload(
            "prop-1",
            "coop-a",
            None,
            &ProposalPayload::FreezeMember {
                member: target.clone(),
                reason: "bad behavior".into(),
                duration_seconds: Some(3600),
            },
            1000,
        )
        .expect("FreezeMember must produce a record");

        assert_eq!(r.proposal_id, "prop-1");
        assert_eq!(r.domain_id, "coop-a");
        assert_eq!(r.effect_kind, "freeze_member");
        assert_eq!(r.target_did.as_deref(), Some(target.to_string().as_str()));
        assert_eq!(r.reason.as_deref(), Some("bad behavior"));
        assert_eq!(r.recorded_at, 1000);
        assert_eq!(r.payload["duration_seconds"], 3600);
        assert_eq!(r.payload["reason"], "bad behavior");
    }

    #[test]
    fn deploy_charter_stores_charter_id_as_target_ref_not_yaml_body() {
        let r = record_from_accepted_payload(
            "prop-c",
            "coop-a",
            None,
            &ProposalPayload::Charter {
                charter_id: "charter-v1".into(),
                charter_yaml: "some: large yaml body".into(),
            },
            1000,
        )
        .expect("Charter must produce a record");

        assert_eq!(r.effect_kind, "deploy_charter");
        assert_eq!(r.target_ref.as_deref(), Some("charter-v1"));
        assert!(r.target_did.is_none());
        // Body is NOT duplicated — only a length reference is kept.
        assert!(r.payload.get("charter_yaml").is_none());
        assert_eq!(r.payload["charter_yaml_bytes"], 21);
    }
}
