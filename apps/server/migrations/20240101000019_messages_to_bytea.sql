-- Convert encrypted_content from TEXT to BYTEA.
-- USING decode(..., 'base64') converts any existing base64-encoded text to raw bytes.
ALTER TABLE messages
    ALTER COLUMN encrypted_content TYPE BYTEA USING decode(encrypted_content, 'base64');

-- Convert nonce from TEXT to BYTEA.
ALTER TABLE messages
    ALTER COLUMN nonce TYPE BYTEA USING decode(nonce, 'base64');

-- Prevent TOAST compression on encrypted (incompressible) data.
ALTER TABLE messages ALTER COLUMN encrypted_content SET STORAGE EXTERNAL;
ALTER TABLE messages ALTER COLUMN nonce SET STORAGE EXTERNAL;
