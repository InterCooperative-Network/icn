//! Vote tallying

use crate::vote::{Vote, VoteChoice};
use serde::{Deserialize, Serialize};

/// Aggregated vote tally for a proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteTally {
    /// Number of votes in favor
    pub for_votes: usize,

    /// Number of votes against
    pub against_votes: usize,

    /// Number of abstentions
    pub abstain_votes: usize,
}

impl VoteTally {
    /// Create a new empty tally
    pub fn new(for_votes: usize, against_votes: usize, abstain_votes: usize) -> Self {
        Self {
            for_votes,
            against_votes,
            abstain_votes,
        }
    }

    /// Create an empty tally
    pub fn empty() -> Self {
        Self {
            for_votes: 0,
            against_votes: 0,
            abstain_votes: 0,
        }
    }

    /// Add a vote to the tally
    pub fn add_vote(&mut self, vote: &Vote) {
        match vote.choice {
            VoteChoice::For => self.for_votes += vote.weight as usize,
            VoteChoice::Against => self.against_votes += vote.weight as usize,
            VoteChoice::Abstain => self.abstain_votes += vote.weight as usize,
        }
    }

    /// Total number of votes cast (for + against + abstain)
    pub fn total_votes(&self) -> usize {
        self.for_votes + self.against_votes + self.abstain_votes
    }

    /// Total number of non-abstain votes (for + against)
    pub fn deciding_votes(&self) -> usize {
        self.for_votes + self.against_votes
    }

    /// Percentage of deciding votes that are "for" (0-100)
    pub fn approval_percentage(&self) -> u8 {
        let deciding = self.deciding_votes();
        if deciding == 0 {
            return 0;
        }

        ((self.for_votes * 100) / deciding) as u8
    }
}

impl From<Vec<Vote>> for VoteTally {
    fn from(votes: Vec<Vote>) -> Self {
        let mut tally = VoteTally::empty();
        for vote in votes {
            tally.add_vote(&vote);
        }
        tally
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::ProposalId;
    use icn_identity::KeyPair;

    #[test]
    fn test_empty_tally() {
        let tally = VoteTally::empty();

        assert_eq!(tally.for_votes, 0);
        assert_eq!(tally.against_votes, 0);
        assert_eq!(tally.abstain_votes, 0);
        assert_eq!(tally.total_votes(), 0);
        assert_eq!(tally.deciding_votes(), 0);
        assert_eq!(tally.approval_percentage(), 0);
    }

    #[test]
    fn test_add_votes() {
        let mut tally = VoteTally::empty();
        let proposal_id = ProposalId::generate();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();

        let vote1 = Vote::new(proposal_id.clone(), kp1.did().clone(), VoteChoice::For);
        let vote2 = Vote::new(proposal_id.clone(), kp2.did().clone(), VoteChoice::Against);
        let vote3 = Vote::new(proposal_id.clone(), kp3.did().clone(), VoteChoice::Abstain);

        tally.add_vote(&vote1);
        tally.add_vote(&vote2);
        tally.add_vote(&vote3);

        assert_eq!(tally.for_votes, 1);
        assert_eq!(tally.against_votes, 1);
        assert_eq!(tally.abstain_votes, 1);
        assert_eq!(tally.total_votes(), 3);
        assert_eq!(tally.deciding_votes(), 2);
        assert_eq!(tally.approval_percentage(), 50);
    }

    #[test]
    fn test_from_vec() {
        let proposal_id = ProposalId::generate();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();
        let kp4 = KeyPair::generate().unwrap();

        let votes = vec![
            Vote::new(proposal_id.clone(), kp1.did().clone(), VoteChoice::For),
            Vote::new(proposal_id.clone(), kp2.did().clone(), VoteChoice::For),
            Vote::new(proposal_id.clone(), kp3.did().clone(), VoteChoice::Against),
            Vote::new(proposal_id.clone(), kp4.did().clone(), VoteChoice::Abstain),
        ];

        let tally = VoteTally::from(votes);

        assert_eq!(tally.for_votes, 2);
        assert_eq!(tally.against_votes, 1);
        assert_eq!(tally.abstain_votes, 1);
        assert_eq!(tally.total_votes(), 4);
        assert_eq!(tally.deciding_votes(), 3);
        assert_eq!(tally.approval_percentage(), 66); // 2/3 = 66%
    }

    #[test]
    fn test_weighted_votes() {
        let mut tally = VoteTally::empty();
        let proposal_id = ProposalId::generate();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();

        let vote1 = Vote::new(proposal_id.clone(), kp1.did().clone(), VoteChoice::For)
            .with_weight(3);
        let vote2 = Vote::new(proposal_id.clone(), kp2.did().clone(), VoteChoice::Against)
            .with_weight(1);

        tally.add_vote(&vote1);
        tally.add_vote(&vote2);

        assert_eq!(tally.for_votes, 3);
        assert_eq!(tally.against_votes, 1);
        assert_eq!(tally.total_votes(), 4);
        assert_eq!(tally.approval_percentage(), 75); // 3/4 = 75%
    }
}
