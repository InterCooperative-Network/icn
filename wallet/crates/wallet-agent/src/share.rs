/*!
 * ICN Wallet Receipt Sharing
 *
 * Provides functionality for sharing execution receipts
 * with selective disclosure for cross-federation trust.
 */

use std::path::Path;
use thiserror::Error;
use icn_wallet_sync::{ExportFormat, VerifiableCredential, export_receipts_to_file};
use crate::import::ExecutionReceipt;
use x25519_dalek::{PublicKey, StaticSecret};
use rand::rngs::OsRng;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AesRng},
    Aes256Gcm, Key, Nonce
};

/// Error types for receipt sharing
#[derive(Error, Debug)]
pub enum ShareError {
    #[error("Export error: {0}")]
    ExportError(#[from] icn_wallet_sync::ExportError),
    
    #[error("Format not supported: {0}")]
    UnsupportedFormat(String),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("Decryption error: {0}")]
    DecryptionError(String),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Base64 error: {0}")]
    Base64Error(#[from] base64::DecodeError),
}

/// Formats for sharing receipts
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareFormat {
    /// JSON format
    Json,
    
    /// CSV format
    Csv,
    
    /// Signed bundle format
    SignedBundle,
    
    /// Encrypted bundle format with recipient's public key
    EncryptedBundle(String), // Public key as base64 string
}

impl From<ShareFormat> for ExportFormat {
    fn from(format: ShareFormat) -> Self {
        match format {
            ShareFormat::Json => ExportFormat::Json,
            ShareFormat::Csv => ExportFormat::Csv,
            ShareFormat::SignedBundle => ExportFormat::SignedBundle,
            ShareFormat::EncryptedBundle(_) => ExportFormat::SignedBundle, // Signed first, then encrypted
        }
    }
}

/// Metadata for encrypted bundles
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedBundleMetadata {
    /// Sender's DID
    pub sender_did: String,
    
    /// Federation scope
    pub federation_scope: String,
    
    /// Timestamp of encryption
    pub timestamp: String,
    
    /// Number of receipts in the bundle
    pub receipt_count: usize,
    
    /// Recipient's DID or federation ID
    pub recipient: String,
}

/// An encrypted bundle of receipts
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedBundle {
    /// Metadata about the bundle
    pub metadata: EncryptedBundleMetadata,
    
    /// Nonce used for encryption (base64)
    pub nonce: String,
    
    /// Ephemeral public key used for key exchange (base64)
    pub ephemeral_public_key: String,
    
    /// Encrypted payload (base64)
    pub payload: String,
}

/// Options for sharing receipts
#[derive(Debug, Clone)]
pub struct ShareOptions {
    /// Format to share in
    pub format: ShareFormat,
    
    /// Recipient public key for encryption (if applicable)
    pub recipient_key: Option<String>,
    
    /// Whether to include full proofs
    pub include_proofs: bool,
    
    /// Custom metadata to include
    pub metadata: Option<serde_json::Value>,
    
    /// Sender DID for encrypted bundles
    pub sender_did: Option<String>,
    
    /// Recipient DID or federation ID for encrypted bundles
    pub recipient_did: Option<String>,
}

impl Default for ShareOptions {
    fn default() -> Self {
        Self {
            format: ShareFormat::Json,
            recipient_key: None,
            include_proofs: true,
            metadata: None,
            sender_did: None,
            recipient_did: None,
        }
    }
}

/// Process receipts for sharing, applying any transformations needed
fn prepare_receipts_for_sharing(
    receipts: &[ExecutionReceipt],
    options: &ShareOptions,
) -> Vec<VerifiableCredential> {
    receipts
        .iter()
        .map(|receipt| {
            let mut credential = receipt.credential.clone();
            
            // If proofs should be excluded, remove them
            if !options.include_proofs {
                credential.proof = None;
            }
            
            // If custom metadata is provided, add it to the credential
            if let Some(metadata) = &options.metadata {
                if let Some(proof) = &mut credential.proof {
                    if proof.is_object() {
                        let proof_obj = proof.as_object_mut().unwrap();
                        proof_obj.insert("metadata".to_string(), metadata.clone());
                    }
                } else if options.include_proofs {
                    // Create a new proof with just metadata
                    credential.proof = Some(serde_json::json!({
                        "type": "SharedProof",
                        "created": chrono::Utc::now().to_rfc3339(),
                        "metadata": metadata.clone()
                    }));
                }
            }
            
            credential
        })
        .collect()
}

/// Share receipts with others in the specified format
pub fn share_receipts(
    receipts: &[ExecutionReceipt],
    options: ShareOptions,
    destination: &Path,
) -> Result<(), ShareError> {
    // For encrypted bundles, use that specific path
    if let ShareFormat::EncryptedBundle(pubkey_b64) = &options.format {
        // Validate we have the required fields
        let sender_did = options.sender_did.clone().unwrap_or_else(|| "unknown".to_string());
        let recipient_did = options.recipient_did.clone().unwrap_or_else(|| "unknown".to_string());
        
        // First prepare the receipts
        let prepared_receipts = prepare_receipts_for_sharing(receipts, &options);
        
        // Encrypt the bundle
        let encrypted_bundle = encrypt_receipt_bundle(
            &prepared_receipts,
            pubkey_b64,
            &sender_did,
            &recipient_did,
            receipts.first().map(|r| r.federation_scope.clone()).unwrap_or_else(|| "unknown".to_string())
        )?;
        
        // Serialize and save the encrypted bundle
        let bundle_json = serde_json::to_string_pretty(&encrypted_bundle)?;
        std::fs::write(destination, bundle_json)?;
        
        return Ok(());
    }
    
    // For other formats, use the standard flow
    // Prepare the receipts for sharing
    let prepared_receipts = prepare_receipts_for_sharing(receipts, &options);
    
    // Export receipts in the specified format
    let export_format: ExportFormat = options.format.clone().into();
    export_receipts_to_file(&prepared_receipts, export_format, destination)?;
    
    Ok(())
}

/// Encrypt a receipt bundle using X25519 key exchange and AES-GCM
pub fn encrypt_receipt_bundle(
    receipts: &[VerifiableCredential],
    recipient_pubkey_b64: &str,
    sender_did: &str,
    recipient_did: &str,
    federation_scope: String,
) -> Result<EncryptedBundle, ShareError> {
    // Deserialize the recipient's public key
    let recipient_pubkey_bytes = BASE64.decode(recipient_pubkey_b64)?;
    if recipient_pubkey_bytes.len() != 32 {
        return Err(ShareError::EncryptionError("Invalid public key length".to_string()));
    }
    
    let mut pubkey_bytes = [0u8; 32];
    pubkey_bytes.copy_from_slice(&recipient_pubkey_bytes);
    let recipient_pubkey = PublicKey::from(pubkey_bytes);
    
    // Generate an ephemeral keypair for the X25519 key exchange
    let ephemeral_secret = StaticSecret::random_from_rng(OsRng);
    let ephemeral_pubkey = PublicKey::from(&ephemeral_secret);
    
    // Perform key exchange to get the shared secret
    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_pubkey);
    
    // Use the shared secret to derive an AES-256 key
    let aes_key = Key::<Aes256Gcm>::from_slice(&shared_secret.as_bytes()[..32]);
    
    // Create a cipher instance
    let cipher = Aes256Gcm::new(aes_key);
    
    // Generate a random nonce
    let nonce = Aes256Gcm::generate_nonce(&mut AesRng);
    
    // Serialize the receipts
    let serialized = serde_json::to_vec(receipts)?;
    
    // Encrypt the serialized receipts
    let encrypted_payload = cipher.encrypt(&nonce, serialized.as_ref())
        .map_err(|e| ShareError::EncryptionError(format!("Failed to encrypt: {}", e)))?;
    
    // Create metadata
    let metadata = EncryptedBundleMetadata {
        sender_did: sender_did.to_string(),
        federation_scope,
        timestamp: chrono::Utc::now().to_rfc3339(),
        receipt_count: receipts.len(),
        recipient: recipient_did.to_string(),
    };
    
    // Create the encrypted bundle
    let bundle = EncryptedBundle {
        metadata,
        nonce: BASE64.encode(nonce),
        ephemeral_public_key: BASE64.encode(ephemeral_pubkey.as_bytes()),
        payload: BASE64.encode(encrypted_payload),
    };
    
    Ok(bundle)
}

/// Decrypt a receipt bundle using the provided private key
pub fn decrypt_receipt_bundle(
    bundle: &EncryptedBundle,
    private_key_b64: &str,
) -> Result<Vec<VerifiableCredential>, ShareError> {
    // Decode the private key
    let private_key_bytes = BASE64.decode(private_key_b64)?;
    if private_key_bytes.len() != 32 {
        return Err(ShareError::DecryptionError("Invalid private key length".to_string()));
    }
    
    let mut private_key_array = [0u8; 32];
    private_key_array.copy_from_slice(&private_key_bytes);
    let private_key = StaticSecret::from(private_key_array);
    
    // Decode the ephemeral public key
    let ephemeral_pubkey_bytes = BASE64.decode(&bundle.ephemeral_public_key)?;
    if ephemeral_pubkey_bytes.len() != 32 {
        return Err(ShareError::DecryptionError("Invalid ephemeral public key length".to_string()));
    }
    
    let mut ephemeral_pubkey_array = [0u8; 32];
    ephemeral_pubkey_array.copy_from_slice(&ephemeral_pubkey_bytes);
    let ephemeral_pubkey = PublicKey::from(ephemeral_pubkey_array);
    
    // Perform key exchange to get the shared secret
    let shared_secret = private_key.diffie_hellman(&ephemeral_pubkey);
    
    // Use the shared secret to derive an AES-256 key
    let aes_key = Key::<Aes256Gcm>::from_slice(&shared_secret.as_bytes()[..32]);
    
    // Create a cipher instance
    let cipher = Aes256Gcm::new(aes_key);
    
    // Decode the nonce
    let nonce_bytes = BASE64.decode(&bundle.nonce)?;
    if nonce_bytes.len() != 12 {
        return Err(ShareError::DecryptionError("Invalid nonce length".to_string()));
    }
    
    let nonce = Nonce::<Aes256Gcm>::from_slice(&nonce_bytes);
    
    // Decode the encrypted payload
    let encrypted_payload = BASE64.decode(&bundle.payload)?;
    
    // Decrypt the payload
    let decrypted_payload = cipher.decrypt(nonce, encrypted_payload.as_ref())
        .map_err(|e| ShareError::DecryptionError(format!("Failed to decrypt: {}", e)))?;
    
    // Deserialize the receipts
    let receipts: Vec<VerifiableCredential> = serde_json::from_slice(&decrypted_payload)?;
    
    Ok(receipts)
}

/// Convenience wrapper to share receipts in JSON format
pub fn share_receipts_as_json(
    receipts: &[ExecutionReceipt],
    destination: &Path,
    include_proofs: bool,
) -> Result<(), ShareError> {
    let options = ShareOptions {
        format: ShareFormat::Json,
        include_proofs,
        ..Default::default()
    };
    
    share_receipts(receipts, options, destination)
}

/// Convenience wrapper to share receipts as a signed bundle
pub fn share_receipts_as_bundle(
    receipts: &[ExecutionReceipt],
    destination: &Path,
    metadata: Option<serde_json::Value>,
) -> Result<(), ShareError> {
    let options = ShareOptions {
        format: ShareFormat::SignedBundle,
        include_proofs: true,
        metadata,
        ..Default::default()
    };
    
    share_receipts(receipts, options, destination)
}

/// Convenience wrapper to share receipts as an encrypted bundle
pub fn share_receipts_as_encrypted_bundle(
    receipts: &[ExecutionReceipt],
    destination: &Path,
    recipient_pubkey: &str,
    sender_did: &str,
    recipient_did: &str,
) -> Result<(), ShareError> {
    let options = ShareOptions {
        format: ShareFormat::EncryptedBundle(recipient_pubkey.to_string()),
        include_proofs: true,
        sender_did: Some(sender_did.to_string()),
        recipient_did: Some(recipient_did.to_string()),
        ..Default::default()
    };
    
    share_receipts(receipts, options, destination)
}

/// Generate a federation share link for an encrypted bundle
pub fn generate_share_link(
    encrypted_bundle: &EncryptedBundle,
    federation_url: &str,
) -> Result<String, ShareError> {
    // Serialize the bundle to JSON
    let bundle_json = serde_json::to_string(encrypted_bundle)?;
    
    // Base64 encode the JSON
    let encoded_bundle = BASE64.encode(bundle_json);
    
    // Construct the URL
    let mut url_base = federation_url.to_string();
    if !url_base.starts_with("http://") && !url_base.starts_with("https://") {
        url_base = format!("https://{}", url_base);
    }
    
    // Remove trailing slash if present
    if url_base.ends_with('/') {
        url_base.pop();
    }
    
    // Generate the share link
    let share_link = format!("icn://{}/verify?bundle={}", 
        url_base.replace("https://", "").replace("http://", ""),
        encoded_bundle
    );
    
    Ok(share_link)
}

/// Generate a federation share link from receipts
pub fn generate_share_link_from_receipts(
    receipts: &[ExecutionReceipt],
    federation_url: &str,
    recipient_pubkey: &str,
    sender_did: &str,
    recipient_did: &str,
) -> Result<String, ShareError> {
    // First, prepare the receipts for sharing
    let options = ShareOptions {
        format: ShareFormat::SignedBundle,
        include_proofs: true,
        sender_did: Some(sender_did.to_string()),
        recipient_did: Some(recipient_did.to_string()),
        ..Default::default()
    };
    
    let prepared_receipts = prepare_receipts_for_sharing(receipts, &options);
    
    // Encrypt the bundle
    let federation_scope = receipts.first()
        .map(|r| r.federation_scope.clone())
        .unwrap_or_else(|| "unknown".to_string());
    
    let encrypted_bundle = encrypt_receipt_bundle(
        &prepared_receipts,
        recipient_pubkey,
        sender_did,
        recipient_did,
        federation_scope
    )?;
    
    // Generate the share link
    generate_share_link(&encrypted_bundle, federation_url)
} 