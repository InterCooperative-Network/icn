-- Initial schema for AgoraNet
-- Timestamp: 2024-01-01 00:00:00

-- Create threads table
CREATE TABLE IF NOT EXISTS threads (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    proposal_cid TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create messages table
CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    author_did TEXT NOT NULL,
    content TEXT NOT NULL,
    reply_to UUID REFERENCES messages(id) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create verified_credentials table
CREATE TABLE IF NOT EXISTS verified_credentials (
    id UUID PRIMARY KEY,
    holder_did TEXT NOT NULL,
    issuer_did TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    credential_cid TEXT NOT NULL,
    issuance_date TIMESTAMP WITH TIME ZONE NOT NULL,
    expiration_date TIMESTAMP WITH TIME ZONE,
    revocation_status BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create credential_links table
CREATE TABLE IF NOT EXISTS credential_links (
    id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    credential_id UUID NOT NULL REFERENCES verified_credentials(id) ON DELETE CASCADE,
    linked_by TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (thread_id, credential_id)
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
CREATE INDEX IF NOT EXISTS idx_messages_thread_id ON messages(thread_id);
CREATE INDEX IF NOT EXISTS idx_messages_reply_to ON messages(reply_to);
CREATE INDEX IF NOT EXISTS idx_verified_credentials_holder_did ON verified_credentials(holder_did);
CREATE INDEX IF NOT EXISTS idx_verified_credentials_issuer_did ON verified_credentials(issuer_did);
CREATE INDEX IF NOT EXISTS idx_verified_credentials_credential_type ON verified_credentials(credential_type);
CREATE INDEX IF NOT EXISTS idx_credential_links_thread_id ON credential_links(thread_id);
CREATE INDEX IF NOT EXISTS idx_credential_links_credential_id ON credential_links(credential_id);
CREATE INDEX IF NOT EXISTS idx_reactions_message_id ON reactions(message_id); 