use anyhow::Result;
use sqlx::{Pool, Postgres};
use sha2::{Sha256, Digest};
use uuid::Uuid;
use std::sync::Arc;

use crate::dag::types::{
    AnchorRequest, AnchorResponse, DagNode, DagNodeType,
    ThreadAnchorRequest, MessageAnchorRequest
};
use crate::models::{Thread, Message};

#[derive(Clone)]
pub struct DagService {
    db_pool: Arc<Pool<Postgres>>,
}

impl DagService {
    pub fn new(db_pool: Arc<Pool<Postgres>>) -> Self {
        Self { db_pool }
    }

    pub async fn anchor_thread(&self, request: ThreadAnchorRequest) -> Result<AnchorResponse> {
        let thread_id = request.thread_id;
        
        // Get thread data from database
        let thread = sqlx::query_as!(
            Thread,
            r#"SELECT * FROM threads WHERE id = $1"#,
            thread_id
        )
        .fetch_one(self.db_pool.as_ref())
        .await?;
        
        // Serialize thread data
        let thread_json = serde_json::to_string(&thread)?;
        
        // Create content hash
        let mut hasher = Sha256::new();
        hasher.update(thread_json.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());
        
        // Create DAG node
        let dag_node = DagNode {
            id: Uuid::new_v4().to_string(),
            node_type: DagNodeType::Thread,
            content_hash: content_hash.clone(),
            signature: request.signature,
            signer_did: request.signer_did,
            parent_refs: vec![],
            created_at: chrono::Utc::now(),
            metadata: None,
        };
        
        // Store DAG node
        let dag_node_json = serde_json::to_string(&dag_node)?;
        let dag_ref = dag_node.id.clone();
        
        sqlx::query!(
            r#"
            INSERT INTO dag_nodes (id, node_type, content_hash, signature, signer_did, parent_refs, content, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            dag_node.id,
            format!("{:?}", dag_node.node_type),
            dag_node.content_hash,
            dag_node.signature,
            dag_node.signer_did,
            &dag_node.parent_refs,
            dag_node_json,
            dag_node.created_at
        )
        .execute(self.db_pool.as_ref())
        .await?;
        
        // Update thread with DAG reference
        sqlx::query!(
            r#"
            UPDATE threads
            SET dag_ref = $1
            WHERE id = $2
            "#,
            dag_ref,
            thread_id
        )
        .execute(self.db_pool.as_ref())
        .await?;
        
        Ok(AnchorResponse {
            dag_ref,
            content_hash,
        })
    }

    pub async fn anchor_message(&self, request: MessageAnchorRequest) -> Result<AnchorResponse> {
        let message_id = request.message_id;
        
        // Get message data from database
        let message = sqlx::query_as!(
            Message,
            r#"SELECT * FROM messages WHERE id = $1"#,
            message_id
        )
        .fetch_one(self.db_pool.as_ref())
        .await?;
        
        // Get thread DAG reference for parent ref
        let thread = sqlx::query!(
            r#"SELECT dag_ref FROM threads WHERE id = $1"#,
            request.thread_id
        )
        .fetch_one(self.db_pool.as_ref())
        .await?;
        
        let parent_refs = match thread.dag_ref {
            Some(ref dag_ref) => vec![dag_ref.clone()],
            None => vec![],
        };
        
        // Serialize message data
        let message_json = serde_json::to_string(&message)?;
        
        // Create content hash
        let mut hasher = Sha256::new();
        hasher.update(message_json.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());
        
        // Create DAG node
        let dag_node = DagNode {
            id: Uuid::new_v4().to_string(),
            node_type: DagNodeType::Message,
            content_hash: content_hash.clone(),
            signature: request.signature,
            signer_did: request.signer_did,
            parent_refs,
            created_at: chrono::Utc::now(),
            metadata: None,
        };
        
        // Store DAG node
        let dag_node_json = serde_json::to_string(&dag_node)?;
        let dag_ref = dag_node.id.clone();
        
        sqlx::query!(
            r#"
            INSERT INTO dag_nodes (id, node_type, content_hash, signature, signer_did, parent_refs, content, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            dag_node.id,
            format!("{:?}", dag_node.node_type),
            dag_node.content_hash,
            dag_node.signature,
            dag_node.signer_did,
            &dag_node.parent_refs,
            dag_node_json,
            dag_node.created_at
        )
        .execute(self.db_pool.as_ref())
        .await?;
        
        // Update message with DAG reference and anchored status
        sqlx::query!(
            r#"
            UPDATE messages
            SET dag_ref = $1, dag_anchored = true
            WHERE id = $2
            "#,
            dag_ref,
            message_id
        )
        .execute(self.db_pool.as_ref())
        .await?;
        
        Ok(AnchorResponse {
            dag_ref,
            content_hash,
        })
    }

    pub async fn create_thread_summary(&self, thread_id: Uuid, content: &str, signer_did: &str, signature: &str) -> Result<AnchorResponse> {
        // Get thread DAG reference for parent ref
        let thread = sqlx::query!(
            r#"SELECT dag_ref FROM threads WHERE id = $1"#,
            thread_id
        )
        .fetch_one(self.db_pool.as_ref())
        .await?;
        
        let parent_refs = match thread.dag_ref {
            Some(ref dag_ref) => vec![dag_ref.clone()],
            None => vec![],
        };
        
        // Create content hash
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());
        
        // Create DAG node
        let dag_node = DagNode {
            id: Uuid::new_v4().to_string(),
            node_type: DagNodeType::ThreadSummary,
            content_hash: content_hash.clone(),
            signature: signature.to_string(),
            signer_did: signer_did.to_string(),
            parent_refs,
            created_at: chrono::Utc::now(),
            metadata: Some(serde_json::json!({
                "thread_id": thread_id.to_string(),
                "summary": content
            })),
        };
        
        // Store DAG node
        let dag_node_json = serde_json::to_string(&dag_node)?;
        let dag_ref = dag_node.id.clone();
        
        sqlx::query!(
            r#"
            INSERT INTO dag_nodes (id, node_type, content_hash, signature, signer_did, parent_refs, content, created_at, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            dag_node.id,
            format!("{:?}", dag_node.node_type),
            dag_node.content_hash,
            dag_node.signature,
            dag_node.signer_did,
            &dag_node.parent_refs,
            dag_node_json,
            dag_node.created_at,
            serde_json::to_value(dag_node.metadata).ok()
        )
        .execute(self.db_pool.as_ref())
        .await?;
        
        Ok(AnchorResponse {
            dag_ref,
            content_hash,
        })
    }
} 