// CONFI — Shell Integration IPC Tests
//
// Validates:
//   - Length-prefixed framing (< 8192 and > 8192 bytes)
//   - Chunked / partial read handling
//   - Oversized payload rejection (> 1 MiB)
//   - Malformed JSON handling
//   - Unicode paths (Arabic, Chinese, emojis, reserved characters)
//   - Bounded retries and unavailable server detection
//   - Broken pipe handling during transmission

use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

use confi_shell::client::{send_pipe_request, IpcClientError};
use confi_shell::protocol::{
    read_framed_message, write_framed_message, ConfiRequest, ConfiResponse, IpcError,
    DEFAULT_MAX_PAYLOAD_SIZE,
};
use confi_shell::server::handle_client_stream;
use confi_shell::uri::build_deep_link;

// ── Mock Stream for testing chunked / partial reads ──────────────────────────

struct ChunkedStream {
    data: Vec<u8>,
    read_pos: usize,
    chunk_size: usize,
    written: Vec<u8>,
}

impl ChunkedStream {
    fn new(data: Vec<u8>, chunk_size: usize) -> Self {
        Self {
            data,
            read_pos: 0,
            chunk_size,
            written: Vec::new(),
        }
    }
}

impl Read for ChunkedStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.read_pos >= self.data.len() {
            return Ok(0); // EOF
        }

        let remaining = self.data.len() - self.read_pos;
        let to_read = remaining.min(self.chunk_size).min(buf.len());
        buf[..to_read].copy_from_slice(&self.data[self.read_pos..self.read_pos + to_read]);
        self.read_pos += to_read;
        Ok(to_read)
    }
}

impl Write for ChunkedStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ── Test 1: Payload below 8192 bytes ─────────────────────────────────────────

#[test]
fn payload_below_8192_bytes_roundtrip() {
    let req = ConfiRequest {
        operation: "compress".to_string(),
        files: vec![
            PathBuf::from("C:\\Docs\\report1.pdf"),
            PathBuf::from("C:\\Docs\\report2.pdf"),
        ],
    };

    let json = serde_json::to_vec(&req).unwrap();
    assert!(json.len() < 8192);

    let mut buf = Vec::new();
    write_framed_message(&mut buf, &json, DEFAULT_MAX_PAYLOAD_SIZE).unwrap();

    let mut cursor = Cursor::new(buf);
    let decoded_bytes = read_framed_message(&mut cursor, DEFAULT_MAX_PAYLOAD_SIZE)
        .unwrap()
        .unwrap();

    let decoded_req: ConfiRequest = serde_json::from_slice(&decoded_bytes).unwrap();
    assert_eq!(decoded_req, req);
}

// ── Test 2: Payload above 8192 bytes (e.g. 100+ long paths) ──────────────────

#[test]
fn payload_above_8192_bytes_roundtrip() {
    // Generate many paths to exceed the previous 8192-byte limit
    let mut files = Vec::new();
    for i in 0..300 {
        files.push(PathBuf::from(format!(
            "C:\\Users\\CONFI_User\\Documents\\Very\\Long\\Path\\Directory\\Structure\\Batch_{}\\Document_Test_Case_{:04}.pdf",
            i / 10,
            i
        )));
    }

    let req = ConfiRequest {
        operation: "batch_compress".to_string(),
        files,
    };

    let json = serde_json::to_vec(&req).unwrap();
    assert!(
        json.len() > 8192,
        "Expected payload > 8192 bytes, got {}",
        json.len()
    );

    let mut buf = Vec::new();
    write_framed_message(&mut buf, &json, DEFAULT_MAX_PAYLOAD_SIZE).unwrap();

    let mut cursor = Cursor::new(buf);
    let decoded_bytes = read_framed_message(&mut cursor, DEFAULT_MAX_PAYLOAD_SIZE)
        .unwrap()
        .unwrap();

    let decoded_req: ConfiRequest = serde_json::from_slice(&decoded_bytes).unwrap();
    assert_eq!(decoded_req.files.len(), 300);
    assert_eq!(decoded_req.operation, "batch_compress");
}

// ── Test 3: Partial / Fragmented Reads ───────────────────────────────────────

#[test]
fn partial_reads_handled_correctly() {
    let req = ConfiRequest {
        operation: "convert".to_string(),
        files: vec![PathBuf::from("D:\\Images\\photo.jpg")],
    };

    let json = serde_json::to_vec(&req).unwrap();
    let mut framed = Vec::new();
    write_framed_message(&mut framed, &json, DEFAULT_MAX_PAYLOAD_SIZE).unwrap();

    // Stream delivering only 3 bytes at a time
    let mut chunked = ChunkedStream::new(framed, 3);
    let decoded_bytes = read_framed_message(&mut chunked, DEFAULT_MAX_PAYLOAD_SIZE)
        .unwrap()
        .unwrap();

    let decoded_req: ConfiRequest = serde_json::from_slice(&decoded_bytes).unwrap();
    assert_eq!(decoded_req, req);
}

// ── Test 4: Oversized payload rejection ───────────────────────────────────────

#[test]
fn oversized_payload_rejected_at_prefix() {
    let fake_len: u32 = (DEFAULT_MAX_PAYLOAD_SIZE + 1024) as u32;
    let mut stream = Cursor::new(fake_len.to_le_bytes());

    let err = read_framed_message(&mut stream, DEFAULT_MAX_PAYLOAD_SIZE).unwrap_err();
    match err {
        IpcError::PayloadTooLarge { length, max } => {
            assert_eq!(length, fake_len as usize);
            assert_eq!(max, DEFAULT_MAX_PAYLOAD_SIZE);
        }
        other => panic!("Expected PayloadTooLarge, got: {other:?}"),
    }
}

// ── Test 5: Malformed JSON handling ──────────────────────────────────────────

#[test]
fn malformed_json_returns_error_response() {
    let bad_json = b"{ not a valid json }";
    let mut framed = Vec::new();
    write_framed_message(&mut framed, bad_json, DEFAULT_MAX_PAYLOAD_SIZE).unwrap();

    let mut stream = Cursor::new(framed);
    let result = handle_client_stream(&mut stream, DEFAULT_MAX_PAYLOAD_SIZE);
    assert!(result.is_err());

    // Check that error response was written back to the stream
    let written = stream.into_inner();
    let mut resp_cursor = Cursor::new(&written[bad_json.len() + 4..]);
    if let Ok(Some(resp_bytes)) = read_framed_message(&mut resp_cursor, DEFAULT_MAX_PAYLOAD_SIZE) {
        let resp: ConfiResponse = serde_json::from_slice(&resp_bytes).unwrap();
        match resp {
            ConfiResponse::Error { code, .. } => assert_eq!(code, "INVALID_JSON"),
            ConfiResponse::Ack { .. } => panic!("Expected Error, got Ack"),
        }
    }
}

// ── Test 6: Unicode Paths & Reserved Characters ──────────────────────────────

#[test]
fn unicode_and_reserved_characters_end_to_end() {
    let req = ConfiRequest {
        operation: "merge".to_string(),
        files: vec![
            PathBuf::from("C:\\مجلد_العمل\\تقرير_الربع_الأول.pdf"), // Arabic
            PathBuf::from("D:\\项目文件\\设计图纸_2026.pdf"),      // Chinese
            PathBuf::from("E:\\Files\\Report #1 (100% & Final)?.pdf"), // Reserved % & # ?
            PathBuf::from("F:\\Path With Spaces\\file+name=value.pdf"), // Spaces, +, =
        ],
    };

    // 1. JSON protocol framing
    let mut framed = Vec::new();
    let json = serde_json::to_vec(&req).unwrap();
    write_framed_message(&mut framed, &json, DEFAULT_MAX_PAYLOAD_SIZE).unwrap();

    let mut stream = Cursor::new(framed);
    let decoded_bytes = read_framed_message(&mut stream, DEFAULT_MAX_PAYLOAD_SIZE)
        .unwrap()
        .unwrap();
    let decoded_req: ConfiRequest = serde_json::from_slice(&decoded_bytes).unwrap();
    assert_eq!(decoded_req, req);

    // 2. Deep-link URI percent encoding
    let uri = build_deep_link(&req.operation, &req.files);
    assert!(!uri.contains(' '));
    assert!(!uri.contains('م'));
    assert!(!uri.contains('项'));
    assert!(uri.starts_with("confi://action/merge?files="));
}

// ── Test 7: Multiple rapid requests sequentially ─────────────────────────────

#[test]
fn multiple_rapid_requests_handled() {
    for i in 0..50 {
        let req = ConfiRequest {
            operation: "split".to_string(),
            files: vec![PathBuf::from(format!("C:\\Docs\\file_{i}.pdf"))],
        };

        let json = serde_json::to_vec(&req).unwrap();
        let mut framed = Vec::new();
        write_framed_message(&mut framed, &json, DEFAULT_MAX_PAYLOAD_SIZE).unwrap();

        let mut stream = Cursor::new(framed);
        let accepted = handle_client_stream(&mut stream, DEFAULT_MAX_PAYLOAD_SIZE).unwrap();
        assert!(accepted.is_some());
        assert_eq!(accepted.unwrap().files[0], req.files[0]);
    }
}

// ── Test 8: Server unavailable ───────────────────────────────────────────────

#[test]
fn server_unavailable_distinct_error() {
    // Non-existent pipe name
    let non_existent_pipe = "\\\\.\\pipe\\confi-non-existent-pipe-test-99999";
    let req = ConfiRequest {
        operation: "open".to_string(),
        files: vec![PathBuf::from("C:\\test.pdf")],
    };

    let result = send_pipe_request(non_existent_pipe, &req, 0, 100);
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        IpcClientError::ServerUnavailable(_) => {}
        other => panic!("Expected ServerUnavailable, got: {other:?}"),
    }
}

// ── Test 9: Broken pipe during transmission ──────────────────────────────────

#[test]
fn broken_pipe_mid_header_returns_error() {
    // Only 2 bytes of the 4-byte header
    let truncated_header = vec![0x10, 0x00];
    let mut cursor = Cursor::new(truncated_header);

    let err = read_framed_message(&mut cursor, DEFAULT_MAX_PAYLOAD_SIZE).unwrap_err();
    match err {
        IpcError::IncompleteHeader { read } => assert_eq!(read, 2),
        other => panic!("Expected IncompleteHeader, got: {other:?}"),
    }
}

#[test]
fn broken_pipe_mid_payload_returns_error() {
    // 4-byte header indicating 100 bytes, but only 10 bytes present
    let len: u32 = 100;
    let mut data = len.to_le_bytes().to_vec();
    data.extend_from_slice(&[0xAA; 10]);

    let mut cursor = Cursor::new(data);
    let err = read_framed_message(&mut cursor, DEFAULT_MAX_PAYLOAD_SIZE).unwrap_err();
    match err {
        IpcError::IncompletePayload { expected, received } => {
            assert_eq!(expected, 100);
            assert_eq!(received, 10);
        }
        other => panic!("Expected IncompletePayload, got: {other:?}"),
    }
}
