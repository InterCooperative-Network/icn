#[derive(Debug, Clone)]
pub struct FederationInfo {
    pub id: String,
    pub name: String,
    pub did: String,
}

#[derive(Debug, Clone)]
pub struct AnchorCredentialOptions {
    pub anchor_type: String,
    pub federation: FederationInfo,
    pub subject_did: String,
    pub dag_root_hash: String,
    pub effective_from: String,
    pub effective_until: Option<String>,
    pub referenced_credentials: Vec<String>,
    pub amendment_id: Option<String>,
    pub previous_amendment_id: Option<String>,
    pub text_hash: Option<String>,
    pub ratified_in_epoch: Option<String>,
    pub description: Option<String>,
}

impl FederationRuntime {
    pub fn create_anchor_credential(&self, options: AnchorCredentialOptions) -> Result<serde_json::Value, Error> {
        let mut payload = serde_json::json!({
            "anchor_type": options.anchor_type,
            "federation": {
                "id": options.federation.id,
                "name": options.federation.name,
                "did": options.federation.did,
            },
            "subject_did": options.subject_did,
            "dag_root_hash": options.dag_root_hash,
            "effective_from": options.effective_from,
            "referenced_credentials": options.referenced_credentials,
        });
        
        if let Some(until) = options.effective_until {
            payload["effective_until"] = serde_json::json!(until);
        }
        
        if let Some(amend_id) = options.amendment_id {
            payload["amendment_id"] = serde_json::json!(amend_id);
        }
        
        if let Some(prev_amend_id) = options.previous_amendment_id {
            payload["previous_amendment_id"] = serde_json::json!(prev_amend_id);
        }
        
        if let Some(text_hash) = options.text_hash {
            payload["text_hash"] = serde_json::json!(text_hash);
        }
        
        if let Some(epoch) = options.ratified_in_epoch {
            payload["ratified_in_epoch"] = serde_json::json!(epoch);
        }
        
        if let Some(desc) = options.description {
            payload["description"] = serde_json::json!(desc);
        }
        
        let identity = self.identity.lock().unwrap();
        let private_key = identity.get_private_key();
        
        let now = chrono::Utc::now();
        let id = format!("urn:anchor:{}:{}", options.anchor_type, uuid::Uuid::new_v4());
        
        let mut credential = serde_json::json!({
            "id": id,
            "title": format!("{} Amendment", options.federation.name),
            "type": ["VerifiableCredential", "AnchorCredential", "AmendmentCredential"],
            "issuer": {
                "did": options.federation.did,
                "name": options.federation.name,
            },
            "subjectDid": options.subject_did,
            "issuanceDate": now.to_rfc3339(),
            "credentialSubject": {
                "id": options.subject_did,
                "anchorType": options.anchor_type,
                "effective_from": options.effective_from,
                "dag_root_hash": options.dag_root_hash,
                "referenced_credentials": options.referenced_credentials,
            },
            "metadata": {
                "federation": {
                    "id": options.federation.id,
                    "name": options.federation.name,
                },
                "dag": {
                    "root_hash": options.dag_root_hash,
                    "timestamp": now.to_rfc3339(),
                },
                "description": options.description,
            },
            "proof": {
                "type": "Ed25519Signature2020",
                "created": now.to_rfc3339(),
                "verificationMethod": format!("{}#controller", options.federation.did),
                "proofPurpose": "assertionMethod",
                "jws": "",
            }
        });
        
        if options.anchor_type == "amendment" {
            if let Some(amendment_id) = options.amendment_id {
                credential["credentialSubject"]["amendment_id"] = serde_json::json!(amendment_id);
            }
            
            if let Some(prev_id) = options.previous_amendment_id {
                credential["credentialSubject"]["previous_amendment_id"] = serde_json::json!(prev_id);
            }
            
            if let Some(text_hash) = options.text_hash {
                credential["credentialSubject"]["text_hash"] = serde_json::json!(text_hash);
            }
            
            if let Some(epoch) = options.ratified_in_epoch {
                credential["credentialSubject"]["ratified_in_epoch"] = serde_json::json!(epoch);
            }
        }
        
        let data_to_sign = serde_json::to_string(&credential).unwrap_or_default();
        let mut hasher = sha2::Sha256::new();
        hasher.update(data_to_sign.as_bytes());
        let hash_result = hasher.finalize();
        let signature = format!("mock_sig_{}", hex::encode(hash_result));
        
        credential["proof"]["jws"] = serde_json::json!(signature);
        
        Ok(credential)
    }
    
    pub fn get_current_dag_root(&self, federation_id: &str) -> Result<DagRootInfo, Error> {
        let root_hash = format!("dag_root_{}", uuid::Uuid::new_v4());
        let timestamp = chrono::Utc::now();
        
        Ok(DagRootInfo {
            root_hash,
            timestamp,
            block_height: 1000,
        })
    }
    
    pub fn get_federation_did(&self, federation_id: &str) -> Result<String, Error> {
        Ok(format!("did:icn:federation:{}", federation_id))
    }
}

#[derive(Debug, Clone)]
pub struct DagRootInfo {
    pub root_hash: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub block_height: u64,
} 