//! Shared mechanisms for Seele's independent runtime processes.
//!
//! Presentation and protocol policy belong to consumers. Resource bounds,
//! private filesystem operations, framing and child ownership belong here.

pub mod fs;
pub mod github;
pub mod process;
pub mod time;
pub mod wire;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T = ()> = std::result::Result<T, Error>;

pub mod inference;

pub mod redact;

#[cfg(feature = "reactor")]
pub mod reactor;

pub mod cancel;

pub mod codex;

pub mod nix;
