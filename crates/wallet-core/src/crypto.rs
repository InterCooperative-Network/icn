use ed25519_dalek::{Signer, Verifier, SigningKey, VerifyingKey, Signature};
use rand::rngs::OsRng;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use serde::de;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use crate::error::{WalletResult, WalletError};

// We'll implement custom serialization for KeyPair
#[derive(Clone)]
pub struct KeyPair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

// Serialized representation of a KeyPair
#[derive(Serialize, Deserialize)]
pub struct KeyPairSerialized {
    secret: String,
    public: String,
}

// Custom serialization for KeyPair
impl Serialize for KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize as a struct with two fields
        let secret = BASE64.encode(self.signing_key.to_bytes());
        let public = BASE64.encode(self.verifying_key.to_bytes());
        
        #[derive(Serialize)]
        struct SerializeHelper {
            secret: String,
            public: String,
        }
        
        SerializeHelper { secret, public }.serialize(serializer)
    }
}

// Custom deserialization for KeyPair
impl<'de> Deserialize<'de> for KeyPair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize from a struct with two fields
        #[derive(Deserialize)]
        struct DeserializeHelper {
            secret: String,
            // Public key is calculated from secret key, but we need to keep this field
            // for proper deserialization from the JSON structure
            #[allow(dead_code)]
            public: String,
        }
        
        let helper = DeserializeHelper::deserialize(deserializer)?;
        
        let secret_bytes = BASE64.decode(&helper.secret)
            .map_err(|e| de::Error::custom(format!("Invalid base64 secret: {}", e)))?;
            
        let signing_key = SigningKey::try_from(secret_bytes.as_slice())
            .map_err(|e| de::Error::custom(format!("Invalid Ed25519 secret key: {}", e)))?;
            
        let verifying_key = VerifyingKey::from(&signing_key);
        
        Ok(KeyPair {
            signing_key,
            verifying_key,
        })
    }
}

impl KeyPair {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);
        
        Self { 
            signing_key, 
            verifying_key,
        }
    }
    
    pub fn from_secret(secret_key_bytes: &[u8]) -> WalletResult<Self> {
        let signing_key = SigningKey::try_from(secret_key_bytes)
            .map_err(|e| WalletError::CryptoError(format!("Invalid secret key: {}", e)))?;
        let verifying_key = VerifyingKey::from(&signing_key);
        
        Ok(Self { 
            signing_key, 
            verifying_key,
        })
    }
    
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }
    
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }
    
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        self.verifying_key.verify(message, signature).is_ok()
    }
    
    pub fn to_serializable(&self) -> KeyPairSerialized {
        KeyPairSerialized {
            secret: BASE64.encode(self.signing_key.to_bytes()),
            public: BASE64.encode(self.verifying_key.to_bytes()),
        }
    }
    
    pub fn from_serializable(serialized: &KeyPairSerialized) -> WalletResult<Self> {
        let secret_bytes = BASE64.decode(&serialized.secret)
            .map_err(|e| WalletError::SerializationError(format!("Failed to decode secret key: {}", e)))?;
        Self::from_secret(&secret_bytes)
    }
} 