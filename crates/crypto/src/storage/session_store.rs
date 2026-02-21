//! SessionStore trait implementation for CryptoStore.

use async_trait::async_trait;
use libsignal_protocol::{ProtocolAddress, SessionRecord, SessionStore, SignalProtocolError};

use crate::storage::CryptoStore;

#[async_trait(?Send)]
impl SessionStore for CryptoStore<'_> {
    async fn load_session(
        &self,
        address: &ProtocolAddress,
    ) -> Result<Option<SessionRecord>, SignalProtocolError> {
        let addr_name = address.name();
        let device_id: u32 = address.device_id().into();

        match self.conn.query_row(
            "SELECT session_data FROM crypto_sessions WHERE address = ?1 AND device_id = ?2",
            rusqlite::params![addr_name, device_id],
            |row| row.get::<_, Vec<u8>>(0),
        ) {
            Ok(bytes) => Ok(Some(SessionRecord::deserialize(&bytes)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(SignalProtocolError::InvalidState(
                "load_session",
                e.to_string(),
            )),
        }
    }

    async fn store_session(
        &mut self,
        address: &ProtocolAddress,
        record: &SessionRecord,
    ) -> Result<(), SignalProtocolError> {
        let addr_name = address.name();
        let device_id: u32 = address.device_id().into();
        let session_bytes = record.serialize()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                SignalProtocolError::InvalidState(
                    "store_session",
                    "system clock before epoch".into(),
                )
            })?
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT INTO crypto_sessions (address, device_id, session_data, created_at, last_used_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(address, device_id) DO UPDATE SET
                     session_data = excluded.session_data,
                     last_used_at = excluded.last_used_at",
                rusqlite::params![addr_name, device_id, session_bytes, now],
            )
            .map_err(|e| SignalProtocolError::InvalidState("store_session", e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::init_test_db;
    use crate::storage::CryptoStore;
    use libsignal_protocol::{DeviceId, ProtocolAddress, SessionRecord, SessionStore};

    #[test]
    fn load_session_returns_none_for_unknown_address() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let addr = ProtocolAddress::new("unknown".to_string(), DeviceId::new(1).unwrap());

        let result = futures::executor::block_on(store.load_session(&addr)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn store_then_load_session_round_trips() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());
        let record = SessionRecord::new_fresh();

        futures::executor::block_on(store.store_session(&addr, &record)).unwrap();
        let loaded = futures::executor::block_on(store.load_session(&addr)).unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn store_session_updates_last_used_at() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());
        let record1 = SessionRecord::new_fresh();

        futures::executor::block_on(store.store_session(&addr, &record1)).unwrap();

        let first_used: i64 = conn
            .query_row(
                "SELECT last_used_at FROM crypto_sessions WHERE address = 'user1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // Store again
        let record2 = SessionRecord::new_fresh();
        futures::executor::block_on(store.store_session(&addr, &record2)).unwrap();

        let second_used: i64 = conn
            .query_row(
                "SELECT last_used_at FROM crypto_sessions WHERE address = 'user1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(second_used >= first_used);
    }
}
