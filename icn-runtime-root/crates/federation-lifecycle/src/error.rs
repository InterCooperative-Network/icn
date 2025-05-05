use thiserror::Error;

/// Errors that can occur during federation lifecycle operations
#[derive(Error, Debug)]
pub enum LifecycleError {
    #[error("Invalid proposal: {0}")]
    InvalidProposal(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Process already in progress: {0}")]
    ProcessAlreadyInProgress(String),

    #[error("Process not found: {0}")]
    ProcessNotFound(String),

    #[error("Challenge window active until {0}")]
    ChallengeWindowActive(String),

    #[error("Invalid federation state: {0}")]
    InvalidFederationState(String),

    #[error("Trust mapping error: {0}")]
    TrustMappingError(String),

    #[error("Quorum requirement not met: {0}")]
    QuorumNotMet(String),

    #[error("Economic inconsistency: {0}")]
    EconomicInconsistency(String),

    #[error("DAG anchoring failed: {0}")]
    DagAnchoringFailed(String),

    #[error("Ledger operation failed: {0}")]
    LedgerOperationFailed(String),

    #[error("Bundle serialization error: {0}")]
    BundleSerializationError(String),

    #[error("Trust bundle error: {0}")]
    TrustBundleError(String),

    #[error("Lineage attestation error: {0}")]
    LineageAttestationError(String),

    #[error("Identity error: {0}")]
    IdentityError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid DAG state: {0}")]
    InvalidDagState(String),

    #[error("Partition map error: {0}")]
    PartitionMapError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Result type for federation lifecycle operations
pub type LifecycleResult<T> = Result<T, LifecycleError>; 