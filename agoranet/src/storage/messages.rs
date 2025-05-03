use sqlx::PgPool;
use crate::types::message::Message;
use super::{Result, StorageError};
use uuid::Uuid;

pub struct MessageRepository {
    pool: PgPool,
}

impl MessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_message(&self, thread_id: Uuid, author_did: &str, content: &str, reply_to: Option<Uuid>) -> Result<Message> {
        let id = Uuid::new_v4();
        
        let message = sqlx::query_as!(
            Message,
            r#"
            INSERT INTO messages (id, thread_id, author_did, content, reply_to, is_system)
            VALUES ($1, $2, $3, $4, $5, false)
            RETURNING id, thread_id, author_did, content, reply_to, is_system, metadata, created_at
            "#,
            id,
            thread_id,
            author_did,
            content,
            reply_to
        )
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(message)
    }
    
    pub async fn create_system_message(&self, thread_id: Uuid, content: &str, metadata: Option<&str>) -> Result<Message> {
        let id = Uuid::new_v4();
        
        let message = sqlx::query_as!(
            Message,
            r#"
            INSERT INTO messages (id, thread_id, content, is_system, metadata)
            VALUES ($1, $2, $3, true, $4)
            RETURNING id, thread_id, author_did, content, reply_to, is_system, metadata, created_at
            "#,
            id,
            thread_id,
            content,
            metadata
        )
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(message)
    }
    
    pub async fn get_message(&self, id: Uuid) -> Result<Message> {
        let message = sqlx::query_as!(
            Message,
            r#"
            SELECT id, thread_id, author_did, content, reply_to, is_system, metadata, created_at
            FROM messages
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::Database)?
        .ok_or(StorageError::NotFound)?;

        Ok(message)
    }
    
    pub async fn get_messages_for_thread(&self, thread_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Message>> {
        let messages = sqlx::query_as!(
            Message,
            r#"
            SELECT id, thread_id, author_did, content, reply_to, is_system, metadata, created_at
            FROM messages
            WHERE thread_id = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            "#,
            thread_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(messages)
    }
    
    pub async fn get_replies_to_message(&self, message_id: Uuid) -> Result<Vec<Message>> {
        let messages = sqlx::query_as!(
            Message,
            r#"
            SELECT id, thread_id, author_did, content, reply_to, is_system, metadata, created_at
            FROM messages
            WHERE reply_to = $1
            ORDER BY created_at ASC
            "#,
            message_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(messages)
    }
    
    pub async fn count_messages_for_thread(&self, thread_id: Uuid) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM messages
            WHERE thread_id = $1
            "#,
            thread_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Database)?
        .count
        .unwrap_or(0);

        Ok(count)
    }
    
    pub async fn delete_message(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query!(
            r#"
            DELETE FROM messages
            WHERE id = $1
            "#,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::Database)?;
        
        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound);
        }
        
        Ok(())
    }
} 