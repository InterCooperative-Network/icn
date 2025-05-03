use sqlx::PgPool;
use crate::types::credential::CredentialLink;
use crate::routes::credentials::CredentialLinkRequest;
use super::{Result, StorageError};
use uuid::Uuid;

pub struct CredentialLinkRepository {
    pool: PgPool,
}

impl CredentialLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_credential_links(&self) -> Result<Vec<CredentialLink>> {
        let links = sqlx::query_as!(
            CredentialLink,
            r#"
            SELECT id, thread_id, credential_cid, linked_by as "linked_by: String", created_at
            FROM credential_links
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(links)
    }

    pub async fn create_credential_link(&self, link_req: &CredentialLinkRequest) -> Result<CredentialLink> {
        let id = Uuid::new_v4();
        // Parse thread_id from string to UUID
        let thread_id = Uuid::parse_str(&link_req.thread_id)
            .map_err(|_| StorageError::Other("Invalid thread_id format".to_string()))?;
        
        let link = sqlx::query_as!(
            CredentialLink,
            r#"
            INSERT INTO credential_links (id, thread_id, credential_cid, linked_by)
            VALUES ($1, $2, $3, $4)
            RETURNING id, thread_id, credential_cid, linked_by as "linked_by: String", created_at
            "#,
            id,
            thread_id,
            &link_req.credential_cid,
            &link_req.signer_did
        )
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(link)
    }

    pub async fn get_links_for_thread(&self, thread_id: Uuid) -> Result<Vec<CredentialLink>> {
        let links = sqlx::query_as!(
            CredentialLink,
            r#"
            SELECT id, thread_id, credential_cid, linked_by as "linked_by: String", created_at
            FROM credential_links
            WHERE thread_id = $1
            ORDER BY created_at DESC
            "#,
            thread_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::Database)?;

        Ok(links)
    }
} 