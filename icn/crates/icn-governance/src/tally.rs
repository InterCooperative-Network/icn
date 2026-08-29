//! Vote tallying

use crate::delegation::DelegationManager;
use crate::domain::GovernanceDomainId;
use crate::error::GovernanceError;
use crate::proposal::ProposalId;
use crate::vote::{Vote, VoteChoice};
use crate::vote_principal::{
    effective_votes, DelegationResolution, DelegationStep, VotingPrincipal,
};
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

/// Raw summation of vote rows, one row at a time.
///
/// This counts **every** row it is given and therefore carries no protection
/// against one cryptographic voter appearing under several DID spellings.
/// Production tallies must use [`VoteTally::try_from_votes`], which reduces
/// rows to one effective vote per principal and fails closed on conflicting
/// stored acts (#2641).
impl From<Vec<Vote>> for VoteTally {
    fn from(votes: Vec<Vote>) -> Self {
        let mut tally = VoteTally::empty();
        for vote in votes {
            tally.add_vote(&vote);
        }
        tally
    }
}

impl VoteTally {
    /// Tally `votes` giving each cryptographic voting principal at most one
    /// effective vote, whatever multibase spelling of its DID named it.
    ///
    /// Fails closed if one principal has conflicting stored acts, rather than
    /// choosing which historical act wins (see [`effective_votes`]).
    pub fn try_from_votes(votes: &[Vote]) -> Result<Self, GovernanceError> {
        let mut tally = VoteTally::empty();
        for vote in effective_votes(votes)? {
            tally.add_vote(vote);
        }
        Ok(tally)
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
) -> Result<VoteTally, GovernanceError> {
    // Reduce stored rows to one effective act per cryptographic principal, so a
    // re-spelled DID cannot vote twice or be delegated to twice (#2641).
    let direct = effective_votes(votes)?;

    // Build a map of principal -> vote for quick lookup
    let mut vote_map: HashMap<VotingPrincipal, &Vote> = HashMap::new();
    for &vote in &direct {
        vote_map.insert(VotingPrincipal::of(&vote.voter)?, vote);
    }

    // Principals that voted *directly*. Delegation de-duplication belongs to
    // `resolution`, not here: marking a delegator counted would short-circuit
    // the next spelling of that principal before its delegate could be
    // compared, and a disagreement would pass unnoticed (#2641).
    let mut counted: HashSet<VotingPrincipal> = HashSet::new();
    let mut resolution = DelegationResolution::new();
    let mut tally = VoteTally::empty();

    // First pass: count direct votes
    for &vote in &direct {
        tally.add_vote(vote);
        counted.insert(VotingPrincipal::of(&vote.voter)?);
    }

    // Second pass: resolve delegated votes for non-voters
    for voter in eligible_voters {
        let voter_principal = VotingPrincipal::of(voter)?;
        if counted.contains(&voter_principal) {
            continue; // Already voted directly
        }

        // Resolve the delegation chain
        let delegate = delegation_manager.resolve_delegate(voter, domain_id, proposal_id);
        let delegate_principal = VotingPrincipal::of(&delegate)?;

        // If the delegate is not the same principal as the voter (i.e., a
        // delegation exists) and the delegate voted, count the vote for the
        // delegator. Comparing principals stops a self-delegation from being
        // laundered into a second vote by re-spelling the delegate DID.
        if delegate_principal != voter_principal {
            // Several spellings of one delegator must agree on the delegate, or
            // `eligible_voters` order would choose the delegated act (#2641).
            if resolution.record(voter, voter_principal, delegate_principal)?
                == DelegationStep::AlreadyExpanded
            {
                continue;
            }
            if let Some(delegate_vote) = vote_map.get(&delegate_principal) {
                // Create a vote for the delegator based on the delegate's choice and weight
                let delegated_vote =
                    Vote::new(proposal_id.clone(), voter.clone(), delegate_vote.choice)
                        .with_weight(delegate_vote.weight);
                tally.add_vote(&delegated_vote);
            }
        }
    }

    Ok(tally)
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
) -> Result<DelegatedTallyResult, GovernanceError> {
    // One effective act per cryptographic principal (#2641).
    let direct = effective_votes(votes)?;

    let mut vote_map: HashMap<VotingPrincipal, &Vote> = HashMap::new();
    for &vote in &direct {
        vote_map.insert(VotingPrincipal::of(&vote.voter)?, vote);
    }
    // Principals that voted *directly*. Delegation de-duplication belongs to
    // `resolution`, not here: marking a delegator counted would short-circuit
    // the next spelling of that principal before its delegate could be
    // compared, and a disagreement would pass unnoticed (#2641).
    let mut counted: HashSet<VotingPrincipal> = HashSet::new();
    let mut resolution = DelegationResolution::new();
    let mut tally = VoteTally::empty();
    let mut delegated_count = 0;
    let direct_count = direct.len();

    // Count direct votes
    for &vote in &direct {
        tally.add_vote(vote);
        counted.insert(VotingPrincipal::of(&vote.voter)?);
    }

    // Resolve delegated votes
    for voter in eligible_voters {
        let voter_principal = VotingPrincipal::of(voter)?;
        if counted.contains(&voter_principal) {
            continue;
        }

        let delegate = delegation_manager.resolve_delegate(voter, domain_id, proposal_id);
        let delegate_principal = VotingPrincipal::of(&delegate)?;

        if delegate_principal != voter_principal {
            // Same rule as the ordinary tally: one principal delegates once, and
            // disagreeing spellings fail closed rather than letting list order
            // pick the act (#2641).
            if resolution.record(voter, voter_principal, delegate_principal)?
                == DelegationStep::AlreadyExpanded
            {
                continue;
            }
            if let Some(delegate_vote) = vote_map.get(&delegate_principal) {
                // Preserve the delegate's vote weight
                let delegated_vote =
                    Vote::new(proposal_id.clone(), voter.clone(), delegate_vote.choice)
                        .with_weight(delegate_vote.weight);
                tally.add_vote(&delegated_vote);
                delegated_count += 1;
            }
        }
    }

    Ok(DelegatedTallyResult {
        tally,
        delegated_vote_count: delegated_count,
        direct_vote_count: direct_count,
    })
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

    /// Re-spell a DID as multibase base16 over the same identifier bytes.
    fn alias_spelling(did: &Did) -> Did {
        let bytes = did.identifier_bytes().expect("test DID must decode");
        Did::from_str(&format!("did:icn:f{}", hex::encode(bytes)))
            .expect("base16 multibase spelling must parse")
    }

    /// #2641: where `eligible_voters` names one principal under several
    /// spellings and those spellings delegate to voters who disagree, counting
    /// the first one encountered would make list order the authority over a
    /// vote. Fail closed instead.
    #[test]
    fn competing_alias_delegations_fail_closed() {
        use crate::delegation::{Delegation, DelegationManager, DelegationScope};

        let kp = icn_identity::KeyPair::generate().unwrap();
        let delegator = kp.did().clone();
        let alias = alias_spelling(&delegator);
        assert_ne!(delegator, alias, "control: the spellings must differ");

        let delegate_a = icn_identity::KeyPair::generate().unwrap().did().clone();
        let delegate_b = icn_identity::KeyPair::generate().unwrap().did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // One key, two spellings, two different delegates.
        let mut manager = DelegationManager::new();
        manager
            .add_delegation(Delegation::new(
                delegator.clone(),
                delegate_a.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();
        manager
            .add_delegation(Delegation::new(
                alias.clone(),
                delegate_b.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Both delegates vote, and they disagree.
        let votes = vec![
            Vote::new(proposal_id.clone(), delegate_a, VoteChoice::For),
            Vote::new(proposal_id.clone(), delegate_b, VoteChoice::Against),
        ];
        let eligible = vec![delegator, alias];

        let err =
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id)
                .expect_err("eligible-list order must not choose which delegated act counts");
        assert!(
            err.to_string().contains("competing delegations"),
            "must name the competing delegations, got: {err}"
        );

        let detailed = compute_detailed_tally_with_delegations(
            &votes,
            &eligible,
            &manager,
            &domain_id,
            &proposal_id,
        );
        assert!(
            detailed.is_err(),
            "the detailed twin must fail closed on the same evidence"
        );
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
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id)
                .expect("distinct principals must tally without conflict");

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
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id)
                .expect("distinct principals must tally without conflict");

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
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id)
                .expect("distinct principals must tally without conflict");

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
            compute_tally_with_delegations(&votes, &eligible, &manager, &domain_id, &proposal_id)
                .expect("distinct principals must tally without conflict");

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
        )
        .expect("distinct principals must tally without conflict");

        assert_eq!(result.direct_vote_count, 1);
        assert_eq!(result.delegated_vote_count, 2);
        assert_eq!(result.tally.for_votes, 3);
    }
}
