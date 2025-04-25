/**
 * Cryptographic utilities for federation credentials
 */

/**
 * Calculate SHA-256 hash of data
 * @param data Data to hash, as string or Uint8Array
 * @returns Hex-encoded hash
 */
export async function sha256(data: string | Uint8Array): Promise<string> {
  const encoder = new TextEncoder();
  const dataToHash = typeof data === 'string' ? encoder.encode(data) : data;
  
  // Use the Web Crypto API to create a SHA-256 hash
  const hashBuffer = await crypto.subtle.digest('SHA-256', dataToHash);
  
  // Convert the hash to a hex string
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Create a deterministic ID from a content string
 * @param content Content to create ID from
 * @param prefix Prefix for the ID
 * @returns ID string in the format prefix:hash
 */
export async function createDeterministicId(content: string, prefix: string = 'urn'): Promise<string> {
  const hash = await sha256(content);
  return `${prefix}:${hash.substring(0, 16)}`;
}

/**
 * Create a deterministic credential ID from a subject DID and type
 * @param subjectDid DID of the credential subject
 * @param type Type of the credential
 * @param timestamp Optional timestamp for uniqueness
 * @returns Credential ID
 */
export async function createCredentialId(
  subjectDid: string, 
  type: string, 
  timestamp: string = new Date().toISOString()
): Promise<string> {
  const content = `${subjectDid}:${type}:${timestamp}`;
  return createDeterministicId(content, 'urn:credential');
}

/**
 * Simple encryption using AES-GCM for protecting sensitive data
 * @param data Data to encrypt
 * @param password Password to derive key from
 * @returns Base64-encoded encrypted data with IV
 */
export async function encryptWithPassword(data: string, password: string): Promise<string> {
  const encoder = new TextEncoder();
  const dataBuffer = encoder.encode(data);
  
  // Generate salt and derive key
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const key = await deriveKeyFromPassword(password, salt);
  
  // Generate IV and encrypt
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const encryptedBuffer = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    dataBuffer
  );
  
  // Combine salt, IV, and encrypted data
  const result = new Uint8Array(salt.length + iv.length + encryptedBuffer.byteLength);
  result.set(salt, 0);
  result.set(iv, salt.length);
  result.set(new Uint8Array(encryptedBuffer), salt.length + iv.length);
  
  // Return as base64
  return btoa(String.fromCharCode(...result));
}

/**
 * Decrypt data encrypted with encryptWithPassword
 * @param encryptedData Base64-encoded encrypted data with IV and salt
 * @param password Password used for encryption
 * @returns Decrypted data as string
 */
export async function decryptWithPassword(encryptedData: string, password: string): Promise<string> {
  // Decode base64
  const encryptedBuffer = Uint8Array.from(atob(encryptedData), c => c.charCodeAt(0));
  
  // Extract salt, IV, and encrypted data
  const salt = encryptedBuffer.slice(0, 16);
  const iv = encryptedBuffer.slice(16, 28);
  const data = encryptedBuffer.slice(28);
  
  // Derive key
  const key = await deriveKeyFromPassword(password, salt);
  
  // Decrypt
  const decryptedBuffer = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv },
    key,
    data
  );
  
  // Return as string
  return new TextDecoder().decode(decryptedBuffer);
}

/**
 * Derive a key from a password and salt using PBKDF2
 * @param password Password
 * @param salt Salt
 * @returns Derived key
 */
async function deriveKeyFromPassword(password: string, salt: Uint8Array): Promise<CryptoKey> {
  // Import the password as a key
  const encoder = new TextEncoder();
  const passwordKey = await crypto.subtle.importKey(
    'raw',
    encoder.encode(password),
    { name: 'PBKDF2' },
    false,
    ['deriveKey']
  );
  
  // Derive a key using PBKDF2
  return crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt,
      iterations: 100000,
      hash: 'SHA-256',
    },
    passwordKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
} 