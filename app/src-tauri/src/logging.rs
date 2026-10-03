//! Local logs only: a daily file in `<app dir>/logs`, seven kept, never
//! uploaded. Tools don't log; the app logs tool id, action, duration and
//! error code, never inputs or outputs.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;

/// Keep it alive for the life of the app: dropping it flushes the file writer.
pub struct LogGuard(#[allow(dead_code)] Option<WorkerGuard>);

/// Set `NAVAJA_LOG` (an `EnvFilter` directive) to change verbosity.
const FILTER_ENV: &str = "NAVAJA_LOG";
/// Other crates are capped at warn so a dependency can't log tool data.
const DEFAULT_FILTER: &str = "warn,navaja_lib=info,navaja_core=info,navaja_tools=info";

pub fn init(app_dir: Option<&Path>) -> LogGuard {
    let filter =
        EnvFilter::try_from_env(FILTER_ENV).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let file = app_dir.and_then(|dir| {
        let logs = dir.join("logs");
        crate::paths::ensure_private_dir(&logs).ok()?;
        RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("navaja")
            .filename_suffix("log")
            .max_log_files(7)
            .build(logs)
            .ok()
    });

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_target(true);
    match file {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            // Debug builds also echo to stderr for development.
            let writer = writer.and(std::io::stderr.with_filter(|_| cfg!(debug_assertions)));
            let _ = builder.with_writer(writer).try_init();
            LogGuard(Some(guard))
        }
        None => {
            let _ = builder.with_writer(std::io::stderr).try_init();
            LogGuard(None)
        }
    }
}
