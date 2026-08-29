use crate::{CliError, RuntimeReply};
use portus_protocol::SemanticErrorCode;
use serde_json::Value;
use std::time::Duration;

pub trait PrivilegeTransport {
    fn admin_request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<RuntimeReply, CliError>;
}

#[derive(Default)]
pub struct UnavailablePrivilege;

impl PrivilegeTransport for UnavailablePrivilege {
    fn admin_request(
        &mut self,
        method: &str,
        _params: Value,
        _timeout: Duration,
    ) -> Result<RuntimeReply, CliError> {
        Err(CliError::new(
            method,
            SemanticErrorCode::DaemonUnavailable,
            "portus-privd admin transport is unavailable",
        ))
    }
}

pub struct SystemPrivilege {
    admin_socket_path: std::path::PathBuf,
}

impl Default for SystemPrivilege {
    fn default() -> Self {
        Self {
            admin_socket_path: "/run/portus/priv/admin.sock".into(),
        }
    }
}

impl SystemPrivilege {
    #[must_use]
    pub fn for_admin_socket(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            admin_socket_path: path.into(),
        }
    }
}

#[cfg(target_os = "linux")]
impl PrivilegeTransport for SystemPrivilege {
    fn admin_request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<RuntimeReply, CliError> {
        use portus_client::{FrameError, UnixRuntimeClient};
        use portus_protocol::{RequestEnvelope, ResponseEnvelope};
        use std::io::ErrorKind;

        let map_frame = |error: FrameError| match error {
            FrameError::Io(io)
                if matches!(io.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) =>
            {
                CliError::new(
                    method,
                    SemanticErrorCode::Timeout,
                    "portus-privd request timed out",
                )
            }
            FrameError::Io(_) => CliError::new(
                method,
                SemanticErrorCode::DaemonUnavailable,
                "portus-privd is unavailable",
            ),
            FrameError::FrameTooLarge { .. }
            | FrameError::TruncatedFrame
            | FrameError::InvalidJson(_) => CliError::new(
                method,
                SemanticErrorCode::ProtocolError,
                "portus-privd returned an invalid bounded protocol frame",
            ),
        };
        let mut client = UnixRuntimeClient::connect_with_limits(
            &self.admin_socket_path,
            portus_client::DEFAULT_MAX_FRAME_BYTES,
            timeout,
        )
        .map_err(map_frame)?;
        let request = RequestEnvelope::new(method, params);
        let request_id = request.request_id;
        let response: ResponseEnvelope<Value> = client.request(&request).map_err(map_frame)?;
        response
            .validate()
            .map_err(|error| CliError::new(method, error.semantic_code(), error.to_string()))?;
        if let Some(error) = response.error {
            return Err(CliError {
                command: method.into(),
                semantic: Box::new(error),
                meta: Box::new(crate::meta_with_request(request_id)),
                human_hint: None,
            });
        }
        Ok(RuntimeReply {
            data: response
                .result
                .expect("validated success response contains result"),
            meta: crate::meta_with_request(request_id),
        })
    }
}

#[cfg(not(target_os = "linux"))]
impl PrivilegeTransport for SystemPrivilege {
    fn admin_request(
        &mut self,
        method: &str,
        _params: Value,
        _timeout: Duration,
    ) -> Result<RuntimeReply, CliError> {
        let _ = &self.admin_socket_path;
        Err(CliError::new(
            method,
            SemanticErrorCode::DaemonUnavailable,
            "portus-privd admin transport requires Linux",
        ))
    }
}
