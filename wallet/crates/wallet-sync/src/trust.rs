use wallet_core::identity::IdentityWallet;
use wallet_types::TrustBundle;
use crate::error::SyncResult;

pub struct TrustBundleValidator {
    identity: IdentityWallet,
}

impl TrustBundleValidator {
    pub fn new(identity: IdentityWallet) -> Self {
        Self { identity }
    }
    
    pub fn validate_bundle(&self, bundle: &TrustBundle) -> SyncResult<bool> {
        // In a real implementation, this would:
        // 1. Verify bundle signatures
        // 2. Check the threshold policy
        // 3. Validate against root of trust
        // 4. Verify timestamps and version
        
        // For this example, we'll just validate some basic properties
        
        // Must have at least one guardian
        if bundle.guardians.is_empty() {
            return Ok(false);
        }
        
        // Fix threshold comparison
        let guardians_count: u32 = bundle.guardians.len().try_into().unwrap_or(u32::MAX);
        if bundle.threshold == 0 || bundle.threshold > guardians_count {
            return Ok(false);
        }
        
        // Bundle must be active
        if !bundle.active {
            return Ok(false);
        }
        
        // In a full implementation, we'd also verify signatures
        // of enough guardians to meet the threshold
        
        Ok(true)
    }
    
    pub fn is_guardian(&self, bundle: &TrustBundle, did: &str) -> bool {
        bundle.guardians.contains(&did.to_string())
    }
    
    pub fn is_self_guardian(&self, bundle: &TrustBundle) -> bool {
        self.is_guardian(bundle, &self.identity.did.to_string())
    }
} 