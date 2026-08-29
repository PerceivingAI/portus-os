use crate::{CredentialStore, FilesystemPolicyAuthorizer, HttpsUpstream, ProtectedApiCore};
use nix::{
    sys::{
        socket::{getsockopt, sockopt::PeerCredentials},
        stat::{Mode, umask},
    },
    unistd::{Group, Uid, User},
};
use portus_client::{FrameError, read_json_line, write_json_line};
use portus_policy::{PolicyPaths, PolicyTrust};
use portus_protected_api::{
    AdminRequest, CLIENT_GROUP, DefinitionCatalog, DefinitionPaths, DefinitionTrust,
    ProviderResponse, SERVICE_GROUP, SERVICE_USER, UseRequest,
};
use portus_protocol::Principal;
use std::{
    fmt, fs,
    io::{BufRead, BufReader, ErrorKind},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use zeroize::Zeroizing;

const USE_SOCKET_MODE: u32 = 0o660;
const ADMIN_SOCKET_MODE: u32 = 0o600;
const ACCEPT_POLL: Duration = Duration::from_millis(50);
pub const DEFAULT_MAX_CONNECTIONS: usize = 32;
pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(120);
const RESPONSE_FRAME_MAX: usize = portus_protected_api::MAX_RESPONSE_BYTES + 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceIdentityTrust {
    Canonical,
    PretrustedFixture,
}

#[derive(Clone, Debug)]
pub struct ProtectedApiServerConfig {
    pub use_socket_path: PathBuf,
    pub admin_socket_path: PathBuf,
    pub store_path: PathBuf,
    pub definition_paths: DefinitionPaths,
    pub definition_trust: DefinitionTrust,
    pub policy_paths: PolicyPaths,
    pub policy_trust: PolicyTrust,
    pub audit_path: PathBuf,
    pub identity_trust: ServiceIdentityTrust,
    pub max_connections: usize,
    pub io_timeout: Duration,
}

impl ProtectedApiServerConfig {
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            use_socket_path: portus_protected_api::CANONICAL_USE_SOCKET.into(),
            admin_socket_path: portus_protected_api::CANONICAL_ADMIN_SOCKET.into(),
            store_path: portus_protected_api::CANONICAL_STORE_PATH.into(),
            definition_paths: DefinitionPaths::canonical(),
            definition_trust: DefinitionTrust::RootOwnedSystem,
            policy_paths: PolicyPaths::canonical(),
            policy_trust: PolicyTrust::RootOwnedSystem,
            audit_path: portus_protected_api::CANONICAL_AUDIT_PATH.into(),
            identity_trust: ServiceIdentityTrust::Canonical,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            io_timeout: DEFAULT_IO_TIMEOUT,
        }
    }
}

#[derive(Debug)]
pub enum ServerError {
    Io(std::io::Error),
    Frame(FrameError),
    Definition(portus_protected_api::DefinitionError),
    Store(crate::StoreError),
    Audit(portus_audit::AuditError),
    InvalidConfiguration(&'static str),
    WrongServiceIdentity,
    SocketOccupied(PathBuf),
    SocketAlreadyActive(PathBuf),
    PeerCredentials(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "protected API server I/O error: {error}"),
            Self::Frame(error) => write!(f, "protected API protocol framing error: {error}"),
            Self::Definition(error) => write!(f, "protected API definition error: {error}"),
            Self::Store(error) => write!(f, "protected API store error: {error}"),
            Self::Audit(error) => write!(f, "protected API audit error: {error}"),
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid protected API server configuration: {message}")
            }
            Self::WrongServiceIdentity => {
                f.write_str("portus-apid must run as the dedicated portus-api service identity")
            }
            Self::SocketOccupied(path) => write!(
                f,
                "protected API socket path is occupied: {}",
                path.display()
            ),
            Self::SocketAlreadyActive(path) => {
                write!(f, "protected API socket already active: {}", path.display())
            }
            Self::PeerCredentials(message) => {
                write!(f, "failed to authenticate protected API peer: {message}")
            }
        }
    }
}
impl std::error::Error for ServerError {}
impl From<std::io::Error> for ServerError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<FrameError> for ServerError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}
impl From<portus_protected_api::DefinitionError> for ServerError {
    fn from(value: portus_protected_api::DefinitionError) -> Self {
        Self::Definition(value)
    }
}
impl From<crate::StoreError> for ServerError {
    fn from(value: crate::StoreError) -> Self {
        Self::Store(value)
    }
}
impl From<portus_audit::AuditError> for ServerError {
    fn from(value: portus_audit::AuditError) -> Self {
        Self::Audit(value)
    }
}
pub type ServerResult<T> = Result<T, ServerError>;

pub struct ProtectedApiServer {
    config: ProtectedApiServerConfig,
    core: Arc<ProtectedApiCore>,
    use_listener: UnixListener,
    admin_listener: UnixListener,
    active_connections: Arc<AtomicUsize>,
}

impl ProtectedApiServer {
    pub fn bind(config: ProtectedApiServerConfig) -> ServerResult<Self> {
        validate_config(&config)?;
        if config.identity_trust == ServiceIdentityTrust::Canonical {
            validate_service_identity_and_paths(&config)?;
            let _ = umask(Mode::from_bits_truncate(0o077));
        }
        prepare_socket_path(&config.use_socket_path)?;
        prepare_socket_path(&config.admin_socket_path)?;
        let definitions =
            DefinitionCatalog::load(&config.definition_paths, config.definition_trust)?;
        let store = CredentialStore::open(&config.store_path)?;
        if config.identity_trust == ServiceIdentityTrust::Canonical {
            verify_store_contract(&config)?;
        }
        let audit = Arc::new(portus_audit::FileAuditSink::open(&config.audit_path)?);
        let authorizer = Arc::new(FilesystemPolicyAuthorizer::new(
            config.policy_paths.clone(),
            config.policy_trust,
        ));
        let core = Arc::new(ProtectedApiCore::new_with_audit(
            store,
            definitions,
            authorizer,
            Arc::new(HttpsUpstream),
            audit,
        ));
        let use_listener = bind_listener(&config.use_socket_path, USE_SOCKET_MODE)?;
        let admin_listener = match bind_listener(&config.admin_socket_path, ADMIN_SOCKET_MODE) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = fs::remove_file(&config.use_socket_path);
                return Err(error);
            }
        };
        if config.identity_trust == ServiceIdentityTrust::Canonical {
            verify_socket_contract(&config)?;
        }
        Ok(Self {
            config,
            core,
            use_listener,
            admin_listener,
            active_connections: Arc::new(AtomicUsize::new(0)),
        })
    }

    #[must_use]
    pub fn core(&self) -> &Arc<ProtectedApiCore> {
        &self.core
    }

    pub fn run_until(self, shutdown: Arc<AtomicBool>) -> ServerResult<()> {
        let mut workers = Vec::new();
        while !shutdown.load(Ordering::Acquire) {
            accept_ready(&self.use_listener, false, &self, &mut workers)?;
            accept_ready(&self.admin_listener, true, &self, &mut workers)?;
            reap(&mut workers);
            thread::sleep(ACCEPT_POLL);
        }
        for worker in workers {
            let _ = worker.join();
        }
        Ok(())
    }
}

impl Drop for ProtectedApiServer {
    fn drop(&mut self) {
        for path in [&self.config.use_socket_path, &self.config.admin_socket_path] {
            if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket()) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn accept_ready(
    listener: &UnixListener,
    admin: bool,
    server: &ProtectedApiServer,
    workers: &mut Vec<JoinHandle<()>>,
) -> ServerResult<()> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(permit) = ConnectionPermit::acquire(
                    Arc::clone(&server.active_connections),
                    server.config.max_connections,
                ) {
                    let core = Arc::clone(&server.core);
                    let config = server.config.clone();
                    workers.push(thread::spawn(move || {
                        let _permit = permit;
                        let _ = serve_connection(stream, core, config, admin);
                    }));
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(ServerError::Io(error)),
        }
    }
}

fn serve_connection(
    stream: UnixStream,
    core: Arc<ProtectedApiCore>,
    config: ProtectedApiServerConfig,
    admin: bool,
) -> ServerResult<()> {
    stream.set_read_timeout(Some(config.io_timeout))?;
    stream.set_write_timeout(Some(config.io_timeout))?;
    let principal = peer_principal(&stream)?;
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;
    let response: ProviderResponse = if admin {
        let Some(request) = read_secret_json_line::<AdminRequest>(
            &mut reader,
            portus_protected_api::MAX_PROTOCOL_FRAME_BYTES,
        )?
        else {
            return Ok(());
        };
        core.dispatch_admin(principal, request)
    } else {
        let Some(request): Option<UseRequest> =
            read_json_line(&mut reader, portus_protected_api::MAX_PROTOCOL_FRAME_BYTES)?
        else {
            return Ok(());
        };
        core.dispatch_use(principal, request)
    };
    write_json_line(&mut writer, &response, RESPONSE_FRAME_MAX)?;
    Ok(())
}

fn read_secret_json_line<T: serde::de::DeserializeOwned>(
    reader: &mut impl BufRead,
    max: usize,
) -> ServerResult<Option<T>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(max.min(4096)));
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(ServerError::InvalidConfiguration(
                    "admin protocol frame exceeds bound or is truncated",
                ))
            };
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index);
        if bytes.len().saturating_add(take) > max {
            return Err(ServerError::InvalidConfiguration(
                "admin protocol frame exceeds bound or is truncated",
            ));
        }
        bytes.extend_from_slice(&available[..take]);
        reader.consume(take + usize::from(newline.is_some()));

        if newline.is_some() {
            break;
        }
    }

    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| ServerError::InvalidConfiguration("admin protocol frame is invalid JSON"))
}

fn peer_principal(stream: &UnixStream) -> ServerResult<Principal> {
    let credentials = getsockopt(stream, PeerCredentials)
        .map_err(|error| ServerError::PeerCredentials(error.to_string()))?;
    Ok(Principal::new(credentials.uid(), credentials.gid()))
}

fn validate_service_identity_and_paths(config: &ProtectedApiServerConfig) -> ServerResult<()> {
    let user = User::from_name(SERVICE_USER)
        .map_err(|_| ServerError::WrongServiceIdentity)?
        .ok_or(ServerError::WrongServiceIdentity)?;
    let group = Group::from_name(SERVICE_GROUP)
        .map_err(|_| ServerError::WrongServiceIdentity)?
        .ok_or(ServerError::WrongServiceIdentity)?;
    if Uid::effective().as_raw() != user.uid.as_raw() || user.gid.as_raw() != group.gid.as_raw() {
        return Err(ServerError::WrongServiceIdentity);
    }
    let store_parent = config
        .store_path
        .parent()
        .ok_or(ServerError::InvalidConfiguration(
            "store path has no parent",
        ))?;
    let metadata = fs::symlink_metadata(store_parent)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != user.uid.as_raw()
        || metadata.gid() != group.gid.as_raw()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(ServerError::InvalidConfiguration(
            "protected store directory is not service-owned mode 0700-or-stricter",
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(&config.store_path) {
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.uid() != user.uid.as_raw()
            || metadata.gid() != group.gid.as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(ServerError::InvalidConfiguration(
                "protected credential database is not service-owned mode 0600-or-stricter",
            ));
        }
    }
    let runtime_parent =
        config
            .use_socket_path
            .parent()
            .ok_or(ServerError::InvalidConfiguration(
                "socket path has no parent",
            ))?;
    let client_group = Group::from_name(CLIENT_GROUP)
        .map_err(|_| {
            ServerError::InvalidConfiguration("failed to resolve protected API client group")
        })?
        .ok_or(ServerError::InvalidConfiguration(
            "protected API client group is missing",
        ))?;
    let metadata = fs::symlink_metadata(runtime_parent)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != user.uid.as_raw()
        || metadata.gid() != client_group.gid.as_raw()
        || metadata.permissions().mode() & 0o2000 == 0
    {
        return Err(ServerError::InvalidConfiguration(
            "protected API runtime directory must be service-owned with setgid client group inheritance",
        ));
    }
    Ok(())
}

fn verify_store_contract(config: &ProtectedApiServerConfig) -> ServerResult<()> {
    let service_user = User::from_name(SERVICE_USER)
        .map_err(|_| ServerError::WrongServiceIdentity)?
        .ok_or(ServerError::WrongServiceIdentity)?;
    let service_group = Group::from_name(SERVICE_GROUP)
        .map_err(|_| ServerError::WrongServiceIdentity)?
        .ok_or(ServerError::WrongServiceIdentity)?;
    for path in [
        config.store_path.clone(),
        PathBuf::from(format!("{}-wal", config.store_path.display())),
        PathBuf::from(format!("{}-shm", config.store_path.display())),
    ] {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(ServerError::Io(error)),
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.uid() != service_user.uid.as_raw()
            || metadata.gid() != service_group.gid.as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(ServerError::InvalidConfiguration(
                "protected credential database/sidecar permissions are not service-private",
            ));
        }
    }
    Ok(())
}

fn verify_socket_contract(config: &ProtectedApiServerConfig) -> ServerResult<()> {
    let client_group = Group::from_name(CLIENT_GROUP)
        .map_err(|_| {
            ServerError::InvalidConfiguration("failed to resolve protected API client group")
        })?
        .ok_or(ServerError::InvalidConfiguration(
            "protected API client group is missing",
        ))?;
    let service_user = User::from_name(SERVICE_USER)
        .map_err(|_| ServerError::WrongServiceIdentity)?
        .ok_or(ServerError::WrongServiceIdentity)?;
    let use_meta = fs::symlink_metadata(&config.use_socket_path)?;
    let admin_meta = fs::symlink_metadata(&config.admin_socket_path)?;
    if use_meta.uid() != service_user.uid.as_raw()
        || use_meta.gid() != client_group.gid.as_raw()
        || use_meta.permissions().mode() & 0o777 != USE_SOCKET_MODE
    {
        return Err(ServerError::InvalidConfiguration(
            "use socket ownership/mode contract was not established",
        ));
    }
    if admin_meta.uid() != service_user.uid.as_raw()
        || admin_meta.permissions().mode() & 0o777 != ADMIN_SOCKET_MODE
    {
        return Err(ServerError::InvalidConfiguration(
            "admin socket ownership/mode contract was not established",
        ));
    }
    Ok(())
}

fn bind_listener(path: &Path, mode: u32) -> ServerResult<UnixListener> {
    let listener = UnixListener::bind(path)?;
    if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
        let _ = fs::remove_file(path);
        return Err(ServerError::Io(error));
    }
    if let Err(error) = listener.set_nonblocking(true) {
        let _ = fs::remove_file(path);
        return Err(ServerError::Io(error));
    }
    Ok(listener)
}

fn validate_config(config: &ProtectedApiServerConfig) -> ServerResult<()> {
    if config.max_connections == 0 {
        return Err(ServerError::InvalidConfiguration(
            "connection limit must be nonzero",
        ));
    }
    if config.use_socket_path == config.admin_socket_path {
        return Err(ServerError::InvalidConfiguration(
            "use/admin socket paths must differ",
        ));
    }
    for path in [&config.use_socket_path, &config.admin_socket_path] {
        if !path.parent().is_some_and(Path::is_dir) {
            return Err(ServerError::InvalidConfiguration(
                "socket parent directory is missing",
            ));
        }
    }
    Ok(())
}

fn prepare_socket_path(path: &Path) -> ServerResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => match UnixStream::connect(path) {
            Ok(_) => Err(ServerError::SocketAlreadyActive(path.to_path_buf())),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionRefused | ErrorKind::NotFound
                ) =>
            {
                fs::remove_file(path)?;
                Ok(())
            }
            Err(error) => Err(ServerError::Io(error)),
        },
        Ok(_) => Err(ServerError::SocketOccupied(path.to_path_buf())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ServerError::Io(error)),
    }
}

fn reap(workers: &mut Vec<JoinHandle<()>>) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let worker = workers.swap_remove(index);
            let _ = worker.join();
        } else {
            index += 1;
        }
    }
}

struct ConnectionPermit {
    count: Arc<AtomicUsize>,
}
impl ConnectionPermit {
    fn acquire(count: Arc<AtomicUsize>, limit: usize) -> Option<Self> {
        loop {
            let current = count.load(Ordering::Acquire);
            if current >= limit {
                return None;
            }
            if count
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(Self { count });
            }
        }
    }
}
impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn secret_reader_preserves_following_frame() {
        let input = Cursor::new(b"{\"value\":1}\n{\"value\":2}\n".to_vec());
        let mut reader = BufReader::new(input);

        let first: serde_json::Value = read_secret_json_line(&mut reader, 64).unwrap().unwrap();
        let second: serde_json::Value = read_secret_json_line(&mut reader, 64).unwrap().unwrap();

        assert_eq!(first["value"], 1);
        assert_eq!(second["value"], 2);
        assert!(
            read_secret_json_line::<serde_json::Value>(&mut reader, 64)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn secret_reader_accepts_frame_exactly_at_bound() {
        let frame = b"{\"v\":1}";
        let mut input = frame.to_vec();
        input.push(b'\n');
        let mut reader = BufReader::new(Cursor::new(input));

        let value: serde_json::Value = read_secret_json_line(&mut reader, frame.len())
            .unwrap()
            .unwrap();
        assert_eq!(value["v"], 1);
    }

    #[test]
    fn secret_reader_rejects_oversized_frame() {
        let mut reader = BufReader::new(Cursor::new(b"123456789\n".to_vec()));
        assert!(matches!(
            read_secret_json_line::<serde_json::Value>(&mut reader, 8),
            Err(ServerError::InvalidConfiguration(
                "admin protocol frame exceeds bound or is truncated"
            ))
        ));
    }

    #[test]
    fn secret_reader_rejects_truncated_frame() {
        let mut reader = BufReader::new(Cursor::new(b"{\"value\":1}".to_vec()));
        assert!(matches!(
            read_secret_json_line::<serde_json::Value>(&mut reader, 64),
            Err(ServerError::InvalidConfiguration(
                "admin protocol frame exceeds bound or is truncated"
            ))
        ));
    }

    #[test]
    fn secret_reader_rejects_malformed_json() {
        let mut reader = BufReader::new(Cursor::new(b"{bad}\n".to_vec()));
        assert!(matches!(
            read_secret_json_line::<serde_json::Value>(&mut reader, 64),
            Err(ServerError::InvalidConfiguration(
                "admin protocol frame is invalid JSON"
            ))
        ));
    }
}
