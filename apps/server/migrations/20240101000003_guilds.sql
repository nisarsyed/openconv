CREATE TABLE guilds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id),
    icon_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_guilds_owner_id ON guilds (owner_id);

CREATE TRIGGER trigger_guilds_updated_at
    BEFORE UPDATE ON guilds
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
