// CONFI — Shell Integration IPC Server
//
// Protocol lifecycle:
//   1. Client connects to the pipe.
//   2. Server reads the 4-byte length prefix.
//   3. Server enforces the max size (default 1 MiB) and reads the exact payload.
//   4. Server decodes and validates the request.
//   5. On success the request is returned to the caller, which acknowledges it.
//   6. On any failure an `ConfiResponse::Error` is written back first.
//
// The prototype has no queue yet: the accepted request is simply logged by the
// caller. The application's real dispatcher arrives with the Tauri app (M6/M7).

use crate::protocol::{
    read_framed_message, validate_request, write_framed_message, ConfiRequest, ConfiResponse,
    IpcError,
};

/// Process a single connection from a read/write stream.
///
/// Returns the accepted request, or `None` when the connection closed before a
/// complete message arrived.
pub fn handle_client_stream<S: std::io::Read + std::io::Write>(
    stream: &mut S,
    max_payload_size: usize,
) -> Result<Option<ConfiRequest>, IpcError> {
    let payload = match read_framed_message(stream, max_payload_size) {
        Ok(Some(payload)) => payload,
        Ok(None) => return Ok(None),
        Err(e) => {
            // Best-effort: the caller is already getting this error.
            let _ = respond(
                stream,
                ConfiResponse::Error {
                    code: "PROTOCOL_ERROR".to_string(),
                    message: e.to_string(),
                },
                max_payload_size,
            );
            return Err(e);
        }
    };

    let req: ConfiRequest = match serde_json::from_slice(&payload) {
        Ok(req) => req,
        Err(e) => {
            respond(
                stream,
                ConfiResponse::Error {
                    code: "INVALID_JSON".to_string(),
                    message: format!("Malformed request JSON: {e}"),
                },
                max_payload_size,
            )?;
            return Err(IpcError::Serialization(e.to_string()));
        }
    };

    if let Err(e) = validate_request(&req) {
        respond(
            stream,
            ConfiResponse::Error {
                code: "VALIDATION_FAILED".to_string(),
                message: e.to_string(),
            },
            max_payload_size,
        )?;
        return Err(e);
    }

    respond(
        stream,
        ConfiResponse::Ack {
            operation: req.operation.clone(),
            accepted_files: req.files.len(),
            message: format!(
                "Accepted '{}' operation with {} file(s)",
                req.operation,
                req.files.len()
            ),
        },
        max_payload_size,
    )?;

    Ok(Some(req))
}

/// Serialize and write one framed response, ignoring failures since the caller
/// is already returning an error or a completed result.
fn respond<S: std::io::Write>(
    stream: &mut S,
    response: ConfiResponse,
    max_payload_size: usize,
) -> Result<(), IpcError> {
    let bytes =
        serde_json::to_vec(&response).map_err(|e| IpcError::Serialization(e.to_string()))?;
    write_framed_message(stream, &bytes, max_payload_size)
}

/// Run a named pipe server loop on Windows.
#[cfg(windows)]
pub fn run_pipe_server(pipe_name: &str, max_payload_size: usize) -> Result<(), String> {
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

        if let Err(e) = unsafe { ConnectNamedPipe(pipe_handle, None) } {
            // ERROR_PIPE_CONNECTED (535) is expected when the client connected
            // before ConnectNamedPipe was called.
            if e.code().0 as u32 != 0x80070217 && e.code().0 != 535 {
                eprintln!("ConnectNamedPipe note: {e}");
            }
        }

        let mut pipe_file = unsafe { std::fs::File::from_raw_handle(pipe_handle.0) };

        match handle_client_stream(&mut pipe_file, max_payload_size) {
            Ok(Some(req)) => eprintln!(
                "  [IPC] Handled request: op='{}', {} file(s)",
                req.operation,
                req.files.len()
            ),
            Ok(None) => eprintln!("  [IPC] Client disconnected"),
            Err(e) => eprintln!("  [IPC] Protocol error: {e}"),
        }

        unsafe {
            let _ = DisconnectNamedPipe(pipe_handle);
        }
        // pipe_file drops here and closes the handle automatically.
    }
}
