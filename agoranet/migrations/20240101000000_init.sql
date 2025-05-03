-- Create threads table
CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    proposal_cid TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create messages table
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    author_did TEXT,
    content TEXT NOT NULL,
    reply_to TEXT REFERENCES messages(id) ON DELETE SET NULL,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create credential_links table
CREATE TABLE IF NOT EXISTS credential_links (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    credential_cid TEXT NOT NULL,
    linked_by TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create reactions table
CREATE TABLE IF NOT EXISTS reactions (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    author_did TEXT NOT NULL,
    reaction_type TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(message_id, author_did, reaction_type)
);

-- Create verified_credentials table
CREATE TABLE IF NOT EXISTS verified_credentials (
    id TEXT PRIMARY KEY,
    credential_cid TEXT NOT NULL,
    holder_did TEXT NOT NULL,
    issuer_did TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    is_valid BOOLEAN NOT NULL DEFAULT TRUE,
    verified_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(credential_cid)
);

-- Create indices
CREATE INDEX IF NOT EXISTS messages_thread_id_idx ON messages(thread_id);
CREATE INDEX IF NOT EXISTS messages_reply_to_idx ON messages(reply_to);
CREATE INDEX IF NOT EXISTS credential_links_thread_id_idx ON credential_links(thread_id);
CREATE INDEX IF NOT EXISTS reactions_message_id_idx ON reactions(message_id);
CREATE INDEX IF NOT EXISTS verified_credentials_holder_did_idx ON verified_credentials(holder_did);
CREATE INDEX IF NOT EXISTS verified_credentials_credential_type_idx ON verified_credentials(credential_type); 