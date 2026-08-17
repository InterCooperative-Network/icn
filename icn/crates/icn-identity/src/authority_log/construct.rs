//! Deterministic construction: genesis, the pre-rotation chain, and event authoring.
//!
//! **No randomness enters establishment construction.** Every establishment body is a pure
//! function of `(continuity root, context nonce, generation, subject, position, prev_digest)`, so
//! a device that crashes after signing and retries re-derives byte-identical bytes and the same
//! `event_id`. That is not a convenience: a non-deterministic construction lets one honest signer
//! author two authorized establishment bodies at one position and halt the subject permanently,
//! with no attacker present (§9.2.1 constraint 2).

use ed25519_dalek::{Signer, SigningKey};
use zeroize::Zeroizing;

use super::body::{
    AuthorityBody, AuthorizeBody, CapabilitySet, Commitment, ContextNonce, EstablishmentBody,
    EventHeader, EventId, InceptionBody, PrincipalKey, PrincipalSet, RevokeBody, SubjectId,
    ValiditySpan,
};
use super::derive::{commitment_for, AuthorityState};
use super::encoding::{CodecError, Writer};
use super::store::{SignedAuthorityEvent, WitnessSignature};
use super::{kind, sha256, KDF_DOMAIN};

/// Which establishment kind a generation is armed for.
///
/// The kind is inside the commitment, so **exactly one** is armed per generation. Path (a) and
/// path (b) can never both be armed at the same position, which is what stops a phrase-holder
/// racing a guardian quorum (§12.2, §9.2.1 constraint 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EstablishmentKind {
    /// A planned rotation. Device grants are preserved.
    Rotate,
    /// A recovery. Device grants are cleared — replace, not extend.
    Recover,
}

impl EstablishmentKind {
    fn kind_tag(self) -> u8 {
        match self {
            EstablishmentKind::Rotate => kind::ROTATE,
            EstablishmentKind::Recover => kind::RECOVER,
        }
    }
}

/// Why a construction attempt was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConstructError {
    /// The requested generation is outside the armed horizon.
    #[error("establishment generation {generation} is outside the armed horizon {horizon}")]
    GenerationOutOfRange {
        /// The requested generation.
        generation: u64,
        /// The last armed generation.
        horizon: u64,
    },
    /// Generation `0` is inception, not an establishment transition.
    #[error("generation 0 is the inception body, not an establishment transition")]
    GenerationZero,
    /// The state's armed commitment does not match this root's chain.
    ///
    /// Returned rather than emitting a body that would fail `authorized` anyway — refusing at
    /// construction time is the difference between a clear error and a silently useless event.
    #[error(
        "state commitment does not match this continuity root's chain at generation {generation}"
    )]
    CommitmentMismatch {
        /// The generation whose commitment was expected.
        generation: u64,
    },
    /// The armed chain is exhausted: no further establishment is possible.
    #[error("the pre-rotation chain is exhausted; no further establishment is armed")]
    ChainExhausted,
    /// A canonical-encoding constraint was violated while building the body.
    #[error("canonical encoding rejected: {0}")]
    Codec(#[from] CodecError),
}

/// The private client-side secret from which a subject's whole authority chain derives.
///
/// **Never published, never sent.** In the architecture this is one entry in the person's
/// continuity root; here it is the per-subject material plus the context nonce.
pub struct ContinuityRoot {
    secret: Zeroizing<[u8; 32]>,
    context_nonce: ContextNonce,
    /// `plan[g - 1]` is the establishment kind armed for generation `g`.
    plan: Vec<EstablishmentKind>,
}

impl std::fmt::Debug for ContinuityRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never render the secret.
        f.debug_struct("ContinuityRoot")
            .field("context_nonce", &hex::encode(self.context_nonce.as_bytes()))
            .field("horizon", &self.plan.len())
            .finish_non_exhaustive()
    }
}

impl ContinuityRoot {
    /// A root arming `horizon` generations, all of them [`EstablishmentKind::Rotate`].
    pub fn new(secret: [u8; 32], context_nonce: ContextNonce, horizon: usize) -> Self {
        ContinuityRoot {
            secret: Zeroizing::new(secret),
            context_nonce,
            plan: vec![EstablishmentKind::Rotate; horizon],
        }
    }

    /// A root arming one explicitly chosen establishment kind per generation.
    pub fn with_plan(
        secret: [u8; 32],
        context_nonce: ContextNonce,
        plan: Vec<EstablishmentKind>,
    ) -> Self {
        ContinuityRoot {
            secret: Zeroizing::new(secret),
            context_nonce,
            plan,
        }
    }

    /// The last armed establishment generation.
    pub fn horizon(&self) -> u64 {
        self.plan.len() as u64
    }

    /// The context nonce carried in the inception body.
    pub fn context_nonce(&self) -> ContextNonce {
        self.context_nonce
    }

    /// The establishment kind armed for `generation`, if any.
    pub fn kind_at(&self, generation: u64) -> Option<EstablishmentKind> {
        if generation == 0 || generation > self.horizon() {
            return None;
        }
        self.plan.get((generation - 1) as usize).copied()
    }

    /// Derive the authority seed for `generation`.
    ///
    /// ```text
    /// seed = SHA-256(LP(KDF_DOMAIN) || b32(context_nonce) || u64be(generation) || b32(root))
    /// ```
    ///
    /// The root is placed **last** so the construction is not vulnerable to length extension, and
    /// the input is a fixed-length, domain-separated preimage.
    ///
    /// The inputs are the pre-subject **context nonce** and the generation — never the
    /// `SubjectId`. That is what breaks the genesis fixed point: the inception body contains
    /// `A_0`, and `SubjectId = H(inception body)`, so deriving `A_0` from `SubjectId` would
    /// require solving `S = H(inception(KDF(root, S), …))`, which offline genesis cannot do.
    fn authority_seed(&self, generation: u64) -> Zeroizing<[u8; 32]> {
        let mut w = Writer::new();
        w.lp(KDF_DOMAIN);
        w.b32(self.context_nonce.as_bytes());
        w.u64(generation);
        w.b32(&self.secret);
        let preimage = Zeroizing::new(w.finish());
        Zeroizing::new(sha256(&preimage))
    }

    /// The signing key for `generation`. Secret material — treat accordingly.
    pub fn authority_signing_key(&self, generation: u64) -> SigningKey {
        SigningKey::from_bytes(&self.authority_seed(generation))
    }

    /// The authority set `A_g`.
    ///
    /// One key per generation, which keeps the chain fully derivable from the root and preserves
    /// the single-writer property. Multi-key generations would need the whole set to be
    /// derivable, or the commitment could no longer pin the body.
    pub fn authority_set(&self, generation: u64) -> PrincipalSet {
        let key = self.authority_signing_key(generation);
        PrincipalSet::single(PrincipalKey::from_signing_key(&key))
    }

    /// The pre-rotation commitment `C_g`.
    ///
    /// Computed by folding the armed chain backwards from the horizon:
    /// `C_{horizon+1} = TERMINAL`, and `C_g = H(kind_g ‖ PS(A_g) ‖ C_{g+1})`. Beyond the horizon
    /// the commitment is [`Commitment::TERMINAL`], which authorizes nothing.
    pub fn commitment(&self, generation: u64) -> Commitment {
        let horizon = self.horizon();
        if generation == 0 || generation > horizon {
            return Commitment::TERMINAL;
        }
        let mut commitment = Commitment::TERMINAL;
        let mut g = horizon;
        while g >= generation {
            let armed = match self.kind_at(g) {
                Some(k) => k,
                // Unreachable for `1 ..= horizon`; stop rather than panic.
                None => return Commitment::TERMINAL,
            };
            commitment = commitment_for(armed.kind_tag(), &self.authority_set(g), &commitment);
            if g == generation {
                break;
            }
            g -= 1;
        }
        commitment
    }

    /// Build and sign the inception body.
    ///
    /// Takes no subject argument — it cannot, because the subject does not exist until this body
    /// has been encoded and hashed.
    pub fn incept(&self) -> Result<SignedAuthorityEvent, ConstructError> {
        let initial_authority = self.authority_set(0);
        let signer = initial_authority
            .canonical_signer()
            .ok_or(CodecError::EmptySet {
                field: "initial_authority",
            })?;
        let body = AuthorityBody::Inception(InceptionBody {
            signer,
            context_nonce: self.context_nonce,
            initial_authority,
            next_commitment: self.commitment(1),
        });
        Ok(sign_body(body, &self.authority_signing_key(0)))
    }

    /// The subject identifier this root produces.
    pub fn subject_id(&self) -> Result<SubjectId, ConstructError> {
        Ok(self.incept()?.body.subject())
    }

    /// Build and sign the establishment body for `generation`.
    ///
    /// Deterministic: calling this twice with the same arguments returns byte-identical bodies
    /// and the same `event_id`. There is no entropy parameter, and there is no way to add one
    /// without changing this signature.
    pub fn establish(
        &self,
        generation: u64,
        subject: SubjectId,
        position: u64,
        prev_digest: EventId,
    ) -> Result<SignedAuthorityEvent, ConstructError> {
        if generation == 0 {
            return Err(ConstructError::GenerationZero);
        }
        let armed = self
            .kind_at(generation)
            .ok_or(ConstructError::GenerationOutOfRange {
                generation,
                horizon: self.horizon(),
            })?;

        let revealed_authority = self.authority_set(generation);
        let signer = revealed_authority
            .canonical_signer()
            .ok_or(CodecError::EmptySet {
                field: "revealed_authority",
            })?;
        let establishment = EstablishmentBody {
            header: EventHeader {
                subject,
                position,
                prev_digest,
                signer,
            },
            revealed_authority,
            next_commitment: self.commitment(generation + 1),
        };
        let body = match armed {
            EstablishmentKind::Rotate => AuthorityBody::Rotate(establishment),
            EstablishmentKind::Recover => AuthorityBody::Recover(establishment),
        };
        Ok(sign_body(body, &self.authority_signing_key(generation)))
    }

    /// Build the establishment body that `state` has armed, refusing if the state's commitment
    /// does not belong to this root's chain.
    ///
    /// This is the safe entry point: it derives the generation from the state instead of trusting
    /// the caller, and refuses at construction time rather than emitting a body that could only
    /// fail candidate authorization in the derived fold.
    pub fn establish_from_state(
        &self,
        state: &AuthorityState,
        subject: SubjectId,
        position: u64,
        prev_digest: EventId,
    ) -> Result<SignedAuthorityEvent, ConstructError> {
        if state.next_commitment.is_terminal() {
            return Err(ConstructError::ChainExhausted);
        }
        let generation = state.generation.saturating_add(1);
        if self.commitment(generation) != state.next_commitment {
            return Err(ConstructError::CommitmentMismatch { generation });
        }
        self.establish(generation, subject, position, prev_digest)
    }
}

/// Sign a body with `key`, producing a durable-layer event.
///
/// Ed25519 signing is deterministic (RFC 8032), so an honest retry produces the same witness as
/// well as the same body — but nothing in the model depends on that: two different valid
/// signatures over one body would simply be two witnesses of one body.
pub fn sign_body(body: AuthorityBody, key: &SigningKey) -> SignedAuthorityEvent {
    let signature = key.sign(&body.signature_preimage());
    SignedAuthorityEvent::new(body, WitnessSignature::from_bytes(signature.to_bytes()))
}

/// Build and sign an `authorize` body.
///
/// A device grant is **not** log-writing authority: nothing this event records can ever author an
/// authority event, and no capability it carries — including [`super::DeviceCapability::Recover`]
/// — makes any later event an establishment event.
#[allow(clippy::too_many_arguments)]
pub fn authorize_event(
    signer_key: &SigningKey,
    subject: SubjectId,
    position: u64,
    prev_digest: EventId,
    device: PrincipalKey,
    capabilities: CapabilitySet,
    validity: Option<ValiditySpan>,
) -> SignedAuthorityEvent {
    let body = AuthorityBody::Authorize(AuthorizeBody {
        header: EventHeader {
            subject,
            position,
            prev_digest,
            signer: PrincipalKey::from_signing_key(signer_key),
        },
        device,
        capabilities,
        validity,
    });
    sign_body(body, signer_key)
}

/// Build and sign a `revoke` body.
pub fn revoke_event(
    signer_key: &SigningKey,
    subject: SubjectId,
    position: u64,
    prev_digest: EventId,
    device: PrincipalKey,
) -> SignedAuthorityEvent {
    let body = AuthorityBody::Revoke(RevokeBody {
        header: EventHeader {
            subject,
            position,
            prev_digest,
            signer: PrincipalKey::from_signing_key(signer_key),
        },
        device,
    });
    sign_body(body, signer_key)
}
