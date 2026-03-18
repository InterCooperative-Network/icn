//! Vote tallying

use crate::delegation::DelegationManager;
use crate::domain::GovernanceDomainId;
use crate::proposal::ProposalId;
use crate::vote::{Vote, VoteChoice};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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

/// Compute a vote tally that accounts for delegations
///
/// This function resolves delegated votes according to these rules:
/// 1. If a member voted directly, their vote is counted (delegation ignored)
/// 2. If a member didn't vote but has a delegation, their delegate's vote is used
/// 3. Delegations are resolved transitively up to max_depth
/// 4. If the ultimate delegate also didn't vote, the delegator's vote is not counted
///
/// # Arguments
/// * `votes` - The direct votes cast on the proposal
/// * `eligible_voters` - All members eligible to vote on this proposal
/// * `delegation_manager` - Manager containing active delegations
/// * `domain_id` - The governance domain ID
/// * `proposal_id` - The proposal ID
///
/// # Returns
/// A VoteTally that includes both direct votes and resolved delegated votes
pub fn compute_tally_with_delegations(
    votes: &[Vote],
    eligible_voters: &[Did],
    delegation_manager: &DelegationManager,
    domain_id: &GovernanceDomainId,
    proposal_id: &ProposalId,
) -> VoteTally {
    // Build a map of voter -> vote for quick lookup
    let vote_map: HashMap<&Did, &Vote> = votes.iter().map(|v| (&v.voter, v)).collect();

    // Track who has already been counted to avoid double-counting
    let mut counted: HashSet<&Did> = HashSet::new();
    let mut tally = VoteTally::empty();

    // First pass: count direct votes
    for vote in votes {
        tally.add_vote(vote);
        counted.insert(&vote.voter);
    }

    // Second pass: resolve delegated votes for non-voters
    for voter in eligible_voters {
        if counted.contains(voter) {
            continue; // Already voted directly
        }

        // Resolve the delegation chain
        let delegate = delegation_manager.resolve_delegate(voter, domain_id, proposal_id);

        // If the delegate is not the same as the voter (i.e., delegation exists)
        // and the delegate voted, count the vote for the delegator
        if delegate != *voter {
            if let Some(delegate_vote) = vote_map.get(&delegate) {
                // Create a vote for the delegator based on the delegate's choice and weight
                let delegated_vote =
                    Vote::new(proposal_id.clone(), voter.clone(), delegate_vote.choice)
                        .with_weight(delegate_vote.weight);
                tally.add_vote(&delegated_vote);
                counted.insert(voter);
            }
        }
    }

    tally
}

/// Result of computing a tally with delegation details
#[derive(Debug, Clone)]
pub struct DelegatedTallyResult {
    /// The computed tally
    pub tally: VoteTally,
    /// Number of votes that were resolved through delegation
    pub delegated_vote_count: usize,
    /// Number of direct votes
    pub direct_vote_count: usize,
}

/// Compute a detailed tally with delegation information
pub fn compute_detailed_tally_with_delegations(
    votes: &[Vote],
    eligible_voters: &[Did],
    delegation_manager: &DelegationManager,
    domain_id: &GovernanceDomainId,
    proposal_id: &ProposalId,
) -> DelegatedTallyResult {
    let vote_map: HashMap<&Did, &Vote> = votes.iter().map(|v| (&v.voter, v)).collect();
    let mut counted: HashSet<&Did> = HashSet::new();
    let mut tally = VoteTally::empty();
    let mut delegated_count = 0;
    let direct_count = votes.len();

    // Count direct votes
    for vote in votes {
        tally.add_vote(vote);
        counted.insert(&vote.voter);
    }

    // Resolve delegated votes
    for voter in eligible_voters {
        if counted.contains(voter) {
            continue;
        }

        let delegate = delegation_manager.resolve_delegate(voter, domain_id, proposal_id);

        if delegate != *voter {
            if let Some(delegate_vote) = vote_map.get(&delegate) {
                // Preserve the delegate's vote weight
                let delegated_vote =
                    Vote::new(proposal_id.clone(), voter.clone(), delegate_vote.choice)
                        .with_weight(delegate_vote.weight);
                tally.add_vote(&delegated_vote);
                counted.insert(voter);
                delegated_count += 1;
            }
        }
    }

    DelegatedTallyResult {
        tally,
        delegated_vote_count: delegated_count,
        direct_vote_count: direct_count,
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

        let vote1 =
            Vote::new(proposal_id.clone(), kp1.did().clone(), VoteChoice::For).with_weight(3);
        let vote2 =
            Vote::new(proposal_id.clone(), kp2.did().clone(), VoteChoice::Against).with_weight(1);

        tally.add_vote(&vote1);
        tally.add_vote(&vote2);

        assert_eq!(tally.for_votes, 3);
        assert_eq!(tally.against_votes, 1);
        assert_eq!(tally.total_votes(), 4);
        assert_eq!(tally.approval_percentage(), 75); // 3/4 = 75%
    }

    // Helper to create deterministic test DIDs
    fn test_did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    #[test]
    fn test_tally_with_single_delegation() {
        use crate::delegation::{Delegation, DelegationManager, DelegationScope};

        let alice = test_did(1);
        let bob = test_did(2);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // Alice delegates to Bob
        let mut manager = DelegationManager::new();
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Only Bob votes
        let votes = vec![Vote::new(proposal_id.clone(), bob.clone(), VoteChoice::For)];
        let eligible = vec![alice.clone(), bob.clone()];

        let tally =
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id);

        // Should count 2 votes: Bob's direct + Alice's delegated
        assert_eq!(tally.for_votes, 2);
        assert_eq!(tally.total_votes(), 2);
    }

    #[test]
    fn test_tally_direct_vote_overrides_delegation() {
        use crate::delegation::{Delegation, DelegationManager, DelegationScope};

        let alice = test_did(1);
        let bob = test_did(2);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // Alice delegates to Bob
        let mut manager = DelegationManager::new();
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Both vote, but differently
        let votes = vec![
            Vote::new(proposal_id.clone(), alice.clone(), VoteChoice::Against), // Alice votes directly
            Vote::new(proposal_id.clone(), bob.clone(), VoteChoice::For),       // Bob votes
        ];
        let eligible = vec![alice.clone(), bob.clone()];

        let tally =
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id);

        // Alice's direct vote should win (Against), Bob votes For
        assert_eq!(tally.for_votes, 1);
        assert_eq!(tally.against_votes, 1);
        assert_eq!(tally.total_votes(), 2);
    }

    #[test]
    fn test_tally_transitive_delegation() {
        use crate::delegation::{Delegation, DelegationManager, DelegationScope};

        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // Alice -> Bob -> Charlie
        let mut manager = DelegationManager::new();
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();
        manager
            .add_delegation(Delegation::new(
                bob.clone(),
                charlie.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Only Charlie votes
        let votes = vec![Vote::new(
            proposal_id.clone(),
            charlie.clone(),
            VoteChoice::For,
        )];
        let eligible = vec![alice.clone(), bob.clone(), charlie.clone()];

        let tally =
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id);

        // Should count 3 votes: Charlie direct, Bob delegated, Alice delegated
        assert_eq!(tally.for_votes, 3);
        assert_eq!(tally.total_votes(), 3);
    }

    #[test]
    fn test_tally_delegate_didnt_vote() {
        use crate::delegation::{Delegation, DelegationManager, DelegationScope};

        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // Alice delegates to Bob, but Bob doesn't vote
        let mut manager = DelegationManager::new();
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Only Charlie votes (not involved in delegation)
        let votes = vec![Vote::new(
            proposal_id.clone(),
            charlie.clone(),
            VoteChoice::For,
        )];
        let eligible = vec![alice.clone(), bob.clone(), charlie.clone()];

        let tally =
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id);

        // Only Charlie's vote counts; Alice's delegation goes to Bob who didn't vote
        assert_eq!(tally.for_votes, 1);
        assert_eq!(tally.total_votes(), 1);
    }

    #[test]
    fn test_detailed_tally_counts() {
        use crate::delegation::{Delegation, DelegationManager, DelegationScope};

        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // Alice and Charlie delegate to Bob
        let mut manager = DelegationManager::new();
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();
        manager
            .add_delegation(Delegation::new(
                charlie.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Only Bob votes
        let votes = vec![Vote::new(proposal_id.clone(), bob.clone(), VoteChoice::For)];
        let eligible = vec![alice.clone(), bob.clone(), charlie.clone()];

        let result = compute_detailed_tally_with_delegations(
            &votes,
            &eligible,
            &manager,
            &domain_id,
            &proposal_id,
        );

        assert_eq!(result.direct_vote_count, 1);
        assert_eq!(result.delegated_vote_count, 2);
        assert_eq!(result.tally.for_votes, 3);
    }
}
