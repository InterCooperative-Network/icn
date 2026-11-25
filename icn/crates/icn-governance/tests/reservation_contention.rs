//! Context Authority Log v0: the writer sequences institutional transitions but cannot
//! manufacture them. Each `eN_*` test is one attack from the acceptance plan (E1–E10); each
//! asserts the exact refusal, or — for E10 — pins what the log cannot detect.
//!
//! Rules are real `ThresholdRuleV0`s and votes are real N4 device acts carried as evidence
//! bundles, so every decision goes through the unchanged N1/N4 verifier.

#![allow(dead_code, unused_imports, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    admit, admit_checkpointed, find_log_fork, preflight_checkpointed, replay_context_log,
    roster_hash, sign_checkpointed_authorization, target_id, transition_subject_matter,
    verify_checkpointed_authorization, Admission, AdmissionRefusal, AdoptedRule,
    AuthorizationEvidenceV0, AuthorizationV0, AuthzRefusal, CanonicalV0, CheckpointedAdmission,
    CheckpointedAuthorizationV0, CheckpointedEvidenceV0, ConflictClass, ConsumedAuthorization,
    ConsumedEntry, ConsumedSet, ConsumedSetError, ContextInceptionV0, ContextRef, ContextStateV0,
    EffectV0, EntryAuthorityV0, EquivocationProof, InMemoryConsumedSet, InputActV0,
    LogCheckpointV0, LogEntryV0, LogForkProofV0, LogRefusal, RelyingFrontierV0, RuleAdoptionV0,
    RuleBinding, RuleError, RuleResolver, RuleSlot, Signed, SignedDesignation, SignedLogEntryV0,
    StandingV0, TransitionV0, Verdict, WriterDesignationV0,
};
use icn_authz_store::{
    DurabilityBarrier, DurableConsumedSet, LocalReservationPolicyV0, LocalResourceReservations,
    ReservationError, ReservationReconciliation, ReservationRequestV0, ResourcePoolV0,
};
use icn_governance::authz_threshold::{GovernanceActV0, Ratio, ThresholdRuleV0};
use icn_governance::vote::VoteChoice;
use icn_identity::authority_log::{
    authorize_event, revoke_event, AuthorityStore, CapabilitySet, ContextNonce, ContinuityRoot,
    DeviceCapability, EstablishmentKind, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1};
use icn_identity::evidence_bundle::EvidenceBundle;
use icn_store::{SledStore, Store};

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
        self.authorize_effect(
            log,
            checkpoint,
            writer,
            voters,
            choice,
            verdict,
            self.effect(),
            op,
        )
    }

    /// [`World::authorize_op`] with an exact caller-supplied effect.
    #[allow(clippy::too_many_arguments)]
    fn authorize_effect(
        &self,
        log: &[SignedLogEntryV0],
        checkpoint: LogCheckpointV0,
        writer: &SigningKey,
        voters: &[&Member],
        choice: VoteChoice,
        verdict: Verdict,
        effect: EffectV0,
        op: [u8; 32],
    ) -> CheckpointedEvidenceV0 {
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

// Fixture snapshot from context_log.rs at cff3681: retained here to avoid editing
// helper files currently locked by the parent stack. Consolidation is follow-up work.
const NOW: u64 = 1_000;
#[test]
fn independently_authorized_requests_at_one_revision_require_reauthorization() {
    let w = world();
    let log = vec![w.genesis()];
    let pool_id = [0x71; 32];
    let policy_hash = [0x72; 32];
    let pool = ResourcePoolV0 {
        context: w.ctx,
        pool_id,
        custodian: w.target,
        unit: "units".into(),
        total_capacity: 10,
        available_capacity: 10,
        revision: 0,
        policy: LocalReservationPolicyV0 {
            enabled: true,
            max_quantity_per_reservation: 10,
            policy_hash,
        },
    };
    let first = ReservationRequestV0 {
        context: w.ctx,
        operation_id: [0xa1; 32],
        pool_id,
        expected_pool_revision: 0,
        quantity: 2,
        unit: "units".into(),
        request_params_hash: [0x73; 32],
        policy_hash,
    };
    let second = ReservationRequestV0 {
        operation_id: [0xa2; 32],
        ..first.clone()
    };
    let authorize = |request: &ReservationRequestV0| {
        w.authorize_effect(
            &log,
            checkpoint(&w.state(&log)),
            &w.writer,
            &[w.m(0), w.m(1)],
            VoteChoice::For,
            Verdict::Pass,
            request.effect(w.target).unwrap(),
            request.operation_id,
        )
    };
    // Both authorizations are complete before either allocation changes capacity.
    let evidence_a = authorize(&first);
    let evidence_b = authorize(&second);
    let original_signed = evidence_b.authorization.encode();
    let frontier = w.frontier(1, None);
    for evidence in [&evidence_a, &evidence_b] {
        preflight_checkpointed(evidence, &frontier, &rules(), w.target, NOW).unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    {
        let store = LocalResourceReservations::open(dir.path(), w.target).unwrap();
        store.initialize_pool(pool).unwrap();
        store
            .reserve_checkpointed(&evidence_a, &frontier, &rules(), NOW, &first)
            .unwrap();
        assert_eq!(
            store.reserve_checkpointed(&evidence_b, &frontier, &rules(), NOW, &second),
            Err(ReservationError::StalePool {
                expected: 0,
                actual: 1
            }),
        );
        assert_eq!(evidence_b.authorization.encode(), original_signed);
        let state = store.inspect_pool(w.ctx, pool_id).unwrap().unwrap();
        assert_eq!((state.revision, state.available_capacity), (1, 8));
        assert!(store
            .inspect_consumed(w.ctx, second.operation_id)
            .unwrap()
            .is_none());
        assert!(store
            .inspect_reservation(w.ctx, second.operation_id)
            .unwrap()
            .is_none());
        assert_eq!(
            store.reconcile(w.ctx, second.operation_id).unwrap(),
            ReservationReconciliation::Absent
        );
    }
    let store = LocalResourceReservations::open(dir.path(), w.target).unwrap();
    assert!(matches!(
        store.reconcile(w.ctx, first.operation_id).unwrap(),
        ReservationReconciliation::Committed { .. }
    ));
    assert_eq!(
        store.reconcile(w.ctx, second.operation_id).unwrap(),
        ReservationReconciliation::Absent
    );
    assert_eq!(
        store.reserve_checkpointed(&evidence_b, &frontier, &rules(), NOW, &second),
        Err(ReservationError::StalePool {
            expected: 0,
            actual: 1
        }),
    );
    let state = store.inspect_pool(w.ctx, pool_id).unwrap().unwrap();
    assert_eq!((state.revision, state.available_capacity), (1, 8));
    // Merely updating the request revision cannot reuse the old signature.
    let revised = ReservationRequestV0 {
        expected_pool_revision: 1,
        ..second
    };
    assert!(matches!(
        store.reserve_checkpointed(&evidence_b, &frontier, &rules(), NOW, &revised),
        Err(ReservationError::InvalidInput(_))
    ));
    let refreshed = authorize(&revised);
    store
        .reserve_checkpointed(&refreshed, &frontier, &rules(), NOW, &revised)
        .unwrap();
    let state = store.inspect_pool(w.ctx, pool_id).unwrap().unwrap();
    assert_eq!((state.revision, state.available_capacity), (2, 6));
}

#[path = "support/institutional_reservation_witness.rs"]
mod institutional_reservation_witness;
