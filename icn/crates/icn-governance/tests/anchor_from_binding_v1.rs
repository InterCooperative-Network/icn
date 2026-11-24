//! P3: a trust anchor derived from the stored institutional authority binding, its V1 inception
//! and its founder-signed genesis (design spec §2.2 "EXTEND: anchor derivation").
//!
//! The fixture section (up to the `P3` banner) is copied from `institutional_authority_proof.rs`
//! with one change: the institution is founded on a `ContextInceptionV1` committing to its domain,
//! instead of a V0 inception with a harness label. It is copied, not shared, because that file is
//! held by other lanes.

#![allow(dead_code, unused_imports)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    find_log_fork, replay_context_log, sign_checkpointed_authorization, transition_subject_matter,
    AdmissionRefusal, AdoptedRule, CanonicalV0, CheckpointedAuthorizationV0,
    CheckpointedEvidenceV0, ContextInceptionV0, ContextInceptionV1, ContextRef, EffectV0,
    InputActV0, LogCheckpointV0, LogEntryV0, LogRefusal, RelyingFrontierV0, RuleResolver, Signed,
    SignedLogEntryV0, TransitionV0, Verdict,
};
use icn_authz_store::{
    AdmissionGradeV0, AdmissionPolicyRefV0, AdmissionRegistryV0, AdmissionRequirementV1,
    DurabilityBarrier, LocalReservationPolicyV0, LocalResourceReservations, ReservationError,
    ReservationReconciliation, ReservationRequestV0, ResourcePoolRecord, ResourcePoolV0,
    ResourcePoolV1,
};
use icn_governance::authz_threshold::{GovernanceActV0, Ratio, ThresholdRuleV0};
use icn_governance::domain::{served_domain_commitment, GovernanceDomainId};
use icn_governance::institutional_admission::{
    admit_institutional, verify_institutional_authorization, InstitutionalAdmissionError,
    InstitutionalAdmissionVerifierV0, InstitutionalEvidenceV0, InstitutionalRefusal,
    InstitutionalSupplementV0, InstitutionalTrustAnchorV0, Legitimacy, RecognitionRequirement,
};
use icn_governance::subject_recognition::RecognitionError;
use icn_governance::subject_recognition::{RecognitionBasisKind, SubjectRecognitionV1};
use icn_governance::vote::VoteChoice;
use icn_identity::authority_log::{
    AuthorityBody, AuthorityStore, ContinuityRoot, DeviceCapability, EstablishmentKind,
    PrincipalKey, SignedAuthorityEvent, SubjectId, WitnessSignature,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1};
use icn_identity::evidence_bundle::EvidenceBundle;
use icn_identity::subject_context::{
    incept_subject_context_v1, verify_subject_context_genesis_v1, ContextSalt,
    SubjectContextDescriptor, SubjectContextGenesisV1, SubjectContextKind,
    SUBJECT_CONTEXT_GENESIS_VERSION,
};
use icn_kernel_api::canonical::{CanonicalHasher, DomainTag};
use icn_store::SledStore;

/// The synthetic rehearsal institution's governance-domain id.
const DOMAIN: &str = "rehearsal.synthetic-coop.example";
/// Another domain, for foreign-Subject and wrong-binding cases.
const FOREIGN_DOMAIN: &str = "rehearsal.other-coop.example";
const ROLE: &str = "member";
const NOW: u64 = 1_000;
const NOT_AFTER: u64 = 1_500;
const OP: [u8; 32] = [0x0a; 32];
const POOL: [u8; 32] = [0x71; 32];
const POLICY: [u8; 32] = [0x72; 32];

/// An uninterpreted label so contexts founded for different synthetic domains differ. Nothing
/// verifies it; the domain/context binding is the relying party's [`InstitutionalTrustAnchorV0`].
fn harness_context_label(domain: &str) -> [u8; 32] {
    let mut h = CanonicalHasher::new(DomainTag::new(
        b"icn:test:harness:authz-context-domain-binding:v0",
    ));
    h.str(domain);
    h.finish()
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn pk(k: &SigningKey) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap()
}

// ---------------------------------------------------------------------------------------------
// GEN-A Subject with its own device
// ---------------------------------------------------------------------------------------------

/// A fresh GEN-A context-scoped Subject. The device generated its own key; the Subject's
/// generation-0 authority signed the authorize event. No legacy `Did` appears anywhere.
struct GenASubject {
    genesis: SubjectContextGenesisV1,
    subject: SubjectId,
    device_key: SigningKey,
    events: Vec<SignedAuthorityEvent>,
}

impl GenASubject {
    fn incept(domain: &str, seed: u8) -> Self {
        let descriptor = SubjectContextDescriptor::governance_domain_v1(
            domain,
            ContextSalt::from_bytes([seed ^ 0x5a; 32]),
        )
        .unwrap();
        let root = ContinuityRoot::with_plan(
            [seed; 32],
            descriptor.context_nonce(),
            vec![EstablishmentKind::Rotate; 2],
        );
        let device_key = key(seed.wrapping_add(0x40));
        let genesis = incept_subject_context_v1(&descriptor, &root, pk(&device_key)).unwrap();
        // GEN-A verification derives the Subject and device; nothing is asserted by the bundle.
        let verified = verify_subject_context_genesis_v1(
            &genesis,
            SubjectContextKind::GovernanceDomainV1,
            domain,
        )
        .unwrap();
        assert_eq!(verified.device, pk(&device_key));
        // The two N1 events the genesis carries, rebuilt from their canonical bytes.
        let events = vec![
            SignedAuthorityEvent::new(
                AuthorityBody::decode(&genesis.inception_body_bytes).unwrap(),
                genesis.inception_witness,
            ),
            SignedAuthorityEvent::new(
                AuthorityBody::decode(&genesis.authorize_body_bytes).unwrap(),
                genesis.authorize_witness,
            ),
        ];
        GenASubject {
            subject: verified.subject,
            genesis,
            device_key,
            events,
        }
    }

    /// A Subject-signed governance act (N4 device act over a `GovernanceActV0`), as an evidence
    /// bundle assembled from the Subject's own N1 history.
    fn act(&self, ctx: ContextRef, matter: [u8; 32], choice: VoteChoice) -> EvidenceBundle {
        let payload = GovernanceActV0 {
            context: ctx.0,
            proposal: matter,
            choice,
            act_nonce: [0x77; 32],
        }
        .canonical_bytes();
        let act = DeviceActV1::new(
            self.subject,
            pk(&self.device_key),
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

    fn parts(&self) -> GenesisParts {
        GenesisParts::of(&self.genesis)
    }
}

// ---------------------------------------------------------------------------------------------
// The synthetic institution's authority context
// ---------------------------------------------------------------------------------------------

struct Rules(Vec<ThresholdRuleV0>);

impl RuleResolver for Rules {
    fn resolve(&self, rule_hash: &[u8; 32]) -> Option<&dyn AdoptedRule> {
        self.0
            .iter()
            .find(|r| &r.rule_hash() == rule_hash)
            .map(|r| r as &dyn AdoptedRule)
    }
}

/// The adopted rule for both slots: 2/3 quorum, 2/3 approval over Subjects holding `ROLE`.
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

struct Institution {
    founders: [SigningKey; 3],
    inception: ContextInceptionV1,
    ctx: ContextRef,
    writer: SigningKey,
    members: Vec<GenASubject>,
    custodian: [u8; 32],
}

impl Institution {
    /// Three founder stewards (2-of-3), a writer, three GEN-A members of `DOMAIN`, and a context
    /// whose label carries the harness binding to `bound_domain`.
    fn new(bound_domain: &str) -> Self {
        let founders = [key(0xF1), key(0xF2), key(0xF3)];
        // P3: a V1 inception committing to `bound_domain` (no harness label convention).
        let inception = ContextInceptionV1::new(
            founders.iter().map(pk).collect(),
            2,
            [0x1b; 32],
            served_domain_commitment(&GovernanceDomainId(bound_domain.to_string())).unwrap(),
        )
        .unwrap();
        Institution {
            ctx: inception.context_ref(),
            inception,
            founders,
            writer: key(0xA1),
            members: vec![
                GenASubject::incept(DOMAIN, 0x01),
                GenASubject::incept(DOMAIN, 0x02),
                GenASubject::incept(DOMAIN, 0x03),
            ],
            custodian: icn_authz::effect_v0::target_id(&pk(&key(0xC1))),
        }
    }

    fn m(&self, i: usize) -> &GenASubject {
        &self.members[i]
    }

    fn genesis_entry(&self) -> SignedLogEntryV0 {
        let mut standing: Vec<(SubjectId, String)> = self
            .members
            .iter()
            .map(|m| (m.subject, ROLE.to_string()))
            .collect();
        standing.sort();
        let body = LogEntryV0 {
            context: self.ctx,
            seq: 1,
            prev: self.ctx.0,
            transition: TransitionV0::Genesis {
                writer: pk(&self.writer),
                transition_rule: strict().rule_hash(),
                effect_rule: strict().rule_hash(),
                standing,
            },
        };
        SignedLogEntryV0::genesis(body, &[&self.founders[0], &self.founders[1]]).unwrap()
    }

    fn tip(&self, log: &[SignedLogEntryV0]) -> LogCheckpointV0 {
        replay_context_log(&self.inception, log, &rules())
            .unwrap()
            .tip()
            .checkpoint
    }

    /// The next entry, decided by For acts from `voters` and sequenced by the writer.
    fn decided(
        &self,
        log: &[SignedLogEntryV0],
        transition: TransitionV0,
        voters: &[&GenASubject],
    ) -> SignedLogEntryV0 {
        let tip = self.tip(log);
        let body = LogEntryV0 {
            context: self.ctx,
            seq: tip.seq + 1,
            prev: tip.head,
            transition,
        };
        let matter = transition_subject_matter(&body);
        let votes = voters
            .iter()
            .map(|m| m.act(self.ctx, matter, VoteChoice::For))
            .collect();
        SignedLogEntryV0::decided(body, &self.writer, votes).unwrap()
    }

    fn request(&self, op: [u8; 32], quantity: u64) -> ReservationRequestV0 {
        ReservationRequestV0 {
            context: self.ctx,
            operation_id: op,
            pool_id: POOL,
            expected_pool_revision: 0,
            quantity,
            unit: "cpu-hours".into(),
            request_params_hash: [0x73; 32],
            policy_hash: POLICY,
        }
    }

    fn pool(&self, capacity: u64) -> ResourcePoolV0 {
        ResourcePoolV0 {
            context: self.ctx,
            pool_id: POOL,
            custodian: self.custodian,
            unit: "cpu-hours".into(),
            total_capacity: capacity,
            available_capacity: capacity,
            revision: 0,
            policy: LocalReservationPolicyV0 {
                enabled: true,
                max_quantity_per_reservation: capacity,
                policy_hash: POLICY,
            },
        }
    }

    /// A checkpointed authorization of exactly `effect` under `op`, at `checkpoint`, with For acts
    /// from `voters` and a Pass verdict the writer signs (and every verifier re-derives).
    fn authorize(
        &self,
        log: &[SignedLogEntryV0],
        checkpoint: LogCheckpointV0,
        voters: &[&GenASubject],
        effect: EffectV0,
        op: [u8; 32],
    ) -> CheckpointedEvidenceV0 {
        let matter = effect.commitment(self.ctx, op);
        let mut vote_bundles: Vec<EvidenceBundle> = voters
            .iter()
            .map(|m| m.act(self.ctx, matter, VoteChoice::For))
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
            operation_id: op,
            not_after: NOT_AFTER,
        };
        CheckpointedEvidenceV0 {
            inception: self.inception.clone().into(),
            log: log.to_vec(),
            authorization: sign_checkpointed_authorization(body, &self.writer),
            vote_bundles,
        }
    }

    fn reservation_evidence(
        &self,
        log: &[SignedLogEntryV0],
        voters: &[&GenASubject],
        request: &ReservationRequestV0,
    ) -> CheckpointedEvidenceV0 {
        self.authorize(
            log,
            self.tip(log),
            voters,
            request.effect(self.custodian).unwrap(),
            request.operation_id,
        )
    }

    fn frontier(&self, min_seq: u64, pinned: Option<LogCheckpointV0>) -> RelyingFrontierV0 {
        RelyingFrontierV0 {
            context: self.ctx,
            min_seq,
            pinned,
        }
    }

    /// The relying party's out-of-band trust: `DOMAIN` is served by this context, at this
    /// frontier, under the strict rule, rehearsal recognition permitted.
    fn trust(&self, min_seq: u64, pinned: Option<LogCheckpointV0>) -> InstitutionalTrustAnchorV0 {
        anchor(
            DOMAIN,
            self.ctx,
            min_seq,
            pinned,
            RecognitionRequirement::RehearsalPermitted,
        )
    }

    fn parts(&self) -> Vec<GenesisParts> {
        self.members.iter().map(GenASubject::parts).collect()
    }
}

/// The honest lifecycle up to a granted fourth member: genesis (3 founders' members), then a
/// fourth GEN-A Subject granted standing by a 2-of-3 Subject decision.
fn honest_history(inst: &Institution) -> (Vec<SignedLogEntryV0>, GenASubject) {
    let fourth = GenASubject::incept(DOMAIN, 0x04);
    let mut log = vec![inst.genesis_entry()];
    log.push(inst.decided(
        &log,
        TransitionV0::GrantStanding {
            subject: fourth.subject,
            role: ROLE.into(),
        },
        &[inst.m(0), inst.m(1)],
    ));
    (log, fourth)
}

// ---------------------------------------------------------------------------------------------
// Independent verifier: holds bytes and out-of-band trust, never the producer's verdict or keys
// ---------------------------------------------------------------------------------------------

/// HARNESS transport of a GEN-A genesis bundle's public fields. GEN-A v1 has no wire codec, so a
/// cross-process verifier cannot receive recognition evidence in a canonical form today.
#[derive(Clone)]
struct GenesisParts {
    context_id: String,
    salt: [u8; 32],
    inception_body: Vec<u8>,
    inception_witness: [u8; 64],
    authorize_body: Vec<u8>,
    authorize_witness: [u8; 64],
}

impl GenesisParts {
    fn of(g: &SubjectContextGenesisV1) -> Self {
        GenesisParts {
            context_id: g.context_id.clone(),
            salt: *g.context_salt.as_bytes(),
            inception_body: g.inception_body_bytes.clone(),
            inception_witness: *g.inception_witness.as_bytes(),
            authorize_body: g.authorize_body_bytes.clone(),
            authorize_witness: *g.authorize_witness.as_bytes(),
        }
    }

    fn genesis(&self) -> SubjectContextGenesisV1 {
        SubjectContextGenesisV1 {
            version: SUBJECT_CONTEXT_GENESIS_VERSION,
            context_kind: SubjectContextKind::GovernanceDomainV1,
            context_id: self.context_id.clone(),
            context_salt: ContextSalt::from_bytes(self.salt),
            inception_body_bytes: self.inception_body.clone(),
            inception_witness: WitnessSignature::from_bytes(self.inception_witness),
            authorize_body_bytes: self.authorize_body.clone(),
            authorize_witness: WitnessSignature::from_bytes(self.authorize_witness),
        }
    }
}

/// Everything the verifier receives, as bytes.
#[derive(Clone)]
struct EvidencePackage {
    inception: Vec<u8>,
    log: Vec<Vec<u8>>,
    authorization: Vec<u8>,
    vote_bundles: Vec<Vec<u8>>,
    recognition_geneses: Vec<GenesisParts>,
}

impl EvidencePackage {
    fn of(ev: &CheckpointedEvidenceV0, geneses: Vec<GenesisParts>) -> Self {
        EvidencePackage {
            inception: ev.inception.canonical_bytes(),
            log: ev.log.iter().map(SignedLogEntryV0::encode).collect(),
            authorization: ev.authorization.encode(),
            vote_bundles: ev
                .vote_bundles
                .iter()
                .map(EvidenceBundle::canonical_bytes)
                .collect(),
            recognition_geneses: geneses,
        }
    }
}

/// The relying party's anchor, from its own configuration.
fn anchor(
    domain: &str,
    context: ContextRef,
    min_seq: u64,
    pinned: Option<LogCheckpointV0>,
    recognition: RecognitionRequirement,
) -> InstitutionalTrustAnchorV0 {
    InstitutionalTrustAnchorV0::new(
        GovernanceDomainId(domain.into()),
        context,
        min_seq,
        pinned,
        vec![strict()],
        recognition,
    )
    .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
enum IndependentRefusal {
    Decode(&'static str),
    /// The library gate (`verify_institutional_authorization`) refused.
    Institutional(InstitutionalRefusal),
}

/// What the independent verifier concludes. The legitimacy is always `RehearsalOnly`: the only
/// recognition basis that exists is non-production.
#[derive(Debug, PartialEq, Eq)]
struct IndependentVerdict {
    context: ContextRef,
    operation_id: [u8; 32],
    effect: EffectV0,
    checkpoint: LogCheckpointV0,
    voters: usize,
    legitimacy: Legitimacy,
}

/// Decode bytes, then run the library institutional verifier under the relying party's anchor.
fn independent_verify(
    pkg: &EvidencePackage,
    trust: &InstitutionalTrustAnchorV0,
) -> Result<IndependentVerdict, IndependentRefusal> {
    let inception = ContextInceptionV0::decode(&pkg.inception)
        .map_err(|_| IndependentRefusal::Decode("inception"))?;
    let log = pkg
        .log
        .iter()
        .map(|b| SignedLogEntryV0::decode(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| IndependentRefusal::Decode("log"))?;
    let authorization = Signed::<CheckpointedAuthorizationV0>::decode(&pkg.authorization)
        .map_err(|_| IndependentRefusal::Decode("authorization"))?;
    let vote_bundles = pkg
        .vote_bundles
        .iter()
        .map(|b| EvidenceBundle::decode(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| IndependentRefusal::Decode("vote bundle"))?;
    let evidence = InstitutionalEvidenceV0 {
        authorization: CheckpointedEvidenceV0 {
            inception: inception.into(),
            log,
            authorization,
            vote_bundles,
        },
        recognitions: pkg
            .recognition_geneses
            .iter()
            .map(GenesisParts::genesis)
            .collect(),
    };
    // Re-derive everything: log, rule, N4 acts, and GEN-B recognition of the whole electorate for
    // the anchor's domain. Nothing the writer or presenter asserts is taken on its word.
    let verified = verify_institutional_authorization(trust, &evidence)
        .map_err(IndependentRefusal::Institutional)?;
    let auth = verified.verified().authorization();
    Ok(IndependentVerdict {
        context: auth.context,
        operation_id: auth.operation_id,
        effect: auth.effect.clone(),
        checkpoint: auth.checkpoint,
        voters: verified.voters().len(),
        legitimacy: verified.legitimacy(),
    })
}

struct FailingBarrier;

impl DurabilityBarrier for FailingBarrier {
    fn sync(&self, _: &SledStore) -> Result<(), String> {
        Err("injected barrier failure".into())
    }
}

fn available(r: &LocalResourceReservations<impl DurabilityBarrier>, ctx: ContextRef) -> u64 {
    r.inspect_pool(ctx, POOL)
        .unwrap()
        .unwrap()
        .available_capacity
}

// ---------------------------------------------------------------------------------------------
// The lifecycle

fn geneses(subjects: &[&GenASubject]) -> Vec<SubjectContextGenesisV1> {
    subjects.iter().map(|s| s.genesis.clone()).collect()
}

fn members_and(inst: &Institution, extra: &[&GenASubject]) -> Vec<SubjectContextGenesisV1> {
    let mut all: Vec<&GenASubject> = inst.members.iter().collect();
    all.extend_from_slice(extra);
    geneses(&all)
}

type GateResult = Result<
    (
        icn_governance::institutional_admission::VerifiedInstitutionalAuthorization,
        icn_authz_store::ReservationRecordV0,
    ),
    InstitutionalAdmissionError<ReservationError>,
>;

/// Admission through the institutional gate. `reached` records whether the target was invoked.
fn reserve_via_gate(
    target: &LocalResourceReservations<impl DurabilityBarrier>,
    trust: &InstitutionalTrustAnchorV0,
    evidence: &InstitutionalEvidenceV0,
    request: &ReservationRequestV0,
    reached: &std::cell::Cell<bool>,
) -> GateResult {
    admit_institutional(trust, evidence, |ev, frontier, rules| {
        reached.set(true);
        // The target receives the anchor's frontier, whatever the presenter would have preferred.
        assert_eq!(*frontier, trust.frontier());
        target.reserve_checkpointed(ev, frontier, rules, NOW, request)
    })
}

fn fresh_target(inst: &Institution) -> (tempfile::TempDir, LocalResourceReservations) {
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    target.initialize_pool(inst.pool(10)).unwrap();
    (dir, target)
}

/// Nothing consumed, no reservation, capacity unchanged.
fn untouched(target: &LocalResourceReservations, inst: &Institution, op: [u8; 32]) {
    assert_eq!(target.inspect_consumed(inst.ctx, op).unwrap(), None);
    assert!(matches!(
        target.reconcile(inst.ctx, op).unwrap(),
        ReservationReconciliation::Absent
    ));
    assert_eq!(available(target, inst.ctx), 10);
}

// ---------------------------------------------------------------------------------------------
// Mandatory admission at the target (experimental V1 pool + custodian-installed verifier)
// ---------------------------------------------------------------------------------------------

fn registry_for(anchors: &[&InstitutionalTrustAnchorV0]) -> AdmissionRegistryV0 {
    let mut registry = AdmissionRegistryV0::new();
    for anchor in anchors {
        registry
            .install(std::sync::Arc::new(InstitutionalAdmissionVerifierV0::new(
                (*anchor).clone(),
            )))
            .unwrap();
    }
    registry
}

fn institutional_pool(
    inst: &Institution,
    anchor: &InstitutionalTrustAnchorV0,
    min_grade: AdmissionGradeV0,
) -> ResourcePoolV1 {
    ResourcePoolV1 {
        pool: inst.pool(10),
        admission: AdmissionRequirementV1::Policy {
            policy: InstitutionalAdmissionVerifierV0::policy_ref(anchor),
            min_grade,
        },
    }
}

/// The custodian's store: its own verifier installed, and a pool that requires it.
fn policy_target(
    inst: &Institution,
    anchor: &InstitutionalTrustAnchorV0,
    min_grade: AdmissionGradeV0,
) -> (tempfile::TempDir, LocalResourceReservations) {
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian)
        .unwrap()
        .with_admission(registry_for(&[anchor]));
    target
        .initialize_pool_v1(institutional_pool(inst, anchor, min_grade))
        .unwrap();
    (dir, target)
}

fn reserve_admitted(
    target: &LocalResourceReservations,
    evidence: &InstitutionalEvidenceV0,
    request: &ReservationRequestV0,
) -> Result<icn_authz_store::ReservationRecordV0, ReservationError> {
    let supplement = InstitutionalSupplementV0 {
        recognitions: evidence.recognitions.clone(),
    };
    target.reserve_admitted(&evidence.authorization, &supplement, NOW, request)
}

fn valid_admission(inst: &Institution) -> (InstitutionalEvidenceV0, ReservationRequestV0) {
    let (log, fourth) = honest_history(inst);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request),
        recognitions: members_and(inst, &[&fourth]),
    };
    (evidence, request)
}

use icn_governance::institutional_admission::TrustAnchorError;
/// A recognised, authorized electorate admits exactly one reservation through the gate; the
// ---------------------------------------------------------------------------------------------
// P3: anchor derived from the binding
// ---------------------------------------------------------------------------------------------
use icn_governance::institutional_domain::anchor_derivation::{
    AnchorDerivationError, BindingDerivedAnchorV1, ContextInception,
};
use icn_governance::institutional_domain::authority_binding::{
    BindingBasisV1, InstitutionalAuthorityBindingV1,
};

/// Independently derived (Python, from the documented encodings; see the lane's evidence
/// `reference_vectors.py`): `served_domain_commitment(DOMAIN)`.
const REF_SERVED_DOMAIN: &str = "380290060d57e67d63ba4fa21c6a9c6b6b650e527269d9ac08df68f269c697a0";
/// Independently derived: the `ContextRef` of `Institution::new(DOMAIN)`'s V1 inception.
const REF_CONTEXT_V1: &str = "e0fe30b95d403842cde4d33d38f29d2c0f4d784ca09cbf45a15e0f777fe7520b";
const REF_SERVED_NFC_CAFE: &str =
    "42de0d2f22d6b5c43ab7e035943cdb8b2691414c7c0ee76081cfe1c729bfd5f6";
const REF_SERVED_NFD_CAFE: &str =
    "b4243cef34303a3e10909be9a5702cbd05f822b360a3f93104993994e4b8b984";

fn hex32(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
    }
    out
}

fn dom(d: &str) -> GovernanceDomainId {
    GovernanceDomainId(d.to_string())
}

fn basis() -> BindingBasisV1 {
    BindingBasisV1::DeclaredGenesis {
        declaration_decision_hash: [0xd1; 32],
        mandate_id: [0xd2; 32],
    }
}

/// The binding an honest P6 would establish for `inst` and `domain`.
fn binding(inst: &Institution, domain: &str) -> InstitutionalAuthorityBindingV1 {
    InstitutionalAuthorityBindingV1::new(
        dom(domain),
        inst.ctx,
        inst.genesis_entry().body.head(),
        basis(),
    )
    .unwrap()
}

fn v1(inst: &Institution) -> ContextInception {
    ContextInception::V1(inst.inception.clone())
}

fn derive_with(
    b: &InstitutionalAuthorityBindingV1,
    inception: &ContextInception,
    genesis: &SignedLogEntryV0,
    min_seq: u64,
    pinned: Option<LogCheckpointV0>,
) -> Result<BindingDerivedAnchorV1, AnchorDerivationError> {
    BindingDerivedAnchorV1::from_binding(
        b,
        inception,
        genesis,
        min_seq,
        pinned,
        vec![strict()],
        RecognitionRequirement::RehearsalPermitted,
    )
}

fn derive(
    inst: &Institution,
    b: &InstitutionalAuthorityBindingV1,
) -> Result<BindingDerivedAnchorV1, AnchorDerivationError> {
    derive_with(b, &v1(inst), &inst.genesis_entry(), 2, None)
}

/// A genesis for `inst`'s context with a different (valid, founder-signed) initial standing.
fn rival_genesis(inst: &Institution) -> SignedLogEntryV0 {
    let mut standing = vec![(inst.m(0).subject, ROLE.to_string())];
    standing.sort();
    let body = LogEntryV0 {
        context: inst.ctx,
        seq: 1,
        prev: inst.ctx.0,
        transition: TransitionV0::Genesis {
            writer: pk(&inst.writer),
            transition_rule: strict().rule_hash(),
            effect_rule: strict().rule_hash(),
            standing,
        },
    };
    SignedLogEntryV0::genesis(body, &[&inst.founders[0], &inst.founders[1]]).unwrap()
}

#[test]
fn reference_vectors_are_reproduced() {
    let inst = Institution::new(DOMAIN);
    assert_eq!(
        served_domain_commitment(&dom(DOMAIN)).unwrap(),
        hex32(REF_SERVED_DOMAIN)
    );
    assert_eq!(inst.inception.served_subject, hex32(REF_SERVED_DOMAIN));
    assert_eq!(inst.ctx.0, hex32(REF_CONTEXT_V1));
    let derived = derive(&inst, &binding(&inst, DOMAIN)).unwrap();
    assert_eq!(derived.context().0, hex32(REF_CONTEXT_V1));
    assert_eq!(derived.domain(), &dom(DOMAIN));
    assert_eq!(derived.genesis_head(), inst.genesis_entry().body.head());
}

/// The derived anchor is exactly the configured V0 anchor a correct relying party would have
/// written by hand: existing admission semantics are unchanged.
#[test]
fn derived_anchor_equals_the_correct_configured_anchor() {
    let inst = Institution::new(DOMAIN);
    let derived = derive(&inst, &binding(&inst, DOMAIN)).unwrap();
    assert_eq!(derived.anchor(), &inst.trust(2, None));
}

/// End to end: a derived anchor, installed as the custodian's mandatory verifier, admits exactly
/// once through #2938's `reserve_admitted`; a replay is refused.
#[test]
fn derived_anchor_admits_once_through_reserve_admitted() {
    let inst = Institution::new(DOMAIN);
    let derived = derive(&inst, &binding(&inst, DOMAIN)).unwrap();
    let (evidence, request) = valid_admission(&inst);
    let (_dir, target) = policy_target(&inst, derived.anchor(), AdmissionGradeV0::Rehearsal);
    let record = reserve_admitted(&target, &evidence, &request).unwrap();
    assert_eq!((record.available_before, record.available_after), (10, 6));
    assert_eq!(
        reserve_admitted(&target, &evidence, &request),
        Err(ReservationError::Authorization(AdmissionRefusal::Replayed))
    );
}

/// A binding naming another context for the same domain (a forged or replaced raw-store record:
/// P2's pinned limit) is refused against the real inception.
#[test]
fn forged_binding_with_another_context_is_refused() {
    let inst = Institution::new(DOMAIN);
    let rogue_founders = [key(0xE1), key(0xE2), key(0xE3)];
    let rogue_inception = ContextInceptionV1::new(
        rogue_founders.iter().map(pk).collect(),
        2,
        [0x1b; 32],
        served_domain_commitment(&dom(DOMAIN)).unwrap(),
    )
    .unwrap();
    let forged = InstitutionalAuthorityBindingV1::new(
        dom(DOMAIN),
        rogue_inception.context_ref(),
        inst.genesis_entry().body.head(),
        basis(),
    )
    .unwrap();
    assert_eq!(
        derive(&inst, &forged),
        Err(AnchorDerivationError::ContextMismatch)
    );
    // An arbitrary context ref is refused the same way.
    let arbitrary = InstitutionalAuthorityBindingV1::new(
        dom(DOMAIN),
        ContextRef([0x55; 32]),
        inst.genesis_entry().body.head(),
        basis(),
    )
    .unwrap();
    assert_eq!(
        derive(&inst, &arbitrary),
        Err(AnchorDerivationError::ContextMismatch)
    );
}

/// Cross-domain substitution: a binding of FOREIGN_DOMAIN to the context founded for DOMAIN.
#[test]
fn cross_domain_binding_substitution_is_refused() {
    let inst = Institution::new(DOMAIN);
    assert_eq!(
        derive(&inst, &binding(&inst, FOREIGN_DOMAIN)),
        Err(AnchorDerivationError::ServedSubjectMismatch)
    );
    // And the converse: the context founded for FOREIGN_DOMAIN bound to DOMAIN.
    let foreign = Institution::new(FOREIGN_DOMAIN);
    assert_eq!(
        derive(&foreign, &binding(&foreign, DOMAIN)),
        Err(AnchorDerivationError::ServedSubjectMismatch)
    );
}

/// An inception whose served commitment is not the binding's, presented with a binding whose
/// context is that inception's: the served-subject check, not the context check, refuses it.
#[test]
fn invalid_inception_commitment_is_refused() {
    let foreign = Institution::new(FOREIGN_DOMAIN);
    let mislabeled = InstitutionalAuthorityBindingV1::new(
        dom(DOMAIN),
        foreign.ctx,
        foreign.genesis_entry().body.head(),
        basis(),
    )
    .unwrap();
    assert_eq!(
        derive(&foreign, &mislabeled),
        Err(AnchorDerivationError::ServedSubjectMismatch)
    );
    // A structurally invalid V1 inception (built by struct literal) is refused before anything.
    let inst = Institution::new(DOMAIN);
    let mut broken = inst.inception.clone();
    broken.threshold = 0;
    assert!(matches!(
        derive_with(
            &binding(&inst, DOMAIN),
            &ContextInception::V1(broken),
            &inst.genesis_entry(),
            2,
            None
        ),
        Err(AnchorDerivationError::InvalidInception(_))
    ));
}

/// V0/V1 confusion: a V0 inception never backs a binding, even one with the same founders.
#[test]
fn v0_inception_cannot_back_a_binding() {
    let inst = Institution::new(DOMAIN);
    let v0 = ContextInceptionV0::new(
        inst.founders.iter().map(pk).collect(),
        2,
        harness_context_label(DOMAIN),
    )
    .unwrap();
    let v0_binding =
        InstitutionalAuthorityBindingV1::new(dom(DOMAIN), v0.context_ref(), [0x77; 32], basis())
            .unwrap();
    for b in [&v0_binding, &binding(&inst, DOMAIN)] {
        assert_eq!(
            derive_with(
                b,
                &ContextInception::V0(v0.clone()),
                &inst.genesis_entry(),
                2,
                None
            ),
            Err(AnchorDerivationError::V0Inception)
        );
    }
    // A V0 and a V1 inception with identical founders and threshold are distinct contexts.
    assert_ne!(v0.context_ref(), inst.ctx);
}

/// Missing founder evidence: below threshold, a non-founder, no founder at all, a genesis of
/// another context, and a non-genesis entry are all refused by the CAL's own genesis rules.
#[test]
fn missing_or_invalid_founder_evidence_is_refused() {
    let inst = Institution::new(DOMAIN);
    let b = binding(&inst, DOMAIN);
    let body = inst.genesis_entry().body;

    let below = SignedLogEntryV0::genesis(body.clone(), &[&inst.founders[0]]).unwrap();
    let outsider =
        SignedLogEntryV0::genesis(body.clone(), &[&inst.founders[0], &key(0x99)]).unwrap();
    let unsigned = SignedLogEntryV0::genesis(body.clone(), &[]).unwrap();
    for genesis in [&below, &outsider, &unsigned] {
        assert!(
            matches!(
                derive_with(&b, &v1(&inst), genesis, 2, None),
                Err(AnchorDerivationError::InvalidGenesis(_))
            ),
            "founder evidence below quorum must be refused"
        );
    }

    let foreign = Institution::new(FOREIGN_DOMAIN);
    assert!(matches!(
        derive_with(&b, &v1(&inst), &foreign.genesis_entry(), 2, None),
        Err(AnchorDerivationError::InvalidGenesis(
            LogRefusal::ContextMismatch { .. }
        ))
    ));

    let (log, _) = honest_history(&inst);
    assert!(matches!(
        derive_with(&b, &v1(&inst), &log[1], 2, None),
        Err(AnchorDerivationError::InvalidGenesis(_))
    ));
}

/// A valid, founder-signed genesis that is not the one the binding names (a rival genesis for the
/// same context), and a binding naming a head no genesis has, are refused.
#[test]
fn genesis_not_named_by_the_binding_is_refused() {
    let inst = Institution::new(DOMAIN);
    assert_eq!(
        derive_with(
            &binding(&inst, DOMAIN),
            &v1(&inst),
            &rival_genesis(&inst),
            2,
            None
        ),
        Err(AnchorDerivationError::GenesisHeadMismatch)
    );
    let wrong_head =
        InstitutionalAuthorityBindingV1::new(dom(DOMAIN), inst.ctx, [0x42; 32], basis()).unwrap();
    assert_eq!(
        derive(&inst, &wrong_head),
        Err(AnchorDerivationError::GenesisHeadMismatch)
    );
}

#[test]
fn frontier_must_start_at_genesis_and_a_genesis_pin_must_match() {
    let inst = Institution::new(DOMAIN);
    let b = binding(&inst, DOMAIN);
    let g = inst.genesis_entry();
    assert_eq!(
        derive_with(&b, &v1(&inst), &g, 0, None),
        Err(AnchorDerivationError::ZeroFrontier)
    );
    let bad_pin = LogCheckpointV0 {
        seq: 1,
        head: [0x42; 32],
    };
    assert_eq!(
        derive_with(&b, &v1(&inst), &g, 1, Some(bad_pin)),
        Err(AnchorDerivationError::PinnedGenesisMismatch)
    );
    let good_pin = LogCheckpointV0 {
        seq: 1,
        head: g.body.head(),
    };
    assert!(derive_with(&b, &v1(&inst), &g, 1, Some(good_pin)).is_ok());
    // A later pin is the relying party's own retained knowledge and passes through unchanged.
    let (log, _) = honest_history(&inst);
    let later = inst.tip(&log);
    let derived = derive_with(&b, &v1(&inst), &g, 2, Some(later)).unwrap();
    assert_eq!(derived.anchor(), &inst.trust(2, Some(later)));
}

/// The configured anchor's own checks still apply to a derived one.
#[test]
fn anchor_checks_still_apply() {
    let inst = Institution::new(DOMAIN);
    let r = BindingDerivedAnchorV1::from_binding(
        &binding(&inst, DOMAIN),
        &v1(&inst),
        &inst.genesis_entry(),
        2,
        None,
        vec![],
        RecognitionRequirement::RehearsalPermitted,
    );
    assert_eq!(
        r,
        Err(AnchorDerivationError::Anchor(TrustAnchorError::NoRules))
    );
    let r = BindingDerivedAnchorV1::from_binding(
        &binding(&inst, DOMAIN),
        &v1(&inst),
        &inst.genesis_entry(),
        2,
        None,
        vec![strict(), strict()],
        RecognitionRequirement::RehearsalPermitted,
    );
    assert_eq!(
        r,
        Err(AnchorDerivationError::Anchor(
            TrustAnchorError::DuplicateRule
        ))
    );
}

/// Exact-byte identity: NFC and NFD spellings are different domains; a context founded for one
/// cannot back a binding for the other.
#[test]
fn domain_identity_is_exact_bytes() {
    let nfc = "caf\u{e9}";
    let nfd = "cafe\u{301}";
    assert_eq!(
        served_domain_commitment(&dom(nfc)).unwrap(),
        hex32(REF_SERVED_NFC_CAFE)
    );
    assert_eq!(
        served_domain_commitment(&dom(nfd)).unwrap(),
        hex32(REF_SERVED_NFD_CAFE)
    );
    let inst = Institution::new(nfc);
    assert!(derive(&inst, &binding(&inst, nfc)).is_ok());
    assert_eq!(
        derive(&inst, &binding(&inst, nfd)),
        Err(AnchorDerivationError::ServedSubjectMismatch)
    );
}

/// Production recognition is still refused through a derived anchor: derivation is not a
/// legitimacy upgrade.
#[test]
fn derivation_does_not_upgrade_legitimacy() {
    let inst = Institution::new(DOMAIN);
    let derived = BindingDerivedAnchorV1::from_binding(
        &binding(&inst, DOMAIN),
        &v1(&inst),
        &inst.genesis_entry(),
        2,
        None,
        vec![strict()],
        RecognitionRequirement::ProductionGrade,
    )
    .unwrap();
    let (evidence, _) = valid_admission(&inst);
    assert!(matches!(
        verify_institutional_authorization(derived.anchor(), &evidence),
        Err(InstitutionalRefusal::RehearsalNotProduction { .. })
    ));
    let rehearsal = derive(&inst, &binding(&inst, DOMAIN)).unwrap();
    let verified = verify_institutional_authorization(rehearsal.anchor(), &evidence).unwrap();
    assert_eq!(verified.legitimacy(), Legitimacy::RehearsalOnly);
}

/// Bit-flip robustness of the stored record: every single-bit corruption of an honest binding's
/// encoding either fails to decode or yields a binding from which no anchor derives.
#[test]
fn corrupted_binding_bytes_never_derive_an_anchor() {
    let inst = Institution::new(DOMAIN);
    let honest = binding(&inst, DOMAIN);
    let bytes = honest.encode();
    let (g, inception) = (inst.genesis_entry(), v1(&inst));
    let mut decoded_but_refused = 0usize;
    for i in 0..bytes.len() * 8 {
        let mut c = bytes.clone();
        c[i / 8] ^= 1 << (i % 8);
        let Ok(b) = InstitutionalAuthorityBindingV1::decode(&c) else {
            continue;
        };
        // Basis hashes are not verified here (P6); a flip there leaves the D/C/h1 triple intact.
        if b.domain() == honest.domain()
            && b.context() == honest.context()
            && b.genesis_head() == honest.genesis_head()
        {
            continue;
        }
        decoded_but_refused += 1;
        assert!(
            derive_with(&b, &inception, &g, 2, None).is_err(),
            "bit {i}: a corrupted binding derived an anchor"
        );
    }
    assert!(
        decoded_but_refused > 0,
        "the property must not pass vacuously"
    );
}
