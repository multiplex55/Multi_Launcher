use tracing_subscriber::EnvFilter;

/// Initialise logging. In debug builds the default level is `debug` while in
/// release builds it falls back to `info`. The level can be overridden via the
/// `RUST_LOG` environment variable.
/// `debug` level can be explicitly enabled via the settings file.
pub fn init(debug: bool) {
    // When debug logging is disabled we force `info` level regardless of the
    // `RUST_LOG` environment variable. This prevents accidental verbose output
    // if the variable happens to be set in the user's environment.
    let level = if debug { "debug" } else { "info" };

    let filter = if debug {
        // Allow `RUST_LOG` to override the level when debug logging is enabled.
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level))
    } else {
        EnvFilter::new(level)
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init();
}
