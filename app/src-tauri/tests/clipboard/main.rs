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
//! - Linux (`linux.rs`): the `x-kde-passwordManagerHint` target, over X11,
//!   which is also how arboard copies on GNOME Wayland (through Xwayland).
//!   With no X display, or where arboard copies over Wayland (a compositor
//!   with data-control, such as KDE's), it prints why and passes without
//!   checking. CI's nextest step has no display, so CI checks Windows and
//!   macOS only.
//!
//! It replaces what is on the clipboard with two random canaries. On Windows
//! and macOS it leaves the second, Navaja's own copy, there. On X11 the
//! clipboard ends up empty: the copy carries the hint, so the last arboard
//! handle to drop gives the selection up rather than hand it to a clipboard
//! manager. The first, the ordinary copy, can reach a clipboard history,
//! cloud clipboard or Universal Clipboard like any other copy.
//!
//! The clipboard belongs to the whole desktop session. So this binary holds
//! one test, `.config/nextest.toml` runs it in a test group of one thread,
//! and the test takes a lock file in the temp directory against a run from
//! another worktree or terminal (`take_turn`). nextest also shows its output
//! on a pass.

#![cfg(any(windows, target_os = "macos", target_os = "linux"))]

use std::collections::hash_map::RandomState;
use std::fs::{File, OpenOptions, TryLockError};
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
/// How many times in all a copy is made, when something else replaces it
/// before it is read.
const COPIES: u32 = 5;
/// How long another run of this test may hold the clipboard.
const TURN: Duration = Duration::from_secs(120);

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
    // Released last, after the clipboard handles below.
    let _turn = take_turn();
    // Held to the end: on X11, the last handle to drop takes the copied
    // text with it.
    let mut ordinary = retry("opening the clipboard", || {
        arboard::Clipboard::new().map_err(|e| e.to_string())
    });
    let registry = Registry::new(Vec::new()).expect("an empty registry is valid");
    let app = AppState::new(registry, SettingsStore::load(None), None);

    // The control: a reader that saw markers on every copy fails here.
    let plain = canary("ordinary");
    let seen = copy_and_read(&reader, &plain, "an ordinary copy", || {
        ordinary.set_text(plain.as_str()).map_err(|e| e.to_string())
    });
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
    let seen = copy_and_read(&reader, &copied, "Navaja's copy", || {
        app.clipboard.copy(copied.clone())
    });
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

/// Copies `canary` with `copy`, then reads the clipboard until it holds it,
/// through busy moments and copies still on their way. Something else may
/// replace the copy before it is read: a cloud clipboard item, an item from
/// a nearby device, or the user copying. Then it copies again, `COPIES`
/// times in all. A failure never prints what the clipboard held instead,
/// which may be someone's own text.
fn copy_and_read(
    reader: &os::Reader,
    canary: &str,
    what: &str,
    mut copy: impl FnMut() -> Result<(), String>,
) -> Contents {
    for _ in 0..COPIES {
        retry(what, &mut copy);
        let deadline = Instant::now() + PATIENCE;
        loop {
            let problem = match reader.read() {
                Ok(contents) if contents.text.as_deref() == Some(canary) => return contents,
                Ok(contents) if contents.text.is_some() => {
                    say(&format!(
                        "after {what}, the clipboard holds another text; copying again"
                    ));
                    thread::sleep(PAUSE);
                    break;
                }
                Ok(_) => "it holds no text".to_owned(),
                Err(error) => error,
            };
            assert!(
                Instant::now() < deadline,
                "after {what}, the clipboard did not hold the copied text within {PATIENCE:?}: {problem}"
            );
            thread::sleep(PAUSE);
        }
    }
    panic!("after {what}, the clipboard held another text each time; {COPIES} copies in all");
}

/// Waits until no other run of this test on the machine holds the
/// clipboard. nextest's test group orders the tests of one run only, and a
/// run from another worktree or terminal would replace this run's copy.
/// Worktrees do not share `target/`, so the lock is a fixed file in the
/// temp directory. The OS releases it when the file is closed or the
/// process ends.
fn take_turn() -> File {
    let path = std::env::temp_dir().join("navaja-clipboard-test.lock");
    // Another user's file in a shared temp directory opens read-only, which
    // is enough to lock it.
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .or_else(|_| File::open(&path))
        .unwrap_or_else(|error| panic!("cannot open {}: {error}", path.display()));
    let deadline = Instant::now() + TURN;
    loop {
        match file.try_lock() {
            Ok(()) => return file,
            Err(TryLockError::WouldBlock) => {
                assert!(
                    Instant::now() < deadline,
                    "another run of this test held {} for {TURN:?}",
                    path.display(),
                );
                thread::sleep(PAUSE);
            }
            Err(TryLockError::Error(error)) => {
                panic!("cannot lock {}: {error}", path.display())
            }
        }
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
