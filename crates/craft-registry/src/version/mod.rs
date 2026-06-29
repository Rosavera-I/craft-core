//! Semantic version resolution for the CRAFT Registry
//!
//! Provides robust version matching, parsing, and comparison utilities
//! that extend the standard semver crate with registry-specific features.

use semver::{Prerelease, Version, VersionReq};

use crate::error::{RegistryError, RegistryResult};

/// Parse a version string with better error messages
pub fn parse_version(version: &str) -> RegistryResult<Version> {
    version.parse::<Version>().map_err(RegistryError::Version)
}

/// Parse a version requirement with better error messages
pub fn parse_version_req(req: &str) -> RegistryResult<VersionReq> {
    req.parse::<VersionReq>().map_err(RegistryError::Version)
}

/// Match a version against requirements
pub fn matches(version: &Version, req: &VersionReq) -> bool {
    req.matches(version)
}

/// Find the best matching version from a list
pub fn resolve_best_version(versions: &[Version], req: &VersionReq) -> Option<Version> {
    let mut stable: Vec<&Version> = versions
        .iter()
        .filter(|v| v.pre == Prerelease::EMPTY && req.matches(v))
        .collect();

    stable.sort_by(|a, b| b.cmp(a));

    if let Some(version) = stable.first() {
        return Some((*version).clone());
    }

    let mut prerelease: Vec<&Version> = versions
        .iter()
        .filter(|v| v.pre != Prerelease::EMPTY)
        .filter(|v| {
            let base = Version::new(v.major, v.minor, v.patch);
            req.matches(&base)
        })
        .collect();

    prerelease.sort_by(|a, b| b.cmp(a));
    prerelease.first().map(|version| (*version).clone())
}

/// Compare two versions
pub fn compare_versions(a: &Version, b: &Version) -> std::cmp::Ordering {
    a.cmp(b)
}

/// Check if a version is a pre-release
pub fn is_prerelease(version: &Version) -> bool {
    version.pre != Prerelease::EMPTY
}

/// Get the next major version
pub fn next_major(version: &Version) -> Version {
    Version::new(version.major + 1, 0, 0)
}

/// Get the next minor version
pub fn next_minor(version: &Version) -> Version {
    Version::new(version.major, version.minor + 1, 0)
}

/// Get the next patch version
pub fn next_patch(version: &Version) -> Version {
    Version::new(version.major, version.minor, version.patch + 1)
}

/// Create a caret version requirement (^x.y.z means >=x.y.z <(x+1).0.0)
pub fn caret_requirement(version: &Version) -> String {
    format!("^{}", version)
}

/// Create a tilde version requirement (~x.y.z means >=x.y.z <x.(y+1).0)
pub fn tilde_requirement(version: &Version) -> String {
    format!("~{}.{}", version.major, version.minor)
}

/// Create an exact version requirement
pub fn exact_requirement(version: &Version) -> String {
    format!("={}", version)
}

/// Format a version to string without build metadata
pub fn format_version(version: &Version) -> String {
    if version.pre.is_empty() {
        format!("{}.{}.{}", version.major, version.minor, version.patch)
    } else {
        format!(
            "{}.{}.{}-{}",
            version.major, version.minor, version.patch, version.pre
        )
    }
}

/// Version resolution for dependencies
#[derive(Debug, Clone)]
pub struct VersionResolver {
    /// Available versions (should be sorted descending)
    versions: Vec<Version>,
}

impl VersionResolver {
    /// Create a new resolver with available versions
    pub fn new(versions: Vec<Version>) -> Self {
        let mut versions = versions;
        versions.sort_by(|a, b| b.cmp(a)); // Sort descending
        Self { versions }
    }

    /// Resolve a version requirement
    pub fn resolve(&self, req: &VersionReq) -> Option<Version> {
        resolve_best_version(&self.versions, req)
    }

    /// Get the latest version
    pub fn latest(&self) -> Option<Version> {
        self.versions.first().cloned()
    }

    /// Get the latest stable (non-prerelease) version
    pub fn latest_stable(&self) -> Option<Version> {
        self.versions.iter().find(|v| !is_prerelease(v)).cloned()
    }

    /// Get all versions matching a requirement
    pub fn matching(&self, req: &VersionReq) -> Vec<Version> {
        self.versions
            .iter()
            .filter(|v| req.matches(v))
            .cloned()
            .collect()
    }

    /// Check if a version exists
    pub fn exists(&self, version: &Version) -> bool {
        self.versions.contains(version)
    }
}

/// Validate a version string for crate naming
/// Crate versions must follow semver and not have build metadata for comparisons
pub fn validate_version(version: &str) -> RegistryResult<Version> {
    let v = parse_version(version)?;

    // Reject versions with empty components that might cause issues
    if v.major == 0 && v.minor == 0 && v.patch == 0 {
        return Err(RegistryError::Validation(
            "Version 0.0.0 is not allowed".to_string(),
        ));
    }

    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert!(parse_version("1.2.3").is_ok());
        assert!(parse_version("1.2.3-alpha").is_ok());
        assert!(parse_version("1.2.3-alpha+build").is_ok());
        assert!(parse_version("invalid").is_err());
    }

    #[test]
    fn test_matches() {
        let v = Version::new(1, 2, 3);
        let req = VersionReq::parse(">= 1.0.0").unwrap();
        assert!(matches(&v, &req));

        let req = VersionReq::parse(">= 2.0.0").unwrap();
        assert!(!matches(&v, &req));
    }

    #[test]
    fn test_resolve_best_version() {
        let versions = vec![
            Version::new(2, 0, 0),
            Version::new(1, 5, 0),
            Version::new(1, 4, 5),
            Version::parse("1.6.0-alpha").unwrap(),
        ];

        let req = VersionReq::parse("^1.0.0").unwrap();
        let best = resolve_best_version(&versions, &req);
        assert_eq!(best, Some(Version::new(1, 5, 0)));

        // Test with only pre-releases
        let req = VersionReq::parse(">= 1.6.0, < 2.0.0").unwrap();
        let best = resolve_best_version(&versions, &req);
        assert_eq!(best, Some(Version::parse("1.6.0-alpha").unwrap()));
    }

    #[test]
    fn test_is_prerelease() {
        assert!(!is_prerelease(&Version::new(1, 0, 0)));
        assert!(is_prerelease(&Version::parse("1.0.0-alpha").unwrap()));
    }

    #[test]
    fn test_version_resolver() {
        let versions = vec![
            Version::new(2, 0, 0),
            Version::new(1, 6, 0),
            Version::new(1, 5, 0),
            Version::parse("1.7.0-beta").unwrap(),
        ];

        let resolver = VersionResolver::new(versions);

        assert_eq!(resolver.latest(), Some(Version::new(2, 0, 0)));
        assert_eq!(resolver.latest_stable(), Some(Version::new(2, 0, 0)));

        let req = VersionReq::parse("^1.0.0").unwrap();
        assert_eq!(resolver.resolve(&req), Some(Version::new(1, 6, 0)));
    }

    #[test]
    fn test_validate_version() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("0.0.0").is_err());
    }

    #[test]
    fn test_version_requirements() {
        let v = Version::new(1, 2, 3);
        assert_eq!(caret_requirement(&v), "^1.2.3");
        assert_eq!(tilde_requirement(&v), "~1.2");
        assert_eq!(exact_requirement(&v), "=1.2.3");
    }
}
