// CONFI — Deep-link URI building
//
// When CONFI is not running, the shell helper hands it a `confi://action/...`
// URI. Only the producing half lives here: nothing parses these URIs yet.
// The parsing half belongs with the app that registers the URI handler (M7),
// which can then reach for the `percent-encoding` crate instead of a local copy.
//
// Format: `confi://action/<action>?files=<encoded_path_1>,<encoded_path_2>`
// Each path is individually percent-encoded, preserving commas as delimiters.

use std::path::PathBuf;

/// Percent-encode a UTF-8 string per RFC 3986 §2.3 (unreserved bytes pass through).
pub fn percent_encode_str(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(bytes.len() * 3);

    for &b in bytes {
        // ALPHA / DIGIT / "-" / "." / "_" / "~"
        if matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }

    out
}

/// Build a `confi://` deep-link URI for an action and set of files.
pub fn build_deep_link(action: &str, files: &[PathBuf]) -> String {
    let encoded_files: Vec<String> = files
        .iter()
        .map(|f| percent_encode_str(&f.to_string_lossy()))
        .collect();

    format!(
        "confi://action/{}?files={}",
        percent_encode_str(action),
        encoded_files.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_and_space_characters_are_encoded() {
        let encoded = percent_encode_str("C:\\My Files\\Doc & Report #1 (100% OK)?.pdf");
        assert!(!encoded.contains(' '));
        assert!(!encoded.contains('&'));
        assert!(!encoded.contains('?'));
        assert!(!encoded.contains('#'));
        assert!(encoded.contains("%25")); // % itself
        assert!(encoded.contains("%20")); // space
    }

    #[test]
    fn non_ascii_filenames_are_encoded() {
        for input in [
            "C:\\مستندات\\تقرير_مالي_2026.pdf",
            "D:\\文档\\工作报告_2026年.pdf",
        ] {
            let encoded = percent_encode_str(input);
            assert!(encoded.is_ascii(), "encoding must be pure ASCII: {encoded}");
        }
    }

    #[test]
    fn deep_link_shape() {
        let files = vec![
            PathBuf::from("C:\\Docs\\A & B.pdf"),
            PathBuf::from("C:\\Docs\\تقرير.pdf"),
        ];
        let uri = build_deep_link("merge", &files);

        assert!(uri.starts_with("confi://action/merge?files="));
        assert!(!uri.contains(' '));
        // Both paths present, still comma-delimited.
        assert_eq!(uri.matches(',').count(), 1);
    }
}
