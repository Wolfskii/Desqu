//! Process-wide `tracing` setup.

/// `default_filter` is used when `RUST_LOG` is not set, e.g. `"screenbound_lib=info,warn"`.
pub fn init(default_filter: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
