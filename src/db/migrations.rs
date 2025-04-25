fn get_migrations() -> Vec<&'static str> {
    vec![
        // ... existing migrations ...
        
        // Token Burns Table for tracking resource consumption
        "CREATE TABLE IF NOT EXISTS token_burns (
            id TEXT PRIMARY KEY,
            token_id TEXT NOT NULL,
            amount INTEGER NOT NULL,
            token_type TEXT NOT NULL,
            federation_scope TEXT NOT NULL,
            owner_did TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            job_id TEXT,
            receipt_id TEXT,
            reason TEXT,
            FOREIGN KEY (token_id) REFERENCES tokens(id)
        )",
        
        // Add job_type and proposal_id fields to token_burns table
        "ALTER TABLE token_burns ADD COLUMN job_type TEXT;",
        "ALTER TABLE token_burns ADD COLUMN proposal_id TEXT;",
        
        // Create indices for the new columns
        "CREATE INDEX IF NOT EXISTS idx_token_burns_job_type ON token_burns(job_type);",
        "CREATE INDEX IF NOT EXISTS idx_token_burns_proposal_id ON token_burns(proposal_id);",
    ]
} 