//! Pure projection from receipt DAG to canonical facts and capsule hashes.

use blake3::Hasher;
use icn_boundary::{CanonicalFacts, Hash};
use serde::Serialize;

use crate::capsule::{build_capsule, StateResolutionCapsule};
use crate::constants::{PROCESS_ID, TARGET_REF};
use crate::receipt_types::{BaselineReceipt, BaselineReceiptBody, FixtureKeys};

#[derive(Debug, thiserror::Error)]
pub enum ProjectorError {
    #[error("empty receipt chain")]
    EmptyChain,
    #[error("receipt {0}: verification failed")]
    ReceiptVerify(usize),
    #[error("receipt {0}: prior link mismatch")]
    PriorLink(usize),
    #[error("missing process session")]
    MissingSession,
    #[error("missing standing snapshot")]
    MissingStanding,
    #[error("missing reservation receipt")]
    MissingReservation,
    #[error("reservation already consumed")]
    ReservationConsumed,
    #[error("vote receipt signer does not match declared member_pubkey (receipt {0})")]
    VoteSignerMismatch(usize),
    #[error("duplicate vote from member")]
    DuplicateVote,
    #[error("non-member vote")]
    NonMemberVote,
    #[error("missing notice for eligible member")]
    MissingNotice,
    #[error("allocation not linked to target process")]
    AllocationMismatch,
    #[error("vote threshold not met")]
    ThresholdNotMet,
    #[error("receipt {0}: signer is not authorized for this receipt type in the baseline fixture")]
    UnauthorizedReceiptSigner(usize),
}

/// Expected verifying keys for non-vote baseline fixture receipts (narrow test harness).
#[derive(Debug, Clone, Copy)]
pub struct BaselineFixtureAuthority {
    pub process_session_opener: [u8; 32],
    pub standing_snapshot_signer: [u8; 32],
    pub notice_signer: [u8; 32],
    pub allocation_signer: [u8; 32],
    pub reservation_signer: [u8; 32],
}

impl BaselineFixtureAuthority {
    pub fn from_fixture_keys(keys: &FixtureKeys) -> Self {
        Self {
            process_session_opener: keys.coop.verifying_key().to_bytes(),
            standing_snapshot_signer: keys.host.verifying_key().to_bytes(),
            notice_signer: keys.coop.verifying_key().to_bytes(),
            allocation_signer: keys.coop.verifying_key().to_bytes(),
            reservation_signer: keys.host.verifying_key().to_bytes(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectedState {
    pub facts: CanonicalFacts,
    pub capsule: StateResolutionCapsule,
    pub capsule_hash: Hash,
    pub canonical_fact_snapshot_hash: Hash,
    pub authority_context_hash: Hash,
    pub agreement_context_hash: Hash,
}

fn hash_postcard<T: Serialize>(domain: &[u8], v: &T) -> Hash {
    let bytes = postcard::to_allocvec(v).expect("serialize");
    let mut h = Hasher::new();
    h.update(domain);
    h.update(&bytes);
    Hash(*h.finalize().as_bytes())
}

/// Project receipts (genesis-to-tip order) into canonical facts.
///
/// `authority` fixes which verifying keys may sign each non-vote receipt type for this fixture
/// (in addition to cryptographic `verify` on each receipt).
pub fn project(
    receipts: &[BaselineReceipt],
    authority: &BaselineFixtureAuthority,
) -> Result<ProjectedState, ProjectorError> {
    if receipts.is_empty() {
        return Err(ProjectorError::EmptyChain);
    }

    let mut prior = [0u8; 32];
    let mut session: Option<(u32, u64, u32)> = None;
    let mut members: Vec<[u8; 32]> = Vec::new();
    let mut notices: Vec<[u8; 32]> = Vec::new();
    let mut votes: Vec<([u8; 32], bool)> = Vec::new();
    let mut allocation_amount: Option<u64> = None;
    let mut reservation_ok: Option<bool> = None;
    let mut receipt_hashes: Vec<Hash> = Vec::new();

    for (i, r) in receipts.iter().enumerate() {
        if r.prior_receipt_hash != prior {
            return Err(ProjectorError::PriorLink(i));
        }
        if !r.verify(PROCESS_ID, TARGET_REF) {
            return Err(ProjectorError::ReceiptVerify(i));
        }
        prior = r.receipt_hash;
        receipt_hashes.push(Hash(r.receipt_hash));

        match &r.body {
            BaselineReceiptBody::ProcessSessionOpened {
                eligible_members,
                allocation_limit,
                required_approvals,
            } => {
                if r.signer != authority.process_session_opener {
                    return Err(ProjectorError::UnauthorizedReceiptSigner(i));
                }
                session = Some((*eligible_members, *allocation_limit, *required_approvals));
            }
            BaselineReceiptBody::StandingContextSnapshot { member_pubkeys } => {
                if r.signer != authority.standing_snapshot_signer {
                    return Err(ProjectorError::UnauthorizedReceiptSigner(i));
                }
                members = member_pubkeys.clone();
            }
            BaselineReceiptBody::NoticeDelivered { member_pubkey } => {
                if r.signer != authority.notice_signer {
                    return Err(ProjectorError::UnauthorizedReceiptSigner(i));
                }
                notices.push(*member_pubkey);
            }
            BaselineReceiptBody::DeliberationEntryRecorded {
                member_pubkey,
                approve,
            } => {
                if !members.contains(member_pubkey) {
                    return Err(ProjectorError::NonMemberVote);
                }
                if r.signer != *member_pubkey {
                    return Err(ProjectorError::VoteSignerMismatch(i));
                }
                if votes.iter().any(|(m, _)| m == member_pubkey) {
                    return Err(ProjectorError::DuplicateVote);
                }
                votes.push((*member_pubkey, *approve));
            }
            BaselineReceiptBody::AllocationRequested { amount } => {
                if r.signer != authority.allocation_signer {
                    return Err(ProjectorError::UnauthorizedReceiptSigner(i));
                }
                allocation_amount = Some(*amount);
            }
            BaselineReceiptBody::ReservationState { not_consumed } => {
                if r.signer != authority.reservation_signer {
                    return Err(ProjectorError::UnauthorizedReceiptSigner(i));
                }
                reservation_ok = Some(*not_consumed);
            }
        }
    }

    let (eligible, alloc_limit, required) = session.ok_or(ProjectorError::MissingSession)?;
    if members.is_empty() {
        return Err(ProjectorError::MissingStanding);
    }
    let reservation_not_consumed = reservation_ok.ok_or(ProjectorError::MissingReservation)?;
    if !reservation_not_consumed {
        return Err(ProjectorError::ReservationConsumed);
    }

    for m in &members {
        if !notices.contains(m) {
            return Err(ProjectorError::MissingNotice);
        }
    }

    let allocation_requested = allocation_amount.ok_or(ProjectorError::AllocationMismatch)?;

    let mut approvals = 0u32;
    let mut rejections = 0u32;
    let abstentions = 0u32;
    for (_, a) in &votes {
        if *a {
            approvals += 1;
        } else {
            rejections += 1;
        }
    }
    let votes_cast = votes.len() as u32;

    let facts = CanonicalFacts {
        eligible_voters: eligible,
        votes_cast,
        approvals,
        rejections,
        abstentions,
        required_approvals: required,
        notice_delivered: true,
        allocation_requested,
        allocation_limit: alloc_limit,
        reservation_not_consumed,
    };

    let standing_context_hash = hash_postcard(BaselineReceipt::STANDING_DOMAIN, &members);
    let notice_sorted = {
        let mut n = notices.clone();
        n.sort();
        n
    };
    let notice_set_hash = hash_postcard(BaselineReceipt::NOTICE_DOMAIN, &notice_sorted);

    let mut vote_tuples = votes.clone();
    vote_tuples.sort_by_key(|a| a.0);
    let vote_set_root = hash_postcard(BaselineReceipt::DELIB_DOMAIN, &vote_tuples);

    let tip = *receipt_hashes.last().ok_or(ProjectorError::EmptyChain)?;

    let capsule = build_capsule(
        receipt_hashes.clone(),
        tip,
        standing_context_hash,
        notice_set_hash,
        vote_set_root,
    );
    let capsule_hash = capsule.compute_hash();

    let canonical_fact_snapshot_hash = hash_postcard(b"icn:baseline:canonical_facts:v1", &facts);
    let authority_context_hash =
        Hash(*blake3::hash(b"icn:baseline:authority_ctx:fixture").as_bytes());
    let agreement_context_hash =
        Hash(*blake3::hash(b"icn:baseline:agreement_ctx:fixture").as_bytes());

    if facts.approvals < facts.required_approvals {
        return Err(ProjectorError::ThresholdNotMet);
    }

    Ok(ProjectedState {
        facts,
        capsule,
        capsule_hash,
        canonical_fact_snapshot_hash,
        authority_context_hash,
        agreement_context_hash,
    })
}
