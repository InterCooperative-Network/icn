//! Vote types

use crate::proposal::ProposalId;
use crate::Timestamp;
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// A voter's choice on a proposal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteChoice {
    /// Vote in favor
    For,

    /// Vote against
    Against,

    /// Abstain from voting (counted for quorum but not for/against)
    Abstain,
}

/// A vote on a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// Proposal being voted on
    pub proposal_id: ProposalId,

    /// DID of the voter
    pub voter: Did,

    /// The vote choice
    pub choice: VoteChoice,

    /// Weight of this vote (default: 1 for 1-member-1-vote)
    pub weight: u64,

    /// When the vote was cast
    pub timestamp: Timestamp,

    /// Optional justification/comment
    pub comment: Option<String>,
}

impl Vote {
    /// Create a new vote
    pub fn new(proposal_id: ProposalId, voter: Did, choice: VoteChoice) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            proposal_id,
            voter,
            choice,
            weight: 1, // 1-member-1-vote
            timestamp: now,
            comment: None,
        }
    }

    /// Add a comment to the vote
    pub fn with_comment(mut self, comment: String) -> Self {
        self.comment = Some(comment);
        self
    }

    /// Set custom vote weight (for future weighted voting)
    pub fn with_weight(mut self, weight: u64) -> Self {
        self.weight = weight;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_vote_creation() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let proposal_id = ProposalId::generate();

        let vote = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::For);

        assert_eq!(vote.proposal_id, proposal_id);
        assert_eq!(vote.voter, did);
        assert_eq!(vote.choice, VoteChoice::For);
        assert_eq!(vote.weight, 1);
        assert!(vote.timestamp > 0);
        assert!(vote.comment.is_none());
    }

    #[test]
    fn test_vote_with_comment() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let proposal_id = ProposalId::generate();

        let vote = Vote::new(proposal_id, did, VoteChoice::Against)
            .with_comment("I disagree with this approach".to_string());

        assert_eq!(vote.choice, VoteChoice::Against);
        assert_eq!(
            vote.comment,
            Some("I disagree with this approach".to_string())
        );
    }

    #[test]
    fn test_vote_with_weight() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let proposal_id = ProposalId::generate();

        let vote = Vote::new(proposal_id, did, VoteChoice::For).with_weight(5);

        assert_eq!(vote.weight, 5);
    }
}
