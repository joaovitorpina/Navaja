//! Helpers shared by the unit tests.

use std::sync::{Arc, Mutex};

/// An in-memory log for a test subscriber.
#[derive(Clone, Default)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CapturedLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Runs `f` with a `trace` level subscriber on this thread, then returns
/// what `f` returned and every line it logged. Events from other threads
/// are not recorded.
pub fn capture_log<T>(f: impl FnOnce() -> T) -> (T, String) {
    let log = CapturedLog::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer({
            let log = log.clone();
            move || log.clone()
        })
        .finish();
    let returned = tracing::subscriber::with_default(subscriber, f);
    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    (returned, text)
}

/// A random marker that no log line or message holds by chance.
pub fn canary() -> String {
    use std::hash::BuildHasher;
    let random = std::collections::hash_map::RandomState::new().hash_one(0u8);
    format!("CANARY-{random:016x}")
}
