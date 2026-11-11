//! Conformance: `icnctl device-authority verify` (the binary) and the library composition it wraps
//! (`EvidenceBundle::decode` → `verify_evidence_bundle`) reach the same verdict, class, act_id and
//! bundle_id on the same bytes.
//!
//! Why this exists: the ScopedAccessAct relying-party fixture (icn-identity tests) calls the
//! library directly. This test pins that the CLI a downstream system would call agrees with it, so
//! a decision made through either path is the same decision. It does **not** modify the CLI.
//!
//! Independence: the library-side class of every N4-A refusal is derived from the library error's
//! own `Debug` variant name, not from a copy of the CLI's mapping table, and every case also pins
//! the expected class. Coverage: every N4-A class reachable from bytes is exercised (11 of 13).
//! `PositionOutOfRange` and `MalformedAct` are NOT reachable through a bundle: decode refuses an
//! out-of-range act position, and a decoded act is always within bounds; a CLI typo in those two
//! arms would go undetected here. `InadmissibleFact` and `malformed` are container-level outcomes
//! whose names the CLI defines; they are compared by construction, not derived independently.
//!
//! Fixture identities only: every key is derived from a seed byte; nothing here is a real person.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    authorize_event, revoke_event, AuthorityStore, CapabilitySet, ContextNonce, ContinuityRoot,
    DeviceCapability, EstablishmentKind, PrincipalKey, SignedAuthorityEvent, SubjectId,
    ValiditySpan,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1, DeviceActVerifyError};
use icn_identity::evidence_bundle::{
    verify_evidence_bundle, BundleError, EvidenceBundle, EvidenceVerifyError,
};
use serde_json::Value;

fn icnctl() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

fn principal(seed: u8) -> (SigningKey, PrincipalKey) {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let p = PrincipalKey::try_from_verifying_key(key.verifying_key()).unwrap();
    (key, p)
}

struct Fixture {
    subject: SubjectId,
    /// 0 inception · 1 authorize A {Sign, Present} · 2 authorize B {Sign, Present} · 3 revoke A
    facts: Vec<SignedAuthorityEvent>,
    a: (SigningKey, PrincipalKey),
    b: (SigningKey, PrincipalKey),
}

fn fixture() -> Fixture {
    let root = ContinuityRoot::with_plan(
        [0x31; 32],
        ContextNonce::from_bytes([0x31 ^ 0xa5; 32]),
        vec![EstablishmentKind::Rotate; 4],
    );
    let inception = root.incept().unwrap();
    let subject = inception.body.subject();
    let est = root.authority_signing_key(0);
    let a = principal(0x41);
    let b = principal(0x42);
    let caps = CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]);
    let auth_a = authorize_event(
        &est,
        subject,
        1,
        inception.body.event_id(),
        a.1,
        caps.clone(),
        None,
    );
    let auth_b = authorize_event(&est, subject, 2, auth_a.body.event_id(), b.1, caps, None);
    let rev_a = revoke_event(&est, subject, 3, auth_b.body.event_id(), a.1);
    Fixture {
        subject,
        facts: vec![inception, auth_a, auth_b, rev_a],
        a,
        b,
    }
}

/// Inception (seed `seed`) followed by one authorize per entry at positions 1, 2, ... (each
/// chained to the previous body), or, with `fork`, every entry at position 1 off the inception.
/// Returns (subject, establishment key, facts).
fn chain(
    seed: u8,
    entries: &[(PrincipalKey, CapabilitySet, Option<ValiditySpan>)],
    fork: bool,
) -> (SubjectId, SigningKey, Vec<SignedAuthorityEvent>) {
    let root = ContinuityRoot::with_plan(
        [seed; 32],
        ContextNonce::from_bytes([seed ^ 0xa5; 32]),
        vec![EstablishmentKind::Rotate; 4],
    );
    let inception = root.incept().unwrap();
    let subject = inception.body.subject();
    let est = root.authority_signing_key(0);
    let mut facts = vec![inception];
    for (i, (device, caps, validity)) in entries.iter().enumerate() {
        let (position, prev) = if fork {
            (1, facts[0].body.event_id())
        } else {
            (i as u64 + 1, facts[i].body.event_id())
        };
        facts.push(authorize_event(
            &est,
            subject,
            position,
            prev,
            *device,
            caps.clone(),
            *validity,
        ));
    }
    (subject, est, facts)
}

fn bundle_bytes(
    facts: &[SignedAuthorityEvent],
    subject: SubjectId,
    device: &(SigningKey, PrincipalKey),
    capability: DeviceCapability,
    position: u64,
    payload: &[u8],
) -> Vec<u8> {
    let act = DeviceActV1::new(subject, device.1, capability, position, payload.to_vec()).unwrap();
    let signed = sign_device_act(&act, &device.0).unwrap();
    let mut store = AuthorityStore::new();
    for f in facts {
        store.ingest(f).unwrap();
    }
    EvidenceBundle::assemble(&store, &signed)
        .unwrap()
        .canonical_bytes()
}

/// The verdict a relying party reaches through the library, in the CLI's document shape.
#[derive(Debug, PartialEq, Eq)]
struct Verdict {
    verdict: String,
    class: Option<String>,
    act_id: Option<String>,
    bundle_id: Option<String>,
}

/// First identifier of a `Debug` rendering: `Refused(DeviceNotAuthorized)` → inner variant name.
fn variant_name(rendered: &str) -> String {
    rendered
        .split(|c: char| !c.is_alphanumeric())
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn library(bytes: &[u8], subject: SubjectId, position: u64) -> Verdict {
    let bundle = match EvidenceBundle::decode(bytes) {
        Ok(b) => b,
        Err(_) => {
            return Verdict {
                verdict: "malformed".into(),
                class: None,
                act_id: None,
                bundle_id: None,
            }
        }
    };
    let bundle_id = Some(hex::encode(bundle.bundle_id()));
    match verify_evidence_bundle(&bundle, subject, position) {
        Ok(v) => Verdict {
            verdict: "authorized".into(),
            class: None,
            act_id: Some(hex::encode(v.act_id())),
            bundle_id,
        },
        Err(EvidenceVerifyError::Bundle(BundleError::Inadmissible { .. })) => Verdict {
            verdict: "refused".into(),
            class: Some("InadmissibleFact".into()),
            act_id: None,
            bundle_id,
        },
        Err(EvidenceVerifyError::Bundle(_)) => Verdict {
            verdict: "malformed".into(),
            class: None,
            act_id: None,
            bundle_id: None,
        },
        Err(EvidenceVerifyError::Act(err)) => {
            let class = match &err {
                DeviceActVerifyError::Refused(inner) => variant_name(&format!("{inner:?}")),
                other => variant_name(&format!("{other:?}")),
            };
            Verdict {
                verdict: "refused".into(),
                class: Some(class),
                act_id: None,
                bundle_id,
            }
        }
    }
}

fn cli(dir: &Path, bytes: &[u8], subject: SubjectId, position: u64) -> (i32, Value) {
    let path = dir.join("bundle.bin");
    std::fs::write(&path, bytes).unwrap();
    let out = Command::new(icnctl())
        .arg("--data-dir")
        .arg(dir)
        .args(["device-authority", "verify", "--bundle"])
        .arg(&path)
        .args(["--subject", &hex::encode(subject.as_bytes())])
        .args(["--position", &position.to_string()])
        .output()
        .expect("icnctl runs");
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is one JSON document ({e}); stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out.status.code().unwrap(), doc)
}

fn cli_verdict(doc: &Value) -> Verdict {
    let s = |k: &str| doc.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let verdict = s("verdict").unwrap();
    Verdict {
        class: s("class"),
        act_id: s("act_id"),
        bundle_id: if verdict == "malformed" {
            None
        } else {
            s("bundle_id")
        },
        verdict,
    }
}

/// (name, bundle bytes, relying-party subject, position, verdict, pinned class, CLI exit code)
type Case = (
    &'static str,
    Vec<u8>,
    SubjectId,
    u64,
    &'static str,
    Option<&'static str>,
    i32,
);

#[test]
fn cli_and_library_agree_on_every_case() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let other_subject = SubjectId::from_bytes([0x5a; 32]);
    let stranger = principal(0x66);
    let payload = b"app-fixture.scoped-access-act:opaque-to-icn";

    let valid = bundle_bytes(
        &f.facts,
        f.subject,
        &f.b,
        DeviceCapability::Present,
        3,
        payload,
    );
    let mut bad_sig = valid.clone();
    let last = bad_sig.len() - 1;
    bad_sig[last] ^= 0x01;
    let present = CapabilitySet::new([DeviceCapability::Present]);
    let sign_only = CapabilitySet::new([DeviceCapability::Sign]);
    let c = principal(0x51);
    let d = principal(0x52);

    // CapabilityNotGranted: C holds Sign only, signs a Present act.
    let (cap_s, _, cap_f) = chain(0x61, &[(c.1, sign_only, None)], false);
    // GrantNotInForce: C's grant is in force only at position 1; D's grant extends the log to 2.
    let (gnf_s, _, gnf_f) = chain(
        0x62,
        &[
            (c.1, present.clone(), Some(ValiditySpan::new(1, 1).unwrap())),
            (d.1, present.clone(), None),
        ],
        false,
    );
    // DeviceIsEstablishmentAuthority: the establishment key signs as if it were a device.
    let (est_s, est_k, est_f) = chain(0x63, &[(c.1, present.clone(), None)], false);
    let est_p = PrincipalKey::try_from_verifying_key(est_k.verifying_key()).unwrap();
    // AuthorityHalted: two authorized candidates at position 1.
    let (fork_s, _, fork_f) = chain(
        0x64,
        &[(c.1, present.clone(), None), (d.1, present.clone(), None)],
        true,
    );
    // SubjectUnknown: the holder carries no facts at all for the Subject.
    let (unk_s, _, _) = chain(0x65, &[], false);
    // InadmissibleFact: one witness signature corrupted in transit (decode still succeeds).
    let (adm_s, _, adm_f) = chain(0x66, &[(c.1, present.clone(), None)], false);
    let mut inadmissible = bundle_bytes(&adm_f, adm_s, &c, DeviceCapability::Present, 1, payload);
    let wsig = adm_f[1].signature.as_bytes();
    let at = inadmissible
        .windows(64)
        .position(|w| w == wsig.as_slice())
        .unwrap();
    inadmissible[at + 10] ^= 0x01;

    #[rustfmt::skip]
    let cases: Vec<Case> = vec![
        ("authorized sibling", valid.clone(), f.subject, 3, "authorized", None, 0),
        ("revoked device", bundle_bytes(&f.facts, f.subject, &f.a, DeviceCapability::Present, 3, payload), f.subject, 3, "refused", Some("DeviceNotAuthorized"), 1),
        ("unauthorized device", bundle_bytes(&f.facts, f.subject, &stranger, DeviceCapability::Present, 3, payload), f.subject, 3, "refused", Some("DeviceNotAuthorized"), 1),
        ("other subject", valid.clone(), other_subject, 3, "refused", Some("ActSubjectMismatch"), 1),
        ("position mismatch", valid.clone(), f.subject, 2, "refused", Some("EvaluationPositionMismatch"), 1),
        ("act signature flipped", bad_sig, f.subject, 3, "refused", Some("BadSignature"), 1),
        ("prefix incomplete", bundle_bytes(&f.facts[..2], f.subject, &f.a, DeviceCapability::Present, 3, payload), f.subject, 3, "refused", Some("PrefixIncomplete"), 1),
        ("capability not granted", bundle_bytes(&cap_f, cap_s, &c, DeviceCapability::Present, 1, payload), cap_s, 1, "refused", Some("CapabilityNotGranted"), 1),
        ("grant not in force", bundle_bytes(&gnf_f, gnf_s, &c, DeviceCapability::Present, 2, payload), gnf_s, 2, "refused", Some("GrantNotInForce"), 1),
        ("establishment key as device", bundle_bytes(&est_f, est_s, &(est_k.clone(), est_p), DeviceCapability::Present, 1, payload), est_s, 1, "refused", Some("DeviceIsEstablishmentAuthority"), 1),
        ("fork halts authority", bundle_bytes(&fork_f, fork_s, &c, DeviceCapability::Present, 1, payload), fork_s, 1, "refused", Some("AuthorityHalted"), 1),
        ("subject unknown", bundle_bytes(&[], unk_s, &c, DeviceCapability::Present, 1, payload), unk_s, 1, "refused", Some("SubjectUnknown"), 1),
        ("witness corrupted", inadmissible, adm_s, 1, "refused", Some("InadmissibleFact"), 1),
        ("garbage", b"not a bundle".to_vec(), f.subject, 3, "malformed", None, 2),
        ("truncated", valid[..valid.len() / 2].to_vec(), f.subject, 3, "malformed", None, 2),
    ];
    let mut classes = std::collections::BTreeSet::new();
    for (name, bytes, subject, position, expected, class, code) in cases {
        let lib = library(&bytes, subject, position);
        let (exit, doc) = cli(dir.path(), &bytes, subject, position);
        assert_eq!(lib.verdict, expected, "{name}: library verdict");
        assert_eq!(lib.class.as_deref(), class, "{name}: library class");
        assert_eq!(exit, code, "{name}: CLI exit code");
        assert_eq!(cli_verdict(&doc), lib, "{name}: CLI vs library\n{doc:#}");
        if let Some(c) = class {
            classes.insert(c);
        }
    }
    assert_eq!(
        classes.len(),
        11,
        "every byte-reachable refusal class is exercised"
    );
}

#[test]
fn cli_and_library_agree_on_the_authorized_evidence() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let bytes = bundle_bytes(
        &f.facts,
        f.subject,
        &f.b,
        DeviceCapability::Present,
        3,
        b"x",
    );
    let bundle = EvidenceBundle::decode(&bytes).unwrap();
    let v = verify_evidence_bundle(&bundle, f.subject, 3).unwrap();
    let (exit, doc) = cli(dir.path(), &bytes, f.subject, 3);
    assert_eq!(exit, 0);
    let e = v.evidence();
    assert_eq!(doc["subject"], hex::encode(e.subject.as_bytes()));
    assert_eq!(doc["device"], hex::encode(e.device.as_bytes()));
    assert_eq!(doc["capability"], format!("{:?}", e.capability));
    assert_eq!(doc["evaluation_position"], e.evaluation_position);
    assert_eq!(doc["grant"]["granted_at"], e.grant.granted_at);
    assert_eq!(doc["generation"], e.generation);
    assert_eq!(doc["fact_count"], bundle.facts().len());
}

#[test]
fn disclosed_gap_the_cli_never_reveals_the_payload() {
    // A relying party that uses ONLY the CLI learns WHO (verdict, act_id) but never WHAT: neither
    // `verify` nor `inspect` prints payload bytes. To apply application rules it must decode the
    // bundle itself and confirm `act.act_id()` equals the CLI's `act_id` before trusting the payload.
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let payload = b"app-fixture.scoped-access-act:SECRET-MARKER";
    let bytes = bundle_bytes(
        &f.facts,
        f.subject,
        &f.b,
        DeviceCapability::Present,
        3,
        payload,
    );
    let (_, verify_doc) = cli(dir.path(), &bytes, f.subject, 3);
    let path = dir.path().join("bundle.bin");
    let inspect = Command::new(icnctl())
        .arg("--data-dir")
        .arg(dir.path())
        .args(["device-authority", "inspect", "--bundle"])
        .arg(&path)
        .output()
        .unwrap();
    let inspect_text = String::from_utf8(inspect.stdout).unwrap();
    for text in [verify_doc.to_string(), inspect_text.clone()] {
        assert!(!text.contains("SECRET-MARKER"));
        assert!(!text.contains(&hex::encode(payload)));
    }
    let inspect_doc: Value = serde_json::from_str(&inspect_text).unwrap();
    assert_eq!(inspect_doc["act"]["payload_len"], payload.len());
    // The binding a CLI-only relying party must perform itself:
    let decoded = EvidenceBundle::decode(&bytes).unwrap();
    assert_eq!(
        verify_doc["act_id"],
        hex::encode(decoded.act().act.act_id())
    );
    assert_eq!(decoded.act().act.payload, payload);
}
