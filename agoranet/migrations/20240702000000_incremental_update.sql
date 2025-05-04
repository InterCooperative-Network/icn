-- INCREMENTAL MIGRATION: Bridge between 20240101000000_init.sql and 20240701000000_redesign.sql
-- This migration adds new tables and modifies existing ones incrementally rather than recreating them

-- Backup existing tables
CREATE TABLE IF NOT EXISTS threads_backup AS SELECT * FROM threads;
CREATE TABLE IF NOT EXISTS messages_backup AS SELECT * FROM messages;
CREATE TABLE IF NOT EXISTS reactions_backup AS SELECT * FROM reactions;
CREATE TABLE IF NOT EXISTS credential_links_backup AS SELECT * FROM credential_links;
CREATE TABLE IF NOT EXISTS verified_credentials_backup AS SELECT * FROM verified_credentials;

-- 1. MODIFY THREADS TABLE: Add new columns to existing table
ALTER TABLE threads 
  ADD COLUMN IF NOT EXISTS creator_did TEXT,
  ADD COLUMN IF NOT EXISTS federation_id TEXT,
  ADD COLUMN IF NOT EXISTS topic_type TEXT DEFAULT 'general',
  ADD COLUMN IF NOT EXISTS proposal_ref TEXT,
  ADD COLUMN IF NOT EXISTS dag_ref TEXT,
  ADD COLUMN IF NOT EXISTS metadata JSONB;

-- Set default creator_did for old threads
UPDATE threads SET creator_did = 'system' WHERE creator_did IS NULL;
-- Make creator_did non-nullable now that we've filled it
ALTER TABLE threads ALTER COLUMN creator_did SET NOT NULL;

-- 2. MODIFY MESSAGES TABLE: Add new columns to existing table
ALTER TABLE threads ALTER COLUMN id TYPE UUID USING id::uuid;

ALTER TABLE messages 
  ALTER COLUMN id TYPE UUID USING id::uuid,
  ALTER COLUMN thread_id TYPE UUID USING thread_id::uuid,
  ALTER COLUMN reply_to TYPE UUID USING reply_to::uuid,
  ADD COLUMN IF NOT EXISTS dag_ref TEXT,
  ADD COLUMN IF NOT EXISTS dag_anchored BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN IF NOT EXISTS credential_refs TEXT[],
  ADD COLUMN IF NOT EXISTS signature TEXT,
  ADD COLUMN IF NOT EXISTS metadata JSONB;

-- Set default author_did for old messages with NULL author
UPDATE messages SET author_did = 'system' WHERE author_did IS NULL;
-- Make author_did non-nullable now that we've filled it
ALTER TABLE messages ALTER COLUMN author_did SET NOT NULL;

-- 3. MODIFY REACTIONS TABLE: Update UUID type
ALTER TABLE reactions
  ALTER COLUMN id TYPE UUID USING id::uuid,
  ALTER COLUMN message_id TYPE UUID USING message_id::uuid;

-- 4. CREATE NEW TABLES FROM REDESIGN
-- Create DAG nodes table
CREATE TABLE IF NOT EXISTS dag_nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    signature TEXT NOT NULL,
    signer_did TEXT NOT NULL,
    parent_refs TEXT[],
    content TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB
);

-- Create federation access table
CREATE TABLE IF NOT EXISTS federation_access (
    id UUID PRIMARY KEY,
    federation_id TEXT NOT NULL,
    participant_did TEXT NOT NULL,
    access_level TEXT NOT NULL DEFAULT 'read',
    granted_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    granted_by TEXT NOT NULL,
    metadata JSONB,
    UNIQUE (federation_id, participant_did)
);

-- Create verifiable credentials table (replace verified_credentials)
CREATE TABLE IF NOT EXISTS credentials (
    id UUID PRIMARY KEY,
    holder_did TEXT NOT NULL,
    issuer_did TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    credential_hash TEXT NOT NULL,
    content JSONB NOT NULL,
    valid_from TIMESTAMP WITH TIME ZONE NOT NULL,
    valid_until TIMESTAMP WITH TIME ZONE,
    revoked BOOLEAN NOT NULL DEFAULT FALSE,
    dag_ref TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB
);

-- Migrate data from verified_credentials to credentials
INSERT INTO credentials (
    id, 
    holder_did, 
    issuer_did, 
    credential_type, 
    credential_hash, 
    content, 
    valid_from, 
    revoked
)
SELECT 
    id::uuid, 
    holder_did, 
    issuer_did, 
    credential_type, 
    credential_cid as credential_hash, 
    jsonb_build_object('cid', credential_cid, 'type', credential_type) as content, 
    verified_at as valid_from, 
    NOT is_valid as revoked
FROM verified_credentials
ON CONFLICT (id) DO NOTHING;

-- Create thread credentials table (replace credential_links)
CREATE TABLE IF NOT EXISTS thread_credentials (
    id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    credential_id UUID NOT NULL REFERENCES credentials(id) ON DELETE CASCADE,
    linked_by TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (thread_id, credential_id)
);

-- Migrate data from credential_links to thread_credentials
-- Note: This requires credentials to exist first, so requires a join
INSERT INTO thread_credentials (
    id,
    thread_id,
    credential_id,
    linked_by,
    created_at
)
SELECT 
    cl.id::uuid, 
    cl.thread_id::uuid, 
    c.id as credential_id, 
    cl.linked_by, 
    cl.created_at
FROM credential_links cl
JOIN credentials c ON cl.credential_cid = c.credential_hash
ON CONFLICT (thread_id, credential_id) DO NOTHING;

-- Create economic intent table for budget proposals
CREATE TABLE IF NOT EXISTS economic_intents (
    id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    creator_did TEXT NOT NULL,
    intent_type TEXT NOT NULL,
    amount NUMERIC NOT NULL,
    token_id TEXT NOT NULL,
    proposal_ref TEXT,
    status TEXT NOT NULL DEFAULT 'open',
    dag_ref TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB
);

-- Create indices for new and modified tables
CREATE INDEX IF NOT EXISTS threads_federation_id_idx ON threads(federation_id);
CREATE INDEX IF NOT EXISTS threads_topic_type_idx ON threads(topic_type);
CREATE INDEX IF NOT EXISTS threads_proposal_ref_idx ON threads(proposal_ref);
CREATE INDEX IF NOT EXISTS messages_author_did_idx ON messages(author_did);
CREATE INDEX IF NOT EXISTS dag_nodes_node_type_idx ON dag_nodes(node_type);
CREATE INDEX IF NOT EXISTS dag_nodes_signer_did_idx ON dag_nodes(signer_did);
CREATE INDEX IF NOT EXISTS federation_access_federation_id_idx ON federation_access(federation_id);
CREATE INDEX IF NOT EXISTS federation_access_participant_did_idx ON federation_access(participant_did);
CREATE INDEX IF NOT EXISTS credentials_holder_did_idx ON credentials(holder_did);
CREATE INDEX IF NOT EXISTS credentials_issuer_did_idx ON credentials(issuer_did);
CREATE INDEX IF NOT EXISTS credentials_credential_type_idx ON credentials(credential_type);
CREATE INDEX IF NOT EXISTS thread_credentials_thread_id_idx ON thread_credentials(thread_id);
CREATE INDEX IF NOT EXISTS thread_credentials_credential_id_idx ON thread_credentials(credential_id);
CREATE INDEX IF NOT EXISTS economic_intents_thread_id_idx ON economic_intents(thread_id);
CREATE INDEX IF NOT EXISTS economic_intents_creator_did_idx ON economic_intents(creator_did);
CREATE INDEX IF NOT EXISTS economic_intents_token_id_idx ON economic_intents(token_id);
CREATE INDEX IF NOT EXISTS economic_intents_proposal_ref_idx ON economic_intents(proposal_ref); 