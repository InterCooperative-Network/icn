# Canonical Encoding Specification

**Version:** 1.0  
**Status:** NORMATIVE  
**Last Updated:** 2026-02-02

## Abstract

This document specifies the deterministic serialization rules for ICN federation protocol objects. Canonical encoding ensures that identical logical objects produce byte-identical serializations, enabling cryptographic verification across implementations.

## Keywords

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

---

## 1. Overview

Canonical encoding is REQUIRED for:
- Objects that are cryptographically signed
- Objects that are cryptographically hashed
- Objects included in consensus state
- Objects used in cross-implementation verification

Non-canonical encoding MAY be used for:
- Wire protocol serialization (where explicit versioning is present)
- Local storage (where only one implementation reads the data)
- Debug output and logging

---

## 2. Encoding Format

### 2.1 Primary Format: CBOR

ICN uses **CBOR (RFC 8949)** as its canonical encoding format with the following constraints:

#### 2.1.1 Definite Length Encoding

All CBOR objects MUST use definite-length encoding:
- Arrays MUST use definite-length array encoding (major type 4)
- Maps MUST use definite-length map encoding (major type 5)
- Byte strings MUST use definite-length encoding (major type 2)
- Text strings MUST use definite-length encoding (major type 3)

Indefinite-length encoding (break stop code 0xFF) MUST NOT be used.

#### 2.1.2 Integer Encoding

Integers MUST be encoded in their shortest form:
- Values 0-23: Encode in initial byte (major type 0 or 1)
- Values 24-255: Use 1-byte argument (additional information 24)
- Values 256-65535: Use 2-byte argument (additional information 25)
- Values 65536-4294967295: Use 4-byte argument (additional information 26)
- Values ≥4294967296: Use 8-byte argument (additional information 27)

Negative integers MUST use major type 1 (negative integer).

#### 2.1.3 Map Key Ordering

All CBOR maps MUST have keys sorted in bytewise lexicographic order:

```rust
// CORRECT: Keys sorted by byte representation
{
  "action_hash": ...,
  "action_type": ...,
  "confirmations": ...,
  "state_root": ...,
  "timestamp": ...
}

// INCORRECT: Keys in insertion order
{
  "timestamp": ...,
  "action_type": ...,
  "state_root": ...,
  "action_hash": ...,
  "confirmations": ...
}
```

**Implementation Note:** Use `BTreeMap<K, V>` in Rust, not `HashMap<K, V>`.

#### 2.1.4 Floating Point

Floating-point numbers SHOULD be avoided in canonical encoding contexts.

If floating-point values are unavoidable, they MUST be encoded as:
- 64-bit IEEE 754 binary64 (CBOR major type 7, additional information 27)
- NaN values MUST use the canonical NaN representation (0x7FF8000000000000)
- Negative zero MUST be normalized to positive zero

**Rationale:** Floating-point representations are non-deterministic across architectures and rounding modes. Use rational numbers or fixed-point arithmetic instead.

### 2.2 Fallback Format: Canonical JSON

For human-readable serialization (e.g., debugging, configuration), **Canonical JSON (RFC 8785)** MAY be used:

#### 2.2.1 Key Ordering

Object keys MUST be sorted lexicographically by UTF-16 code unit:

```json
{
  "action_hash": "...",
  "action_type": "...",
  "confirmations": [...],
  "state_root": "...",
  "timestamp": 1735905000
}
```

#### 2.2.2 Whitespace

Canonical JSON MUST NOT contain:
- Insignificant whitespace (spaces, tabs, newlines outside strings)
- Comments

#### 2.2.3 Number Encoding

Numbers MUST be encoded without leading zeros or trailing decimal points:
- `42` (not `042` or `42.0`)
- `-123` (not `-0123`)

#### 2.2.4 String Escaping

String escaping MUST follow JSON rules:
- Control characters (U+0000 to U+001F) MUST be escaped
- Forward slash (U+002F) MAY be escaped (optional)
- Unicode escapes MUST use lowercase hexadecimal

### 2.3 Wire Format: Postcard

For network transmission, ICN uses **Postcard** (compact binary format):

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Message {
    // ...
}

let encoded = postcard::to_allocvec(&msg)?;
let decoded: Message = postcard::from_bytes(&encoded)?;
```

Postcard is NOT canonical and MUST NOT be used for:
- Signature payloads
- Hash preimages
- Consensus state

---

## 3. Data Type Constraints

### 3.1 Ordered Collections

**CRITICAL:** Data structures that are hashed or signed MUST use ordered collections:

```rust
// CORRECT: Deterministic ordering
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
pub struct GovernanceProof {
    pub metadata: BTreeMap<String, String>,  // ✅ Ordered
    pub confirmations: Vec<Did>,              // ✅ Sorted (see 3.2)
}

// INCORRECT: Non-deterministic ordering
// This example shows what NOT to do - HashMap iteration order is undefined,
// causing different implementations to produce different byte sequences for
// the same logical data, breaking cross-language interoperability.
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct BadGovernanceProof {
    pub metadata: HashMap<String, String>,    // ❌ Non-deterministic iteration order
    pub confirmations: Vec<Did>,              // ⚠️  Must be sorted (but not enforced here)
}
```

**Rules:**
- Use `BTreeMap<K, V>`, not `HashMap<K, V>` (deterministic key ordering)
- Use `BTreeSet<T>`, not `HashSet<T>` (deterministic element ordering)
- For `Vec<T>` representing sets, explicitly sort before encoding (see 3.2)

### 3.2 Set-Like Vectors

When using `Vec<T>` to represent a set (unordered collection), elements MUST be sorted before canonical encoding:

```rust
impl GovernanceProof {
    pub fn canonicalize(&mut self) {
        // Sort confirmations (DIDs are UTF-8 strings)
        self.confirmations.sort();
        
        // Sort allocations by recipient DID
        self.allocations.sort_by_key(|a| a.recipient.clone());
    }
}
```

**Sorting Rules:**
- String-like types (Did, CurrencyId, CellId): UTF-8 lexicographic order
- Numeric types: Numeric ascending order
- Compound types: Sort by first differing field (lexicographic tuple order)

### 3.3 Optional Fields

Optional fields MUST be handled consistently:

```rust
#[derive(Serialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub description: String,
    
    // CBOR: Omit field if None (default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}
```

**Rules:**
- `None` values SHOULD be omitted from CBOR maps
- Empty strings MUST be encoded as empty strings, not omitted
- Empty collections (Vec, BTreeMap) MUST be encoded as empty, not omitted

### 3.4 Binary Data

Binary data MUST be encoded as:
- **CBOR:** Byte string (major type 2)
- **JSON:** Hexadecimal string with `0x` prefix or Base58btc multibase

```rust
#[derive(Serialize, Deserialize)]
pub struct SignedData {
    #[serde(with = "hex::serde")]
    pub signature: [u8; 64],  // JSON: "0xabcd..." / CBOR: byte string
}
```

### 3.5 Enum Serialization

Enum types (such as `ActionType`, `ProposalState`, `DecisionOutcome`) MUST serialize consistently across implementations:

**String Serialization (for JSON and human-readable formats):**
- Enum variants MUST serialize as **snake_case** ASCII strings
- Transformation rule: Convert PascalCase to snake_case by inserting underscores before uppercase letters (except the first) and lowercasing all characters
- Acronyms are treated as single words and lowercased as a unit

**Examples:**
```rust
ActionType::SettleCrossCoop   → "settle_cross_coop"
ActionType::AdmitMember       → "admit_member"
ActionType::RotateKey         → "rotate_key"
ActionType::DIDRotate         → "did_rotate"  // Acronym treated as single word
```

**Numeric Serialization (for binary formats):**
- When serializing to u8 (e.g., in CBOR or state root computation), use the explicit numeric mapping defined for each enum type
- Numeric mappings are defined in the specification for each enum (see GOVERNANCE_STATE_MACHINE.md for ProposalState mapping)

**Cross-Language Consistency:**
All implementations MUST produce identical string and numeric representations for the same enum variant to ensure interoperability.

---

## 4. Cryptographic Primitives

### 4.1 Hash Functions

#### 4.1.1 BLAKE3

ICN uses **BLAKE3** for content hashing:

```rust
use blake3;

fn hash_content(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}
```

**Properties:**
- Output: 32 bytes (256 bits)
- Collision resistance: 128-bit security
- Preimage resistance: 256-bit security

#### 4.1.2 Domain Separation

All hashes MUST use domain separation to prevent cross-protocol attacks:

```rust
fn typed_hash(domain: &str, data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.as_bytes());
    hasher.update(&[0x00]);  // Null separator
    hasher.update(data);
    hasher.finalize().into()
}
```

**Format:** `BLAKE3(domain || 0x00 || data)`

**Domain Strings:**
- `icn-federation:governance-proof:v1`
- `icn-federation:action:v1`
- `icn-federation:state-root:v1`
- `icn-federation:constitution:v1`
- `icn-governance:proposal:v1`
- `icn-governance:vote:v1`
- `icn-ledger:settlement:v1`
- `icn-ledger:journal-entry:v1`

### 4.2 Signatures

#### 4.2.1 Ed25519

ICN uses **Ed25519** (RFC 8032) for digital signatures:

```rust
use ed25519_dalek::{Signer, Verifier, SigningKey, VerifyingKey};

// Signing
let signature = signing_key.sign(canonical_payload);

// Verification
verifying_key.verify(canonical_payload, &signature)?;
```

**Properties:**
- Public key: 32 bytes
- Signature: 64 bytes
- Security: ~128-bit (curve25519)

**Payload:** MUST be the canonical CBOR encoding of the signed object.

#### 4.2.2 ML-DSA (Post-Quantum)

For post-quantum security, ICN uses **ML-DSA (Dilithium)** in hybrid mode:

```rust
pub struct HybridSignature {
    pub ed25519: ed25519_dalek::Signature,  // 64 bytes
    pub ml_dsa: Vec<u8>,                    // ~2420 bytes (ML-DSA-65)
}
```

Verification MUST succeed for BOTH signatures.

#### 4.2.3 Signature Encoding

Signatures in canonical objects MUST be encoded as:
- **CBOR:** Byte string (64 bytes for Ed25519, struct for hybrid)
- **JSON:** Hexadecimal string with `0x` prefix

```rust
#[derive(Serialize, Deserialize)]
pub struct GovernanceProof {
    // ...
    #[serde(with = "hex::serde")]
    pub signature: [u8; 64],
}
```

---

## 5. Identifier Normalization

### 5.1 DID (Decentralized Identifier)

DIDs MUST be normalized before encoding:

**Format:** `did:icn:<base58btc-pubkey>`

**Normalization:**
1. Lowercase the `did` and `icn` components
2. Preserve case of the Base58btc identifier (case-sensitive)
3. Verify the identifier is valid multibase Base58btc
4. Verify the decoded public key is exactly 32 bytes

```rust
impl Did {
    pub fn normalize(&self) -> Result<Did> {
        let s = self.0.as_str();
        if !s.starts_with("did:icn:") {
            return Err(Error::InvalidDidPrefix);
        }
        
        let identifier = &s[8..];  // Skip "did:icn:"
        
        // Verify Base58btc encoding
        let decoded = bs58::decode(identifier).into_vec()?;
        if decoded.len() != 32 {
            return Err(Error::InvalidDidLength);
        }
        
        Ok(Did(s.to_string()))
    }
}
```

### 5.2 CurrencyId

Currency identifiers MUST be normalized:

**Format:** `<scope>:<symbol>` (e.g., `food-coop:HOURS`)

**Normalization:**
1. Lowercase the scope component
2. Uppercase the symbol component
3. Verify scope matches `[a-z0-9-]+`
4. Verify symbol matches `[A-Z]{2,6}`

```rust
impl CurrencyId {
    pub fn normalize(&self) -> Result<CurrencyId> {
        let parts: Vec<&str> = self.0.split(':').collect();
        if parts.len() != 2 {
            return Err(Error::InvalidCurrencyFormat);
        }
        
        let scope = parts[0].to_lowercase();
        let symbol = parts[1].to_uppercase();
        
        // Validate format
        if !scope.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(Error::InvalidScope);
        }
        if !symbol.chars().all(|c| c.is_ascii_uppercase()) || symbol.len() < 2 || symbol.len() > 6 {
            return Err(Error::InvalidSymbol);
        }
        
        Ok(CurrencyId(format!("{}:{}", scope, symbol)))
    }
}
```

### 5.3 CellId

Cell identifiers MUST be normalized:

**Format:** `cell:<base58btc-hash>` (e.g., `cell:z3v8AqW...`)

**Normalization:**
1. Lowercase the `cell` prefix
2. Preserve case of the Base58btc hash (case-sensitive)
3. Verify the identifier is valid multibase Base58btc
4. Verify the decoded hash is exactly 32 bytes

---

## 6. Integer Overflow Protection

### 6.1 Checked Arithmetic

All arithmetic operations in canonical encoding contexts MUST use checked arithmetic:

```rust
impl Settlement {
    pub fn compute_balance(&self) -> Result<i64> {
        let mut balance: i64 = 0;
        
        for txn in &self.transactions {
            // CORRECT: Checked addition
            balance = balance.checked_add(txn.amount)
                .ok_or(Error::IntegerOverflow)?;
        }
        
        Ok(balance)
    }
}
```

**Rules:**
- Use `checked_add`, `checked_sub`, `checked_mul`
- Never use `+`, `-`, `*` for consensus-critical arithmetic
- Return `Error::IntegerOverflow` on overflow
- Explicitly document overflow behavior in spec

### 6.2 Range Validation

Numeric fields MUST validate ranges before encoding:

```rust
impl GovernanceProof {
    pub fn validate(&self) -> Result<()> {
        // Timestamp: Must be within reasonable bounds
        if self.timestamp == 0 || self.timestamp > 2147483647 {
            return Err(Error::InvalidTimestamp);
        }
        
        // Sequence: Must be monotonically increasing
        if self.sequence == 0 {
            return Err(Error::InvalidSequence);
        }
        
        Ok(())
    }
}
```

---

## 7. Canonical Encoding Trait

### 7.1 Canonicalize Trait

All objects that require canonical encoding MUST implement the `Canonicalize` trait:

```rust
pub trait Canonicalize {
    /// Modify self to canonical form (sort collections, normalize identifiers)
    fn canonicalize(&mut self);
    
    /// Verify self is in canonical form
    fn is_canonical(&self) -> bool;
}

pub trait CanonicalEncode: Canonicalize + Serialize {
    /// Encode to canonical CBOR
    fn encode_canonical(&mut self) -> Result<Vec<u8>> {
        self.canonicalize();
        let mut buf = Vec::new();
        ciborium::ser::into_writer(self, &mut buf)
            .map_err(|e| Error::EncodingError(e.to_string()))?;
        Ok(buf)
    }
    
    /// Compute typed hash
    fn hash_canonical(&mut self, domain: &str) -> Result<[u8; 32]> {
        let encoded = self.encode_canonical()?;
        Ok(typed_hash(domain, &encoded))
    }
}
```

### 7.2 Example Implementation

```rust
impl Canonicalize for GovernanceProof {
    fn canonicalize(&mut self) {
        // Sort confirmations (DIDs)
        self.confirmations.sort();
        
        // Sort decision records by key
        // (BTreeMap is already ordered)
        
        // Normalize identifiers
        self.federation_id = self.federation_id.normalize().unwrap();
        
        // Clamp timestamps to valid range
        if self.timestamp > 2147483647 {
            self.timestamp = 2147483647;
        }
    }
    
    fn is_canonical(&self) -> bool {
        // Check if confirmations are sorted
        for i in 1..self.confirmations.len() {
            if self.confirmations[i] <= self.confirmations[i - 1] {
                return false;
            }
        }
        
        // Check identifier normalization
        // Note: federation_id is a String; to verify normalization, parse as Did
        if let Ok(did) = Did::new(&self.federation_id) {
            if let Ok(normalized) = did.normalize() {
                if self.federation_id != normalized.as_str() {
                    return false;
                }
            } else {
                return false; // Invalid DID format
            }
        } else {
            return false; // Not a valid DID
        }
        
        true
    }
}

impl CanonicalEncode for GovernanceProof {}
```

---

## 8. Test Vectors

### 8.1 Minimal GovernanceProof

**Input:**
```json
{
  "federation_id": "fed:z5AqBkLuA...",
  "action_type": "SettleCrossCoop",
  "action_hash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
  "state_root": "0xfedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
  "prev_state_root": "0x0000000000000000000000000000000000000000000000000000000000000000",
  "sequence": 1,
  "timestamp": 1735905000,
  "confirmations": [
    "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
    "did:icn:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd"
  ],
  "decision_records": {},
  "signature": "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}
```

**Canonical CBOR (hex):**
```
a9                                      # map(9)
   6c 666564657261 74696f6e5f6964       # text(13) "federation_id"
   71 6665643a7a 354171426b4c75 41...   # text(17) "fed:z5AqBkLuA..."
   6b 616374696f 6e5f74797065           # text(11) "action_type"
   70 536574746c 6543726f7373 436f6f70 # text(16) "SettleCrossCoop"
   6b 616374696f 6e5f68617368           # text(11) "action_hash"
   5820 12345678 90abcdef 12345678...   # bytes(32)
   6a 73746174 655f726f6f74             # text(10) "state_root"
   5820 fedcba98 76543210 fedcba98...   # bytes(32)
   70 707265765f 73746174 655f726f6f74 # text(15) "prev_state_root"
   5820 00000000 00000000 00000000...   # bytes(32)
   68 73657175 656e6365                 # text(8) "sequence"
   01                                   # unsigned(1)
   69 74696d65 7374616d70               # text(9) "timestamp"
   1a 67679a88                          # unsigned(1735905000)
   6d 636f6e66 69726d61 74696f6e73     # text(13) "confirmations"
   82                                   # array(2)
      78 3c 6469643a 69636e3a 7a364d6b 686158674... # text(60) "did:icn:z6Mkha..."
      78 3c 6469643a 69636e3a 7a364d6b 664e7a54... # text(60) "did:icn:z6MkfN..."
   70 64656369 73696f6e 5f726563 6f726473 # text(16) "decision_records"
   a0                                   # map(0)
   69 7369676e 61747572 65             # text(9) "signature"
   5840 01234567 89abcdef 01234567...   # bytes(64)
```

**Typed Hash (domain: `icn-federation:governance-proof:v1`):**
```
0x3f8c9d2e1a7b6c4d5e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d
```

### 8.2 Action Hash Computation

**Input FederationAction (SettleCrossCoop):**
```json
{
  "settlements": [
    {
      "from_coop": "did:icn:food-coop:z6Mkh...",
      "to_coop": "did:icn:tool-coop:z6Mkf...",
      "amount": 1000,
      "currency": "food-coop:HOURS"
    }
  ]
}
```

**Canonical CBOR:**
```
a1                                      # map(1)
   6b 73657474 6c656d65 6e7473         # text(11) "settlements"
   81                                   # array(1)
      a4                                # map(4)
         69 66726f6d 5f636f6f 70       # text(9) "from_coop"
         78 2c 6469643a 69636e3a 666f6f642d... # text(44) "did:icn:food-coop:..."
         67 746f5f63 6f6f70             # text(7) "to_coop"
         78 2c 6469643a 69636e3a 746f6f6c2d... # text(44) "did:icn:tool-coop:..."
         66 616d6f75 6e74                # text(6) "amount"
         1903e8                          # unsigned(1000)
         68 63757272 656e6379           # text(8) "currency"
         72 666f6f64 2d636f6f 703a484f 555253 # text(18) "food-coop:HOURS"
```

**Typed Hash (domain: `icn-federation:action:v1`):**
```
0x7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b
```

---

## 9. Compliance Checklist

Implementations MUST satisfy the following requirements:

### 9.1 Encoding Format

- [ ] Uses CBOR for canonical encoding
- [ ] Uses definite-length encoding (no break codes)
- [ ] Encodes integers in shortest form
- [ ] Sorts CBOR map keys bytewise lexicographically
- [ ] Uses `BTreeMap<K, V>`, not `HashMap<K, V>`
- [ ] Uses `BTreeSet<T>`, not `HashSet<T>`
- [ ] Sorts set-like `Vec<T>` before encoding

### 9.2 Cryptographic Primitives

- [ ] Uses BLAKE3 for content hashing
- [ ] Uses typed hashing with domain separation
- [ ] Uses Ed25519 for signatures
- [ ] Supports ML-DSA for post-quantum signatures
- [ ] Verifies signatures against canonical payloads

### 9.3 Identifiers

- [ ] Normalizes DIDs before encoding
- [ ] Normalizes CurrencyIds before encoding
- [ ] Normalizes CellIds before encoding
- [ ] Validates identifier formats

### 9.4 Arithmetic

- [ ] Uses checked arithmetic for all consensus-critical operations
- [ ] Validates numeric ranges before encoding
- [ ] Returns errors on overflow/underflow

### 9.5 Trait Implementation

- [ ] Implements `Canonicalize` trait for all signed/hashed objects
- [ ] Implements `CanonicalEncode` trait
- [ ] Provides `is_canonical()` verification

### 9.6 Test Coverage

- [ ] Includes test vectors for all canonical objects
- [ ] Tests hash stability across versions
- [ ] Tests cross-implementation compatibility
- [ ] Tests edge cases (empty collections, max values, special characters)

---

## 10. Security Considerations

### 10.1 Hash Collision Attacks

**Threat:** An attacker creates two different objects with the same hash.

**Mitigation:**
- Use BLAKE3 (128-bit collision resistance)
- Use typed hashing to prevent cross-protocol collisions
- Validate object structure before hashing

### 10.2 Signature Malleability

**Threat:** An attacker modifies a signature without invalidating it.

**Mitigation:**
- Ed25519 signatures are non-malleable (RFC 8032)
- Verify signatures against canonical payloads only
- Reject non-canonical encodings

### 10.3 Integer Overflow

**Threat:** Arithmetic overflow causes incorrect state transitions.

**Mitigation:**
- Use checked arithmetic
- Validate ranges before encoding
- Use larger integer types where appropriate (i64, u64)

### 10.4 Identifier Confusion

**Threat:** Similar-looking identifiers are confused (homograph attack).

**Mitigation:**
- Normalize identifiers before comparison
- Validate identifier formats
- Use case-sensitive comparison for Base58btc components

### 10.5 Canonicalization Bypass

**Threat:** An attacker submits non-canonical objects that pass validation.

**Mitigation:**
- Always canonicalize before signing/hashing
- Reject non-canonical objects at verification
- Implement `is_canonical()` checks

---

## 11. References

- [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) - Key words for use in RFCs
- [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949) - Concise Binary Object Representation (CBOR)
- [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) - JSON Canonicalization Scheme (JCS)
- [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032) - Edwards-Curve Digital Signature Algorithm (EdDSA)
- [BLAKE3 Specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf)
- [Multibase Specification](https://github.com/multiformats/multibase)

---

## Appendix A: Rust Implementation Example

```rust
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;

/// Trait for objects requiring canonical encoding
pub trait Canonicalize {
    fn canonicalize(&mut self);
    fn is_canonical(&self) -> bool;
}

/// Trait for canonical CBOR encoding
pub trait CanonicalEncode: Canonicalize + Serialize {
    fn encode_canonical(&mut self) -> Result<Vec<u8>, Error> {
        self.canonicalize();
        let mut buf = Vec::new();
        ciborium::ser::into_writer(self, &mut buf)
            .map_err(|e| Error::EncodingError(e.to_string()))?;
        Ok(buf)
    }
    
    fn hash_canonical(&mut self, domain: &str) -> Result<[u8; 32], Error> {
        let encoded = self.encode_canonical()?;
        Ok(typed_hash(domain, &encoded))
    }
}

/// Typed hash with domain separation
pub fn typed_hash(domain: &str, data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.as_bytes());
    hasher.update(&[0x00]);
    hasher.update(data);
    (*hasher.finalize().as_bytes()).into()
}

/// Example: GovernanceProof with canonical encoding
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GovernanceProof {
    pub federation_id: String,
    pub action_type: ActionType,
    pub action_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub prev_state_root: [u8; 32],
    pub sequence: u64,
    pub timestamp: u64,
    pub decision_records: BTreeMap<String, Vec<u8>>,  // Ordered map
    /// Multi-signature map: keyed by signer DID (string form), values are Ed25519 signatures.
    /// BTreeMap ensures deterministic ordering for canonical encoding.
    pub signatures: BTreeMap<String, [u8; 64]>,
}

impl Canonicalize for GovernanceProof {
    fn canonicalize(&mut self) {
        // BTreeMap fields (signatures, decision_records) are already ordered by key
        
        // Normalize identifiers (omitted for brevity)
    }
    
    fn is_canonical(&self) -> bool {
        // BTreeMap guarantees sorted keys, so signatures are already canonical
        true
    }
}

impl CanonicalEncode for GovernanceProof {}
```

---

**Document Status:** NORMATIVE - Implementations MUST comply with this specification.

**Change History:**
- 2026-02-02: Initial version 1.0
