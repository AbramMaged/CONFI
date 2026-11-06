// CONFI — Shell Integration IPC Protocol
//
// Defines length-prefixed framing and structured request/response types.
// Framing specification:
//   - 4-byte little-endian length prefix indicating payload byte length.
//   - Followed by exactly `length` bytes of JSON UTF-8 payload.
//   - Configurable maximum payload size (default: 1 MiB = 1,048,576 bytes).
//   - Bounded reads preventing unbounded memory allocation on corrupt streams.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;

/// Default maximum payload size: 1 MiB.
pub const DEFAULT_MAX_PAYLOAD_SIZE: usize = 1024 * 1024;

/// Unified request format sent from shell helper to the main application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfiRequest {
    pub operation: String,
    pub files: Vec<PathBuf>,
}

/// Structured response returned over the IPC channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ConfiResponse {
    Ack {
        operation: String,
        accepted_files: usize,
        message: String,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Errors occurring at the IPC protocol/framing layer.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Incomplete length header (read {read} of 4 bytes)")]
    IncompleteHeader { read: usize },

    #[error("Payload size {length} bytes exceeds maximum allowed size of {max} bytes")]
    PayloadTooLarge { length: usize, max: usize },

    #[error("Incomplete payload (expected {expected} bytes, received {received} bytes)")]
    IncompletePayload { expected: usize, received: usize },

    #[error("JSON serialization/deserialization failed: {0}")]
    Serialization(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Read a length-prefixed message from a stream.
///
/// Returns `Ok(None)` on clean EOF before any bytes are read.
/// Returns `Err(IpcError)` on partial reads, oversized payloads, or I/O errors.
pub fn read_framed_message<R: Read>(
    reader: &mut R,
    max_size: usize,
) -> Result<Option<Vec<u8>>, IpcError> {
    let mut len_buf = [0u8; 4];
    let mut bytes_read = 0;

    while bytes_read < 4 {
        match reader.read(&mut len_buf[bytes_read..]) {
            Ok(0) => {
                if bytes_read == 0 {
                    // Clean EOF between messages
                    return Ok(None);
                } else {
                    return Err(IpcError::IncompleteHeader { read: bytes_read });
                }
            }
            Ok(n) => {
                bytes_read += n;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(IpcError::Io(e)),
        }
    }

    let payload_len = u32::from_le_bytes(len_buf) as usize;

    if payload_len > max_size {
        return Err(IpcError::PayloadTooLarge {
            length: payload_len,
            max: max_size,
        });
    }

    let mut payload = vec![0u8; payload_len];
    let mut payload_read = 0;

    while payload_read < payload_len {
        match reader.read(&mut payload[payload_read..]) {
            Ok(0) => {
                return Err(IpcError::IncompletePayload {
                    expected: payload_len,
                    received: payload_read,
                });
            }
            Ok(n) => {
                payload_read += n;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(IpcError::Io(e)),
        }
    }

    Ok(Some(payload))
}

/// Write a length-prefixed message to a stream.
pub fn write_framed_message<W: Write>(
    writer: &mut W,
    payload: &[u8],
    max_size: usize,
) -> Result<(), IpcError> {
    if payload.len() > max_size {
        return Err(IpcError::PayloadTooLarge {
            length: payload.len(),
            max: max_size,
        });
    }

    let len_prefix = (payload.len() as u32).to_le_bytes();
    writer.write_all(&len_prefix)?;
    writer.write_all(payload)?;
    writer.flush()?;

    Ok(())
}

/// Validate a `ConfiRequest` before queueing or processing.
pub fn validate_request(req: &ConfiRequest) -> Result<(), IpcError> {
    let op = req.operation.trim();
    if op.is_empty() {
        return Err(IpcError::InvalidRequest(
            "Operation cannot be empty".to_string(),
        ));
    }

    const VALID_OPERATIONS: &[&str] = &[
        "open",
        "compress",
        "convert",
        "split",
        "merge",
        "resize",
        "batch_convert",
        "batch_compress",
    ];

    if !VALID_OPERATIONS.contains(&op) {
        return Err(IpcError::InvalidRequest(format!(
            "Unknown operation '{op}'"
        )));
    }

    if req.files.is_empty() {
        return Err(IpcError::InvalidRequest(
            "File list cannot be empty".to_string(),
        ));
    }

    for (idx, file) in req.files.iter().enumerate() {
        if file.as_os_str().is_empty() {
            return Err(IpcError::InvalidRequest(format!(
                "File path at index {idx} is empty"
            )));
        }
    }

    Ok(())
}
