use std::fs;
use std::path::Path;

use craft_registry::{RegistryClient, RegistryError, RegistryResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RegistryConfig {
    pub registry_url: String,
    pub auth_token: String,
    pub default_org: Option<String>,
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
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InviteOrgMemberResponse {
    Member {
        user: UserResponse,
        role: String,
        joined_at: String,
    },
    Invitation {
        id: String,
        email: String,
        role: String,
        created_at: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamResponse {
    pub id: String,
    pub org: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub visibility: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HarnessResponse {
    pub id: String,
    pub org: String,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub keywords: Option<Vec<String>>,
    pub git_repository_url: Option<String>,
    pub total_downloads: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionResponse {
    pub id: String,
    pub version: String,
    pub git_ref: Option<String>,
    pub git_commit_sha: Option<String>,
    pub description: Option<String>,
    pub readme_content: Option<String>,
    pub package_size_bytes: Option<i64>,
    pub content_sha256: String,
    pub download_count: i64,
    pub is_yanked: bool,
    pub yanked_reason: Option<String>,
    pub published_by: Option<String>,
    pub published_at: String,
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
    pub display_name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub visibility: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct InviteTeamMemberRequest<'a> {
    pub user_id: &'a str,
    pub role: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct CreateHarnessRequest<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub visibility: Option<&'a str>,
    pub keywords: Option<&'a [String]>,
    pub team: Option<&'a str>,
    pub git_repository_url: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct CloudRegistry {
    client: RegistryClient,
    default_org: Option<String>,
}

impl CloudRegistry {
    pub fn from_config_file(path: &Path) -> RegistryResult<Self> {
        let contents = fs::read_to_string(path).map_err(RegistryError::Io)?;
        let config: RegistryConfig = toml::from_str(&contents)
            .map_err(|err| RegistryError::Config(format!("invalid registry config: {err}")))?;

        Self::from_config(config, None)
    }

    pub fn from_config_file_with_registry(
        path: &Path,
        registry_url: Option<&str>,
    ) -> RegistryResult<Self> {
        let contents = fs::read_to_string(path).map_err(RegistryError::Io)?;
        let config: RegistryConfig = toml::from_str(&contents)
            .map_err(|err| RegistryError::Config(format!("invalid registry config: {err}")))?;
        Self::from_config(config, registry_url)
    }

    fn from_config(config: RegistryConfig, registry_url: Option<&str>) -> RegistryResult<Self> {
        let registry_url = registry_url.unwrap_or(&config.registry_url);
        Ok(Self {
            client: RegistryClient::new(registry_url)?.with_token(config.auth_token),
            default_org: config.default_org,
        })
    }

    pub fn default_org(&self) -> Option<&str> {
        self.default_org.as_deref()
    }

    pub async fn list_orgs(&self) -> RegistryResult<Vec<OrgResponse>> {
        self.client.get("/api/v1/orgs").await
    }

    pub async fn create_org(&self, request: &CreateOrgRequest<'_>) -> RegistryResult<OrgResponse> {
        self.client.post("/api/v1/orgs", request).await
    }

    pub async fn get_org(&self, name: &str) -> RegistryResult<OrgResponse> {
        self.client
            .get(&format!("/api/v1/orgs/{}", path_segment(name)?))
            .await
    }

    pub async fn list_org_members(&self, name: &str) -> RegistryResult<Vec<MemberResponse>> {
        self.client
            .get(&format!("/api/v1/orgs/{}/members", path_segment(name)?))
            .await
    }

    pub async fn invite_org_member(
        &self,
        name: &str,
        request: &InviteOrgMemberRequest<'_>,
    ) -> RegistryResult<InviteOrgMemberResponse> {
        self.client
            .post(
                &format!("/api/v1/orgs/{}/invites", path_segment(name)?),
                request,
            )
            .await
    }

    pub async fn remove_org_member(&self, name: &str, user_id: &str) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!(
                "/api/v1/orgs/{}/members/{}",
                path_segment(name)?,
                path_segment(user_id)?
            ))
            .await
    }

    pub async fn delete_org(&self, name: &str) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!("/api/v1/orgs/{}", path_segment(name)?))
            .await
    }

    pub async fn list_teams(&self, org: &str) -> RegistryResult<Vec<TeamResponse>> {
        self.client
            .get(&format!("/api/v1/orgs/{}/teams", path_segment(org)?))
            .await
    }

    pub async fn create_team(
        &self,
        org: &str,
        request: &CreateTeamRequest<'_>,
    ) -> RegistryResult<TeamResponse> {
        self.client
            .post(
                &format!("/api/v1/orgs/{}/teams", path_segment(org)?),
                request,
            )
            .await
    }

    pub async fn get_team(&self, org: &str, team: &str) -> RegistryResult<TeamResponse> {
        self.client
            .get(&format!(
                "/api/v1/orgs/{}/teams/{}",
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
                "/api/v1/orgs/{}/teams/{}/members",
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
                    "/api/v1/orgs/{}/teams/{}/members",
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
        user_id: &str,
    ) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!(
                "/api/v1/orgs/{}/teams/{}/members/{}",
                path_segment(org)?,
                path_segment(team)?,
                path_segment(user_id)?
            ))
            .await
    }

    pub async fn delete_team(&self, org: &str, team: &str) -> RegistryResult<()> {
        self.client
            .delete_empty(&format!(
                "/api/v1/orgs/{}/teams/{}",
                path_segment(org)?,
                path_segment(team)?
            ))
            .await
    }

    pub async fn get_harness(&self, org: &str, name: &str) -> RegistryResult<HarnessResponse> {
        self.client
            .get(&format!(
                "/api/v1/harnesses/{}/{}",
                path_segment(org)?,
                path_segment(name)?
            ))
            .await
    }

    pub async fn create_harness(
        &self,
        org: &str,
        request: &CreateHarnessRequest<'_>,
    ) -> RegistryResult<HarnessResponse> {
        self.client
            .post(
                &format!("/api/v1/harnesses/{}", path_segment(org)?),
                request,
            )
            .await
    }

    pub async fn list_harness_versions(
        &self,
        org: &str,
        name: &str,
    ) -> RegistryResult<Vec<VersionResponse>> {
        self.client
            .get(&format!(
                "/api/v1/harnesses/{}/{}/versions",
                path_segment(org)?,
                path_segment(name)?
            ))
            .await
    }

    pub async fn resolve_version(
        &self,
        org: &str,
        name: &str,
        requirement: Option<&str>,
    ) -> RegistryResult<VersionResponse> {
        let mut versions = self.list_harness_versions(org, name).await?;
        versions.retain(|version| !version.is_yanked);
        select_version(versions, org, name, requirement)
    }

    pub async fn publish_harness(
        &self,
        org: &str,
        name: &str,
        version: &str,
        description: Option<&str>,
        package: Vec<u8>,
    ) -> RegistryResult<VersionResponse> {
        let package_part = reqwest::multipart::Part::bytes(package)
            .file_name(format!("{org}-{name}-{version}.tar.gz"))
            .mime_str("application/gzip")
            .map_err(|err| {
                RegistryError::Validation(format!("invalid package MIME type: {err}"))
            })?;
        let mut form = reqwest::multipart::Form::new()
            .text("version", version.to_string())
            .part("package", package_part);
        if let Some(description) = description {
            form = form.text("description", description.to_string());
        }

        self.client
            .post_multipart(
                &format!(
                    "/api/v1/harnesses/{}/{}/versions",
                    path_segment(org)?,
                    path_segment(name)?
                ),
                form,
            )
            .await
    }

    /// Publish a complete harness package, creating its registry record when
    /// needed. This is the package-manager API used by `craft harness publish`.
    pub async fn publish_package(
        &self,
        org: &str,
        name: &str,
        version: &str,
        description: Option<&str>,
        package: Vec<u8>,
    ) -> RegistryResult<VersionResponse> {
        let content_sha256 = craft_registry::storage::compute_sha256(&package);
        let package_part = reqwest::multipart::Part::bytes(package)
            .file_name(format!("{org}-{name}-{version}.tar.gz"))
            .mime_str("application/gzip")
            .map_err(|err| {
                RegistryError::Validation(format!("invalid package MIME type: {err}"))
            })?;
        let mut form = reqwest::multipart::Form::new()
            .text("org", org.to_string())
            .text("name", name.to_string())
            .text("version", version.to_string())
            .text("content_sha256", content_sha256)
            .part("package", package_part);
        if let Some(description) = description {
            form = form.text("description", description.to_string());
        }

        self.client.post_multipart("/api/v1/packages", form).await
    }

    pub async fn download_harness(
        &self,
        org: &str,
        name: &str,
        version: &str,
    ) -> RegistryResult<Vec<u8>> {
        let bytes = self
            .client
            .download(&format!(
                "/api/v1/harnesses/{}/{}/download/{}",
                path_segment(org)?,
                path_segment(name)?,
                path_segment(version)?
            ))
            .await?;
        Ok(bytes.to_vec())
    }
}

fn select_version(
    versions: Vec<VersionResponse>,
    org: &str,
    name: &str,
    requirement: Option<&str>,
) -> RegistryResult<VersionResponse> {
    if versions.is_empty() {
        return Err(RegistryError::NotFound(format!(
            "no published versions found for {org}/{name}"
        )));
    }

    let parsed = versions
        .into_iter()
        .filter_map(|version| {
            semver::Version::parse(&version.version)
                .ok()
                .map(|parsed| (parsed, version))
        })
        .collect::<Vec<_>>();

    if let Some(requirement) = requirement {
        let req = semver::VersionReq::parse(requirement).map_err(|err| {
            RegistryError::Validation(format!(
                "invalid version requirement `{requirement}`: {err}"
            ))
        })?;
        parsed
            .into_iter()
            .filter(|(version, _)| req.matches(version))
            .max_by(|left, right| left.0.cmp(&right.0))
            .map(|(_, version)| version)
            .ok_or_else(|| {
                RegistryError::NotFound(format!("no version of {org}/{name} matches {requirement}"))
            })
    } else {
        parsed
            .iter()
            .filter(|(version, _)| version.pre.is_empty())
            .max_by(|left, right| left.0.cmp(&right.0))
            .or_else(|| parsed.iter().max_by(|left, right| left.0.cmp(&right.0)))
            .map(|(_, version)| version.clone())
            .ok_or_else(|| {
                RegistryError::NotFound(format!(
                    "no valid semantic versions found for {org}/{name}"
                ))
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn version(value: &str) -> VersionResponse {
        VersionResponse {
            id: value.to_string(),
            version: value.to_string(),
            git_ref: None,
            git_commit_sha: None,
            description: None,
            readme_content: None,
            package_size_bytes: None,
            content_sha256: format!("sha256-{value}"),
            download_count: 0,
            is_yanked: false,
            yanked_reason: None,
            published_by: None,
            published_at: String::new(),
        }
    }

    #[test]
    fn latest_version_prefers_stable_release() {
        let selected = select_version(
            vec![version("1.9.0"), version("2.0.0-beta.1"), version("1.10.0")],
            "acme",
            "designer",
            None,
        )
        .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(selected.version, "1.10.0");
    }

    #[test]
    fn version_requirement_selects_highest_match() {
        let selected = select_version(
            vec![version("1.1.0"), version("1.4.0"), version("2.0.0")],
            "acme",
            "designer",
            Some("^1.0"),
        )
        .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(selected.version, "1.4.0");
    }
}
