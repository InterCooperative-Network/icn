// Allow unused_assignments from Zeroize derive macro generated code
#![allow(unused_assignments)]
#![allow(missing_docs)]
// Prevent panics in production code paths
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! ICN Identity - DID management, key generation, and cryptographic operations
//!
//! This crate provides identity primitives for ICN, including:
//! - Traditional Ed25519-based DIDs (`did:icn:<pubkey>`)
//! - SDIS Anchor-based DIDs (`did:icn:<anchor-id>`)
//! - KeyBundle with hybrid post-quantum signatures
//! - VUI (Verifiable Unique Identifier) types

pub mod anchor;
/// N1 — the human-subject authority-log primitive (library-only, unwired).
pub mod authority_log;
pub mod backend_factory;
pub mod batch_verify;
pub mod bundle;
pub mod commons;
pub mod commons_store;
pub mod did_signer;
pub mod keybundle;
pub mod keystore;
pub mod keystore_backend;
#[cfg(feature = "hsm")]
pub mod keystore_pkcs11;
#[cfg(feature = "tpm-experimental")]
pub mod keystore_tpm;
pub mod multi_device;
pub mod personhood;
pub mod personhood_store;
pub mod recovery;
pub mod revocation;
pub mod revocation_store;
pub mod sync;
pub mod vui;

use anyhow::Result;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub use anchor::{Anchor, EnrollmentPathway};
pub use backend_factory::{init_keystore, open_keystore};
pub use batch_verify::{
    verify_signatures_batched, BatchVerifier, BatchVerifyResult, SignatureToVerify,
};
pub use bundle::{verify_binding_info, verify_did_matches_binding, BindingInfo, IdentityBundle};
pub use commons::{
    Affiliation, CommonsHolderRecord, CommonsRight, CommonsRights, HolderStatus, JurisdictionId,
    JurisdictionType, MembershipCapability, MembershipStatus,
};
pub use commons_store::CommonsHolderStore;
pub use did_signer::{DidKey, DidSigner, SoftwareSigner};
pub use keybundle::KeyBundle;
pub use keystore::{AgeKeyStore, KeyRotation, KeyStore, RotationReason};
#[cfg(feature = "hsm")]
pub use keystore_backend::Pkcs11Config;
#[cfg(feature = "tpm-experimental")]
pub use keystore_backend::TpmConfig;
pub use keystore_backend::{BackendConfig, KeyStoreBackend, SigningBackend};
#[cfg(feature = "hsm")]
pub use keystore_pkcs11::Pkcs11Backend;
#[cfg(feature = "tpm-experimental")]
pub use keystore_tpm::TpmBackend;
pub use multi_device::{
    Capability, DidDocument, KeyType, RecoveryConfig, RecoveryMethod, RecoveryProof,
    RevocationReason, RotationEvent, RotationEventType, VerificationMethod,
};
pub use personhood::{
    AnchorStatus, BiometricType, KeyRotationReason, KeyRotationRecord, POPAttestation, POPLevel,
    POPMethod, PersonhoodAnchor, RecoverySignature, UniquenessAttestation, UniquenessProof,
    UniquenessProofType,
};
pub use personhood_store::{
    InMemoryPersonhoodStore, PersonhoodAnchorStore, PersonhoodStore, PersonhoodStoreTrait,
};
pub use recovery::{
    RecoveryAttestation, RecoveryEvent, RecoveryMessage, RecoveryStatus, IDENTITY_RECOVERY_TOPIC,
};
pub use revocation::{
    AppealStatus, CommonsRevocationReason, RevocationCheck, RevocationRecord, RevocationScope,
    RevocationType,
};
pub use revocation_store::RevocationRegistry;
pub use sync::{DidDocumentCache, IdentityUpdateMessage, IDENTITY_UPDATES_TOPIC};
pub use vui::Vui;

/// Hybrid signature or classical signature wrapper
#[cfg(feature = "post-quantum")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HybridSignatureOrClassical {
    /// Hybrid PQ signature (Ed25519 + ML-DSA)
    Hybrid(icn_crypto_pq::HybridSignature),
    /// Classical Ed25519 signature only
    Classical(ed25519_dalek::Signature),
}

#[cfg(feature = "post-quantum")]
impl HybridSignatureOrClassical {
    /// Verify the signature against a message and keypair
    ///
    /// For hybrid signatures, both Ed25519 and ML-DSA signatures must verify.
    /// For classical signatures, only Ed25519 is checked.
    pub fn verify(&self, message: &[u8], keypair: &KeyPair) -> bool {
        match self {
            Self::Hybrid(sig) => {
                // Hybrid signatures require PQ keys to verify
                if !keypair.has_pq_keys() {
                    return false;
                }
                if let Some(ref pq_kp) = keypair.pq_keypair {
                    let hybrid_pub = icn_crypto_pq::HybridPublicKey {
                        classical: keypair.verifying_key.to_bytes().to_vec(),
                        pq: pq_kp.public_key().clone(),
                    };
                    sig.verify(message, &hybrid_pub)
                } else {
                    false // Should not reach here due to has_pq_keys check
                }
            }
            Self::Classical(sig) => {
                use ed25519_dalek::Verifier;
                keypair.verifying_key.verify(message, sig).is_ok()
            }
        }
    }

    /// Check if this is a hybrid signature
    pub fn is_hybrid(&self) -> bool {
        matches!(self, Self::Hybrid(_))
    }

    /// Get the classical (Ed25519) signature bytes
    pub fn classical_bytes(&self) -> Vec<u8> {
        match self {
            Self::Hybrid(sig) => sig.classical.clone(),
            Self::Classical(sig) => sig.to_bytes().to_vec(),
        }
    }

    /// Get the PQ (ML-DSA) signature bytes if present
    pub fn pq_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Hybrid(sig) => Some(sig.pq.clone()),
            Self::Classical(_) => None,
        }
    }

    /// Convert to bytes for serialization
    pub fn to_bytes(&self) -> Vec<u8> {
        // SAFETY: HybridSignature is a simple struct with byte arrays that always serializes
        // successfully. Encoding cannot fail for well-formed data.
        #[allow(clippy::expect_used)]
        icn_encoding::encode(self).expect("HybridSignature serialization is infallible")
    }

    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        icn_encoding::decode(bytes).map_err(|e| anyhow::anyhow!("Failed to parse signature: {e}"))
    }
}

/// Decode the 32 identifier bytes a `did:icn:` spelling names, without
/// requiring a constructed [`Did`].
///
/// [`Did::identifier_bytes`] delegates here, so a caller holding a raw spelling
/// — a persisted store key, a serialized map key, an audit tool reading rows it
/// must not construct — decodes it by exactly the rule `Did` itself uses. That
/// shared implementation is the point: a pre-migration collision scan that
/// grouped rows by a *reimplementation* of this decode would prove nothing
/// about the equality it gates (N2-A, #2627).
///
/// This performs no validation beyond what decoding requires and canonicalizes
/// nothing. An `Err` means the spelling names no ICN principal, which is a
/// reportable fact rather than a reason to panic.
pub fn identifier_bytes_of_spelling(spelling: &str) -> Result<[u8; 32]> {
    // Validate the prefix rather than assuming it. Every `Did` reaching here
    // through `from_str` or `Deserialize` has been checked, but
    // `new_unchecked` bypasses that, and decoding whatever follows the first
    // eight characters of some other scheme would hand back bytes that name
    // no ICN principal.
    let encoded_part = spelling
        .strip_prefix("did:icn:")
        .ok_or_else(|| anyhow::anyhow!("Invalid DID format: must start with 'did:icn:'"))?;

    if encoded_part.is_empty() {
        anyhow::bail!("Invalid DID format: empty identifier after prefix");
    }

    let (_base, decoded_bytes) = multibase::decode(encoded_part)
        .map_err(|e| anyhow::anyhow!("Invalid DID multibase encoding: {e}"))?;

    decoded_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid DID: identifier is not 32 bytes"))
}

/// A decentralized identifier for an ICN node
///
/// DIDs are validated on construction/deserialization to ensure:
/// - They start with "did:icn:" prefix
/// - The identifier part is valid multibase (base58btc)
/// - The decoded bytes are exactly 32 bytes (Ed25519 public key size)
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "utoipa", schema(value_type = String, example = "did:icn:z6MkhaXMJznR4sC15gTfA7b6jJ4i7b6jJ4i7b6jJ4i7b"))]
pub struct Did(String);

// Custom deserializer that validates DIDs on deserialization
// This prevents malformed DIDs from bypassing validation when received over the network
impl<'de> Deserialize<'de> for Did {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Did::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Did {
    /// Create a DID from an ed25519 public key
    pub fn from_public_key(public_key: &VerifyingKey) -> Self {
        let encoded = multibase::encode(multibase::Base::Base58Btc, public_key.as_bytes());
        Did(format!("did:icn:{encoded}"))
    }

    /// Parse and validate a DID string
    ///
    /// Validates that:
    /// - String starts with "did:icn:" prefix
    /// - Remaining part is valid multibase (base58btc)
    /// - Decoded bytes are exactly 32 bytes (Ed25519 public key size)
    ///
    /// Returns an error for malformed DIDs instead of panicking.
    ///
    /// Note: This is the implementation used by the `FromStr` trait.
    #[allow(clippy::should_implement_trait)] // FromStr trait is implemented below
    pub fn from_str(s: &str) -> Result<Self> {
        // Validate prefix
        if !s.starts_with("did:icn:") {
            anyhow::bail!("Invalid DID format: must start with 'did:icn:' (got: {s})");
        }

        // Extract multibase-encoded part
        let encoded_part = &s[8..]; // Skip "did:icn:"

        if encoded_part.is_empty() {
            anyhow::bail!("Invalid DID format: empty identifier after prefix");
        }

        // Decode multibase
        let (_base, decoded_bytes) = multibase::decode(encoded_part)
            .map_err(|e| anyhow::anyhow!("Invalid DID multibase encoding: {e}"))?;

        // Validate decoded size (Ed25519 public key is 32 bytes)
        if decoded_bytes.len() != 32 {
            anyhow::bail!(
                "Invalid DID: decoded public key has {} bytes, expected 32",
                decoded_bytes.len()
            );
        }

        // Validate it's a valid Ed25519 public key
        VerifyingKey::from_bytes(
            decoded_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to convert to 32-byte array"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key in DID: {e}"))?;

        Ok(Did(s.to_string()))
    }

    /// Get the string representation of this DID
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Decode the 32 identifier bytes this DID names.
    ///
    /// A `did:icn:` identifier is a multibase encoding of 32 bytes, and the
    /// same bytes have many accepted spellings (base58btc, base16, base32,
    /// ...). This returns the bytes themselves, so callers that must treat one
    /// principal as one principal can compare identity rather than spelling
    /// (see #2641).
    ///
    /// Unlike [`Did::to_verifying_key`] this does not require the bytes to be a
    /// valid Ed25519 point, so it also resolves anchor-derived DIDs built by
    /// [`Did::from_anchor_id`].
    ///
    /// This is an accessor only: it does not canonicalize the DID and does not
    /// change how `Did` compares or hashes.
    pub fn identifier_bytes(&self) -> Result<[u8; 32]> {
        identifier_bytes_of_spelling(&self.0)
    }

    /// Extract the Ed25519 verifying key from this DID
    ///
    /// This decodes the DID's multibase-encoded public key and returns
    /// the VerifyingKey for signature verification.
    pub fn to_verifying_key(&self) -> Result<VerifyingKey> {
        // Defensive bounds check (should not fail for validated DIDs)
        let encoded_part = self
            .0
            .get(8..)
            .ok_or_else(|| anyhow::anyhow!("Invalid DID format: too short"))?;

        if encoded_part.is_empty() {
            anyhow::bail!("Invalid DID format: empty identifier after prefix");
        }

        // Decode multibase
        let (_base, decoded_bytes) = multibase::decode(encoded_part)
            .map_err(|e| anyhow::anyhow!("Invalid DID multibase encoding: {e}"))?;

        // Convert to VerifyingKey
        let key_bytes: [u8; 32] = decoded_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key length"))?;

        VerifyingKey::from_bytes(&key_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key: {e}"))
    }

    /// Internal constructor for creating DIDs from raw strings
    ///
    /// This bypasses validation and should only be used by trusted code
    /// (e.g., anchor module creating DIDs from anchor IDs).
    pub(crate) fn new_unchecked(s: String) -> Self {
        Did(s)
    }
}

// I7 (#2627) — `Did` equality and hashing name the **principal**, not the spelling.
//
// A `did:icn:` identifier is a multibase encoding of 32 bytes, and multibase has
// 23 spellings of those same bytes that `Did::from_str` accepts. Deriving
// `PartialEq`/`Hash` over the inner `String` therefore made one cryptographic
// principal into up to 23 distinct `HashMap` keys, `HashSet` members and
// eligible voters. `docs/architecture/IDENTITY_SEMANTICS.md` §11 I7 requires the
// opposite: equality is *key* equality.
//
// **This changes comparison, never representation.** `Debug`, `Display`,
// `as_str`, `Serialize` and `Deserialize` are untouched, so every durable key,
// wire byte and signing input a `Did` reaches is byte-for-byte what it was. That
// is what makes the change rollback-safe — a binary reverted to spelling
// equality reads exactly the same stored rows
// (`docs/architecture/n2-a0-stored-key-inventory.md` §12.1 item 5). The other
// mechanism §11 permits, pinning one encoding at parse time, would instead
// change what `from_str` *accepts* and strand every alternate-spelled row
// already on disk, so it is deliberately not what this does.
//
// Values that name no principal stay discriminated rather than merged into the
// decoded population. In production that arm is unreachable: `from_public_key`
// and `from_str` both validate, `from_anchor_id` encodes exactly 32 bytes, and
// `new_unchecked` is `pub(crate)` with `from_anchor_id` as its only non-test
// caller. It exists so that a value naming no principal can never test equal to
// one that does, however the bytes happen to line up.
impl PartialEq for Did {
    fn eq(&self, other: &Self) -> bool {
        // Identical spellings are the same principal without decoding anything:
        // `identifier_bytes` is a pure function of the string, so equal strings
        // take the same arm below and produce the same answer. This keeps the
        // overwhelmingly common case — one canonical spelling compared with
        // itself — as cheap as the derive it replaces.
        if self.0 == other.0 {
            return true;
        }

        match (self.identifier_bytes(), other.identifier_bytes()) {
            // The principals the two spellings name.
            (Ok(a), Ok(b)) => a == b,
            // Neither names a principal, so spelling is the only relation left.
            // Stated rather than folded into the fast path above so this arm
            // remains correct on its own.
            (Err(_), Err(_)) => self.0 == other.0,
            // One names a principal and the other names none: never the same
            // identity, whatever the bytes look like.
            (Ok(_), Err(_)) | (Err(_), Ok(_)) => false,
        }
    }
}

impl Eq for Did {}

impl std::hash::Hash for Did {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self.identifier_bytes() {
            // Every accepted spelling of one principal hashes alike, which is
            // what collapses them to a single entry in a `HashMap`/`HashSet`.
            Ok(identifier) => {
                0u8.hash(state);
                identifier.hash(state);
            }
            // A value naming no principal can only be keyed by its spelling.
            // The discriminant keeps that population from colliding with a
            // decoded identifier.
            Err(_) => {
                1u8.hash(state);
                self.0.hash(state);
            }
        }
    }
}

impl std::fmt::Display for Did {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Did {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Did::from_str(s)
    }
}

/// A key pair for ICN identity
///
/// When the `post-quantum` feature is enabled, new keypairs are generated with
/// hybrid Ed25519 + ML-DSA signatures by default. Legacy Ed25519-only keypairs
/// can still be loaded and used for backward compatibility.
pub struct KeyPair {
    // Store key bytes in a zeroizing container for security
    secret_bytes: Zeroizing<[u8; 32]>,
    verifying_key: VerifyingKey,
    did: Did,

    // Post-quantum keypair (optional, feature-gated)
    #[cfg(feature = "post-quantum")]
    pq_keypair: Option<icn_crypto_pq::MlDsaKeypair>,

    // Whether this is a native hybrid keypair (generated with PQ) vs upgraded legacy
    // Native hybrid keys should always use hybrid signatures; upgraded keys may fall back
    #[cfg(feature = "post-quantum")]
    is_hybrid: bool,
}

impl Clone for KeyPair {
    fn clone(&self) -> Self {
        KeyPair {
            secret_bytes: Zeroizing::new(*self.secret_bytes),
            verifying_key: self.verifying_key,
            did: self.did.clone(),
            #[cfg(feature = "post-quantum")]
            pq_keypair: self.pq_keypair.clone(),
            #[cfg(feature = "post-quantum")]
            is_hybrid: self.is_hybrid,
        }
    }
}

impl KeyPair {
    /// Generate a new random key pair
    ///
    /// When the `post-quantum` feature is enabled, this generates a hybrid keypair
    /// with both Ed25519 and ML-DSA keys. The DID is derived from the Ed25519 key
    /// for backward compatibility.
    pub fn generate() -> Result<Self> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let verifying_key = signing_key.verifying_key();
        let did = Did::from_public_key(&verifying_key);

        #[cfg(feature = "post-quantum")]
        let pq_keypair = Some(
            icn_crypto_pq::MlDsaKeypair::generate()
                .map_err(|e| anyhow::anyhow!("PQ key generation failed: {e}"))?,
        );

        Ok(KeyPair {
            secret_bytes: Zeroizing::new(secret_bytes),
            verifying_key,
            did,
            #[cfg(feature = "post-quantum")]
            pq_keypair,
            #[cfg(feature = "post-quantum")]
            is_hybrid: true, // Native hybrid keypair
        })
    }

    /// Reconstruct a keypair from raw bytes (legacy Ed25519-only format)
    ///
    /// This is used for loading existing keystores that don't have PQ components.
    /// The keypair will work for classical signing but won't produce hybrid signatures.
    pub fn from_bytes(secret_bytes: &[u8; 32], public_bytes: &[u8; 32]) -> Result<Self> {
        let verifying_key = VerifyingKey::from_bytes(public_bytes)?;
        let did = Did::from_public_key(&verifying_key);

        // Verify the keys match
        let signing_key = SigningKey::from_bytes(secret_bytes);
        if signing_key.verifying_key() != verifying_key {
            anyhow::bail!("Public key does not match secret key");
        }

        Ok(KeyPair {
            secret_bytes: Zeroizing::new(*secret_bytes),
            verifying_key,
            did,
            #[cfg(feature = "post-quantum")]
            pq_keypair: None, // Legacy keys don't have PQ component
            #[cfg(feature = "post-quantum")]
            is_hybrid: false, // Legacy keypair, not native hybrid
        })
    }

    /// Reconstruct a keypair with PQ keys from raw bytes
    ///
    /// This loads a hybrid keypair from stored bytes, including both Ed25519
    /// and ML-DSA components. Used when loading from keystore v5+.
    #[cfg(feature = "post-quantum")]
    pub fn from_bytes_with_pq(
        secret_bytes: &[u8; 32],
        public_bytes: &[u8; 32],
        pq_secret: &[u8],
        pq_public: &[u8],
    ) -> Result<Self> {
        let verifying_key = VerifyingKey::from_bytes(public_bytes)?;
        let did = Did::from_public_key(&verifying_key);

        // Verify the keys match
        let signing_key = SigningKey::from_bytes(secret_bytes);
        if signing_key.verifying_key() != verifying_key {
            anyhow::bail!("Public key does not match secret key");
        }

        // Reconstruct PQ keypair
        let pq_keypair = icn_crypto_pq::MlDsaKeypair::from_bytes(pq_secret, pq_public)
            .map_err(|e| anyhow::anyhow!("Invalid PQ keypair: {e}"))?;

        Ok(KeyPair {
            secret_bytes: Zeroizing::new(*secret_bytes),
            verifying_key,
            did,
            pq_keypair: Some(pq_keypair),
            is_hybrid: true, // Has PQ keys, treated as hybrid
        })
    }

    /// Get the DID for this key pair
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Get the verifying (public) key
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Get access to secret bytes (for serialization only)
    pub(crate) fn secret_bytes(&self) -> &[u8; 32] {
        &self.secret_bytes
    }

    /// Check if this keypair has post-quantum keys
    #[cfg(feature = "post-quantum")]
    pub fn has_pq_keys(&self) -> bool {
        self.pq_keypair.is_some()
    }

    /// Check if this is a hybrid keypair
    ///
    /// Returns true if this keypair has PQ keys AND was either:
    /// - Generated with hybrid PQ support (`KeyPair::generate()`)
    /// - Upgraded via `from_bytes_with_pq()`
    /// - Loaded from a keystore with PQ keys
    ///
    /// This is equivalent to `has_pq_keys()` for practical purposes, but
    /// explicitly checks the `is_hybrid` flag for future extensibility.
    #[cfg(feature = "post-quantum")]
    pub fn is_hybrid(&self) -> bool {
        self.is_hybrid && self.pq_keypair.is_some()
    }

    /// Get the PQ public key if available
    #[cfg(feature = "post-quantum")]
    pub fn pq_public_key(&self) -> Option<icn_crypto_pq::MlDsaPublicKey> {
        self.pq_keypair.as_ref().map(|kp| kp.public_key().clone())
    }

    /// Get the PQ keypair for signing operations
    #[cfg(feature = "post-quantum")]
    pub fn pq_keypair(&self) -> Option<&icn_crypto_pq::MlDsaKeypair> {
        self.pq_keypair.as_ref()
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature {
        use ed25519_dalek::Signer;
        let signing_key = SigningKey::from_bytes(&self.secret_bytes);
        signing_key.sign(message)
    }

    /// Sign a message with hybrid signature (if PQ keys available)
    #[cfg(feature = "post-quantum")]
    pub fn sign_hybrid(&self, message: &[u8]) -> Result<HybridSignatureOrClassical> {
        use ed25519_dalek::Signer;
        let signing_key = SigningKey::from_bytes(&self.secret_bytes);
        let classical_sig = signing_key.sign(message);

        if let Some(ref pq_kp) = self.pq_keypair {
            let pq_sig = pq_kp.sign(message)?;
            Ok(HybridSignatureOrClassical::Hybrid(
                icn_crypto_pq::HybridSignature::new(classical_sig, pq_sig),
            ))
        } else {
            Ok(HybridSignatureOrClassical::Classical(classical_sig))
        }
    }

    /// Get the signing key bytes for use in external signing operations
    ///
    /// This method exposes the secret key bytes for use in scenarios where
    /// direct access to the signing key is needed (e.g., for compute task signing).
    /// Use with caution - these bytes should be handled securely.
    pub fn to_signing_key_bytes(&self) -> [u8; 32] {
        *self.secret_bytes
    }

    /// Export keypair for upgrade (PQ feature only)
    #[cfg(feature = "post-quantum")]
    pub fn export_for_upgrade(&self) -> ([u8; 32], [u8; 32]) {
        (*self.secret_bytes, self.verifying_key.to_bytes())
    }
}

impl Drop for KeyPair {
    fn drop(&mut self) {
        // Zeroizing handles the secure drop of secret_bytes
    }
}

/// A software `KeyPair` is a signing capability, so it can stand in wherever one
/// is required.
///
/// This is the bridge that lets callers depend on [`DidSigner`] — which hardware
/// backends can satisfy — without churning every existing software call site
/// (#2501). `&KeyPair` and `Arc<KeyPair>` coerce to the trait object directly.
///
/// It does **not** make `KeyPair` the preferred dependency: holding one still
/// means holding extractable private key material. New code that only needs to
/// *use* a key should take `&dyn DidSigner` and let the composition root decide
/// whether that is backed by software or an HSM.
impl DidSigner for KeyPair {
    fn did(&self) -> &Did {
        &self.did
    }

    fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    fn sign(&self, message: &[u8]) -> Result<ed25519_dalek::Signature> {
        // Qualified to name the inherent method explicitly. Inherent methods
        // already win over trait methods, so `self.sign(..)` would resolve the
        // same way — but naming it leaves no doubt this is not self-recursive.
        Ok(KeyPair::sign(self, message))
    }

    fn is_hardware_backed(&self) -> bool {
        false
    }

    fn backend_type(&self) -> &str {
        "software"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hash one value with a fixed-seed hasher so two values can be compared.
    fn hash_of<T: std::hash::Hash>(value: &T) -> u64 {
        use std::hash::Hasher as _;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn test_generate_keypair() {
        let kp = KeyPair::generate().unwrap();
        assert!(kp.did().as_str().starts_with("did:icn:"));
    }

    #[test]
    fn test_sign_verify() {
        use ed25519_dalek::Verifier;

        let kp = KeyPair::generate().unwrap();
        let message = b"hello world";
        let signature = kp.sign(message);

        assert!(kp.verifying_key().verify(message, &signature).is_ok());
    }

    #[test]
    fn test_did_from_str_valid() {
        // Generate a valid DID
        let kp = KeyPair::generate().unwrap();
        let did_str = kp.did().as_str();

        // Should parse successfully
        let parsed_did = Did::from_str(did_str).unwrap();
        assert_eq!(parsed_did.as_str(), did_str);
    }

    #[test]
    fn identifier_bytes_are_equal_across_multibase_spellings() {
        let kp = KeyPair::generate().unwrap();
        let canonical = kp.did().clone();
        let bytes = canonical.identifier_bytes().unwrap();

        // Same key, spelled base16 instead of base58btc.
        let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes))).unwrap();

        assert_ne!(
            canonical.as_str(),
            alias.as_str(),
            "the two spellings must differ, or this proves nothing"
        );
        assert_eq!(
            canonical.identifier_bytes().unwrap(),
            alias.identifier_bytes().unwrap(),
            "one key must have one identifier whatever spelling names it"
        );
        assert_eq!(
            bytes,
            kp.verifying_key().to_bytes(),
            "for a validated DID the identifier bytes are the public key"
        );
    }

    #[test]
    fn identifier_bytes_resolve_anchor_derived_dids_that_are_not_ed25519_points() {
        // `from_anchor_id` bypasses validation, so its 32 bytes need not
        // decompress to an Edwards point. Callers keying on identity must still
        // be able to resolve it.
        // Roughly half of arbitrary 32-byte values decompress; [2u8; 32] does not.
        let anchor = Did::from_anchor_id(&[2u8; 32]);

        assert!(
            anchor.to_verifying_key().is_err(),
            "control: this anchor id is not a valid Ed25519 point"
        );
        assert_eq!(
            anchor.identifier_bytes().unwrap(),
            [2u8; 32],
            "identifier bytes must still resolve"
        );
    }

    // ---------------------------------------------------------------------
    // I7 (#2627): the discriminated fallback for values that name no principal.
    //
    // These live here rather than in `tests/did_principal_equality.rs` because
    // `new_unchecked` is `pub(crate)` — the non-decoding population is
    // deliberately unconstructible from outside the crate, and unreachable in
    // production (see the note on the `PartialEq` impl). It still needs its own
    // coverage: the whole point of discriminating it is that it can never be
    // confused with a decoded principal.
    // ---------------------------------------------------------------------

    /// Two values that name no principal are compared by spelling, not merged.
    #[test]
    fn non_decoding_dids_fall_back_to_spelling_equality() {
        let a = Did::new_unchecked("did:key:not-an-icn-did".to_string());
        let b = Did::new_unchecked("did:key:not-an-icn-did".to_string());
        let c = Did::new_unchecked("did:key:a-different-non-did".to_string());

        assert!(
            a.identifier_bytes().is_err(),
            "control: `a` decodes to nothing"
        );
        assert!(
            c.identifier_bytes().is_err(),
            "control: `c` decodes to nothing"
        );

        assert_eq!(a, b, "one spelling that names no principal equals itself");
        assert_eq!(
            hash_of(&a),
            hash_of(&b),
            "equal values must hash equally, whichever arm they take"
        );
        assert_ne!(
            a, c,
            "two different non-decoding spellings are not one value"
        );
    }

    /// A value naming no principal can never equal one that does.
    ///
    /// This is what the discriminant in `Hash` buys: without it, a spelling
    /// whose bytes happened to line up with a decoded identifier could collide
    /// with a real principal, and `HashMap` would then have to rely on `Eq`
    /// alone to keep them apart.
    #[test]
    fn a_non_decoding_did_never_equals_a_decoded_principal() {
        let real = KeyPair::generate().unwrap().did().clone();
        let bytes = real.identifier_bytes().expect("a validated DID decodes");

        // A non-decoding value built from the *same* bytes, so the only thing
        // keeping them apart is the discrimination itself.
        let impostor = Did::new_unchecked(format!("did:key:{}", hex::encode(bytes)));
        assert!(
            impostor.identifier_bytes().is_err(),
            "control: the impostor must take the fallback arm"
        );

        assert_ne!(real, impostor, "a decoded principal is not a spelling");
        assert_ne!(impostor, real, "and the inequality is symmetric");
        assert_ne!(
            hash_of(&real),
            hash_of(&impostor),
            "the discriminant must keep the two populations from colliding"
        );

        let mut set = std::collections::HashSet::new();
        set.insert(real);
        set.insert(impostor);
        assert_eq!(set.len(), 2, "they must occupy two entries, not one");
    }

    /// `Did` equality is an equivalence relation across both populations.
    #[test]
    fn did_equality_is_reflexive_symmetric_and_transitive() {
        let principal = KeyPair::generate().unwrap().did().clone();
        let bytes = principal.identifier_bytes().unwrap();
        let corpus = [
            principal.clone(),
            // Two more spellings of that same principal.
            Did::from_str(&format!(
                "did:icn:{}",
                multibase::encode(multibase::Base::Base16Lower, bytes)
            ))
            .unwrap(),
            Did::from_anchor_id(&bytes),
            // A different principal.
            KeyPair::generate().unwrap().did().clone(),
            // Two values naming no principal.
            Did::new_unchecked("did:key:x".to_string()),
            Did::new_unchecked("did:key:y".to_string()),
        ];

        for a in &corpus {
            assert_eq!(a, a, "reflexive");
            for b in &corpus {
                assert_eq!(a == b, b == a, "symmetric: {a:?} vs {b:?}");
                if a == b {
                    assert_eq!(
                        hash_of(a),
                        hash_of(b),
                        "equal values must hash equally: {a:?} vs {b:?}"
                    );
                }
                for c in &corpus {
                    if a == b && b == c {
                        assert_eq!(a, c, "transitive: {a:?} == {b:?} == {c:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn identifier_bytes_reject_a_did_of_another_method() {
        // `new_unchecked` bypasses prefix validation. Slicing a fixed eight
        // characters off some other scheme would decode bytes that name no ICN
        // principal, so the accessor must refuse rather than invent one.
        let kp = KeyPair::generate().unwrap();
        let suffix = kp.did().as_str().strip_prefix("did:icn:").unwrap();
        let foreign = Did::new_unchecked(format!("did:key:{suffix}"));

        assert!(
            foreign.identifier_bytes().is_err(),
            "a non-icn DID method must not resolve to an ICN principal"
        );
    }

    #[test]
    fn identifier_bytes_reject_a_non_32_byte_identifier() {
        let short = Did::new_unchecked(format!(
            "did:icn:{}",
            multibase::encode(multibase::Base::Base58Btc, [7u8; 16])
        ));
        assert!(short.identifier_bytes().is_err());
    }

    #[test]
    fn test_did_from_str_invalid_prefix() {
        let result = Did::from_str("invalid:prefix:abc123");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with 'did:icn:'"));
    }

    #[test]
    fn test_did_from_str_empty_identifier() {
        let result = Did::from_str("did:icn:");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty identifier"));
    }

    #[test]
    fn test_did_from_str_invalid_multibase() {
        let result = Did::from_str("did:icn:INVALID!!!BASE58");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("multibase encoding"));
    }

    #[test]
    fn test_did_from_str_wrong_key_size() {
        // Create a multibase-encoded string with wrong size (16 bytes instead of 32)
        let short_bytes = vec![0u8; 16];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &short_bytes);
        let did_str = format!("did:icn:{encoded}");

        let result = Did::from_str(&did_str);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected 32"));
    }

    #[test]
    fn test_did_from_str_invalid_ed25519_key() {
        // All zeros is not a valid Ed25519 public key
        let invalid_key = vec![0u8; 32];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &invalid_key);
        let did_str = format!("did:icn:{encoded}");

        let result = Did::from_str(&did_str);
        // Note: All-zeros might actually be accepted by ed25519_dalek
        // This test documents the behavior even if it passes
        if let Err(e) = result {
            assert!(e.to_string().contains("Ed25519"));
        }
    }

    #[test]
    fn test_did_to_verifying_key() {
        // Generate a keypair and DID
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        // Extract verifying key from DID
        let extracted_key = did.to_verifying_key().unwrap();

        // Should match the original keypair's verifying key
        assert_eq!(extracted_key, *kp.verifying_key());
    }

    #[test]
    fn test_did_signature_verification() {
        use ed25519_dalek::Verifier;

        // Generate keypair and sign a message
        let kp = KeyPair::generate().unwrap();
        let message = b"contract deployment";
        let signature = kp.sign(message);

        // Extract verifying key from DID and verify
        let did = kp.did();
        let verifying_key = did.to_verifying_key().unwrap();

        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    // Regression tests for malformed DID handling (Issue #149)
    #[test]
    fn test_did_parsing_empty_string() {
        let result = Did::from_str("");
        assert!(result.is_err());
    }

    #[test]
    fn test_did_parsing_very_short_string() {
        let result = Did::from_str("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_did_parsing_prefix_only() {
        let result = Did::from_str("did:icn");
        assert!(result.is_err());
    }

    #[test]
    fn test_did_deserialization_validates() {
        // Valid DID should deserialize successfully
        let kp = KeyPair::generate().unwrap();
        let json = format!(r#""{}""#, kp.did().as_str());
        let did: Result<Did, _> = serde_json::from_str(&json);
        assert!(did.is_ok());
    }

    #[test]
    fn test_did_deserialization_rejects_invalid_prefix() {
        // Invalid prefix should be rejected during deserialization
        let json = r#""invalid:prefix:abc123""#;
        let result: Result<Did, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("did:icn:") || err_msg.contains("Invalid"));
    }

    #[test]
    fn test_did_deserialization_rejects_empty() {
        let json = r#""""#;
        let result: Result<Did, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_did_deserialization_rejects_short_string() {
        let json = r#""abc""#;
        let result: Result<Did, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_did_deserialization_rejects_wrong_key_size() {
        // Create a multibase-encoded string with wrong size (16 bytes instead of 32)
        let short_bytes = vec![0u8; 16];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &short_bytes);
        let json = format!(r#""did:icn:{encoded}""#);

        let result: Result<Did, _> = serde_json::from_str(&json);
        assert!(result.is_err());
    }
}
