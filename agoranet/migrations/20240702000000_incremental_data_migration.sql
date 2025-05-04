-- Incremental data migration
-- Timestamp: 2024-07-02 00:00:00

-- Migrate data from verified_credentials to new credentials table
INSERT INTO credentials (
    id, 
    holder_did, 
    issuer_did, 
    credential_type, 
    credential_hash, 
    content, 
    valid_from, 
    valid_until, 
    revoked, 
    created_at
)
SELECT 
    id,
    holder_did,
    issuer_did,
    credential_type,
    credential_cid AS credential_hash,
    jsonb_build_object(
        'id', id,
        'type', credential_type,
        'issuer', issuer_did,
        'holder', holder_did,
        'cid', credential_cid
    ) AS content,
    issuance_date AS valid_from,
    expiration_date AS valid_until,
    revocation_status AS revoked,
    created_at
FROM verified_credentials
ON CONFLICT (id) DO NOTHING;

-- Migrate data from credential_links to thread_credentials
INSERT INTO thread_credentials (
    id,
    thread_id,
    credential_id,
    linked_by,
    created_at
)
SELECT 
    id,
    thread_id,
    credential_id,
    linked_by,
    created_at
FROM credential_links
ON CONFLICT (thread_id, credential_id) DO NOTHING;

-- Add signature column to thread table
ALTER TABLE threads ADD COLUMN IF NOT EXISTS signature_cid TEXT;

-- Update threads table for proposal DAG anchoring
UPDATE threads
SET dag_ref = proposal_ref
WHERE dag_ref IS NULL AND proposal_ref IS NOT NULL;

-- Signature verification functions
CREATE OR REPLACE FUNCTION verify_signature(
    message TEXT,
    signature TEXT,
    public_key TEXT
) RETURNS BOOLEAN AS $$
BEGIN
    -- This is a placeholder - actual implementation would use crypto functions
    -- based on the signature type (ed25519, secp256k1, etc.)
    RETURN TRUE;
END;
$$ LANGUAGE plpgsql; 