use portus_protected_api::{AdminRequest, ProviderResponse};
use std::{fmt, path::PathBuf, time::Duration};

#[derive(Debug)]
pub enum TransportError {
    Unavailable,
    Timeout,
    Protocol,
    Io(std::io::Error),
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => f.write_str("protected API admin interface is unavailable"),
            Self::Timeout => f.write_str("protected API admin request timed out"),
            Self::Protocol => {
                f.write_str("protected API admin interface returned an invalid protocol frame")
            }
            Self::Io(error) => write!(f, "protected API admin I/O error: {error}"),
        }
    }
}
impl std::error::Error for TransportError {}

pub trait AdminTransport {
    fn send(
        &mut self,
        request: &AdminRequest,
        timeout: Duration,
    ) -> Result<ProviderResponse, TransportError>;
}

pub struct SystemAdminTransport {
    socket_path: PathBuf,
}
impl Default for SystemAdminTransport {
    fn default() -> Self {
        Self {
            socket_path: portus_protected_api::CANONICAL_ADMIN_SOCKET.into(),
        }
    }
}
impl SystemAdminTransport {
    #[must_use]
    pub fn for_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
        }
    }
}

#[cfg(target_os = "linux")]
impl AdminTransport for SystemAdminTransport {
    fn send(
        &mut self,
        request: &AdminRequest,
        timeout: Duration,
    ) -> Result<ProviderResponse, TransportError> {
        use portus_client::read_json_line;
        use std::{
            io::{BufReader, ErrorKind, Write},
            os::unix::net::UnixStream,
        };
        use zeroize::Zeroizing;
        let stream =
            UnixStream::connect(&self.socket_path).map_err(|_| TransportError::Unavailable)?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(TransportError::Io)?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(TransportError::Io)?;
        let reader_stream = stream.try_clone().map_err(TransportError::Io)?;
        let mut reader = BufReader::new(reader_stream);
        let mut writer = stream;
        let mut encoded =
            Zeroizing::new(serde_json::to_vec(request).map_err(|_| TransportError::Protocol)?);
        encoded.push(b'\n');
        if encoded.len() > portus_protected_api::MAX_PROTOCOL_FRAME_BYTES {
            return Err(TransportError::Protocol);
        }
        writer.write_all(&encoded).map_err(|error| {
            if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) {
                TransportError::Timeout
            } else {
                TransportError::Io(error)
            }
        })?;
        writer.flush().map_err(TransportError::Io)?;
        let response: ProviderResponse = read_json_line(
            &mut reader,
            portus_protected_api::MAX_RESPONSE_BYTES + 64 * 1024,
        )
        .map_err(|error| match error {
            portus_client::FrameError::Io(io)
                if matches!(io.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) =>
            {
                TransportError::Timeout
            }
            portus_client::FrameError::Io(io) => TransportError::Io(io),
            _ => TransportError::Protocol,
        })?
        .ok_or(TransportError::Protocol)?;
        response.validate().map_err(|_| TransportError::Protocol)?;
        if response.request_id != request.request_id {
            return Err(TransportError::Protocol);
        }
        Ok(response)
    }
}

#[cfg(not(target_os = "linux"))]
impl AdminTransport for SystemAdminTransport {
    fn send(
        &mut self,
        _request: &AdminRequest,
        _timeout: Duration,
    ) -> Result<ProviderResponse, TransportError> {
        let _ = &self.socket_path;
        Err(TransportError::Unavailable)
    }
}
