use crate::ids::{FileId, UserId};
use serde::{Deserialize, Serialize};

/// Response returned after a successful file upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct FileResponse {
    pub id: FileId,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Response for file metadata queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct FileMetaResponse {
    pub id: FileId,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub uploader_id: UserId,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_response_serializes() {
        let resp = FileResponse {
            id: FileId::new(),
            file_name: "test.bin".into(),
            mime_type: "application/octet-stream".into(),
            size_bytes: 1024,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("id").is_some());
        assert_eq!(json["file_name"], "test.bin");
        assert_eq!(json["size_bytes"], 1024);
    }

    #[test]
    fn file_meta_response_serializes() {
        let resp = FileMetaResponse {
            id: FileId::new(),
            file_name: "doc.pdf".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 2048,
            uploader_id: UserId::new(),
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("uploader_id").is_some());
        assert_eq!(json["mime_type"], "application/pdf");
    }

    #[test]
    fn file_response_roundtrip() {
        let resp = FileResponse {
            id: FileId::new(),
            file_name: "test.bin".into(),
            mime_type: "application/octet-stream".into(),
            size_bytes: 1024,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: FileResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.file_name, "test.bin");
        assert_eq!(back.size_bytes, 1024);
    }
}
