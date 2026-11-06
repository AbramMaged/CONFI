// CONFI — Deep-Link URI & Percent-Encoding
//
// Implements standard RFC 3986 percent-encoding for UTF-8 byte sequences.
// Correctly handles:
//   - Non-ASCII characters (Arabic, Chinese, Cyrillic, etc.)
//   - Reserved URI characters (%, &, ?, =, #, +, spaces, etc.)
//   - Safe path preservation across Windows shell invocations.

use std::path::PathBuf;

use crate::protocol::ConfiRequest;

/// Check if an ASCII byte is unreserved per RFC 3986 §2.3.
/// Unreserved = ALPHA / DIGIT / "-" / "." / "_" / "~"
#[inline]
pub fn is_unreserved(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~')
}

/// Percent-encode arbitrary bytes into an RFC 3986-compliant ASCII string.
pub fn percent_encode_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Percent-encode a UTF-8 string by encoding its raw UTF-8 bytes.
pub fn percent_encode_str(s: &str) -> String {
    percent_encode_bytes(s.as_bytes())
}

/// Percent-decode an RFC 3986-encoded string back into a UTF-8 string.
pub fn percent_decode(encoded: &str) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut chars = encoded.bytes().peekable();

    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars
                .next()
                .ok_or_else(|| "Premature end of percent sequence".to_string())?;
            let h2 = chars
                .next()
                .ok_or_else(|| "Premature end of percent sequence".to_string())?;

            let byte_val = hex_chars_to_byte(h1, h2)
                .ok_or_else(|| format!("Invalid hex sequence: %{}{}", h1 as char, h2 as char))?;
            bytes.push(byte_val);
        } else {
            bytes.push(b);
        }
    }

    String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8 in decoded payload: {e}"))
}

fn hex_chars_to_byte(h1: u8, h2: u8) -> Option<u8> {
    let n1 = hex_digit_to_val(h1)?;
    let n2 = hex_digit_to_val(h2)?;
    Some((n1 << 4) | n2)
}

fn hex_digit_to_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Build a `confi://` deep-link URI for an action and set of files.
///
/// Format: `confi://action/<action>?files=<encoded_path_1>,<encoded_path_2>`
/// Each path is individually percent-encoded, preserving commas as delimiters.
pub fn build_deep_link(action: &str, files: &[PathBuf]) -> String {
    let encoded_action = percent_encode_str(action);
    let encoded_files: Vec<String> = files
        .iter()
        .map(|f| percent_encode_str(&f.to_string_lossy()))
        .collect();

    format!(
        "confi://action/{}?files={}",
        encoded_action,
        encoded_files.join(",")
    )
}

/// Parse a `confi://action/...` deep-link URI into a `ConfiRequest`.
pub fn parse_deep_link(uri: &str) -> Result<ConfiRequest, String> {
    let trimmed = uri.trim();
    const PREFIX: &str = "confi://action/";
    if !trimmed.starts_with(PREFIX) {
        return Err(format!("URI does not start with '{PREFIX}'"));
    }

    let remainder = &trimmed[PREFIX.len()..];
    let mut parts = remainder.splitn(2, '?');
    let action_raw = parts.next().unwrap_or("");
    let query_raw = parts.next().unwrap_or("");

    let operation = percent_decode(action_raw)?;

    let mut files = Vec::new();
    for param in query_raw.split('&') {
        if let Some(files_csv) = param.strip_prefix("files=") {
            for item in files_csv.split(',') {
                if !item.is_empty() {
                    let decoded_path = percent_decode(item)?;
                    files.push(PathBuf::from(decoded_path));
                }
            }
        }
    }

    Ok(ConfiRequest { operation, files })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ascii_and_reserved_chars() {
        let input = "C:\\My Files\\Doc & Report #1 (100% OK)?.pdf";
        let encoded = percent_encode_str(input);
        assert!(!encoded.contains(' '));
        assert!(!encoded.contains('&'));
        assert!(!encoded.contains('?'));
        assert!(!encoded.contains('#'));
        // % is encoded as %25
        assert!(encoded.contains("%25"));
        assert!(encoded.contains("%20"));

        let decoded = percent_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn roundtrip_arabic_filenames() {
        let input = "C:\\مستندات\\تقرير_مالي_2026.pdf";
        let encoded = percent_encode_str(input);
        assert!(!encoded.contains('م'));
        let decoded = percent_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn roundtrip_chinese_filenames() {
        let input = "D:\\文档\\工作报告_2026年.pdf";
        let encoded = percent_encode_str(input);
        assert!(!encoded.contains('文'));
        let decoded = percent_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn deep_link_formatting_and_parsing() {
        let files = vec![
            PathBuf::from("C:\\Docs\\A & B.pdf"),
            PathBuf::from("C:\\Docs\\تقرير.pdf"),
            PathBuf::from("C:\\Docs\\100%_done.png"),
        ];

        let uri = build_deep_link("merge", &files);
        assert!(uri.starts_with("confi://action/merge?files="));

        let parsed = parse_deep_link(&uri).unwrap();
        assert_eq!(parsed.operation, "merge");
        assert_eq!(parsed.files, files);
    }
}
