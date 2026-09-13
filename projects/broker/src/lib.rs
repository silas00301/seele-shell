//! Broker protocol and lifecycle. Only metadata crosses the list interface;
//! compiled schemas, source context, prompts, and results are memory-only.
pub mod health;
pub mod lifecycle;
pub mod runner;
pub mod transport;
pub mod validation;

pub const MAX_MESSAGE: usize = 256 * 1024;
pub const MAX_REPLY: usize = MAX_MESSAGE * 2;
pub type Failure = &'static str;
pub type Result<T> = std::result::Result<T, Failure>;
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
