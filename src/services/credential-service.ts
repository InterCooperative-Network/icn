import { WalletCredential, VerificationResult } from '../../packages/credential-utils/types';
import { FederationCredentialSync } from '../federation/credential-sync';
import { receiptToVC, walletCredentialToVC, generateVC } from '../../packages/credential-utils/utils/receiptToVC';
import { exportCredentialAsVC } from '../../packages/credential-utils/utils/credentialExport';
import { DID } from '@icn/identity';

interface CredentialStoreOptions {
  storageKey?: string;
  runtimeApiEndpoint: string;
  autoSync?: boolean;
  syncInterval?: number;
}

/**
 * Service for managing wallet credentials, including synchronization and storage
 */
export class CredentialService {
  private credentialSync: FederationCredentialSync;
  private storageKey: string;
  private cachedCredentials: Map<string, WalletCredential> = new Map();
  
  constructor(userDid: DID, options: CredentialStoreOptions) {
    this.storageKey = options.storageKey || 'icn_wallet_credentials';
    
    // Initialize the credential sync
    this.credentialSync = new FederationCredentialSync(userDid, {
      runtimeApiEndpoint: options.runtimeApiEndpoint,
      autoSync: options.autoSync || false,
      pollingInterval: options.syncInterval || 60000,
    });
    
    // Load existing credentials from storage
    this.loadFromStorage();
  }
  
  /**
   * Load credentials from persistent storage
   */
  private loadFromStorage(): void {
    try {
      const storedData = localStorage.getItem(this.storageKey);
      if (storedData) {
        const parsedData = JSON.parse(storedData) as WalletCredential[];
        parsedData.forEach(credential => {
          this.cachedCredentials.set(credential.id, credential);
        });
      }
    } catch (error) {
      console.error('Error loading credentials from storage:', error);
    }
  }
  
  /**
   * Save credentials to persistent storage
   */
  private saveToStorage(): void {
    try {
      const credentialsArray = Array.from(this.cachedCredentials.values());
      localStorage.setItem(this.storageKey, JSON.stringify(credentialsArray));
    } catch (error) {
      console.error('Error saving credentials to storage:', error);
    }
  }
  
  /**
   * Start automatic synchronization of credentials
   */
  public startSync(): void {
    this.credentialSync.startAutoSync();
  }
  
  /**
   * Stop automatic synchronization of credentials
   */
  public stopSync(): void {
    this.credentialSync.stopAutoSync();
  }
  
  /**
   * Manually synchronize credentials from the runtime
   */
  public async syncCredentials(): Promise<WalletCredential[]> {
    try {
      const syncedCredentials = await this.credentialSync.syncCredentials();
      
      // Update local cache with new credentials
      syncedCredentials.forEach(credential => {
        this.cachedCredentials.set(credential.id, credential);
      });
      
      // Save to persistent storage
      this.saveToStorage();
      
      return syncedCredentials;
    } catch (error) {
      console.error('Failed to sync credentials:', error);
      throw error;
    }
  }
  
  /**
   * Get all credentials
   */
  public getCredentials(): WalletCredential[] {
    return Array.from(this.cachedCredentials.values());
  }
  
  /**
   * Get a credential by ID
   */
  public getCredential(id: string): WalletCredential | undefined {
    return this.cachedCredentials.get(id);
  }
  
  /**
   * Filter credentials by type
   */
  public getCredentialsByType(type: string): WalletCredential[] {
    return this.getCredentials().filter(cred => cred.type === type);
  }
  
  /**
   * Export a credential as a verifiable credential JSON
   */
  public exportCredential(id: string): void {
    const credential = this.cachedCredentials.get(id);
    if (credential) {
      exportCredentialAsVC(credential);
    } else {
      throw new Error(`Credential with ID ${id} not found`);
    }
  }
  
  /**
   * Export multiple credentials as a verifiable presentation
   */
  public exportPresentation(credentialIds: string[], holderDid: string): string {
    const credentials = credentialIds
      .map(id => this.cachedCredentials.get(id))
      .filter((cred): cred is WalletCredential => cred !== undefined);
    
    const presentation = generateVC(credentials, holderDid);
    return JSON.stringify(presentation, null, 2);
  }
  
  /**
   * Verify a credential against the runtime
   */
  public async verifyCredential(id: string): Promise<VerificationResult> {
    return this.credentialSync.verifyCredential(id);
  }
  
  /**
   * Delete a credential from storage
   */
  public deleteCredential(id: string): boolean {
    const deleted = this.cachedCredentials.delete(id);
    if (deleted) {
      this.saveToStorage();
    }
    return deleted;
  }
  
  /**
   * Clear all credentials
   */
  public clearCredentials(): void {
    this.cachedCredentials.clear();
    this.saveToStorage();
  }
}

/**
 * Create a new credential service instance
 */
export const createCredentialService = (
  userDid: DID,
  runtimeApiEndpoint: string,
  options: Partial<CredentialStoreOptions> = {}
): CredentialService => {
  return new CredentialService(userDid, {
    runtimeApiEndpoint,
    ...options,
  });
}; 