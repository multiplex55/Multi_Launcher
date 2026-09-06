use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialise logging. In debug builds the default level is `debug` while in
/// release builds it falls back to `info`. The level can be overridden via the
/// `RUST_LOG` environment variable. `debug` level can be explicitly enabled via
/// the settings file. When `log_file` is `Some`, logs are also written to the
/// given file path.
///
/// The returned guard must be retained for as long as file logging is needed.
/// Dropping it flushes pending records and shuts down the background writer.
pub fn init(debug: bool, log_file: Option<PathBuf>) -> Option<WorkerGuard> {
    let level = if debug { "debug" } else { "info" };

    let filter = if debug {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level))
    } else {
        EnvFilter::new(level)
    };

    let console_layer = fmt::layer().with_writer(std::io::stderr);

    if let Some(path) = log_file
        && let (Some(dir), Some(file)) = (path.parent(), path.file_name())
    {
        let file_appender = tracing_appender::rolling::never(dir, file);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let file_layer = fmt::layer().with_ansi(false).with_writer(non_blocking);
        return tracing_subscriber::registry()
            .with(filter)
            .with(console_layer)
            .with(file_layer)
            .try_init()
            .ok()
            .map(|()| guard);
    }

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .try_init();
    None
}
