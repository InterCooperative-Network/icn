import { CredentialService } from './credential-service';
import { WalletCredential } from '../../packages/credential-utils/types';
import { 
  filterCredentialsByFederation,
  groupCredentialsByFederation 
} from '../../packages/credential-utils/utils/federation';
import axios from 'axios';

export interface SearchOptions {
  query?: string;
  federationId?: string;
  type?: string;
  role?: string;
  proposalId?: string;
}

export interface ThreadSearchResult {
  id: string;
  title: string;
  content: string;
  author_did: string;
  created_at: string;
  updated_at: string;
  tags: string[];
  proposal_id?: string;
  federation_id?: string;
  status: string;
  url: string;
}

export interface FederationInfo {
  id: string;
  name: string;
  logo?: string;
}

/**
 * Service for searching credentials and threads with federation awareness
 */
export class SearchService {
  private credentialService: CredentialService;
  private agoraNetEndpoint: string;
  
  constructor(credentialService: CredentialService, agoraNetEndpoint: string) {
    this.credentialService = credentialService;
    this.agoraNetEndpoint = agoraNetEndpoint;
  }
  
  /**
   * Search for credentials matching the specified criteria
   * @param options Search options
   * @returns Matching credentials
   */
  async searchCredentials(options: SearchOptions = {}): Promise<WalletCredential[]> {
    // Get all credentials from the credential service
    const allCredentials = this.credentialService.getCredentials();
    
    // Apply filters based on options
    return allCredentials.filter(credential => {
      // Filter by federation if specified
      if (options.federationId && options.federationId !== 'all') {
        const credentialFederationId = 
          credential.metadata?.federation?.id || 
          credential.metadata?.agoranet?.federation_id || 
          'unfederated';
          
        if (credentialFederationId !== options.federationId) {
          return false;
        }
      }
      
      // Filter by credential type if specified
      if (options.type && options.type !== 'all' && credential.type !== options.type) {
        return false;
      }
      
      // Filter by role if specified
      if (options.role && 
          options.role !== 'all' && 
          credential.credentialSubject.role !== options.role) {
        return false;
      }
      
      // Filter by proposal ID if specified
      if (options.proposalId && 
          (!credential.credentialSubject.proposalId || 
           !credential.credentialSubject.proposalId.includes(options.proposalId))) {
        return false;
      }
      
      // Filter by text search query if specified
      if (options.query) {
        const query = options.query.toLowerCase().trim();
        
        // Search in title
        if (credential.title.toLowerCase().includes(query)) return true;
        
        // Search in credential subject fields
        const subjectStr = JSON.stringify(credential.credentialSubject).toLowerCase();
        if (subjectStr.includes(query)) return true;
        
        // Search in tags if any
        if (credential.tags?.some(tag => tag.toLowerCase().includes(query))) return true;
        
        // Search in issuer name/DID
        if (credential.issuer.name?.toLowerCase().includes(query) || 
            credential.issuer.did.toLowerCase().includes(query)) return true;
            
        // No match found
        return false;
      }
      
      // If we reached here, the credential passed all filters
      return true;
    });
  }
  
  /**
   * Search for threads matching the specified criteria
   * @param options Search options
   * @returns Matching threads
   */
  async searchThreads(options: SearchOptions = {}): Promise<ThreadSearchResult[]> {
    try {
      // Build query parameters for AgoraNet API
      const params: Record<string, string> = {};
      
      if (options.query) {
        params.query = options.query;
      }
      
      if (options.federationId && options.federationId !== 'all') {
        params.federation_id = options.federationId;
      }
      
      if (options.proposalId) {
        params.proposal_id = options.proposalId;
      }
      
      // Make API request to AgoraNet
      const endpoint = `${this.agoraNetEndpoint.replace(/\/$/, '')}/api/threads`;
      const response = await axios.get(endpoint, { params });
      
      // Map response to ThreadSearchResult objects
      return (response.data.threads || []).map((thread: any) => ({
        ...thread,
        url: `${this.agoraNetEndpoint}/threads/${thread.id}`
      }));
    } catch (error) {
      console.error('Error searching threads:', error);
      return [];
    }
  }
  
  /**
   * Get all available federations from the user's credentials
   * @returns Array of federation information
   */
  getFederations(): FederationInfo[] {
    const credentials = this.credentialService.getCredentials();
    const federationMap = new Map<string, FederationInfo>();
    
    credentials.forEach(cred => {
      if (cred.metadata?.federation?.id) {
        federationMap.set(cred.metadata.federation.id, {
          id: cred.metadata.federation.id,
          name: cred.metadata.federation.name || cred.metadata.federation.id,
          logo: cred.metadata.federation.logo
        });
      } else if (cred.metadata?.agoranet?.federation_id) {
        const fedId = cred.metadata.agoranet.federation_id;
        if (!federationMap.has(fedId)) {
          federationMap.set(fedId, {
            id: fedId,
            name: fedId
          });
        }
      }
    });
    
    return Array.from(federationMap.values());
  }
  
  /**
   * Group credentials by federation
   * @param credentials Credentials to group
   * @returns Grouped credentials by federation ID
   */
  groupByFederation(credentials: WalletCredential[]): Record<string, WalletCredential[]> {
    return groupCredentialsByFederation(credentials);
  }
} 