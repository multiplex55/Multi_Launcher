use tracing_subscriber::EnvFilter;

/// Initialise logging. In debug builds the default level is `debug` while in
/// release builds it falls back to `info`. The level can be overridden via the
/// `RUST_LOG` environment variable.
/// `debug` level can be explicitly enabled via the settings file.
pub fn init(debug: bool) {
    // Pick a sensible default but honour any user supplied `RUST_LOG` filter.
    let default_level = if debug { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init();
}
