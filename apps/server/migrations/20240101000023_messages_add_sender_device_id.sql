ALTER TABLE messages ADD COLUMN sender_device_id UUID REFERENCES devices(id);
