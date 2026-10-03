//! OS-specific shell behaviour, one module per OS (docs/architecture.md §1).

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::is_elevated;
#[cfg(windows)]
pub use windows::is_elevated;
