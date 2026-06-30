//! S3-compatible storage backend.

use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use chrono::Utc;
use reqwest::{Client, Method, StatusCode, Url, header};
use ring::hmac;
use sha2::{Digest, Sha256};

use crate::{
    error::{RegistryError, RegistryResult},
    storage::{Storage, build_storage_path, compute_sha256},
};

#[derive(Debug, Clone)]
pub struct S3Storage {
    bucket: String,
    region: String,
    endpoint: Option<String>,
    access_key_id: String,
    secret_access_key: String,
    client: Client,
}

impl S3Storage {
    pub fn new(
        bucket: &str,
        region: &str,
        endpoint: Option<&str>,
        access_key_id: &str,
        secret_access_key: &str,
    ) -> RegistryResult<Self> {
        if bucket.trim().is_empty() {
            return Err(RegistryError::Config("S3 bucket is required".to_string()));
        }
        if region.trim().is_empty() {
            return Err(RegistryError::Config("S3 region is required".to_string()));
        }
        if access_key_id.trim().is_empty() || secret_access_key.trim().is_empty() {
            return Err(RegistryError::Config(
                "S3 access key ID and secret access key are required".to_string(),
            ));
        }

        Ok(Self {
            bucket: bucket.to_string(),
            region: region.to_string(),
            endpoint: endpoint.map(|value| value.trim_end_matches('/').to_string()),
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
            client: Client::new(),
        })
    }

    fn object_url(&self, key: &str) -> RegistryResult<Url> {
        let encoded_key = encode_s3_path(key);
        let url = if let Some(endpoint) = &self.endpoint {
            format!("{}/{}/{}", endpoint, self.bucket, encoded_key)
        } else {
            format!(
                "https://{}.s3.{}.amazonaws.com/{}",
                self.bucket, self.region, encoded_key
            )
        };

        Url::parse(&url).map_err(|err| {
            RegistryError::Config(format!("invalid S3 endpoint/object URL `{url}`: {err}"))
        })
    }

    async fn request(
        &self,
        method: Method,
        key: &str,
        payload: Bytes,
    ) -> RegistryResult<reqwest::Response> {
        let url = self.object_url(key)?;
        let payload_hash = sha256_hex(&payload);
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_scope = now.format("%Y%m%d").to_string();
        let host = url
            .host_str()
            .ok_or_else(|| RegistryError::Config("S3 object URL is missing a host".to_string()))?;
        let host_header = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        let canonical_uri = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        let canonical_headers = format!(
            "host:{host_header}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
        );
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "{}\n{}\n\n{}\n{}\n{}",
            method.as_str(),
            canonical_uri,
            canonical_headers,
            signed_headers,
            payload_hash
        );
        let credential_scope = format!("{date_scope}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let signature = sign_hex(
            &self.secret_access_key,
            &date_scope,
            &self.region,
            &string_to_sign,
        );
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key_id, credential_scope, signed_headers, signature
        );

        let mut request = self
            .client
            .request(method, url)
            .header(header::HOST, host_header)
            .header("x-amz-content-sha256", payload_hash)
            .header("x-amz-date", amz_date)
            .header(header::AUTHORIZATION, authorization);

        if !payload.is_empty() {
            request = request.body(payload);
        }

        request.send().await.map_err(RegistryError::Http)
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn store(
        &self,
        org: &str,
        harness: &str,
        version: &str,
        content: &[u8],
    ) -> RegistryResult<String> {
        let hash = compute_sha256(content);
        let storage_path = build_storage_path(org, harness, version, &hash);
        let response = self
            .request(Method::PUT, &storage_path, Bytes::copy_from_slice(content))
            .await?;

        if response.status().is_success() {
            return Ok(storage_path);
        }

        Err(s3_status_error("store", response).await)
    }

    async fn retrieve(&self, path: &str) -> RegistryResult<Vec<u8>> {
        let response = self.request(Method::GET, path, Bytes::new()).await?;
        if response.status().is_success() {
            return Ok(response.bytes().await?.to_vec());
        }

        Err(s3_status_error("retrieve", response).await)
    }

    async fn retrieve_body(&self, path: &str) -> RegistryResult<Body> {
        let response = self.request(Method::GET, path, Bytes::new()).await?;
        if response.status().is_success() {
            return Ok(Body::from_stream(response.bytes_stream()));
        }

        Err(s3_status_error("stream", response).await)
    }

    async fn exists(&self, path: &str) -> RegistryResult<bool> {
        let response = self.request(Method::HEAD, path, Bytes::new()).await?;
        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            _ => Err(s3_status_error("head", response).await),
        }
    }

    async fn delete(&self, path: &str) -> RegistryResult<()> {
        let response = self.request(Method::DELETE, path, Bytes::new()).await?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }

        Err(s3_status_error("delete", response).await)
    }

    fn public_url(&self, path: &str) -> RegistryResult<String> {
        Ok(self.object_url(path)?.to_string())
    }
}

async fn s3_status_error(operation: &str, response: reqwest::Response) -> RegistryError {
    let status = response.status();
    let body = response.text().await.unwrap_or_else(|_| String::new());
    if status == StatusCode::NOT_FOUND {
        RegistryError::NotFound(format!("S3 object not found during {operation}"))
    } else {
        RegistryError::Storage(format!("S3 {operation} failed with {status}: {body}"))
    }
}

fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_ref());
    hex_bytes(&hasher.finalize())
}

fn sign_hex(secret: &str, date: &str, region: &str, string_to_sign: &str) -> String {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let date_region_key = hmac_sha256(&date_key, region.as_bytes());
    let date_region_service_key = hmac_sha256(&date_region_key, b"s3");
    let signing_key = hmac_sha256(&date_region_service_key, b"aws4_request");
    hex_bytes(&hmac_sha256(&signing_key, string_to_sign.as_bytes()))
}

fn hmac_sha256(key: &[u8], value: &[u8]) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::sign(&key, value).as_ref().to_vec()
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn encode_s3_path(path: &str) -> String {
    path.split('/')
        .map(percent_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_s3_key_segments_without_escaping_slashes() {
        assert_eq!(encode_s3_path("org/name v1/a+b"), "org/name%20v1/a%2Bb");
    }

    #[test]
    fn signs_with_expected_hex_length() {
        let signature = sign_hex(
            "secret",
            "20260630",
            "us-east-1",
            "AWS4-HMAC-SHA256\n20260630T010203Z\n20260630/us-east-1/s3/aws4_request\nabc",
        );
        assert_eq!(signature.len(), 64);
    }
}
