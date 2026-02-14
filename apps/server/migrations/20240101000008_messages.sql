CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    channel_id UUID REFERENCES channels(id) ON DELETE CASCADE,
    dm_channel_id UUID REFERENCES dm_channels(id) ON DELETE CASCADE,
    sender_id UUID NOT NULL REFERENCES users(id),
    encrypted_content TEXT NOT NULL,
    nonce TEXT NOT NULL,
    edited_at TIMESTAMPTZ,
    deleted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Exactly one of channel_id or dm_channel_id must be set.
ALTER TABLE messages ADD CONSTRAINT chk_messages_channel_xor
    CHECK (
        (channel_id IS NOT NULL AND dm_channel_id IS NULL) OR
        (channel_id IS NULL AND dm_channel_id IS NOT NULL)
    );

-- Partial indexes for efficient message history queries.
CREATE INDEX idx_messages_channel_created ON messages (channel_id, created_at)
    WHERE channel_id IS NOT NULL;

CREATE INDEX idx_messages_dm_channel_created ON messages (dm_channel_id, created_at)
    WHERE dm_channel_id IS NOT NULL;
