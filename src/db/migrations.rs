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
    ]
} 