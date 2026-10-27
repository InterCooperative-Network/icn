//! N1 history segment — a canonical, facts-only excerpt of one Subject's authority log over a
//! contiguous position range (ADR-0089 §3).
//!
//! This module answers exactly one question:
//!
//! > Are these bytes one canonical, structurally valid excerpt of one Subject's N1 facts over
//! > `from..=to`, and does every carried `(body, witness)` pair pass N1 admission?
//!
//! It is the first implementation slice of ADR-0089 (accepted 2026-10-27, fixture-only). It is a
//! pure data type: no store, no filesystem, no network, no daemon or CLI involvement, and no
//! authorization effect. Import, retention, the import-incomplete marker and offline registry
//! access are later slices and are deliberately absent here.
//!
//! # The invariant: the segment transports evidence, it does not interpret authority
//!
//! Nothing here decides which body wins, whether a Subject is halted, or whether any key had
//! authority. A segment may carry facts that the N1 fold would never select — an `authorize`
//! signed by a key that held no authority, or both candidates of a fork — and they are carried,
//! because deciding they are unselectable would require running the fold, which is interpretation.
//! The intended flow is, and may only be:
//!
//! ```text
//! segment bytes
//!     ↓ AuthorityHistorySegment::decode   -- strict, fail-closed, structural (§3 rules 1-6)
//! AuthorityHistorySegment                 -- canonical and bounded; no witness checked yet
//!     ↓ AuthorityHistorySegment::admit    -- every pair through AuthorityStore::ingest (rule 7)
//! AdmittedAuthorityHistorySegment         -- every pair admitted; still not authority
//! ```
//!
//! The two stages are two types. [`AuthorityHistorySegment::admit`] is the only way to obtain an
//! [`AdmittedAuthorityHistorySegment`], so code that takes one cannot be handed a segment whose
//! witnesses were never checked.
//!
//! [`AuthorityStore`] has no ingest path other than [`AuthorityStore::ingest`], which calls
//! [`crate::authority_log::admissible`]. This module therefore adds no second validator of
//! signatures or bodies: admission is N1's decision, reported verbatim.
//!
//! # What a valid segment does not establish
//!
//! Not completeness (an omitted fork candidate at or below `to` is undetectable), not that `to` is
//! the Subject's tip (later facts, including revocations, may exist), and not freshness (nothing
//! dates a segment; it can be replayed from any earlier time). ADR-0089 §4.
//!
//! # Canonical bytes
//!
//! All framing is N1's, reused: `LP(x) := u32be(len(x)) || x`, `b32`/`b64` are raw bytes,
//! integers are big-endian and fixed width.
//!
//! ```text
//! history_segment_v1 :=
//!       LP(SEGMENT_DOMAIN)
//!    || u16be(SEGMENT_VERSION)
//!    || b32(subject)
//!    || u64be(from)
//!    || u64be(to)
//!    || u32be(fact_count)                   -- <= MAX_SEGMENT_FACTS
//!    || fact * fact_count                   -- strictly ascending by event_id
//!
//! fact :=
//!       b32(event_id)                       -- SHA-256(canonical_body), stated, not inferred
//!    || LP(canonical_body)                  -- the N1 canonical body bytes, verbatim
//!    || u32be(witness_count)                -- 1..=MAX_WITNESSES_PER_FACT
//!    || b64(signature) * witness_count      -- strictly ascending bytewise
//!
//! segment_id := SHA-256(history_segment_v1)
//! ```
//!
//! The fact layout is N4-B's ([`BundledFact`]), so one body appears once with all of its
//! witnesses. Ordering is by digest and by signature bytes because those are the only canonical
//! identifiers available; neither order is a chain order, an arrival order or a preference.
//!
//! # Validity rules (ADR-0089 §3)
//!
//! 1. `from <= to <= MAX_POSITION`.
//! 2. Every fact's body names `subject`, and its stated `event_id` is the digest of its body.
//! 3. Every fact's position lies in `from..=to`.
//! 4. **Continuity:** every position in `from..=to` holds at least one fact.
//! 5. **Linkage:** every fact at position `p > from` names in `prev_digest` a fact this segment
//!    holds at `p - 1`. If `from == 0` the segment holds the inception, whose event id is
//!    `subject`.
//! 6. Facts ascend strictly by `event_id`; witnesses ascend strictly within a fact; every fact has
//!    at least one witness.
//! 7. Every `(body, witness)` pair passes N1 admission, all or nothing ([`AuthorityHistorySegment::admit`]).
//!
//! Rules 1-6 are checked by [`AuthorityHistorySegment::decode`]; rule 7 by
//! [`AuthorityHistorySegment::admit`]. The import-time anchoring rule (a fact at `from > 0` must
//! link to a fact the node already retains at `from - 1`) needs a store and is not checked here.
//!
//! Several facts at one position are allowed (rule 4 says "at least one"). They are evidence of a
//! fork and are kept.
//!
//! # Bounds
//!
//! [`MAX_SEGMENT_BYTES`] is checked before any byte is parsed. Every declared count is checked
//! against both its protocol bound and the bytes remaining before anything is allocated for it
//! (`Reader::count`). The bounds are ADR-0089 §8's provisional values.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::authority_log::encoding::{Reader, Writer};
use crate::authority_log::{
    sha256, AdmissionError, AuthorityBody, AuthorityStore, CodecError, EventId,
    SignedAuthorityEvent, SubjectId, WitnessSignature, MAX_POSITION,
};
use crate::evidence_bundle::BundledFact;

/// Domain separator that opens every canonical segment. Length-prefixed in the encoding.
///
/// Pairwise distinct from every N1 separator (`icn.authority-log*`), the N4 separators
/// (`icn.n4.*`) and every GEN separator, so segment bytes can never be presented as, or mistaken
/// for, any of those.
pub const SEGMENT_DOMAIN: &[u8] = b"icn.n1.history-segment";

/// Canonical segment version. Decoding rejects any other value.
pub const SEGMENT_VERSION: u16 = 1;

/// Largest accepted encoded segment, checked before parsing (ADR-0089 §8, provisional).
pub const MAX_SEGMENT_BYTES: usize = 1 << 20;

/// Most facts one segment may carry (ADR-0089 §8, provisional).
pub const MAX_SEGMENT_FACTS: usize = 4096;

/// Most witnesses one fact may carry (ADR-0089 §8, provisional).
pub const MAX_WITNESSES_PER_FACT: usize = 16;

/// Minimum wire size of one fact: event_id (32) + body length prefix (4) + witness count (4).
const MIN_FACT_WIRE_LEN: usize = 32 + 4 + 4;

/// Why a segment could not be constructed, decoded or admitted. Every variant is a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SegmentError {
    /// The framing did not parse: wrong domain, truncation, trailing bytes, or a declared count
    /// the input cannot hold.
    #[error("segment framing rejected: {0}")]
    Framing(CodecError),
    /// The segment version is not [`SEGMENT_VERSION`].
    #[error("unsupported history segment version {0}")]
    Version(u16),
    /// The encoded segment exceeds [`MAX_SEGMENT_BYTES`]. Checked before parsing.
    #[error("segment is {len} bytes, above the {max}-byte bound")]
    TooLarge {
        /// The encoded length.
        len: usize,
        /// The bound.
        max: usize,
    },
    /// More facts than [`MAX_SEGMENT_FACTS`].
    #[error("segment declares {count} facts, above the bound of {max}")]
    TooManyFacts {
        /// The declared or supplied count.
        count: usize,
        /// The bound.
        max: usize,
    },
    /// A fact carries more witnesses than [`MAX_WITNESSES_PER_FACT`].
    #[error("fact {event_id} carries {count} witnesses, above the bound of {max}")]
    TooManyWitnesses {
        /// The fact's stated digest.
        event_id: EventId,
        /// The declared or supplied count.
        count: usize,
        /// The bound.
        max: usize,
    },
    /// The range is not `from <= to <= MAX_POSITION` (rule 1).
    #[error("segment range {from}..={to} is not within 0..=MAX_POSITION in ascending order")]
    Range {
        /// The declared first position.
        from: u64,
        /// The declared last position.
        to: u64,
    },
    /// A carried body belongs to a Subject other than the segment's (rule 2).
    #[error("fact {event_id} belongs to another subject")]
    FactSubjectMismatch {
        /// The fact's stated digest.
        event_id: EventId,
    },
    /// The stated `event_id` is not the digest of the carried body (rule 2).
    #[error("fact states event_id {stated} but its body hashes to {computed}")]
    EventIdMismatch {
        /// The digest the fact declared.
        stated: EventId,
        /// The digest of the body actually carried.
        computed: EventId,
    },
    /// A carried body's position lies outside `from..=to` (rule 3).
    #[error("fact {event_id} at position {position} lies outside {from}..={to}")]
    FactOutsideRange {
        /// The fact's stated digest.
        event_id: EventId,
        /// The body's position.
        position: u64,
        /// The segment's first position.
        from: u64,
        /// The segment's last position.
        to: u64,
    },
    /// `from == 0` but the segment does not hold the Subject's inception (rule 5).
    #[error("segment starts at position 0 but does not carry the subject's inception")]
    MissingInception,
    /// A position in `from..=to` holds no fact (rule 4). Reports the lowest such position.
    #[error("no fact at position {position}")]
    ContinuityGap {
        /// The lowest uncovered position.
        position: u64,
    },
    /// A fact at `p > from` does not name a fact this segment holds at `p - 1` (rule 5).
    #[error("fact {event_id} at position {position} does not link to a carried fact at the previous position")]
    BrokenLinkage {
        /// The unlinked fact's digest.
        event_id: EventId,
        /// Its position.
        position: u64,
    },
    /// Facts were not in strictly ascending `event_id` order, or one appeared twice (rule 6).
    #[error("facts are not strictly ascending by event_id (duplicate or reordered fact)")]
    FactOrder,
    /// Witnesses were not in strictly ascending byte order, or one appeared twice (rule 6).
    #[error("witnesses of fact {event_id} are not strictly ascending (duplicate or reordered)")]
    WitnessOrder {
        /// The fact's stated digest.
        event_id: EventId,
    },
    /// A fact carried no witness (rule 6).
    #[error("fact {event_id} carries no witness")]
    NoWitness {
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
    /// A body supplied to [`AuthorityHistorySegment::new`] does not round-trip through its own
    /// canonical encoding. Never raised by decode, whose bodies come from bytes.
    #[error(
        "fact {event_id} carries a body that does not round-trip through its canonical encoding"
    )]
    NonCanonicalBody {
        /// The fact's stated digest.
        event_id: EventId,
    },
    /// A `(body, witness)` pair was refused by canonical N1 admission (rule 7).
    ///
    /// Raised by [`AuthorityHistorySegment::admit`], never by decode: a segment can be
    /// structurally canonical and still carry a witness that does not verify.
    #[error("fact {event_id} was refused by N1 admission: {source}")]
    Inadmissible {
        /// The fact's stated digest.
        event_id: EventId,
        /// N1's own admission error.
        #[source]
        source: AdmissionError,
    },
}

impl From<CodecError> for SegmentError {
    fn from(error: CodecError) -> Self {
        SegmentError::Framing(error)
    }
}

/// A canonical, facts-only excerpt of one Subject's N1 history over `from..=to`.
///
/// Construct through [`AuthorityHistorySegment::new`] or [`AuthorityHistorySegment::decode`].
/// Both apply the same rules — `new` encodes its inputs and decodes them strictly — so a value of
/// this type always has exactly one canonical encoding and always decodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityHistorySegment {
    subject: SubjectId,
    from: u64,
    to: u64,
    facts: BTreeMap<EventId, BundledFact>,
}

impl AuthorityHistorySegment {
    /// Build a segment from parts, refusing anything [`AuthorityHistorySegment::decode`] would
    /// refuse.
    ///
    /// `facts` is keyed by the caller's stated `event_id`; a key that is not the digest of its
    /// body is refused as [`SegmentError::EventIdMismatch`], never re-keyed. Nothing is repaired,
    /// reordered or dropped: the returned segment carries exactly the supplied facts.
    pub fn new(
        subject: SubjectId,
        from: u64,
        to: u64,
        facts: BTreeMap<EventId, BundledFact>,
    ) -> Result<Self, SegmentError> {
        // Bound the counts before encoding: the encoder frames counts as u32 and would wrap.
        if facts.len() > MAX_SEGMENT_FACTS {
            return Err(SegmentError::TooManyFacts {
                count: facts.len(),
                max: MAX_SEGMENT_FACTS,
            });
        }
        for (event_id, fact) in &facts {
            if fact.witnesses.len() > MAX_WITNESSES_PER_FACT {
                return Err(SegmentError::TooManyWitnesses {
                    event_id: *event_id,
                    count: fact.witnesses.len(),
                    max: MAX_WITNESSES_PER_FACT,
                });
            }
            // Public body fields allow a struct literal whose bytes decode to a different body.
            // Refuse it rather than return a segment that carries something the caller did not
            // supply.
            match AuthorityBody::decode(&fact.body.canonical_bytes()) {
                Ok(decoded) if decoded == fact.body => {}
                Ok(_) => {
                    return Err(SegmentError::NonCanonicalBody {
                        event_id: *event_id,
                    })
                }
                Err(source) => {
                    return Err(SegmentError::Body {
                        event_id: *event_id,
                        source,
                    })
                }
            }
        }
        let candidate = AuthorityHistorySegment {
            subject,
            from,
            to,
            facts,
        };
        // One validator: whatever `new` accepts is exactly what `decode` accepts. Every body
        // round-trips (checked above), so the decoded segment is the candidate.
        Self::decode(&candidate.canonical_bytes())
    }

    /// The Subject this segment is an excerpt of.
    pub fn subject(&self) -> SubjectId {
        self.subject
    }

    /// The inclusive position range `(from, to)` the segment covers.
    pub fn range(&self) -> (u64, u64) {
        (self.from, self.to)
    }

    /// The carried facts, keyed and ordered by `event_id`.
    pub fn facts(&self) -> &BTreeMap<EventId, BundledFact> {
        &self.facts
    }

    /// The canonical byte encoding.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(SEGMENT_DOMAIN);
        w.u16(SEGMENT_VERSION);
        w.b32(self.subject.as_bytes());
        w.u64(self.from);
        w.u64(self.to);
        // Bounded by MAX_SEGMENT_FACTS and MAX_WITNESSES_PER_FACT, enforced by every constructor.
        w.u32(self.facts.len() as u32);
        for (event_id, fact) in &self.facts {
            w.b32(event_id.as_bytes());
            w.lp(&fact.body.canonical_bytes());
            w.u32(fact.witnesses.len() as u32);
            for witness in &fact.witnesses {
                w.b64(witness.as_bytes());
            }
        }
        w.finish()
    }

    /// `SHA-256(canonical_bytes)`. Any change to any fact, witness or header field changes it.
    pub fn segment_id(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }

    /// Strict decode of rules 1-6. Fail-closed on every structural defect; see [`SegmentError`].
    ///
    /// **What success proves:** the bytes are one canonical segment; every carried body is a
    /// strict canonical N1 body for this Subject whose stated `event_id` is its digest and whose
    /// position lies in range; every position in range is covered; every fact above `from` links
    /// to a carried fact one position below; a segment from 0 carries the inception.
    ///
    /// **What it does not prove:** that any witness verifies (that is
    /// [`AuthorityHistorySegment::admit`]), that the excerpt is complete or current, or that any
    /// carried fact confers authority.
    pub fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        if bytes.len() > MAX_SEGMENT_BYTES {
            return Err(SegmentError::TooLarge {
                len: bytes.len(),
                max: MAX_SEGMENT_BYTES,
            });
        }
        let mut r = Reader::new(bytes);
        r.expect_lp(SEGMENT_DOMAIN, "domain")?;
        let version = r.u16("version")?;
        if version != SEGMENT_VERSION {
            return Err(SegmentError::Version(version));
        }
        let subject = SubjectId::from_bytes(r.b32("subject")?);
        let from = r.u64("from")?;
        let to = r.u64("to")?;
        if from > to || to > MAX_POSITION {
            return Err(SegmentError::Range { from, to });
        }

        let fact_count = r.count(MIN_FACT_WIRE_LEN, "fact_count")? as usize;
        if fact_count > MAX_SEGMENT_FACTS {
            return Err(SegmentError::TooManyFacts {
                count: fact_count,
                max: MAX_SEGMENT_FACTS,
            });
        }
        let mut facts = BTreeMap::new();
        let mut previous_event_id: Option<EventId> = None;
        for _ in 0..fact_count {
            let stated = EventId::from_bytes(r.b32("event_id")?);
            if let Some(prev) = previous_event_id {
                if stated <= prev {
                    return Err(SegmentError::FactOrder);
                }
            }
            previous_event_id = Some(stated);

            let body_bytes = r.lp("canonical_body")?;
            let body = AuthorityBody::decode(body_bytes).map_err(|source| SegmentError::Body {
                event_id: stated,
                source,
            })?;
            let computed = body.event_id();
            if computed != stated {
                return Err(SegmentError::EventIdMismatch { stated, computed });
            }
            if body.subject() != subject {
                return Err(SegmentError::FactSubjectMismatch { event_id: stated });
            }
            let position = body.position();
            if position < from || position > to {
                return Err(SegmentError::FactOutsideRange {
                    event_id: stated,
                    position,
                    from,
                    to,
                });
            }

            let witness_count = r.count(64, "witness_count")? as usize;
            if witness_count == 0 {
                return Err(SegmentError::NoWitness { event_id: stated });
            }
            if witness_count > MAX_WITNESSES_PER_FACT {
                return Err(SegmentError::TooManyWitnesses {
                    event_id: stated,
                    count: witness_count,
                    max: MAX_WITNESSES_PER_FACT,
                });
            }
            let mut witnesses = BTreeSet::new();
            let mut previous_witness: Option<[u8; 64]> = None;
            for _ in 0..witness_count {
                let signature = r.b64("witness_signature")?;
                if let Some(prev) = previous_witness {
                    if signature <= prev {
                        return Err(SegmentError::WitnessOrder { event_id: stated });
                    }
                }
                previous_witness = Some(signature);
                witnesses.insert(WitnessSignature::from_bytes(signature));
            }

            facts.insert(stated, BundledFact { body, witnesses });
        }
        r.finish()?;

        check_history(subject, from, to, &facts)?;

        Ok(AuthorityHistorySegment {
            subject,
            from,
            to,
            facts,
        })
    }

    /// Admit every carried `(body, witness)` pair through canonical N1 admission into an empty
    /// in-memory store (rule 7), and return the segment together with that store.
    ///
    /// Every pair goes through [`AuthorityStore::ingest`] — every witness, not one per fact.
    /// Fails closed on the first refusal and returns no segment: a partially admitted store is a
    /// different history than the one carried, so it is dropped. The store is in memory only;
    /// nothing is persisted, and no fact is filtered by what the N1 fold would conclude from it.
    ///
    /// Consumes the segment, so admission moves it rather than copying it.
    pub fn admit(self) -> Result<AdmittedAuthorityHistorySegment, SegmentError> {
        let mut store = AuthorityStore::new();
        for (event_id, fact) in &self.facts {
            for witness in &fact.witnesses {
                let event = SignedAuthorityEvent::new(fact.body.clone(), *witness);
                store
                    .ingest(&event)
                    .map_err(|source| SegmentError::Inadmissible {
                        event_id: *event_id,
                        source,
                    })?;
            }
        }
        Ok(AdmittedAuthorityHistorySegment {
            segment: self,
            store,
        })
    }
}

/// A history segment whose every carried `(body, witness)` pair passed N1 admission (rule 7).
///
/// The fields are private and there is no public constructor: [`AuthorityHistorySegment::admit`]
/// is the only way to obtain one, and it returns one only when every pair was admitted. A
/// consumer that must not act on unchecked witnesses — the future importer — takes this type, and
/// a structurally decoded [`AuthorityHistorySegment`] is not accepted in its place.
///
/// Building one outside this module does not compile (the fields are private, E0451):
///
/// ```compile_fail,E0451
/// use icn_identity::authority_log::AuthorityStore;
/// use icn_identity::history_segment::{AdmittedAuthorityHistorySegment, AuthorityHistorySegment};
/// fn forge(segment: AuthorityHistorySegment, store: AuthorityStore) -> AdmittedAuthorityHistorySegment {
///     AdmittedAuthorityHistorySegment { segment, store }
/// }
/// ```
///
/// Nor does passing a decoded segment where an admitted one is required (E0308):
///
/// ```compile_fail,E0308
/// use icn_identity::history_segment::{AdmittedAuthorityHistorySegment, AuthorityHistorySegment};
/// fn import(_: &AdmittedAuthorityHistorySegment) {}
/// fn skip_admission(decoded: &AuthorityHistorySegment) {
///     import(decoded)
/// }
/// ```
///
/// Stable rustdoc does not check the error code of a `compile_fail` test, so this control compiles
/// everything the tests above use except the struct literal and the substitution. If it compiles,
/// those are what fail above.
///
/// ```
/// use icn_identity::authority_log::AuthorityStore;
/// use icn_identity::history_segment::{
///     AdmittedAuthorityHistorySegment, AuthorityHistorySegment, SegmentError,
/// };
/// fn import(_: &AdmittedAuthorityHistorySegment) {}
/// fn admit_then_import(decoded: AuthorityHistorySegment) -> Result<(), SegmentError> {
///     import(&decoded.admit()?);
///     Ok(())
/// }
/// fn read(a: &AdmittedAuthorityHistorySegment) -> (&AuthorityHistorySegment, &AuthorityStore) {
///     (a.segment(), a.store())
/// }
/// fn parts(segment: AuthorityHistorySegment, store: AuthorityStore) -> (AuthorityHistorySegment, AuthorityStore) {
///     (segment, store)
/// }
/// ```
///
/// # What admission does not establish
///
/// Admissible is not authorized. A fact signed by a key that held no authority, and both
/// candidates of a fork, are admitted and kept here; whether any key had authority is what
/// [`crate::authority_log::derive()`] concludes over [`AdmittedAuthorityHistorySegment::store`], and
/// nothing in this type records or caches that conclusion. Admission is also not persistence, not
/// retention permission, not import-time anchoring against retained facts, and not completeness
/// or freshness (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedAuthorityHistorySegment {
    segment: AuthorityHistorySegment,
    store: AuthorityStore,
}

impl AdmittedAuthorityHistorySegment {
    /// The admitted segment, unchanged: the same facts, witnesses, range and canonical bytes it
    /// had before admission.
    pub fn segment(&self) -> &AuthorityHistorySegment {
        &self.segment
    }

    /// The in-memory store built by ingesting every carried pair, and nothing else: its bodies
    /// are exactly the segment's bodies and its witnesses exactly the segment's witnesses.
    pub fn store(&self) -> &AuthorityStore {
        &self.store
    }
}

/// Rules 4 and 5 over a fact set that already satisfies rules 2, 3 and 6.
fn check_history(
    subject: SubjectId,
    from: u64,
    to: u64,
    facts: &BTreeMap<EventId, BundledFact>,
) -> Result<(), SegmentError> {
    // An inception's event id is its Subject, so this asks for exactly that body.
    if from == 0 && !facts.contains_key(&EventId::from_bytes(*subject.as_bytes())) {
        return Err(SegmentError::MissingInception);
    }

    // Continuity. Every position is already inside from..=to, so the distinct positions, walked in
    // ascending order, must be exactly from, from + 1, ..., to.
    let positions: BTreeSet<u64> = facts.values().map(|fact| fact.body.position()).collect();
    let mut expected = from;
    for &position in &positions {
        if position != expected {
            return Err(SegmentError::ContinuityGap { position: expected });
        }
        expected += 1;
    }
    if expected <= to {
        return Err(SegmentError::ContinuityGap { position: expected });
    }

    // Linkage. Above `from`, every fact names a carried fact one position below. A fact at `from`
    // is not checked here: for `from == 0` it is the inception, and for `from > 0` its parent is
    // outside the segment (anchoring against retained facts is an import-time rule).
    for (event_id, fact) in facts {
        let position = fact.body.position();
        if position == from {
            continue;
        }
        let linked = fact
            .body
            .prev_digest()
            .and_then(|prev| facts.get(&prev))
            .is_some_and(|parent| parent.body.position() + 1 == position);
        if !linked {
            return Err(SegmentError::BrokenLinkage {
                event_id: *event_id,
                position,
            });
        }
    }
    Ok(())
}
