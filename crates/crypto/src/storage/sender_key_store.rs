//! SenderKeyStore no-op implementation for CryptoStore.
//!
//! Group messaging (Sender Keys) is out of scope for V1, but libsignal
//! requires this trait to be present for some operations.

use async_trait::async_trait;
use libsignal_protocol::{ProtocolAddress, SenderKeyRecord, SenderKeyStore, SignalProtocolError};
use uuid::Uuid;

use crate::storage::CryptoStore;

#[async_trait(?Send)]
impl SenderKeyStore for CryptoStore<'_> {
    async fn store_sender_key(
        &mut self,
        _sender: &ProtocolAddress,
        _distribution_id: Uuid,
        _record: &SenderKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        Ok(())
    }

    async fn load_sender_key(
        &mut self,
        _sender: &ProtocolAddress,
        _distribution_id: Uuid,
    ) -> Result<Option<SenderKeyRecord>, SignalProtocolError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_test_db;
    use crate::storage::CryptoStore;
    use libsignal_protocol::{DeviceId, ProtocolAddress};

    #[test]
    fn store_sender_key_returns_ok() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());
        let dist_id = Uuid::new_v4();
        // SenderKeyRecord::new_empty() is pub(crate), so deserialize from empty protobuf
        let record = SenderKeyRecord::deserialize(&[]).unwrap();

        let result = futures::executor::block_on(
            store.store_sender_key(&addr, dist_id, &record),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn load_sender_key_returns_none() {
        let conn = init_test_db();
        let mut store = CryptoStore::new(&conn);
        let addr = ProtocolAddress::new("user1".to_string(), DeviceId::new(1).unwrap());
        let dist_id = Uuid::new_v4();

        let result = futures::executor::block_on(store.load_sender_key(&addr, dist_id)).unwrap();
        assert!(result.is_none());
    }
}
