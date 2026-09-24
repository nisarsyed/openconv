#!/usr/bin/env bash
#
# Put two already-registered dev users into a shared guild and channel.
#
# Guild and channel creation are not wired to the backend yet — the client's
# `createGuild` only mutates local store state — so there is no way to set this
# up through the UI. This seeds it directly so the messaging pipeline can be
# exercised end to end.
#
# Usage: scripts/seed-dev-guild.sh alice@test.local bob@test.local
#
# Safe to re-run: the guild is looked up by name and reused.

set -euo pipefail

EMAIL_A="${1:?usage: $0 <email-a> <email-b>}"
EMAIL_B="${2:?usage: $0 <email-a> <email-b>}"
GUILD_NAME="${GUILD_NAME:-Dev Guild}"
CHANNEL_NAME="${CHANNEL_NAME:-general}"
CONTAINER="${CONTAINER:-openconv-postgres}"

psql() { docker exec -i "$CONTAINER" psql -U openconv -d openconv -v ON_ERROR_STOP=1 "$@"; }

psql -q <<SQL
DO \$\$
DECLARE
    uid_a  UUID;
    uid_b  UUID;
    gid    UUID;
    rid    UUID;
    cid    UUID;
BEGIN
    SELECT id INTO uid_a FROM users WHERE email = '${EMAIL_A}';
    SELECT id INTO uid_b FROM users WHERE email = '${EMAIL_B}';
    IF uid_a IS NULL THEN RAISE EXCEPTION 'no user with email ${EMAIL_A} — register first'; END IF;
    IF uid_b IS NULL THEN RAISE EXCEPTION 'no user with email ${EMAIL_B} — register first'; END IF;

    SELECT id INTO gid FROM guilds WHERE name = '${GUILD_NAME}' AND owner_id = uid_a;
    IF gid IS NULL THEN
        INSERT INTO guilds (name, owner_id) VALUES ('${GUILD_NAME}', uid_a) RETURNING id INTO gid;
    END IF;

    -- ADMINISTRATOR (bit 0) short-circuits permission resolution to "all", which
    -- is what a member needs to pass the READ_MESSAGES / SEND_MESSAGES checks.
    -- A member with no role at all resolves to Forbidden.
    SELECT id INTO rid FROM roles WHERE guild_id = gid AND name = '@everyone';
    IF rid IS NULL THEN
        INSERT INTO roles (guild_id, name, permissions, position, role_type)
        VALUES (gid, '@everyone', 1, 0, 'everyone') RETURNING id INTO rid;
    END IF;

    INSERT INTO guild_members (user_id, guild_id) VALUES (uid_a, gid), (uid_b, gid)
        ON CONFLICT DO NOTHING;
    INSERT INTO guild_member_roles (user_id, guild_id, role_id)
        VALUES (uid_a, gid, rid), (uid_b, gid, rid)
        ON CONFLICT DO NOTHING;

    SELECT id INTO cid FROM channels WHERE guild_id = gid AND name = '${CHANNEL_NAME}';
    IF cid IS NULL THEN
        INSERT INTO channels (guild_id, name, channel_type, position)
        VALUES (gid, '${CHANNEL_NAME}', 'text', 0) RETURNING id INTO cid;
    END IF;

    RAISE NOTICE 'guild=% channel=% members=%, %', gid, cid, uid_a, uid_b;
END
\$\$;
SQL

echo
echo "Seeded. Guild/channel:"
psql -tAc "SELECT g.name || ' / #' || c.name || '  (channel_id=' || c.id || ')'
           FROM guilds g JOIN channels c ON c.guild_id = g.id
           WHERE g.name = '${GUILD_NAME}';"
echo "Members:"
psql -tAc "SELECT '  ' || u.email || '  perms=' || r.permissions
           FROM guild_members gm
           JOIN users u ON u.id = gm.user_id
           JOIN guilds g ON g.id = gm.guild_id
           JOIN guild_member_roles gmr ON gmr.user_id = gm.user_id AND gmr.guild_id = gm.guild_id
           JOIN roles r ON r.id = gmr.role_id
           WHERE g.name = '${GUILD_NAME}' ORDER BY u.email;"
echo
echo "Now restart both app windows (or toggle connection) so ready_data picks it up."
