// CONFI — Office document conversion
//
// Converts Word / PowerPoint / Excel documents to PDF through the installed
// Microsoft Office, using COM automation (Word/PowerPoint/Excel export a real
// PDF, so Arabic text, images and fonts come out faithfully).
//
// If Office is not installed the call fails with a clear UNSUPPORTED_FORMAT
// error rather than writing a partial file or silently degrading the document.
//
// A LibreOffice (`soffice --headless`) backend was prototyped and removed: it
// converts correctly from a shell, but when spawned by this process it dies with
// STATUS_STACK_BUFFER_OVERRUN (0xc0000409) for both a .docx and a .pptx. Adding
// it back requires solving that first; `detect_backend` would then prefer it.
//
// Deliberately NOT supported: PDF -> Office. Reconstructing layout from a PDF is
// reconstructing the document, which AGENTS.md §10 keeps out of scope.
//
// ponytail: COM is driven through a short PowerShell command rather than
// hand-written IDispatch bindings. It costs ~0.3 s of process startup and keeps
// this to ~40 lines of Rust; swap in native COM if start-up latency ever matters.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::common::errors::{ConfiError, ConfiResult, ErrorCode};
use crate::common::fs::{finalize, require_absent, temp_path_for};

/// Which Office application owns a document type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Word,
    PowerPoint,
    Excel,
}

impl DocumentKind {
    pub fn label(self) -> &'static str {
        match self {
            DocumentKind::Word => "word",
            DocumentKind::PowerPoint => "powerpoint",
            DocumentKind::Excel => "excel",
        }
    }
}

/// Extensions CONFI can convert to PDF.
const WORD_EXTENSIONS: &[&str] = &["doc", "docx", "rtf", "odt", "txt"];
const POWERPOINT_EXTENSIONS: &[&str] = &["ppt", "pptx", "odp"];
const EXCEL_EXTENSIONS: &[&str] = &["xls", "xlsx", "ods", "csv"];

/// Identify a convertible document by extension. Detection by extension is
/// acceptable here because the converter itself (Office/LibreOffice) validates
/// the real contents and fails loudly on a mismatch.
pub fn kind_of(path: &Path) -> Option<DocumentKind> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;

    match extension.as_str() {
        e if WORD_EXTENSIONS.contains(&e) => Some(DocumentKind::Word),
        e if POWERPOINT_EXTENSIONS.contains(&e) => Some(DocumentKind::PowerPoint),
        e if EXCEL_EXTENSIONS.contains(&e) => Some(DocumentKind::Excel),
        _ => None,
    }
}

/// Whether CONFI can turn this file into a PDF.
pub fn is_convertible(path: &Path) -> bool {
    kind_of(path).is_some()
}

/// Description of the conversion backend that would be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    MsOffice,
    None,
}

/// Result of a document conversion.
#[derive(Debug)]
pub struct ConversionOutcome {
    pub output_path: PathBuf,
    pub page_count: u32,
    pub size_bytes: u64,
    pub backend: Backend,
}

/// Convert an Office document to PDF at `output_path`.
pub fn convert_to_pdf(input_path: &Path, output_path: &Path) -> ConfiResult<ConversionOutcome> {
    let kind = kind_of(input_path).ok_or_else(|| {
        ConfiError::unsupported_format(format!(
            "'{}' is not a document CONFI can convert. Supported: Word (.doc, .docx, .rtf, .odt, .txt), \
             PowerPoint (.ppt, .pptx, .odp), Excel (.xls, .xlsx, .ods, .csv).",
            input_path.display()
        ))
    })?;

    if !input_path.is_file() {
        return Err(ConfiError::invalid_input(format!(
            "File not found: {}",
            input_path.display()
        )));
    }

    require_absent(output_path)?;

    let backend = detect_backend();
    if backend == Backend::None {
        return Err(ConfiError::new(
            ErrorCode::UnsupportedFormat,
            "Converting documents to PDF needs Microsoft Office installed on this machine. \
             CONFI does not send files anywhere.",
        ));
    }

    let temp_pdf = temp_path_for(output_path);

    let result = match backend {
        Backend::MsOffice => convert_with_ms_office(input_path, &temp_pdf, kind),
        Backend::None => unreachable!("checked above"),
    };

    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp_pdf);
        return Err(error);
    }

    if !temp_pdf.exists() {
        return Err(ConfiError::processing_error(
            "The converter reported success but produced no PDF.",
        ));
    }

    // Validate before the file reaches the user's destination.
    let page_count = match crate::pdf_engine::page_count(&temp_pdf) {
        Ok(count) if count > 0 => count,
        Ok(_) => {
            let _ = std::fs::remove_file(&temp_pdf);
            return Err(ConfiError::processing_error(
                "The converted PDF has no pages. The output has been discarded.",
            ));
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temp_pdf);
            return Err(ConfiError::processing_error_detail(
                "The converted PDF could not be reopened and was discarded.",
                format!("{error} ({error:?})"),
            ));
        }
    };

    let size_bytes = std::fs::metadata(&temp_pdf).map(|m| m.len()).unwrap_or(0);
    finalize(&temp_pdf, output_path)?;

    Ok(ConversionOutcome {
        output_path: output_path.to_owned(),
        page_count,
        size_bytes,
        backend,
    })
}

// ── Backend detection ───────────────────────────────────────────────────────

/// Which converter is available on this machine.
pub fn detect_backend() -> Backend {
    if cfg!(windows) && ms_office_registered() {
        return Backend::MsOffice;
    }
    Backend::None
}

/// Office installs register COM ProgIDs; their presence is the cheapest reliable
/// signal that automation will work.
#[cfg(windows)]
fn ms_office_registered() -> bool {
    let mut command = std::process::Command::new("reg");
    command
        .args(["query", r"HKCR\PowerPoint.Application", "/ve"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn ms_office_registered() -> bool {
    false
}

// ── Microsoft Office backend ────────────────────────────────────────────────

fn convert_with_ms_office(input: &Path, output: &Path, kind: DocumentKind) -> ConfiResult<()> {
    if !cfg!(windows) {
        return Err(ConfiError::unsupported_format(
            "Microsoft Office automation is only available on Windows.",
        ));
    }

    let script = match kind {
        DocumentKind::PowerPoint => power_point_script(input, output),
        DocumentKind::Word => word_script(input, output),
        DocumentKind::Excel => excel_script(input, output),
    };

    run_powershell(&script)
}

fn ps_path(path: &Path) -> String {
    // Single-quoted PowerShell literal; embedded quotes are doubled.
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn power_point_script(input: &Path, output: &Path) -> String {
    // WithWindow must be msoTrue (-1): passing msoFalse makes PowerPoint refuse
    // to open the presentation ("PowerPoint could not open the file").
    // Arguments: FileName, ReadOnly, Untitled, WithWindow.
    format!(
        r#"$ErrorActionPreference='Stop'
$app = New-Object -ComObject PowerPoint.Application
try {{
    $pres = $app.Presentations.Open({inp}, -1, 0, -1)
    try {{ $pres.SaveAs({out}, 32) }} finally {{ $pres.Close() }}
}} finally {{ $app.Quit() }}
"#,
        inp = ps_path(input),
        out = ps_path(output)
    )
}

fn word_script(input: &Path, output: &Path) -> String {
    // Two measured requirements:
    //  - DisplayAlerts must be off, or Word sits on a hidden dialog and the
    //    conversion never returns.
    //  - OpenNoRepairDialog instead of Open: some real-world .docx files make
    //    plain Open fail with 0x800A13E9 (the repair prompt cannot be answered
    //    from an automated session), while OpenNoRepairDialog opens them and
    //    exports correctly.
    format!(
        r#"$ErrorActionPreference='Stop'
$app = New-Object -ComObject Word.Application
try {{
    $app.Visible = $false
    $app.DisplayAlerts = 0
    $doc = $app.Documents.OpenNoRepairDialog({inp}, $false, $true, $false)
    try {{ $doc.SaveAs2({out}, 17) }} finally {{ $doc.Close($false) }}
}} finally {{ $app.Quit() }}
"#,
        inp = ps_path(input),
        out = ps_path(output)
    )
}

fn excel_script(input: &Path, output: &Path) -> String {
    format!(
        r#"$ErrorActionPreference='Stop'
$app = New-Object -ComObject Excel.Application
try {{
    $app.Visible = $false
    $app.DisplayAlerts = $false
    $wb = $app.Workbooks.Open({inp}, 0, $true)
    try {{ $wb.ExportAsFixedFormat(0, {out}) }} finally {{ $wb.Close($false) }}
}} finally {{ $app.Quit() }}
"#,
        inp = ps_path(input),
        out = ps_path(output)
    )
}

/// Run a PowerShell command and wait for it, with a timeout so a stuck Office
/// dialog cannot hang the application forever.
fn run_powershell(script: &str) -> ConfiResult<()> {
    const TIMEOUT: Duration = Duration::from_secs(120);

    // -WindowStyle Hidden and null stdio mirror the invocation that Office
    // automation was measured working under. With piped stdio Word can sit for
    // ~80 s and then fail with 0x800A13E9 instead of converting.
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            ConfiError::processing_error_detail(
                "Could not start PowerShell for Office automation",
                e.to_string(),
            )
        })?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut stderr = String::new();
                if let Some(mut handle) = child.stderr.take() {
                    use std::io::Read;
                    let _ = handle.read_to_string(&mut stderr);
                }
                return Err(ConfiError::processing_error_detail(
                    "Microsoft Office could not convert this document.",
                    stderr.trim().to_string(),
                ));
            }
            Ok(None) => {
                if started.elapsed() > TIMEOUT {
                    let _ = child.kill();
                    return Err(ConfiError::processing_error(
                        "Converting this document took too long and was stopped.",
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(ConfiError::processing_error_detail(
                    "Could not determine whether the conversion finished",
                    e.to_string(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_convertible_extensions() {
        assert_eq!(kind_of(Path::new("a.pptx")), Some(DocumentKind::PowerPoint));
        assert_eq!(kind_of(Path::new("a.PPT")), Some(DocumentKind::PowerPoint));
        assert_eq!(kind_of(Path::new("a.docx")), Some(DocumentKind::Word));
        assert_eq!(kind_of(Path::new("a.xlsx")), Some(DocumentKind::Excel));
        assert_eq!(kind_of(Path::new("a.pdf")), None);
        assert_eq!(kind_of(Path::new("a.png")), None);
        assert_eq!(kind_of(Path::new("no-extension")), None);
    }

    #[test]
    fn rejects_non_documents_with_a_clear_code() {
        let dir = tempfile::TempDir::new().unwrap();
        let input = dir.path().join("photo.png");
        std::fs::write(&input, b"not really a png").unwrap();

        let error = convert_to_pdf(&input, &dir.path().join("out.pdf")).unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedFormat);
        assert!(error.message.contains("not a document CONFI can convert"));
    }

    #[test]
    fn missing_input_is_invalid_input() {
        let dir = tempfile::TempDir::new().unwrap();
        let error =
            convert_to_pdf(&dir.path().join("nope.pptx"), &dir.path().join("out.pdf")).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn refuses_to_overwrite_existing_output() {
        let dir = tempfile::TempDir::new().unwrap();
        let input = dir.path().join("deck.pptx");
        std::fs::write(&input, b"stub").unwrap();
        let output = dir.path().join("out.pdf");
        std::fs::write(&output, b"existing").unwrap();

        let error = convert_to_pdf(&input, &output).unwrap_err();
        assert_eq!(error.code, ErrorCode::OutputError);
        assert_eq!(std::fs::read(&output).unwrap(), b"existing");
    }
}
