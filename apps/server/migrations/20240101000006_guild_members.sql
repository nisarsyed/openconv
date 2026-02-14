CREATE TABLE guild_members (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, guild_id)
);

CREATE TABLE guild_member_roles (
    user_id UUID NOT NULL,
    guild_id UUID NOT NULL,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, guild_id, role_id),
    FOREIGN KEY (user_id, guild_id) REFERENCES guild_members(user_id, guild_id) ON DELETE CASCADE
);
