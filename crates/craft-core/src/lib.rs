use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use craft_manifest::{Manifest, ManifestError, load_manifest};
use rusqlite::{Connection, OptionalExtension, params};
use semver::Version;


pub mod version;
pub use version::{VersionConstraint, VersionError, VersionResolver, ResolvedVersion, VersionedHarness, HarnessDependency};

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

#[derive(Debug, Clone)]
pub struct CraftHome {
    root: PathBuf,
}

impl CraftHome {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_env() -> Result<Self, CraftError> {
        if let Some(path) = env::var_os("CRAFT_HOME") {
            return Ok(Self::new(path));
        }

        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| CraftError::Config("HOME is not set; set CRAFT_HOME".to_string()))?;
        Ok(Self::new(home.join(".craft")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn harnesses_dir(&self) -> PathBuf {
        self.root.join("harnesses")
    }

    pub fn registry_path(&self) -> PathBuf {
        self.root.join("registry.sqlite3")
    }

    pub fn ensure(&self) -> Result<(), CraftError> {
        fs::create_dir_all(self.harnesses_dir())?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubSource {
    pub owner: String,
    pub repo: String,
    pub reference: Option<String>,
}

impl GithubSource {
    pub fn parse(input: &str) -> Result<Self, CraftError> {
        let raw = input.strip_prefix("github:").ok_or_else(|| {
            CraftError::InvalidSource("source must start with github:".to_string())
        })?;
        let (path, reference) = match raw.split_once('@') {
            Some((path, reference)) if !reference.trim().is_empty() => {
                (path, Some(reference.trim().to_string()))
            }
            Some(_) => {
                return Err(CraftError::InvalidSource(
                    "github source reference must not be empty".to_string(),
                ));
            }
            None => (raw, None),
        };
        let (owner, repo) = path.split_once('/').ok_or_else(|| {
            CraftError::InvalidSource("github source must be github:owner/repo".to_string())
        })?;
        validate_slug("owner", owner)?;
        validate_slug("repo", repo)?;
        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            reference,
        })
    }

    /// Parse a source with optional version constraint (e.g., ^1.2.0, ~2.0.0)
    pub fn parse_with_constraint(input: &str) -> Result<(Self, Option<VersionConstraint>), CraftError> {
        let raw = input.strip_prefix("github:").ok_or_else(|| {
            CraftError::InvalidSource("source must start with github:".to_string())
        })?;
        
        // Try to extract version constraint
        let (path, reference, constraint) = match raw.split_once('@') {
            Some((path, reference)) if !reference.trim().is_empty() => {
                let reference = reference.trim();
                // Check if reference looks like a version constraint
                if reference.starts_with('^') || reference.starts_with('~') || 
                   reference.starts_with(">=") || reference.starts_with('<') ||
                   reference.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    // Try to parse as version constraint
                    match VersionConstraint::parse(reference) {
                        Ok(constraint) => (path, None, Some(constraint)),
                        Err(_) => (path, Some(reference.to_string()), None)
                    }
                } else {
                    (path, Some(reference.to_string()), None)
                }
            }
            Some(_) => {
                return Err(CraftError::InvalidSource(
                    "github source reference must not be empty".to_string(),
                ));
            }
            None => (raw, None, None),
        };
        
        let (owner, repo) = path.split_once('/').ok_or_else(|| {
            CraftError::InvalidSource("github source must be github:owner/repo".to_string())
        })?;
        validate_slug("owner", owner)?;
        validate_slug("repo", repo)?;
        
        Ok((
            Self {
                owner: owner.to_string(),
                repo: repo.to_string(),
                reference,
            },
            constraint
        ))
    }

    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.repo)
    }

    pub fn source_id(&self) -> String {
        match &self.reference {
            Some(reference) => format!("github:{}/{}@{}", self.owner, self.repo, reference),
            None => format!("github:{}/{}", self.owner, self.repo),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledHarness {
    pub name: String,
    pub version: String,
    pub source: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallResult {
    pub harness: InstalledHarness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeResult {
    pub output_path: PathBuf,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactConflict {
    pub harness_name: String,
    pub artifact_type: String,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    OrderedMerge,
    Merge,
    Override,
    Fail,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        Self::OrderedMerge
    }
}

impl ConflictStrategy {
    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "ordered-merge" => Some(Self::OrderedMerge),
            "merge" => Some(Self::Merge),
            "override" => Some(Self::Override),
            "fail" => Some(Self::Fail),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OrderedMerge => "ordered-merge",
            Self::Merge => "merge",
            Self::Override => "override",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionPlan {
    pub strategy: ConflictStrategy,
    pub harnesses: Vec<CompositionHarness>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionHarness {
    pub name: String,
    pub version: String,
    pub source: String,
    pub path: PathBuf,
    pub prompt_path: PathBuf,
    pub memory_schema_path: PathBuf,
    pub mcp_tools_path: PathBuf,
    pub tdd_validators_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub harness_name: String,
    pub tdd_path: PathBuf,
    pub checks_run: bool,
    pub runner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeArtifact {
    pub harness: InstalledHarness,
    pub manifest: Manifest,
    pub system_prompt: String,
    pub memory_schema: String,
    pub mcp_tools: String,
    pub tdd_validators: String,
}

#[derive(Debug)]
pub enum CraftError {
    Config(String),
    InvalidSource(String),
    InvalidName(String),
    MissingHarness(String),
    Io {
        message: String,
        source: std::io::Error,
    },
    Manifest(ManifestError),
    CommandFailed(String),
    Registry {
        message: String,
        source: rusqlite::Error,
    },
}

impl fmt::Display for CraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CraftError::Config(message)
            | CraftError::InvalidSource(message)
            | CraftError::InvalidName(message)
            | CraftError::MissingHarness(message)
            | CraftError::CommandFailed(message) => write!(f, "{message}"),
            CraftError::Io { message, .. } | CraftError::Registry { message, .. } => {
                write!(f, "{message}")
            }
            CraftError::Manifest(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CraftError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CraftError::Io { source, .. } => Some(source),
            CraftError::Manifest(error) => Some(error),
            CraftError::Registry { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl CraftError {
    pub fn code(&self) -> &'static str {
        match self {
            CraftError::Config(_) => "config",
            CraftError::InvalidSource(_) => "invalid-source",
            CraftError::InvalidName(_) => "invalid-name",
            CraftError::MissingHarness(_) => "missing-harness",
            CraftError::Io { .. } => "io",
            CraftError::Manifest(_) => "manifest",
            CraftError::CommandFailed(_) => "runtime",
            CraftError::Registry { .. } => "sqlite",
        }
    }

    fn io(message: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            message: message.into(),
            source,
        }
    }

    fn registry(message: impl Into<String>, source: rusqlite::Error) -> Self {
        Self::Registry {
            message: message.into(),
            source,
        }
    }
}

impl From<std::io::Error> for CraftError {
    fn from(value: std::io::Error) -> Self {
        Self::io(value.to_string(), value)
    }
}

impl From<ManifestError> for CraftError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl From<rusqlite::Error> for CraftError {
    fn from(value: rusqlite::Error) -> Self {
        Self::registry(value.to_string(), value)
    }
}

pub struct HarnessManager {
    home: CraftHome,
}

impl HarnessManager {
    pub fn new(home: CraftHome) -> Self {
        Self { home }
    }

    /// Install a harness from a GitHub source.
    pub fn install_github(&self, source: &GithubSource) -> Result<InstallResult, CraftError> {
        self.home.ensure()?;
        let checkout = self.home.harnesses_dir().join(&source.repo);
        if checkout.exists() {
            return Err(CraftError::InvalidSource(format!(
                "target harness directory already exists: {}",
                checkout.display()
            )));
        }

        run_command(
            "git",
            ["clone", "--depth", "1", source.clone_url().as_str()],
            &self.home.harnesses_dir(),
        )?;

        if let Some(reference) = &source.reference {
            run_command(
                "git",
                ["fetch", "--depth", "1", "origin", reference],
                &checkout,
            )?;
            run_command("git", ["checkout", "FETCH_HEAD"], &checkout)?;
        }

        let manifest = load_manifest(checkout.join("craft.toml"))?;
        let installed = InstalledHarness {
            name: manifest.harness.name.clone(),
            version: manifest.harness.version.clone(),
            source: source.source_id(),
            path: checkout.clone(),
        };
        let registry = HarnessRegistry::open(self.home.registry_path())?;
        registry.upsert(&installed)?;
        Ok(InstallResult { harness: installed })
    }

    /// Install a harness by version constraint.
    /// If the constraint is satisfied by an already-installed version, return that.
    /// Otherwise, clone from the source and install.
    pub fn install_with_constraint(
        &self,
        source: &GithubSource,
        constraint: &VersionConstraint,
    ) -> Result<InstallResult, CraftError> {
        let registry = HarnessRegistry::open(self.home.registry_path())?;
        
        // Check if a matching version already exists
        if let Some(existing) = registry.find_version(&source.repo, constraint)? {
            return Ok(InstallResult { harness: existing });
        }
        
        // Otherwise, install from source
        self.install_github(source)
    }

    /// Check if a harness version matching the constraint is already installed.
    pub fn is_version_installed(
        &self,
        harness_name: &str,
        constraint: &VersionConstraint,
    ) -> Result<Option<InstalledHarness>, CraftError> {
        let registry = HarnessRegistry::open(self.home.registry_path())?;
        registry.find_version(harness_name, constraint)
    }

    /// Resolve a list of harness dependencies to specific versions.
    pub fn resolve_dependencies(
        &self,
        dependencies: &[HarnessDependency],
    ) -> Result<Vec<ResolvedVersion>, CraftError> {
        let registry = HarnessRegistry::open(self.home.registry_path())?;
        let resolver = registry.to_version_resolver()?;
        resolver.resolve_dependencies(dependencies).map_err(|e| {
            CraftError::InvalidSource(e.to_string())
        })
    }

    pub fn registry(&self) -> Result<HarnessRegistry, CraftError> {
        self.home.ensure()?;
        HarnessRegistry::open(self.home.registry_path())
    }
}

#[derive(Debug, Clone)]
pub struct HarnessRegistry {
    path: PathBuf,
}

impl HarnessRegistry {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, CraftError> {
        let registry = Self { path: path.into() };
        if let Some(parent) = registry.path.parent() {
            fs::create_dir_all(parent)?;
        }
        registry.init_schema()?;
        Ok(registry)
    }

    fn init_schema(&self) -> Result<(), CraftError> {
        let conn = self.connection()?;
        
        // Create harnesses table (primary version tracking - allows multiple versions)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS harnesses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                source TEXT NOT NULL,
                path TEXT NOT NULL,
                installed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(name, version)
            );",
        )?;

        // Create index for fast lookups by name
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_harnesses_name ON harnesses(name);",
        )?;

        // Create default_versions table for tracking which version is the default
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS default_versions (
                name TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                FOREIGN KEY (name, version) REFERENCES harnesses(name, version)
                    ON DELETE CASCADE
            );",
        )?;

        Ok(())
    }

    /// Upsert a harness into the registry.
    /// If the same version already exists, it updates the source and path.
    pub fn upsert(&self, harness: &InstalledHarness) -> Result<(), CraftError> {
        self.connection()?.execute(
            "INSERT INTO harnesses (name, version, source, path)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name, version) DO UPDATE SET
                source = excluded.source,
                path = excluded.path,
                installed_at = CURRENT_TIMESTAMP;",
            params![
                harness.name,
                harness.version,
                harness.source,
                harness.path.to_string_lossy()
            ],
        )?;
        
        // Set as default if no default exists for this harness
        let conn = self.connection()?;
        let has_default: bool = conn.query_row(
            "SELECT 1 FROM default_versions WHERE name = ?1;",
            params![&harness.name],
            |_| Ok(true),
        ).unwrap_or(false);
        
        if !has_default {
            conn.execute(
                "INSERT OR REPLACE INTO default_versions (name, version)
                 VALUES (?1, ?2);",
                params![&harness.name, &harness.version],
            )?;
        }
        
        Ok(())
    }

    /// List all harnesses (one entry per harness, using default version)
    pub fn list(&self) -> Result<Vec<InstalledHarness>, CraftError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("
                SELECT h.name, h.version, h.source, h.path 
                FROM harnesses h
                JOIN default_versions d ON h.name = d.name AND h.version = d.version
                ORDER BY h.name;
            ")?;
        let rows = statement.query_map([], installed_harness_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List all versions of a specific harness
    pub fn list_versions(&self, name: &str) -> Result<Vec<InstalledHarness>, CraftError> {
        validate_harness_name(name)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("
                SELECT name, version, source, path 
                FROM harnesses 
                WHERE name = ?1
                ORDER BY version DESC;
            ")?;
        let rows = statement.query_map(params![name], installed_harness_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get info for the default version of a harness
    pub fn info(&self, name: &str) -> Result<InstalledHarness, CraftError> {
        validate_harness_name(name)?;
        self.connection()?
            .query_row(
                "SELECT h.name, h.version, h.source, h.path 
                 FROM harnesses h
                 JOIN default_versions d ON h.name = d.name AND h.version = d.version
                 WHERE h.name = ?1;",
                params![name],
                installed_harness_from_row,
            )
            .optional()?
            .ok_or_else(|| CraftError::MissingHarness(format!("harness `{name}` is not installed")))
    }

    /// Get info for a specific version of a harness
    pub fn info_version(&self, name: &str, version: &str) -> Result<InstalledHarness, CraftError> {
        validate_harness_name(name)?;
        self.connection()?
            .query_row(
                "SELECT name, version, source, path FROM harnesses WHERE name = ?1 AND version = ?2;",
                params![name, version],
                installed_harness_from_row,
            )
            .optional()?
            .ok_or_else(|| CraftError::MissingHarness(
                format!("harness `{name}` version `{version}` is not installed")
            ))
    }

    /// Set the default version for a harness
    pub fn set_default_version(&self, name: &str, version: &str) -> Result<(), CraftError> {
        validate_harness_name(name)?;
        // Verify the version exists
        let exists = self.connection()?.query_row(
            "SELECT 1 FROM harnesses WHERE name = ?1 AND version = ?2;",
            params![name, version],
            |_| Ok(true),
        ).unwrap_or(false);
        
        if !exists {
            return Err(CraftError::MissingHarness(
                format!("harness `{name}` version `{version}` is not installed")
            ));
        }
        
        self.connection()?.execute(
            "INSERT OR REPLACE INTO default_versions (name, version) VALUES (?1, ?2);",
            params![name, version],
        )?;
        Ok(())
    }

    /// Get the default version for a harness
    pub fn get_default_version(&self, name: &str) -> Result<Option<String>, CraftError> {
        validate_harness_name(name)?;
        let version = self.connection()?.query_row(
            "SELECT version FROM default_versions WHERE name = ?1;",
            params![name],
            |row| row.get::<_, String>(0),
        ).optional()?;
        Ok(version)
    }

    /// Find the best matching harness version for a constraint
    pub fn find_version(&self, name: &str, constraint: &VersionConstraint) -> Result<Option<InstalledHarness>, CraftError> {
        validate_harness_name(name)?;
        let versions = self.list_versions(name)?;
        
        // Filter by constraint and sort by version descending
        let mut matching: Vec<_> = versions
            .into_iter()
            .filter(|h| {
                Version::parse(&h.version).map(|v| constraint.matches(&v)).unwrap_or(false)
            })
            .collect();
        
        // Sort by parsed version descending
        matching.sort_by(|a, b| {
            let va = Version::parse(&a.version).unwrap_or_else(|_| Version::new(0, 0, 0));
            let vb = Version::parse(&b.version).unwrap_or_else(|_| Version::new(0, 0, 0));
            vb.cmp(&va)
        });
        
        Ok(matching.into_iter().next())
    }

    /// Uninstall a specific version of a harness
    pub fn uninstall_version(
        &self,
        name: &str,
        version: Option<&str>,
        remove_files: bool,
    ) -> Result<InstalledHarness, CraftError> {
        let harness = if let Some(v) = version {
            self.info_version(name, v)?
        } else {
            self.info(name)?
        };
        
        // Use a single connection for all operations in this method
        let conn = self.connection()?;
        
        // Check if this was the default BEFORE deleting
        let was_default: bool = conn.query_row(
            "SELECT 1 FROM default_versions WHERE name = ?1 AND version = ?2;",
            params![&harness.name, &harness.version],
            |_| Ok(true),
        ).unwrap_or(false);
        
        conn.execute("DELETE FROM harnesses WHERE name = ?1 AND version = ?2;", 
            params![&harness.name, &harness.version])?;
        
        // Update default version if this was the default
        if was_default {
            // Find another version to set as default
            let new_default: Option<String> = conn.query_row(
                "SELECT version FROM harnesses WHERE name = ?1 ORDER BY version DESC LIMIT 1;",
                params![&harness.name],
                |row| row.get(0),
            ).optional()?;
            
            if let Some(v) = new_default {
                conn.execute(
                    "INSERT OR REPLACE INTO default_versions (name, version) VALUES (?1, ?2);",
                    params![&harness.name, v],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM default_versions WHERE name = ?1;",
                    params![&harness.name],
                )?;
            }
        }
        
        if remove_files && harness.path.exists() {
            fs::remove_dir_all(&harness.path)?;
        }
        Ok(harness)
    }

    /// Uninstall all versions of a harness (legacy alias)
    pub fn uninstall(
        &self,
        name: &str,
        remove_files: bool,
    ) -> Result<InstalledHarness, CraftError> {
        self.uninstall_version(name, None, remove_files)
    }

    /// Get all available harnesses and their versions for the resolver
    pub fn to_version_resolver(&self) -> Result<VersionResolver, CraftError> {
        let mut resolver = VersionResolver::new();
        let conn = self.connection()?;
        
        let mut stmt = conn.prepare(
            "SELECT name, version, source, path FROM harnesses;"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        
        for row in rows {
            let (name, version_str, source, path) = row?;
            if let Ok(version) = Version::parse(&version_str) {
                resolver.add_version(name, version, source, path);
            }
        }
        
        Ok(resolver)
    }

    fn connection(&self) -> Result<Connection, CraftError> {
        Connection::open(&self.path).map_err(|err| {
            CraftError::registry(
                format!("failed to open registry {}: {err}", self.path.display()),
                err,
            )
        })
    }
}

fn installed_harness_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<InstalledHarness, rusqlite::Error> {
    Ok(InstalledHarness {
        name: row.get(0)?,
        version: row.get(1)?,
        source: row.get(2)?,
        path: PathBuf::from(row.get::<_, String>(3)?),
    })
}

pub fn compose_harnesses(
    registry: &HarnessRegistry,
    harness_names: &[String],
    output_path: impl AsRef<Path>,
    strategy: ConflictStrategy,
) -> Result<ComposeResult, CraftError> {
    let (artifacts, warnings) = collect_compose_artifacts(registry, harness_names)?;
    let contents = render_compose(&artifacts, &warnings, strategy);
    let output_path = output_path.as_ref().to_path_buf();
    fs::write(&output_path, contents)?;
    Ok(ComposeResult {
        output_path,
        warnings,
    })
}

pub fn plan_composition(
    registry: &HarnessRegistry,
    harness_names: &[String],
    strategy: ConflictStrategy,
) -> Result<CompositionPlan, CraftError> {
    let (artifacts, warnings) = collect_compose_artifacts(registry, harness_names)?;
    Ok(CompositionPlan {
        strategy,
        harnesses: artifacts
            .into_iter()
            .map(|artifact| {
                let path = artifact.harness.path;
                let manifest = artifact.manifest;
                CompositionHarness {
                    name: manifest.harness.name,
                    version: manifest.harness.version,
                    source: artifact.harness.source,
                    prompt_path: path.join(manifest.prompts.system),
                    memory_schema_path: path.join(manifest.memory.schema),
                    mcp_tools_path: path.join(manifest.tools.mcp),
                    tdd_validators_path: path.join(manifest.validators.tdd),
                    path,
                }
            })
            .collect(),
        warnings,
    })
}

pub fn collect_compose_artifacts(
    registry: &HarnessRegistry,
    harness_names: &[String],
) -> Result<(Vec<ComposeArtifact>, Vec<String>), CraftError> {
    if harness_names.is_empty() {
        return Err(CraftError::InvalidName(
            "compose requires at least one harness".to_string(),
        ));
    }

    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_harnesses = BTreeMap::new();

    for name in harness_names {
        let installed = registry.info(name)?;
        let manifest = load_manifest(installed.path.join("craft.toml"))?;
        note_duplicate_harness(
            &mut seen_harnesses,
            &manifest.harness.name,
            &installed.source,
            &mut warnings,
        );

        artifacts.push(ComposeArtifact {
            system_prompt: read_harness_artifact(&installed.path, &manifest.prompts.system)?,
            memory_schema: read_harness_artifact(&installed.path, &manifest.memory.schema)?,
            mcp_tools: read_harness_artifact(&installed.path, &manifest.tools.mcp)?,
            tdd_validators: read_harness_artifact(&installed.path, &manifest.validators.tdd)?,
            harness: installed,
            manifest,
        });
    }

    Ok((artifacts, warnings))
}

pub fn validate_harness_project(root: impl AsRef<Path>) -> Result<ValidationResult, CraftError> {
    let project = HarnessProject::load(root.as_ref())?;
    let root = project.root().canonicalize().map_err(|err| {
        CraftError::io(
            format!(
                "failed to resolve harness root {}: {err}",
                project.root().display()
            ),
            err,
        )
    })?;
    run_tdd_validators(&root, project.manifest())
}

pub fn test_installed_harness(
    registry: &HarnessRegistry,
    name: &str,
) -> Result<ValidationResult, CraftError> {
    let installed = registry.info(name)?;
    validate_harness_project(installed.path)
}

fn run_tdd_validators(root: &Path, manifest: &Manifest) -> Result<ValidationResult, CraftError> {
    let tdd_path = root.join(&manifest.validators.tdd);
    let contents = fs::read_to_string(&tdd_path).map_err(|err| {
        CraftError::io(format!("failed to read {}: {err}", tdd_path.display()), err)
    })?;
    if tdd_is_empty(&contents) {
        return Ok(ValidationResult {
            harness_name: manifest.harness.name.clone(),
            tdd_path,
            checks_run: false,
            runner: None,
        });
    }

    let runner = TddRunner::detect()?;
    runner.run(&tdd_path, root)?;
    Ok(ValidationResult {
        harness_name: manifest.harness.name.clone(),
        tdd_path,
        checks_run: true,
        runner: Some(runner.label()),
    })
}

fn tdd_is_empty(contents: &str) -> bool {
    contents.lines().all(|line| {
        let line = line.trim();
        line.is_empty() || line.starts_with('#')
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TddRunner {
    Binary(PathBuf),
    PythonModule(String),
}

impl TddRunner {
    fn detect() -> Result<Self, CraftError> {
        if let Some(path) = find_on_path("tdd-dsl") {
            return Ok(Self::Binary(path));
        }

        let python = find_on_path("python").or_else(|| find_on_path("python3"));
        if let Some(python) = python {
            let output = Command::new(&python)
                .arg("-m")
                .arg("tdd_dsl")
                .arg("--help")
                .output();
            if matches!(output, Ok(output) if output.status.success()) {
                return Ok(Self::PythonModule(python.to_string_lossy().to_string()));
            }
        }

        Err(CraftError::CommandFailed(
            "TDD validators require `tdd-dsl` on PATH or `python -m tdd_dsl`".to_string(),
        ))
    }

    fn run(&self, tdd_path: &Path, cwd: &Path) -> Result<(), CraftError> {
        let output = match self {
            Self::Binary(binary) => Command::new(binary).arg(tdd_path).current_dir(cwd).output(),
            Self::PythonModule(python) => Command::new(python)
                .arg("-m")
                .arg("tdd_dsl")
                .arg(tdd_path)
                .current_dir(cwd)
                .output(),
        }
        .map_err(|err| {
            CraftError::CommandFailed(format!("failed to run {}: {err}", self.label()))
        })?;

        command_output_result(&self.label(), output)
    }

    fn label(&self) -> String {
        match self {
            Self::Binary(binary) => binary.to_string_lossy().to_string(),
            Self::PythonModule(python) => format!("{python} -m tdd_dsl"),
        }
    }
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(binary);
        if candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    })
}

fn command_output_result(label: &str, output: Output) -> Result<(), CraftError> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let details = if !stderr.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    Err(CraftError::CommandFailed(format!(
        "{label} failed{}{}",
        if details.is_empty() { "" } else { ": " },
        details
    )))
}

fn read_harness_artifact(root: &Path, relative_path: &Path) -> Result<String, CraftError> {
    let path = root.join(relative_path);
    fs::read_to_string(&path)
        .map_err(|err| CraftError::io(format!("failed to read {}: {err}", path.display()), err))
}

fn render_compose(artifacts: &[ComposeArtifact], warnings: &[String], strategy: ConflictStrategy) -> String {
    let mut output = String::new();
    output.push_str("# Generated by craft compose\n\n");
    output.push_str("[compose]\n");
    output.push_str(&format!("strategy = \"{}\"\n", strategy.as_str()));
    output.push_str("harnesses = [");
    for (index, artifact) in artifacts.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&quoted(&artifact.manifest.harness.name));
    }
    output.push_str("]\n\n");

    for warning in warnings {
        output.push_str("# warning: ");
        output.push_str(warning);
        output.push('\n');
    }
    if !warnings.is_empty() {
        output.push('\n');
    }

    for artifact in artifacts {
        output.push_str("[[harness]]\n");
        output.push_str("name = ");
        output.push_str(&quoted(&artifact.manifest.harness.name));
        output.push('\n');
        output.push_str("version = ");
        output.push_str(&quoted(&artifact.manifest.harness.version));
        output.push('\n');
        output.push_str("source = ");
        output.push_str(&quoted(&artifact.harness.source));
        output.push('\n');
        output.push_str("path = ");
        output.push_str(&quoted(&artifact.harness.path.to_string_lossy()));
        output.push_str("\n\n");
    }

    output.push_str("[prompts]\n");
    output.push_str("system = ");
    output.push_str(&quoted(&merged_system_prompt(artifacts)));
    output.push_str("\n\n");

    output.push_str("[prompts.sources]\n");
    for artifact in artifacts {
        output.push_str(&quoted_key(&artifact.manifest.harness.name));
        output.push_str(" = ");
        output.push_str(&quoted(&artifact.manifest.prompts.system.to_string_lossy()));
        output.push('\n');
    }
    output.push('\n');

    output.push_str("[memory.schemas]\n");
    match strategy {
        ConflictStrategy::OrderedMerge | ConflictStrategy::Merge => {
            for artifact in artifacts {
                output.push_str(&quoted_key(&artifact.manifest.harness.name));
                output.push_str(" = ");
                output.push_str(&quoted(&artifact.memory_schema));
                output.push('\n');
            }
        }
        ConflictStrategy::Override => {
            if let Some(last) = artifacts.last() {
                output.push_str("_merged = ");
                output.push_str(&quoted(&last.memory_schema));
                output.push('\n');
            }
        }
        ConflictStrategy::Fail => {
            for artifact in artifacts {
                output.push_str(&quoted_key(&artifact.manifest.harness.name));
                output.push_str(" = ");
                output.push_str(&quoted(&artifact.memory_schema));
                output.push('\n');
            }
        }
    }
    output.push('\n');

    output.push_str("[tools.mcp]\n");
    match strategy {
        ConflictStrategy::OrderedMerge | ConflictStrategy::Merge => {
            for artifact in artifacts {
                output.push_str(&quoted_key(&artifact.manifest.harness.name));
                output.push_str(" = ");
                output.push_str(&quoted(&artifact.mcp_tools));
                output.push('\n');
            }
        }
        ConflictStrategy::Override => {
            if let Some(last) = artifacts.last() {
                output.push_str("_merged = ");
                output.push_str(&quoted(&last.mcp_tools));
                output.push('\n');
            }
        }
        ConflictStrategy::Fail => {
            for artifact in artifacts {
                output.push_str(&quoted_key(&artifact.manifest.harness.name));
                output.push_str(" = ");
                output.push_str(&quoted(&artifact.mcp_tools));
                output.push('\n');
            }
        }
    }
    output.push('\n');

    output.push_str("[validators.tdd]\n");
    match strategy {
        ConflictStrategy::OrderedMerge | ConflictStrategy::Merge => {
            for artifact in artifacts {
                output.push_str(&quoted_key(&artifact.manifest.harness.name));
                output.push_str(" = ");
                output.push_str(&quoted(&artifact.tdd_validators));
                output.push('\n');
            }
        }
        ConflictStrategy::Override => {
            if let Some(last) = artifacts.last() {
                output.push_str("_merged = ");
                output.push_str(&quoted(&last.tdd_validators));
                output.push('\n');
            }
        }
        ConflictStrategy::Fail => {
            for artifact in artifacts {
                output.push_str(&quoted_key(&artifact.manifest.harness.name));
                output.push_str(" = ");
                output.push_str(&quoted(&artifact.tdd_validators));
                output.push('\n');
            }
        }
    }
    output.push('\n');

    output
}

fn merged_system_prompt(artifacts: &[ComposeArtifact]) -> String {
    let mut merged = String::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        if index > 0 {
            merged.push_str("\n\n");
        }
        merged.push_str("# Harness: ");
        merged.push_str(&artifact.manifest.harness.name);
        merged.push_str("\n\n");
        merged.push_str(artifact.system_prompt.trim());
        merged.push('\n');
    }
    merged
}

fn run_command<I, S>(binary: &str, args: I, cwd: &Path) -> Result<(), CraftError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(binary)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|err| CraftError::CommandFailed(format!("failed to run {binary}: {err}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CraftError::CommandFailed(format!(
            "{binary} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn validate_slug(label: &str, value: &str) -> Result<(), CraftError> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(CraftError::InvalidSource(format!(
            "github {label} contains invalid characters"
        )));
    }
    Ok(())
}

fn validate_harness_name(name: &str) -> Result<(), CraftError> {
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        Err(CraftError::InvalidName(format!(
            "invalid harness name `{name}`"
        )))
    } else {
        Ok(())
    }
}

fn note_duplicate_harness(
    seen: &mut BTreeMap<String, String>,
    harness_name: &str,
    source: &str,
    warnings: &mut Vec<String>,
) {
    if let Some(previous) = seen.insert(harness_name.to_string(), source.to_string()) {
        warnings.push(format!(
            "harness `{harness_name}` appears more than once; later source `{source}` follows earlier `{previous}`"
        ));
    }
}

fn quoted(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04X}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    format!("\"{escaped}\"")
}

fn quoted_key(value: &str) -> String {
    quoted(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_github_sources() {
        let source = GithubSource::parse("github:JMoak/craft-godot-designer@v0.1.0")
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(source.owner, "JMoak");
        assert_eq!(source.repo, "craft-godot-designer");
        assert_eq!(source.reference, Some("v0.1.0".to_string()));
        assert_eq!(
            source.clone_url(),
            "https://github.com/JMoak/craft-godot-designer.git"
        );
    }

    #[test]
    fn registry_round_trips_harnesses() {
        let root = temp_root("craft-registry");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3"))
            .unwrap_or_else(|err| panic!("{err}"));
        registry
            .upsert(&InstalledHarness {
                name: "godot-designer".to_string(),
                version: "0.1.0".to_string(),
                source: "github:JMoak/craft-godot-designer".to_string(),
                path: root.join("harnesses/godot-designer"),
            })
            .unwrap_or_else(|err| panic!("{err}"));

        let rows = registry.list().unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "godot-designer");

        let info = registry
            .info("godot-designer")
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(info.version, "0.1.0");

        let removed = registry
            .uninstall("godot-designer", false)
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(removed.name, "godot-designer");
        assert!(
            registry
                .list()
                .unwrap_or_else(|err| panic!("{err}"))
                .is_empty()
        );

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn compose_writes_merged_config() {
        let root = temp_root("craft-compose");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3"))
            .unwrap_or_else(|err| panic!("{err}"));
        create_harness(&root, "godot-designer");
        create_harness(&root, "roguelike-specialist");

        for name in ["godot-designer", "roguelike-specialist"] {
            registry
                .upsert(&InstalledHarness {
                    name: name.to_string(),
                    version: "0.1.0".to_string(),
                    source: format!("github:JMoak/craft-{name}"),
                    path: root.join(name),
                })
                .unwrap_or_else(|err| panic!("{err}"));
        }

        let result = compose_harnesses(
            &registry,
            &[
                "godot-designer".to_string(),
                "roguelike-specialist".to_string(),
            ],
            root.join("craft.compose.toml"),
            ConflictStrategy::OrderedMerge,
        )
        .unwrap_or_else(|err| panic!("{err}"));

        let contents =
            fs::read_to_string(&result.output_path).unwrap_or_else(|err| panic!("{err}"));
        assert!(contents.contains("strategy = \"ordered-merge\""));
        assert!(contents.contains("name = \"godot-designer\""));
        assert!(contents.contains("name = \"roguelike-specialist\""));
        assert!(contents.contains("[prompts]"));
        assert!(
            contents.contains("# Harness: godot-designer\\n\\nSystem prompt for godot-designer")
        );
        assert!(contents.contains("[memory.schemas]"));
        assert!(
            contents
                .contains("\"godot-designer\" = \"[facts]\\nowner = \\\"godot-designer\\\"\\n\"")
        );
        assert!(contents.contains("[tools.mcp]"));
        assert!(contents.contains("name = \\\"godot-designer-tools\\\""));
        assert!(contents.contains("[validators.tdd]"));
        assert!(contents.contains("check godot-designer"));
        assert!(result.warnings.is_empty());

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn compose_artifacts_collects_harness_sources() {
        let root = temp_root("craft-compose-artifacts");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3"))
            .unwrap_or_else(|err| panic!("{err}"));
        create_harness(&root, "godot-designer");
        registry
            .upsert(&InstalledHarness {
                name: "godot-designer".to_string(),
                version: "0.1.0".to_string(),
                source: "github:JMoak/craft-godot-designer".to_string(),
                path: root.join("godot-designer"),
            })
            .unwrap_or_else(|err| panic!("{err}"));

        let (artifacts, warnings) =
            collect_compose_artifacts(&registry, &["godot-designer".to_string()])
                .unwrap_or_else(|err| panic!("{err}"));

        assert!(warnings.is_empty());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].manifest.harness.name, "godot-designer");
        assert_eq!(
            artifacts[0].harness.source,
            "github:JMoak/craft-godot-designer"
        );
        assert!(artifacts[0].system_prompt.contains("godot-designer"));
        assert!(
            artifacts[0]
                .memory_schema
                .contains("owner = \"godot-designer\"")
        );
        assert!(artifacts[0].mcp_tools.contains("godot-designer-tools"));
        assert!(artifacts[0].tdd_validators.contains("check godot-designer"));

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn composition_plan_warns_for_duplicate_harness_names() {
        let root = temp_root("craft-compose-duplicates");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3"))
            .unwrap_or_else(|err| panic!("{err}"));
        create_harness(&root, "godot-designer");
        registry
            .upsert(&InstalledHarness {
                name: "godot-designer".to_string(),
                version: "0.1.0".to_string(),
                source: "github:JMoak/craft-godot-designer".to_string(),
                path: root.join("godot-designer"),
            })
            .unwrap_or_else(|err| panic!("{err}"));

        let result = plan_composition(
            &registry,
            &["godot-designer".to_string(), "godot-designer".to_string()],
            ConflictStrategy::OrderedMerge,
        )
        .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(result.harnesses.len(), 2);
        assert!(result.warnings[0].contains("appears more than once"));
        assert!(result.warnings[0].contains("github:JMoak/craft-godot-designer"));

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    fn create_harness(root: &Path, name: &str) {
        let harness_root = root.join(name);
        fs::create_dir_all(harness_root.join("prompts")).unwrap_or_else(|err| panic!("{err}"));
        fs::create_dir_all(harness_root.join("memory")).unwrap_or_else(|err| panic!("{err}"));
        fs::create_dir_all(harness_root.join("tools")).unwrap_or_else(|err| panic!("{err}"));
        fs::create_dir_all(harness_root.join("validators")).unwrap_or_else(|err| panic!("{err}"));
        fs::write(
            harness_root.join("prompts/system.md"),
            format!("System prompt for {name}\n"),
        )
        .unwrap_or_else(|err| panic!("{err}"));
        fs::write(
            harness_root.join("memory/schema.toml"),
            format!("[facts]\nowner = \"{name}\"\n"),
        )
        .unwrap_or_else(|err| panic!("{err}"));
        fs::write(
            harness_root.join("tools/mcp.toml"),
            format!("[[server]]\nname = \"{name}-tools\"\n"),
        )
        .unwrap_or_else(|err| panic!("{err}"));
        fs::write(
            harness_root.join("validators/checks.tdd"),
            format!("check {name}\n"),
        )
        .unwrap_or_else(|err| panic!("{err}"));
        fs::write(
            harness_root.join("craft.toml"),
            format!(
                r#"[harness]
name = "{name}"
version = "0.1.0"
description = "Test harness"
authors = ["JMoak"]

[model]
min_context = 4096
recommended = ["llama3.1:8b"]

[prompts]
system = "prompts/system.md"

[memory]
schema = "memory/schema.toml"

[tools]
mcp = "tools/mcp.toml"

[validators]
tdd = "validators/checks.tdd"
"#
            ),
        )
        .unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn compose_with_override_strategy_uses_last_harness() {
        let root = temp_root("craft-compose-override");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3"))
            .unwrap_or_else(|err| panic!("{err}"));
        create_harness(&root, "godot-designer");
        create_harness(&root, "roguelike-specialist");

        for name in ["godot-designer", "roguelike-specialist"] {
            registry
                .upsert(&InstalledHarness {
                    name: name.to_string(),
                    version: "0.1.0".to_string(),
                    source: format!("github:JMoak/craft-{name}"),
                    path: root.join(name),
                })
                .unwrap_or_else(|err| panic!("{err}"));
        }

        let result = compose_harnesses(
            &registry,
            &[
                "godot-designer".to_string(),
                "roguelike-specialist".to_string(),
            ],
            root.join("craft.compose.toml"),
            ConflictStrategy::Override,
        )
        .unwrap_or_else(|err| panic!("{err}"));

        let contents =
            fs::read_to_string(&result.output_path).unwrap_or_else(|err| panic!("{err}"));
        assert!(contents.contains("strategy = \"override\""));
        assert!(contents.contains("_merged = "));
        assert!(contents.contains("roguelike-specialist"));

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn compose_with_merge_strategy_namespaces_artifacts() {
        let root = temp_root("craft-compose-merge");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3"))
            .unwrap_or_else(|err| panic!("{err}"));
        create_harness(&root, "godot-designer");
        create_harness(&root, "roguelike-specialist");

        for name in ["godot-designer", "roguelike-specialist"] {
            registry
                .upsert(&InstalledHarness {
                    name: name.to_string(),
                    version: "0.1.0".to_string(),
                    source: format!("github:JMoak/craft-{name}"),
                    path: root.join(name),
                })
                .unwrap_or_else(|err| panic!("{err}"));
        }

        let result = compose_harnesses(
            &registry,
            &[
                "godot-designer".to_string(),
                "roguelike-specialist".to_string(),
            ],
            root.join("craft.compose.toml"),
            ConflictStrategy::Merge,
        )
        .unwrap_or_else(|err| panic!("{err}"));

        let contents =
            fs::read_to_string(&result.output_path).unwrap_or_else(|err| panic!("{err}"));
        assert!(contents.contains("strategy = \"merge\""));
        assert!(contents.contains("\"godot-designer\" = \"[facts]"));
        assert!(contents.contains("\"roguelike-specialist\" = \"[facts]"));

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn compose_with_fail_strategy_outputs_namespaced() {
        let root = temp_root("craft-compose-fail");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3"))
            .unwrap_or_else(|err| panic!("{err}"));
        create_harness(&root, "godot-designer");
        create_harness(&root, "roguelike-specialist");

        for name in ["godot-designer", "roguelike-specialist"] {
            registry
                .upsert(&InstalledHarness {
                    name: name.to_string(),
                    version: "0.1.0".to_string(),
                    source: format!("github:JMoak/craft-{name}"),
                    path: root.join(name),
                })
                .unwrap_or_else(|err| panic!("{err}"));
        }

        let result = compose_harnesses(
            &registry,
            &[
                "godot-designer".to_string(),
                "roguelike-specialist".to_string(),
            ],
            root.join("craft.compose.toml"),
            ConflictStrategy::Fail,
        )
        .unwrap_or_else(|err| panic!("{err}"));

        let contents =
            fs::read_to_string(&result.output_path).unwrap_or_else(|err| panic!("{err}"));
        assert!(contents.contains("strategy = \"fail\""));
        assert!(contents.contains("\"godot-designer\" = \"[facts]"));
        assert!(contents.contains("\"roguelike-specialist\" = \"[facts]"));

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn conflict_strategy_from_string_is_case_sensitive() {
        assert_eq!(ConflictStrategy::from_string("ordered-merge"), Some(ConflictStrategy::OrderedMerge));
        assert_eq!(ConflictStrategy::from_string("merge"), Some(ConflictStrategy::Merge));
        assert_eq!(ConflictStrategy::from_string("override"), Some(ConflictStrategy::Override));
        assert_eq!(ConflictStrategy::from_string("fail"), Some(ConflictStrategy::Fail));
        assert_eq!(ConflictStrategy::from_string("MERGE"), None);
        assert_eq!(ConflictStrategy::from_string("unknown"), None);
    }

    #[test]
    fn conflict_strategy_as_str_returns_expected_values() {
        assert_eq!(ConflictStrategy::OrderedMerge.as_str(), "ordered-merge");
        assert_eq!(ConflictStrategy::Merge.as_str(), "merge");
        assert_eq!(ConflictStrategy::Override.as_str(), "override");
        assert_eq!(ConflictStrategy::Fail.as_str(), "fail");
    }

    fn temp_root(prefix: &str) -> PathBuf {

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|err| panic!("{err}"))
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{unique}"))
    }

    #[test]
    fn registry_multiple_versions() {
        let root = temp_root("craft-registry-multi");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3")).unwrap();

        // Install first version
        registry.upsert(&InstalledHarness {
            name: "test-harness".to_string(),
            version: "1.0.0".to_string(),
            source: "github:test/repo".to_string(),
            path: root.join("v1"),
        }).unwrap();

        // Install second version
        registry.upsert(&InstalledHarness {
            name: "test-harness".to_string(),
            version: "1.1.0".to_string(),
            source: "github:test/repo".to_string(),
            path: root.join("v1.1"),
        }).unwrap();

        // Both versions should be retrievable
        let versions = registry.list_versions("test-harness").unwrap();
        assert_eq!(versions.len(), 2);

        // Default should be the first installed
        let default = registry.info("test-harness").unwrap();
        assert_eq!(default.version, "1.0.0");

        // But we can set a different default
        registry.set_default_version("test-harness", "1.1.0").unwrap();
        let new_default = registry.info("test-harness").unwrap();
        assert_eq!(new_default.version, "1.1.0");

        // Can query specific version
        let specific = registry.info_version("test-harness", "1.0.0").unwrap();
        assert_eq!(specific.version, "1.0.0");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registry_find_version_with_constraint() {
        let root = temp_root("craft-registry-constraint");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3")).unwrap();

        // Install multiple versions
        for (version, path) in [
            ("1.0.0", root.join("v1")),
            ("1.2.0", root.join("v1.2")),
            ("1.5.0", root.join("v1.5")),
            ("2.0.0", root.join("v2")),
        ] {
            registry.upsert(&InstalledHarness {
                name: "dep-harness".to_string(),
                version: version.to_string(),
                source: "github:owner/dep".to_string(),
                path,
            }).unwrap();
        }

        // Find matching version
        let constraint = VersionConstraint::parse("^1.0.0").unwrap();
        let found = registry.find_version("dep-harness", &constraint).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().version, "1.5.0"); // Highest matching

        // Constraint that doesn't match 2.x
        let compatibility = VersionConstraint::parse("^1.2.0").unwrap();
        let found_compat = registry.find_version("dep-harness", &compatibility).unwrap();
        assert!(found_compat.is_some());
        assert_eq!(found_compat.unwrap().version, "1.5.0");

        // No match
        let no_match = VersionConstraint::parse("^3.0.0").unwrap();
        let found_none = registry.find_version("dep-harness", &no_match).unwrap();
        assert!(found_none.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registry_uninstall_version_updates_default() {
        let root = temp_root("craft-uninstall-update");
        let registry = HarnessRegistry::open(root.join("registry.sqlite3")).unwrap();

        // Install two versions
        registry.upsert(&InstalledHarness {
            name: "multi-version".to_string(),
            version: "1.0.0".to_string(),
            source: "src1".to_string(),
            path: root.join("v1"),
        }).unwrap();

        registry.upsert(&InstalledHarness {
            name: "multi-version".to_string(),
            version: "2.0.0".to_string(),
            source: "src2".to_string(),
            path: root.join("v2"),
        }).unwrap();

        // Verify both versions exist
        let all_versions = registry.list_versions("multi-version").unwrap();
        assert_eq!(all_versions.len(), 2, "Expected 2 versions but found {}", all_versions.len());

        // Set v2 as default
        registry.set_default_version("multi-version", "2.0.0").unwrap();
        let current_default = registry.get_default_version("multi-version").unwrap();
        assert_eq!(current_default, Some("2.0.0".to_string()), "Default should be 2.0.0");

        // Uninstall v2
        registry.uninstall_version("multi-version", Some("2.0.0"), false).unwrap();

        // Default should fall back to v1
        let remaining = registry.list_versions("multi-version").unwrap();
        assert_eq!(remaining.len(), 1, "Expected 1 remaining version");
        assert_eq!(remaining[0].version, "1.0.0");

        let new_default = registry.get_default_version("multi-version").unwrap();
        assert_eq!(new_default, Some("1.0.0".to_string()), "Default should be 1.0.0 after uninstalling 2.0.0");

        fs::remove_dir_all(root).unwrap();
    }
}
