//! Message types for contract distribution over gossip

use crate::ast::Contract;
use crate::types::ContractInstallation;
use icn_identity::Did;
use icn_ledger::ContentHash;
use serde::{Deserialize, Serialize};

/// Maximum number of participants/signatures (must match ast::MAX_PARTICIPANTS)
const MAX_PARTICIPANTS: usize = 100;

/// Message announcing a new contract deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeploymentMessage {
    /// Hash of the contract code
    pub code_hash: ContentHash,

    /// The contract itself
    pub contract: Contract,

    /// Installation metadata
    pub installation: ContractInstallation,

    /// Signature from deployer
    pub deployer_signature: Vec<u8>,
}

impl ContractDeploymentMessage {
    /// Create a new contract deployment message
    pub fn new(
        code_hash: ContentHash,
        contract: Contract,
        installation: ContractInstallation,
        deployer_signature: Vec<u8>,
    ) -> Self {
        ContractDeploymentMessage {
            code_hash,
            contract,
            installation,
            deployer_signature,
        }
    }

    /// Compute canonical bytes for signing
    ///
    /// Returns SHA-256 hash of: code_hash || installed_at_timestamp
    /// This ensures signatures are bound to specific deployments.
    pub fn signing_bytes(&self) -> Vec<u8> {
        Self::compute_signing_bytes(&self.code_hash, self.installation.installed_at)
    }

    /// Helper to compute signing bytes for a contract before deployment
    ///
    /// This static method allows clients to compute what to sign
    /// before creating the full deployment message.
    pub fn compute_signing_bytes(code_hash: &ContentHash, installed_at: u64) -> Vec<u8> {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(code_hash.as_bytes());
        hasher.update(&installed_at.to_le_bytes());
        hasher.finalize().to_vec()
    }

    /// Verify the deployment message
    pub fn verify(&self) -> anyhow::Result<()> {
        use anyhow::{bail, Context};
        use ed25519_dalek::{Signature, Verifier};

        // Validate contract structure
        self.contract
            .validate()
            .context("Contract validation failed")?;

        // Verify deployer is in participants
        if !self.contract.participants.contains(&self.installation.installed_by) {
            bail!(
                "Deployer {} is not a contract participant",
                self.installation.installed_by
            );
        }

        // Prevent DoS: validate signature count before creating HashSet (H2: Security fix)
        if self.installation.signatures.len() > MAX_PARTICIPANTS {
            bail!(
                "Too many signatures: {} (max {})",
                self.installation.signatures.len(),
                MAX_PARTICIPANTS
            );
        }

        // Verify all participants have signed (Phase 10C)
        let participant_set: std::collections::HashSet<_> =
            self.contract.participants.iter().collect();
        let signature_set: std::collections::HashSet<_> = self
            .installation
            .signatures
            .iter()
            .map(|(did, _)| did)
            .collect();

        if participant_set != signature_set {
            bail!(
                "Participant signatures incomplete: need {:?}, got {:?}",
                participant_set,
                signature_set
            );
        }

        // Get canonical signing bytes
        let signing_bytes = self.signing_bytes();

        // Verify deployer signature
        let deployer_key = self.installation.installed_by.to_verifying_key()
            .context("Failed to extract deployer verifying key")?;
        let deployer_sig = Signature::from_bytes(
            self.deployer_signature.as_slice().try_into()
                .map_err(|_| anyhow::anyhow!("Invalid deployer signature length: expected 64 bytes"))?
        );
        deployer_key.verify(&signing_bytes, &deployer_sig)
            .map_err(|e| anyhow::anyhow!("Deployer signature verification failed: {}", e))?;

        // Verify deployer signature matches installation.signatures (M2: Security fix)
        let deployer_sig_in_installation = self.installation.signatures
            .iter()
            .find(|(did, _)| did == &self.installation.installed_by)
            .map(|(_, sig)| sig)
            .context("Deployer signature missing from installation.signatures")?;

        if &self.deployer_signature != deployer_sig_in_installation {
            bail!("Deployer signature mismatch: deployer_signature field doesn't match installation.signatures");
        }

        // Verify each participant signature
        for (participant_did, sig_bytes) in &self.installation.signatures {
            let participant_key = participant_did.to_verifying_key()
                .context(format!("Failed to extract verifying key for {}", participant_did))?;

            if sig_bytes.len() != 64 {
                bail!("Invalid signature length for {}: expected 64 bytes, got {}",
                      participant_did, sig_bytes.len());
            }

            let signature = Signature::from_bytes(
                sig_bytes.as_slice().try_into()
                    .map_err(|_| anyhow::anyhow!("Failed to parse signature for {}", participant_did))?
            );

            participant_key.verify(&signing_bytes, &signature)
                .map_err(|e| anyhow::anyhow!(
                    "Participant signature verification failed for {}: {}",
                    participant_did, e
                ))?;
        }

        Ok(())
    }
}

/// Request to execute a contract rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractExecutionRequest {
    /// Hash of the contract to execute
    pub code_hash: ContentHash,

    /// Rule name to invoke
    pub rule_name: String,

    /// Arguments for the rule
    pub args: std::collections::HashMap<String, crate::types::Value>,

    /// Who is requesting execution
    pub caller: Did,

    /// Timestamp for deterministic execution
    pub timestamp: u64,
}

/// Response from contract execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractExecutionResponse {
    /// Execution result
    pub result: crate::types::ExecutionResult,

    /// Whether execution succeeded
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,
}
