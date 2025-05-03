-- Add creator_did and signature_cid to threads table
ALTER TABLE threads
ADD COLUMN creator_did VARCHAR(128),
ADD COLUMN signature_cid VARCHAR(128); 