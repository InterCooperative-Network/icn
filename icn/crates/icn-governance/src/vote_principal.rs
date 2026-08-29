//! Cryptographic voting-principal identity for governance acts.
//!
//! A [`Did`] retains whatever multibase spelling it was parsed from, and its
//! `Eq`/`Hash` are string equality (#2627 / N2-A is the tranche that changes
//! that). One Ed25519 key therefore has many accepted textual names.
//!
//! Governance must not let representation buy voting weight (#2641), so vote
//! admission and vote counting resolve each voter to the 32-byte public key its
//! DID decodes to, rather than to the spelling that named it.
//!
//! This is deliberately local to governance. It does not canonicalize DIDs, does
//! not change `Did` equality, and does not re-key any persisted store — the
//! membership/vote re-key stays behind the `IDENTITY_SEMANTICS.md` §7.5 gate.

use crate::error::GovernanceError;
use crate::vote::Vote;
use icn_identity::Did;
use std::collections::HashMap;

/// The decoded cryptographic principal that a voter DID names.
///
/// Two `Did`s whose multibase identifiers decode to the same 32 bytes are the
/// same `VotingPrincipal`, whatever spelling they carry.
///
/// Identity is the decoded bytes rather than a parsed `VerifyingKey`. For a DID
/// that passed `Did::from_str` validation the two are equivalent — those bytes
/// *are* the Ed25519 public key — but keying on bytes also resolves
/// anchor-derived DIDs (`Did::from_anchor_id`), which are legitimate principals
/// in this codebase yet need not decompress to an Edwards point. Demanding a
/// verifying key here would refuse to tally those rather than protect them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VotingPrincipal([u8; 32]);

impl VotingPrincipal {
    /// Resolve the principal a voter DID names.
    ///
    /// Fails closed: a DID whose identifier does not decode to 32 bytes names
    /// no principal, and must be neither admitted nor counted.
    pub fn of(did: &Did) -> Result<Self, GovernanceError> {
        let bytes = did.identifier_bytes().map_err(|e| {
            GovernanceError::InvalidVoter(format!("voter DID does not decode: {e}"))
        })?;
        Ok(Self(bytes))
    }

    /// The decoded identifier bytes this principal is.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Whether two stored acts by one principal express the same effective vote.
///
/// Only the fields that carry voting power are compared. Two rows that differ
/// solely in comment or timestamp express the same act and are not a conflict.
fn same_effective_act(a: &Vote, b: &Vote) -> bool {
    a.choice == b.choice && a.weight == b.weight
}

/// Reduce stored vote rows to at most one effective vote per cryptographic
/// principal.
///
/// Returned in first-encountered order, so the caller's ordering is preserved
/// and no ordering of the underlying storage becomes an authority rule.
///
/// # Rules
///
/// * Rows whose voter DIDs decode to different keys are different voters.
/// * Several rows for one principal that express the *same* effective act
///   collapse to one contribution. No survivor is being chosen: every candidate
///   contributes identically, so the result does not depend on which is kept.
/// * Several rows for one principal that express *conflicting* acts fail closed.
///   Deciding which of two conflicting historical acts is authoritative is a
///   migration/conflict policy owned by `IDENTITY_SEMANTICS.md` §7.5, not by
///   this function, and guessing would silently rewrite governance history.
pub fn effective_votes(votes: &[Vote]) -> Result<Vec<&Vote>, GovernanceError> {
    let mut order: Vec<&Vote> = Vec::new();
    let mut seen: HashMap<VotingPrincipal, usize> = HashMap::new();

    for vote in votes {
        let principal = VotingPrincipal::of(&vote.voter)?;
        match seen.get(&principal) {
            None => {
                seen.insert(principal, order.len());
                order.push(vote);
            }
            Some(&idx) => {
                let kept = order[idx];
                if !same_effective_act(kept, vote) {
                    return Err(GovernanceError::ConflictingVoteRecords(format!(
                        "proposal {}: one voting principal has conflicting stored acts \
                         ({:?} weight {} as '{}' vs {:?} weight {} as '{}'); \
                         resolving this requires the §7.5-gated vote migration, \
                         so the tally fails closed rather than choosing a winner",
                        vote.proposal_id.0,
                        kept.choice,
                        kept.weight,
                        kept.voter,
                        vote.choice,
                        vote.weight,
                        vote.voter,
                    )));
                }
            }
        }
    }

    Ok(order)
}

/// How many distinct voting principals a member list names.
///
/// Quorum denominators must count voters rather than DID spellings. The tally
/// numerator gives one principal one vote (#2641), so a list naming one key
/// under several spellings would otherwise claim several electorate slots for a
/// single voter and read their vote as partial turnout.
pub fn distinct_principals(members: &[Did]) -> Result<usize, GovernanceError> {
    let mut seen = std::collections::HashSet::new();
    for member in members {
        seen.insert(VotingPrincipal::of(member)?);
    }
    Ok(seen.len())
}

/// Tracks which delegate each principal resolved to while a delegated tally is
/// being expanded.
///
/// One principal delegates once. Where an eligible-voter list names it under
/// several spellings, those spellings must resolve to the same delegate — else
/// the order of that list would decide which delegated act counts, which is a
/// hidden selector over a vote rather than a democratic outcome.
#[derive(Debug, Default)]
pub struct DelegationResolution {
    seen: HashMap<VotingPrincipal, VotingPrincipal>,
}

/// What [`DelegationResolution::record`] found for a principal.
#[derive(Debug, PartialEq, Eq)]
pub enum DelegationStep {
    /// First spelling of this principal: expand its delegation.
    Expand,
    /// A previous spelling already resolved to the same delegate: skip.
    AlreadyExpanded,
}

impl DelegationResolution {
    /// Create an empty resolution tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the delegate this principal resolved to, failing closed if an
    /// earlier spelling of the same principal resolved to a different one.
    pub fn record(
        &mut self,
        delegator: &Did,
        delegator_principal: VotingPrincipal,
        delegate_principal: VotingPrincipal,
    ) -> Result<DelegationStep, GovernanceError> {
        match self.seen.get(&delegator_principal) {
            Some(&already) if already != delegate_principal => {
                Err(GovernanceError::CompetingDelegations(format!(
                    "one voting principal holds competing delegations under different DID \
                     spellings ('{delegator}' resolves elsewhere than a previous spelling of \
                     the same key); refusing to let list order choose which delegated act counts"
                )))
            }
            Some(_) => Ok(DelegationStep::AlreadyExpanded),
            None => {
                self.seen.insert(delegator_principal, delegate_principal);
                Ok(DelegationStep::Expand)
            }
        }
    }
}

/// The single effective act already recorded for the principal `voter` names.
///
/// Returns `Ok(None)` when that principal has not voted. Fails closed when the
/// stored rows for that principal conflict with one another, so neither
/// admission nor lookup silently picks one of two conflicting historical acts.
///
/// A conflicting pair belonging to some *other* voter does not block this voter
/// from acting: conflicts are only ever reported within one principal.
///
/// Every row is still decoded, because a row's principal cannot be compared
/// without decoding it. A row whose voter DID does not decode therefore fails
/// this lookup closed whoever it belongs to — an unidentifiable voter cannot be
/// ruled out as being this principal, so it cannot be safely skipped.
pub fn prior_act_for<'a>(
    votes: &'a [Vote],
    voter: &Did,
) -> Result<Option<&'a Vote>, GovernanceError> {
    Ok(acts_for(votes, voter)?.first().copied())
}

/// Every stored row naming the principal `voter` names, in encounter order.
///
/// Fails closed on the same terms as [`prior_act_for`]. More than one row means
/// the rows were written before duplicate-act prevention existed: they agree, or
/// this would have returned an error, but they are still several rows for one
/// voter and callers that must write must take that into account.
pub fn acts_for<'a>(votes: &'a [Vote], voter: &Did) -> Result<Vec<&'a Vote>, GovernanceError> {
    let principal = VotingPrincipal::of(voter)?;
    let mut found: Vec<&'a Vote> = Vec::new();

    for vote in votes {
        if VotingPrincipal::of(&vote.voter)? != principal {
            continue;
        }
        match found.first() {
            None => found.push(vote),
            Some(kept) => {
                if !same_effective_act(kept, vote) {
                    return Err(GovernanceError::ConflictingVoteRecords(format!(
                        "proposal {}: one voting principal has conflicting stored acts \
                         ({:?} weight {} as '{}' vs {:?} weight {} as '{}'); \
                         resolving this requires the §7.5-gated vote migration, \
                         so governance fails closed rather than choosing a winner",
                        vote.proposal_id.0,
                        kept.choice,
                        kept.weight,
                        kept.voter,
                        vote.choice,
                        vote.weight,
                        vote.voter,
                    )));
                }
                found.push(vote);
            }
        }
    }

    Ok(found)
}

/// Reject a second vote from a principal that has already acted.
///
/// Constructs [`GovernanceError::AlreadyVoted`], enforcing INV-6 ("duplicate
/// vote must be rejected") by cryptographic principal rather than by spelling,
/// so re-spelling a voter DID no longer buys a second counted vote (#2641).
pub fn ensure_has_not_voted(votes: &[Vote], voter: &Did) -> Result<(), GovernanceError> {
    if let Some(existing) = prior_act_for(votes, voter)? {
        return Err(GovernanceError::AlreadyVoted(format!(
            "{} has already voted on proposal {} (recorded under DID spelling '{}')",
            voter, existing.proposal_id.0, existing.voter
        )));
    }
    Ok(())
}
