use portus_protected_api::{ProviderResponse, UseRequest};
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
            Self::Unavailable => f.write_str("protected API provider is unavailable"),
            Self::Timeout => f.write_str("protected API request timed out"),
            Self::Protocol => {
                f.write_str("protected API provider returned an invalid protocol frame")
            }
            Self::Io(error) => write!(f, "protected API client I/O error: {error}"),
        }
    }
}
impl std::error::Error for TransportError {}

pub trait ApiTransport {
    fn send(
        &mut self,
        request: &UseRequest,
        timeout: Duration,
    ) -> Result<ProviderResponse, TransportError>;
}

pub struct SystemApiTransport {
    socket_path: PathBuf,
}

impl Default for SystemApiTransport {
    fn default() -> Self {
        Self {
            socket_path: portus_protected_api::CANONICAL_USE_SOCKET.into(),
        }
    }
}

impl SystemApiTransport {
    #[must_use]
    pub fn for_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
        }
    }
}

#[cfg(target_os = "linux")]
impl ApiTransport for SystemApiTransport {
    fn send(
        &mut self,
        request: &UseRequest,
        timeout: Duration,
    ) -> Result<ProviderResponse, TransportError> {
        use portus_client::{read_json_line, write_json_line};
        use std::{io::BufReader, os::unix::net::UnixStream};
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
        write_json_line(
            &mut writer,
            request,
            portus_protected_api::MAX_PROTOCOL_FRAME_BYTES,
        )
        .map_err(map_frame)?;
        let response: ProviderResponse = read_json_line(
            &mut reader,
            portus_protected_api::MAX_RESPONSE_BYTES + 64 * 1024,
        )
        .map_err(map_frame)?
        .ok_or(TransportError::Protocol)?;
        response.validate().map_err(|_| TransportError::Protocol)?;
        if response.request_id != request.request_id {
            return Err(TransportError::Protocol);
        }
        Ok(response)
    }
}

#[cfg(target_os = "linux")]
fn map_frame(error: portus_client::FrameError) -> TransportError {
    use std::io::ErrorKind;
    match error {
        portus_client::FrameError::Io(io)
            if matches!(io.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) =>
        {
            TransportError::Timeout
        }
        portus_client::FrameError::Io(io) => TransportError::Io(io),
        _ => TransportError::Protocol,
    }
}

#[cfg(not(target_os = "linux"))]
impl ApiTransport for SystemApiTransport {
    fn send(
        &mut self,
        _request: &UseRequest,
        _timeout: Duration,
    ) -> Result<ProviderResponse, TransportError> {
        let _ = &self.socket_path;
        Err(TransportError::Unavailable)
    }
}
