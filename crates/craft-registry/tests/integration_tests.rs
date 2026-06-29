//! Integration tests for the CRAFT Registry
//!
//! Tests for:
//! - Harness publish workflow
//! - Harness install/download
//! - Team ACL (member, maintainer, admin roles)

use craft_registry::{Role, Visibility, auth::hash_password, db::*, error::RegistryResult};

/// Setup test database connection
async fn setup_test_db() -> RegistryResult<Option<Database>> {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("skipping registry integration test; TEST_DATABASE_URL is not set");
        return Ok(None);
    };

    let db = Database::new(&database_url).await?;

    // Run migrations
    db.migrate().await?;

    Ok(Some(db))
}

/// Clean up test data
async fn cleanup_test_data(pool: &sqlx::PgPool) -> RegistryResult<()> {
    // Delete in reverse dependency order
    sqlx::query("DELETE FROM audit_logs")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM access_tokens")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM harness_versions")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM harnesses")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM team_members")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM org_members")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM teams").execute(pool).await.ok();
    sqlx::query("DELETE FROM users").execute(pool).await.ok();
    sqlx::query("DELETE FROM organizations")
        .execute(pool)
        .await
        .ok();

    Ok(())
}

/// Create a test user
async fn create_test_user(pool: &sqlx::PgPool, username: &str) -> RegistryResult<User> {
    let email = format!("{}@test.com", username);
    let password_hash = hash_password("testpassword").ok();

    create_user(
        pool,
        username,
        &email,
        password_hash.as_deref(),
        Some(username),
    )
    .await
}

#[tokio::test]
async fn test_harness_publish_workflow() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    // Clean up any existing data
    cleanup_test_data(pool).await.ok();

    // Create test user
    let user = create_test_user(pool, "publisher").await?;

    // Create organization
    let org = create_org(
        pool,
        "test-org",
        Some("Test Org"),
        None,
        Visibility::Private,
    )
    .await?;

    // Add user as admin
    add_org_member(pool, org.id, user.id, Role::Admin).await?;

    // Create harness
    let harness = create_harness(
        pool,
        org.id,
        None,
        "test-harness",
        Some("A test harness"),
        Visibility::Private,
        Some(&["test".to_string(), "harness".to_string()]),
        Some("https://github.com/test-org/test-harness.git"),
    )
    .await?;

    assert_eq!(harness.name, "test-harness");
    assert_eq!(harness.org_id, org.id);
    assert_eq!(harness.visibility(), Visibility::Private);

    // Publish a version
    let version_str = "1.0.0";
    let semver_version = version_str.parse().unwrap();

    let version = create_harness_version(
        pool,
        harness.id,
        &semver_version,
        Some("v1.0.0"),
        Some("abc123def456"),
        Some("Initial release"),
        Some("# Test Harness\n\nThis is a test harness."),
        1024,
        "sha256hash123456789",
        "test-org/test-harness/1.0.0/ab/sha256hash123456789",
        user.id,
    )
    .await?;

    assert_eq!(version.version, version_str);
    assert_eq!(version.harness_id, harness.id);
    assert!(!version.is_yanked);

    // Verify we can retrieve the version
    let retrieved = get_harness_version(pool, harness.id, version_str).await?;
    assert_eq!(retrieved.id, version.id);
    assert_eq!(retrieved.content_sha256, "sha256hash123456789");

    // Test latest version resolution
    let latest = get_latest_harness_version(pool, harness.id).await?;
    assert_eq!(latest.version, version_str);

    // Publish another version
    let semver_version2 = "1.1.0".parse().unwrap();
    let _version2 = create_harness_version(
        pool,
        harness.id,
        &semver_version2,
        Some("v1.1.0"),
        Some("def789abc012"),
        Some("Bug fixes"),
        None,
        2048,
        "sha256hash987654321",
        "test-org/test-harness/1.1.0/ab/sha256hash987654321",
        user.id,
    )
    .await?;

    // Verify latest points to new version
    let latest = get_latest_harness_version(pool, harness.id).await?;
    assert_eq!(latest.version, "1.1.0");

    // Test version list
    let versions = list_harness_versions(pool, harness.id).await?;
    assert_eq!(versions.len(), 2);

    // Cleanup
    cleanup_test_data(pool).await.ok();

    Ok(())
}

#[tokio::test]
async fn test_team_acl_permissions() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    cleanup_test_data(pool).await.ok();

    // Create users
    let admin_user = create_test_user(pool, "admin-user").await?;
    let maintainer_user = create_test_user(pool, "maintainer-user").await?;
    let member_user = create_test_user(pool, "member-user").await?;
    let outsider_user = create_test_user(pool, "outsider-user").await?;

    // Create organization
    let org = create_org(
        pool,
        "acl-test-org",
        Some("ACL Test Org"),
        None,
        Visibility::Private,
    )
    .await?;

    // Set up organization membership
    add_org_member(pool, org.id, admin_user.id, Role::Admin).await?;
    add_org_member(pool, org.id, maintainer_user.id, Role::Maintainer).await?;
    add_org_member(pool, org.id, member_user.id, Role::Member).await?;
    // outsider_user is NOT a member

    // Create team
    let team = create_team(
        pool,
        org.id,
        "test-team",
        Some("Test Team"),
        Visibility::Private,
    )
    .await?;

    // Set up team membership with different roles
    add_team_member(pool, team.id, admin_user.id, Role::Admin).await?;
    add_team_member(pool, team.id, maintainer_user.id, Role::Maintainer).await?;
    add_team_member(pool, team.id, member_user.id, Role::Member).await?;

    // Test: Admin can invite new team members
    let new_user = create_test_user(pool, "new-user").await?;

    // First add to org (admins can do this)
    let result = add_org_member(pool, org.id, new_user.id, Role::Member).await;
    assert!(result.is_ok(), "Admin should be able to add org members");

    // Then add to team (admins can do this too)
    let result = add_team_member(pool, team.id, new_user.id, Role::Member).await;
    assert!(result.is_ok(), "Admin should be able to add team members");

    // Test: Maintainer can also invite to team
    let another_user = create_test_user(pool, "another-user").await?;
    add_org_member(pool, org.id, another_user.id, Role::Member).await?;
    let result = add_team_member(pool, team.id, another_user.id, Role::Member).await;
    assert!(
        result.is_ok(),
        "Maintainer should be able to add team members"
    );

    // Test: Regular members cannot invite
    // (This would be handled at authorization layer - here we test the DB allows the op)
    // The actual ACL check happens in handler code

    // Test: Verify role hierarchy
    let admin_check = check_org_role(pool, org.id, admin_user.id, Role::Admin).await?;
    assert!(admin_check, "Admin should satisfy Admin role check");

    let maintainer_is_admin = check_org_role(pool, org.id, maintainer_user.id, Role::Admin).await?;
    assert!(
        !maintainer_is_admin,
        "Maintainer should NOT satisfy Admin role check"
    );

    let maintainer_check =
        check_org_role(pool, org.id, maintainer_user.id, Role::Maintainer).await?;
    assert!(
        maintainer_check,
        "Maintainer should satisfy Maintainer role check"
    );

    // Test: Member satisfies Member role check
    let member_check = check_org_role(pool, org.id, member_user.id, Role::Member).await?;
    assert!(member_check, "Any member should satisfy Member role check");

    // Test: Outsider is not a member
    let outsider_check = is_org_member(pool, org.id, outsider_user.id).await?;
    assert!(!outsider_check, "Outsider should not be an org member");

    // Test: Team membership checks
    let team_admin = get_team_member(pool, team.id, admin_user.id).await?;
    assert_eq!(team_admin.role(), Role::Admin);

    let team_maintainer = get_team_member(pool, team.id, maintainer_user.id).await?;
    assert_eq!(team_maintainer.role(), Role::Maintainer);

    // Test: Remove member
    remove_team_member(pool, team.id, new_user.id).await?;
    let team_members = list_team_members(pool, team.id).await?;
    assert_eq!(
        team_members.len(),
        3,
        "Team should have 3 members after removal"
    );

    // Cleanup
    cleanup_test_data(pool).await.ok();

    Ok(())
}

#[tokio::test]
async fn test_version_yank_and_unyank() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    cleanup_test_data(pool).await.ok();

    // Setup
    let user = create_test_user(pool, "yank-test-user").await?;
    let org = create_org(pool, "yank-test-org", None, None, Visibility::Private).await?;
    add_org_member(pool, org.id, user.id, Role::Maintainer).await?;

    let harness = create_harness(
        pool,
        org.id,
        None,
        "yank-test-harness",
        None,
        Visibility::Private,
        None,
        None,
    )
    .await?;

    let semver = "1.0.0".parse().unwrap();
    let version = create_harness_version(
        pool,
        harness.id,
        &semver,
        None,
        None,
        None,
        None,
        100,
        "hash123",
        "path/to/package",
        user.id,
    )
    .await?;

    assert!(!version.is_yanked);

    // Yank version
    let yanked = yank_version(pool, version.id, Some("Security vulnerability")).await?;
    assert!(yanked.is_yanked);
    assert_eq!(
        yanked.yanked_reason,
        Some("Security vulnerability".to_string())
    );

    // Verify yanked version doesn't appear in latest queries
    let result = get_latest_harness_version(pool, harness.id).await;
    // This will fail because all versions are yanked - expected behavior
    assert!(
        result.is_err(),
        "Yanked versions should not appear in latest"
    );

    // Unyank version
    let unyanked = unyank_version(pool, version.id).await?;
    assert!(!unyanked.is_yanked);
    assert!(unyanked.yanked_reason.is_none());

    // Cleanup
    cleanup_test_data(pool).await.ok();

    Ok(())
}

#[tokio::test]
async fn test_download_count_tracking() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    cleanup_test_data(pool).await.ok();

    // Setup
    let user = create_test_user(pool, "download-test-user").await?;
    let org = create_org(pool, "dl-test-org", None, None, Visibility::Private).await?;
    add_org_member(pool, org.id, user.id, Role::Member).await?;

    let harness = create_harness(
        pool,
        org.id,
        None,
        "dl-test-harness",
        None,
        Visibility::Public,
        None,
        None,
    )
    .await?;

    let semver = "1.0.0".parse().unwrap();
    let version = create_harness_version(
        pool, harness.id, &semver, None, None, None, None, 100, "hash", "path", user.id,
    )
    .await?;

    assert_eq!(version.download_count, 0);

    // Simulate downloads
    for _ in 0..5 {
        increment_download_count(pool, version.id).await?;
    }

    // Verify download count
    let updated = get_version_by_id(pool, version.id).await?;
    assert_eq!(updated.download_count, 5);

    // Verify harness total downloads updated
    let updated_harness = get_harness_by_id(pool, harness.id).await?;
    assert_eq!(updated_harness.total_downloads, 5);

    // Cleanup
    cleanup_test_data(pool).await.ok();

    Ok(())
}

#[tokio::test]
async fn test_multi_tenancy_isolation() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    cleanup_test_data(pool).await.ok();

    // Create two separate organizations
    let org1 = create_org(
        pool,
        "org1",
        Some("Organization 1"),
        None,
        Visibility::Private,
    )
    .await?;
    let org2 = create_org(
        pool,
        "org2",
        Some("Organization 2"),
        None,
        Visibility::Private,
    )
    .await?;

    // Create users in each org
    let user1 = create_test_user(pool, "org1-user").await?;
    let user2 = create_test_user(pool, "org2-user").await?;

    add_org_member(pool, org1.id, user1.id, Role::Admin).await?;
    add_org_member(pool, org2.id, user2.id, Role::Admin).await?;

    // Create harnesses with same name in different orgs
    let harness1 = create_harness(
        pool,
        org1.id,
        None,
        "shared-name",
        Some("Org 1 harness"),
        Visibility::Private,
        None,
        None,
    )
    .await?;

    let harness2 = create_harness(
        pool,
        org2.id,
        None,
        "shared-name",
        Some("Org 2 harness"),
        Visibility::Private,
        None,
        None,
    )
    .await?;

    // Verify they are different harnesses
    assert_ne!(harness1.id, harness2.id);

    // Load each by org + name and verify correct retrieval
    let retrieved1 = get_harness_by_org_and_name(pool, org1.id, "shared-name").await?;
    assert_eq!(retrieved1.id, harness1.id);
    assert_eq!(retrieved1.description, Some("Org 1 harness".to_string()));

    let retrieved2 = get_harness_by_org_and_name(pool, org2.id, "shared-name").await?;
    assert_eq!(retrieved2.id, harness2.id);
    assert_eq!(retrieved2.description, Some("Org 2 harness".to_string()));

    // Test: List harnesses by org is isolated
    let org1_harnesses = list_harnesses_by_org(pool, org1.id).await?;
    assert_eq!(org1_harnesses.len(), 1);
    assert_eq!(org1_harnesses[0].id, harness1.id);

    let org2_harnesses = list_harnesses_by_org(pool, org2.id).await?;
    assert_eq!(org2_harnesses.len(), 1);
    assert_eq!(org2_harnesses[0].id, harness2.id);

    // Cleanup
    cleanup_test_data(pool).await.ok();

    Ok(())
}

#[tokio::test]
async fn test_visibility_public_internal_private() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    cleanup_test_data(pool).await.ok();

    let org = create_org(
        pool,
        "visibility-org",
        Some("Visibility Test Org"),
        None,
        Visibility::Private,
    )
    .await?;

    let public = create_harness(
        pool,
        org.id,
        None,
        "public-harness",
        Some("Public harness"),
        Visibility::Public,
        None,
        None,
    )
    .await?;
    let internal = create_harness(
        pool,
        org.id,
        None,
        "internal-harness",
        Some("Internal harness"),
        Visibility::Internal,
        None,
        None,
    )
    .await?;
    let private = create_harness(
        pool,
        org.id,
        None,
        "private-harness",
        Some("Private harness"),
        Visibility::Private,
        None,
        None,
    )
    .await?;

    assert_eq!(public.visibility, "public");
    assert_eq!(internal.visibility, "internal");
    assert_eq!(private.visibility, "private");

    cleanup_test_data(pool).await.ok();

    Ok(())
}
#[tokio::test]
async fn test_access_token_lifecycle() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    cleanup_test_data(pool).await.ok();

    let user = create_test_user(pool, "token-user").await?;

    // Create access token
    let scopes = vec!["read".to_string(), "write".to_string()];
    let token = create_access_token(
        pool,
        user.id,
        None,
        "my-token",
        "hash123456",
        "crp_hash12",
        &scopes,
        None,
    )
    .await?;

    assert_eq!(token.name, "my-token");
    assert_eq!(token.scopes, vec!["read", "write"]);

    // List tokens
    let tokens = list_user_access_tokens(pool, user.id).await?;
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].id, token.id);

    // Revoke token
    revoke_access_token(pool, token.id).await?;

    // Verify token is revoked (would fail lookup)
    let tokens = list_user_access_tokens(pool, user.id).await?;
    assert!(tokens.is_empty(), "Revoked token should not be listed");

    // Cleanup
    cleanup_test_data(pool).await.ok();

    Ok(())
}

#[tokio::test]
async fn test_audit_logging() -> RegistryResult<()> {
    let Some(db) = setup_test_db().await? else {
        return Ok(());
    };
    let pool = db.pool();

    cleanup_test_data(pool).await.ok();

    let user = create_test_user(pool, "audit-user").await?;
    let org = create_org(pool, "audit-org", None, None, Visibility::Private).await?;
    add_org_member(pool, org.id, user.id, Role::Admin).await?;

    // Create audit log entry
    let log = create_audit_log(
        pool,
        Some(org.id),
        Some(user.id),
        "org.create",
        "organization",
        Some(org.id),
        None,
        None,
        None,
    )
    .await?;

    assert_eq!(log.action, "org.create");
    assert_eq!(log.org_id, Some(org.id));
    assert_eq!(log.user_id, Some(user.id));

    // List audit logs
    let logs = list_audit_logs(pool, org.id, 10, 0).await?;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].id, log.id);

    // Cleanup
    cleanup_test_data(pool).await.ok();

    Ok(())
}
