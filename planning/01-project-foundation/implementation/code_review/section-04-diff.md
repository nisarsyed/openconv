diff --git a/Cargo.lock b/Cargo.lock
index 9d944e4..62e7346 100644
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -2457,6 +2457,7 @@ name = "openconv-server"
 version = "0.1.0"
 dependencies = [
  "axum",
+ "chrono",
  "dotenvy",
  "openconv-shared",
  "serde",
diff --git a/apps/server/Cargo.toml b/apps/server/Cargo.toml
index 33477e1..fa2ce05 100644
--- a/apps/server/Cargo.toml
+++ b/apps/server/Cargo.toml
@@ -22,3 +22,6 @@ tower-http = { workspace = true }
 dotenvy = { workspace = true }
 toml = { workspace = true }
 thiserror = { workspace = true }
+
+[dev-dependencies]
+chrono = { workspace = true }
diff --git a/apps/server/migrations/.gitkeep b/apps/server/migrations/.gitkeep
deleted file mode 100644
index e69de29..0000000
diff --git a/apps/server/migrations/20240101000001_users.sql b/apps/server/migrations/20240101000001_users.sql
new file mode 100644
index 0000000..51d532a
--- /dev/null
+++ b/apps/server/migrations/20240101000001_users.sql
@@ -0,0 +1,25 @@
+-- Reusable trigger function: sets updated_at = NOW() on row update.
+CREATE OR REPLACE FUNCTION set_updated_at()
+RETURNS TRIGGER AS $$
+BEGIN
+    NEW.updated_at = NOW();
+    RETURN NEW;
+END;
+$$ LANGUAGE plpgsql;
+
+-- Users table
+CREATE TABLE users (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    public_key TEXT NOT NULL UNIQUE,
+    email TEXT NOT NULL UNIQUE,
+    display_name TEXT NOT NULL,
+    avatar_url TEXT,
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
+    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
+
+CREATE INDEX idx_users_email ON users (email);
+
+CREATE TRIGGER trigger_users_updated_at
+    BEFORE UPDATE ON users
+    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
diff --git a/apps/server/migrations/20240101000002_pre_key_bundles.sql b/apps/server/migrations/20240101000002_pre_key_bundles.sql
new file mode 100644
index 0000000..447e90e
--- /dev/null
+++ b/apps/server/migrations/20240101000002_pre_key_bundles.sql
@@ -0,0 +1,9 @@
+CREATE TABLE pre_key_bundles (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
+    key_data BYTEA NOT NULL,
+    is_used BOOLEAN NOT NULL DEFAULT false,
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
+
+CREATE INDEX idx_pre_key_bundles_user_id ON pre_key_bundles (user_id);
diff --git a/apps/server/migrations/20240101000003_guilds.sql b/apps/server/migrations/20240101000003_guilds.sql
new file mode 100644
index 0000000..83a2f0f
--- /dev/null
+++ b/apps/server/migrations/20240101000003_guilds.sql
@@ -0,0 +1,14 @@
+CREATE TABLE guilds (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    name TEXT NOT NULL,
+    owner_id UUID NOT NULL REFERENCES users(id),
+    icon_url TEXT,
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
+    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
+
+CREATE INDEX idx_guilds_owner_id ON guilds (owner_id);
+
+CREATE TRIGGER trigger_guilds_updated_at
+    BEFORE UPDATE ON guilds
+    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
diff --git a/apps/server/migrations/20240101000004_channels.sql b/apps/server/migrations/20240101000004_channels.sql
new file mode 100644
index 0000000..06afcb9
--- /dev/null
+++ b/apps/server/migrations/20240101000004_channels.sql
@@ -0,0 +1,17 @@
+CREATE TABLE channels (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
+    name TEXT NOT NULL,
+    channel_type TEXT NOT NULL DEFAULT 'text',
+    position INTEGER NOT NULL DEFAULT 0,
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
+    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
+
+ALTER TABLE channels ADD CONSTRAINT uq_channels_guild_name UNIQUE (guild_id, name);
+
+CREATE INDEX idx_channels_guild_id ON channels (guild_id);
+
+CREATE TRIGGER trigger_channels_updated_at
+    BEFORE UPDATE ON channels
+    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
diff --git a/apps/server/migrations/20240101000005_roles.sql b/apps/server/migrations/20240101000005_roles.sql
new file mode 100644
index 0000000..da60765
--- /dev/null
+++ b/apps/server/migrations/20240101000005_roles.sql
@@ -0,0 +1,8 @@
+CREATE TABLE roles (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
+    name TEXT NOT NULL,
+    permissions BIGINT NOT NULL DEFAULT 0,
+    position INTEGER NOT NULL DEFAULT 0,
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
diff --git a/apps/server/migrations/20240101000006_guild_members.sql b/apps/server/migrations/20240101000006_guild_members.sql
new file mode 100644
index 0000000..4eadb81
--- /dev/null
+++ b/apps/server/migrations/20240101000006_guild_members.sql
@@ -0,0 +1,14 @@
+CREATE TABLE guild_members (
+    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
+    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
+    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
+    PRIMARY KEY (user_id, guild_id)
+);
+
+CREATE TABLE guild_member_roles (
+    user_id UUID NOT NULL,
+    guild_id UUID NOT NULL,
+    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
+    PRIMARY KEY (user_id, guild_id, role_id),
+    FOREIGN KEY (user_id, guild_id) REFERENCES guild_members(user_id, guild_id) ON DELETE CASCADE
+);
diff --git a/apps/server/migrations/20240101000007_dm_channels.sql b/apps/server/migrations/20240101000007_dm_channels.sql
new file mode 100644
index 0000000..b7bccb4
--- /dev/null
+++ b/apps/server/migrations/20240101000007_dm_channels.sql
@@ -0,0 +1,10 @@
+CREATE TABLE dm_channels (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
+
+CREATE TABLE dm_channel_members (
+    dm_channel_id UUID NOT NULL REFERENCES dm_channels(id) ON DELETE CASCADE,
+    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
+    PRIMARY KEY (dm_channel_id, user_id)
+);
diff --git a/apps/server/migrations/20240101000008_messages.sql b/apps/server/migrations/20240101000008_messages.sql
new file mode 100644
index 0000000..93029e1
--- /dev/null
+++ b/apps/server/migrations/20240101000008_messages.sql
@@ -0,0 +1,25 @@
+CREATE TABLE messages (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    channel_id UUID REFERENCES channels(id) ON DELETE CASCADE,
+    dm_channel_id UUID REFERENCES dm_channels(id) ON DELETE CASCADE,
+    sender_id UUID NOT NULL REFERENCES users(id),
+    encrypted_content TEXT NOT NULL,
+    nonce TEXT NOT NULL,
+    edited_at TIMESTAMPTZ,
+    deleted BOOLEAN NOT NULL DEFAULT false,
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
+
+-- Exactly one of channel_id or dm_channel_id must be set.
+ALTER TABLE messages ADD CONSTRAINT chk_messages_channel_xor
+    CHECK (
+        (channel_id IS NOT NULL AND dm_channel_id IS NULL) OR
+        (channel_id IS NULL AND dm_channel_id IS NOT NULL)
+    );
+
+-- Partial indexes for efficient message history queries.
+CREATE INDEX idx_messages_channel_created ON messages (channel_id, created_at)
+    WHERE channel_id IS NOT NULL;
+
+CREATE INDEX idx_messages_dm_channel_created ON messages (dm_channel_id, created_at)
+    WHERE dm_channel_id IS NOT NULL;
diff --git a/apps/server/migrations/20240101000009_files.sql b/apps/server/migrations/20240101000009_files.sql
new file mode 100644
index 0000000..e34a6ed
--- /dev/null
+++ b/apps/server/migrations/20240101000009_files.sql
@@ -0,0 +1,14 @@
+CREATE TABLE files (
+    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
+    uploader_id UUID NOT NULL REFERENCES users(id),
+    message_id UUID REFERENCES messages(id) ON DELETE SET NULL,
+    file_name TEXT NOT NULL,
+    mime_type TEXT NOT NULL,
+    size_bytes BIGINT NOT NULL,
+    storage_path TEXT NOT NULL,
+    encrypted_blob_key TEXT NOT NULL,
+    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
+);
+
+CREATE INDEX idx_files_message_id ON files (message_id)
+    WHERE message_id IS NOT NULL;
diff --git a/apps/server/tests/migrations.rs b/apps/server/tests/migrations.rs
new file mode 100644
index 0000000..756a27a
--- /dev/null
+++ b/apps/server/tests/migrations.rs
@@ -0,0 +1,564 @@
+use sqlx::PgPool;
+
+/// All migrations apply successfully to a fresh database.
+#[sqlx::test]
+async fn all_migrations_apply_successfully(pool: PgPool) {
+    let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
+    assert_eq!(row.0, 1);
+}
+
+/// Users table has expected columns and accepts a valid insert.
+#[sqlx::test]
+async fn users_table_insert_and_select(pool: PgPool) {
+    let id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(id)
+    .bind("pk_test_user_1")
+    .bind("test@example.com")
+    .bind("Test User")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let row: (uuid::Uuid, String, String, String) = sqlx::query_as(
+        "SELECT id, public_key, email, display_name FROM users WHERE id = $1",
+    )
+    .bind(id)
+    .fetch_one(&pool)
+    .await
+    .unwrap();
+
+    assert_eq!(row.0, id);
+    assert_eq!(row.1, "pk_test_user_1");
+    assert_eq!(row.2, "test@example.com");
+    assert_eq!(row.3, "Test User");
+}
+
+/// Users table enforces UNIQUE constraint on public_key.
+#[sqlx::test]
+async fn users_table_unique_public_key(pool: PgPool) {
+    sqlx::query(
+        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
+    )
+    .bind("same_pk")
+    .bind("user1@example.com")
+    .bind("User 1")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let result = sqlx::query(
+        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
+    )
+    .bind("same_pk")
+    .bind("user2@example.com")
+    .bind("User 2")
+    .execute(&pool)
+    .await;
+
+    assert!(result.is_err());
+}
+
+/// Users table enforces UNIQUE constraint on email.
+#[sqlx::test]
+async fn users_table_unique_email(pool: PgPool) {
+    sqlx::query(
+        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
+    )
+    .bind("pk_1")
+    .bind("same@example.com")
+    .bind("User 1")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let result = sqlx::query(
+        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
+    )
+    .bind("pk_2")
+    .bind("same@example.com")
+    .bind("User 2")
+    .execute(&pool)
+    .await;
+
+    assert!(result.is_err());
+}
+
+/// The updated_at trigger automatically updates on user row modification.
+#[sqlx::test]
+async fn users_updated_at_trigger(pool: PgPool) {
+    let id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(id)
+    .bind("pk_trigger_test")
+    .bind("trigger@example.com")
+    .bind("Before")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let before: (chrono::DateTime<chrono::Utc>,) =
+        sqlx::query_as("SELECT updated_at FROM users WHERE id = $1")
+            .bind(id)
+            .fetch_one(&pool)
+            .await
+            .unwrap();
+
+    // Use pg_sleep to ensure clock advances
+    sqlx::query("SELECT pg_sleep(0.1)")
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
+        .bind("After")
+        .bind(id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let after: (chrono::DateTime<chrono::Utc>,) =
+        sqlx::query_as("SELECT updated_at FROM users WHERE id = $1")
+            .bind(id)
+            .fetch_one(&pool)
+            .await
+            .unwrap();
+
+    assert!(after.0 > before.0, "updated_at should advance after UPDATE");
+}
+
+/// Guilds table FK on owner_id rejects a nonexistent user.
+#[sqlx::test]
+async fn guilds_fk_owner_id_rejects_nonexistent_user(pool: PgPool) {
+    let fake_user = uuid::Uuid::new_v4();
+    let result = sqlx::query(
+        "INSERT INTO guilds (name, owner_id) VALUES ($1, $2)",
+    )
+    .bind("Test Guild")
+    .bind(fake_user)
+    .execute(&pool)
+    .await;
+
+    assert!(result.is_err());
+}
+
+/// The updated_at trigger automatically updates on guild row modification.
+#[sqlx::test]
+async fn guilds_updated_at_trigger(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_guild_trigger")
+    .bind("guild_trigger@example.com")
+    .bind("Owner")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let guild_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
+        .bind(guild_id)
+        .bind("Before Name")
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let before: (chrono::DateTime<chrono::Utc>,) =
+        sqlx::query_as("SELECT updated_at FROM guilds WHERE id = $1")
+            .bind(guild_id)
+            .fetch_one(&pool)
+            .await
+            .unwrap();
+
+    sqlx::query("SELECT pg_sleep(0.1)")
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query("UPDATE guilds SET name = $1 WHERE id = $2")
+        .bind("After Name")
+        .bind(guild_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let after: (chrono::DateTime<chrono::Utc>,) =
+        sqlx::query_as("SELECT updated_at FROM guilds WHERE id = $1")
+            .bind(guild_id)
+            .fetch_one(&pool)
+            .await
+            .unwrap();
+
+    assert!(after.0 > before.0, "updated_at should advance after UPDATE");
+}
+
+/// Channels table CASCADE deletes when guild is deleted.
+#[sqlx::test]
+async fn channels_cascade_delete_on_guild_delete(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_cascade_test")
+    .bind("cascade@example.com")
+    .bind("Cascade Tester")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let guild_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
+        .bind(guild_id)
+        .bind("Doomed Guild")
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let channel_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO channels (id, guild_id, name) VALUES ($1, $2, $3)")
+        .bind(channel_id)
+        .bind(guild_id)
+        .bind("general")
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query("DELETE FROM guilds WHERE id = $1")
+        .bind(guild_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let count: (i64,) =
+        sqlx::query_as("SELECT COUNT(*) FROM channels WHERE id = $1")
+            .bind(channel_id)
+            .fetch_one(&pool)
+            .await
+            .unwrap();
+
+    assert_eq!(count.0, 0, "channel should be cascade deleted");
+}
+
+/// Channels table enforces UNIQUE on (guild_id, name).
+#[sqlx::test]
+async fn channels_unique_guild_id_name(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_chan_unique")
+    .bind("chan_unique@example.com")
+    .bind("Chan Unique")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let guild_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
+        .bind(guild_id)
+        .bind("Unique Guild")
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query("INSERT INTO channels (guild_id, name) VALUES ($1, $2)")
+        .bind(guild_id)
+        .bind("general")
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let result = sqlx::query("INSERT INTO channels (guild_id, name) VALUES ($1, $2)")
+        .bind(guild_id)
+        .bind("general")
+        .execute(&pool)
+        .await;
+
+    assert!(result.is_err());
+}
+
+/// Messages table CHECK constraint rejects null channel_id AND null dm_channel_id.
+#[sqlx::test]
+async fn messages_check_rejects_both_null(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_msg_null")
+    .bind("msg_null@example.com")
+    .bind("Msg Null")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let result = sqlx::query(
+        "INSERT INTO messages (sender_id, encrypted_content, nonce) VALUES ($1, $2, $3)",
+    )
+    .bind(user_id)
+    .bind("encrypted")
+    .bind("nonce123")
+    .execute(&pool)
+    .await;
+
+    assert!(result.is_err(), "should reject both channel_id and dm_channel_id being NULL");
+}
+
+/// Messages table CHECK constraint rejects both channel_id AND dm_channel_id set.
+#[sqlx::test]
+async fn messages_check_rejects_both_set(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_msg_both")
+    .bind("msg_both@example.com")
+    .bind("Msg Both")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let guild_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
+        .bind(guild_id)
+        .bind("Both Guild")
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let channel_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO channels (id, guild_id, name) VALUES ($1, $2, $3)")
+        .bind(channel_id)
+        .bind(guild_id)
+        .bind("general")
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let dm_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
+        .bind(dm_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let result = sqlx::query(
+        "INSERT INTO messages (sender_id, channel_id, dm_channel_id, encrypted_content, nonce) VALUES ($1, $2, $3, $4, $5)",
+    )
+    .bind(user_id)
+    .bind(channel_id)
+    .bind(dm_id)
+    .bind("encrypted")
+    .bind("nonce123")
+    .execute(&pool)
+    .await;
+
+    assert!(result.is_err(), "should reject both channel_id and dm_channel_id being set");
+}
+
+/// Messages table accepts channel_id set with dm_channel_id null.
+#[sqlx::test]
+async fn messages_accepts_channel_id_only(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_msg_chan")
+    .bind("msg_chan@example.com")
+    .bind("Msg Chan")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let guild_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
+        .bind(guild_id)
+        .bind("Chan Guild")
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let channel_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO channels (id, guild_id, name) VALUES ($1, $2, $3)")
+        .bind(channel_id)
+        .bind(guild_id)
+        .bind("general")
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query(
+        "INSERT INTO messages (sender_id, channel_id, encrypted_content, nonce) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind(channel_id)
+    .bind("encrypted")
+    .bind("nonce123")
+    .execute(&pool)
+    .await
+    .unwrap();
+}
+
+/// Messages table accepts dm_channel_id set with channel_id null.
+#[sqlx::test]
+async fn messages_accepts_dm_channel_id_only(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_msg_dm")
+    .bind("msg_dm@example.com")
+    .bind("Msg DM")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let dm_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
+        .bind(dm_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
+        .bind(dm_id)
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query(
+        "INSERT INTO messages (sender_id, dm_channel_id, encrypted_content, nonce) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind(dm_id)
+    .bind("encrypted")
+    .bind("nonce123")
+    .execute(&pool)
+    .await
+    .unwrap();
+}
+
+/// Guild members composite PK prevents duplicate membership.
+#[sqlx::test]
+async fn guild_members_no_duplicate_membership(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_gm_dup")
+    .bind("gm_dup@example.com")
+    .bind("GM Dup")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let guild_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
+        .bind(guild_id)
+        .bind("Dup Guild")
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
+        .bind(user_id)
+        .bind(guild_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let result = sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
+        .bind(user_id)
+        .bind(guild_id)
+        .execute(&pool)
+        .await;
+
+    assert!(result.is_err(), "duplicate guild membership should be rejected");
+}
+
+/// DM channel members composite PK prevents duplicate membership.
+#[sqlx::test]
+async fn dm_channel_members_no_duplicate_membership(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_dm_dup")
+    .bind("dm_dup@example.com")
+    .bind("DM Dup")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    let dm_id = uuid::Uuid::new_v4();
+    sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
+        .bind(dm_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
+        .bind(dm_id)
+        .bind(user_id)
+        .execute(&pool)
+        .await
+        .unwrap();
+
+    let result =
+        sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
+            .bind(dm_id)
+            .bind(user_id)
+            .execute(&pool)
+            .await;
+
+    assert!(result.is_err(), "duplicate DM membership should be rejected");
+}
+
+/// Files table allows null message_id.
+#[sqlx::test]
+async fn files_allows_null_message_id(pool: PgPool) {
+    let user_id = uuid::Uuid::new_v4();
+    sqlx::query(
+        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
+    )
+    .bind(user_id)
+    .bind("pk_file_null")
+    .bind("file_null@example.com")
+    .bind("File Null")
+    .execute(&pool)
+    .await
+    .unwrap();
+
+    sqlx::query(
+        "INSERT INTO files (uploader_id, file_name, mime_type, size_bytes, storage_path, encrypted_blob_key) VALUES ($1, $2, $3, $4, $5, $6)",
+    )
+    .bind(user_id)
+    .bind("test.txt")
+    .bind("text/plain")
+    .bind(1024_i64)
+    .bind("/storage/test.txt")
+    .bind("enc_key_123")
+    .execute(&pool)
+    .await
+    .unwrap();
+}
