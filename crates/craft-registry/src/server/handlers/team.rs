//! Team request handlers

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    Role, Visibility,
    auth::AuthUser,
    db::*,
    error::{RegistryError, RegistryResult},
    server::{AppState, handlers::UserResponse},
};

/// Create team request
#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

/// Team response
#[derive(Debug, Serialize)]
pub struct TeamResponse {
    pub id: String,
    pub org: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub visibility: String,
    pub created_at: String,
}

/// Create team handler
pub async fn create_team_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(org_name): Path<String>,
    Json(req): Json<CreateTeamRequest>,
) -> RegistryResult<Json<TeamResponse>> {
    super::org::validate_resource_name(&req.name)?;

    let org = get_org_by_name(state.db.pool(), &org_name).await?;

    let can_create = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_create {
        return Err(RegistryError::Forbidden(
            "Organization admin access required".to_string(),
        ));
    }

    let visibility = match req.visibility.as_deref() {
        Some("public") => Visibility::Public,
        Some("internal") => Visibility::Internal,
        _ => Visibility::Private,
    };

    let team = create_named_team(
        state.db.pool(),
        org.id,
        &req.name,
        req.display_name.as_deref(),
        req.description.as_deref(),
        visibility,
    )
    .await?;

    add_team_member(
        state.db.pool(),
        team.id,
        auth_user.user_id,
        Role::Maintainer,
    )
    .await?;

    Ok(Json(TeamResponse {
        id: team.id.to_string(),
        org: org_name,
        name: team.name,
        display_name: team.display_name,
        description: team.description,
        visibility: team.visibility,
        created_at: team.created_at.to_rfc3339(),
    }))
}

/// Get team handler
pub async fn get_team_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, team_name)): Path<(String, String)>,
) -> RegistryResult<Json<TeamResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    if !auth_user.is_admin
        && !check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Member)
            .await
            .unwrap_or(false)
    {
        return Err(RegistryError::Forbidden(
            "Organization membership required".to_string(),
        ));
    }

    Ok(Json(TeamResponse {
        id: team.id.to_string(),
        org: org_name,
        name: team.name,
        display_name: team.display_name,
        description: team.description,
        visibility: team.visibility,
        created_at: team.created_at.to_rfc3339(),
    }))
}

/// List teams handler
pub async fn list_teams_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(org_name): Path<String>,
) -> RegistryResult<Json<Vec<TeamResponse>>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;

    if !auth_user.is_admin
        && !check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Member)
            .await
            .unwrap_or(false)
    {
        return Err(RegistryError::Forbidden(
            "Organization membership required".to_string(),
        ));
    }

    let teams = list_teams_by_org(state.db.pool(), org.id).await?;

    Ok(Json(
        teams
            .into_iter()
            .map(|t| TeamResponse {
                id: t.id.to_string(),
                org: org_name.clone(),
                name: t.name,
                display_name: t.display_name,
                description: t.description,
                visibility: t.visibility,
                created_at: t.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// Update team request
#[derive(Debug, Deserialize)]
pub struct UpdateTeamRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

/// Update team handler
pub async fn update_team_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, team_name)): Path<(String, String)>,
    Json(req): Json<UpdateTeamRequest>,
) -> RegistryResult<Json<TeamResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    // Check permissions
    let is_team_maintainer = check_team_role(
        state.db.pool(),
        team.id,
        auth_user.user_id,
        Role::Maintainer,
    )
    .await
    .unwrap_or(false);
    let is_org_admin = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !is_team_maintainer && !is_org_admin {
        return Err(RegistryError::Forbidden(
            "Team maintainer access required".to_string(),
        ));
    }

    let visibility = req.visibility.as_deref().map(|v| match v {
        "public" => Visibility::Public,
        "internal" => Visibility::Internal,
        _ => Visibility::Private,
    });

    let team = update_team(
        state.db.pool(),
        team.id,
        req.display_name.as_deref(),
        req.description.as_deref(),
        visibility,
    )
    .await?;

    Ok(Json(TeamResponse {
        id: team.id.to_string(),
        org: org_name,
        name: team.name,
        display_name: team.display_name,
        description: team.description,
        visibility: team.visibility,
        created_at: team.created_at.to_rfc3339(),
    }))
}

/// Delete team handler
pub async fn delete_team_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, team_name)): Path<(String, String)>,
) -> RegistryResult<StatusCode> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    // Check permissions
    let is_org_admin = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !is_org_admin {
        return Err(RegistryError::Forbidden(
            "Organization admin access required".to_string(),
        ));
    }

    delete_team(state.db.pool(), team.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Team member response
#[derive(Debug, Serialize)]
pub struct TeamMemberResponse {
    pub user: UserResponse,
    pub role: String,
    pub joined_at: String,
}

/// List team members handler
pub async fn list_team_members_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, team_name)): Path<(String, String)>,
) -> RegistryResult<Json<Vec<TeamMemberResponse>>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    if !auth_user.is_admin
        && !check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Member)
            .await
            .unwrap_or(false)
    {
        return Err(RegistryError::Forbidden(
            "Organization membership required".to_string(),
        ));
    }

    let members = list_team_members(state.db.pool(), team.id).await?;

    Ok(Json(
        members
            .into_iter()
            .map(|(member, user)| TeamMemberResponse {
                user: user.into(),
                role: member.role,
                joined_at: member.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// Invite team member request
#[derive(Debug, Deserialize)]
pub struct InviteTeamMemberRequest {
    pub user_id: uuid::Uuid,
    pub role: Option<String>,
}

/// Invite team member handler
pub async fn invite_team_member(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, team_name)): Path<(String, String)>,
    Json(req): Json<InviteTeamMemberRequest>,
) -> RegistryResult<Json<TeamMemberResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    // Check permissions (team maintainer or above)
    let can_invite = check_team_role(
        state.db.pool(),
        team.id,
        auth_user.user_id,
        Role::Maintainer,
    )
    .await
    .unwrap_or(false);

    let is_org_admin = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_invite && !is_org_admin {
        return Err(RegistryError::Forbidden(
            "Team maintainer access required".to_string(),
        ));
    }

    let target_user = get_user_by_id(state.db.pool(), req.user_id).await?;

    // Ensure target is org member first
    let is_org_member = is_org_member(state.db.pool(), org.id, target_user.id).await?;
    if !is_org_member {
        // Auto-add to org as member
        add_org_member(state.db.pool(), org.id, target_user.id, Role::Member).await?;
    }

    let role = match req.role.as_deref() {
        Some("maintainer") => Role::Maintainer,
        _ => Role::Member,
    };

    let member = add_team_member(state.db.pool(), team.id, target_user.id, role).await?;

    Ok(Json(TeamMemberResponse {
        user: target_user.into(),
        role: member.role,
        joined_at: member.created_at.to_rfc3339(),
    }))
}

/// Remove team member handler
pub async fn remove_team_member(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, team_name, user_id)): Path<(String, String, uuid::Uuid)>,
) -> RegistryResult<StatusCode> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    let is_self = user_id == auth_user.user_id;

    // Check permissions
    let can_remove = check_team_role(
        state.db.pool(),
        team.id,
        auth_user.user_id,
        Role::Maintainer,
    )
    .await
    .unwrap_or(false);

    let is_org_admin = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_remove && !is_org_admin && !is_self {
        return Err(RegistryError::Forbidden(
            "Team maintainer access required".to_string(),
        ));
    }

    crate::db::remove_team_member(state.db.pool(), team.id, user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Helper: check team role
async fn check_team_role(
    pool: &sqlx::PgPool,
    team_id: uuid::Uuid,
    user_id: uuid::Uuid,
    min_role: Role,
) -> RegistryResult<bool> {
    let member = get_team_member(pool, team_id, user_id).await?;
    let member_role = member.role();

    Ok(match (min_role, member_role) {
        (Role::Member, _) => true,
        (Role::Maintainer, Role::Maintainer)
        | (Role::Maintainer, Role::Admin)
        | (Role::Maintainer, Role::Owner) => true,
        (Role::Admin, Role::Admin) | (Role::Admin, Role::Owner) => true,
        (Role::Owner, Role::Owner) => true,
        _ => false,
    })
}
