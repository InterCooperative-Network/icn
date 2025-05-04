/**
 * Simple DID resolver for did:key identifiers
 * @returns {Object} A resolver that can resolve did:key identifiers
 */
export function getResolver() {
  return {
    /**
     * Resolve a DID to a DID Document
     * @param {string} did - The DID to resolve
     * @returns {Promise<Object>} The resolved DID Document
     */
    resolve: async (did) => {
      // Basic validation
      if (!did.startsWith('did:')) {
        throw new Error(`Invalid DID: ${did}`);
      }
      
      if (did.startsWith('did:key:')) {
        return resolveDidKey(did);
      }
      
      if (did.startsWith('did:icn:')) {
        return resolveDidIcn(did);
      }
      
      throw new Error(`Unsupported DID method: ${did}`);
    }
  };
}

/**
 * Resolves a did:key identifier
 * @param {string} did - The did:key identifier
 * @returns {Object} The resolved DID Document
 */
function resolveDidKey(did) {
  // Extract the public key from the DID
  const publicKeyBase58 = did.split(':')[2];
  
  // Construct a minimal DID Document
  return {
    '@context': 'https://w3id.org/did/v1',
    id: did,
    verificationMethod: [
      {
        id: `${did}#keys-1`,
        type: 'Ed25519VerificationKey2018',
        controller: did,
        publicKeyBase58
      }
    ],
    authentication: [`${did}#keys-1`],
    assertionMethod: [`${did}#keys-1`]
  };
}

/**
 * Resolves a did:icn identifier by making a request to the runtime
 * @param {string} did - The did:icn identifier
 * @returns {Promise<Object>} The resolved DID Document
 */
async function resolveDidIcn(did) {
  try {
    // In a production environment, you'd make a network request to resolve the DID
    // For example:
    // const response = await fetch(`/api/runtime/identity/did/${encodeURIComponent(did)}`);
    // const didDoc = await response.json();
    
    // For simplicity, we'll create a mock DID document
    const mockPublicKeyBase58 = 'zLfzQvrQtxQCVMNbEfcHTpzLGosUgLxkPC2HKwx4D1XGZ1s';
    
    return {
      '@context': 'https://w3id.org/did/v1',
      id: did,
      verificationMethod: [
        {
          id: `${did}#keys-1`,
          type: 'Ed25519VerificationKey2018',
          controller: did,
          publicKeyBase58: mockPublicKeyBase58
        }
      ],
      authentication: [`${did}#keys-1`],
      assertionMethod: [`${did}#keys-1`]
    };
  } catch (error) {
    console.error('Error resolving ICN DID:', error);
    throw error;
  }
} 