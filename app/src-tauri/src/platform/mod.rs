//! OS-specific shell behaviour, one module per OS (docs/architecture.md §1).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::tray_available;
#[cfg(target_os = "macos")]
pub use macos::disable_peer_connections;
#[cfg(unix)]
pub use unix::is_elevated;
#[cfg(windows)]
pub use windows::is_elevated;

/// Windows and macOS always have a tray.
#[cfg(not(target_os = "linux"))]
pub fn tray_available() -> bool {
    true
}
