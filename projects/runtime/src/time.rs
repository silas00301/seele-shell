//! UTC formatting with caller-owned storage, safe across worker threads.
use std::ffi::CStr;
use std::time::{SystemTime, UNIX_EPOCH};
pub fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    format_timestamp(seconds).unwrap_or_default()
}
pub fn format_timestamp(seconds: libc::time_t) -> Option<String> {
    // SAFETY: all storage is caller-owned, formats are NUL-terminated, and
    // strftime success guarantees a terminated result within the buffer.
    unsafe {
        let mut utc: libc::tm = std::mem::zeroed();
        if libc::gmtime_r(&seconds, &mut utc).is_null() {
            return None;
        }
        let mut buffer = [0 as libc::c_char; 64];
        if libc::strftime(
            buffer.as_mut_ptr(),
            buffer.len(),
            c"%Y-%m-%dT%H:%M:%SZ".as_ptr(),
            &utc,
        ) == 0
        {
            return None;
        }
        Some(CStr::from_ptr(buffer.as_ptr()).to_str().ok()?.to_owned())
    }
}
