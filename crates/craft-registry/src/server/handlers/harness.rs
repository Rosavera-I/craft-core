//! Harness and version request handlers

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::Arc;
use tar::Archive;

use crate::{
    auth::AuthUser,
    db::*,
    error::{RegistryError, RegistryResult},
    server::AppState,
    storage::{compute_sha256, create_storage},
    Visibility,
};

/// Create harness request
#[derive(Debug, Deserialize)]
pub struct CreateHarnessRequest {
    pub name: String,
    pub description: Option<String>,
    pub visibility: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub team: Option<String>,
    pub git_repository_url: Option<String>,
}

/// Harness response
#[derive(Debug, Serialize)]
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

/// Create harness handler
pub async fn create_harness_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(org_name): Path<String>,
    Json(req): Json<CreateHarnessRequest>,
) -> RegistryResult<Json<HarnessResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;

    // Check permissions (must be org maintainer or higher)
    let can_create = check_org_role(state.db.pool(), org.id, auth_user.user_id, crate::Role::Maintainer)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_create {
        return Err(RegistryError::Auth(
            "Maintainer access required".to_string(),
        ));
    }

    let visibility = match req.visibility.as_deref() {
        Some("public") => Visibility::Public,
        Some("internal") => Visibility::Internal,
        _ => Visibility::Private,
    };

    // Resolve team if specified
    let team_id = if let Some(team_name) = &req.team {
        let team = get_team_by_org_and_name(state.db.pool(), org.id, team_name).await?;
        Some(team.id)
    } else {
        None
    };

    let harness = create_harness(
        state.db.pool(),
        org.id,
        team_id,
        &req.name,
        req.description.as_deref(),
        visibility,
        req.keywords.as_deref(),
        req.git_repository_url.as_deref(),
    )
    .await?;

    Ok(Json(HarnessResponse {
        id: harness.id.to_string(),
        org: org_name,
        name: harness.name,
        description: harness.description,
        visibility: harness.visibility,
        keywords: harness.keywords,
        git_repository_url: harness.git_repository_url,
        total_downloads: harness.total_downloads,
        created_at: harness.created_at.to_rfc3339(),
    }))
}

/// Get harness handler
pub async fn get_harness_handler(
    State(state): State<Arc<AppState>>,
    Path((org_name, harness_name)): Path<(String, String)>,
) -> RegistryResult<Json<HarnessResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;

    Ok(Json(HarnessResponse {
        id: harness.id.to_string(),
        org: org_name,
        name: harness.name,
        description: harness.description,
        visibility: harness.visibility,
        keywords: harness.keywords,
        git_repository_url: harness.git_repository_url,
        total_downloads: harness.total_downloads,
        created_at: harness.created_at.to_rfc3339(),
    }))
}

/// Update harness request
#[derive(Debug, Deserialize)]
pub struct UpdateHarnessRequest {
    pub description: Option<String>,
    pub visibility: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub git_repository_url: Option<String>,
}

/// Update harness handler
pub async fn update_harness_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, harness_name)): Path<(String, String)>,
    Json(req): Json<UpdateHarnessRequest>,
) -> RegistryResult<Json<HarnessResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;

    // Check permissions
    let can_update = check_org_role(state.db.pool(), org.id, auth_user.user_id, crate::Role::Maintainer)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_update {
        return Err(RegistryError::Auth(
            "Maintainer access required".to_string(),
        ));
    }

    Ok(Json(HarnessResponse {
        id: harness.id.to_string(),
        org: org_name,
        name: harness.name,
        description: req.description.or(harness.description),
        visibility: req.visibility.unwrap_or(harness.visibility),
        keywords: req.keywords.or(harness.keywords),
        git_repository_url: req.git_repository_url.or(harness.git_repository_url),
        total_downloads: harness.total_downloads,
        created_at: harness.created_at.to_rfc3339(),
    }))
}

/// Delete harness handler
pub async fn delete_harness_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, harness_name)): Path<(String, String)>,
) -> RegistryResult<StatusCode> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;

    // Check permissions
    let can_delete = check_org_role(state.db.pool(), org.id, auth_user.user_id, crate::Role::Admin)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_delete {
        return Err(RegistryError::Auth(
            "Admin access required".to_string(),
        ));
    }

    delete_harness(state.db.pool(), harness.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Version response
#[derive(Debug, Serialize)]
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

/// Publish a package in one authenticated request.
///
/// This is the package-manager-facing equivalent of creating a harness and
/// then publishing one of its versions. Multipart fields are `org`, `name`,
/// `version`, and `package`; `description`, `visibility`, and checksum or git
/// metadata are optional. The authenticated caller must be an organization
/// maintainer (or registry administrator).
pub async fn publish_package_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    mut multipart: Multipart,
) -> RegistryResult<Json<VersionResponse>> {
    let mut org_name = None;
    let mut harness_name = None;
    let mut version_str = None;
    let mut description = None;
    let mut visibility = None;
    let mut expected_sha256 = None;
    let mut package_data = None;

    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await?;
        match name.as_str() {
            "org" => org_name = Some(String::from_utf8_lossy(&data).trim().to_string()),
            "name" => harness_name = Some(String::from_utf8_lossy(&data).trim().to_string()),
            "version" => version_str = Some(String::from_utf8_lossy(&data).trim().to_string()),
            "description" => description = Some(String::from_utf8_lossy(&data).trim().to_string()),
            "visibility" => visibility = Some(String::from_utf8_lossy(&data).trim().to_string()),
            "content_sha256" | "sha256" => {
                expected_sha256 = Some(String::from_utf8_lossy(&data).trim().to_string())
            }
            "package" => package_data = Some(data.to_vec()),
            _ => {}
        }
    }

    let org_name = required_package_field("org", org_name)?;
    let harness_name = required_package_field("name", harness_name)?;
    let version_str = required_package_field("version", version_str)?;
    if !is_package_identifier(&org_name) || !is_package_identifier(&harness_name) {
        return Err(RegistryError::Validation(
            "org and name must contain only letters, numbers, hyphen, or underscore".to_string(),
        ));
    }
    let package_data = package_data
        .ok_or_else(|| RegistryError::Validation("Package file required".to_string()))?;

    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    require_publish_access(&state, &auth_user, org.id).await?;
    let harness = match get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await {
        Ok(harness) => harness,
        Err(RegistryError::NotFound(_)) => {
            let visibility = match visibility.as_deref() {
                Some("public") => Visibility::Public,
                Some("internal") => Visibility::Internal,
                Some("private") | None => Visibility::Private,
                Some(value) => {
                    return Err(RegistryError::Validation(format!(
                        "invalid package visibility `{value}`"
                    )));
                }
            };
            create_harness(
                state.db.pool(),
                org.id,
                None,
                &harness_name,
                description.as_deref(),
                visibility,
                None,
                None,
            )
            .await?
        }
        Err(error) => return Err(error),
    };

    let version = store_published_package(
        &state,
        auth_user.user_id,
        &org_name,
        &harness_name,
        harness.id,
        &version_str,
        description.as_deref(),
        expected_sha256.as_deref(),
        package_data,
    )
    .await?;

    Ok(Json(version.into()))
}

#[derive(Debug, Deserialize)]
struct PublishMetadata {
    version: Option<String>,
    git_ref: Option<String>,
    git_commit_sha: Option<String>,
    description: Option<String>,
    readme_content: Option<String>,
    content_sha256: Option<String>,
}

impl From<HarnessVersion> for VersionResponse {
    fn from(v: HarnessVersion) -> Self {
        Self {
            id: v.id.to_string(),
            version: v.version,
            git_ref: v.git_ref,
            git_commit_sha: v.git_commit_sha,
            description: v.description,
            readme_content: v.readme_content,
            package_size_bytes: v.package_size_bytes,
            content_sha256: v.content_sha256,
            download_count: v.download_count,
            is_yanked: v.is_yanked,
            yanked_reason: v.yanked_reason,
            published_by: v.published_by.map(|id| id.to_string()),
            published_at: v.published_at.to_rfc3339(),
        }
    }
}

/// List harness versions
pub async fn list_harness_versions_handler(
    State(state): State<Arc<AppState>>,
    Path((org_name, harness_name)): Path<(String, String)>,
) -> RegistryResult<Json<Vec<VersionResponse>>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;
    let versions = list_harness_versions(state.db.pool(), harness.id).await?;

    Ok(Json(versions.into_iter().map(Into::into).collect()))
}

/// Get specific version
pub async fn get_version_handler(
    State(state): State<Arc<AppState>>,
    Path((org_name, harness_name, version)): Path<(String, String, String)>,
) -> RegistryResult<Json<VersionResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;
    let version = get_harness_version(state.db.pool(), harness.id, &version).await?;

    Ok(Json(version.into()))
}

/// Search query
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// Search harnesses
pub async fn search_harnesses(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> RegistryResult<Json<Vec<HarnessResponse>>> {
    let harnesses = search_harnesses_db(state.db.pool(), &query.q, query.limit).await?;

    Ok(Json(
        harnesses
            .into_iter()
            .map(|h| HarnessResponse {
                id: h.id.to_string(),
                org: "org".to_string(), // Would need to fetch org name
                name: h.name,
                description: h.description,
                visibility: h.visibility,
                keywords: h.keywords,
                git_repository_url: h.git_repository_url,
                total_downloads: h.total_downloads,
                created_at: h.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// Publish version (multipart upload)
pub async fn publish_version_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, harness_name)): Path<(String, String)>,
    mut multipart: Multipart,
) -> RegistryResult<Json<VersionResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;

    // Check permissions
    let can_publish = check_org_role(state.db.pool(), org.id, auth_user.user_id, crate::Role::Maintainer)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_publish {
        return Err(RegistryError::Auth(
            "Maintainer access required".to_string(),
        ));
    }

    // Process multipart form
    let mut metadata: Option<PublishMetadata> = None;
    let mut version_str = None;
    let mut git_ref = None;
    let mut git_commit_sha = None;
    let mut description = None;
    let mut readme_content = None;
    let mut expected_sha256 = None;
    let mut package_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await?;

        match name.as_str() {
            "metadata" => {
                metadata = Some(serde_json::from_slice::<PublishMetadata>(&data)?);
            }
            "version" => version_str = Some(String::from_utf8_lossy(&data).trim().to_string()),
            "git_ref" => git_ref = Some(String::from_utf8_lossy(&data).trim().to_string()),
            "git_commit_sha" => {
                git_commit_sha = Some(String::from_utf8_lossy(&data).trim().to_string())
            }
            "description" => {
                description = Some(String::from_utf8_lossy(&data).trim().to_string())
            }
            "readme_content" => {
                readme_content = Some(String::from_utf8_lossy(&data).trim().to_string())
            }
            "content_sha256" | "sha256" => {
                expected_sha256 = Some(String::from_utf8_lossy(&data).trim().to_string())
            }
            "package" => package_data = Some(data.to_vec()),
            _ => {}
        }
    }

    if let Some(metadata) = metadata {
        version_str = version_str.or(metadata.version);
        git_ref = git_ref.or(metadata.git_ref);
        git_commit_sha = git_commit_sha.or(metadata.git_commit_sha);
        description = description.or(metadata.description);
        readme_content = readme_content.or(metadata.readme_content);
        expected_sha256 = expected_sha256.or(metadata.content_sha256);
    }

    let version_str = version_str.ok_or_else(|| {
        RegistryError::Validation("Version field required".to_string())
    })?;

    let semver_version = version_str.parse().map_err(RegistryError::Version)?;
    let package_data = package_data.ok_or_else(|| {
        RegistryError::Validation("Package file required".to_string())
    })?;

    // Compute hash
    let content_sha256 = compute_sha256(&package_data);
    if let Some(expected_sha256) = expected_sha256 {
        if expected_sha256 != content_sha256 {
            return Err(RegistryError::Package(format!(
                "Checksum mismatch: expected {expected_sha256}, got {content_sha256}"
            )));
        }
    }
    validate_package_manifest(&package_data, &harness_name, &version_str)?;
    let package_size = package_data.len() as i64;

    // Store package
    let storage = create_storage(&state.config.storage)?;
    let storage_path = storage
        .store(&org_name, &harness_name, &version_str, &package_data)
        .await?;

    // Create version record
    let version = create_harness_version(
        state.db.pool(),
        harness.id,
        &semver_version,
        git_ref.as_deref(),
        git_commit_sha.as_deref(),
        description.as_deref(),
        readme_content.as_deref(),
        package_size,
        &content_sha256,
        &storage_path,
        auth_user.user_id,
    )
    .await?;

    Ok(Json(version.into()))
}

fn required_package_field(
    field: &str,
    value: Option<String>,
) -> RegistryResult<String> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RegistryError::Validation(format!("{field} field required")))
}

fn is_package_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
}

async fn require_publish_access(
    state: &AppState,
    auth_user: &AuthUser,
    org_id: uuid::Uuid,
) -> RegistryResult<()> {
    let can_publish = check_org_role(
        state.db.pool(),
        org_id,
        auth_user.user_id,
        crate::Role::Maintainer,
    )
    .await
    .unwrap_or(false);
    if auth_user.is_admin || can_publish {
        Ok(())
    } else {
        Err(RegistryError::Auth(
            "Maintainer access required".to_string(),
        ))
    }
}

#[allow(clippy::too_many_arguments)]
async fn store_published_package(
    state: &AppState,
    published_by: uuid::Uuid,
    org_name: &str,
    harness_name: &str,
    harness_id: uuid::Uuid,
    version_str: &str,
    description: Option<&str>,
    expected_sha256: Option<&str>,
    package_data: Vec<u8>,
) -> RegistryResult<HarnessVersion> {
    let semver_version = version_str.parse().map_err(RegistryError::Version)?;
    let content_sha256 = compute_sha256(&package_data);
    if let Some(expected_sha256) = expected_sha256
        && expected_sha256 != content_sha256
    {
        return Err(RegistryError::Package(format!(
            "Checksum mismatch: expected {expected_sha256}, got {content_sha256}"
        )));
    }
    validate_package_manifest(&package_data, harness_name, version_str)?;

    let storage = create_storage(&state.config.storage)?;
    let storage_path = storage
        .store(org_name, harness_name, version_str, &package_data)
        .await?;
    create_harness_version(
        state.db.pool(),
        harness_id,
        &semver_version,
        None,
        None,
        description,
        None,
        package_data.len() as i64,
        &content_sha256,
        &storage_path,
        published_by,
    )
    .await
}

/// Yank version request
#[derive(Debug, Deserialize)]
pub struct YankRequest {
    pub reason: Option<String>,
}

/// Yank version
pub async fn yank_version_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, harness_name, version)): Path<(String, String, String)>,
    Json(req): Json<YankRequest>,
) -> RegistryResult<Json<VersionResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;
    let version = get_harness_version(state.db.pool(), harness.id, &version).await?;

    // Check permissions
    let can_yank = check_org_role(state.db.pool(), org.id, auth_user.user_id, crate::Role::Maintainer)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_yank {
        return Err(RegistryError::Auth(
            "Maintainer access required".to_string(),
        ));
    }

    let version = yank_version(state.db.pool(), version.id, req.reason.as_deref()).await?;

    Ok(Json(version.into()))
}

/// Unyank version
pub async fn unyank_version_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((org_name, harness_name, version)): Path<(String, String, String)>,
) -> RegistryResult<Json<VersionResponse>> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;
    let version = get_harness_version(state.db.pool(), harness.id, &version).await?;

    // Check permissions
    let can_unyank = check_org_role(state.db.pool(), org.id, auth_user.user_id, crate::Role::Maintainer)
        .await
        .unwrap_or(false);

    if !auth_user.is_admin && !can_unyank {
        return Err(RegistryError::Auth(
            "Maintainer access required".to_string(),
        ));
    }

    let version = unyank_version(state.db.pool(), version.id).await?;

    Ok(Json(version.into()))
}

/// Download version
pub async fn download_version_handler(
    State(state): State<Arc<AppState>>,
    Path((org_name, harness_name, version_str)): Path<(String, String, String)>,
) -> RegistryResult<Response> {
    let org = get_org_by_name(state.db.pool(), &org_name).await?;
    let harness = get_harness_by_org_and_name(state.db.pool(), org.id, &harness_name).await?;
    let version = get_harness_version(state.db.pool(), harness.id, &version_str).await?;

    let storage = create_storage(&state.config.storage)?;
    let body = storage.retrieve_body(&version.storage_path).await?;

    // Increment download count (fire and forget)
    let pool = state.db.pool().clone();
    let version_id = version.id;
    tokio::spawn(async move {
        let _ = increment_download_count(&pool, version_id).await;
    });

    let filename = format!("{}-{}-{}.tar.gz", org_name, harness_name, version_str);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/gzip")
        .header("x-content-sha256", version.content_sha256)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .map_err(|err| {
            RegistryError::Internal(format!("failed to build download response: {err}"))
        })?;

    Ok(response.into_response())
}

fn validate_package_manifest(
    package_data: &[u8],
    expected_name: &str,
    expected_version: &str,
) -> RegistryResult<()> {
    let decoder = GzDecoder::new(package_data);
    let mut archive = Archive::new(decoder);
    let mut manifest_contents = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path
            .file_name()
            .is_some_and(|file_name| file_name == "craft.toml")
        {
            let mut contents = String::new();
            entry.read_to_string(&mut contents)?;
            manifest_contents = Some(contents);
            break;
        }
    }

    let manifest_contents = manifest_contents.ok_or_else(|| {
        RegistryError::Package("Package must include a craft.toml manifest".to_string())
    })?;
    let manifest = craft_manifest::parse_manifest(&manifest_contents)
        .map_err(|err| RegistryError::Package(format!("Invalid craft.toml: {err}")))?;

    if manifest.harness.name != expected_name {
        return Err(RegistryError::Package(format!(
            "Manifest harness.name `{}` does not match route harness `{expected_name}`",
            manifest.harness.name
        )));
    }
    if manifest.harness.version != expected_version {
        return Err(RegistryError::Package(format!(
            "Manifest harness.version `{}` does not match uploaded version `{expected_version}`",
            manifest.harness.version
        )));
    }

    Ok(())
}
