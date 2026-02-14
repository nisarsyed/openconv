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
