use rusqlite::{Connection, Result};

fn configure_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
}

pub fn init_db(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure_connection(&conn)?;
    Ok(conn)
}

pub fn init_db_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure_connection(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_db_in_memory() {
        let conn = init_db_in_memory().expect("should create in-memory db");
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("should query journal_mode");
        // In-memory databases use "memory" journal mode regardless of WAL setting
        assert!(
            mode == "wal" || mode == "memory",
            "unexpected journal_mode: {mode}"
        );
    }

    #[test]
    fn test_init_db_connection_is_functional() {
        let conn = init_db_in_memory().expect("should create in-memory db");
        let result: i64 = conn
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("should execute query");
        assert_eq!(result, 1);

        // Verify foreign keys are enabled
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("should query foreign_keys");
        assert_eq!(fk, 1, "foreign_keys should be enabled");
    }
}
