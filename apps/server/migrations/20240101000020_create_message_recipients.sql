CREATE TABLE message_recipients (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    device_id UUID NOT NULL,
    ciphertext BYTEA NOT NULL,
    message_type TEXT NOT NULL,  -- 'prekey' or 'signal'
    PRIMARY KEY (message_id, user_id, device_id)
);

-- Prevent TOAST compression on encrypted (incompressible) data.
ALTER TABLE message_recipients ALTER COLUMN ciphertext SET STORAGE EXTERNAL;

-- Index for looking up a specific user+device's messages efficiently
CREATE INDEX idx_message_recipients_user ON message_recipients (user_id, device_id, message_id);

-- Make encrypted_content and nonce nullable (content now in message_recipients)
ALTER TABLE messages ALTER COLUMN encrypted_content DROP NOT NULL;
ALTER TABLE messages ALTER COLUMN nonce DROP NOT NULL;
