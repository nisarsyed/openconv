ALTER TABLE pre_key_bundles ADD COLUMN device_id UUID REFERENCES devices(id) ON DELETE CASCADE;
