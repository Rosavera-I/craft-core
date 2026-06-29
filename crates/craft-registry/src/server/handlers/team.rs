//! Team request handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthUser,
    db::*,
    error::{RegistryError, RegistryResult},
    server::{handlers::UserResponse, AppState},
    Role, Visibility,
};

/// Create team request
#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

/// Team response
#[derive(Debug, Serialize)]
pub struct TeamResponse {
    pub id: String,
    pub org: String,
    pub name: String,
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
    let org = get_org_by_name(state.db.pool(), &org_name).await?;

    // Check permissions (must be org member)
    let is_member = is_org_member(state.db.pool(), org.id, auth_user.user_id).await?;

    if !auth_user.is_admin && !is_member {
        return Err(RegistryError::Auth(
            "Organization membership required".to_string(),
        ));
    }

    let visibility = match req.visibility.as_deref() {
        Some("public") => Visibility::Public,
        Some("internal") => Visibility::Internal,
        _ => Visibility::Private,
    };

    let team = create_team(
        state.db.pool(),
        org.id,
        &req.name,
        req.description.as_deref(),
        visibility,
    )
    .await?;

    // Add creator as team admin
    add_team_member(state.db.pool(), team.id, auth_user.user_id, Role::Admin).await?;

    Ok(Json(TeamResponse {
        id: team.id.to_string(),
        org: org_name,
        name: team.name,
        description: team.description,
        visibility: team.visibility,
        created_at: team.created_at.to_rfc3339(),
    }))
}

/// Get team handler
pub async fn get_team_handler(
    State(state): State<Arc<AppState>>,
    Path((org_name, team_name)): Path<(String, String)>,
) -> RegistryResult<Json<TeamResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    Ok(Json(TeamResponse {
        id: team.id.to_string(),
        org: org_name,
        name: team.name,
        description: team.description,
        visibility: team.visibility,
        created_at: team.created_at.to_rfc3339(),
    }))
}

/// List teams handler
pub async fn list_teams_handler(
    State(state): State<Arc<AppState>>,
    Path(org_name): Path<String>,
) -> RegistryResult<Json<Vec<TeamResponse>>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let teams = list_teams_by_org(state.db.pool(), org.id).await?;

    Ok(Json(
        teams
            .into_iter()
            .map(|t| TeamResponse {
                id: t.id.to_string(),
                org: org_name.clone(),
                name: t.name,
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
    let is_team_admin = check_team_admin(state.db.pool(), team.id, auth_user.user_id).await;
    let is_org_admin = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !is_team_admin && !is_org_admin {
        return Err(RegistryError::Auth(
            "Team admin access required".to_string(),
        ));
    }

    Ok(Json(TeamResponse {
        id: team.id.to_string(),
        org: org_name,
        name: team.name,
        description: req.description.or(team.description),
        visibility: req
            .visibility
            .map(|v| match v.as_str() {
                "public" => "public".to_string(),
                "internal" => "internal".to_string(),
                _ => "private".to_string(),
            })
            .unwrap_or(team.visibility),
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
    let is_team_admin = check_team_admin(state.db.pool(), team.id, auth_user.user_id).await;
    let is_org_admin = check_org_role(state.db.pool(), org.id, auth_user.user_id, Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !is_team_admin && !is_org_admin {
        return Err(RegistryError::Auth(
            "Team admin access required".to_string(),
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
    Path((org_name, team_name)): Path<(String, String)>,
) -> RegistryResult<Json<Vec<TeamMemberResponse>>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;
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
    pub username: String,
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
    let can_invite = check_team_role(state.db.pool(), team.id, auth_user.user_id, Role::Maintainer)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_invite {
        return Err(RegistryError::Auth(
            "Team maintainer access required".to_string(),
        ));
    }

    let target_user = get_user_by_username(state.db.pool(), &req.username).await?;

    // Ensure target is org member first
    let is_org_member = is_org_member(state.db.pool(), org.id, target_user.id).await?;
    if !is_org_member {
        // Auto-add to org as member
        add_org_member(state.db.pool(), org.id, target_user.id, Role::Member).await?;
    }

    let role = match req.role.as_deref() {
        Some("admin") => Role::Admin,
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
    Path((org_name, team_name, username)): Path<(String, String, String)>,
) -> RegistryResult<StatusCode> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let team = get_team_by_org_and_name(state.db.pool(), org.id, &team_name).await?;

    let target_user = get_user_by_username(state.db.pool(), &username).await?;
    let is_self = target_user.id == auth_user.user_id;

    // Check permissions
    let can_remove = check_team_role(state.db.pool(), team.id, auth_user.user_id, Role::Maintainer)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_remove && !is_self {
        return Err(RegistryError::Auth(
            "Team maintainer access required".to_string(),
        ));
    }

    remove_team_member(state.db.pool(), team.id, target_user.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Helper: check if user is team admin
async fn check_team_admin(pool: &sqlx::PgPool, team_id: uuid::Uuid, user_id: uuid::Uuid) -> bool {
    let member = get_team_member(pool, team_id, user_id).await;
    matches!(member, Ok(m) if m.role == "admin")
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
        (Role::Maintainer, Role::Maintainer) | (Role::Maintainer, Role::Admin) => true,
        (Role::Admin, Role::Admin) => true,
        _ => false,
    })
}
