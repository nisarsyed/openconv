use crate::auth_service::AppError;
use rusqlite::Connection;

pub struct CachedFile {
    pub id: String,
    pub message_id: Option<String>,
    pub file_name: String,
    pub file_size: i64,
    pub mime_type: Option<String>,
    pub local_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub created_at: i64,
}

pub fn insert_file(conn: &Connection, file: &CachedFile) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR REPLACE INTO cached_files (id, message_id, file_name, file_size, mime_type, local_path, thumbnail_path, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            file.id,
            file.message_id,
            file.file_name,
            file.file_size,
            file.mime_type,
            file.local_path,
            file.thumbnail_path,
            file.created_at,
        ],
    )?;
    Ok(())
}

pub fn get_file(conn: &Connection, id: &str) -> Result<Option<CachedFile>, AppError> {
    let result = conn.query_row(
        "SELECT id, message_id, file_name, file_size, mime_type, local_path, thumbnail_path, created_at
         FROM cached_files WHERE id = ?1",
        [id],
        |row| {
            Ok(CachedFile {
                id: row.get(0)?,
                message_id: row.get(1)?,
                file_name: row.get(2)?,
                file_size: row.get(3)?,
                mime_type: row.get(4)?,
                local_path: row.get(5)?,
                thumbnail_path: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    );
    match result {
        Ok(f) => Ok(Some(f)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_files_for_message(
    conn: &Connection,
    message_id: &str,
) -> Result<Vec<CachedFile>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, file_name, file_size, mime_type, local_path, thumbnail_path, created_at
         FROM cached_files WHERE message_id = ?1
         ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([message_id], |row| {
        Ok(CachedFile {
            id: row.get(0)?,
            message_id: row.get(1)?,
            file_name: row.get(2)?,
            file_size: row.get(3)?,
            mime_type: row.get(4)?,
            local_path: row.get(5)?,
            thumbnail_path: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    let mut files = Vec::new();
    for row in rows {
        files.push(row?);
    }
    Ok(files)
}

pub fn update_local_paths(
    conn: &Connection,
    file_id: &str,
    local_path: &str,
    thumbnail_path: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE cached_files SET local_path = ?1, thumbnail_path = ?2 WHERE id = ?3",
        rusqlite::params![local_path, thumbnail_path, file_id],
    )?;
    Ok(())
}

pub fn delete_file(conn: &Connection, file_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM cached_files WHERE id = ?1", [file_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
        )
        .unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::cache::migrations::run_cache_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_insert_and_get_file() {
        let conn = test_conn();
        let file = CachedFile {
            id: "f1".into(),
            message_id: Some("m1".into()),
            file_name: "photo.png".into(),
            file_size: 1024,
            mime_type: Some("image/png".into()),
            local_path: Some("/tmp/photo.png".into()),
            thumbnail_path: Some("/tmp/photo_thumb.jpg".into()),
            created_at: 1000,
        };
        insert_file(&conn, &file).unwrap();

        let retrieved = get_file(&conn, "f1").unwrap().unwrap();
        assert_eq!(retrieved.id, "f1");
        assert_eq!(retrieved.message_id.as_deref(), Some("m1"));
        assert_eq!(retrieved.file_name, "photo.png");
        assert_eq!(retrieved.file_size, 1024);
        assert_eq!(retrieved.mime_type.as_deref(), Some("image/png"));
        assert_eq!(retrieved.local_path.as_deref(), Some("/tmp/photo.png"));
        assert_eq!(
            retrieved.thumbnail_path.as_deref(),
            Some("/tmp/photo_thumb.jpg")
        );
    }

    #[test]
    fn test_get_file_not_found() {
        let conn = test_conn();
        let result = get_file(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_files_for_message() {
        let conn = test_conn();
        insert_file(
            &conn,
            &CachedFile {
                id: "f1".into(),
                message_id: Some("m1".into()),
                file_name: "a.png".into(),
                file_size: 100,
                mime_type: None,
                local_path: None,
                thumbnail_path: None,
                created_at: 1000,
            },
        )
        .unwrap();
        insert_file(
            &conn,
            &CachedFile {
                id: "f2".into(),
                message_id: Some("m1".into()),
                file_name: "b.pdf".into(),
                file_size: 200,
                mime_type: None,
                local_path: None,
                thumbnail_path: None,
                created_at: 1001,
            },
        )
        .unwrap();
        insert_file(
            &conn,
            &CachedFile {
                id: "f3".into(),
                message_id: Some("m2".into()),
                file_name: "c.txt".into(),
                file_size: 50,
                mime_type: None,
                local_path: None,
                thumbnail_path: None,
                created_at: 1002,
            },
        )
        .unwrap();

        let m1_files = get_files_for_message(&conn, "m1").unwrap();
        assert_eq!(m1_files.len(), 2);
        assert_eq!(m1_files[0].id, "f1");
        assert_eq!(m1_files[1].id, "f2");

        let m2_files = get_files_for_message(&conn, "m2").unwrap();
        assert_eq!(m2_files.len(), 1);
    }

    #[test]
    fn test_update_local_paths() {
        let conn = test_conn();
        insert_file(
            &conn,
            &CachedFile {
                id: "f1".into(),
                message_id: None,
                file_name: "photo.png".into(),
                file_size: 1024,
                mime_type: Some("image/png".into()),
                local_path: None,
                thumbnail_path: None,
                created_at: 1000,
            },
        )
        .unwrap();

        update_local_paths(&conn, "f1", "/data/photo.png", Some("/data/thumb.jpg")).unwrap();

        let f = get_file(&conn, "f1").unwrap().unwrap();
        assert_eq!(f.local_path.as_deref(), Some("/data/photo.png"));
        assert_eq!(f.thumbnail_path.as_deref(), Some("/data/thumb.jpg"));
    }

    #[test]
    fn test_delete_file() {
        let conn = test_conn();
        insert_file(
            &conn,
            &CachedFile {
                id: "f1".into(),
                message_id: None,
                file_name: "test.txt".into(),
                file_size: 10,
                mime_type: None,
                local_path: None,
                thumbnail_path: None,
                created_at: 1000,
            },
        )
        .unwrap();

        delete_file(&conn, "f1").unwrap();
        assert!(get_file(&conn, "f1").unwrap().is_none());
    }

    #[test]
    fn test_insert_file_upsert() {
        let conn = test_conn();
        insert_file(
            &conn,
            &CachedFile {
                id: "f1".into(),
                message_id: None,
                file_name: "old.txt".into(),
                file_size: 10,
                mime_type: None,
                local_path: None,
                thumbnail_path: None,
                created_at: 1000,
            },
        )
        .unwrap();

        // Re-insert with same ID should overwrite
        insert_file(
            &conn,
            &CachedFile {
                id: "f1".into(),
                message_id: Some("m1".into()),
                file_name: "new.txt".into(),
                file_size: 20,
                mime_type: Some("text/plain".into()),
                local_path: Some("/data/new.txt".into()),
                thumbnail_path: None,
                created_at: 2000,
            },
        )
        .unwrap();

        let f = get_file(&conn, "f1").unwrap().unwrap();
        assert_eq!(f.file_name, "new.txt");
        assert_eq!(f.file_size, 20);
    }
}
