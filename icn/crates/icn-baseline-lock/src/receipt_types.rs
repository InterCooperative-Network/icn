//! Deterministic signed receipt DAG for the baseline fixture (test-only).

use blake3::Hasher;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::constants::{PROCESS_ID, TARGET_REF};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BaselineReceiptBody {
    ProcessSessionOpened {
        eligible_members: u32,
        allocation_limit: u64,
        required_approvals: u32,
    },
    StandingContextSnapshot {
        member_pubkeys: Vec<[u8; 32]>,
    },
    NoticeDelivered {
        member_pubkey: [u8; 32],
    },
    DeliberationEntryRecorded {
        member_pubkey: [u8; 32],
        approve: bool,
    },
    AllocationRequested {
        amount: u64,
    },
    ReservationState {
        not_consumed: bool,
    },
}

#[derive(Debug, Clone)]
pub struct BaselineReceipt {
    pub prior_receipt_hash: [u8; 32],
    pub body: BaselineReceiptBody,
    pub signer: [u8; 32],
    pub signature: [u8; 64],
    pub receipt_hash: [u8; 32],
}

impl BaselineReceipt {
    pub const SESSION_DOMAIN: &[u8] = b"icn:baseline:process_session_opened:v1";
    pub const STANDING_DOMAIN: &[u8] = b"icn:baseline:standing_snapshot:v1";
    pub const NOTICE_DOMAIN: &[u8] = b"icn:baseline:notice_delivered:v1";
    pub const DELIB_DOMAIN: &[u8] = b"icn:baseline:deliberation_entry:v1";
    pub const ALLOC_DOMAIN: &[u8] = b"icn:baseline:allocation_requested:v1";
    pub const RES_DOMAIN: &[u8] = b"icn:baseline:reservation_state:v1";

    pub fn domain_for_body(body: &BaselineReceiptBody) -> &'static [u8] {
        match body {
            BaselineReceiptBody::ProcessSessionOpened { .. } => Self::SESSION_DOMAIN,
            BaselineReceiptBody::StandingContextSnapshot { .. } => Self::STANDING_DOMAIN,
            BaselineReceiptBody::NoticeDelivered { .. } => Self::NOTICE_DOMAIN,
            BaselineReceiptBody::DeliberationEntryRecorded { .. } => Self::DELIB_DOMAIN,
            BaselineReceiptBody::AllocationRequested { .. } => Self::ALLOC_DOMAIN,
            BaselineReceiptBody::ReservationState { .. } => Self::RES_DOMAIN,
        }
    }

    pub fn compute_receipt_hash(
        domain: &[u8],
        prior: &[u8; 32],
        process_id: &str,
        target_ref: &str,
        body: &BaselineReceiptBody,
    ) -> [u8; 32] {
        let body_bytes = postcard::to_allocvec(body).expect("baseline receipt body must serialize");
        let mut h = Hasher::new();
        h.update(domain);
        h.update(prior);
        h.update(&(process_id.len() as u64).to_le_bytes());
        h.update(process_id.as_bytes());
        h.update(&(target_ref.len() as u64).to_le_bytes());
        h.update(target_ref.as_bytes());
        h.update(&body_bytes);
        *h.finalize().as_bytes()
    }

    pub fn sign(
        domain: &[u8],
        prior: &[u8; 32],
        process_id: &str,
        target_ref: &str,
        body: BaselineReceiptBody,
        signer: &SigningKey,
    ) -> Self {
        let receipt_hash = Self::compute_receipt_hash(domain, prior, process_id, target_ref, &body);
        let sig = signer.sign(&receipt_hash);
        let mut sigb = [0u8; 64];
        sigb.copy_from_slice(&sig.to_bytes());
        Self {
            prior_receipt_hash: *prior,
            body,
            signer: signer.verifying_key().to_bytes(),
            signature: sigb,
            receipt_hash,
        }
    }

    pub fn verify(&self, process_id: &str, target_ref: &str) -> bool {
        let domain = Self::domain_for_body(&self.body);
        let expect = Self::compute_receipt_hash(
            domain,
            &self.prior_receipt_hash,
            process_id,
            target_ref,
            &self.body,
        );
        if expect != self.receipt_hash {
            return false;
        }
        let vk = match VerifyingKey::from_bytes(&self.signer) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let mut sigb = [0u8; 64];
        sigb.copy_from_slice(&self.signature);
        let sig = Signature::from_bytes(&sigb);
        vk.verify(&self.receipt_hash, &sig).is_ok()
    }
}

pub fn signing_key_for_label(label: &[u8]) -> SigningKey {
    let h = blake3::hash(label);
    let mut b = [0u8; 32];
    b.copy_from_slice(h.as_bytes());
    SigningKey::from_bytes(&b)
}

pub struct FixtureKeys {
    pub coop: SigningKey,
    pub host: SigningKey,
    pub member_a: SigningKey,
    pub member_b: SigningKey,
    pub member_c: SigningKey,
}

impl Default for FixtureKeys {
    fn default() -> Self {
        Self {
            coop: signing_key_for_label(b"baseline-coop"),
            host: signing_key_for_label(b"baseline-host"),
            member_a: signing_key_for_label(b"baseline-member-a"),
            member_b: signing_key_for_label(b"baseline-member-b"),
            member_c: signing_key_for_label(b"baseline-member-c"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FixtureOptions {
    pub broken_prior_link_index: Option<usize>,
    pub wrong_process_id_globally: bool,
    pub wrong_target_ref_globally: bool,
    pub invalid_signature_receipt_index: Option<usize>,
    pub skip_notice_index: Option<usize>,
    pub duplicate_vote_member: Option<usize>,
    /// Second deliberation receipt: body names `member_a`, signed by a non-member key (spoof).
    pub spoof_vote_signer: bool,
    pub non_member_vote: bool,
    pub omit_standing: bool,
    pub omit_reservation: bool,
    pub double_reservation_consumed: bool,
    pub allocation_over_limit: bool,
    pub threshold_fail: bool,
}

fn append_signed(
    out: &mut Vec<BaselineReceipt>,
    prior: &mut [u8; 32],
    opt: &FixtureOptions,
    domain: &[u8],
    process_id: &str,
    target_ref: &str,
    body: BaselineReceiptBody,
    signer: &SigningKey,
) {
    let mut r = BaselineReceipt::sign(domain, prior, process_id, target_ref, body, signer);
    if opt.invalid_signature_receipt_index == Some(out.len()) {
        r.signature[0] ^= 0xFF;
    }
    *prior = r.receipt_hash;
    out.push(r);
}

/// Which standing member (0 = A, 1 = B, 2 = C) is named in vote slot `i` when duplicating member `d`.
fn vote_member_index_for_slot(i: usize, dup_member: usize) -> usize {
    if dup_member == 2 {
        if i == 0 {
            0
        } else {
            2
        }
    } else if i <= dup_member {
        i
    } else if i == dup_member + 1 {
        dup_member
    } else {
        i
    }
}

fn vote_approve_for_slot(i: usize, dup_member: usize, votes: &[(&SigningKey, bool); 3]) -> bool {
    if dup_member == 2 {
        if i == 0 {
            votes[0].1
        } else {
            votes[2].1
        }
    } else if i == dup_member + 1 && dup_member < 2 {
        votes[dup_member].1
    } else {
        votes[i].1
    }
}

/// Build the canonical receipt chain (plus optional faults).
pub fn build_receipt_chain(keys: &FixtureKeys, opt: &FixtureOptions) -> Vec<BaselineReceipt> {
    let process_id = if opt.wrong_process_id_globally {
        "wrong-process-id"
    } else {
        PROCESS_ID
    };
    let target_ref = if opt.wrong_target_ref_globally {
        "wrong-target-ref"
    } else {
        TARGET_REF
    };

    let mut prior = [0u8; 32];
    let mut out = Vec::new();

    append_signed(
        &mut out,
        &mut prior,
        opt,
        BaselineReceipt::SESSION_DOMAIN,
        process_id,
        target_ref,
        BaselineReceiptBody::ProcessSessionOpened {
            eligible_members: 3,
            allocation_limit: 1000,
            required_approvals: 2,
        },
        &keys.coop,
    );

    if !opt.omit_standing {
        append_signed(
            &mut out,
            &mut prior,
            opt,
            BaselineReceipt::STANDING_DOMAIN,
            process_id,
            target_ref,
            BaselineReceiptBody::StandingContextSnapshot {
                member_pubkeys: vec![
                    keys.member_a.verifying_key().to_bytes(),
                    keys.member_b.verifying_key().to_bytes(),
                    keys.member_c.verifying_key().to_bytes(),
                ],
            },
            &keys.host,
        );
    }

    for (i, k) in [&keys.member_a, &keys.member_b, &keys.member_c]
        .iter()
        .enumerate()
    {
        if opt.skip_notice_index == Some(i) {
            continue;
        }
        append_signed(
            &mut out,
            &mut prior,
            opt,
            BaselineReceipt::NOTICE_DOMAIN,
            process_id,
            target_ref,
            BaselineReceiptBody::NoticeDelivered {
                member_pubkey: k.verifying_key().to_bytes(),
            },
            &keys.coop,
        );
    }

    let votes: [(&SigningKey, bool); 3] = if opt.threshold_fail {
        [
            (&keys.member_a, true),
            (&keys.member_b, false),
            (&keys.member_c, false),
        ]
    } else {
        [
            (&keys.member_a, true),
            (&keys.member_b, true),
            (&keys.member_c, false),
        ]
    };

    let member_sk = [&keys.member_a, &keys.member_b, &keys.member_c];

    for i in 0..3 {
        let (body, signer): (BaselineReceiptBody, &SigningKey) = if opt.non_member_vote && i == 2 {
            let approve = votes[i].1;
            (
                BaselineReceiptBody::DeliberationEntryRecorded {
                    member_pubkey: signing_key_for_label(b"non-member-stranger")
                        .verifying_key()
                        .to_bytes(),
                    approve,
                },
                &signing_key_for_label(b"non-member-stranger"),
            )
        } else if opt.spoof_vote_signer && i == 1 {
            (
                BaselineReceiptBody::DeliberationEntryRecorded {
                    member_pubkey: keys.member_a.verifying_key().to_bytes(),
                    approve: votes[1].1,
                },
                &signing_key_for_label(b"vote-signer-spoof-stranger"),
            )
        } else if let Some(dup) = opt.duplicate_vote_member {
            let idx = vote_member_index_for_slot(i, dup);
            let sk = member_sk[idx];
            let approve = vote_approve_for_slot(i, dup, &votes);
            (
                BaselineReceiptBody::DeliberationEntryRecorded {
                    member_pubkey: sk.verifying_key().to_bytes(),
                    approve,
                },
                sk,
            )
        } else {
            let (sk, approve) = votes[i];
            (
                BaselineReceiptBody::DeliberationEntryRecorded {
                    member_pubkey: sk.verifying_key().to_bytes(),
                    approve,
                },
                sk,
            )
        };
        append_signed(
            &mut out,
            &mut prior,
            opt,
            BaselineReceipt::DELIB_DOMAIN,
            process_id,
            target_ref,
            body,
            signer,
        );
    }

    let amount = if opt.allocation_over_limit { 2000 } else { 500 };
    append_signed(
        &mut out,
        &mut prior,
        opt,
        BaselineReceipt::ALLOC_DOMAIN,
        process_id,
        target_ref,
        BaselineReceiptBody::AllocationRequested { amount },
        &keys.coop,
    );

    if !opt.omit_reservation {
        let not_consumed = !opt.double_reservation_consumed;
        append_signed(
            &mut out,
            &mut prior,
            opt,
            BaselineReceipt::RES_DOMAIN,
            process_id,
            target_ref,
            BaselineReceiptBody::ReservationState { not_consumed },
            &keys.host,
        );
    }

    if let Some(i) = opt.broken_prior_link_index {
        if i < out.len() {
            out[i].prior_receipt_hash = [0xEE; 32];
        }
    }

    out
}
