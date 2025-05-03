use sqlx::PgPool;
use crate::types::reaction::Reaction;
use super::{Result, StorageError};
use uuid::Uuid;

pub struct ReactionRepository {
    pool: PgPool,
}

impl ReactionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn add_reaction(&self, message_id: Uuid, author_did: &str, reaction_type: &str) -> Result<Reaction> {
        // First check if this user already reacted to this message with this reaction type
        let existing = sqlx::query!(
            r#"
            SELECT id FROM reactions
            WHERE message_id = $1 AND author_did = $2 AND reaction_type = $3
            "#,
            message_id,
            author_did,
            reaction_type
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::Database)?;
        
        // If reaction already exists, return error
        if existing.is_some() {
            return Err(StorageError::Other("Reaction already exists".to_string()));
        }
        
        let id = Uuid::new_v4();
        
        let reaction = sqlx::query_as!(
            Reaction,
            r#"
            INSERT INTO reactions (id, message_id, author_did, reaction_type)
            VALUES ($1, $2, $3, $4)
            RETURNING id, message_id, author_did, reaction_type, created_at
            "#,
            id,
            message_id,
            author_did,
            reaction_type
        )
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(reaction)
    }
    
    pub async fn remove_reaction(&self, message_id: Uuid, author_did: &str, reaction_type: &str) -> Result<()> {
        let result = sqlx::query!(
            r#"
            DELETE FROM reactions
            WHERE message_id = $1 AND author_did = $2 AND reaction_type = $3
            "#,
            message_id,
            author_did,
            reaction_type
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::Database)?;
        
        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound);
        }
        
        Ok(())
    }
    
    pub async fn get_reactions_for_message(&self, message_id: Uuid) -> Result<Vec<Reaction>> {
        let reactions = sqlx::query_as!(
            Reaction,
            r#"
            SELECT id, message_id, author_did, reaction_type, created_at
            FROM reactions
            WHERE message_id = $1
            ORDER BY created_at ASC
            "#,
            message_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(reactions)
    }
    
    pub async fn count_reactions_by_type(&self, message_id: Uuid) -> Result<Vec<(String, i64)>> {
        let counts = sqlx::query!(
            r#"
            SELECT reaction_type, COUNT(*) as count
            FROM reactions
            WHERE message_id = $1
            GROUP BY reaction_type
            ORDER BY count DESC
            "#,
            message_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Database)?;
        
        let result = counts.into_iter()
            .map(|row| (row.reaction_type, row.count.unwrap_or(0)))
            .collect();
            
        Ok(result)
    }
    
    pub async fn has_user_reacted(&self, message_id: Uuid, author_did: &str, reaction_type: &str) -> Result<bool> {
        let exists = sqlx::query!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM reactions
                WHERE message_id = $1 AND author_did = $2 AND reaction_type = $3
            ) as exists
            "#,
            message_id,
            author_did,
            reaction_type
        )
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Database)?
        .exists
        .unwrap_or(false);
        
        Ok(exists)
    }
} 