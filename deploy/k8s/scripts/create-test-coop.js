#!/usr/bin/env node
/**
 * Create a test cooperative on the ICN gateway
 *
 * Usage: node create-test-coop.js
 */

const crypto = require('crypto');

const GATEWAY_URL = 'https://api.icn.zone';
const COOP_ID = 'test-coop';
const COOP_NAME = 'Test Cooperative';

// Generate Ed25519 keypair
function generateKeyPair() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');

  // Get raw bytes
  const pubKeyDer = publicKey.export({ type: 'spki', format: 'der' });
  const privKeyDer = privateKey.export({ type: 'pkcs8', format: 'der' });

  // Ed25519 public key is the last 32 bytes of the SPKI structure
  const pubKeyBytes = pubKeyDer.slice(-32);

  // Create DID from public key using multibase base58btc
  const did = `did:icn:${base58btcEncode(pubKeyBytes)}`;

  return { publicKey, privateKey, pubKeyBytes, did };
}

// Base58btc encoding (Bitcoin alphabet) with multibase prefix
function base58btcEncode(bytes) {
  const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let num = BigInt('0x' + Buffer.from(bytes).toString('hex'));
  let result = '';

  while (num > 0n) {
    const remainder = Number(num % 58n);
    num = num / 58n;
    result = ALPHABET[remainder] + result;
  }

  // Handle leading zeros
  for (const byte of bytes) {
    if (byte === 0) {
      result = '1' + result;
    } else {
      break;
    }
  }

  // Add multibase prefix 'z' for base58btc
  return 'z' + result;
}

async function main() {
  console.log('=== ICN Test Cooperative Setup ===\n');

  // Generate keypair
  console.log('1. Generating Ed25519 keypair...');
  const { privateKey, did } = generateKeyPair();
  console.log(`   DID: ${did}\n`);

  // Request challenge
  console.log('2. Requesting authentication challenge...');
  const challengeRes = await fetch(`${GATEWAY_URL}/v1/auth/challenge`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ did }),
  });

  if (!challengeRes.ok) {
    const error = await challengeRes.text();
    throw new Error(`Challenge request failed: ${error}`);
  }

  const challengeData = await challengeRes.json();
  console.log(`   Nonce: ${challengeData.nonce.substring(0, 16)}...`);
  console.log(`   Expires in: ${challengeData.expires_in}s\n`);

  // Sign the challenge
  console.log('3. Signing challenge...');
  const nonceBytes = Buffer.from(challengeData.nonce, 'hex');
  const signature = crypto.sign(null, nonceBytes, privateKey);
  const signatureHex = signature.toString('hex');
  console.log(`   Signature: ${signatureHex.substring(0, 32)}...\n`);

  // Verify and get token
  console.log('4. Verifying signature and getting token...');
  const verifyRes = await fetch(`${GATEWAY_URL}/v1/auth/verify`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      did,
      signature: signatureHex,
      coop_id: COOP_ID,
      scopes: ['coop:write', 'coop:read', 'coop:admin', 'ledger:read', 'ledger:write'],
    }),
  });

  if (!verifyRes.ok) {
    const error = await verifyRes.text();
    throw new Error(`Verification failed: ${error}`);
  }

  const authData = await verifyRes.json();
  console.log(`   Token received (expires in ${authData.expires_in}s)\n`);

  // Create cooperative
  console.log('5. Creating cooperative...');
  const createRes = await fetch(`${GATEWAY_URL}/v1/coops`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${authData.token}`,
    },
    body: JSON.stringify({
      id: COOP_ID,
      name: COOP_NAME,
    }),
  });

  if (!createRes.ok) {
    const error = await createRes.text();
    if (createRes.status === 409) {
      console.log('   Cooperative already exists!\n');
    } else {
      throw new Error(`Create coop failed: ${error}`);
    }
  } else {
    const coopData = await createRes.json();
    console.log(`   Created: ${coopData.name} (${coopData.id})`);
    console.log(`   Steward: ${coopData.members?.[0]?.did || did}\n`);
  }

  console.log('=== Setup Complete ===');
  console.log(`\nCooperative "${COOP_NAME}" is ready at ${GATEWAY_URL}`);
  console.log(`Coop ID: ${COOP_ID}`);
  console.log(`Steward DID: ${did}`);
}

main().catch(err => {
  console.error('\nError:', err.message);
  process.exit(1);
});
