use actix_web::{post, get, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Mutex;
use crate::models::thread::Thread;
use crate::database::Database;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LinkedCredential {
    /// Unique ID for this credential link
    pub id: String,
    
    /// ID of the credential being linked
    pub credential_id: String,
    
    /// ID of the proposal this credential is related to
    pub proposal_id: String,
    
    /// DID of the credential issuer
    pub issuer_did: String,
    
    /// DID of the credential subject
    pub subject_did: String,
    
    /// Type of credential (e.g., "vote", "finalization", "proposal")
    pub credential_type: String,
    
    /// ID of the thread this credential is linked to
    pub thread_id: String,
    
    /// Timestamp when this link was created
    pub created_at: String,
    
    /// Optional metadata about the credential
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct CredentialLinkRequest {
    /// ID of the credential to link
    pub credential_id: String,
    
    /// ID of the proposal related to this credential
    pub proposal_id: String,
    
    /// DID of the credential issuer
    pub issuer_did: String,
    
    /// DID of the credential subject
    pub subject_did: String,
    
    /// Type of credential (e.g., "vote", "finalization", "proposal")
    pub credential_type: String,
    
    /// Optional ID of the thread to link to (if not provided, will find by proposal_id)
    pub thread_id: Option<String>,
    
    /// Optional metadata about the credential
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CredentialLinkResponse {
    /// The linked credential data
    pub linked_credential: LinkedCredential,
    
    /// URL to the linked thread
    pub thread_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GetCredentialLinksRequest {
    /// Optional thread ID to filter by
    pub thread_id: Option<String>,
    
    /// Optional proposal ID to filter by
    pub proposal_id: Option<String>,
    
    /// Optional DID to filter by (subject or issuer)
    pub did: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GetCredentialLinksResponse {
    /// List of linked credentials
    pub linked_credentials: Vec<LinkedCredential>,
}

/// Link a credential to a thread
/// 
/// This endpoint allows users to link their verifiable credentials to discussion threads,
/// providing cryptographic proof of their participation in governance processes.
#[post("/api/threads/credential-link")]
pub async fn link_credential(
    db: web::Data<Mutex<Database>>,
    req: web::Json<CredentialLinkRequest>,
) -> impl Responder {
    let mut db = db.lock().unwrap();
    
    // Find the thread by ID or proposal ID
    let thread_id = match &req.thread_id {
        Some(id) => id.clone(),
        None => {
            // Find thread by proposal ID
            match db.threads.iter().find(|t| t.proposal_id == Some(req.proposal_id.clone())) {
                Some(thread) => thread.id.clone(),
                None => {
                    return HttpResponse::NotFound().json(serde_json::json!({
                        "error": "Thread not found for the given proposal ID"
                    }));
                }
            }
        }
    };
    
    // Ensure the thread exists
    if !db.threads.iter().any(|t| t.id == thread_id) {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "Thread not found"
        }));
    }
    
    // Create the link
    let linked_credential = LinkedCredential {
        id: Uuid::new_v4().to_string(),
        credential_id: req.credential_id.clone(),
        proposal_id: req.proposal_id.clone(),
        issuer_did: req.issuer_did.clone(),
        subject_did: req.subject_did.clone(),
        credential_type: req.credential_type.clone(),
        thread_id: thread_id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: req.metadata.clone(),
    };
    
    // Store the link
    db.credential_links.push(linked_credential.clone());
    
    // Generate thread URL
    let thread_url = format!("/threads/{}", thread_id);
    
    HttpResponse::Created().json(CredentialLinkResponse {
        linked_credential,
        thread_url,
    })
}

/// Get credentials linked to a thread
/// 
/// Retrieves all credentials linked to a specific thread, proposal, or DID.
#[get("/api/threads/credential-links")]
pub async fn get_credential_links(
    db: web::Data<Mutex<Database>>,
    query: web::Query<GetCredentialLinksRequest>,
) -> impl Responder {
    let db = db.lock().unwrap();
    
    // Filter credential links based on query parameters
    let linked_credentials: Vec<LinkedCredential> = db.credential_links.iter()
        .filter(|link| {
            // Filter by thread ID if provided
            if let Some(thread_id) = &query.thread_id {
                if link.thread_id != *thread_id {
                    return false;
                }
            }
            
            // Filter by proposal ID if provided
            if let Some(proposal_id) = &query.proposal_id {
                if link.proposal_id != *proposal_id {
                    return false;
                }
            }
            
            // Filter by DID if provided
            if let Some(did) = &query.did {
                if link.subject_did != *did && link.issuer_did != *did {
                    return false;
                }
            }
            
            true
        })
        .cloned()
        .collect();
    
    HttpResponse::Ok().json(GetCredentialLinksResponse {
        linked_credentials,
    })
}

/// Register credential linking routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(link_credential)
       .service(get_credential_links);
} 