use std::path::{Path, PathBuf};

use craft_manifest::{Manifest, ManifestError, load_manifest};

#[derive(Debug, Clone)]
pub struct HarnessProject {
    root: PathBuf,
    manifest: Manifest,
}

impl HarnessProject {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let root = root.as_ref().to_path_buf();
        let manifest = load_manifest(root.join("craft.toml"))?;
        Ok(Self { root, manifest })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}
