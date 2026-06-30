//! Local filesystem storage backend

use async_trait::async_trait;
use axum::body::Body;
use std::path::PathBuf;
use tokio_util::io::ReaderStream;

use crate::{
    error::RegistryResult,
    storage::{
        Storage, build_storage_path, compute_sha256, delete_file_async, read_file_async,
        write_file_async,
    },
};

/// Local filesystem storage
#[derive(Debug, Clone)]
pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    /// Create new local storage
    pub fn new(base_path: &str) -> RegistryResult<Self> {
        let base_path = PathBuf::from(base_path);

        // Create base directory if it doesn't exist
        std::fs::create_dir_all(&base_path)?;

        Ok(Self { base_path })
    }

    /// Get full path for a storage path
    fn full_path(&self, path: &str) -> PathBuf {
        self.base_path.join(path)
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn store(
        &self,
        org: &str,
        harness: &str,
        version: &str,
        content: &[u8],
    ) -> RegistryResult<String> {
        let hash = compute_sha256(content);
        let storage_path = build_storage_path(org, harness, version, &hash);
        let full_path = self.full_path(&storage_path);

        // Check if already exists (content-addressed)
        if full_path.exists() {
            return Ok(storage_path);
        }

        // Write file
        write_file_async(&full_path, content).await?;

        Ok(storage_path)
    }

    async fn retrieve(&self, path: &str) -> RegistryResult<Vec<u8>> {
        let full_path = self.full_path(path);
        read_file_async(&full_path).await
    }

    async fn retrieve_body(&self, path: &str) -> RegistryResult<Body> {
        let full_path = self.full_path(path);
        let file = tokio::fs::File::open(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                crate::error::RegistryError::NotFound(format!(
                    "File not found: {}",
                    full_path.display()
                ))
            } else {
                crate::error::RegistryError::Io(e)
            }
        })?;

        Ok(Body::from_stream(ReaderStream::new(file)))
    }

    async fn exists(&self, path: &str) -> RegistryResult<bool> {
        let full_path = self.full_path(path);
        Ok(full_path.exists())
    }

    async fn delete(&self, path: &str) -> RegistryResult<()> {
        let full_path = self.full_path(path);
        delete_file_async(&full_path).await
    }

    fn public_url(&self, path: &str) -> RegistryResult<String> {
        // For local storage, return a relative path
        Ok(format!("/packages/{}", path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let content = b"test package content";
        let org = "testorg";
        let harness = "testharness";
        let version = "1.0.0";

        // Store
        let path = storage.store(org, harness, version, content).await.unwrap();
        assert!(!path.is_empty());

        // Exists
        assert!(storage.exists(&path).await.unwrap());

        // Retrieve
        let retrieved = storage.retrieve(&path).await.unwrap();
        assert_eq!(retrieved, content);

        // Delete
        storage.delete(&path).await.unwrap();
        assert!(!storage.exists(&path).await.unwrap());
    }

    #[tokio::test]
    async fn test_content_addressed_dedup() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let content = b"duplicate content";

        // Store same content twice
        let path1 = storage
            .store("org1", "harness1", "1.0.0", content)
            .await
            .unwrap();
        let path2 = storage
            .store("org1", "harness1", "1.0.0", content)
            .await
            .unwrap();

        // Should return same path (content-addressed)
        assert_eq!(path1, path2);
    }
}
