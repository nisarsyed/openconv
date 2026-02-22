ALTER TABLE guilds ADD COLUMN deleted_at TIMESTAMPTZ;

-- Partial index: efficiently query active guilds by owner
CREATE INDEX idx_guilds_owner_active ON guilds (owner_id) WHERE deleted_at IS NULL;
