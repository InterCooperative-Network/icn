//! `icnctl gov proof export|verify` — portable governance proof bundles (icn#2465, first slice).
//!
//! `export` reads the exact stored `GovernanceProofV2` bytes out of a node's governance store
//! with the daemon stopped, under the same data-directory exclusion and N2-A gate as every other
//! offline maintenance command. `verify` needs nothing but the bundle file and a signer DID the
//! operator pins: it opens no store, contacts no daemon, gateway or network. Both are dispatched
//! before any RPC client exists.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;
use icn_governance::proof_bundle::{
    evaluate_standing_at, expect_membership, export_bundle, stored_proof_key, verify_bundle,
    BundleVerifyError,
};
use icn_governance::MembershipAction;
use sha2::{Digest, Sha256};

#[derive(Subcommand, Debug)]
pub(crate) enum ProofCommands {
    /// Export a closed proposal's signed decision proof as a portable bundle.
    ///
    /// Reads the node's governance store directly; stop the daemon first (the data directory
    /// is locked while it runs).
    Export {
        /// Proposal whose decision proof to export
        #[arg(long)]
        proposal_id: String,
        /// Where to write the bundle
        #[arg(long)]
        out: PathBuf,
    },
    /// Verify a bundle offline against a signer you pin.
    ///
    /// Exit codes: 0 verified; 2 integrity (also clap's usage-error code); 3 decision
    /// inconsistent, or an admission and removal with the same decision time; 4 signature invalid
    /// or absent; 5 signer not the pinned one; 6 malformed or unsupported bundle; 7 unusable
    /// pinned signer; 8 a member was expected but a proof binds no payload the pinned signer
    /// signed; 9 a bound payload is not the expected membership change or domain (admission or
    /// removal); 10 with --at, the admission was not yet decided at that time; 11 with --at, a
    /// supplied removal revoked it by then; 1 could not read a bundle. Decisions are ordered by
    /// the pinned node's own decision clock. The JSON `source` names the failing bundle. With --expect-member, success means the pinned node states that its
    /// domain accepted this change; it does not show the change is still current or
    /// institutionally authorized.
    Verify {
        /// Bundle file to verify
        #[arg(long)]
        bundle: PathBuf,
        /// DID of the node expected to have attested the decision (obtained out of band)
        #[arg(long)]
        expect_signer: String,
        /// Also require that the decision bound this membership change for this DID
        /// (compared by key, so any spelling of the DID matches)
        #[arg(long)]
        expect_member: Option<String>,
        /// The expected membership action, with --expect-member
        #[arg(long, value_parser = ["add", "remove"], default_value = "add", requires = "expect_member")]
        expect_action: String,
        /// The domain ID the pinned node recorded for the decision, with --expect-member (a label
        /// chosen on that node, not an authenticated institution; omit it and any domain passes)
        #[arg(long, requires = "expect_member")]
        expect_domain: Option<String>,
        /// Evaluate the member's standing at this unix time (seconds), supplied by you — never
        /// taken from the bundle. Requires --expect-member and --expect-domain. Reports at best
        /// "admitted, no revocation observed"; never current membership.
        #[arg(long, requires_all = ["expect_member", "expect_domain"])]
        at: Option<u64>,
        /// A removal decision bundle to consider as revocation evidence (repeatable). Each is
        /// verified under the same pinned signer; a bundle that fails is an error, not skipped.
        #[arg(long, requires = "at")]
        revocation: Vec<PathBuf>,
    },
}

/// Process exit code for a verification failure, grouped by the layer that failed.
pub(crate) fn exit_code(err: &BundleVerifyError) -> i32 {
    match err {
        BundleVerifyError::IntegrityMismatch => 2,
        BundleVerifyError::ProposalIdMismatch { .. }
        | BundleVerifyError::DecisionHashMismatch
        | BundleVerifyError::AttestationDecisionMismatch => 3,
        BundleVerifyError::NoAttestation | BundleVerifyError::InvalidSignature => 4,
        BundleVerifyError::UntrustedSigner { .. } => 5,
        BundleVerifyError::Malformed(_)
        | BundleVerifyError::SchemaMismatch { .. }
        | BundleVerifyError::ProofEncoding
        | BundleVerifyError::ProofMalformed(_) => 6,
        BundleVerifyError::InvalidPinnedSigner(_) => 7,
        BundleVerifyError::PayloadBindingMismatch(_) => 3,
        BundleVerifyError::PayloadBindingInvalidSignature => 4,
        BundleVerifyError::PayloadUnbound => 8,
        BundleVerifyError::SubjectMismatch { .. } => 9,
        BundleVerifyError::NotYetEffective { .. } => 10,
        BundleVerifyError::RevokedByEvidence { .. } => 11,
        BundleVerifyError::AmbiguousOrder { .. } => 3,
    }
}

pub(crate) fn run(cmd: ProofCommands, data_dir: &Path) -> Result<()> {
    match cmd {
        ProofCommands::Export { proposal_id, out } => export(data_dir, &proposal_id, &out),
        ProofCommands::Verify {
            bundle,
            expect_signer,
            expect_member,
            expect_action,
            expect_domain,
            at,
            revocation,
        } => {
            let bytes = std::fs::read(&bundle)
                .with_context(|| format!("reading bundle {}", bundle.display()))?;
            let action = if expect_action == "remove" {
                MembershipAction::Remove
            } else {
                MembershipAction::Add
            };
            let result =
                verify_bundle(&bytes, &expect_signer).and_then(|verified| match &expect_member {
                    Some(member) => {
                        expect_membership(&verified, action, member, expect_domain.as_deref())
                            .map(|()| verified)
                    }
                    None => Ok(verified),
                });
            // Which bundle an error came from: the admission, or the i-th --revocation.
            let mut source = bundle.display().to_string();
            let mut removals = Vec::new();
            for (i, path) in revocation.iter().enumerate() {
                let bytes = std::fs::read(path)
                    .with_context(|| format!("reading revocation bundle {}", path.display()))?;
                removals.push((
                    format!("revocation[{i}] {}", path.display()),
                    verify_bundle(&bytes, &expect_signer),
                ));
            }
            let result = result.and_then(|verified| match at {
                None => Ok((verified, None)),
                Some(at) => {
                    let mut verified_removals = Vec::new();
                    for (label, removal) in removals {
                        match removal {
                            Ok(r) => verified_removals.push(r),
                            Err(e) => {
                                source = label;
                                return Err(e);
                            }
                        }
                    }
                    let removals = verified_removals;
                    let standing = evaluate_standing_at(
                        &verified,
                        &removals,
                        expect_member.as_deref().unwrap_or_default(),
                        expect_domain.as_deref().unwrap_or_default(),
                        at,
                    )?;
                    Ok((verified, Some(standing)))
                }
            });
            match result {
                Ok((verified, standing)) => {
                    let mut out = serde_json::to_value(&verified)?;
                    out["verified"] = serde_json::Value::Bool(true);
                    if let Some(standing) = standing {
                        out["standing"] = serde_json::to_value(&standing)?;
                    }
                    println!("{}", serde_json::to_string_pretty(&out)?);
                    Ok(())
                }
                Err(e) => {
                    let out = serde_json::json!({
                        "verified": false,
                        "code": e.code(),
                        "reason": e.to_string(),
                        "source": source,
                    });
                    println!("{}", serde_json::to_string_pretty(&out)?);
                    std::process::exit(exit_code(&e));
                }
            }
        }
    }
}

fn export(data_dir: &Path, proposal_id: &str, out: &Path) -> Result<()> {
    // Joined before the gate and held past it, as for the other offline maintenance commands.
    let domain = crate::StorageDomain::join(data_dir, "gov proof export")?;
    crate::enforce_n2a_gate(data_dir, "gov proof export")?;
    let store = domain.open_store(&data_dir.join("store").join("governance"))?;
    let stored =
        icn_store::Store::get(&store, &stored_proof_key(proposal_id))?.with_context(|| {
            format!(
            "no decision proof is stored for proposal {proposal_id} (force-closed, still open, \
             or closed on a node without a signing key)"
        )
        })?;
    let bundle = export_bundle(proposal_id, &stored)?;
    std::fs::write(out, &bundle).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "exported": out.display().to_string(),
            "proposal_id": proposal_id,
            "bundle_sha256": hex::encode(Sha256::digest(&bundle)),
            "bundle_bytes": bundle.len(),
        }))?
    );
    Ok(())
}
