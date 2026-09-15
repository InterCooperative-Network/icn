//! GEN-A — context-scoped Subject genesis over the N1 authority log.
//!
//! This module answers exactly one question, and deliberately no others:
//!
//! > What does it mean for a human Subject to exist **as this Subject, in this ICN context**,
//! > before any institution recognizes, enrols, authorizes or governs them?
//!
//! The answer is a deterministic derivation that turns a *named context* plus fresh public
//! randomness into the 32 opaque bytes N1 already accepts as its
//! [`ContextNonce`](crate::authority_log::ContextNonce), plus two witness-independent reference
//! hashes that later semantic layers can point at.
//!
//! # What this is not
//!
//! **No registrar exists.** Nothing here consults a service, a directory, a database row or an
//! institution. A Subject is established by the person's own client from material the person
//! holds; the governance domain is named as a *context*, never as an issuer or an owner.
//!
//! A verified [`SubjectContextGenesisV1`] proves exactly one thing:
//!
//! > A fresh N1 Subject was incepted for one explicitly named governance-domain context, with
//! > one narrow initial device authorization, under existing N1 authority-log semantics.
//!
//! It does **not** establish institutional recognition, membership, standing, governance
//! participation, voting rights, economic authority, delegation, session authorization,
//! guardian recovery, replication or finality. Each of those is a later, separate fact that may
//! *reference* [`SubjectContextRef`] — none of them is implied by it.
//!
//! # The bootstrap separation invariant
//!
//! The initial device principal **MUST NOT** be a member of the inception's generation-0
//! establishment authority set. A capability set attenuates what a principal may *do*; it cannot
//! attenuate what that principal already *is*. If the device principal is the establishment
//! authority, then the narrow `{Sign, Present}` credential is simultaneously the authority-log
//! writer key, and whoever holds it can mint arbitrary authorize and revoke events — so the
//! grant's narrowness is illusory.
//!
//! N1 has no objection to such a history: the authorize event is admissible, its signer really
//! does hold authority, and the fold yields a live grant. **GEN-A is deliberately stricter than
//! N1 validity here.** Both [`incept_subject_context_v1`] and
//! [`verify_subject_context_genesis_v1`] enforce it, the latter independently, because a bundle
//! may be assembled by anyone.
//!
//! This is the GEN-A *bootstrap* rule. It says nothing about whether some future profile may let
//! a Subject authorize a device that holds establishment authority under different semantics.
//!
//! **Nor is it a current-authority statement.** Verification folds a *fresh* store holding only
//! the two genesis events, so a verified bundle says the device **was** authorized at position
//! 1, never that it is authorized **now**. A later revocation or rotation is simply not in the
//! bundle. A caller needing current device authority must consult the Subject's live authority
//! log through [`crate::authority_log::derive`], and must not substitute this result for it.
//!
//! # Layering
//!
//! GEN-A is an **outer binding protocol over N1**, not a change to N1. N1 receives 32 bytes and
//! is otherwise byte-for-byte untouched: no N1 wire format, canonical body, signature preimage
//! or digest rule is altered or extended here. The derivations below live in their own hash
//! domains ([`GEN_CONTEXT_DOMAIN`] and friends), distinct from every
//! [`crate::authority_log`] domain separator.
//!
//! # Derivation
//!
//! All framing is N1's, reused rather than reimplemented: `LP(x) := u32be(len(x)) || x`,
//! `b32(x) := 32 raw bytes`, integers big-endian, hash SHA-256.
//!
//! ```text
//! context_preimage :=
//!       LP(GEN_CONTEXT_DOMAIN)
//!    || u16be(GEN_CONTEXT_VERSION)
//!    || u8(context_kind)
//!    || LP(context_id_utf8)
//!    || b32(context_salt)
//!
//! context_nonce := SHA-256(context_preimage)
//! ```
//!
//! The nonce is then handed to N1 unchanged, and N1's own rules produce the Subject:
//! `SubjectId = event_id(inception body) = SHA-256(canonical inception body)`.
//!
//! # Why the salt is mandatory (invariant I4b)
//!
//! `docs/architecture/IDENTITY_SEMANTICS.md` §10/I11 requires a `ContextNonce` to be generated
//! independently with cryptographically secure randomness per context, prohibits **deterministic
//! derivation from a globally stable public value**, and admits an alternative generation method
//! only with an *explicit unlinkability argument*. This is that argument.
//!
//! 1. The banned construction is `nonce = H(context_id)`: the context id is globally stable and
//!    public, so every human in one governance domain would publish the **same** nonce in their
//!    inception body — a free cross-subject grouping handle visible to anyone who sees two
//!    inception bodies.
//! 2. [`ContextSalt`] is 32 bytes drawn from the operating system CSPRNG, once, per
//!    (Subject, context). It is the *only* varying input, and it carries full entropy.
//! 3. With the public prefix fixed, `salt |-> SHA-256(prefix || salt)` over a uniform 32-byte
//!    salt is computationally indistinguishable from a uniform 32-byte draw. Two Subjects in the
//!    same context therefore publish independent, unlinkable nonces, which is exactly the
//!    property the direct-draw rule exists to guarantee.
//! 4. Deriving rather than drawing buys one thing the direct draw cannot: a client that retained
//!    its salt can **reproduce** the nonce after a restart, without storing the nonce separately
//!    and without weakening (3).
//! 5. An observer holding only an inception body sees the nonce alone. Recovering `context_id`
//!    from it requires guessing the 32-byte salt, so the nonce still names nothing and leaks no
//!    context.
//!
//! "Never reused" is preserved in the sense the rule means it: a distinct intended Subject or a
//! distinct context draws a distinct salt and therefore a distinct nonce. Reproducing the *same*
//! nonce for the *same* intended Subject during recovery is the goal, not a violation.
//!
//! # One Subject per context is a client invariant, not a protocol guarantee
//!
//! There is no registrar, so no protocol oracle can prove a person did not deliberately create a
//! second Subject with a second salt in the same context. Clients MUST refuse a second fresh
//! genesis for an already-indexed context unless a separately specified recovery or migration
//! flow authorizes it. **Do not read global uniqueness out of GEN-A.**
//!
//! # Determinism requires the whole continuity configuration
//!
//! N1's inception body commits `next_commitment`, which is folded from the continuity root's
//! establishment plan and horizon. So the retry/recovery invariant is stronger than "same salt":
//!
//! ```text
//! same continuity secret + same plan/horizon + same context descriptor + same salt
//!     -> same ContextNonce -> same canonical inception body -> same SubjectId
//! ```
//!
//! A client that reconstructs a *different* plan has not reconstructed the same Subject, and
//! must not claim continuity. GEN-A does not own the plan; the caller supplies a configured
//! [`ContinuityRoot`], and N7 still owns the general recovery protocol.
//!
//! # Transport is deliberately out of scope
//!
//! [`SubjectContextGenesisV1`] is an in-memory verification bundle with **no** serde impl and no
//! wire encoding. That is intentional for this slice: it makes "serde/JSON bytes are not the
//! cryptographic identity of an N1 event" structurally true rather than merely tested. A strict
//! deterministic container may be specified separately; verification will still consume the
//! canonical N1 bytes carried here, never a re-serialization of decoded fields.

use thiserror::Error;

use crate::authority_log::encoding::Writer;
use crate::authority_log::{
    admissible_bytes, authorize_event, derive, sha256, AdmissionError, AuthorityBody,
    AuthorityState, AuthorityStore, AuthorityView, CapabilitySet, ConstructError, ContextNonce,
    ContinuityRoot, DeviceCapability, EventId, PrincipalKey, SubjectId, WitnessSignature,
};

/// Domain separator for the GEN context-nonce derivation.
///
/// Length-prefixed and distinct from every `authority_log` separator, so a GEN preimage can
/// never be reinterpreted as an N1 body, signature, commitment or KDF preimage.
pub const GEN_CONTEXT_DOMAIN: &[u8] = b"icn.gen.subject-context";

/// Version of the context-nonce derivation. Frozen for v1.
pub const GEN_CONTEXT_VERSION: u16 = 1;

/// Domain separator for [`SubjectContextRef`].
pub const SUBJECT_CONTEXT_REF_DOMAIN: &[u8] = b"icn.gen.subject-context-ref";

/// Version of the subject-context reference derivation. Frozen for v1.
pub const SUBJECT_CONTEXT_REF_VERSION: u16 = 1;

/// Domain separator for [`InitialDeviceBindingRef`].
pub const INITIAL_DEVICE_BINDING_REF_DOMAIN: &[u8] = b"icn.gen.initial-device-binding-ref";

/// Version of the initial-device-binding reference derivation. Frozen for v1.
pub const INITIAL_DEVICE_BINDING_REF_VERSION: u16 = 1;

/// Version tag carried by [`SubjectContextGenesisV1`]. Verification rejects any other value.
pub const SUBJECT_CONTEXT_GENESIS_VERSION: u16 = 1;

/// The position of the initial device authorization. Inception is position `0`.
const INITIAL_AUTHORIZE_POSITION: u64 = 1;

/// The kind of context a Subject is scoped to.
///
/// Exactly one kind is defined in v1. This is **not** a claim that a governance domain is the
/// final institution or entity identifier: it is the concrete runtime decision space ICN has
/// today. A later GEN version may define further kinds without disturbing this one, because the
/// kind tag is inside every preimage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubjectContextKind {
    /// The context is an ICN governance domain, named by the UTF-8 bytes of its identifier.
    GovernanceDomainV1,
}

impl SubjectContextKind {
    /// The wire tag. Never renumber these.
    pub const fn tag(self) -> u8 {
        match self {
            SubjectContextKind::GovernanceDomainV1 => 0x01,
        }
    }
}

/// Fresh public randomness binding one Subject to one context.
///
/// **Public, not secret** — it travels in the genesis evidence. It is nonetheless the entire
/// entropy source of the derived nonce, so it must come from a CSPRNG and must be retained by
/// the person's client for recovery. It is never a global identifier and must not be indexed as
/// one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextSalt([u8; 32]);

impl ContextSalt {
    /// Wrap raw salt bytes — for recovery from retained material and for fixed test vectors.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        ContextSalt(bytes)
    }

    /// The raw salt bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Draw a fresh salt from the operating-system CSPRNG.
    ///
    /// This is the generation site invariant I4b governs: the freshness of these bytes is what
    /// makes the derived nonce unlinkable across Subjects sharing a context.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut bytes);
        ContextSalt(bytes)
    }
}

impl std::fmt::Debug for ContextSalt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Public data, so rendering it is safe; hex keeps it greppable against vectors.
        write!(f, "ContextSalt({})", hex::encode(self.0))
    }
}

/// A witness-independent reference to **one Subject's context genesis**.
///
/// This is the basis a later recognition fact should name. It identifies *which context-bound
/// Subject inception* is being referred to, and deliberately does not depend on the initial
/// device grant — so replacing or revoking a device never disturbs it. Re-signing the inception
/// body does not change it either, because witnesses are outside the preimage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubjectContextRef([u8; 32]);

impl SubjectContextRef {
    /// Wrap raw reference bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        SubjectContextRef(bytes)
    }

    /// The raw reference bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A witness-independent reference to the **Alpha bootstrap device-binding fact**.
///
/// Separate from [`SubjectContextRef`] on purpose: a device grant is revocable state, so
/// anchoring anything durable to it would anchor to something designed to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InitialDeviceBindingRef([u8; 32]);

impl InitialDeviceBindingRef {
    /// Wrap raw reference bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        InitialDeviceBindingRef(bytes)
    }

    /// The raw reference bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The named context a Subject is being incepted for, together with its fresh salt.
///
/// The context id is carried as validated bytes rather than as a governance type on purpose:
/// `icn-identity` is a kernel-class crate under `scripts/firewall-taxonomy.toml` and must not
/// depend on the domain crate that owns `GovernanceDomainId` — which already depends on this
/// one. The *byte* contract is unchanged: for [`SubjectContextKind::GovernanceDomainV1`] the
/// context id is exactly the UTF-8 bytes of `GovernanceDomainId.0`, with no case folding, no
/// Unicode normalization and no display-name substitution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectContextDescriptor {
    kind: SubjectContextKind,
    context_id: String,
    salt: ContextSalt,
}

impl SubjectContextDescriptor {
    /// Describe a governance-domain context.
    ///
    /// Rejects an empty context id at this boundary even though the legacy `GovernanceDomainId`
    /// newtype does not enforce that invariant: an empty context is not a context, and admitting
    /// one would let an unnamed scope masquerade as a named one.
    pub fn governance_domain_v1(
        context_id: &str,
        salt: ContextSalt,
    ) -> Result<Self, SubjectContextError> {
        if context_id.is_empty() {
            return Err(SubjectContextError::EmptyContextId);
        }
        Ok(SubjectContextDescriptor {
            kind: SubjectContextKind::GovernanceDomainV1,
            context_id: context_id.to_string(),
            salt,
        })
    }

    /// The context kind.
    pub fn kind(&self) -> SubjectContextKind {
        self.kind
    }

    /// The context id, as the exact string whose UTF-8 bytes enter the preimage.
    pub fn context_id(&self) -> &str {
        &self.context_id
    }

    /// The context salt.
    pub fn salt(&self) -> ContextSalt {
        self.salt
    }

    /// The canonical GEN context preimage. Exposed so vectors can pin the exact bytes.
    pub fn context_preimage(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(GEN_CONTEXT_DOMAIN);
        w.u16(GEN_CONTEXT_VERSION);
        w.u8(self.kind.tag());
        w.lp(self.context_id.as_bytes());
        w.b32(self.salt.as_bytes());
        w.finish()
    }

    /// The N1 context nonce this descriptor derives.
    pub fn context_nonce(&self) -> ContextNonce {
        ContextNonce::from_bytes(sha256(&self.context_preimage()))
    }

    /// The witness-independent reference for a Subject incepted under this descriptor.
    pub fn subject_context_ref(&self, inception_event_id: EventId) -> SubjectContextRef {
        let mut w = Writer::new();
        w.lp(SUBJECT_CONTEXT_REF_DOMAIN);
        w.u16(SUBJECT_CONTEXT_REF_VERSION);
        w.u8(self.kind.tag());
        w.lp(self.context_id.as_bytes());
        w.b32(self.salt.as_bytes());
        w.b32(inception_event_id.as_bytes());
        SubjectContextRef(sha256(&w.finish()))
    }
}

/// The reference identifying the initial device-binding fact.
pub fn initial_device_binding_ref(
    subject_context_ref: SubjectContextRef,
    initial_authorize_event_id: EventId,
) -> InitialDeviceBindingRef {
    let mut w = Writer::new();
    w.lp(INITIAL_DEVICE_BINDING_REF_DOMAIN);
    w.u16(INITIAL_DEVICE_BINDING_REF_VERSION);
    w.b32(subject_context_ref.as_bytes());
    w.b32(initial_authorize_event_id.as_bytes());
    InitialDeviceBindingRef(sha256(&w.finish()))
}

/// The capability set the Alpha genesis profile grants the initial device.
///
/// Exactly `{Sign, Present}`. `Encrypt` is not granted because nothing in this profile addresses
/// content to the Subject yet, and `Recover` is not granted because an app-layer label named
/// `Recover` must never be mistaken for N1 establishment authority.
pub fn alpha_initial_device_capabilities() -> CapabilitySet {
    CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present])
}

/// Why a Subject-context genesis could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SubjectContextError {
    /// The context id was empty.
    #[error("context id must not be empty")]
    EmptyContextId,
    /// The supplied continuity root does not carry this descriptor's derived nonce.
    #[error("continuity root's context nonce does not match the descriptor's derived nonce")]
    ContextNonceMismatch,
    /// The initial device principal is the Subject's own generation-0 establishment authority.
    #[error("initial device principal must not be the generation-0 establishment authority")]
    InitialDeviceIsAuthority,
    /// N1 refused to construct the inception body.
    #[error("N1 inception construction failed: {0}")]
    Construct(#[from] ConstructError),
}

/// Why a [`SubjectContextGenesisV1`] bundle was rejected.
///
/// Every variant is a refusal. There is no partial success and no "verified except" state.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GenesisVerifyError {
    /// The bundle's version tag is not [`SUBJECT_CONTEXT_GENESIS_VERSION`].
    #[error("unsupported subject-context genesis version {0}")]
    UnsupportedVersion(u16),
    /// The bundle's context kind is not the one the verifier was asked to check.
    #[error("bundle context kind does not match the claimed context kind")]
    ContextKindMismatch,
    /// The bundle's context id is empty.
    #[error("context id must not be empty")]
    EmptyContextId,
    /// The bundle's context id is not byte-identical to the claimed one.
    #[error("bundle context id is not byte-identical to the claimed context id")]
    ContextIdMismatch,
    /// The inception bytes/witness were not admissible under N1.
    #[error("inception event is not admissible: {0}")]
    InceptionNotAdmissible(AdmissionError),
    /// The authorize bytes/witness were not admissible under N1.
    #[error("authorize event is not admissible: {0}")]
    AuthorizeNotAdmissible(AdmissionError),
    /// The first body was not an N1 inception body.
    #[error("first event is not an inception body")]
    NotAnInception,
    /// The inception body's context nonce is not the one this context derives.
    #[error("inception context nonce is not the nonce derived from the claimed context")]
    ContextNonceMismatch,
    /// The second body was not an N1 authorize body.
    #[error("second event is not an authorize body")]
    NotAnAuthorize,
    /// The authorize body names a different Subject.
    #[error("authorize event names a different subject")]
    AuthorizeSubjectMismatch,
    /// The authorize body is not at position 1.
    #[error("authorize event is at position {0}, expected {INITIAL_AUTHORIZE_POSITION}")]
    AuthorizePositionMismatch(u64),
    /// The authorize body's parent is not the inception event.
    #[error("authorize event's prev_digest is not the inception event id")]
    AuthorizeParentMismatch,
    /// The granted capabilities are not exactly the Alpha profile's set.
    #[error("authorize event does not grant exactly the Alpha initial-device capabilities")]
    CapabilityMismatch,
    /// The grant carries a validity span; the Alpha profile requires none.
    #[error("authorize event carries a validity span; the Alpha genesis profile requires none")]
    UnexpectedValiditySpan,
    /// The authorized device is the inception's own establishment authority.
    ///
    /// N1 accepts such a history: the authorize event is admissible, its signer holds authority,
    /// and the derived grant is live. GEN-A refuses it anyway — see the module docs on the
    /// bootstrap separation invariant.
    #[error("initial device principal is the inception's establishment authority")]
    InitialDeviceIsAuthority,
    /// Derivation did not yield a clean live authority view.
    #[error("derived authority view is not live and clean for this subject")]
    AuthorityNotLive,
    /// The derived view did not place the device grant where the bundle claims it.
    #[error("derived authority state does not carry the expected initial device grant")]
    DeviceGrantMismatch,
}

/// A verification bundle for one context-scoped Subject genesis.
///
/// It carries the **exact canonical N1 bytes** of the two events plus the context material
/// needed to recompute the nonce. No decoded N1 field is duplicated as a second authoritative
/// copy — the Subject, the device and both event ids are *derived* during verification, never
/// asserted by the bundle. The private continuity secret and establishment plan are never
/// exported here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectContextGenesisV1 {
    /// Must equal [`SUBJECT_CONTEXT_GENESIS_VERSION`].
    pub version: u16,
    /// The context kind.
    pub context_kind: SubjectContextKind,
    /// The context id, whose UTF-8 bytes enter the preimage.
    pub context_id: String,
    /// The public salt.
    pub context_salt: ContextSalt,
    /// `AuthorityBody::canonical_bytes()` of the N1 inception event.
    pub inception_body_bytes: Vec<u8>,
    /// A valid witness over the inception body.
    pub inception_witness: WitnessSignature,
    /// `AuthorityBody::canonical_bytes()` of the N1 initial authorize event.
    pub authorize_body_bytes: Vec<u8>,
    /// A valid witness over the authorize body.
    pub authorize_witness: WitnessSignature,
}

/// What a verified genesis establishes — and nothing more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSubjectContext {
    /// The Subject, derived from the inception body's own digest.
    pub subject: SubjectId,
    /// The initial device Principal, read from the derived authority state.
    pub device: PrincipalKey,
    /// The inception event id.
    pub inception_event_id: EventId,
    /// The initial authorize event id.
    pub initial_authorize_event_id: EventId,
    /// The stable, device-independent reference later recognition facts should name.
    pub subject_context_ref: SubjectContextRef,
    /// The reference naming this bootstrap device-binding fact.
    pub initial_device_binding_ref: InitialDeviceBindingRef,
    /// The derived authority state at the frontier.
    pub authority: AuthorityState,
    /// The first position with no authorized candidate.
    pub frontier: u64,
}

/// Construct a context-scoped Subject genesis through the existing N1 API.
///
/// `root` must already carry `descriptor.context_nonce()`; passing a root built with any other
/// nonce is refused rather than silently producing a Subject that does not belong to the
/// described context. GEN-A does not choose the continuity secret, plan or horizon — the caller
/// supplies a configured [`ContinuityRoot`], and the plan/horizon it was built with is part of
/// what must be retained to reproduce this Subject.
///
/// `device` is the initial device's **public** Principal. GEN-A deliberately does not take the
/// device's signing key: the authorize event is signed by the Subject's own generation-0
/// authority, so the device's secret is never needed here and must not be handed over. The
/// device generates its own key and never receives the continuity or root authority key either.
pub fn incept_subject_context_v1(
    descriptor: &SubjectContextDescriptor,
    root: &ContinuityRoot,
    device: PrincipalKey,
) -> Result<SubjectContextGenesisV1, SubjectContextError> {
    if root.context_nonce() != descriptor.context_nonce() {
        return Err(SubjectContextError::ContextNonceMismatch);
    }

    // The bootstrap device must not BE the Subject's establishment authority. A device grant
    // attenuates what a principal may do, but it cannot attenuate what that principal already
    // is: if the device principal is the generation-0 authority, then the "narrow" device
    // credential is also the authority-log writer key, and whoever holds it can mint authorize
    // and revoke events at will. That collapses device capability into establishment authority
    // — exactly the separation GEN-A exists to hold — so it is refused here rather than
    // produced and left for a verifier to catch.
    if root.authority_set(0).members().contains(&device) {
        return Err(SubjectContextError::InitialDeviceIsAuthority);
    }

    let inception = root.incept()?;
    let subject = inception.body.subject();
    let inception_event_id = inception.body.event_id();

    let authorize = authorize_event(
        &root.authority_signing_key(0),
        subject,
        INITIAL_AUTHORIZE_POSITION,
        inception_event_id,
        device,
        alpha_initial_device_capabilities(),
        None,
    );

    Ok(SubjectContextGenesisV1 {
        version: SUBJECT_CONTEXT_GENESIS_VERSION,
        context_kind: descriptor.kind(),
        context_id: descriptor.context_id().to_string(),
        context_salt: descriptor.salt(),
        inception_body_bytes: inception.body.canonical_bytes(),
        inception_witness: inception.signature,
        authorize_body_bytes: authorize.body.canonical_bytes(),
        authorize_witness: authorize.signature,
    })
}

/// Verify a genesis bundle against an independently supplied context claim.
///
/// The verifier reconstructs every semantic claim rather than trusting the bundle to assert it:
/// the nonce is recomputed from the context material, the Subject comes from N1's own digest
/// rule, the device comes from the derived authority state, and both reference hashes are
/// recomputed from body/event references with witnesses excluded.
///
/// `claimed_context_id` is supplied by the caller — typically `GovernanceDomainId.0` for the
/// domain the verifier actually cares about. Comparison is byte-exact: no normalization, no case
/// folding, no display-name substitution.
pub fn verify_subject_context_genesis_v1(
    bundle: &SubjectContextGenesisV1,
    claimed_context_kind: SubjectContextKind,
    claimed_context_id: &str,
) -> Result<VerifiedSubjectContext, GenesisVerifyError> {
    // 1-2. Version, kind and a byte-exact, non-empty context id.
    if bundle.version != SUBJECT_CONTEXT_GENESIS_VERSION {
        return Err(GenesisVerifyError::UnsupportedVersion(bundle.version));
    }
    // Unreachable while `SubjectContextKind` has a single variant, and therefore deliberately
    // untested: there is no second kind to mismatch against. It is kept rather than deferred so
    // that adding a v2 context kind cannot silently let a bundle of one kind satisfy a claim
    // about another — the check would then already be in place at the boundary that needs it.
    if bundle.context_kind != claimed_context_kind {
        return Err(GenesisVerifyError::ContextKindMismatch);
    }
    if bundle.context_id.is_empty() {
        return Err(GenesisVerifyError::EmptyContextId);
    }
    if bundle.context_id.as_bytes() != claimed_context_id.as_bytes() {
        return Err(GenesisVerifyError::ContextIdMismatch);
    }

    // 3. Recompute the nonce from the context material the bundle carries.
    let descriptor = SubjectContextDescriptor {
        kind: bundle.context_kind,
        context_id: bundle.context_id.clone(),
        salt: bundle.context_salt,
    };
    let expected_nonce = descriptor.context_nonce();

    // 4-5. Admit the inception through N1's existing state-independent path, then require it to
    // be an inception whose nonce is the one this context derives. Admission proves only that
    // some key signed these bytes; the nonce check is what binds those bytes to this context.
    let inception_body = admissible_bytes(&bundle.inception_body_bytes, &bundle.inception_witness)
        .map_err(GenesisVerifyError::InceptionNotAdmissible)?;
    let inception = match &inception_body {
        AuthorityBody::Inception(body) => body,
        _ => return Err(GenesisVerifyError::NotAnInception),
    };
    if inception.context_nonce != expected_nonce {
        return Err(GenesisVerifyError::ContextNonceMismatch);
    }

    // 6. The Subject is N1's own digest rule; GEN-A does not invent an identifier.
    let subject = inception_body.subject();
    let inception_event_id = inception_body.event_id();

    // 7-8. Admit the authorize event and pin every field the Alpha profile fixes.
    let authorize_body = admissible_bytes(&bundle.authorize_body_bytes, &bundle.authorize_witness)
        .map_err(GenesisVerifyError::AuthorizeNotAdmissible)?;
    let authorize = match &authorize_body {
        AuthorityBody::Authorize(body) => body,
        _ => return Err(GenesisVerifyError::NotAnAuthorize),
    };
    if authorize.header.subject != subject {
        return Err(GenesisVerifyError::AuthorizeSubjectMismatch);
    }
    if authorize.header.position != INITIAL_AUTHORIZE_POSITION {
        return Err(GenesisVerifyError::AuthorizePositionMismatch(
            authorize.header.position,
        ));
    }
    if authorize.header.prev_digest != inception_event_id {
        return Err(GenesisVerifyError::AuthorizeParentMismatch);
    }
    if authorize.capabilities != alpha_initial_device_capabilities() {
        return Err(GenesisVerifyError::CapabilityMismatch);
    }
    if authorize.validity.is_some() {
        return Err(GenesisVerifyError::UnexpectedValiditySpan);
    }
    let device = authorize.device;

    // The GEN-A bootstrap separation invariant, enforced independently of the constructor
    // because a bundle may be assembled by anyone.
    //
    // This is the LOAD-BEARING check: it reads the authority set out of the *decoded inception
    // body*, never from a bundle-supplied assertion, and it holds before any derivation runs.
    // N1 itself has no objection to this configuration — the authorize event is admissible, the
    // signer genuinely holds authority, and the fold yields a live grant — so nothing downstream
    // of N1 would reject it. GEN-A is deliberately stricter than N1 validity here: the Alpha
    // profile requires the bootstrap device and the establishment authority to be distinct
    // principals, because one key that is both is a device credential that can rewrite the
    // authority log.
    if inception.initial_authority.members().contains(&device) {
        return Err(GenesisVerifyError::InitialDeviceIsAuthority);
    }

    let initial_authorize_event_id = authorize_body.event_id();

    // 9-10. Fold both events through N1's own derivation. This is what distinguishes an
    // *admissible* authorize event from an *authorized* one: admission would accept a grant
    // signed by any key at all, and only the derived view rejects one the Subject never made.
    let mut store = AuthorityStore::new();
    store
        .ingest(&crate::authority_log::SignedAuthorityEvent::new(
            inception_body.clone(),
            bundle.inception_witness,
        ))
        .map_err(GenesisVerifyError::InceptionNotAdmissible)?;
    store
        .ingest(&crate::authority_log::SignedAuthorityEvent::new(
            authorize_body.clone(),
            bundle.authorize_witness,
        ))
        .map_err(GenesisVerifyError::AuthorizeNotAdmissible)?;

    let view = derive(subject, &store);
    let (authority, frontier) = match view {
        AuthorityView::Live { state, frontier } => (state, frontier),
        _ => return Err(GenesisVerifyError::AuthorityNotLive),
    };
    // Exactly two events were folded, so the frontier is the position after the grant. A lower
    // frontier would mean the grant was admissible but never selected into the chain.
    if frontier != INITIAL_AUTHORIZE_POSITION + 1 {
        return Err(GenesisVerifyError::AuthorityNotLive);
    }
    let grant = authority
        .devices
        .get(&device)
        .ok_or(GenesisVerifyError::DeviceGrantMismatch)?;
    if grant.capabilities != alpha_initial_device_capabilities()
        || grant.validity.is_some()
        || grant.granted_at != INITIAL_AUTHORIZE_POSITION
        || authority.devices.len() != 1
    {
        return Err(GenesisVerifyError::DeviceGrantMismatch);
    }
    // Defence in depth over the derived writer set. For a two-event genesis bundle this is
    // *equivalent* to the check above and cannot fire on its own: no establishment event can
    // occupy position 1, so the fold never advances past generation 0 and the derived authority
    // set is still the inception's `initial_authority`. It is kept because it states the
    // invariant against the structure a reader actually cares about — "the device is not a log
    // writer in the derived history" — and would keep holding if a later profile ever admitted a
    // bundle whose prefix contains a rotation.
    if authority.authority.contains(&device) {
        return Err(GenesisVerifyError::InitialDeviceIsAuthority);
    }

    // 11. Both references are recomputed from semantic body/event references. Witness bytes are
    // not in either preimage, so re-signing either body leaves both references unchanged.
    let subject_context_ref = descriptor.subject_context_ref(inception_event_id);
    let initial_device_binding_ref =
        initial_device_binding_ref(subject_context_ref, initial_authorize_event_id);

    Ok(VerifiedSubjectContext {
        subject,
        device,
        inception_event_id,
        initial_authorize_event_id,
        subject_context_ref,
        initial_device_binding_ref,
        authority,
        frontier,
    })
}
