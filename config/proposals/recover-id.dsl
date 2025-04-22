// Title: Identity Recovery Request
// Description: Request for recovery of a lost or compromised identity via guardian threshold signatures
// Author: {{AUTHOR}}
// Date: {{DATE}}

identity_recovery {
  // The DID of the identity to recover
  target_did: "{{TARGET_DID}}",
  
  // Required quorum of guardians (minimum signatures needed)
  quorum_threshold: {{THRESHOLD}},
  
  // Optional explanation for recovery request
  reason: "{{REASON}}",
  
  // Guardian signatures required for approval
  guardian_signatures: [
    // Will be populated during voting process
    // Each guardian will add their signature by voting
  ],
  
  // Recovery parameters
  recovery_params: {
    // Type of recovery (full, partial, key-only)
    recovery_type: "{{RECOVERY_TYPE}}",
    
    // New public key if key rotation is needed
    new_public_key: "{{NEW_PUBLIC_KEY}}",
    
    // Whether to reset guardian set after recovery
    reset_guardians: {{RESET_GUARDIANS}},
    
    // Recovery bundle hash (encrypted identity data)
    recovery_bundle_hash: "{{RECOVERY_BUNDLE_HASH}}"
  },
  
  // Validation function
  validate: |
    (ctx) => {
      // Verify request is valid
      const targetDid = ctx.params.target_did;
      
      // Check if identity exists
      if (!ctx.state.identityExists(targetDid)) {
        return { valid: false, reason: "Identity does not exist" };
      }
      
      // Check if recovery is already in progress
      if (ctx.state.isRecoveryActive(targetDid)) {
        return { valid: false, reason: "Recovery already in progress" };
      }
      
      // Validation passes
      return { valid: true };
    }
}

// Execute recovery when quorum is reached
execute: |
  (ctx) => {
    const { target_did, guardian_signatures, quorum_threshold, recovery_params } = ctx.params;
    
    // Get guardian set for this identity
    const guardianSet = ctx.state.getGuardianSet(target_did);
    
    // Verify we have enough valid signatures
    const validSignatures = guardian_signatures.filter(sig => {
      return ctx.state.verifyGuardianSignature(target_did, sig.guardian_did, sig.signature);
    });
    
    if (validSignatures.length < quorum_threshold) {
      return {
        success: false,
        message: `Insufficient valid signatures: ${validSignatures.length}/${quorum_threshold}`
      };
    }
    
    // Perform the recovery based on recovery type
    const recoveryResult = ctx.state.recoverIdentity(
      target_did,
      recovery_params.recovery_type,
      recovery_params.recovery_bundle_hash,
      recovery_params.new_public_key,
      recovery_params.reset_guardians
    );
    
    // Log the recovery in AgoraNet
    ctx.agoranet.notify({
      type: "identity_recovery",
      target_did,
      guardian_count: validSignatures.length,
      timestamp: ctx.timestamp
    });
    
    return {
      success: recoveryResult.success,
      message: recoveryResult.message,
      details: {
        recovery_id: recoveryResult.recovery_id,
        recovered_by: validSignatures.map(sig => sig.guardian_did)
      }
    };
  }

// Allow guardians to vote on this recovery proposal
on_vote: |
  (ctx, vote) => {
    const { target_did } = ctx.params;
    
    // Only allow guardians to vote
    if (!ctx.state.isGuardian(target_did, vote.voter_did)) {
      return {
        valid: false,
        message: "Only guardians can vote on recovery proposals"
      };
    }
    
    // Add signature if vote is "yes"
    if (vote.option === "yes") {
      const signature = vote.signature;
      
      // Verify the signature
      if (!ctx.state.verifyGuardianSignature(target_did, vote.voter_did, signature)) {
        return {
          valid: false,
          message: "Invalid guardian signature"
        };
      }
      
      // Add the signature to guardian_signatures
      ctx.params.guardian_signatures.push({
        guardian_did: vote.voter_did,
        signature: signature,
        timestamp: vote.timestamp
      });
      
      return {
        valid: true,
        message: "Guardian signature recorded"
      };
    }
    
    // Accept "no" votes (they just don't add a signature)
    return {
      valid: true,
      message: vote.option === "no" ? "Guardian rejected recovery" : "Invalid vote option"
    };
  } 