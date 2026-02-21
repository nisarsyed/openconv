ALTER TABLE roles ADD COLUMN role_type TEXT NOT NULL DEFAULT 'custom';
ALTER TABLE roles ADD CONSTRAINT uq_roles_guild_position UNIQUE (guild_id, position);
