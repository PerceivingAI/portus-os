use crate::{AdminTransport, Command, ProtectedApiCommand, SecretReader, parse_from};
use portus_protected_api::{AdminAction, AdminRequest, ProviderErrorCode, ProviderSuccess};
use std::{ffi::OsString, io::Write, time::Duration};

pub struct RenderedOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
}

pub fn run_from<I, T>(
    args: I,
    transport: &mut dyn AdminTransport,
    secret_reader: &mut dyn SecretReader,
) -> RenderedOutput
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(args, transport, secret_reader, &mut stdout, &mut stderr);
    RenderedOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code: code,
    }
}

pub fn run_with<I, T>(
    args: I,
    transport: &mut dyn AdminTransport,
    secret_reader: &mut dyn SecretReader,
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
        Command::ProtectedApi { command } => match command {
            ProtectedApiCommand::Provision {
                credential_ref,
                provider,
                label,
            } => {
                let secret = match secret_reader.read_secret("Protected credential: ") {
                    Ok(secret) => secret,
                    Err(message) => {
                        let _ = writeln!(stderr, "{message}");
                        return 2;
                    }
                };
                AdminAction::CredentialProvision {
                    credential_ref: credential_ref.clone(),
                    provider_id: provider.clone(),
                    safe_label: label.clone(),
                    secret,
                }
            }
            ProtectedApiCommand::Rotate { credential_ref } => {
                let secret = match secret_reader.read_secret("Replacement credential: ") {
                    Ok(secret) => secret,
                    Err(message) => {
                        let _ = writeln!(stderr, "{message}");
                        return 2;
                    }
                };
                AdminAction::CredentialRotate {
                    credential_ref: credential_ref.clone(),
                    secret,
                }
            }
            ProtectedApiCommand::Revoke { credential_ref } => AdminAction::CredentialRevoke {
                credential_ref: credential_ref.clone(),
            },
            ProtectedApiCommand::Delete { credential_ref } => AdminAction::CredentialDelete {
                credential_ref: credential_ref.clone(),
            },
            ProtectedApiCommand::Show { credential_ref } => AdminAction::CredentialShow {
                credential_ref: credential_ref.clone(),
            },
            ProtectedApiCommand::List => AdminAction::CredentialList,
        },
    };
    let request = AdminRequest::new(action);
    if let Err(error) = request.validate() {
        return render_error(&error, cli.json, stdout, stderr);
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
        return render_error(
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
        Err(error) => return render_error(&error, cli.json, stdout, stderr),
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

fn render_human(result: &ProviderSuccess, out: &mut dyn Write) {
    match result {
        ProviderSuccess::CredentialMutation { credential }
        | ProviderSuccess::CredentialShow { credential } => {
            let _ = writeln!(out, "credential  {}", credential.credential_ref);
            let _ = writeln!(out, "provider    {}", credential.provider_id);
            let _ = writeln!(out, "generation  {}", credential.generation);
            let _ = writeln!(out, "state       {}", credential.state);
        }
        ProviderSuccess::CredentialDeleted { credential_ref } => {
            let _ = writeln!(out, "deleted {credential_ref}");
        }
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
                let _ = writeln!(out, "No protected credentials configured.");
            }
        }
        _ => {
            let _ = writeln!(out, "protected API administrative operation completed");
        }
    }
}

fn render_error(
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
        ProviderErrorCode::InvalidRequest => 2,
        ProviderErrorCode::PermissionDenied => 5,
        ProviderErrorCode::CredentialNotFound | ProviderErrorCode::CredentialRevoked => 6,
        ProviderErrorCode::StoreUnavailable | ProviderErrorCode::ProviderDefinitionInvalid => 3,
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransportError;
    use portus_protected_api::{
        CredentialMetadata, CredentialState, ProviderResponse, SecretMaterial,
    };
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct FakeSecrets {
        reads: AtomicUsize,
    }
    impl SecretReader for FakeSecrets {
        fn read_secret(&mut self, _prompt: &str) -> Result<SecretMaterial, String> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            SecretMaterial::new("fixture-secret-value".into()).map_err(str::to_string)
        }
    }

    struct FakeTransport {
        replies: VecDeque<ProviderResponse>,
        serialized_requests: Vec<String>,
    }
    impl AdminTransport for FakeTransport {
        fn send(
            &mut self,
            request: &AdminRequest,
            _timeout: Duration,
        ) -> Result<ProviderResponse, TransportError> {
            self.serialized_requests
                .push(serde_json::to_string(request).unwrap());
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
    fn provisioning_reads_secret_out_of_band_and_never_renders_it() {
        let response = ProviderResponse::success(
            portus_protocol::RequestId::new(),
            ProviderSuccess::CredentialMutation {
                credential: metadata(),
            },
        );
        let mut transport = FakeTransport {
            replies: VecDeque::from([response]),
            serialized_requests: Vec::new(),
        };
        let mut secrets = FakeSecrets {
            reads: AtomicUsize::new(0),
        };
        let output = run_from(
            [
                "portus-auth",
                "protected-api",
                "provision",
                "openai/main",
                "--provider",
                "openai",
            ],
            &mut transport,
            &mut secrets,
        );
        assert_eq!(output.exit_code, 0);
        assert_eq!(secrets.reads.load(Ordering::Relaxed), 1);
        assert!(!output.stdout.contains("fixture-secret-value"));
        assert!(!output.stderr.contains("fixture-secret-value"));
        assert!(transport.serialized_requests[0].contains("fixture-secret-value"));
    }

    #[test]
    fn non_secret_admin_actions_do_not_read_secret_input() {
        let response = ProviderResponse::success(
            portus_protocol::RequestId::new(),
            ProviderSuccess::CredentialShow {
                credential: metadata(),
            },
        );
        let mut transport = FakeTransport {
            replies: VecDeque::from([response]),
            serialized_requests: Vec::new(),
        };
        let mut secrets = FakeSecrets {
            reads: AtomicUsize::new(0),
        };
        let output = run_from(
            ["portus-auth", "protected-api", "show", "openai/main"],
            &mut transport,
            &mut secrets,
        );
        assert_eq!(output.exit_code, 0);
        assert_eq!(secrets.reads.load(Ordering::Relaxed), 0);
    }
}
