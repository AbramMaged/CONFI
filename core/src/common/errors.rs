// CONFI — Error types
//
// All errors use structured codes so the platform layer (Tauri, UniFFI) can
// translate them to user-readable messages without string matching.
//
// Rule: never expose `detail` to the user in production builds. It is for
// internal diagnostics and logging only.

use thiserror::Error;

/// Canonical error codes used across the entire CONFI engine.
/// These map 1:1 to the codes defined in API.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    /// Input file is missing, unreadable, empty, or the wrong type.
    InvalidInput,
    /// Format was detected but is not supported in this release.
    UnsupportedFormat,
    /// Operation is logically incompatible with the input type (e.g., PDF
    /// rotate applied to a JPEG).
    InvalidOperation,
    /// Insufficient filesystem or content-URI permissions.
    FileAccessDenied,
    /// Could not write output to the specified destination.
    OutputError,
    /// Input document is structurally invalid (truncated, corrupt headers, etc.)
    CorruptedFile,
    /// Input PDF is encrypted and requires a password before processing.
    PasswordRequired,
    /// Not enough storage to complete the operation.
    InsufficientSpace,
    /// User-initiated cancellation.
    OperationCancelled,
    /// Unexpected internal failure (library error, OOM, etc.)
    ProcessingError,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ErrorCode::InvalidInput => "INVALID_INPUT",
            ErrorCode::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            ErrorCode::InvalidOperation => "INVALID_OPERATION",
            ErrorCode::FileAccessDenied => "FILE_ACCESS_DENIED",
            ErrorCode::OutputError => "OUTPUT_ERROR",
            ErrorCode::CorruptedFile => "CORRUPTED_FILE",
            ErrorCode::PasswordRequired => "PASSWORD_REQUIRED",
            ErrorCode::InsufficientSpace => "INSUFFICIENT_SPACE",
            ErrorCode::OperationCancelled => "OPERATION_CANCELLED",
            ErrorCode::ProcessingError => "PROCESSING_ERROR",
        };
        write!(f, "{s}")
    }
}

/// The primary error type returned by all CONFI engine functions.
///
/// `message` is intended for user display (translated by the platform layer).
/// `detail` is for internal diagnostics only — never shown to users in production.
#[derive(Debug, Error, Clone)]
#[error("{code}: {message}")]
pub struct ConfiError {
    pub code: ErrorCode,
    /// Human-readable message suitable for display after platform-layer i18n.
    pub message: String,
    /// Internal diagnostic detail. Must not be exposed to users in production.
    #[source]
    pub detail: Option<DetailError>,
}

/// Wraps a diagnostic string as a `std::error::Error` so `thiserror`'s
/// `#[source]` attribute can chain it.
#[derive(Debug, Clone, Error)]
#[error("{0}")]
pub struct DetailError(pub String);

impl ConfiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(
        code: ErrorCode,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: Some(DetailError(detail.into())),
        }
    }

    // ── Convenience constructors ─────────────────────────────────────────────

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn unsupported_format(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnsupportedFormat, message)
    }

    pub fn invalid_operation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidOperation, message)
    }

    pub fn file_access_denied(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::FileAccessDenied, message)
    }

    pub fn output_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::OutputError, message)
    }

    pub fn corrupted_file(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::CorruptedFile, message)
    }

    pub fn password_required() -> Self {
        Self::new(
            ErrorCode::PasswordRequired,
            "This PDF is password-protected. Remove the password before processing.",
        )
    }

    pub fn insufficient_space() -> Self {
        Self::new(
            ErrorCode::InsufficientSpace,
            "Not enough storage space to complete this operation.",
        )
    }

    pub fn processing_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ProcessingError, message)
    }

    pub fn processing_error_detail(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::with_detail(ErrorCode::ProcessingError, message, detail)
    }
}

/// Shorthand result type used throughout the engine.
pub type ConfiResult<T> = Result<T, ConfiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_display() {
        assert_eq!(ErrorCode::InvalidInput.to_string(), "INVALID_INPUT");
        assert_eq!(
            ErrorCode::UnsupportedFormat.to_string(),
            "UNSUPPORTED_FORMAT"
        );
        assert_eq!(ErrorCode::CorruptedFile.to_string(), "CORRUPTED_FILE");
        assert_eq!(ErrorCode::PasswordRequired.to_string(), "PASSWORD_REQUIRED");
        assert_eq!(ErrorCode::ProcessingError.to_string(), "PROCESSING_ERROR");
    }

    #[test]
    fn error_has_code_and_message() {
        let e = ConfiError::invalid_input("File not found.");
        assert_eq!(e.code, ErrorCode::InvalidInput);
        assert_eq!(e.message, "File not found.");
        assert!(e.detail.is_none());
    }

    #[test]
    fn error_with_detail_does_not_expose_detail_in_display() {
        let e = ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This PDF is damaged and cannot be opened.",
            "EOF at offset 1024; expected cross-reference table",
        );
        // The user-visible Display should contain code + message, not the detail.
        let display = e.to_string();
        assert!(display.contains("CORRUPTED_FILE"));
        assert!(display.contains("This PDF is damaged"));
        // Detail is available internally for diagnostics.
        assert!(e.detail.is_some());
    }
}
