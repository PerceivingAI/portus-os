use portus_protected_api::{ProviderDefinition, ProviderError, ProviderErrorCode, SecretMaterial};
use reqwest::{
    Method,
    blocking::Client,
    header::{HeaderName, HeaderValue},
};
use std::{io::Read, time::Duration};
use zeroize::Zeroizing;

pub struct UpstreamRequest<'a> {
    pub definition: &'a ProviderDefinition,
    pub operation: &'a str,
    pub body: Vec<u8>,
    pub secret: &'a SecretMaterial,
}

#[derive(Clone, Debug)]
pub struct UpstreamResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait UpstreamTransport: Send + Sync {
    fn execute(&self, request: UpstreamRequest<'_>) -> Result<UpstreamResponse, ProviderError>;
}

#[derive(Default)]
pub struct HttpsUpstream;

impl UpstreamTransport for HttpsUpstream {
    fn execute(&self, request: UpstreamRequest<'_>) -> Result<UpstreamResponse, ProviderError> {
        let operation = request.definition.operation(request.operation)?;
        let url = request.definition.operation_url(request.operation)?;
        if url.scheme() != "https" {
            return Err(ProviderError::new(
                ProviderErrorCode::TlsError,
                "protected upstream requires HTTPS",
            ));
        }
        let timeout = Duration::from_millis(request.definition.limits.timeout_ms);
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .timeout(timeout)
            .build()
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorCode::TlsError,
                    "failed to construct verified TLS client",
                )
            })?;
        let method = Method::from_bytes(operation.method.as_bytes()).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::ProviderDefinitionInvalid,
                "provider HTTP method is invalid",
            )
        })?;
        let header_name = HeaderName::from_bytes(
            request.definition.authentication.header.as_bytes(),
        )
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::ProviderDefinitionInvalid,
                "provider authentication header is invalid",
            )
        })?;
        let auth = Zeroizing::new(format!(
            "{}{}",
            request.definition.authentication.prefix,
            request.secret.expose_for_serialization()
        ));
        let header_value = HeaderValue::from_str(&auth).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::ProviderDefinitionInvalid,
                "provider authentication value is invalid",
            )
        })?;
        let response = client
            .request(method, url)
            .header(header_name, header_value)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request.body)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    ProviderError::new(
                        ProviderErrorCode::Timeout,
                        "protected upstream request timed out",
                    )
                } else if error.is_connect() {
                    ProviderError::new(
                        ProviderErrorCode::TlsError,
                        "verified TLS upstream connection failed",
                    )
                } else {
                    ProviderError::new(
                        ProviderErrorCode::UpstreamError,
                        "protected upstream transport failed",
                    )
                }
            })?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            return Err(ProviderError::new(
                ProviderErrorCode::RedirectRejected,
                "credential-bearing redirect was rejected",
            )
            .with_upstream_status(status));
        }
        if !response.status().is_success() {
            return Err(ProviderError::new(
                ProviderErrorCode::UpstreamError,
                "protected upstream returned a non-success status",
            )
            .with_upstream_status(status));
        }
        if response
            .content_length()
            .is_some_and(|length| length > request.definition.limits.max_response_bytes as u64)
        {
            return Err(ProviderError::new(
                ProviderErrorCode::ResponseTooLarge,
                "protected upstream response exceeds configured bound",
            ));
        }
        let mut body = Vec::new();
        response
            .take(request.definition.limits.max_response_bytes as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorCode::UpstreamError,
                    "failed to read protected upstream response",
                )
            })?;
        if body.len() > request.definition.limits.max_response_bytes {
            return Err(ProviderError::new(
                ProviderErrorCode::ResponseTooLarge,
                "protected upstream response exceeds configured bound",
            ));
        }
        Ok(UpstreamResponse { status, body })
    }
}
