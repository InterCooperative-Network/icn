use crate::resources::token_burn::TokenBurn;

impl WalletDb {
    /// Records a new token burn in the database
    pub fn add_token_burn(&self, burn: &TokenBurn) -> Result<(), rusqlite::Error> {
        let conn = self.connection.lock().unwrap();
        conn.execute(
            "INSERT INTO token_burns 
            (id, token_id, amount, token_type, federation_scope, owner_did, timestamp, job_id, receipt_id, reason) 
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                burn.id,
                burn.token_id,
                burn.amount,
                burn.token_type,
                burn.federation_scope,
                burn.owner_did,
                burn.timestamp,
                burn.job_id,
                burn.receipt_id,
                burn.reason,
            ],
        )?;
        Ok(())
    }

    /// Gets all token burns from the database
    pub fn get_all_token_burns(&self) -> Result<Vec<TokenBurn>, rusqlite::Error> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, token_id, amount, token_type, federation_scope, owner_did, timestamp, job_id, receipt_id, reason 
            FROM token_burns 
            ORDER BY timestamp DESC"
        )?;
        
        let burns = stmt.query_map([], |row| {
            Ok(TokenBurn {
                id: row.get(0)?,
                token_id: row.get(1)?,
                amount: row.get(2)?,
                token_type: row.get(3)?,
                federation_scope: row.get(4)?,
                owner_did: row.get(5)?,
                timestamp: row.get(6)?,
                job_id: row.get(7)?,
                receipt_id: row.get(8)?,
                reason: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
        
        Ok(burns)
    }

    /// Gets token burns for a specific token type
    pub fn get_token_burns_by_type(&self, token_type: &str) -> Result<Vec<TokenBurn>, rusqlite::Error> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, token_id, amount, token_type, federation_scope, owner_did, timestamp, job_id, receipt_id, reason 
            FROM token_burns 
            WHERE token_type = ?1
            ORDER BY timestamp DESC"
        )?;
        
        let burns = stmt.query_map([token_type], |row| {
            Ok(TokenBurn {
                id: row.get(0)?,
                token_id: row.get(1)?,
                amount: row.get(2)?,
                token_type: row.get(3)?,
                federation_scope: row.get(4)?,
                owner_did: row.get(5)?,
                timestamp: row.get(6)?,
                job_id: row.get(7)?,
                receipt_id: row.get(8)?,
                reason: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
        
        Ok(burns)
    }
    
    /// Gets token burns for a specific federation scope
    pub fn get_token_burns_by_federation(&self, federation: &str) -> Result<Vec<TokenBurn>, rusqlite::Error> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, token_id, amount, token_type, federation_scope, owner_did, timestamp, job_id, receipt_id, reason 
            FROM token_burns 
            WHERE federation_scope = ?1
            ORDER BY timestamp DESC"
        )?;
        
        let burns = stmt.query_map([federation], |row| {
            Ok(TokenBurn {
                id: row.get(0)?,
                token_id: row.get(1)?,
                amount: row.get(2)?,
                token_type: row.get(3)?,
                federation_scope: row.get(4)?,
                owner_did: row.get(5)?,
                timestamp: row.get(6)?,
                job_id: row.get(7)?,
                receipt_id: row.get(8)?,
                reason: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
        
        Ok(burns)
    }
} 