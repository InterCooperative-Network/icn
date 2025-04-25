/**
 * Identity utilities for federation-scoped credentials
 */
import * as jose from 'jose';
import { createDid, isValidDid, extractDIDScope, extractDIDUsername } from './did';
import { v4 as uuidv4 } from 'uuid';
import { sha256 } from './crypto';
import { 
  ScopedIdentity, 
  KeyType, 
  KeyMaterial,
  FederationMember,
  SignatureParams,
  SignatureResult
} from '../types/identity';

/**
 * Create a federation-scoped DID with optional fragment
 * @param federationId The federation ID 
 * @param memberId The member identifier within the federation
 * @param fragment Optional fragment to append to the DID
 * @returns The scoped DID string
 */
export function createScopedDID(federationId: string, memberId: string, fragment?: string): string {
  const did = `did:icn:${federationId}:${memberId}`;
  return fragment ? `${did}#${fragment}` : did;
}

/**
 * Create a federation member DID with standard controller fragment
 * @param federationId The federation ID
 * @param memberId The member identifier within the federation
 * @returns The federation member DID with controller fragment
 */
export function createFederationMemberDID(federationId: string, memberId: string): string {
  return createScopedDID(federationId, memberId, 'controller');
}

/**
 * Generate key material for a new identity
 * @param keyType Type of key to generate
 * @returns Generated key material
 */
export async function generateKeyMaterial(keyType: KeyType = KeyType.Ed25519): Promise<KeyMaterial> {
  switch (keyType) {
    case KeyType.Ed25519: {
      const { publicKey, privateKey } = await jose.generateKeyPair('EdDSA');
      const publicKeyJwk = await jose.exportJWK(publicKey);
      const privateKeyJwk = await jose.exportJWK(privateKey);
      
      return {
        keyType: KeyType.Ed25519,
        publicKeyBase64: Buffer.from(JSON.stringify(publicKeyJwk)).toString('base64'),
        privateKeyBase64: Buffer.from(JSON.stringify(privateKeyJwk)).toString('base64'),
        keyId: `key-${uuidv4().slice(0, 8)}`
      };
    }
    
    case KeyType.Ecdsa: {
      const { publicKey, privateKey } = await jose.generateKeyPair('ES256');
      const publicKeyJwk = await jose.exportJWK(publicKey);
      const privateKeyJwk = await jose.exportJWK(privateKey);
      
      return {
        keyType: KeyType.Ecdsa,
        publicKeyBase64: Buffer.from(JSON.stringify(publicKeyJwk)).toString('base64'),
        privateKeyBase64: Buffer.from(JSON.stringify(privateKeyJwk)).toString('base64'),
        keyId: `key-${uuidv4().slice(0, 8)}`
      };
    }
    
    case KeyType.Secp256k1: {
      // Placeholder, would use actual secp256k1 implementation
      const { publicKey, privateKey } = await jose.generateKeyPair('ES256K');
      const publicKeyJwk = await jose.exportJWK(publicKey);
      const privateKeyJwk = await jose.exportJWK(privateKey);
      
      return {
        keyType: KeyType.Secp256k1,
        publicKeyBase64: Buffer.from(JSON.stringify(publicKeyJwk)).toString('base64'),
        privateKeyBase64: Buffer.from(JSON.stringify(privateKeyJwk)).toString('base64'),
        keyId: `key-${uuidv4().slice(0, 8)}`
      };
    }
    
    default:
      throw new Error(`Unsupported key type: ${keyType}`);
  }
}

/**
 * Create a new ScopedIdentity
 * @param scope The identity scope (e.g., federation name)
 * @param username The username within the scope
 * @param keyType The key type to generate
 * @returns A new scoped identity
 */
export async function createScopedIdentity(
  scope: string,
  username: string,
  keyType: KeyType = KeyType.Ed25519
): Promise<ScopedIdentity> {
  const keyMaterial = await generateKeyMaterial(keyType);
  const did = createDid(scope, username);
  const now = new Date().toISOString();
  
  return {
    did,
    scope,
    username,
    keyMaterial,
    createdAt: now,
    updatedAt: now,
    metadata: {}
  };
}

/**
 * Create a federation member identity
 * @param federationId The federation ID
 * @param username The member username
 * @param role The role of the member in the federation
 * @param weight The voting weight of the member
 * @param keyType The key type to use
 * @returns A new federation member identity
 */
export async function createFederationMember(
  federationId: string,
  username: string,
  role: string,
  weight: number = 1,
  keyType: KeyType = KeyType.Ed25519
): Promise<FederationMember> {
  const base = await createScopedIdentity(federationId, username, keyType);
  const now = new Date().toISOString();
  
  return {
    ...base,
    federationMembership: {
      federationId,
      role: {
        role,
        weight,
      },
      memberSince: now,
    },
    federationMetadata: {
      weight,
      canSignCredentials: true,
      canApproveAmendments: true,
      canInitiateRecovery: role === 'admin',
    }
  };
}

/**
 * Create a signature using an identity's private key
 * @param params Signature parameters
 * @param privateKeyBase64 Base64-encoded private key
 * @returns Signature result
 */
export async function createSignature(
  params: SignatureParams,
  privateKeyBase64: string
): Promise<SignatureResult> {
  const now = new Date().toISOString();
  
  // Convert data to string if it's a Uint8Array
  const dataToSign = params.data instanceof Uint8Array 
    ? Buffer.from(params.data).toString('utf8')
    : params.data;
    
  // Parse private key
  const privateKeyObj = JSON.parse(Buffer.from(privateKeyBase64, 'base64').toString('utf8'));
  const privateKey = await jose.importJWK(privateKeyObj);
  
  // Get the appropriate algorithm based on key type
  const alg = privateKeyObj.crv === 'Ed25519' ? 'EdDSA' 
    : privateKeyObj.crv === 'P-256' ? 'ES256'
    : privateKeyObj.crv === 'secp256k1' ? 'ES256K'
    : 'EdDSA'; // default
    
  // Sign the data
  const jws = await new jose.CompactSign(new TextEncoder().encode(dataToSign))
    .setProtectedHeader({ alg, typ: 'JWT' })
    .sign(privateKey);
    
  // Create verification method ID
  const verificationMethod = params.federationId 
    ? `${params.signerDid}#controller`
    : `${params.signerDid}#${privateKeyObj.kid || 'keys-1'}`;
    
  // Return formatted signature
  return {
    signerDid: params.signerDid,
    signature: jws,
    verificationMethod,
    created: now,
    proofPurpose: 'assertionMethod',
    signatureType: params.signatureType || 'JWS'
  };
}

/**
 * Verify a signature using the signer's public key
 * @param data The data that was signed
 * @param signature The signature to verify
 * @param publicKeyBase64 Base64-encoded public key
 * @returns True if signature is valid, false otherwise
 */
export async function verifySignature(
  data: string | Uint8Array,
  signature: string,
  publicKeyBase64: string
): Promise<boolean> {
  try {
    // Convert data to buffer if it's a string
    const dataBuffer = typeof data === 'string'
      ? new TextEncoder().encode(data)
      : data;
      
    // Parse public key
    const publicKeyObj = JSON.parse(Buffer.from(publicKeyBase64, 'base64').toString('utf8'));
    const publicKey = await jose.importJWK(publicKeyObj);
    
    // Verify the signature
    const { payload } = await jose.compactVerify(signature, publicKey);
    
    // Compare the payload with the original data
    const payloadStr = new TextDecoder().decode(payload);
    const dataStr = typeof data === 'string' ? data : new TextDecoder().decode(dataBuffer);
    
    return payloadStr === dataStr;
  } catch (error) {
    console.error('Signature verification failed:', error);
    return false;
  }
}

/**
 * Add identity proof to a verifiable credential
 * @param credential The credential to add proof to
 * @param signerIdentity The identity of the signer
 * @param privateKeyBase64 Base64-encoded private key
 * @returns The credential with proof added
 */
export async function addIdentityProofToCredential(
  credential: any,
  signerIdentity: ScopedIdentity | FederationMember,
  privateKeyBase64: string
): Promise<any> {
  // Create a copy of the credential without the proof
  const credentialWithoutProof = { ...credential };
  delete credentialWithoutProof.proof;
  
  // Convert the credential to a string for signing
  const dataToSign = JSON.stringify(credentialWithoutProof);
  
  // Create signature params
  const params: SignatureParams = {
    signerDid: signerIdentity.did,
    data: dataToSign,
    signatureType: 'JWS',
  };
  
  // Add federation ID if this is a federation member
  if ('federationMembership' in signerIdentity) {
    params.federationId = signerIdentity.federationMembership.federationId;
  }
  
  // Create the signature
  const signatureResult = await createSignature(params, privateKeyBase64);
  
  // Create the proof object
  const proof = {
    type: 'JwsSignature2020',
    created: signatureResult.created,
    verificationMethod: signatureResult.verificationMethod,
    proofPurpose: signatureResult.proofPurpose,
    jws: signatureResult.signature
  };
  
  // Return the credential with proof
  return {
    ...credential,
    proof
  };
}

/**
 * Creates a DID Document for a scoped identity
 * @param identity The scoped identity
 * @returns DID Document object
 */
export function createDIDDocument(identity: ScopedIdentity): any {
  const keyId = `${identity.did}#${identity.keyMaterial.keyId || 'keys-1'}`;
  
  // Parse the public key
  const publicKeyObj = JSON.parse(
    Buffer.from(identity.keyMaterial.publicKeyBase64, 'base64').toString('utf8')
  );
  
  // Determine verification method type based on key type
  let verificationMethodType: string;
  switch (identity.keyMaterial.keyType) {
    case KeyType.Ed25519:
      verificationMethodType = 'Ed25519VerificationKey2020';
      break;
    case KeyType.Ecdsa:
      verificationMethodType = 'EcdsaSecp256r1VerificationKey2019';
      break;
    case KeyType.Secp256k1:
      verificationMethodType = 'EcdsaSecp256k1VerificationKey2019';
      break;
    default:
      verificationMethodType = 'JsonWebKey2020';
  }
  
  return {
    '@context': [
      'https://www.w3.org/ns/did/v1',
      'https://w3id.org/security/suites/jws-2020/v1'
    ],
    id: identity.did,
    controller: identity.did,
    verificationMethod: [
      {
        id: keyId,
        type: verificationMethodType,
        controller: identity.did,
        publicKeyJwk: publicKeyObj
      }
    ],
    authentication: [keyId],
    assertionMethod: [keyId],
    // Add federation specific information if this is a federation member
    ...('federationMembership' in identity) && {
      service: [
        {
          id: `${identity.did}#federation`,
          type: 'FederationMembership',
          serviceEndpoint: `federation:${(identity as FederationMember).federationMembership.federationId}`,
          role: (identity as FederationMember).federationMembership.role.role,
          memberSince: (identity as FederationMember).federationMembership.memberSince
        }
      ]
    }
  };
} 