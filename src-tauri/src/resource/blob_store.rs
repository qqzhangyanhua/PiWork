use std::path::PathBuf;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobInput {
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageRef {
    pub object_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBlob {
    pub reference: StorageRef,
    pub object_key: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobMetadata {
    pub size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceStorageError {
    #[error("invalid blob object key")]
    InvalidObjectKey,
    #[error("blob I/O failed")]
    Io(#[source] std::io::Error),
}

impl From<std::io::Error> for ResourceStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, input: BlobInput) -> Result<StoredBlob, ResourceStorageError>;
    async fn read(&self, reference: &StorageRef) -> Result<Vec<u8>, ResourceStorageError>;
    async fn stat(&self, reference: &StorageRef) -> Result<BlobMetadata, ResourceStorageError>;
    async fn delete(&self, reference: &StorageRef) -> Result<(), ResourceStorageError>;
}
