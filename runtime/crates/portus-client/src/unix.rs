use crate::{
    DEFAULT_IO_TIMEOUT, DEFAULT_MAX_FRAME_BYTES, FrameError, read_json_line, write_json_line,
};
use portus_protocol::{RequestEnvelope, ResponseEnvelope};
use serde::{Serialize, de::DeserializeOwned};
use std::{io::BufReader, os::unix::net::UnixStream, path::Path, time::Duration};

/// Thin synchronous client for the local `portusd` Unix-domain JSONL protocol.
pub struct UnixRuntimeClient {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    max_frame_bytes: usize,
}

impl UnixRuntimeClient {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, FrameError> {
        Self::connect_with_limits(path, DEFAULT_MAX_FRAME_BYTES, DEFAULT_IO_TIMEOUT)
    }

    pub fn connect_with_limits(
        path: impl AsRef<Path>,
        max_frame_bytes: usize,
        io_timeout: Duration,
    ) -> Result<Self, FrameError> {
        let writer = UnixStream::connect(path)?;
        writer.set_read_timeout(Some(io_timeout))?;
        writer.set_write_timeout(Some(io_timeout))?;
        let reader_stream = writer.try_clone()?;
        reader_stream.set_read_timeout(Some(io_timeout))?;
        Ok(Self {
            reader: BufReader::new(reader_stream),
            writer,
            max_frame_bytes,
        })
    }

    pub fn request<P, R>(
        &mut self,
        request: &RequestEnvelope<P>,
    ) -> Result<ResponseEnvelope<R>, FrameError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        self.send(request)?;
        self.read()?.ok_or(FrameError::TruncatedFrame)
    }

    pub fn send<T>(&mut self, value: &T) -> Result<(), FrameError>
    where
        T: Serialize,
    {
        write_json_line(&mut self.writer, value, self.max_frame_bytes)
    }

    pub fn read<T>(&mut self) -> Result<Option<T>, FrameError>
    where
        T: DeserializeOwned,
    {
        read_json_line(&mut self.reader, self.max_frame_bytes)
    }
}
