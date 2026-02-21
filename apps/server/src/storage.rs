use std::sync::Arc;

use object_store::local::LocalFileSystem;
use object_store::memory::InMemory;
use object_store::ObjectStore;

use crate::config::FileStorageConfig;

/// Creates an ObjectStore backend from the file storage configuration.
///
/// - `"local"` backend: creates directory if needed, uses `LocalFileSystem`
/// - `"memory"` backend: uses `InMemory` (for testing)
pub fn create_object_store(
    config: &FileStorageConfig,
) -> Result<Arc<dyn ObjectStore>, Box<dyn std::error::Error>> {
    match config.backend.as_str() {
        "local" => {
            std::fs::create_dir_all(&config.local_path)?;
            let store = LocalFileSystem::new_with_prefix(&config.local_path)?;
            Ok(Arc::new(store))
        }
        "memory" => Ok(Arc::new(InMemory::new())),
        other => Err(format!("unknown file storage backend: {other}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::path::Path;
    use object_store::PutPayload;

    #[tokio::test]
    async fn inmemory_backend_roundtrip() {
        let config = FileStorageConfig {
            backend: "memory".into(),
            local_path: String::new(),
            max_file_size_bytes: 1024,
        };
        let store = create_object_store(&config).unwrap();
        let path = Path::from("test/blob.bin");
        let data = b"hello world".to_vec();

        store
            .put(&path, PutPayload::from(data.clone()))
            .await
            .unwrap();
        let result = store.get(&path).await.unwrap();
        let bytes = result.bytes().await.unwrap();
        assert_eq!(bytes.as_ref(), data.as_slice());
    }

    #[tokio::test]
    async fn local_backend_initializes() {
        let dir = tempfile::tempdir().unwrap();
        let config = FileStorageConfig {
            backend: "local".into(),
            local_path: dir.path().to_str().unwrap().into(),
            max_file_size_bytes: 1024,
        };
        let store = create_object_store(&config).unwrap();
        let path = Path::from("test.bin");
        let data = b"test data".to_vec();

        store
            .put(&path, PutPayload::from(data.clone()))
            .await
            .unwrap();
        let result = store.get(&path).await.unwrap();
        let bytes = result.bytes().await.unwrap();
        assert_eq!(bytes.as_ref(), data.as_slice());
    }

    #[test]
    fn unknown_backend_returns_error() {
        let config = FileStorageConfig {
            backend: "s3".into(),
            local_path: String::new(),
            max_file_size_bytes: 1024,
        };
        assert!(create_object_store(&config).is_err());
    }
}
