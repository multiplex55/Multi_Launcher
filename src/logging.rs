use tracing_subscriber::EnvFilter;

/// Initialise logging. In debug builds the default level is `debug` while in
/// release builds it falls back to `info`. The level can be overridden via the
/// `RUST_LOG` environment variable.
pub fn init() {
    // Pick a sensible default depending on build type but honour any user
    // supplied `RUST_LOG` filter.
    let default_level = if cfg!(debug_assertions) { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init();
}
