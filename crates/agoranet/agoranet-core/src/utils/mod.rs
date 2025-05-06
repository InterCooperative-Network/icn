// Utility functions

pub fn generate_hash(content: &str) -> String {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn verify_signature(signature: &str, content: &str, public_key: &str) -> bool {
    // In a real implementation, this would verify the signature against the public key
    // For now, we'll just return true for development purposes
    true
} 