//! Governance meaning for the EXPERIMENTAL authority-to-effect spine v0 (`icn_authz::effect_v0`).
//!
//! - [`GovernanceActV0`]: the bytes a voter's device signs, carried as the opaque
//!   `DeviceActV1.payload` (capability `Sign`). It binds the vote to one context and one proposal.
//! - [`ThresholdRuleV0`]: an [`AdoptedRule`] that counts one act per Subject, enforces quorum over
//!   the eligible set and approval over deciding votes, and refuses duplicates and unbound acts.
//!
//! The tally semantics match [`crate::tally::VoteTally`]: Abstain counts toward quorum only;
//! approval is `For / (For + Against)`.
//!
//! This module does **not** touch the live path: `Vote`, `vote_cast` and the governance actor are
//! keyed by legacy `Did`, while these acts are keyed by N4 `SubjectId`. No bridge is implied.

use std::collections::BTreeSet;

use icn_authz::effect_v0::{AdoptedRule, RuleBinding, RuleError, Verdict};
use icn_identity::authority_log::SubjectId;
use icn_identity::device_authority::VerifiedDeviceAct;
use sha2::{Digest, Sha256};

use crate::vote::VoteChoice;

/// Domain of [`GovernanceActV0`] bytes.
pub const GOVERNANCE_ACT_DOMAIN: &[u8] = b"icn.governance.authz-act.v0";
/// Domain of [`ThresholdRuleV0`] artifacts.
pub const THRESHOLD_RULE_DOMAIN: &[u8] = b"icn.governance.threshold-rule.v0";
/// Version of both v0 encodings.
pub const GOVERNANCE_AUTHZ_V0_VERSION: u16 = 0;
/// Maximum bytes of the rule's role label (matches `icn_authz::effect_v0::MAX_ROLE_LEN`).
pub const MAX_RULE_ROLE_LEN: usize = icn_authz::effect_v0::MAX_ROLE_LEN;

/// Why governance v0 bytes did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GovernanceAuthzCodecError {
    /// Wrong length, domain, version, or trailing bytes.
    #[error("malformed framing")]
    Framing,
    /// A field held a value outside its range.
    #[error("invalid field value: {0}")]
    InvalidValue(&'static str),
}

fn choice_tag(choice: VoteChoice) -> u8 {
    match choice {
        VoteChoice::For => 1,
        VoteChoice::Against => 2,
        VoteChoice::Abstain => 3,
    }
}

fn choice_from_tag(tag: u8) -> Result<VoteChoice, GovernanceAuthzCodecError> {
    match tag {
        1 => Ok(VoteChoice::For),
        2 => Ok(VoteChoice::Against),
        3 => Ok(VoteChoice::Abstain),
        _ => Err(GovernanceAuthzCodecError::InvalidValue("choice")),
    }
}

/// Fixed prefix: `u32be(len(domain)) || domain || u16be(version)`.
fn header(domain: &[u8]) -> Vec<u8> {
    let mut out = (domain.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(domain);
    out.extend_from_slice(&GOVERNANCE_AUTHZ_V0_VERSION.to_be_bytes());
    out
}

fn strip_header<'a>(bytes: &'a [u8], domain: &[u8]) -> Result<&'a [u8], GovernanceAuthzCodecError> {
    bytes
        .strip_prefix(header(domain).as_slice())
        .ok_or(GovernanceAuthzCodecError::Framing)
}

fn b32(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    out
}

/// A vote, as signed by a voter's device inside `DeviceActV1.payload`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GovernanceActV0 {
    /// The `ContextRef` bytes of the deciding context.
    pub context: [u8; 32],
    /// The proposal (the authorization's `subject_matter`).
    pub proposal: [u8; 32],
    /// The choice.
    pub choice: VoteChoice,
    /// Distinguishes otherwise identical acts. Not a replay defence by itself.
    pub act_nonce: [u8; 32],
}

impl GovernanceActV0 {
    const BODY_LEN: usize = 32 + 32 + 1 + 32;

    /// Canonical bytes.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = header(GOVERNANCE_ACT_DOMAIN);
        out.extend_from_slice(&self.context);
        out.extend_from_slice(&self.proposal);
        out.push(choice_tag(self.choice));
        out.extend_from_slice(&self.act_nonce);
        out
    }

    /// Strict decode.
    pub fn decode(bytes: &[u8]) -> Result<Self, GovernanceAuthzCodecError> {
        let body = strip_header(bytes, GOVERNANCE_ACT_DOMAIN)?;
        if body.len() != Self::BODY_LEN {
            return Err(GovernanceAuthzCodecError::Framing);
        }
        Ok(GovernanceActV0 {
            context: b32(&body[0..32]),
            proposal: b32(&body[32..64]),
            choice: choice_from_tag(body[64])?,
            act_nonce: b32(&body[65..97]),
        })
    }
}

/// A ratio `num / den` with `num <= den` and `den > 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    /// Numerator.
    pub num: u16,
    /// Denominator, non-zero.
    pub den: u16,
}

impl Ratio {
    fn valid(self) -> bool {
        self.den > 0 && self.num <= self.den
    }

    /// `part / whole >= num / den`, exactly, in integers.
    fn met_by(self, part: usize, whole: usize) -> bool {
        (part as u128) * u128::from(self.den) >= u128::from(self.num) * (whole as u128)
    }
}

/// Quorum over eligible Subjects and approval over deciding votes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThresholdRuleV0 {
    /// The standing role a voter must hold.
    pub role: String,
    /// Minimum `(For + Against + Abstain) / eligible`.
    pub quorum: Ratio,
    /// Minimum `For / (For + Against)`; numerator must be non-zero.
    pub approval: Ratio,
}

impl ThresholdRuleV0 {
    fn validate(&self) -> Result<(), RuleError> {
        if self.role.len() > MAX_RULE_ROLE_LEN {
            return Err(RuleError::MalformedArtifact("role too long"));
        }
        if !self.quorum.valid() {
            return Err(RuleError::MalformedArtifact("quorum"));
        }
        if !self.approval.valid() || self.approval.num == 0 {
            return Err(RuleError::MalformedArtifact("approval"));
        }
        Ok(())
    }

    /// The canonical artifact; `rule_hash = SHA-256(artifact)`.
    pub fn artifact(&self) -> Vec<u8> {
        let mut out = header(THRESHOLD_RULE_DOMAIN);
        out.extend_from_slice(&(self.role.len() as u32).to_be_bytes());
        out.extend_from_slice(self.role.as_bytes());
        for v in [
            self.quorum.num,
            self.quorum.den,
            self.approval.num,
            self.approval.den,
        ] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out
    }

    /// Strict decode of an artifact.
    pub fn from_artifact(bytes: &[u8]) -> Result<Self, RuleError> {
        let framing = RuleError::MalformedArtifact("framing");
        let body = strip_header(bytes, THRESHOLD_RULE_DOMAIN).map_err(|_| framing.clone())?;
        let (len, rest) = body.split_first_chunk::<4>().ok_or(framing.clone())?;
        let len = u32::from_be_bytes(*len) as usize;
        if len > MAX_RULE_ROLE_LEN || rest.len() != len + 8 {
            return Err(framing);
        }
        let role = String::from_utf8(rest[..len].to_vec())
            .map_err(|_| RuleError::MalformedArtifact("role utf-8"))?;
        let n = |i: usize| u16::from_be_bytes([rest[len + i], rest[len + i + 1]]);
        let rule = ThresholdRuleV0 {
            role,
            quorum: Ratio {
                num: n(0),
                den: n(2),
            },
            approval: Ratio {
                num: n(4),
                den: n(6),
            },
        };
        rule.validate()?;
        Ok(rule)
    }
}

impl AdoptedRule for ThresholdRuleV0 {
    fn rule_hash(&self) -> [u8; 32] {
        Sha256::digest(self.artifact()).into()
    }

    fn required_role(&self) -> &str {
        &self.role
    }

    fn evaluate(
        &self,
        binding: &RuleBinding,
        eligible: &BTreeSet<SubjectId>,
        acts: &[VerifiedDeviceAct],
    ) -> Result<Verdict, RuleError> {
        self.validate()?;
        let (mut for_votes, mut against, mut abstain) = (0usize, 0usize, 0usize);
        let mut voted = BTreeSet::new();
        for act in acts {
            let act_id = act.act_id();
            let subject = act.act().subject;
            if !eligible.contains(&subject) {
                return Err(RuleError::IneligibleSubject(subject));
            }
            let vote = GovernanceActV0::decode(&act.act().payload)
                .map_err(|_| RuleError::MalformedAct { act_id })?;
            if vote.context != binding.context.0 || vote.proposal != binding.subject_matter {
                return Err(RuleError::ActNotBound { act_id });
            }
            // One act per Subject. A second act is refused, not silently dropped: which of two
            // acts "counts" would otherwise be the writer's choice.
            if !voted.insert(subject) {
                return Err(RuleError::DuplicateSubject(subject));
            }
            match vote.choice {
                VoteChoice::For => for_votes += 1,
                VoteChoice::Against => against += 1,
                VoteChoice::Abstain => abstain += 1,
            }
        }
        let participated = for_votes + against + abstain;
        let deciding = for_votes + against;
        let pass = !eligible.is_empty()
            && self.quorum.met_by(participated, eligible.len())
            && deciding > 0
            && self.approval.met_by(for_votes, deciding);
        Ok(if pass { Verdict::Pass } else { Verdict::Fail })
    }
}
