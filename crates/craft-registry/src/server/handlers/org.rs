//! Organization request handlers

use axum::{
    extract::{Path, Query, State},
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
    let visibility = match req.visibility.as_deref() {
        Some("public") => Visibility::Public,
        Some("internal") => Visibility::Internal,
        _ => Visibility::Private,
    };

    let org = create_org(
        state.db.pool(),
        &req.name,
        req.display_name.as_deref(),
        req.description.as_deref(),
        visibility,
    )
    .await?;

    // Add creator as admin
    add_org_member(state.db.pool(), org.id, auth_user.user_id, Role::Admin).await?;

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
        return Err(RegistryError::Auth(
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

    // Check permissions (need admin role)
    let has_admin = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !has_admin {
        return Err(RegistryError::Auth(
            "Admin access required".to_string(),
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
    Path(name): Path<String>,
) -> RegistryResult<Json<Vec<OrgMemberResponse>>> {
    let org = get_org_by_name(state.db.pool(), &name).await?;
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
    pub username: String,
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
        return Err(RegistryError::Auth(
            "Admin access required".to_string(),
        ));
    }

    let target_user = get_user_by_username(state.db.pool(), &req.username).await?;

    let role = match req.role.as_deref() {
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
    Path((name, username)): Path<(String, String)>,
) -> RegistryResult<StatusCode> {
    let org = get_org_by_name(state.db.pool(), &name).await?;

    // Check permissions
    let can_remove = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    // Users can also remove themselves
    let target_user = get_user_by_username(state.db.pool(), &username).await?;
    let is_self = target_user.id == auth_user.user_id;

    if !auth_user.is_admin && !can_remove && !is_self {
        return Err(RegistryError::Auth(
            "Admin access required".to_string(),
        ));
    }

    remove_org_member(state.db.pool(), org.id, target_user.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// Use axum's StatusCode
use axum::http::StatusCode;
