/**
 * Basic Authentication Example
 *
 * Shows how to authenticate with the ICN Gateway API.
 *
 * Run: npx ts-node examples/basic-auth.ts
 */

import { ICNClient, SignatureProvider } from '../src';

// You would replace this with actual Ed25519 signing
// using a library like @noble/ed25519 or tweetnacl
async function createMockSigner(privateKey: Uint8Array): Promise<SignatureProvider> {
  return {
    async sign(challenge: string): Promise<string> {
      // In production, sign the challenge with Ed25519
      // const signature = await ed25519.sign(
      //   new TextEncoder().encode(challenge),
      //   privateKey
      // );
      // return Buffer.from(signature).toString('hex');

      // Mock signature for example
      console.log('Signing challenge:', challenge);
      return 'mock-signature-hex';
    },
  };
}

async function main() {
  // Create client
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
  });

  // Check gateway health
  const health = await client.health();
  console.log('Gateway status:', health.status);
  console.log('Network peers:', health.network_peers);

  // Your DID and private key
  const myDid = 'did:icn:example123';
  const myPrivateKey = new Uint8Array(32); // Your actual private key

  // Method 1: Manual challenge-response flow
  console.log('\n--- Manual Auth Flow ---');
  const challenge = await client.getChallenge(myDid);
  console.log('Got challenge, expires at:', new Date(challenge.expires_at * 1000));

  // Sign the challenge (implement your own signing)
  const signature = 'your-hex-signature';

  // Verify and get token
  const auth = await client.verify(
    myDid,
    signature,
    'my-coop', // cooperative ID
    ['ledger:read', 'ledger:write', 'coop:read'] // scopes
  );
  console.log('Token expires at:', new Date(auth.expires_at * 1000));

  // Set token for future requests
  client.setToken(auth.token);

  // Method 2: Using SignatureProvider (cleaner)
  console.log('\n--- SignatureProvider Auth ---');
  const signer = await createMockSigner(myPrivateKey);

  // This handles the full flow automatically
  const authResult = await client.authenticate(
    myDid,
    signer,
    'my-coop',
    ['ledger:read', 'ledger:write']
  );
  console.log('Authenticated! Token expires at:', new Date(authResult.expires_at * 1000));

  // Now make authenticated requests
  console.log('\n--- Authenticated Requests ---');
  const balance = await client.getBalance('my-coop', myDid);
  console.log('My balance:', balance.balance, balance.currency);
}

main().catch(console.error);
