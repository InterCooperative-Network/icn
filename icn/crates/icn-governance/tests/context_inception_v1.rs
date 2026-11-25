//! P1 (context→domain commitment), EXPERIMENTAL: a Context Authority Log whose inception is a
//! `ContextInceptionV1` serving a governance decision space, through the unchanged genesis
//! founder quorum, log replay, checkpointed verification and fork detection.
//!
//! What this pins:
//! - a founder quorum over a V1 context's genesis replays, and that quorum's signatures cover the
//!   `served_subject` (through `C`) with no new signed object;
//! - checkpointed authorization verifies under a V1 context, and evidence for any other context
//!   (the V0 twin, another domain) is refused;
//! - fork and equivocation proofs work on V1 contexts, and are not transferable to the V0 twin.
//!
//! Fixture identities only: every key is derived from a seed byte.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    find_log_fork, preflight_checkpointed, replay_context_log, sign_checkpointed_authorization,
    target_id, transition_subject_matter, verify_checkpointed_authorization, AdmissionRefusal,
    AdoptedRule, AuthzCodecError, CanonicalV0, CheckpointedAuthorizationV0, CheckpointedEvidenceV0,
    ContextInception, ContextInceptionV0, ContextInceptionV1, ContextRef, EffectV0,
    EntryAuthorityV0, InputActV0, LogCheckpointV0, LogEntryV0, LogRefusal, RelyingFrontierV0,
    RuleResolver, SignedLogEntryV0, TransitionV0, Verdict,
};
use icn_governance::authz_threshold::{GovernanceActV0, Ratio, ThresholdRuleV0};
use icn_governance::domain::{served_domain_commitment, GovernanceDomainId};
use icn_governance::vote::VoteChoice;
use icn_identity::authority_log::AuthorityStore;
use icn_identity::authority_log::{
    authorize_event, CapabilitySet, ContextNonce, ContinuityRoot, DeviceCapability,
    EstablishmentKind, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1};
use icn_identity::evidence_bundle::EvidenceBundle;

const ROLE: &str = "member";
const OP: [u8; 32] = [0x09; 32];
const NOT_AFTER: u64 = 1_500;
const NOW: u64 = 1_000;
const LABEL: [u8; 32] = [0x1a; 32];
const DOMAIN: &str = "coop-a";

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn pk(k: &SigningKey) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap()
}

/// A Subject with one Sign device authorized at N1 position 1.
struct Member {
    subject: SubjectId,
    events: Vec<SignedAuthorityEvent>,
    device_key: SigningKey,
    device: PrincipalKey,
}

impl Member {
    fn new(seed: u8) -> Self {
        let root = ContinuityRoot::with_plan(
            [seed; 32],
            ContextNonce::from_bytes([seed ^ 0xa5; 32]),
            vec![EstablishmentKind::Rotate; 2],
        );
        let inception = root.incept().unwrap();
        let subject = inception.body.subject();
        let device_key = key(seed.wrapping_add(0x40));
        let device = pk(&device_key);
        let authorize = authorize_event(
            &root.authority_signing_key(0),
            subject,
            1,
            inception.body.event_id(),
            device,
            CapabilitySet::new([DeviceCapability::Sign]),
            None,
        );
        Member {
            subject,
            events: vec![inception, authorize],
            device_key,
            device,
        }
    }

    fn vote(&self, ctx: ContextRef, matter: [u8; 32]) -> EvidenceBundle {
        let payload = GovernanceActV0 {
            context: ctx.0,
            proposal: matter,
            choice: VoteChoice::For,
            act_nonce: [0x77; 32],
        }
        .canonical_bytes();
        let act = DeviceActV1::new(
            self.subject,
            self.device,
            DeviceCapability::Sign,
            1,
            payload,
        )
        .unwrap();
        let signed = sign_device_act(&act, &self.device_key).unwrap();
        let mut store = AuthorityStore::new();
        store.ingest_all(self.events.iter());
        EvidenceBundle::assemble(&store, &signed).unwrap()
    }
}

struct Rules(Vec<ThresholdRuleV0>);

impl RuleResolver for Rules {
    fn resolve(&self, rule_hash: &[u8; 32]) -> Option<&dyn AdoptedRule> {
        self.0
            .iter()
            .find(|r| &r.rule_hash() == rule_hash)
            .map(|r| r as &dyn AdoptedRule)
    }
}

fn strict() -> ThresholdRuleV0 {
    ThresholdRuleV0 {
        role: ROLE.into(),
        quorum: Ratio { num: 2, den: 3 },
        approval: Ratio { num: 2, den: 3 },
    }
}

fn rules() -> Rules {
    Rules(vec![strict()])
}

fn founder_keys() -> [SigningKey; 3] {
    [key(0xF1), key(0xF2), key(0xF3)]
}

fn founder_principals() -> Vec<PrincipalKey> {
    founder_keys().iter().map(pk).collect()
}

fn served(domain: &str) -> [u8; 32] {
    served_domain_commitment(&GovernanceDomainId::new(domain)).unwrap()
}

/// A V1 inception serving the governance decision space `domain`.
fn v1_for(domain: &str) -> ContextInceptionV1 {
    ContextInceptionV1::new(founder_principals(), 2, LABEL, served(domain)).unwrap()
}

/// The V0 inception with the same founders, threshold and label (no served subject).
fn v0_twin() -> ContextInceptionV0 {
    ContextInceptionV0::new(founder_principals(), 2, LABEL).unwrap()
}

struct World {
    founders: [SigningKey; 3],
    inception: ContextInception,
    ctx: ContextRef,
    writer: SigningKey,
    members: Vec<Member>,
    target: [u8; 32],
}

fn world_for(inception: ContextInception) -> World {
    World {
        ctx: inception.context_ref(),
        inception,
        founders: founder_keys(),
        writer: key(0xA1),
        members: vec![Member::new(0x01), Member::new(0x02), Member::new(0x03)],
        target: target_id(&pk(&key(0xC1))),
    }
}

fn world() -> World {
    world_for(v1_for(DOMAIN).into())
}

impl World {
    fn genesis_body(&self, members: &[&Member]) -> LogEntryV0 {
        let mut standing: Vec<(SubjectId, String)> = members
            .iter()
            .map(|m| (m.subject, ROLE.to_string()))
            .collect();
        standing.sort();
        LogEntryV0 {
            context: self.ctx,
            seq: 1,
            prev: self.ctx.0,
            transition: TransitionV0::Genesis {
                writer: pk(&self.writer),
                transition_rule: strict().rule_hash(),
                effect_rule: strict().rule_hash(),
                standing,
            },
        }
    }

    fn genesis_signed_by(&self, founders: &[&SigningKey]) -> SignedLogEntryV0 {
        let members: Vec<&Member> = self.members.iter().collect();
        SignedLogEntryV0::genesis(self.genesis_body(&members), founders).unwrap()
    }

    fn genesis(&self) -> SignedLogEntryV0 {
        self.genesis_signed_by(&[&self.founders[0], &self.founders[1]])
    }

    fn tip(&self, log: &[SignedLogEntryV0]) -> LogCheckpointV0 {
        replay_context_log(&self.inception, log, &rules())
            .unwrap()
            .tip()
            .checkpoint
    }

    /// Grant standing to `newcomer`, sequenced by the writer with For votes from members 0, 1.
    fn grant(&self, log: &[SignedLogEntryV0], newcomer: &Member) -> SignedLogEntryV0 {
        let tip = self.tip(log);
        let body = LogEntryV0 {
            context: self.ctx,
            seq: tip.seq + 1,
            prev: tip.head,
            transition: TransitionV0::GrantStanding {
                subject: newcomer.subject,
                role: ROLE.into(),
            },
        };
        let matter = transition_subject_matter(&body);
        let votes = self.members[..2]
            .iter()
            .map(|m| m.vote(self.ctx, matter))
            .collect();
        SignedLogEntryV0::decided(body, &self.writer, votes).unwrap()
    }

    /// A Pass checkpointed authorization at the log tip, carrying `inception`, with For votes
    /// from the three founding members.
    fn authorize_with(
        &self,
        log: &[SignedLogEntryV0],
        inception: ContextInception,
    ) -> CheckpointedEvidenceV0 {
        let checkpoint = self.tip(log);
        let effect = EffectV0 {
            target: self.target,
            action: "reserve".into(),
            params_hash: [0x33; 32],
        };
        let matter = effect.commitment(self.ctx, OP);
        // All three founding members vote: 3 of an electorate of up to 4 meets 2/3.
        let mut vote_bundles: Vec<EvidenceBundle> = self
            .members
            .iter()
            .map(|m| m.vote(self.ctx, matter))
            .collect();
        vote_bundles.sort_by_key(|b| b.act().act.act_id());
        let inputs = vote_bundles
            .iter()
            .map(|b| InputActV0 {
                act_id: b.act().act.act_id(),
                subject: b.subject(),
                position: b.evaluation_position(),
            })
            .collect();
        let body = CheckpointedAuthorizationV0 {
            context: self.ctx,
            checkpoint,
            subject_matter: matter,
            rule_hash: strict().rule_hash(),
            verdict: Verdict::Pass,
            inputs,
            effect,
            operation_id: OP,
            not_after: NOT_AFTER,
        };
        CheckpointedEvidenceV0 {
            inception,
            log: log.to_vec(),
            authorization: sign_checkpointed_authorization(body, &self.writer),
            vote_bundles,
        }
    }

    fn authorize(&self, log: &[SignedLogEntryV0]) -> CheckpointedEvidenceV0 {
        self.authorize_with(log, self.inception.clone())
    }

    fn frontier(&self, min_seq: u64) -> RelyingFrontierV0 {
        RelyingFrontierV0 {
            context: self.ctx,
            min_seq,
            pinned: None,
        }
    }
}

// ------------------------------------------------------------------------------------------
// The commitment
// ------------------------------------------------------------------------------------------

#[test]
fn v1_context_commits_to_the_served_domain() {
    let w = world();
    assert_eq!(w.inception.served_subject(), Some(served(DOMAIN)));
    // Reference vector (computed independently; see `domain.rs` tests).
    assert_eq!(
        hex::encode(served(DOMAIN)),
        "dc0235281c57ac74fa3106a07927ff206c7bfe8117e340501d4a3df30d534e52"
    );
    // A context serving another domain, and the V0 twin, are different contexts.
    assert_ne!(v1_for("coop-b").context_ref(), w.ctx);
    assert_ne!(v0_twin().context_ref(), w.ctx);
    assert_eq!(ContextInception::from(v0_twin()).served_subject(), None);
}

// ------------------------------------------------------------------------------------------
// Genesis founder quorum under V1
// ------------------------------------------------------------------------------------------

#[test]
fn founder_quorum_over_a_v1_genesis_replays() {
    let w = world();
    let log = vec![w.genesis()];
    // Through the owned carrier and through a plain `&ContextInceptionV1`.
    let carried = replay_context_log(&w.inception, &log, &rules()).unwrap();
    let ContextInception::V1(v1) = &w.inception else {
        unreachable!()
    };
    let direct = replay_context_log(v1, &log, &rules()).unwrap();
    assert_eq!(carried.tip(), direct.tip());
    assert_eq!(carried.tip().context, w.ctx);
    assert_eq!(carried.tip().checkpoint.seq, 1);
    assert_eq!(carried.tip().checkpoint.head, log[0].body.head());
    // All three founders is also a quorum.
    let all = w.genesis_signed_by(&[&w.founders[0], &w.founders[1], &w.founders[2]]);
    assert!(replay_context_log(&w.inception, &[all], &rules()).is_ok());
}

#[test]
fn v1_genesis_below_threshold_or_by_a_non_founder_is_refused() {
    let w = world();
    let one = w.genesis_signed_by(&[&w.founders[0]]);
    assert_eq!(
        replay_context_log(&w.inception, &[one], &rules()).map(|l| l.tip().clone()),
        Err(LogRefusal::FounderQuorumNotMet {
            valid: 1,
            threshold: 2
        })
    );
    let outsider = key(0xEE);
    let rogue = w.genesis_signed_by(&[&w.founders[0], &outsider]);
    assert_eq!(
        replay_context_log(&w.inception, &[rogue], &rules()).map(|l| l.tip().clone()),
        Err(LogRefusal::NotAFounder)
    );
}

/// The founders' genesis signatures cover the served subject: the same signed genesis does not
/// replay as the genesis of a context serving another domain, nor of the V0 twin, and moving the
/// body to that context breaks every founder signature.
#[test]
fn founder_signatures_cover_the_served_subject() {
    let w = world();
    let genesis = w.genesis();

    let other = ContextInception::from(v1_for("coop-b"));
    assert_eq!(
        replay_context_log(&other, std::slice::from_ref(&genesis), &rules())
            .map(|l| l.tip().clone()),
        Err(LogRefusal::ContextMismatch { seq: 1 })
    );
    let twin = v0_twin();
    assert_eq!(
        replay_context_log(&twin, std::slice::from_ref(&genesis), &rules())
            .map(|l| l.tip().clone()),
        Err(LogRefusal::ContextMismatch { seq: 1 })
    );

    // Transplant: rewrite the body to the other context, keep the founder signatures.
    let mut transplanted = genesis.clone();
    let other_ctx = other.context_ref();
    transplanted.body.context = other_ctx;
    transplanted.body.prev = other_ctx.0;
    assert!(matches!(
        transplanted.authority,
        EntryAuthorityV0::Founders(_)
    ));
    assert_eq!(
        replay_context_log(&other, &[transplanted], &rules()).map(|l| l.tip().clone()),
        Err(LogRefusal::BadFounderSignature)
    );
}

#[test]
fn replay_refuses_a_v1_inception_with_a_zero_served_subject() {
    let w = world();
    let ContextInception::V1(mut v1) = w.inception.clone() else {
        unreachable!()
    };
    v1.served_subject = [0; 32];
    assert_eq!(
        replay_context_log(&v1, &[w.genesis()], &rules()).map(|l| l.tip().clone()),
        Err(LogRefusal::InvalidInception(
            AuthzCodecError::InvalidValue {
                field: "served_subject"
            }
        ))
    );
}

// ------------------------------------------------------------------------------------------
// Checkpointed authorization under V1
// ------------------------------------------------------------------------------------------

#[test]
fn checkpointed_authorization_verifies_under_a_v1_context() {
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.grant(&log, &Member::new(0x04)));
    let ev = w.authorize(&log);
    let verified = verify_checkpointed_authorization(&ev, &w.frontier(2), &rules()).unwrap();
    assert_eq!(verified.writer(), pk(&w.writer));
    assert_eq!(verified.authorization().context, w.ctx);
    assert_eq!(ev.inception.served_subject(), Some(served(DOMAIN)));
    // And target-side preflight accepts it.
    let pre = preflight_checkpointed(&ev, &w.frontier(2), &rules(), w.target, NOW).unwrap();
    assert_eq!(pre.verified().writer(), pk(&w.writer));
}

#[test]
fn checkpointed_evidence_for_another_context_is_refused() {
    let w = world();
    let log = vec![w.genesis()];
    // The V0 twin (same founders and label) is a different context.
    let ev = w.authorize_with(&log, v0_twin().into());
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(1), &rules()),
        Err(LogRefusal::UntrustedContext)
    );
    assert_eq!(
        preflight_checkpointed(&ev, &w.frontier(1), &rules(), w.target, NOW).map(|_| ()),
        Err(AdmissionRefusal::UntrustedContext)
    );
    // A V1 inception serving another domain is a different context.
    let ev = w.authorize_with(&log, v1_for("coop-b").into());
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(1), &rules()),
        Err(LogRefusal::UntrustedContext)
    );
    // A zeroed served subject is refused before the context is even compared.
    let ContextInception::V1(mut zero) = w.inception.clone() else {
        unreachable!()
    };
    zero.served_subject = [0; 32];
    let ev = w.authorize_with(&log, zero.into());
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(1), &rules()),
        Err(LogRefusal::InvalidInception(
            AuthzCodecError::InvalidValue {
                field: "served_subject"
            }
        ))
    );
}

/// The carried inception survives a byte round trip through the versioned codec.
#[test]
fn carried_v1_inception_round_trips() {
    let w = world();
    let ev = w.authorize(&[w.genesis()]);
    let bytes = ev.inception.canonical_bytes();
    let decoded = ContextInception::decode(&bytes).unwrap();
    assert_eq!(decoded, ev.inception);
    let rebuilt = CheckpointedEvidenceV0 {
        inception: decoded,
        ..ev.clone()
    };
    assert!(verify_checkpointed_authorization(&rebuilt, &w.frontier(1), &rules()).is_ok());
}

// ------------------------------------------------------------------------------------------
// Fork and equivocation under V1
// ------------------------------------------------------------------------------------------

#[test]
fn writer_equivocation_on_a_v1_context_yields_an_attributable_proof() {
    let w = world();
    let base = vec![w.genesis()];
    let mut a = base.clone();
    a.push(w.grant(&a, &Member::new(0x04)));
    let mut b = base.clone();
    b.push(w.grant(&b, &Member::new(0x05)));
    assert!(replay_context_log(&w.inception, &a, &rules()).is_ok());
    assert!(replay_context_log(&w.inception, &b, &rules()).is_ok());

    let proof = find_log_fork(&w.inception, &a, &b).expect("a fork");
    assert_eq!(proof.equivocators(&w.inception), Some(vec![pk(&w.writer)]));
    let ContextInception::V1(v1) = &w.inception else {
        unreachable!()
    };
    assert_eq!(proof.equivocators(v1), Some(vec![pk(&w.writer)]));
    // Not transferable: under the V0 twin or another domain's context it is no proof at all.
    assert_eq!(proof.equivocators(&v0_twin()), None);
    assert_eq!(find_log_fork(&v0_twin(), &a, &b), None);
    assert_eq!(proof.equivocators(&v1_for("coop-b")), None);
    // A prefix is not a fork.
    assert_eq!(find_log_fork(&w.inception, &a, &a[..1]), None);
}

#[test]
fn founder_equivocation_at_a_v1_genesis_names_the_overlap() {
    let w = world();
    let a = w.genesis_signed_by(&[&w.founders[0], &w.founders[1]]);
    let members: Vec<&Member> = w.members[..2].iter().collect();
    let b = SignedLogEntryV0::genesis(w.genesis_body(&members), &[&w.founders[1], &w.founders[2]])
        .unwrap();
    let proof = find_log_fork(&w.inception, &[a], &[b]).expect("a fork");
    assert_eq!(
        proof.equivocators(&w.inception),
        Some(vec![pk(&w.founders[1])])
    );
}
