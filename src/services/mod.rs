// Services module for ICN wallet

// Export federation sync service
pub mod federation_sync;
pub use federation_sync::{
    FederationSyncError,
    FederationSyncService,
    FederationSyncConfig,
    CredentialSyncData,
    CredentialStatus,
    CredentialTrustScore,
}; 

// Export onboarding service
pub mod onboarding;
pub use onboarding::{
    OnboardingService,
    OnboardingError,
    FederationInvitePayload,
    QrFormat,
}; 