//! Shared, linear-time secret syntax redaction. Apply before truncation so
//! cropped credentials cannot evade the matcher; callers bound input bytes.
use regex::Regex;
use std::sync::LazyLock;
static PEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)-----BEGIN [^\r\n-]+-----.*?(?:-----END [^\r\n-]+-----|$)").unwrap()
});
static HEADERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?im)^(\s*(?:authorization|proxy-authorization|cookie|set-cookie)\s*[:=])[^\r\n]*")
        .unwrap()
});
// Quoted values may contain escaped quotes, spaces or newlines. A missing
// closing quote consumes the remaining report rather than exposing its tail.
// Repeated segments also cover shell concatenation such as 'part'\''tail'.
const VALUE: &str = r#"(?:"(?:\\.|[^"\\])*(?:"|$)|'(?:\\.|[^'\\])*(?:'|$)|\\.|[^\s,;}"'\\]+)+"#;
static FIELDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r#"(?is)(["']?[A-Za-z0-9_.-]*(?:pass(?:word)?|token|secret|api[_-]?key|auth(?:entication|orization)?|cookie|credential|private[_-]?key)[A-Za-z0-9_.-]*["']?\s*[:=]\s*){VALUE}"#)).unwrap()
});
static FLAGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r#"(?is)(--?(?:pass(?:word)?|token|secret|api[_-]?key|authorization|cookie|credential|private[_-]?key)(?:=|\s+)){VALUE}"#)).unwrap()
});
static TOKENS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:(?:bearer|basic)\s+[^\s,;]+|(?:sk-|gh[pousr]_|github_pat_|glpat-|xox[baprs]-)[A-Za-z0-9_-]+|(?:AKIA|ASIA)[0-9A-Z]{16}|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})").unwrap()
});
static USERINFO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b([a-z][a-z0-9+.-]*://)[^/?#\s]+@").unwrap());
static LINKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)https?://[^\s]+").unwrap());
static NAMES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:pass(?:word)?|token|secret|api[_-]?key|auth(?:entication|orization)?|cookie|credential|private[_-]?key)").unwrap()
});
pub fn secret_name(name: &str) -> bool {
    NAMES.is_match(name)
}
pub fn secrets(text: &str, omit_urls: bool) -> String {
    let text = PEM.replace_all(text, "[REDACTED PRIVATE MATERIAL]");
    let text = HEADERS.replace_all(&text, "$1 [REDACTED]");
    let text = FIELDS.replace_all(&text, "$1[REDACTED]");
    let text = FLAGS.replace_all(&text, "$1[REDACTED]");
    let text = TOKENS.replace_all(&text, "[REDACTED TOKEN]");
    let text = USERINFO.replace_all(&text, "$1[REDACTED]@");
    if omit_urls {
        LINKS.replace_all(&text, "[link omitted]").into_owned()
    } else {
        text.into_owned()
    }
}
pub fn visible(character: char) -> bool {
    !matches!(character,'\u{061c}'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}')
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_escapes_unclosed_values_and_modern_tokens_never_leak_tails() {
        for input in [
            r#"{"token":"prefix\"secret-tail"}"#,
            r#"--password "prefix\"secret-tail""#,
            "--password 'prefix'\\''secret-tail'",
            "token=\"prefix secret-tail",
            "token='prefix\nsecret-tail",
            "diagnostic Basic c2VjcmV0LXRhaWw=",
            "github_pat_secret_tail_1234567890",
            "ASIA0123456789ABCDEF",
            "https://alice:prefix@secret-tail@example.test/path",
        ] {
            let output = secrets(input, false);
            for private in [
                "secret-tail",
                "secret_tail",
                "c2VjcmV0LXRhaWw",
                "0123456789ABCDEF",
            ] {
                assert!(
                    !output.contains(private),
                    "redaction leaked a synthetic credential tail"
                );
            }
        }
        let output = secrets(
            r#"{"token":"prefix\"secret-tail", "public":"retained"}"#,
            false,
        );
        assert!(output.contains("retained"));
        assert!(
            secrets("https://alice:prefix@secret-tail@example.test/path", false)
                .contains("example.test/path")
        );
    }

    #[test]
    fn covers_multiline_private_keys_headers_quoted_flags_and_tokens() {
        let input = "TOKEN=known-secret\nAuthorization: Bearer opaque-value\nremote=https://alice:password@example.test/repo\ngithub=ghp_abcdefghijklmnopqrstuvwxyz123456\n\"api_token\": \"json-secret\"\nExecStart=demo --password cli-secret\n-----BEGIN PRIVATE KEY-----\nunterminated-private-data";
        let output = secrets(input, false);
        for secret in [
            "known-secret",
            "opaque-value",
            "alice:password",
            "ghp_abcdefghijklmnopqrstuvwxyz123456",
            "json-secret",
            "cli-secret",
            "unterminated-private-data",
        ] {
            assert!(!output.contains(secret), "{secret}");
        }
        assert!(output.contains("example.test"));
        assert!(!secrets(input, true).contains("example.test"));
    }
}
