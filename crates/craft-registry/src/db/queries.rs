//! Database queries for the CRAFT Registry

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    Role, Visibility,
    db::*,
    error::{Context, RegistryError, RegistryResult},
};

// ============================================================================
// Organization Queries
// ============================================================================

/// Create a new organization
pub async fn create_org(
    pool: &PgPool,
    name: &str,
    display_name: Option<&str>,
    description: Option<&str>,
    visibility: Visibility,
) -> RegistryResult<Organization> {
    let visibility_str = match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    };

    let org = sqlx::query_as::<_, Organization>(
        r#"
        INSERT INTO organizations (name, display_name, description, visibility)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(display_name)
    .bind(description)
    .bind(visibility_str)
    .fetch_one(pool)
    .await?;

    Ok(org)
}

/// Create a new organization owned by the given user.
pub async fn create_owned_org(
    pool: &PgPool,
    name: &str,
    display_name: Option<&str>,
    description: Option<&str>,
    visibility: Visibility,
    owner_id: Uuid,
) -> RegistryResult<Organization> {
    let visibility_str = match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    };

    let mut tx = pool.begin().await?;
    let org = sqlx::query_as::<_, Organization>(
        r#"
        INSERT INTO organizations (name, display_name, description, visibility, owner_id)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(display_name)
    .bind(description)
    .bind(visibility_str)
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO org_members (org_id, user_id, role)
        VALUES ($1, $2, 'owner')
        ON CONFLICT (org_id, user_id) DO UPDATE SET role = 'owner'
        "#,
    )
    .bind(org.id)
    .bind(owner_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(org)
}

/// Get organization by ID
pub async fn get_org_by_id(pool: &PgPool, id: Uuid) -> RegistryResult<Organization> {
    let org = sqlx::query_as::<_, Organization>(
        r#"SELECT * FROM organizations WHERE id = $1 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .context(format!("Organization not found: {}", id))?;

    Ok(org)
}

/// Get organization by name
pub async fn get_org_by_name(pool: &PgPool, name: &str) -> RegistryResult<Organization> {
    let org = sqlx::query_as::<_, Organization>(
        r#"SELECT * FROM organizations WHERE name = $1 AND deleted_at IS NULL"#,
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    .context(format!("Organization not found: {}", name))?;

    Ok(org)
}

/// List organizations with pagination
pub async fn list_orgs(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> RegistryResult<Vec<Organization>> {
    let orgs = sqlx::query_as::<_, Organization>(
        r#"
        SELECT * FROM organizations 
        WHERE deleted_at IS NULL AND visibility = 'public'
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(orgs)
}

/// List organizations visible to a user through membership.
pub async fn list_orgs_for_user(pool: &PgPool, user_id: Uuid) -> RegistryResult<Vec<Organization>> {
    let orgs = sqlx::query_as::<_, Organization>(
        r#"
        SELECT o.*
        FROM organizations o
        INNER JOIN org_members om ON om.org_id = o.id
        WHERE om.user_id = $1 AND o.deleted_at IS NULL
        ORDER BY o.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(orgs)
}

/// Update organization
pub async fn update_org(
    pool: &PgPool,
    id: Uuid,
    display_name: Option<&str>,
    description: Option<&str>,
    visibility: Option<Visibility>,
) -> RegistryResult<Organization> {
    let visibility_str = visibility.map(|v| match v {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    });

    let org = sqlx::query_as::<_, Organization>(
        r#"
        UPDATE organizations 
        SET display_name = COALESCE($2, display_name),
            description = COALESCE($3, description),
            visibility = COALESCE($4, visibility)
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(display_name)
    .bind(description)
    .bind(visibility_str)
    .fetch_one(pool)
    .await?;

    Ok(org)
}

/// Soft delete organization
pub async fn delete_org(pool: &PgPool, id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"UPDATE organizations SET deleted_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Team Queries
// ============================================================================

/// Create a new team
pub async fn create_team(
    pool: &PgPool,
    org_id: Uuid,
    name: &str,
    description: Option<&str>,
    visibility: Visibility,
) -> RegistryResult<Team> {
    let visibility_str = match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    };

    let team = sqlx::query_as::<_, Team>(
        r#"
        INSERT INTO teams (org_id, name, description, visibility)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(org_id)
    .bind(name)
    .bind(description)
    .bind(visibility_str)
    .fetch_one(pool)
    .await?;

    Ok(team)
}

/// Create a new team with display metadata.
pub async fn create_named_team(
    pool: &PgPool,
    org_id: Uuid,
    name: &str,
    display_name: Option<&str>,
    description: Option<&str>,
    visibility: Visibility,
) -> RegistryResult<Team> {
    let visibility_str = match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    };

    let team = sqlx::query_as::<_, Team>(
        r#"
        INSERT INTO teams (org_id, name, display_name, description, visibility)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(org_id)
    .bind(name)
    .bind(display_name)
    .bind(description)
    .bind(visibility_str)
    .fetch_one(pool)
    .await?;

    Ok(team)
}

/// Update team metadata.
pub async fn update_team(
    pool: &PgPool,
    id: Uuid,
    display_name: Option<&str>,
    description: Option<&str>,
    visibility: Option<Visibility>,
) -> RegistryResult<Team> {
    let visibility_str = visibility.map(|v| match v {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    });

    let team = sqlx::query_as::<_, Team>(
        r#"
        UPDATE teams
        SET display_name = COALESCE($2, display_name),
            description = COALESCE($3, description),
            visibility = COALESCE($4, visibility)
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(display_name)
    .bind(description)
    .bind(visibility_str)
    .fetch_one(pool)
    .await?;

    Ok(team)
}

/// Get team by ID
pub async fn get_team_by_id(pool: &PgPool, id: Uuid) -> RegistryResult<Team> {
    let team = sqlx::query_as::<_, Team>(
        r#"
        SELECT * FROM teams 
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .context(format!("Team not found: {}", id))?;

    Ok(team)
}

/// Get team by org and name
pub async fn get_team_by_org_and_name(
    pool: &PgPool,
    org_id: Uuid,
    name: &str,
) -> RegistryResult<Team> {
    let team = sqlx::query_as::<_, Team>(
        r#"
        SELECT * FROM teams 
        WHERE org_id = $1 AND name = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(org_id)
    .bind(name)
    .fetch_optional(pool)
    .await?
    .context(format!("Team not found: {}/{}", org_id, name))?;

    Ok(team)
}

/// List teams in an organization
pub async fn list_teams_by_org(pool: &PgPool, org_id: Uuid) -> RegistryResult<Vec<Team>> {
    let teams = sqlx::query_as::<_, Team>(
        r#"
        SELECT * FROM teams 
        WHERE org_id = $1 AND deleted_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    Ok(teams)
}

/// Delete team
pub async fn delete_team(pool: &PgPool, id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"UPDATE teams SET deleted_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// User Queries
// ============================================================================

/// Create a new user
pub async fn create_user(
    pool: &PgPool,
    username: &str,
    email: &str,
    password_hash: Option<&str>,
    display_name: Option<&str>,
) -> RegistryResult<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (username, email, password_hash, display_name)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .fetch_one(pool)
    .await?;

    Ok(user)
}

/// Count users for first-run bootstrap checks.
pub async fn count_users(pool: &PgPool) -> RegistryResult<i64> {
    let count = sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM users"#)
        .fetch_one(pool)
        .await?;

    Ok(count)
}

/// Create the first local admin user.
pub async fn create_bootstrap_admin_user(pool: &PgPool) -> RegistryResult<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (username, email, display_name, password_hash, is_admin)
        VALUES ('admin', 'admin@local.craft', 'CRAFT Admin', NULL, TRUE)
        RETURNING *
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(user)
}

/// Get user by ID
pub async fn get_user_by_id(pool: &PgPool, id: Uuid) -> RegistryResult<User> {
    let user = sqlx::query_as::<_, User>(r#"SELECT * FROM users WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .context(format!("User not found: {}", id))?;

    Ok(user)
}

/// Get user by username
pub async fn get_user_by_username(pool: &PgPool, username: &str) -> RegistryResult<User> {
    let user = sqlx::query_as::<_, User>(r#"SELECT * FROM users WHERE username = $1"#)
        .bind(username)
        .fetch_optional(pool)
        .await?
        .context(format!("User not found: {}", username))?;

    Ok(user)
}

/// Get user by email
pub async fn get_user_by_email(pool: &PgPool, email: &str) -> RegistryResult<User> {
    let user = sqlx::query_as::<_, User>(r#"SELECT * FROM users WHERE email = $1"#)
        .bind(email)
        .fetch_optional(pool)
        .await?
        .context(format!("User not found: {}", email))?;

    Ok(user)
}

/// Create or update an SSO-backed user from GitHub profile data.
pub async fn upsert_github_user(
    pool: &PgPool,
    github_login: &str,
    email: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
) -> RegistryResult<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (username, email, display_name, avatar_url, password_hash)
        VALUES ($1, $2, $3, $4, NULL)
        ON CONFLICT (email) DO UPDATE SET
            username = EXCLUDED.username,
            display_name = COALESCE(EXCLUDED.display_name, users.display_name),
            avatar_url = COALESCE(EXCLUDED.avatar_url, users.avatar_url),
            last_login_at = NOW()
        RETURNING *
        "#,
    )
    .bind(github_login)
    .bind(email)
    .bind(display_name)
    .bind(avatar_url)
    .fetch_one(pool)
    .await?;

    Ok(user)
}

/// Update last login timestamp
pub async fn update_user_last_login(pool: &PgPool, id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"UPDATE users SET last_login_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List organization slugs for JWT claims.
pub async fn list_user_org_slugs(pool: &PgPool, user_id: Uuid) -> RegistryResult<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT o.name
        FROM organizations o
        INNER JOIN org_members om ON om.org_id = o.id
        WHERE om.user_id = $1 AND o.deleted_at IS NULL
        ORDER BY o.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// List team slugs for JWT claims as org/team.
pub async fn list_user_team_slugs(pool: &PgPool, user_id: Uuid) -> RegistryResult<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT o.name || '/' || t.name
        FROM teams t
        INNER JOIN organizations o ON o.id = t.org_id
        INNER JOIN team_members tm ON tm.team_id = t.id
        WHERE tm.user_id = $1 AND t.deleted_at IS NULL AND o.deleted_at IS NULL
        ORDER BY o.name, t.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

// ============================================================================
// Device Authorization Queries
// ============================================================================

/// Create a pending OAuth device authorization grant.
pub async fn create_device_authorization(
    pool: &PgPool,
    device_code: &str,
    user_code: &str,
    client_id: &str,
    expires_at: DateTime<Utc>,
    interval_secs: i32,
) -> RegistryResult<DeviceAuthorization> {
    let grant = sqlx::query_as::<_, DeviceAuthorization>(
        r#"
        INSERT INTO device_authorizations
            (device_code, user_code, client_id, expires_at, interval_secs)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(device_code)
    .bind(user_code)
    .bind(client_id)
    .bind(expires_at)
    .bind(interval_secs)
    .fetch_one(pool)
    .await?;

    Ok(grant)
}

/// Get a device authorization grant by device code.
pub async fn get_device_authorization_by_device_code(
    pool: &PgPool,
    device_code: &str,
) -> RegistryResult<DeviceAuthorization> {
    let grant = sqlx::query_as::<_, DeviceAuthorization>(
        r#"SELECT * FROM device_authorizations WHERE device_code = $1"#,
    )
    .bind(device_code)
    .fetch_optional(pool)
    .await?
    .context("Device authorization not found")?;

    Ok(grant)
}

/// Get a device authorization grant by user code.
pub async fn get_device_authorization_by_user_code(
    pool: &PgPool,
    user_code: &str,
) -> RegistryResult<DeviceAuthorization> {
    let grant = sqlx::query_as::<_, DeviceAuthorization>(
        r#"SELECT * FROM device_authorizations WHERE user_code = $1"#,
    )
    .bind(user_code)
    .fetch_optional(pool)
    .await?
    .context("Device authorization not found")?;

    Ok(grant)
}

/// Record a device poll attempt and optionally increase the poll interval.
pub async fn record_device_poll(
    pool: &PgPool,
    id: Uuid,
    interval_secs: i32,
) -> RegistryResult<DeviceAuthorization> {
    let grant = sqlx::query_as::<_, DeviceAuthorization>(
        r#"
        UPDATE device_authorizations
        SET poll_count = poll_count + 1,
            last_poll_at = NOW(),
            interval_secs = $2
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(interval_secs)
    .fetch_one(pool)
    .await?;

    Ok(grant)
}

/// Mark a device authorization expired.
pub async fn expire_device_authorization(
    pool: &PgPool,
    id: Uuid,
) -> RegistryResult<DeviceAuthorization> {
    let grant = sqlx::query_as::<_, DeviceAuthorization>(
        r#"
        UPDATE device_authorizations
        SET status = 'expired'
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(grant)
}

/// Approve a device authorization for a user.
pub async fn approve_device_authorization(
    pool: &PgPool,
    id: Uuid,
    user_id: Uuid,
) -> RegistryResult<DeviceAuthorization> {
    let grant = sqlx::query_as::<_, DeviceAuthorization>(
        r#"
        UPDATE device_authorizations
        SET status = 'approved',
            user_id = $2,
            approved_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(grant)
}

// ============================================================================
// Org Member Queries
// ============================================================================

/// Add member to organization
pub async fn add_org_member(
    pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
    role: Role,
) -> RegistryResult<OrgMember> {
    let role_str = match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Maintainer => "maintainer",
        Role::Member => "member",
    };

    let member = sqlx::query_as::<_, OrgMember>(
        r#"
        INSERT INTO org_members (org_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (org_id, user_id) DO UPDATE SET role = $3
        RETURNING *
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(role_str)
    .fetch_one(pool)
    .await?;

    Ok(member)
}

/// Get org membership
pub async fn get_org_member(
    pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> RegistryResult<OrgMember> {
    let member = sqlx::query_as::<_, OrgMember>(
        r#"SELECT * FROM org_members WHERE org_id = $1 AND user_id = $2"#,
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .context("Not a member of this organization")?;

    Ok(member)
}

/// Check if user is org member (any role)
pub async fn is_org_member(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> RegistryResult<bool> {
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM org_members WHERE org_id = $1 AND user_id = $2"#,
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

/// Check if user has at least the specified role in org
pub async fn check_org_role(
    pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
    min_role: Role,
) -> RegistryResult<bool> {
    let member = get_org_member(pool, org_id, user_id).await?;
    let member_role = member.role();

    Ok(matches!(
        (min_role, member_role),
        (Role::Member, _)
            | (Role::Maintainer, Role::Maintainer)
            | (Role::Maintainer, Role::Admin)
            | (Role::Maintainer, Role::Owner)
            | (Role::Admin, Role::Admin)
            | (Role::Admin, Role::Owner)
            | (Role::Owner, Role::Owner)
    ))
}

/// Remove member from organization
pub async fn remove_org_member(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> RegistryResult<()> {
    let member = get_org_member(pool, org_id, user_id).await?;
    if member.role() == Role::Owner {
        let owner_count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM org_members WHERE org_id = $1 AND role = 'owner'"#,
        )
        .bind(org_id)
        .fetch_one(pool)
        .await?;

        if owner_count <= 1 {
            return Err(RegistryError::Validation(
                "Cannot remove the last organization owner".to_string(),
            ));
        }
    }

    sqlx::query(r#"DELETE FROM org_members WHERE org_id = $1 AND user_id = $2"#)
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List org members
pub async fn list_org_members(
    pool: &PgPool,
    org_id: Uuid,
) -> RegistryResult<Vec<(OrgMember, User)>> {
    let rows = sqlx::query(
        r#"
        SELECT
            om.id AS member_id,
            om.org_id AS member_org_id,
            om.user_id AS member_user_id,
            om.role AS member_role,
            om.created_at AS member_created_at,
            om.updated_at AS member_updated_at,
            u.id AS user_id,
            u.username AS user_username,
            u.email AS user_email,
            u.display_name AS user_display_name,
            u.avatar_url AS user_avatar_url,
            u.password_hash AS user_password_hash,
            u.is_active AS user_is_active,
            u.is_admin AS user_is_admin,
            u.created_at AS user_created_at,
            u.updated_at AS user_updated_at,
            u.last_login_at AS user_last_login_at
        FROM org_members om
        JOIN users u ON om.user_id = u.id
        WHERE om.org_id = $1
        ORDER BY om.created_at DESC
        "#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for row in rows {
        let member = OrgMember {
            id: row.try_get("member_id")?,
            org_id: row.try_get("member_org_id")?,
            user_id: row.try_get("member_user_id")?,
            role: row.try_get("member_role")?,
            created_at: row.try_get("member_created_at")?,
            updated_at: row.try_get("member_updated_at")?,
        };
        let user = User {
            id: row.try_get("user_id")?,
            username: row.try_get("user_username")?,
            email: row.try_get("user_email")?,
            display_name: row.try_get("user_display_name")?,
            avatar_url: row.try_get("user_avatar_url")?,
            password_hash: row.try_get("user_password_hash")?,
            is_active: row.try_get("user_is_active")?,
            is_admin: row.try_get("user_is_admin")?,
            created_at: row.try_get("user_created_at")?,
            updated_at: row.try_get("user_updated_at")?,
            last_login_at: row.try_get("user_last_login_at")?,
        };
        results.push((member, user));
    }

    Ok(results)
}

// ============================================================================
// Team Member Queries
// ============================================================================

/// Add member to team
pub async fn add_team_member(
    pool: &PgPool,
    team_id: Uuid,
    user_id: Uuid,
    role: Role,
) -> RegistryResult<TeamMember> {
    let role_str = match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Maintainer => "maintainer",
        Role::Member => "member",
    };

    let member = sqlx::query_as::<_, TeamMember>(
        r#"
        INSERT INTO team_members (team_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (team_id, user_id) DO UPDATE SET role = $3
        RETURNING *
        "#,
    )
    .bind(team_id)
    .bind(user_id)
    .bind(role_str)
    .fetch_one(pool)
    .await?;

    Ok(member)
}

/// Get team membership
pub async fn get_team_member(
    pool: &PgPool,
    team_id: Uuid,
    user_id: Uuid,
) -> RegistryResult<TeamMember> {
    let member = sqlx::query_as::<_, TeamMember>(
        r#"SELECT * FROM team_members WHERE team_id = $1 AND user_id = $2"#,
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .context("Not a member of this team")?;

    Ok(member)
}

/// Remove member from team
pub async fn remove_team_member(pool: &PgPool, team_id: Uuid, user_id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"DELETE FROM team_members WHERE team_id = $1 AND user_id = $2"#)
        .bind(team_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List team members
pub async fn list_team_members(
    pool: &PgPool,
    team_id: Uuid,
) -> RegistryResult<Vec<(TeamMember, User)>> {
    let rows = sqlx::query(
        r#"
        SELECT
            tm.id AS member_id,
            tm.team_id AS member_team_id,
            tm.user_id AS member_user_id,
            tm.role AS member_role,
            tm.created_at AS member_created_at,
            tm.updated_at AS member_updated_at,
            u.id AS user_id,
            u.username AS user_username,
            u.email AS user_email,
            u.display_name AS user_display_name,
            u.avatar_url AS user_avatar_url,
            u.password_hash AS user_password_hash,
            u.is_active AS user_is_active,
            u.is_admin AS user_is_admin,
            u.created_at AS user_created_at,
            u.updated_at AS user_updated_at,
            u.last_login_at AS user_last_login_at
        FROM team_members tm
        JOIN users u ON tm.user_id = u.id
        WHERE tm.team_id = $1
        ORDER BY tm.created_at DESC
        "#,
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for row in rows {
        let member = TeamMember {
            id: row.try_get("member_id")?,
            team_id: row.try_get("member_team_id")?,
            user_id: row.try_get("member_user_id")?,
            role: row.try_get("member_role")?,
            created_at: row.try_get("member_created_at")?,
            updated_at: row.try_get("member_updated_at")?,
        };
        let user = User {
            id: row.try_get("user_id")?,
            username: row.try_get("user_username")?,
            email: row.try_get("user_email")?,
            display_name: row.try_get("user_display_name")?,
            avatar_url: row.try_get("user_avatar_url")?,
            password_hash: row.try_get("user_password_hash")?,
            is_active: row.try_get("user_is_active")?,
            is_admin: row.try_get("user_is_admin")?,
            created_at: row.try_get("user_created_at")?,
            updated_at: row.try_get("user_updated_at")?,
            last_login_at: row.try_get("user_last_login_at")?,
        };
        results.push((member, user));
    }

    Ok(results)
}

// ============================================================================
// Harness Queries
// ============================================================================

/// Create a new harness
#[allow(clippy::too_many_arguments)]
pub async fn create_harness(
    pool: &PgPool,
    org_id: Uuid,
    team_id: Option<Uuid>,
    name: &str,
    description: Option<&str>,
    visibility: Visibility,
    keywords: Option<&[String]>,
    git_repository_url: Option<&str>,
) -> RegistryResult<Harness> {
    let visibility_str = match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    };

    let harness = sqlx::query_as::<_, Harness>(
        r#"
        INSERT INTO harnesses (org_id, team_id, name, description, visibility, keywords, git_repository_url)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(org_id)
    .bind(team_id)
    .bind(name)
    .bind(description)
    .bind(visibility_str)
    .bind(keywords)
    .bind(git_repository_url)
    .fetch_one(pool)
    .await?;

    Ok(harness)
}

/// Get harness by ID
pub async fn get_harness_by_id(pool: &PgPool, id: Uuid) -> RegistryResult<Harness> {
    let harness = sqlx::query_as::<_, Harness>(
        r#"
        SELECT * FROM harnesses 
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .context(format!("Harness not found: {}", id))?;

    Ok(harness)
}

/// Get harness by org and name
pub async fn get_harness_by_org_and_name(
    pool: &PgPool,
    org_id: Uuid,
    name: &str,
) -> RegistryResult<Harness> {
    let harness = sqlx::query_as::<_, Harness>(
        r#"
        SELECT * FROM harnesses 
        WHERE org_id = $1 AND name = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(org_id)
    .bind(name)
    .fetch_optional(pool)
    .await?
    .context(format!("Harness not found: {}/{}", org_id, name))?;

    Ok(harness)
}

/// List harnesses in an organization
pub async fn list_harnesses_by_org(pool: &PgPool, org_id: Uuid) -> RegistryResult<Vec<Harness>> {
    let harnesses = sqlx::query_as::<_, Harness>(
        r#"
        SELECT * FROM harnesses 
        WHERE org_id = $1 AND deleted_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    Ok(harnesses)
}

/// Search harnesses by keyword
pub async fn search_harnesses(
    pool: &PgPool,
    query: &str,
    limit: i64,
) -> RegistryResult<Vec<Harness>> {
    let harnesses = sqlx::query_as::<_, Harness>(
        r#"
        SELECT * FROM harnesses 
        WHERE deleted_at IS NULL 
        AND visibility = 'public'
        AND (
            name ILIKE $1 
            OR description ILIKE $1
            OR $1 = ANY(keywords)
        )
        ORDER BY total_downloads DESC, created_at DESC
        LIMIT $2
        "#,
    )
    .bind(format!("%{}%", query))
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(harnesses)
}

/// Delete harness (soft delete)
pub async fn delete_harness(pool: &PgPool, id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"UPDATE harnesses SET deleted_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Harness Version Queries
// ============================================================================

/// Create a new harness version
#[allow(clippy::too_many_arguments)]
pub async fn create_harness_version(
    pool: &PgPool,
    harness_id: Uuid,
    version: &semver::Version,
    git_ref: Option<&str>,
    git_commit_sha: Option<&str>,
    description: Option<&str>,
    readme_content: Option<&str>,
    package_size_bytes: i64,
    content_sha256: &str,
    storage_path: &str,
    published_by: Uuid,
) -> RegistryResult<HarnessVersion> {
    let version = sqlx::query_as::<_, HarnessVersion>(
        r#"
        INSERT INTO harness_versions (
            harness_id, version, major, minor, patch, prerelease, build_metadata,
            git_ref, git_commit_sha, description, readme_content, package_size_bytes,
            content_sha256, storage_path, published_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        RETURNING *
        "#,
    )
    .bind(harness_id)
    .bind(version.to_string())
    .bind(version.major as i32)
    .bind(version.minor as i32)
    .bind(version.patch as i32)
    .bind(
        version
            .pre
            .as_str()
            .is_empty()
            .then(|| version.pre.to_string()),
    )
    .bind(
        version
            .build
            .as_str()
            .is_empty()
            .then(|| version.build.to_string()),
    )
    .bind(git_ref)
    .bind(git_commit_sha)
    .bind(description)
    .bind(readme_content)
    .bind(package_size_bytes)
    .bind(content_sha256)
    .bind(storage_path)
    .bind(published_by)
    .fetch_one(pool)
    .await?;

    Ok(version)
}

/// Get version by ID
pub async fn get_version_by_id(pool: &PgPool, id: Uuid) -> RegistryResult<HarnessVersion> {
    let version =
        sqlx::query_as::<_, HarnessVersion>(r#"SELECT * FROM harness_versions WHERE id = $1"#)
            .bind(id)
            .fetch_optional(pool)
            .await?
            .context(format!("Version not found: {}", id))?;

    Ok(version)
}

/// Get specific version of a harness
pub async fn get_harness_version(
    pool: &PgPool,
    harness_id: Uuid,
    version: &str,
) -> RegistryResult<HarnessVersion> {
    let version = sqlx::query_as::<_, HarnessVersion>(
        r#"
        SELECT * FROM harness_versions 
        WHERE harness_id = $1 AND version = $2
        "#,
    )
    .bind(harness_id)
    .bind(version)
    .fetch_optional(pool)
    .await?
    .context(format!("Version {} not found", version))?;

    Ok(version)
}

/// Get the latest version of a harness
pub async fn get_latest_harness_version(
    pool: &PgPool,
    harness_id: Uuid,
) -> RegistryResult<HarnessVersion> {
    let version = sqlx::query_as::<_, HarnessVersion>(
        r#"
        SELECT * FROM harness_versions 
        WHERE harness_id = $1 AND is_yanked = FALSE
        ORDER BY major DESC, minor DESC, patch DESC, prerelease DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(harness_id)
    .fetch_optional(pool)
    .await?
    .context("No versions found for this harness")?;

    Ok(version)
}

/// List all versions of a harness
pub async fn list_harness_versions(
    pool: &PgPool,
    harness_id: Uuid,
) -> RegistryResult<Vec<HarnessVersion>> {
    let versions = sqlx::query_as::<_, HarnessVersion>(
        r#"
        SELECT * FROM harness_versions 
        WHERE harness_id = $1
        ORDER BY major DESC, minor DESC, patch DESC, prerelease DESC NULLS LAST
        "#,
    )
    .bind(harness_id)
    .fetch_all(pool)
    .await?;

    Ok(versions)
}

/// Yank a version
pub async fn yank_version(
    pool: &PgPool,
    id: Uuid,
    reason: Option<&str>,
) -> RegistryResult<HarnessVersion> {
    let version = sqlx::query_as::<_, HarnessVersion>(
        r#"
        UPDATE harness_versions 
        SET is_yanked = TRUE, yanked_reason = $2
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(reason)
    .fetch_one(pool)
    .await?;

    Ok(version)
}

/// Unyank a version
pub async fn unyank_version(pool: &PgPool, id: Uuid) -> RegistryResult<HarnessVersion> {
    let version = sqlx::query_as::<_, HarnessVersion>(
        r#"
        UPDATE harness_versions 
        SET is_yanked = FALSE, yanked_reason = NULL
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(version)
}

/// Increment download count
pub async fn increment_download_count(pool: &PgPool, version_id: Uuid) -> RegistryResult<()> {
    sqlx::query(
        r#"
        UPDATE harness_versions 
        SET download_count = download_count + 1 
        WHERE id = $1
        "#,
    )
    .bind(version_id)
    .execute(pool)
    .await?;

    // Also increment harness total downloads
    sqlx::query(
        r#"
        UPDATE harnesses 
        SET total_downloads = total_downloads + 1 
        WHERE id = (SELECT harness_id FROM harness_versions WHERE id = $1)
        "#,
    )
    .bind(version_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Resolve version requirements (semver matching)
pub async fn resolve_version(
    pool: &PgPool,
    harness_id: Uuid,
    version_req: &str,
) -> RegistryResult<Option<HarnessVersion>> {
    // Parse the version requirement
    let req = version_req.parse::<semver::VersionReq>()?;

    // Get all non-yanked versions
    let versions = list_harness_versions(pool, harness_id).await?;

    // Find the best matching version
    let mut candidates: Vec<(semver::Version, HarnessVersion)> = Vec::new();

    for v in versions {
        if v.is_yanked {
            continue;
        }

        let semver_version = format!(
            "{}.{}.{}{}{}",
            v.major,
            v.minor,
            v.patch,
            v.prerelease
                .as_ref()
                .map(|p| format!("-{}", p))
                .unwrap_or_default(),
            v.build_metadata
                .as_ref()
                .map(|b| format!("+{}", b))
                .unwrap_or_default()
        )
        .parse::<semver::Version>()?;

        if req.matches(&semver_version) {
            candidates.push((semver_version, v));
        }
    }

    // Sort by version descending (highest first)
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    Ok(candidates.into_iter().next().map(|(_, v)| v))
}

// ============================================================================
// Access Token Queries
// ============================================================================

/// Create a new access token
#[allow(clippy::too_many_arguments)]
pub async fn create_access_token(
    pool: &PgPool,
    user_id: Uuid,
    org_id: Option<Uuid>,
    name: &str,
    token_hash: &str,
    token_prefix: &str,
    scopes: &[String],
    expires_at: Option<DateTime<Utc>>,
) -> RegistryResult<AccessToken> {
    let token = sqlx::query_as::<_, AccessToken>(
        r#"
        INSERT INTO access_tokens (user_id, org_id, name, token_hash, token_prefix, scopes, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(name)
    .bind(token_hash)
    .bind(token_prefix)
    .bind(scopes)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;

    Ok(token)
}

/// Get access token by hash
pub async fn get_access_token_by_hash(
    pool: &PgPool,
    token_hash: &str,
) -> RegistryResult<AccessToken> {
    let token = sqlx::query_as::<_, AccessToken>(
        r#"
        SELECT * FROM access_tokens 
        WHERE token_hash = $1 
        AND revoked_at IS NULL
        AND (expires_at IS NULL OR expires_at > NOW())
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?
    .context("Invalid or expired token")?;

    Ok(token)
}

/// List user's access tokens
pub async fn list_user_access_tokens(
    pool: &PgPool,
    user_id: Uuid,
) -> RegistryResult<Vec<AccessToken>> {
    let tokens = sqlx::query_as::<_, AccessToken>(
        r#"
        SELECT * FROM access_tokens 
        WHERE user_id = $1 AND revoked_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(tokens)
}

/// Revoke access token
pub async fn revoke_access_token(pool: &PgPool, id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"UPDATE access_tokens SET revoked_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update last used timestamp
pub async fn update_token_last_used(pool: &PgPool, id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"UPDATE access_tokens SET last_used_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Audit Log Queries
// ============================================================================

/// Create audit log entry
#[allow(clippy::too_many_arguments)]
pub async fn create_audit_log(
    pool: &PgPool,
    org_id: Option<Uuid>,
    user_id: Option<Uuid>,
    action: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    details: Option<serde_json::Value>,
    ip_address: Option<std::net::IpAddr>,
    user_agent: Option<&str>,
) -> RegistryResult<AuditLog> {
    let log = sqlx::query_as::<_, AuditLog>(
        r#"
        INSERT INTO audit_logs (org_id, user_id, action, resource_type, resource_id, details, ip_address, user_agent)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(details)
    .bind(ip_address)
    .bind(user_agent)
    .fetch_one(pool)
    .await?;

    Ok(log)
}

/// List audit logs for an organization
pub async fn list_audit_logs(
    pool: &PgPool,
    org_id: Uuid,
    limit: i64,
    offset: i64,
) -> RegistryResult<Vec<AuditLog>> {
    let logs = sqlx::query_as::<_, AuditLog>(
        r#"
        SELECT * FROM audit_logs 
        WHERE org_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(org_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(logs)
}

// ============================================================================
// Rate Limiting Queries
// ============================================================================

/// Check and update rate limit
pub async fn check_rate_limit(
    pool: &PgPool,
    key: &str,
    window_secs: i64,
    max_requests: i32,
) -> RegistryResult<(bool, i32, i64)> {
    let window_start = Utc::now() - Duration::seconds(window_secs);

    // Clean up old entries
    sqlx::query(r#"DELETE FROM rate_limit_entries WHERE window_start < $1"#)
        .bind(window_start)
        .execute(pool)
        .await?;

    // Try to increment existing entry
    let result = sqlx::query_as::<_, (i32, DateTime<Utc>)>(
        r#"
        UPDATE rate_limit_entries 
        SET request_count = request_count + 1, updated_at = NOW()
        WHERE key = $1 AND window_start >= $2
        RETURNING request_count, window_start
        "#,
    )
    .bind(key)
    .bind(window_start)
    .fetch_optional(pool)
    .await?;

    let (count, window) = match result {
        Some((count, window)) => (count, window),
        None => {
            // Create new entry
            sqlx::query(
                r#"
                INSERT INTO rate_limit_entries (key, window_start, request_count)
                VALUES ($1, NOW(), 1)
                ON CONFLICT (key) DO UPDATE SET request_count = 1, window_start = NOW()
            "#,
            )
            .bind(key)
            .execute(pool)
            .await?;
            (1, Utc::now())
        }
    };

    let allowed = count <= max_requests;
    let reset_secs = (window + Duration::seconds(window_secs) - Utc::now()).num_seconds();

    Ok((allowed, count, reset_secs.max(0)))
}

// ============================================================================
// Webhook Queries
// ============================================================================

/// Create webhook
pub async fn create_webhook(
    pool: &PgPool,
    org_id: Uuid,
    harness_id: Option<Uuid>,
    name: &str,
    url: &str,
    secret: Option<&str>,
    events: &[String],
) -> RegistryResult<Webhook> {
    let webhook = sqlx::query_as::<_, Webhook>(
        r#"
        INSERT INTO webhooks (org_id, harness_id, name, url, secret, events)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(org_id)
    .bind(harness_id)
    .bind(name)
    .bind(url)
    .bind(secret)
    .bind(events)
    .fetch_one(pool)
    .await?;

    Ok(webhook)
}

/// Get webhook by ID
pub async fn get_webhook_by_id(pool: &PgPool, id: Uuid) -> RegistryResult<Webhook> {
    let webhook = sqlx::query_as::<_, Webhook>(r#"SELECT * FROM webhooks WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .context(format!("Webhook not found: {}", id))?;

    Ok(webhook)
}

/// List webhooks for org/harness
pub async fn list_webhooks(
    pool: &PgPool,
    org_id: Uuid,
    harness_id: Option<Uuid>,
) -> RegistryResult<Vec<Webhook>> {
    let webhooks = match harness_id {
        Some(hid) => {
            sqlx::query_as::<_, Webhook>(
                r#"
                SELECT * FROM webhooks 
                WHERE org_id = $1 AND harness_id = $2 AND is_active = TRUE
                ORDER BY created_at DESC
                "#,
            )
            .bind(org_id)
            .bind(hid)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, Webhook>(
                r#"
                SELECT * FROM webhooks 
                WHERE org_id = $1 AND is_active = TRUE
                ORDER BY created_at DESC
                "#,
            )
            .bind(org_id)
            .fetch_all(pool)
            .await?
        }
    };

    Ok(webhooks)
}

/// Update webhook last triggered
pub async fn update_webhook_triggered(
    pool: &PgPool,
    id: Uuid,
    error: Option<&str>,
) -> RegistryResult<()> {
    sqlx::query(
        r#"
        UPDATE webhooks 
        SET last_triggered_at = NOW(), last_error = $2
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete webhook
pub async fn delete_webhook(pool: &PgPool, id: Uuid) -> RegistryResult<()> {
    sqlx::query(r#"DELETE FROM webhooks WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
