//! Shared bounded local PortusOS client and JSONL framing support.
//!
//! Framing is transport-independent so malformed/oversized input can be tested
//! on development hosts without running `portusd`. The actual local runtime
//! transport is a Unix domain socket on supported PortusOS targets.

mod framing;

pub use framing::{
    DEFAULT_IO_TIMEOUT, DEFAULT_MAX_FRAME_BYTES, FrameError, read_json_line, write_json_line,
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::UnixRuntimeClient;
