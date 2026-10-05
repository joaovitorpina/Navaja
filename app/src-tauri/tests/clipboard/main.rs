//! The clipboard privacy markers (docs/architecture.md §5, "Clipboard"):
//! Navaja's copy carries every marker `src/clipboard.rs` sets on this OS,
//! and an ordinary copy carries none of them, which shows the check can tell
//! the two apart.
//!
//! The copy goes through `AppState::clipboard`, the call `copy_text` makes.
//! The clipboard is read back through the OS's own API, never arboard, so an
//! arboard update or a refactor that drops a marker fails here:
//! - Windows (`windows.rs`): the three registered formats, through Win32.
//! - macOS (`macos.rs`): `org.nspasteboard.ConcealedType`, through
//!   `NSPasteboard`.
//! - Linux (`linux.rs`): the `x-kde-passwordManagerHint` target, over X11.
//!   With no X display, or in a Wayland session, it prints why and passes
//!   without checking. CI's nextest step has no display, so CI checks
//!   Windows and macOS only.
//!
//! It replaces what is on the clipboard with two random canaries, and leaves
//! the second, Navaja's own copy, there. The first, the ordinary copy, can
//! reach a clipboard history like any other copy. The clipboard belongs to
//! the whole machine, so this binary holds one test, and
//! `.config/nextest.toml` runs it in a test group of one thread and shows
//! its output on a pass too.

#![cfg(any(windows, target_os = "macos", target_os = "linux"))]

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::thread;
use std::time::{Duration, Instant};

use navaja_core::Registry;
use navaja_lib::testing::{AppState, SettingsStore};

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod os;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;

/// How long a clipboard that is busy, or still taking a copy, gets.
const PATIENCE: Duration = Duration::from_secs(10);
const PAUSE: Duration = Duration::from_millis(50);

/// A marker Navaja's copy must carry: a clipboard format (Windows), a
/// pasteboard type (macOS) or a selection target (Linux).
pub struct Marker {
    pub name: &'static str,
    /// The data it must hold, for messages.
    pub value: &'static str,
    pub accepts: fn(&[u8]) -> bool,
}

/// What the clipboard holds, as the OS reports it.
#[derive(Default)]
pub struct Contents {
    /// Its text, if it holds any.
    pub text: Option<String>,
    /// Every format, type or target it offers, by name, for messages.
    pub offered: Vec<String>,
    /// The data of each of the OS's markers it carries.
    pub markers: Vec<(&'static str, Vec<u8>)>,
}

impl Contents {
    fn marker(&self, name: &str) -> Option<&[u8]> {
        self.markers
            .iter()
            .find(|(found, _)| *found == name)
            .map(|(_, data)| data.as_slice())
    }
}

#[test]
fn copy_carries_the_privacy_markers() {
    let reader = match os::Reader::open() {
        Ok(reader) => reader,
        Err(reason) => {
            say(&format!("skipped, nothing checked: {reason}"));
            return;
        }
    };
    // Held to the end: on X11, the last handle to drop takes the copied
    // text with it.
    let mut ordinary = retry("opening the clipboard", || {
        arboard::Clipboard::new().map_err(|e| e.to_string())
    });
    let registry = Registry::new(Vec::new()).expect("an empty registry is valid");
    let app = AppState::new(registry, SettingsStore::load(None), None);

    // The control: a reader that saw markers on every copy fails here.
    let plain = canary("ordinary");
    retry("an ordinary copy", || {
        ordinary.set_text(plain.as_str()).map_err(|e| e.to_string())
    });
    let seen = wait_for(&reader, &plain, "an ordinary copy");
    for marker in os::MARKERS {
        assert!(
            seen.marker(marker.name).is_none(),
            "an ordinary copy carries {}; the clipboard offers {:?}",
            marker.name,
            seen.offered,
        );
    }
    say(&format!("an ordinary copy offers {:?}", seen.offered));

    // Navaja's copy, through the call `copy_text` makes.
    let copied = canary("navaja");
    retry("Navaja's copy", || app.clipboard.copy(copied.clone()));
    let seen = wait_for(&reader, &copied, "Navaja's copy");
    for marker in os::MARKERS {
        let Some(data) = seen.marker(marker.name) else {
            panic!(
                "Navaja's copy lacks {}; the clipboard offers {:?}",
                marker.name, seen.offered,
            );
        };
        assert!(
            (marker.accepts)(data),
            "Navaja's copy carries {} as {data:02x?}, not {}",
            marker.name,
            marker.value,
        );
    }
    say(&format!("Navaja's copy offers {:?}", seen.offered));
    for marker in os::MARKERS {
        let data = seen.marker(marker.name).unwrap_or_default();
        say(&format!(
            "Navaja's copy carries {} = {data:02x?} ({}); the ordinary copy did not",
            marker.name, marker.value,
        ));
    }
}

/// A random text that nothing else puts on the clipboard.
fn canary(kind: &str) -> String {
    let random = RandomState::new().hash_one(kind);
    format!("navaja-clipboard-test-{kind}-{random:016x}")
}

/// Runs `attempt` until it succeeds, for up to `PATIENCE`: another program
/// may hold the clipboard for a moment, as Windows' clipboard service does
/// to read each new copy.
fn retry<T>(what: &str, mut attempt: impl FnMut() -> Result<T, String>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        match attempt() {
            Ok(value) => return value,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "{what} failed for {PATIENCE:?}: {error}"
                );
                thread::sleep(PAUSE);
            }
        }
    }
}

/// Reads the clipboard until it holds `canary`, through busy moments and
/// copies still on their way. A failure never prints what the clipboard
/// held instead, which may be someone's own text.
fn wait_for(reader: &os::Reader, canary: &str, what: &str) -> Contents {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let problem = match reader.read() {
            Ok(contents) if contents.text.as_deref() == Some(canary) => return contents,
            Ok(contents) if contents.text.is_none() => "it holds no text".to_owned(),
            Ok(_) => "it holds another text".to_owned(),
            Err(error) => error,
        };
        assert!(
            Instant::now() < deadline,
            "after {what}, the clipboard did not hold the copied text within {PATIENCE:?}: {problem}"
        );
        thread::sleep(PAUSE);
    }
}

/// Test output, which nextest captures; `.config/nextest.toml` shows it on a
/// pass too, so a CI log says what was checked or why nothing was.
#[expect(
    clippy::print_stdout,
    reason = "the test's report, captured by nextest"
)]
fn say(line: &str) {
    println!("clipboard markers, {}: {line}", std::env::consts::OS);
}
