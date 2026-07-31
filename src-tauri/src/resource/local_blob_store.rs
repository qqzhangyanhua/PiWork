use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::blob_store::{
    BlobInput, BlobMetadata, BlobStore, ResourceStorageError, StorageRef, StoredBlob,
};

#[derive(Clone)]
pub struct LocalBlobStore {
    root: PathBuf,
}

impl LocalBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub fn path_for(&self, reference: &StorageRef) -> Result<PathBuf, ResourceStorageError> {
        let parts = reference.object_key.split('/').collect::<Vec<_>>();
        let valid_hash = |value: &str, length: usize| {
            value.len() == length
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        };
        if parts.len() != 3
            || parts[0] != "blobs"
            || !valid_hash(parts[1], 2)
            || !valid_hash(parts[2], 64)
            || !parts[2].starts_with(parts[1])
        {
            return Err(ResourceStorageError::InvalidObjectKey);
        }
        Ok(self.root.join(parts[0]).join(parts[1]).join(parts[2]))
    }

    pub async fn list_objects_older_than(
        &self,
        cutoff: SystemTime,
    ) -> Result<Vec<StorageRef>, ResourceStorageError> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let blobs = root.join("blobs");
            if !blobs.exists() {
                return Ok(Vec::new());
            }
            let mut references = Vec::new();
            for prefix in fs::read_dir(&blobs)? {
                let prefix = prefix?;
                if prefix.file_type()?.is_symlink() || !prefix.file_type()?.is_dir() {
                    continue;
                }
                let prefix_name = prefix.file_name().to_string_lossy().into_owned();
                for object in fs::read_dir(prefix.path())? {
                    let object = object?;
                    let file_type = object.file_type()?;
                    if file_type.is_symlink() || !file_type.is_file() {
                        continue;
                    }
                    let modified = object.metadata()?.modified()?;
                    if modified > cutoff {
                        continue;
                    }
                    let reference = StorageRef {
                        object_key: format!(
                            "blobs/{prefix_name}/{}",
                            object.file_name().to_string_lossy()
                        ),
                    };
                    let validator = LocalBlobStore::new(&root);
                    if validator.path_for(&reference).is_ok() {
                        references.push(reference);
                    }
                }
            }
            Ok(references)
        })
        .await
        .map_err(|error| ResourceStorageError::Io(std::io::Error::other(error.to_string())))?
    }

    fn put_blocking(root: &Path, source_path: &Path) -> Result<StoredBlob, ResourceStorageError> {
        let staging_dir = root.join("staging");
        fs::create_dir_all(&staging_dir)?;
        let staging_path = staging_dir.join(format!("{}.part", Uuid::new_v4()));
        let result = (|| {
            let mut source = File::open(source_path)?;
            let mut staging = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&staging_path)?;
            let mut hasher = Sha256::new();
            let mut size = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = source.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                staging.write_all(&buffer[..read])?;
                hasher.update(&buffer[..read]);
                size = size
                    .checked_add(read as u64)
                    .ok_or_else(|| std::io::Error::other("blob size overflow"))?;
            }
            staging.sync_all()?;
            drop(staging);

            let sha256 = hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let object_key = format!("blobs/{}/{sha256}", &sha256[..2]);
            let target = root.join("blobs").join(&sha256[..2]).join(&sha256);
            fs::create_dir_all(target.parent().expect("blob target has a parent"))?;
            if target.exists() {
                fs::remove_file(&staging_path)?;
            } else if let Err(error) = fs::rename(&staging_path, &target) {
                if target.exists() {
                    let _ = fs::remove_file(&staging_path);
                } else {
                    return Err(error.into());
                }
            }
            Ok(StoredBlob {
                reference: StorageRef {
                    object_key: object_key.clone(),
                },
                object_key,
                sha256,
                size,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(staging_path);
        }
        result
    }
}

#[async_trait]
impl BlobStore for LocalBlobStore {
    async fn put(&self, input: BlobInput) -> Result<StoredBlob, ResourceStorageError> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || Self::put_blocking(&root, &input.source_path))
            .await
            .map_err(|error| ResourceStorageError::Io(std::io::Error::other(error.to_string())))?
    }

    async fn read(&self, reference: &StorageRef) -> Result<Vec<u8>, ResourceStorageError> {
        let path = self.path_for(reference)?;
        tokio::task::spawn_blocking(move || fs::read(path))
            .await
            .map_err(|error| ResourceStorageError::Io(std::io::Error::other(error.to_string())))?
            .map_err(Into::into)
    }

    async fn stat(&self, reference: &StorageRef) -> Result<BlobMetadata, ResourceStorageError> {
        let path = self.path_for(reference)?;
        tokio::task::spawn_blocking(move || fs::metadata(path))
            .await
            .map_err(|error| ResourceStorageError::Io(std::io::Error::other(error.to_string())))?
            .map(|metadata| BlobMetadata {
                size: metadata.len(),
            })
            .map_err(Into::into)
    }

    async fn delete(&self, reference: &StorageRef) -> Result<(), ResourceStorageError> {
        let path = self.path_for(reference)?;
        tokio::task::spawn_blocking(move || match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        })
        .await
        .map_err(|error| ResourceStorageError::Io(std::io::Error::other(error.to_string())))?
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::LocalBlobStore;
    use crate::resource::blob_store::{BlobInput, BlobStore, StorageRef};

    #[tokio::test]
    async fn put_is_content_addressed_and_deduplicated() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.png");
        std::fs::write(&source, b"same bytes").unwrap();
        let store = LocalBlobStore::new(root.path().join("objects"));

        let first = store
            .put(BlobInput {
                source_path: source.clone(),
            })
            .await
            .unwrap();
        let second = store
            .put(BlobInput {
                source_path: source,
            })
            .await
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(first.sha256.len(), 64);
        assert_eq!(first.size, 10);
        assert_eq!(
            store.path_for(&first.reference).unwrap(),
            root.path().join("objects").join(&first.object_key)
        );
    }

    #[tokio::test]
    async fn storage_references_cannot_escape_the_resource_root() {
        let root = tempfile::tempdir().unwrap();
        let store = LocalBlobStore::new(root.path());
        let error = store
            .read(&StorageRef {
                object_key: "../secret".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "invalid blob object key");
    }
}
