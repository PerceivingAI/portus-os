use crate::{RequestId, SemanticErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(u32);

impl ProtocolVersion {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    pub fn ensure_compatible(self) -> Result<(), ProtocolError> {
        if self == CURRENT_PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(ProtocolError::IncompatibleVersion {
                expected: CURRENT_PROTOCOL_VERSION,
                received: self,
            })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    IncompatibleVersion {
        expected: ProtocolVersion,
        received: ProtocolVersion,
    },
    EmptyMethod,
    InvalidResponseShape,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompatibleVersion { expected, received } => write!(
                f,
                "incompatible protocol version: expected {}, received {}",
                expected.get(),
                received.get()
            ),
            Self::EmptyMethod => f.write_str("runtime request method must not be empty"),
            Self::InvalidResponseShape => {
                f.write_str("runtime response must contain exactly one success result or error")
            }
        }
    }
}

impl Error for ProtocolError {}

impl ProtocolError {
    #[must_use]
    pub const fn semantic_code(&self) -> SemanticErrorCode {
        match self {
            Self::IncompatibleVersion { .. } => SemanticErrorCode::IncompatibleProtocol,
            Self::EmptyMethod | Self::InvalidResponseShape => SemanticErrorCode::ProtocolError,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SemanticError {
    pub code: SemanticErrorCode,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, Value>,
}

impl SemanticError {
    #[must_use]
    pub fn new(code: SemanticErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
            details: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }
}

impl fmt::Debug for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticError")
            .field("code", &self.code)
            .field("message", &self.message)
            .field("retryable", &self.retryable)
            .field("details", &"<omitted>")
            .finish()
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequestEnvelope<T> {
    pub version: ProtocolVersion,
    pub request_id: RequestId,
    pub method: String,
    pub params: T,
}

impl<T> RequestEnvelope<T> {
    #[must_use]
    pub fn new(method: impl Into<String>, params: T) -> Self {
        Self {
            version: CURRENT_PROTOCOL_VERSION,
            request_id: RequestId::new(),
            method: method.into(),
            params,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.version.ensure_compatible()?;
        if self.method.is_empty() {
            return Err(ProtocolError::EmptyMethod);
        }
        Ok(())
    }
}

impl<T> fmt::Debug for RequestEnvelope<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestEnvelope")
            .field("version", &self.version)
            .field("request_id", &self.request_id)
            .field("method", &self.method)
            .field("params", &"<omitted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>", serialize = "T: Serialize"))]
pub struct ResponseEnvelope<T> {
    pub version: ProtocolVersion,
    pub request_id: RequestId,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SemanticError>,
}

impl<T> ResponseEnvelope<T> {
    #[must_use]
    pub fn success(request_id: RequestId, result: T) -> Self {
        Self {
            version: CURRENT_PROTOCOL_VERSION,
            request_id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(request_id: RequestId, error: SemanticError) -> Self {
        Self {
            version: CURRENT_PROTOCOL_VERSION,
            request_id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.version.ensure_compatible()?;
        match (self.ok, self.result.is_some(), self.error.is_some()) {
            (true, true, false) | (false, false, true) => Ok(()),
            _ => Err(ProtocolError::InvalidResponseShape),
        }
    }
}

impl<T> fmt::Debug for ResponseEnvelope<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ResponseEnvelope");
        debug
            .field("version", &self.version)
            .field("request_id", &self.request_id)
            .field("ok", &self.ok);
        if let Some(error) = &self.error {
            debug.field("error", error);
        } else {
            debug.field("result", &"<omitted>");
        }
        debug.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trip_matches_runtime_envelope_shape() {
        let request = RequestEnvelope::new("index.query", json!({"type": "application"}));
        request.validate().unwrap();

        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["version"], 1);
        assert_eq!(encoded["method"], "index.query");
        assert!(encoded["request_id"].as_str().unwrap().starts_with("req_"));
        assert_eq!(encoded["params"]["type"], "application");

        let decoded: RequestEnvelope<Value> = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn additive_unknown_request_fields_are_ignored_within_same_version() {
        let request_id = RequestId::new();
        let encoded = json!({
            "version": 1,
            "request_id": request_id,
            "method": "health.list",
            "params": {},
            "future_additive_field": {"ignored": true}
        });

        let decoded: RequestEnvelope<Value> = serde_json::from_value(encoded).unwrap();
        decoded.validate().unwrap();
    }

    #[test]
    fn incompatible_protocol_fails_closed() {
        let request = RequestEnvelope {
            version: ProtocolVersion::new(2),
            request_id: RequestId::new(),
            method: "index.query".to_owned(),
            params: json!({}),
        };
        let error = request.validate().unwrap_err();
        assert_eq!(
            error.semantic_code(),
            SemanticErrorCode::IncompatibleProtocol
        );
    }

    #[test]
    fn empty_method_is_rejected() {
        let request = RequestEnvelope::new("", json!({}));
        assert_eq!(request.validate(), Err(ProtocolError::EmptyMethod));
    }

    #[test]
    fn success_and_failure_responses_enforce_shape() {
        let request_id = RequestId::new();
        let success = ResponseEnvelope::success(request_id, json!({"count": 1}));
        success.validate().unwrap();

        let failure: ResponseEnvelope<Value> = ResponseEnvelope::failure(
            request_id,
            SemanticError::new(
                SemanticErrorCode::ProviderUnavailable,
                "provider unavailable",
            )
            .retryable(true),
        );
        failure.validate().unwrap();

        let invalid = ResponseEnvelope {
            version: CURRENT_PROTOCOL_VERSION,
            request_id,
            ok: true,
            result: Some(json!({})),
            error: Some(SemanticError::new(SemanticErrorCode::Internal, "invalid")),
        };
        assert_eq!(invalid.validate(), Err(ProtocolError::InvalidResponseShape));
    }

    #[test]
    fn debug_omits_generic_payloads_and_error_details() {
        let request = RequestEnvelope::new("test.method", json!({"private": "do-not-log"}));
        let request_debug = format!("{request:?}");
        assert!(!request_debug.contains("do-not-log"));

        let error = SemanticError::new(SemanticErrorCode::Internal, "safe summary")
            .with_detail("private", json!("do-not-log-detail"));
        let response: ResponseEnvelope<Value> =
            ResponseEnvelope::failure(request.request_id, error);
        let response_debug = format!("{response:?}");
        assert!(!response_debug.contains("do-not-log-detail"));
    }

    #[test]
    fn semantic_error_round_trips_structured_details() {
        let error = SemanticError::new(
            SemanticErrorCode::PreconditionFailed,
            "Task state no longer matches the requested precondition.",
        )
        .with_detail("expected_state", json!("running"));
        let json = serde_json::to_string(&error).unwrap();
        let decoded: SemanticError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, error);
    }
}
