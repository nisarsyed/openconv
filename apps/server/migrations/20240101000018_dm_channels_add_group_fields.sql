ALTER TABLE dm_channels ADD COLUMN name TEXT;
ALTER TABLE dm_channels ADD COLUMN creator_id UUID REFERENCES users(id);
ALTER TABLE dm_channels ADD COLUMN is_group BOOLEAN NOT NULL DEFAULT false;
