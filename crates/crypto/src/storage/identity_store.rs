//! IdentityKeyStore trait implementation for CryptoStore.

use async_trait::async_trait;
use libsignal_protocol::{
    Direction, IdentityChange, IdentityKey, IdentityKeyPair, IdentityKeyStore, ProtocolAddress,
    SignalProtocolError,
};

use crate::storage::CryptoStore;

#[async_trait(?Send)]
impl IdentityKeyStore for CryptoStore<'_> {
    async fn get_identity_key_pair(&self) -> Result<IdentityKeyPair, SignalProtocolError> {
        let (pub_bytes, priv_bytes): (Vec<u8>, Vec<u8>) = self
            .conn
            .query_row(
                "SELECT public_key, private_key FROM crypto_identity_keys WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| {
                SignalProtocolError::InvalidState(
                    "get_identity_key_pair",
                    "no identity key stored".into(),
                )
            })?;

        let public = IdentityKey::decode(&pub_bytes)?;
        let private = libsignal_protocol::PrivateKey::deserialize(&priv_bytes)?;
        Ok(IdentityKeyPair::new(public, private))
    }

    async fn get_local_registration_id(&self) -> Result<u32, SignalProtocolError> {
        let value = self
            .get_config("registration_id")
            .map_err(|e| {
                SignalProtocolError::InvalidState("get_local_registration_id", e.to_string())
            })?
            .ok_or_else(|| {
                SignalProtocolError::InvalidState(
                    "get_local_registration_id",
                    "registration_id not found in config".into(),
                )
            })?;

        if value.len() != 4 {
            return Err(SignalProtocolError::InvalidState(
                "get_local_registration_id",
                "invalid registration_id length".into(),
            ));
        }
        Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
    }

    async fn save_identity(
        &mut self,
        address: &ProtocolAddress,
        identity: &IdentityKey,
    ) -> Result<IdentityChange, SignalProtocolError> {
        let addr_name = address.name();
        let device_id: u32 = address.device_id().into();
        let key_bytes = identity.serialize();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                SignalProtocolError::InvalidState(
                    "save_identity",
                    "system clock before epoch".into(),
                )
            })?
            .as_secs() as i64;

        // Check if there's an existing key for this address
        let existing: Option<Vec<u8>> = match self.conn.query_row(
            "SELECT identity_key FROM crypto_trusted_identities WHERE address = ?1 AND device_id = ?2",
            rusqlite::params![addr_name, device_id],
            |row| row.get(0),
        ) {
            Ok(val) => Some(val),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(SignalProtocolError::InvalidState("save_identity", e.to_string())),
        };

        let changed = existing
            .as_ref()
            .map(|existing_key| existing_key.as_slice() != key_bytes.as_ref())
            .unwrap_or(false);

        self.conn
            .execute(
                "INSERT INTO crypto_trusted_identities (address, device_id, identity_key, first_seen_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(address, device_id) DO UPDATE SET identity_key = excluded.identity_key",
                rusqlite::params![addr_name, device_id, key_bytes.as_ref(), now],
            )
            .map_err(|e| SignalProtocolError::InvalidState("save_identity", e.to_string()))?;

        Ok(IdentityChange::from_changed(changed))
    }

    async fn is_trusted_identity(
        &self,
        address: &ProtocolAddress,
        identity: &IdentityKey,
        _direction: Direction,
    ) -> Result<bool, SignalProtocolError> {
        let addr_name = address.name();
        let device_id: u32 = address.device_id().into();

        match self.conn.query_row(
            "SELECT identity_key FROM crypto_trusted_identities WHERE address = ?1 AND device_id = ?2",
            rusqlite::params![addr_name, device_id],
            |row| row.get::<_, Vec<u8>>(0),
        ) {
            Ok(stored_key) => Ok(stored_key.as_slice() == identity.serialize().as_ref()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true), // TOFU: trust on first use
            Err(e) => Err(SignalProtocolError::InvalidState("is_trusted_identity", e.to_string())),
        }
    }

    async fn get_identity(
        &self,
        address: &ProtocolAddress,
    ) -> Result<Option<IdentityKey>, SignalProtocolError> {
        let addr_name = address.name();
        let device_id: u32 = address.device_id().into();

        match self.conn.query_row(
            "SELECT identity_key FROM crypto_trusted_identities WHERE address = ?1 AND device_id = ?2",
            rusqlite::params![addr_name, device_id],
            |row| row.get::<_, Vec<u8>>(0),
        ) {
            Ok(bytes) => Ok(Some(IdentityKey::decode(&bytes)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(SignalProtocolError::InvalidState("get_identity", e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_test_db;
    use crate::storage::CryptoStore;
    use libsignal_protocol::{DeviceId, IdentityKeyPair, ProtocolAddress};

    #[test]
    fn get_identity_key_pair_returns_error_when_empty() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let result = futures::executor::block_on(store.get_identity_key_pair());
        assert!(result.is_err());
    }

    #[test]
    fn save_then_get_identity_key_pair_round_trips() {
        let conn = init_test_db();
        let pair = IdentityKeyPair::generate(&mut rand::rng());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO crypto_identity_keys (id, public_key, private_key, created_at) VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![pair.public_key().serialize().as_ref(), pair.private_key().serialize(), now],
        )
        .unwrap();

        let store = CryptoStore::new(&conn);
        let loaded = futures::executor::block_on(store.get_identity_key_pair()).unwrap();
        assert_eq!(loaded.public_key(), pair.public_key());
        assert_eq!(loaded.private_key().serialize(), pair.private_key().serialize());
    }

    #[test]
    fn get_local_registration_id_stores_and_retrieves() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let reg_id: u32 = 12345;
        store.store_config("registration_id", &reg_id.to_be_bytes()).unwrap();

        let loaded = futures::executor::block_on(store.get_local_registration_id()).unwrap();
        assert_eq!(loaded, reg_id);
    }

    #[test]
    fn save_identity_stores_new_identity() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let pair = IdentityKeyPair::generate(&mut rand::rng());
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());

        let result = futures::executor::block_on(
            store.save_identity(&addr, pair.identity_key()),
        )
        .unwrap();
        assert_eq!(result, IdentityChange::NewOrUnchanged);

        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_trusted_identities WHERE address = 'user1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn save_identity_updates_existing_identity() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let pair_a = IdentityKeyPair::generate(&mut rand::rng());
        let pair_b = IdentityKeyPair::generate(&mut rand::rng());
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());

        futures::executor::block_on(store.save_identity(&addr, pair_a.identity_key())).unwrap();
        let result = futures::executor::block_on(
            store.save_identity(&addr, pair_b.identity_key()),
        )
        .unwrap();
        assert_eq!(result, IdentityChange::ReplacedExisting);

        let stored_key: Vec<u8> = conn
            .query_row(
                "SELECT identity_key FROM crypto_trusted_identities WHERE address = 'user1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_key.as_slice(), pair_b.identity_key().serialize().as_ref());
    }

    #[test]
    fn is_trusted_identity_returns_true_for_matching_key() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let pair = IdentityKeyPair::generate(&mut rand::rng());
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());

        futures::executor::block_on(store.save_identity(&addr, pair.identity_key())).unwrap();
        let trusted = futures::executor::block_on(
            store.is_trusted_identity(&addr, pair.identity_key(), Direction::Sending),
        )
        .unwrap();
        assert!(trusted);
    }

    #[test]
    fn is_trusted_identity_returns_true_for_unknown_address() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let pair = IdentityKeyPair::generate(&mut rand::rng());
        let addr = ProtocolAddress::new("never-seen".to_string(), DeviceId::new(1).unwrap());

        let trusted = futures::executor::block_on(
            store.is_trusted_identity(&addr, pair.identity_key(), Direction::Receiving),
        )
        .unwrap();
        assert!(trusted);
    }

    #[test]
    fn is_trusted_identity_returns_false_for_changed_key() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let pair_a = IdentityKeyPair::generate(&mut rand::rng());
        let pair_b = IdentityKeyPair::generate(&mut rand::rng());
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());

        futures::executor::block_on(store.save_identity(&addr, pair_a.identity_key())).unwrap();
        let trusted = futures::executor::block_on(
            store.is_trusted_identity(&addr, pair_b.identity_key(), Direction::Sending),
        )
        .unwrap();
        assert!(!trusted);
    }
}
