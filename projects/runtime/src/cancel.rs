//! Cancellation adapters avoid polling bridge threads between native services.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
pub trait Cancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}
impl Cancellation for AtomicUsize {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Relaxed) != 0
    }
}
impl Cancellation for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}
impl<T: Cancellation> Cancellation for Arc<T> {
    fn is_cancelled(&self) -> bool {
        self.as_ref().is_cancelled()
    }
}
