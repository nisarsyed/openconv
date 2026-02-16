//! PreKeyStore trait implementation for CryptoStore.

use async_trait::async_trait;
use libsignal_protocol::{PreKeyId, PreKeyRecord, PreKeyStore, SignalProtocolError};

use crate::storage::CryptoStore;

#[async_trait(?Send)]
impl PreKeyStore for CryptoStore<'_> {
    async fn get_pre_key(&self, prekey_id: PreKeyId) -> Result<PreKeyRecord, SignalProtocolError> {
        let id: u32 = prekey_id.into();
        let record_bytes: Vec<u8> = self
            .conn
            .query_row(
                "SELECT record FROM crypto_pre_keys WHERE key_id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| SignalProtocolError::InvalidPreKeyId)?;

        PreKeyRecord::deserialize(&record_bytes)
    }

    async fn save_pre_key(
        &mut self,
        prekey_id: PreKeyId,
        record: &PreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        let id: u32 = prekey_id.into();
        let record_bytes = record.serialize()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                SignalProtocolError::InvalidState(
                    "save_pre_key",
                    "system clock before epoch".into(),
                )
            })?
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO crypto_pre_keys (key_id, record, uploaded, created_at) VALUES (?1, ?2, 0, ?3)",
                rusqlite::params![id, record_bytes, now],
            )
            .map_err(|e| SignalProtocolError::InvalidState("save_pre_key", e.to_string()))?;

        Ok(())
    }

    async fn remove_pre_key(&mut self, prekey_id: PreKeyId) -> Result<(), SignalProtocolError> {
        let id: u32 = prekey_id.into();
        self.conn
            .execute("DELETE FROM crypto_pre_keys WHERE key_id = ?1", [id])
            .map_err(|e| SignalProtocolError::InvalidState("remove_pre_key", e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_test_db;
    use crate::storage::CryptoStore;
    use libsignal_protocol::{KeyPair, PreKeyRecord};

    #[test]
    fn save_then_get_pre_key_round_trips() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let key_pair = KeyPair::generate(&mut rand::rng());
        let record = PreKeyRecord::new(PreKeyId::from(1), &key_pair);

        futures::executor::block_on(store.save_pre_key(PreKeyId::from(1), &record)).unwrap();
        let loaded = futures::executor::block_on(store.get_pre_key(PreKeyId::from(1))).unwrap();

        assert_eq!(loaded.serialize().unwrap(), record.serialize().unwrap());
    }

    #[test]
    fn get_pre_key_returns_error_for_nonexistent_id() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let result = futures::executor::block_on(store.get_pre_key(PreKeyId::from(99999)));
        assert!(result.is_err());
    }

    #[test]
    fn remove_pre_key_deletes_the_key() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let key_pair = KeyPair::generate(&mut rand::rng());
        let record = PreKeyRecord::new(PreKeyId::from(1), &key_pair);

        futures::executor::block_on(store.save_pre_key(PreKeyId::from(1), &record)).unwrap();
        futures::executor::block_on(store.remove_pre_key(PreKeyId::from(1))).unwrap();

        let result = futures::executor::block_on(store.get_pre_key(PreKeyId::from(1)));
        assert!(result.is_err());
    }

    #[test]
    fn remove_pre_key_for_nonexistent_id_does_not_error() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let result = futures::executor::block_on(store.remove_pre_key(PreKeyId::from(99999)));
        assert!(result.is_ok());
    }
}
