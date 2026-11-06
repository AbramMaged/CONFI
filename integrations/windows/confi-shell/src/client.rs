// CONFI — Shell Integration IPC Client
//
// Sends framed requests to the running CONFI instance via named pipe.
// Features:
//   - Bounded retries with `WaitNamedPipe` for handling busy pipe instances.
//   - Distinguishes between:
//       * ServerUnavailable (app not running -> trigger deep-link launch)
//       * PipeBusy (app running but congested)
//       * BrokenPipe (connection severed during transmission)
//       * ProtocolError (framing or deserialization errors)
//   - Structured request transmission and structured acknowledgement receipt.

use std::fs::OpenOptions;

use crate::protocol::{
    read_framed_message, write_framed_message, ConfiRequest, ConfiResponse, IpcError,
    DEFAULT_MAX_PAYLOAD_SIZE,
};

/// Errors encountered by the IPC client.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IpcClientError {
    #[error("CONFI server is not running (pipe not found): {0}")]
    ServerUnavailable(String),

    #[error("CONFI named pipe is busy: {0}")]
    PipeBusy(String),

    #[error("Connection broken during IPC transmission: {0}")]
    BrokenPipe(String),

    #[error("IPC protocol violation: {0}")]
    ProtocolError(String),

    #[error("I/O error: {0}")]
    Io(String),
}

impl From<IpcError> for IpcClientError {
    fn from(err: IpcError) -> Self {
        match err {
            IpcError::Io(ref e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                IpcClientError::BrokenPipe(err.to_string())
            }
            IpcError::IncompleteHeader { .. } | IpcError::IncompletePayload { .. } => {
                IpcClientError::BrokenPipe(err.to_string())
            }
            IpcError::PayloadTooLarge { .. } | IpcError::Serialization(_) => {
                IpcClientError::ProtocolError(err.to_string())
            }
            IpcError::InvalidRequest(msg) => IpcClientError::ProtocolError(msg),
            IpcError::Io(e) => IpcClientError::Io(e.to_string()),
        }
    }
}

/// Compute the user-specific named pipe path.
pub fn get_pipe_name() -> String {
    format!(
        "\\\\.\\pipe\\confi-ipc-{}",
        std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string())
    )
}

/// Connect to the named pipe using bounded retries with `WaitNamedPipe`.
#[cfg(windows)]
pub fn connect_to_pipe(
    pipe_name: &str,
    max_retries: u32,
    timeout_per_retry_ms: u32,
) -> Result<std::fs::File, IpcClientError> {
    use windows::core::HSTRING;
    use windows::Win32::System::Pipes::WaitNamedPipeW;

    let pipe_hstring = HSTRING::from(pipe_name);

    for attempt in 0..=max_retries {
        match OpenOptions::new().read(true).write(true).open(pipe_name) {
            Ok(file) => return Ok(file),
            Err(e) => {
                let raw_code = e.raw_os_error();
                match raw_code {
                    // ERROR_FILE_NOT_FOUND (2): server not listening
                    Some(2) => {
                        return Err(IpcClientError::ServerUnavailable(format!(
                            "Pipe '{}' does not exist. CONFI is not running.",
                            pipe_name
                        )));
                    }
                    // ERROR_PIPE_BUSY (231): server instance in use
                    Some(231) => {
                        if attempt < max_retries {
                            // Wait for pipe instance to become available
                            let wait_ok = unsafe {
                                WaitNamedPipeW(&pipe_hstring, timeout_per_retry_ms).as_bool()
                            };
                            if !wait_ok {
                                eprintln!(
                                    "WaitNamedPipeW timed out (attempt {}/{})",
                                    attempt + 1,
                                    max_retries
                                );
                            }
                            continue;
                        } else {
                            return Err(IpcClientError::PipeBusy(format!(
                                "Named pipe '{}' remained busy after {} retries",
                                pipe_name, max_retries
                            )));
                        }
                    }
                    Some(109) => {
                        return Err(IpcClientError::BrokenPipe(e.to_string()));
                    }
                    _ => {
                        return Err(IpcClientError::Io(format!(
                            "Failed to open pipe '{}': {e} (code: {raw_code:?})",
                            pipe_name
                        )));
                    }
                }
            }
        }
    }

    Err(IpcClientError::PipeBusy(
        "Exhausted connection retries".to_string(),
    ))
}

#[cfg(not(windows))]
pub fn connect_to_pipe(
    _pipe_name: &str,
    _max_retries: u32,
    _timeout_per_retry_ms: u32,
) -> Result<std::fs::File, IpcClientError> {
    Err(IpcClientError::ServerUnavailable(
        "Named pipes are only supported on Windows".to_string(),
    ))
}

/// Send a request over the named pipe and receive the structured acknowledgement.
pub fn send_pipe_request(
    pipe_name: &str,
    req: &ConfiRequest,
    max_retries: u32,
    timeout_per_retry_ms: u32,
) -> Result<ConfiResponse, IpcClientError> {
    let mut pipe_file = connect_to_pipe(pipe_name, max_retries, timeout_per_retry_ms)?;

    // Serialize request
    let json = serde_json::to_vec(req)
        .map_err(|e| IpcClientError::ProtocolError(format!("Failed to serialize request: {e}")))?;

    // Send framed request
    write_framed_message(&mut pipe_file, &json, DEFAULT_MAX_PAYLOAD_SIZE)?;

    // Receive framed acknowledgement / error response
    let resp_bytes =
        read_framed_message(&mut pipe_file, DEFAULT_MAX_PAYLOAD_SIZE)?.ok_or_else(|| {
            IpcClientError::BrokenPipe(
                "Server disconnected before sending acknowledgement".to_string(),
            )
        })?;

    let response: ConfiResponse = serde_json::from_slice(&resp_bytes).map_err(|e| {
        IpcClientError::ProtocolError(format!("Invalid response JSON from server: {e}"))
    })?;

    Ok(response)
}
