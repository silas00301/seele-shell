pub mod capture;
pub mod context;
pub mod suggestions;
pub type Result<T> = std::result::Result<T, &'static str>;
pub const SESSION_ENV: &str = "SEELE_SHELL_AI_SESSION";
pub const MAX_CAPTURE_BYTES: usize = 64 * 1024;
pub const MAX_STDERR_BYTES: usize = 16 * 1024;
pub const MAX_REQUEST_CHARS: usize = 8 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;

pub fn clean_display(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|c| {
            (!c.is_control() || matches!(c, '\n' | '\t')) && seele_runtime::redact::visible(*c)
        })
        .take(limit)
        .collect()
}
