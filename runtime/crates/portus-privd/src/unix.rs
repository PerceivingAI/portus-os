use crate::{
    CANONICAL_ADMIN_SOCKET, CANONICAL_AUDIT_PATH, CANONICAL_USE_SOCKET, FilesystemPolicyRepository,
    PrivilegeCore, UnavailableExecutor,
};
use nix::{
    sys::socket::{getsockopt, sockopt::PeerCredentials},
    unistd::{Gid, Group, Uid, chown},
};
use portus_client::{
    DEFAULT_IO_TIMEOUT, DEFAULT_MAX_FRAME_BYTES, FrameError, read_json_line, write_json_line,
};
use portus_policy::{PolicyError, PolicyPaths, PolicySnapshot, PolicyTrust};
use portus_protocol::{Principal, RequestEnvelope, ResponseEnvelope};
use serde_json::Value;
use std::{
    fmt, fs,
    io::{BufReader, ErrorKind},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
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

const USE_SOCKET_MODE: u32 = 0o660;
const ADMIN_SOCKET_MODE: u32 = 0o600;
const USE_SOCKET_GROUP: &str = "portus-priv-users";
const ACCEPT_POLL: Duration = Duration::from_millis(50);
pub const DEFAULT_MAX_CONNECTIONS: usize = 32;

#[derive(Clone, Debug)]
pub struct PrivilegeServerConfig {
    pub use_socket_path: PathBuf,
    pub admin_socket_path: PathBuf,
    pub policy_paths: PolicyPaths,
    pub policy_trust: PolicyTrust,
    pub audit_path: PathBuf,
    pub max_frame_bytes: usize,
    pub io_timeout: Duration,
    pub max_connections: usize,
}

impl PrivilegeServerConfig {
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            use_socket_path: CANONICAL_USE_SOCKET.into(),
            admin_socket_path: CANONICAL_ADMIN_SOCKET.into(),
            policy_paths: PolicyPaths::canonical(),
            policy_trust: PolicyTrust::RootOwnedSystem,
            audit_path: CANONICAL_AUDIT_PATH.into(),
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            io_timeout: DEFAULT_IO_TIMEOUT,
            max_connections: DEFAULT_MAX_CONNECTIONS,
        }
    }
}

#[derive(Debug)]
pub enum PrivilegeServerError {
    Io(std::io::Error),
    Frame(FrameError),
    Policy(PolicyError),
    Audit(portus_audit::AuditError),
    NotRoot,
    InvalidConfiguration(&'static str),
    SocketOccupied(PathBuf),
    SocketAlreadyActive(PathBuf),
    PeerCredentials(String),
}

impl fmt::Display for PrivilegeServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "privilege server I/O error: {error}"),
            Self::Frame(error) => write!(f, "privilege protocol framing error: {error}"),
            Self::Policy(error) => write!(f, "privilege policy error: {error}"),
            Self::Audit(error) => write!(f, "privilege audit error: {error}"),
            Self::NotRoot => f.write_str("portus-privd must run as UID 0"),
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid privilege server configuration: {message}")
            }
            Self::SocketOccupied(path) => {
                write!(f, "privilege socket path is occupied: {}", path.display())
            }
            Self::SocketAlreadyActive(path) => {
                write!(f, "privilege socket already active: {}", path.display())
            }
            Self::PeerCredentials(message) => {
                write!(f, "failed to authenticate privilege peer: {message}")
            }
        }
    }
}
impl std::error::Error for PrivilegeServerError {}
impl From<std::io::Error> for PrivilegeServerError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<FrameError> for PrivilegeServerError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}
impl From<PolicyError> for PrivilegeServerError {
    fn from(value: PolicyError) -> Self {
        Self::Policy(value)
    }
}
impl From<portus_audit::AuditError> for PrivilegeServerError {
    fn from(value: portus_audit::AuditError) -> Self {
        Self::Audit(value)
    }
}
pub type PrivilegeServerResult<T> = Result<T, PrivilegeServerError>;

pub struct PrivilegeServer {
    config: PrivilegeServerConfig,
    core: Arc<PrivilegeCore>,
    use_listener: UnixListener,
    admin_listener: UnixListener,
    active_connections: Arc<AtomicUsize>,
}

impl PrivilegeServer {
    pub fn bind(config: PrivilegeServerConfig) -> PrivilegeServerResult<Self> {
        if !Uid::effective().is_root() {
            return Err(PrivilegeServerError::NotRoot);
        }
        validate_config(&config)?;
        prepare_socket_path(&config.use_socket_path)?;
        prepare_socket_path(&config.admin_socket_path)?;
        let snapshot = PolicySnapshot::load(&config.policy_paths, config.policy_trust)?;
        let repository = Arc::new(FilesystemPolicyRepository::new(config.policy_paths.clone()));
        let audit = Arc::new(portus_audit::FileAuditSink::open(&config.audit_path)?);
        let core = Arc::new(PrivilegeCore::new_with_audit(
            snapshot,
            repository,
            Arc::new(UnavailableExecutor),
            audit,
        ));
        let use_group = Group::from_name(USE_SOCKET_GROUP)
            .map_err(|error| PrivilegeServerError::PeerCredentials(error.to_string()))?
            .ok_or(PrivilegeServerError::InvalidConfiguration(
                "required portus-priv-users group is missing",
            ))?;
        let use_listener = bind_listener(&config.use_socket_path, USE_SOCKET_MODE, use_group.gid)?;
        let admin_listener = match bind_listener(
            &config.admin_socket_path,
            ADMIN_SOCKET_MODE,
            Gid::from_raw(0),
        ) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = fs::remove_file(&config.use_socket_path);
                return Err(error);
            }
        };
        Ok(Self {
            config,
            core,
            use_listener,
            admin_listener,
            active_connections: Arc::new(AtomicUsize::new(0)),
        })
    }

    #[must_use]
    pub fn core(&self) -> &Arc<PrivilegeCore> {
        &self.core
    }

    pub fn run_until(self, shutdown: Arc<AtomicBool>) -> PrivilegeServerResult<()> {
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

impl Drop for PrivilegeServer {
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
    server: &PrivilegeServer,
    workers: &mut Vec<JoinHandle<()>>,
) -> PrivilegeServerResult<()> {
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
            Err(error) => return Err(PrivilegeServerError::Io(error)),
        }
    }
}

fn serve_connection(
    stream: UnixStream,
    core: Arc<PrivilegeCore>,
    config: PrivilegeServerConfig,
    admin: bool,
) -> PrivilegeServerResult<()> {
    stream.set_read_timeout(Some(config.io_timeout))?;
    stream.set_write_timeout(Some(config.io_timeout))?;
    let principal = peer_principal(&stream)?;
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;
    let request: RequestEnvelope<Value> = match read_json_line(&mut reader, config.max_frame_bytes)?
    {
        Some(request) => request,
        None => return Ok(()),
    };
    let response: ResponseEnvelope<Value> = if admin {
        core.dispatch_admin(principal, request)
    } else {
        core.dispatch_use(principal, request)
    };
    write_json_line(&mut writer, &response, config.max_frame_bytes)?;
    Ok(())
}

fn peer_principal(stream: &UnixStream) -> PrivilegeServerResult<Principal> {
    let credentials = getsockopt(stream, PeerCredentials)
        .map_err(|error| PrivilegeServerError::PeerCredentials(error.to_string()))?;
    Ok(Principal::new(credentials.uid(), credentials.gid()))
}

fn bind_listener(path: &Path, mode: u32, gid: Gid) -> PrivilegeServerResult<UnixListener> {
    let listener = UnixListener::bind(path)?;
    if let Err(error) = chown(path, Some(Uid::from_raw(0)), Some(gid)) {
        let _ = fs::remove_file(path);
        return Err(PrivilegeServerError::PeerCredentials(error.to_string()));
    }
    if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
        let _ = fs::remove_file(path);
        return Err(PrivilegeServerError::Io(error));
    }
    if let Err(error) = listener.set_nonblocking(true) {
        let _ = fs::remove_file(path);
        return Err(PrivilegeServerError::Io(error));
    }
    Ok(listener)
}

fn validate_config(config: &PrivilegeServerConfig) -> PrivilegeServerResult<()> {
    if config.max_frame_bytes == 0 || config.max_connections == 0 {
        return Err(PrivilegeServerError::InvalidConfiguration(
            "frame/connection limits must be nonzero",
        ));
    }
    for path in [&config.use_socket_path, &config.admin_socket_path] {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(PrivilegeServerError::InvalidConfiguration(
                "socket path has no parent",
            ))?;
        if !parent.is_dir() {
            return Err(PrivilegeServerError::InvalidConfiguration(
                "socket parent directory is missing",
            ));
        }
    }
    Ok(())
}

fn prepare_socket_path(path: &Path) -> PrivilegeServerResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => match UnixStream::connect(path) {
            Ok(_) => Err(PrivilegeServerError::SocketAlreadyActive(
                path.to_path_buf(),
            )),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionRefused | ErrorKind::NotFound
                ) =>
            {
                fs::remove_file(path)?;
                Ok(())
            }
            Err(error) => Err(PrivilegeServerError::Io(error)),
        },
        Ok(_) => Err(PrivilegeServerError::SocketOccupied(path.to_path_buf())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PrivilegeServerError::Io(error)),
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
