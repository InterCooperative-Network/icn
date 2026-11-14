//! Context Authority Log v0: the writer sequences institutional transitions but cannot
//! manufacture them. Each `eN_*` test is one attack from the acceptance plan (E1–E10); each
//! asserts the exact refusal, or — for E10 — pins what the log cannot detect.
//!
//! Rules are real `ThresholdRuleV0`s and votes are real N4 device acts carried as evidence
//! bundles, so every decision goes through the unchanged N1/N4 verifier.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    admit, admit_checkpointed, find_log_fork, replay_context_log, roster_hash,
    sign_checkpointed_authorization, target_id, transition_subject_matter,
    verify_checkpointed_authorization, Admission, AdmissionRefusal, AdoptedRule,
    AuthorizationEvidenceV0, AuthorizationV0, AuthzRefusal, CanonicalV0, CheckpointedAdmission,
    CheckpointedAuthorizationV0, CheckpointedEvidenceV0, ConflictClass, ConsumedAuthorization,
    ConsumedEntry, ConsumedSet, ConsumedSetError, ContextInceptionV0, ContextRef, ContextStateV0,
    EffectV0, EntryAuthorityV0, EquivocationProof, InMemoryConsumedSet, InputActV0,
    LogCheckpointV0, LogEntryV0, LogForkProofV0, LogRefusal, RelyingFrontierV0, RuleAdoptionV0,
    RuleBinding, RuleError, RuleResolver, RuleSlot, Signed, SignedDesignation, SignedLogEntryV0,
    StandingV0, TransitionV0, Verdict, WriterDesignationV0,
};
use icn_governance::authz_threshold::{GovernanceActV0, Ratio, ThresholdRuleV0};
use icn_governance::vote::VoteChoice;
use icn_identity::authority_log::{
    authorize_event, revoke_event, AuthorityStore, CapabilitySet, ContextNonce, ContinuityRoot,
    DeviceCapability, EstablishmentKind, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1};
use icn_identity::evidence_bundle::EvidenceBundle;

const ROLE: &str = "member";
const OP: [u8; 32] = [0x09; 32];
const NOT_AFTER: u64 = 1_500;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn principal(seed: u8) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(key(seed).verifying_key()).unwrap()
}

/// A Subject with one Sign device authorized at N1 position 1.
struct Member {
    root: ContinuityRoot,
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
        let device = principal(seed.wrapping_add(0x40));
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
            root,
            subject,
            events: vec![inception, authorize],
            device_key,
            device,
        }
    }

    fn revoke_device(&mut self) {
        let prev = self.events.last().unwrap().body.event_id();
        self.events.push(revoke_event(
            &self.root.authority_signing_key(0),
            self.subject,
            2,
            prev,
            self.device,
        ));
    }

    fn vote_at(
        &self,
        ctx: ContextRef,
        matter: [u8; 32],
        choice: VoteChoice,
        position: u64,
    ) -> EvidenceBundle {
        let payload = GovernanceActV0 {
            context: ctx.0,
            proposal: matter,
            choice,
            act_nonce: [0x77; 32],
        }
        .canonical_bytes();
        let act = DeviceActV1::new(
            self.subject,
            self.device,
            DeviceCapability::Sign,
            position,
            payload,
        )
        .unwrap();
        let signed = sign_device_act(&act, &self.device_key).unwrap();
        let mut store = AuthorityStore::new();
        store.ingest_all(self.events.iter());
        EvidenceBundle::assemble(&store, &signed).unwrap()
    }

    fn vote(&self, ctx: ContextRef, matter: [u8; 32]) -> EvidenceBundle {
        self.vote_at(ctx, matter, VoteChoice::For, 1)
    }
}

/// Rule implementations this verifier holds.
struct Rules(Vec<ThresholdRuleV0>);

impl RuleResolver for Rules {
    fn resolve(&self, rule_hash: &[u8; 32]) -> Option<&dyn AdoptedRule> {
        self.0
            .iter()
            .find(|r| &r.rule_hash() == rule_hash)
            .map(|r| r as &dyn AdoptedRule)
    }
}

/// 2/3 quorum, 2/3 approval: the context's founding rule for both slots.
fn strict() -> ThresholdRuleV0 {
    ThresholdRuleV0 {
        role: ROLE.into(),
        quorum: Ratio { num: 2, den: 3 },
        approval: Ratio { num: 2, den: 3 },
    }
}

/// One participant decides anything: what an attacker would like to be in force.
fn lax() -> ThresholdRuleV0 {
    ThresholdRuleV0 {
        role: ROLE.into(),
        quorum: Ratio { num: 1, den: 4 },
        approval: Ratio { num: 1, den: 2 },
    }
}

fn rules() -> Rules {
    Rules(vec![strict(), lax()])
}

struct World {
    founders: [SigningKey; 3],
    inception: ContextInceptionV0,
    ctx: ContextRef,
    writer: SigningKey,
    members: Vec<Member>,
    target: [u8; 32],
}

fn world() -> World {
    world_in([0x1a; 32])
}

/// A world whose only difference is its context (the inception nonce): same founders, writer,
/// members and target.
fn world_in(nonce: [u8; 32]) -> World {
    let founders = [key(0xF1), key(0xF2), key(0xF3)];
    let inception = ContextInceptionV0::new(
        founders
            .iter()
            .map(|k| PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap())
            .collect(),
        2,
        nonce,
    )
    .unwrap();
    World {
        ctx: inception.context_ref(),
        inception,
        founders,
        writer: key(0xA1),
        members: vec![Member::new(0x01), Member::new(0x02), Member::new(0x03)],
        target: target_id(&principal(0xC1)),
    }
}

impl World {
    fn pk(k: &SigningKey) -> PrincipalKey {
        PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap()
    }

    fn genesis_with(&self, members: &[&Member], founders: &[&SigningKey]) -> SignedLogEntryV0 {
        let mut standing: Vec<(SubjectId, String)> = members
            .iter()
            .map(|m| (m.subject, ROLE.to_string()))
            .collect();
        standing.sort();
        let body = LogEntryV0 {
            context: self.ctx,
            seq: 1,
            prev: self.ctx.0,
            transition: TransitionV0::Genesis {
                writer: Self::pk(&self.writer),
                transition_rule: strict().rule_hash(),
                effect_rule: strict().rule_hash(),
                standing,
            },
        };
        SignedLogEntryV0::genesis(body, founders).unwrap()
    }

    fn genesis(&self) -> SignedLogEntryV0 {
        let members: Vec<&Member> = self.members.iter().collect();
        self.genesis_with(&members, &[&self.founders[0], &self.founders[1]])
    }

    fn state(&self, log: &[SignedLogEntryV0]) -> ContextStateV0 {
        replay_context_log(&self.inception, log, &rules())
            .unwrap()
            .tip()
            .clone()
    }

    fn body(&self, log: &[SignedLogEntryV0], transition: TransitionV0) -> LogEntryV0 {
        let tip = self.state(log).checkpoint;
        LogEntryV0 {
            context: self.ctx,
            seq: tip.seq + 1,
            prev: tip.head,
            transition,
        }
    }

    /// The next entry, sequenced by `writer`, with a For vote from each of `voters`.
    fn next(
        &self,
        log: &[SignedLogEntryV0],
        transition: TransitionV0,
        writer: &SigningKey,
        voters: &[&Member],
    ) -> SignedLogEntryV0 {
        let body = self.body(log, transition);
        let matter = transition_subject_matter(&body);
        let votes = voters.iter().map(|m| m.vote(self.ctx, matter)).collect();
        SignedLogEntryV0::decided(body, writer, votes).unwrap()
    }

    fn m(&self, i: usize) -> &Member {
        &self.members[i]
    }

    fn effect(&self) -> EffectV0 {
        EffectV0 {
            target: self.target,
            action: "reserve".into(),
            params_hash: [0x33; 32],
        }
    }

    /// A checkpointed authorization at `checkpoint`, signed by `writer`, Pass, with For votes.
    fn authorize(
        &self,
        log: &[SignedLogEntryV0],
        checkpoint: LogCheckpointV0,
        writer: &SigningKey,
        voters: &[&Member],
    ) -> CheckpointedEvidenceV0 {
        self.authorize_op(
            log,
            checkpoint,
            writer,
            voters,
            VoteChoice::For,
            Verdict::Pass,
            OP,
        )
    }

    /// [`World::authorize`] with the votes' `choice`, the signed `verdict` and the operation id.
    #[allow(clippy::too_many_arguments)]
    fn authorize_op(
        &self,
        log: &[SignedLogEntryV0],
        checkpoint: LogCheckpointV0,
        writer: &SigningKey,
        voters: &[&Member],
        choice: VoteChoice,
        verdict: Verdict,
        op: [u8; 32],
    ) -> CheckpointedEvidenceV0 {
        let effect = self.effect();
        let matter = effect.commitment(self.ctx, op);
        let mut vote_bundles: Vec<EvidenceBundle> = voters
            .iter()
            .map(|m| m.vote_at(self.ctx, matter, choice, 1))
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
            verdict,
            inputs,
            effect,
            operation_id: op,
            not_after: NOT_AFTER,
        };
        CheckpointedEvidenceV0 {
            inception: self.inception.clone(),
            log: log.to_vec(),
            authorization: sign_checkpointed_authorization(body, writer),
            vote_bundles,
        }
    }

    fn frontier(&self, min_seq: u64, pinned: Option<LogCheckpointV0>) -> RelyingFrontierV0 {
        RelyingFrontierV0 {
            context: self.ctx,
            min_seq,
            pinned,
        }
    }

    fn replay(&self, log: &[SignedLogEntryV0]) -> Result<ContextStateV0, LogRefusal> {
        replay_context_log(&self.inception, log, &rules()).map(|l| l.tip().clone())
    }
}

fn grant(m: &Member) -> TransitionV0 {
    TransitionV0::GrantStanding {
        subject: m.subject,
        role: ROLE.into(),
    }
}

fn revoke(m: &Member) -> TransitionV0 {
    TransitionV0::RevokeStanding {
        subject: m.subject,
        role: ROLE.into(),
    }
}

fn checkpoint(state: &ContextStateV0) -> LogCheckpointV0 {
    state.checkpoint
}

// ---------------------------------------------------------------------------------------------
// Positive
// ---------------------------------------------------------------------------------------------

#[test]
fn p1_state_is_derived_from_authorized_transitions_only() {
    let w = world();
    let m4 = Member::new(0x04);
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]));
    let state = w.replay(&log).unwrap();
    assert_eq!(state.checkpoint.seq, 2);
    assert_eq!(
        state.eligible(ROLE),
        [w.m(0), w.m(1), w.m(2), &m4]
            .iter()
            .map(|m| m.subject)
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(state.writer, World::pk(&w.writer));
}

#[test]
fn p2_log_is_transferable_as_bytes_and_replays_identically() {
    let w = world();
    let m4 = Member::new(0x04);
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]));
    log.push(w.next(&log, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1), &m4]));
    let decoded: Vec<SignedLogEntryV0> = log
        .iter()
        .map(|e| SignedLogEntryV0::decode(&e.encode()).unwrap())
        .collect();
    assert_eq!(decoded, log);
    assert_eq!(w.replay(&decoded), w.replay(&log));
    // Trailing bytes and truncation are refused.
    let mut bytes = log[1].encode();
    bytes.push(0);
    assert!(SignedLogEntryV0::decode(&bytes).is_err());
    let bytes = log[1].encode();
    assert!(SignedLogEntryV0::decode(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn p3_checkpointed_authorization_verifies_against_the_frontier() {
    let w = world();
    let log = vec![w.genesis()];
    let cp = checkpoint(&w.state(&log));
    let ev = w.authorize(&log, cp, &w.writer, &[w.m(0), w.m(1)]);
    let v = verify_checkpointed_authorization(&ev, &w.frontier(1, Some(cp)), &rules()).unwrap();
    assert_eq!(v.writer(), World::pk(&w.writer));
    assert_eq!(v.presented_tip(), cp);
}

// ---------------------------------------------------------------------------------------------
// E1 — omitted member
// ---------------------------------------------------------------------------------------------

/// The log-derived counterpart of #2884's `known_gap_writer_controls_the_electorate`: the
/// electorate is no longer the writer's to supply, so puppets do not count.
#[test]
fn e1_puppet_electorate_is_refused() {
    let w = world();
    let log = vec![w.genesis()];
    let cp = checkpoint(&w.state(&log));
    let puppets = [Member::new(0x0a), Member::new(0x0b)];
    let ev = w.authorize(&log, cp, &w.writer, &[&puppets[0], &puppets[1]]);
    assert!(matches!(
        verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()),
        Err(LogRefusal::IneligibleVoter { .. })
    ));
}

/// The history holds a fourth member. The writer's signed Pass, counted as if the electorate
/// were three, does not reproduce: 2 of 4 participating misses the 2/3 quorum.
#[test]
fn e1_truncated_electorate_does_not_reproduce_the_verdict() {
    let w = world();
    let m4 = Member::new(0x04);
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]));
    let cp = checkpoint(&w.state(&log));
    let ev = w.authorize(&log, cp, &w.writer, &[w.m(0), w.m(1)]);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(2, Some(cp)), &rules()),
        Err(LogRefusal::VerdictMismatch {
            signed: Verdict::Pass,
            evaluated: Verdict::Fail
        })
    );
}

/// Omitting the grant by presenting the shorter history is a rollback: refused by the pin or the
/// frontier, never silently accepted.
#[test]
fn e1_omitting_the_grant_by_truncating_history_is_refused() {
    let w = world();
    let m4 = Member::new(0x04);
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]));
    let pinned = checkpoint(&w.state(&log));
    let short = &log[..1];
    let ev = w.authorize(
        short,
        checkpoint(&w.state(short)),
        &w.writer,
        &[w.m(0), w.m(1)],
    );
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(0, Some(pinned)), &rules()),
        Err(LogRefusal::PinnedCheckpointMissing { seq: 2 })
    );
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(2, None), &rules()),
        Err(LogRefusal::CheckpointBelowFrontier {
            checkpoint: 1,
            frontier: 2
        })
    );
}

// ---------------------------------------------------------------------------------------------
// E2 — fabricated member
// ---------------------------------------------------------------------------------------------

#[test]
fn e2_writer_cannot_grant_standing_on_its_own_word() {
    let w = world();
    let puppet = Member::new(0x0a);
    let log = vec![w.genesis()];
    // No votes at all.
    let lone = w.next(&log, grant(&puppet), &w.writer, &[]);
    assert_eq!(
        w.replay(&[log[0].clone(), lone]),
        Err(LogRefusal::TransitionNotAuthorized { seq: 2 })
    );
    // One genuine member is below the transition rule.
    let one = w.next(&log, grant(&puppet), &w.writer, &[w.m(0)]);
    assert_eq!(
        w.replay(&[log[0].clone(), one]),
        Err(LogRefusal::TransitionNotAuthorized { seq: 2 })
    );
    // The puppet voting for itself is not in the electorate.
    let selfie = w.next(&log, grant(&puppet), &w.writer, &[w.m(0), &puppet]);
    assert!(matches!(
        w.replay(&[log[0].clone(), selfie]),
        Err(LogRefusal::IneligibleVoter { at: 1, .. })
    ));
}

#[test]
fn e2_founders_cannot_act_after_genesis() {
    let w = world();
    let puppet = Member::new(0x0a);
    let log = vec![w.genesis()];
    let body = w.body(&log, grant(&puppet));
    let founders_only =
        SignedLogEntryV0::genesis(body, &[&w.founders[0], &w.founders[1], &w.founders[2]]).unwrap();
    assert_eq!(
        w.replay(&[log[0].clone(), founders_only]),
        Err(LogRefusal::WrongAuthority { seq: 2 })
    );
}

#[test]
fn e2_genesis_needs_the_founder_quorum() {
    let w = world();
    let members: Vec<&Member> = w.members.iter().collect();
    let one = w.genesis_with(&members, &[&w.founders[0]]);
    assert_eq!(
        w.replay(&[one]),
        Err(LogRefusal::FounderQuorumNotMet {
            valid: 1,
            threshold: 2
        })
    );
    let outsider = key(0xEE);
    let forged = w.genesis_with(&members, &[&w.founders[0], &outsider]);
    assert_eq!(w.replay(&[forged]), Err(LogRefusal::NotAFounder));
}

// ---------------------------------------------------------------------------------------------
// E3 / E9 — rule replacement
// ---------------------------------------------------------------------------------------------

#[test]
fn e3_unauthorized_rule_replacement_is_refused() {
    let w = world();
    let log = vec![w.genesis()];
    for slot in [RuleSlot::Transition, RuleSlot::Effect] {
        let adopt = TransitionV0::AdoptRule {
            slot,
            rule_hash: lax().rule_hash(),
        };
        let entry = w.next(&log, adopt, &w.writer, &[w.m(0)]);
        assert_eq!(
            w.replay(&[log[0].clone(), entry]),
            Err(LogRefusal::TransitionNotAuthorized { seq: 2 })
        );
    }
}

/// The lax rule WOULD pass its own adoption (one vote is enough under it). It is evaluated under
/// the rule in force before the entry, so it cannot.
#[test]
fn e9_a_rule_cannot_approve_its_own_adoption() {
    let w = world();
    let log = vec![w.genesis()];
    let adopt = TransitionV0::AdoptRule {
        slot: RuleSlot::Transition,
        rule_hash: lax().rule_hash(),
    };
    let entry = w.next(&log, adopt, &w.writer, &[w.m(0)]);

    // Under the proposed rule, the same vote passes.
    let state = w.state(&log);
    let EntryAuthorityV0::Decision(d) = &entry.authority else {
        unreachable!()
    };
    let acts: Vec<_> = d
        .vote_bundles
        .iter()
        .map(|b| icn_identity::evidence_bundle::verify_evidence_bundle(b, b.subject(), 1).unwrap())
        .collect();
    let binding = RuleBinding {
        context: w.ctx,
        subject_matter: transition_subject_matter(&entry.body),
    };
    assert_eq!(
        lax().evaluate(&binding, &state.eligible(ROLE), &acts),
        Ok(Verdict::Pass)
    );

    // Under the rule in force, it does not.
    assert_eq!(
        w.replay(&[log[0].clone(), entry]),
        Err(LogRefusal::TransitionNotAuthorized { seq: 2 })
    );
}

/// An authorized adoption changes the rule from the NEXT position on, and the effect rule a
/// checkpointed authorization cites must be the one in force at its checkpoint.
#[test]
fn e3_authorized_rule_change_applies_from_the_next_position() {
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.next(
        &log,
        TransitionV0::AdoptRule {
            slot: RuleSlot::Effect,
            rule_hash: lax().rule_hash(),
        },
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    assert_eq!(w.state(&log).effect_rule, lax().rule_hash());
    // Citing the old effect rule at the new checkpoint is refused.
    let cp = checkpoint(&w.state(&log));
    let ev = w.authorize(&log, cp, &w.writer, &[w.m(0), w.m(1)]);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(2, None), &rules()),
        Err(LogRefusal::EffectRuleNotInForce)
    );
}

// ---------------------------------------------------------------------------------------------
// E4 — rollback
// ---------------------------------------------------------------------------------------------

#[test]
fn e4_checkpoint_below_the_frontier_is_refused() {
    let w = world();
    let m4 = Member::new(0x04);
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]));
    let old = checkpoint(&w.replay(&log[..1]).unwrap());
    let ev = w.authorize(&log, old, &w.writer, &[w.m(0), w.m(1)]);
    // Historically valid at position 1 ...
    assert!(verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()).is_ok());
    // ... but not current for a party whose frontier is 2.
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(2, None), &rules()),
        Err(LogRefusal::CheckpointBelowFrontier {
            checkpoint: 1,
            frontier: 2
        })
    );
}

#[test]
fn e4_a_checkpoint_from_another_history_is_refused() {
    let w = world();
    let mut log = vec![w.genesis()];
    let cp = checkpoint(&w.state(&log));
    log.push(w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    let mut forged = cp;
    forged.head[0] ^= 1;
    let ev = w.authorize(&log, forged, &w.writer, &[w.m(0), w.m(1)]);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()),
        Err(LogRefusal::CheckpointNotInLog)
    );
}

// ---------------------------------------------------------------------------------------------
// E5 — revocation
// ---------------------------------------------------------------------------------------------

#[test]
fn e5_revoked_standing_does_not_count_at_or_after_the_revocation() {
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    let cp = checkpoint(&w.state(&log));
    assert_eq!(w.state(&log).eligible(ROLE).len(), 2);
    let ev = w.authorize(&log, cp, &w.writer, &[w.m(1), w.m(2)]);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(2, None), &rules()),
        Err(LogRefusal::IneligibleVoter {
            at: 2,
            subject: w.m(2).subject
        })
    );
    // At the pre-revocation checkpoint the vote is historically valid, and a frontier at the
    // revocation refuses it: historical validity is not current authority.
    let before = checkpoint(&w.replay(&log[..1]).unwrap());
    let ev = w.authorize(&log, before, &w.writer, &[w.m(1), w.m(2)]);
    assert!(verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()).is_ok());
    assert!(matches!(
        verify_checkpointed_authorization(&ev, &w.frontier(2, None), &rules()),
        Err(LogRefusal::CheckpointBelowFrontier { .. })
    ));
}

/// A revoked member is also out of the electorate for the very next transition.
#[test]
fn e5_revoked_member_cannot_authorize_later_transitions() {
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    let entry = w.next(
        &log,
        grant(&Member::new(0x0a)),
        &w.writer,
        &[w.m(0), w.m(2)],
    );
    log.push(entry);
    assert_eq!(
        w.replay(&log),
        Err(LogRefusal::IneligibleVoter {
            at: 2,
            subject: w.m(2).subject
        })
    );
}

// ---------------------------------------------------------------------------------------------
// E6 — conflicting histories
// ---------------------------------------------------------------------------------------------

#[test]
fn e6_writer_equivocation_yields_an_attributable_proof() {
    let w = world();
    let base = vec![w.genesis()];
    let mut a = base.clone();
    a.push(w.next(&a, grant(&Member::new(0x04)), &w.writer, &[w.m(0), w.m(1)]));
    let mut b = base.clone();
    b.push(w.next(&b, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    // Both histories replay: a hash chain alone cannot tell which is "the" history.
    assert!(w.replay(&a).is_ok() && w.replay(&b).is_ok());

    let proof = find_log_fork(&w.inception, &a, &b).expect("a fork");
    assert_eq!(
        proof.equivocators(&w.inception),
        Some(vec![World::pk(&w.writer)])
    );
    // The proof stands alone: it re-verifies after a byte round trip of both entries.
    let rt = LogForkProofV0 {
        first: SignedLogEntryV0::decode(&proof.first.encode()).unwrap(),
        second: SignedLogEntryV0::decode(&proof.second.encode()).unwrap(),
    };
    assert_eq!(
        rt.equivocators(&w.inception),
        Some(vec![World::pk(&w.writer)])
    );
    // A relying party pinned on one side refuses the other.
    let pin_a = checkpoint(&w.state(&a));
    let ev_b = w.authorize(&b, checkpoint(&w.state(&b)), &w.writer, &[w.m(0), w.m(1)]);
    assert_eq!(
        verify_checkpointed_authorization(&ev_b, &w.frontier(2, Some(pin_a)), &rules()),
        Err(LogRefusal::PinnedCheckpointMismatch { seq: 2 })
    );
}

#[test]
fn e6_non_conflicts_are_not_proofs() {
    let w = world();
    let mut a = vec![w.genesis()];
    a.push(w.next(&a, grant(&Member::new(0x04)), &w.writer, &[w.m(0), w.m(1)]));
    // A prefix is not a fork.
    assert_eq!(find_log_fork(&w.inception, &a, &a[..1]), None);
    // The same entry twice is not a fork.
    let same = LogForkProofV0 {
        first: a[1].clone(),
        second: a[1].clone(),
    };
    assert_eq!(same.equivocators(&w.inception), None);
    // Different positions are not a fork.
    let skew = LogForkProofV0 {
        first: a[0].clone(),
        second: a[1].clone(),
    };
    assert_eq!(skew.equivocators(&w.inception), None);
    // A forged signature attributes nothing.
    let mut b = vec![a[0].clone()];
    let mut forged = w.next(&b, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]);
    if let EntryAuthorityV0::Decision(d) = &mut forged.authority {
        d.signature[0] ^= 1;
    }
    b.push(forged);
    assert_eq!(find_log_fork(&w.inception, &a, &b), None);
}

#[test]
fn e6_founder_equivocation_at_genesis_names_the_overlap() {
    let w = world();
    let members: Vec<&Member> = w.members.iter().collect();
    let a = w.genesis_with(&members, &[&w.founders[0], &w.founders[1]]);
    let b = w.genesis_with(&members[..2], &[&w.founders[1], &w.founders[2]]);
    let proof = find_log_fork(&w.inception, &[a], &[b]).expect("a fork");
    assert_eq!(
        proof.equivocators(&w.inception),
        Some(vec![World::pk(&w.founders[1])])
    );
}

// ---------------------------------------------------------------------------------------------
// E7 — writer replacement
// ---------------------------------------------------------------------------------------------

#[test]
fn e7_authorized_successor_continues_without_the_old_writer() {
    let w = world();
    let w2 = key(0xA2);
    let mut log = vec![w.genesis()];
    let appoint = TransitionV0::AppointWriter {
        writer: World::pk(&w2),
    };
    // Sequenced by the INCOMING writer: the outgoing one need not cooperate.
    log.push(w.next(&log, appoint, &w2, &[w.m(0), w.m(1)]));
    assert_eq!(w.state(&log).writer, World::pk(&w2));

    // The old writer can no longer sequence ...
    let stale = w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    );
    assert_eq!(
        w.replay(&[log.clone(), vec![stale]].concat()),
        Err(LogRefusal::NotTheWriter { seq: 3 })
    );
    // ... the successor can.
    log.push(w.next(&log, grant(&Member::new(0x04)), &w2, &[w.m(0), w.m(1)]));
    assert!(w.replay(&log).is_ok());

    // An old-writer authorization at a post-appointment checkpoint is refused ...
    let cp = checkpoint(&w.state(&log));
    let ev = w.authorize(&log, cp, &w.writer, &[w.m(0), w.m(1), w.m(2)]);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(3, None), &rules()),
        Err(LogRefusal::AuthorizationNotByWriter)
    );
    // ... and at its own pre-appointment checkpoint it is history, refused by a frontier at the
    // appointment.
    let before = checkpoint(&w.replay(&log[..1]).unwrap());
    let ev = w.authorize(&log, before, &w.writer, &[w.m(0), w.m(1)]);
    assert!(verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()).is_ok());
    assert!(matches!(
        verify_checkpointed_authorization(&ev, &w.frontier(2, None), &rules()),
        Err(LogRefusal::CheckpointBelowFrontier { .. })
    ));
}

#[test]
fn e7_self_appointment_and_third_party_sequencing_are_refused() {
    let w = world();
    let usurper = key(0xEE);
    let log = vec![w.genesis()];
    let appoint = TransitionV0::AppointWriter {
        writer: World::pk(&usurper),
    };
    // No decision.
    let seized = w.next(&log, appoint.clone(), &usurper, &[]);
    assert_eq!(
        w.replay(&[log[0].clone(), seized]),
        Err(LogRefusal::TransitionNotAuthorized { seq: 2 })
    );
    // A third key may not sequence an appointment of someone else, even with a passing decision.
    let w2 = key(0xA2);
    let appoint_w2 = TransitionV0::AppointWriter {
        writer: World::pk(&w2),
    };
    let relayed = w.next(&log, appoint_w2, &usurper, &[w.m(0), w.m(1)]);
    assert_eq!(
        w.replay(&[log[0].clone(), relayed]),
        Err(LogRefusal::NotTheWriter { seq: 2 })
    );
}

// ---------------------------------------------------------------------------------------------
// E8 — missing evidence
// ---------------------------------------------------------------------------------------------

#[test]
fn e8_missing_evidence_fails_closed() {
    let w = world();
    let m4 = Member::new(0x04);
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]));
    log.push(w.next(&log, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1), &m4]));

    // An entry missing from the middle.
    assert_eq!(
        w.replay(&[log[0].clone(), log[2].clone()]),
        Err(LogRefusal::SequenceGap {
            expected: 2,
            found: 3
        })
    );
    // No log at all.
    assert_eq!(w.replay(&[]), Err(LogRefusal::Empty));
    // A verifier without the rule in force.
    assert_eq!(
        replay_context_log(&w.inception, &log, &Rules(vec![lax()])),
        Err(LogRefusal::RuleUnavailable {
            rule_hash: strict().rule_hash()
        })
    );
    // A stripped vote bundle.
    let mut stripped = log[1].clone();
    if let EntryAuthorityV0::Decision(d) = &mut stripped.authority {
        d.vote_bundles.pop();
    }
    assert!(matches!(
        w.replay(&[log[0].clone(), stripped]),
        Err(LogRefusal::Vote { at: 1, refusal }) if matches!(*refusal, AuthzRefusal::InputActMissing(_))
    ));
    // Missing authorization votes.
    let cp = checkpoint(&w.state(&log));
    let mut ev = w.authorize(&log, cp, &w.writer, &[w.m(0), w.m(1)]);
    ev.vote_bundles.clear();
    assert!(matches!(
        verify_checkpointed_authorization(&ev, &w.frontier(3, None), &rules()),
        Err(LogRefusal::Vote { .. })
    ));
}

// ---------------------------------------------------------------------------------------------
// Binding, ordering and identity of the context
// ---------------------------------------------------------------------------------------------

/// A decision is bound to its exact position and predecessor: re-sequencing the same votes for
/// the same transition later is refused.
#[test]
fn transition_decisions_cannot_be_replayed_at_another_position() {
    let w = world();
    let m4 = Member::new(0x04);
    let mut log = vec![w.genesis()];
    let grant_entry = w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]);
    log.push(grant_entry.clone());
    log.push(w.next(&log, revoke(&m4), &w.writer, &[w.m(0), w.m(1), w.m(2)]));
    // Re-grant m4 at position 4 by recycling the position-2 votes.
    let body = w.body(&log, grant(&m4));
    let EntryAuthorityV0::Decision(d) = grant_entry.authority else {
        unreachable!()
    };
    log.push(SignedLogEntryV0::decided(body, &w.writer, d.vote_bundles).unwrap());
    assert!(matches!(
        w.replay(&log),
        Err(LogRefusal::RuleRefused {
            at: 3,
            error: RuleError::ActNotBound { .. }
        })
    ));
}

#[test]
fn chain_order_and_context_are_enforced() {
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    log.push(w.next(&log, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1), w.m(2)]));
    // Reordered.
    assert!(matches!(
        w.replay(&[log[0].clone(), log[2].clone(), log[1].clone()]),
        Err(LogRefusal::SequenceGap { .. })
    ));
    // A genesis repeated later.
    let mut again = log.clone();
    again.truncate(2);
    let mut g = w.genesis();
    g.body.seq = 3;
    again.push(g);
    assert!(w.replay(&again).is_err());
    // A tampered entry breaks the next link (and its own signature).
    let mut tampered = log.clone();
    tampered[1].body.transition = grant(&Member::new(0x05));
    assert!(w.replay(&tampered).is_err());
    // Another context's frontier.
    let ev = w.authorize(
        &log,
        checkpoint(&w.state(&log)),
        &w.writer,
        &[w.m(0), w.m(1)],
    );
    let other = RelyingFrontierV0 {
        context: ContextRef([0xEE; 32]),
        min_seq: 0,
        pinned: None,
    };
    assert_eq!(
        verify_checkpointed_authorization(&ev, &other, &rules()),
        Err(LogRefusal::UntrustedContext)
    );
}

#[test]
fn no_op_transitions_are_refused() {
    let w = world();
    let log = vec![w.genesis()];
    for t in [
        grant(w.m(0)),
        revoke(&Member::new(0x0a)),
        TransitionV0::AdoptRule {
            slot: RuleSlot::Transition,
            rule_hash: strict().rule_hash(),
        },
        TransitionV0::AppointWriter {
            writer: World::pk(&w.writer),
        },
    ] {
        let e = w.next(&log, t, &w.writer, &[w.m(0), w.m(1)]);
        assert_eq!(
            w.replay(&[log[0].clone(), e]),
            Err(LogRefusal::NoOpTransition { seq: 2 })
        );
    }
}

#[test]
fn entry_and_authorization_encodings_are_domain_separated() {
    let w = world();
    let g = w.genesis();
    let bytes = g.body.canonical_bytes();
    assert!(bytes[4..].starts_with(icn_authz::effect_v0::LOG_ENTRY_DOMAIN));
    assert_eq!(LogEntryV0::decode(&bytes).unwrap(), g.body);
    let ev = w.authorize(
        &[g],
        checkpoint(&w.state(&[w.genesis()])),
        &w.writer,
        &[w.m(0)],
    );
    let a = ev.authorization.body.canonical_bytes();
    assert_eq!(
        CheckpointedAuthorizationV0::decode(&a).unwrap(),
        ev.authorization.body
    );
    // A log entry never decodes as an authorization, nor the reverse.
    assert!(CheckpointedAuthorizationV0::decode(&bytes).is_err());
    assert!(LogEntryV0::decode(&a).is_err());
}

// ---------------------------------------------------------------------------------------------
// Pinned limits: what the log does NOT detect. Each asserts the attack SUCCEEDS today.
// ---------------------------------------------------------------------------------------------

/// E10. A member submits a grant; the writer never sequences it. The presented log is valid and
/// says nothing about the omission. Detecting it needs evidence of submission outside the log.
#[test]
fn known_limit_e10_censorship_is_not_detected_by_the_log() {
    let w = world();
    let m4 = Member::new(0x04);
    let log = vec![w.genesis()];
    let _submitted_but_dropped = w.next(&log, grant(&m4), &w.writer, &[w.m(0), w.m(1)]);
    let state = w.replay(&log).unwrap();
    assert!(!state.eligible(ROLE).contains(&m4.subject));
}

/// Without a pin or a frontier, a relying party accepts whichever valid history it is shown:
/// a hash chain does not prove that no other history exists.
#[test]
fn known_limit_unpinned_party_accepts_either_branch() {
    let w = world();
    let base = vec![w.genesis()];
    let mut a = base.clone();
    a.push(w.next(&a, grant(&Member::new(0x04)), &w.writer, &[w.m(0), w.m(1)]));
    let mut b = base;
    b.push(w.next(&b, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    // Branch a has four members (three votes reach quorum); branch b has two.
    for (log, voters) in [
        (&a, vec![w.m(0), w.m(1), w.m(2)]),
        (&b, vec![w.m(0), w.m(1)]),
    ] {
        let ev = w.authorize(log, checkpoint(&w.state(log)), &w.writer, &voters);
        assert!(verify_checkpointed_authorization(&ev, &w.frontier(0, None), &rules()).is_ok());
    }
}

/// #2884 known gap D persists inside the log: votes are evaluated at the N1 position the writer
/// lists, so a device revoked after it still counts.
#[test]
fn known_limit_vote_position_freshness_is_still_writer_chosen() {
    let w = world();
    let mut members = vec![Member::new(0x01), Member::new(0x02), Member::new(0x03)];
    members[0].revoke_device();
    let w = World { members, ..w };
    let mut log = vec![w.genesis()];
    let body = w.body(&log, grant(&Member::new(0x04)));
    let matter = transition_subject_matter(&body);
    let votes = vec![
        w.m(0).vote_at(w.ctx, matter, VoteChoice::For, 1),
        w.m(1).vote(w.ctx, matter),
    ];
    log.push(SignedLogEntryV0::decided(body, &w.writer, votes).unwrap());
    assert!(w.replay(&log).is_ok());
}

// ---------------------------------------------------------------------------------------------
// Guards found uncovered by the mutation sweep
// ---------------------------------------------------------------------------------------------

/// A pin is a floor: the presented log contains the pinned position, but an authorization that
/// relies on an older position is refused even when `min_seq` alone would allow it.
#[test]
fn pinned_checkpoint_raises_the_frontier() {
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    let pinned = checkpoint(&w.state(&log));
    let old = checkpoint(&w.replay(&log[..1]).unwrap());
    let ev = w.authorize(&log, old, &w.writer, &[w.m(1), w.m(2)]);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(0, Some(pinned)), &rules()),
        Err(LogRefusal::CheckpointBelowFrontier {
            checkpoint: 1,
            frontier: 2
        })
    );
}

/// A resolver that hands back another rule under the adopted hash is refused, so a verifier's
/// rule table cannot silently substitute a laxer rule.
#[test]
fn a_resolver_that_substitutes_a_rule_is_refused() {
    struct Lying(ThresholdRuleV0);
    impl RuleResolver for Lying {
        fn resolve(&self, _: &[u8; 32]) -> Option<&dyn AdoptedRule> {
            Some(&self.0)
        }
    }
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, grant(&Member::new(0x04)), &w.writer, &[w.m(0)]));
    assert_eq!(
        replay_context_log(&w.inception, &log, &Lying(lax())),
        Err(LogRefusal::RuleImplementationMismatch)
    );
}

/// Votes for one operation do not authorize another: the signed subject matter must be the
/// effect's own commitment.
#[test]
fn votes_are_bound_to_the_effect_and_operation() {
    let w = world();
    let log = vec![w.genesis()];
    let cp = checkpoint(&w.state(&log));
    let mut ev = w.authorize(&log, cp, &w.writer, &[w.m(0), w.m(1)]);
    let mut body = ev.authorization.body.clone();
    body.operation_id = [0x0a; 32];
    ev.authorization = sign_checkpointed_authorization(body, &w.writer);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()),
        Err(LogRefusal::SubjectMatterNotBoundToEffect)
    );
    // Rebinding the subject matter to the new operation leaves the votes unbound.
    let mut body = ev.authorization.body.clone();
    body.subject_matter = body.effect.commitment(w.ctx, body.operation_id);
    ev.authorization = sign_checkpointed_authorization(body, &w.writer);
    assert!(matches!(
        verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()),
        Err(LogRefusal::RuleRefused {
            error: RuleError::ActNotBound { .. },
            ..
        })
    ));
}

/// Splicing: an entry whose decision was cast on one branch cannot be appended to another, even
/// where the same voters are eligible on both.
#[test]
fn an_entry_cannot_be_spliced_onto_another_history() {
    let w = world();
    let base = vec![w.genesis()];
    let mut a = base.clone();
    a.push(w.next(&a, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    let a3 = w.next(&a, grant(&Member::new(0x05)), &w.writer, &[w.m(0), w.m(1)]);
    let mut b = base;
    b.push(w.next(
        &b,
        TransitionV0::AdoptRule {
            slot: RuleSlot::Effect,
            rule_hash: lax().rule_hash(),
        },
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    b.push(a3);
    assert_eq!(w.replay(&b), Err(LogRefusal::BrokenChain { seq: 3 }));
}

/// An entry naming another context is refused even when this context's members voted on it.
#[test]
fn an_entry_naming_another_context_is_refused() {
    let w = world();
    let log = vec![w.genesis()];
    let mut body = w.body(&log, grant(&Member::new(0x05)));
    body.context = ContextRef([0xEE; 32]);
    let matter = transition_subject_matter(&body);
    let votes = vec![w.m(0).vote(w.ctx, matter), w.m(1).vote(w.ctx, matter)];
    let entry = SignedLogEntryV0::decided(body, &w.writer, votes).unwrap();
    assert_eq!(
        w.replay(&[log[0].clone(), entry]),
        Err(LogRefusal::ContextMismatch { seq: 2 })
    );
}

/// Genesis is anchored to the context ref, so a founder quorum cannot sign two "first" entries
/// that differ only in what they claim to extend.
#[test]
fn genesis_must_extend_the_context_ref() {
    let w = world();
    let mut g = w.genesis();
    g.body.prev = [0u8; 32];
    let g = SignedLogEntryV0::genesis(g.body, &[&w.founders[0], &w.founders[1]]).unwrap();
    assert_eq!(w.replay(&[g]), Err(LogRefusal::BrokenChain { seq: 1 }));
}

/// The authorization's own context field must be the trusted context: downstream consumption is
/// keyed by it.
#[test]
fn an_authorization_naming_another_context_is_refused() {
    let w = world();
    let log = vec![w.genesis()];
    let cp = checkpoint(&w.state(&log));
    let mut ev = w.authorize(&log, cp, &w.writer, &[w.m(0), w.m(1)]);
    let mut body = ev.authorization.body.clone();
    body.context = ContextRef([0xEE; 32]);
    ev.authorization = sign_checkpointed_authorization(body, &w.writer);
    assert_eq!(
        verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()),
        Err(LogRefusal::AuthorizationContextMismatch)
    );
}

/// A fork proof claims conflicting successors of ONE head. Two same-position entries on
/// already-divergent histories are not that proof (the real one is at the first divergence).
#[test]
fn a_fork_proof_needs_a_common_predecessor() {
    let w = world();
    let base = vec![w.genesis()];
    let mut a = base.clone();
    a.push(w.next(&a, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    a.push(w.next(&a, grant(&Member::new(0x05)), &w.writer, &[w.m(0), w.m(1)]));
    let mut b = base;
    b.push(w.next(&b, grant(&Member::new(0x04)), &w.writer, &[w.m(0), w.m(1)]));
    b.push(w.next(&b, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1), w.m(2)]));
    let late = LogForkProofV0 {
        first: a[2].clone(),
        second: b[2].clone(),
    };
    assert_eq!(late.equivocators(&w.inception), None);
    let first = find_log_fork(&w.inception, &a, &b).unwrap();
    assert_eq!(first.first.body.seq, 2);
}
// ---------------------------------------------------------------------------------------------
// Admission — `admit_checkpointed` (VERIFIED → ADMITTED; nothing here is executed or durable)
// ---------------------------------------------------------------------------------------------
//
// Each test is one requirement (A–T) of the checkpointed-admission slice. "Without consuming" is
// asserted twice: the backend's `consume` was never called, and a valid authorization for the
// same operation id still admits afterwards.

/// The target's clock, inside the authorization window. Admission uses it for expiry only.
const NOW: u64 = 1_000;

/// [`InMemoryConsumedSet`] that counts `consume` calls.
#[derive(Default)]
struct Counting {
    inner: InMemoryConsumedSet,
    calls: usize,
}

impl ConsumedSet for Counting {
    fn consume(
        &mut self,
        context: ContextRef,
        operation_id: [u8; 32],
        entry: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        self.calls += 1;
        self.inner.consume(context, operation_id, entry)
    }
}

/// A backend that always fails.
struct Failing;

impl ConsumedSet for Failing {
    fn consume(
        &mut self,
        _: ContextRef,
        _: [u8; 32],
        _: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        Err(ConsumedSetError("backend unavailable".into()))
    }
}

/// One shared set behind a lock taken **only** inside `consume`, so verification runs
/// concurrently and only the consumption step is serialized.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<InMemoryConsumedSet>>);

impl ConsumedSet for Shared {
    fn consume(
        &mut self,
        context: ContextRef,
        operation_id: [u8; 32],
        entry: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        self.0
            .lock()
            .map_err(|_| ConsumedSetError("poisoned".into()))?
            .consume(context, operation_id, entry)
    }
}

fn inputs_of(bundles: &[EvidenceBundle]) -> Vec<InputActV0> {
    let mut inputs: Vec<InputActV0> = bundles
        .iter()
        .map(|b| InputActV0 {
            act_id: b.act().act.act_id(),
            subject: b.subject(),
            position: b.evaluation_position(),
        })
        .collect();
    inputs.sort_by_key(|i| i.act_id);
    inputs
}

impl World {
    fn admit(
        &self,
        ev: &CheckpointedEvidenceV0,
        frontier: &RelyingFrontierV0,
        now: u64,
        consumed: &mut dyn ConsumedSet,
    ) -> Result<CheckpointedAdmission, AdmissionRefusal> {
        admit_checkpointed(ev, frontier, &rules(), self.target, now, consumed)
    }

    /// The honest authorization at genesis (members 0 and 1 For, Pass, [`OP`]).
    fn honest(&self) -> (Vec<SignedLogEntryV0>, CheckpointedEvidenceV0) {
        let log = vec![self.genesis()];
        let ev = self.authorize(
            &log,
            checkpoint(&self.state(&log)),
            &self.writer,
            &[self.m(0), self.m(1)],
        );
        (log, ev)
    }

    /// The same decision under #2884's designation model: same inception, founders, writer,
    /// rule, electorate and votes, for `op`.
    fn legacy(&self, op: [u8; 32], voters: &[&Member]) -> AuthorizationEvidenceV0 {
        let effect = self.effect();
        let matter = effect.commitment(self.ctx, op);
        let mut vote_bundles: Vec<EvidenceBundle> =
            voters.iter().map(|m| m.vote(self.ctx, matter)).collect();
        vote_bundles.sort_by_key(|b| b.act().act.act_id());
        let standing: Vec<StandingV0> = self
            .members
            .iter()
            .map(|m| StandingV0 {
                context: self.ctx,
                epoch: 1,
                subject: m.subject,
                role: ROLE.into(),
                not_after: NOT_AFTER,
            })
            .collect();
        let rule_hash = strict().rule_hash();
        let mut ev = AuthorizationEvidenceV0 {
            inception: self.inception.clone(),
            designation: SignedDesignation::sign(
                WriterDesignationV0 {
                    context: self.ctx,
                    epoch: 1,
                    writer: Self::pk(&self.writer),
                    start_head: [1; 32],
                },
                &[&self.founders[0], &self.founders[1]],
            )
            .unwrap(),
            standing: standing
                .iter()
                .map(|s| Signed::sign(s.clone(), &self.writer))
                .collect(),
            rule_adoption: Signed::sign(
                RuleAdoptionV0 {
                    context: self.ctx,
                    epoch: 1,
                    rule_hash,
                },
                &self.writer,
            ),
            rule_artifact: strict().artifact(),
            authorization: Signed::sign(
                AuthorizationV0 {
                    context: self.ctx,
                    epoch: 1,
                    subject_matter: matter,
                    rule_hash,
                    verdict: Verdict::Pass,
                    inputs: inputs_of(&vote_bundles),
                    roster_hash: roster_hash(&standing),
                    effect,
                    operation_id: op,
                    not_after: NOT_AFTER,
                },
                &self.writer,
            ),
            vote_bundles,
        };
        ev.canonicalize();
        ev
    }

    fn admit_legacy(
        &self,
        ev: &AuthorizationEvidenceV0,
        consumed: &mut dyn ConsumedSet,
    ) -> Result<Admission, AdmissionRefusal> {
        admit(ev, &strict(), self.ctx, self.target, NOW, 1, consumed)
    }
}

/// Q's probe: nothing was consumed for `(context, OP)`, because the honest authorization for
/// that operation id still admits.
fn assert_not_consumed(w: &World, consumed: &mut dyn ConsumedSet) {
    let (_, ev) = w.honest();
    assert!(
        w.admit(&ev, &w.frontier(1, None), NOW, consumed).is_ok(),
        "a refused request consumed the operation id"
    );
}

fn equivocation(r: Result<impl std::fmt::Debug, AdmissionRefusal>) -> EquivocationProof {
    match r {
        Err(AdmissionRefusal::Equivocation(proof)) => *proof,
        other => panic!("expected a conflict, got {other:?}"),
    }
}

/// A: a verified Pass for this target admits once, and the admission carries the log-derived
/// writer and the exact authorized operation.
#[test]
fn a_valid_checkpointed_pass_admits_once() {
    let w = world();
    let (log, ev) = w.honest();
    let cp = checkpoint(&w.state(&log));
    let frontier = w.frontier(1, Some(cp));
    let mut consumed = Counting::default();
    let admission = w.admit(&ev, &frontier, NOW, &mut consumed).unwrap();
    let v = admission.verified();
    assert_eq!(v.writer(), World::pk(&w.writer));
    assert_eq!(v.signed(), &ev.authorization);
    assert_eq!(v.authorization().operation_id, OP);
    assert_eq!(v.authorization().effect, w.effect());
    assert_eq!(v.presented_tip(), cp);
    assert_eq!(consumed.calls, 1);
    assert_eq!(
        w.admit(&ev, &frontier, NOW, &mut consumed).unwrap_err(),
        AdmissionRefusal::Replayed
    );
}

/// B: an honestly reported Fail is a verified decision, and is still not admitted.
#[test]
fn b_fail_verdict_is_refused_without_consuming() {
    let w = world();
    let log = vec![w.genesis()];
    let cp = checkpoint(&w.state(&log));
    let ev = w.authorize_op(
        &log,
        cp,
        &w.writer,
        &[w.m(0), w.m(1)],
        VoteChoice::Against,
        Verdict::Fail,
        OP,
    );
    assert!(verify_checkpointed_authorization(&ev, &w.frontier(1, None), &rules()).is_ok());
    let mut consumed = Counting::default();
    assert_eq!(
        w.admit(&ev, &w.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::VerdictNotPass
    );
    assert_eq!(consumed.calls, 0);
    assert_not_consumed(&w, &mut consumed);
}

/// C: the effect names another custodian.
#[test]
fn c_wrong_target_is_refused_without_consuming() {
    let w = world();
    let (_, ev) = w.honest();
    let mut consumed = Counting::default();
    let elsewhere = target_id(&principal(0xC2));
    assert_eq!(
        admit_checkpointed(
            &ev,
            &w.frontier(1, None),
            &rules(),
            elsewhere,
            NOW,
            &mut consumed
        )
        .unwrap_err(),
        AdmissionRefusal::WrongTarget
    );
    assert_eq!(consumed.calls, 0);
    assert_not_consumed(&w, &mut consumed);
}

/// D: a target serves only the context of its own frontier, whatever the evidence claims.
#[test]
fn d_wrong_context_is_refused_without_consuming() {
    let w = world();
    let other = world_in([0x2b; 32]);
    let (_, ev) = w.honest();
    let mut consumed = Counting::default();
    // A different context's frontier.
    assert_eq!(
        w.admit(&ev, &other.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::UntrustedContext
    );
    // This context's frontier, the other context's inception.
    let mut swapped = ev.clone();
    swapped.inception = other.inception.clone();
    assert_eq!(
        w.admit(&swapped, &w.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::UntrustedContext
    );
    assert_eq!(consumed.calls, 0);
    assert_not_consumed(&w, &mut consumed);
}

/// E: expiry is the target's clock against the signed `not_after`, inclusive.
#[test]
fn e_expired_authorization_is_refused_without_consuming() {
    let w = world();
    let (_, ev) = w.honest();
    let mut consumed = Counting::default();
    assert_eq!(
        w.admit(&ev, &w.frontier(1, None), NOT_AFTER + 1, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Expired {
            not_after: NOT_AFTER,
            now: NOT_AFTER + 1
        }
    );
    assert_eq!(consumed.calls, 0);
    assert!(w
        .admit(&ev, &w.frontier(1, None), NOT_AFTER, &mut consumed)
        .is_ok());
}

/// F: a checkpoint below the relying party's frontier (`min_seq`, or raised by a pin).
#[test]
fn f_checkpoint_below_the_frontier_is_refused() {
    let w = world();
    let mut log = vec![w.genesis()];
    let old = checkpoint(&w.state(&log));
    log.push(w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    let pin = checkpoint(&w.state(&log));
    let ev = w.authorize(&log, old, &w.writer, &[w.m(0), w.m(1)]);
    let mut consumed = Counting::default();
    for frontier in [w.frontier(2, None), w.frontier(0, Some(pin))] {
        assert_eq!(
            w.admit(&ev, &frontier, NOW, &mut consumed).unwrap_err(),
            AdmissionRefusal::Log(LogRefusal::CheckpointBelowFrontier {
                checkpoint: 1,
                frontier: 2
            })
        );
    }
    assert_eq!(consumed.calls, 0);
    // The same authorization is historically valid for a party at the lower frontier.
    assert!(w
        .admit(&ev, &w.frontier(1, None), NOW, &mut consumed)
        .is_ok());
}

/// G: presented history that stops before the pinned checkpoint.
#[test]
fn g_history_missing_the_pinned_checkpoint_is_refused() {
    let w = world();
    let mut log = vec![w.genesis()];
    log.push(w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    let pin = checkpoint(&w.state(&log));
    let truncated = &log[..1];
    let ev = w.authorize(
        truncated,
        checkpoint(&w.state(truncated)),
        &w.writer,
        &[w.m(0), w.m(1)],
    );
    let mut consumed = Counting::default();
    assert_eq!(
        w.admit(&ev, &w.frontier(1, Some(pin)), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Log(LogRefusal::PinnedCheckpointMissing { seq: 2 })
    );
    assert_eq!(consumed.calls, 0);
}

/// H: presented history from another branch than the one pinned.
#[test]
fn h_history_conflicting_with_the_pin_is_refused() {
    let w = world();
    let mut a = vec![w.genesis()];
    a.push(w.next(&a, grant(&Member::new(0x04)), &w.writer, &[w.m(0), w.m(1)]));
    let mut b = vec![w.genesis()];
    b.push(w.next(&b, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    let ev = w.authorize(&b, checkpoint(&w.state(&b)), &w.writer, &[w.m(0), w.m(1)]);
    let mut consumed = Counting::default();
    assert_eq!(
        w.admit(
            &ev,
            &w.frontier(1, Some(checkpoint(&w.state(&a)))),
            NOW,
            &mut consumed
        )
        .unwrap_err(),
        AdmissionRefusal::Log(LogRefusal::PinnedCheckpointMismatch { seq: 2 })
    );
    assert_eq!(consumed.calls, 0);
    assert_not_consumed(&w, &mut consumed);
}

/// I: revoked standing, an unauthorized rule change, and a writer-asserted rule all stay refused
/// through admission.
#[test]
fn i_revocation_and_rule_capture_stay_refused_through_admission() {
    let w = world();
    let mut consumed = Counting::default();

    // Revoked member's vote at the revocation checkpoint.
    let mut log = vec![w.genesis()];
    log.push(w.next(&log, revoke(w.m(2)), &w.writer, &[w.m(0), w.m(1)]));
    let ev = w.authorize(
        &log,
        checkpoint(&w.state(&log)),
        &w.writer,
        &[w.m(1), w.m(2)],
    );
    assert!(matches!(
        w.admit(&ev, &w.frontier(2, None), NOW, &mut consumed),
        Err(AdmissionRefusal::Log(LogRefusal::IneligibleVoter { subject, .. })) if subject == w.m(2).subject
    ));

    // A writer-sequenced effect-rule replacement without a passing decision.
    let mut log = vec![w.genesis()];
    let adopt = TransitionV0::AdoptRule {
        slot: RuleSlot::Effect,
        rule_hash: lax().rule_hash(),
    };
    log.push(w.next(&log, adopt, &w.writer, &[w.m(0)]));
    let ev = w.authorize(
        &log,
        checkpoint(&w.replay(&log[..1]).unwrap()),
        &w.writer,
        &[w.m(0), w.m(1)],
    );
    assert_eq!(
        w.admit(&ev, &w.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Log(LogRefusal::TransitionNotAuthorized { seq: 2 })
    );

    // The writer asserting the lax rule is in force, with one vote.
    let (_, mut ev) = w.honest();
    let mut body = ev.authorization.body.clone();
    body.rule_hash = lax().rule_hash();
    ev.authorization = sign_checkpointed_authorization(body, &w.writer);
    assert_eq!(
        w.admit(&ev, &w.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Log(LogRefusal::EffectRuleNotInForce)
    );
    assert_eq!(consumed.calls, 0);
    assert_not_consumed(&w, &mut consumed);
}

/// J: the identical authorization again — from re-encoded bytes, or with a longer presented
/// history — is a replay, not a new admission.
#[test]
fn j_identical_replay_is_refused() {
    let w = world();
    let (mut log, ev) = w.honest();
    let mut consumed = InMemoryConsumedSet::default();
    w.admit(&ev, &w.frontier(1, None), NOW, &mut consumed)
        .unwrap();

    let mut decoded = ev.clone();
    decoded.authorization = Signed::decode(&ev.authorization.encode()).unwrap();
    assert_eq!(
        w.admit(&decoded, &w.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Replayed
    );

    log.push(w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    let mut longer = ev.clone();
    longer.log = log;
    assert_eq!(
        w.admit(&longer, &w.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Replayed
    );
}

/// K: a different authorization by the same writer for a consumed (context, operation id) is a
/// conflict attributable to that writer; a tampered proof is not one.
#[test]
fn k_conflicting_reuse_by_one_writer_is_attributable_equivocation() {
    let w = world();
    let (mut log, first) = w.honest();
    let mut consumed = InMemoryConsumedSet::default();
    w.admit(&first, &w.frontier(1, None), NOW, &mut consumed)
        .unwrap();

    log.push(w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    let second = w.authorize(
        &log,
        checkpoint(&w.state(&log)),
        &w.writer,
        // Four members after the grant: three votes meet the 2/3 quorum.
        &[w.m(0), w.m(1), w.m(2)],
    );
    let proof = equivocation(w.admit(&second, &w.frontier(1, None), NOW, &mut consumed));
    assert!(proof.is_valid());
    assert_eq!(
        proof.classify(),
        Some(ConflictClass::Equivocation {
            signer: World::pk(&w.writer)
        })
    );
    assert_eq!(
        proof.first.authorization,
        ConsumedAuthorization::Checkpointed(first.authorization.clone())
    );
    assert_eq!(
        proof.second.authorization,
        ConsumedAuthorization::Checkpointed(second.authorization.clone())
    );

    // A proof whose second statement was altered after signing proves nothing.
    let mut forged = proof.clone();
    if let ConsumedAuthorization::Checkpointed(s) = &mut forged.second.authorization {
        s.body.not_after += 1;
    }
    assert!(!forged.is_valid());
    assert_eq!(forged.classify(), None);
    // Nor does a "proof" of one statement against itself.
    let mut same = proof.clone();
    same.second = same.first.clone();
    assert_eq!(same.classify(), None);
    // The first admission remains the one consumed.
    assert_eq!(
        w.admit(&first, &w.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Replayed
    );
}

/// L: legacy and checkpointed admission share one `(context, operation_id)` space, in either
/// order. One writer key signed both; the kinds differ, so the conflict is reported as
/// cross-kind, not as equivocation.
#[test]
fn l_legacy_and_checkpointed_cannot_both_consume_one_operation() {
    let w = world();
    let (_, checkpointed) = w.honest();
    let legacy = w.legacy(OP, &[w.m(0), w.m(1)]);
    let frontier = w.frontier(1, None);
    // Each is admissible on its own: the collision is real, not a fixture artefact.
    assert!(w
        .admit_legacy(&legacy, &mut InMemoryConsumedSet::default())
        .is_ok());
    assert!(w
        .admit(
            &checkpointed,
            &frontier,
            NOW,
            &mut InMemoryConsumedSet::default()
        )
        .is_ok());

    // Legacy first.
    let mut consumed = InMemoryConsumedSet::default();
    w.admit_legacy(&legacy, &mut consumed).unwrap();
    let proof = equivocation(w.admit(&checkpointed, &frontier, NOW, &mut consumed));
    assert!(proof.is_valid());
    assert_eq!(proof.classify(), Some(ConflictClass::CrossKind));
    assert!(matches!(
        proof.first.authorization,
        ConsumedAuthorization::Legacy(_)
    ));
    assert_eq!(
        w.admit_legacy(&legacy, &mut consumed).unwrap_err(),
        AdmissionRefusal::Replayed
    );

    // Checkpointed first.
    let mut consumed = InMemoryConsumedSet::default();
    w.admit(&checkpointed, &frontier, NOW, &mut consumed)
        .unwrap();
    let proof = equivocation(w.admit_legacy(&legacy, &mut consumed));
    assert!(proof.is_valid());
    assert_eq!(proof.classify(), Some(ConflictClass::CrossKind));
    assert!(matches!(
        proof.first.authorization,
        ConsumedAuthorization::Checkpointed(_)
    ));
    assert_eq!(
        w.admit(&checkpointed, &frontier, NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Replayed
    );
}

/// M: operation ids are scoped to their context. Two contexts with the same founders, writer,
/// members, target and operation id are two operations; neither context's evidence is
/// admitted under the other's frontier.
#[test]
fn m_operation_ids_are_scoped_to_their_context() {
    let a = world();
    let b = world_in([0x2b; 32]);
    assert_ne!(a.ctx, b.ctx);
    assert_eq!(a.target, b.target);
    let (_, ev_a) = a.honest();
    let (_, ev_b) = b.honest();
    assert_eq!(
        ev_a.authorization.body.operation_id,
        ev_b.authorization.body.operation_id
    );

    let mut consumed = InMemoryConsumedSet::default();
    assert!(a
        .admit(&ev_a, &a.frontier(1, None), NOW, &mut consumed)
        .is_ok());
    assert!(b
        .admit(&ev_b, &b.frontier(1, None), NOW, &mut consumed)
        .is_ok());
    assert_eq!(
        a.admit(&ev_a, &a.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Replayed
    );
    assert_eq!(
        b.admit(&ev_b, &a.frontier(1, None), NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::UntrustedContext
    );
    // Within one context, a different operation id is a different operation.
    let log = vec![a.genesis()];
    let other_op = a.authorize_op(
        &log,
        checkpoint(&a.state(&log)),
        &a.writer,
        &[a.m(0), a.m(1)],
        VoteChoice::For,
        Verdict::Pass,
        [0x0a; 32],
    );
    assert!(a
        .admit(&other_op, &a.frontier(1, None), NOW, &mut consumed)
        .is_ok());
}

/// N: a backend failure yields no admission, and is not mistaken for a replay or a conflict.
#[test]
fn n_consumed_set_failure_fails_closed() {
    let w = world();
    let (_, ev) = w.honest();
    assert_eq!(
        w.admit(&ev, &w.frontier(1, None), NOW, &mut Failing)
            .unwrap_err(),
        AdmissionRefusal::ConsumedSet(ConsumedSetError("backend unavailable".into()))
    );
}

/// O: stripped, unlisted or relisted N4 evidence.
#[test]
fn o_invalid_or_incomplete_vote_evidence_is_refused() {
    let w = world();
    let (_, honest) = w.honest();
    let mut consumed = Counting::default();
    let refused = |ev: &CheckpointedEvidenceV0, consumed: &mut Counting| match w.admit(
        ev,
        &w.frontier(1, None),
        NOW,
        consumed,
    ) {
        Err(AdmissionRefusal::Log(LogRefusal::Vote { refusal, .. })) => *refusal,
        other => panic!("expected a vote refusal, got {other:?}"),
    };

    let mut stripped = honest.clone();
    stripped.vote_bundles.pop();
    assert!(matches!(
        refused(&stripped, &mut consumed),
        AuthzRefusal::InputActMissing(_)
    ));

    let mut unlisted = honest.clone();
    unlisted
        .vote_bundles
        .push(w.m(2).vote(w.ctx, w.effect().commitment(w.ctx, OP)));
    assert!(matches!(
        refused(&unlisted, &mut consumed),
        AuthzRefusal::UnlistedAct(_)
    ));

    let mut duplicated = honest.clone();
    let again = duplicated.vote_bundles[0].clone();
    duplicated.vote_bundles.push(again);
    assert!(matches!(
        refused(&duplicated, &mut consumed),
        AuthzRefusal::DuplicateAct(_)
    ));
    assert_eq!(consumed.calls, 0);
    assert_not_consumed(&w, &mut consumed);
}

/// P: after a succession, the outgoing writer's old authorization is history (refused by a
/// frontier at the appointment), its new one is not the writer's, and the successor's admits.
/// A successor reusing an operation id the predecessor consumed conflicts across two keys,
/// which is not single-signer equivocation.
#[test]
fn p_writer_succession_and_stale_writer_through_admission() {
    let w = world();
    let w2 = key(0xA2);
    let mut log = vec![w.genesis()];
    let before = checkpoint(&w.state(&log));
    let appoint = TransitionV0::AppointWriter {
        writer: World::pk(&w2),
    };
    log.push(w.next(&log, appoint, &w2, &[w.m(0), w.m(1)]));
    let after = checkpoint(&w.state(&log));
    let at_appointment = w.frontier(2, None);
    let mut consumed = Counting::default();

    let stale_history = w.authorize(&log, before, &w.writer, &[w.m(0), w.m(1)]);
    assert_eq!(
        w.admit(&stale_history, &at_appointment, NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Log(LogRefusal::CheckpointBelowFrontier {
            checkpoint: 1,
            frontier: 2
        })
    );
    let stale_writer = w.authorize(&log, after, &w.writer, &[w.m(0), w.m(1)]);
    assert_eq!(
        w.admit(&stale_writer, &at_appointment, NOW, &mut consumed)
            .unwrap_err(),
        AdmissionRefusal::Log(LogRefusal::AuthorizationNotByWriter)
    );
    assert_eq!(consumed.calls, 0);

    let successor = w.authorize(&log, after, &w2, &[w.m(0), w.m(1)]);
    let admitted = w
        .admit(&successor, &at_appointment, NOW, &mut consumed)
        .unwrap();
    assert_eq!(admitted.verified().writer(), World::pk(&w2));

    // A party at the lower frontier admitted the predecessor's authorization first.
    let mut consumed = InMemoryConsumedSet::default();
    w.admit(&stale_history, &w.frontier(1, None), NOW, &mut consumed)
        .unwrap();
    let proof = equivocation(w.admit(&successor, &w.frontier(1, None), NOW, &mut consumed));
    assert!(proof.is_valid());
    assert_eq!(proof.classify(), Some(ConflictClass::DifferentSigners));
}

/// S: concurrent admissions of the same and of conflicting authorizations, sharing one set, end
/// with exactly one consumption. Verification runs in parallel; only `consume` is serialized.
#[test]
fn s_concurrent_admission_consumes_at_most_once() {
    let w = world();
    let (mut log, same) = w.honest();
    log.push(w.next(
        &log,
        grant(&Member::new(0x04)),
        &w.writer,
        &[w.m(0), w.m(1)],
    ));
    let conflicting = w.authorize(
        &log,
        checkpoint(&w.state(&log)),
        &w.writer,
        // Four members after the grant: three votes meet the 2/3 quorum.
        &[w.m(0), w.m(1), w.m(2)],
    );
    let frontier = w.frontier(1, None);
    let shared = Shared::default();
    let target = w.target;

    let results: Vec<Result<CheckpointedAdmission, AdmissionRefusal>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let ev = if i % 2 == 0 { &same } else { &conflicting };
                let mut set = shared.clone();
                let frontier = &frontier;
                s.spawn(move || admit_checkpointed(ev, frontier, &rules(), target, NOW, &mut set))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results.iter().all(|r| matches!(
        r,
        Ok(_) | Err(AdmissionRefusal::Replayed) | Err(AdmissionRefusal::Equivocation(_))
    )));
}

/// T: a valid admission needs the writer in force at the checkpoint. Re-signing the honest body
/// under another key — or under a founder — does not admit, and consumes nothing.
#[test]
fn t_resigned_authorization_is_refused_without_consuming() {
    let w = world();
    let (_, honest) = w.honest();
    let mut consumed = Counting::default();
    for impostor in [key(0xEE), w.founders[0].clone()] {
        let mut ev = honest.clone();
        ev.authorization =
            sign_checkpointed_authorization(honest.authorization.body.clone(), &impostor);
        assert_eq!(
            w.admit(&ev, &w.frontier(1, None), NOW, &mut consumed)
                .unwrap_err(),
            AdmissionRefusal::Log(LogRefusal::AuthorizationNotByWriter)
        );
    }
    assert_eq!(consumed.calls, 0);
    assert_not_consumed(&w, &mut consumed);
}
