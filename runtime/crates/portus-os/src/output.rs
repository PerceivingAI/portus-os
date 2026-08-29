use crate::CLI_OUTPUT_SCHEMA_VERSION;
use portus_protocol::{RequestId, SemanticError, SemanticErrorCode};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize)]
pub struct CliMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug)]
pub struct CliSuccess {
    pub command: &'static str,
    pub data: Value,
    pub meta: CliMeta,
    pub human: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CliError {
    pub command: String,
    pub semantic: Box<SemanticError>,
    pub meta: Box<CliMeta>,
    pub human_hint: Option<String>,
}

impl CliError {
    #[must_use]
    pub fn new(
        command: impl Into<String>,
        code: SemanticErrorCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            command: command.into(),
            semantic: Box::new(SemanticError::new(code, message)),
            meta: Box::new(CliMeta::default()),
            human_hint: None,
        }
    }

    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        self.semantic.code.exit_code()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
}

pub fn render_success(success: &CliSuccess, json_mode: bool) -> RenderedOutput {
    if json_mode {
        let envelope = json!({
            "schema_version": CLI_OUTPUT_SCHEMA_VERSION,
            "command": success.command,
            "ok": true,
            "data": success.data,
            "meta": success.meta,
        });
        RenderedOutput {
            stdout: format!(
                "{}\n",
                serde_json::to_string(&envelope).expect("CLI envelope serializes")
            ),
            stderr: String::new(),
            exit_code: 0,
        }
    } else {
        RenderedOutput {
            stdout: if success.human.is_empty() {
                String::new()
            } else {
                format!("{}\n", success.human.join("\n"))
            },
            stderr: String::new(),
            exit_code: 0,
        }
    }
}

pub fn render_error(error: &CliError, json_mode: bool) -> RenderedOutput {
    if json_mode {
        let envelope = json!({
            "schema_version": CLI_OUTPUT_SCHEMA_VERSION,
            "command": error.command,
            "ok": false,
            "error": error.semantic,
            "meta": error.meta,
        });
        RenderedOutput {
            stdout: format!(
                "{}\n",
                serde_json::to_string(&envelope).expect("CLI error envelope serializes")
            ),
            stderr: String::new(),
            exit_code: error.exit_code(),
        }
    } else {
        let mut lines = vec![format!(
            "{}: {}",
            error.semantic.code, error.semantic.message
        )];
        if let Some(hint) = &error.human_hint {
            lines.push(hint.clone());
        }
        RenderedOutput {
            stdout: String::new(),
            stderr: format!("{}\n", lines.join("\n")),
            exit_code: error.exit_code(),
        }
    }
}

pub fn meta_with_request(request_id: RequestId) -> CliMeta {
    CliMeta {
        request_id: Some(request_id),
        ..CliMeta::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_success_has_locked_outer_shape() {
        let success = CliSuccess {
            command: "status",
            data: json!({"runtime":"healthy"}),
            meta: CliMeta::default(),
            human: vec!["runtime  healthy".into()],
        };
        let rendered = render_success(&success, true);
        let value: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
        assert_eq!(value["schema_version"], CLI_OUTPUT_SCHEMA_VERSION);
        assert_eq!(value["command"], "status");
        assert_eq!(value["ok"], true);
        assert!(value.get("data").is_some());
        assert!(value.get("meta").is_some());
    }

    #[test]
    fn json_error_uses_semantic_exit_family() {
        let error = CliError::new(
            "status",
            SemanticErrorCode::DaemonUnavailable,
            "daemon unavailable",
        );
        let rendered = render_error(&error, true);
        assert_eq!(rendered.exit_code, 3);
        assert!(rendered.stderr.is_empty());
        let value: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
        assert_eq!(value["error"]["code"], "daemon_unavailable");
    }

    #[test]
    fn human_errors_stay_on_stderr() {
        let error = CliError::new("status", SemanticErrorCode::Timeout, "timed out");
        let rendered = render_error(&error, false);
        assert!(rendered.stdout.is_empty());
        assert!(rendered.stderr.contains("timeout"));
        assert_eq!(rendered.exit_code, 8);
    }
}
