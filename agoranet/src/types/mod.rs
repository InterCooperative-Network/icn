// Types module for AgoraNet
// Will contain data structures and serialization/deserialization logic

// Thread related types
pub mod thread {
    use serde::{Serialize, Deserialize};
    use chrono::{DateTime, Utc};
    use uuid::Uuid;
    use sqlx::FromRow;

    #[derive(Debug, Serialize, Deserialize, FromRow)]
    pub struct Thread {
        pub id: Uuid,
        pub title: String,
        pub proposal_cid: Option<String>,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }
    
    // Thread response shape for API endpoints
    #[derive(Debug, Serialize)]
    pub struct ThreadResponse {
        pub id: String,
        pub title: String,
        pub proposal_cid: Option<String>,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
        pub message_count: Option<i64>,
    }
    
    impl From<Thread> for ThreadResponse {
        fn from(thread: Thread) -> Self {
            Self {
                id: thread.id.to_string(),
                title: thread.title,
                proposal_cid: thread.proposal_cid,
                created_at: thread.created_at,
                updated_at: thread.updated_at,
                message_count: None,
            }
        }
    }
}

// Credential related types
pub mod credential {
    use serde::{Serialize, Deserialize};
    use chrono::{DateTime, Utc};
    use uuid::Uuid;
    use sqlx::FromRow;

    #[derive(Debug, Serialize, Deserialize, FromRow)]
    pub struct CredentialLink {
        pub id: Uuid,
        pub thread_id: Uuid,
        pub credential_cid: String,
        pub linked_by: String, // DID
        pub created_at: DateTime<Utc>,
    }
    
    // Credential link response for API endpoints
    #[derive(Debug, Serialize)]
    pub struct CredentialLinkResponse {
        pub id: String,
        pub thread_id: String,
        pub credential_cid: String,
        pub linked_by: String,
        pub timestamp: i64,
    }
    
    impl From<CredentialLink> for CredentialLinkResponse {
        fn from(link: CredentialLink) -> Self {
            Self {
                id: link.id.to_string(),
                thread_id: link.thread_id.to_string(),
                credential_cid: link.credential_cid,
                linked_by: link.linked_by,
                timestamp: link.created_at.timestamp(),
            }
        }
    }
}

// Message related types
pub mod message {
    use serde::{Serialize, Deserialize};
    use chrono::{DateTime, Utc};
    use uuid::Uuid;
    use sqlx::FromRow;

    #[derive(Debug, Serialize, Deserialize, FromRow)]
    pub struct Message {
        pub id: Uuid,
        pub thread_id: Uuid,
        pub author_did: Option<String>,
        pub content: String,
        pub reply_to: Option<Uuid>,
        pub is_system: bool,
        pub metadata: Option<String>,
        pub created_at: DateTime<Utc>,
    }
    
    // Message response for API endpoints
    #[derive(Debug, Serialize)]
    pub struct MessageResponse {
        pub id: String,
        pub thread_id: String,
        pub author_did: Option<String>,
        pub content: String,
        pub reply_to: Option<String>,
        pub is_system: bool,
        pub created_at: DateTime<Utc>,
        pub reactions: Option<Vec<ReactionCount>>,
    }
    
    #[derive(Debug, Serialize)]
    pub struct ReactionCount {
        pub reaction_type: String,
        pub count: i64,
    }
    
    impl From<Message> for MessageResponse {
        fn from(msg: Message) -> Self {
            Self {
                id: msg.id.to_string(),
                thread_id: msg.thread_id.to_string(),
                author_did: msg.author_did,
                content: msg.content,
                reply_to: msg.reply_to.map(|id| id.to_string()),
                is_system: msg.is_system,
                created_at: msg.created_at,
                reactions: None,
            }
        }
    }
    
    // Request to create a message
    #[derive(Debug, Deserialize)]
    pub struct CreateMessageRequest {
        pub content: String,
        pub reply_to: Option<String>,
    }
}

// Reaction related types
pub mod reaction {
    use serde::{Serialize, Deserialize};
    use chrono::{DateTime, Utc};
    use uuid::Uuid;
    use sqlx::FromRow;

    #[derive(Debug, Serialize, Deserialize, FromRow)]
    pub struct Reaction {
        pub id: Uuid,
        pub message_id: Uuid,
        pub author_did: String,
        pub reaction_type: String,
        pub created_at: DateTime<Utc>,
    }
    
    // Reaction request and response for API endpoints
    #[derive(Debug, Serialize, Deserialize)]
    pub struct ReactionRequest {
        pub reaction_type: String,
    }
    
    #[derive(Debug, Serialize)]
    pub struct ReactionResponse {
        pub id: String,
        pub message_id: String,
        pub author_did: String,
        pub reaction_type: String,
        pub created_at: DateTime<Utc>,
    }
    
    impl From<Reaction> for ReactionResponse {
        fn from(reaction: Reaction) -> Self {
            Self {
                id: reaction.id.to_string(),
                message_id: reaction.message_id.to_string(),
                author_did: reaction.author_did,
                reaction_type: reaction.reaction_type,
                created_at: reaction.created_at,
            }
        }
    }
} 