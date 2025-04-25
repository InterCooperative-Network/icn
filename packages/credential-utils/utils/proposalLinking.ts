import axios from 'axios';
import { WalletCredential } from '../types';

/**
 * Interface for the AgoraNet credential linking response
 */
interface CredentialLinkResponse {
  linked_credential: {
    id: string;
    credential_id: string;
    proposal_id: string;
    issuer_did: string;
    subject_did: string;
    credential_type: string;
    thread_id: string;
    created_at: string;
  };
  thread_url: string;
}

/**
 * Options for linking a credential to an AgoraNet thread
 */
export interface CredentialLinkOptions {
  /**
   * The AgoraNet endpoint URL
   */
  agoraNetEndpoint: string;
  
  /**
   * Optional specific thread ID to link to
   * If not provided, AgoraNet will find a thread by proposal ID
   */
  threadId?: string;
  
  /**
   * Additional metadata to include with the link
   */
  metadata?: Record<string, any>;
}

/**
 * Result of linking a credential to an AgoraNet thread
 */
export interface CredentialLinkResult {
  /**
   * Success status of the operation
   */
  success: boolean;
  
  /**
   * URL to the linked thread
   */
  threadUrl?: string;
  
  /**
   * ID of the linked thread
   */
  threadId?: string;
  
  /**
   * Error message if the operation failed
   */
  error?: string;
}

/**
 * Link a governance credential to an AgoraNet discussion thread
 * 
 * @param credential The credential to link to an AgoraNet thread
 * @param options Options for linking
 * @returns Result of the linking operation
 */
export async function linkCredentialToAgoraThread(
  credential: WalletCredential,
  options: CredentialLinkOptions
): Promise<CredentialLinkResult> {
  try {
    // Ensure the credential has a proposal ID
    const proposalId = credential.credentialSubject.proposalId;
    if (!proposalId) {
      return {
        success: false,
        error: 'Credential does not contain a proposal ID'
      };
    }
    
    // Construct API endpoint URL
    const endpoint = `${options.agoraNetEndpoint.replace(/\/$/, '')}/api/threads/credential-link`;
    
    // Create request payload
    const payload = {
      credential_id: credential.id,
      proposal_id: proposalId,
      issuer_did: credential.issuer.did,
      subject_did: credential.subjectDid,
      credential_type: credential.type,
      thread_id: options.threadId,
      metadata: {
        ...options.metadata,
        credential_title: credential.title,
        credential_type: credential.type,
        trust_level: credential.trustLevel,
        tags: credential.tags
      }
    };
    
    // Make the API request
    const response = await axios.post(endpoint, payload);
    
    // Parse the response
    const result = response.data as CredentialLinkResponse;
    
    return {
      success: true,
      threadUrl: result.thread_url,
      threadId: result.linked_credential.thread_id
    };
  } catch (error) {
    console.error('Error linking credential to AgoraNet:', error);
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error occurred'
    };
  }
}

/**
 * Get all credentials linked to a specific thread
 * 
 * @param agoraNetEndpoint The AgoraNet endpoint URL
 * @param threadId ID of the thread to get links for
 * @returns Array of linked credential metadata
 */
export async function getThreadLinkedCredentials(
  agoraNetEndpoint: string,
  threadId: string
): Promise<any[]> {
  try {
    const endpoint = `${agoraNetEndpoint.replace(/\/$/, '')}/api/threads/credential-links`;
    const response = await axios.get(endpoint, {
      params: { thread_id: threadId }
    });
    
    return response.data.linked_credentials || [];
  } catch (error) {
    console.error('Error fetching linked credentials:', error);
    return [];
  }
}

/**
 * Get all threads linked to a specific credential
 * 
 * @param agoraNetEndpoint The AgoraNet endpoint URL
 * @param credentialId ID of the credential to find links for
 * @returns Array of thread IDs and URLs
 */
export async function getCredentialLinkedThreads(
  agoraNetEndpoint: string,
  credentialId: string
): Promise<{ threadId: string; threadUrl: string }[]> {
  try {
    const endpoint = `${agoraNetEndpoint.replace(/\/$/, '')}/api/threads/credential-links`;
    const response = await axios.get(endpoint, {
      params: { credential_id: credentialId }
    });
    
    return (response.data.linked_credentials || []).map((link: any) => ({
      threadId: link.thread_id,
      threadUrl: `/threads/${link.thread_id}`
    }));
  } catch (error) {
    console.error('Error fetching linked threads:', error);
    return [];
  }
} 