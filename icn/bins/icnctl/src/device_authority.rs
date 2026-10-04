//! `icnctl device-authority` — a stateless relying party over the canonical N4-B portable
//! evidence bundle.
//!
//! The first entry point to ICN device authority that is not a Rust API: a downstream system
//! (a Home runtime, a deployment check, another process) hands this verb a canonical
//! `icn.n4.evidence-bundle` file and states, as the relying party, *which Subject* it evaluates
//! for and *at which log position*. The verb decodes the bundle, re-admits every carried
//! `(body, witness)` through the N1 admission gate and runs the existing N4-A verifier. It reads
//! the file and nothing else: no data directory, no daemon, no network, no clock. Its stdout is
//! one JSON document; its exit code is the verdict class.
//!
//! | exit | meaning |
//! |---|---|
//! | 0 | `authorized`: the act is covered; the evidence (a grant, not a bool) is printed |
//! | 1 | `refused`: every fact was admissible and the N4-A verifier refused the act, or one fact was inadmissible; the class is printed |
//! | 2 | `malformed`: the bytes are not a strict canonical bundle, or the arguments are unusable |
//!
//! What this verb is not: an authority. It can only refuse, or return a grant that already exists
//! in the Subject's log (`N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.2). It never chooses the
//! evaluation position, fetches facts, repairs malformed evidence, normalizes a fork or claims
//! currentness. The Subject and position are **never** taken from the bundle (N4 invariant 6):
//! the bundle states its own `(S, E)` and the act must agree with them, but the relying party
//! states its own, and a disagreement is N4-A's refusal.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use icn_identity::authority_log::{SubjectId, ValiditySpan};
use icn_identity::device_authority::{
    DeviceActVerifyError, DeviceAuthorityEvidence, DeviceAuthorityRefusal,
};
use icn_identity::evidence_bundle::{
    verify_evidence_bundle, BundleError, EvidenceBundle, EvidenceVerifyError,
};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Exit code for a refused act or an inadmissible fact.
const EXIT_REFUSED: i32 = 1;
/// Exit code for bytes or arguments that could not be evaluated at all.
const EXIT_MALFORMED: i32 = 2;

#[derive(Subcommand, Debug)]
pub enum DeviceAuthorityCommands {
    /// Decide, as the relying party, whether the bundle's act is covered by the named Subject's
    /// delegation at the given position. Reads the file and nothing else.
    Verify {
        /// A canonical N4-B portable evidence bundle file (icn.n4.evidence-bundle, version 1).
        #[arg(long)]
        bundle: PathBuf,
        /// The Subject YOU evaluate for, as 64 hex characters. Never taken from the bundle.
        #[arg(long)]
        subject: String,
        /// The log position YOU evaluate at (1..=MAX_POSITION). Never taken from the bundle.
        #[arg(long)]
        position: u64,
    },
    /// Describe a bundle's scope, facts and act without deciding anything.
    Inspect {
        /// A canonical N4-B portable evidence bundle file.
        #[arg(long)]
        bundle: PathBuf,
    },
}

pub fn handle_device_authority_command(cmd: DeviceAuthorityCommands) -> Result<()> {
    match cmd {
        DeviceAuthorityCommands::Verify {
            bundle,
            subject,
            position,
        } => verify(&bundle, &subject, position),
        DeviceAuthorityCommands::Inspect { bundle } => inspect(&bundle),
    }
}

fn verify(path: &Path, subject_hex: &str, position: u64) -> Result<()> {
    let subject = match parse_subject(subject_hex) {
        Ok(subject) => subject,
        Err(reason) => return finish(malformed(&reason), EXIT_MALFORMED),
    };
    let bundle = match read_bundle(path) {
        Ok(bundle) => bundle,
        Err(reason) => return finish(malformed(&reason), EXIT_MALFORMED),
    };
    match verify_evidence_bundle(&bundle, subject, position) {
        Ok(evidence) => finish(authorized(&bundle, &evidence), 0),
        Err(EvidenceVerifyError::Bundle(BundleError::Inadmissible { event_id, source })) => finish(
            json!({
                "verdict": "refused",
                "class": "InadmissibleFact",
                "event_id": hex::encode(event_id.as_bytes()),
                "detail": source.to_string(),
                "bundle_id": hex::encode(bundle.bundle_id()),
            }),
            EXIT_REFUSED,
        ),
        // Only admission can fail after a successful decode; anything else is not evaluable.
        Err(EvidenceVerifyError::Bundle(err)) => {
            finish(malformed(&err.to_string()), EXIT_MALFORMED)
        }
        Err(EvidenceVerifyError::Act(err)) => finish(
            json!({
                "verdict": "refused",
                "class": refusal_class(&err),
                "detail": err.to_string(),
                "evaluation_position": position,
                "bundle_id": hex::encode(bundle.bundle_id()),
            }),
            EXIT_REFUSED,
        ),
    }
}

fn inspect(path: &Path) -> Result<()> {
    let bundle = match read_bundle(path) {
        Ok(bundle) => bundle,
        Err(reason) => return finish(malformed(&reason), EXIT_MALFORMED),
    };
    let act = &bundle.act().act;
    let facts: Vec<Value> = bundle
        .summarize()
        .into_iter()
        .map(|f| {
            json!({
                "index": f.index,
                "kind_tag": f.kind_tag,
                "subject": hex::encode(f.subject.as_bytes()),
                "position": f.position,
                "event_id": hex::encode(f.event_id.as_bytes()),
                "witness_count": f.witness_count,
            })
        })
        .collect();
    finish(
        json!({
            "bundle_id": hex::encode(bundle.bundle_id()),
            "subject": hex::encode(bundle.subject().as_bytes()),
            "evaluation_position": bundle.evaluation_position(),
            "fact_count": bundle.facts().len(),
            "facts": facts,
            "act": {
                "subject": hex::encode(act.subject.as_bytes()),
                "device": hex::encode(act.device.as_bytes()),
                "device_did": act.device.to_did().to_string(),
                "capability": format!("{:?}", act.capability),
                "evaluation_position": act.evaluation_position,
                "payload_len": act.payload.len(),
                "act_id": hex::encode(act.act_id()),
            },
        }),
        0,
    )
}

fn read_bundle(path: &Path) -> std::result::Result<EvidenceBundle, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    EvidenceBundle::decode(&bytes).map_err(|e| e.to_string())
}

fn parse_subject(hex_str: &str) -> std::result::Result<SubjectId, String> {
    let bytes = hex::decode(hex_str.trim()).map_err(|e| format!("subject is not hex: {e}"))?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "subject must be exactly 32 bytes (64 hex characters)".to_string())?;
    Ok(SubjectId::from_bytes(array))
}

fn authorized(bundle: &EvidenceBundle, evidence: &DeviceAuthorityEvidence) -> Value {
    let capabilities: Vec<String> = evidence
        .grant
        .capabilities
        .members()
        .iter()
        .map(|c| format!("{c:?}"))
        .collect();
    json!({
        "verdict": "authorized",
        "subject": hex::encode(evidence.subject.as_bytes()),
        "device": hex::encode(evidence.device.as_bytes()),
        "device_did": evidence.device.to_did().to_string(),
        "capability": format!("{:?}", evidence.capability),
        "evaluation_position": evidence.evaluation_position,
        "grant": {
            "capabilities": capabilities,
            "validity": evidence.grant.validity.map(span),
            "granted_at": evidence.grant.granted_at,
        },
        "generation": evidence.generation,
        "act_id": hex::encode(bundle.act().act.act_id()),
        "bundle_id": hex::encode(bundle.bundle_id()),
        "fact_count": bundle.facts().len(),
    })
}

fn span(v: ValiditySpan) -> Value {
    json!({ "from_position": v.from_position, "until_position": v.until_position })
}

fn malformed(reason: &str) -> Value {
    json!({ "verdict": "malformed", "detail": reason })
}

/// The stable class name of a refusal: the N4-A vocabulary, never a payload byte.
fn refusal_class(err: &DeviceActVerifyError) -> &'static str {
    match err {
        DeviceActVerifyError::ActSubjectMismatch => "ActSubjectMismatch",
        DeviceActVerifyError::EvaluationPositionMismatch { .. } => "EvaluationPositionMismatch",
        DeviceActVerifyError::BadSignature => "BadSignature",
        DeviceActVerifyError::Refused(refusal) => match refusal {
            DeviceAuthorityRefusal::PositionOutOfRange(_) => "PositionOutOfRange",
            DeviceAuthorityRefusal::SubjectUnknown => "SubjectUnknown",
            DeviceAuthorityRefusal::AuthorityHalted { .. } => "AuthorityHalted",
            DeviceAuthorityRefusal::PrefixIncomplete { .. } => "PrefixIncomplete",
            DeviceAuthorityRefusal::DeviceIsEstablishmentAuthority => {
                "DeviceIsEstablishmentAuthority"
            }
            DeviceAuthorityRefusal::DeviceNotAuthorized => "DeviceNotAuthorized",
            DeviceAuthorityRefusal::GrantNotInForce { .. } => "GrantNotInForce",
            DeviceAuthorityRefusal::CapabilityNotGranted(_) => "CapabilityNotGranted",
        },
    }
}

/// Print the document and end the process with the verdict's exit code. stdout is the result;
/// it is flushed before exiting so a piped reader always sees the whole document.
fn finish(document: Value, code: i32) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, &document).context("writing the verdict")?;
    out.write_all(b"\n").context("writing the verdict")?;
    out.flush().context("flushing the verdict")?;
    if code == 0 {
        return Ok(());
    }
    if code != EXIT_REFUSED && code != EXIT_MALFORMED {
        bail!("internal: unexpected exit code {code}");
    }
    std::process::exit(code)
}
