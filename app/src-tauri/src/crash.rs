//! The panic hook. It replaces the default hook, which prints the panic
//! payload (possibly pasted input) to stderr, and never chains to it.
//!
//! It records only where the panic happened and on which thread: one log
//! line, plus a local crash file with a backtrace. Nothing is uploaded.

use std::backtrace::Backtrace;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Crash files kept; older ones are deleted.
const KEEP: usize = 10;

pub fn install(app_dir: Option<PathBuf>) {
    let crash_dir = app_dir.map(|dir| dir.join("crash"));
    std::panic::set_hook(Box::new(move |info| {
        // Deliberately never reads info.payload().
        let location = info.location().map_or_else(
            || "unknown".to_owned(),
            |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
        );
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_owned();
        tracing::error!(%location, %thread, "panic (payload withheld)");
        if let Some(dir) = &crash_dir {
            write_report(dir, &location, &thread);
        }
    }));
}

fn write_report(dir: &Path, location: &str, thread: &str) {
    if crate::paths::ensure_private_dir(dir).is_err() {
        return;
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let report = format!(
        "Navaja {} panicked.\nlocation: {location}\nthread: {thread}\n\
         The panic message is not recorded because it may contain your input.\n\n\
         backtrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        Backtrace::force_capture()
    );
    let _ = std::fs::write(dir.join(format!("crash-{millis}.txt")), report);
    prune(dir);
}

fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .collect();
    // crash-<millis>.txt sorts chronologically once the numbers have equal width.
    files.sort_by_key(|p| p.file_name().map(|n| (n.len(), n.to_owned())));
    let excess = files.len().saturating_sub(KEEP);
    for old in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_the_newest_files() {
        let dir = std::env::temp_dir().join(format!("navaja-crash-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Mixed widths: 999 is older than 1000 although it sorts after it as text.
        let millis: Vec<u64> = (990..1002).collect();
        for m in &millis {
            std::fs::write(dir.join(format!("crash-{m}.txt")), "").unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        prune(&dir);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        let mut expected: Vec<String> = millis[millis.len() - KEEP..]
            .iter()
            .map(|m| format!("crash-{m}.txt"))
            .collect();
        expected.push("notes.txt".to_owned());
        expected.sort();
        assert_eq!(left, expected);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
