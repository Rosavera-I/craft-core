//! Semver version resolution for harness dependencies.
//!
//! This module provides:
//! - Semver range parsing (e.g., `^1.2.0`, `>=1.0.0 <2.0.0`)
//! - Version constraint satisfaction checking
//! - Conflict resolution when multiple harnesses depend on different versions

use semver::{Version, VersionReq};
use std::collections::{BTreeMap, HashMap};

/// A version requirement that can be parsed from a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionConstraint {
    /// The underlying semver version requirement
    pub req: VersionReq,
    /// The original string representation
    pub original: String,
}

impl VersionConstraint {
    /// Parse a version constraint from a string.
    ///
    /// Supports:
    /// - Exact versions: `1.2.3`
    /// - Caret ranges: `^1.2.0` (compatible with >=1.2.0 <2.0.0)
    /// - Tilde ranges: `~1.2.0` (compatible with >=1.2.0 <1.3.0)
    /// - Comparison operators: `>=1.0.0`, `<2.0.0`, `>1.0.0`, `<=2.0.0`
    /// - Wildcards: `1.x`, `1.2.x`
    /// - Combined ranges: `>=1.0.0 <2.0.0`
    pub fn parse(input: &str) -> Result<Self, VersionError> {
        let trimmed = input.trim();

        // Handle npm-style caret (^) prefix
        if let Some(rest) = trimmed.strip_prefix('^') {
            let version_str = rest.trim();
            let version = parse_version(version_str)?;
            // ^1.2.3 := >=1.2.3 <2.0.0
            // ^0.2.3 := >=0.2.3 <0.3.0
            // ^0.0.3 := >=0.0.3 <0.0.4
            let major = version.major;
            let minor = version.minor;
            let patch = version.patch;
            let req_str = if major == 0 {
                if minor == 0 {
                    format!(">={}.{}.{},<0.0.{}", major, minor, patch, patch + 1)
                } else {
                    format!(">={}.{}.{},<0.{}.{}", major, minor, patch, minor + 1, 0)
                }
            } else {
                format!(">={}.{}.{},<{}.{}.{}", major, minor, patch, major + 1, 0, 0)
            };
            return Ok(Self {
                req: VersionReq::parse(&req_str).map_err(|e| VersionError::InvalidRange {
                    input: trimmed.to_string(),
                    reason: e.to_string(),
                })?,
                original: trimmed.to_string(),
            });
        }

        // Handle npm-style tilde (~) prefix
        if let Some(rest) = trimmed.strip_prefix('~') {
            let version_str = rest.trim();
            let version = parse_version(version_str)?;
            // ~1.2.3 := >=1.2.3 <1.3.0
            let major = version.major;
            let minor = version.minor;
            let patch = version.patch;
            let req_str = format!(">={}.{}.{},<{}.{}.{}", major, minor, patch, major, minor + 1, 0);
            return Ok(Self {
                req: VersionReq::parse(&req_str).map_err(|e| VersionError::InvalidRange {
                    input: trimmed.to_string(),
                    reason: e.to_string(),
                })?,
                original: trimmed.to_string(),
            });
        }

        // Handle wildcards like "1.x" or "1.2.x"
        if trimmed.ends_with(".x") || trimmed.ends_with(".X") || trimmed.ends_with(".*") {
            let parts: Vec<&str> = trimmed.split('.').collect();
            let req_str = match parts.len() {
                2 => {
                    // 1.x means >=1.0.0 <2.0.0
                    let major: u64 = parts[0].parse().map_err(|_| VersionError::InvalidRange {
                        input: trimmed.to_string(),
                        reason: "invalid major version".to_string(),
                    })?;
                    format!(">={}.{}.{},<{}.{}.{}", major, 0, 0, major + 1, 0, 0)
                }
                3 => {
                    // 1.2.x means >=1.2.0 <1.3.0
                    let major: u64 = parts[0].parse().map_err(|_| VersionError::InvalidRange {
                        input: trimmed.to_string(),
                        reason: "invalid major version".to_string(),
                    })?;
                    let minor: u64 = parts[1].parse().map_err(|_| VersionError::InvalidRange {
                        input: trimmed.to_string(),
                        reason: "invalid minor version".to_string(),
                    })?;
                    format!(">={}.{}.{},<{}.{}.{}", major, minor, 0, major, minor + 1, 0)
                }
                _ => {
                    return Err(VersionError::InvalidRange {
                        input: trimmed.to_string(),
                        reason: "invalid wildcard format".to_string(),
                    })
                }
            };
            return Ok(Self {
                req: VersionReq::parse(&req_str).map_err(|e| VersionError::InvalidRange {
                    input: trimmed.to_string(),
                    reason: e.to_string(),
                })?,
                original: trimmed.to_string(),
            });
        }

        // Try parsing as standard version requirement
        let req = if trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            // Starts with digit, treat as exact version
            let version = parse_version(trimmed)?;
            VersionReq::parse(&format!(">={}", version))
                .map_err(|e| VersionError::InvalidRange {
                    input: trimmed.to_string(),
                    reason: e.to_string(),
                })?
        } else {
            VersionReq::parse(trimmed).map_err(|e| VersionError::InvalidRange {
                input: trimmed.to_string(),
                reason: e.to_string(),
            })?
        };

        Ok(Self {
            req,
            original: trimmed.to_string(),
        })
    }

    /// Check if a version satisfies this constraint.
    pub fn matches(&self, version: &Version) -> bool {
        self.req.matches(version)
    }

    /// Get the underlying version requirement.
    pub fn as_req(&self) -> &VersionReq {
        &self.req
    }
}

/// Parse a version string into a Version.
fn parse_version(input: &str) -> Result<Version, VersionError> {
    Version::parse(input.trim()).map_err(|e| VersionError::InvalidVersion {
        input: input.to_string(),
        reason: e.to_string(),
    })
}

/// Errors that can occur during version parsing or resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    InvalidVersion { input: String, reason: String },
    InvalidRange { input: String, reason: String },
    NoMatchingVersion { name: String, constraint: String },
    Conflict { message: String },
}

impl std::fmt::Display for VersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidVersion { input, reason } => {
                write!(f, "invalid version '{}': {}", input, reason)
            }
            Self::InvalidRange { input, reason } => {
                write!(f, "invalid version range '{}': {}", input, reason)
            }
            Self::NoMatchingVersion { name, constraint } => {
                write!(
                    f,
                    "no matching version of '{}' found for constraint '{}'",
                    name, constraint
                )
            }
            Self::Conflict { message } => write!(f, "version conflict: {}", message),
        }
    }
}

impl std::error::Error for VersionError {}

/// A harness dependency with a version constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessDependency {
    /// The name of the harness
    pub name: String,
    /// The version constraint
    pub constraint: VersionConstraint,
    /// The source of this dependency (which harness requires it)
    pub requested_by: String,
}

/// A resolved harness version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVersion {
    /// The harness name
    pub name: String,
    /// The resolved version
    pub version: Version,
    /// The source URL/path
    pub source: String,
    /// Which harnesses requested this dependency
    pub requested_by: Vec<String>,
}

/// A registry that supports versioned harnesses.
#[derive(Debug, Default)]
pub struct VersionResolver {
    /// Available versions for each harness name
    available: BTreeMap<String, Vec<VersionedHarness>>,
}

/// A versioned harness entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedHarness {
    pub version: Version,
    pub source: String,
    pub path: String,
}

impl VersionResolver {
    /// Create a new empty resolver.
    pub fn new() -> Self {
        Self {
            available: BTreeMap::new(),
        }
    }

    /// Register an available harness version.
    pub fn add_version(
        &mut self,
        name: impl Into<String>,
        version: Version,
        source: impl Into<String>,
        path: impl Into<String>,
    ) {
        let name = name.into();
        let entry = VersionedHarness {
            version,
            source: source.into(),
            path: path.into(),
        };
        self.available.entry(name).or_default().push(entry);
    }

    /// Find the best matching version for a constraint.
    pub fn find_best_match(
        &self,
        name: &str,
        constraint: &VersionConstraint,
    ) -> Result<Option<VersionedHarness>, VersionError> {
        let versions = self
            .available
            .get(name)
            .ok_or_else(|| VersionError::NoMatchingVersion {
                name: name.to_string(),
                constraint: constraint.original.clone(),
            })?;

        // Sort by version descending to get highest matching version first
        let mut matches: Vec<_> = versions.iter().filter(|v| constraint.matches(&v.version)).collect();
        matches.sort_by(|a, b| b.version.cmp(&a.version));

        Ok(matches.into_iter().next().cloned())
    }

    /// Resolve dependencies with conflict resolution.
    ///
    /// This uses a simple resolution strategy:
    /// 1. Collect all unique dependencies with their constraints
    /// 2. For each unique harness name, find versions that satisfy ALL constraints
    /// 3. If no single version satisfies all constraints, report the conflict
    /// 4. Otherwise, select the highest matching version
    pub fn resolve_dependencies(
        &self,
        dependencies: &[HarnessDependency],
    ) -> Result<Vec<ResolvedVersion>, VersionError> {
        // Group dependencies by harness name
        let mut grouped: HashMap<String, Vec<&HarnessDependency>> = HashMap::new();
        for dep in dependencies {
            grouped.entry(dep.name.clone()).or_default().push(dep);
        }

        let mut resolved = Vec::new();

        for (name, deps) in grouped {
            // Merge all constraints for this harness
            let versions = self.available.get(&name).ok_or_else(|| {
                VersionError::NoMatchingVersion {
                    name: name.clone(),
                    constraint: deps
                        .first()
                        .map(|d| d.constraint.original.clone())
                        .unwrap_or_default(),
                }
            })?;

            // Find versions that satisfy ALL constraints
            let mut valid_versions: Vec<_> = versions
                .iter()
                .filter(|v| deps.iter().all(|d| d.constraint.matches(&v.version)))
                .collect();

            if valid_versions.is_empty() {
                // Build a conflict message
                let constraints: Vec<_> = deps.iter().map(|d| &d.constraint.original).collect();
                let requested_by: Vec<_> = deps.iter().map(|d| &d.requested_by).collect();
                return Err(VersionError::Conflict {
                    message: format!(
                        "no version of '{}' satisfies constraints {:?}\n  requested by: {:?}",
                        name, constraints, requested_by
                    ),
                });
            }

            // Sort by version descending and pick the highest
            valid_versions.sort_by(|a, b| b.version.cmp(&a.version));
            let best = valid_versions.into_iter().next().unwrap();

            let requested_by = deps.iter().map(|d| d.requested_by.clone()).collect();

            resolved.push(ResolvedVersion {
                name,
                version: best.version.clone(),
                source: best.source.clone(),
                requested_by,
            });
        }

        // Sort by name for deterministic output
        resolved.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(resolved)
    }

    /// Get all available versions for a harness.
    pub fn get_versions(&self, name: &str) -> Option<&[VersionedHarness]> {
        self.available.get(name).map(|v| v.as_slice())
    }
}

impl From<semver::Error> for VersionError {
    fn from(e: semver::Error) -> Self {
        VersionError::InvalidVersion {
            input: e.to_string(),
            reason: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_version() {
        let constraint = VersionConstraint::parse("1.2.3").unwrap();
        assert!(constraint.matches(&Version::parse("1.2.3").unwrap()));
        assert!(constraint.matches(&Version::parse("1.2.4").unwrap())); // >= means compatible
        assert!(constraint.matches(&Version::parse("1.3.0").unwrap()));
        assert!(constraint.matches(&Version::parse("2.0.0").unwrap()));
    }

    #[test]
    fn parses_caret_range() {
        let constraint = VersionConstraint::parse("^1.2.3").unwrap();
        assert!(constraint.matches(&Version::parse("1.2.3").unwrap()));
        assert!(constraint.matches(&Version::parse("1.3.0").unwrap()));
        assert!(constraint.matches(&Version::parse("1.9.9").unwrap()));
        assert!(!constraint.matches(&Version::parse("2.0.0").unwrap()));
        assert!(!constraint.matches(&Version::parse("1.2.2").unwrap()));
    }

    #[test]
    fn parses_caret_zero_major() {
        // ^0.2.3 := >=0.2.3 <0.3.0
        let constraint = VersionConstraint::parse("^0.2.3").unwrap();
        assert!(constraint.matches(&Version::parse("0.2.3").unwrap()));
        assert!(constraint.matches(&Version::parse("0.2.9").unwrap()));
        assert!(!constraint.matches(&Version::parse("0.3.0").unwrap()));
        assert!(!constraint.matches(&Version::parse("1.0.0").unwrap()));
    }

    #[test]
    fn parses_tilde_range() {
        // ~1.2.3 := >=1.2.3 <1.3.0
        let constraint = VersionConstraint::parse("~1.2.3").unwrap();
        assert!(constraint.matches(&Version::parse("1.2.3").unwrap()));
        assert!(constraint.matches(&Version::parse("1.2.9").unwrap()));
        assert!(!constraint.matches(&Version::parse("1.3.0").unwrap()));
        assert!(!constraint.matches(&Version::parse("1.2.2").unwrap()));
    }

    #[test]
    fn parses_comparison_operators() {
        let constraint = VersionConstraint::parse(">=1.0.0,<2.0.0").unwrap();
        assert!(constraint.matches(&Version::parse("1.0.0").unwrap()));
        assert!(constraint.matches(&Version::parse("1.5.0").unwrap()));
        assert!(!constraint.matches(&Version::parse("0.9.9").unwrap()));
        assert!(!constraint.matches(&Version::parse("2.0.0").unwrap()));
    }

    #[test]
    fn reject_invalid_version() {
        let result = VersionConstraint::parse("not-a-version");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_simple_dependency() {
        let mut resolver = VersionResolver::new();
        resolver.add_version("test-harness", Version::parse("1.0.0").unwrap(), "github:owner/repo", "/path/v1");
        resolver.add_version("test-harness", Version::parse("1.1.0").unwrap(), "github:owner/repo", "/path/v1.1");
        resolver.add_version("test-harness", Version::parse("2.0.0").unwrap(), "github:owner/repo", "/path/v2");

        let deps = vec![HarnessDependency {
            name: "test-harness".to_string(),
            constraint: VersionConstraint::parse("^1.0.0").unwrap(),
            requested_by: "consumer".to_string(),
        }];

        let resolved = resolver.resolve_dependencies(&deps).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].version, Version::parse("1.1.0").unwrap()); // Highest matching version
    }

    #[test]
    fn resolve_conflicting_versions() {
        let mut resolver = VersionResolver::new();
        resolver.add_version("dep-a", Version::parse("1.0.0").unwrap(), "source1", "/path");
        resolver.add_version("dep-a", Version::parse("2.0.0").unwrap(), "source1", "/path");

        let deps = vec![
            HarnessDependency {
                name: "dep-a".to_string(),
                constraint: VersionConstraint::parse("^1.0.0").unwrap(),
                requested_by: "consumer-a".to_string(),
            },
            HarnessDependency {
                name: "dep-a".to_string(),
                constraint: VersionConstraint::parse("^2.0.0").unwrap(),
                requested_by: "consumer-b".to_string(),
            },
        ];

        let result = resolver.resolve_dependencies(&deps);
        assert!(matches!(result, Err(VersionError::Conflict { .. })));
    }

    #[test]
    fn resolve_compatible_constraints() {
        let mut resolver = VersionResolver::new();
        resolver.add_version("dep-a", Version::parse("1.2.0").unwrap(), "source1", "/path");
        resolver.add_version("dep-a", Version::parse("1.5.0").unwrap(), "source1", "/path");
        resolver.add_version("dep-a", Version::parse("2.0.0").unwrap(), "source1", "/path");

        let deps = vec![
            HarnessDependency {
                name: "dep-a".to_string(),
                constraint: VersionConstraint::parse("^1.0.0").unwrap(),
                requested_by: "consumer-a".to_string(),
            },
            HarnessDependency {
                name: "dep-a".to_string(),
                constraint: VersionConstraint::parse(">=1.2.0,<2.0.0").unwrap(),
                requested_by: "consumer-b".to_string(),
            },
        ];

        let resolved = resolver.resolve_dependencies(&deps).unwrap();
        assert_eq!(resolved[0].version, Version::parse("1.5.0").unwrap());
    }

    #[test]
    fn resolve_multiple_dependencies() {
        let mut resolver = VersionResolver::new();
        resolver.add_version("harness-a", Version::parse("1.0.0").unwrap(), "source1", "/path/a1");
        resolver.add_version("harness-b", Version::parse("2.0.0").unwrap(), "source2", "/path/b2");

        let deps = vec![
            HarnessDependency {
                name: "harness-a".to_string(),
                constraint: VersionConstraint::parse("^1.0.0").unwrap(),
                requested_by: "root".to_string(),
            },
            HarnessDependency {
                name: "harness-b".to_string(),
                constraint: VersionConstraint::parse("^2.0.0").unwrap(),
                requested_by: "root".to_string(),
            },
        ];

        let resolved = resolver.resolve_dependencies(&deps).unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].name, "harness-a");
        assert_eq!(resolved[1].name, "harness-b");
    }
}