//! Organization request handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthUser,
    db::*,
    error::{RegistryError, RegistryResult},
    server::{AppState, PaginationParams},
    Role, Visibility,
};

/// Create organization request
#[derive(Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

/// Organization response
#[derive(Debug, Serialize)]
pub struct OrgResponse {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    pub visibility: String,
    pub created_at: String,
}

impl From<Organization> for OrgResponse {
    fn from(org: Organization) -> Self {
        Self {
            id: org.id.to_string(),
            name: org.name,
            display_name: org.display_name,
            description: org.description,
            owner_id: org.owner_id.map(|id| id.to_string()),
            visibility: org.visibility,
            created_at: org.created_at.to_rfc3339(),
        }
    }
}

/// Create organization handler
pub async fn create_org_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<CreateOrgRequest>,
) -> RegistryResult<Json<OrgResponse>> {
    validate_resource_name(&req.name)?;

    let visibility = match req.visibility.as_deref() {
        Some("public") => Visibility::Public,
        Some("internal") => Visibility::Internal,
        _ => Visibility::Private,
    };

    let org = create_owned_org(
        state.db.pool(),
        &req.name,
        req.display_name.as_deref(),
        req.description.as_deref(),
        visibility,
        auth_user.user_id,
    )
    .await?;

    // Log action
    create_audit_log(
        state.db.pool(),
        Some(org.id),
        Some(auth_user.user_id),
        "org.create",
        "organization",
        Some(org.id),
        None,
        None, // IP would come from request extensions
        None,
    )
    .await
    .ok();

    Ok(Json(org.into()))
}

/// List organizations for the authenticated user.
pub async fn list_user_orgs_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> RegistryResult<Json<Vec<OrgResponse>>> {
    let orgs = if auth_user.is_admin {
        list_orgs(state.db.pool(), 100, 0).await?
    } else {
        list_orgs_for_user(state.db.pool(), auth_user.user_id).await?
    };

    Ok(Json(orgs.into_iter().map(Into::into).collect()))
}

/// Get organization (public view)
pub async fn get_org_public(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> RegistryResult<Json<OrgResponse>> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    // Only return public orgs for public endpoint
    if org.visibility() != Visibility::Public {
        return Err(RegistryError::NotFound(
            "Organization not found".to_string(),
        ));
    }

    Ok(Json(org.into()))
}

/// Get organization details for members.
pub async fn get_org_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(name): Path<String>,
) -> RegistryResult<Json<OrgResponse>> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    if !auth_user.is_admin
        && !check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Member)
            .await
            .unwrap_or(false)
    {
        return Err(RegistryError::Forbidden(
            "Organization membership required".to_string(),
        ));
    }

    Ok(Json(org.into()))
}

/// Update organization request
#[derive(Debug, Deserialize)]
pub struct UpdateOrgRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

/// Update organization handler
pub async fn update_org_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(name): Path<String>,
    Json(req): Json<UpdateOrgRequest>,
) -> RegistryResult<Json<OrgResponse>> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    // Check permissions (need admin role)
    let is_member = is_org_member(state.db.pool(), org.id, auth_user.user_id).await?;
    let has_admin_role = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !(is_member && has_admin_role) {
        return Err(RegistryError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let visibility = req.visibility.as_deref().map(|v| match v {
        "public" => Visibility::Public,
        "internal" => Visibility::Internal,
        _ => Visibility::Private,
    });

    let org = update_org(
        state.db.pool(),
        org.id,
        req.display_name.as_deref(),
        req.description.as_deref(),
        visibility,
    )
    .await?;

    Ok(Json(org.into()))
}

/// Delete organization handler
pub async fn delete_org_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(name): Path<String>,
) -> RegistryResult<StatusCode> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    let has_owner = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Owner)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !has_owner {
        return Err(RegistryError::Forbidden(
            "Owner access required".to_string(),
        ));
    }

    delete_org(state.db.pool(), org.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// List public organizations
pub async fn list_public_orgs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> RegistryResult<Json<Vec<OrgResponse>>> {
    let orgs = list_orgs(state.db.pool(), params.limit, params.offset).await?;
    Ok(Json(orgs.into_iter().map(Into::into).collect()))
}

/// Org member response
#[derive(Debug, Serialize)]
pub struct OrgMemberResponse {
    pub user: super::UserResponse,
    pub role: String,
    pub joined_at: String,
}

/// List org members handler
pub async fn list_org_members_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(name): Path<String>,
) -> RegistryResult<Json<Vec<OrgMemberResponse>>> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    if !auth_user.is_admin
        && !check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Member)
            .await
            .unwrap_or(false)
    {
        return Err(RegistryError::Forbidden(
            "Organization membership required".to_string(),
        ));
    }

    let members = list_org_members(state.db.pool(), org.id).await?;

    let response = members
        .into_iter()
        .map(|(member, user)| OrgMemberResponse {
            user: user.into(),
            role: member.role,
            joined_at: member.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(response))
}

/// Invite org member request
#[derive(Debug, Deserialize)]
pub struct InviteOrgMemberRequest {
    pub email: String,
    pub role: Option<String>,
}

/// Invite org member handler
pub async fn invite_org_member(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(name): Path<String>,
    Json(req): Json<InviteOrgMemberRequest>,
) -> RegistryResult<Json<OrgMemberResponse>> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    // Check permissions
    let can_invite = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_invite {
        return Err(RegistryError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let target_user = get_user_by_email(state.db.pool(), &req.email).await?;

    let role = match req.role.as_deref() {
        Some("owner") => Role::Owner,
        Some("admin") => Role::Admin,
        Some("maintainer") => Role::Maintainer,
        _ => Role::Member,
    };

    let member = add_org_member(state.db.pool(), org.id, target_user.id, role).await?;

    Ok(Json(OrgMemberResponse {
        user: target_user.into(),
        role: member.role,
        joined_at: member.created_at.to_rfc3339(),
    }))
}

/// Remove org member handler
pub async fn remove_org_member(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((name, user_id)): Path<(String, uuid::Uuid)>,
) -> RegistryResult<StatusCode> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    // Check permissions
    let can_remove = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    // Users can also remove themselves
    let is_self = user_id == auth_user.user_id;

    if !auth_user.is_admin && !can_remove && !is_self {
        return Err(RegistryError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    crate::db::remove_org_member(state.db.pool(), org.id, user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Update org member role request.
#[derive(Debug, Deserialize)]
pub struct UpdateOrgMemberRoleRequest {
    pub role: String,
}

/// Change an organization member role.
pub async fn update_org_member_role(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((name, user_id)): Path<(String, uuid::Uuid)>,
    Json(req): Json<UpdateOrgMemberRoleRequest>,
) -> RegistryResult<Json<OrgMemberResponse>> {
    let org = get_org_by_name(state.db.pool(), &name).await?;
    let is_owner = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Owner)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !is_owner {
        return Err(RegistryError::Forbidden(
            "Owner access required".to_string(),
        ));
    }

    let role = match req.role.as_str() {
        "owner" => Role::Owner,
        "admin" => Role::Admin,
        "maintainer" => Role::Maintainer,
        "member" => Role::Member,
        _ => {
            return Err(RegistryError::Validation(format!(
                "Invalid organization role: {}",
                req.role
            )));
        }
    };

    let target_user = get_user_by_id(state.db.pool(), user_id).await?;
    let existing_member = get_org_member(state.db.pool(), org.id, user_id).await?;
    if existing_member.role() == Role::Owner && role != Role::Owner {
        let owner_count = list_org_members(state.db.pool(), org.id)
            .await?
            .into_iter()
            .filter(|(member, _)| member.role() == Role::Owner)
            .count();

        if owner_count <= 1 {
            return Err(RegistryError::Validation(
                "Cannot demote the last organization owner".to_string(),
            ));
        }
    }

    let member = add_org_member(state.db.pool(), org.id, user_id, role).await?;

    Ok(Json(OrgMemberResponse {
        user: target_user.into(),
        role: member.role,
        joined_at: member.created_at.to_rfc3339(),
    }))
}

pub(super) fn validate_resource_name(name: &str) -> RegistryResult<()> {
    const RESERVED: &[&str] = &["admin", "api", "www", "git", "craft"];

    if name.len() < 2 || name.len() > 32 {
        return Err(RegistryError::Validation(
            "Name must be 2-32 characters".to_string(),
        ));
    }

    if RESERVED.contains(&name) {
        return Err(RegistryError::Validation(format!(
            "Name is reserved: {name}"
        )));
    }

    let bytes = name.as_bytes();
    let valid_edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    if !valid_edge(bytes[0]) || !valid_edge(bytes[bytes.len() - 1]) {
        return Err(RegistryError::Validation(
            "Name must start and end with a lowercase letter or number".to_string(),
        ));
    }

    if !bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(RegistryError::Validation(
            "Name may only contain lowercase letters, numbers, and hyphens".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_resource_name;

    #[test]
    fn resource_names_accept_url_safe_slugs() {
        assert!(validate_resource_name("my-org").is_ok());
        assert!(validate_resource_name("a1").is_ok());
        assert!(validate_resource_name("team-42").is_ok());
    }

    #[test]
    fn resource_names_reject_reserved_or_invalid_slugs() {
        for name in ["api", "A-team", "-bad", "bad-", "bad_name", "x"] {
            assert!(validate_resource_name(name).is_err(), "{name} should fail");
        }
    }
}
