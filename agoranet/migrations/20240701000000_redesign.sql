-- Safe migration that doesn't drop existing data
-- Create backup tables if needed (uncomment for production)
-- SELECT 'CREATE TABLE reactions_backup AS SELECT * FROM reactions' WHERE EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'reactions');
-- SELECT 'CREATE TABLE credential_links_backup AS SELECT * FROM credential_links' WHERE EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'credential_links');
-- SELECT 'CREATE TABLE verified_credentials_backup AS SELECT * FROM verified_credentials' WHERE EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'verified_credentials');
-- SELECT 'CREATE TABLE messages_backup AS SELECT * FROM messages' WHERE EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'messages');
-- SELECT 'CREATE TABLE threads_backup AS SELECT * FROM threads' WHERE EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'threads');

-- Create threads table with new fields
CREATE TABLE IF NOT EXISTS threads (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    creator_did TEXT NOT NULL,
    federation_id TEXT,
    topic_type TEXT NOT NULL DEFAULT 'general',
    proposal_ref TEXT,
    dag_ref TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB
);

-- Create messages table with DAG support
CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    author_did TEXT NOT NULL,
    content TEXT NOT NULL,
    reply_to UUID REFERENCES messages(id) ON DELETE SET NULL,
    signature TEXT,
    dag_ref TEXT,
    dag_anchored BOOLEAN NOT NULL DEFAULT FALSE,
    credential_refs TEXT[],
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB
);

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

-- Create verifiable credentials table
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

-- Create thread credentials table
CREATE TABLE IF NOT EXISTS thread_credentials (
    id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    credential_id UUID NOT NULL REFERENCES credentials(id) ON DELETE CASCADE,
    linked_by TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (thread_id, credential_id)
);

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

-- Create reactions table
CREATE TABLE IF NOT EXISTS reactions (
    id UUID PRIMARY KEY,
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    author_did TEXT NOT NULL,
    reaction_type TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(message_id, author_did, reaction_type)
);

-- Create indices
CREATE INDEX IF NOT EXISTS threads_federation_id_idx ON threads(federation_id);
CREATE INDEX IF NOT EXISTS threads_topic_type_idx ON threads(topic_type);
CREATE INDEX IF NOT EXISTS threads_proposal_ref_idx ON threads(proposal_ref);
CREATE INDEX IF NOT EXISTS messages_thread_id_idx ON messages(thread_id);
CREATE INDEX IF NOT EXISTS messages_reply_to_idx ON messages(reply_to);
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
CREATE INDEX IF NOT EXISTS reactions_message_id_idx ON reactions(message_id); 