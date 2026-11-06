// CONFI — Core data models
//
// Shared across all engine modules and both platform layers (Tauri IPC / UniFFI).
// Everything here is used by an engine operation today. Job, Progress and
// OperationResult types are deliberately absent: they get defined in
// `job_manager` together with the first consumer, so they cannot drift from it.

use serde::{Deserialize, Serialize};

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

/// Resize dimensions — either explicit pixels or a percentage.
///
/// Deserialization is hand-written (see `RawResizeDimensions`) because the
/// obvious `#[serde(untagged)]` enum silently mis-reads valid input: with an
/// all-optional `Pixels` variant, `{"percentage": 50}` parses as
/// `{width: None, height: None}` and the resize becomes a no-op.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ResizeDimensions {
    /// Scale by percentage (e.g., 50 = 50% of original size).
    Percentage { percentage: u32 },
    /// Target width and/or height in pixels. A None value means "unconstrained".
    Pixels {
        width: Option<u32>,
        height: Option<u32>,
    },
}

/// Wire shape used to inspect a resize request before it becomes a variant.
///
/// `deny_unknown_fields` plus the explicit match below are what stop four
/// silent failures: unknown keys being ignored, a percentage request being read
/// as "no constraint", both forms supplied with one silently winning, and an
/// empty object meaning "resize to the original size".
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResizeDimensions {
    width: Option<u32>,
    height: Option<u32>,
    percentage: Option<u32>,
}

impl<'de> Deserialize<'de> for ResizeDimensions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawResizeDimensions::deserialize(deserializer)?;
        let has_pixels = raw.width.is_some() || raw.height.is_some();

        match (raw.percentage, has_pixels) {
            (Some(_), true) => Err(serde::de::Error::custom(
                "specify either 'percentage' or 'width'/'height', not both",
            )),
            (None, false) => Err(serde::de::Error::custom(
                "specify 'percentage' or at least one of 'width'/'height'",
            )),
            (Some(percentage), false) => Ok(ResizeDimensions::Percentage { percentage }),
            (None, true) => Ok(ResizeDimensions::Pixels {
                width: raw.width,
                height: raw.height,
            }),
        }
    }
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
    fn resize_dimensions_accept_both_shapes() {
        // API.md shape 1: explicit pixels, either or both.
        let pixels: ResizeDimensions =
            serde_json::from_str(r#"{"width": 800, "height": 600}"#).unwrap();
        assert!(matches!(
            pixels,
            ResizeDimensions::Pixels {
                width: Some(800),
                height: Some(600)
            }
        ));

        let width_only: ResizeDimensions = serde_json::from_str(r#"{"width": 800}"#).unwrap();
        assert!(matches!(
            width_only,
            ResizeDimensions::Pixels {
                width: Some(800),
                height: None
            }
        ));

        let height_only: ResizeDimensions = serde_json::from_str(r#"{"height": 600}"#).unwrap();
        assert!(matches!(
            height_only,
            ResizeDimensions::Pixels {
                width: None,
                height: Some(600)
            }
        ));

        // API.md shape 2: percentage. Must not be swallowed by the Pixels variant.
        let percent: ResizeDimensions = serde_json::from_str(r#"{"percentage": 50}"#).unwrap();
        assert!(matches!(
            percent,
            ResizeDimensions::Percentage { percentage: 50 }
        ));
    }

    #[test]
    fn resize_dimensions_reject_invalid_input() {
        // Negative: rejected by the u32 type itself.
        assert!(serde_json::from_str::<ResizeDimensions>(r#"{"percentage": -50}"#).is_err());
        assert!(serde_json::from_str::<ResizeDimensions>(r#"{"width": -800}"#).is_err());

        // Unknown field: must not be silently dropped.
        assert!(
            serde_json::from_str::<ResizeDimensions>(r#"{"width": 800, "heigth": 600}"#).is_err()
        );

        // Both forms at once: ambiguous, one would otherwise win silently.
        assert!(serde_json::from_str::<ResizeDimensions>(
            r#"{"percentage": 50, "width": 800}"#
        )
        .is_err());

        // Nothing at all: would otherwise mean "resize to the original size".
        assert!(serde_json::from_str::<ResizeDimensions>("{}").is_err());
        assert!(serde_json::from_str::<ResizeDimensions>(r#"{"quality": 80}"#).is_err());
    }
}
