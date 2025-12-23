//! Governance gossip messages for distributed coordination

use crate::{Delegation, DelegationId, GovernanceDomain, Proposal, ProposalId, Timestamp, Vote};
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Governance message types for gossip protocol
///
/// These messages are broadcast on the `governance:proposal` topic
/// to coordinate distributed decision-making.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceMessage {
    /// A new governance domain has been created
    DomainCreated {
        /// The domain that was created
        domain: GovernanceDomain,
    },

    /// A domain's configuration has been updated
    DomainUpdated {
        /// The updated domain
        domain: GovernanceDomain,
    },

    /// A new proposal has been created
    ProposalCreated {
        /// The proposal
        proposal: Proposal,
    },

    /// A proposal has been opened for voting
    ProposalOpened {
        /// Proposal ID
        id: ProposalId,

        /// When voting opened
        opened_at: u64,

        /// When voting closes
        closes_at: u64,
    },

    /// A vote has been cast on a proposal
    VoteCast {
        /// The vote
        vote: Vote,

        /// Optional signature from the voter (for verification)
        signature: Option<Vec<u8>>,
    },

    /// A proposal has been closed with a final result
    ProposalClosed {
        /// Proposal ID
        id: ProposalId,

        /// Final state (Accepted, Rejected, NoQuorum)
        outcome: ProposalOutcome,

        /// When the proposal was closed
        closed_at: u64,

        /// Vote tally snapshot
        tally: TallySnapshot,
    },

    /// A proposal has been cancelled
    ProposalCancelled {
        /// Proposal ID
        id: ProposalId,

        /// Who cancelled it (should be the proposer)
        cancelled_by: Did,

        /// When it was cancelled
        cancelled_at: u64,
    },

    /// A new vote delegation has been created
    DelegationCreated {
        /// The delegation
        delegation: Delegation,
    },

    /// A vote delegation has been revoked
    DelegationRevoked {
        /// Delegation ID
        id: DelegationId,

        /// Who revoked it (the delegator)
        revoked_by: Did,

        /// When it was revoked
        revoked_at: Timestamp,
    },
}

/// Outcome of a closed proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalOutcome {
    Accepted,
    Rejected,
    NoQuorum,
}

/// Snapshot of vote tally at closure time
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TallySnapshot {
    pub for_votes: usize,
    pub against_votes: usize,
    pub abstain_votes: usize,
    pub eligible_voters: usize,
}

impl GovernanceMessage {
    /// Create a DomainCreated message
    pub fn domain_created(domain: GovernanceDomain) -> Self {
        Self::DomainCreated { domain }
    }

    /// Create a DomainUpdated message
    pub fn domain_updated(domain: GovernanceDomain) -> Self {
        Self::DomainUpdated { domain }
    }

    /// Create a ProposalCreated message
    pub fn proposal_created(proposal: Proposal) -> Self {
        Self::ProposalCreated { proposal }
    }

    /// Create a ProposalOpened message
    pub fn proposal_opened(id: ProposalId, opened_at: u64, closes_at: u64) -> Self {
        Self::ProposalOpened {
            id,
            opened_at,
            closes_at,
        }
    }

    /// Create a VoteCast message
    pub fn vote_cast(vote: Vote, signature: Option<Vec<u8>>) -> Self {
        Self::VoteCast { vote, signature }
    }

    /// Create a ProposalClosed message
    pub fn proposal_closed(
        id: ProposalId,
        outcome: ProposalOutcome,
        closed_at: u64,
        tally: TallySnapshot,
    ) -> Self {
        Self::ProposalClosed {
            id,
            outcome,
            closed_at,
            tally,
        }
    }

    /// Create a ProposalCancelled message
    pub fn proposal_cancelled(id: ProposalId, cancelled_by: Did, cancelled_at: u64) -> Self {
        Self::ProposalCancelled {
            id,
            cancelled_by,
            cancelled_at,
        }
    }

    /// Create a DelegationCreated message
    pub fn delegation_created(delegation: Delegation) -> Self {
        Self::DelegationCreated { delegation }
    }

    /// Create a DelegationRevoked message
    pub fn delegation_revoked(id: DelegationId, revoked_by: Did, revoked_at: Timestamp) -> Self {
        Self::DelegationRevoked {
            id,
            revoked_by,
            revoked_at,
        }
    }

    /// Get a short description of this message type for logging
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::DomainCreated { .. } => "DomainCreated",
            Self::DomainUpdated { .. } => "DomainUpdated",
            Self::ProposalCreated { .. } => "ProposalCreated",
            Self::ProposalOpened { .. } => "ProposalOpened",
            Self::VoteCast { .. } => "VoteCast",
            Self::ProposalClosed { .. } => "ProposalClosed",
            Self::ProposalCancelled { .. } => "ProposalCancelled",
            Self::DelegationCreated { .. } => "DelegationCreated",
            Self::DelegationRevoked { .. } => "DelegationRevoked",
        }
    }

    /// Serialize to bytes for gossip transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize from bytes received via gossip
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl TallySnapshot {
    /// Create a tally snapshot
    pub fn new(
        for_votes: usize,
        against_votes: usize,
        abstain_votes: usize,
        eligible_voters: usize,
    ) -> Self {
        Self {
            for_votes,
            against_votes,
            abstain_votes,
            eligible_voters,
        }
    }

    /// Total votes cast
    pub fn total_votes(&self) -> usize {
        self.for_votes + self.against_votes + self.abstain_votes
    }

    /// Calculate quorum percentage (votes cast / eligible voters * 100)
    pub fn quorum_percentage(&self) -> u8 {
        if self.eligible_voters == 0 {
            return 0;
        }
        ((self.total_votes() * 100) / self.eligible_voters) as u8
    }

    /// Calculate approval percentage (for votes / deciding votes * 100)
    pub fn approval_percentage(&self) -> u8 {
        let deciding = self.for_votes + self.against_votes;
        if deciding == 0 {
            return 0;
        }
        ((self.for_votes * 100) / deciding) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GovernanceConfig, VoteChoice};
    use icn_identity::KeyPair;

    #[test]
    fn test_message_serialization() {
        let kp = KeyPair::generate().unwrap();
        let _did = kp.did().clone();
        let config = GovernanceConfig::cooperative_default();
        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let msg = GovernanceMessage::domain_created(domain);

        let bytes = msg.to_bytes().unwrap();
        let deserialized = GovernanceMessage::from_bytes(&bytes).unwrap();

        assert_eq!(msg.message_type(), deserialized.message_type());
    }

    #[test]
    fn test_vote_cast_message() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let proposal_id = ProposalId::generate();

        let vote = Vote::new(proposal_id, did, VoteChoice::For);
        let msg = GovernanceMessage::vote_cast(vote.clone(), None);

        match msg {
            GovernanceMessage::VoteCast { vote: v, signature } => {
                assert_eq!(v.choice, VoteChoice::For);
                assert!(signature.is_none());
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_proposal_closed_message() {
        let proposal_id = ProposalId::generate();
        let tally = TallySnapshot::new(60, 30, 10, 100);

        let msg = GovernanceMessage::proposal_closed(
            proposal_id.clone(),
            ProposalOutcome::Accepted,
            12345678,
            tally.clone(),
        );

        match msg {
            GovernanceMessage::ProposalClosed {
                id,
                outcome,
                tally: t,
                ..
            } => {
                assert_eq!(id, proposal_id);
                assert_eq!(outcome, ProposalOutcome::Accepted);
                assert_eq!(t.for_votes, 60);
                assert_eq!(t.total_votes(), 100);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_tally_snapshot_calculations() {
        let tally = TallySnapshot::new(60, 30, 10, 100);

        assert_eq!(tally.total_votes(), 100);
        assert_eq!(tally.quorum_percentage(), 100); // 100/100 = 100%
        assert_eq!(tally.approval_percentage(), 66); // 60/90 = 66%
    }

    #[test]
    fn test_tally_snapshot_no_voters() {
        let tally = TallySnapshot::new(0, 0, 0, 0);

        assert_eq!(tally.total_votes(), 0);
        assert_eq!(tally.quorum_percentage(), 0);
        assert_eq!(tally.approval_percentage(), 0);
    }

    #[test]
    fn test_message_type_names() {
        let config = GovernanceConfig::cooperative_default();
        let domain = GovernanceDomain::new("Test".to_string(), config);

        assert_eq!(
            GovernanceMessage::domain_created(domain).message_type(),
            "DomainCreated"
        );
        assert_eq!(
            GovernanceMessage::proposal_opened(ProposalId::generate(), 0, 100).message_type(),
            "ProposalOpened"
        );
    }
}
