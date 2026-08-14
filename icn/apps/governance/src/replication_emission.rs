//! Signed governance emission — the sending half of #2469 (slice 3).
//!
//! Decides whether an outbound [`GovernanceMessage`] can be wrapped in a
//! [`SignedGovernanceOp`] and, if so, builds the frame. Everything here is **emission only**.
//! No function in this module is reachable from the receive path, and slice 3 restores no
//! remote state application whatsoever: the #2470 containment is untouched.
//!
//! # The two-phase rule
//!
//! Eligibility and commitment fail in opposite directions, deliberately:
//!
//! - **Eligibility is best-effort.** Any reason we cannot establish a signed envelope —
//!   unresolvable domain, non-`StaticList` membership, absent key, an author we do not hold
//!   the key for — degrades to the legacy payload, which is byte-identical to what this node
//!   published before slice 3. Nothing regresses, and the common `cooperative_default()`
//!   profile (`trust_threshold(0.3)`) keeps working exactly as it does today.
//! - **Commitment is fail-closed.** Once we have decided to sign, the sequence must be
//!   durably recorded before the frame goes out. A failure there aborts the publish rather
//!   than quietly downgrading, because a signed operation carrying an unrecorded sequence is
//!   an ambiguity no receiver can detect (see [`crate::replication_sequence`]).
//!
//! # Who may be named as author
//!
//! The envelope's `author` is the **acting principal** — the member whose authority the
//! operation claims — not the node that relays it. `SignedGovernanceOp::sign` enforces this
//! by refusing any key that does not derive `author`, and the ingress design
//! (`docs/design/authenticated-governance-replication.md` §5.6 step 5) re-checks the op's
//! internal principal against it.
//!
//! A `GovernanceActor` holds exactly one key: the node's own identity key
//! (`icn-core/src/supervisor/lifecycle.rs:782`). It therefore cannot sign for anyone else.
//! `GovernanceCommand::CastVote` accepts a `voter` parameter and never constrains it to
//! `self.did`, so in the gateway composition — many members voting through one node — the
//! node holds none of their keys and every vote takes the legacy path. That is a custody
//! fact, not a gap to be patched: signing a member's vote with the node's key would assert an
//! authorship that does not exist, which is exactly the forgery this envelope prevents.
//!
//! # Eligible is not restorable
//!
//! Several op kinds can be signed here that no v1 receiver may ever apply
//! (`V1_RESTORABLE_OP_KINDS` is `[VoteCast]` alone). Signing them costs nothing and keeps one
//! emission rule instead of two, but it confers no application rights: restorability is
//! decided at ingress, in a later slice.

use anyhow::{Context, Result};

use icn_governance::replication::{
    static_membership_hash, GovernanceOpKind, ReplicationAuthority, SignedGovernanceOp,
};
use icn_governance::{
    DelegationScope, GovernanceDomainId, GovernanceMessage, MembershipSource, ProposalId,
};
use icn_identity::Did;

use crate::replication_sequence::GovernanceSequenceCounter;
use crate::state_store::GovernanceStateStore;

/// Why an outbound message was not signed.
///
/// Carried rather than merely logged so tests can assert *which* guard fired. A test that
/// only asserted "the payload is legacy" would pass just as happily if signing were broken
/// outright, or if the wrong guard rejected it for the wrong reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyReason {
    /// The message is not a replicable state mutation (deliberation, comments, reactions).
    NotReplicable,
    /// The operation has no acting principal in the message, or none that v1 recognises as
    /// an authority root — `DomainCreated`/`DomainUpdated` (no root exists) and
    /// `ProposalOpened`/`ProposalClosed` (root undecided; see design §7.2).
    NoActingPrincipal,
    /// The governance domain could not be resolved from authoritative local state, or the
    /// operation spans domains (a `Blanket` delegation).
    DomainUnresolved,
    /// The domain's membership is not a `StaticList`, so no deterministic snapshot exists.
    /// `TrustThreshold` resolves against each node's local trust graph, so two honest nodes
    /// can compute different membership (design §5.5).
    MembershipNotStatic,
    /// No signing key is available — a locked or hardware-backed keystore.
    NoSigningKey,
    /// The available key does not belong to the acting principal. Includes every
    /// gateway-hosted operation.
    KeyNotPrincipals,
}

impl LegacyReason {
    /// Stable label for logs and metrics.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotReplicable => "not_replicable",
            Self::NoActingPrincipal => "no_acting_principal",
            Self::DomainUnresolved => "domain_unresolved",
            Self::MembershipNotStatic => "membership_not_static",
            Self::NoSigningKey => "no_signing_key",
            Self::KeyNotPrincipals => "key_not_principals",
        }
    }
}

/// What a message is eligible for, before any sequence is consumed.
///
/// Split from frame construction so the decision can be tested without a durable counter,
/// and so no sequence is burned deciding that an operation is ineligible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eligibility {
    /// Sign as `author` over `domain_id`, under this membership snapshot.
    Signed {
        author: Did,
        domain_id: GovernanceDomainId,
        authority: ReplicationAuthority,
    },
    /// Emit the legacy payload unchanged.
    Legacy(LegacyReason),
}

/// The acting principal an operation claims authority from, if the message names one.
///
/// `DomainCreated`/`DomainUpdated` name none because no authority root for domain mutation
/// exists at all (design fact 8). `ProposalOpened`/`ProposalClosed` name none because their
/// authority root is a tally, not a member, and is undecided (design §7.2). Returning `None`
/// for those is the fail-closed answer: the alternative is to name the relaying node as
/// author, which would be a claim about authority that nothing in the system supports.
fn acting_principal(msg: &GovernanceMessage) -> Option<Did> {
    match msg {
        GovernanceMessage::VoteCast { vote, .. } => Some(vote.voter.clone()),
        GovernanceMessage::ProposalCreated { proposal } => Some(proposal.proposer.clone()),
        GovernanceMessage::DelegationCreated { delegation } => Some(delegation.delegator.clone()),
        GovernanceMessage::DelegationRevoked { revoked_by, .. } => Some(revoked_by.clone()),
        _ => None,
    }
}

/// Resolve the governance domain an operation belongs to, from **authoritative local state**.
///
/// Never inferred from the topic string. A topic is attacker-influenced on receipt and
/// merely conventional on emission; the proposal and delegation records are the authority on
/// which domain an operation belongs to, and every local command persists them before it
/// publishes.
///
/// A store read failure is reported as `None` (ineligible) rather than propagated. The local
/// write has already succeeded by this point, so degrading to the legacy payload preserves
/// today's exact behaviour, whereas failing the publish would turn a transient read error
/// into a new liveness regression.
fn resolve_domain(
    store: &dyn GovernanceStateStore,
    msg: &GovernanceMessage,
) -> Option<GovernanceDomainId> {
    match msg {
        GovernanceMessage::VoteCast { vote, .. } => domain_of_proposal(store, &vote.proposal_id),
        GovernanceMessage::ProposalCreated { proposal } => domain_of_proposal(store, &proposal.id),
        GovernanceMessage::DelegationCreated { delegation } => {
            domain_of_scope(store, &delegation.scope)
        }
        GovernanceMessage::DelegationRevoked { id, .. } => {
            // The revocation message carries no scope, so the stored delegation is the only
            // authority on which domain it touches.
            let delegation = store.get_delegation(id).ok().flatten()?;
            domain_of_scope(store, &delegation.scope)
        }
        _ => None,
    }
}

fn domain_of_proposal(
    store: &dyn GovernanceStateStore,
    id: &ProposalId,
) -> Option<GovernanceDomainId> {
    store.get_proposal(id).ok().flatten().map(|p| p.domain_id)
}

/// A `Blanket` delegation spans every domain, so it has no single domain to bind. Signing it
/// under any one of them would understate its reach; this is the ambiguous case the design
/// requires to fail closed.
fn domain_of_scope(
    store: &dyn GovernanceStateStore,
    scope: &DelegationScope,
) -> Option<GovernanceDomainId> {
    match scope {
        DelegationScope::Domain(id) => Some(id.clone()),
        DelegationScope::Proposal(pid) => domain_of_proposal(store, pid),
        DelegationScope::Blanket => None,
    }
}

/// Decide whether `msg` can be emitted as a signed operation.
///
/// `local_did` is the node's own DID and `key_did` the DID derived from the available signing
/// key, or `None` when no key is available. Both are checked: the key must belong to the
/// acting principal *and* the node must be that principal. The second check is redundant with
/// the first whenever the key really is the node's own, and it is kept because that is an
/// assumption about wiring rather than a property of this module — if a future composition
/// ever hands the actor a key that is not its own, this fails closed instead of silently
/// emitting under a borrowed identity.
pub fn classify(
    store: &dyn GovernanceStateStore,
    msg: &GovernanceMessage,
    local_did: &Did,
    key_did: Option<&Did>,
) -> Eligibility {
    if GovernanceOpKind::from_message(msg).is_none() {
        return Eligibility::Legacy(LegacyReason::NotReplicable);
    }

    let Some(principal) = acting_principal(msg) else {
        return Eligibility::Legacy(LegacyReason::NoActingPrincipal);
    };

    let Some(domain_id) = resolve_domain(store, msg) else {
        return Eligibility::Legacy(LegacyReason::DomainUnresolved);
    };

    let Some(domain) = store.get_domain(&domain_id).ok().flatten() else {
        return Eligibility::Legacy(LegacyReason::DomainUnresolved);
    };

    let MembershipSource::StaticList(ref members) = domain.config.membership.source else {
        return Eligibility::Legacy(LegacyReason::MembershipNotStatic);
    };

    let Some(key_did) = key_did else {
        return Eligibility::Legacy(LegacyReason::NoSigningKey);
    };

    if key_did != &principal || local_did != &principal {
        return Eligibility::Legacy(LegacyReason::KeyNotPrincipals);
    }

    Eligibility::Signed {
        author: principal,
        domain_id: domain_id.clone(),
        authority: ReplicationAuthority::StaticMembership {
            membership_hash: static_membership_hash(&domain_id, members),
        },
    }
}

/// Build the signed wire frame, consuming one durable sequence.
///
/// The returned bytes become the gossip entry's `data` verbatim. `GossipActor::store_entry`
/// re-derives `entry.hash` from exactly these bytes (#2583), so the frame is bound to its
/// digest with no additional mechanism here — duplicating that binding would create a second
/// thing to keep in sync with it.
///
/// # Errors
///
/// Propagates a sequence-persistence failure. See the module docs: at this point the decision
/// to sign has been made, and emitting an operation whose sequence was never recorded is the
/// one outcome that must not happen silently.
pub fn build_signed_frame(
    signing_key: &ed25519_dalek::SigningKey,
    author: Did,
    domain_id: GovernanceDomainId,
    authority: ReplicationAuthority,
    sequences: &GovernanceSequenceCounter,
    msg: &GovernanceMessage,
) -> Result<Vec<u8>> {
    let seq = sequences
        .next_sequence(&author, &domain_id)
        .context("Failed to allocate a durable governance replication sequence")?;

    let signed = SignedGovernanceOp::sign(signing_key, author, domain_id, authority, seq, msg)
        .context("Failed to sign governance operation")?;

    Ok(signed.encode())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use icn_governance::replication::GOV_OP_MAGIC;
    use icn_governance::{
        Delegation, GovernanceConfig, GovernanceDomain, GovernanceMessage, MembershipConfig,
        Proposal, ProposalId, ProposalPayload, Vote, VoteChoice,
    };
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use std::sync::Arc;

    use crate::state_store::SledGovernanceStateStore;

    /// A store plus the tempdir that backs it — the dir must outlive the store.
    struct Fixture {
        _tmp: tempfile::TempDir,
        store: SledGovernanceStateStore,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        let raw: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(tmp.path()).expect("sled"));
        Fixture {
            _tmp: tmp,
            store: SledGovernanceStateStore::new(raw),
        }
    }

    fn keypair() -> KeyPair {
        KeyPair::generate().unwrap()
    }

    fn signing_key(kp: &KeyPair) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&kp.to_signing_key_bytes())
    }

    /// A `StaticList` domain — the only membership source eligible for a v1 signed envelope.
    fn static_domain(id: &str, members: Vec<Did>) -> GovernanceDomain {
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = MembershipConfig::static_list(members);
        GovernanceDomain {
            id: GovernanceDomainId::new(id),
            name: id.to_string(),
            description: None,
            config,
            created_at: 0,
            updated_at: 0,
        }
    }

    /// The stock cooperative profile. Asserting it is `TrustThreshold` here keeps the
    /// "default profile is ineligible" claim pinned to the profile rather than to a literal
    /// this test happened to write.
    fn default_profile_domain(id: &str) -> GovernanceDomain {
        let domain = GovernanceDomain {
            id: GovernanceDomainId::new(id),
            name: id.to_string(),
            description: None,
            config: GovernanceConfig::cooperative_default(),
            created_at: 0,
            updated_at: 0,
        };
        assert!(
            matches!(
                domain.config.membership.source,
                MembershipSource::TrustThreshold(_)
            ),
            "cooperative_default() is expected to be TrustThreshold; if this changed, the \
             eligibility story in the design doc needs revisiting"
        );
        domain
    }

    fn seeded_proposal(f: &Fixture, domain: &GovernanceDomain, proposer: &Did) -> Proposal {
        let proposal = Proposal::new(
            domain.id.clone(),
            proposer.clone(),
            "t".to_string(),
            "d".to_string(),
            ProposalPayload::Text {
                body: "c".to_string(),
            },
        );
        f.store.save_domain(domain).unwrap();
        f.store.save_proposal(&proposal).unwrap();
        proposal
    }

    fn vote_msg(proposal: &Proposal, voter: &Did) -> GovernanceMessage {
        GovernanceMessage::vote_cast(
            Vote::new(proposal.id.clone(), voter.clone(), VoteChoice::For),
            None,
        )
    }

    #[test]
    fn eligible_static_list_vote_is_signed_with_the_proposals_domain() {
        let f = fixture();
        let kp = keypair();
        let alice = kp.did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        let proposal = seeded_proposal(&f, &domain, &alice);

        match classify(&f.store, &vote_msg(&proposal, &alice), &alice, Some(&alice)) {
            Eligibility::Signed {
                author,
                domain_id,
                authority,
            } => {
                assert_eq!(author, alice);
                assert_eq!(domain_id, domain.id, "domain must come from the proposal");
                assert_eq!(
                    authority,
                    ReplicationAuthority::StaticMembership {
                        membership_hash: static_membership_hash(&domain.id, &[alice]),
                    }
                );
            }
            other => panic!("expected Signed, got {other:?}"),
        }
    }

    /// The domain must come from the proposal record, not from anything the caller supplies.
    /// A proposal stored under a different domain must produce a different binding.
    #[test]
    fn the_bound_domain_tracks_the_stored_proposal() {
        let f = fixture();
        let alice = keypair().did().clone();

        let domain_a = static_domain("coop-a", vec![alice.clone()]);
        let domain_b = static_domain("coop-b", vec![alice.clone()]);
        f.store.save_domain(&domain_b).unwrap();
        let proposal = seeded_proposal(&f, &domain_a, &alice);

        let Eligibility::Signed { domain_id, .. } =
            classify(&f.store, &vote_msg(&proposal, &alice), &alice, Some(&alice))
        else {
            panic!("expected Signed");
        };
        assert_eq!(domain_id, domain_a.id);
        assert_ne!(domain_id, domain_b.id);
    }

    /// The gateway composition: the node relays a vote for a member whose key it does not
    /// hold. It must name neither itself nor the voter as author.
    #[test]
    fn a_vote_cast_by_someone_else_falls_back_to_legacy() {
        let f = fixture();
        let node = keypair().did().clone();
        let bob = keypair().did().clone();
        let domain = static_domain("coop-a", vec![node.clone(), bob.clone()]);
        let proposal = seeded_proposal(&f, &domain, &node);

        assert_eq!(
            classify(&f.store, &vote_msg(&proposal, &bob), &node, Some(&node)),
            Eligibility::Legacy(LegacyReason::KeyNotPrincipals),
        );
    }

    /// A key that is not this node's own must not sign either, even when it *does* match the
    /// acting principal — that would be emitting under a borrowed identity.
    #[test]
    fn a_key_that_is_not_this_nodes_own_does_not_sign() {
        let f = fixture();
        let node = keypair().did().clone();
        let alice = keypair().did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        let proposal = seeded_proposal(&f, &domain, &alice);

        assert_eq!(
            classify(&f.store, &vote_msg(&proposal, &alice), &node, Some(&alice)),
            Eligibility::Legacy(LegacyReason::KeyNotPrincipals),
        );
    }

    #[test]
    fn trust_threshold_domain_falls_back_to_legacy() {
        let f = fixture();
        let alice = keypair().did().clone();
        let domain = default_profile_domain("coop-trust");
        let proposal = seeded_proposal(&f, &domain, &alice);

        assert_eq!(
            classify(&f.store, &vote_msg(&proposal, &alice), &alice, Some(&alice)),
            Eligibility::Legacy(LegacyReason::MembershipNotStatic),
        );
    }

    #[test]
    fn absent_signing_key_falls_back_to_legacy_without_panicking() {
        let f = fixture();
        let alice = keypair().did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        let proposal = seeded_proposal(&f, &domain, &alice);

        assert_eq!(
            classify(&f.store, &vote_msg(&proposal, &alice), &alice, None),
            Eligibility::Legacy(LegacyReason::NoSigningKey),
        );
    }

    #[test]
    fn an_unknown_proposal_leaves_the_domain_unresolved() {
        let f = fixture();
        let alice = keypair().did().clone();
        let orphan = Vote::new(
            ProposalId::new("never-stored"),
            alice.clone(),
            VoteChoice::For,
        );

        assert_eq!(
            classify(
                &f.store,
                &GovernanceMessage::vote_cast(orphan, None),
                &alice,
                Some(&alice)
            ),
            Eligibility::Legacy(LegacyReason::DomainUnresolved),
        );
    }

    #[test]
    fn deliberation_messages_are_not_replicable() {
        let f = fixture();
        let alice = keypair().did().clone();

        assert_eq!(
            classify(
                &f.store,
                &GovernanceMessage::deliberation_started(ProposalId::new("p"), 0, 1),
                &alice,
                Some(&alice)
            ),
            Eligibility::Legacy(LegacyReason::NotReplicable),
        );
    }

    /// `ProposalOpened` / `ProposalClosed` / `Domain*` are replicable kinds with no
    /// member-level authority root, so they must not be signed under the relaying node's
    /// identity even though everything else about them is eligible.
    #[test]
    fn lifecycle_and_domain_messages_have_no_acting_principal() {
        let f = fixture();
        let alice = keypair().did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        let proposal = seeded_proposal(&f, &domain, &alice);

        let messages = [
            GovernanceMessage::proposal_opened(proposal.id.clone(), 0, 1),
            GovernanceMessage::domain_created(domain.clone()),
            GovernanceMessage::domain_updated(domain.clone()),
        ];

        for msg in messages {
            assert_eq!(
                classify(&f.store, &msg, &alice, Some(&alice)),
                Eligibility::Legacy(LegacyReason::NoActingPrincipal),
                "{} must not be signed under the relaying node's identity",
                msg.message_type()
            );
        }
    }

    #[test]
    fn a_blanket_delegation_spans_domains_and_is_not_signed() {
        let f = fixture();
        let alice = keypair().did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        f.store.save_domain(&domain).unwrap();

        let delegation = Delegation::new(
            alice.clone(),
            keypair().did().clone(),
            DelegationScope::Blanket,
        );

        assert_eq!(
            classify(
                &f.store,
                &GovernanceMessage::delegation_created(delegation),
                &alice,
                Some(&alice)
            ),
            Eligibility::Legacy(LegacyReason::DomainUnresolved),
        );
    }

    #[test]
    fn a_domain_scoped_delegation_is_signed_under_that_domain() {
        let f = fixture();
        let alice = keypair().did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        f.store.save_domain(&domain).unwrap();

        let delegation = Delegation::new(
            alice.clone(),
            keypair().did().clone(),
            DelegationScope::Domain(domain.id.clone()),
        );

        match classify(
            &f.store,
            &GovernanceMessage::delegation_created(delegation),
            &alice,
            Some(&alice),
        ) {
            Eligibility::Signed { domain_id, .. } => assert_eq!(domain_id, domain.id),
            other => panic!("expected Signed, got {other:?}"),
        }
    }

    #[test]
    fn the_signed_frame_carries_the_exact_message_and_is_magic_prefixed() {
        let f = fixture();
        let kp = keypair();
        let alice = kp.did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        let proposal = seeded_proposal(&f, &domain, &alice);
        let msg = vote_msg(&proposal, &alice);

        let Eligibility::Signed {
            author,
            domain_id,
            authority,
        } = classify(&f.store, &msg, &alice, Some(&alice))
        else {
            panic!("expected Signed");
        };

        let sequences = GovernanceSequenceCounter::in_memory();
        let frame = build_signed_frame(
            &signing_key(&kp),
            author,
            domain_id,
            authority,
            &sequences,
            &msg,
        )
        .unwrap();

        assert!(
            frame.starts_with(GOV_OP_MAGIC),
            "frame must be recognisable by magic"
        );

        let decoded = SignedGovernanceOp::decode(&frame).unwrap();
        decoded.verify().expect("signature must verify");
        assert_eq!(
            decoded.op_bytes(),
            msg.to_bytes().unwrap(),
            "the signed bytes must be the exact transmitted operation"
        );
        assert_eq!(*decoded.author(), alice);
        assert_eq!(*decoded.domain_id(), domain.id);
        assert_eq!(decoded.seq(), 1);
        // Decodes back to an equivalent message (GovernanceMessage has no PartialEq, so
        // compare on the wire bytes, which is the representation that actually travels).
        assert_eq!(
            decoded.decode_op().unwrap().to_bytes().unwrap(),
            msg.to_bytes().unwrap()
        );
    }

    /// The signature must cover exactly these bytes: mutating the transmitted payload must
    /// break verification rather than silently changing what was authorised.
    #[test]
    fn a_mutated_payload_no_longer_verifies() {
        let f = fixture();
        let kp = keypair();
        let alice = kp.did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        let proposal = seeded_proposal(&f, &domain, &alice);
        let msg = vote_msg(&proposal, &alice);

        let Eligibility::Signed {
            author,
            domain_id,
            authority,
        } = classify(&f.store, &msg, &alice, Some(&alice))
        else {
            panic!("expected Signed");
        };

        let sequences = GovernanceSequenceCounter::in_memory();
        let frame = build_signed_frame(
            &signing_key(&kp),
            author,
            domain_id,
            authority,
            &sequences,
            &msg,
        )
        .unwrap();

        // The op payload sits between the header and the trailing length-prefixed signature
        // (64 bytes + 4 length bytes), so this lands inside the signed body.
        let mut tampered = frame.clone();
        let target = tampered.len() - 68 - 1;
        tampered[target] ^= 0x01;

        match SignedGovernanceOp::decode(&tampered) {
            Ok(op) => assert!(op.verify().is_err(), "tampered frame must not verify"),
            Err(_) => { /* framing rejected it even earlier, which is also fail-closed */ }
        }
    }

    #[test]
    fn each_emission_consumes_a_fresh_sequence() {
        let f = fixture();
        let kp = keypair();
        let alice = kp.did().clone();
        let domain = static_domain("coop-a", vec![alice.clone()]);
        let proposal = seeded_proposal(&f, &domain, &alice);
        let msg = vote_msg(&proposal, &alice);
        let sequences = GovernanceSequenceCounter::in_memory();

        let mut seqs = Vec::new();
        for _ in 0..3 {
            let Eligibility::Signed {
                author,
                domain_id,
                authority,
            } = classify(&f.store, &msg, &alice, Some(&alice))
            else {
                panic!("expected Signed");
            };
            let frame = build_signed_frame(
                &signing_key(&kp),
                author,
                domain_id,
                authority,
                &sequences,
                &msg,
            )
            .unwrap();
            seqs.push(SignedGovernanceOp::decode(&frame).unwrap().seq());
        }

        assert_eq!(seqs, vec![1, 2, 3]);
    }
}
