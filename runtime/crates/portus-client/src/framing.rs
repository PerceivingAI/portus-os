use serde::{Serialize, de::DeserializeOwned};
use std::{
    error::Error,
    fmt,
    io::{self, BufRead, Write},
    time::Duration,
};

pub const DEFAULT_MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    FrameTooLarge { limit: usize },
    TruncatedFrame,
    InvalidJson(serde_json::Error),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "runtime framing I/O error: {error}"),
            Self::FrameTooLarge { limit } => {
                write!(f, "runtime JSONL frame exceeds {limit} bytes")
            }
            Self::TruncatedFrame => f.write_str("runtime JSONL frame ended before newline"),
            Self::InvalidJson(error) => write!(f, "invalid runtime JSON: {error}"),
        }
    }
}

impl Error for FrameError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidJson(error) => Some(error),
            Self::FrameTooLarge { .. } | Self::TruncatedFrame => None,
        }
    }
}

impl From<io::Error> for FrameError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for FrameError {
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidJson(value)
    }
}

/// Reads one newline-terminated bounded JSON value.
///
/// `Ok(None)` means clean EOF before a new frame began. A partial frame at EOF
/// is rejected rather than interpreted as a complete request.
pub fn read_json_line<R, T>(reader: &mut R, max_bytes: usize) -> Result<Option<T>, FrameError>
where
    R: BufRead,
    T: DeserializeOwned,
{
    let frame = match read_frame(reader, max_bytes)? {
        Some(frame) => frame,
        None => return Ok(None),
    };
    serde_json::from_slice(&frame)
        .map(Some)
        .map_err(FrameError::from)
}

pub fn write_json_line<W, T>(writer: &mut W, value: &T, max_bytes: usize) -> Result<(), FrameError>
where
    W: Write,
    T: Serialize,
{
    let frame = serde_json::to_vec(value)?;
    if frame.len() > max_bytes {
        return Err(FrameError::FrameTooLarge { limit: max_bytes });
    }
    writer.write_all(&frame)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn read_frame<R: BufRead>(reader: &mut R, max_bytes: usize) -> Result<Option<Vec<u8>>, FrameError> {
    let mut frame = Vec::with_capacity(max_bytes.min(4096));
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(FrameError::TruncatedFrame)
            };
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index);
        if frame.len().saturating_add(take) > max_bytes {
            return Err(FrameError::FrameTooLarge { limit: max_bytes });
        }
        frame.extend_from_slice(&available[..take]);
        reader.consume(take + usize::from(newline.is_some()));

        if newline.is_some() {
            return Ok(Some(frame));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::io::{BufReader, Cursor};

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct Example {
        value: u32,
    }

    #[test]
    fn reads_one_bounded_jsonl_frame_and_preserves_following_frame() {
        let input = Cursor::new(b"{\"value\":1}\n{\"value\":2}\n".to_vec());
        let mut reader = BufReader::new(input);
        assert_eq!(
            read_json_line(&mut reader, 64).unwrap(),
            Some(Example { value: 1 })
        );
        assert_eq!(
            read_json_line(&mut reader, 64).unwrap(),
            Some(Example { value: 2 })
        );
        assert_eq!(read_json_line::<_, Example>(&mut reader, 64).unwrap(), None);
    }

    #[test]
    fn oversized_frame_is_rejected_without_unbounded_allocation() {
        let input = Cursor::new(b"123456789\n".to_vec());
        let mut reader = BufReader::new(input);
        assert!(matches!(
            read_json_line::<_, serde_json::Value>(&mut reader, 8),
            Err(FrameError::FrameTooLarge { limit: 8 })
        ));
    }

    #[test]
    fn partial_frame_at_eof_is_rejected() {
        let input = Cursor::new(b"{\"value\":1}".to_vec());
        let mut reader = BufReader::new(input);
        assert!(matches!(
            read_json_line::<_, Example>(&mut reader, 64),
            Err(FrameError::TruncatedFrame)
        ));
    }

    #[test]
    fn malformed_json_is_rejected() {
        let input = Cursor::new(b"{bad}\n".to_vec());
        let mut reader = BufReader::new(input);
        assert!(matches!(
            read_json_line::<_, Example>(&mut reader, 64),
            Err(FrameError::InvalidJson(_))
        ));
    }

    #[test]
    fn writer_enforces_same_bound() {
        let mut output = Vec::new();
        write_json_line(&mut output, &Example { value: 7 }, 64).unwrap();
        assert_eq!(output, b"{\"value\":7}\n");

        let mut output = Vec::new();
        assert!(matches!(
            write_json_line(&mut output, &Example { value: 7 }, 4),
            Err(FrameError::FrameTooLarge { limit: 4 })
        ));
        assert!(output.is_empty());
    }
}
