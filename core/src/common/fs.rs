// CONFI — Safe output writing
//
// Every engine operation follows the same rule (AGENTS.md §3): write to a
// temporary file beside the destination, validate it, then rename into place.
// Rename is atomic on the same volume, so a crash or a failed validation never
// leaves a half-written file at the path the user chose.
//
// All engines route through these three helpers so the rule is enforced in one
// place instead of being re-implemented per operation.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::common::errors::{ConfiError, ConfiResult};

/// Build a temp path beside `output`.
///
/// The name carries a random component so two jobs writing to the same
/// destination cannot overwrite each other's temporary file.
pub fn temp_path_for(output: &Path) -> PathBuf {
    let stem = output.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let ext = output.extension().and_then(|e| e.to_str()).unwrap_or("tmp");
    output.with_file_name(format!(".confi-tmp-{}-{stem}.{ext}", Uuid::new_v4()))
}

/// Refuse to write over an existing file. Callers pass `overwrite_existing`
/// down from the platform layer once it supports the option.
pub fn require_absent(output: &Path) -> ConfiResult<()> {
    if output.exists() {
        return Err(ConfiError::output_error(format!(
            "Output file already exists: {}. Choose a different name.",
            output.display()
        )));
    }
    Ok(())
}

/// Move a validated temp file to its final destination, discarding it on failure.
pub fn finalize(tmp: &Path, output: &Path) -> ConfiResult<()> {
    std::fs::rename(tmp, output).map_err(|e| {
        let _ = std::fs::remove_file(tmp);
        ConfiError::output_error(format!(
            "Failed to finalize output '{}': {e}",
            output.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_paths_differ_for_same_output() {
        let output = Path::new("C:\\out\\report.pdf");
        let a = temp_path_for(output);
        let b = temp_path_for(output);
        assert_ne!(a, b, "concurrent jobs must not share a temp file");
        assert_eq!(a.parent(), output.parent());
        assert!(a.to_string_lossy().ends_with("report.pdf"));
    }

    #[test]
    fn require_absent_rejects_existing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("out.pdf");
        assert!(require_absent(&path).is_ok());
        std::fs::write(&path, b"x").unwrap();
        let err = require_absent(&path).unwrap_err();
        assert_eq!(err.code, crate::common::errors::ErrorCode::OutputError);
    }
}
