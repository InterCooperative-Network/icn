//! N4-B — the `(N1 facts, device act)` container: what a client carries to a relying party.
//!
//! N4-A (`device_authority`) decides one question for one relying party over an
//! [`AuthorityStore`] it already holds. It deliberately left one boundary unbuilt
//! (`N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.2 item 2): a strict, deterministic container that a
//! non-Rust client can hand to *any* relying party — a CLI, a hosted verifier, another process —
//! holding the Subject's retained canonical facts and one signed act, so that the relying party
//! can rebuild the store and run the N4-A verifier itself. This module is that container.
//!
//! # What is deliberately true here
//!
//! - **The bundle is bytes, not authority.** Every fact it carries is re-admitted through
//!   [`AuthorityStore::ingest`], the same gate a live event passes. Nothing in the bundle is
//!   believed because it is in the bundle.
//! - **One inadmissible fact fails the whole bundle.** A replica may drop a bad event and go on;
//!   a verifier handed a container by an untrusted party must not: silently dropping a fact
//!   would let the sender choose which facts the relying party evaluates against.
//! - **Withholding a fact can only shrink what the bundle proves.** The N4-A prefix rule makes
//!   a missing position a refusal (`PrefixIncomplete`), never an extension of authority.
//! - **The relying party still supplies the Subject and the position.** The bundle names
//!   neither; [`verify_bundle`] takes both from the caller, exactly as [`verify_device_act`] does
//!   (N4 invariant 6).
//! - **Canonical and order-free.** Facts are encoded in one order — ascending `(event_id,
//!   witness)` — and duplicates collapse, so one set of facts and one act have one encoding and
//!   one [`DeviceAuthorityBundleV1::bundle_id`]. The decoder refuses any other order.
//! - **No clock, no network, no filesystem, no randomness.** Same bytes, same `(subject, E)` ⇒
//!   same verdict, everywhere.
//!
//! # What is deliberately not here
//!
//! No transport (how the bytes travel is the carrier's business). No persistence: the N1-D fact
//! store (#2800) persists facts for a *replica*; this is what a *client* carries, and the fact
//! record below is laid out as `event_id ‖ witness → canonical body` so the two framings are one.
//! No enrollment ceremony, no revocation propagation, no recovery, no gateway route, no session
//! coupling, no new capability vocabulary. Fixture identities only in the tests.
//!
//! # Canonical bundle bytes
//!
//! N1 framing, reused: `LP(x) := u32be(len(x)) || x`; `b32`/`b64` are raw bytes; integers are
//! big-endian.
//!
//! ```text
//! bundle_v1 :=
//!       LP(DEVICE_AUTHORITY_BUNDLE_DOMAIN)
//!    || u16be(DEVICE_AUTHORITY_BUNDLE_VERSION)
//!    || u32be(fact_count)
//!    || fact_count × fact
//!    || LP(device_act_v1)          -- the act's canonical bytes (N4-A)
//!    || b64(act_signature)
//!
//! fact :=
//!       b32(event_id)              -- SHA-256(canonical_body); checked on decode
//!    || b64(witness_signature)     -- one Ed25519 witness over the body's signature preimage
//!    || LP(canonical_body)         -- the N1 canonical body bytes, exactly
//!
//! bundle_id := SHA-256(bundle_v1)
//! ```
//!
//! Facts are strictly ascending by `(event_id, witness_signature)`; `fact_count ≤`
//! [`MAX_BUNDLE_FACTS`]. A `fact_count` the remaining input cannot hold is refused before any
//! allocation. Trailing bytes are an error. [`DEVICE_AUTHORITY_BUNDLE_DOMAIN`] is distinct from
//! every N1, GEN and N4-A domain separator.

use thiserror::Error;

use crate::authority_log::encoding::{Reader, Writer};
use crate::authority_log::{
    sha256, AdmissionError, AuthorityBody, AuthorityStore, CodecError, EventId,
    SignedAuthorityEvent, SubjectId, WitnessSignature,
};
use crate::device_authority::{
    verify_device_act, DeviceActError, DeviceActSignature, DeviceActV1, DeviceActVerifyError,
    DeviceAuthorityEvidence, SignedDeviceAct,
};

/// Domain separator that opens every canonical bundle. Length-prefixed in the encoding.
pub const DEVICE_AUTHORITY_BUNDLE_DOMAIN: &[u8] = b"icn.n4.device-authority-bundle";

/// Canonical bundle version. Decoding rejects any other value.
pub const DEVICE_AUTHORITY_BUNDLE_VERSION: u16 = 1;

/// Upper bound on the facts one bundle may carry. A protocol constant: it bounds what a verifier
/// must admit and hash for one act. A Subject's whole log fits many times over.
pub const MAX_BUNDLE_FACTS: u32 = 1 << 16;

/// The smallest possible fact record: `event_id ‖ witness ‖ LP(empty)`. Used only to bound the
/// declared count against the remaining input before allocating.
const MIN_FACT_LEN: usize = 32 + 64 + 4;

/// Why bundle bytes could not be constructed or decoded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BundleError {
    /// The bytes are not a strict canonical bundle (framing, domain, version, count, body).
    #[error("canonical bundle is malformed: {0}")]
    Codec(#[from] CodecError),
    /// More facts than [`MAX_BUNDLE_FACTS`].
    #[error("bundle carries {0} facts, above the {MAX_BUNDLE_FACTS}-fact bound")]
    TooManyFacts(usize),
    /// Fact `index` is not strictly after its predecessor in `(event_id, witness)` order: the
    /// bundle is either reordered or carries a duplicate.
    #[error("fact {index} is not in canonical order after its predecessor")]
    FactsNotCanonicallyOrdered {
        /// Zero-based index of the offending fact record.
        index: u32,
    },
    /// Fact `index` declares an `event_id` that is not the digest of the body it carries.
    #[error("fact {index} carries an event id that is not the digest of its body")]
    EventIdMismatch {
        /// Zero-based index of the offending fact record.
        index: u32,
    },
    /// Fact `index` carries bytes that decode but are not the canonical encoding of the body.
    #[error("fact {index} is not the canonical encoding of its body")]
    NonCanonicalFact {
        /// Zero-based index of the offending fact record.
        index: u32,
    },
    /// The act is not a strict canonical device act.
    #[error("device act is malformed: {0}")]
    Act(#[from] DeviceActError),
}

/// Why a bundle, as a whole, could not establish device authority.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BundleVerifyError {
    /// The bytes did not decode.
    #[error("{0}")]
    Bundle(#[from] BundleError),
    /// Fact `index` decoded but was refused by N1 admission. The whole bundle is refused: a
    /// verifier handed bytes by an untrusted party never drops a fact and carries on.
    #[error("fact {index} is not admissible: {error}")]
    InadmissibleFact {
        /// Zero-based index of the refused fact.
        index: u32,
        /// Why admission refused it.
        #[source]
        error: AdmissionError,
    },
    /// Every fact was admitted and the act was still refused — the N4-A verdict.
    #[error("{0}")]
    Act(#[from] DeviceActVerifyError),
}

/// A set of retained N1 facts and one signed device act, in canonical form.
///
/// Construct through [`DeviceAuthorityBundleV1::new`] (which canonicalises the facts) or
/// [`DeviceAuthorityBundleV1::decode`] (which refuses anything non-canonical). The fields are
/// private so that no bundle can exist whose encoding is not canonical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAuthorityBundleV1 {
    facts: Vec<SignedAuthorityEvent>,
    act: SignedDeviceAct,
}

/// The canonical ordering key of a fact record.
fn fact_key(fact: &SignedAuthorityEvent) -> ([u8; 32], [u8; 64]) {
    (*fact.body.event_id().as_bytes(), *fact.signature.as_bytes())
}

/// What one fact in a bundle is, for inspection. Decides nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFactSummary {
    /// Zero-based index in the canonical order.
    pub index: u32,
    /// The N1 body kind tag (`0x01` inception … `0x05` recover).
    pub kind_tag: u8,
    /// The Subject the body names.
    pub subject: SubjectId,
    /// The body's log position.
    pub position: u64,
    /// The body's digest.
    pub event_id: EventId,
}

impl DeviceAuthorityBundleV1 {
    /// Build a bundle, canonicalising the facts: sorted by `(event_id, witness)`, duplicates
    /// collapsed. The act is taken as given; its bytes are already canonical by construction.
    pub fn new(
        facts: impl IntoIterator<Item = SignedAuthorityEvent>,
        act: SignedDeviceAct,
    ) -> Result<Self, BundleError> {
        let mut facts: Vec<SignedAuthorityEvent> = facts.into_iter().collect();
        facts.sort_by_key(fact_key);
        facts.dedup_by_key(|fact| fact_key(fact));
        if facts.len() > MAX_BUNDLE_FACTS as usize {
            return Err(BundleError::TooManyFacts(facts.len()));
        }
        Ok(DeviceAuthorityBundleV1 { facts, act })
    }

    /// The facts, in canonical order.
    pub fn facts(&self) -> &[SignedAuthorityEvent] {
        &self.facts
    }

    /// The signed act.
    pub fn act(&self) -> &SignedDeviceAct {
        &self.act
    }

    /// The canonical byte encoding.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(DEVICE_AUTHORITY_BUNDLE_DOMAIN);
        w.u16(DEVICE_AUTHORITY_BUNDLE_VERSION);
        // `new` and `decode` both bound the count by MAX_BUNDLE_FACTS, which fits a u32.
        w.u32(self.facts.len() as u32);
        for fact in &self.facts {
            w.b32(fact.body.event_id().as_bytes());
            w.b64(fact.signature.as_bytes());
            w.lp(&fact.body.canonical_bytes());
        }
        w.lp(&self.act.act.canonical_bytes());
        w.b64(self.act.signature.as_bytes());
        w.finish()
    }

    /// Strict decode. Wrong domain or version, a count the input cannot hold, a fact out of
    /// canonical order or duplicated, an `event_id` that is not its body's digest, a body that
    /// is not canonical, a malformed act, or trailing bytes: each is an error. Admissibility of
    /// the facts is **not** decided here; that is [`verify_bundle`]'s first step.
    pub fn decode(bytes: &[u8]) -> Result<Self, BundleError> {
        let mut r = Reader::new(bytes);
        r.expect_lp(DEVICE_AUTHORITY_BUNDLE_DOMAIN, "domain")?;
        let version = r.u16("version")?;
        if version != DEVICE_AUTHORITY_BUNDLE_VERSION {
            return Err(CodecError::UnsupportedVersion(version).into());
        }
        let count = r.count(MIN_FACT_LEN, "fact_count")?;
        if count > MAX_BUNDLE_FACTS {
            return Err(BundleError::TooManyFacts(count as usize));
        }
        let mut facts = Vec::with_capacity(count as usize);
        let mut previous: Option<([u8; 32], [u8; 64])> = None;
        for index in 0..count {
            let event_id = r.b32("event_id")?;
            let witness = r.b64("witness")?;
            let body_bytes = r.lp("body")?;
            let body = AuthorityBody::decode(body_bytes)?;
            if body.canonical_bytes() != body_bytes {
                return Err(BundleError::NonCanonicalFact { index });
            }
            if body.event_id().as_bytes() != &event_id {
                return Err(BundleError::EventIdMismatch { index });
            }
            let key = (event_id, witness);
            if previous.is_some_and(|p| key <= p) {
                return Err(BundleError::FactsNotCanonicallyOrdered { index });
            }
            previous = Some(key);
            facts.push(SignedAuthorityEvent::new(
                body,
                WitnessSignature::from_bytes(witness),
            ));
        }
        let act = DeviceActV1::decode(r.lp("act")?)?;
        let signature = DeviceActSignature::from_bytes(r.b64("act_signature")?);
        r.finish()?;
        Ok(DeviceAuthorityBundleV1 {
            facts,
            act: SignedDeviceAct { act, signature },
        })
    }

    /// `SHA-256(canonical_bytes)`. Witness-*dependent*, unlike an act id: two bundles carrying
    /// the same facts under different witnesses are different bundles, and that is correct — the
    /// bundle identifies what was carried, not what it proves.
    pub fn bundle_id(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }

    /// Describe each fact without deciding anything.
    pub fn summarize(&self) -> Vec<BundleFactSummary> {
        self.facts
            .iter()
            .enumerate()
            .map(|(index, fact)| BundleFactSummary {
                index: index as u32,
                kind_tag: fact.body.kind_tag(),
                subject: fact.body.subject(),
                position: fact.body.position(),
                event_id: fact.body.event_id(),
            })
            .collect()
    }
}

/// Rebuild the store a bundle describes, admitting every fact through the N1 gate.
///
/// Fail-closed on the first inadmissible fact, naming it. Nothing is dropped.
pub fn store_from_bundle(
    bundle: &DeviceAuthorityBundleV1,
) -> Result<AuthorityStore, BundleVerifyError> {
    let mut store = AuthorityStore::new();
    for (index, fact) in bundle.facts.iter().enumerate() {
        store
            .ingest(fact)
            .map_err(|error| BundleVerifyError::InadmissibleFact {
                index: index as u32,
                error,
            })?;
    }
    Ok(store)
}

/// Verify a bundle's act for `subject` at the relying party's `position`.
///
/// Exactly [`verify_device_act`] over the store the bundle rehydrates: the bundle adds no
/// semantics and removes none. `subject` and `position` come from the caller, never the bytes.
pub fn verify_bundle(
    bundle: &DeviceAuthorityBundleV1,
    subject: SubjectId,
    position: u64,
) -> Result<DeviceAuthorityEvidence, BundleVerifyError> {
    let store = store_from_bundle(bundle)?;
    verify_device_act(&bundle.act, subject, &store, position).map_err(BundleVerifyError::Act)
}

/// [`DeviceAuthorityBundleV1::decode`] then [`verify_bundle`], for callers holding only bytes.
pub fn verify_bundle_bytes(
    bytes: &[u8],
    subject: SubjectId,
    position: u64,
) -> Result<DeviceAuthorityEvidence, BundleVerifyError> {
    let bundle = DeviceAuthorityBundleV1::decode(bytes)?;
    verify_bundle(&bundle, subject, position)
}
