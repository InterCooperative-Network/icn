use crate::resources::{Resource, ResourceToken, TokenBurn};
use chrono::{DateTime, Duration, Utc, NaiveDateTime};

/// Federation usage statistics
pub struct FederationStats {
    /// Federation ID
    pub federation_id: String,
    /// Total tokens burned in this federation
    pub total_tokens_burned: f64,
    /// Average daily token burn
    pub avg_daily_burn: f64,
    /// Peak usage day
    pub peak_daily_burn: f64,
    /// Date of peak usage
    pub peak_date: Option<DateTime<Utc>>,
    /// How many days of data were analyzed
    pub period_days: i64,
}

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

    /// Get aggregated token burn statistics for all federations
    pub fn get_federation_burn_stats(&self, period_days: Option<i64>) -> Result<Vec<FederationStats>, Error> {
        let conn = self.pool.get()?;
        
        // Calculate cutoff date if period is specified
        let date_filter = if let Some(days) = period_days {
            let cutoff = Utc::now() - Duration::days(days);
            format!(" AND timestamp >= {}", cutoff.timestamp())
        } else {
            String::new()
        };
        
        // Get unique federation IDs
        let mut stmt = conn.prepare(
            &format!("SELECT DISTINCT federation_scope FROM token_burns WHERE 1=1{}", date_filter)
        )?;
        
        let federation_ids: Vec<String> = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
        
        let mut results = Vec::new();
        
        for federation_id in federation_ids {
            // Calculate total burn for this federation
            let mut stmt = conn.prepare(
                &format!("SELECT SUM(amount) FROM token_burns WHERE federation_scope = ?{}", date_filter)
            )?;
            
            let total_burn: f64 = stmt.query_row([&federation_id], |row| {
                row.get::<_, f64>(0)
            }).unwrap_or(0.0);
            
            // Get oldest timestamp to calculate duration
            let mut stmt = conn.prepare(
                &format!("SELECT MIN(timestamp) FROM token_burns WHERE federation_scope = ?{}", date_filter)
            )?;
            
            let oldest_timestamp: i64 = stmt.query_row([&federation_id], |row| {
                row.get::<_, i64>(0)
            }).unwrap_or(Utc::now().timestamp());
            
            // Calculate duration in days
            let start_date = DateTime::<Utc>::from_timestamp(oldest_timestamp, 0)
                .unwrap_or_default();
            let now = Utc::now();
            let duration_days = (now - start_date).num_days().max(1); // At least 1 day
            
            // Calculate daily average
            let avg_daily = total_burn / duration_days as f64;
            
            // Find peak day
            let mut stmt = conn.prepare(
                &format!("
                    SELECT 
                        DATE(datetime(timestamp, 'unixepoch')) as day,
                        SUM(amount) as daily_total
                    FROM token_burns 
                    WHERE federation_scope = ?{}
                    GROUP BY day
                    ORDER BY daily_total DESC
                    LIMIT 1
                ", date_filter)
            )?;
            
            let (peak_day, peak_amount): (String, f64) = stmt.query_row([&federation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                ))
            }).unwrap_or(("unknown".to_string(), 0.0));
            
            // Parse peak date
            let peak_date = if peak_day != "unknown" {
                NaiveDateTime::parse_from_str(&format!("{} 00:00:00", peak_day), "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            } else {
                None
            };
            
            results.push(FederationStats {
                federation_id,
                total_tokens_burned: total_burn,
                avg_daily_burn: avg_daily,
                peak_daily_burn: peak_amount,
                peak_date,
                period_days: duration_days,
            });
        }
        
        Ok(results)
    }
    
    /// Gets token burn statistics for a specific federation 
    pub fn get_federation_burn_stats_by_id(&self, federation_id: &str, period_days: Option<i64>) -> Result<Option<FederationStats>, Error> {
        self.get_federation_burn_stats(period_days)?
            .into_iter()
            .find(|stats| stats.federation_id == federation_id)
            .map_or(Ok(None), |stats| Ok(Some(stats)))
    }
} 