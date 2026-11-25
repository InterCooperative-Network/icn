//! Context Authority Log v0: the writer sequences institutional transitions but cannot
//! manufacture them. Each `eN_*` test is one attack from the acceptance plan (E1–E10); each
//! asserts the exact refusal, or — for E10 — pins what the log cannot detect.
//!
//! Rules are real `ThresholdRuleV0`s and votes are real N4 device acts carried as evidence
//! bundles, so every decision goes through the unchanged N1/N4 verifier.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    find_log_fork, replay_context_log, sign_checkpointed_authorization, target_id,
    transition_subject_matter, verify_checkpointed_authorization, AdoptedRule, AuthzRefusal,
    CanonicalV0, CheckpointedAuthorizationV0, CheckpointedEvidenceV0, ContextInceptionV0,
    ContextRef, ContextStateV0, EffectV0, EntryAuthorityV0, LogCheckpointV0, LogEntryV0,
    LogForkProofV0, LogRefusal, RelyingFrontierV0, RuleBinding, RuleError, RuleResolver, RuleSlot,
    SignedLogEntryV0, TransitionV0, Verdict,
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
    let founders = [key(0xF1), key(0xF2), key(0xF3)];
    let inception = ContextInceptionV0::new(
        founders
            .iter()
            .map(|k| PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap())
            .collect(),
        2,
        [0x1a; 32],
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
        let effect = self.effect();
        let matter = effect.commitment(self.ctx, OP);
        let mut vote_bundles: Vec<EvidenceBundle> =
            voters.iter().map(|m| m.vote(self.ctx, matter)).collect();
        vote_bundles.sort_by_key(|b| b.act().act.act_id());
        let inputs = vote_bundles
            .iter()
            .map(|b| icn_authz::effect_v0::InputActV0 {
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
    assert_eq!(v.writer, World::pk(&w.writer));
    assert_eq!(v.presented_tip, cp);
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
