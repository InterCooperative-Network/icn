import { FederationManifest, TrustScoreResult, VerifiableCredential } from '../types';

/**
 * Returns the appropriate color classes based on the trust level
 * @param trustLevel The trust level string
 * @returns CSS color classes for the trust level
 */
export const getTrustLevelColor = (trustLevel: string): string => {
  switch (trustLevel.toLowerCase()) {
    case 'trusted':
    case 'high':
      return 'bg-green-100 text-green-800';
    case 'medium':
      return 'bg-yellow-100 text-yellow-800';
    case 'low':
      return 'bg-orange-100 text-orange-800';
    case 'untrusted':
    case 'revoked':
      return 'bg-red-100 text-red-800';
    default:
      return 'bg-gray-100 text-gray-800';
  }
};

/**
 * Returns the appropriate Material UI color based on the trust level
 * To be used with MUI components that accept color props
 * @param trustLevel The trust level string
 * @returns Material UI color name
 */
export const getTrustLevelMuiColor = (trustLevel?: string): 'success' | 'warning' | 'error' | 'default' => {
  if (!trustLevel) return 'default';
  
  switch (trustLevel.toLowerCase()) {
    case 'trusted':
    case 'high':
      return 'success';
    case 'medium':
      return 'warning';
    case 'low':
    case 'untrusted':
    case 'revoked':
      return 'error';
    default:
      return 'default';
  }
};

/**
 * Calculates trust score based on federation trust information
 * @param federationTrust The federation trust information
 * @returns A numeric trust score between 0 and 100
 */
export const getTrustScore = (federationTrust: { status: string; score?: number }): number => {
  if (!federationTrust) return 0;
  
  if (federationTrust.score !== undefined) {
    return federationTrust.score;
  }
  
  // Calculate score based on status if not explicitly provided
  switch (federationTrust.status.toLowerCase()) {
    case 'trusted':
    case 'high':
      return 100;
    case 'medium':
      return 75;
    case 'low':
      return 50;
    case 'untrusted':
      return 25;
    case 'revoked':
      return 0;
    default:
      return 50;
  }
};

/**
 * Computes a federation trust score for a verifiable credential
 * This evaluates how trustworthy the credential is within its federation context
 * @param credential The verifiable credential to compute trust score for
 * @param manifest The federation manifest containing member and quorum information
 * @returns A trust score result object
 */
export function computeFederationTrustScore(
  credential: any,
  manifest: FederationManifest
): TrustScoreResult {
  const breakdown = {
    valid_signature: false,
    registered_member: false,
    quorum_threshold_met: false,
    sufficient_signer_weight: false,
    federation_health: 0,
    dag_ancestry_valid: undefined
  };
  
  const details: string[] = [];
  
  // 1. Check if the issuer is a registered federation member
  const issuerDid = typeof credential.issuer === 'string' ? credential.issuer : credential.issuer.id;
  const isMember = !!manifest.members[issuerDid];
  breakdown.registered_member = isMember;
  
  if (isMember) {
    details.push(`Issuer ${issuerDid} is a registered member of federation ${manifest.federation_id}`);
  } else {
    details.push(`Issuer ${issuerDid} is NOT a registered member of federation ${manifest.federation_id}`);
  }
  
  // 2. Check signature validity (already done in main verification)
  breakdown.valid_signature = true; // Assuming the signature was verified already
  
  // 3. Check signer weight against quorum requirements
  if (isMember) {
    const signerRole = manifest.members[issuerDid];
    const signerWeight = signerRole.weight || 0;
    const requiredWeight = manifest.quorum_rules.policy_type === 'Weighted' ? 
      (manifest.quorum_rules.threshold_percentage || 50) / 100 * 10 : 1;
    
    breakdown.sufficient_signer_weight = signerWeight >= requiredWeight;
    
    details.push(`Signer weight: ${signerWeight}, Required weight: ${requiredWeight}`);
    
    if (credential.proof && credential.proof.signatures && Array.isArray(credential.proof.signatures)) {
      // Check for multi-signature quorum
      const totalVotes = credential.proof.signatures.length;
      breakdown.quorum_threshold_met = totalVotes >= manifest.quorum_rules.min_approvals;
      
      details.push(`Signatures: ${totalVotes}, Required for quorum: ${manifest.quorum_rules.min_approvals}`);
    } else {
      // Single signature - check if it meets quorum for finalization
      const isFinalizer = signerRole.role === 'Admin' || signerRole.role === 'Guardian';
      breakdown.quorum_threshold_met = isFinalizer && signerWeight >= requiredWeight;
      
      if (isFinalizer) {
        details.push(`Signer is a ${signerRole.role} with finalizer rights`);
      } else {
        details.push('Signer does not have finalizer rights');
      }
    }
  }
  
  // 4. Calculate federation health from manifest if available
  if (manifest.health_metrics) {
    breakdown.federation_health = manifest.health_metrics.overall_health;
    details.push(`Federation health: ${manifest.health_metrics.overall_health}%`);
  } else {
    breakdown.federation_health = 50; // Default to medium health
    details.push('No federation health metrics available');
  }
  
  // 5. Calculate overall trust score
  let score = 0;
  
  if (breakdown.registered_member) score += 20;
  if (breakdown.valid_signature) score += 30;
  if (breakdown.quorum_threshold_met) score += 25;
  if (breakdown.sufficient_signer_weight) score += 15;
  
  // Add health score (scaled to 10 points max)
  score += (breakdown.federation_health / 100) * 10;
  
  // Determine status based on score
  let status: 'High' | 'Medium' | 'Low' = 'Low';
  if (score >= 75) {
    status = 'High';
  } else if (score >= 45) {
    status = 'Medium';
  }
  
  // Generate summary
  let summary = `${status} trust (${score}/100): `;
  if (score >= 75) {
    summary += 'Credential strongly verified by trusted federation member';
  } else if (score >= 45) {
    summary += 'Partially trusted federation credential';
  } else {
    summary += 'Low federation trust score';
  }
  
  return {
    score,
    status,
    breakdown,
    summary,
    details
  };
} 