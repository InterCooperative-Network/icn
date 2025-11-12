//! Message types for contract distribution over gossip

use crate::ast::Contract;
use crate::types::ContractInstallation;
use icn_identity::Did;
use icn_ledger::ContentHash;
use serde::{Deserialize, Serialize};

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

    /// Verify the deployment message
    pub fn verify(&self) -> anyhow::Result<()> {
        use anyhow::{bail, Context};

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

        // Verify all participants have signed
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

        // TODO: Verify cryptographic signatures (requires KeyPair integration)

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
