//! N4-A relying-party evaluation over the daemon's durable N1 registry (ADR-0088), not an
//! in-memory store.
//!
//! N4-A's own suite builds its `AuthorityStore` in memory. The daemon's store is the one
//! [`AuthorityRegistry`] rehydrates from sled at every start, so the question here is whether a
//! relying-party verdict survives that round trip unchanged: the same answers over
//! `registry.state()` after a reopen as over an independent in-memory store built from the same
//! facts, at every position the relying party may pick.
//!
//! Facts are written offline under the storage lock, as every other registry test does; nothing
//! here is a runtime producer. Fixture identities only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use ed25519_dalek::SigningKey;
use icn_core::authority_registry::AuthorityRegistry;
use icn_core::DataDirLock;
use icn_identity::authority_log::{
    authorize_event, revoke_event, AuthorityStore, CapabilitySet, ContextNonce, ContinuityRoot,
    DeviceCapability, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{
    sign_device_act, verify_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal,
    SignedDeviceAct,
};

const DEVICE_A: u8 = 10;
const DEVICE_B: u8 = 11;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn principal(seed: u8) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(key(seed).verifying_key()).unwrap()
}

fn root(seed: u8) -> ContinuityRoot {
    ContinuityRoot::new([seed; 32], ContextNonce::from_bytes([seed ^ 0xa5; 32]), 2)
}

fn sign_grant() -> CapabilitySet {
    CapabilitySet::new([DeviceCapability::Sign])
}

/// Subject S: A authorized at 1, B at 2, A revoked at 3. Subject T: two competing grants at 1, so
/// it is halted.
fn history() -> (SubjectId, SubjectId, Vec<SignedAuthorityEvent>) {
    let (rs, rt) = (root(1), root(2));
    let s = rs.incept().unwrap();
    let sid = s.body.subject();
    let auth = rs.authority_signing_key(0);
    let a = authorize_event(
        &auth,
        sid,
        1,
        s.body.event_id(),
        principal(DEVICE_A),
        sign_grant(),
        None,
    );
    let b = authorize_event(
        &auth,
        sid,
        2,
        a.body.event_id(),
        principal(DEVICE_B),
        sign_grant(),
        None,
    );
    let revoke_a = revoke_event(&auth, sid, 3, b.body.event_id(), principal(DEVICE_A));

    let t = rt.incept().unwrap();
    let tid = t.body.subject();
    let t_auth = rt.authority_signing_key(0);
    let t1 = authorize_event(
        &t_auth,
        tid,
        1,
        t.body.event_id(),
        principal(20),
        sign_grant(),
        None,
    );
    let t2 = authorize_event(
        &t_auth,
        tid,
        1,
        t.body.event_id(),
        principal(21),
        sign_grant(),
        None,
    );

    (sid, tid, vec![s, a, b, revoke_a, t, t1, t2])
}

fn act(subject: SubjectId, device: u8, position: u64) -> SignedDeviceAct {
    let act = DeviceActV1::new(
        subject,
        principal(device),
        DeviceCapability::Sign,
        position,
        b"n4a-over-registry".to_vec(),
    )
    .unwrap();
    sign_device_act(&act, &key(device)).unwrap()
}

fn lock(dir: &Path) -> DataDirLock {
    DataDirLock::acquire(dir, "the N4-A over registry test").unwrap()
}

/// Every verdict the relying party can ask for, for both devices, at every position 1..=4.
fn verdicts(
    subject: SubjectId,
    store: &AuthorityStore,
) -> Vec<(u8, u64, Result<u64, DeviceActVerifyError>)> {
    let mut out = Vec::new();
    for device in [DEVICE_A, DEVICE_B] {
        for position in 1..=4 {
            let verdict =
                verify_device_act(&act(subject, device, position), subject, store, position)
                    .map(|verified| verified.evidence().grant.granted_at);
            out.push((device, position, verdict));
        }
    }
    out
}

#[test]
fn device_verdicts_over_the_rehydrated_registry_equal_those_over_the_facts_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let (s, t, events) = history();

    {
        let lock = lock(dir.path());
        let (mut registry, _) = AuthorityRegistry::open(dir.path(), &lock).unwrap();
        for event in &events {
            registry.ingest(event).unwrap();
        }
    }

    // A fresh open rehydrates from the durable store; nothing is carried over in memory.
    let lock = lock(dir.path());
    let (registry, _) = AuthorityRegistry::open(dir.path(), &lock).unwrap();

    let mut independent = AuthorityStore::new();
    for event in &events {
        independent.ingest(event).unwrap();
    }
    assert_eq!(registry.state(), &independent);

    let durable = verdicts(s, registry.state());
    assert_eq!(durable, verdicts(s, &independent));

    let not_authorized = || {
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized,
        ))
    };
    let at = |device: u8, position: u64| {
        durable
            .iter()
            .find(|(d, p, _)| *d == device && *p == position)
            .map(|(_, _, v)| v.clone())
            .unwrap()
    };
    // Historical evaluation: A's authorship before its revocation still stands at 1 and 2.
    assert_eq!(at(DEVICE_A, 1), Ok(1));
    assert_eq!(at(DEVICE_A, 2), Ok(1));
    // At the revocation A is refused, and B, authorized independently, is not affected.
    assert_eq!(at(DEVICE_A, 3), not_authorized());
    assert_eq!(at(DEVICE_B, 1), not_authorized());
    assert_eq!(at(DEVICE_B, 2), Ok(2));
    assert_eq!(at(DEVICE_B, 3), Ok(2));
    // Past the last retained fact the prefix is incomplete for both: the node never reads its
    // retained state as current beyond what it holds.
    for device in [DEVICE_A, DEVICE_B] {
        assert!(
            matches!(
                at(device, 4),
                Err(DeviceActVerifyError::Refused(
                    DeviceAuthorityRefusal::PrefixIncomplete { .. }
                ))
            ),
            "{device}: {:?}",
            at(device, 4)
        );
    }

    // A fork persisted through the registry still halts the Subject after rehydration.
    for device in [20, 21] {
        let refused = verify_device_act(&act(t, device, 1), t, registry.state(), 1).unwrap_err();
        assert!(
            matches!(
                refused,
                DeviceActVerifyError::Refused(DeviceAuthorityRefusal::AuthorityHalted { .. })
            ),
            "{refused:?}"
        );
    }
}
