-- Allow channel_events to record DM messages.
--
-- The table previously required a guild channel_id, so DM messages could not be
-- logged for sync at all. Mirror the XOR shape already used by `messages`:
-- exactly one of channel_id / dm_channel_id is set.

ALTER TABLE channel_events ALTER COLUMN channel_id DROP NOT NULL;

ALTER TABLE channel_events ADD COLUMN dm_channel_id UUID
    REFERENCES dm_channels(id) ON DELETE CASCADE;

ALTER TABLE channel_events ADD CONSTRAINT chk_channel_events_channel_xor
    CHECK (
        (channel_id IS NOT NULL AND dm_channel_id IS NULL) OR
        (channel_id IS NULL AND dm_channel_id IS NOT NULL)
    );

CREATE INDEX idx_channel_events_dm_channel_seq
    ON channel_events (dm_channel_id, sequence)
    WHERE dm_channel_id IS NOT NULL;
