// CONFI — Core data models
//
// Shared across all engine modules and both platform layers (Tauri IPC / UniFFI).
// All types derive Serialize/Deserialize so they can cross the IPC boundary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Detected file format ────────────────────────────────────────────────────

/// Formats that CONFI Core can detect and/or process.
/// Detection is always from magic bytes, never from file extension alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Pdf,
    Jpeg,
    Png,
    Webp,
    Bmp,
    /// Detected but not supported in this release.
    Unsupported(String),
    /// Could not determine format (truncated, empty, random bytes).
    Unknown,
}

impl std::fmt::Display for FileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileFormat::Pdf => write!(f, "pdf"),
            FileFormat::Jpeg => write!(f, "jpeg"),
            FileFormat::Png => write!(f, "png"),
            FileFormat::Webp => write!(f, "webp"),
            FileFormat::Bmp => write!(f, "bmp"),
            FileFormat::Unsupported(s) => write!(f, "unsupported({s})"),
            FileFormat::Unknown => write!(f, "unknown"),
        }
    }
}

impl FileFormat {
    /// Returns the canonical lowercase file extension for this format.
    pub fn extension(&self) -> Option<&'static str> {
        match self {
            FileFormat::Pdf => Some("pdf"),
            FileFormat::Jpeg => Some("jpg"),
            FileFormat::Png => Some("png"),
            FileFormat::Webp => Some("webp"),
            FileFormat::Bmp => Some("bmp"),
            _ => None,
        }
    }

    /// Returns the MIME type for this format.
    pub fn mime_type(&self) -> Option<&'static str> {
        match self {
            FileFormat::Pdf => Some("application/pdf"),
            FileFormat::Jpeg => Some("image/jpeg"),
            FileFormat::Png => Some("image/png"),
            FileFormat::Webp => Some("image/webp"),
            FileFormat::Bmp => Some("image/bmp"),
            _ => None,
        }
    }

    /// Whether this format is supported for processing in CONFI Core.
    pub fn is_supported(&self) -> bool {
        matches!(
            self,
            FileFormat::Pdf
                | FileFormat::Jpeg
                | FileFormat::Png
                | FileFormat::Webp
                | FileFormat::Bmp
        )
    }
}

// ── File information ────────────────────────────────────────────────────────

/// Metadata about a file as reported by the file engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub file_id: String,
    pub name: String,
    pub detected_format: FileFormat,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub is_valid: bool,
    /// Validation failure reason. None if `is_valid` is true.
    pub validation_error: Option<String>,
}

impl FileInfo {
    pub fn new(name: impl Into<String>, detected_format: FileFormat, size_bytes: u64) -> Self {
        let mime = detected_format.mime_type().map(str::to_owned);
        Self {
            file_id: Uuid::new_v4().to_string(),
            name: name.into(),
            detected_format,
            mime_type: mime,
            size_bytes,
            is_valid: true,
            validation_error: None,
        }
    }

    pub fn invalid(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            file_id: Uuid::new_v4().to_string(),
            name: name.into(),
            detected_format: FileFormat::Unknown,
            mime_type: None,
            size_bytes: 0,
            is_valid: false,
            validation_error: Some(reason.into()),
        }
    }
}

// ── Job status and lifecycle ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Queued,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            JobStatus::Pending => "pending",
            JobStatus::Queued => "queued",
            JobStatus::Processing => "processing",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        };
        write!(f, "{s}")
    }
}

/// Operation types that can be submitted as a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    MergePdf,
    SplitPdf,
    RotatePdf,
    OrganizePdf,
    CompressPdf,
    PdfToImages,
    ImagesToPdf,
    ConvertImage,
    CompressImage,
    ResizeImage,
}

// ── Progress reporting ──────────────────────────────────────────────────────

/// Progress update emitted during a running job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub job_id: String,
    /// 0–100 when deterministic. Ignored when `is_indeterminate` is true.
    pub progress_percentage: u8,
    /// Human-readable description of what is currently happening.
    pub current_operation: String,
    /// When true, the UI must show a spinner rather than a progress bar.
    /// Never fake progress — if the operation cannot measure it, set this true.
    pub is_indeterminate: bool,
}

// ── Operation result ────────────────────────────────────────────────────────

/// Output file information included in a completed job result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFileInfo {
    pub name: String,
    pub size_bytes: u64,
}

/// The result returned when a job finishes (success or failure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub job_id: String,
    pub status: JobStatus,
    /// Output files produced. Empty on failure.
    pub outputs: Vec<OutputFileInfo>,
    /// Original total input size (bytes).
    pub original_size_bytes: u64,
    /// Size reduction percentage (positive = smaller). None when not applicable.
    pub reduction_percentage: Option<f64>,
    /// Error detail when `status` is Failed.
    pub error: Option<OperationError>,
}

/// Serializable error summary (the full `ConfiError` is for internal use;
/// this subset is safe to cross the IPC boundary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationError {
    pub code: String,
    pub message: String,
}

impl OperationResult {
    pub fn success(
        job_id: impl Into<String>,
        outputs: Vec<OutputFileInfo>,
        original_size_bytes: u64,
    ) -> Self {
        let total_output: u64 = outputs.iter().map(|o| o.size_bytes).sum();
        let reduction = if original_size_bytes > 0 && total_output < original_size_bytes {
            Some((original_size_bytes - total_output) as f64 / original_size_bytes as f64 * 100.0)
        } else {
            None
        };
        Self {
            job_id: job_id.into(),
            status: JobStatus::Completed,
            outputs,
            original_size_bytes,
            reduction_percentage: reduction,
            error: None,
        }
    }

    pub fn failure(
        job_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            status: JobStatus::Failed,
            outputs: vec![],
            original_size_bytes: 0,
            reduction_percentage: None,
            error: Some(OperationError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

// ── Unified request format ──────────────────────────────────────────────────

/// The single request type accepted by all CONFI entry points.
///
/// Whether the request originates from the app UI, a shell verb, or a COM
/// extension — it is always normalized to this format before dispatch.
/// This is the `ConfiRequest` described in ARCHITECTURE.md.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfiRequest {
    pub operation: OperationType,
    pub files: Vec<String>,
    #[serde(default)]
    pub options: serde_json::Value,
}

// ── Image-specific types ────────────────────────────────────────────────────

/// Supported output image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageOutputFormat {
    Jpeg,
    Png,
    Webp,
    Bmp,
}

impl ImageOutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageOutputFormat::Jpeg => "jpg",
            ImageOutputFormat::Png => "png",
            ImageOutputFormat::Webp => "webp",
            ImageOutputFormat::Bmp => "bmp",
        }
    }
}

/// Resize dimensions — either explicit pixels or a percentage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResizeDimensions {
    /// Target width and/or height in pixels. A None value means "unconstrained".
    Pixels {
        width: Option<u32>,
        height: Option<u32>,
    },
    /// Scale by percentage (e.g., 50 = 50% of original size).
    Percentage { percentage: u32 },
}

/// Job timestamps for persistence and history display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobTimestamps {
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_format_extension_and_mime() {
        assert_eq!(FileFormat::Jpeg.extension(), Some("jpg"));
        assert_eq!(FileFormat::Pdf.mime_type(), Some("application/pdf"));
        assert_eq!(FileFormat::Unknown.extension(), None);
        assert_eq!(FileFormat::Unknown.mime_type(), None);
    }

    #[test]
    fn file_format_support() {
        assert!(FileFormat::Pdf.is_supported());
        assert!(FileFormat::Jpeg.is_supported());
        assert!(FileFormat::Png.is_supported());
        assert!(FileFormat::Webp.is_supported());
        assert!(FileFormat::Bmp.is_supported());
        assert!(!FileFormat::Unknown.is_supported());
        assert!(!FileFormat::Unsupported("tiff".to_string()).is_supported());
    }

    #[test]
    fn operation_result_reduction_calculated() {
        let result = OperationResult::success(
            "job-1",
            vec![OutputFileInfo {
                name: "out.pdf".to_string(),
                size_bytes: 800,
            }],
            1000,
        );
        let reduction = result.reduction_percentage.unwrap();
        assert!((reduction - 20.0).abs() < 0.001);
    }

    #[test]
    fn operation_result_no_reduction_when_output_larger() {
        let result = OperationResult::success(
            "job-2",
            vec![OutputFileInfo {
                name: "out.pdf".to_string(),
                size_bytes: 1200,
            }],
            1000,
        );
        // Output larger than input — reduction_percentage must be None.
        assert!(result.reduction_percentage.is_none());
    }

    #[test]
    fn confi_request_roundtrips_json() {
        let req = ConfiRequest {
            operation: OperationType::MergePdf,
            files: vec!["C:\\a.pdf".to_string(), "C:\\b.pdf".to_string()],
            options: serde_json::Value::Null,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: ConfiRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.operation, OperationType::MergePdf);
        assert_eq!(decoded.files.len(), 2);
    }
}
