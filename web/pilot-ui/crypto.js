/**
 * ICN Crypto Module - WebCrypto-based Identity and Signing
 *
 * Provides:
 * - Ed25519 keypair generation
 * - DID derivation from public key
 * - IndexedDB encrypted storage
 * - Challenge-response signing
 */

// IndexedDB configuration
const DB_NAME = 'icn-identity';
const DB_VERSION = 1;
const STORE_NAME = 'keypairs';

/**
 * Initialize IndexedDB for storing encrypted keypairs
 */
async function initDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);

        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);

        request.onupgradeneeded = (event) => {
            const db = event.target.result;
            if (!db.objectStoreNames.contains(STORE_NAME)) {
                db.createObjectStore(STORE_NAME, { keyPath: 'did' });
            }
        };
    });
}

/**
 * Base58 encoding (Bitcoin alphabet)
 * Used for DID encoding
 */
const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

function base58Encode(buffer) {
    const bytes = new Uint8Array(buffer);
    const digits = [0];

    for (let i = 0; i < bytes.length; i++) {
        let carry = bytes[i];
        for (let j = 0; j < digits.length; j++) {
            carry += digits[j] << 8;
            digits[j] = carry % 58;
            carry = (carry / 58) | 0;
        }
        while (carry > 0) {
            digits.push(carry % 58);
            carry = (carry / 58) | 0;
        }
    }

    // Handle leading zeros
    for (let i = 0; i < bytes.length && bytes[i] === 0; i++) {
        digits.push(0);
    }

    return digits.reverse().map(d => BASE58_ALPHABET[d]).join('');
}

function base58Decode(str) {
    const bytes = [0];

    for (let i = 0; i < str.length; i++) {
        const c = str[i];
        const digit = BASE58_ALPHABET.indexOf(c);
        if (digit < 0) throw new Error('Invalid base58 character');

        let carry = digit;
        for (let j = 0; j < bytes.length; j++) {
            carry += bytes[j] * 58;
            bytes[j] = carry & 0xff;
            carry >>= 8;
        }
        while (carry > 0) {
            bytes.push(carry & 0xff);
            carry >>= 8;
        }
    }

    // Handle leading '1's (zeros)
    for (let i = 0; i < str.length && str[i] === '1'; i++) {
        bytes.push(0);
    }

    return new Uint8Array(bytes.reverse());
}

/**
 * Derive DID from Ed25519 public key
 * Format: did:icn:<base58-pubkey>
 */
function deriveDID(publicKey) {
    const base58Pubkey = base58Encode(publicKey);
    return `did:icn:${base58Pubkey}`;
}

/**
 * Extract public key bytes from DID
 */
function publicKeyFromDID(did) {
    if (!did.startsWith('did:icn:')) {
        throw new Error('Invalid DID format');
    }
    const base58Part = did.substring(8); // Remove 'did:icn:' prefix
    return base58Decode(base58Part);
}

/**
 * Generate a new Ed25519 keypair
 * Returns { publicKey: Uint8Array, privateKey: Uint8Array, did: string }
 */
async function generateKeypair() {
    // Generate Ed25519 keypair using WebCrypto
    const keypair = await crypto.subtle.generateKey(
        {
            name: 'Ed25519',
        },
        true, // extractable
        ['sign', 'verify']
    );

    // Export keys as raw bytes
    const publicKeyBytes = new Uint8Array(await crypto.subtle.exportKey('raw', keypair.publicKey));
    const privateKeyBytes = new Uint8Array(await crypto.subtle.exportKey('pkcs8', keypair.privateKey));

    const did = deriveDID(publicKeyBytes);

    return {
        publicKey: publicKeyBytes,
        privateKey: privateKeyBytes,
        did: did,
        cryptoKeypair: keypair // Keep the CryptoKey objects for signing
    };
}

/**
 * Encrypt data using a password-derived key
 * Uses PBKDF2 + AES-GCM
 */
async function encryptWithPassword(data, password) {
    const encoder = new TextEncoder();
    const salt = crypto.getRandomValues(new Uint8Array(16));
    const iv = crypto.getRandomValues(new Uint8Array(12));

    // Derive key from password
    const passwordKey = await crypto.subtle.importKey(
        'raw',
        encoder.encode(password),
        'PBKDF2',
        false,
        ['deriveKey']
    );

    const key = await crypto.subtle.deriveKey(
        {
            name: 'PBKDF2',
            salt: salt,
            iterations: 100000,
            hash: 'SHA-256'
        },
        passwordKey,
        { name: 'AES-GCM', length: 256 },
        false,
        ['encrypt']
    );

    // Encrypt the data
    const encrypted = await crypto.subtle.encrypt(
        { name: 'AES-GCM', iv: iv },
        key,
        typeof data === 'string' ? encoder.encode(data) : data
    );

    // Combine salt + iv + encrypted data
    const result = new Uint8Array(salt.length + iv.length + encrypted.byteLength);
    result.set(salt, 0);
    result.set(iv, salt.length);
    result.set(new Uint8Array(encrypted), salt.length + iv.length);

    return result;
}

/**
 * Decrypt data using a password-derived key
 */
async function decryptWithPassword(encryptedData, password) {
    const encoder = new TextEncoder();
    const data = new Uint8Array(encryptedData);

    // Extract salt, iv, and ciphertext
    const salt = data.slice(0, 16);
    const iv = data.slice(16, 28);
    const ciphertext = data.slice(28);

    // Derive key from password
    const passwordKey = await crypto.subtle.importKey(
        'raw',
        encoder.encode(password),
        'PBKDF2',
        false,
        ['deriveKey']
    );

    const key = await crypto.subtle.deriveKey(
        {
            name: 'PBKDF2',
            salt: salt,
            iterations: 100000,
            hash: 'SHA-256'
        },
        passwordKey,
        { name: 'AES-GCM', length: 256 },
        false,
        ['decrypt']
    );

    // Decrypt
    const decrypted = await crypto.subtle.decrypt(
        { name: 'AES-GCM', iv: iv },
        key,
        ciphertext
    );

    return new Uint8Array(decrypted);
}

/**
 * Store keypair in IndexedDB (encrypted)
 */
async function storeKeypair(did, keypair, password) {
    const db = await initDB();

    // Serialize keypair
    const serialized = JSON.stringify({
        publicKey: Array.from(keypair.publicKey),
        privateKey: Array.from(keypair.privateKey),
        did: did
    });

    // Encrypt with password
    const encrypted = await encryptWithPassword(serialized, password);

    return new Promise((resolve, reject) => {
        const tx = db.transaction([STORE_NAME], 'readwrite');
        const store = tx.objectStore(STORE_NAME);
        const request = store.put({
            did: did,
            encrypted: Array.from(encrypted),
            createdAt: Date.now()
        });

        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

/**
 * Load keypair from IndexedDB (decrypt)
 */
async function loadKeypair(did, password) {
    const db = await initDB();

    return new Promise(async (resolve, reject) => {
        const tx = db.transaction([STORE_NAME], 'readonly');
        const store = tx.objectStore(STORE_NAME);
        const request = store.get(did);

        request.onsuccess = async () => {
            const result = request.result;
            if (!result) {
                reject(new Error('Keypair not found'));
                return;
            }

            try {
                // Decrypt
                const encryptedBytes = new Uint8Array(result.encrypted);
                const decrypted = await decryptWithPassword(encryptedBytes, password);
                const decoder = new TextDecoder();
                const serialized = JSON.parse(decoder.decode(decrypted));

                // Reconstruct keypair
                const keypair = {
                    publicKey: new Uint8Array(serialized.publicKey),
                    privateKey: new Uint8Array(serialized.privateKey),
                    did: serialized.did
                };

                // Import private key as CryptoKey for signing
                const privateKeyCrypto = await crypto.subtle.importKey(
                    'pkcs8',
                    keypair.privateKey,
                    { name: 'Ed25519' },
                    true,
                    ['sign']
                );

                keypair.cryptoKeypair = { privateKey: privateKeyCrypto };

                resolve(keypair);
            } catch (err) {
                reject(new Error('Failed to decrypt keypair (wrong password?)'));
            }
        };

        request.onerror = () => reject(request.error);
    });
}

/**
 * List all stored DIDs
 */
async function listStoredDIDs() {
    const db = await initDB();

    return new Promise((resolve, reject) => {
        const tx = db.transaction([STORE_NAME], 'readonly');
        const store = tx.objectStore(STORE_NAME);
        const request = store.getAllKeys();

        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

/**
 * Sign a message with a keypair
 * Returns base64-encoded signature
 */
async function signMessage(message, keypair) {
    const encoder = new TextEncoder();
    const messageBytes = typeof message === 'string' ? encoder.encode(message) : message;

    const signature = await crypto.subtle.sign(
        'Ed25519',
        keypair.cryptoKeypair.privateKey,
        messageBytes
    );

    // Return as base64
    return btoa(String.fromCharCode(...new Uint8Array(signature)));
}

/**
 * Sign a challenge (for challenge-response auth)
 */
async function signChallenge(challenge, keypair) {
    return signMessage(challenge, keypair);
}

// Export public API
window.ICNCrypto = {
    generateKeypair,
    storeKeypair,
    loadKeypair,
    listStoredDIDs,
    signMessage,
    signChallenge,
    deriveDID,
    publicKeyFromDID
};
