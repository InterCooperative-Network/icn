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
    export_bundle, stored_proof_key, verify_bundle, BundleVerifyError,
};
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
    /// Exit codes: 0 verified; 2 integrity; 3 decision inconsistent; 4 signature invalid or
    /// absent; 5 signer not the pinned one; 6 malformed or unsupported bundle; 7 unusable
    /// pinned signer; 1 could not read the bundle.
    Verify {
        /// Bundle file to verify
        #[arg(long)]
        bundle: PathBuf,
        /// DID of the node expected to have attested the decision (obtained out of band)
        #[arg(long)]
        expect_signer: String,
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
    }
}

pub(crate) fn run(cmd: ProofCommands, data_dir: &Path) -> Result<()> {
    match cmd {
        ProofCommands::Export { proposal_id, out } => export(data_dir, &proposal_id, &out),
        ProofCommands::Verify {
            bundle,
            expect_signer,
        } => {
            let bytes = std::fs::read(&bundle)
                .with_context(|| format!("reading bundle {}", bundle.display()))?;
            match verify_bundle(&bytes, &expect_signer) {
                Ok(verified) => {
                    let mut out = serde_json::to_value(&verified)?;
                    out["verified"] = serde_json::Value::Bool(true);
                    println!("{}", serde_json::to_string_pretty(&out)?);
                    Ok(())
                }
                Err(e) => {
                    let out = serde_json::json!({
                        "verified": false,
                        "code": e.code(),
                        "reason": e.to_string(),
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
