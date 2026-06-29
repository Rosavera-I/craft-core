//! HTTP client for the CRAFT Registry API

use reqwest::{Client, ClientBuilder, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use url::Url;

use crate::error::{RegistryError, RegistryResult};

/// HTTP client for interacting with the CRAFT Registry
#[derive(Debug, Clone)]
pub struct RegistryClient {
    base_url: Url,
    auth_token: Option<String>,
    client: Client,
}

impl RegistryClient {
    /// Create a new registry client
    pub fn new(base_url: &str) -> RegistryResult<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|e| RegistryError::Validation(format!("Invalid URL: {}", e)))?;

        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(RegistryError::Http)?;

        Ok(Self {
            base_url,
            auth_token: None,
            client,
        })
    }

    /// Set authentication token
    pub fn with_token(mut self, token: String) -> Self {
        self.auth_token = Some(token);
        self
    }

    /// Build a request with auth header if available
    fn build_request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> RegistryResult<reqwest::RequestBuilder> {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| RegistryError::Validation(format!("Invalid path: {}", e)))?;

        let mut builder = self.client.request(method, url);

        if let Some(ref token) = self.auth_token {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        }

        Ok(builder)
    }

    /// GET request
    pub async fn get<T>(&self, path: &str) -> RegistryResult<T>
    where
        T: DeserializeOwned,
    {
        let response = self
            .build_request(reqwest::Method::GET, path)?
            .send()
            .await?;

        handle_response(response).await
    }

    /// POST request with body
    pub async fn post<T, B>(&self, path: &str, body: &B) -> RegistryResult<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let response = self
            .build_request(reqwest::Method::POST, path)?
            .json(body)
            .send()
            .await?;

        handle_response(response).await
    }

    /// PUT request with body
    pub async fn put<T, B>(&self, path: &str, body: &B) -> RegistryResult<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let response = self
            .build_request(reqwest::Method::PUT, path)?
            .json(body)
            .send()
            .await?;

        handle_response(response).await
    }

    /// DELETE request
    pub async fn delete<T>(&self, path: &str) -> RegistryResult<T>
    where
        T: DeserializeOwned,
    {
        let response = self
            .build_request(reqwest::Method::DELETE, path)?
            .send()
            .await?;

        handle_response(response).await
    }

    /// POST multipart request (for file uploads)
    pub async fn post_multipart<T>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> RegistryResult<T>
    where
        T: DeserializeOwned,
    {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| RegistryError::Validation(format!("Invalid path: {}", e)))?;

        let mut builder = self.client.post(url);

        if let Some(ref token) = self.auth_token {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        }

        let response = builder.multipart(form).send().await?;

        handle_response(response).await
    }

    /// Download a file
    pub async fn download(&self, path: &str) -> RegistryResult<bytes::Bytes> {
        let response = self
            .build_request(reqwest::Method::GET, path)?
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.bytes().await?)
        } else {
            Err(parse_error(response).await)
        }
    }
}

/// Handle API response
async fn handle_response<T>(response: reqwest::Response) -> RegistryResult<T>
where
    T: DeserializeOwned,
{
    if response.status().is_success() {
        Ok(response.json().await?)
    } else {
        Err(parse_error(response).await)
    }
}

/// Parse error response
async fn parse_error(response: reqwest::Response) -> RegistryError {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    match status {
        StatusCode::NOT_FOUND => RegistryError::NotFound(text),
        StatusCode::CONFLICT => RegistryError::Conflict(text),
        StatusCode::UNAUTHORIZED => RegistryError::Auth(text),
        StatusCode::TOO_MANY_REQUESTS => RegistryError::RateLimited(60),
        StatusCode::BAD_REQUEST => RegistryError::Validation(text),
        _ => RegistryError::Internal(format!("HTTP {}: {}", status, text)),
    }
}

/// Login request
#[derive(Debug, Clone, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response
#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

/// User info
#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

/// API error response
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    pub error: ApiError,
}

/// API error details
#[derive(Debug, Clone, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub status: u16,
}
