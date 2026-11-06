// CONFI — Shell Integration IPC Server
//
// Implements named pipe listener for receiving shell integration requests.
// Protocol Lifecycle:
//   1. Client connects to pipe.
//   2. Server reads 4-byte length prefix.
//   3. Server enforces max size (default: 1 MiB) and reads exact payload bytes.
//   4. Server decodes and validates request syntax and semantics.
//   5. If valid, request is dispatched to enqueue handler and `ConfiResponse::Ack` is returned.
//   6. If invalid or malformed, `ConfiResponse::Error` is returned.
//   7. Connection is cleanly flushed and disconnected.

use crate::protocol::{
    read_framed_message, write_framed_message, ConfiRequest, ConfiResponse, IpcError,
};

/// Trait defining the application request dispatcher / enqueue handler.
pub trait RequestHandler {
    fn handle_request(&self, req: ConfiRequest) -> Result<String, String>;
}

/// Default echo handler that simulates accepting the request into an application queue.
pub struct DefaultQueueHandler;

impl RequestHandler for DefaultQueueHandler {
    fn handle_request(&self, req: ConfiRequest) -> Result<String, String> {
        Ok(format!(
            "Accepted '{}' operation with {} file(s) into queue",
            req.operation,
            req.files.len()
        ))
    }
}

/// Process a single connection from a read/write stream (used for both named pipe and tests).
pub fn handle_client_stream<S: std::io::Read + std::io::Write, H: RequestHandler>(
    stream: &mut S,
    handler: &H,
    max_payload_size: usize,
) -> Result<Option<ConfiRequest>, IpcError> {
    let payload = match read_framed_message(stream, max_payload_size) {
        Ok(Some(p)) => p,
        Ok(None) => return Ok(None),
        Err(e) => {
            // Write structured error back to client if possible
            let err_resp = ConfiResponse::Error {
                code: "PROTOCOL_ERROR".to_string(),
                message: e.to_string(),
            };
            if let Ok(err_bytes) = serde_json::to_vec(&err_resp) {
                let _ = write_framed_message(stream, &err_bytes, max_payload_size);
            }
            return Err(e);
        }
    };

    // Deserialize request
    let req: ConfiRequest = match serde_json::from_slice(&payload) {
        Ok(r) => r,
        Err(e) => {
            let err_resp = ConfiResponse::Error {
                code: "INVALID_JSON".to_string(),
                message: format!("Malformed request JSON: {e}"),
            };
            let err_bytes = serde_json::to_vec(&err_resp).unwrap();
            write_framed_message(stream, &err_bytes, max_payload_size)?;
            return Err(IpcError::Serialization(e.to_string()));
        }
    };

    // Validate request
    if let Err(e) = crate::protocol::validate_request(&req) {
        let err_resp = ConfiResponse::Error {
            code: "VALIDATION_FAILED".to_string(),
            message: e.to_string(),
        };
        let err_bytes = serde_json::to_vec(&err_resp).unwrap();
        write_framed_message(stream, &err_bytes, max_payload_size)?;
        return Err(e);
    }

    // Dispatch to application enqueue handler
    match handler.handle_request(req.clone()) {
        Ok(msg) => {
            let ack = ConfiResponse::Ack {
                operation: req.operation.clone(),
                accepted_files: req.files.len(),
                message: msg,
            };
            let ack_bytes = serde_json::to_vec(&ack).unwrap();
            write_framed_message(stream, &ack_bytes, max_payload_size)?;
            Ok(Some(req))
        }
        Err(e) => {
            let err_resp = ConfiResponse::Error {
                code: "QUEUE_REJECTED".to_string(),
                message: e,
            };
            let err_bytes = serde_json::to_vec(&err_resp).unwrap();
            write_framed_message(stream, &err_bytes, max_payload_size)?;
            Ok(None)
        }
    }
}

/// Run a named pipe server loop on Windows.
#[cfg(windows)]
pub fn run_pipe_server<H: RequestHandler>(
    pipe_name: &str,
    handler: &H,
    max_payload_size: usize,
) -> Result<(), String> {
    use std::os::windows::io::FromRawHandle;
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    let pipe_hstring = HSTRING::from(pipe_name);

    eprintln!("CONFI Named Pipe Server listening on: {pipe_name}");
    eprintln!("Press Ctrl+C to terminate.");

    loop {
        let pipe_handle = unsafe {
            CreateNamedPipeW(
                &pipe_hstring,
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                windows::Win32::System::Pipes::PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                None,
            )
        };

        if pipe_handle.is_invalid() {
            return Err(format!(
                "Failed to create named pipe: {:?}",
                std::io::Error::last_os_error()
            ));
        }

        let connected = unsafe { ConnectNamedPipe(pipe_handle, None) };
        if let Err(e) = connected {
            // ERROR_PIPE_CONNECTED (535) is expected when client connected prior to ConnectNamedPipe
            if e.code().0 as u32 != 0x80070217 && e.code().0 != 535 {
                eprintln!("ConnectNamedPipe note: {e}");
            }
        }

        // Convert HANDLE to std::fs::File for buffered reading and writing
        let mut pipe_file = unsafe { std::fs::File::from_raw_handle(pipe_handle.0) };

        match handle_client_stream(&mut pipe_file, handler, max_payload_size) {
            Ok(Some(req)) => {
                eprintln!(
                    "  [IPC] Handled request: op='{}', {} file(s)",
                    req.operation,
                    req.files.len()
                );
            }
            Ok(None) => {
                eprintln!("  [IPC] Client disconnected or rejected");
            }
            Err(e) => {
                eprintln!("  [IPC] Protocol error: {e}");
            }
        }

        unsafe {
            let _ = DisconnectNamedPipe(pipe_handle);
        }
        // pipe_file drops here and closes handle automatically
    }
}
