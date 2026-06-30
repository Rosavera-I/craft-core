//! Storage backends for harness packages
//!
//! Supports local filesystem and S3-compatible storage with content-addressed
//! storage using SHA-256 hashes.

use async_trait::async_trait;
use axum::body::Body;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

use crate::{
    StorageConfig,
    error::{RegistryError, RegistryResult},
};

mod local;
mod s3;

pub use local::LocalStorage;
pub use s3::S3Storage;

/// Storage backend trait
#[async_trait]
pub trait Storage: Send + Sync {
    /// Store a package and return the storage path
    async fn store(
        &self,
        org: &str,
        harness: &str,
        version: &str,
        content: &[u8],
    ) -> RegistryResult<String>;

    /// Retrieve a package by storage path
    async fn retrieve(&self, path: &str) -> RegistryResult<Vec<u8>>;

    /// Retrieve a package as an HTTP response body.
    async fn retrieve_body(&self, path: &str) -> RegistryResult<Body> {
        Ok(Body::from(self.retrieve(path).await?))
    }

    /// Check if a package exists
    async fn exists(&self, path: &str) -> RegistryResult<bool>;

    /// Delete a package
    async fn delete(&self, path: &str) -> RegistryResult<()>;

    /// Get the public URL for a package (if applicable)
    fn public_url(&self, path: &str) -> RegistryResult<String>;
}

/// Create storage backend from config
pub fn create_storage(config: &StorageConfig) -> RegistryResult<Box<dyn Storage>> {
    match config {
        StorageConfig::Local { base_path } => Ok(Box::new(LocalStorage::new(base_path)?)),
        StorageConfig::S3 {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
        } => Ok(Box::new(S3Storage::new(
            bucket,
            region,
            endpoint.as_deref(),
            access_key_id,
            secret_access_key,
        )?)),
    }
}

/// Compute SHA-256 hash of content
pub fn compute_sha256(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

/// Verify content against expected hash
pub fn verify_content(content: &[u8], expected_hash: &str) -> RegistryResult<()> {
    let actual_hash = compute_sha256(content);
    if actual_hash != expected_hash {
        return Err(RegistryError::Package(format!(
            "Content hash mismatch: expected {}, got {}",
            expected_hash, actual_hash
        )));
    }
    Ok(())
}

/// Build storage path from components
pub fn build_storage_path(org: &str, harness: &str, version: &str, hash: &str) -> String {
    // Use first 2 chars of hash as prefix for better filesystem distribution
    let prefix = &hash[..2];
    let suffix = &hash[2..];
    format!("{}/{}/{}/{}/{}", org, harness, version, prefix, suffix)
}

/// Read file asynchronously
pub async fn read_file_async(path: &Path) -> RegistryResult<Vec<u8>> {
    let mut file = tokio::fs::File::open(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            RegistryError::NotFound(format!("File not found: {}", path.display()))
        } else {
            RegistryError::Io(e)
        }
    })?;

    let mut contents = Vec::new();
    file.read_to_end(&mut contents).await?;
    Ok(contents)
}

/// Write file asynchronously
pub async fn write_file_async(path: &Path, content: &[u8]) -> RegistryResult<()> {
    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Delete file asynchronously
pub async fn delete_file_async(path: &Path) -> RegistryResult<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(RegistryError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sha256() {
        let content = b"hello world";
        let hash = compute_sha256(content);
        // Known SHA-256 hash for "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_verify_content() {
        let content = b"hello world";
        let hash = compute_sha256(content);

        // Should succeed with correct hash
        assert!(verify_content(content, &hash).is_ok());

        // Should fail with incorrect hash
        assert!(verify_content(content, "wrong_hash").is_err());
    }

    #[test]
    fn test_build_storage_path() {
        let path = build_storage_path("myorg", "myharness", "1.0.0", "abcdef123456");
        assert_eq!(path, "myorg/myharness/1.0.0/ab/cdef123456");
    }
}
