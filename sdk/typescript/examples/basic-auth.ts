/**
 * Basic Authentication Example — DEV/DEMO ONLY
 *
 * Shows DID key authentication (challenge/verify) against the ICN Gateway API.
 *
 * IMPORTANT: this demonstrates DID *key control* plus dev/demo self-serve token
 * issuance, where the caller supplies its own `coop_id`. DID key control is NOT
 * cooperative authority. Passing a caller-chosen `coop_id` to `/auth/verify` is
 * fail-closed in production (PR #2077); the gateway honors it only under an explicit
 * dev opt-in (ICN_DEV_MODE or the daemon's --insecure-gateway-no-jwt) on a loopback
 * bind. This is NOT how production
 * cooperative authority is obtained — trusted issuance is tracked by #2080. In
 * production, obtain a cooperative-scoped token from a trusted institutional path
 * and pass it to the client directly (see examples/seed-demo-data.ts).
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
  console.log('Got challenge, expires in:', challenge.expires_in, 'seconds');

  // Sign the challenge (implement your own signing)
  const signature = 'your-hex-signature';

  // Verify and get token (DEV/DEMO ONLY — self-asserted coop_id, fail-closed in production)
  const auth = await client.verify(
    myDid,
    signature,
    'my-coop', // self-asserted coop_id (dev/demo only)
    ['ledger:read', 'ledger:write', 'coop:read'] // scopes
  );
  console.log('Token expires at:', new Date(auth.expires_at));

  // Set token for future requests
  client.setToken(auth.token);

  // Method 2: Using SignatureProvider (cleaner)
  console.log('\n--- SignatureProvider Auth ---');
  const signer = await createMockSigner(myPrivateKey);

  // This handles the full flow automatically (still DEV/DEMO ONLY — self-asserted coop_id)
  const authResult = await client.authenticate(
    myDid,
    signer,
    'my-coop', // self-asserted coop_id (dev/demo only)
    ['ledger:read', 'ledger:write']
  );
  console.log('Authenticated! Token expires at:', new Date(authResult.expires_at));

  // Now make authenticated requests
  console.log('\n--- Authenticated Requests ---');
  const position = await client.getPosition('my-coop', myDid);
  console.log('My position:', position.position, position.unit);
}

main().catch(console.error);
