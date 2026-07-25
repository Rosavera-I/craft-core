use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub harness: Harness,
    pub package: PackageMetadata,
    pub dependencies: BTreeMap<String, String>,
    pub entry_points: BTreeMap<String, PathBuf>,
    pub model: Model,
    pub prompts: Prompts,
    pub memory: Memory,
    pub tools: Tools,
    pub validators: Validators,
}

/// Optional distribution metadata carried with a published harness package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageMetadata {
    pub license: Option<String>,
    pub repository: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Harness {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub min_context: u32,
    pub recommended: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompts {
    pub system: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memory {
    pub schema: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    pub mcp: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validators {
    pub tdd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Io(String),
    Parse(String),
    MissingField(&'static str),
    InvalidSemver(String),
    InvalidValue(String),
    MissingPath(PathBuf),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Io(message) => write!(f, "{message}"),
            ManifestError::Parse(message) => write!(f, "{message}"),
            ManifestError::MissingField(field) => write!(f, "missing required field `{field}`"),
            ManifestError::InvalidSemver(version) => {
                write!(f, "invalid semantic version `{version}`")
            }
            ManifestError::InvalidValue(message) => write!(f, "{message}"),
            ManifestError::MissingPath(path) => {
                write!(f, "manifest references missing path `{}`", path.display())
            }
        }
    }
}

impl std::error::Error for ManifestError {}

pub fn load_manifest(path: impl AsRef<Path>) -> Result<Manifest, ManifestError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)
        .map_err(|err| ManifestError::Io(format!("failed to read {}: {err}", path.display())))?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    parse_manifest(&contents)?.validate(base_dir)
}

pub fn parse_manifest(input: &str) -> Result<Manifest, ManifestError> {
    let table = parse_tables(input)?;

    let manifest = Manifest {
        harness: Harness {
            name: required_string(&table, "harness", "name")?,
            version: required_string(&table, "harness", "version")?,
            description: required_string(&table, "harness", "description")?,
            authors: required_array(&table, "harness", "authors")?,
        },
        package: PackageMetadata {
            license: optional_string(&table, "package", "license")?,
            repository: optional_string(&table, "package", "repository")?,
            keywords: optional_array(&table, "package", "keywords")?.unwrap_or_default(),
        },
        dependencies: string_table(&table, "dependencies")?,
        entry_points: string_table(&table, "entrypoints")?
            .into_iter()
            .map(|(name, path)| (name, PathBuf::from(path)))
            .collect(),
        model: Model {
            min_context: required_u32(&table, "model", "min_context")?,
            recommended: required_array(&table, "model", "recommended")?,
        },
        prompts: Prompts {
            system: PathBuf::from(required_string(&table, "prompts", "system")?),
        },
        memory: Memory {
            schema: PathBuf::from(required_string(&table, "memory", "schema")?),
        },
        tools: Tools {
            mcp: PathBuf::from(required_string(&table, "tools", "mcp")?),
        },
        validators: Validators {
            tdd: PathBuf::from(required_string(&table, "validators", "tdd")?),
        },
    };

    validate_semver(&manifest.harness.version)?;
    for (dependency, requirement) in &manifest.dependencies {
        validate_dependency_name(dependency)?;
        semver::VersionReq::parse(requirement).map_err(|err| {
            ManifestError::InvalidValue(format!(
                "invalid version requirement `{requirement}` for dependency `{dependency}`: {err}"
            ))
        })?;
    }
    for (name, path) in &manifest.entry_points {
        validate_non_empty("entrypoints name", name)?;
        validate_package_path(path)?;
    }
    Ok(manifest)
}

impl Manifest {
    pub fn validate(self, base_dir: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let base_dir = base_dir.as_ref();
        validate_name(&self.harness.name)?;
        validate_semver(&self.harness.version)?;
        validate_non_empty("harness.description", &self.harness.description)?;
        if self.harness.authors.is_empty() {
            return Err(ManifestError::InvalidValue(
                "harness.authors must contain at least one author".to_string(),
            ));
        }
        if self.model.min_context == 0 {
            return Err(ManifestError::InvalidValue(
                "model.min_context must be greater than zero".to_string(),
            ));
        }
        if self.model.recommended.is_empty() {
            return Err(ManifestError::InvalidValue(
                "model.recommended must contain at least one model".to_string(),
            ));
        }

        for relative_path in self.entry_points.values() {
            let full_path = base_dir.join(relative_path);
            if !full_path.is_file() {
                return Err(ManifestError::MissingPath(full_path));
            }
        }

        for relative_path in [
            &self.prompts.system,
            &self.memory.schema,
            &self.tools.mcp,
            &self.validators.tdd,
        ] {
            validate_package_path(relative_path)?;
            let full_path = base_dir.join(relative_path);
            if !full_path.exists() {
                return Err(ManifestError::MissingPath(full_path));
            }
        }

        Ok(self)
    }
}

type TableMap = BTreeMap<String, BTreeMap<String, String>>;

fn parse_tables(input: &str) -> Result<TableMap, ManifestError> {
    let mut tables: TableMap = BTreeMap::new();
    let mut section: Option<String> = None;

    for (line_index, raw_line) in input.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let name = name.trim();
            if name.is_empty() {
                return Err(parse_error(line_index, "empty table name"));
            }
            section = Some(name.to_string());
            tables.entry(name.to_string()).or_default();
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(parse_error(line_index, "expected `key = value`"));
        };

        let Some(section_name) = &section else {
            return Err(parse_error(line_index, "field appears before any table"));
        };

        tables
            .entry(section_name.clone())
            .or_default()
            .insert(key.trim().to_string(), value.trim().to_string());
    }

    Ok(tables)
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (index, character) in line.char_indices() {
        match character {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

fn required_string(
    tables: &TableMap,
    section: &'static str,
    key: &'static str,
) -> Result<String, ManifestError> {
    let value = required_raw(tables, section, key)?;
    parse_string(value)
        .ok_or_else(|| ManifestError::Parse(format!("`{section}.{key}` must be a quoted string")))
}

fn required_array(
    tables: &TableMap,
    section: &'static str,
    key: &'static str,
) -> Result<Vec<String>, ManifestError> {
    let value = required_raw(tables, section, key)?;
    parse_array(value).ok_or_else(|| {
        ManifestError::Parse(format!(
            "`{section}.{key}` must be an array of quoted strings"
        ))
    })
}

fn optional_string(
    tables: &TableMap,
    section: &str,
    key: &str,
) -> Result<Option<String>, ManifestError> {
    tables
        .get(section)
        .and_then(|fields| fields.get(key))
        .map(|value| {
            parse_string(value).ok_or_else(|| {
                ManifestError::Parse(format!("`{section}.{key}` must be a quoted string"))
            })
        })
        .transpose()
}

fn optional_array(
    tables: &TableMap,
    section: &str,
    key: &str,
) -> Result<Option<Vec<String>>, ManifestError> {
    tables
        .get(section)
        .and_then(|fields| fields.get(key))
        .map(|value| {
            parse_array(value).ok_or_else(|| {
                ManifestError::Parse(format!(
                    "`{section}.{key}` must be an array of quoted strings"
                ))
            })
        })
        .transpose()
}

fn string_table(
    tables: &TableMap,
    section: &str,
) -> Result<BTreeMap<String, String>, ManifestError> {
    tables
        .get(section)
        .map(|fields| {
            fields
                .iter()
                .map(|(key, value)| {
                    parse_string(value)
                        .map(|value| (parse_table_key(key), value))
                        .ok_or_else(|| {
                            ManifestError::Parse(format!(
                                "`{section}.{key}` must be a quoted string"
                            ))
                        })
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(BTreeMap::new()))
}

fn parse_table_key(key: &str) -> String {
    parse_string(key).unwrap_or_else(|| key.to_string())
}

fn required_u32(
    tables: &TableMap,
    section: &'static str,
    key: &'static str,
) -> Result<u32, ManifestError> {
    let value = required_raw(tables, section, key)?;
    value
        .parse::<u32>()
        .map_err(|_| ManifestError::Parse(format!("`{section}.{key}` must be an integer")))
}

fn required_raw<'a>(
    tables: &'a TableMap,
    section: &'static str,
    key: &'static str,
) -> Result<&'a str, ManifestError> {
    tables
        .get(section)
        .and_then(|fields| fields.get(key))
        .map(String::as_str)
        .ok_or(ManifestError::MissingField(match (section, key) {
            ("harness", "name") => "harness.name",
            ("harness", "version") => "harness.version",
            ("harness", "description") => "harness.description",
            ("harness", "authors") => "harness.authors",
            ("model", "min_context") => "model.min_context",
            ("model", "recommended") => "model.recommended",
            ("prompts", "system") => "prompts.system",
            ("memory", "schema") => "memory.schema",
            ("tools", "mcp") => "tools.mcp",
            ("validators", "tdd") => "validators.tdd",
            _ => "unknown",
        }))
}

fn parse_string(value: &str) -> Option<String> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(str::to_string)
}

fn parse_array(value: &str) -> Option<Vec<String>> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }

    inner
        .split(',')
        .map(|item| parse_string(item.trim()))
        .collect::<Option<Vec<_>>>()
}

fn validate_semver(version: &str) -> Result<(), ManifestError> {
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
    {
        Ok(())
    } else {
        Err(ManifestError::InvalidSemver(version.to_string()))
    }
}

fn validate_name(name: &str) -> Result<(), ManifestError> {
    validate_non_empty("harness.name", name)?;
    if name.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }) {
        Ok(())
    } else {
        Err(ManifestError::InvalidValue(
            "harness.name must use lowercase letters, digits, and hyphens".to_string(),
        ))
    }
}

fn validate_dependency_name(name: &str) -> Result<(), ManifestError> {
    let Some((org, harness)) = name.split_once('/') else {
        return Err(ManifestError::InvalidValue(format!(
            "dependency `{name}` must use org/name coordinates"
        )));
    };
    if [org, harness].iter().all(|part| {
        !part.is_empty()
            && part.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '-'
                    || character == '_'
            })
    }) {
        Ok(())
    } else {
        Err(ManifestError::InvalidValue(format!(
            "dependency `{name}` contains an invalid package coordinate"
        )))
    }
}

fn validate_package_path(path: &Path) -> Result<(), ManifestError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        Err(ManifestError::InvalidValue(format!(
            "package path `{}` must be relative and stay within the package",
            path.display()
        )))
    } else {
        Ok(())
    }
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        Err(ManifestError::InvalidValue(format!(
            "`{field}` must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn parse_error(line_index: usize, message: &str) -> ManifestError {
    ManifestError::Parse(format!("line {}: {message}", line_index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const VALID_MANIFEST: &str = r#"
[harness]
name = "godot-designer"
version = "0.1.0"
description = "Godot 4 expertise harness"
authors = ["JMoak"]

[package]
license = "MIT"
repository = "https://example.com/godot-designer"
keywords = ["godot", "design"]

[dependencies]
"acme/tdd-architect" = "^1.2.0"

[entrypoints]
system = "prompts/system.md"

[model]
min_context = 4096
recommended = ["llama3.1:8b", "qwen2.5:7b"]

[prompts]
system = "prompts/system.md"

[memory]
schema = "memory/schema.toml"

[tools]
mcp = "tools/mcp.toml"

[validators]
tdd = "validators/checks.tdd"
"#;

    #[test]
    fn parses_valid_manifest() {
        let manifest = parse_manifest(VALID_MANIFEST).unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(manifest.harness.name, "godot-designer");
        assert_eq!(manifest.harness.version, "0.1.0");
        assert_eq!(manifest.model.min_context, 4096);
        assert_eq!(manifest.package.license.as_deref(), Some("MIT"));
        assert_eq!(
            manifest
                .dependencies
                .get("acme/tdd-architect")
                .map(String::as_str),
            Some("^1.2.0")
        );
        assert_eq!(
            manifest.entry_points.get("system"),
            Some(&PathBuf::from("prompts/system.md"))
        );
        assert_eq!(
            manifest.model.recommended,
            vec!["llama3.1:8b".to_string(), "qwen2.5:7b".to_string()]
        );
    }

    #[test]
    fn rejects_missing_required_field() {
        let input = VALID_MANIFEST.replace("name = \"godot-designer\"\n", "");
        let error = match parse_manifest(&input) {
            Ok(_) => panic!("expected manifest parsing to fail"),
            Err(error) => error,
        };

        assert_eq!(error, ManifestError::MissingField("harness.name"));
    }

    #[test]
    fn rejects_invalid_semver() {
        let input = VALID_MANIFEST.replace("version = \"0.1.0\"", "version = \"first\"");
        let error = match parse_manifest(&input) {
            Ok(_) => panic!("expected manifest parsing to fail"),
            Err(error) => error,
        };

        assert_eq!(error, ManifestError::InvalidSemver("first".to_string()));
    }

    #[test]
    fn rejects_invalid_dependency_requirement() {
        let input = VALID_MANIFEST.replace("^1.2.0", "definitely-not-semver");
        let error = match parse_manifest(&input) {
            Ok(_) => panic!("invalid dependency requirement should fail validation"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("invalid version requirement"));
    }

    #[test]
    fn rejects_entrypoint_outside_package() {
        let input = VALID_MANIFEST.replace(
            "system = \"prompts/system.md\"",
            "system = \"../outside.md\"",
        );
        let error = match parse_manifest(&input) {
            Ok(_) => panic!("path traversal should fail validation"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("stay within the package"));
    }

    #[test]
    fn validates_referenced_paths() {
        let root = temp_root("craft-manifest-validates");
        fs::create_dir_all(root.join("prompts")).unwrap_or_else(|err| panic!("{err}"));
        fs::create_dir_all(root.join("memory")).unwrap_or_else(|err| panic!("{err}"));
        fs::create_dir_all(root.join("tools")).unwrap_or_else(|err| panic!("{err}"));
        fs::create_dir_all(root.join("validators")).unwrap_or_else(|err| panic!("{err}"));
        fs::write(root.join("prompts/system.md"), "").unwrap_or_else(|err| panic!("{err}"));
        fs::write(root.join("memory/schema.toml"), "").unwrap_or_else(|err| panic!("{err}"));
        fs::write(root.join("tools/mcp.toml"), "").unwrap_or_else(|err| panic!("{err}"));
        fs::write(root.join("validators/checks.tdd"), "").unwrap_or_else(|err| panic!("{err}"));

        parse_manifest(VALID_MANIFEST)
            .unwrap_or_else(|err| panic!("{err}"))
            .validate(&root)
            .unwrap_or_else(|err| panic!("{err}"));

        fs::remove_dir_all(root).unwrap_or_else(|err| panic!("{err}"));
    }

    fn temp_root(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|err| panic!("{err}"))
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}"))
    }
}
