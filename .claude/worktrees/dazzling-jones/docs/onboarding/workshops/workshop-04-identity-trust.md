# Workshop 4: Identity and Trust Hands-On

## Goal
Practice creating identities, understanding the keystore, and exploring trust
graph operations.

## Prerequisites
- Completed Module 4 reading
- ICN binaries built (`cargo build --release`)

## Estimated time
2-3 hours

## Part 1: Create and Inspect an Identity

### Steps
1. Create a temporary data directory:
   ```bash
   export ICN_DATA=$(mktemp -d)
   export ICN_PASSPHRASE="workshop-test-passphrase"
   ```

2. Initialize a new identity:
   ```bash
   ./target/release/icnctl --data-dir "$ICN_DATA" id init
   ```

3. Show the identity:
   ```bash
   ./target/release/icnctl --data-dir "$ICN_DATA" id show
   ```

### Expected output
You should see:
- A DID in the format `did:icn:<base58-public-key>`
- The public key fingerprint
- Keystore version information

### Questions to answer
1. What cryptographic algorithm is used for the keypair?
2. Where is the encrypted keystore file stored?
3. What happens if you run `id show` without setting `ICN_PASSPHRASE`?

### Checkpoint
- [ ] You created a new identity successfully
- [ ] You can explain the DID format

## Part 2: Explore the Keystore Code

### Steps
1. Open `icn/crates/icn-identity/src/keystore.rs`
2. Find the `KeyStore` trait definition
3. Locate the encryption implementation

### Questions to answer
1. What encryption scheme is used for the keystore?
2. What is the purpose of the `lock()` method?
3. How does the code handle invalid passphrases?

### Code snippet to find
Look for the Age encryption usage:
```rust
// The keystore uses Age encryption
let encryptor = age::Encryptor::with_user_passphrase(...);
```

### Checkpoint
- [ ] You found the encryption implementation
- [ ] You can explain the security boundary the keystore provides

## Part 3: Identity Bundle Deep Dive

### Steps
1. Open `icn/crates/icn-identity/src/bundle.rs`
2. Find the `IdentityBundle` struct
3. Trace how the TLS binding signature is created

### Questions to answer
1. What does `tls_binding_sig` prove?
2. Why is the TLS cert bound to the DID key?
3. What happens during key rotation?

### Checkpoint
- [ ] You understand DID-TLS binding
- [ ] You can explain why binding is important for transport security

## Part 4: Trust Graph Exploration

### Steps
1. Open `icn/crates/icn-trust/src/types.rs`
2. List all `TrustGraphType` variants
3. Open `icn/crates/icn-trust/src/lib.rs`
4. Find the methods for adding/querying trust edges

### Expected findings
Trust graph types:
- Social: relationships between members
- EconomicReliability: payment history, creditworthiness
- TechnicalReliability: uptime, network behavior

### Questions to answer
1. Why are trust dimensions modeled separately?
2. How would a "well-known partner" differ from a "new member" in trust scores?
3. What is transitive trust and how is it computed?

### Checkpoint
- [ ] You can list all trust dimensions
- [ ] You understand why dimensions are independent

## Part 5: Trust-Gated Rate Limiting

### Steps
1. Search for rate limit usage in the codebase:
   ```bash
   grep -r "rate_limit" icn/crates/ --include="*.rs" | head -20
   ```

2. Find where trust class determines rate limits
3. Map trust levels to their rate limits:

| Trust Class | Trust Score Range | Rate Limit |
|-------------|------------------|------------|
| Isolated | < 0.1 | ? msg/sec |
| Known | 0.1 - 0.4 | ? msg/sec |
| Partner | 0.4 - 0.7 | ? msg/sec |
| Federated | > 0.7 | ? msg/sec |

### Expected values
From `docs/security/production-hardening.md`:
- Isolated: 10 msg/sec
- Known: 50 msg/sec
- Partner: 100 msg/sec
- Federated: 200 msg/sec

### Checkpoint
- [ ] You found the trust-gated rate limiting code
- [ ] You understand how trust influences system access

## Part 6: Key Rotation Exercise

### Steps
1. Using your test identity from Part 1, rotate the key:
   ```bash
   ./target/release/icnctl --data-dir "$ICN_DATA" id rotate
   ```

2. Show the identity again:
   ```bash
   ./target/release/icnctl --data-dir "$ICN_DATA" id show
   ```

### Questions to answer
1. Did the DID change after rotation?
2. What happens to old signatures after rotation?
3. Where is the key history stored?

### Checkpoint
- [ ] You successfully rotated a key
- [ ] You understand the implications of key rotation

## Cleanup

Remove the temporary data directory:
```bash
rm -rf "$ICN_DATA"
```

## Summary
After completing this workshop you should be able to:
- Create and manage ICN identities via CLI
- Navigate the keystore and bundle code
- Understand trust graph dimensions and their purposes
- Explain how trust influences system behavior

## Troubleshooting

### "Failed to unlock keystore"
Ensure `ICN_PASSPHRASE` environment variable is set correctly.

### "Keystore not found"
The `--data-dir` path must match where you initialized the identity.

### "Permission denied"
Check file permissions on the data directory.

## Next steps
Proceed to Module 5: Network and Gossip
