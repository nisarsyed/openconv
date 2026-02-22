CREATE TABLE channel_events (
    sequence BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    channel_id UUID NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,  -- 'message_created', 'message_updated', 'message_deleted'
    message_id UUID NOT NULL REFERENCES messages(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_channel_events_channel_seq ON channel_events (channel_id, sequence);
CREATE INDEX idx_channel_events_seq ON channel_events (sequence);
