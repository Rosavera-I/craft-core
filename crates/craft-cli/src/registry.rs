use std::fs;
use std::path::Path;

use craft_registry::{RegistryClient, RegistryError, RegistryResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RegistryConfig {
    pub registry_url: String,
    pub auth_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrgResponse {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    pub visibility: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemberResponse {
    pub user: UserResponse,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamResponse {
    pub id: String,
    pub org: String,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreateOrgRequest<'a> {
    pub name: &'a str,
    pub display_name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub visibility: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct InviteOrgMemberRequest<'a> {
    pub email: &'a str,
    pub role: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct CreateTeamRequest<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub visibility: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct InviteTeamMemberRequest<'a> {
    pub username: &'a str,
    pub role: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct CloudRegistry {
    client: RegistryClient,
}

impl CloudRegistry {
    pub fn from_config_file(path: &Path) -> RegistryResult<Self> {
        let contents = fs::read_to_string(path).map_err(RegistryError::Io)?;
        let config: RegistryConfig = toml::from_str(&contents)
            .map_err(|err| RegistryError::Config(format!("invalid registry config: {err}")))?;

        Ok(Self {
            client: RegistryClient::new(&config.registry_url)?.with_token(config.auth_token),
        })
    }

    pub async fn list_orgs(&self) -> RegistryResult<Vec<OrgResponse>> {
        self.client.get("api/v1/user/orgs").await
    }

    pub async fn create_org(&self, request: &CreateOrgRequest<'_>) -> RegistryResult<OrgResponse> {
        self.client.post("api/v1/orgs", request).await
    }

    pub async fn get_org(&self, name: &str) -> RegistryResult<OrgResponse> {
        self.client
            .get(&format!("api/v1/user/orgs/{}", path_segment(name)?))
            .await
    }

    pub async fn list_org_members(&self, name: &str) -> RegistryResult<Vec<MemberResponse>> {
        self.client
            .get(&format!("api/v1/orgs/{}/members", path_segment(name)?))
            .await
    }

    pub async fn invite_org_member(
        &self,
        name: &str,
        request: &InviteOrgMemberRequest<'_>,
    ) -> RegistryResult<MemberResponse> {
        self.client
            .post(
                &format!("api/v1/orgs/{}/members", path_segment(name)?),
                request,
            )
            .await
    }

    pub async fn remove_org_member(&self, name: &str, user_id: &str) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!(
                "api/v1/orgs/{}/members/{}",
                path_segment(name)?,
                path_segment(user_id)?
            ))
            .await
    }

    pub async fn delete_org(&self, name: &str) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!("api/v1/orgs/{}", path_segment(name)?))
            .await
    }

    pub async fn list_teams(&self, org: &str) -> RegistryResult<Vec<TeamResponse>> {
        self.client
            .get(&format!("api/v1/orgs/{}/teams", path_segment(org)?))
            .await
    }

    pub async fn create_team(
        &self,
        org: &str,
        request: &CreateTeamRequest<'_>,
    ) -> RegistryResult<TeamResponse> {
        self.client
            .post(
                &format!("api/v1/orgs/{}/teams", path_segment(org)?),
                request,
            )
            .await
    }

    pub async fn get_team(&self, org: &str, team: &str) -> RegistryResult<TeamResponse> {
        self.client
            .get(&format!(
                "api/v1/teams/{}/{}",
                path_segment(org)?,
                path_segment(team)?
            ))
            .await
    }

    pub async fn list_team_members(
        &self,
        org: &str,
        team: &str,
    ) -> RegistryResult<Vec<MemberResponse>> {
        self.client
            .get(&format!(
                "api/v1/teams/{}/{}/members",
                path_segment(org)?,
                path_segment(team)?
            ))
            .await
    }

    pub async fn invite_team_member(
        &self,
        org: &str,
        team: &str,
        request: &InviteTeamMemberRequest<'_>,
    ) -> RegistryResult<MemberResponse> {
        self.client
            .post(
                &format!(
                    "api/v1/teams/{}/{}/members",
                    path_segment(org)?,
                    path_segment(team)?
                ),
                request,
            )
            .await
    }

    pub async fn remove_team_member(
        &self,
        org: &str,
        team: &str,
        username: &str,
    ) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!(
                "api/v1/teams/{}/{}/members/{}",
                path_segment(org)?,
                path_segment(team)?,
                path_segment(username)?
            ))
            .await
    }

    pub async fn delete_team(&self, org: &str, team: &str) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!(
                "api/v1/teams/{}/{}",
                path_segment(org)?,
                path_segment(team)?
            ))
            .await
    }
}

pub fn block_on<T>(
    future: impl std::future::Future<Output = RegistryResult<T>>,
) -> RegistryResult<T> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(RegistryError::Io)?
        .block_on(future)
}

fn path_segment(value: &str) -> RegistryResult<&str> {
    if value.is_empty() || value.contains('/') || value.contains('?') || value.contains('#') {
        return Err(RegistryError::Validation(format!(
            "invalid path segment: {value}"
        )));
    }
    Ok(value)
}
