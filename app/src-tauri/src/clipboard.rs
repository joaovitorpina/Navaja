//! Copying from Rust, so sensitive copies can be kept out of clipboard
//! history and cloud sync with each OS's exclusion markers.

use std::sync::{Mutex, PoisonError};

/// Larger copies are refused.
const MAX_BYTES: usize = 64 * 1024 * 1024;

/// One long-lived clipboard handle: on Linux the copying process serves the
/// data, so it must stay alive after the copy.
#[derive(Default)]
pub struct Clipboard(Mutex<Option<arboard::Clipboard>>);

impl Clipboard {
    pub fn copy(&self, text: String, sensitive: bool) -> Result<(), String> {
        if text.len() > MAX_BYTES {
            return Err("text is too large to copy".to_owned());
        }
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            *guard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        let Some(clipboard) = guard.as_mut() else {
            return Err("clipboard unavailable".to_owned());
        };
        let set = clipboard.set();
        let set = if sensitive { exclude(set) } else { set };
        set.text(text).map_err(|e| e.to_string())
    }
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
