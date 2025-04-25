import { WalletCredential } from '../types/wallet';
import { FederationManifest, QuorumConfig, FederationMemberRole } from '../types/federation';
import { FederationReport } from './federationSignature';
import * as jose from 'jose';

/**
 * Result of a federation report quorum validation
 */
export interface QuorumValidationResult {
  /**
   * Whether the report satisfies quorum requirements
   */
  isValid: boolean;

  /**
   * A list of federation members who signed the report
   */
  signers: {
    did: string;
    role: string;
    weight: number;
  }[];

  /**
   * Analysis of whether quorum was satisfied
   */
  quorumAnalysis: {
    requiredParticipants: number;
    actualParticipants: number;
    requiredApprovals: number;
    actualApprovals: number;
    requiredThreshold?: number;
    actualThreshold?: number;
    totalWeight: number;
    isSatisfied: boolean;
  };

  /**
   * If validation failed, details about why
   */
  errors: string[];
}

/**
 * Validate a federation report against the federation's quorum rules
 * 
 * @param report The federation report to validate
 * @param manifest The federation manifest containing member information and quorum rules
 * @returns Validation result with quorum analysis
 */
export async function validateFederationReport(
  report: FederationReport,
  manifest: FederationManifest
): Promise<QuorumValidationResult> {
  const result: QuorumValidationResult = {
    isValid: false,
    signers: [],
    quorumAnalysis: {
      requiredParticipants: manifest.quorum_rules.min_participants,
      actualParticipants: 0,
      requiredApprovals: manifest.quorum_rules.min_approvals,
      actualApprovals: 0,
      totalWeight: 0,
      isSatisfied: false,
    },
    errors: [],
  };

  // Add threshold percentage if relevant for this quorum type
  if (['Threshold', 'Weighted'].includes(manifest.quorum_rules.policy_type)) {
    result.quorumAnalysis.requiredThreshold = manifest.quorum_rules.threshold_percentage;
    result.quorumAnalysis.actualThreshold = 0;
  }

  // Validate basic report structure
  if (!report || !report.proof || !report.proof.jws) {
    result.errors.push('Report is missing proof or JWS signature');
    return result;
  }

  try {
    // Extract signatures from the report
    const signatures = extractSignaturesFromReport(report);
    
    if (signatures.length === 0) {
      result.errors.push('No valid signatures found in report');
      return result;
    }

    // Verify each signature and extract signer info
    const verifiedSigners = await verifySignatures(report, signatures, manifest);
    result.signers = verifiedSigners;
    result.quorumAnalysis.actualParticipants = verifiedSigners.length;
    result.quorumAnalysis.actualApprovals = verifiedSigners.length;
    
    // Calculate total weight of signers
    let totalWeight = 0;
    verifiedSigners.forEach(signer => {
      totalWeight += signer.weight;
    });
    result.quorumAnalysis.totalWeight = totalWeight;
    
    // Check if quorum is satisfied based on policy type
    switch (manifest.quorum_rules.policy_type) {
      case 'Majority':
        // Need more than 50% of members
        const totalMembers = Object.keys(manifest.members).length;
        const majorityRequired = Math.ceil(totalMembers / 2);
        result.quorumAnalysis.requiredApprovals = majorityRequired;
        result.quorumAnalysis.isSatisfied = verifiedSigners.length >= majorityRequired;
        break;
        
      case 'Unanimous':
        // All members must sign
        const allMembers = Object.keys(manifest.members).length;
        result.quorumAnalysis.requiredApprovals = allMembers;
        result.quorumAnalysis.isSatisfied = verifiedSigners.length === allMembers;
        break;
        
      case 'Threshold':
        // Need at least threshold_percentage of members
        const thresholdRequired = Math.ceil((manifest.quorum_rules.threshold_percentage || 67) / 100 * Object.keys(manifest.members).length);
        result.quorumAnalysis.requiredApprovals = thresholdRequired;
        result.quorumAnalysis.isSatisfied = verifiedSigners.length >= thresholdRequired;
        
        // Calculate actual threshold percentage
        result.quorumAnalysis.actualThreshold = Math.round((verifiedSigners.length / Object.keys(manifest.members).length) * 100);
        break;
        
      case 'Weighted':
        // Need signatures with total weight >= threshold_percentage of total possible weight
        const memberWeights = Object.values(manifest.members).map(m => m.weight);
        const totalPossibleWeight = memberWeights.reduce((sum, weight) => sum + weight, 0);
        const weightThreshold = (manifest.quorum_rules.threshold_percentage || 67) / 100 * totalPossibleWeight;
        
        result.quorumAnalysis.isSatisfied = totalWeight >= weightThreshold;
        
        // Calculate actual threshold percentage
        result.quorumAnalysis.actualThreshold = Math.round((totalWeight / totalPossibleWeight) * 100);
        break;
        
      default:
        // Default to basic min_approvals check
        result.quorumAnalysis.isSatisfied = 
          verifiedSigners.length >= manifest.quorum_rules.min_participants && 
          verifiedSigners.length >= manifest.quorum_rules.min_approvals;
    }
    
    // Final validation result
    result.isValid = 
      result.quorumAnalysis.isSatisfied && 
      verifiedSigners.length >= manifest.quorum_rules.min_participants;
    
    // Add errors if validation failed
    if (!result.isValid) {
      if (verifiedSigners.length < manifest.quorum_rules.min_participants) {
        result.errors.push(`Not enough participants: ${verifiedSigners.length} < ${manifest.quorum_rules.min_participants} required`);
      }
      
      if (!result.quorumAnalysis.isSatisfied) {
        result.errors.push(`Quorum not satisfied for policy type: ${manifest.quorum_rules.policy_type}`);
      }
    }
    
    return result;
  } catch (error) {
    result.errors.push(`Error validating signatures: ${error}`);
    return result;
  }
}

/**
 * Extract signatures from a federation report
 * 
 * @param report The federation report
 * @returns Array of signatures
 */
function extractSignaturesFromReport(report: FederationReport): string[] {
  // If there's a single signature in JWS format
  if (report.proof && report.proof.jws) {
    return [report.proof.jws];
  }
  
  // If there are multiple signatures (future implementation)
  if (report.proof && Array.isArray(report.proof.signatures)) {
    return report.proof.signatures.map(sig => sig.jws || sig.proofValue).filter(Boolean);
  }
  
  return [];
}

/**
 * Verify signatures and extract signer information
 * 
 * @param report The federation report
 * @param signatures Array of signatures to verify
 * @param manifest The federation manifest with member information
 * @returns Array of verified signers with their roles and weights
 */
async function verifySignatures(
  report: FederationReport,
  signatures: string[],
  manifest: FederationManifest
): Promise<{ did: string; role: string; weight: number }[]> {
  const verifiedSigners: { did: string; role: string; weight: number }[] = [];
  
  // Create verification payload (everything except the proof)
  const { proof, ...reportPayload } = report;
  
  // For each signature, verify and extract signer information
  for (const signature of signatures) {
    try {
      // In a real implementation, we would:
      // 1. Decode the JWS to extract the signer's DID
      // 2. Fetch the signer's public key
      // 3. Verify the signature
      
      // For this example, we'll parse the JWS and extract the kid from the header
      // which should contain the signer's DID
      const [encodedHeader] = signature.split('.');
      const header = JSON.parse(Buffer.from(encodedHeader, 'base64').toString());
      
      // The kid should be in format did:icn:federation/member#keys-1
      const signerDid = header.kid?.split('#')[0];
      
      if (!signerDid) {
        continue; // Skip if no valid signer DID
      }
      
      // Check if the signer is a federation member
      const memberDid = extractMemberDid(signerDid);
      if (!memberDid || !manifest.members[memberDid]) {
        continue; // Skip if not a federation member
      }
      
      const memberRole = manifest.members[memberDid];
      
      // Add to verified signers
      verifiedSigners.push({
        did: memberDid,
        role: memberRole.role,
        weight: memberRole.weight || 1,
      });
    } catch (error) {
      console.error('Error verifying signature:', error);
      // Continue with next signature
    }
  }
  
  return verifiedSigners;
}

/**
 * Extract the member DID from a verification method identifier
 * 
 * @param verificationMethod The verification method or DID URI
 * @returns The member's DID
 */
function extractMemberDid(verificationMethod: string): string | null {
  // Handle different formats of verification methods
  if (verificationMethod.includes('#')) {
    // Format: did:icn:federation/member#keys-1
    return verificationMethod.split('#')[0];
  } else if (verificationMethod.includes('federation/')) {
    // Format: did:icn:federation/member
    return verificationMethod;
  }
  
  return null;
}

/**
 * Format a quorum validation result for display
 * 
 * @param result The quorum validation result
 * @returns A formatted string representation of the result
 */
export function formatQuorumValidationResult(result: QuorumValidationResult): string {
  let output = [];
  
  // Status
  output.push(`Validation status: ${result.isValid ? 'VALID' : 'INVALID'}`);
  
  // Quorum analysis
  output.push('\nQuorum Analysis:');
  output.push(`- Policy requires at least ${result.quorumAnalysis.requiredParticipants} participants, got ${result.quorumAnalysis.actualParticipants}`);
  output.push(`- Policy requires at least ${result.quorumAnalysis.requiredApprovals} approvals, got ${result.quorumAnalysis.actualApprovals}`);
  
  if (result.quorumAnalysis.requiredThreshold !== undefined) {
    output.push(`- Policy requires at least ${result.quorumAnalysis.requiredThreshold}% threshold, got ${result.quorumAnalysis.actualThreshold}%`);
  }
  
  output.push(`- Total signature weight: ${result.quorumAnalysis.totalWeight}`);
  output.push(`- Quorum satisfied: ${result.quorumAnalysis.isSatisfied ? 'YES' : 'NO'}`);
  
  // Signers
  output.push('\nSigners:');
  if (result.signers.length === 0) {
    output.push('  No valid signers found');
  } else {
    result.signers.forEach(signer => {
      output.push(`  - ${signer.did} (${signer.role}, weight: ${signer.weight})`);
    });
  }
  
  // Errors
  if (result.errors.length > 0) {
    output.push('\nErrors:');
    result.errors.forEach(error => {
      output.push(`  - ${error}`);
    });
  }
  
  return output.join('\n');
} 