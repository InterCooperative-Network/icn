//! Governance primitives for ICN
//!
//! This module provides proposal management, voting, and governance utilities
//! that CCL contracts can use to implement cooperative decision-making.

use crate::types::{Proposal, ProposalID, ProposalStatus, Vote, VoteCounts, MemberRole};
use anyhow::{Context, Result};
use icn_identity::Did;
use icn_ledger::ContentHash;
use std::collections::HashMap;
use tracing::{debug, info};

/// Governance manager handles proposals and roles
pub struct GovernanceManager {
    /// Active proposals (id -> Proposal)
    proposals: HashMap<ProposalID, Proposal>,

    /// Member roles (did -> roles)
    roles: HashMap<Did, Vec<MemberRole>>,

    /// All members in the system
    members: Vec<Did>,
}

impl GovernanceManager {
    /// Create a new governance manager
    pub fn new() -> Self {
        GovernanceManager {
            proposals: HashMap::new(),
            roles: HashMap::new(),
            members: Vec::new(),
        }
    }

    /// Add a member to the system
    pub fn add_member(&mut self, member: Did) {
        if !self.members.contains(&member) {
            self.members.push(member);
        }
    }

    /// Remove a member from the system
    pub fn remove_member(&mut self, member: &Did) {
        self.members.retain(|m| m != member);
        self.roles.remove(member);
    }

    /// Get total member count
    pub fn member_count(&self) -> u64 {
        self.members.len() as u64
    }

    /// Check if DID is a member
    pub fn is_member(&self, did: &Did) -> bool {
        self.members.contains(did)
    }

    /// Get all members
    pub fn members(&self) -> &[Did] {
        &self.members
    }

    // ========================================================================
    // Proposal Management
    // ========================================================================

    /// Create a new proposal
    pub fn proposal_create(
        &mut self,
        author: Did,
        subject: String,
        payload_ref: ContentHash,
        category: String,
        timestamp: u64,
    ) -> Result<ProposalID> {
        // Validate author is a member
        if !self.is_member(&author) {
            anyhow::bail!("Only members can create proposals");
        }

        let proposal = Proposal::new(author, subject, payload_ref, category, timestamp);
        let id = proposal.id.clone();

        info!("Creating proposal: {} by {:?}", id, proposal.author);
        self.proposals.insert(id.clone(), proposal);

        Ok(id)
    }

    /// Cast a vote on a proposal
    pub fn proposal_vote(
        &mut self,
        id: &ProposalID,
        voter: Did,
        vote: Vote,
        timestamp: u64,
    ) -> Result<()> {
        // Validate voter is a member
        if !self.is_member(&voter) {
            anyhow::bail!("Only members can vote");
        }

        let proposal = self
            .proposals
            .get_mut(id)
            .context("Proposal not found")?;

        // Check proposal is still open
        if proposal.status != ProposalStatus::Open {
            anyhow::bail!("Proposal is not open for voting");
        }

        debug!("Vote cast on {}: {:?} by {:?}", id, vote, voter);
        proposal.add_vote(voter, vote, timestamp);

        Ok(())
    }

    /// Get proposal by ID
    pub fn proposal_state(&self, id: &ProposalID) -> Result<&Proposal> {
        self.proposals.get(id).context("Proposal not found")
    }

    /// Get all proposals
    pub fn all_proposals(&self) -> Vec<&Proposal> {
        self.proposals.values().collect()
    }

    /// Update proposal status
    pub fn update_proposal_status(&mut self, id: &ProposalID, status: ProposalStatus) -> Result<()> {
        let proposal = self
            .proposals
            .get_mut(id)
            .context("Proposal not found")?;

        info!("Updating proposal {} status to {:?}", id, status);
        proposal.status = status;

        Ok(())
    }

    // ========================================================================
    // Voting Calculations
    // ========================================================================

    /// Check if quorum is met
    pub fn quorum_met(
        &self,
        proposal_id: &ProposalID,
        quorum_threshold: f64,
    ) -> Result<bool> {
        let proposal = self.proposal_state(proposal_id)?;
        let total_members = self.member_count();

        // If no members, quorum cannot be met
        if total_members == 0 {
            return Ok(false);
        }

        let voters = proposal.voters().len() as f64;
        let total_members_f64 = total_members as f64;

        Ok(voters / total_members_f64 >= quorum_threshold)
    }

    /// Check if vote threshold is met
    pub fn threshold_met(
        &self,
        proposal_id: &ProposalID,
        threshold: f64,
    ) -> Result<bool> {
        let proposal = self.proposal_state(proposal_id)?;
        let counts = proposal.count_votes();

        let total_votes = counts.consent + counts.blocks.len() as u64;
        if total_votes == 0 {
            return Ok(false);
        }

        let yes_ratio = counts.consent as f64 / total_votes as f64;
        Ok(yes_ratio >= threshold)
    }

    /// Check if consent is met (no blocks or blocks within allowed limit)
    pub fn consent_met(
        &self,
        proposal_id: &ProposalID,
        max_blocks: u64,
    ) -> Result<bool> {
        let proposal = self.proposal_state(proposal_id)?;
        let counts = proposal.count_votes();

        Ok(counts.blocks.len() as u64 <= max_blocks)
    }

    // ========================================================================
    // Role Management
    // ========================================================================

    /// Assign a role to a member
    pub fn assign_role(
        &mut self,
        member: Did,
        role: String,
        assigned_at: u64,
        expires_at: Option<u64>,
    ) -> Result<()> {
        if !self.is_member(&member) {
            anyhow::bail!("Cannot assign role to non-member");
        }

        let member_role = MemberRole {
            member: member.clone(),
            role: role.clone(),
            assigned_at,
            expires_at,
        };

        self.roles
            .entry(member)
            .or_insert_with(Vec::new)
            .push(member_role);

        info!("Assigned role '{}' to {:?}", role, member);
        Ok(())
    }

    /// Remove a role from a member
    pub fn remove_role(&mut self, member: &Did, role: &str) -> Result<()> {
        if let Some(roles) = self.roles.get_mut(member) {
            roles.retain(|r| r.role != role);
            info!("Removed role '{}' from {:?}", role, member);
        }
        Ok(())
    }

    /// Check if member has a specific role
    pub fn has_role(&self, member: &Did, role: &str) -> bool {
        if let Some(roles) = self.roles.get(member) {
            roles.iter().any(|r| r.role == role)
        } else {
            false
        }
    }

    /// Get all members with a specific role
    pub fn members_with_role(&self, role: &str) -> Vec<Did> {
        let mut members = Vec::new();
        for (member, roles) in &self.roles {
            if roles.iter().any(|r| r.role == role) {
                members.push(member.clone());
            }
        }
        members
    }

    /// Get all roles for a member
    pub fn get_roles(&self, member: &Did) -> Vec<String> {
        if let Some(roles) = self.roles.get(member) {
            roles.iter().map(|r| r.role.clone()).collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for GovernanceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_member_management() {
        let mut gov = GovernanceManager::new();
        let alice = test_did();
        let bob = test_did();

        assert_eq!(gov.member_count(), 0);
        assert!(!gov.is_member(&alice));

        gov.add_member(alice.clone());
        assert_eq!(gov.member_count(), 1);
        assert!(gov.is_member(&alice));

        gov.add_member(bob.clone());
        assert_eq!(gov.member_count(), 2);

        gov.remove_member(&alice);
        assert_eq!(gov.member_count(), 1);
        assert!(!gov.is_member(&alice));
        assert!(gov.is_member(&bob));
    }

    #[test]
    fn test_proposal_creation() {
        let mut gov = GovernanceManager::new();
        let alice = test_did();
        gov.add_member(alice.clone());

        let payload = ContentHash::from_bytes(b"proposal details");
        let id = gov
            .proposal_create(
                alice.clone(),
                "Test Proposal".to_string(),
                payload,
                "general".to_string(),
                1000,
            )
            .unwrap();

        let proposal = gov.proposal_state(&id).unwrap();
        assert_eq!(proposal.subject, "Test Proposal");
        assert_eq!(proposal.author, alice);
        assert_eq!(proposal.status, ProposalStatus::Open);
    }

    #[test]
    fn test_proposal_voting() {
        let mut gov = GovernanceManager::new();
        let alice = test_did();
        let bob = test_did();
        gov.add_member(alice.clone());
        gov.add_member(bob.clone());

        let payload = ContentHash::from_bytes(b"proposal");
        let id = gov
            .proposal_create(
                alice.clone(),
                "Test".to_string(),
                payload,
                "general".to_string(),
                1000,
            )
            .unwrap();

        // Alice votes consent
        gov.proposal_vote(&id, alice.clone(), Vote::Consent, 1001)
            .unwrap();

        // Bob votes block
        gov.proposal_vote(&id, bob.clone(), Vote::Block("Not ready".to_string()), 1002)
            .unwrap();

        let proposal = gov.proposal_state(&id).unwrap();
        let counts = proposal.count_votes();
        assert_eq!(counts.consent, 1);
        assert_eq!(counts.blocks.len(), 1);
        assert_eq!(counts.abstain, 0);
    }

    #[test]
    fn test_quorum_calculation() {
        let mut gov = GovernanceManager::new();
        let members: Vec<Did> = (0..10).map(|_| test_did()).collect();
        for member in &members {
            gov.add_member(member.clone());
        }

        let payload = ContentHash::from_bytes(b"proposal");
        let id = gov
            .proposal_create(
                members[0].clone(),
                "Test".to_string(),
                payload,
                "general".to_string(),
                1000,
            )
            .unwrap();

        // 3 votes out of 10 members = 30%
        for i in 0..3 {
            gov.proposal_vote(&id, members[i].clone(), Vote::Consent, 1001 + i as u64)
                .unwrap();
        }

        // 50% quorum not met
        assert!(!gov.quorum_met(&id, 0.5).unwrap());

        // 20% quorum met
        assert!(gov.quorum_met(&id, 0.2).unwrap());
    }

    #[test]
    fn test_threshold_calculation() {
        let mut gov = GovernanceManager::new();
        let members: Vec<Did> = (0..10).map(|_| test_did()).collect();
        for member in &members {
            gov.add_member(member.clone());
        }

        let payload = ContentHash::from_bytes(b"proposal");
        let id = gov
            .proposal_create(
                members[0].clone(),
                "Test".to_string(),
                payload,
                "general".to_string(),
                1000,
            )
            .unwrap();

        // 7 consent, 3 block = 70% yes
        for i in 0..7 {
            gov.proposal_vote(&id, members[i].clone(), Vote::Consent, 1001 + i as u64)
                .unwrap();
        }
        for i in 7..10 {
            gov.proposal_vote(&id, members[i].clone(), Vote::Block("no".to_string()), 1001 + i as u64)
                .unwrap();
        }

        // 2/3 threshold met (70% > 66.6%)
        assert!(gov.threshold_met(&id, 0.66).unwrap());

        // 80% threshold not met
        assert!(!gov.threshold_met(&id, 0.8).unwrap());
    }

    #[test]
    fn test_role_management() {
        let mut gov = GovernanceManager::new();
        let alice = test_did();
        let bob = test_did();
        gov.add_member(alice.clone());
        gov.add_member(bob.clone());

        // Assign roles
        gov.assign_role(alice.clone(), "admin".to_string(), 1000, None)
            .unwrap();
        gov.assign_role(bob.clone(), "treasurer".to_string(), 1000, None)
            .unwrap();
        gov.assign_role(bob.clone(), "member".to_string(), 1000, None)
            .unwrap();

        // Check roles
        assert!(gov.has_role(&alice, "admin"));
        assert!(!gov.has_role(&alice, "treasurer"));
        assert!(gov.has_role(&bob, "treasurer"));
        assert!(gov.has_role(&bob, "member"));

        // Get members with role
        let admins = gov.members_with_role("admin");
        assert_eq!(admins.len(), 1);
        assert_eq!(admins[0], alice);

        let treasurers = gov.members_with_role("treasurer");
        assert_eq!(treasurers.len(), 1);

        // Remove role
        gov.remove_role(&bob, "treasurer").unwrap();
        assert!(!gov.has_role(&bob, "treasurer"));
        assert!(gov.has_role(&bob, "member"));
    }
}
