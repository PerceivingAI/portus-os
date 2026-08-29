use crate::{
    PROTOCOL_VERSION, SecretMaterial, validate_credential_ref, validate_operation_id,
    validate_provider_id,
};
use portus_protocol::RequestId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
        impl $name {
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
    };
}

string_enum!(ProviderErrorCode {
    InvalidRequest => "invalid_request",
    PermissionDenied => "permission_denied",
    ApprovalRequired => "approval_required",
    CredentialNotFound => "credential_not_found",
    CredentialRevoked => "credential_revoked",
    ProviderDefinitionInvalid => "provider_definition_invalid",
    OperationNotAllowed => "operation_not_allowed",
    RequestTooLarge => "request_too_large",
    ResponseTooLarge => "response_too_large",
    Timeout => "timeout",
    TlsError => "tls_error",
    RedirectRejected => "redirect_rejected",
    UpstreamError => "upstream_error",
    StoreUnavailable => "store_unavailable",
    Internal => "internal",
});

string_enum!(CredentialState {
    Active => "active",
    Revoked => "revoked",
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderError {
    pub code: ProviderErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
}

impl ProviderError {
    #[must_use]
    pub fn new(code: ProviderErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            upstream_status: None,
        }
    }

    #[must_use]
    pub fn with_upstream_status(mut self, status: u16) -> Self {
        self.upstream_status = Some(status);
        self
    }
}

impl std::error::Error for ProviderError {}
impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialMetadata {
    pub credential_ref: String,
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_label: Option<String>,
    pub generation: u64,
    pub state: CredentialState,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseRequest {
    pub protocol_version: u32,
    pub request_id: RequestId,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

impl UseRequest {
    #[must_use]
    pub fn new(action: UseAction) -> Self {
        let mut request = Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: RequestId::new(),
            action: String::new(),
            credential_ref: None,
            operation: None,
            payload: None,
        };
        match action {
            UseAction::CredentialList => request.action = "credential.list".into(),
            UseAction::CredentialShow { credential_ref } => {
                request.action = "credential.show".into();
                request.credential_ref = Some(credential_ref);
            }
            UseAction::Request {
                credential_ref,
                operation,
                payload,
            } => {
                request.action = "request".into();
                request.credential_ref = Some(credential_ref);
                request.operation = Some(operation);
                request.payload = Some(payload);
            }
            UseAction::Health => request.action = "health".into(),
        }
        request
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProviderError::new(
                ProviderErrorCode::InvalidRequest,
                "incompatible protected API protocol version",
            ));
        }
        match self.action.as_str() {
            "credential.list" | "health"
                if self.credential_ref.is_none()
                    && self.operation.is_none()
                    && self.payload.is_none() =>
            {
                Ok(())
            }
            "credential.show" if self.operation.is_none() && self.payload.is_none() => {
                validate_credential_ref(self.credential_ref.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "credential.show requires credential_ref",
                    )
                })?)
            }
            "request" => {
                validate_credential_ref(self.credential_ref.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "request requires credential_ref",
                    )
                })?)?;
                validate_operation_id(self.operation.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "request requires operation",
                    )
                })?)?;
                if self.payload.is_none() {
                    return Err(ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "request requires payload",
                    ));
                }
                Ok(())
            }
            _ => Err(ProviderError::new(
                ProviderErrorCode::InvalidRequest,
                "protected API use action or field combination is invalid",
            )),
        }
    }

    pub fn into_action(self) -> Result<UseAction, ProviderError> {
        self.validate()?;
        match self.action.as_str() {
            "credential.list" => Ok(UseAction::CredentialList),
            "credential.show" => Ok(UseAction::CredentialShow {
                credential_ref: self.credential_ref.expect("validated credential_ref"),
            }),
            "request" => Ok(UseAction::Request {
                credential_ref: self.credential_ref.expect("validated credential_ref"),
                operation: self.operation.expect("validated operation"),
                payload: self.payload.expect("validated payload"),
            }),
            "health" => Ok(UseAction::Health),
            _ => unreachable!("validated action"),
        }
    }
}

#[derive(Debug)]
pub enum UseAction {
    CredentialList,
    CredentialShow {
        credential_ref: String,
    },
    Request {
        credential_ref: String,
        operation: String,
        payload: Value,
    },
    Health,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminRequest {
    pub protocol_version: u32,
    pub request_id: RequestId,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretMaterial>,
}

impl AdminRequest {
    #[must_use]
    pub fn new(action: AdminAction) -> Self {
        let mut request = Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: RequestId::new(),
            action: String::new(),
            credential_ref: None,
            provider_id: None,
            safe_label: None,
            secret: None,
        };
        match action {
            AdminAction::CredentialProvision {
                credential_ref,
                provider_id,
                safe_label,
                secret,
            } => {
                request.action = "credential.provision".into();
                request.credential_ref = Some(credential_ref);
                request.provider_id = Some(provider_id);
                request.safe_label = safe_label;
                request.secret = Some(secret);
            }
            AdminAction::CredentialRotate {
                credential_ref,
                secret,
            } => {
                request.action = "credential.rotate".into();
                request.credential_ref = Some(credential_ref);
                request.secret = Some(secret);
            }
            AdminAction::CredentialRevoke { credential_ref } => {
                request.action = "credential.revoke".into();
                request.credential_ref = Some(credential_ref);
            }
            AdminAction::CredentialDelete { credential_ref } => {
                request.action = "credential.delete".into();
                request.credential_ref = Some(credential_ref);
            }
            AdminAction::CredentialShow { credential_ref } => {
                request.action = "credential.show".into();
                request.credential_ref = Some(credential_ref);
            }
            AdminAction::CredentialList => request.action = "credential.list".into(),
        }
        request
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProviderError::new(
                ProviderErrorCode::InvalidRequest,
                "incompatible protected API protocol version",
            ));
        }
        match self.action.as_str() {
            "credential.provision" => {
                validate_credential_ref(self.credential_ref.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "provision requires credential_ref",
                    )
                })?)?;
                validate_provider_id(self.provider_id.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "provision requires provider_id",
                    )
                })?)?;
                validate_safe_label(self.safe_label.as_deref())?;
                if self.secret.is_none() {
                    return Err(ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "provision requires secret input",
                    ));
                }
                Ok(())
            }
            "credential.rotate" => {
                validate_credential_ref(self.credential_ref.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "rotate requires credential_ref",
                    )
                })?)?;
                if self.provider_id.is_some() || self.safe_label.is_some() || self.secret.is_none()
                {
                    return Err(ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "rotate field combination is invalid",
                    ));
                }
                Ok(())
            }
            "credential.revoke" | "credential.delete" | "credential.show" => {
                validate_credential_ref(self.credential_ref.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "credential action requires credential_ref",
                    )
                })?)?;
                if self.provider_id.is_some() || self.safe_label.is_some() || self.secret.is_some()
                {
                    return Err(ProviderError::new(
                        ProviderErrorCode::InvalidRequest,
                        "credential action contains forbidden fields",
                    ));
                }
                Ok(())
            }
            "credential.list"
                if self.credential_ref.is_none()
                    && self.provider_id.is_none()
                    && self.safe_label.is_none()
                    && self.secret.is_none() =>
            {
                Ok(())
            }
            _ => Err(ProviderError::new(
                ProviderErrorCode::InvalidRequest,
                "protected API admin action or field combination is invalid",
            )),
        }
    }

    pub fn into_action(mut self) -> Result<AdminAction, ProviderError> {
        self.validate()?;
        match self.action.as_str() {
            "credential.provision" => Ok(AdminAction::CredentialProvision {
                credential_ref: self
                    .credential_ref
                    .take()
                    .expect("validated credential_ref"),
                provider_id: self.provider_id.take().expect("validated provider_id"),
                safe_label: self.safe_label.take(),
                secret: self.secret.take().expect("validated secret"),
            }),
            "credential.rotate" => Ok(AdminAction::CredentialRotate {
                credential_ref: self
                    .credential_ref
                    .take()
                    .expect("validated credential_ref"),
                secret: self.secret.take().expect("validated secret"),
            }),
            "credential.revoke" => Ok(AdminAction::CredentialRevoke {
                credential_ref: self
                    .credential_ref
                    .take()
                    .expect("validated credential_ref"),
            }),
            "credential.delete" => Ok(AdminAction::CredentialDelete {
                credential_ref: self
                    .credential_ref
                    .take()
                    .expect("validated credential_ref"),
            }),
            "credential.show" => Ok(AdminAction::CredentialShow {
                credential_ref: self
                    .credential_ref
                    .take()
                    .expect("validated credential_ref"),
            }),
            "credential.list" => Ok(AdminAction::CredentialList),
            _ => unreachable!("validated action"),
        }
    }
}

#[derive(Debug)]
pub enum AdminAction {
    CredentialProvision {
        credential_ref: String,
        provider_id: String,
        safe_label: Option<String>,
        secret: SecretMaterial,
    },
    CredentialRotate {
        credential_ref: String,
        secret: SecretMaterial,
    },
    CredentialRevoke {
        credential_ref: String,
    },
    CredentialDelete {
        credential_ref: String,
    },
    CredentialShow {
        credential_ref: String,
    },
    CredentialList,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderResponse {
    pub protocol_version: u32,
    pub request_id: RequestId,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<Vec<CredentialMetadata>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<CredentialMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_write_failures: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_credential_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProviderError>,
}

impl ProviderResponse {
    #[must_use]
    pub fn success(request_id: RequestId, result: ProviderSuccess) -> Self {
        let mut response = Self::empty(request_id, true);
        match result {
            ProviderSuccess::CredentialList { credentials } => {
                response.credentials = Some(credentials)
            }
            ProviderSuccess::CredentialShow { credential }
            | ProviderSuccess::CredentialMutation { credential } => {
                response.credential = Some(credential)
            }
            ProviderSuccess::Request {
                provider_id,
                operation,
                upstream_status,
                body,
            } => {
                response.provider_id = Some(provider_id);
                response.operation = Some(operation);
                response.upstream_status = Some(upstream_status);
                response.body = Some(body);
            }
            ProviderSuccess::Health {
                health,
                reason_code,
                credential_count,
                provider_count,
                audit_write_failures,
            } => {
                response.health = Some(health);
                response.reason_code = Some(reason_code);
                response.credential_count = Some(credential_count);
                response.provider_count = Some(provider_count);
                response.audit_write_failures = Some(audit_write_failures);
            }
            ProviderSuccess::CredentialDeleted { credential_ref } => {
                response.deleted_credential_ref = Some(credential_ref)
            }
        }
        response
    }

    #[must_use]
    pub fn failure(request_id: RequestId, error: ProviderError) -> Self {
        let mut response = Self::empty(request_id, false);
        response.error = Some(error);
        response
    }

    fn empty(request_id: RequestId, ok: bool) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            ok,
            provider_id: None,
            operation: None,
            upstream_status: None,
            body: None,
            credentials: None,
            credential: None,
            health: None,
            reason_code: None,
            credential_count: None,
            provider_count: None,
            audit_write_failures: None,
            deleted_credential_ref: None,
            error: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(invalid_response());
        }
        if !self.ok {
            if self.error.is_some() && self.success_field_count() == 0 {
                return Ok(());
            }
            return Err(invalid_response());
        }
        if self.error.is_some() {
            return Err(invalid_response());
        }
        let request_shape = self.provider_id.is_some()
            && self.operation.is_some()
            && self.upstream_status.is_some()
            && self.body.is_some()
            && self.success_field_count() == 4;
        let list_shape = self.credentials.is_some() && self.success_field_count() == 1;
        let credential_shape = self.credential.is_some() && self.success_field_count() == 1;
        let health_shape = self.health.is_some()
            && self.reason_code.is_some()
            && self.credential_count.is_some()
            && self.provider_count.is_some()
            && self.audit_write_failures.is_some()
            && self.success_field_count() == 5;
        let delete_shape = self.deleted_credential_ref.is_some() && self.success_field_count() == 1;
        if request_shape || list_shape || credential_shape || health_shape || delete_shape {
            Ok(())
        } else {
            Err(invalid_response())
        }
    }

    pub fn success_value(&self) -> Result<ProviderSuccess, ProviderError> {
        self.validate()?;
        if !self.ok {
            return Err(self
                .error
                .clone()
                .expect("validated failure contains error"));
        }
        if let Some(credentials) = &self.credentials {
            return Ok(ProviderSuccess::CredentialList {
                credentials: credentials.clone(),
            });
        }
        if let Some(credential) = &self.credential {
            return Ok(ProviderSuccess::CredentialShow {
                credential: credential.clone(),
            });
        }
        if let (Some(provider_id), Some(operation), Some(upstream_status), Some(body)) = (
            &self.provider_id,
            &self.operation,
            self.upstream_status,
            &self.body,
        ) {
            return Ok(ProviderSuccess::Request {
                provider_id: provider_id.clone(),
                operation: operation.clone(),
                upstream_status,
                body: body.clone(),
            });
        }
        if let (
            Some(health),
            Some(reason_code),
            Some(credential_count),
            Some(provider_count),
            Some(audit_write_failures),
        ) = (
            &self.health,
            &self.reason_code,
            self.credential_count,
            self.provider_count,
            self.audit_write_failures,
        ) {
            return Ok(ProviderSuccess::Health {
                health: health.clone(),
                reason_code: reason_code.clone(),
                credential_count,
                provider_count,
                audit_write_failures,
            });
        }
        if let Some(credential_ref) = &self.deleted_credential_ref {
            return Ok(ProviderSuccess::CredentialDeleted {
                credential_ref: credential_ref.clone(),
            });
        }
        Err(invalid_response())
    }

    fn success_field_count(&self) -> usize {
        [
            self.provider_id.is_some(),
            self.operation.is_some(),
            self.upstream_status.is_some(),
            self.body.is_some(),
            self.credentials.is_some(),
            self.credential.is_some(),
            self.health.is_some(),
            self.reason_code.is_some(),
            self.credential_count.is_some(),
            self.provider_count.is_some(),
            self.audit_write_failures.is_some(),
            self.deleted_credential_ref.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }
}

#[derive(Clone, Debug)]
pub enum ProviderSuccess {
    CredentialList {
        credentials: Vec<CredentialMetadata>,
    },
    CredentialShow {
        credential: CredentialMetadata,
    },
    Request {
        provider_id: String,
        operation: String,
        upstream_status: u16,
        body: Value,
    },
    Health {
        health: String,
        reason_code: String,
        credential_count: usize,
        provider_count: usize,
        audit_write_failures: u64,
    },
    CredentialMutation {
        credential: CredentialMetadata,
    },
    CredentialDeleted {
        credential_ref: String,
    },
}

fn invalid_response() -> ProviderError {
    ProviderError::new(
        ProviderErrorCode::InvalidRequest,
        "invalid protected API response envelope",
    )
}

fn validate_safe_label(value: Option<&str>) -> Result<(), ProviderError> {
    if value.is_some_and(|label| {
        label.trim().is_empty()
            || label.len() > crate::MAX_SAFE_LABEL_BYTES
            || label.contains(['\0', '\n', '\r'])
    }) {
        return Err(ProviderError::new(
            ProviderErrorCode::InvalidRequest,
            "safe credential label is invalid",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_success_uses_locked_flat_v1_wire_shape() {
        let response = ProviderResponse::success(
            RequestId::new(),
            ProviderSuccess::Request {
                provider_id: "openai".into(),
                operation: "openai.responses.create".into(),
                upstream_status: 200,
                body: serde_json::json!({"id":"response_1"}),
            },
        );
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["provider_id"], "openai");
        assert_eq!(value["operation"], "openai.responses.create");
        assert_eq!(value["upstream_status"], 200);
        assert!(value.get("result").is_none());
        assert!(response.validate().is_ok());
    }

    #[test]
    fn unknown_identity_or_destination_fields_are_rejected() {
        for value in [
            serde_json::json!({"protocol_version":1,"request_id":RequestId::new(),"action":"request","credential_ref":"openai/main","operation":"openai.responses.create","payload":{},"caller_uid":0}),
            serde_json::json!({"protocol_version":1,"request_id":RequestId::new(),"action":"request","credential_ref":"openai/main","operation":"openai.responses.create","payload":{},"url":"http://attacker/"}),
            serde_json::json!({"protocol_version":1,"request_id":RequestId::new(),"action":"request","credential_ref":"openai/main","operation":"openai.responses.create","payload":{},"authorization":"Bearer stolen"}),
        ] {
            assert!(serde_json::from_value::<UseRequest>(value).is_err());
        }
    }

    #[test]
    fn secret_debug_is_redacted_and_no_export_action_deserializes() {
        let secret = SecretMaterial::new("top-secret".into()).unwrap();
        assert!(!format!("{secret:?}").contains("top-secret"));
        let export = serde_json::json!({"protocol_version":1,"request_id":RequestId::new(),"action":"credential.export","credential_ref":"openai/main"});
        let parsed: AdminRequest = serde_json::from_value(export).unwrap();
        assert!(parsed.validate().is_err());
    }
}
