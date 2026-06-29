//! Unit tests for version resolution
//!
//! Tests for:
//! - Semantic version parsing
//! - Version requirement matching
//! - Best version selection
//! - Pre-release handling

use craft_registry::version::*;
use semver::{Version, VersionReq};

#[test]
fn test_parse_valid_versions() {
    let cases = [
        ("1.0.0", Version::new(1, 0, 0)),
        ("1.2.3", Version::new(1, 2, 3)),
        ("0.1.0", Version::new(0, 1, 0)),
    ];

    for (input, expected) in cases {
        let result = parse_version(input).unwrap();
        assert_eq!(result, expected, "Failed parsing: {}", input);
    }
}

#[test]
fn test_parse_versions_with_prerelease() {
    let cases = [
        ("1.0.0-alpha", true),
        ("1.0.0-alpha.1", true),
        ("1.0.0-beta", true),
        ("1.0.0-rc.1", true),
        ("1.0.0", false),
    ];

    for (input, is_pre) in cases {
        let version = parse_version(input).unwrap();
        assert_eq!(
            is_prerelease(&version),
            is_pre,
            "Prerelease check failed for: {}",
            input
        );
    }
}

#[test]
fn test_parse_version_requirements() {
    let cases = ["^1.0.0", "~1.2.0", ">= 1.0.0, < 2.0.0", "1.2.3", ">= 1.0.0"];

    for req in cases {
        let result = parse_version_req(req);
        assert!(result.is_ok(), "Failed parsing requirement: {}", req);
    }
}

#[test]
fn test_version_matching() {
    let version = Version::new(1, 2, 3);

    // Exact match
    assert!(
        matches(&version, &VersionReq::parse("=1.2.3").unwrap()),
        "Exact match failed"
    );

    // Caret (^1.0.0 matches >=1.0.0 <2.0.0)
    assert!(matches(&version, &VersionReq::parse("^1.0.0").unwrap()));
    assert!(!matches(&version, &VersionReq::parse("^2.0.0").unwrap()));

    // Tilde (~1.2.0 matches >=1.2.0 <1.3.0)
    assert!(matches(&version, &VersionReq::parse("~1.2.0").unwrap()));
    assert!(!matches(&version, &VersionReq::parse("~1.1.0").unwrap()));

    // Range
    assert!(matches(
        &version,
        &VersionReq::parse(">=1.0.0, <2.0.0").unwrap()
    ));
    assert!(!matches(&version, &VersionReq::parse(">=2.0.0").unwrap()));
}

#[test]
fn test_best_version_resolution_stable_preference() {
    let versions = vec![
        Version::parse("1.0.0-alpha").unwrap(),
        Version::new(1, 0, 0),
        Version::new(1, 1, 0),
        Version::parse("1.2.0-beta").unwrap(),
        Version::new(1, 2, 0),
    ];

    let req = VersionReq::parse("^1.0.0").unwrap();
    let best = resolve_best_version(&versions, &req);

    // Should prefer 1.2.0 (stable) over 1.2.0-beta (prerelease)
    assert_eq!(best, Some(Version::new(1, 2, 0)));
}

#[test]
fn test_best_version_resolution_prerelease_fallback() {
    let versions = vec![
        Version::parse("1.0.0-alpha").unwrap(),
        Version::parse("1.0.0-beta").unwrap(),
        Version::parse("1.1.0-rc.1").unwrap(),
    ];

    let req = VersionReq::parse(">=1.0.0").unwrap();
    let best = resolve_best_version(&versions, &req);

    // Should return highest prerelease since no stable versions
    assert_eq!(best, Some(Version::parse("1.1.0-rc.1").unwrap()));
}

#[test]
fn test_best_version_resolution_exact_req() {
    let versions = vec![
        Version::new(1, 0, 0),
        Version::new(1, 1, 0),
        Version::new(1, 2, 0),
    ];

    let req = VersionReq::parse(">=1.1.0, <1.2.0").unwrap();
    let best = resolve_best_version(&versions, &req);

    assert_eq!(best, Some(Version::new(1, 1, 0)));
}

#[test]
fn test_version_comparison() {
    let v1 = Version::new(1, 0, 0);
    let v2 = Version::new(1, 0, 1);
    let v3 = Version::new(1, 1, 0);
    let v4 = Version::new(2, 0, 0);

    assert_eq!(compare_versions(&v1, &v2), std::cmp::Ordering::Less);
    assert_eq!(compare_versions(&v2, &v1), std::cmp::Ordering::Greater);
    assert_eq!(compare_versions(&v1, &v1), std::cmp::Ordering::Equal);
    assert_eq!(compare_versions(&v2, &v3), std::cmp::Ordering::Less);
    assert_eq!(compare_versions(&v3, &v4), std::cmp::Ordering::Less);
}

#[test]
fn test_prerelease_ignored_by_default() {
    // By default, semver doesn't match prereleases unless explicitly requested
    let version = Version::parse("1.0.0-alpha").unwrap();
    let req = VersionReq::parse("^1.0.0").unwrap();

    assert!(!matches(&version, &req));
}

#[test]
fn test_resolver_version_exists() {
    let versions = vec![Version::new(1, 0, 0), Version::new(1, 1, 0)];

    let resolver = VersionResolver::new(versions);

    assert!(resolver.exists(&Version::new(1, 0, 0)));
    assert!(resolver.exists(&Version::new(1, 1, 0)));
    assert!(!resolver.exists(&Version::new(2, 0, 0)));
}

#[test]
fn test_resolver_latest() {
    let versions = vec![
        Version::new(1, 0, 0),
        Version::new(1, 1, 0),
        Version::new(1, 0, 5),
    ];

    let resolver = VersionResolver::new(versions);

    assert_eq!(resolver.latest(), Some(Version::new(1, 1, 0)));
}

#[test]
fn test_resolver_latest_stable() {
    let versions = vec![
        Version::new(1, 0, 0),
        Version::parse("1.1.0-alpha").unwrap(),
        Version::new(1, 0, 5),
    ];

    let resolver = VersionResolver::new(versions);

    // Should skip the prerelease
    assert_eq!(resolver.latest_stable(), Some(Version::new(1, 0, 5)));
}

#[test]
fn test_caret_requirement_generation() {
    let version = Version::new(1, 2, 3);
    let req = caret_requirement(&version);
    assert_eq!(req, "^1.2.3");

    // Test that it produces valid requirements
    let parsed: VersionReq = req.parse().unwrap();
    assert!(parsed.matches(&version));
    assert!(parsed.matches(&Version::new(1, 5, 0)));
    assert!(!parsed.matches(&Version::new(2, 0, 0)));
}

#[test]
fn test_tilde_requirement_generation() {
    let version = Version::new(1, 2, 3);
    let req = tilde_requirement(&version);
    assert_eq!(req, "~1.2");

    let parsed: VersionReq = req.parse().unwrap();
    assert!(parsed.matches(&version));
    assert!(parsed.matches(&Version::new(1, 2, 5)));
    assert!(!parsed.matches(&Version::new(1, 3, 0)));
}

#[test]
fn test_exact_requirement_generation() {
    let version = Version::new(1, 2, 3);
    let req = exact_requirement(&version);
    assert_eq!(req, "=1.2.3");

    let parsed: VersionReq = req.parse().unwrap();
    assert!(parsed.matches(&version));
    assert!(!parsed.matches(&Version::new(1, 2, 4)));
}

#[test]
fn test_validate_version_rejects_zero_zero_zero() {
    // 0.0.0 is typically not allowed in registries
    assert!(validate_version("0.0.0").is_err());
    assert!(validate_version("0.0.1").is_ok());
    assert!(validate_version("0.1.0").is_ok());
    assert!(validate_version("1.0.0").is_ok());
}

#[test]
fn test_version_formatting() {
    let v1 = Version::new(1, 2, 3);
    assert_eq!(format_version(&v1), "1.2.3");

    let v2 = Version::parse("1.2.3-alpha").unwrap();
    assert_eq!(format_version(&v2), "1.2.3-alpha");
}

#[test]
fn test_next_version_functions() {
    let version = Version::new(1, 2, 3);

    assert_eq!(next_major(&version), Version::new(2, 0, 0));
    assert_eq!(next_minor(&version), Version::new(1, 3, 0));
    assert_eq!(next_patch(&version), Version::new(1, 2, 4));
}

#[test]
fn test_complex_version_requirements() {
    let versions = vec![
        Version::new(1, 0, 0),
        Version::new(1, 5, 0),
        Version::new(2, 0, 0),
        Version::new(2, 1, 0),
    ];

    // Greater than
    let req = VersionReq::parse(">1.0.0").unwrap();
    let matching = resolve_best_version(&versions, &req);
    assert_eq!(matching, Some(Version::new(2, 1, 0)));

    // Less than
    let req = VersionReq::parse("<2.0.0").unwrap();
    let matching = resolve_best_version(&versions, &req);
    assert_eq!(matching, Some(Version::new(1, 5, 0)));

    // Wildcard
    let req = VersionReq::parse("1.*").unwrap();
    let matching = resolve_best_version(&versions, &req);
    assert_eq!(matching, Some(Version::new(1, 5, 0)));
}
