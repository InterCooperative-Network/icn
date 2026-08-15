//! `admissible(body, signature)` — the **state-independent** admission test (§9.2.1).
//!
//! This is a free function. It takes a body and a signature and nothing else: no store, no
//! frontier, no set of currently authorized devices, no arrival history, no receiver clock, no
//! other subject's log.
//!
//! State-independence is not a stylistic preference. It is what makes the admission filter
//! commute with the join — `filter(A) ∪ filter(B) = filter(A ∪ B)` — so that two replicas that
//! eventually learn the same facts hold the same durable set regardless of delivery order. The
//! moment admission consults local state, that identity fails and the durable layer stops being
//! a join.
//!
//! Admissibility proves only *"some key signed these bytes"*. Whether that key had **authority**
//! is a separate question answered inside the derived fold. An admissible body can be completely
//! unauthorized, and that is intentional: spam enters the durable set, is never selected by the
//! derived view, and therefore cannot manufacture duplicity.

use ed25519_dalek::Signature;
use thiserror::Error;

use super::body::AuthorityBody;
use super::encoding::CodecError;
use super::store::WitnessSignature;
use super::MAX_POSITION;

/// Why a body/signature pair was not admitted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdmissionError {
    /// The canonical encoding did not parse, or was not canonical.
    #[error("canonical encoding rejected: {0}")]
    Codec(#[from] CodecError),
    /// The body did not round-trip through its own canonical encoding.
    ///
    /// This is load-bearing, not cosmetic. Some struct fields are public, so a body can be built
    /// by struct literal without passing through a validating constructor — an inverted
    /// [`super::ValiditySpan`], for instance. Re-decoding is where such a body is caught, and it
    /// is also what would catch a future change that made the encoding lossy or ambiguous rather
    /// than letting one logical body split into two durable elements.
    #[error("body does not round-trip through its canonical encoding")]
    NonCanonicalEncoding,
    /// The signature does not verify under the signer key carried inline in the body.
    #[error("signature does not verify under the body's inline signer key")]
    SignatureInvalid,
    /// The position exceeds the absolute protocol bound.
    #[error("position {position} exceeds MAX_POSITION ({max})")]
    PositionOutOfRange {
        /// The offending position.
        position: u64,
        /// The absolute bound.
        max: u64,
    },
}

/// Decide admissibility from the bytes alone.
///
/// The four rules, in the order §9.2.1 states them:
///
/// 1. the canonical encoding parses, and the version and domain separator are as specified;
/// 2. the signature verifies under `body.signer_key`, carried inline — this proves only that
///    *some* key signed this body;
/// 3. for an inception body `σ == event_id(b)`, which holds **by representation** because an
///    inception body carries no subject field and [`AuthorityBody::subject`] returns its own
///    digest; every other kind names its subject explicitly in the header;
/// 4. `position ≤ MAX_POSITION`, an **absolute** protocol constant — deliberately not
///    `local_frontier + C`, which would make this function state-dependent.
pub fn admissible(
    body: &AuthorityBody,
    signature: &WitnessSignature,
) -> Result<(), AdmissionError> {
    // Rule 1. Re-decode the body's own canonical bytes. This enforces every strictness rule the
    // decoder applies — ordered non-empty sets, position 0 reserved for inception, a validity
    // span that is not inverted, an inception signer inside its own initial authority set — on
    // bodies that reached here without passing through a validating constructor, and it keeps the
    // encode/decode pair a bijection.
    let canonical = body.canonical_bytes();
    let decoded = AuthorityBody::decode(&canonical)?;
    if &decoded != body || decoded.canonical_bytes() != canonical {
        return Err(AdmissionError::NonCanonicalEncoding);
    }

    // Rule 4. An absolute bound, checked before any expensive work.
    let position = body.position();
    if position > MAX_POSITION {
        return Err(AdmissionError::PositionOutOfRange {
            position,
            max: MAX_POSITION,
        });
    }

    // Rule 2. Verify under the inline signer key. This says nothing about authority.
    let signer = body.signer();
    let sig = Signature::from_bytes(signature.as_bytes());
    signer
        .verifying_key()
        .verify_strict(&body.signature_preimage(), &sig)
        .map_err(|_| AdmissionError::SignatureInvalid)?;

    // Rule 3 is structural: see the doc comment above. Nothing to check at runtime, and nothing a
    // hostile encoder can vary.
    Ok(())
}

/// Decode a wire body and decide admissibility in one step.
///
/// This is the path an untrusted peer's bytes take. It is still state-independent.
pub fn admissible_bytes(
    canonical: &[u8],
    signature: &WitnessSignature,
) -> Result<AuthorityBody, AdmissionError> {
    let body = AuthorityBody::decode(canonical)?;
    admissible(&body, signature)?;
    Ok(body)
}
