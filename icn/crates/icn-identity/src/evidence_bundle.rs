//! N4-A portable evidence bundle — transport framing over canonical N1 facts plus one signed
//! device act.
//!
//! This module answers exactly one question:
//!
//! > How does a holder of N1 facts hand a **stateless** verifier exactly the evidence the
//! > existing N1 + N4-A semantics need to evaluate one [`SignedDeviceAct`], such that the verifier
//! > reconstructs the same [`AuthorityStore`] and reaches the same verdict without `icnd`?
//!
//! It is the container named as "the one missing boundary to build next" in
//! `docs/architecture/N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.2 item 2. Its contract is
//! `docs/architecture/N4A_PORTABLE_EVIDENCE_BUNDLE.md`.
//!
//! # The invariant: the container transports evidence, it does not interpret authority
//!
//! Nothing in this module decides which body wins, whether a Subject is halted, whether a prefix
//! is complete, or whether a device is authorized. Those remain, exclusively, the job of N1
//! admission ([`crate::authority_log::admissible`]) and the N1 fold ([`crate::authority_log::derive_prefix`]) as consumed by N4-A
//! ([`crate::device_authority::verify_device_act`]). The intended flow is, and may only be:
//!
//! ```text
//! portable bytes
//!     ↓ EvidenceBundle::decode          -- strict, fail-closed, structural
//! N1 canonical bodies + witnesses
//!     ↓ EvidenceBundle::admit           -- every pair through AuthorityStore::ingest
//! AuthorityStore
//!     ↓ verify_device_act(.., E)        -- the existing N4-A verifier, derive_prefix inside
//! DeviceAuthorityEvidence | refusal
//! ```
//!
//! [`AuthorityStore`] has no ingest path other than [`AuthorityStore::ingest`], which calls
//! [`crate::authority_log::admissible`]. Decoding a bundle therefore **cannot** cause a fact to bypass canonical
//! admission: there is no constructor to bypass it with.
//!
//! # What a bundle carries, and why exactly that
//!
//! A bundle is scoped to one Subject `S` and one evaluation position `E`, both of which the
//! enclosed act names. It carries **every retained N1 body for `S` at position `≤ E`**, together
//! with **every retained witness over each such body**, and nothing else. This is not a heuristic:
//! `derive_prefix(S, store, E)` is defined as the N1 fold over precisely that body set, so a
//! verifier that admits exactly these facts folds exactly what the bundler would have folded.
//!
//! - Bodies above `E` are provably irrelevant to evaluation at `E` (the fold never sees them) and
//!   are **refused** by the decoder, so a later revocation cannot be smuggled into a historical
//!   proof — and does not need to be.
//! - Bodies at or below `E` that the fold would never *select* (unauthorized spam, orphaned
//!   branches) are **carried**, because deciding they are unselectable would require running the
//!   fold, which is interpretation.
//! - Both candidates of a fork at or below `E` are therefore carried, and the fork survives
//!   transport as a fork. A gap survives as a gap: there is nothing to carry at the missing
//!   position, and the verifier reports `PrefixIncomplete`.
//! - An empty fact list is legal. It transports "the bundler holds no inception for `S`", which the
//!   verifier refuses as `SubjectUnknown`. The decoder does not normalize that into an error of
//!   its own.
//!
//! # Canonical bytes
//!
//! All framing is N1's, reused: `LP(x) := u32be(len(x)) || x`, `b32`/`b64` are raw bytes,
//! integers are big-endian and fixed width.
//!
//! ```text
//! evidence_bundle_v1 :=
//!       LP(BUNDLE_DOMAIN)
//!    || u16be(BUNDLE_VERSION)
//!    || b32(subject)                        -- S; must equal act.subject
//!    || u64be(evaluation_position)          -- E; must equal act.evaluation_position
//!    || u32be(fact_count)
//!    || fact * fact_count                   -- strictly ascending by event_id
//!    || LP(device_act_v1)                   -- the act's canonical bytes, verbatim (N4-A §6)
//!    || b64(act_signature)
//!
//! fact :=
//!       b32(event_id)                       -- SHA-256(canonical_body), stated, not inferred
//!    || LP(canonical_body)                  -- the N1 canonical body bytes, verbatim
//!    || u32be(witness_count)                -- ≥ 1
//!    || b64(signature) * witness_count      -- strictly ascending bytewise
//!
//! bundle_id := SHA-256(evidence_bundle_v1)
//! ```
//!
//! The fact layout is the N1-D durable record (`event_id || signature → canonical_body`) grouped
//! by body: one body appears once, with all of its witnesses. The `event_id` is carried explicitly
//! so that a substituted or corrupted body is caught as a **disagreement** between the stated
//! digest and the carried bytes, exactly as a disagreeing key and value are caught on rehydration.
//!
//! **Ordering is by digest, and digests carry no authority meaning.** Facts are ordered by
//! `event_id` and witnesses by signature bytes because those are the only canonical identifiers
//! the durable layer has. Neither order is a chain order, an arrival order or a preference; the
//! decoder rebuilds set-valued state and the fold orders bodies by position itself.
//!
//! # Bounds
//!
//! No new limit is introduced. Every count and length is bounded by the input itself through N1's
//! `Reader` (`count` refuses a declared element count the remaining bytes cannot hold; `lp`
//! refuses a declared length that overruns). The act payload is bounded by N4-A's
//! `MAX_DEVICE_ACT_PAYLOAD`, and `E` by N1's [`MAX_POSITION`]. Memory consumed is therefore
//! proportional to bytes the caller chose to read.
//!
//! # What this module is not
//!
//! Not a network protocol, not N3 replication, not a persistence format, not a session format, not
//! an enrollment or recovery protocol, not a Home or deployment format, and not a second authority
//! model. No clock, no randomness, no "latest", no repair-on-read, no branch selection.
//!
//! # Convergence
//!
//! This is the **one** canonical N4-B container. A second implementation of the same boundary
//! (`DeviceAuthorityBundleV1`, domain `icn.n4.device-authority-bundle`, flat per-witness records,
//! an invented `MAX_BUNDLE_FACTS`) existed on this branch and was removed in favour of this one on
//! 2026-10-04; `N4B_PORTABLE_EVIDENCE_BUNDLE.md` §11 records why. What survived from it is below
//! the decoder: [`EvidenceBundle::summarize`] for inspection and [`verify_evidence_bundle`], the
//! thin composition `admit → verify_device_act` for a relying party that states its own `(S, E)`.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::authority_log::encoding::{Reader, Writer};
use crate::authority_log::{
    sha256, AdmissionError, AuthorityBody, AuthorityStore, CodecError, EventId,
    SignedAuthorityEvent, SubjectId, WitnessSignature, MAX_POSITION,
};
use crate::device_authority::{
    verify_device_act, DeviceActError, DeviceActSignature, DeviceActV1, DeviceActVerifyError,
    DeviceAuthorityEvidence, SignedDeviceAct,
};

/// Domain separator that opens every canonical bundle. Length-prefixed in the encoding.
///
/// Pairwise distinct from every N1 separator (`icn.authority-log*`), the N4-A act separator
/// (`icn.n4.device-act`) and every GEN separator, so bundle bytes can never be presented as, or
/// mistaken for, any of those.
pub const BUNDLE_DOMAIN: &[u8] = b"icn.n4.evidence-bundle";

/// Canonical bundle version. Decoding rejects any other value.
pub const BUNDLE_VERSION: u16 = 1;

/// Why a bundle could not be assembled, decoded or admitted. Every variant is a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BundleError {
    /// The container framing did not parse: wrong domain, unsupported version, truncation,
    /// trailing bytes, or a declared count the input cannot hold.
    #[error("bundle framing rejected: {0}")]
    Framing(CodecError),
    /// `E` is `0` or above [`MAX_POSITION`].
    #[error("evaluation position {0} is outside 1..=MAX_POSITION")]
    PositionOutOfRange(u64),
    /// Facts were not in strictly ascending `event_id` order, or one `event_id` appeared twice.
    ///
    /// Strictness is what makes the encoding canonical: accepting a reordered or duplicated fact
    /// would give one logical bundle more than one byte identity.
    #[error("facts are not strictly ascending by event_id (duplicate or reordered fact)")]
    FactOrder,
    /// A fact carried no witness. A body with no signature over it is not a durable fact.
    #[error("fact {event_id} carries no witness")]
    NoWitness {
        /// The fact's stated digest.
        event_id: EventId,
    },
    /// Witnesses were not in strictly ascending byte order, or one appeared twice.
    #[error("witnesses of fact {event_id} are not strictly ascending (duplicate or reordered)")]
    WitnessOrder {
        /// The fact's stated digest.
        event_id: EventId,
    },
    /// The carried body bytes are not a strict canonical N1 body.
    #[error("fact {event_id} carries a malformed N1 body: {source}")]
    Body {
        /// The fact's stated digest.
        event_id: EventId,
        /// N1's own decode error.
        #[source]
        source: CodecError,
    },
    /// The stated `event_id` is not the digest of the carried body.
    ///
    /// This is the substitution / corruption check: the body decoded, but it is not the body the
    /// fact claims to be, so the witnesses cannot be taken to be over it.
    #[error("fact states event_id {stated} but its body hashes to {computed}")]
    EventIdMismatch {
        /// The digest the fact declared.
        stated: EventId,
        /// The digest of the body actually carried.
        computed: EventId,
    },
    /// A carried body belongs to a Subject other than the one the bundle is scoped to.
    #[error("fact {event_id} belongs to another subject")]
    FactSubjectMismatch {
        /// The fact's stated digest.
        event_id: EventId,
    },
    /// A carried body sits above the bundle's evaluation position. Facts after `E` are never
    /// required to prove authority at `E`, so a bundle that carries one is not a prefix bundle.
    #[error("fact {event_id} at position {position} lies outside the prefix through {through}")]
    FactOutsideScope {
        /// The fact's stated digest.
        event_id: EventId,
        /// The body's position.
        position: u64,
        /// The bundle's evaluation position.
        through: u64,
    },
    /// The enclosed act bytes are not a strict canonical device act.
    #[error("enclosed device act is malformed: {0}")]
    Act(#[from] DeviceActError),
    /// The act names a Subject other than the bundle's.
    #[error("enclosed act names a different subject than the bundle")]
    ActSubjectMismatch,
    /// The act binds an evaluation position other than the bundle's.
    #[error("enclosed act binds evaluation position {act}, bundle is scoped to {bundle}")]
    ActPositionMismatch {
        /// The position bound in the act.
        act: u64,
        /// The position the bundle header declares.
        bundle: u64,
    },
    /// A `(body, witness)` pair was refused by canonical N1 admission.
    ///
    /// Raised by [`EvidenceBundle::admit`], never by [`EvidenceBundle::decode`]: a bundle can be
    /// structurally canonical and still carry a witness that does not verify. Admission is N1's
    /// decision, reported verbatim.
    #[error("fact {event_id} was refused by N1 admission: {source}")]
    Inadmissible {
        /// The fact's stated digest.
        event_id: EventId,
        /// N1's own admission error.
        #[source]
        source: AdmissionError,
    },
    /// The store offered for assembly holds a body for which it holds no witness.
    ///
    /// Unreachable for a store filled through [`AuthorityStore::ingest`], which always records a
    /// witness alongside a body; reported rather than silently skipped if it ever happens.
    #[error("store holds body {event_id} with no witness over it")]
    StoreLacksWitness {
        /// The body's digest.
        event_id: EventId,
    },
}

impl From<CodecError> for BundleError {
    fn from(error: CodecError) -> Self {
        BundleError::Framing(error)
    }
}

/// One N1 body with every witness carried over it.
///
/// The canonical body bytes are kept **verbatim** alongside the decoded body: what is re-encoded
/// for the wire is what was received, and the decoded body exists only so admission and the fold
/// can use typed values. `AuthorityBody::canonical_bytes()` is a bijection on well-formed bodies,
/// so the two never disagree once decode has succeeded; the invariant is nonetheless checked
/// rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledFact {
    /// The decoded N1 body.
    pub body: AuthorityBody,
    /// Every signature carried over `body`, in canonical byte order. Never empty.
    pub witnesses: BTreeSet<WitnessSignature>,
}

/// The portable evidence container: canonical N1 facts for one Subject through one position, plus
/// one signed device act naming that Subject and position.
///
/// Construct through [`EvidenceBundle::assemble`] (from a store) or [`EvidenceBundle::decode`]
/// (from bytes). Both enforce the scope rule. Neither interprets authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBundle {
    subject: SubjectId,
    evaluation_position: u64,
    facts: BTreeMap<EventId, BundledFact>,
    act: SignedDeviceAct,
}

impl EvidenceBundle {
    /// Assemble the bundle for `signed` from the facts `store` retains.
    ///
    /// The Subject and evaluation position are read from the act — a bundle for any other
    /// `(S, E)` would be refused by the verifier at step 1 or 2 of N4-A §7 and is therefore not a
    /// bundle anyone can use. The facts selected are exactly `{ b ∈ Bodies_store(S) :
    /// position(b) ≤ E }` with every witness the store holds over each; see the module
    /// documentation for why that set and no other.
    ///
    /// This performs **no** derivation. It does not check that the prefix is complete, live or
    /// unforked, and it does not check that the act verifies. A bundle that transports a gap, a
    /// fork or an unauthorized device is a correct bundle; the verifier is where those are refused.
    pub fn assemble(store: &AuthorityStore, signed: &SignedDeviceAct) -> Result<Self, BundleError> {
        let subject = signed.act.subject;
        let through = signed.act.evaluation_position;
        check_position(through)?;

        let mut facts = BTreeMap::new();
        for body in store.bodies_for(subject) {
            if body.position() > through {
                continue;
            }
            let event_id = body.event_id();
            let witnesses: BTreeSet<WitnessSignature> = store
                .witnesses_for(event_id)
                .into_iter()
                .map(|witness| witness.signature)
                .collect();
            if witnesses.is_empty() {
                return Err(BundleError::StoreLacksWitness { event_id });
            }
            facts.insert(event_id, BundledFact { body, witnesses });
        }

        Ok(EvidenceBundle {
            subject,
            evaluation_position: through,
            facts,
            act: signed.clone(),
        })
    }

    /// The Subject this bundle is evidence for.
    pub fn subject(&self) -> SubjectId {
        self.subject
    }

    /// The evaluation position this bundle's facts reach through.
    pub fn evaluation_position(&self) -> u64 {
        self.evaluation_position
    }

    /// The carried facts, keyed and ordered by `event_id`.
    pub fn facts(&self) -> &BTreeMap<EventId, BundledFact> {
        &self.facts
    }

    /// The enclosed signed act.
    pub fn act(&self) -> &SignedDeviceAct {
        &self.act
    }

    /// The canonical byte encoding — deterministic for a given set of facts and act regardless of
    /// how the store was filled, iterated or joined.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(BUNDLE_DOMAIN);
        w.u16(BUNDLE_VERSION);
        w.b32(self.subject.as_bytes());
        w.u64(self.evaluation_position);
        // A bundle is bounded by the bytes that produced it or the store that produced it; a
        // `u32` count matches every other collection in the N1 framing.
        w.u32(self.facts.len() as u32);
        for (event_id, fact) in &self.facts {
            w.b32(event_id.as_bytes());
            w.lp(&fact.body.canonical_bytes());
            w.u32(fact.witnesses.len() as u32);
            for witness in &fact.witnesses {
                w.b64(witness.as_bytes());
            }
        }
        w.lp(&self.act.act.canonical_bytes());
        w.b64(self.act.signature.as_bytes());
        w.finish()
    }

    /// `SHA-256(canonical_bytes)`. A content identity for the whole container: any change to any
    /// fact, witness, scope field or act byte changes it.
    pub fn bundle_id(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }

    /// Strict decode. Fail-closed on every structural defect; see [`BundleError`].
    ///
    /// **What success proves:** the bytes are one canonical bundle; every carried body is a strict
    /// canonical N1 body whose stated `event_id` is its digest, belongs to the bundle's Subject
    /// and sits at or below the bundle's position; every body has at least one witness; the act
    /// is a strict canonical device act naming the bundle's Subject and position.
    ///
    /// **What it does not prove:** that any witness verifies (that is [`EvidenceBundle::admit`]),
    /// that the prefix is complete or unforked, or that the device is authorized (that is
    /// [`crate::device_authority::verify_device_act`]). A decoded bundle is evidence, not a verdict.
    pub fn decode(bytes: &[u8]) -> Result<Self, BundleError> {
        let mut r = Reader::new(bytes);
        r.expect_lp(BUNDLE_DOMAIN, "domain")?;
        let version = r.u16("version")?;
        if version != BUNDLE_VERSION {
            return Err(CodecError::UnsupportedVersion(version).into());
        }
        let subject = SubjectId::from_bytes(r.b32("subject")?);
        let through = r.u64("evaluation_position")?;
        check_position(through)?;

        // Minimum wire size of one fact: event_id (32) + body length prefix (4) + witness count
        // (4). A body and a witness are themselves non-empty, but the count check only needs a
        // lower bound to refuse an impossible count before any allocation.
        let fact_count = r.count(32 + 4 + 4, "fact_count")?;
        let mut facts = BTreeMap::new();
        let mut previous_event_id: Option<EventId> = None;
        for _ in 0..fact_count {
            let stated = EventId::from_bytes(r.b32("event_id")?);
            if let Some(prev) = previous_event_id {
                if stated <= prev {
                    return Err(BundleError::FactOrder);
                }
            }
            previous_event_id = Some(stated);

            let body_bytes = r.lp("canonical_body")?;
            let body = AuthorityBody::decode(body_bytes).map_err(|source| BundleError::Body {
                event_id: stated,
                source,
            })?;
            let computed = body.event_id();
            if computed != stated {
                return Err(BundleError::EventIdMismatch { stated, computed });
            }
            if body.subject() != subject {
                return Err(BundleError::FactSubjectMismatch { event_id: stated });
            }
            let position = body.position();
            if position > through {
                return Err(BundleError::FactOutsideScope {
                    event_id: stated,
                    position,
                    through,
                });
            }

            let witness_count = r.count(64, "witness_count")?;
            if witness_count == 0 {
                return Err(BundleError::NoWitness { event_id: stated });
            }
            let mut witnesses = BTreeSet::new();
            let mut previous_witness: Option<[u8; 64]> = None;
            for _ in 0..witness_count {
                let signature = r.b64("witness_signature")?;
                if let Some(prev) = previous_witness {
                    if signature <= prev {
                        return Err(BundleError::WitnessOrder { event_id: stated });
                    }
                }
                previous_witness = Some(signature);
                witnesses.insert(WitnessSignature::from_bytes(signature));
            }

            facts.insert(stated, BundledFact { body, witnesses });
        }

        let act_bytes = r.lp("device_act")?;
        let act = DeviceActV1::decode(act_bytes)?;
        let signature = DeviceActSignature::from_bytes(r.b64("act_signature")?);
        r.finish()?;

        if act.subject != subject {
            return Err(BundleError::ActSubjectMismatch);
        }
        if act.evaluation_position != through {
            return Err(BundleError::ActPositionMismatch {
                act: act.evaluation_position,
                bundle: through,
            });
        }

        Ok(EvidenceBundle {
            subject,
            evaluation_position: through,
            facts,
            act: SignedDeviceAct { act, signature },
        })
    }

    /// Re-admit every carried `(body, witness)` pair through canonical N1 admission and return the
    /// resulting store.
    ///
    /// There is no other way to turn a bundle into an [`AuthorityStore`]: every pair goes through
    /// [`AuthorityStore::ingest`], which is the same free function of body and signature that gates
    /// a live event. Fails closed on the first refusal — a partially admitted store is a different
    /// Subject history than the one that was bundled, and returning one would let a single bad
    /// witness silently change a derived authority state.
    ///
    /// The returned store is the input to [`crate::device_authority::verify_device_act`] with this bundle's
    /// [`EvidenceBundle::act`], [`EvidenceBundle::subject`] and
    /// [`EvidenceBundle::evaluation_position`]. That call, and only that call, decides authority.
    pub fn admit(&self) -> Result<AuthorityStore, BundleError> {
        let mut store = AuthorityStore::new();
        for (event_id, fact) in &self.facts {
            for witness in &fact.witnesses {
                let event = SignedAuthorityEvent::new(fact.body.clone(), *witness);
                store
                    .ingest(&event)
                    .map_err(|source| BundleError::Inadmissible {
                        event_id: *event_id,
                        source,
                    })?;
            }
        }
        Ok(store)
    }
}

fn check_position(position: u64) -> Result<(), BundleError> {
    if position == 0 || position > MAX_POSITION {
        return Err(BundleError::PositionOutOfRange(position));
    }
    Ok(())
}

// =============================================================================================
// Inspection and the thin verification composition.
//
// Ported from the superseded `DeviceAuthorityBundleV1` (N4-B convergence, 2026-10-04). Neither
// item adds semantics: `summarize` describes a decoded container and decides nothing;
// `verify_evidence_bundle` is `admit` followed by the existing N4-A verifier with the relying
// party's own Subject and position, and makes no decision of its own.
// =============================================================================================

/// What one carried fact is, for inspection. Decides nothing: a description of the decoded
/// container, produced without admission and without derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFactSummary {
    /// Zero-based index in canonical (ascending `event_id`) order.
    pub index: u32,
    /// The N1 body kind tag (`0x01` inception … `0x05` recover).
    pub kind_tag: u8,
    /// The Subject the body names.
    pub subject: SubjectId,
    /// The body's log position.
    pub position: u64,
    /// The body's digest, as stated in the container and checked at decode.
    pub event_id: EventId,
    /// How many witnesses the container carries over this body. Never zero after decode.
    pub witness_count: u32,
}

impl EvidenceBundle {
    /// Describe the carried facts in canonical order. Admits nothing, evaluates nothing: a
    /// bundle whose act would be refused summarizes exactly as one whose act would be accepted.
    pub fn summarize(&self) -> Vec<BundleFactSummary> {
        self.facts
            .iter()
            .enumerate()
            .map(|(index, (event_id, fact))| BundleFactSummary {
                // Bounded by the input through `Reader::count`, which fits a u32.
                index: index as u32,
                kind_tag: fact.body.kind_tag(),
                subject: fact.body.subject(),
                position: fact.body.position(),
                event_id: *event_id,
                witness_count: fact.witnesses.len() as u32,
            })
            .collect()
    }
}

/// Why a bundle, taken as a whole, did not yield device-authority evidence.
///
/// Two sources, kept distinct because they prove different things (§2 of the contract): the
/// container was refused at decode or a carried pair was refused by N1 admission, or every fact
/// was admitted and the existing N4-A verifier refused the act.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EvidenceVerifyError {
    /// The bytes did not decode, or a `(body, witness)` pair was refused by N1 admission.
    #[error("{0}")]
    Bundle(#[from] BundleError),
    /// Every fact was admitted and the N4-A verifier refused the act — the N4-A verdict.
    #[error("{0}")]
    Act(#[from] DeviceActVerifyError),
}

/// [`EvidenceBundle::admit`] followed by the existing N4-A verifier, for a relying party that
/// states its own `subject` and `position`.
///
/// Composition only. `subject` and `position` come from the caller, never from the bytes: a
/// relying party whose Subject or position disagrees with the act is refused by N4-A itself
/// (`ActSubjectMismatch`, `EvaluationPositionMismatch`), and this function adds no check, no
/// selection and no fallback of its own. The header's `(S, E)` was already required to agree
/// with the act at decode; what is decided here is decided by `verify_device_act` alone.
pub fn verify_evidence_bundle(
    bundle: &EvidenceBundle,
    subject: SubjectId,
    position: u64,
) -> Result<DeviceAuthorityEvidence, EvidenceVerifyError> {
    let store = bundle.admit()?;
    verify_device_act(bundle.act(), subject, &store, position).map_err(EvidenceVerifyError::Act)
}

/// [`EvidenceBundle::decode`] then [`verify_evidence_bundle`], for callers holding only bytes.
pub fn verify_evidence_bundle_bytes(
    bytes: &[u8],
    subject: SubjectId,
    position: u64,
) -> Result<DeviceAuthorityEvidence, EvidenceVerifyError> {
    let bundle = EvidenceBundle::decode(bytes)?;
    verify_evidence_bundle(&bundle, subject, position)
}
