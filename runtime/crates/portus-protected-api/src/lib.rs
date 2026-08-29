//! Shared protected-API provider wire/domain/definition contract.
//!
//! This crate deliberately contains no credential store and no upstream HTTP
//! executor. It is safe to use from the daemon and its two thin clients.

mod definition;
mod protocol;
mod secret;

pub use definition::{
    AuthenticationDefinition, DefinitionCatalog, DefinitionError, DefinitionLimits,
    DefinitionPaths, DefinitionResult, DefinitionTrust, OperationDefinition, ProviderDefinition,
};
pub use protocol::{
    AdminAction, AdminRequest, CredentialMetadata, CredentialState, ProviderError,
    ProviderErrorCode, ProviderResponse, ProviderSuccess, UseAction, UseRequest,
};
pub use secret::SecretMaterial;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TIMEOUT_MS: u64 = 120_000;
pub const MAX_PROTOCOL_FRAME_BYTES: usize = MAX_REQUEST_BYTES + 64 * 1024;
pub const MAX_PROVIDER_DEFINITIONS: usize = 32;
pub const MAX_OPERATIONS_PER_PROVIDER: usize = 64;
pub const MAX_CREDENTIAL_REF_BYTES: usize = 128;
pub const MAX_PROVIDER_ID_BYTES: usize = 64;
pub const MAX_OPERATION_ID_BYTES: usize = 128;
pub const MAX_SAFE_LABEL_BYTES: usize = 192;

pub const CANONICAL_PROVIDER_DEFINITIONS_DIR: &str = "/etc/portus/protected-api/providers.d";
pub const CANONICAL_STORE_PATH: &str = "/var/lib/portus/protected-api/credentials.db";
pub const CANONICAL_USE_SOCKET: &str = "/run/portus/protected-api/use.sock";
pub const CANONICAL_ADMIN_SOCKET: &str = "/run/portus/protected-api/admin.sock";
pub const CANONICAL_AUDIT_PATH: &str = "/var/log/portus/audit/portus-apid.jsonl";
pub const CLIENT_GROUP: &str = "portus-api-users";
pub const SERVICE_USER: &str = "portus-api";
pub const SERVICE_GROUP: &str = "portus-api";
pub const POLICY_ACTION: &str = "protected-api.request";

pub fn policy_resource(credential_ref: &str, operation: &str) -> Result<String, ProviderError> {
    validate_credential_ref(credential_ref)?;
    validate_operation_id(operation)?;
    let resource = format!("{credential_ref}|{operation}");
    if resource.len() > 256 {
        return Err(ProviderError::new(
            ProviderErrorCode::InvalidRequest,
            "protected API policy resource is too long",
        ));
    }
    Ok(resource)
}

pub fn validate_credential_ref(value: &str) -> Result<(), ProviderError> {
    let mut segments = value.split('/');
    let Some(first) = segments.next() else {
        return invalid("credential reference is invalid");
    };
    let Some(second) = segments.next() else {
        return invalid("credential reference is invalid");
    };
    if segments.next().is_some()
        || value.len() > MAX_CREDENTIAL_REF_BYTES
        || !valid_id_segment(first)
        || !valid_id_segment(second)
    {
        return invalid("credential reference is invalid");
    }
    Ok(())
}

pub fn validate_provider_id(value: &str) -> Result<(), ProviderError> {
    if value.is_empty() || value.len() > MAX_PROVIDER_ID_BYTES || !valid_id_segment(value) {
        return invalid("provider id is invalid");
    }
    Ok(())
}

pub fn validate_operation_id(value: &str) -> Result<(), ProviderError> {
    if value.is_empty()
        || value.len() > MAX_OPERATION_ID_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
    {
        return invalid("operation id is invalid");
    }
    Ok(())
}

fn valid_id_segment(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn invalid<T>(message: &'static str) -> Result<T, ProviderError> {
    Err(ProviderError::new(
        ProviderErrorCode::InvalidRequest,
        message,
    ))
}
