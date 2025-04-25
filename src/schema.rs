/// Create the token_burns table to track resource token consumption
pub fn create_token_burns_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS token_burns (
            id TEXT PRIMARY KEY,
            token_id TEXT NOT NULL,
            amount REAL NOT NULL,
            token_type TEXT NOT NULL,
            federation_scope TEXT NOT NULL,
            owner_did TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            job_id TEXT,
            receipt_id TEXT,
            reason TEXT,
            FOREIGN KEY (token_id) REFERENCES resource_tokens(id)
        )",
        [],
    )?;
    
    // Create indices for frequently queried fields
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_token_burns_owner_did ON token_burns(owner_did)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_token_burns_token_type ON token_burns(token_type)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_token_burns_federation_scope ON token_burns(federation_scope)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_token_burns_job_id ON token_burns(job_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_token_burns_receipt_id ON token_burns(receipt_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_token_burns_timestamp ON token_burns(timestamp DESC)",
        [],
    )?;

    Ok(())
}

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    create_tokens_table(conn)?;
    create_resource_tokens_table(conn)?;
    create_token_burns_table(conn)?;
    Ok(())
} 