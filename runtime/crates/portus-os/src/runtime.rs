use crate::{CliError, CliMeta};
use portus_protocol::{SemanticErrorCode, TaskEvent, TaskId};
use serde_json::Value;
use std::time::Duration;

#[cfg(target_os = "linux")]
use crate::meta_with_request;
#[cfg(target_os = "linux")]
use portus_client::FrameError;
#[cfg(target_os = "linux")]
use portus_protocol::{
    ProtocolError, RequestEnvelope, ResponseEnvelope, TaskEventStreamFrame,
    TaskEventStreamFrameKind,
};
#[cfg(target_os = "linux")]
use std::io::ErrorKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskStreamEnd {
    pub terminal_state: String,
}

pub trait RuntimeTransport {
    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<RuntimeReply, CliError>;

    fn follow_task_events(
        &mut self,
        task_id: &TaskId,
        after_sequence: Option<u64>,
        limit: u16,
        timeout: Duration,
        on_event: &mut dyn FnMut(TaskEvent) -> Result<(), CliError>,
    ) -> Result<TaskStreamEnd, CliError> {
        let _ = (task_id, after_sequence, limit, timeout, on_event);
        Err(CliError::new(
            "task.events",
            SemanticErrorCode::Unsupported,
            "runtime transport does not support task event streaming",
        ))
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeReply {
    pub data: Value,
    pub meta: CliMeta,
}

pub struct SystemRuntime {
    socket_path: std::path::PathBuf,
}

impl Default for SystemRuntime {
    fn default() -> Self {
        Self {
            socket_path: std::path::PathBuf::from("/run/portus/portusd.sock"),
        }
    }
}

impl SystemRuntime {
    /// Constructs a runtime transport for an explicit local socket path.
    ///
    /// The installed `portus-os` binary uses `Default` and therefore always
    /// selects the canonical `/run/portus/portusd.sock`. This constructor exists
    /// for isolated integration fixtures and embedded first-party tests.
    #[must_use]
    pub fn for_socket(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
        }
    }
}

#[cfg(target_os = "linux")]
impl RuntimeTransport for SystemRuntime {
    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<RuntimeReply, CliError> {
        use portus_client::UnixRuntimeClient;

        let mut client = UnixRuntimeClient::connect_with_limits(
            &self.socket_path,
            portus_client::DEFAULT_MAX_FRAME_BYTES,
            timeout,
        )
        .map_err(|error| frame_error(method, error))?;
        let request = RequestEnvelope::new(method, params);
        let request_id = request.request_id;
        let response: ResponseEnvelope<Value> = client
            .request(&request)
            .map_err(|error| frame_error(method, error))?;
        if let Err(error) = response.validate() {
            return Err(protocol_error(method, error));
        }
        if let Some(error) = response.error {
            return Err(CliError {
                command: method.to_string(),
                semantic: Box::new(error),
                meta: Box::new(meta_with_request(request_id)),
                human_hint: None,
            });
        }
        Ok(RuntimeReply {
            data: response
                .result
                .expect("validated success response contains a result"),
            meta: meta_with_request(request_id),
        })
    }

    fn follow_task_events(
        &mut self,
        task_id: &TaskId,
        after_sequence: Option<u64>,
        limit: u16,
        timeout: Duration,
        on_event: &mut dyn FnMut(TaskEvent) -> Result<(), CliError>,
    ) -> Result<TaskStreamEnd, CliError> {
        use portus_client::UnixRuntimeClient;

        let mut client = UnixRuntimeClient::connect_with_limits(
            &self.socket_path,
            portus_client::DEFAULT_MAX_FRAME_BYTES,
            timeout,
        )
        .map_err(|error| frame_error("task.events", error))?;
        let request = RequestEnvelope::new(
            "task.events.follow",
            serde_json::json!({
                "task_id": task_id,
                "after_sequence": after_sequence,
                "limit": limit,
            }),
        );
        let request_id = request.request_id;
        client
            .send(&request)
            .map_err(|error| frame_error("task.events", error))?;
        loop {
            let frame: TaskEventStreamFrame = client
                .read()
                .map_err(|error| frame_error("task.events", error))?
                .ok_or_else(|| {
                    CliError::new(
                        "task.events",
                        SemanticErrorCode::ProtocolError,
                        "task event stream ended without a terminal frame",
                    )
                })?;
            frame.validate().map_err(|message| {
                CliError::new("task.events", SemanticErrorCode::ProtocolError, message)
            })?;
            if frame.request_id != request_id {
                return Err(CliError::new(
                    "task.events",
                    SemanticErrorCode::ProtocolError,
                    "task event stream request identity changed",
                ));
            }
            match frame.frame {
                TaskEventStreamFrameKind::Event => {
                    on_event(frame.event.expect("validated event frame contains event"))?;
                }
                TaskEventStreamFrameKind::End => {
                    return Ok(TaskStreamEnd {
                        terminal_state: frame
                            .terminal_state
                            .expect("validated end frame contains terminal state"),
                    });
                }
                TaskEventStreamFrameKind::Error => {
                    return Err(CliError {
                        command: "task.events".into(),
                        semantic: Box::new(
                            frame.error.expect("validated error frame contains error"),
                        ),
                        meta: Box::new(meta_with_request(request_id)),
                        human_hint: None,
                    });
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
impl RuntimeTransport for SystemRuntime {
    fn follow_task_events(
        &mut self,
        _task_id: &TaskId,
        _after_sequence: Option<u64>,
        _limit: u16,
        _timeout: Duration,
        _on_event: &mut dyn FnMut(TaskEvent) -> Result<(), CliError>,
    ) -> Result<TaskStreamEnd, CliError> {
        Err(CliError::new(
            "task.events",
            SemanticErrorCode::DaemonUnavailable,
            "portusd runtime transport requires Linux",
        ))
    }

    fn request(
        &mut self,
        method: &str,
        _params: Value,
        _timeout: Duration,
    ) -> Result<RuntimeReply, CliError> {
        let _ = &self.socket_path;
        Err(CliError::new(
            method,
            SemanticErrorCode::DaemonUnavailable,
            "portusd runtime transport requires Linux",
        ))
    }
}

#[cfg(target_os = "linux")]
fn protocol_error(command: &str, error: ProtocolError) -> CliError {
    CliError::new(command, error.semantic_code(), error.to_string())
}

#[cfg(target_os = "linux")]
fn frame_error(command: &str, error: FrameError) -> CliError {
    match error {
        FrameError::Io(io) if matches!(io.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
            CliError::new(
                command,
                SemanticErrorCode::Timeout,
                "runtime request timed out",
            )
        }
        FrameError::Io(_) => CliError::new(
            command,
            SemanticErrorCode::DaemonUnavailable,
            "portusd is unavailable",
        ),
        FrameError::FrameTooLarge { .. }
        | FrameError::TruncatedFrame
        | FrameError::InvalidJson(_) => CliError::new(
            command,
            SemanticErrorCode::ProtocolError,
            "portusd returned an invalid bounded protocol frame",
        ),
    }
}

#[cfg(all(test, not(target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn non_linux_system_runtime_never_fakes_transport() {
        let mut runtime = SystemRuntime::for_socket("does-not-matter");
        let error = runtime
            .request(
                "runtime.status",
                serde_json::json!({}),
                Duration::from_millis(100),
            )
            .unwrap_err();
        assert_eq!(error.semantic.code, SemanticErrorCode::DaemonUnavailable);
    }
}
