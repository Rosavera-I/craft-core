use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
struct ComposeArtifact {
    harness: InstalledHarness,
    manifest: Manifest,
    system_prompt: String,
    memory_schema: String,
    mcp_tools: String,
    tdd_validators: String,
}

#[derive(Debug)]
pub enum CraftError {
    Config(String),
    InvalidSource(String),
    InvalidName(String),
    MissingHarness(String),
    Io(String),
    Manifest(ManifestError),
    CommandFailed(String),
    Registry(String),
}

impl fmt::Display for CraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CraftError::Config(message)
            | CraftError::InvalidSource(message)
            | CraftError::InvalidName(message)
            | CraftError::MissingHarness(message)
            | CraftError::Io(message)
            | CraftError::CommandFailed(message)
            | CraftError::Registry(message) => write!(f, "{message}"),
            CraftError::Manifest(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CraftError {}

impl From<std::io::Error> for CraftError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<ManifestError> for CraftError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

pub struct HarnessManager {
    home: CraftHome,
}

impl HarnessManager {
    pub fn new(home: CraftHome) -> Self {
        Self { home }
    }

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
            name: manifest.harness.name,
            version: manifest.harness.version,
            source: source.source_id(),
            path: checkout,
        };
        let registry = HarnessRegistry::open(self.home.registry_path())?;
        registry.upsert(&installed)?;
        Ok(InstallResult { harness: installed })
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
        registry.exec(
            "CREATE TABLE IF NOT EXISTS harnesses (
                name TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                source TEXT NOT NULL,
                path TEXT NOT NULL,
                installed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )?;
        Ok(registry)
    }

    pub fn upsert(&self, harness: &InstalledHarness) -> Result<(), CraftError> {
        let sql = format!(
            "INSERT INTO harnesses (name, version, source, path)
             VALUES ({}, {}, {}, {})
             ON CONFLICT(name) DO UPDATE SET
                version = excluded.version,
                source = excluded.source,
                path = excluded.path,
                installed_at = CURRENT_TIMESTAMP;",
            sql_string(&harness.name),
            sql_string(&harness.version),
            sql_string(&harness.source),
            sql_string(&harness.path.to_string_lossy()),
        );
        self.exec(&sql)
    }

    pub fn list(&self) -> Result<Vec<InstalledHarness>, CraftError> {
        let output =
            self.query("SELECT name, version, source, path FROM harnesses ORDER BY name;")?;
        parse_registry_rows(&output)
    }

    pub fn info(&self, name: &str) -> Result<InstalledHarness, CraftError> {
        validate_harness_name(name)?;
        let output = self.query(&format!(
            "SELECT name, version, source, path FROM harnesses WHERE name = {};",
            sql_string(name)
        ))?;
        let mut rows = parse_registry_rows(&output)?;
        rows.pop()
            .ok_or_else(|| CraftError::MissingHarness(format!("harness `{name}` is not installed")))
    }

    pub fn uninstall(
        &self,
        name: &str,
        remove_files: bool,
    ) -> Result<InstalledHarness, CraftError> {
        let harness = self.info(name)?;
        self.exec(&format!(
            "DELETE FROM harnesses WHERE name = {};",
            sql_string(name)
        ))?;
        if remove_files && harness.path.exists() {
            fs::remove_dir_all(&harness.path)?;
        }
        Ok(harness)
    }

    fn exec(&self, sql: &str) -> Result<(), CraftError> {
        let status = Command::new("sqlite3")
            .arg(&self.path)
            .arg(sql)
            .status()
            .map_err(|err| CraftError::Registry(format!("failed to run sqlite3: {err}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Registry(format!(
                "sqlite3 exited with status {status}"
            )))
        }
    }

    fn query(&self, sql: &str) -> Result<String, CraftError> {
        let output = Command::new("sqlite3")
            .arg("-batch")
            .arg("-separator")
            .arg("\t")
            .arg(&self.path)
            .arg(sql)
            .output()
            .map_err(|err| CraftError::Registry(format!("failed to run sqlite3: {err}")))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(CraftError::Registry(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }
}

pub fn compose_harnesses(
    registry: &HarnessRegistry,
    harness_names: &[String],
    output_path: impl AsRef<Path>,
) -> Result<ComposeResult, CraftError> {
    if harness_names.is_empty() {
        return Err(CraftError::InvalidName(
            "compose requires at least one harness".to_string(),
        ));
    }

    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let mut prompts = BTreeMap::new();
    let mut memory_schemas = BTreeMap::new();
    let mut tool_bindings = BTreeMap::new();
    let mut validators = BTreeMap::new();

    for name in harness_names {
        let installed = registry.info(name)?;
        let manifest = load_manifest(installed.path.join("craft.toml"))?;
        note_conflict(
            &mut prompts,
            "prompts.system",
            &manifest.harness.name,
            &mut warnings,
        );
        note_conflict(
            &mut memory_schemas,
            "memory.schemas",
            &manifest.harness.name,
            &mut warnings,
        );
        note_conflict(
            &mut tool_bindings,
            "tools.mcp",
            &manifest.harness.name,
            &mut warnings,
        );
        note_conflict(
            &mut validators,
            "validators.tdd",
            &manifest.harness.name,
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

    let contents = render_compose(&artifacts, &warnings);
    let output_path = output_path.as_ref().to_path_buf();
    fs::write(&output_path, contents)?;
    Ok(ComposeResult {
        output_path,
        warnings,
    })
}

fn read_harness_artifact(root: &Path, relative_path: &Path) -> Result<String, CraftError> {
    let path = root.join(relative_path);
    fs::read_to_string(&path)
        .map_err(|err| CraftError::Io(format!("failed to read {}: {err}", path.display())))
}

fn render_compose(artifacts: &[ComposeArtifact], warnings: &[String]) -> String {
    let mut output = String::new();
    output.push_str("# Generated by craft compose\n\n");
    output.push_str("[compose]\n");
    output.push_str("strategy = \"ordered-merge\"\n");
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
    for artifact in artifacts {
        output.push_str(&quoted_key(&artifact.manifest.harness.name));
        output.push_str(" = ");
        output.push_str(&quoted(&artifact.memory_schema));
        output.push('\n');
    }
    output.push('\n');

    output.push_str("[tools.mcp]\n");
    for artifact in artifacts {
        output.push_str(&quoted_key(&artifact.manifest.harness.name));
        output.push_str(" = ");
        output.push_str(&quoted(&artifact.mcp_tools));
        output.push('\n');
    }
    output.push('\n');

    output.push_str("[validators.tdd]\n");
    for artifact in artifacts {
        output.push_str(&quoted_key(&artifact.manifest.harness.name));
        output.push_str(" = ");
        output.push_str(&quoted(&artifact.tdd_validators));
        output.push('\n');
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

fn parse_registry_rows(output: &str) -> Result<Vec<InstalledHarness>, CraftError> {
    let mut rows = Vec::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(CraftError::Registry(format!(
                "unexpected registry row shape: {line}"
            )));
        }
        rows.push(InstalledHarness {
            name: parts[0].to_string(),
            version: parts[1].to_string(),
            source: parts[2].to_string(),
            path: PathBuf::from(parts[3]),
        });
    }
    Ok(rows)
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

fn note_conflict(
    seen: &mut BTreeMap<String, String>,
    key: &str,
    harness_name: &str,
    warnings: &mut Vec<String>,
) {
    if let Some(previous) = seen.insert(key.to_string(), harness_name.to_string()) {
        warnings.push(format!(
            "`{key}` from `{harness_name}` overrides earlier value from `{previous}`"
        ));
    }
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
    fn compose_writes_merged_config_with_warnings() {
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
        assert!(!result.warnings.is_empty());

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

    fn temp_root(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|err| panic!("{err}"))
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{unique}"))
    }
}
