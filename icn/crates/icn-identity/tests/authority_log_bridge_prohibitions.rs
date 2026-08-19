//! N2-E1 — the settled legacy-bridge prohibitions, as executable regression tests.
//!
//! These encode constraints that `docs/architecture/IDENTITY_SEMANTICS.md` already closed. They
//! add **no** bridge machinery: no evidence object, no proof format, no credential schema, no
//! legacy-DID resolver, no migration registry. The bridge-evidence object is N2-E2 and remains
//! blocked (§7.4). This file only makes the *negative* space enforceable, so that a later slice
//! that violates it fails a test instead of quietly widening the model.
//!
//! # What is enforced here, and what is not
//!
//! | Constraint | Contract | Enforcement reachable at this layer |
//! |---|---|---|
//! | A legacy key-derived `Did` must not silently become a human `SubjectId` | §7.2.1, **I1** | **Yes — structural.** Absence of `From` impls, plus the fact that a subject that is not `event_id` of an inception body derives to `Unknown`. |
//! | Signing bridge evidence is not enrollment | §7.2.3, **I5** | **Yes — behavioural.** A key that signs anything gains no standing; only an ordinary `authorize` event authorized by the current authority grants it. |
//! | No global or service-visible forward index `legacy Did -> subjects it seeded` | §7.2.7, **I10** | **Partly — structural.** The store's keyed lookup surface is subject-keyed and event-keyed only; there is no principal-keyed reverse direction. |
//! | Disclosure stays bounded to the relevant context | §7.2.4 | **Partly.** Derivation is context-local: one subject's fold reads only its own bodies. |
//! | Retention / erasability stay possible | §7.3 | **Partly — structural.** Subjects are separable: a store built without a subject's events derives that subject to `Unknown` and leaves every other subject intact. No global index has to be torn down first. |
//! | No unauthorized replication or export | §7.2 | **No — policy, not cryptography.** The contract says so explicitly: a holder of a whole store can copy, log, forward or correlate it. The enforcement point is storage and replication control, which does not exist yet. Claiming it here as a data-model property would be an overclaim, so this file does not claim it. |
//! | No mandatory global correlation between a person's contexts | §5 | **Partly — structural.** Nothing in the model links two subjects that share a principal; see [`two_subjects_sharing_one_legacy_principal_are_not_linked`]. |
//!
//! The honest summary: this is **library/type-level enforcement only**. It binds the N1
//! `authority_log` primitive. It does not bind a gateway, a replication path, or any storage
//! service, because none of those consume `authority_log` yet.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use std::marker::PhantomData;

use ed25519_dalek::Signer;

use authority_log_support::{authorize_at, principal, store_of, stranger, subject};
use icn_identity::authority_log::{
    authorize_event, derive, AuthorityView, PrincipalKey, SubjectId,
};
use icn_identity::Did;

// ---------------------------------------------------------------------------
// I1 — absence of conversions into `SubjectId`
// ---------------------------------------------------------------------------

/// Compile-time probe for "does `SubjectId: From<T>` exist?".
///
/// The blanket trait impl supplies `false` for every `T`; the inherent impl supplies `true`, but
/// only where the conversion actually exists. Inherent associated items shadow trait items during
/// path resolution, so [`ConversionProbe::EXISTS`] reads `true` exactly when the forbidden impl
/// has been added. This detects a *future* violation at compile time without requiring one now.
struct ConversionProbe<T>(PhantomData<T>);

trait ConversionAbsent {
    const EXISTS: bool = false;
}

impl<T> ConversionAbsent for ConversionProbe<T> {}

impl<T> ConversionProbe<T>
where
    SubjectId: From<T>,
{
    const EXISTS: bool = true;
}

/// The probe must be able to *see* a conversion, or every assertion below is vacuous.
///
/// `impl<T> From<T> for T` is in the standard library, so `SubjectId: From<SubjectId>` always
/// holds. If this control ever reads `false` the probe has stopped working and the negative
/// assertions mean nothing.
// The assertions below are deliberately constant: `EXISTS` is resolved at compile time,
// which is exactly what lets this detect a *future* forbidden impl. Constancy is the
// mechanism, so `assertions_on_constants` is not a defect here.
#[allow(clippy::assertions_on_constants)]
#[test]
fn conversion_probe_discriminates() {
    assert!(
        ConversionProbe::<SubjectId>::EXISTS,
        "probe control failed: it cannot detect the reflexive From impl, so the negative \
         assertions in this file would pass vacuously"
    );
}

/// §7.2.1 / I1 — a legacy key-derived `Did` must not silently become a human `SubjectId`.
// The assertions below are deliberately constant: `EXISTS` is resolved at compile time,
// which is exactly what lets this detect a *future* forbidden impl. Constancy is the
// mechanism, so `assertions_on_constants` is not a defect here.
#[allow(clippy::assertions_on_constants)]
#[test]
fn no_implicit_conversion_from_legacy_did_into_subject_id() {
    assert!(
        !ConversionProbe::<Did>::EXISTS,
        "a `From<Did> for SubjectId` impl would let a legacy key-derived DID silently become a \
         human subject, which IDENTITY_SEMANTICS §7.2.1 forbids"
    );
    assert!(
        !ConversionProbe::<PrincipalKey>::EXISTS,
        "a legacy key is a Principal (§7.2.3); a Principal is not a Subject"
    );
    assert!(
        !ConversionProbe::<[u8; 32]>::EXISTS,
        "raw 32 bytes must not convert into a subject: a principal key is also 32 bytes"
    );
}

/// The bytes of a legacy principal, wrapped as a `SubjectId`, name nothing.
///
/// `SubjectId::from_bytes` is deliberately available for wire decoding, so wrapping is *possible*.
/// This proves it is also *inert*: a subject is `event_id(inception body)`, so a subject with no
/// inception body derives to [`AuthorityView::Unknown`]. Wrapping is not a shortcut to standing.
#[test]
fn legacy_principal_bytes_wrapped_as_a_subject_derive_to_nothing() {
    let alpha = subject(0x11, 2);
    let (_, legacy) = stranger(0x77);

    let store = store_of(std::slice::from_ref(&alpha.inception));

    let masquerade = SubjectId::from_bytes(legacy.as_bytes());
    assert_eq!(
        derive(masquerade, &store),
        AuthorityView::Unknown,
        "a legacy key's bytes wrapped as a SubjectId must not resolve to any authority"
    );

    // The real subject still derives, so the store is not simply empty.
    assert!(
        matches!(derive(alpha.subject, &store), AuthorityView::Live { .. }),
        "control: the genuine subject must derive, or the assertion above is vacuous"
    );
}

/// A subject is the digest of its own inception body, never a key.
#[test]
fn a_subject_is_the_digest_of_an_inception_body_not_a_key() {
    let alpha = subject(0x12, 1);
    let (_, legacy) = stranger(0x78);

    assert_eq!(
        alpha.subject.as_bytes(),
        alpha.inception.body.event_id().as_bytes(),
        "sigma = event_id(inception body)"
    );
    assert_ne!(
        alpha.subject.as_bytes(),
        &legacy.as_bytes(),
        "a subject must never coincide with a principal key"
    );
}

// ---------------------------------------------------------------------------
// I5 — signing evidence is not enrollment
// ---------------------------------------------------------------------------

/// §7.2.3 / I5 — a legacy key that signs valid bridge evidence gains no standing.
///
/// The signature here stands in for bridge evidence. Its *contents* are N2-E2 and deliberately
/// undefined; what matters is that producing a valid signature over anything at all leaves the
/// signer outside both the authority set and the device grants.
#[test]
fn signing_evidence_does_not_enroll_the_legacy_key() {
    let alpha = subject(0x21, 2);
    let (legacy_key, legacy) = stranger(0x79);

    // The legacy key produces a genuine Ed25519 signature over an arbitrary payload.
    let evidence = legacy_key.sign(b"bridge evidence for a migrating person");
    assert!(
        legacy
            .verifying_key()
            .verify_strict(b"bridge evidence for a migrating person", &evidence)
            .is_ok(),
        "control: the evidence signature must actually verify, or this test proves nothing"
    );

    let store = store_of(std::slice::from_ref(&alpha.inception));
    let state = derive(alpha.subject, &store)
        .state()
        .expect("subject derives")
        .clone();

    assert!(
        !state.authority.contains(&legacy),
        "signing evidence must not place the legacy key in the authority set"
    );
    assert!(
        !state.devices.contains_key(&legacy),
        "signing evidence must not create a device grant (I5: evidence is not delegation)"
    );
}

/// The *only* route to standing is an ordinary `authorize` event, on the same terms as any other
/// device principal (§7.2.3). This is the positive control for the test above.
#[test]
fn only_an_ordinary_authorize_event_grants_the_legacy_key_standing() {
    let alpha = subject(0x22, 2);
    let (_, legacy) = stranger(0x7a);

    let store = store_of(std::slice::from_ref(&alpha.inception));
    let before = derive(alpha.subject, &store).state().unwrap().clone();
    assert!(!before.devices.contains_key(&legacy));

    // An authorize event signed by the *current authority* — not by the legacy key.
    let grant = authorize_at(&alpha, 0, 1, alpha.genesis(), legacy);
    let store = store_of(&[alpha.inception.clone(), grant]);
    let after = derive(alpha.subject, &store).state().unwrap().clone();

    assert!(
        after.devices.contains_key(&legacy),
        "an authorize event from the current authority is the ordinary enrollment route"
    );
    assert!(
        !after.authority.contains(&legacy),
        "a device grant is delegated authority, not membership of the authority set"
    );
}

/// A legacy key cannot enroll itself: an `authorize` event it signs is not authorized.
#[test]
fn the_legacy_key_cannot_authorize_itself() {
    let alpha = subject(0x23, 2);
    let (legacy_key, legacy) = stranger(0x7b);

    // Well-formed and correctly signed — but signed by a key outside the authority set.
    let self_grant = authorize_event(
        &legacy_key,
        alpha.subject,
        1,
        alpha.genesis(),
        legacy,
        authority_log_support::caps(),
        None,
    );

    let store = store_of(&[alpha.inception.clone(), self_grant]);
    let state = derive(alpha.subject, &store).state().unwrap().clone();

    assert!(
        !state.devices.contains_key(&legacy),
        "prior existence of a key confers nothing; a self-signed grant is not authorized"
    );
    assert!(!state.authority.contains(&legacy));
}

// ---------------------------------------------------------------------------
// I10 — no global forward index, bounded context, separability
// ---------------------------------------------------------------------------

/// §7.2.7 / I10 — one legacy principal authorized in two contexts creates no link between them.
///
/// This is the shape the forward index would have to have: `legacy Did -> every subject it
/// seeded`. Deriving either subject yields state that names only that subject's own chain, and
/// the store offers no principal-keyed lookup to walk the other way.
#[test]
fn two_subjects_sharing_one_legacy_principal_are_not_linked() {
    let alpha = subject(0x31, 2);
    let beta = subject(0x32, 2);
    let (_, legacy) = stranger(0x7c);

    let store = store_of(&[
        alpha.inception.clone(),
        authorize_at(&alpha, 0, 1, alpha.genesis(), legacy),
        beta.inception.clone(),
        authorize_at(&beta, 0, 1, beta.genesis(), legacy),
    ]);

    // The same principal really is a device in both — otherwise there is nothing to correlate.
    let a_state = derive(alpha.subject, &store).state().unwrap().clone();
    let b_state = derive(beta.subject, &store).state().unwrap().clone();
    assert!(a_state.devices.contains_key(&legacy));
    assert!(b_state.devices.contains_key(&legacy));

    // Subject-keyed retrieval stays inside one context.
    for body in store.bodies_for(alpha.subject) {
        assert_eq!(
            body.subject(),
            alpha.subject,
            "bodies_for must not return another subject's bodies"
        );
    }
    assert!(
        a_state.authority.is_disjoint(&b_state.authority),
        "two contexts must not share authority material"
    );
    assert_ne!(alpha.subject, beta.subject);
}

/// Subjects are separable, so context-local retention and erasure remain possible (§7.3).
///
/// A store assembled without a subject's events derives that subject to `Unknown` while every
/// other subject stays fully derivable. Nothing global has to be rebuilt or torn down — which is
/// precisely what a forward index would have made impossible.
#[test]
fn dropping_one_subject_leaves_the_other_intact() {
    let alpha = subject(0x41, 2);
    let beta = subject(0x42, 2);
    let (_, legacy) = stranger(0x7d);

    let full = store_of(&[
        alpha.inception.clone(),
        authorize_at(&alpha, 0, 1, alpha.genesis(), legacy),
        beta.inception.clone(),
        authorize_at(&beta, 0, 1, beta.genesis(), legacy),
    ]);
    assert!(matches!(
        derive(alpha.subject, &full),
        AuthorityView::Live { .. }
    ));

    let without_alpha = store_of(&[
        beta.inception.clone(),
        authorize_at(&beta, 0, 1, beta.genesis(), legacy),
    ]);

    assert_eq!(
        derive(alpha.subject, &without_alpha),
        AuthorityView::Unknown,
        "an erased context must leave no residual derivable authority"
    );
    let b_state = derive(beta.subject, &without_alpha)
        .state()
        .expect("the retained subject is unaffected")
        .clone();
    assert!(
        b_state.devices.contains_key(&legacy),
        "erasing one context must not degrade another"
    );
}

/// A legacy `Did` reaches the model only as a Principal (§7.2.3) — never as a subject.
///
/// `PrincipalKey::from_did` is the sole legitimate legacy seam. It yields a Principal, and the
/// round trip back to a `Did` is stable; at no point does a subject appear.
// The assertions below are deliberately constant: `EXISTS` is resolved at compile time,
// which is exactly what lets this detect a *future* forbidden impl. Constancy is the
// mechanism, so `assertions_on_constants` is not a defect here.
#[allow(clippy::assertions_on_constants)]
#[test]
fn a_legacy_did_enters_only_as_a_principal() {
    let (legacy_key, legacy) = stranger(0x7e);
    let legacy_did: Did = legacy.to_did();

    let recovered =
        PrincipalKey::from_did(&legacy_did).expect("a key-derived DID yields a principal");
    assert_eq!(recovered, principal(&legacy_key));

    // The forward direction exists; the direction into subject-space does not.
    assert!(!ConversionProbe::<Did>::EXISTS);
}
