//! Copying from Rust, so every copy carries the exclusion markers each OS
//! offers:
//! - Windows: kept out of clipboard history and cloud sync, and excluded from
//!   clipboard monitoring.
//! - macOS: marked `org.nspasteboard.ConcealedType`, which clipboard-history
//!   apps honour. Universal Clipboard (Handoff) is not excluded yet; that
//!   needs NSPasteboard's current-host-only option (a recorded follow-up).
//! - Linux: marked `x-kde-passwordManagerHint`, which Klipper and similar
//!   history managers honour.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, PoisonError};

/// Larger copies are refused (docs/architecture.md §5, "Input limits").
const MAX_BYTES: usize = 64 * 1024 * 1024;

/// The size check, apart from the clipboard, so a test can cover it without
/// touching the real one.
fn check_size(bytes: usize) -> Result<(), String> {
    if bytes > MAX_BYTES {
        return Err("text is too large to copy".to_owned());
    }
    Ok(())
}

/// One long-lived clipboard handle: on Linux the copying process serves the
/// data, so it must stay alive after the copy.
#[derive(Default)]
pub struct Clipboard(Mutex<Option<arboard::Clipboard>>);

impl Clipboard {
    pub fn copy(&self, text: String) -> Result<(), String> {
        check_size(text.len())?;
        let mut handle = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        // A panic in arboard fails this copy instead of the app. The panic
        // hook records only where it happened, never the payload.
        catch_unwind(AssertUnwindSafe(|| set_text(&mut handle, text))).unwrap_or_else(|_| {
            // The handle may be half-updated; the next copy opens a new one.
            *handle = None;
            Err("clipboard failed".to_owned())
        })
    }

    /// Drops the handle at exit. On X11 this hands the copied text to the
    /// clipboard manager, or clears it, as the password-manager hint asks.
    pub fn release(&self) {
        let handle = self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
        drop(handle);
    }
}

fn set_text(handle: &mut Option<arboard::Clipboard>, text: String) -> Result<(), String> {
    if handle.is_none() {
        *handle = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    let Some(clipboard) = handle.as_mut() else {
        return Err("clipboard unavailable".to_owned());
    };
    exclude(clipboard.set())
        .text(text)
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn exclude(set: arboard::Set<'_>) -> arboard::Set<'_> {
    use arboard::SetExtWindows;
    set.exclude_from_history()
        .exclude_from_cloud()
        .exclude_from_monitoring()
}

#[cfg(target_os = "macos")]
fn exclude(set: arboard::Set<'_>) -> arboard::Set<'_> {
    use arboard::SetExtApple;
    set.exclude_from_history()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn exclude(set: arboard::Set<'_>) -> arboard::Set<'_> {
    use arboard::SetExtLinux;
    set.exclude_from_history()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_over_64_mib_are_refused() {
        assert_eq!(MAX_BYTES, 64 * 1024 * 1024);
        assert_eq!(check_size(0), Ok(()));
        assert_eq!(check_size(MAX_BYTES), Ok(()));
        assert_eq!(
            check_size(MAX_BYTES + 1),
            Err("text is too large to copy".to_owned())
        );
    }
}
