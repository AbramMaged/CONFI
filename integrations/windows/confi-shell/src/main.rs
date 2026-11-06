// CONFI — Windows Shell Integration Helper & Prototype
//
// PURPOSE: Validates Windows Explorer context-menu selection delivery and IPC.
//
// Features:
//   - OS-native command-line parsing via `std::env::args_os()` to preserve Unicode paths.
//   - Robust length-prefixed IPC over Windows named pipes.
//   - Bounded retries via `WaitNamedPipeW` for busy instances.
//   - Clean fallback to RFC 3986 percent-encoded deep-link URI when CONFI is closed.
//   - Separation of IPC transport from application file processing.

use std::ffi::OsString;
use std::path::PathBuf;

use confi_shell::client::{get_pipe_name, send_pipe_request, IpcClientError};
use confi_shell::protocol::{
    validate_request, ConfiRequest, ConfiResponse, DEFAULT_MAX_PAYLOAD_SIZE,
};
use confi_shell::server::run_pipe_server;
use confi_shell::uri::build_deep_link;

fn main() {
    let args: Vec<OsString> = std::env::args_os().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let first_arg = args[1].to_string_lossy();
    match first_arg.as_ref() {
        "--listen" => {
            #[cfg(windows)]
            {
                let pipe_name = get_pipe_name();
                if let Err(e) = run_pipe_server(&pipe_name, DEFAULT_MAX_PAYLOAD_SIZE) {
                    eprintln!("Pipe server error: {e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(windows))]
            {
                eprintln!("Named pipe server is only available on Windows.");
                std::process::exit(1);
            }
        }
        "--action" => {
            if args.len() < 4 {
                eprintln!(
                    "Error: --action requires an action name and --files with at least one path."
                );
                print_usage();
                std::process::exit(1);
            }
            let action = args[2].to_string_lossy().to_string();
            let files = parse_files(&args[3..]);
            handle_action(&action, files);
        }
        "--register" => {
            eprintln!("[PROTOTYPE] Registry registration placeholder.");
            eprintln!("In production, this registers shell verbs in HKCU\\Software\\Classes\\.");
        }
        "--unregister" => {
            eprintln!("[PROTOTYPE] Registry cleanup placeholder.");
        }
        _ => {
            eprintln!("Unknown argument: {}", first_arg);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("CONFI Shell Integration Helper (Prototype)");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  confi-shell.exe --action <action> --files <path1> [<path2> ...]");
    eprintln!("  confi-shell.exe --listen           Start named pipe listener");
    eprintln!("  confi-shell.exe --register          Register shell verbs (placeholder)");
    eprintln!("  confi-shell.exe --unregister        Remove shell verbs (placeholder)");
    eprintln!();
    eprintln!(
        "Actions: open, compress, convert, split, merge, resize, batch_convert, batch_compress"
    );
}

fn parse_files(args: &[OsString]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut in_files = false;

    for arg in args {
        let lossy = arg.to_string_lossy();
        if lossy == "--files" {
            in_files = true;
            continue;
        }
        if lossy.starts_with("--") {
            in_files = false;
            continue;
        }
        if in_files {
            files.push(PathBuf::from(arg));
        }
    }

    files
}

fn handle_action(action: &str, files: Vec<PathBuf>) {
    eprintln!("═══════════════════════════════════════════════════════════════");
    eprintln!("  CONFI Shell Helper — Received Request");
    eprintln!("═══════════════════════════════════════════════════════════════");
    eprintln!("  Action:     {action}");
    eprintln!("  File count: {}", files.len());
    eprintln!();

    let mut valid_files = Vec::new();
    let mut errors = Vec::new();

    for (i, path) in files.iter().enumerate() {
        let display = path.display().to_string();
        let exists = path.exists();
        let is_file = path.is_file();

        eprintln!("  File {}: {}", i + 1, display);
        eprintln!("    exists:  {exists}");
        eprintln!("    is_file: {is_file}");

        if !exists {
            errors.push(format!("File not found: {display}"));
        } else if !is_file {
            errors.push(format!("Not a regular file: {display}"));
        } else {
            valid_files.push(path.clone());
        }
    }

    eprintln!();
    eprintln!("  Valid files:  {}", valid_files.len());
    eprintln!("  Errors:       {}", errors.len());

    for err in &errors {
        eprintln!("    ✗ {err}");
    }

    if valid_files.is_empty() {
        eprintln!();
        eprintln!("  No valid files to process. Exiting.");
        std::process::exit(1);
    }

    let request = ConfiRequest {
        operation: action.to_string(),
        files: valid_files,
    };

    if let Err(e) = validate_request(&request) {
        eprintln!("  Request validation error: {e}");
        std::process::exit(1);
    }

    let json = serde_json::to_string_pretty(&request).unwrap();
    eprintln!();
    eprintln!("  ConfiRequest JSON:");
    eprintln!("{json}");
    eprintln!();

    let pipe_name = get_pipe_name();
    match send_pipe_request(&pipe_name, &request, 3, 500) {
        Ok(ConfiResponse::Ack {
            operation,
            accepted_files,
            message,
        }) => {
            eprintln!("  ✓ Request accepted by running CONFI instance via named pipe!");
            eprintln!("    Operation:      {operation}");
            eprintln!("    Files accepted: {accepted_files}");
            eprintln!("    Message:        {message}");
        }
        Ok(ConfiResponse::Error { code, message }) => {
            eprintln!("  ✗ CONFI instance rejected request:");
            eprintln!("    Code:    {code}");
            eprintln!("    Message: {message}");
            std::process::exit(1);
        }
        Err(IpcClientError::ServerUnavailable(_)) => {
            eprintln!("  ⓘ CONFI is not running. Launching via deep-link URI...");
            let uri = build_deep_link(action, &request.files);
            eprintln!("  Deep-link URI: {uri}");
            // In production M7: ShellExecuteExW(uri)
        }
        Err(e) => {
            eprintln!("  ✗ IPC transmission error: {e}");
            std::process::exit(1);
        }
    }

    eprintln!("═══════════════════════════════════════════════════════════════");
    println!("{json}");
}
