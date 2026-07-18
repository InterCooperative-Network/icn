//! Mechanical receipt-chain verification (ADR-0026 "re-verifiable" invariant).
//!
//! ADR-0026 states that a receipt is *re-verifiable*: "anyone with the public
//! keys can verify the signature chain … proof hashes bind the set." The typed
//! receipt classes in [`crate::proof`] each carry the primitives to enforce this
//! (`compute_*_hash`, `verify`, `verify_binding`, `verify_signature`), but before
//! this module nothing on the durable **read / audit / export** path actually
//! called them: `icnctl audit verify` compared claimed hash *strings*, and the
//! gateway receipt store hydrated records with `serde_json::from_slice` without
//! recomputing a single hash (a byte-altered-but-self-consistent record passed).
//!
//! This module supplies the missing mechanical verifier. It is **pure** — it
//! takes already-decoded receipt values and returns a structured verdict; it
//! constructs no store, performs no I/O, and reuses the canonical hashing in
//! [`crate::proof`] rather than defining a second cryptography.
//!
//! ## What a verdict does and does NOT mean
//!
//! This verifier answers exactly one question per check — *is this record
//! internally consistent and, where a key is supplied, authentically signed?* It
//! deliberately does **not** conflate:
//!
//! - **Integrity** — the record's claimed hash matches its own canonical bytes.
//! - **Authenticity** — a signature over that hash verifies under a supplied key.
//! - **Authorization** — the signer was permitted to record this. *(out of scope)*
//! - **Legitimacy** — a real institution ratified the underlying act. *(out of scope)*
//!
//! A `Pass` here means integrity (and authenticity when a key was available). It
//! is **not** evidence that the recorded act was authorized or legitimate; those
//! are governance facts, not hash facts. Do not let a green verifier launder an
//! unauthorized or illegitimate act.
//!
//! Truth class of this module: implemented + unit-tested. It is not wired into
//! every audit surface yet (see the accompanying spec
//! `docs/spec/receipt-chain-verification.md` for the wiring plan and the
//! per-ladder-class recompute shims that remain follow-on work).

use crate::proof::{GovernanceDecisionReceipt, GovernanceProof, Hash};

/// Outcome of a single verification check.
///
/// The taxonomy is deliberately four-valued and fail-closed: an evaluator that
/// cannot *safely* decide a check reports [`Unresolved`](VerificationStatus::Unresolved)
/// (missing input, e.g. no key material) or fails, never a silent pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    /// The check was evaluated and the record is consistent/authentic.
    Pass,
    /// The check was evaluated and the record is inconsistent/inauthentic.
    Fail,
    /// The check could not be evaluated because a required input was absent
    /// (e.g. no verifying key). This is NOT a pass — the caller must treat it
    /// as "unproven," never "proven good."
    Unresolved,
    /// The check does not apply to this record class (e.g. a signature check on
    /// a deterministic, unsigned receipt).
    NotApplicable,
}

impl VerificationStatus {
    /// Fail-closed severity ordering used to fold a set of checks into one
    /// overall status: `Fail` (worst) > `Unresolved` > `Pass` >
    /// `NotApplicable` (least). Any failure dominates; any *unproven*
    /// (`Unresolved`) check keeps the whole set from reading as proven; but a
    /// `NotApplicable` check — one that genuinely does not apply, e.g. a
    /// signature check on an unsigned deterministic receipt — must **not** drag
    /// an otherwise-passing set below `Pass`. So a set of `{Pass, NotApplicable}`
    /// folds to `Pass`, while a set that is entirely `NotApplicable` folds to
    /// `NotApplicable` (nothing applicable was checked).
    fn severity(self) -> u8 {
        match self {
            VerificationStatus::Fail => 3,
            VerificationStatus::Unresolved => 2,
            VerificationStatus::Pass => 1,
            VerificationStatus::NotApplicable => 0,
        }
    }
}

/// A named check and its outcome.
#[derive(Debug, Clone)]
pub struct CheckVerdict {
    /// Stable identifier for the check performed.
    pub check: &'static str,
    /// Result of the check.
    pub status: VerificationStatus,
    /// Human-readable detail (never contains secrets or private key material).
    pub detail: String,
}

impl CheckVerdict {
    fn new(check: &'static str, status: VerificationStatus, detail: impl Into<String>) -> Self {
        Self {
            check,
            status,
            detail: detail.into(),
        }
    }
    fn pass(check: &'static str, detail: impl Into<String>) -> Self {
        Self::new(check, VerificationStatus::Pass, detail)
    }
    fn fail(check: &'static str, detail: impl Into<String>) -> Self {
        Self::new(check, VerificationStatus::Fail, detail)
    }
    fn unresolved(check: &'static str, detail: impl Into<String>) -> Self {
        Self::new(check, VerificationStatus::Unresolved, detail)
    }
}

/// Fold a set of checks into one status. Empty input is fail-closed to
/// `Unresolved` (nothing was actually verified), never `Pass`.
pub fn overall_status(checks: &[CheckVerdict]) -> VerificationStatus {
    checks
        .iter()
        .map(|c| c.status)
        .max_by_key(|s| s.severity())
        .unwrap_or(VerificationStatus::Unresolved)
}

/// Verify a deterministic [`GovernanceDecisionReceipt`].
///
/// Recomputes `decision_hash` from the receipt's own canonical fields via the
/// existing [`GovernanceDecisionReceipt::verify`] path and compares to the
/// claimed value. This is a pure **integrity** check; the type carries no
/// signature, so authenticity is [`NotApplicable`](VerificationStatus::NotApplicable).
pub fn verify_decision_receipt(receipt: &GovernanceDecisionReceipt) -> Vec<CheckVerdict> {
    let mut checks = Vec::new();
    if receipt.verify() {
        checks.push(CheckVerdict::pass(
            "decision_hash_integrity",
            "recomputed decision_hash matches claimed value",
        ));
    } else {
        checks.push(CheckVerdict::fail(
            "decision_hash_integrity",
            "recomputed decision_hash does NOT match claimed value (tampered or corrupt)",
        ));
    }
    checks.push(CheckVerdict::new(
        "signature",
        VerificationStatus::NotApplicable,
        "GovernanceDecisionReceipt is deterministic and unsigned; authenticity via GovernanceProof",
    ));
    checks
}

/// Verify a legacy signed [`GovernanceProof`].
///
/// Performs the proof-hash **binding** check (integrity) unconditionally, and a
/// **signature** check (authenticity) only when a verifying key is supplied —
/// otherwise the signature check is [`Unresolved`](VerificationStatus::Unresolved),
/// never a pass. This mirrors the check the gossip-ingress path already performs,
/// but makes it available on the read/audit path.
pub fn verify_governance_proof(
    proof: &GovernanceProof,
    verifying_key: Option<&ed25519_dalek::VerifyingKey>,
) -> Vec<CheckVerdict> {
    let mut checks = Vec::new();

    if proof.verify_binding() {
        checks.push(CheckVerdict::pass(
            "proof_hash_binding",
            "recomputed proof_hash matches claimed value",
        ));
    } else {
        checks.push(CheckVerdict::fail(
            "proof_hash_binding",
            "recomputed proof_hash does NOT match claimed value (tampered or corrupt)",
        ));
    }

    match verifying_key {
        Some(vk) => {
            if proof.verify_signature(vk) {
                checks.push(CheckVerdict::pass(
                    "signature",
                    "signature verifies under supplied key",
                ));
            } else {
                checks.push(CheckVerdict::fail(
                    "signature",
                    "signature does NOT verify under supplied key",
                ));
            }
        }
        None => checks.push(CheckVerdict::unresolved(
            "signature",
            "no verifying key supplied; authenticity unproven (fail-closed, not a pass)",
        )),
    }

    checks
}

/// A minimal link in a receipt chain: a record and the predecessor it claims to
/// follow. `prior` is `None` for a genesis/first record.
///
/// This is the generic shape shared by the ADR-0026 ladder (activation →
/// mutation-plan → mutation-applied) and by domain-policy adoption receipts
/// (`record_hash` / `supersedes`). Verifying links is orthogonal to verifying
/// each record's own hash — a chain of individually-valid records can still be
/// mis-linked.
#[derive(Debug, Clone, Copy)]
pub struct ChainLink {
    /// The record's own canonical hash.
    pub record_hash: Hash,
    /// The `record_hash` this record claims to supersede/follow, if any.
    pub prior: Option<Hash>,
}

/// Verify that an ordered chain of links is internally consistent: each
/// non-genesis link's `prior` must equal the immediately preceding link's
/// `record_hash`, and exactly the first link may be genesis (`prior == None`).
///
/// Fail-closed: a genesis marker appearing mid-chain, or a broken back-pointer,
/// is a `Fail`. An empty chain is `NotApplicable` (nothing to link).
pub fn verify_chain_links(links: &[ChainLink]) -> Vec<CheckVerdict> {
    let mut checks = Vec::new();
    if links.is_empty() {
        checks.push(CheckVerdict::new(
            "chain_links",
            VerificationStatus::NotApplicable,
            "no links to verify",
        ));
        return checks;
    }

    // Genesis: only the first link may have no predecessor.
    if links[0].prior.is_some() {
        checks.push(CheckVerdict::fail(
            "chain_genesis",
            "first link claims a predecessor but has no antecedent in this chain",
        ));
    } else {
        checks.push(CheckVerdict::pass(
            "chain_genesis",
            "first link is genesis (no predecessor)",
        ));
    }

    for (i, link) in links.iter().enumerate().skip(1) {
        let expected = links[i - 1].record_hash;
        match link.prior {
            None => checks.push(CheckVerdict::fail(
                "chain_link",
                format!("link {i} is a mid-chain genesis (missing predecessor pointer)"),
            )),
            Some(prior) if prior == expected => checks.push(CheckVerdict::pass(
                "chain_link",
                format!("link {i} correctly supersedes its predecessor"),
            )),
            Some(_) => checks.push(CheckVerdict::fail(
                "chain_link",
                format!("link {i} predecessor pointer does not match the prior record_hash"),
            )),
        }
    }

    checks
}

/// A record's *claimed* identity paired with the hash *recomputed* from its
/// content. Two records that assert the same claimed hash but recompute to
/// different content are a hash collision / forgery attempt.
#[derive(Debug, Clone, Copy)]
pub struct HashClaim {
    /// The record_hash the entry asserts as its identity.
    pub claimed: Hash,
    /// The hash recomputed from the entry's canonical content.
    pub recomputed: Hash,
}

/// Detect two failure modes across a set of records sharing (or asserting) a
/// hash identity:
///
/// 1. any entry whose `claimed != recomputed` (integrity failure), and
/// 2. two entries asserting the same `claimed` hash whose `recomputed` content
///    differs (a collision: one canonical identity mapped to two payloads).
pub fn detect_hash_conflicts(entries: &[HashClaim]) -> Vec<CheckVerdict> {
    let mut checks = Vec::new();

    for (i, e) in entries.iter().enumerate() {
        if e.claimed != e.recomputed {
            checks.push(CheckVerdict::fail(
                "record_integrity",
                format!("entry {i}: recomputed content hash does not match claimed identity"),
            ));
        }
    }

    // Collision: same claimed identity, differing recomputed content.
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            if entries[i].claimed == entries[j].claimed
                && entries[i].recomputed != entries[j].recomputed
            {
                checks.push(CheckVerdict::fail(
                    "hash_collision",
                    format!(
                        "entries {i} and {j} assert the same record_hash but differ in content"
                    ),
                ));
            }
        }
    }

    if checks.is_empty() {
        checks.push(CheckVerdict::pass(
            "record_integrity",
            "all entries: claimed identity matches recomputed content; no collisions",
        ));
    }

    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proof::ProofOutcome;
    use crate::tally::VoteTally;

    fn tally() -> VoteTally {
        VoteTally {
            for_votes: 3,
            against_votes: 1,
            abstain_votes: 0,
        }
    }

    fn valid_decision_receipt() -> GovernanceDecisionReceipt {
        // Empty vote set is fine: the receipt is self-consistent over whatever
        // vote_hash `new` computes. We are testing hash re-verification, not tally.
        GovernanceDecisionReceipt::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally(),
            &[],
        )
    }

    // ---- Slice A core falsification test: tamper a byte -> FAIL --------------

    #[test]
    fn valid_decision_receipt_passes() {
        let r = valid_decision_receipt();
        let checks = verify_decision_receipt(&r);
        // A valid unsigned decision receipt has integrity=Pass and signature=N/A;
        // N/A must not drag the overall verdict below Pass.
        assert_eq!(overall_status(&checks), VerificationStatus::Pass);
        // The integrity check specifically must PASS.
        let integrity = checks
            .iter()
            .find(|c| c.check == "decision_hash_integrity")
            .expect("integrity check present");
        assert_eq!(integrity.status, VerificationStatus::Pass);
        // The signature check is genuinely N/A for this unsigned type.
        let sig = checks
            .iter()
            .find(|c| c.check == "signature")
            .expect("signature check present");
        assert_eq!(sig.status, VerificationStatus::NotApplicable);
    }

    #[test]
    fn all_not_applicable_folds_to_not_applicable() {
        // A set with nothing applicable folds to NotApplicable, not Pass.
        let checks = vec![CheckVerdict::new(
            "x",
            VerificationStatus::NotApplicable,
            "n/a",
        )];
        assert_eq!(overall_status(&checks), VerificationStatus::NotApplicable);
    }

    #[test]
    fn tampered_decision_field_without_rehash_fails() {
        // The exact attack the old string-comparison `icnctl audit verify`
        // could not catch: mutate a field, keep the stored decision_hash.
        let mut r = valid_decision_receipt();
        r.outcome = ProofOutcome::Rejected; // flip the recorded outcome
                                            // decision_hash left as-is (the "byte" not updated)
        let checks = verify_decision_receipt(&r);
        let integrity = checks
            .iter()
            .find(|c| c.check == "decision_hash_integrity")
            .expect("integrity check present");
        assert_eq!(
            integrity.status,
            VerificationStatus::Fail,
            "tampered receipt must FAIL integrity"
        );
        assert_eq!(overall_status(&checks), VerificationStatus::Fail);
    }

    #[test]
    fn tampered_decision_hash_bytes_fail() {
        let mut r = valid_decision_receipt();
        r.decision_hash[0] ^= 0xff; // corrupt the claimed identity directly
        let checks = verify_decision_receipt(&r);
        assert_eq!(overall_status(&checks), VerificationStatus::Fail);
    }

    // ---- Legacy signed proof: binding + signature ---------------------------

    fn signed_proof() -> (GovernanceProof, ed25519_dalek::VerifyingKey) {
        let signer = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        let vk = signer.verifying_key();
        let mut p = GovernanceProof::new(
            "prop-1".to_string(),
            "coop:test".to_string(),
            ProofOutcome::Accepted,
            tally(),
            &[],
            1_800_000_000,
            "did:icn:nodeA".to_string(),
        );
        p.sign(&signer);
        (p, vk)
    }

    #[test]
    fn valid_signed_proof_passes_with_key() {
        let (p, vk) = signed_proof();
        let checks = verify_governance_proof(&p, Some(&vk));
        assert_eq!(overall_status(&checks), VerificationStatus::Pass);
    }

    #[test]
    fn proof_without_key_is_unresolved_not_pass() {
        let (p, _vk) = signed_proof();
        let checks = verify_governance_proof(&p, None);
        // Binding passes; signature is unproven -> overall Unresolved (fail-closed).
        assert_eq!(overall_status(&checks), VerificationStatus::Unresolved);
    }

    #[test]
    fn tampered_proof_binding_fails() {
        let (mut p, vk) = signed_proof();
        p.proposal_id = "prop-EVIL".to_string(); // change content, keep proof_hash
        let checks = verify_governance_proof(&p, Some(&vk));
        let binding = checks
            .iter()
            .find(|c| c.check == "proof_hash_binding")
            .expect("binding check present");
        assert_eq!(binding.status, VerificationStatus::Fail);
        assert_eq!(overall_status(&checks), VerificationStatus::Fail);
    }

    #[test]
    fn wrong_key_signature_fails() {
        let (p, _vk) = signed_proof();
        let other = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]).verifying_key();
        let checks = verify_governance_proof(&p, Some(&other));
        let sig = checks
            .iter()
            .find(|c| c.check == "signature")
            .expect("signature check present");
        assert_eq!(sig.status, VerificationStatus::Fail);
    }

    // ---- Chain link verification -------------------------------------------

    #[test]
    fn valid_chain_links_pass() {
        let h1 = [1u8; 32];
        let h2 = [2u8; 32];
        let h3 = [3u8; 32];
        let links = vec![
            ChainLink {
                record_hash: h1,
                prior: None,
            },
            ChainLink {
                record_hash: h2,
                prior: Some(h1),
            },
            ChainLink {
                record_hash: h3,
                prior: Some(h2),
            },
        ];
        assert_eq!(
            overall_status(&verify_chain_links(&links)),
            VerificationStatus::Pass
        );
    }

    #[test]
    fn broken_supersession_link_fails() {
        let h1 = [1u8; 32];
        let h2 = [2u8; 32];
        let h3 = [3u8; 32];
        let links = vec![
            ChainLink {
                record_hash: h1,
                prior: None,
            },
            ChainLink {
                record_hash: h2,
                prior: Some(h1),
            },
            // h3 claims to follow h1, not h2 -> broken.
            ChainLink {
                record_hash: h3,
                prior: Some(h1),
            },
        ];
        assert_eq!(
            overall_status(&verify_chain_links(&links)),
            VerificationStatus::Fail
        );
    }

    #[test]
    fn mid_chain_genesis_fails() {
        let h1 = [1u8; 32];
        let h2 = [2u8; 32];
        let links = vec![
            ChainLink {
                record_hash: h1,
                prior: None,
            },
            ChainLink {
                record_hash: h2,
                prior: None,
            }, // second genesis
        ];
        assert_eq!(
            overall_status(&verify_chain_links(&links)),
            VerificationStatus::Fail
        );
    }

    // ---- Collision / integrity detection -----------------------------------

    #[test]
    fn hash_collision_detected() {
        let claimed = [7u8; 32];
        let entries = vec![
            HashClaim {
                claimed,
                recomputed: claimed,
            },
            HashClaim {
                claimed,
                recomputed: [8u8; 32],
            }, // same identity, different content
        ];
        assert_eq!(
            overall_status(&detect_hash_conflicts(&entries)),
            VerificationStatus::Fail
        );
    }

    #[test]
    fn mismatched_claim_detected() {
        let entries = vec![HashClaim {
            claimed: [7u8; 32],
            recomputed: [8u8; 32],
        }];
        assert_eq!(
            overall_status(&detect_hash_conflicts(&entries)),
            VerificationStatus::Fail
        );
    }

    #[test]
    fn empty_checks_are_fail_closed() {
        // A verifier that evaluated nothing must never read as proven.
        assert_eq!(overall_status(&[]), VerificationStatus::Unresolved);
    }
}
