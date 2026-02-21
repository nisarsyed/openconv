use sqlx::PgPool;

/// Helper to extract the PostgreSQL error code from a sqlx::Error.
fn pg_error_code(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db_err) => db_err.code().map(|c| c.to_string()),
        _ => None,
    }
}

const PG_UNIQUE_VIOLATION: &str = "23505";
const PG_FK_VIOLATION: &str = "23503";
const PG_CHECK_VIOLATION: &str = "23514";

/// All migrations apply successfully to a fresh database.
#[sqlx::test]
async fn all_migrations_apply_successfully(pool: PgPool) {
    let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, 1);
}

/// Users table has expected columns and accepts a valid insert.
#[sqlx::test]
async fn users_table_insert_and_select(pool: PgPool) {
    let id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind("pk_test_user_1")
        .bind("test@example.com")
        .bind("Test User")
        .execute(&pool)
        .await
        .unwrap();

    let row: (uuid::Uuid, String, String, String) =
        sqlx::query_as("SELECT id, public_key, email, display_name FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(row.0, id);
    assert_eq!(row.1, "pk_test_user_1");
    assert_eq!(row.2, "test@example.com");
    assert_eq!(row.3, "Test User");
}

/// Users table enforces UNIQUE constraint on public_key.
#[sqlx::test]
async fn users_table_unique_public_key(pool: PgPool) {
    sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
        .bind("same_pk")
        .bind("user1@example.com")
        .bind("User 1")
        .execute(&pool)
        .await
        .unwrap();

    let err =
        sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
            .bind("same_pk")
            .bind("user2@example.com")
            .bind("User 2")
            .execute(&pool)
            .await
            .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
}

/// Users table enforces UNIQUE constraint on email.
#[sqlx::test]
async fn users_table_unique_email(pool: PgPool) {
    sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
        .bind("pk_1")
        .bind("same@example.com")
        .bind("User 1")
        .execute(&pool)
        .await
        .unwrap();

    let err =
        sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
            .bind("pk_2")
            .bind("same@example.com")
            .bind("User 2")
            .execute(&pool)
            .await
            .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
}

/// The updated_at trigger automatically updates on user row modification.
#[sqlx::test]
async fn users_updated_at_trigger(pool: PgPool) {
    let id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind("pk_trigger_test")
        .bind("trigger@example.com")
        .bind("Before")
        .execute(&pool)
        .await
        .unwrap();

    let before: (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT updated_at FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Use pg_sleep to ensure clock advances
    sqlx::query("SELECT pg_sleep(0.1)")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
        .bind("After")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let after: (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT updated_at FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(after.0 > before.0, "updated_at should advance after UPDATE");
}

/// Guilds table FK on owner_id rejects a nonexistent user.
#[sqlx::test]
async fn guilds_fk_owner_id_rejects_nonexistent_user(pool: PgPool) {
    let fake_user = uuid::Uuid::new_v4();
    let err = sqlx::query("INSERT INTO guilds (name, owner_id) VALUES ($1, $2)")
        .bind("Test Guild")
        .bind(fake_user)
        .execute(&pool)
        .await
        .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_FK_VIOLATION));
}

/// The updated_at trigger automatically updates on guild row modification.
#[sqlx::test]
async fn guilds_updated_at_trigger(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_guild_trigger")
        .bind("guild_trigger@example.com")
        .bind("Owner")
        .execute(&pool)
        .await
        .unwrap();

    let guild_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(guild_id)
        .bind("Before Name")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let before: (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT updated_at FROM guilds WHERE id = $1")
            .bind(guild_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    sqlx::query("SELECT pg_sleep(0.1)")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("UPDATE guilds SET name = $1 WHERE id = $2")
        .bind("After Name")
        .bind(guild_id)
        .execute(&pool)
        .await
        .unwrap();

    let after: (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT updated_at FROM guilds WHERE id = $1")
            .bind(guild_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(after.0 > before.0, "updated_at should advance after UPDATE");
}

/// Channels table CASCADE deletes when guild is deleted.
#[sqlx::test]
async fn channels_cascade_delete_on_guild_delete(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_cascade_test")
        .bind("cascade@example.com")
        .bind("Cascade Tester")
        .execute(&pool)
        .await
        .unwrap();

    let guild_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(guild_id)
        .bind("Doomed Guild")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let channel_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO channels (id, guild_id, name) VALUES ($1, $2, $3)")
        .bind(channel_id)
        .bind(guild_id)
        .bind("general")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("DELETE FROM guilds WHERE id = $1")
        .bind(guild_id)
        .execute(&pool)
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels WHERE id = $1")
        .bind(channel_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count.0, 0, "channel should be cascade deleted");
}

/// Channels table enforces UNIQUE on (guild_id, name).
#[sqlx::test]
async fn channels_unique_guild_id_name(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_chan_unique")
        .bind("chan_unique@example.com")
        .bind("Chan Unique")
        .execute(&pool)
        .await
        .unwrap();

    let guild_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(guild_id)
        .bind("Unique Guild")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO channels (guild_id, name) VALUES ($1, $2)")
        .bind(guild_id)
        .bind("general")
        .execute(&pool)
        .await
        .unwrap();

    let err = sqlx::query("INSERT INTO channels (guild_id, name) VALUES ($1, $2)")
        .bind(guild_id)
        .bind("general")
        .execute(&pool)
        .await
        .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
}

/// Messages table CHECK constraint rejects null channel_id AND null dm_channel_id.
#[sqlx::test]
async fn messages_check_rejects_both_null(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_msg_null")
        .bind("msg_null@example.com")
        .bind("Msg Null")
        .execute(&pool)
        .await
        .unwrap();

    let err = sqlx::query(
        "INSERT INTO messages (sender_id, encrypted_content, nonce) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind("encrypted")
    .bind("nonce123")
    .execute(&pool)
    .await
    .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_CHECK_VIOLATION));
}

/// Messages table CHECK constraint rejects both channel_id AND dm_channel_id set.
#[sqlx::test]
async fn messages_check_rejects_both_set(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_msg_both")
        .bind("msg_both@example.com")
        .bind("Msg Both")
        .execute(&pool)
        .await
        .unwrap();

    let guild_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(guild_id)
        .bind("Both Guild")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let channel_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO channels (id, guild_id, name) VALUES ($1, $2, $3)")
        .bind(channel_id)
        .bind(guild_id)
        .bind("general")
        .execute(&pool)
        .await
        .unwrap();

    let dm_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
        .bind(dm_id)
        .execute(&pool)
        .await
        .unwrap();

    let err = sqlx::query(
        "INSERT INTO messages (sender_id, channel_id, dm_channel_id, encrypted_content, nonce) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(channel_id)
    .bind(dm_id)
    .bind("encrypted")
    .bind("nonce123")
    .execute(&pool)
    .await
    .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_CHECK_VIOLATION));
}

/// Messages table accepts channel_id set with dm_channel_id null.
#[sqlx::test]
async fn messages_accepts_channel_id_only(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_msg_chan")
        .bind("msg_chan@example.com")
        .bind("Msg Chan")
        .execute(&pool)
        .await
        .unwrap();

    let guild_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(guild_id)
        .bind("Chan Guild")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let channel_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO channels (id, guild_id, name) VALUES ($1, $2, $3)")
        .bind(channel_id)
        .bind(guild_id)
        .bind("general")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO messages (sender_id, channel_id, encrypted_content, nonce) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(channel_id)
    .bind("encrypted")
    .bind("nonce123")
    .execute(&pool)
    .await
    .unwrap();
}

/// Messages table accepts dm_channel_id set with channel_id null.
#[sqlx::test]
async fn messages_accepts_dm_channel_id_only(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_msg_dm")
        .bind("msg_dm@example.com")
        .bind("Msg DM")
        .execute(&pool)
        .await
        .unwrap();

    let dm_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
        .bind(dm_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
        .bind(dm_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO messages (sender_id, dm_channel_id, encrypted_content, nonce) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(dm_id)
    .bind("encrypted")
    .bind("nonce123")
    .execute(&pool)
    .await
    .unwrap();
}

/// Guild members composite PK prevents duplicate membership.
#[sqlx::test]
async fn guild_members_no_duplicate_membership(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_gm_dup")
        .bind("gm_dup@example.com")
        .bind("GM Dup")
        .execute(&pool)
        .await
        .unwrap();

    let guild_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(guild_id)
        .bind("Dup Guild")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_id)
        .bind(guild_id)
        .execute(&pool)
        .await
        .unwrap();

    let err = sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_id)
        .bind(guild_id)
        .execute(&pool)
        .await
        .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
}

/// DM channel members composite PK prevents duplicate membership.
#[sqlx::test]
async fn dm_channel_members_no_duplicate_membership(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_dm_dup")
        .bind("dm_dup@example.com")
        .bind("DM Dup")
        .execute(&pool)
        .await
        .unwrap();

    let dm_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
        .bind(dm_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
        .bind(dm_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let err =
        sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
            .bind(dm_id)
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
}

// ─── Section 03: Devices table ───────────────────────────────────────────────

/// Devices table accepts UUID v7 primary key.
#[sqlx::test]
async fn devices_table_accepts_uuid_v7_pk(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_devices_v7")
        .bind("devices_v7@example.com")
        .bind("Devices V7")
        .execute(&pool)
        .await
        .unwrap();

    // UUID v7 (time-sortable) -- simulate with now_v7
    let device_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("MacBook Pro")
        .execute(&pool)
        .await
        .unwrap();

    let row: (uuid::Uuid, String) =
        sqlx::query_as("SELECT id, device_name FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(row.0, device_id);
    assert_eq!(row.1, "MacBook Pro");
}

/// Devices table enforces user_id foreign key.
#[sqlx::test]
async fn devices_fk_rejects_nonexistent_user(pool: PgPool) {
    let fake_user = uuid::Uuid::new_v4();
    let device_id = uuid::Uuid::now_v7();

    let err = sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(fake_user)
        .bind("Ghost Device")
        .execute(&pool)
        .await
        .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_FK_VIOLATION));
}

/// Devices table ON DELETE CASCADE removes devices when user deleted.
#[sqlx::test]
async fn devices_cascade_delete_on_user_removal(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_dev_cascade")
        .bind("dev_cascade@example.com")
        .bind("Dev Cascade")
        .execute(&pool)
        .await
        .unwrap();

    let device_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("iPhone 15")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count.0, 0, "device should be cascade deleted with user");
}

/// Devices table unique constraint on (user_id, id) prevents duplicate device entries.
#[sqlx::test]
async fn devices_unique_constraint_user_id_device_id(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_dev_unique")
        .bind("dev_unique@example.com")
        .bind("Dev Unique")
        .execute(&pool)
        .await
        .unwrap();

    let device_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("Device A")
        .execute(&pool)
        .await
        .unwrap();

    // Same PK should fail (PK violation, which is also a unique violation)
    let err = sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("Device B")
        .execute(&pool)
        .await
        .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
}

// ─── Section 03: Refresh tokens table ────────────────────────────────────────

/// Refresh tokens table enforces user_id and device_id foreign keys.
#[sqlx::test]
async fn refresh_tokens_fk_enforcement(pool: PgPool) {
    // Test 1: Non-existent user_id (both user_id and device_id are fake)
    let err = sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at) VALUES ($1, $2, $3, $4, NOW() + INTERVAL '7 days')",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(uuid::Uuid::new_v4())
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_FK_VIOLATION));

    // Test 2: Valid user_id but non-existent device_id
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_rt_fk_test")
        .bind("rt_fk_test@example.com")
        .bind("RT FK Test")
        .execute(&pool)
        .await
        .unwrap();

    let err = sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at) VALUES ($1, $2, $3, $4, NOW() + INTERVAL '7 days')",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(user_id)
    .bind(uuid::Uuid::now_v7()) // non-existent device
    .bind(uuid::Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_FK_VIOLATION));
}

/// Refresh tokens table ON DELETE CASCADE removes tokens when user deleted.
#[sqlx::test]
async fn refresh_tokens_cascade_delete_on_user_removal(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_rt_cascade")
        .bind("rt_cascade@example.com")
        .bind("RT Cascade")
        .execute(&pool)
        .await
        .unwrap();

    let device_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("Test Device")
        .execute(&pool)
        .await
        .unwrap();

    let jti = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at) VALUES ($1, $2, $3, $4, NOW() + INTERVAL '7 days')",
    )
    .bind(jti)
    .bind(user_id)
    .bind(device_id)
    .bind(uuid::Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM refresh_tokens WHERE jti = $1")
        .bind(jti)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        count.0, 0,
        "refresh token should be cascade deleted with user"
    );
}

/// Refresh tokens family index exists.
#[sqlx::test]
async fn refresh_tokens_family_index_exists(pool: PgPool) {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM pg_indexes WHERE tablename = 'refresh_tokens' AND indexname = 'idx_refresh_tokens_family'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.0, 1, "idx_refresh_tokens_family should exist");
}

// ─── Section 03: Users public_key_changed_at ─────────────────────────────────

/// Users table public_key_changed_at column is nullable and defaults to NULL.
#[sqlx::test]
async fn users_public_key_changed_at_defaults_to_null(pool: PgPool) {
    let id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind("pk_keychange_null")
        .bind("keychange_null@example.com")
        .bind("Key Change Null")
        .execute(&pool)
        .await
        .unwrap();

    let row: (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT public_key_changed_at FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(
        row.0.is_none(),
        "public_key_changed_at should default to NULL"
    );
}

// ─── Section 03: Pre-key bundles device_id ───────────────────────────────────

/// Pre_key_bundles.device_id column is nullable (existing rows survive migration).
#[sqlx::test]
async fn prekey_bundles_device_id_nullable(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_prekey_nullable")
        .bind("prekey_nullable@example.com")
        .bind("Prekey Nullable")
        .execute(&pool)
        .await
        .unwrap();

    // Insert without device_id -- should succeed (NULL device_id)
    sqlx::query("INSERT INTO pre_key_bundles (user_id, key_data) VALUES ($1, $2)")
        .bind(user_id)
        .bind(b"test_key_data" as &[u8])
        .execute(&pool)
        .await
        .unwrap();
}

/// Pre_key_bundles.device_id references devices(id).
#[sqlx::test]
async fn prekey_bundles_device_id_fk_enforcement(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_prekey_fk")
        .bind("prekey_fk@example.com")
        .bind("Prekey FK")
        .execute(&pool)
        .await
        .unwrap();

    let fake_device = uuid::Uuid::now_v7();
    let err = sqlx::query(
        "INSERT INTO pre_key_bundles (user_id, key_data, device_id) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(b"test_key_data" as &[u8])
    .bind(fake_device)
    .execute(&pool)
    .await
    .unwrap_err();

    assert_eq!(pg_error_code(&err).as_deref(), Some(PG_FK_VIOLATION));
}

/// Query fetches bundles where device_id matches OR device_id IS NULL.
#[sqlx::test]
async fn prekey_bundles_query_matches_device_or_null(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_prekey_query")
        .bind("prekey_query@example.com")
        .bind("Prekey Query")
        .execute(&pool)
        .await
        .unwrap();

    let device_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("Query Device")
        .execute(&pool)
        .await
        .unwrap();

    // Bundle with device_id
    sqlx::query("INSERT INTO pre_key_bundles (user_id, key_data, device_id) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(b"device_key" as &[u8])
        .bind(device_id)
        .execute(&pool)
        .await
        .unwrap();

    // Bundle without device_id (legacy)
    sqlx::query("INSERT INTO pre_key_bundles (user_id, key_data) VALUES ($1, $2)")
        .bind(user_id)
        .bind(b"legacy_key" as &[u8])
        .execute(&pool)
        .await
        .unwrap();

    // Query should return both
    let rows: Vec<(Vec<u8>,)> = sqlx::query_as(
        "SELECT key_data FROM pre_key_bundles WHERE user_id = $1 AND (device_id = $2 OR device_id IS NULL) AND is_used = false",
    )
    .bind(user_id)
    .bind(device_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        rows.len(),
        2,
        "should return device-specific and legacy NULL bundles"
    );
}

// ─── Section 03: Refresh token cleanup ───────────────────────────────────────

/// Cleanup task deletes tokens with expires_at < NOW().
#[sqlx::test]
async fn cleanup_deletes_expired_tokens(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_cleanup_expired")
        .bind("cleanup_expired@example.com")
        .bind("Cleanup Expired")
        .execute(&pool)
        .await
        .unwrap();

    let device_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("Cleanup Device")
        .execute(&pool)
        .await
        .unwrap();

    // Insert an expired refresh token (expires_at in the past)
    let jti = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at) VALUES ($1, $2, $3, $4, NOW() - INTERVAL '1 hour')",
    )
    .bind(jti)
    .bind(user_id)
    .bind(device_id)
    .bind(uuid::Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    let count = openconv_server::tasks::cleanup::cleanup_expired_refresh_tokens(&pool)
        .await
        .unwrap();

    assert_eq!(count, 1, "should delete exactly 1 expired token");

    let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM refresh_tokens WHERE jti = $1")
        .bind(jti)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(remaining.0, 0, "expired token should be deleted");
}

/// Cleanup task does not delete non-expired tokens.
#[sqlx::test]
async fn cleanup_preserves_non_expired_tokens(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_cleanup_preserve")
        .bind("cleanup_preserve@example.com")
        .bind("Cleanup Preserve")
        .execute(&pool)
        .await
        .unwrap();

    let device_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO devices (id, user_id, device_name) VALUES ($1, $2, $3)")
        .bind(device_id)
        .bind(user_id)
        .bind("Preserve Device")
        .execute(&pool)
        .await
        .unwrap();

    // Insert a non-expired refresh token (expires_at in the future)
    let jti = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at) VALUES ($1, $2, $3, $4, NOW() + INTERVAL '7 days')",
    )
    .bind(jti)
    .bind(user_id)
    .bind(device_id)
    .bind(uuid::Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    openconv_server::tasks::cleanup::cleanup_expired_refresh_tokens(&pool)
        .await
        .unwrap();

    let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM refresh_tokens WHERE jti = $1")
        .bind(jti)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(remaining.0, 1, "non-expired token should be preserved");
}

/// Files table allows null message_id.
#[sqlx::test]
async fn files_allows_null_message_id(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind("pk_file_null")
        .bind("file_null@example.com")
        .bind("File Null")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO files (uploader_id, file_name, mime_type, size_bytes, storage_path, encrypted_blob_key) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind("test.txt")
    .bind("text/plain")
    .bind(1024_i64)
    .bind("/storage/test.txt")
    .bind("enc_key_123")
    .execute(&pool)
    .await
    .unwrap();
}
