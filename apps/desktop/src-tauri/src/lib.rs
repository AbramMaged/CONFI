// CONFI — desktop host
//
// Every command here is a thin adapter: it takes paths and primitives from the
// UI, calls the shared core engine, and turns `ConfiError` into something the
// frontend can display. No file processing logic lives in this crate.
//
// Blocking work runs on `spawn_blocking` so a long operation never freezes the
// window.

use std::path::Path;

use serde::Serialize;

use confi_core::common::errors::{ConfiError, ConfiResult};
use confi_core::common::models::{FileFormat, ImageOutputFormat, ResizeDimensions};
use confi_core::file_engine::{detection, validation};
use confi_core::image_engine::{compression, conversion, resize};
use confi_core::pdf_engine;

// ── Wire types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ErrorDto {
    pub code: String,
    pub message: String,
}

impl From<ConfiError> for ErrorDto {
    fn from(error: ConfiError) -> Self {
        Self {
            code: error.code.to_string(),
            message: error.message,
        }
    }
}

type CommandResult<T> = Result<T, ErrorDto>;

#[derive(Debug, Serialize)]
pub struct FileRow {
    pub path: String,
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
    pub is_valid: bool,
    /// Page count for PDFs, `width × height` for images.
    pub detail: Option<String>,
    pub error: Option<ErrorDto>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Run a blocking engine call off the UI thread.
async fn blocking<T, F>(job: F) -> CommandResult<T>
where
    F: FnOnce() -> ConfiResult<T> + Send + 'static,
    T: Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(job).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(ErrorDto::from(error)),
        Err(join_error) => Err(ErrorDto {
            code: "PROCESSING_ERROR".to_string(),
            message: format!("The operation stopped unexpectedly: {join_error}"),
        }),
    }
}

fn image_format_from(name: &str) -> ConfiResult<ImageOutputFormat> {
    match name {
        "jpeg" | "jpg" => Ok(ImageOutputFormat::Jpeg),
        "png" => Ok(ImageOutputFormat::Png),
        "webp" => Ok(ImageOutputFormat::Webp),
        "bmp" => Ok(ImageOutputFormat::Bmp),
        other => Err(ConfiError::invalid_input(format!(
            "Unknown image format '{other}'."
        ))),
    }
}

fn detect_one(raw_path: &str) -> FileRow {
    let path = Path::new(raw_path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| raw_path.to_string());

    let format = match detection::detect_from_path(path) {
        Ok(format) => format,
        Err(error) => {
            return FileRow {
                path: raw_path.to_string(),
                name,
                format: "unknown".to_string(),
                size_bytes: 0,
                is_valid: false,
                detail: None,
                error: Some(error.into()),
            };
        }
    };

    match validation::validate_file(path, &format) {
        Ok(validated) => {
            let detail = if format == FileFormat::Pdf {
                pdf_engine::page_count(path)
                    .ok()
                    .map(|pages| format!("{pages} page(s)"))
            } else {
                validated
                    .dimensions
                    .map(|(width, height)| format!("{width} × {height} px"))
            };

            FileRow {
                path: raw_path.to_string(),
                name,
                format: format.to_string(),
                size_bytes: validated.size_bytes,
                is_valid: true,
                detail,
                error: None,
            }
        }
        Err(error) => FileRow {
            path: raw_path.to_string(),
            name,
            format: format.to_string(),
            size_bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            is_valid: false,
            detail: None,
            error: Some(error.into()),
        },
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
async fn detect_files(paths: Vec<String>) -> Vec<FileRow> {
    tauri::async_runtime::spawn_blocking(move || {
        paths.iter().map(|p| detect_one(p)).collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn convert_image(
    input: String,
    format: String,
    quality: Option<u8>,
    output: String,
) -> CommandResult<String> {
    blocking(move || {
        let output_format = image_format_from(&format)?;
        let result = conversion::convert_image(
            Path::new(&input),
            output_format,
            quality,
            Path::new(&output),
        )?;

        let mut summary = format!(
            "Converted to {format}: {} bytes",
            result.output_size_bytes
        );
        for warning in &result.warnings {
            summary.push_str(&format!("\n{warning}"));
        }
        Ok(summary)
    })
    .await
}

#[tauri::command]
async fn compress_image(
    input: String,
    quality: Option<u8>,
    output: String,
) -> CommandResult<String> {
    blocking(move || {
        let format = detection::detect_from_path(Path::new(&input))?;
        let result = compression::compress_image(
            Path::new(&input),
            &format,
            quality,
            Path::new(&output),
        )?;

        if result.size_increased {
            return Err(ConfiError::with_detail(
                confi_core::common::errors::ErrorCode::OutputError,
                result
                    .warnings
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "The file could not be made smaller.".to_string()),
                "size_increased",
            ));
        }

        let saved = result
            .input_size_bytes
            .saturating_sub(result.output_size_bytes);
        Ok(format!(
            "Compressed: {} → {} bytes (saved {saved} bytes)",
            result.input_size_bytes, result.output_size_bytes
        ))
    })
    .await
}

#[tauri::command]
async fn resize_image(
    input: String,
    mode: String,
    width: Option<u32>,
    height: Option<u32>,
    percentage: Option<u32>,
    output: String,
) -> CommandResult<String> {
    blocking(move || {
        let dimensions = match mode.as_str() {
            "percentage" => ResizeDimensions::Percentage {
                percentage: percentage.ok_or_else(|| {
                    ConfiError::invalid_input("A percentage is required for this resize mode.")
                })?,
            },
            "pixels" => ResizeDimensions::Pixels { width, height },
            other => {
                return Err(ConfiError::invalid_input(format!(
                    "Unknown resize mode '{other}'."
                )));
            }
        };

        let result = resize::resize_image(Path::new(&input), dimensions, Path::new(&output))?;
        Ok(format!(
            "Resized {}×{} → {}×{} ({} bytes)",
            result.original_dimensions.0,
            result.original_dimensions.1,
            result.output_dimensions.0,
            result.output_dimensions.1,
            result.output_size_bytes
        ))
    })
    .await
}

#[tauri::command]
async fn merge_pdfs(inputs: Vec<String>, output: String) -> CommandResult<String> {
    blocking(move || {
        let paths: Vec<std::path::PathBuf> = inputs.iter().map(std::path::PathBuf::from).collect();
        let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        let result = pdf_engine::merge_pdfs(&refs, Path::new(&output))?;
        Ok(format!(
            "Merged {} files into {} page(s), {} bytes",
            inputs.len(),
            result.page_count,
            result.size_bytes
        ))
    })
    .await
}

#[tauri::command]
async fn split_pdf(input: String, dir: String) -> CommandResult<String> {
    blocking(move || {
        let results = pdf_engine::split_to_pages(Path::new(&input), Path::new(&dir))?;
        Ok(format!(
            "Split into {} file(s) in {dir}",
            results.len()
        ))
    })
    .await
}

#[tauri::command]
async fn rotate_pdf(input: String, degrees: u16, output: String) -> CommandResult<String> {
    blocking(move || {
        let pages = pdf_engine::page_count(Path::new(&input))?;
        let selection: Vec<u32> = (1..=pages).collect();
        let result = pdf_engine::rotate_pdf(Path::new(&input), &selection, degrees, Path::new(&output))?;
        Ok(format!(
            "Rotated {} page(s) by {degrees}°, {} bytes",
            result.page_count, result.size_bytes
        ))
    })
    .await
}

// ── Entry point ─────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            detect_files,
            convert_image,
            compress_image,
            resize_image,
            merge_pdfs,
            split_pdf,
            rotate_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running the CONFI application");
}

// ── Smoke tests for the command adapter ─────────────────────────────────────
//
// The engine itself is covered by the core test suite; these check the adapter
// between Tauri and the engine: argument handling, the blocking bridge, and the
// error shape the frontend receives.

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("confi-cmd-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_files_reports_valid_pdf_and_flags_junk() {
        let dir = scratch_dir("detect");

        // Large enough to pass the PDF structural checks (>100 bytes, %%EOF).
        let good = dir.join("document.pdf");
        let mut bytes = b"%PDF-1.7\n1 0 obj\n<< >>\nendobj\n".to_vec();
        bytes.extend(std::iter::repeat_n(b' ', 88));
        bytes.extend_from_slice(b"\n%%EOF\n");
        std::fs::write(&good, &bytes).unwrap();

        let bad = dir.join("junk.pdf");
        std::fs::write(&bad, b"this is not a pdf").unwrap();

        let rows = tauri::async_runtime::block_on(detect_files(vec![
            good.display().to_string(),
            bad.display().to_string(),
        ]));

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].format, "pdf");
        assert!(rows[0].is_valid, "valid PDF should be accepted: {:?}", rows[0].error);
        assert_eq!(rows[0].name, "document.pdf");
        assert!(!rows[1].is_valid, "junk bytes must not be accepted");
        assert!(rows[1].error.is_some());
    }

    #[test]
    fn engine_errors_surface_with_a_code() {
        let dir = scratch_dir("errors");
        let missing = dir.join("does-not-exist.pdf");

        let result = tauri::async_runtime::block_on(merge_pdfs(
            vec![missing.display().to_string(), missing.display().to_string()],
            dir.join("out.pdf").display().to_string(),
        ));

        let error = result.expect_err("merging missing files must fail");
        assert_eq!(error.code, "INVALID_INPUT");
        assert!(!error.message.is_empty());
    }
}

