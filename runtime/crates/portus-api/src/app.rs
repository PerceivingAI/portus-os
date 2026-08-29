use crate::{ApiTransport, Command, CredentialCommand, parse_from};
use portus_protected_api::{
    MAX_REQUEST_BYTES, ProviderErrorCode, ProviderSuccess, UseAction, UseRequest,
};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::{Read, Write},
    time::Duration,
};

pub struct RenderedOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
}

pub fn run_from<I, T>(args: I, transport: &mut dyn ApiTransport) -> RenderedOutput
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdin = std::io::empty();
    let code = run_with_io(args, transport, &mut stdin, &mut stdout, &mut stderr);
    RenderedOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code: code,
    }
}

pub fn run_with_io<I, T>(
    args: I,
    transport: &mut dyn ApiTransport,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            return if error.use_stderr() { 2 } else { 0 };
        }
    };
    let action = match &cli.command {
        Command::Credential {
            command: CredentialCommand::List,
        } => UseAction::CredentialList,
        Command::Credential {
            command: CredentialCommand::Show { credential_ref },
        } => UseAction::CredentialShow {
            credential_ref: credential_ref.clone(),
        },
        Command::Health => UseAction::Health,
        Command::Request {
            credential_ref,
            operation,
            input,
        } => {
            let bytes = match read_payload(input, stdin) {
                Ok(bytes) => bytes,
                Err(message) => {
                    let _ = writeln!(stderr, "{message}");
                    return 2;
                }
            };
            let payload: Value = match serde_json::from_slice(&bytes) {
                Ok(payload) => payload,
                Err(_) => {
                    let _ = writeln!(stderr, "request input must be valid JSON");
                    return 2;
                }
            };
            UseAction::Request {
                credential_ref: credential_ref.clone(),
                operation: operation.clone(),
                payload,
            }
        }
    };
    let request = UseRequest::new(action);
    if let Err(error) = request.validate() {
        return render_provider_error(&error, cli.json, stdout, stderr);
    }
    let response = match transport.send(&request, Duration::from_millis(cli.timeout_ms)) {
        Ok(response) => response,
        Err(error) => {
            if cli.json {
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::json!({"ok":false,"error":{"code":"provider_unavailable","message":error.to_string()}})
                );
            } else {
                let _ = writeln!(stderr, "{error}");
            }
            return 3;
        }
    };
    if !response.ok {
        return render_provider_error(
            response
                .error
                .as_ref()
                .expect("validated failure has error"),
            cli.json,
            stdout,
            stderr,
        );
    }
    let result = match response.success_value() {
        Ok(result) => result,
        Err(error) => return render_provider_error(&error, cli.json, stdout, stderr),
    };
    if cli.json {
        let _ = writeln!(
            stdout,
            "{}",
            serde_json::to_string(&response).expect("provider response serializes")
        );
    } else {
        render_human(&result, stdout);
    }
    0
}

fn read_payload(path: &str, stdin: &mut dyn Read) -> Result<Vec<u8>, &'static str> {
    if path == "-" {
        return read_bounded(stdin);
    }
    let mut file =
        std::fs::File::open(path).map_err(|_| "request input file could not be opened")?;
    read_bounded(&mut file)
}

fn read_bounded(reader: &mut dyn Read) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "request input could not be read")?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err("request input exceeds 8 MiB protocol ceiling");
    }
    Ok(bytes)
}

fn render_human(result: &ProviderSuccess, out: &mut dyn Write) {
    match result {
        ProviderSuccess::CredentialList { credentials } => {
            for credential in credentials {
                let _ = writeln!(
                    out,
                    "{}  provider={} generation={} state={}",
                    credential.credential_ref,
                    credential.provider_id,
                    credential.generation,
                    credential.state
                );
            }
            if credentials.is_empty() {
                let _ = writeln!(out, "No visible protected credentials.");
            }
        }
        ProviderSuccess::CredentialShow { credential }
        | ProviderSuccess::CredentialMutation { credential } => {
            let _ = writeln!(out, "credential  {}", credential.credential_ref);
            let _ = writeln!(out, "provider    {}", credential.provider_id);
            let _ = writeln!(out, "generation  {}", credential.generation);
            let _ = writeln!(out, "state       {}", credential.state);
        }
        ProviderSuccess::Request {
            provider_id,
            operation,
            upstream_status,
            body,
        } => {
            let _ = writeln!(out, "provider  {provider_id}");
            let _ = writeln!(out, "operation {operation}");
            let _ = writeln!(out, "status    {upstream_status}");
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(body).unwrap_or_else(|_| "{}".into())
            );
        }
        ProviderSuccess::Health {
            health,
            reason_code,
            credential_count,
            provider_count,
            audit_write_failures,
        } => {
            let _ = writeln!(out, "health       {health}");
            let _ = writeln!(out, "reason       {reason_code}");
            let _ = writeln!(out, "credentials  {credential_count}");
            let _ = writeln!(out, "providers    {provider_count}");
            let _ = writeln!(out, "audit errors {audit_write_failures}");
        }
        ProviderSuccess::CredentialDeleted { credential_ref } => {
            let _ = writeln!(out, "deleted {credential_ref}");
        }
    }
}

fn render_provider_error(
    error: &portus_protected_api::ProviderError,
    json: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    if json {
        let _ = writeln!(stdout, "{}", serde_json::json!({"ok":false,"error":error}));
    } else {
        let _ = writeln!(stderr, "{}: {}", error.code, error.message);
    }
    match error.code {
        ProviderErrorCode::InvalidRequest | ProviderErrorCode::RequestTooLarge => 2,
        ProviderErrorCode::PermissionDenied | ProviderErrorCode::ApprovalRequired => 5,
        ProviderErrorCode::CredentialNotFound | ProviderErrorCode::CredentialRevoked => 6,
        ProviderErrorCode::ProviderDefinitionInvalid | ProviderErrorCode::StoreUnavailable => 3,
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransportError;
    use portus_protected_api::{CredentialMetadata, CredentialState, ProviderResponse};
    use std::collections::VecDeque;

    struct FakeTransport {
        replies: VecDeque<ProviderResponse>,
        requests: Vec<UseRequest>,
    }

    impl ApiTransport for FakeTransport {
        fn send(
            &mut self,
            request: &UseRequest,
            _timeout: Duration,
        ) -> Result<ProviderResponse, TransportError> {
            self.requests
                .push(serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap());
            Ok(self.replies.pop_front().unwrap())
        }
    }

    fn metadata() -> CredentialMetadata {
        CredentialMetadata {
            credential_ref: "openai/main".into(),
            provider_id: "openai".into(),
            safe_label: Some("Main".into()),
            generation: 1,
            state: CredentialState::Active,
            created_at: "1".into(),
            rotated_at: None,
            revoked_at: None,
            updated_at: "1".into(),
        }
    }

    #[test]
    fn request_reads_json_payload_from_stdin() {
        let request_id = portus_protocol::RequestId::new();
        let mut transport = FakeTransport {
            replies: VecDeque::from([ProviderResponse::success(
                request_id,
                ProviderSuccess::Request {
                    provider_id: "openai".into(),
                    operation: "openai.responses.create".into(),
                    upstream_status: 200,
                    body: serde_json::json!({"ok":true}),
                },
            )]),
            requests: Vec::new(),
        };
        let mut input: &[u8] = br#"{"model":"test"}"#;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_with_io(
            [
                "portus-api",
                "request",
                "openai/main",
                "openai.responses.create",
            ],
            &mut transport,
            &mut input,
            &mut out,
            &mut err,
        );
        assert_eq!(code, 0);
        assert!(err.is_empty());
        assert_eq!(transport.requests[0].action, "request");
        assert_eq!(
            transport.requests[0].payload.as_ref().unwrap()["model"],
            "test"
        );
    }

    #[test]
    fn safe_metadata_output_has_no_raw_credential_field() {
        let request_id = portus_protocol::RequestId::new();
        let mut transport = FakeTransport {
            replies: VecDeque::from([ProviderResponse::success(
                request_id,
                ProviderSuccess::CredentialShow {
                    credential: metadata(),
                },
            )]),
            requests: Vec::new(),
        };
        let output = run_from(
            ["portus-api", "credential", "show", "openai/main", "--json"],
            &mut transport,
        );
        assert_eq!(output.exit_code, 0);
        assert!(!output.stdout.contains("secret"));
        assert!(!output.stdout.contains("authorization"));
    }
}
