import axios from 'axios';
import { DID } from '@icn/identity';
import { WalletCredential, VerificationResult } from '../../packages/credential-utils/types';
import { calculateTrustScore } from '../../packages/credential-utils/utils/trustScore';
import { getCredentialTypeName } from '../../packages/credential-utils/utils/credentialExport';

// Define the receipt structure from the API
interface Receipt {
  id: string;
  job_id: string;
  proposal_id?: string;
  created_at: string;
  node_id: string;
  signature: string;
  verifications?: number;
  verified_at?: string;
  verified?: boolean;
  execution_hash?: string;
}

// Configuration for credential sync
interface CredentialSyncConfig {
  runtimeApiEndpoint: string;
  pollingInterval?: number; // in milliseconds
  autoSync?: boolean;
}

/**
 * FederationCredentialSync handles synchronizing federation receipts as credentials
 * for a specific DID from an ICN Runtime node
 */
export class FederationCredentialSync {
  private config: CredentialSyncConfig;
  private userDid: DID;
  private credentials: Map<string, WalletCredential> = new Map();
  private syncInterval?: NodeJS.Timeout;
  private syncInProgress: boolean = false;
  
  /**
   * Create a new federation credential sync
   * @param userDid The DID to sync credentials for
   * @param config Configuration for the sync
   */
  constructor(userDid: DID, config: CredentialSyncConfig) {
    this.userDid = userDid;
    this.config = {
      pollingInterval: 60000, // Default: 1 minute
      autoSync: false,
      ...config,
    };
    
    if (this.config.autoSync) {
      this.startAutoSync();
    }
  }
  
  /**
   * Start automatic synchronization of credentials
   */
  public startAutoSync(): void {
    if (this.syncInterval) {
      clearInterval(this.syncInterval);
    }
    
    // Initial sync
    this.syncCredentials();
    
    // Set up regular polling
    this.syncInterval = setInterval(() => {
      this.syncCredentials();
    }, this.config.pollingInterval);
  }
  
  /**
   * Stop automatic synchronization
   */
  public stopAutoSync(): void {
    if (this.syncInterval) {
      clearInterval(this.syncInterval);
      this.syncInterval = undefined;
    }
  }
  
  /**
   * Synchronize credentials from the runtime
   * @returns A promise that resolves when sync is complete
   */
  public async syncCredentials(): Promise<WalletCredential[]> {
    if (this.syncInProgress) {
      console.warn('Credential sync already in progress');
      return Array.from(this.credentials.values());
    }
    
    this.syncInProgress = true;
    
    try {
      const response = await axios.get(
        `${this.config.runtimeApiEndpoint}/api/receipts`,
        {
          params: {
            did: this.userDid,
            limit: 100
          }
        }
      );
      
      const receipts: Receipt[] = response.data;
      
      // Process new receipts
      const newCredentials = await Promise.all(
        receipts
          .filter(receipt => !this.credentials.has(receipt.id))
          .map(receipt => this.receiptToCredential(receipt))
      );
      
      // Add new credentials to the store
      newCredentials.forEach(credential => {
        this.credentials.set(credential.id, credential);
      });
      
      return Array.from(this.credentials.values());
    } catch (error) {
      console.error('Failed to sync credentials:', error);
      throw error;
    } finally {
      this.syncInProgress = false;
    }
  }
  
  /**
   * Convert a receipt from the API to a wallet credential
   * @param receipt The receipt from the API
   * @returns A wallet credential
   */
  private async receiptToCredential(receipt: Receipt): Promise<WalletCredential> {
    // Determine credential type based on metadata or pattern matching
    const credentialType = this.determineCredentialType(receipt);
    
    // Calculate trust score based on verification status and other factors
    const trustScore = calculateTrustScore({
      verified: receipt.verified || false,
      verificationCount: receipt.verifications || 0,
      issuerTrust: 0.8, // Can be adjusted based on federation trust
      age: new Date(receipt.created_at),
      type: credentialType
    });
    
    // Create the credential
    const credential: WalletCredential = {
      id: receipt.id,
      title: this.generateCredentialTitle(receipt, credentialType),
      type: credentialType,
      issuer: {
        did: receipt.node_id,
        // Federation name could be fetched and added here
      },
      subjectDid: this.userDid,
      issuanceDate: receipt.created_at,
      credentialSubject: {
        proposalId: receipt.proposal_id,
        executionHash: receipt.execution_hash,
        jobId: receipt.job_id,
        // Additional metadata could be added here
      },
      proof: {
        type: 'Ed25519Signature2020',
        created: receipt.created_at,
        verificationMethod: `${receipt.node_id}#keys-1`,
        proofPurpose: 'assertionMethod',
        proofValue: receipt.signature
      },
      trustLevel: trustScore > 0.7 ? 'High' : (trustScore > 0.4 ? 'Medium' : 'Low'),
      tags: ['federation', 'governance', credentialType],
      metadata: {
        icon: 'shield',
        description: `Federation governance action: ${getCredentialTypeName(credentialType)}`
      }
    };
    
    return credential;
  }
  
  /**
   * Get all synchronized credentials
   */
  public getCredentials(): WalletCredential[] {
    return Array.from(this.credentials.values());
  }
  
  /**
   * Get a credential by ID
   * @param id Credential ID
   */
  public getCredential(id: string): WalletCredential | undefined {
    return this.credentials.get(id);
  }
  
  /**
   * Determine the credential type based on receipt metadata
   * @param receipt The receipt to analyze
   * @returns The credential type
   */
  private determineCredentialType(receipt: Receipt): string {
    // This could be enhanced with more sophisticated logic
    if (receipt.node_id === this.userDid) {
      return 'finalization';
    }
    
    // Default to 'vote' if the user wasn't the finalizer
    return 'vote';
  }
  
  /**
   * Generate a human-readable title for the credential
   * @param receipt The receipt data
   * @param type The credential type
   */
  private generateCredentialTitle(receipt: Receipt, type: string): string {
    const proposalId = receipt.proposal_id ? 
      `${receipt.proposal_id.substring(0, 8)}...` : 
      'Unknown Proposal';
    
    const typeName = getCredentialTypeName(type);
    return `${typeName} for ${proposalId}`;
  }
  
  /**
   * Verify a credential against the runtime
   * @param credentialId The ID of the credential to verify
   */
  public async verifyCredential(credentialId: string): Promise<VerificationResult> {
    const credential = this.credentials.get(credentialId);
    
    if (!credential) {
      return {
        valid: false,
        status: 'error',
        message: 'Credential not found'
      };
    }
    
    try {
      const response = await axios.post(
        `${this.config.runtimeApiEndpoint}/api/receipts/${credentialId}/verify`,
        {}
      );
      
      const result = response.data;
      
      return {
        valid: result.valid,
        status: result.valid ? 'success' : 'error',
        message: result.valid ? 'Credential verified successfully' : 'Credential verification failed',
        details: result
      };
    } catch (error) {
      return {
        valid: false,
        status: 'error',
        message: `Verification failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
        details: error
      };
    }
  }
}

// Export a function to create a credential sync instance
export const createCredentialSync = (
  userDid: DID, 
  runtimeApiEndpoint: string,
  options: Partial<CredentialSyncConfig> = {}
): FederationCredentialSync => {
  return new FederationCredentialSync(userDid, {
    runtimeApiEndpoint,
    ...options
  });
}; 