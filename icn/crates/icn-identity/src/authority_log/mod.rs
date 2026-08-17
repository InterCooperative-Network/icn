//! N1 — the human-subject authority-log primitive.
//!
//! This is a **library-only** implementation of the authority-state model specified in
//! `docs/architecture/HUMAN_IDENTITY_ARCHITECTURE.md` §9.1, §9.2, §9.2.1 and §9.2.2. It is
//! additive: nothing in this module is wired into the gateway, gossip, governance, or the
//! existing [`crate::multi_device`] path, and `multi_device.rs` is deliberately untouched.
//!
//! The purpose is to prove that the merged architecture's core authority-state model can be
//! represented, merged, derived, and adversarially tested with the properties it claims — not
//! to build the identity system.
//!
//! # The two layers, which must not be collapsed
//!
//! * **Durable replicated state** answers *"what authenticated bodies have I learned?"* It is
//!   two grow-only sets joined by union ([`AuthorityStore`]).
//! * **Derived authority view** answers *"which of those bodies have authority in the subject's
//!   history?"* It is a pure function of the durable body set ([`derive`]).
//!
//! An admissible body can be unauthorized. That is intentional: spam enters the durable set and
//! is never selected by the derived view, so **spam cannot manufacture duplicity**.
//!
//! # 1. The canonical event preimage
//!
//! Hand-rolled, versioned, domain-separated, length-prefixed and deterministic. It is
//! deliberately *not* serde/bincode: `RotationEvent::signing_message` (finding F12) signs a bare
//! bincode encoding of a struct with the proof field zeroed, which is neither versioned nor
//! domain-separated.
//!
//! ```text
//! LP(x)  := u32be(x.len()) || x
//! b32(x) := 32 raw bytes, no prefix
//! P(k)   := u8(0x01) || b32(ed25519_public_key)        -- a principal, tagged
//! PS(k)  := u32be(1) || P(k)                           -- exactly one logical writer
//! CS(C)  := u32be(|C|) || u8(tag) || ...               -- strictly ascending, may be empty
//! SPAN   := u8(0x00) OR (u8(0x01) || u64be(from) || u64be(until))
//!
//! canonical_body := LP(DOMAIN) || u16be(PROTOCOL_VERSION) || u8(kind) || payload
//!
//! HDR                     := b32(subject) || u64be(position) || b32(prev_digest) || P(signer)
//! payload(Inception 0x01) := P(signer) || b32(context_nonce) || PS(initial_authority)
//!                            || b32(next_commitment)
//! payload(Rotate    0x02) := HDR || PS(revealed_authority) || b32(next_commitment)
//! payload(Authorize 0x03) := HDR || P(device) || CS(capabilities) || SPAN
//! payload(Revoke    0x04) := HDR || P(device)
//! payload(Recover   0x05) := HDR || PS(revealed_authority) || b32(next_commitment)
//! ```
//!
//! * Integers are big-endian and fixed width; enums are a single tag byte; unknown tags and
//!   unknown versions are rejected.
//! * **`Did` canonicalization (absorbs O10):** a principal is encoded as the *decoded* 32-byte
//!   Ed25519 public key, never as the multibase string. Two DID string encodings of one key
//!   therefore produce one preimage. See [`PrincipalKey`].
//! * The inception body carries no subject, position or prev field: its subject is
//!   self-addressing, its position is the protocol constant `0`, and it has no parent. Every
//!   other kind rejects `position == 0`.
//! * Decoding is strict — trailing bytes, a writer-set cardinality other than one, duplicate
//!   capability members and an inception signer outside `initial_authority` are all rejected.
//!   Together with the fixed field order this makes the encoding a bijection on well-formed
//!   bodies: one logical body has exactly one canonical encoding, and one canonical encoding
//!   denotes exactly one logical body.
//!
//! Digests, stated once and used everywhere (§9.2.1):
//!
//! ```text
//! event_id           = SHA-256(canonical_body)
//! SubjectId σ        = event_id(inception body)
//! prev_digest        = event_id(parent body)
//! signature preimage = LP(SIGNATURE_DOMAIN) || canonical_body
//! ```
//!
//! **Signatures are outside every one of these digests.** Hashing a whole signed event would let
//! a retrying signer mint a second `SubjectId`, or name a parent variant that dedup discards.
//!
//! # 2. The pre-rotation commitment scheme
//!
//! Commitments are indexed by **establishment generation** `g`, not by log position: positions
//! also carry `authorize`/`revoke` events, so a position-indexed chain would burn a generation on
//! every non-establishment event. Inception is generation 0.
//!
//! ```text
//! A_g = { pubkey(SHA-256(LP(KDF_DOMAIN) || b32(nonce) || u64be(g) || b32(root))) }
//! C_g = SHA-256(LP(COMMITMENT_DOMAIN) || u8(kind_g) || PS(A_g) || b32(C_{g+1}))
//! C_{horizon+1} = TERMINAL_COMMITMENT                 -- chain exhausted, nothing further armed
//! ```
//!
//! The commitment covers the *next* commitment, so it is a backward-linked chain computed from a
//! horizon. Revealing generation `g` therefore reveals every signer-chosen field of the
//! establishment body at once.
//!
//! Candidate authorization **enforces** canonical derivation rather than merely permitting it:
//!
//! 1. `state.next_commitment != TERMINAL_COMMITMENT`;
//! 2. `SHA-256(COMMITMENT_DOMAIN ‖ kind ‖ PS(revealed) ‖ next_commitment) == state.next_commitment`;
//! 3. `body.signer == sole(revealed)` — the singleton-writer rule, which pins the last field the
//!    signer would otherwise be free to choose.
//!
//! The derived state pins `subject`, `position` and `prev_digest`. Every field of the body is
//! therefore determined, so **at most one authorized establishment body can exist at a position**
//! — §9.2.1 constraints 2 and 3, both closed, without threshold signing. A non-canonical body
//! (from a malicious quorum, or from an honest retry with fresh randomness) fails the internal
//! candidate-authorization check and never enters the candidate set.
//!
//! Because `kind` is inside the commitment, **exactly one next authority and exactly one
//! establishment kind is armed per generation**: `Rotate` and `Recover` can never both be armed
//! at the same position, so path (a) and path (b) cannot race (§12.2).
//!
//! **Genesis avoids the fixed-point cycle.** `A_0` is derived from a locally chosen *pre-subject
//! context nonce*, never from `SubjectId`: the inception body contains `A_0`, and
//! `SubjectId = H(inception body)`, so `A_0 = KDF(root ‖ SubjectId)` would require solving
//! `S = H(inception(KDF(root, S)))`, which offline genesis cannot generally do. No derivation
//! input anywhere in this module is a `SubjectId`.
//!
//! **No randomness enters establishment construction.** [`ContinuityRoot::establish`] takes no
//! entropy source; an honest crash and retry re-derives byte-identical bytes and the same
//! `event_id`.
//!
//! # 3. The event taxonomy
//!
//! | kind | subject | position | prev | establishment | authorization rule | apply | validity span |
//! |---|---|---|---|---|---|---|---|
//! | `Inception` | self-addressing | `0` | none | no — it *is* the root | selected by `σ == event_id`, never by candidate authorization | authority `= A_0`, next `= C_1`, generation `0`, no devices | no |
//! | `Rotate` | named | `≥ 1` | yes | **yes** | pre-image reveal | authority `= A_g`, next `= C_{g+1}`, generation `+1`, devices **preserved** | no |
//! | `Recover` | named | `≥ 1` | yes | **yes** | pre-image reveal | authority `= A_g`, next `= C_{g+1}`, generation `+1`, devices **cleared** | no |
//! | `Authorize` | named | `≥ 1` | yes | no | signer ∈ `state.authority` | records a device grant | **yes** |
//! | `Revoke` | named | `≥ 1` | yes | no | signer ∈ `state.authority` | removes a device grant | no |
//!
//! Two invariants here are normative, not stylistic:
//!
//! * **An event is an establishment event iff candidate authorization requires an authenticated
//!   pre-rotation pre-image reveal.** Never a capability flag. [`DeviceCapability::Recover`]
//!   exists in this module *deliberately* and is completely inert with respect to establishment:
//!   if a capability bit could mint establishment priority, a compromised current key would win
//!   [`supersede`] against the legitimate holder's rotation — which is finding F13/F13a one layer
//!   up. No delegation path may mint establishment authority.
//! * **Every authority principal named inside a body is a raw key** ([`PrincipalKey`]), never a
//!   [`SubjectId`]. Otherwise candidate authorization would depend on another subject's log and
//!   [`derive`]
//!   would stop being a closed pure function. There is no conversion from `SubjectId` into
//!   `PrincipalKey` anywhere in this module, and the wire format tags the two differently.
//!
//! Devices are **not** log writers. Each authority set contains exactly one logical writer, so the
//! single-writer property (§9.2, consensus number 1) is preserved. A future threshold/group key
//! remains one [`PrincipalKey`]. Validity spans are position-denominated and never wall-clock
//! (O-N1 stays inside the pure model).
//!
//! # 4. The replicated state model (§9.2.1, inherited — not redesigned)
//!
//! ```text
//! Bodies(σ)  = { b : ∃ s. admissible(b, s) ∧ b.subject = σ }      -- grow-only
//! Witness(σ) = { (event_id(b), s) : admissible(b, s) }            -- grow-only
//! join(A, B) = (A.bodies ∪ B.bodies, A.witnesses ∪ B.witnesses)
//! ```
//!
//! Associativity, commutativity, idempotence and monotonicity are inherited from set union, so
//! receipt order cannot change durable state.
//!
//! [`admissible`] is a **free function**: it takes a body and a signature and nothing else — no
//! frontier, no store, no arrival history, no clock. That is what makes the filter commute with
//! union, `filter(A) ∪ filter(B) = filter(A ∪ B)`, and it is why [`MAX_POSITION`] is an absolute
//! protocol constant rather than `local_frontier + C`.
//!
//! [`supersede`] is the **only** selection rule. It selects by authority class — establishment
//! over non-establishment — never by content. There is no lowest-hash, first-seen, longest-chain,
//! lexicographic, arrival-order or random tiebreak anywhere in this module. Where authority class
//! cannot distinguish candidates, [`derive`] **halts**. The [`std::collections::BTreeSet`]
//! ordering used for storage and diagnostics is never consulted as a selector: [`resolve`] is the
//! single decision point and it returns [`Resolution::Halt`] for any candidate set of size ≥ 2.
//!
//! # What N1 does not solve
//!
//! Stated here so it cannot be lost downstream. N1 does **not** solve **O9** (global convergent
//! revocation), **O14** (total-loss recovery is only mechanically dissolved — a lost backup is
//! still a lost subject, §12.1), or **O16/O16′** (a single compromised current key halts a
//! subject unilaterally; that is reproduced as a test, not prevented).
//!
//! * **O-N7 — irreversible settlement is OPEN.** Authority-log state is not by itself a
//!   sufficient basis for irreversible settlement under late fork discovery. One receiver can
//!   settle an act under an apparently-live branch before learning of a fork another receiver
//!   already sees; if settled effects cannot roll back, downstream state diverges permanently.
//!   N1 does not define finality depth.
//! * **O-N8 — adversarial storage pressure is OPEN.** The durable admissible set is
//!   attacker-writable: anyone can mint unlimited admissible bodies for a public σ by signing
//!   arbitrary bytes with a fresh key. A bounded store needs a *protocol-specified*,
//!   authority-aware admission and compaction rule, and the gap region beyond the derived
//!   frontier stays unresolved because authorization is not yet computable there. This module
//!   therefore uses an **unbounded in-memory representation** rather than pretending to solve
//!   storage policy, and the convergence claims proven here hold only absent adversarial storage
//!   pressure.
//! * **O-N9 — historical authorization evidence is OPEN.** After a superseding establishment
//!   event, orphaned bodies remain durable facts, but the effective authority history may no
//!   longer prove that a key first authorized in the orphaned suffix was ever authorized.
//!   *"K signed these bytes"* survives; *"K was authorized in the accepted history"* does not.
//!   N1 does not invent an accepted-at or branch-endorsement evidence format.
//!
//! Guardian recovery is **not** implemented here. N7 (FROST-class threshold signing) is a hard
//! prerequisite and is necessary but not sufficient: a group key proves a quorum signed a
//! message, not that the quorum can sign only one body. The two rules that close the remaining
//! gap — candidate authorization enforcing canonical derivation, and exactly one next authority armed per
//! position — are implemented here in the generic establishment machinery, but the
//! guardian workflow itself is out of scope.

mod admission;
mod body;
mod construct;
mod derive;
mod encoding;
mod store;

pub use admission::{admissible, admissible_bytes, AdmissionError};
pub use body::{
    AuthorityBody, AuthorizeBody, CapabilitySet, Commitment, ContextNonce, DeviceCapability,
    EstablishmentBody, EventHeader, EventId, InceptionBody, PrincipalKey, PrincipalSet, RevokeBody,
    SubjectId, ValiditySpan,
};
pub use construct::{
    authorize_event, revoke_event, sign_body, ConstructError, ContinuityRoot, EstablishmentKind,
};
pub use derive::{
    derive, resolve, supersede, AuthorityState, AuthorityView, DeviceGrant, Resolution,
};
pub use encoding::CodecError;
pub use store::{AuthorityStore, SignedAuthorityEvent, Witness, WitnessSignature};

/// Domain separator that opens every canonical body encoding.
///
/// Length-prefixed, so it can never be confused with a payload prefix of a different length.
pub const DOMAIN: &[u8] = b"icn.authority-log";

/// Canonical-encoding version. Decoding rejects any other value.
pub const PROTOCOL_VERSION: u16 = 1;

/// Domain separator for the signature preimage.
///
/// Distinct from [`DOMAIN`] so that a signature over an authority body can never be reinterpreted
/// as a signature over the digest preimage, or over any other ICN structure.
pub const SIGNATURE_DOMAIN: &[u8] = b"icn.authority-log.sig";

/// Domain separator for pre-rotation commitments.
pub const COMMITMENT_DOMAIN: &[u8] = b"icn.authority-log.commit";

/// Domain separator for continuity-root key derivation.
pub const KDF_DOMAIN: &[u8] = b"icn.authority-log.kdf";

/// The **absolute** upper bound on an event position.
///
/// This is a protocol constant, deliberately *not* `local_frontier + C`. A frontier-relative
/// bound would make [`admissible`] depend on local state, which breaks
/// `filter(A) ∪ filter(B) = filter(A ∪ B)` and therefore breaks the join (§9.2.1).
pub const MAX_POSITION: u64 = 1_048_576;

/// Canonical-body kind tags. These are wire values and must never be renumbered.
pub(crate) mod kind {
    pub(crate) const INCEPTION: u8 = 0x01;
    pub(crate) const ROTATE: u8 = 0x02;
    pub(crate) const AUTHORIZE: u8 = 0x03;
    pub(crate) const REVOKE: u8 = 0x04;
    pub(crate) const RECOVER: u8 = 0x05;
}

/// Tag byte introducing a principal. Principals are always raw Ed25519 public keys.
///
/// A distinct tag space is what makes "a `SubjectId` placed in a principal field" a decode
/// error rather than a silently accepted 32-byte blob.
pub(crate) const PRINCIPAL_TAG_ED25519: u8 = 0x01;

/// Compute SHA-256 over `input`.
pub(crate) fn sha256(input: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input);
    hasher.finalize().into()
}
