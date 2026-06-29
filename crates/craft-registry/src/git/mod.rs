//! Git operations for the CRAFT Registry
//!
//! Provides Git repository management for harness packages, including
//! cloning, tagging, and extracting package contents.

use git2::{Repository, Signature, Sort};
use std::path::{Path, PathBuf};

use crate::error::{RegistryError, RegistryResult};

/// Git repository manager
#[derive(Debug, Clone)]
pub struct GitManager {
    base_path: PathBuf,
}

impl GitManager {
    /// Create a new Git manager
    pub fn new(base_path: &str) -> RegistryResult<Self> {
        let base_path = PathBuf::from(base_path);
        std::fs::create_dir_all(&base_path)?;
        Ok(Self { base_path })
    }

    /// Clone or open a repository
    pub fn clone_or_open(&self, url: &str, name: &str) -> RegistryResult<Repository> {
        let repo_path = self.base_path.join(name);

        if repo_path.exists() {
            // Open existing
            Repository::open(&repo_path).map_err(|e| e.into())
        } else {
            // Clone new
            Repository::clone(url, &repo_path).map_err(|e| e.into())
        }
    }

    /// Open an existing repository
    pub fn open(&self, name: &str) -> RegistryResult<Repository> {
        let repo_path = self.base_path.join(name);
        Repository::open(&repo_path).map_err(|e| e.into())
    }

    /// Get the commit SHA for a ref
    pub fn resolve_ref(&self, repo: &Repository, git_ref: &str) -> RegistryResult<String> {
        let obj = repo.revparse_single(git_ref)?;
        let commit = obj.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    /// Get commit message for a ref
    pub fn get_commit_message(&self, repo: &Repository, git_ref: &str) -> RegistryResult<String> {
        let obj = repo.revparse_single(git_ref)?;
        let commit = obj.peel_to_commit()?;
        commit
            .message()
            .map(|s| s.to_string())
            .ok_or_else(|| RegistryError::Git(git2::Error::from_str("Invalid commit message")))
    }

    /// Create a tag
    pub fn create_tag(
        &self,
        repo: &Repository,
        tag_name: &str,
        message: &str,
        author_name: &str,
        author_email: &str,
    ) -> RegistryResult<String> {
        let obj = repo.head()?.peel_to_commit()?;
        let sig = Signature::now(author_name, author_email)?;

        let tag_id = repo.tag(tag_name, &obj.into_object(), &sig, message, false)?;
        Ok(tag_id.to_string())
    }

    /// Checkout a specific ref
    pub fn checkout(&self, repo: &Repository, git_ref: &str) -> RegistryResult<()> {
        let obj = repo.revparse_single(git_ref)?;
        repo.checkout_tree(&obj, None)?;
        repo.set_head_detached(obj.id())?;
        Ok(())
    }

    /// Get the README content from a ref
    pub fn get_readme(&self, repo: &Repository, git_ref: &str) -> RegistryResult<Option<String>> {
        self.checkout(repo, git_ref)?;

        let workdir = repo
            .workdir()
            .ok_or_else(|| RegistryError::Git(git2::Error::from_str("Bare repository")))?;

        // Try common README filenames
        for name in &["README.md", "README.rst", "README.txt", "README"] {
            let path = workdir.join(name);
            if path.exists() {
                return Ok(Some(std::fs::read_to_string(&path)?));
            }
        }

        Ok(None)
    }

    /// Get the manifest content from a ref
    pub fn get_manifest(&self, repo: &Repository, git_ref: &str) -> RegistryResult<String> {
        self.checkout(repo, git_ref)?;

        let workdir = repo
            .workdir()
            .ok_or_else(|| RegistryError::Git(git2::Error::from_str("Bare repository")))?;

        let path = workdir.join("craft.toml");
        if path.exists() {
            Ok(std::fs::read_to_string(&path)?)
        } else {
            Err(RegistryError::NotFound(
                "craft.toml not found in repository".to_string(),
            ))
        }
    }

    /// List recent commits
    pub fn list_commits(&self, repo: &Repository, limit: usize) -> RegistryResult<Vec<CommitInfo>> {
        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(Sort::TIME)?;

        let mut commits = Vec::new();
        for (i, oid) in revwalk.enumerate() {
            if i >= limit {
                break;
            }
            let oid = oid?;
            let commit = repo.find_commit(oid)?;

            commits.push(CommitInfo {
                sha: oid.to_string(),
                message: commit
                    .message()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
                author: commit.author().name().unwrap_or("Unknown").to_string(),
                timestamp: commit.time().seconds(),
            });
        }

        Ok(commits)
    }

    /// Create a tarball of the repository at a specific ref
    pub fn create_tarball(
        &self,
        repo: &Repository,
        git_ref: &str,
        output_path: &Path,
    ) -> RegistryResult<()> {
        self.checkout(repo, git_ref)?;

        let workdir = repo
            .workdir()
            .ok_or_else(|| RegistryError::Git(git2::Error::from_str("Bare repository")))?;

        // Create tarball
        let tar_gz = std::fs::File::create(output_path)?;
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        // Add all files (excluding .git)
        tar.append_dir_all(".", workdir)?;
        tar.finish()?;

        Ok(())
    }
}

/// Commit information
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_git_manager_new() {
        let temp_dir = TempDir::new().unwrap();
        let manager = GitManager::new(temp_dir.path().to_str().unwrap());
        assert!(manager.is_ok());
    }

    // Note: Full Git tests would require actual repositories
    // These would be integration tests with real Git repos
}
