use crate::resources::{Resource, ResourceToken, TokenBurn};

impl Repository {
    /// Records a token burn in the database
    pub fn record_token_burn(&self, burn: &TokenBurn) -> Result<(), Error> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO token_burns (id, token_id, amount, token_type, federation_scope, owner_did, timestamp, job_id, job_type, proposal_id, receipt_id, reason) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                burn.id,
                burn.token_id,
                burn.amount,
                burn.token_type,
                burn.federation_scope,
                burn.owner_did,
                burn.timestamp,
                burn.job_id,
                burn.job_type,
                burn.proposal_id,
                burn.receipt_id,
                burn.reason,
            ],
        )?;
        Ok(())
    }

    /// Retrieves all token burns for a specific owner
    pub fn get_token_burns_by_owner(&self, owner_did: &str) -> Result<Vec<TokenBurn>, Error> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, token_id, amount, token_type, federation_scope, owner_did, timestamp, job_id, job_type, proposal_id, receipt_id, reason 
             FROM token_burns 
             WHERE owner_did = ?1 
             ORDER BY timestamp DESC"
        )?;
        
        let burn_iter = stmt.query_map(params![owner_did], |row| {
            Ok(TokenBurn {
                id: row.get(0)?,
                token_id: row.get(1)?,
                amount: row.get(2)?,
                token_type: row.get(3)?,
                federation_scope: row.get(4)?,
                owner_did: row.get(5)?,
                timestamp: row.get(6)?,
                job_id: row.get(7)?,
                job_type: row.get(8)?,
                proposal_id: row.get(9)?,
                receipt_id: row.get(10)?,
                reason: row.get(11)?,
            })
        })?;

        let mut burns = Vec::new();
        for burn in burn_iter {
            burns.push(burn?);
        }
        
        Ok(burns)
    }

    /// Retrieves token burns filtered by various criteria
    pub fn get_token_burns_filtered(
        &self,
        owner_did: Option<&str>,
        token_type: Option<&str>,
        federation_scope: Option<&str>,
        job_id: Option<&str>,
        job_type: Option<&str>,
        proposal_id: Option<&str>,
        receipt_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<TokenBurn>, Error> {
        let conn = self.pool.get()?;
        
        let mut query = String::from(
            "SELECT id, token_id, amount, token_type, federation_scope, owner_did, timestamp, job_id, job_type, proposal_id, receipt_id, reason 
             FROM token_burns WHERE 1=1"
        );
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();
        
        if let Some(owner) = owner_did {
            query.push_str(" AND owner_did = ?");
            params.push(Box::new(owner.to_string()));
        }
        
        if let Some(t_type) = token_type {
            query.push_str(" AND token_type = ?");
            params.push(Box::new(t_type.to_string()));
        }
        
        if let Some(fed_scope) = federation_scope {
            query.push_str(" AND federation_scope = ?");
            params.push(Box::new(fed_scope.to_string()));
        }
        
        if let Some(job) = job_id {
            query.push_str(" AND job_id = ?");
            params.push(Box::new(job.to_string()));
        }
        
        if let Some(j_type) = job_type {
            query.push_str(" AND job_type = ?");
            params.push(Box::new(j_type.to_string()));
        }
        
        if let Some(proposal) = proposal_id {
            query.push_str(" AND proposal_id = ?");
            params.push(Box::new(proposal.to_string()));
        }
        
        if let Some(receipt) = receipt_id {
            query.push_str(" AND receipt_id = ?");
            params.push(Box::new(receipt.to_string()));
        }
        
        query.push_str(" ORDER BY timestamp DESC");
        
        if let Some(lim) = limit {
            query.push_str(" LIMIT ?");
            params.push(Box::new(lim));
        }
        
        let mut stmt = conn.prepare(&query)?;
        
        let param_refs: Vec<&dyn ToSql> = params.iter()
            .map(|p| p.as_ref())
            .collect();
        
        let burn_iter = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(TokenBurn {
                id: row.get(0)?,
                token_id: row.get(1)?,
                amount: row.get(2)?,
                token_type: row.get(3)?,
                federation_scope: row.get(4)?,
                owner_did: row.get(5)?,
                timestamp: row.get(6)?,
                job_id: row.get(7)?,
                job_type: row.get(8)?,
                proposal_id: row.get(9)?,
                receipt_id: row.get(10)?,
                reason: row.get(11)?,
            })
        })?;

        let mut burns = Vec::new();
        for burn in burn_iter {
            burns.push(burn?);
        }
        
        Ok(burns)
    }
} 