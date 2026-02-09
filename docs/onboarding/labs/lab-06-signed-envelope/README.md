# Lab 06: Signed Envelopes with Replay Protection

## Goal
Implement cryptographic signatures and replay attack prevention.

## Requirements

### 1. Signed Envelope
```rust
pub struct SignedEnvelope {
    pub payload: Vec<u8>,
    pub sender_did: String,
    pub sequence: u64,      // For replay protection
    pub signature: Vec<u8>,  // Ed25519 signature
}

impl SignedEnvelope {
    pub fn sign(payload: Vec<u8>, keypair: &Keypair, sequence: u64) -> Self {
        let message = [&payload[..], &sequence.to_le_bytes()].concat();
        let signature = keypair.sign(&message);
        
        SignedEnvelope {
            payload,
            sender_did: did_from_pubkey(keypair.public()),
            sequence,
            signature: signature.to_bytes().to_vec(),
        }
    }
    
    pub fn verify(&self, public_key: &PublicKey) -> Result<(), VerifyError> {
        let message = [&self.payload[..], &self.sequence.to_le_bytes()].concat();
        let signature = Signature::from_bytes(&self.signature)?;
        
        public_key.verify(&message, &signature)
            .map_err(|_| VerifyError::InvalidSignature)
    }
}
```

### 2. Replay Guard
```rust
pub struct ReplayGuard {
    last_seen: HashMap<String, u64>,  // DID → highest sequence
}

impl ReplayGuard {
    pub fn check(&mut self, envelope: &SignedEnvelope) -> Result<(), ReplayError> {
        let last = self.last_seen.get(&envelope.sender_did).unwrap_or(&0);
        
        if envelope.sequence <= *last {
            return Err(ReplayError::StaleSequence {
                sender: envelope.sender_did.clone(),
                received: envelope.sequence,
                expected: *last + 1,
            });
        }
        
        self.last_seen.insert(envelope.sender_did.clone(), envelope.sequence);
        Ok(())
    }
}
```

## Tests to Write

### Test 1: Valid Signature Verifies
```rust
#[test]
fn test_valid_signature() {
    let keypair = Keypair::generate();
    let payload = b"hello world".to_vec();
    
    let envelope = SignedEnvelope::sign(payload, &keypair, 1);
    assert!(envelope.verify(keypair.public()).is_ok());
}
```

### Test 2: Tampered Payload Fails
```rust
#[test]
fn test_tampered_payload_fails() {
    let keypair = Keypair::generate();
    let payload = b"hello world".to_vec();
    
    let mut envelope = SignedEnvelope::sign(payload, &keypair, 1);
    envelope.payload = b"goodbye world".to_vec();  // Tamper!
    
    assert!(envelope.verify(keypair.public()).is_err());
}
```

### Test 3: Wrong Key Fails
```rust
#[test]
fn test_wrong_key_fails() {
    let keypair1 = Keypair::generate();
    let keypair2 = Keypair::generate();
    
    let envelope = SignedEnvelope::sign(b"test".to_vec(), &keypair1, 1);
    assert!(envelope.verify(keypair2.public()).is_err());
}
```

### Test 4: Replay Attack Caught
```rust
#[test]
fn test_replay_attack_caught() {
    let keypair = Keypair::generate();
    let mut guard = ReplayGuard::new();
    
    let env1 = SignedEnvelope::sign(b"msg1".to_vec(), &keypair, 1);
    let env2 = SignedEnvelope::sign(b"msg2".to_vec(), &keypair, 2);
    let env1_replay = env1.clone();
    
    assert!(guard.check(&env1).is_ok());
    assert!(guard.check(&env2).is_ok());
    assert!(guard.check(&env1_replay).is_err());  // Replay caught!
}
```

### Test 5: Out-of-Order Rejected
```rust
#[test]
fn test_out_of_order_rejected() {
    let keypair = Keypair::generate();
    let mut guard = ReplayGuard::new();
    
    let env1 = SignedEnvelope::sign(b"msg1".to_vec(), &keypair, 1);
    let env3 = SignedEnvelope::sign(b"msg3".to_vec(), &keypair, 3);
    let env2 = SignedEnvelope::sign(b"msg2".to_vec(), &keypair, 2);
    
    assert!(guard.check(&env1).is_ok());
    assert!(guard.check(&env3).is_ok());  // Gap allowed (may arrive later)
    assert!(guard.check(&env2).is_err()); // Old sequence rejected
}
```

## Done When
- Valid signatures verify correctly
- Tampered messages fail verification
- Wrong key fails verification
- Replay attacks caught by ReplayGuard
- Out-of-order messages rejected
- All tests pass

## Resources
- `icn-net/src/envelope.rs` (SignedEnvelope)
- `icn-identity/` (Ed25519 keypairs)
- `ed25519-dalek` crate docs
